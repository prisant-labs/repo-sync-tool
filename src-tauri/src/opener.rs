//! Edge OS-integration: open a repo's folder, remote, editor, or terminal.
//!
//! `reposync-core` stays OS-neutral; launching the file manager, browser, editor,
//! or terminal is an edge concern, so it lives here in the shell (like
//! `localtime`). Each helper SPAWNS and detaches - it never waits, because the
//! launched app outlives the command. A missing clone path returns `PathMissing`
//! rather than opening the file manager on nothing.
//!
//! Process spawning from a Rust command handler needs no Tauri capability
//! (capabilities gate what the webview may invoke, not what backend Rust does),
//! and matches how the git engine already shells out.
//!
//! Hardening (BL-NI-24 / BL-NI-36, Fable audit 2026-07-04 findings 1/2/8/9):
//!
//!   - Every stored path is defensively normalized on the way in: legacy rows
//!     persisted before the `dunce::canonicalize` fix carry a Windows
//!     extended-length (`\\?\`) prefix that explorer/editors/terminals choke on.
//!     [`strip_verbatim_prefix`] removes it (no data migration).
//!   - The remote URL comes from a cloned repo's `.git/config` and is fully
//!     attacker-controlled, so it is parsed and validated to an `https://` URL
//!     before it ever reaches the OS ([`remote_url_to_web_url`]); a `file://`, a
//!     local `.exe` path, or a UNC path is rejected rather than executed.
//!   - The editor is resolved to a concrete executable up front and spawned
//!     DIRECTLY (no `cmd /C`), so a repo folder named `docs&calc` or one with a
//!     `%VAR%` segment cannot inject a command, and a misconfigured editor
//!     surfaces as a typed error instead of a false success.

use std::path::{Path, PathBuf};
use std::process::Command;

use reposync_core::error::AppError;

/// Spawn a detached child, mapping a spawn failure to a typed error. Never waits.
fn spawn_detached(mut cmd: Command, what: &str) -> Result<(), AppError> {
    match cmd.spawn() {
        Ok(_child) => Ok(()),
        Err(e) => Err(AppError::Unexpected {
            context: format!("failed to launch {what}: {e}"),
        }),
    }
}

/// The clone path must exist on disk; a moved/deleted clone is a clear error.
fn require_dir(path: &Path) -> Result<(), AppError> {
    if path.exists() {
        Ok(())
    } else {
        Err(AppError::PathMissing {
            path: path.display().to_string(),
        })
    }
}

/// Strip a Windows extended-length ("verbatim") prefix from a stored path string.
///
/// `std::fs::canonicalize` used to persist `\\?\C:\...` / `\\?\UNC\...` paths;
/// explorer, editors, and terminals mishandle that prefix. New rows are written
/// clean via `dunce::canonicalize` in the core, but rows added before that fix
/// still carry the prefix, so we strip it defensively at open time:
///
///   - `\\?\UNC\server\share\repo` -> `\\server\share\repo`
///   - `\\?\C:\Users\me\repo`      -> `C:\Users\me\repo`
///
/// Any other input (an already-clean path, every POSIX path) is returned
/// unchanged, so this is safe to call unconditionally on every platform.
fn strip_verbatim_prefix(raw: &str) -> String {
    if let Some(rest) = raw.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{rest}")
    } else if let Some(rest) = raw.strip_prefix(r"\\?\") {
        rest.to_string()
    } else {
        raw.to_string()
    }
}

/// Normalize a stored clone path before handing it to the OS (see
/// [`strip_verbatim_prefix`]).
fn normalize_stored_path(path: &Path) -> PathBuf {
    PathBuf::from(strip_verbatim_prefix(path.to_string_lossy().as_ref()))
}

