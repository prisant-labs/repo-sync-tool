# Updater end-to-end proof (local channel) - E-18

> **STATUS, 2026-08-07: this procedure has NEVER BEEN RUN.** It is tracked as
> **BL-NI-42 (run the updater E2E install proof)** in [`docs/backlog.md`](../docs/backlog.md).
> Nothing below is a record of a passing run; every "proves" in this file is what the
> procedure WOULD prove once executed. Do not cite it as evidence that the updater
> works.
>
> **The private-repo framing this file was written under is also gone.** The
> repository has been PUBLIC since 2026-07-17, so the live endpoint is no longer
> blocked by visibility. What blocks it now is different and larger, and is set out
> under "The real remaining sequence" below.

The auto-updater (E-18) is fully wired. This runbook drives the whole mechanism
against a local `http://localhost` channel, using a **disposable test keypair** an
agent may freely generate (it never ships) and the **test-only config overlay**
`src-tauri/tauri.updater-e2e.conf.json`. It exercises every step the live endpoint
will later exercise except the live endpoint itself.

This is a dogfood / manual procedure: it builds two full `dist` bundles and drives a
real OS install, so it is NOT run in the fast gate. The pure decision logic (version
gating, the reachable-vs-unreachable mapping, the ship-dark decision, and the
config-hygiene grep) is unit-tested in `src-tauri/src/updates.rs`. That unit coverage
is what exists today; this runbook is what does not.

## What this procedure would prove locally

- detect -> download -> **signature-verify** -> install -> relaunch-at-new-version
- tampered artifact is rejected (verification failure, current version retained)
- offline / unreachable is handled gracefully (silent launch check, gentle manual copy)
- downgrade protection (an equal/older manifest version yields "up to date")
- the `auto_update_check` toggle gates the launch check but not the manual button

## The real remaining sequence (supersedes "waits for the public flip")

Going public removed one blocker and revealed that it was never the binding one.
The steps below are ORDERED; each depends on the one before it.

1. **Generate and custody the production keypair.** Human-only. The private key goes
   to CI secrets (`TAURI_SIGNING_PRIVATE_KEY`), and whoever holds it can authorize an
   update every installed copy will trust.
2. **Commit the real public key** to `plugins.updater.pubkey` in
   `src-tauri/tauri.conf.json`, replacing the ship-dark placeholder. This is what
   `updates::updater_is_live` inspects, and it is compiled INTO each build.
3. **Verify the release workflow's updater arguments** before relying on a real cut.
   `.github/workflows/release.yml` still carries an explicit note that how its args
   thread through (Tauri CLI vs cargo passthrough) needs confirming. Related and
   larger: **the Release workflow has never completed successfully.** Its only run,
   for `v0.9.0` on 2026-07-05, failed in under four seconds with zero steps executed,
   and the Windows installers on that release were produced outside the pipeline.
4. **Cut a release that actually produces signed artifacts plus `latest.json`,** and
   confirm both are attached.
5. **Manually install that build as a bootstrap.** This step is easy to miss and is
   the reason the key alone changes nothing for existing users: every already-installed
   copy has the PLACEHOLDER key compiled in and concludes it has no update channel
   before touching the network. Publishing a signed release does not reach it. Only a
   manual install of a build carrying the real public key puts a machine on the update
   path.
6. **Run this local runbook** (the negatives are the valuable part), then **repeat the
   detect-and-install proof against the live endpoint** from the bootstrap build.
7. **The winget submission** (`wingetcreate submit`), which needs public artifact URLs
   and is independent of the updater.

## Prerequisites

- The Tauri CLI (`pnpm tauri`), Node, and a static file server (e.g. `npx serve` or
  `python -m http.server`).
- Disk + time for two `dist` (full-LTO) builds.

## Steps

1. **Generate a DISPOSABLE test keypair** (agent-generable - it never ships):

   ```sh
   pnpm tauri signer generate -w "$TMP/reposync-e2e.key"
   # prints the PUBLIC key; note it. The private key is at $TMP/reposync-e2e.key.
   ```

2. **Point the E2E overlay at the disposable public key.** Copy the overlay and
   replace the `DISPOSABLE_TEST_UPDATER_PUBKEY_E2E_ONLY` sentinel with the public key
   from step 1 (do this in a scratch copy so the committed overlay keeps the sentinel;
   the committed sentinel is what the config-hygiene gate keys off):

   ```sh
   sed "s#DISPOSABLE_TEST_UPDATER_PUBKEY_E2E_ONLY#<PUBKEY>#" \
     src-tauri/tauri.updater-e2e.conf.json > "$TMP/updater-e2e.local.json"
   ```

3. **Build version B (the "newer" build), signed with the disposable key.** Bump
   `src-tauri/tauri.conf.json` `version` to a higher value (e.g. `0.9.1`), then:

   ```sh
   TAURI_SIGNING_PRIVATE_KEY="$(cat "$TMP/reposync-e2e.key")" \
   TAURI_SIGNING_PRIVATE_KEY_PASSWORD="" \
     pnpm tauri build --config "$TMP/updater-e2e.local.json"
   ```

   This produces the NSIS `-setup.exe`, its `.sig`, and (via `createUpdaterArtifacts`)
   the updater artifact. Copy the installer + read its `.sig` contents.

4. **Craft `latest.json`** for version B and serve it from `http://localhost:8787`
   alongside the installer:

   ```json
   {
     "version": "0.9.1",
     "notes": "E2E test build",
     "pub_date": "2026-07-05T00:00:00Z",
     "platforms": {
       "windows-x86_64": {
         "signature": "<contents of the .sig file>",
         "url": "http://localhost:8787/RepoSync_0.9.1_x64-setup.exe"
       }
     }
   }
   ```

   ```sh
   cd "$TMP/serve" && npx serve -l 8787    # or: python -m http.server 8787
   ```

5. **Build + run version A (the current build)** with the same overlay so it points at
   the local endpoint and trusts the disposable key. Reset the version to the lower
   value first, then `pnpm tauri build --config "$TMP/updater-e2e.local.json"` and run
   the installed app. In Settings > Updates, click **Check for updates**: it should
   detect 0.9.1, and **Install and restart** should download, verify the signature,
   install, and relaunch as 0.9.1.

## Negative / edge checks

- **Tampered artifact:** corrupt the served `-setup.exe` (or serve a mismatched
  `signature`). Install must ABORT on verification failure; the app stays on the
  current version and reports the failure. A bad signature must never install.
- **Offline / unreachable:** stop the file server. The on-launch check stays silent
  (logs only); the manual "Check for updates" shows "could not reach the update
  server." The app is unaffected. (This mirrors the shipped private-repo 404.)
- **Downgrade protection:** serve a `latest.json` whose `version` equals, then is lower
  than, the running version. Both yield "up to date"; no install is offered.
- **Toggle:** with `auto_update_check` OFF, confirm no launch check fires; the manual
  button still works. With it ON, the launch check fires once and never auto-installs.

## Pre-tag config-hygiene gate (production stays clean)

Before cutting the v0.9.0 tag, confirm the committed production config contains none of
the test-only markers (this is enforced two ways - run either):

```sh
node scripts/check-updater-config-hygiene.mjs
# or the in-suite Rust test:
cargo test -p reposync --lib -- updates::tests::production_tauri_conf_has_no_test_only_updater_markers
```

The overlay files (`tauri.updater-e2e.conf.json`, `tauri.updater-prod.conf.json`) are
committed but INERT unless explicitly passed via `--config`; they never merge into the
production `tauri.conf.json`, which stays TLS-only with no test pubkey.
