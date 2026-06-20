---
effort: E-03
plan_for: spec.md
status: ready
---

# E-03 Implementation Plan

## Approach

Build the read side first (it is the cheapest to test and unblocks the cross-check in E-04), then the CLI side and its parsers, then discovery and version probing, and finally the "git unavailable" state plumbing. Lock the `git2`/`libgit2-sys` build configuration before writing any `git2` code so Windows native-build friction surfaces at dependency-resolution time, not mid-effort. Keep every parser a pure function over captured bytes so it can be unit-tested from string fixtures with no real repo, and so E-04 can reuse those same parsers against real repos.

## Steps

1. **Pin `git2` with vendored, no transports.** In `reposync-core/Cargo.toml`, depend on `git2`/`libgit2-sys` with `default-features = false` and the `vendored` feature, with network transports (OpenSSL, libssh2) disabled. Confirm `cargo build` on Windows pulls no OpenSSL. This is the single highest-friction dependency in the project (brief Section 4 / Architecture subsection 6, "Git engine"); settle it first.
2. **Declare the `GitEngine` trait.** In `git/mod.rs`, define the trait covering the network/mutation operations (CLI-backed) and the cheap reads (git2-backed), expressed so a second all-CLI read impl is a drop-in. Define the engine-level result/state types here, including the distinct "git unavailable" state value. Construction must succeed in the degraded state (no `git` present yields a usable engine in the "git unavailable" state, never an `Err`), so app launch is never gated on git discovery (AC9). Model ahead/behind as `Option`-shaped so the no-upstream and deleted-upstream states can report `None` rather than `(0, 0)` (AC11).
3. **Implement `inspect.rs` (git2 reads).** HEAD SHA, active branch, dirty status (working-tree changes), detached-HEAD detection, and ahead/behind counts via `git2`. Never opens a network transport, never mutates. These are the cheap-read half of the boundary rule.
4. **Implement `cli.rs` execution.** A single `tokio::process::Command` runner that takes the resolved git path plus args and returns a captured struct: `raw_command`, `raw_stdout`, `raw_stderr`, `exit_code`, `duration_ms`. Every later CLI operation goes through this one capture point so the audit-trail fields (E-09) are uniform and unmissable.
5. **Write the pure parsers.** One pure function per output format, each taking captured stdout (and exit code where relevant) and returning a typed result, with zero I/O:
   - `fetch` (classify into at least success / no-op / auth-failure / network-failure / unknown from stderr + exit code; "unknown" is the conservative fallback, and the auth-failure vs network-failure split is what E-07 keys its pause-vs-retry decision on),
   - `rev-parse` (resolve a ref or HEAD to a SHA),
   - `status --porcelain=v2` (clean vs dirty, tracked/untracked entries),
   - `for-each-ref` (branch and upstream enumeration),
   - `rev-list --left-right --count` (ahead/behind as two integers).
   Unit-test each from string fixtures captured from real `git` output.
6. **Git discovery.** Resolve `git` in order: explicit `settings.git_executable_path`, then `PATH` lookup, then well-known Windows locations (`%ProgramFiles%\Git\cmd\git.exe`, Scoop/winget shims). Cache the resolved path. Keep the lookup list a single table so the macOS port adds its entries in one place.
7. **Version probing + floor.** Run `git --version` through the `cli.rs` runner, parse the version, enforce the **>= 2.30** floor, and return a clear, non-blocking below-floor warning rather than failing. Re-probe on settings change.
8. **"git unavailable" state + re-probe.** Make a missing or below-floor git a first-class state value the shell can read, and expose a callable re-probe entry point that flips the state off "git unavailable" on a later successful discovery. This effort owns the signal and the re-probe and asserts only those in isolation (a distinct state value plus a re-probe that flips it); the scheduled-check pause/resume BEHAVIOR is E-08's loop control, which consumes this signal and re-probe (AC8).
9. **Verify.** Run `cargo test -p reposync-core` (parser unit tests + a minimal git2 read smoke), `cargo clippy --all -- -D warnings`, and confirm the dependency-hygiene gate and the no-OpenSSL build hold on Windows CI.

## Test strategy

- **Parsers: pure unit tests from string fixtures.** Capture representative real `git` output for each command (clean, dirty, ahead, behind, detached, no-upstream, deleted-upstream, auth failure, network failure) and assert the parser's typed result. This is the bulk of the test surface and needs no real repo.
- **Discovery + version probe:** unit-test the version parser and the floor comparison directly; test the discovery ordering with injected candidate paths so it does not depend on the host's real `git`.
- **`inspect.rs`:** a minimal smoke test against a throwaway repo here; the exhaustive, deterministic exercise across all 7 states and the git2-vs-CLI agreement check live in E-04 and reuse these parsers and reads.
- The capture struct from `cli.rs` is asserted to populate all five fields (`raw_command`, `raw_stdout`, `raw_stderr`, `exit_code`, `duration_ms`) so E-09 can rely on them.

## Files / modules touched

- `crates/reposync-core/Cargo.toml` (pin `git2`/`libgit2-sys` vendored, no transports).
- `crates/reposync-core/src/git/mod.rs` (`GitEngine` trait, engine result/state types, "git unavailable" state).
- `crates/reposync-core/src/git/cli.rs` (`tokio::process::Command` runner + capture; the five pure parsers).
- `crates/reposync-core/src/git/inspect.rs` (`git2` reads).
- Git discovery + version probing live alongside the engine (in `git/mod.rs` or a small `git/discover.rs` submodule; keep it inside the `git/` module either way).
- Parser test fixtures under the crate's test tree.

## Risks and mitigations

- **`libgit2-sys` native build friction on Windows.** Mitigated by `vendored` + no OpenSSL/libssh2 (brief Section 4 / Architecture subsection 6, "Git engine") and by pinning the dep first (step 1). The `GitEngine` trait is the deeper insurance: an all-CLI read impl is a localized swap if the toolchain ever wins.
- **Porcelain v2 output drift across git versions.** The 2.30 floor plus pinned git in CI (E-04) stabilizes the output the parsers see; the parser fixtures are captured from a known git version.
- **`git2` ahead/behind disagreeing with `rev-list --left-right --count`** on no-upstream / deleted-upstream. Surfaced deliberately by the E-04 cross-check; this effort keeps both behind the trait so the agreed-on resolution is a contained change. Flag any disagreement to the spec's open questions.
- **Credential-helper-driven `fetch` behavior is environment-dependent.** Parsers classify from exit code + stderr text, which is inherently fuzzy; keep classification conservative (unknown-failure is a valid result) and refine as real captures accumulate.

## Definition of done

All eleven acceptance criteria checked: both engines behind the `GitEngine` trait, all five parsers pure and unit-tested (with the `fetch` parser classifying at least success / no-op / auth-failure / network-failure / unknown), `git2` vendored with no network transports building clean on Windows, discovery + the 2.30 floor enforced, "git not found" representable as a first-class state with a distinct value and a re-probe that flips it (pause/resume behavior deferred to E-08), engine construction succeeding in the degraded state so launch is never gated on git discovery, and the provisional `None` ahead/behind contract for the no-upstream and deleted-upstream states in place for E-04 to ratify. `cargo test`, `cargo clippy --all -- -D warnings`, and the dependency-hygiene gate green on Windows CI, and the branch ready for self-merge per the visibility-tiered policy in `EXECUTION.md`.
