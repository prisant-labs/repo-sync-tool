# Runbook: cut a tag and publish a release

The tag-cutting ceremony for RepoSync, adapted from the `pm-skills` 6-gate runbook for a Tauri desktop app. Gates G0 through G4, plus two sub-gates (G1.5, G2.5) for release-PR and version-bump mechanics that the generic 6-gate template did not need to spell out. No gate may be bypassed; each is a deliberate go/no-go.

This is the EXECUTE + NOTES layer. The PLAN layer is the release plan (`plan_vX.Y.Z/plan_vX.Y.Z.md`).

**v0.9.0 ships PRIVATE.** The ratified 2026-07-04 decision (see `plan_v0.9.0/plan_v0.9.0.md`) is: ship v0.9.0 complete, with the full ceremony below (flip the release PR, merge, tag, GitHub Release), but the repo itself stays private. Public launch is a separate, later, human-only milestone ("the public flip"), not a version tag. That milestone is not new G-numbered gates here, since it is a one-time event rather than a per-release ceremony; it is the **Public flip checklist appendix** at the bottom of this document.

## Preconditions

- [ ] Every phase before Ship in the release's execution plan is done (see `plan_v0.9.0/execution-plan.md`'s phase table): Phase 0 Rails, Phase 1 Correctness, Phase 2 Dogfood, Phase 3 OS integration completion, Phase 4 New features. This ceremony (G0 onward) is that plan's Phase 5, Ship (private).
- [ ] CI is green on the release PR's head commit (both runners: Windows build + bundle, macOS build + bundle, all gates). For v0.9.0 this is PR #2 (Build RepoSync V1).
- [ ] The Codex (or equivalent) adversarial review of the final integration diff is clean, not just each effort's own review (detailed in G1).
- [ ] The dogfood report is filed (Phase 2 of the execution plan) and its findings are fixed in-branch or explicitly filed to `docs/backlog.md` with an owning effort.
- [ ] You are on a clean working tree, on the release PR's branch. You move to `main` at G1.5 once that PR merges.
- [ ] The release plan's readiness checks have been reviewed and you understand what is red.

## G0: Pre-tag readiness

- [ ] The release plan's readiness checks all pass and every doc-update checklist box is checked (verify by hand; a release tool may automate this later).
- [ ] CI is green on the release commit (both runners: Windows build + bundle, macOS build + bundle, all gates).
- [ ] The GitHub milestone `vX.Y.Z` is at 100% (every effort issue closed), if issues are in use.
- [ ] No open blocker-labelled issues for this milestone.

**Blocking rule:** any red gate or non-green CI stops the cut. Fix or explicitly waive (a waiver is a documented decision in the plan, not a silent skip).

## G1: Adversarial review status

- [ ] Every substantial effort in the release has had its Codex (or equivalent) adversarial review, with findings fixed-in-effort or filed to `docs/backlog.md` with an owning effort.
- [ ] The final integration pass, the whole diff since the last release rather than just per-effort diffs, has its own Codex adversarial review, and it is clean.
- [ ] No unaddressed high-severity finding remains open for in-scope work.

## G1.5: Flip and merge the release PR

- [ ] Flip the release PR from draft to ready for review. For v0.9.0 this is PR #2 (Build RepoSync V1).
- [ ] Merge the PR into `main`. While the repo is private, the agent merges autonomously once CI is green (EXECUTION.md's private/pre-public merge policy); once the repo is public, this merge requires human review (EXECUTION.md's human-only list). The public flip does not happen automatically here; it stays a separate decision (see the appendix).
- [ ] Confirm `main` is green after the merge itself, not just on the pre-merge PR head. A merge can surface conflicts or interactions the PR view never ran.

## G2: Version bump + CHANGELOG

- [ ] On `main`, run `node scripts/bump-version.mjs X.Y.Z`. Confirm all four version sources agree: root `Cargo.toml` (`[workspace.package]`), `src-tauri/Cargo.toml` (`[package]`), `package.json`, `src-tauri/tauri.conf.json`.
- [ ] `cargo check` and `pnpm install` still succeed after the bump (lockfiles updated if needed).
- [ ] In `CHANGELOG.md`, move the `[Unreleased]` items into a new `## [X.Y.Z] - YYYY-MM-DD` section; leave a fresh empty `[Unreleased]`.

## G2.5: Commit release-prep and re-verify

- [ ] Commit the version bump + CHANGELOG as a single "release: vX.Y.Z" commit directly on `main`.
- [ ] Re-run the local gate (cargo check/clippy/test/fmt, the `cargo tree -p reposync-core` no-tauri check, pnpm typecheck/lint/build) and confirm green.
- [ ] **Updater config-hygiene gate (E-18).** Confirm the committed production `src-tauri/tauri.conf.json` contains NEITHER `dangerousInsecureTransportProtocol` NOR the disposable test pubkey - both belong only in the test-only E2E overlay (`src-tauri/tauri.updater-e2e.conf.json`). Run `node scripts/check-updater-config-hygiene.mjs` (or the in-suite `cargo test -p reposync --lib -- updates::tests::production_tauri_conf_has_no_test_only_updater_markers`). A dirty production config blocks the tag.
- [ ] **Capture the exact commit sha.** The tag goes on THIS sha and only this sha.

