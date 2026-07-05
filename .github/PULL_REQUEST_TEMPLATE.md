<!-- Thanks for contributing to RepoSync. Please fill out the checklist below. -->

## What this PR does

<!-- A short description of the change and the effort it belongs to, e.g. "E-03 (git engine): add ahead/behind parser". -->

## Related effort / issue

<!-- Link the effort folder (docs/internal/release-plans/plan_v0.9.0/E-NN-slug/) and/or issue this addresses. -->

## Checklist

- [ ] Branch was created off the default branch (no direct commits to it)
- [ ] `cargo check`, `cargo clippy --all -- -D warnings`, and `cargo test` pass locally
- [ ] `pnpm typecheck` and `pnpm lint` pass locally
- [ ] `reposync-core` pulls no `tauri` dependency (dependency-hygiene gate stays green)
- [ ] No em-dashes (U+2014) or en-dashes (U+2013) anywhere in the diff
- [ ] Tests were added or updated for the behavior changed
- [ ] Docs / effort spec updated if the contract changed

## Notes for reviewers

<!-- Anything that needs a closer look, known gaps, or follow-ups. -->
