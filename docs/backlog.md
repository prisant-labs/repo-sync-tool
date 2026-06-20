# RepoSync Backlog - ideas and deferred work

This is the living, durable home for everything not in the current build slice: features cut to a later version, decisions that are a human's to make, technical questions flagged non-blocking in the effort specs, and forward-looking tech hardening. The current build slice itself lives in [AGENTS/efforts/README.md](../AGENTS/efforts/README.md) (the V1 execution plan) and the per-effort `spec.md` files; this file holds what is deliberately outside it.

It is a seed, kept accurate now and grown as each effort lands. When an item moves into a build slice, give it an effort and move it out of here; when a new idea surfaces, capture it in "New ideas" below.

Adversarial reviews (Claude and Codex passes over the plan, specs, and code) feed new items into the tables below as they surface gaps, risks, and questions.

## Status legend

| Status | Meaning |
|---|---|
| deferred | Agreed for a later version (V1.1); not in V1 scope |
| open | Awaiting a decision or an answer; non-blocking for current work |
| watch | A future condition to monitor; no action yet |
| seam-stubbed | The integration point exists in V1 code behind a seam; the feature plugs in later |

## V1.1 cut features

Cut from V1 by the ratified scope ledger ([AGENTS/efforts/README.md](../AGENTS/efforts/README.md), scope ledger; rationale in [docs/internal/v1-architecture-and-decisions.md](internal/v1-architecture-and-decisions.md) Section 3). Each is either heavy, optional, or unverifiable on Windows-only hardware. Some are already seam-stubbed in V1 code so the V1.1 feature is a thin plug-in, not a retrofit; the rest are pure UI surface and out of the V1 efforts entirely.

| ID | Feature | Source / rationale | Seam status | Status |
|---|---|---|---|---|
| BL-V11-01 (tray popup window) | Frameless left-click popup window anchored near the tray icon (the native right-click menu is kept in V1). | Scope ledger; brief Section 3 (heavy, OS-specific geometry, unverifiable on macOS from Windows). | UI surface; out of the V1 efforts entirely. Native menu stays. | deferred |
| BL-V11-02 (keyring PAT) | OS-keychain-backed GitHub Personal Access Token for the authenticated rate limit (5000/hour vs 60/hour unauthenticated). | Scope ledger; brief Section 3 (a three-platform credential vault for an optional rate-limit lift most personal users will not hit). | Seam-stubbed in E-10: a `TokenProvider` whose V1 impl returns `None`; the V1.1 PAT impl reads from Windows Credential Manager / macOS Keychain and flips `settings.github_token_present`, leaving fetch/cache/backoff untouched. See [E-10 (GitHub client) spec](../AGENTS/efforts/E-10-github-client/spec.md) AC5 and V1.1 extension points. | seam-stubbed |
| BL-V11-03 (weekly summary) | Weekly aggregation card, alongside the daily summary V1 ships. | Scope ledger; brief Section 3. | Seam-stubbed in E-11: weekly is left as a V1.1 extension point on the daily aggregation. See [E-11 (summary engine) spec](../AGENTS/efforts/E-11-summary-engine/spec.md). The `summary_week() -> WeeklySummary` IPC command exists in the brief's surface (Section 4.4). | seam-stubbed |
| BL-V11-04 (grouping / tags) | User-defined groups and tags for repos (the mockups show a Groups sidebar). | Scope ledger; brief Section 3. | UI surface; out of the V1 efforts entirely. | deferred |
| BL-V11-05 (saved filters) | Persisted, named filters over the repo list. | Scope ledger; brief Section 3. | UI surface; out of the V1 efforts entirely. | deferred |
| BL-V11-06 (custom command recipes) | User-defined command recipes per repo. | Scope ledger; brief Section 3. | UI surface; out of the V1 efforts entirely. | deferred |
| BL-V11-07 (auto-updater) | In-app self-update. | Scope ledger; brief Section 3. | Out of the V1 efforts entirely; depends on a signing posture not yet set (see BL-DEC-01). | deferred |

## Deferred human decisions

Calls that are jp's to make, not the agent's. Each binds at a specific moment, not at the start; until then they are non-blocking. Source: [EXECUTION.md](../EXECUTION.md) (human-only list) and [docs/internal/v1-architecture-and-decisions.md](internal/v1-architecture-and-decisions.md) decision ledger and Section 7.