/// Reveal the repo folder in the OS file manager.
pub fn open_folder(path: &Path) -> Result<(), AppError> {
    let normalized = normalize_stored_path(path);
    let path = normalized.as_path();
    require_dir(path)?;
    #[cfg(windows)]
    let cmd = {
        // explorer returns a nonzero exit even on success and is a GUI app; we
        // spawn and detach, so its exit code is irrelevant. The path is passed as
        // a single argv argument, so no shell metacharacter interpretation applies.
        let mut c = Command::new("explorer");
        c.arg(path);
        c
    };
    #[cfg(target_os = "macos")]
    let cmd = {
        let mut c = Command::new("open");
        c.arg(path);
        c
    };
    #[cfg(all(unix, not(target_os = "macos")))]
    let cmd = {
        let mut c = Command::new("xdg-open");
        c.arg(path);
        c
    };
    spawn_detached(cmd, "file manager")
}

/// Open a URL in the default browser.
///
/// Callers that pass an untrusted URL (`repo_open_remote`) MUST route through
/// [`open_remote`], which validates the scheme first. This low-level helper does
/// not validate; it exists so [`open_remote`] can reuse the per-OS launch.
pub fn open_url(url: &str) -> Result<(), AppError> {
    #[cfg(windows)]
    let cmd = {
        let mut c = Command::new("explorer");
        c.arg(url);
        c
    };
    #[cfg(target_os = "macos")]
    let cmd = {
        let mut c = Command::new("open");
        c.arg(url);
        c
    };
    #[cfg(all(unix, not(target_os = "macos")))]
    let cmd = {
        let mut c = Command::new("xdg-open");
        c.arg(url);
        c
    };
    spawn_detached(cmd, "browser")
}

/// The typed rejection for a remote URL that is not a browsable web remote.
///
/// `InvalidSetting` (not `NotFound`): the remote DOES exist in `.git/config`, it
/// is simply not an `http(s)`/`ssh` URL we can safely translate to a browser URL.
/// `NotFound` is reserved for a repo with NO remote configured at all (raised by
/// the command handler before we get here). The `field` names the offending value.
fn invalid_remote() -> AppError {
    AppError::InvalidSetting {
        field: "remote_origin_url".into(),
    }
}

/// Case-insensitive scheme prefix match, returning the remainder after the scheme.
/// Uses `get(..)` so a multi-byte leading char can never panic on a byte-slice.
fn strip_scheme_ci<'a>(s: &'a str, scheme: &str) -> Option<&'a str> {
    match s.get(..scheme.len()) {
        Some(head) if head.eq_ignore_ascii_case(scheme) => Some(&s[scheme.len()..]),
        _ => None,
    }
}

/// A dotted, separator-free host token (`github.com`, `gitlab.example.com`).
///
/// Gates scp/ssh translation so a Windows drive letter (`C`), a UNC fragment, or
/// a bare token (`javascript`) can never be promoted to a host. A dotless host
/// (an intranet name like `gitserver`) is deliberately unsupported for the
/// translated forms - such remotes are rare and can be opened via their http URL.
fn is_dotted_host(host: &str) -> bool {
    !host.is_empty()
        && host.contains('.')
        && !host.starts_with('.')
        && !host.ends_with('.')
        && host
            .chars()
            .all(|c| !c.is_whitespace() && c != '/' && c != '\\' && c != ':' && c != '@')
}

/// A host is acceptable in an already-`http(s)` URL: non-empty, no whitespace or
/// backslash. `http(s)` opens in a browser (never executes), so the dotted-host
/// rule required for the translated forms is not needed here.
fn is_web_host(host: &str) -> bool {
    !host.is_empty() && host.chars().all(|c| !c.is_whitespace() && c != '\\')
}

/// Reduce a remote's repo path (`owner/repo.git`, `/owner/repo/`) to `owner/repo`.
/// Rejects an empty path or one containing a backslash (a Windows path fragment).
fn clean_repo_path(path: &str) -> Option<String> {
    let p = path.trim_start_matches('/').trim_end_matches('/');
    let p = p.strip_suffix(".git").unwrap_or(p);
    let p = p.trim_end_matches('/');
    if p.is_empty() || p.contains('\\') {
        None
    } else {
        Some(p.to_string())
    }
}

