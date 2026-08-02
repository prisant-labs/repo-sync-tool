//! redact - strip credentials out of captured process output before it travels.
//!
//! THIS IS DEFENSE IN DEPTH, NOT A FIX FOR A DEMONSTRATED LEAK. That
//! distinction is recorded because it was got wrong once already, and the wrong
//! version is the more persuasive one.
//!
//! The plausible story is that a remote URL embedding a token leaks through
//! `git fetch`'s error output. Checked rather than assumed, on git 2.40.1:
//!
//! ```text
//! $ git remote add origin https://x-access-token:ghp_XXXX@127.0.0.1:1/a/b.git
//! $ git fetch --all
//! fatal: unable to access 'https://127.0.0.1:1/a/b.git/': Failed to connect
//! ```
//!
//! Git sanitizes the userinfo itself, on both the connection-refused and the
//! DNS-failure paths. And none of the four commands this crate captures -
//! `fetch --all --prune`, `pull --ff-only`, `rev-parse`, `status
//! --porcelain=v2` - prints a remote URL at all. Remote URLs are read
//! in-process through `git2`, never through a captured subprocess. So there is
//! no known path by which a credential reaches captured output today.
//!
//! It is still worth having, for reasons that do not depend on that story:
//!
//! - Git's sanitization is GIT's behavior, not a guarantee this crate makes. It
//!   varies by version and by code path, and the floor here is git 2.30.
//! - `run_git` is a shared chokepoint that will grow new commands. The next one
//!   added is not required to be as careful as the current four.
//! - A user running with `GIT_TRACE`/`GIT_CURL_VERBOSE`, or a credential helper
//!   that prints, can put an `Authorization:` header on stderr.
//! - When the V1.1 keyring PAT lands, RepoSync will hold a token itself, and
//!   what ends up in captured output stops being purely git's decision.
//!
//! The honest summary: this closes a class before it opens, at the cost of one
//! pure function on a path that already exists.
//!
//! WHERE THIS RUNS, AND WHY NOT AT THE OBVIOUS PLACE. Redaction happens at
//! CAPTURE, in `git::cli::run_git`, not at the activity write sink where
//! `cap_stream` lives. Captured stderr does not only become an activity row: it
//! becomes [`AppError::FetchFailed`], which crosses IPC to the UI and is
//! rendered on screen, and it is interpolated into `tracing` events that land in
//! a rotating log a user may attach to a bug report. Redacting at the database
//! boundary would clean exactly one of those three. Cleaning at capture makes
//! every downstream consumer safe by construction.
//!
//! It also runs BEFORE size capping, because capping first can bisect a token
//! and persist the surviving half.
//!
//! WHAT THIS DOES AND DOES NOT PROMISE. Two mechanisms, deliberately:
//!
//! - URL userinfo removal is STRUCTURAL. Any `scheme://...@host` loses its
//!   userinfo. It is complete for the shape it covers and provable by test.
//! - Known token prefixes and `Authorization:` headers are a HEURISTIC. They
//!   catch common cases and cannot be complete, because secrets are not an
//!   enumerable set.
//!
//! The published promise is worded to match: URL credentials are removed, and
//! well-known token formats are best-effort. Claiming "secrets are redacted"
//! would be the more dangerous error, because a user who believes it stops
//! reading logs before sending them.
//!
//! [`AppError::FetchFailed`]: crate::error::AppError::FetchFailed

/// What replaces anything removed. Visible on purpose: a reader must be able to
/// tell "a credential was here and was removed" from "nothing was here", or the
/// log stops being trustworthy evidence in the other direction.
pub const REDACTED: &str = "***";

/// Token prefixes worth matching. Kept SHORT on purpose - a long speculative
/// list produces false positives, and a false positive corrupts the evidence the
/// capture exists to preserve.
const TOKEN_PREFIXES: &[&str] = &[
    // GitHub, in the documented `ghX_` family plus fine-grained PATs.
    "github_pat_",
    "ghp_",
    "gho_",
    "ghu_",
    "ghs_",
    "ghr_",
    // GitLab personal access tokens.
    "glpat-",
];

/// Characters that may appear in the body of a token, used to find its end.
fn is_token_body(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '-'
}

/// Remove credentials from `input`.
///
/// Safe to call on arbitrary bytes-turned-string: it only ever slices at indices
/// produced by matching ASCII patterns, so it cannot split a multi-byte
/// character.
pub fn redact_secrets(input: &str) -> String {
    let stage = redact_url_userinfo(input);
    let stage = redact_token_prefixes(&stage);
    redact_authorization_headers(&stage)
}

