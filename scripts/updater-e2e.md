# Updater end-to-end proof (local channel) - E-18

The auto-updater (E-18) is fully wired and signed, but the LIVE GitHub Releases
endpoint cannot be exercised while the repo is private (an unauthenticated GET of
`releases/latest/download/latest.json` returns 404). This runbook proves the whole
mechanism NOW against a local `http://localhost` channel, using a **disposable test
keypair** an agent may freely generate (it never ships) and the **test-only config
overlay** `src-tauri/tauri.updater-e2e.conf.json`. It exercises every step the live
endpoint will later exercise except the live endpoint itself.

This is a dogfood / manual procedure: it builds two full `dist` bundles and drives a
real OS install, so it is NOT run in the fast gate. The pure decision logic (version
gating, the reachable-vs-unreachable mapping, the ship-dark decision, and the
config-hygiene grep) is unit-tested in `src-tauri/src/updates.rs`.

## What this proves locally (while private)

- detect -> download -> **signature-verify** -> install -> relaunch-at-new-version
- tampered artifact is rejected (verification failure, current version retained)
- offline / unreachable is handled gracefully (silent launch check, gentle manual copy)
- downgrade protection (an equal/older manifest version yields "up to date")
- the `auto_update_check` toggle gates the launch check but not the manual button

## What waits for the PUBLIC FLIP (cannot run while private)

- the LIVE TLS endpoint serving `latest.json` from a public GitHub Release
- a shipped client (which enforces TLS) fetching + installing over the real endpoint
- the winget submission (`wingetcreate submit`)

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
