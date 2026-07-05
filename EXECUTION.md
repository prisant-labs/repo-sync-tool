# EXECUTION.md - How RepoSync gets built and shipped

This is the operating contract between the human operator (jp) and the AI agent(s) building RepoSync. It defines what an agent may do autonomously, what must stop and hand off to a human, and how merges and releases are gated. Ratified on 2026-06-19 from Section 3 of `docs/internal/v1-architecture-and-decisions.md`.

## Roles

- **Operator (human): jp.** Holds money, legal identity, and publishing authority. Final approver for everything on the human-only list.
- **Agent.** Drives code, tests, CI, and draft pull requests. Autonomous everywhere outside the human-only list, subject to the merge policy.

## The boundary, in one rule

Anything that spends money, asserts a legal identity, publishes to the world, or cannot be cleanly undone stays with the human. Everything upstream of that line is agent-autonomous.

## Agent-safe (proceed without asking)

- Scaffold the Tauri + React + Rust project and workspace
- Write and refactor Rust and TypeScript source
- Author and update unit, integration, and UI tests
- Run CI locally and in GitHub Actions; iterate until the matrix is green
- Run `cargo check`, `clippy`, `cargo test`, `pnpm typecheck`, `pnpm lint`, `pnpm tauri dev` on Windows
- Create feature branches and commit to them
- Open and update **draft** pull requests
- Build unsigned local artifacts for inspection

## Human-only (stop and hand off, with reason)

| Action | Why it is human-only |
| --- | --- |
| Apple Developer Program enrollment | Money (paid annual fee) + legal identity verification |
| Windows code-signing certificate procurement | Money + organizational/identity validation by a CA |
| Storing signing / notarization secrets in CI | Custody of credentials and legal responsibility for their use |
| Flipping the repo from private to public | Publishing decision; irreversible in practice |
| Merging to the default branch once public | Canonical-state decision (see merge policy) |
| Cutting a public release tag / GitHub Release | Publishing; users will install it; effectively irreversible |
| Any force-push or history rewrite on a shared branch | Irreversibility; destroys recoverable history for collaborators |

## Merge policy (visibility-tiered, ratified)

- **While the repo is private / pre-public:** the agent may self-merge pull requests once CI is green. Speed is the priority, and a bad merge is cheap to undo in private history.
- **The moment the repo is public** (itself a human-only action): merges to the default branch require human review. Unreviewed code must not become canonical once the world can install it.
- The agent always branches first and commits/pushes per the workflow; this policy governs the **merge** step specifically.

## CI gates (the boundary checkpoints)

- Required green before any merge: `cargo check`, `cargo clippy --all -- -D warnings`, `cargo test`, `pnpm typecheck`, `pnpm lint`.
- Dependency hygiene gate: `cargo tree -p reposync-core` must show no `tauri` dependency. `reposync-core` stays Tauri-free.
- Build matrix: **Windows** (the real GA bar: launches + human-validated + signed-or-documented) and **macOS** (compiles + bundles in CI only, no human-validated or signed clause, until real Mac access exists).
- Git is pinned in CI so `git` porcelain output stays stable for the fixture harness.

## Standing rules

- Never use em-dashes or en-dashes anywhere. Use " - " or restructure. (Enforced by a PreToolUse hook on Edit/Write.)
- `reposync-core` must never import `tauri`, even transitively.
- Co-authored-by trailer on agent commits.
- No force-push without an explicit human go-ahead.
- Pre-committed **descope triggers** (see `docs/internal/program-roadmap.md`) convert slippage into a deliberate, pre-agreed cut rather than a silent slide.
