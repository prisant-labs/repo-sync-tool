---
effort: E-05
plan_for: spec.md
status: ready
---

# E-05 Implementation Plan

## Approach

Define the error vocabulary once, exhaustively, and lock the codes with a golden test before any other effort starts raising them. Build it as a single self-contained module in `crates/reposync-core/src/error.rs` with no I/O and no Tauri: a `thiserror` enum, the stable accessors (`code()` and `remediation()`) plus the derived `retryable()`, the frozen `AppErrorPayload` serialized form, and tests that prove uniqueness, stability, and round-trip. The discipline that matters is not the code volume (the enum is mechanical) but the up-front decisions: the namespacing convention, the code-vs-copy separation, and the snapshot that makes a rename fail CI. Author remediation copy as correct first drafts now; treat the codes as frozen the moment they land.

## Steps

1. **Fill the `error.rs` stub.** Replace the E-01 (Foundation) placeholder in `crates/reposync-core/src/error.rs` with the real `AppError` `thiserror` enum. Add `thiserror`, `serde`, and `specta` to `reposync-core/Cargo.toml` from the workspace dependency table (all pinned per E-01). Confirm none of these drag `tauri` (re-run the E-01 hygiene gate).
2. **Enumerate variants by domain.** Add every variant from the spec's taxonomy, grouped with section comments: git (`git.*`), filesystem (`fs.*`), network/GitHub (`net.*`/`github.*`), database (`db.*`), config (`config.*`), and the `internal.unexpected` catch-all. Give each variant the context fields the spec calls for (path for `fs.path_missing`, found+required version for `git.too_old`, reset-at for `github.rate_limited`, exit code/stderr for `git.command_failed`, etc.). Annotate each with `#[error("...")]` for the `Display`/`message` text.
3. **Add the stable code scheme.** Implement `fn code(&self) -> &'static str` returning the `domain.specific` literal for each variant. Keep the codes in one match so the full set is reviewable in one place. Document in a module doc comment that codes are a frozen contract: additive-only, never renamed.
4. **Add the remediation scheme.** Implement `fn remediation(&self) -> &'static str` returning the authored-now string per variant, in a separate match from `code()` so copy and identity are visibly decoupled. Note in the doc comment that these strings are revisable copy.
4b. **Add the derived `retryable` accessor.** Implement `fn retryable(&self) -> bool` (and optionally `fn severity(&self) -> Severity`) as a derived accessor over the variant set: transient cases (`net.offline`, `net.timeout`, `github.rate_limited`, `db.locked`) return `true`, terminal cases return `false`. This is a third match, separate from `code()` and `remediation()`, so retry/severity logic never entangles with the code identity. It is computed, not serialized, so it does not touch the `AppErrorPayload` wire shape. E-07 (Update-policy engine) network-retry and E-08 (Scheduler) 3-strikes auto-pause branch on it.
5. **Implement the frozen serialized wire shape.** The shape is decided, not open: serialize through the `AppErrorPayload` struct `{ code: String, message: String, remediation: String, context: Option<serde_json::Value> }`, serialized as `{ code, message, remediation, context }`, built from an `AppError` via `From<&AppError>` (code from `code()`, message from `Display`, remediation from `remediation()`, context as the flattened per-variant fields or `null`). Derive `specta::Type`, `serde::Serialize`/`Deserialize`, `Debug`, `Clone` on `AppErrorPayload`. The struct form is chosen over a serde-tagged enum so `specta::Type` generates a single clean TypeScript object for E-06 (IPC contract) to mirror, not a discriminated union. This is frozen the moment it lands.
6. **Write the golden code test.** A test that collects the code of one constructed instance of every variant and asserts the full sorted set equals a committed snapshot. This is the contract guard: adding a variant updates the snapshot deliberately; renaming or removing one fails the build. Pair it with a test asserting all codes are unique and all are non-empty, domain-prefixed strings.
7. **Write the remediation + retryable + round-trip tests.** Assert every variant has a non-empty remediation. Assert `retryable()` returns `true` for the transient variants (`net.offline`, `net.timeout`, `github.rate_limited`, `db.locked`) and `false` for the terminal ones. Assert a representative sample (at least one per domain, including variants with context fields) round-trips through `serde_json::to_string` then `from_str` as the `AppErrorPayload` shape with `code`/`message`/`remediation`/`context` intact, with `context` typed as expected (e.g. `github.rate_limited` carries `reset_at` as a number, `git.too_old` carries found/required as strings).
8. **Verify.** `cargo test -p reposync-core`, `cargo clippy --all -- -D warnings`, and the `cargo tree -p reposync-core | grep -i tauri` hygiene gate all green locally; push and confirm the matrix is green on both runners.

## Test strategy

- **Golden code snapshot (the most valuable test in this effort).** The committed set of all machine codes is the contract; the snapshot fails CI on any rename or accidental removal, which is exactly the guardrail an agent-driven build needs for a frozen vocabulary.
- **Coverage invariants over the variant set.** Uniqueness of codes, non-emptiness of codes and remediations, and domain-prefix conformance are asserted by iterating a hand-maintained list of one constructed instance per variant. The list itself doubles as documentation that every variant was considered.
- **Serde round-trip against the frozen `AppErrorPayload` shape.** Confirms the wire shape `{ code, message, remediation, context }` is stable and lossless for the fields the frontend depends on, including context-carrying variants with their named context types (e.g. `reset_at` as an `i64`, `git.too_old` found/required as `String`s), so E-06 (IPC contract) can mirror `AppErrorPayload` into the seam with confidence.
- No git, network, or DB is touched; the module is pure, so all tests are fast and deterministic and run headless on every runner.

## Files / modules touched

- `crates/reposync-core/src/error.rs` (the entire effort lands here).
- `crates/reposync-core/Cargo.toml` (add `thiserror`, `serde`, `specta` from the workspace table).
- `crates/reposync-core/src/lib.rs` (ensure `pub mod error;` and a `pub use error::AppError;` re-export if convenient for E-06).
- A test module (inline `#[cfg(test)]` in `error.rs`) plus the committed code snapshot.

## Risks and mitigations

- **Premature code churn.** If codes change after downstream efforts depend on them, the contract breaks silently. Mitigation: the golden snapshot test makes any rename a loud, deliberate CI failure; the module doc states the additive-only rule.
- **Code-vs-copy entanglement.** If presentation logic leaks into the code identity (e.g. encoding severity in the code string), copy revisions could force code changes. Mitigation: `code()`, `remediation()`, and `retryable()`/`severity()` are separate matches; the retryable/severity hint is a derived accessor, never part of the code.
- **Serialized shape churns the TS type.** A messy `serde` representation would produce an awkward generated TypeScript type in E-06. Mitigation: the wire shape is frozen here as the flat `AppErrorPayload` struct (`{ code, message, remediation, context }`, struct not tagged enum) and asserted with a round-trip test before E-06 consumes it.
- **Variant under-coverage.** Missing a failure state the acceptance criteria require would let a real failure fall through to the catch-all. Mitigation: AC5 enumerates the named states explicitly and the spec table cites each to a brief section; the fixture harness (E-04) later exercises the git states against real repos.

## Definition of done

All seven acceptance criteria checked: the full enum exists with stable namespaced codes and authored remediations, the golden snapshot and round-trip tests pass, `AppError` derives `serde` + `specta::Type` and serializes as the frozen `AppErrorPayload` shape, the derived `retryable()` accessor is present and tested, the named failure states from AC5 are all distinctly representable, and the no-Tauri hygiene gate stays green. CI green on both runners and the branch ready for self-merge per the visibility-tiered policy in `EXECUTION.md`.