| ID | Decision | Source / rationale | Binds when | Status |
|---|---|---|---|---|
| BL-DEC-01 (Windows code-signing) | Procure a Windows code-signing certificate, or adopt Azure Trusted Signing, and store signing secrets in CI. | EXECUTION.md human-only list; brief decision ledger (Code signing: ship first public build unsigned, add Windows signing as a fast-follow). Money + identity validation by a CA; credential custody. | Near public ship. Until then, CI produces an unsigned artifact and documents the signing step. | open |
| BL-DEC-02 (Apple enrollment + macOS signing) | Apple Developer Program enrollment, plus storing Apple notarization secrets in CI, to produce a signed/notarized macOS build. | EXECUTION.md human-only list; brief Section 2 (cannot notarize from Windows; needs a Mac or a macOS CI runner holding Apple credentials). Money (paid annual fee) + legal identity. | When macOS GA is pursued. Gated by a descope trigger: if not unblocked by end of week 4, drop macOS from the V1 GA bar and ship Windows-only GA (brief Section 3 triggers). | open |
| BL-DEC-03 (go-public timing) | Flip the repo from private to public. | EXECUTION.md human-only list; brief decision ledger (go public at Phase 0 exit; `_LOCAL/` quarantined; one-way door). Publishing decision, irreversible in practice; also flips the merge policy from agent self-merge to human-reviewed. | At Phase 0 exit. | open |
| BL-DEC-04 (license confirmation) | Confirm the open-source license. | Brief decision ledger (MIT default, Apache 2.0 defensible); [E-01 (foundation) spec](../AGENTS/efforts/E-01-foundation/spec.md) open question. MIT is used now and ships in V1's `LICENSE`. | Binds at first public commit. Use MIT now; reconfirm at go-public. | open |
| BL-DEC-05 (brand / product name) | Final product name. | Brief decision ledger (keep "RepoSync" working title; isolate the brand string to one constant). | Deferred to pre-GA. The brand string is isolated to a single constant so a rename is cheap. | open |

## Open technical questions

Flagged non-blocking in the effort specs (the corresponding effort proceeds on a stated default and confirms during integration). Each links to where it is recorded.

| ID | Question | Default / direction | Source | Status |
|---|---|---|---|---|
| BL-TQ-01 (fetch_failed vs command_failed) | The exact boundary between `git.fetch_failed` (common, user-actionable fetch failure) and `git.command_failed` (rare unexpected non-zero git exit). | Keep both codes so the common case is distinct from the rare one; confirm the boundary during E-03 wiring. | [E-05 (error taxonomy) spec](../AGENTS/efforts/E-05-error-taxonomy/spec.md) open questions. | open |
| BL-TQ-02 (repo_open_* return shape) | Whether `repo_open_folder/terminal/editor/remote` return `Result<(), AppError>` or a richer result. | `Result<(), AppError>`, since they shell out to a success-or-error outcome; confirm during E-03/E-09 wiring. | [E-06 (IPC contract) spec](../AGENTS/efforts/E-06-ipc-contract/spec.md) open questions. | open |
| BL-TQ-03 (E-07 attention threshold consumed by E-11) | Whether the daily summary should adopt E-07's threshold-based "needs attention" definition once E-07 lands, so the summary and the Repos view agree. | V1 uses a deliberately E-07-free definition (`last_error_code` set OR `is_dirty`); decide on alignment after E-07 lands. Until then the two may diverge for behind-but-not-errored repos. | [E-11 (summary engine) spec](../AGENTS/efforts/E-11-summary-engine/spec.md) open questions; [E-07 (update-policy engine) spec](../AGENTS/efforts/E-07-update-policy-engine/spec.md). | open |
| BL-TQ-04 (fixtures on macOS runner) | Whether the E-04 git fixture suite runs on the macOS CI runner, or Windows-and-Linux only, given pinned-git byte-stability is the goal. | Not yet pinned in the spec; resolve when the CI matrix is wired against pinned git. | Raised against the [E-04 (fixture harness) spec](../AGENTS/efforts/E-04-git-fixture-harness/spec.md) (pinned-git stability) and [E-01 (foundation) spec](../AGENTS/efforts/E-01-foundation/spec.md) AC8 (CI pins git). | open |
| BL-TQ-05 (MSI vs NSIS installer) | The exact Windows installer format for V1. | Default to whichever the Tauri bundler produces most reliably user-mode with `downloadBootstrapper`; flag the final pick at the packaging spike. | [E-12 (tracer bullet + packaging) spec](../AGENTS/efforts/E-12-tracer-bullet/spec.md) open questions. | open |

## Tech watch and future hardening

Forward-looking items to monitor or harden; no action required now.

| ID | Item | Source / rationale | Status |
|---|---|---|---|
| BL-TW-01 (specta RC to stable) | Upgrade `tauri-specta` and `specta` from the pinned release-candidate versions (2.0.0-rc.x) to stable once released, and re-check the codegen. | Brief Section 4.4 (v2 is RC-stage; pin exactly and re-check at each dependency review); [E-01 (foundation) spec](../AGENTS/efforts/E-01-foundation/spec.md) AC7. The pinning prevents a silent RC bump from breaking codegen. | watch |
| BL-TW-02 (VS Build Tools 2019 to 2022) | Upgrade the build machine's Visual Studio Build Tools from 2019 to 2022. | Build-machine state noted in [E-01 (foundation) spec](../AGENTS/efforts/E-01-foundation/spec.md) Task Summary (MSVC Build Tools 2019 present). Watch for a need to move to 2022. | watch |
| BL-TW-03 (Linux WebKitGTK smoke target) | Add a Linux WebKitGTK CI smoke target as a cheap WKWebView-divergence canary (WebKitGTK is not WKWebView but catches a meaningful share of WebKit-vs-Chromium CSS/JS divergence for free, no Mac hardware). | Brief Section 4.2 (WebView-divergence mitigations); [E-01 (foundation) spec](../AGENTS/efforts/E-01-foundation/spec.md) V1.1 extension points. | watch |
| BL-TW-04 (real macOS signing/notarization CI job) | Add a real macOS signing/notarization CI job once Apple credentials exist (`codesign` to `xcrun notarytool` to `stapler`, on a macOS runner holding Apple secrets). | Brief Section 2; [E-12 (tracer bullet + packaging) spec](../AGENTS/efforts/E-12-tracer-bullet/spec.md) (the V1 deliverable is the documented runbook, not the exercised job); [E-01 (foundation) spec](../AGENTS/efforts/E-01-foundation/spec.md) V1.1 extension points. Gated by BL-DEC-02. | watch |