/// Extract the host from an `http(s)` remainder (`host[:port][/path][?..][#..]`).
fn web_host_of(rest: &str) -> &str {
    let host = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    let host = host.rsplit('@').next().unwrap_or(host); // drop any userinfo
    host.split(':').next().unwrap_or(host) // drop any :port
}

/// Translate a git remote URL into an `https://` URL safe to hand to the browser,
/// rejecting anything that is not a browsable web remote.
///
/// Accepted:
///   - `http(s)://host/...`                     -> passed through unchanged.
///   - `[user@]host:owner/repo[.git]` (scp)     -> `https://host/owner/repo`.
///   - `ssh://[user@]host[:port]/owner/repo[.git]` -> `https://host/owner/repo`.
///
/// Rejected as [`invalid_remote`]: `file://`, `git://`, any other scheme,
/// `javascript:`, a Windows/UNC/local path, and a dotless host in the translated
/// forms. This is the security boundary for `repo_open_remote`: the raw string is
/// attacker-controlled, so a crafted `file://`/`.exe`/UNC remote must never reach
/// the OS launcher.
fn remote_url_to_web_url(raw: &str) -> Result<String, AppError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(invalid_remote());
    }

    // 1. Already a web URL: accept http(s) as-is (validate a host is present).
    if let Some(rest) =
        strip_scheme_ci(trimmed, "https://").or_else(|| strip_scheme_ci(trimmed, "http://"))
    {
        if is_web_host(web_host_of(rest)) {
            return Ok(trimmed.to_string());
        }
        return Err(invalid_remote());
    }

    // 2. ssh:// URL -> https translation.
    if let Some(rest) = strip_scheme_ci(trimmed, "ssh://") {
        // rest = [user@]host[:port]/path
        let (authority, path) = rest.split_once('/').ok_or_else(invalid_remote)?;
        let host = authority.rsplit('@').next().unwrap_or(authority);
        let host = host.split(':').next().unwrap_or(host);
        if !is_dotted_host(host) {
            return Err(invalid_remote());
        }
        let repo_path = clean_repo_path(path).ok_or_else(invalid_remote)?;
        return Ok(format!("https://{host}/{repo_path}"));
    }

    // 3. Any other explicit scheme (git://, file://, ftp://, ...) is not browsable.
    if trimmed.contains("://") {
        return Err(invalid_remote());
    }

    // 4. scp-like `[user@]host:owner/repo`. git treats `host:path` as scp-like
    //    only when there is no slash before the first colon (else it is a local
    //    path); a bare drive letter (`C:\...`) is a Windows path, not a host. The
    //    dotted-host requirement rejects the drive letter and bare tokens like
    //    `javascript`, so `C:\Windows\cmd.exe` and `javascript:alert(1)` fall out
    //    here as invalid.
    if let Some((before, after)) = trimmed.split_once(':') {
        if before.contains('/') || before.contains('\\') {
            return Err(invalid_remote());
        }
        let host = before.rsplit('@').next().unwrap_or(before);
        if !is_dotted_host(host) {
            return Err(invalid_remote());
        }
        let repo_path = clean_repo_path(after).ok_or_else(invalid_remote)?;
        return Ok(format!("https://{host}/{repo_path}"));
    }

    Err(invalid_remote())
}

/// Open the repo's remote in the browser, validating/translating the raw
/// `.git/config` URL first (see [`remote_url_to_web_url`]).
pub fn open_remote(raw_url: &str) -> Result<(), AppError> {
    let web_url = remote_url_to_web_url(raw_url)?;
    open_url(&web_url)
}

/// Whether `s` starts with a `X:` drive prefix (a Windows drive-qualified path).
#[cfg(windows)]
fn has_drive_prefix(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':'
}

