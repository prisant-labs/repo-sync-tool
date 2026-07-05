# CI and release pipeline plan (v0.9.0)

Phase 0 (Rails) plan for the continuous-integration and release-tag workflows. Scope: diagnose why PR #2 (build RepoSync V1) is red on all four checks, prescribe the `ci.yml` fixes, define the test-tiering strategy that makes the suite completable, and specify the `release.yml` pipeline for the private v0.9.0 tag. Owning effort: E-01 (foundation, workspace, and CI) owns `ci.yml`; the release pipeline touches E-18 (auto-update and distribution).

This is a plan, not an execution log. No workflow file is edited here. The ordered checklist in Section 6 is what the Phase 0 build agents execute after "go".

Related docs:

- `docs/internal/release-plans/runbook_cut-tag-release.md` - the G0 through G4 tag ceremony that consumes a green CI. This plan supplies the mechanics the runbook assumes.
- `docs/internal/release-plans/plan_v0.9.0/plan_v0.9.0.md` - the release plan and readiness checks.
- `docs/internal/release-plans/plan_v0.9.0/E-18-auto-update/spec.md` - the updater plugin, signing, and winget-prep work that `release.yml` consumes.
- `_LOCAL/audit/2026-07-04_18-21_fable-audit.md` - the audit that surfaced the red CI and the non-completing test suite (facts table; findings 3 and the test-suite fact).

## 1. Root-cause diagnosis

### 1.1 The single blocker: `cargo fmt --all -- --check` fails

Both failing runs, 28696799432 (push) and 28696800727 (pull_request), fail identically on all four legs (windows-latest and macos-latest on each run). Every leg gets through checkout, git-floor verify, Rust install, rust-cache, pnpm, Node, and `pnpm install --frozen-lockfile`, then dies at the very first gate, `cargo fmt (check)` (`ci.yml:103`), after 1 to 2 minutes. Nothing past that step runs: `cargo check`, `cargo clippy`, `cargo test`, the bindings gate, typecheck, lint, the tauri-free assertion, and `Build and bundle` all report skipped.

Windows leg (job 85107746188) excerpt:

```
Run cargo fmt --all -- --check
Diff in ...\crates\reposync-core\src\store.rs:1126:
         let r1 = insert_repo(&pool, "alpha", "C:/repos/alpha").await;
         let r2 = insert_repo(&pool, "beta", "C:/repos/beta").await;
-        group_assign(&pool, r1, backend.id).await.expect("assign r1");
-        group_assign(&pool, r2, backend.id).await.expect("assign r2");
+        group_assign(&pool, r1, backend.id)
+            .await
+            .expect("assign r1");
+        group_assign(&pool, r2, backend.id)
+            .await
+            .expect("assign r2");
Diff in ...\crates\reposync-core\src\store.rs:1182:
Diff in ...\crates\reposync-core\src\store.rs:1189:
Diff in ...\crates\reposync-core\src\store.rs:1194:
Diff in ...\src-tauri\src\commands\mod.rs:10:
Diff in ...\src-tauri\src\lib.rs:24:
##[error]Process completed with exit code 1.
```

macOS leg (job 85107746202) is the same six diff sites, exit code 1.

Root cause: the 2026-07-03 build session (groups, tray, cadence; commits 8fc806c..03a5ef6) committed Rust that was never run through `cargo fmt`. The offending files are exactly the ones that session touched:

- `crates/reposync-core/src/store.rs` at lines 1126, 1182, 1189, 1194 (new groups tests; rustfmt wants the `.await.expect(...)` chains broken across lines).
- `src-tauri/src/commands/mod.rs:10` (import block).
- `src-tauri/src/lib.rs:24` (the multi-item event `use` that grew when tray and groups events landed).

The `cargo fmt` gate itself is healthy and doing its job. It was added deliberately in commit d9b3225 ("normalize rustfmt across the tree + enforce fmt in CI") precisely because formatting drift had accumulated unnoticed once before. The gate is not the problem; unformatted commits are. The fix is mechanical: run `cargo fmt --all` and commit the result.

### 1.2 Why this matters more than a formatting nit: the fail-fast mask

