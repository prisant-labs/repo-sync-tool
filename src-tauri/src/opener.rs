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

use std::path::Path;
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

/// Reveal the repo folder in the OS file manager.
pub fn open_folder(path: &Path) -> Result<(), AppError> {
    require_dir(path)?;
    #[cfg(windows)]
    let cmd = {
        // explorer returns a nonzero exit even on success and is a GUI app; we
        // spawn and detach, so its exit code is irrelevant.
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

/// Open the repo folder in the configured editor (e.g. `code`).
pub fn open_editor(editor_cmd: &str, path: &Path) -> Result<(), AppError> {
    require_dir(path)?;
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        // Editors like VS Code ship a `.cmd` shim (`code.cmd`) that CreateProcess
        // will not resolve for a bare `code`. Route through `cmd /C` so PATHEXT
        // resolution finds it, and suppress the transient console window.
        let mut c = Command::new("cmd");
        c.arg("/C")
            .arg(editor_cmd)
            .arg(path)
            .creation_flags(CREATE_NO_WINDOW);
        spawn_detached(c, "editor")
    }
    #[cfg(not(windows))]
    {
        let mut c = Command::new(editor_cmd);
        c.arg(path);
        spawn_detached(c, "editor")
    }
}

/// Open a terminal at the repo folder using the configured terminal command.
pub fn open_terminal(terminal_cmd: &str, path: &Path) -> Result<(), AppError> {
    require_dir(path)?;
    #[cfg(windows)]
    {
        // Windows Terminal (the default `wt`) ignores an inherited working dir,
        // so it needs `-d <path>`; any other terminal gets the working dir set.
        let base = terminal_cmd.trim();
        let is_wt = base.eq_ignore_ascii_case("wt") || base.eq_ignore_ascii_case("wt.exe");
        let mut c = Command::new(base);
        if is_wt {
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
}
