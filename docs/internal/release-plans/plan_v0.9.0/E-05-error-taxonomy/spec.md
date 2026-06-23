---
effort: E-05
title: Error Taxonomy (AppError)
status: ready
tier: MUST
scope: V1 (non-GUI)
depends_on: [E-01]
source: docs/internal/v1-architecture-and-decisions.md (Sections 6, 4.4, 4.5, 4.6, 4.10)
---

# E-05 - Error Taxonomy (AppError)

## Task Summary

> Agents keep this block current as work proceeds.

- **State:** not started.
- **Next:** enumerate the full `AppError` variant set in `crates/reposync-core/src/error.rs`, grouped by domain, each with a stable machine code and a remediation string.
- **Blockers:** none beyond E-01 (Foundation, workspace, CI), which owns the empty `error.rs` stub this effort fills.

## Context

Every failure RepoSync can hit - a yanked upstream, a missing local path, a detached HEAD, an expired token, a rate-limited GitHub call, a too-old or absent `git` - must surface as a **distinct, named state with its own remediation**, not a generic "something went wrong." The product's core promise is "obvious at a glance" (brief Section 5, State semantics, the `### 4` subsection under `## 5. UI and UX`): the Dashboard promotes a failed-auth repo to the top with an ember accent and a specific message, while a behind-but-clean repo recedes. That promise is only buildable if the backend hands the frontend a structured, exhaustively-enumerated error type rather than a string.

This effort defines that type: `AppError`, a `thiserror` enum of roughly 30 variants grouped by domain (git, filesystem, network/GitHub, database, config), where each variant maps to a **stable machine code** (the contract) plus an **authored-now remediation string** (copy that can be refined later without ever changing a code). `AppError` lives in `crates/reposync-core/src/error.rs`, derives `serde::Serialize`/`Deserialize` plus `specta::Type`, and is serialized across the IPC boundary as the structured error payload that E-06 (IPC contract) freezes into the typed seam.

**The serialized wire shape is frozen now so E-06 can mirror it.** An `AppError` serializes to a single flat struct:

```rust
#[derive(serde::Serialize, serde::Deserialize, specta::Type, Debug, Clone)]
pub struct AppErrorPayload {
    pub code: String,                         // the stable `domain.specific` string from code()
    pub message: String,                      // the Display text (#[error("...")])
    pub remediation: String,                  // the authored-now copy from remediation()
    pub context: Option<serde_json::Value>,   // structured per-variant context (path, versions, reset_at, ...)
}
```

serialized as `{ code, message, remediation, context }`. This is the frozen form: every `AppError` crossing IPC takes exactly this shape, and the generated TypeScript type E-06 emits mirrors it field-for-field. The `code` field is the stable `domain.specific` identifier; `context` is `null` when a variant carries no structured fields. This struct form is chosen deliberately over a serde-tagged enum representation so the generated TS type is a single clean object rather than a discriminated union the frontend must narrow.

The load-bearing constraints: machine codes are forever (the UI keys presentation, telemetry-free analytics, and tests off them), so they are chosen carefully now and never renamed; remediation strings are mutable copy that never reach into the code identity; and `AppError` carries zero Tauri dependency, consistent with the `reposync-core` hygiene rule from E-01 (Foundation).

## In scope

- The full `AppError` enum in `crates/reposync-core/src/error.rs`: every variant this effort can derive from the brief, grouped by domain.
- A **stable machine-code scheme**: a `code(&self) -> &'static str` (or equivalent) returning a stable string identifier per variant, in a documented namespacing convention (domain prefix + specific code).
- A **remediation approach**: a `remediation(&self) -> &'static str` (or equivalent) returning an authored-now, user-facing remediation string per variant, decoupled from the code so copy can be revised without a contract change.
- Derives: `thiserror::Error`, `serde::Serialize`, `serde::Deserialize`, `specta::Type`, plus `Debug` and `Clone` as needed for the IPC payload.
- The frozen serialized shape: the `AppErrorPayload` struct `{ code: String, message: String, remediation: String, context: Option<serde_json::Value> }` (defined in Context above), serialized as `{ code, message, remediation, context }`, so the frontend gets machine code plus human copy plus context in one flat payload and E-06 mirrors a single clean TS type.
- A derived `retryable(&self) -> bool` accessor (and optionally a `severity(&self) -> Severity`) so the scheduler and UI can branch on transient-vs-terminal without re-matching every variant. This is a derived accessor computed from the variant; it does not change the wire shape (it is not a serialized field unless a later decision adds it).
- Variant context fields where the brief's acceptance criteria need them, with their Rust types named so the generated TS is determined: the offending path as a `String` for a missing-local-path error (`fs.path_missing`, `fs.path_not_accessible`); the `git.too_old` found and required versions each as a `String`; the `github.rate_limited` `reset_at` as an `i64` unix-epoch seconds; the `git.command_failed` exit code as an `i32` and captured stderr as a `String`; the `config.invalid_setting` field name as a `String`. These context fields flatten into the `context` object of the payload.
- Unit tests asserting: every variant has a non-empty, unique code; every variant has a non-empty remediation; the serialized shape round-trips through `serde_json`; codes are stable string literals (a snapshot/golden test over the full code set).