`cargo fmt` is the first gate step, and it fails before anything compiles. That means CI has not actually run `cargo check`, `cargo clippy`, `cargo test`, or `pnpm tauri build` against the tree since the GUI, the tray (`tray-icon` feature), and groups landed. `ci.yml` was last touched at commit 2abf5b0 (E-04, git fixture harness, 2026-06-23), well before any of that work. So beyond fixing fmt, we currently have zero CI evidence that the shell crate even compiles on the runners with the real React frontend and the `tray-icon` feature enabled.

The green past the fmt step is unknown. Two latent failures are predictable from the audit and from reading the tree, but neither can be confirmed until CI gets past fmt (no local cargo runs are permitted for this diagnosis):

### 1.3 Predicted latent failure A (confirmed by the audit): `cargo test --workspace` does not complete

This one is not a guess. The audit's facts table records `cargo test --workspace` as DID NOT COMPLETE in 10 minutes, twice, with no failing test observed: 105 of 276 core unit tests in about 15 minutes single-threaded, with the git-fixture tests dominating because each fixture spawns many `git` subprocesses. The tree confirms the mechanism: `crates/reposync-core/src/git/fixtures.rs` builds real repositories with `Command::new("git")` calls (init, config, add, commit, symbolic-ref), and the fixtures are consumed by roughly a hundred tests across `git/cli.rs` (41), `git/discover.rs` (15), `git/inspect.rs` (6), `git/mod.rs` (7), `git/fixtures.rs` (14), `policy.rs` (37), and `repo.rs` (18), plus three integration tests (`git_fixture_cross_check`, `policy_fixture_matrix`, `scheduler_integration`). Thousands of process spawns, run single-threaded, on a hosted Windows runner where process creation is slower than local. The `cargo test --workspace` step at `ci.yml:113` is a multi-minute-to-effectively-hanging wall that the fmt error is currently hiding. Section 3 addresses it.

### 1.4 Predicted latent failure B (verify, do not assume): frontend dist absent at compile time

`dist/` is gitignored (`.gitignore:10`), so a fresh CI checkout has no `../dist`. The workflow compiles the `reposync` shell crate at `cargo check` (`ci.yml:107`), `cargo clippy` (110), `cargo test` (113), and the `export_bindings` test (132), all before the only step that builds the frontend, `pnpm tauri build` at line 169 (which runs `beforeBuildCommand: pnpm build` from `tauri.conf.json`). Tauri v2's `generate_context!` embeds `frontendDist` at compile time; a missing `../dist` at those earlier compile points is a plausible failure the fmt error is masking.

Honesty caveat: this may have been tolerated historically (some Tauri v2 versions emit a warning and an empty asset set for a missing dir rather than failing), so treat it as "verify once CI gets past fmt", not a certainty. Either way the fix is cheap and worth doing unconditionally: build the frontend once, early, so every cargo step that links the shell crate embeds a real dist. See Section 2.2.

### 1.5 What is NOT broken

- The runner OS matrix, checkout, git-floor verify, and toolchain install all pass.
- `pnpm install --frozen-lockfile` passes, so the committed `pnpm-lock.yaml` is current.
- The static gates the audit ran locally all pass (clippy, typecheck, lint, build), so the code is healthy under formatting; CI simply never reaches them.

## 2. `ci.yml` fix plan

### 2.1 Unblock fmt (mechanical, not a workflow change)

Run `cargo fmt --all` on the PR #2 branch and commit the formatting-only diff. Keep the `cargo fmt (check)` gate as is. This is step 1 of Phase 0 and its real value is not the formatting itself; it is getting a CI run that reaches the later steps so we finally learn the true post-GUI state.

### 2.2 Build the frontend before the cargo steps

Insert an explicit `pnpm build` step immediately after `Install frontend dependencies` and before `cargo fmt`/`cargo check`/`cargo clippy`/`cargo test`. Effects:

- `../dist` exists for `generate_context!` at every point the shell crate compiles, removing the ambiguity in Section 1.4.
- A frontend build break surfaces early and on its own, instead of being buried inside `pnpm tauri build` at the end.