/// Replace the userinfo of any `scheme://userinfo@host` with [`REDACTED`].
///
/// ALL userinfo goes, not just the password half. Both of these are real GitHub
/// patterns and only one has a colon:
///
/// ```text
/// https://x-access-token:ghp_XXXX@github.com/o/r.git
/// https://ghp_XXXX@github.com/o/r.git
/// ```
///
/// Keeping the username to stay diagnostic-friendly would leak the second form
/// entirely. The cost is over-redaction of a harmless `ssh://git@github.com`,
/// which is a small diagnostic loss and the right side to err on.
///
/// The scp-like form `git@github.com:org/repo.git` has no `://` and is left
/// alone, which is correct: it carries no password field at all.
fn redact_url_userinfo(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;

    while let Some(scheme_end) = rest.find("://") {
        let after_scheme = scheme_end + "://".len();
        out.push_str(&rest[..after_scheme]);
        rest = &rest[after_scheme..];

        // The authority ends at the first delimiter. Anything past that is path,
        // query, or ordinary prose, and an '@' out there is not userinfo - it is
        // an email address in a commit message.
        let authority_end = rest
            .find(|c: char| c == '/' || c == '?' || c == '#' || c.is_whitespace() || c == '\'')
            .unwrap_or(rest.len());
        let authority = &rest[..authority_end];

        // rfind, not find: a password may itself contain '@', and the LAST one
        // is the real separator.
        if let Some(at) = authority.rfind('@') {
            out.push_str(REDACTED);
            out.push('@');
            out.push_str(&authority[at + 1..]);
        } else {
            out.push_str(authority);
        }
        rest = &rest[authority_end..];
    }

    out.push_str(rest);
    out
}

/// Replace any run beginning with a known token prefix with [`REDACTED`].
fn redact_token_prefixes(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut idx = 0;

    while idx < input.len() {
        // Find the earliest occurrence of any prefix at or after idx.
        let hit = TOKEN_PREFIXES
            .iter()
            .filter_map(|p| input[idx..].find(p).map(|off| (idx + off, *p)))
            .min_by_key(|(pos, prefix)| (*pos, std::cmp::Reverse(prefix.len())));

        let Some((pos, prefix)) = hit else {
            out.push_str(&input[idx..]);
            return out;
        };

        out.push_str(&input[idx..pos]);

        // Consume the prefix plus the token body that follows it.
        let mut end = pos + prefix.len();
        for c in input[end..].chars() {
            if is_token_body(c) {
                end += c.len_utf8();
            } else {
                break;
            }
        }
        out.push_str(REDACTED);
        idx = end;
    }

    out
}