## Out of scope

- The IPC command and event signatures that *carry* `AppError` (E-06 IPC contract owns the contract surface; this effort owns only the error type the contract references).
- The git operations that *raise* these errors (E-03 Git engine, E-07 Update-policy engine); this effort defines the error vocabulary, not the code paths that produce each variant.
- The DB rows or activity records that persist an error code (E-02 Persistence, E-09 Activity writer); `repo_local_state.last_error_code` stores a code this effort defines, but the storage is those efforts' work.
- Final remediation copy. Strings are authored now as correct, plain-language first drafts; a later copy pass refines them without touching codes.
- Localization / i18n of remediation strings (not a V1 concern).

## Contract / deliverables

1. `AppError` exists in `crates/reposync-core/src/error.rs` as a `thiserror` enum with the full enumerated variant set, grouped and documented by domain.
2. Every variant derives a stable machine code via a `code()` accessor; codes are unique, namespaced by domain, and asserted stable by a golden test.
3. Every variant carries an authored-now remediation string via a `remediation()` accessor, decoupled from the code.
4. `AppError` derives `serde::Serialize`/`Deserialize` and `specta::Type`; its serialized form is the frozen `AppErrorPayload` struct `{ code, message, remediation, context }`, and round-trips through `serde_json`.
5. `AppError` exposes a derived `retryable(&self) -> bool` accessor so E-07's network-retry and E-08's 3-strikes auto-pause can branch on it; it is computed from the variant and does not change the wire shape.
6. `reposync-core` still has no `tauri` dependency after this effort (the E-01 hygiene gate stays green).

## The enumerated taxonomy (V1)

Codes are namespaced `domain.specific`. Remediation strings shown are first-draft intent, not final copy. Every code is stable from the moment it lands.

### Git domain (`git.*`)

| Code | Variant intent | Remediation intent | Source |
|---|---|---|---|
| `git.not_found` | The `git` executable could not be located (PATH, known install locations, or settings override all failed). | "Git was not found. Install Git for Windows or set the path in Settings." | brief Section 4.10d |
| `git.too_old` | A `git` was found but is below the 2.30 floor the engine requires. Carries found + required version. | "Your Git is older than the supported minimum (2.30). Update Git." | brief Section 4.10d |
| `git.not_a_repo` | The local path exists but is not a git working tree. | "This folder is not a Git repository. Re-add the correct path." | brief Sections 6, 4.6 |
| `git.no_upstream` | The current branch has no configured upstream / tracking branch. | "This branch has no upstream. Set a tracking branch to check it." | brief Section 6 (fixture states) |
| `git.deleted_upstream` | The configured upstream / remote branch no longer exists (yanked or renamed). | "The upstream branch is gone. Update the remote or branch config." | brief Sections 2, 6 |
| `git.detached_head` | HEAD is detached (not on a branch), so fast-forward semantics do not apply. | "HEAD is detached. Check out a branch to enable updates." | brief Sections 2, 6 |
| `git.dirty_tree` | The working tree has uncommitted changes; policy skipped the update. | "Uncommitted changes were found. Commit or stash, then retry." | brief Sections 5, 6 |
| `git.ff_not_possible` | A `pull --ff-only` cannot fast-forward (history diverged). | "The branch has diverged and cannot fast-forward. Resolve manually." | brief Sections 5, 6 |
| `git.auth_failed` | A network git op failed authentication (credential helper rejected / token expired). | "Authentication failed. Refresh your Git credentials and retry." | brief Sections 2, 5, 6 |
| `git.fetch_failed` | A `fetch`/`pull` failed for a non-auth, non-network reason (captured exit code/stderr). | "The Git operation failed. See the activity log for the raw output." | brief Sections 6, 4.6 |
| `git.command_failed` | A git subprocess exited non-zero outside the more specific cases; carries exit code and captured stderr. | "A Git command failed. See the activity log for details." | brief Section 4.6 |