/// Pure core of Windows editor resolution: find the first existing executable for
/// `cmd` given the PATH directories, the PATHEXT extensions, and an existence
/// predicate. Factored out (with an injected `exists`) so the decision logic is
/// unit-tested without touching the real filesystem or environment.
///
///   - An explicit path (contains a separator or a `X:` drive prefix) is NOT
///     searched on PATH: it is tried as-is, then with each PATHEXT appended.
///   - A bare name is searched across the PATH dirs in order; within each dir the
///     name is tried as-is when it already carries an extension, then with each
///     PATHEXT appended. This is what lets a bare `code` resolve to `code.cmd`.
#[cfg(windows)]
fn resolve_in_paths(
    cmd: &str,
    path_dirs: &[PathBuf],
    pathext: &[String],
    exists: &dyn Fn(&Path) -> bool,
) -> Option<PathBuf> {
    let looks_like_path = cmd.contains('/') || cmd.contains('\\') || has_drive_prefix(cmd);
    let has_extension = Path::new(cmd).extension().is_some();

    let try_base = |base: PathBuf| -> Option<PathBuf> {
        // A name that already carries an extension is tried verbatim first.
        if has_extension && exists(&base) {
            return Some(base);
        }
        // Then the PATHEXT candidates (`base` + `.EXE`, `base` + `.CMD`, ...).
        for ext in pathext {
            let mut s = base.clone().into_os_string();
            s.push(ext);
            let cand = PathBuf::from(s);
            if exists(&cand) {
                return Some(cand);
            }
        }
        // Finally an extensionless file that nonetheless exists (rare on Windows).
        if !has_extension && exists(&base) {
            return Some(base);
        }
        None
    };

    if looks_like_path {
        return try_base(PathBuf::from(cmd));
    }
    path_dirs.iter().find_map(|dir| try_base(dir.join(cmd)))
}

/// Resolve a configured editor command to a concrete executable path, honoring
/// PATH and PATHEXT (so VS Code's `code.cmd` shim resolves for a bare `code`).
/// `None` means the editor could not be found - surfaced as an error, not a
/// silent success.
///
/// Dogfood (Phase 2) must verify on a real Windows box: (1) a bare `code`
/// resolves to the VS Code `.cmd` shim and opens the folder; (2) an editor
/// configured as a full `.exe` path opens; (3) a bogus editor name yields the
/// "invalid setting" toast, not "Opened editor".
#[cfg(windows)]
fn resolve_executable(cmd: &str) -> Option<PathBuf> {
    use std::env;
    let path_dirs: Vec<PathBuf> = env::var_os("PATH")
        .map(|p| env::split_paths(&p).collect())
        .unwrap_or_default();
    let pathext: Vec<String> = env::var("PATHEXT")
        .unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".to_string())
        .split(';')
        .filter(|e| !e.is_empty())
        .map(|e| e.to_string())
        .collect();
    resolve_in_paths(cmd, &path_dirs, &pathext, &|p| p.exists())
}

/// Open the repo folder in the configured editor (e.g. `code`).
pub fn open_editor(editor_cmd: &str, path: &Path) -> Result<(), AppError> {
    let normalized = normalize_stored_path(path);
    let path = normalized.as_path();
    require_dir(path)?;
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        // Resolve the configured editor to a concrete executable up front (PATH +
        // PATHEXT, so `code.cmd` still resolves), then spawn it DIRECTLY with the
        // repo path as a plain argv argument. This eliminates the old
        // `cmd /C <editor> <path>` hop: a folder named `docs&calc` or one with a
        // `%VAR%` segment is inert (no cmd metacharacter/variable interpretation),
        // and a misconfigured editor surfaces here as an error instead of cmd.exe
        // spawning successfully and swallowing the real failure. When the resolved
        // target is a `.cmd`/`.bat` shim, Rust's std applies safe cmd-quoting
        // internally (post-CVE-2024-24576), so the path argument stays inert.
        let resolved = resolve_executable(editor_cmd).ok_or_else(|| AppError::InvalidSetting {
            field: "editor_command".into(),
        })?;
        let mut c = Command::new(&resolved);
        c.arg(path).creation_flags(CREATE_NO_WINDOW);
        spawn_detached(c, "editor")
    }
    #[cfg(not(windows))]
    {
        // POSIX: no shell hop, so no metacharacter class; a missing editor makes
        // spawn() fail, which `spawn_detached` maps to a typed error (not a false
        // success). Passed as a plain argv argument.
        let mut c = Command::new(editor_cmd);
        c.arg(path);
        spawn_detached(c, "editor")
    }
}

