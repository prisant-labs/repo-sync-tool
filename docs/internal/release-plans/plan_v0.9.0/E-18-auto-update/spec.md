---
effort: E-18
title: Auto-Update and Distribution
tracking-issue: "#20"
status: ready
tier: SHOULD
scope: V1 (integration; distribution / native chrome)
depends_on: [E-12, E-06, E-02, E-01]
source: docs/internal/program-roadmap.md (scope ledger, "auto-updater" CUT to V1.1); jp ratification 2026-07-04 (promoted to v0.9.0, private-ship framing) in _LOCAL/plans/2026-07-04_18-25_ship-plan-context-pack.md; docs/internal/release-plans/plan_v0.9.0/E-12-tracer-bullet/spec.md (packaging spike, installer targets, macOS/Windows signing posture); docs/backlog.md BL-V11-07 (auto-updater) + BL-DEC-01 (signing posture)
---

# E-18 - Auto-Update and Distribution

## Task Summary

> Agents keep this block current as work proceeds.

- **State:** not started (spec drafted 2026-07-04). This is a NEW SHOULD effort, jp-ratified on 2026-07-04, promoted from the roadmap's CUT-to-V1.1 "auto-updater" line (parked as BL-V11-07 (auto-updater: in-app self-update), which depended on the then-unset signing posture BL-DEC-01). No updater plugin, updater config, signing keypair, `latest.json` pipeline, or winget manifest exists in the repo today: `tauri.conf.json` has no `plugins.updater` block and no `createUpdaterArtifacts`, `.github/workflows/release.yml` is a stub that does not emit an update manifest, and there is no `packages/winget/` folder.
- **Next:** the build phase (Phase 4 of the ship plan) (1) has jp generate the production updater signing keypair once (human-only; artifacts signed exclusively in CI, never held by an agent), (2) adds `tauri-plugin-updater` + `tauri-plugin-process`, sets `bundle.createUpdaterArtifacts` and `plugins.updater` (pubkey + endpoints + Windows `installMode`) in `tauri.conf.json`, (3) wires the on-launch check + the Settings "Check for updates" action + the `auto_update_check` toggle, (4) teaches `release.yml` to sign the updater artifacts and emit/attach `latest.json`, and (5) prepares the winget multi-file manifest under `packages/winget/`, validated OFFLINE only. Everything except the live GitHub endpoint and the actual winget submission is E2E-verified now (local file-server or token-authenticated draft-release channel).
- **Blockers:** the LIVE update channel is blocked by the private-repo constraint - unauthenticated GitHub Release asset URLs return 404 while the repo is private, so a shipped client cannot fetch `latest.json` from the real endpoint until the public flip. The full mechanism (plugin, signing, manifest generation, signature verification, settings UI, failure handling) is buildable and testable now against a local channel; only the endpoint going live and the winget PR wait on the flip.

## Context

RepoSync is a resident tray utility: users install it once and rarely revisit the download page. Without an update path, every installed copy silently drifts to a stale, potentially insecure build, which is the worst failure mode for a tool that runs unattended for weeks. This effort adds signed, user-confirmed auto-update delivered over GitHub Releases, plus the winget packaging prep, so the private v0.9.0 build is update-ready the moment the repo goes public.

The auto-updater was CUT to V1.1 in the original scope ledger (`docs/internal/program-roadmap.md`) and parked as BL-V11-07 (auto-updater: in-app self-update), blocked on a signing posture that was not yet set (BL-DEC-01). jp promoted it back into v0.9.0 on 2026-07-04. The promotion trigger is the ship decision itself: v0.9.0 ships COMPLETE but PRIVATE, with the public flip as a separate later milestone. Shipping a resident app with no update mechanism and then flipping public later would strand every early install on the first build with no clean upgrade. The resolution is to build the whole mechanism now, verify it end to end against a local channel, and leave only two things gated behind the public flip: the live GitHub endpoint lighting up, and the winget submission PR.

