---
effort: E-18
title: Auto-Update and Distribution - implementation plan
status: ready
---

# E-18 - Auto-Update and Distribution - Implementation Plan

This effort is a **serial `src-tauri` chain** (it edits the shell crate, `tauri.conf.json`, capabilities, and `release.yml`) plus a **frontend slice** (Settings) and a **docs/manifest slice** (winget + runbook). Per the ship-plan serialization rule, the `src-tauri`-touching steps do not run concurrently with other shell-crate work; the winget manifest and the frontend Settings section can parallelize once the E-06 amendment (step 3) has regenerated `bindings.ts`.

## Ordered steps

1. **Generate the PRODUCTION updater keypair (HUMAN-ONLY, jp; gate for CI signing only).** jp runs `pnpm tauri signer generate -w reposync-updater.key` (password-protected) via the Tauri signer, then:
   - stores the private key content in the GitHub Actions secret `TAURI_SIGNING_PRIVATE_KEY` and its password in `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` (these secrets are the ONLY home for the production private material),
   - keeps a separate human-held backup of the private key (see the key-loss risk in the spec),
   - hands the **public key** string to the build agent to commit in `tauri.conf.json`.
   An agent never creates, prints, or holds the production private key (per `EXECUTION.md`); **production artifacts are signed exclusively in CI** from those secrets. Steps 2+ can proceed with only the public key. **Ship-dark fallback:** if the production secrets are not in place by Phase 5 (the ship phase), ship the updater DARK - fully wired but disabled in the shipped config - and move activation to the public-flip checklist. This is a named human action item for jp and must not block the release. (Local E2E signing does NOT use this key - see step 10 and the test strategy, which use a disposable test keypair an agent may generate.)

