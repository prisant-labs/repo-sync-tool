# Runbook: cut a tag and publish a release

The tag-cutting ceremony for RepoSync, adapted from the `pm-skills` 6-gate runbook for a Tauri desktop app. Gates G0 through G4, plus two sub-gates (G1.5, G2.5) for release-PR and version-bump mechanics that the generic 6-gate template did not need to spell out. No gate may be bypassed; each is a deliberate go/no-go.

This is the EXECUTE + NOTES layer. The PLAN layer is the release plan (`plan_vX.Y.Z/plan_vX.Y.Z.md`).

> **THE REPO IS PUBLIC (since 2026-07-17). Three rules in this runbook changed at that moment, and they are the ones most likely to be followed from memory:**
>
> 1. **Merging to `main` is a human decision, enforced by agreement rather than by GitHub.** The private-era "agent merges autonomously once CI is green" allowance in G1.5 is over. Read the mechanism note under G1.5 before assuming branch protection backs this up: it does not, and it cannot.
> 2. **Cutting a release tag and publishing a Release is HUMAN-ONLY** (`EXECUTION.md`). The "Private repo, agent-cuttable" note under G3 no longer applies, and neither does the G3 manual-fallback path, which was explicitly scoped to a private/pre-public cut.
> 3. **`main` is branch-protected.** A pull request plus FOUR green checks is required - `build (windows-latest)`, `build (macos-latest)`, `slow git-fixture tests (windows)`, and `dependency advisories` (added to the required set 2026-07-31, when the gate itself landed; before that it ran and blocked nothing). Force-push and deletion are blocked, and protection is enforced on admins. The tag must sit on a commit reachable from `main`, which the release preflight now enforces mechanically (see G0).
>
> The **Public flip checklist appendix** at the bottom is now a RECORD of a completed milestone, not a to-do list. Its per-row status is marked there.

**Historical context (v0.9.0 shipped PRIVATE).** The ratified 2026-07-04 decision (see `plan_v0.9.0/plan_v0.9.0.md`) was: ship v0.9.0 complete, with the full ceremony below, but keep the repo private, with public launch as a separate later milestone. That milestone has since happened. The private-era framing is kept in this document only where it explains why a gate is shaped the way it is.

## Preconditions

- [ ] Every phase before Ship in the release's execution plan is done (see `plan_v0.9.0/execution-plan.md`'s phase table): Phase 0 Rails, Phase 1 Correctness, Phase 2 Dogfood, Phase 3 OS integration completion, Phase 4 New features. This ceremony (G0 onward) is that plan's Phase 5, Ship (private).
- [ ] CI is green on the release PR's head commit (both runners: Windows build + bundle, macOS build + bundle, all gates). For v0.9.0 this is PR #2 (Build RepoSync V1).
- [ ] The Codex (or equivalent) adversarial review of the final integration diff is clean, not just each effort's own review (detailed in G1).
- [ ] The dogfood report is filed (Phase 2 of the execution plan) and its findings are fixed in-branch or explicitly filed to `docs/backlog.md` with an owning effort.
- [ ] You are on a clean working tree, on the release PR's branch. You move to `main` at G1.5 once that PR merges.
- [ ] The release plan's readiness checks have been reviewed and you understand what is red.

## G0: Pre-tag readiness