### Filesystem domain (`fs.*`)

| Code | Variant intent | Remediation intent | Source |
|---|---|---|---|
| `fs.path_missing` | The repo's recorded `local_path` no longer exists on disk. Carries the path. | "The repository folder is missing. Reconnect it or remove the repo." | brief Sections 2, 6 |
| `fs.path_not_accessible` | The path exists but cannot be read (permissions / lock). Carries the path. | "RepoSync cannot access this folder. Check permissions." | brief Section 4.10 |
| `fs.duplicate_repo` | The path (or its resolved canonical form) is already registered (`repos.local_path UNIQUE`). | "This repository is already being watched." | brief Sections 6, 4.5 |
| `fs.not_a_directory` | The supplied path is a file or otherwise not a directory. | "That path is not a folder. Pick a repository folder." | brief Section 4.5 |
| `fs.onedrive_synced` | The resolved app-data dir falls under a OneDrive-synced root (WAL corruption hazard). | "App data is in a synced folder, which can corrupt the database. Move it out of OneDrive." | brief Section 4.10c |

### Network / GitHub domain (`net.*`, `github.*`)

The `github.*` rows draw from two places now reflected in the frontmatter `source:` set: the brief's Section 6 GitHub-client workstream (which raises rate-limit, 404, and non-GitHub-remote conditions during enrichment) and the IPC command surface in Architecture Section 4.4 (where `repo_refresh_metadata` and the enrichment commands surface these errors to the frontend).

| Code | Variant intent | Remediation intent | Source |
|---|---|---|---|
| `net.offline` | A network operation failed because connectivity was lost. | "No network connection. RepoSync will retry automatically." | brief Sections 2, 5, 6 |
| `net.timeout` | A network request timed out. | "The request timed out. RepoSync will retry." | brief Section 5 |
| `github.rate_limited` | The GitHub API returned a rate-limit response. Carries the reset-at time. | "GitHub's rate limit was reached. Enrichment resumes after the reset time." | brief Sections 6, 4.4 |
| `github.not_found` | The GitHub resource (repo / release) returned 404. | "The GitHub repository was not found. It may be private or renamed." | brief Section 4.4 |
| `github.api_error` | A non-success GitHub API response outside the specific cases; carries status. | "GitHub returned an error. Metadata enrichment was skipped this cycle." | brief Section 4.4 |
| `github.not_a_github_remote` | The remote origin is not a GitHub host, so enrichment does not apply. | "This remote is not on GitHub, so release metadata is unavailable." | brief Sections 4.4, 4.5 |

### Database domain (`db.*`)

| Code | Variant intent | Remediation intent | Source |
|---|---|---|---|
| `db.migration_failed` | A startup migration failed; the DB is moved aside and a fresh one created. | "The database could not be upgraded. Your old data was backed up; see the log." | brief Section 4.10b |
| `db.locked` | A `database is locked` condition (e.g. a sync agent holding the WAL sidecars). | "The database is busy. Close other RepoSync instances and retry." | brief Sections 4.10b, 4.10c |
| `db.query_failed` | A general persistence/query failure; carries a sanitized cause. | "A database error occurred. See the log for details." | brief Section 4.5 |
| `db.not_found` | A requested row (repo id, activity id) does not exist. | "That item no longer exists. Refresh and try again." | brief Section 4.5 |

### Config domain (`config.*`)

| Code | Variant intent | Remediation intent | Source |
|---|---|---|---|
| `config.invalid_setting` | A settings value is out of range or malformed (e.g. cadence, quiet hours). Carries the field. | "A setting has an invalid value. Correct it in Settings." | brief Sections 4.4, 4.5 |
| `config.invalid_policy` | An update mode / policy combination is not valid for V1. | "That update mode is not available. Choose a supported mode." | brief Section 5 |
| `config.quiet_hours_malformed` | Quiet-hours configuration cannot be parsed. | "Quiet hours are misconfigured. Re-enter the time window in Settings." | brief Section 4.7 |

### Catch-all (`internal.*`)