Architecturally this is a **native edge**, not a webview screen, so it fits the same "platform-specific code is a thin edge" rule as the tray (E-13 (tray native menu)), notifications (E-14 (desktop notifications)), and autostart (E-15 (autostart)). The plugin (`tauri-plugin-updater`) abstracts the OS-specific install; the decision to check is trivial; the real integrity boundary is the **minisign signature** on each artifact, verified against a public key baked into the binary. The posture is consistent with the no-telemetry OSS framing (RepoSync is an OSS community contribution, not a commercial product): the updater **checks only**, the **user confirms** every install, the default is **on** but nothing installs silently, and the only network call is an unauthenticated GET of a version manifest - no analytics, no phone-home, no account.

## In scope

### (a) Plugin integration and signing keypair lifecycle

- Add `tauri-plugin-updater` (Rust + `@tauri-apps/plugin-updater` JS) and `tauri-plugin-process` (Rust + `@tauri-apps/plugin-process` JS, for the post-install relaunch), registered in `src-tauri/src/lib.rs` and permitted in `src-tauri/capabilities/default.json`.
- An **on-launch update check**: on startup, if the `auto_update_check` setting is on, check the endpoint once in the background. If an update is available, surface a non-blocking prompt; the user confirms before any download or install. Never auto-install.
- A **manual "Check for updates"** action in Settings that runs the same check on demand and reports the result (up to date / update available / could not reach the update server).
- **Signature verification is mandatory:** every downloaded artifact is verified against the embedded public key before it is applied; a verification failure aborts the install and leaves the running version untouched (see failure behavior below).
- **The signing keypair lifecycle - two cleanly separated keypairs, documented and executed once:**
  - **Production keypair (human-only, generated once by jp).** jp generates a single updater keypair with `pnpm tauri signer generate` (produces a password-protected private key + a public key) via the Tauri signer. This is **human-only secret handling** per `EXECUTION.md`; an agent never creates, holds, or prints the production private key. The **private key content and its password** are stored ONLY as the GitHub Actions secrets `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`; **production updater artifacts are signed exclusively in CI** from those secrets. The **public key** goes into `tauri.conf.json` `plugins.updater.pubkey` (committed to the repo; it is not a secret).
  - **Disposable test keypair (agent-generable, never ships).** The local end-to-end proof (see (c)) signs its version-B artifact with a throwaway keypair that an agent may freely generate, because it never ships: its public key lives ONLY in the TEST-ONLY updater config overlay, never in the production `tauri.conf.json`. This keeps the "agents never touch the production private key" boundary intact while still letting an agent run the full detect -> verify -> install loop locally. The disposable key carries none of the production key's stakes below.
  - **Rotation is a one-way property and must be documented:** the production public key is compiled into every installed binary, so already-installed clients trust ONLY the key that shipped in their build. Rotating the key requires first shipping a release still signed with the OLD key that carries the NEW pubkey in its config, then signing subsequent releases with the new key; clients that skip that bridging release cannot verify later updates and must reinstall manually. Losing the production private key has the same effect: no future update will verify for existing installs. The production private key is therefore backed up as a human-held secret (per `EXECUTION.md`), separate from CI.
  - **Ship-dark fallback (named human action item for jp).** If the production keypair does not yet exist in the GitHub Actions secrets by Phase 5 (the ship phase), the updater ships **DARK** - fully wired but disabled in the shipped config - and activation moves to the public-flip checklist. Shipping dark is preferable to blocking the release on the human-only key step.

### (b) Update channel: GitHub Releases `latest.json`