/// Blank the value of any `Authorization:` header to end of line.
///
/// Only reachable when a user turns on Git's own HTTP tracing, which is exactly
/// the situation where someone is capturing output to send to somebody else.
fn redact_authorization_headers(input: &str) -> String {
    const HEADER: &str = "authorization:";
    let lower = input.to_ascii_lowercase();
    let mut out = String::with_capacity(input.len());
    let mut idx = 0;

    while let Some(off) = lower[idx..].find(HEADER) {
        let start = idx + off;
        let value_start = start + HEADER.len();
        out.push_str(&input[idx..value_start]);
        let line_end = input[value_start..]
            .find('\n')
            .map(|n| value_start + n)
            .unwrap_or(input.len());
        out.push(' ');
        out.push_str(REDACTED);
        idx = line_end;
    }

    out.push_str(&input[idx..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The exact string this module exists for: what git prints when a remote
    /// with an embedded token rejects the fetch.
    #[test]
    fn the_real_git_error_loses_its_token() {
        let input = "fatal: unable to access \
                     'https://x-access-token:ghp_ABCDEFGHIJKLMNOP@github.com/acme/widgets.git/': \
                     The requested URL returned error: 403";
        let out = redact_secrets(input);

        assert!(
            !out.contains("ghp_ABCDEFGHIJKLMNOP"),
            "token survived: {out}"
        );
        assert!(!out.contains("x-access-token"), "userinfo survived: {out}");
        assert!(
            out.contains("github.com/acme/widgets.git"),
            "the diagnostic value must survive: {out}"
        );
        assert!(out.contains("403"), "the actual error must survive: {out}");
    }

    /// Token-as-username with NO password. Preserving usernames "for
    /// diagnostics" would leak this shape completely, which is why all userinfo
    /// goes rather than only the half after a colon.
    #[test]
    fn a_token_used_as_the_username_is_removed() {
        let out = redact_secrets("https://ghp_SECRETVALUE1234@github.com/o/r.git");
        assert!(!out.contains("ghp_SECRETVALUE1234"), "{out}");
        assert!(out.starts_with("https://***@github.com"), "{out}");
    }

    #[test]
    fn a_password_containing_an_at_sign_is_still_fully_removed() {
        let out = redact_secrets("https://user:p@ss@github.com/o/r.git");
        assert!(!out.contains("p@ss"), "rfind must take the LAST '@': {out}");
        assert_eq!(out, "https://***@github.com/o/r.git");
    }

    /// The scp-like form has no scheme and no password field. Mangling it would
    /// destroy the most common remote spelling there is for no security gain.
    #[test]
    fn the_scp_like_remote_form_is_left_alone() {
        let input = "git@github.com:acme/widgets.git";
        assert_eq!(redact_secrets(input), input);
    }

    #[test]
    fn a_url_with_no_userinfo_is_left_alone() {
        let input = "https://github.com/acme/widgets.git";
        assert_eq!(redact_secrets(input), input);
    }

    /// An '@' after the authority is an email address, not a credential. Commit
    /// messages and author lines are full of them and must survive intact.
    #[test]
    fn an_email_address_in_prose_survives() {
        let input = "Author: Dana Scully <dana@example.com> pushed to https://github.com/o/r";
        assert_eq!(redact_secrets(input), input);
    }

    #[test]
    fn an_email_in_a_url_path_is_not_mistaken_for_userinfo() {
        let input = "https://github.com/o/r/issues?q=author%3Adana@example.com";
        assert_eq!(
            redact_secrets(input),
            input,
            "the authority ends at the first '/', so a later '@' is not userinfo"
        );
    }

    #[test]
    fn every_url_in_a_multi_line_capture_is_covered() {
        let input = "remote: https://a:ghp_ONE234@github.com/x.git failed\n\
                     remote: https://b:glpat-TWO567@gitlab.com/y.git failed";
        let out = redact_secrets(input);
        assert!(!out.contains("ghp_ONE234"), "{out}");
        assert!(!out.contains("glpat-TWO567"), "{out}");
        assert!(!out.contains("a:"), "{out}");
        assert_eq!(out.lines().count(), 2, "line structure must be preserved");
    }

    #[test]
    fn a_bare_token_outside_a_url_is_removed() {
        let out = redact_secrets("hint: try again with token ghp_LOOSETOKEN99 set");
        assert_eq!(out, "hint: try again with token *** set");
    }

    #[test]
    fn a_fine_grained_pat_prefix_is_matched_before_the_short_one() {
        let out = redact_secrets("github_pat_11ABCDE_xyzXYZ0123");
        assert_eq!(
            out, REDACTED,
            "the whole fine-grained PAT must go, not just a leading fragment"
        );
    }

    #[test]
    fn authorization_headers_are_blanked_to_end_of_line() {
        let input = "> GET /x HTTP/1.1\nAuthorization: Bearer abc.def.ghi\n> Accept: */*";
        let out = redact_secrets(input);
        assert!(!out.contains("abc.def.ghi"), "{out}");
        assert!(out.contains("Authorization: ***"), "{out}");
        assert!(
            out.contains("> Accept: */*"),
            "redaction must stop at the newline, not eat the rest: {out}"
        );
    }

    /// Git output is arbitrary UTF-8. The redactor slices at indices from ASCII
    /// pattern matches, so it must never split a multi-byte character.
    #[test]
    fn arbitrary_utf8_is_handled_without_panicking() {
        let input = "分岐 'https://u:ghp_UNICODE123@github.com/o/r.git' に失敗しました";
        let out = redact_secrets(input);
        assert!(!out.contains("ghp_UNICODE123"), "{out}");
        assert!(out.contains("分岐"), "{out}");
        assert!(out.contains("に失敗しました"), "{out}");
    }

    #[test]
    fn ordinary_output_is_returned_unchanged() {
        for input in [
            "",
            "Already up to date.",
            "From github.com:acme/widgets\n   abc1234..def5678  main -> origin/main",
            "fatal: not a git repository",
        ] {
            assert_eq!(redact_secrets(input), input, "input was {input:?}");
        }
    }

    /// Redaction must be safe to apply twice: it runs at capture, and a future
    /// change could reasonably apply it again on a different path.
    #[test]
    fn redaction_is_idempotent() {
        let once = redact_secrets("https://u:ghp_TOKEN12345@github.com/o/r.git");
        assert_eq!(redact_secrets(&once), once);
    }
}