> **Partly automated since 2026-07-30.** `release.yml` now runs a `preflight` job that the build matrix `needs`, so a tag CANNOT build until it passes. It mechanically enforces: the tagged commit is reachable from `main`; the tag agrees with all FOUR version sources (`package.json`, `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml`, and `[workspace.package]` in the root `Cargo.toml`); `CHANGELOG.md` has a matching section; `SECURITY.md` exists; fmt, clippy, tests, bindings freshness, the frontend gates, and the no-Tauri-in-core rule all pass on the tagged commit; and the updater config-hygiene script runs. Those checks no longer depend on anyone remembering them. The boxes below remain for the judgement calls the workflow cannot make.

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
- [ ] Merge the PR into `main`. **A human decides the merge** (the repo is public; `EXECUTION.md`'s human-only list). The private-era agent-autonomous merge ended at the 2026-07-17 flip. Branch protection independently requires the four checks green and the branch up to date with `main`, so a stale branch must be updated before it can merge.

> **What enforces what, because this used to read as though GitHub enforced all of it.**
>
> Branch protection on `main` enforces exactly two things: the four required status checks, and strict up-to-date-ness. `required_approving_review_count` is **0**, and that is deliberate rather than an oversight.
>
> Raising it would not do what it appears to. There is one collaborator with write access, so an approval requirement binds nobody except the person it is meant to empower - and GitHub does not permit self-approval, so with `enforce_admins` on it would make merging impossible rather than reviewed. Outside contributors are already unable to merge, because they have no write access at all.
>
> More to the point, the rule exists to stop an AGENT merging unreviewed, and that is not mechanizable here: an agent acting with the maintainer's token is indistinguishable from the maintainer at the protection layer. No GitHub setting can tell those apart.
>
> So the no-self-merge rule is a **working agreement**, and it lives where agents actually read it: the "Hard conventions" section of `AGENTS.md`. This note exists so nobody re-derives the above and concludes the protection config is misconfigured.
>
> Revisit if a second maintainer or an outside contributor with write access ever appears. At that point an approval requirement starts binding someone real, and `enforce_admins` becomes the knob to reconsider.
- [ ] Confirm `main` is green after the merge itself, not just on the pre-merge PR head. A merge can surface conflicts or interactions the PR view never ran.

## G2: Version bump + CHANGELOG

- [ ] On `main`, run `node scripts/bump-version.mjs X.Y.Z`. Confirm all four version sources agree: root `Cargo.toml` (`[workspace.package]`), `src-tauri/Cargo.toml` (`[package]`), `package.json`, `src-tauri/tauri.conf.json`.
- [ ] `cargo check` and `pnpm install` still succeed after the bump (lockfiles updated if needed).
- [ ] In `CHANGELOG.md`, move the `[Unreleased]` items into a new `## [X.Y.Z] - YYYY-MM-DD` section; leave a fresh empty `[Unreleased]`.

## G2.5: Commit release-prep and re-verify

- [ ] Commit the version bump + CHANGELOG as a single "release: vX.Y.Z" commit directly on `main`.
- [ ] Re-run the local gate (cargo check/clippy/test/fmt, the `cargo tree -p reposync-core` no-tauri check, pnpm typecheck/lint/build) and confirm green.
- [ ] **Updater config-hygiene gate (E-18).** Confirm the committed production `src-tauri/tauri.conf.json` contains NEITHER `dangerousInsecureTransportProtocol` NOR the disposable test pubkey - both belong only in the test-only E2E overlay (`src-tauri/tauri.updater-e2e.conf.json`). Run `node scripts/check-updater-config-hygiene.mjs` (or the in-suite `cargo test -p reposync --lib -- updates::tests::production_tauri_conf_has_no_test_only_updater_markers`). A dirty production config blocks the tag. **Now also enforced automatically by the release preflight** (2026-07-30) - until then this script existed but was wired into no workflow, so it depended entirely on someone reaching this line and running it. Running it here is still worth doing to catch the problem before you push a tag rather than after.
- [ ] **Capture the exact commit sha.** The tag goes on THIS sha and only this sha.

## G3: Tag and push

- [ ] Create the annotated tag on the captured sha: `git tag -a vX.Y.Z -m "RepoSync vX.Y.Z"`.
- [ ] Push the tag: `git push origin vX.Y.Z`.
- [ ] `.github/workflows/release.yml` fires on the `v*` tag: builds Windows + macOS with the `dist` profile (full LTO) and creates a DRAFT GitHub Release with both platform artifacts attached, plus the `latest.json` updater manifest (E-18 (auto-update and distribution), see `plan_v0.9.0/E-18-auto-update/spec.md`). **Ship-dark note:** the updater artifacts + `latest.json` are produced ONLY when the `TAURI_SIGNING_PRIVATE_KEY` secret is present (the workflow's "Compute updater build args" step merges `tauri.updater-prod.conf.json` to flip `createUpdaterArtifacts` on). If jp has not yet done the human-only production-key step (generate the keypair -> Actions secrets + commit the real pubkey into `tauri.conf.json`, replacing the ship-dark placeholder), the updater ships DARK: the installers still build and the Release still cuts, but there is no `latest.json`. Verify `latest.json` is present on the draft's assets before moving to G4 IF the key is in place; if shipping dark, note it and move on (updater activation moves to the public-flip checklist).

### G3 fallback: manual cut when `release.yml` cannot run - NO LONGER AVAILABLE

> **Retired 2026-07-17 (the public flip).** This path was explicitly scoped to a private/pre-public cut, where an agent could cut the Release itself. On a public repo, cutting a release tag and publishing a Release is human-only per `EXECUTION.md`, and the whole point of the new preflight is that artifacts are not built from unverified code. Bypassing CI by hand would defeat both. The section is kept as the record of how the v0.9.0 cut actually happened (decision D4), not as an option.
>
> If Actions genuinely cannot run, the correct response is to fix Actions, not to route around the gate. Actions is free and unlimited for public repositories, so the original billing block that forced this fallback should no longer be reachable.

If `release.yml` cannot run (GitHub Actions unavailable, billing exhausted, or the workflow fails to start), cut the Release by hand instead of waiting on CI:

- [ ] Build installers locally: `pnpm tauri build`. A keyless build ships dark (`createUpdaterArtifacts` stays off), the same ship-dark posture as the CI path without the signing secret.
- [ ] Cut the Release directly:
  ```
  gh release create v<x.y.z> --prerelease --title "RepoSync v<x.y.z> (private)" --notes-file <changelog-body-file> target/release/bundle/nsis/*-setup.exe target/release/bundle/msi/*.msi
  ```
- [ ] This manual path is permitted for a private/pre-public cut (an agent may do it, per `EXECUTION.md`); it is not available once the repo goes public, where cutting a release is human-only.
- [ ] Record the manual cut and its reason (which precondition was unavailable and why) as a waiver in the release plan's Open Questions / Decisions section, per the No-bypass policy below. v0.9.0's own waiver is decision D4 in `plan_v0.9.0/plan_v0.9.0.md`.

**One version, both platforms.** The single bumped version stamps both the Windows MSI/NSIS and the macOS `.app`/`.dmg`. The platform lives in the artifact filename, not the version. macOS is unsigned until signing is unblocked (human-only per `EXECUTION.md`); say so in the Release notes rather than blocking the Windows cut.

**Human-only, since 2026-07-17.** Cutting this tag and publishing the Release is a human action. `EXECUTION.md`'s human-only line covers "Cutting a public release tag / GitHub Release", and the repo is public, so it applies. The private-era agent-autonomous cut described in the v0.9.0 ship decision was scoped to the private repo and ended at the flip. An agent may prepare everything up to the tag (version bump, changelog, gates green, a release PR ready for review); pushing the tag and publishing the Release is yours.

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

## Appendix: Public flip checklist - EXECUTED 2026-07-17, partially complete

> **This is now a RECORD, not a plan.** The flip happened on 2026-07-17. The rows below carry their real status, because the flip went ahead with several rows still open - which is a legitimate call for a pre-release project, but only if the open ones stay visible instead of being quietly inherited as "done."
>
> **Status at 2026-07-30: 5 done, 5 still open.** Everything still open is human-gated and money- or hardware-blocked. Nothing on this list is agent-doable.
>
> | Row | Status |
> |---|---|
> | Repo visibility change | **DONE** 2026-07-17 |
> | GitHub Actions actually runs | **DONE** - verified green on this repo |
> | License and community files | **DONE** - LICENSE, CONTRIBUTING.md, SECURITY.md all present |
> | README install instructions | **DONE** 2026-07-30 |
> | Updater endpoint verified live | **DONE** in part - endpoint resolves; the end-to-end install proof still needs the production key |
> | Windows Authenticode signing | **OPEN** - BL-DEC-01, money-gated |
> | macOS notarization and signing | **OPEN** - BL-DEC-02, Apple enrollment |
> | Updater production key + activation | **OPEN** - the updater still ships DARK behind the placeholder key |
> | Winget manifest submitted | **OPEN** - needs a public installer URL from a signed release |
> | Re-run G0-G4 for the next tag | **OPEN** - no tag cut since the flip |

This was the readiness bar for the human-only milestone where RepoSync's repo went from private to public (`EXECUTION.md`: "Flipping the repo from private to public"). It did not happen at a version tag and was not gated by G0 through G4. Some rows assume v0.9.0 as the private-ship baseline.

- [x] **Repo visibility change.** DONE 2026-07-17. The repo is public at `github.com/prisant-labs/repo-sync-tool`, MIT-licensed, under the `prisant-labs` org. From this point on, merges to `main` require human review (`EXECUTION.md` merge policy); the private-era agent self-merge autonomy ended here. `main` is branch-protected: PR required, three green checks, strict up-to-date, conversation resolution, force-push and deletion blocked, enforced on admins.
- [ ] **GitHub Actions actually runs.** `release.yml` and CI must genuinely run on the tag that activates the public flip; the v0.9.0 cut worked around a hard block with the G3 manual fallback above, which is not a substitute for working CI going forward. History: at the v0.9.0 cut the then-host `product-on-purpose` org (free plan) had exhausted its Actions billing, so every job was rejected before starting (decision D4 in `plan_v0.9.0/plan_v0.9.0.md`). Two things have since changed the premise, and BOTH should be confirmed rather than assumed: (1) 2026-07-17, the repo moved to the `prisant-labs` org (team plan), which may lift the block on its own; (2) Actions is free and unlimited for PUBLIC repos, so the visibility flip in the first row above may moot this row entirely. Verify a green run on this repo before relying on either.
- [ ] **Windows Authenticode code-signing.** Procure a code-signing certificate (or adopt Azure Trusted Signing) and wire the secret into CI so Windows installers stop shipping unsigned. Human-only and money-gated (a CA identity-validation cost); tracked as BL-DEC-01 (Windows code-signing) in `docs/backlog.md`.
- [ ] **macOS notarization and signing.** Unblock Apple Developer Program enrollment and notarization credentials so the macOS `.app`/`.dmg` can ship signed instead of the compile-verified-only posture decided for v0.9.0 (decision D2 in `plan_v0.9.0/plan_v0.9.0.md`).
- [ ] **License and community files verified current.**
  - [ ] `LICENSE` (MIT, already present at the repo root) still matches the intended terms.
  - [ ] `CONTRIBUTING.md` (already present) reflects the actual contribution workflow at flip time, not the pre-launch internal one.
  - [x] `SECURITY.md` exists and states a vulnerability-reporting process. **DONE** 2026-07-17 (PR #25): reporting goes through GitHub's private vulnerability advisory form, which was enabled on the repo at the same time. A published [security model](../../security-model.md) followed on 2026-07-30, covering trust boundaries, the controls in place, and the weaknesses still open.
- [ ] **Updater production key + activation (if it shipped DARK).** E-18 (auto-update and distribution) shipped DARK if jp had not yet done the human-only key step by the ship phase. To activate: (1) `pnpm tauri signer generate` the production keypair (human-only, never held by an agent); (2) put the private key content + password in the GitHub Actions secrets `TAURI_SIGNING_PRIVATE_KEY` + `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`, and keep a separate human-held backup of the private key; (3) commit the PUBLIC key into `src-tauri/tauri.conf.json` `plugins.updater.pubkey`, replacing the `SHIP_DARK__...` placeholder; (4) cut the activating release - `release.yml` then signs the updater artifacts and emits `latest.json` automatically. Rotation is one-way: existing installs trust only the key that shipped in their build, so a lost/rotated key needs a bridging release still signed with the OLD key carrying the NEW pubkey (see the E-18 spec risk section).
- [ ] **Winget manifest submitted.** E-18 prepares the winget manifest under `packages/winget/` during v0.9.0 (passes `winget validate` offline), but submission to `microsoft/winget-pkgs` waits until here: winget requires public artifact URLs that do not exist while the repo is private. At the flip: finalize the placeholder `InstallerUrl` + `InstallerSha256` in `packages/winget/PrisantLabs.RepoSync.installer.yaml` against the real public installer asset, re-run `winget validate --manifest packages/winget`, then `wingetcreate submit packages/winget` and track the moderation PR.
- [ ] **Updater endpoint verified live.** Confirm the `tauri-plugin-updater` endpoint (the `latest.json` URL baked into the shipped installer) resolves publicly and unauthenticated, not merely from a collaborator's already-authenticated GitHub session. Private-repo release assets require auth to fetch; a public repo does not, but verify it directly rather than assuming the visibility flip alone fixes it. Confirm an installed older-version client detects, verifies, and installs the update end to end against the live TLS endpoint (the flow proven locally now via `scripts/updater-e2e.md`).
- [x] **README install instructions updated. DONE 2026-07-30.** The repo went public on 2026-07-17 with a 16-byte README (`# repo-sync-tool`), so this row was outstanding for 13 days on a public repository. `README.md` now carries what RepoSync is, the safety model, install and build-from-source instructions, a doc index, and an explicit platform and signing status table stating that the installers are unsigned, macOS is unvalidated on hardware, and the updater is dark. The `winget install` line still waits on the submission row above.
- [ ] **Re-run this runbook's G0 through G4 ceremony for the next tag**, if the flip is not happening at the same moment as a version bump. The flip and a release are independent events that may or may not coincide.