/// Whether the configured terminal command is Windows Terminal, detected by the
/// executable's file STEM so a bare `wt`, `wt.exe`, or a full path like
/// `C:\Users\me\...\wt.exe` (a per-user install off PATH) all match (BL-NI-36) -
/// not just the raw configured string.
#[cfg(windows)]
fn is_windows_terminal(terminal_cmd: &str) -> bool {
    Path::new(terminal_cmd.trim())
        .file_stem()
        .and_then(|s| s.to_str())
        .is_some_and(|stem| stem.eq_ignore_ascii_case("wt"))
}

/// Open a terminal at the repo folder using the configured terminal command.
pub fn open_terminal(terminal_cmd: &str, path: &Path) -> Result<(), AppError> {
    let normalized = normalize_stored_path(path);
    let path = normalized.as_path();
    require_dir(path)?;
    #[cfg(windows)]
    {
        // Windows Terminal ignores an inherited working dir, so it needs
        // `-d <path>`; any other terminal gets its working dir set. The path is a
        // plain argv argument in both cases (no shell), so it needs no escaping.
        let base = terminal_cmd.trim();
        let mut c = Command::new(base);
        if is_windows_terminal(base) {
            c.arg("-d").arg(path);
        } else {
            c.current_dir(path);
        }
        spawn_detached(c, "terminal")
    }
    #[cfg(not(windows))]
    {
        let mut c = Command::new(terminal_cmd);
        c.current_dir(path);
        spawn_detached(c, "terminal")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn require_dir_rejects_a_missing_path() {
        let missing = Path::new("this/path/does/not/exist/repo-xyz");
        assert!(matches!(
            require_dir(missing),
            Err(AppError::PathMissing { .. })
        ));
    }

    #[test]
    fn require_dir_accepts_an_existing_dir() {
        // The crate manifest dir always exists during the test run.
        let here = Path::new(env!("CARGO_MANIFEST_DIR"));
        assert!(require_dir(here).is_ok());
    }

    // --- Finding 1(b): extended-length ("\\?\") prefix normalization ---

    #[test]
    fn strips_verbatim_drive_prefix() {
        assert_eq!(
            strip_verbatim_prefix(r"\\?\C:\Users\me\repo"),
            r"C:\Users\me\repo"
        );
    }

    #[test]
    fn strips_verbatim_unc_prefix_to_a_plain_unc_path() {
        assert_eq!(
            strip_verbatim_prefix(r"\\?\UNC\server\share\repo"),
            r"\\server\share\repo"
        );
    }

    #[test]
    fn leaves_already_clean_and_posix_paths_unchanged() {
        assert_eq!(
            strip_verbatim_prefix(r"C:\already\clean"),
            r"C:\already\clean"
        );
        assert_eq!(strip_verbatim_prefix("/home/me/repo"), "/home/me/repo");
        assert_eq!(strip_verbatim_prefix(""), "");
    }

    #[test]
    fn normalize_stored_path_round_trips_a_verbatim_path() {
        assert_eq!(
            normalize_stored_path(Path::new(r"\\?\C:\Users\me\repo")),
            PathBuf::from(r"C:\Users\me\repo")
        );
    }

    // --- Finding 2: remote URL translation / validation table ---

    fn web(raw: &str) -> String {
        remote_url_to_web_url(raw).expect("should translate")
    }

    fn rejected(raw: &str) -> bool {
        matches!(
            remote_url_to_web_url(raw),
            Err(AppError::InvalidSetting { .. })
        )
    }

    #[test]
    fn accepts_https_and_http_as_is() {
        assert_eq!(
            web("https://github.com/owner/repo"),
            "https://github.com/owner/repo"
        );
        assert_eq!(
            web("https://github.com/owner/repo.git"),
            "https://github.com/owner/repo.git"
        );
        assert_eq!(
            web("http://example.com/owner/repo"),
            "http://example.com/owner/repo"
        );
        // Userinfo + port are preserved; the host still validates.
        assert_eq!(
            web("https://user@gitlab.com:443/group/repo"),
            "https://user@gitlab.com:443/group/repo"
        );
    }

    #[test]
    fn translates_scp_like_ssh_for_the_major_hosts() {
        assert_eq!(
            web("git@github.com:owner/repo.git"),
            "https://github.com/owner/repo"
        );
        assert_eq!(
            web("git@github.com:owner/repo"),
            "https://github.com/owner/repo"
        );
        assert_eq!(
            web("git@gitlab.com:group/subgroup/repo.git"),
            "https://gitlab.com/group/subgroup/repo"
        );
        assert_eq!(
            web("git@bitbucket.org:owner/repo.git"),
            "https://bitbucket.org/owner/repo"
        );
        // A non-`git` user, and the userless form, both translate.
        assert_eq!(
            web("alice@github.com:owner/repo.git"),
            "https://github.com/owner/repo"
        );
        assert_eq!(
            web("github.com:owner/repo.git"),
            "https://github.com/owner/repo"
        );
    }

    #[test]
    fn translates_ssh_scheme_including_ports() {
        assert_eq!(
            web("ssh://git@github.com/owner/repo.git"),
            "https://github.com/owner/repo"
        );
        assert_eq!(
            web("ssh://git@github.com:22/owner/repo.git"),
            "https://github.com/owner/repo"
        );
        // Bitbucket Server style ssh with a non-standard port and a leading-slash path.
        assert_eq!(
            web("ssh://git@bitbucket.example.com:7999/proj/repo.git"),
            "https://bitbucket.example.com/proj/repo"
        );
    }

    #[test]
    fn rejects_malicious_and_non_browsable_remotes() {
        // Local Windows exe path (the click-to-execute vector).
        assert!(rejected(r"C:\Windows\System32\cmd.exe"));
        assert!(rejected("C:/Users/evil/calc.exe"));
        // UNC path.
        assert!(rejected(r"\\attacker\share\evil.exe"));
        // Verbatim/UNC fragments.
        assert!(rejected(r"\\?\C:\evil"));
        // file:// and other non-browsable schemes.
        assert!(rejected("file:///C:/Users/evil/repo"));
        assert!(rejected("git://github.com/owner/repo"));
        assert!(rejected("ftp://example.com/x"));
        // javascript: (no `//`, dotless "host").
        assert!(rejected("javascript:alert(1)"));
        // Empty / whitespace-only.
        assert!(rejected(""));
        assert!(rejected("   "));
        // A bare token with no scheme, colon, or host.
        assert!(rejected("cmd.exe"));
        // Dotless host in a translated form is unsupported (documented).
        assert!(rejected("gitserver:owner/repo"));
        // scp form with an empty repo path.
        assert!(rejected("git@github.com:"));
    }

    // --- Findings 3/4 + 5: Windows editor resolution and wt detection ---
    // These test Windows-specific logic (PATHEXT, drive letters, wt), so they are
    // gated to the Windows build where the functions exist.

    #[cfg(windows)]
    mod windows {
        use super::*;
        use std::collections::HashSet;

        /// Build the exact candidate `resolve_in_paths` would test: `dir/name` + ext.
        fn cand(dir: &Path, name: &str, ext: &str) -> PathBuf {
            let mut s = dir.join(name).into_os_string();
            s.push(ext);
            PathBuf::from(s)
        }

        // The returned closure OWNS its set, so it does not borrow `paths`; this
        // lets callers pass a temporary array literal without a `let` binding.
        fn exists_set(paths: &[PathBuf]) -> impl Fn(&Path) -> bool {
            let set: HashSet<PathBuf> = paths.iter().cloned().collect();
            move |p: &Path| set.contains(p)
        }

        #[test]
        fn resolves_a_bare_name_to_a_cmd_shim_on_path() {
            let dir_a = PathBuf::from("/opt/a");
            let dir_b = PathBuf::from("/opt/b");
            let pathext = vec![".COM".into(), ".EXE".into(), ".BAT".into(), ".CMD".into()];
            // Only `code.CMD` exists, and only in the second dir.
            let want = cand(&dir_b, "code", ".CMD");
            let exists = exists_set(std::slice::from_ref(&want));
            let got = resolve_in_paths("code", &[dir_a, dir_b], &pathext, &exists);
            assert_eq!(got, Some(want));
        }

        #[test]
        fn resolves_a_bare_name_to_an_exe_on_path() {
            let dir = PathBuf::from("/opt/bin");
            let pathext = vec![".EXE".into(), ".CMD".into()];
            let want = cand(&dir, "git", ".EXE");
            let exists = exists_set(std::slice::from_ref(&want));
            assert_eq!(
                resolve_in_paths("git", &[dir], &pathext, &exists),
                Some(want)
            );
        }

        #[test]
        fn honors_path_dir_order() {
            let dir_a = PathBuf::from("/opt/a");
            let dir_b = PathBuf::from("/opt/b");
            let pathext = vec![".CMD".into()];
            let in_a = cand(&dir_a, "code", ".CMD");
            let in_b = cand(&dir_b, "code", ".CMD");
            // Both exist; the earlier PATH dir wins.
            let exists = exists_set(&[in_a.clone(), in_b]);
            assert_eq!(
                resolve_in_paths("code", &[dir_a, dir_b], &pathext, &exists),
                Some(in_a)
            );
        }

        #[test]
        fn resolves_an_explicit_exe_path_without_searching_path() {
            let pathext = vec![".EXE".into()];
            let explicit = PathBuf::from(r"C:\tools\editor.exe");
            let exists = exists_set(std::slice::from_ref(&explicit));
            // PATH is empty on purpose: an explicit path must not be searched there.
            assert_eq!(
                resolve_in_paths(r"C:\tools\editor.exe", &[], &pathext, &exists),
                Some(explicit)
            );
        }

        #[test]
        fn resolves_an_explicit_extensionless_path_via_pathext() {
            let pathext = vec![".EXE".into()];
            let want = cand(Path::new(r"C:\tools"), "editor", ".EXE");
            let exists = exists_set(std::slice::from_ref(&want));
            assert_eq!(
                resolve_in_paths(r"C:\tools\editor", &[], &pathext, &exists),
                Some(want)
            );
        }

        #[test]
        fn returns_none_when_nothing_resolves() {
            let dir = PathBuf::from("/opt/bin");
            let pathext = vec![".EXE".into(), ".CMD".into()];
            let exists = exists_set(&[]); // nothing exists
            assert_eq!(
                resolve_in_paths("no-such-editor", &[dir], &pathext, &exists),
                None
            );
        }

        #[test]
        fn detects_windows_terminal_by_stem() {
            assert!(is_windows_terminal("wt"));
            assert!(is_windows_terminal("wt.exe"));
            assert!(is_windows_terminal("WT.EXE"));
            assert!(is_windows_terminal("  wt  "));
            // Full path to a per-user install off PATH (BL-NI-36).
            assert!(is_windows_terminal(
                r"C:\Users\me\AppData\Local\Microsoft\WindowsApps\wt.exe"
            ));
            assert!(is_windows_terminal("C:/tools/wt.exe"));
        }

        #[test]
        fn does_not_mistake_other_terminals_for_wt() {
            assert!(!is_windows_terminal("powershell"));
            assert!(!is_windows_terminal("pwsh.exe"));
            assert!(!is_windows_terminal(r"C:\tools\alacritty.exe"));
            assert!(!is_windows_terminal("wezterm.exe"));
            assert!(!is_windows_terminal(""));
        }
    }
}