| Code | Variant intent | Remediation intent | Source |
|---|---|---|---|
| `internal.unexpected` | A bug-class error that escaped the typed cases; carries a context string. Last resort only. | "Something went wrong inside RepoSync. Please file an issue with the log." | brief Section 6 (every failure must surface distinctly) |

## Acceptance criteria

- [ ] AC1: `AppError` is a `thiserror` enum in `crates/reposync-core/src/error.rs` covering the full enumerated taxonomy above (~30 variants across git, fs, net/github, db, config, plus the catch-all). Source: brief Section 6 (`AppError` workstream).
- [ ] AC2: Every variant exposes a stable, unique, domain-namespaced machine code via a `code()` accessor; a golden/snapshot test asserts the complete code set and fails on any rename. Source: brief Section 6 ("each mapped to a stable machine code"; "Machine codes are the contract").
- [ ] AC3: Every variant exposes an authored-now remediation string via a `remediation()` accessor, decoupled from the code so copy can be refined later without changing codes. Source: brief Section 6 ("plus a remediation string"; "Remediation strings can be authored now and refined as copy later without touching the codes").
- [ ] AC4: `AppError` derives `serde::Serialize`/`Deserialize` and `specta::Type`; its serialized form is the frozen `AppErrorPayload` struct `{ code: String, message: String, remediation: String, context: Option<serde_json::Value> }` (serialized as `{ code, message, remediation, context }`), and round-trips through `serde_json` in a test. Source: brief Sections 4.3, 6 ("Serialized across IPC as structured `AppError`").
- [ ] AC5: The variant set distinctly represents every failure the acceptance criteria require to surface as its own state: network lost, missing local path, deleted upstream, detached HEAD, auth failure, ff-not-possible, not-a-git-repo, duplicate, rate-limited, git-not-found. Source: brief Sections 2, 5, 6, 4.10d.
- [ ] AC6: `AppError` exposes a derived `retryable(&self) -> bool` accessor (computed from the variant, not a serialized field, so the wire shape is unchanged) that E-07 (Update-policy engine, network-retry) and E-08 (Scheduler, 3-strikes auto-pause) branch on; a test asserts the transient variants (`net.offline`, `net.timeout`, `github.rate_limited`, `db.locked`) are retryable and the terminal ones are not. Source: brief Sections 5, 6 (retry and auto-pause behavior).
- [ ] AC7: `reposync-core` has no `tauri`/`tauri-*` dependency after this effort; the E-01 dependency-hygiene gate stays green (`specta` is allowed; `tauri-specta` is not in core). Source: brief Section 4.3.

## Dependencies

- Upstream: E-01 (Foundation, workspace, CI) - owns the `error.rs` stub, the workspace, and the no-Tauri hygiene gate this effort must keep green.
- Downstream: E-06 (IPC contract) references `AppError` as the error half of every command return; E-03 (Git engine), E-07 (Update-policy engine), E-09 (Activity writer), and E-02 (Persistence) raise and persist these codes.

## V1.1 extension points

- Variants added in V1.1 (e.g. PAT/keyring auth errors for the deferred keyring GitHub token path) are **additive**: new codes, never renames or removals of V1 codes. The golden code test enforces this discipline.
- Remediation strings get a dedicated copy pass (and potentially i18n) without touching any code.
- The derived `retryable()` (and optional `severity()`) accessor lands in V1 (see Decisions); promoting either onto the serialized `AppErrorPayload` as an explicit wire field, if the frontend ever needs it directly, is a non-breaking additive change.

## Decisions

- **Resolved (retryable/severity): add a derived `retryable(&self) -> bool` accessor in V1.** The scheduler's network-retry (E-07 Update-policy engine) and the 3-strikes auto-pause (E-08 Scheduler) must branch on whether a failure is transient, so `AppError` exposes `retryable()` computed from the variant (and optionally `severity()` for warning-vs-hard-fail). These are derived accessors, not serialized fields, so they do not change the frozen `AppErrorPayload` wire shape; if a later decision wants `retryable` on the wire it is an additive field, not a rename. This closes the prior open question inside E-05 rather than deferring it.

## Open questions

- The exact split between `git.fetch_failed` and `git.command_failed` (specific network/auth fetch failure vs. generic non-zero git exit) - both are kept so the common, user-actionable fetch failure is distinct from the rare unexpected one, but the boundary is an engineering call to confirm during E-03 (Git engine).