2. **Add the plugins and pins.** Add `tauri-plugin-updater` and `tauri-plugin-process` to `src-tauri/Cargo.toml`, pinned in the root `[workspace.dependencies]` table (consistent with the existing Tauri-stack pins; follow E-01's "pin Tauri-related crates" discipline). Add the JS packages `@tauri-apps/plugin-updater` and `@tauri-apps/plugin-process` to `package.json`. Register both plugins in `src-tauri/src/lib.rs` (`.plugin(tauri_plugin_updater::Builder::new().build())`, `.plugin(tauri_plugin_process::init())`). Add the permissions to `src-tauri/capabilities/default.json` (`updater:default`, and the `process` permission needed for relaunch).

3. **Amend the E-06 (IPC contract) seam (additive).** In `reposync-core::ipc`, add the Tauri-free `UpdateAvailability` payload (`serde` + `specta::Type`). In `src-tauri` commands, add `app_check_for_update() -> UpdateAvailability` and `app_install_update() -> Result<(), AppError>`, both `#[tauri::command] #[specta::specta]`, and register them in `collect_commands!`. Add `auto_update_check: bool` to the `Settings` struct. Regenerate `src/lib/bindings.ts` and confirm the stale-check CI gate is green and `reposync-core` still has no Tauri dependency (E-06 AC6/AC7).

4. **Additive migration `0006` for the toggle.** Add `crates/reposync-core/migrations/0006_auto_update.sql` adding `auto_update_check` to the `settings` singleton, defaulting `1` (on). Follow the E-02 additive-migration pattern; do not alter `0001`-`0005` (the prior migrations, including the Phase 1 BL-NI-34 (stale `check_frequency_min` default) fix and E-17 (branch and PR intelligence); see the migration-collision risk below). Wire the column through the settings read/write path (`settings_get` / `settings_set`) so `Settings.auto_update_check` round-trips.

5. **Configure the updater in `tauri.conf.json`.** Set `bundle.createUpdaterArtifacts: true`. Add `plugins.updater` with `pubkey` (from step 1), `endpoints: ["https://github.com/prisant-labs/repo-sync-tool/releases/latest/download/latest.json"]` (the FINAL public URL, inert while private), and `windows.installMode: "passive"`. Confirm the NSIS artifact is the updater target (see step 8 / the NSIS-vs-MSI decision).

6. **Wire the check + install logic (`app_check_for_update` / `app_install_update`).** Implement `app_check_for_update` over `app.updater()?.check()`, mapping to `UpdateAvailability` and distinguishing reachable-no-update from unreachable (so the UI can render "up to date" vs "couldn't reach the server"). Implement `app_install_update` as `download_and_install` (signature-verified by the plugin) then relaunch via `tauri-plugin-process`; on verification/download failure return a typed `AppError` and leave the current version intact. Add the on-launch background check in `setup`, gated by reading `auto_update_check`; surface a non-blocking "update available" prompt and never auto-install.

7. **Build the Settings "Updates" section** (`src/screens/settings.tsx`): a new card showing the current version (read-only), the `auto_update_check` toggle (default on, copy: "Check for updates on launch. You confirm before anything installs."), and a "Check for updates" button calling `app_check_for_update` with an inline outcome (up to date / vX.Y.Z available + confirm-to-install calling `app_install_update` / couldn't reach the update server). Follow the existing `Card` / `Field` / `Switch` patterns already in the file; keep all text AA-contrast (no gray-on-gray).

8. **Extend `release.yml` to sign + emit `latest.json`.** Pass `TAURI_SIGNING_PRIVATE_KEY` / `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` into the tauri-action build env; enable the updater-artifact + `latest.json` generation/attachment (verify the exact tauri-action inputs and the "prefer NSIS" option against the action's current README before the first cut - `release.yml` already carries this verify-before-relying caution). Confirm the draft Release ends up with the installers, their `.sig` files, and `latest.json` attached.

9. **Prepare the winget manifest (offline).** Create `packages/winget/` with the three-file manifest (version, installer, `en-US` default locale), `PackageIdentifier` `PrisantLabs.RepoSync`, NSIS installer type + silent switch, and a placeholder `InstallerUrl` + SHA256 (finalized at the flip). Run `winget validate --manifest packages/winget` if the tool is available locally; record the result. Do NOT submit. Write the public-flip submission checklist (finalize URL + hash against the public artifact, re-validate, `wingetcreate submit`) into this plan's "Public-flip checklist" below.

10. **E2E-verify against a local channel (the core proof).** See the test strategy below: run the whole detect -> download -> verify -> install -> relaunch loop against a `http://localhost` file server serving a crafted `latest.json` + an artifact signed with a **disposable test keypair**, driven through a **TEST-ONLY Tauri config overlay** (`src-tauri/tauri.updater-e2e.conf.json`) merged in with the Tauri CLI `--config` flag, plus the tampered-artifact negative test. This proves the mechanism now, independent of the private endpoint, without touching the production key or the production `tauri.conf.json`.

11. **Update the runbook.** Add an updater step to `runbook_cut-tag-release.md` (G3/G4): confirm `latest.json` + `.sig` files are attached to the draft Release and that the manifest version/urls are correct before publishing. (Owned by the runbook agent in this doc pass; this step records the requirement for that owner.)

12. **Verify + gate.** Scoped gates during work (`cargo -p reposync` / `cargo -p reposync-core`, scoped `tsc`); a full local gate sweep at phase end; Codex adversarial review of the updater command + install path (security-sensitive). Confirm `bindings.ts` stale-check green and the no-Tauri-in-core gate green. **Pre-tag release gate (production config hygiene):** before the v0.9.0 tag, verify by inspection/grep that the committed production `src-tauri/tauri.conf.json` contains **neither** `dangerousInsecureTransportProtocol` **nor** the disposable test public key (both belong only in the E2E overlay). The live GitHub Releases endpoint cannot be exercised while the repo is private; live-endpoint verification is a public-flip checklist item, not a pre-tag gate.

## Test strategy

- **Local-channel E2E (primary, runs while private).** Build version `A` (current) and version `B` (higher). Generate a **disposable test keypair** (an agent may do this freely - it never ships) and sign `B`'s NSIS artifact with it. Serve a crafted `latest.json` (version `B`, the artifact `url` pointing at the local server, the test-key `signature`) from `http://localhost:<port>`. Point the updater at the local server via a **TEST-ONLY config overlay** - a separate file `src-tauri/tauri.updater-e2e.conf.json`, passed with the Tauri CLI `--config` merge flag and used exclusively by the E2E script - that sets `plugins.updater.dangerousInsecureTransportProtocol: true` (Tauri v2 production builds enforce TLS on updater endpoints, so a plain `http://localhost` channel needs this test-only opt-in), points `endpoints` at the local server, and carries the disposable test keypair's **public key**. The overlay may be committed (it is inert unless explicitly passed and never merges into the production `tauri.conf.json`). Run build `A`; confirm it detects `B`, downloads, **verifies the signature** (against the test pubkey in the overlay), installs, and relaunches as `B`. This exercises every step the live GitHub endpoint will later exercise, except the live endpoint itself (blocked while private; see the pre-tag release gate and public-flip checklist).
- **Negative: tampered artifact.** Corrupt the served artifact (or serve a mismatched `signature`); confirm `app_install_update` aborts on verification failure, the current version is retained, and the UI reports the verification failure. A bad signature must never produce an installed binary.
- **Negative: offline / unreachable.** With no server (mirrors the private-repo 404), confirm the on-launch check stays silent (logs only) and the manual "Check for updates" shows "couldn't reach the update server," app unaffected.
- **Downgrade protection.** Serve a `latest.json` with a version equal-to and then lower-than the running version; confirm both yield "up to date" and no install is offered.
- **Toggle behavior.** With `auto_update_check` off, confirm no launch check fires; the manual button still works. With it on, confirm the launch check fires once and never auto-installs.
- **Unit-testable seams.** The reachable-vs-unreachable mapping and the version-comparison / gate decision are small and can be unit-tested in the shell where they do not require the plugin; the plugin call + real OS install are covered by the manual Windows E2E above (the plugin and the OS installer are the only untested-by-unit pieces, same posture as E-14 (desktop notifications)).
- **Codegen.** `bindings.ts` regenerates cleanly with the two new commands + the `UpdateAvailability` type + `Settings.auto_update_check`; the stale-check gate is green.

## Files touched

- `src-tauri/Cargo.toml` - add `tauri-plugin-updater`, `tauri-plugin-process` (pinned via workspace).
- `Cargo.toml` (root `[workspace.dependencies]`) - add the two plugin pins.
- `package.json` - add `@tauri-apps/plugin-updater`, `@tauri-apps/plugin-process`.
- `src-tauri/src/lib.rs` - register both plugins; add the on-launch gated check in `setup`.
- `src-tauri/src/commands/` - `app_check_for_update`, `app_install_update`; register in `collect_commands!`.
- `crates/reposync-core/src/ipc.rs` - the Tauri-free `UpdateAvailability` payload; `auto_update_check` on `Settings`.
- `crates/reposync-core/migrations/0006_auto_update.sql` (new) - additive `settings.auto_update_check` column (default on); plus the settings read/write path wiring.
- `src-tauri/tauri.conf.json` - `bundle.createUpdaterArtifacts`, `plugins.updater` (pubkey + final endpoint + `installMode`); stays TLS-only (no `dangerousInsecureTransportProtocol`, no test pubkey - enforced by the pre-tag release gate).
- `src-tauri/tauri.updater-e2e.conf.json` (new; TEST-ONLY) - the E2E transport overlay (`dangerousInsecureTransportProtocol` + local `http://localhost` endpoint + disposable test pubkey), merged via the Tauri CLI `--config` flag; used only by the E2E script and never merged into production `tauri.conf.json`.
- `src-tauri/capabilities/default.json` - `updater` + `process` permissions.
- `src/lib/bindings.ts` - regenerated (do not hand-edit).
- `src/screens/settings.tsx` - the "Updates" section.
- `.github/workflows/release.yml` - signing env + `latest.json` generation/attachment.
- `packages/winget/` (new) - the three-file winget manifest (offline-validated).
- `docs/internal/release-plans/runbook_cut-tag-release.md` - updater/`latest.json` verification at G3/G4 (owned by the runbook agent; recorded here as the requirement).

> NO code, config, or workflow changes are made in this doc pass. The list above is what the Phase 4 build will touch; the production private key (step 1), the CI secrets, and the winget submission are human-only. The disposable test keypair (steps 10 / test strategy) is agent-generable because it never ships.

## Public-flip checklist (deferred; not done now)

At the public flip, after the repo is public and a public Release exists:

1. **If the updater shipped DARK** (the production keypair was absent at the ship phase): confirm the production keypair now exists in the GitHub Actions secrets, enable the updater in the shipped config, and cut the activating release. This is a named human action item for jp; without it, the remaining steps cannot verify.
2. Confirm the live endpoint serves `latest.json` (`https://github.com/prisant-labs/repo-sync-tool/releases/latest/download/latest.json` returns 200 with a valid manifest).
3. Confirm an installed older-version client detects, verifies, and installs the update end to end against the **live TLS endpoint** (the same flow proven locally now via the test overlay; this live-endpoint verification is the piece that could not run while the repo was private, since a shipped client enforces TLS and the private endpoint 404s).
4. Finalize the winget `InstallerUrl` + SHA256 against the public installer asset; re-run `winget validate --manifest packages/winget`.
5. Submit the winget manifest: `wingetcreate submit packages/winget` (opens the PR to `microsoft/winget-pkgs`); track the moderation PR.
6. (Later, human-only, independent) procure the Windows Authenticode cert and add installer signing to remove SmartScreen friction.

## Risks

- **Key management** (highest stakes): the production private key's loss/leak is unrecoverable for existing installs. Human-only generation by jp + CI-only signing, CI secret + separate backup, documented rotation; local E2E uses a disposable throwaway key (never ships, agent-generable); ship dark if the production secrets are absent at the ship phase. See the spec risk section.
- **NSIS vs MSI as the update artifact:** both are built; NSIS per-user is the smooth unattended path, MSI generally needs elevation. Target NSIS for the updater manifest and confirm the generated `latest.json` points at it; reconcile with E-12's open packaging pick. Flag to jp if the bundler forces otherwise.
- **tauri-action manifest-generation drift:** verify the action's updater inputs against its current README before the first cut (same caution `release.yml` already carries).
- **Migration collision (caught, resolved):** an earlier draft reserved `0004` for this toggle. The 2026-07-04 Codex adversarial review flagged that `0004` collides with the Phase 1 BL-NI-34 (stale `check_frequency_min` default) fix and `0005` with E-17 (branch and PR intelligence). Resolved by the fixed sequence: `0004` = the BL-NI-34 fix, `0005` = E-17 (branch and PR intelligence), `0006` = E-18 (this effort). E-18's migration is `0006_auto_update.sql`, additive-only and single-owner; coordinate with sibling efforts adding schema in the same phase.
- **Private-endpoint 404 misread as a bug:** honest failure copy + documented behavior + the independent local E2E proof.

## Definition of done

- All nine ACs met. `tauri-plugin-updater` + `tauri-plugin-process` integrated; on-launch (toggle-gated) + manual checks route through `app_check_for_update`; user-confirmed installs via `app_install_update` with mandatory signature verification.
- The production signing keypair generated once by jp (human-only), private key + password in CI secrets only + a human-held backup, public key committed, production artifacts signed exclusively in CI; rotation/loss semantics documented; if the production secrets are absent by the ship phase, the updater ships dark with activation on the public-flip checklist. Local E2E signing uses a disposable test keypair (agent-generable, never ships).
- `release.yml` signs the updater artifacts and attaches a valid `latest.json`; `createUpdaterArtifacts` on.
- The final public TLS endpoint is configured (inert while private); the full flow is E2E-verified against a local `http://localhost` channel via the test-only transport overlay and a disposable test keypair, including the tampered-artifact rejection, offline handling, and downgrade protection; before the tag, the production `tauri.conf.json` is confirmed free of `dangerousInsecureTransportProtocol` and the test pubkey; live-endpoint verification is deferred to the public flip.
- The Settings "Updates" section (current version + default-on checks-only toggle + "Check for updates") is present, AA-contrast, and matches the no-telemetry posture.
- The winget manifest is prepared under `packages/winget/` and passes `winget validate` OFFLINE; the public-flip submission checklist is recorded; submission is deferred.
- The E-06 amendment is additive and green (`bindings.ts` stale-check + no-Tauri-in-core gates); local gate green; the updater command + install path passed a Codex adversarial review.
