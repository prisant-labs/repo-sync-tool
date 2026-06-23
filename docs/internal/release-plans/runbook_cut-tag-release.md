# Runbook: cut a tag and publish a release

The tag-cutting ceremony for RepoSync, adapted from the `pm-skills` 6-gate runbook for a Tauri desktop app. Six gates, G0 through G4. No gate may be bypassed; each is a deliberate go/no-go.

This is the EXECUTE + NOTES layer. The PLAN layer is the release plan (`plan_vX.Y.Z/plan_vX.Y.Z.md`).

## Preconditions

- You are on a clean working tree, on the release branch (or `main` once the repo is public and the merge policy flips).
- The release plan's readiness checks have been reviewed and you understand what is red.

## G0: Pre-tag readiness

- [ ] The release plan's readiness checks all pass and every doc-update checklist box is checked (verify by hand; a release tool may automate this later).
- [ ] CI is green on the release commit (both runners: Windows build + bundle, macOS build + bundle, all gates).
- [ ] The GitHub milestone `vX.Y.Z` is at 100% (every effort issue closed), if issues are in use.
- [ ] No open blocker-labelled issues for this milestone.

**Blocking rule:** any red gate or non-green CI stops the cut. Fix or explicitly waive (a waiver is a documented decision in the plan, not a silent skip).

## G1: Adversarial review status

- [ ] Every substantial effort in the release has had its Codex (or equivalent) adversarial review, with findings fixed-in-effort or filed to `docs/backlog.md` with an owning effort.
- [ ] No unaddressed high-severity finding remains open for in-scope work.

## G2: Version bump + CHANGELOG

- [ ] Run `node scripts/bump-version.mjs X.Y.Z`. Confirm all four version sources agree: root `Cargo.toml` (`[workspace.package]`), `src-tauri/Cargo.toml` (`[package]`), `package.json`, `src-tauri/tauri.conf.json`.
- [ ] `cargo check` and `pnpm install` still succeed after the bump (lockfiles updated if needed).
- [ ] In `CHANGELOG.md`, move the `[Unreleased]` items into a new `## [X.Y.Z] - YYYY-MM-DD` section; leave a fresh empty `[Unreleased]`.

## G2.5: Commit release-prep and re-verify

- [ ] Commit the version bump + CHANGELOG as a single "release: vX.Y.Z" commit.
- [ ] Re-run the local gate (cargo check/clippy/test/fmt, the `cargo tree -p reposync-core` no-tauri check, pnpm typecheck/lint/build) and confirm green.
- [ ] **Capture the exact commit sha.** The tag goes on THIS sha and only this sha.

## G3: Tag and push

- [ ] Create the annotated tag on the captured sha: `git tag -a vX.Y.Z -m "RepoSync vX.Y.Z"`.
- [ ] Push the tag: `git push origin vX.Y.Z`.
- [ ] `.github/workflows/release.yml` fires on the `v*` tag: builds Windows + macOS with the `dist` profile (full LTO) and creates a DRAFT GitHub Release with both platform artifacts attached.

**One version, both platforms.** The single bumped version stamps both the Windows MSI/NSIS and the macOS `.app`/`.dmg`. The platform lives in the artifact filename, not the version. macOS is unsigned until signing is unblocked (human-only per `EXECUTION.md`); say so in the Release notes rather than blocking the Windows cut.

## G4: Post-tag hygiene

- [ ] Edit the draft Release: paste the `CHANGELOG.md` vX.Y.Z section as the body; confirm both artifacts attached; state the macOS posture (shipped-unsigned-beta or deferred).
- [ ] Publish the Release.
- [ ] Set the release plan frontmatter `status: released`.
- [ ] Open a fresh `[Unreleased]` section in `CHANGELOG.md` (if not already).
- [ ] Wrap the session (`/jp-wrap-session`).

## No-bypass policy

No gate is skipped to "save time." A waiver is a maintainer decision recorded in the release plan's Open Questions / Decisions section with a reason. A silent skip is not a waiver.

## Rollback semantics

If a published release is broken: delete the tag (`git push origin :vX.Y.Z`) and the GitHub Release, fix forward on the branch, and re-cut as the next patch (`vX.Y.Z+1`). Do not re-point an existing tag at a new sha. A tag is immutable once it has been public.