## New ideas

Open capture for anything not yet placed above. Adversarial reviews (Claude and Codex) also feed items here.

| ID | Idea | Source / rationale | Status |
|---|---|---|---|
| BL-NI-01 (capability / window-label consistency) | The default capability binds `windows: ["main"]` but `app.windows` is empty. Reconcile when the real window model lands: create a `main` window, declare it with `create: false`, or scope the capability to the actual windows. | Codex adversarial review of E-01. Schema-valid now; fragile for E-06 / the GUI effort. | open |
| BL-NI-02 (macOS CI unsigned-bundle flag) | Use `cargo tauri build --ci --no-sign` for the macOS unsigned CI bundle step rather than plain `cargo tauri build` (verify the flag exists in the Tauri CLI 2.11.x first). | Codex adversarial review of E-01. Plain build is already unsigned without signing config, so non-blocking; revisit at the E-12 packaging spike. | watch |
| BL-NI-03 (robust exact git pin on CI) | Implement a reliable way to install/select an EXACT `git` version on the Windows CI runner for the E-04 fixture harness (needed for byte-stable porcelain). Three mechanisms failed or were fragile during E-01: Homebrew exact-assert (macOS has no historical versions), `choco install git --version` (installer exits 1 over the preinstalled git), and portable MinGit (its PATH did not shadow the runner's stock git). Candidates: debug the MinGit cmd/mingw64 path layout, a pinned container, or `setup-git`-style action. Until then CI verifies the >= 2.30 floor only. | E-01 CI iteration; owned by E-04 (the fixture harness defines the exact version). | open |
| BL-NI-04 (failed check emits completion event) | `check_now` records an activity row then returns `FetchFailed`, short-circuiting before `repo:check-completed` is emitted, so other windows never see failed completions. Return a typed failed `CheckResult` or add a failure-carrying completion event emitted before the error. | Codex review of E-12 tracer; owned by E-06 (IPC contract) / E-09 (activity). | open |
| BL-NI-05 (fetch classification enum) | The tracer's fetch result is only `success: bool`. E-03/E-07 need at least no-op / auth / network / unknown classes (from exit code + stderr) so the policy can pause-vs-retry correctly. | Codex review of E-12 tracer; owned by E-03 (git engine). | open |
| BL-NI-06 (IPC i64 id bounding) | `RepoId`/ids cross IPC as i64 cast to JS number (`dangerously_cast_bigints_to_number`). Timestamps/counts are within 2^53, but SQLite rowids are not formally bounded. Use string ids, a narrower id type, or enforce a max-safe-integer invariant. | Codex review of E-12 tracer; owned by E-06 (IPC contract). | open |
| BL-NI-07 (transactional multi-row writes) | `add` and `check_now` perform multiple INSERT/UPDATE without a transaction; a partial failure can leave `repos` without `repo_local_state`, or state updated without the activity audit row. Wrap each in a SQL transaction and check `rows_affected`. | Codex review of E-12 tracer; owned by E-02 (persistence). | open |
| BL-NI-08 (CI actions off deprecated Node 20) | CI annotations warn `actions/checkout@v4`, `actions/setup-node@v4`, and `pnpm/action-setup@v4` are forced from the deprecated Node 20 onto Node 24. Bump to versions targeting Node 24 (or pin runner behavior) before the warnings become failures. | E-01 CI run annotations. | watch |
| BL-NI-09 (CI build time vs release profile) | The Windows CI bundle took 35m15s, driven by `[profile.release]` (`opt-level="z"` + `lto=true` + `codegen-units=1`, tuned for the <30MB bundle). RESOLVED 2026-06-20 (jp ratified the fast-CI tradeoff): `[profile.release]` switched to `lto="thin"` + `codegen-units=16` for fast per-push CI; full LTO moved to a new `[profile.dist]` (`inherits="release"`, `lto="fat"`, `codegen-units=1`) for release-tag artifacts; CI now builds via the prebuilt `pnpm tauri build` instead of `cargo install tauri-cli`. Verify the actual per-push build time on the next run, and wire the release workflow to select `--profile dist`. | E-01 CI run (35m15s Windows leg). | done |
