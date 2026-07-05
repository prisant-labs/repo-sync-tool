# macOS signing and notarization runbook (E-12 tracer bullet)

Documentation only. Nothing in this file is executed on Windows hardware, and
the credential-bearing steps are HUMAN-ONLY per `EXECUTION.md`. This runbook
records the path so it is ready the moment real Mac access and Apple credentials
exist; it is not a task the agent runs.

## Why this is documentation, not automation

RepoSync ships Windows first. macOS is "compiles + bundles in CI only" until
real Mac access exists (see `EXECUTION.md` -> CI gates). Signing and
notarization require an Apple Developer account, a Developer ID certificate, and
storing those secrets in CI. Every one of those is on the human-only list:

| Step | Why it is HUMAN-ONLY (per EXECUTION.md) |
| --- | --- |
| Apple Developer Program enrollment | Money (paid annual fee) + legal identity verification |
| Obtaining the Developer ID Application certificate | Tied to the enrolled legal identity |
| Storing signing / notarization secrets in CI | Custody of credentials and legal responsibility for their use |
| Cutting a public macOS release | Publishing; users install it; effectively irreversible |

The agent may build an UNSIGNED macOS bundle in CI (agent-safe: "Build unsigned
local artifacts for inspection"). It may NOT enroll, procure certificates, store
secrets, or publish a signed build. When the project reaches signed macOS
releases, a human performs the credential steps below and the CI runner (holding
those secrets) performs the mechanical steps.

## The credential model: CI runner holds the Apple secrets

The signing identity and notarization credentials live as CI secrets on the
macOS runner, never on a developer machine and never in the repo. A human
operator stores them once (HUMAN-ONLY); the runner consumes them at build time.
The secrets needed:

- `APPLE_CERTIFICATE` - base64 of the Developer ID Application `.p12`.
- `APPLE_CERTIFICATE_PASSWORD` - the `.p12` export password.
- `APPLE_SIGNING_IDENTITY` - e.g. `Developer ID Application: Name (TEAMID)`.
- `APPLE_ID` + `APPLE_PASSWORD` (an app-specific password) OR an App Store
  Connect API key (`APPLE_API_KEY`, `APPLE_API_ISSUER`, `APPLE_API_KEY_PATH`).
- `APPLE_TEAM_ID` - the 10-character team identifier.

On the runner, the certificate is imported into a temporary keychain that is
created, unlocked, and deleted within the job so the identity never persists on
the host.

## The mechanical path: codesign -> notarytool -> stapler

The signing identity must be a Developer ID Application certificate. Hardened
runtime is required for notarization.

### 1. Sign (codesign)

Sign nested code inside-out, then the app bundle last, with hardened runtime and
a timestamp. `tauri build` performs this when the signing identity is present in
the environment; the equivalent manual invocation for the `.app` is:

```sh
codesign --force --options runtime --timestamp \
  --sign "$APPLE_SIGNING_IDENTITY" \
  "RepoSync.app"

# Verify the signature and that hardened runtime is on.
codesign --verify --deep --strict --verbose=2 "RepoSync.app"
codesign --display --verbose=4 "RepoSync.app"
```

For distribution as a `.dmg`, sign the `.dmg` after creating it, or notarize the
`.app` (zipped) and then build and staple the `.dmg`.

### 2. Notarize (xcrun notarytool submit)

Submit a zipped `.app` (or the `.dmg`) to Apple and wait for the result.
`notarytool` replaced the deprecated `altool`.

```sh
# Zip the app for submission (ditto preserves macOS metadata).
ditto -c -k --keepParent "RepoSync.app" "RepoSync.zip"

# Submit and block until Apple returns accepted/invalid.
xcrun notarytool submit "RepoSync.zip" \
  --apple-id "$APPLE_ID" \
  --password "$APPLE_PASSWORD" \
  --team-id "$APPLE_TEAM_ID" \
  --wait

# (Alternative auth) App Store Connect API key:
# xcrun notarytool submit "RepoSync.zip" \
#   --key "$APPLE_API_KEY_PATH" --key-id "$APPLE_API_KEY" \
#   --issuer "$APPLE_API_ISSUER" --wait

# On rejection, pull the log to see what failed.
# xcrun notarytool log <submission-id> --apple-id ... --password ... --team-id ...
```

### 3. Staple (xcrun stapler staple)

Attach the notarization ticket to the artifact so Gatekeeper validates offline.
Staple the distributable artifact (the `.app` and/or the `.dmg`).

```sh
xcrun stapler staple "RepoSync.app"
xcrun stapler validate "RepoSync.app"

# If distributing a .dmg, build it from the stapled .app, then staple the dmg:
# xcrun stapler staple "RepoSync.dmg"
```

## Tauri specifics

Tauri's macOS bundler signs during `tauri build` when the signing environment
variables above are present, and can run notarization for you. The relevant
config lives under `bundle.macOS` in `tauri.conf.json` (e.g.
`signingIdentity`, `hardenedRuntime`, `entitlements`, `providerShortName`).
This tracer leaves `tauri.conf.json` bundle config UNCHANGED; wiring the macOS
signing config is a later, human-gated step. Until then, CI builds an unsigned
macOS bundle for inspection only.

## CI sketch (for when secrets exist - do not enable now)

A macOS job, gated so it only signs on release tags and only when the secrets
are present:

1. Import `$APPLE_CERTIFICATE` into a temporary keychain (create, unlock, set as
   default for the job; delete the keychain on cleanup).
2. Run `tauri build` with the signing identity and notarization credentials in
   the environment; let Tauri sign + submit, or run the three steps above
   manually.
3. Staple the resulting artifact and upload it.

Until a human completes enrollment, obtains the certificate, and stores the
secrets, this job stays in "build unsigned bundle" mode. Flipping it on is a
HUMAN-ONLY action.