`pnpm tauri build` still re-runs `pnpm build` via `beforeBuildCommand`; the redundant rebuild is about 28 seconds (the audit's measured frontend build time) and is acceptable. Do not remove `beforeBuildCommand` from `tauri.conf.json`, since `pnpm tauri dev` and local builds rely on it.

### 2.3 Collapse `cargo check` into clippy and strengthen the clippy scope

Two issues with the current Rust gates:

- `cargo check --workspace --all-targets` (`ci.yml:107`) and `cargo clippy --all -- -D warnings` (110) both compile overlapping graphs. Clippy runs the compiler; a clippy pass with `--all-targets` subsumes the check pass. Drop the standalone `cargo check` step and let clippy be the type-check gate.
- The current clippy line lacks `--all-targets`, so it lints only lib and bin targets and skips tests, examples, and benches. This is a real gap: CI clippy is weaker than the local gate the audit ran (`cargo clippy --workspace --all-targets`, which passed in 1m30s). New warnings in test code would not be caught in CI.

Replace both with a single `cargo clippy --workspace --all-targets -- -D warnings`. This saves one full compile pass and closes the coverage gap at the same time.

### 2.4 Toolchain

`dtolnay/rust-toolchain@stable` with `clippy, rustfmt` is correct and needs no structural change. One known hazard to flag: `@stable` floats, and combined with `-D warnings` a new stable release can turn CI red with no code change (a new clippy lint fires as an error). SHOULD, not MUST for Phase 0: add a `rust-toolchain.toml` at the repo root pinning the channel so local and CI agree and stable bumps become a deliberate PR rather than a surprise red. Track as a backlog item rather than blocking Phase 0.

### 2.5 pnpm and Node setup

No change. `pnpm/action-setup@v4` reads `packageManager` (`pnpm@10.33.4`) as the single source of truth; do not also pass a `version` input (the workflow comment already notes action-setup errors if both exist). `actions/setup-node@v4` with `node-version: 22` and `cache: pnpm`, then `pnpm install --frozen-lockfile`, is correct. The runner log line about "Node 20 deprecated, running Node 24" refers to the JS-action runtime, not our `node-version`; no action needed.

### 2.6 OS dependencies: intentionally thin, and keep it that way

The matrix is windows-latest and macos-latest only. There is no Linux leg, so none of the usual Tauri Linux system-dependency pain applies (no `apt-get`, no `libwebkit2gtk`, no `libgtk-3-dev`, and no `libayatana-appindicator3-dev` that the `tray-icon` feature would otherwise need on Linux). The Windows runner ships WebView2, MSVC, and the WiX and NSIS toolchains the bundler fetches on demand; the macOS runner ships the Xcode command-line tools. Nothing extra to install beyond the Rust and Node toolchains.

Call this out so a future contributor does not add a Linux leg without also adding its deps. If a Linux leg is ever added (not in scope for v0.9.0), it needs at minimum `libwebkit2gtk-4.1-dev`, `libgtk-3-dev`, `librsvg2-dev`, and `libayatana-appindicator3-dev` (the last for the tray icon). Document, do not add.

### 2.7 Caching strategy

Both caches are already present and correctly wired; the improvements are about stability, not adding caches:

- Rust: `Swatinem/rust-cache@v2` keys on `Cargo.lock` and the rustc version and caches the cargo registry plus the `target` dir (including the compiled vendored-libgit2 C objects). Keep it. Set `save-if: ${{ github.ref == 'refs/heads/main' }}` (extend to the active release branch during the v0.9.0 push) so only trunk writes the shared cache and PR branches read it. This stops each PR from poisoning or thrashing a shared cache; the trade-off is that a PR that changes dependencies builds those cold, which is acceptable.
- Keep `cache-on-failure: false` (via the existing `CACHE_ON_FAILURE` env). Do not cache a `target` dir from a failed run.
- Keep `CARGO_INCREMENTAL: "0"`. Incremental compilation hurts a clean-cache CI build and is correctly disabled.
- pnpm: the store is already cached by `setup-node`'s `cache: pnpm`, keyed on `pnpm-lock.yaml`. No change.

Note for expectations: the first green run after this branch merges will be cold-ish because the new dependencies (reqwest with rustls, the `tray-icon` feature, the growing shell crate) invalidate the prior cache. That is a one-time cost.

### 2.8 Keep the two contract gates

Do not lose these in the rework; they are load-bearing:

- The bindings-drift gate (`ci.yml:128`, Windows only): regenerates `src/lib/bindings.ts` via the `export_bindings` test and fails if the working tree drifts. This is the IPC contract guard (E-06). It stays Windows-only because the `export_bindings` test needs the comctl32-v6 manifest that `build.rs` attaches only on Windows and MSVC.
- The tauri-free assertion (`ci.yml:148`): `cargo tree -p reposync-core` must show no `tauri`. This is the foundation's most load-bearing invariant (E-01 AC2).

Both fold cleanly into the reworked job. The fixture-harness public-surface cross-check (`ci.yml:118`) moves into the full test lane (Section 3).

## 3. Test-tiering strategy

### 3.1 The problem, restated precisely

276 tests. Wall time is dominated not by any single slow test but by the sheer count of `git` subprocess spawns run single-threaded: about a hundred tests build real fixture repositories, each firing off several `git` invocations. The audit measured 105 of 276 tests in about 15 minutes single-threaded and the suite failing to finish in 10. On the hosted Windows runner, process creation is slower still. `cargo test --workspace` in CI is therefore either a multi-minute wall or an effective hang.

Two properties to fix: the suite must complete deterministically (a hung `git` must fail loudly, not stall the runner), and the PR gate must be fast.

### 3.2 The two tiers

- Fast unit tier (the PR gate): the pure-logic tests that never spawn `git` and never build a fixture repo. These live in `error.rs`, `ipc.rs`, `paths.rs`, `notify.rs`, `activity.rs`, `summary.rs`, `db.rs`, `store.rs`, `github.rs` (behind its transport seam, no network), `autostart.rs`, and the fake-clock portions of `scheduler.rs`. Seconds, not minutes.
- Slow git-fixture tier: everything that touches `git::fixtures`, namely `git/cli.rs`, `git/discover.rs`, `git/inspect.rs`, `git/mod.rs`, `git/fixtures.rs`, `policy.rs`, and `repo.rs`, plus the three integration tests. This tier is where the subprocess cost lives.

### 3.3 Options evaluated

Option A - cargo-nextest. A drop-in test runner that executes each test in its own process under a work-stealing parallel scheduler, with per-test timeouts, retries, and filter expressions. It does not reduce the number of `git` spawns, but it parallelizes them across all runner cores instead of running them single-threaded, which is exactly the bottleneck. It realistically turns the single-threaded 15-plus-minute run into a few minutes. Its filter expressions let us select a tier at the command line with zero source edits (for example, exclude the fixture-bearing modules for the PR gate). Its `--slow-timeout` gives a per-test timeout so a hung `git` fails loudly instead of stalling. Trade-offs: it is a new tool to install on the runner (via a setup action or `cargo-binstall`), the invocation differs from `cargo test`, and it does not run doctests (the core has effectively none, so this does not matter here).

Option B - feature-gating. Put the fixture-heavy tests behind a cargo feature (say `slow-tests`); the PR gate runs without it, a separate lane runs with it. Trade-offs: invasive and brittle. It requires `#[cfg(feature = "slow-tests")]` on many test modules across `cli.rs`, `discover.rs`, `policy.rs`, `repo.rs`, and the integration tests; it couples test selection to a compile-time feature (toggling forces a recompile); and it fragments what "cargo test" means locally, since developers must remember the flag. It also does nothing about the underlying single-threaded slowness of the tests that remain.

Option C - `#[ignore]` plus `--ignored`. Mark the slow tests `#[ignore]`; the PR gate runs plain `cargo test` (skips them), a separate lane runs `cargo test -- --ignored`. Trade-offs: the same per-test churn as Option B (an attribute on every slow test, easy to forget on new ones), it scatters the tier boundary across the tree instead of one place, and, like B, it does not address the single-threaded wall for the tests that do run.

### 3.4 Recommendation: adopt cargo-nextest as the primary lever; split by filter, not by source edits

> **Decision (2026-07-04, Phase 0 implementation):** the program went with Option C (`#[ignore]` + plain `cargo test`) instead of this section's nextest recommendation, because the execution plan's gate protocol standardizes on `cargo test --workspace` (fast tier) / `-- --ignored` (slow tier) with no new runner tooling, and the fixture tests parallelize acceptably under cargo's default thread scheduler (they use per-Command env and per-fixture tempdirs, not process-global state). The slow lane's wedge protection comes from the job-level `timeout-minutes: 30` budget instead of nextest's per-test `--slow-timeout`. Nextest remains the fallback lever if the fast tier regresses past the Section 3.6 targets.

Nextest attacks the actual bottleneck (single-threaded process spawning) directly and with zero source churn. Concretely:

- Replace the two `cargo test` steps with nextest. The PR gate runs the fast tier by excluding the fixture-bearing paths with a filter expression (the exact expression is tuned during implementation; the point is it lives in the workflow, not in `#[cfg]` attributes sprinkled through the tree). The integration tests are their own binaries and are excluded from the PR gate by kind.
- The full suite runs as a second lane, `cargo nextest run --workspace --features test-support`, which includes the fixture tier and the three integration tests, plus the fixture-harness public-surface cross-check that currently lives at `ci.yml:118`.
- Set `--slow-timeout` (a per-test timeout with terminate-after) so a wedged `git` subprocess turns into a clear test failure rather than a 6-hour runner stall. This alone fixes the "did not complete in 10 minutes" symptom's worst failure mode.

Feature-gating (B) is rejected as highest-churn and compile-time-coupled. `#[ignore]` (C) is rejected as the primary mechanism for the same churn reason and because it leaves the single-threaded slowness unsolved. Keep `#[ignore]` in reserve for any individual test that is pathologically slow even under nextest (for example, one that builds a very large history), so it is excluded from both lanes' fast path by exception rather than as the tiering strategy.

### 3.5 Does the slow tier gate merge?

Decide from the first real measurement, which cannot be taken until CI gets past the fmt gate (no local cargo here, and the audit's numbers were single-threaded). Two shapes:

- If nextest brings the full suite to a few minutes, run the whole suite on every PR as a required check and skip a hard split entirely. This is the preferred outcome: simplest, and no test is ever quietly un-run.
- If the full suite is still too slow for the PR critical path, make the fast tier the required PR gate and run the full lane as a separate job that is required on the release PR head (the runbook's G0 needs CI green on the release commit) and otherwise runs on a schedule and on a `full-tests` label.

Either way the full suite must be green on PR #2's head before the tag is cut.

### 3.6 Target PR-gate wall time

Target the whole PR gate (fmt, clippy with `-D warnings`, the fast test tier, the bindings-drift gate, typecheck, lint, and the tauri-free assertion) at well under 10 minutes, with the fast test tier itself in the low single-digit minutes. The long pole on the PR gate is `Build and bundle` (`pnpm tauri build`, a thin-LTO release compile of the shell crate), not the tests; tests should stop being a factor. Aim to keep the full nextest run (all tiers) under about 5 minutes of test wall time on the runner once parallelized. These numbers are targets to verify against the first green run, not measured facts, and this plan does not claim otherwise.

## 4. Release pipeline plan (`release.yml`)

`release.yml` is a self-described stub: it fires on `v*` tags, builds Windows and macOS with `--profile dist` via `tauri-apps/tauri-action@v0`, and creates a draft prerelease. Its own header flags that the tauri-action inputs and the `--profile dist` threading are unvalidated. Hardening it is E-18 (auto-update and distribution) work in Phase 4, not Phase 0; this section specifies the target so the runbook's G3 and G4 have something concrete to consume.

### 4.1 Windows artifacts: ship both NSIS and MSI

`tauri.conf.json` `bundle.targets` already lists `msi`, `nsis`, `app`, `dmg`. For v0.9.0, Windows is the primary supported target and ships both Windows installers: the NSIS `-setup.exe` (the recommended default and the format the updater consumes) and the MSI (WiX, for enterprise and group-policy installs). The bundler fetches the WiX and NSIS toolchains on the runner. `tauri-action` uploads every produced bundle to the draft Release.

### 4.2 Updater manifest (`latest.json`) and signing

This is E-18's build; `release.yml` consumes it. None of it exists in the repo today - verified directly against `src-tauri/Cargo.toml` and `tauri.conf.json`: no updater plugin exists in the repo today. Requirements:

- Add `tauri-plugin-updater` and a `plugins.updater` block to `tauri.conf.json` (`endpoints`, the public key, and `bundle.createUpdaterArtifacts: true`).
- Generate a signing keypair with `tauri signer generate`. The private key and its password become the repo secrets `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`; the public key is baked into `tauri.conf.json` `plugins.updater.pubkey`. `tauri-action` reads those env vars and, with updater artifacts enabled, produces the signed update artifact (the NSIS installer plus its `.sig`) and generates `latest.json` (version, publication date, and per-platform `{signature, url}`).
- Verify the exact `tauri-action` inputs and how it emits and attaches `latest.json` against the action's current README before the first cut (the stub header already flags this). The runbook's G3 checks for `latest.json` on the draft's assets before proceeding, and G4 smoke-tests the update check.

### 4.3 Distribution profile and build

`--profile dist` selects `[profile.dist]` from the root `Cargo.toml` (full LTO, single codegen unit) for maximal-size artifacts. Confirm the flag actually reaches cargo: `tauri-action` passes `args` to `tauri build`, and `tauri build --profile dist` maps to `cargo build --profile dist` in the Tauri v2 CLI. The `Cargo.toml` comment records the dist build at roughly 35 minutes on the Windows runner; that is acceptable for a tag cut and is exactly why per-push CI uses the thin-LTO `release` profile instead.

Same frontend-dist note as CI: `release.yml` runs `pnpm install --frozen-lockfile` but relies on `tauri-action` invoking the Tauri CLI (which runs `beforeBuildCommand: pnpm build`). Verify `tauri-action` calls the CLI rather than raw cargo so the frontend is actually built before the shell crate compiles.

### 4.4 Artifact attachment on tag push

`tauri-action` creates the draft Release keyed on `tagName: github.ref_name` and attaches the per-platform bundles, their signatures, and `latest.json`. Keep `releaseDraft: true` so a human or the agent reviews before publishing (runbook G4). `prerelease: true` is defensible for a 0.9.x pre-1.0 build; keep it and keep it consistent with `CHANGELOG.md` and the runbook. `fail-fast: false` stays, so a macOS failure does not suppress the Windows signal and vice versa.

The version stamped into both platforms' bundles comes from `tauri.conf.json` (set by `scripts/bump-version.mjs` at runbook G2). One version, both platforms; the platform lives in the artifact filename, not the version.

### 4.5 macOS stays compiling, unsigned

Per the platform decision and EXECUTION.md, the macOS `.app` and `.dmg` build UNSIGNED and un-notarized: no signing secrets on the macOS leg. macOS keeps compiling and bundling to hold the dual-platform architecture, but is not a supported install target for v0.9.0 (Gatekeeper will block an unsigned `.dmg` for end users, which is acceptable because macOS is not the v0.9.0 audience). The Release body must state macOS is unsigned beta (the stub already does). Signing and notarization are human-only and arrive as a later, separate job.

## 5. The private-repo constraint

v0.9.0 ships with the full release ceremony but the repo stays private (ratified 2026-07-04; see `plan_v0.9.0.md` and the runbook). This splits the release mechanics into what works now and what waits for the separate, later, human-only public flip.

Works now, on the private repo:

- CI on every push and PR, both runners building and bundling. It is only red today because of the fmt blocker in Section 1.
- Draft GitHub Releases on a `v*` tag with the Windows MSI and NSIS plus the macOS `.app` and `.dmg` attached. Assets require authentication to download, which a collaborator or the agent has.
- `latest.json` generation, signing, and attachment. The manifest is produced and its shape and signatures are verifiable now, even though the download endpoint is not yet public.
- The installer smoke test from the downloaded artifact (runbook G4), performed by an authenticated collaborator fetching the private-repo asset URL.
- Agent-autonomous cut, merge of PR #2, tag, and private Release, under EXECUTION.md's private and pre-public merge policy (runbook G1.5 and G3).

Waits for the public flip:

- Winget submission to `microsoft/winget-pkgs`. Winget requires public, unauthenticated artifact URLs that do not exist while the repo is private. E-18 BUILDS and validates the manifest during v0.9.0; submission is a public-flip appendix item in the runbook.
- A live, unauthenticated updater endpoint. The `url` in `latest.json` points at a private-repo Release asset that requires auth to fetch, so the shipped installer's updater cannot pull an update unauthenticated until the repo is public. Auto-update is wired and signed in v0.9.0 but is not exercisable end-to-end until the flip, where the runbook appendix verifies the endpoint resolves publicly.
- Public install instructions and README download links. A stranger cannot fetch private assets, so real install docs wait for the flip.
- A public release tag becomes human-only at and after the flip (EXECUTION.md); the private-era self-merge and self-cut autonomy ends there.

## 6. Ordered Phase 0 implementation checklist

Phase 0 is Rails. The goal is a green CI on PR #2's head and a completable test suite; `release.yml` hardening is explicitly Phase 4 and E-18, not Phase 0. Model-tier assignments follow the context pack (Opus for CI redesign, Sonnet for standard workflow edits, Haiku for gate runs and mechanical steps, Fable for integration review).

1. (Sonnet or Haiku, mechanical) Run `cargo fmt --all` on the PR #2 branch and commit the formatting-only diff. This unblocks all four checks. This is a code change executed by the Phase 0 build agents, not by this doc pass.
2. (Haiku) Push and let CI re-run. READ the run to discover the true next failure past fmt: does the shell crate compile with the frontend dist absent, does the test step wall, does the bundle succeed. This "learn the true state" step cannot be skipped or predicted; the fmt mask has hidden it since the GUI landed.
3. (Sonnet) Edit `ci.yml`: add an explicit `pnpm build` after `Install frontend dependencies` and before the cargo steps (Section 2.2); drop the standalone `cargo check` and change clippy to `cargo clippy --workspace --all-targets -- -D warnings` (Section 2.3).
4. (Opus) Adopt cargo-nextest (Section 3): add the install step, replace the two `cargo test` steps with a fast-tier `nextest run` PR gate (fixtures excluded by filter) plus a full-suite lane (`--features test-support`, folding in the fixture-harness cross-check), and set `--slow-timeout`. Decide required-vs-scheduled for the full lane from the first measured run (Section 3.5). Preserve the bindings-drift gate and the tauri-free assertion (Section 2.8).
5. (Opus or Sonnet) Caching (Section 2.7): set rust-cache `save-if` to main and the active release branch; confirm the pnpm store cache; keep `CARGO_INCREMENTAL: "0"` and `cache-on-failure: false`.
6. (Sonnet, SHOULD not MUST) Add a `rust-toolchain.toml` channel pin so `-D warnings` does not float against `@stable` (Section 2.4). If deferred, file it to backlog instead of blocking Phase 0.
7. (Haiku) Run one full gate sweep on the branch head after the `ci.yml` changes and confirm both runners are green end to end. This is the first time CI reaches `Build and bundle` since the GUI, tray, and groups landed, so treat a green here as the real Phase 0 exit signal, not the fmt fix alone.
8. (Fable) Integration review of the reworked `ci.yml` and the measured timings; confirm the PR-gate target in Section 3.6 is met and the full suite is green on PR #2's head.

Backlog note (not this file's to write; flag for the roadmap-sweep agent who owns `docs/backlog.md`): the CI findings that outlive Phase 0 as tracked items are the weaker-than-local clippy scope now being fixed, the floating-`@stable` plus `-D warnings` hazard, and the frontend-dist-at-compile-time risk if it turns out to be real. `release.yml` validation (the `--profile dist` threading, the `tauri-action` updater and `latest.json` inputs) is tracked under E-18, not Phase 0.