- The update channel is a static `latest.json` manifest attached to each GitHub Release, in the Tauri v2 updater format: top-level `version`, `notes`, `pub_date`, and a `platforms` map keyed by `<os>-<arch>` (`windows-x86_64`, `darwin-x86_64`, and `darwin-aarch64` when a Mac build exists), each carrying the artifact `url` and its minisign `signature`.
- The manifest is **generated by the release pipeline**, not hand-written: with `bundle.createUpdaterArtifacts` enabled and the signing key env vars present, the Tauri bundler produces the update artifacts and their `.sig` files, and the release workflow emits `latest.json` and attaches it to the Release alongside the installers. (The exact tauri-action inputs that generate/attach the manifest are verified against the action's current README before the first cut, consistent with the caution already written into `release.yml`.)
- The configured endpoint is the **final** GitHub URL from day one: `https://github.com/product-on-purpose/repo-sync-tool/releases/latest/download/latest.json`. It is inert (404) while the repo is private and lights up automatically at the public flip with no config change (see (c)).

### (c) The private-repo constraint, stated honestly

While the repo is private, GitHub Release **assets are not downloadable without authentication** - an unauthenticated GET of `releases/latest/download/latest.json` (and of the installer assets it points to) returns 404. A shipped client sends no credentials (embedding a PAT in an OSS binary is rejected: it is a leaked secret and defeats the point), so the live endpoint cannot serve updates until the repo is public. This effort splits cleanly along that line:

- **Ships NOW (fully built and verified while private):**
  - The complete plugin wiring, the `plugins.updater` config with the **final** endpoint and the real pubkey, `createUpdaterArtifacts` on, the Settings surface, and all failure handling.
  - The release pipeline that signs the updater artifacts and generates + attaches `latest.json`.
  - **End-to-end verification against a local channel:** a documented manual test that points the updater at a `http://localhost` file server via a **TEST-ONLY config overlay** (a separate `src-tauri/tauri.updater-e2e.conf.json`, merged in with the Tauri CLI `--config` flag and used only by the E2E script) that sets `plugins.updater.dangerousInsecureTransportProtocol` (Tauri v2 production builds enforce TLS on updater endpoints, so a plain `http://localhost` channel needs this test-only opt-in), points the endpoint at the local server, and carries the **disposable test keypair's** public key. The overlay serves a crafted `latest.json` plus an artifact signed with that disposable test key, run from an older-versioned build, proving detect -> download -> signature-verify -> install -> relaunch-at-new-version, plus the negative test that a tampered artifact is rejected. The overlay may be committed (it is inert unless explicitly passed and never merges into the production `tauri.conf.json`, which stays TLS-only). The **live GitHub Releases endpoint cannot be exercised while the repo is private** - live-endpoint verification is a public-flip checklist item.
  - Because the shipped endpoint is already the final public URL, the private build's on-launch and manual checks simply report "could not reach the update server" (handled gracefully, no error spam); nothing is mis-wired.

- **Activates at the PUBLIC FLIP (no code change, only visibility + a release):**
  - The live endpoint starts serving `latest.json` the moment the repo is public and a public Release exists; installed clients begin finding updates on their next check.
  - The winget submission PR (see (d)) is opened, since it needs a public `InstallerUrl`.

### (d) winget manifest preparation (offline-only)

- Prepare the winget multi-file manifest in-repo under `packages/winget/`: the version manifest, the installer manifest, and the default-locale (`en-US`) manifest, using the current winget manifest schema, with `PackageIdentifier` `ProductOnPurpose.RepoSync`, the NSIS installer type and its silent switch, and the SHA256 + `InstallerUrl` of the release artifact (a placeholder URL while private, finalized at the flip).
- **Validate OFFLINE only:** run `winget validate --manifest packages/winget` (and `wingetcreate` schema checks) if the tool is available locally. No network submission. `wingetcreate submit` / the PR to `microsoft/winget-pkgs` is explicitly deferred.
- A **submission checklist** is written into this effort's implementation plan for the public flip (finalize `InstallerUrl` + hash against the public artifact, re-validate, `wingetcreate submit`), but the submission itself is out of scope now.

### (e) Settings surface

- An **"Updates"** section in the Settings screen (`src/screens/settings.tsx`) with:
  - The **current app version** (read-only), so the user can see what they are running.
  - An **auto-update toggle** (`auto_update_check`), default **on**, labeled to make the behavior explicit and consistent with the show-option-meaning-up-front preference: it controls whether RepoSync **checks** for updates on launch; it never installs without the user confirming. Copy makes clear this is a check, not a silent update.
  - A **"Check for updates"** button that runs the manual check and shows the outcome inline (up to date / vX.Y.Z available with a confirm-to-install affordance / could not reach the update server).
- The toggle is persisted in the `settings` singleton so the Rust launch-check path can read it headlessly (it is not a frontend-only preference).
- No new telemetry, no account, no "share usage data" surface - the Updates section is checks-and-install only, matching the OSS no-telemetry posture.

### (f) Failure behavior

- **Offline / endpoint unreachable (includes the private-repo 404):** the check resolves as "no update / could not check." The on-launch check logs and stays silent (no error toast on every cold start); the manual check shows a gentle "couldn't reach the update server" line. The app continues normally on its current version.
- **Signature mismatch / tampered or corrupt artifact:** `download_and_install` fails verification **before** replacing the running binary; the install aborts, the current version is retained, and the UI reports "update could not be verified, staying on your current version" (logged with detail). A bad signature never results in an installed unverified binary.
- **Downgrade protection:** the updater only offers an update when the manifest `version` is semver-greater than the running version; a manifest advertising an equal or older version yields "up to date." The signature check is the integrity boundary; the version comparison prevents a stale or rolled-back manifest from pushing an older build.
- **Interrupted download / install:** a failed or interrupted download leaves the current install intact (the new artifact is only applied after a complete, verified download); the next check retries. No partial-state install.

## E-06 (IPC contract) amendment

E-18 amends the frozen E-06 (IPC contract) surface additively (a new entry in `collect_commands!` and one optional-additive field on `Settings`; both are non-breaking revisions caught by the stale-`bindings.ts` CI gate, per E-06 AC6). The new symbols live in `src-tauri` (commands) and `reposync-core::ipc` (payload types), regenerated into `src/lib/bindings.ts`.

**New commands (app-level, thin wrappers over the plugin so there is one typed path shared by the launch check and the Settings button, and so the `auto_update_check` toggle is enforced in one place):**

- `app_check_for_update() -> UpdateAvailability` - runs the plugin check and returns a typed result: `{ current_version: String, available: bool, new_version: Option<String>, notes: Option<String>, error: Option<AppError> }` (a reachable-but-no-update result and an unreachable result are distinguished so the UI can render "up to date" vs "couldn't reach the server" correctly).
- `app_install_update() -> Result<(), AppError>` - downloads, verifies, and installs the pending update, then relaunches (via `tauri-plugin-process`). Called only after the user confirms. Returns a typed error on verification/download failure, leaving the current version intact.

**New payload type (Tauri-free, in `reposync-core::ipc`):** `UpdateAvailability` deriving `serde` + `specta::Type`, matching the shape above.

**Settings field addition:** `Settings` gains `auto_update_check: bool` (default `true`). It mirrors a new `auto_update_check` column on the `settings` singleton (additive migration `0006`, following the E-02 pattern of storing every toggle in the settings row). This is a provisional-additive field per E-06's rules.

**No new events.** Download progress is handled by the plugin's `download_and_install` progress callback, not a frozen `tauri_specta::Event`; the E-06 event set is unchanged. (Recorded so a reviewer does not expect an `update:*` event in `events.rs`.)

**Alternative considered:** call the plugin's own JS commands (`@tauri-apps/plugin-updater` `check()` / `downloadAndInstall()`) directly from the frontend and add no `app_*` commands. Rejected as the default because the launch-check path (Rust) and the Settings button (JS) would then diverge, and the `auto_update_check` gate would live only on the JS side; the thin typed wrapper keeps a single enforced path. See Open questions if this is revisited.

## Contract / deliverables

1. `tauri-plugin-updater` + `tauri-plugin-process` added, registered in `lib.rs`, and permitted in `capabilities/default.json`; `bundle.createUpdaterArtifacts` and `plugins.updater` (pubkey + final GitHub endpoint + Windows `installMode: passive`) set in `tauri.conf.json`.
2. The production signing keypair generated once by jp (human-only), its private key + password held ONLY as GitHub Actions secrets, public key committed in `tauri.conf.json`, and production artifacts signed exclusively in CI; the rotation and loss semantics documented; a ship-dark fallback if the secrets are not in place by the ship phase. The disposable test keypair used for local E2E is agent-generable and never ships.
3. `release.yml` extended to sign the updater artifacts and generate + attach `latest.json` (Tauri v2 format) to the GitHub Release, alongside the existing installer artifacts.
4. On-launch check (gated by `auto_update_check`) + manual "Check for updates" in Settings, both routed through `app_check_for_update`; user-confirmed install via `app_install_update`; mandatory signature verification.
5. The Settings "Updates" section: current version, the default-on `auto_update_check` toggle (checks only), and the "Check for updates" button with inline outcome.
6. The private-repo split honored: final TLS endpoint configured now (inert while private), E2E-verified against a local `http://localhost` channel using the test-only transport overlay and a disposable test keypair; winget submission + live-endpoint verification deferred to the public flip.
7. winget multi-file manifest prepared under `packages/winget/` and validated OFFLINE; a public-flip submission checklist recorded.
8. Failure behavior implemented: graceful offline, signature-mismatch abort with current version retained, downgrade protection, no partial installs.

## Acceptance criteria

- [ ] AC1: `tauri-plugin-updater` is integrated with a check on launch (gated by the `auto_update_check` setting) and a manual "Check for updates" action in Settings; every install is user-confirmed and no update installs silently. Source: in-scope (a)/(e); Tauri v2 updater plugin (`app.updater()?.check()` / `download_and_install`); no-telemetry OSS posture (project framing: OSS, not commercial).
- [ ] AC2: Downloaded update artifacts are verified against the embedded public key before being applied; a signature mismatch aborts the install and leaves the running version unchanged. Source: in-scope (a)/(f); Tauri updater signature model (`plugins.updater.pubkey` + minisign `.sig`).
- [ ] AC3: The production signing keypair is generated once by jp with `tauri signer generate` (human-only); its private key + password live only in GitHub Actions secrets (`TAURI_SIGNING_PRIVATE_KEY` [+ password]), the public key is committed in `tauri.conf.json`, and key rotation + key-loss semantics are documented. Production artifacts are signed exclusively in CI, and no agent ever signs with the production key: production-signing verification is satisfied by a CI run's signed artifact passing signature verification against the committed public key. Local E2E signing instead uses a disposable test keypair (AC5). If the production secrets are absent by the ship phase, the updater ships DARK (disabled in the shipped config) with activation moved to the public-flip checklist - a named human action item for jp. Human-only secret handling per `EXECUTION.md`. Source: in-scope (a); Tauri signing env vars; `EXECUTION.md` human-only allowlist.
- [ ] AC4: The release pipeline generates a Tauri v2 `latest.json` (with per-platform `url` + `signature`) and attaches it to the GitHub Release; `bundle.createUpdaterArtifacts` is enabled. Source: in-scope (b); Tauri updater static-JSON format + `createUpdaterArtifacts`; `release.yml`.
- [ ] AC5: The private-repo constraint is honored: the final public GitHub endpoint is configured now and is inert (404) while private; the full path (manifest generation, signature verification, install, relaunch, and the tampered-artifact rejection) is verified end to end against a local `http://localhost` channel using a test-only config overlay (`dangerousInsecureTransportProtocol` + the disposable test pubkey, merged via the Tauri CLI `--config` flag, never merged into production `tauri.conf.json`) and a disposable-test-keypair-signed artifact; the live GitHub Releases endpoint cannot be exercised while the repo is private, so live-endpoint verification and the winget submission wait on the public flip. Source: in-scope (c); ratified private-ship framing (context pack, 2026-07-04).
- [ ] AC6: A winget multi-file manifest (`PackageIdentifier` `ProductOnPurpose.RepoSync`) is prepared under `packages/winget/` and passes `winget validate` OFFLINE; the actual submission to `microsoft/winget-pkgs` is deferred to the public flip with a recorded checklist. Source: in-scope (d); ratified decision (winget submission deferred to the public flip).
- [ ] AC7: The Settings screen shows the current version, a default-on `auto_update_check` toggle whose copy makes clear it checks (never silently installs), and a "Check for updates" button with an inline outcome; no telemetry/account surface is added. Source: in-scope (e); show-option-meaning-up-front + no-telemetry posture.
- [ ] AC8: Failure behavior is implemented and observable - graceful offline / unreachable (including the private 404), signature-mismatch abort retaining the current version, semver downgrade protection, and no partial-state install on an interrupted download. Source: in-scope (f); Tauri updater semver + signature semantics.
- [ ] AC9: The E-06 (IPC contract) amendment lands additively - `app_check_for_update` / `app_install_update` commands, the `UpdateAvailability` payload (Tauri-free in `reposync-core::ipc`), and `Settings.auto_update_check` - regenerated into `bindings.ts` with the stale-check CI gate green and no `reposync-core` Tauri dependency. Source: E-06 spec (additive-revision rule, AC6, AC7); this spec's E-06 amendment section.

## Dependencies

- Upstream: E-12 (tracer bullet + packaging spike) - owns the installer targets (MSI/NSIS), the `dist` profile, and the Windows/macOS signing posture this builds on; E-06 (IPC contract) - the frozen seam this amends additively; E-02 (persistence and paths) - the `settings` singleton the `auto_update_check` toggle persists into (additive migration `0006`); E-01 (foundation, workspace, CI) - `release.yml`, `tauri.conf.json`, capabilities, and the workspace dependency-pin table.
- Downstream: none hard. The GUI Settings screen renders the Updates section; the release runbook (`runbook_cut-tag-release.md`) gains an updater-artifact + `latest.json` verification step at G3/G4.
- External / human-only (per `EXECUTION.md`): generating the production keypair and installing the CI secrets (jp), plus the ship-dark activation at the public flip if those secrets are absent at ship; the winget submission PR at the public flip; Windows Authenticode signing of the installer (separate from the updater's minisign; deferred, see out of scope).

## Out of scope

- **macOS update delivery beyond building the artifacts.** The `darwin-*` entries are emitted into `latest.json` so the format is complete, but macOS updater delivery is not exercised until Mac access exists; macOS code signing / notarization is human-only and deferred (E-12 V1.1 extension point). Windows is the primary supported updater target.
- **Windows Authenticode signing of the installer itself** (Azure Trusted Signing / a code-signing cert). That is a separate signature from the updater's minisign and is human-only cert procurement, already an E-12 V1.1 extension point. The updater's minisign signature (the integrity boundary for auto-update) IS done here.
- **The winget submission PR** to `microsoft/winget-pkgs` (needs a public `InstallerUrl`; public-flip only) and any store submission (Microsoft Store, etc).
- **Silent / forced / staged updates, and multiple update channels** (stable vs beta). V1 is a single channel, check-only, user-confirmed. Channels and rollout control are V1.1.
- **Rich in-app release-notes rendering** beyond displaying the manifest `notes` string.
- **A self-hosted or CDN update server.** The channel is GitHub Releases; the `{{target}}`/`{{arch}}`/`{{current_version}}` templated dynamic-server option is a V1.1 extension point, not V1.

## Risks

- **Key management is the highest-stakes risk.** The private key signs every future update; losing it or leaking it is unrecoverable for existing installs (leak -> attacker can sign a malicious update that verifies; loss -> no future update verifies and users must reinstall). Only the **production** key carries this risk: it is generated once by jp, signs exclusively in CI, is held only as GitHub Actions secrets + a separate human-held backup, and no agent ever holds it. Local E2E signs with a disposable throwaway keypair that never ships (its pubkey lives only in the test overlay), so agent-run testing never touches the production key. Mitigation: generate once, human-only handling, private key in CI secrets + a separate human-held backup, pubkey rotation semantics documented, and never printed or committed; if the production secrets are not in place by the ship phase, the updater ships dark rather than blocking the release. This is why AC3 is explicit and human-gated.
- **NSIS vs MSI updater support.** The project's `tauri.conf.json` bundles both `msi` and `nsis` on Windows, and with `createUpdaterArtifacts` the bundler produces updater artifacts + signatures for BOTH. In practice, **NSIS per-user (`installMode: passive`, no elevation) is the smooth auto-update path** and matches E-12's user-mode / `downloadBootstrapper` install decision; MSI self-update tends to require elevation / a UAC prompt and is awkward for a resident per-user app. **Decision (see below): the auto-update channel targets the NSIS installer**; MSI remains available as a manual download but is not the updater artifact. This must be confirmed against the current tauri-plugin-updater behavior when both targets are present (the updater manifest must point at the NSIS artifact), and it should align with E-12's still-open MSI-vs-NSIS packaging pick. Verified against Tauri v2 docs: on Windows the bundler emits both MSI and NSIS with signatures; the risk is which one the manifest references and whether MSI is even a viable unattended-update target - it generally is not for per-user installs.
- **tauri-action manifest generation drift.** The exact tauri-action inputs that generate and attach `latest.json` (and prefer the NSIS artifact) must be verified against the action's current README before the first cut - `release.yml` already carries this caution for the `--profile dist` passthrough; the same discipline applies to the updater outputs.
- **Endpoint inert-while-private is easy to misread as broken.** Because the shipped endpoint 404s while private, a dogfooder will see "couldn't reach the update server." Mitigation: the failure copy is honest and non-alarming, the behavior is documented here and in the runbook, and the local-channel E2E test proves the mechanism independently of the private endpoint.
- **RC-stage codegen coupling.** The updater commands regenerate `bindings.ts` through the pinned `tauri-specta` RC (E-06); a plugin/tauri-specta version skew could break codegen. Mitigation: follow E-06's exact-pin + stale-check discipline; add the plugin pins to the workspace dependency table.
- **`auto_update_check` migration ordering.** Adding the `settings` column is an additive `0006` migration; it must default `true` and not collide with any other in-flight migration from a sibling effort. The 2026-07-04 Codex adversarial review caught an earlier `0004` collision and it is resolved by the fixed sequence: `0004` = the Phase 1 BL-NI-34 (stale `check_frequency_min` default) fix, `0005` = E-17 (branch and PR intelligence), `0006` = E-18 (this effort). Mitigation: single owner for the migration number; additive-only, following E-02's pattern.

## V1.1 extension points

- **Update channels** (stable / beta / nightly) via the `{{target}}`-templated dynamic endpoint, letting dogfooders opt into pre-releases.
- **Delta / background updates** and a configurable "install on next launch" mode, once the check-only V1 posture has been validated in the wild.
- **The winget submission** becomes live at the public flip (it is prepared, not submitted, here); Microsoft Store submission (NSIS `/S` silent install is already store-compatible) is a later distribution channel.
- **Windows Authenticode signing** of the installer (Azure Trusted Signing) layers on once the cert is procured (human-only), removing the SmartScreen friction; independent of the updater's minisign.
- **Self-hosted update server** if GitHub Releases ever becomes insufficient (rate limits, private-channel needs).

## Open questions

- **Thin `app_*` wrapper vs the plugin's own JS commands (recommended: the wrapper).** Recommendation: keep `app_check_for_update` / `app_install_update` so the launch check and the Settings button share one typed path and the `auto_update_check` gate lives in one place. Revisit only if the wrapper proves to add no value over calling `@tauri-apps/plugin-updater` directly.
- **NSIS as the sole auto-update artifact (recommended: yes).** Recommendation: target NSIS per-user for auto-update and keep MSI as a manual/enterprise download only. Confirm during wiring that the generated `latest.json` references the NSIS artifact when both targets are built, and reconcile with E-12's open MSI-vs-NSIS packaging decision. Flag to jp if the bundler forces a different choice.
- **On-launch check timing.** Whether the launch check fires immediately or after a short delay / on an interval for a long-resident process. Default: once shortly after startup (gated by the toggle); a periodic re-check is a small V1.1 add.
- **Local-channel E2E test form.** Whether the committed test guidance uses a `localhost` file server or a token-authenticated draft-release fetch. Default: document the `localhost` file-server path as the primary (no token needed, fully offline) and the token draft-release as the alternative; neither ships as committed config.