## G3: Tag and push

- [ ] Create the annotated tag on the captured sha: `git tag -a vX.Y.Z -m "RepoSync vX.Y.Z"`.
- [ ] Push the tag: `git push origin vX.Y.Z`.
- [ ] `.github/workflows/release.yml` fires on the `v*` tag: builds Windows + macOS with the `dist` profile (full LTO) and creates a DRAFT GitHub Release with both platform artifacts attached, plus the `latest.json` updater manifest (E-18 (auto-update and distribution), see `plan_v0.9.0/E-18-auto-update/spec.md`). **Ship-dark note:** the updater artifacts + `latest.json` are produced ONLY when the `TAURI_SIGNING_PRIVATE_KEY` secret is present (the workflow's "Compute updater build args" step merges `tauri.updater-prod.conf.json` to flip `createUpdaterArtifacts` on). If jp has not yet done the human-only production-key step (generate the keypair -> Actions secrets + commit the real pubkey into `tauri.conf.json`, replacing the ship-dark placeholder), the updater ships DARK: the installers still build and the Release still cuts, but there is no `latest.json`. Verify `latest.json` is present on the draft's assets before moving to G4 IF the key is in place; if shipping dark, note it and move on (updater activation moves to the public-flip checklist).

### G3 fallback: manual cut when `release.yml` cannot run

If `release.yml` cannot run (GitHub Actions unavailable, billing exhausted, or the workflow fails to start), cut the Release by hand instead of waiting on CI:

- [ ] Build installers locally: `pnpm tauri build`. A keyless build ships dark (`createUpdaterArtifacts` stays off), the same ship-dark posture as the CI path without the signing secret.
- [ ] Cut the Release directly:
  ```
  gh release create v<x.y.z> --prerelease --title "RepoSync v<x.y.z> (private)" --notes-file <changelog-body-file> target/release/bundle/nsis/*-setup.exe target/release/bundle/msi/*.msi
  ```
- [ ] This manual path is permitted for a private/pre-public cut (an agent may do it, per `EXECUTION.md`); it is not available once the repo goes public, where cutting a release is human-only.
- [ ] Record the manual cut and its reason (which precondition was unavailable and why) as a waiver in the release plan's Open Questions / Decisions section, per the No-bypass policy below. v0.9.0's own waiver is decision D4 in `plan_v0.9.0/plan_v0.9.0.md`.

**One version, both platforms.** The single bumped version stamps both the Windows MSI/NSIS and the macOS `.app`/`.dmg`. The platform lives in the artifact filename, not the version. macOS is unsigned until signing is unblocked (human-only per `EXECUTION.md`); say so in the Release notes rather than blocking the Windows cut.

**Private repo, agent-cuttable.** While RepoSync stays private, cutting this tag and Release is agent-autonomous under the ratified v0.9.0 ship decision and `EXECUTION.md`'s private/pre-public merge policy: `EXECUTION.md`'s human-only line is scoped to a *public* release tag specifically ("Cutting a public release tag / GitHub Release"). The moment the repo goes public (its own, separate, human-only decision), cutting a release tag becomes human-only too. See the Public flip checklist appendix.

## G4: Post-tag hygiene

- [ ] **Installer smoke test, from the download, not the local build.** Download the Windows installer (and `latest.json`, if present) directly from the draft Release's asset URLs, the same way a real user would fetch them, not from local build output. Run the installer end to end on the downloaded artifact: install, launch, confirm the app starts, and confirm the update check reads `latest.json` cleanly if the updater has landed. A green local build only tells you the code works; only the downloaded artifact tells you the upload and packaging pipeline works.
- [ ] Edit the draft Release: paste the `CHANGELOG.md` vX.Y.Z section as the body; confirm both artifacts (and `latest.json`, once applicable) are attached; state BOTH platforms' signing posture, not just macOS's: macOS (shipped-unsigned-beta or deferred) AND Windows (installers signed with Authenticode, or unsigned - state which).
- [ ] Publish the Release.
- [ ] Set the release plan frontmatter `status: released`.
- [ ] Open a fresh `[Unreleased]` section in `CHANGELOG.md` (if not already).
- [ ] Wrap the session (`/jp-wrap-session`).

## No-bypass policy

No gate is skipped to "save time." A waiver is a maintainer decision recorded in the release plan's Open Questions / Decisions section with a reason. A silent skip is not a waiver.

## Rollback semantics

If a published release is broken: delete the tag (`git push origin :vX.Y.Z`) and the GitHub Release, fix forward on the branch, and re-cut as the next patch (`vX.Y.Z+1`). Do not re-point an existing tag at a new sha. A tag is immutable once it has been public.

## Appendix: Public flip checklist

This is the readiness bar for the separate, later, human-only milestone where RepoSync's repo goes from private to public (`EXECUTION.md`: "Flipping the repo from private to public"). It does not happen at a version tag, is not gated by G0 through G4 above, and is not something an agent runs autonomously end to end: jp decides when RepoSync is ready for the world, on whatever version is current at the time. Some rows below assume v0.9.0 as the private-ship baseline; update the specifics if a later version ships first.

- [ ] **Repo visibility change.** jp flips the GitHub repo from private to public (human-only, `EXECUTION.md`). From this point on, merges to `main` require human review (`EXECUTION.md` merge policy); the private-era agent self-merge autonomy ends here.
- [ ] **GitHub Actions billing fixed.** The `product-on-purpose` org's Actions billing must be restored (it was exhausted at the v0.9.0 cut, per decision D4 in `plan_v0.9.0/plan_v0.9.0.md`), so that `release.yml` and CI can actually run on the tag that activates the public flip; the v0.9.0 cut worked around this with the G3 manual fallback above, which is not a substitute for working CI going forward.
- [ ] **Windows Authenticode code-signing.** Procure a code-signing certificate (or adopt Azure Trusted Signing) and wire the secret into CI so Windows installers stop shipping unsigned. Human-only and money-gated (a CA identity-validation cost); tracked as BL-DEC-01 (Windows code-signing) in `docs/backlog.md`.
- [ ] **macOS notarization and signing.** Unblock Apple Developer Program enrollment and notarization credentials so the macOS `.app`/`.dmg` can ship signed instead of the compile-verified-only posture decided for v0.9.0 (decision D2 in `plan_v0.9.0/plan_v0.9.0.md`).
- [ ] **License and community files verified current.**
  - [ ] `LICENSE` (MIT, already present at the repo root) still matches the intended terms.
  - [ ] `CONTRIBUTING.md` (already present) reflects the actual contribution workflow at flip time, not the pre-launch internal one.
  - [ ] `SECURITY.md` exists (it does not yet, as of the v0.9.0 private ship) and states a vulnerability-reporting contact and process. Public repos are expected to carry one.
- [ ] **Updater production key + activation (if it shipped DARK).** E-18 (auto-update and distribution) shipped DARK if jp had not yet done the human-only key step by the ship phase. To activate: (1) `pnpm tauri signer generate` the production keypair (human-only, never held by an agent); (2) put the private key content + password in the GitHub Actions secrets `TAURI_SIGNING_PRIVATE_KEY` + `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`, and keep a separate human-held backup of the private key; (3) commit the PUBLIC key into `src-tauri/tauri.conf.json` `plugins.updater.pubkey`, replacing the `SHIP_DARK__...` placeholder; (4) cut the activating release - `release.yml` then signs the updater artifacts and emits `latest.json` automatically. Rotation is one-way: existing installs trust only the key that shipped in their build, so a lost/rotated key needs a bridging release still signed with the OLD key carrying the NEW pubkey (see the E-18 spec risk section).
- [ ] **Winget manifest submitted.** E-18 prepares the winget manifest under `packages/winget/` during v0.9.0 (passes `winget validate` offline), but submission to `microsoft/winget-pkgs` waits until here: winget requires public artifact URLs that do not exist while the repo is private. At the flip: finalize the placeholder `InstallerUrl` + `InstallerSha256` in `packages/winget/ProductOnPurpose.RepoSync.installer.yaml` against the real public installer asset, re-run `winget validate --manifest packages/winget`, then `wingetcreate submit packages/winget` and track the moderation PR.
- [ ] **Updater endpoint verified live.** Confirm the `tauri-plugin-updater` endpoint (the `latest.json` URL baked into the shipped installer) resolves publicly and unauthenticated, not merely from a collaborator's already-authenticated GitHub session. Private-repo release assets require auth to fetch; a public repo does not, but verify it directly rather than assuming the visibility flip alone fixes it. Confirm an installed older-version client detects, verifies, and installs the update end to end against the live TLS endpoint (the flow proven locally now via `scripts/updater-e2e.md`).
- [ ] **README install instructions updated.** `README.md` gets real, copy-pasteable install instructions written for a stranger arriving from GitHub (download link, `winget install` command once the submission above lands, the unsigned-macOS caveat if still applicable), replacing any collaborator-oriented notes.
- [ ] **Re-run this runbook's G0 through G4 ceremony for the next tag**, if the flip is not happening at the same moment as a version bump. The flip and a release are independent events that may or may not coincide.
