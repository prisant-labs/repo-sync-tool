# Contributing to RepoSync

Thanks for your interest in contributing. RepoSync is a cross-platform
(Windows-first) desktop tray utility built with Tauri v2 (Rust) and a
React/TypeScript frontend. This guide covers local setup, the quality gates,
and the standing rules every change must respect.

## Prerequisites

You need the following installed:

- Rust (stable toolchain). The repo pins the channel in `rust-toolchain.toml`;
  `rustup` will pick it up automatically.
- pnpm (the frontend package manager) and a current Node LTS.
- A C toolchain and the platform WebView dependencies required by Tauri v2.
  See the Tauri prerequisites guide for your operating system.

Install dependencies:

```sh
pnpm install
```

## Quality gates

Every pull request must pass the same gates that CI runs. Run them locally
before you open a PR.

Rust:

```sh
cargo check --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Frontend (from the repo root):

```sh
pnpm typecheck
pnpm lint
```

A green pull request means all of the above pass with no warnings.

## Branch-first workflow

Do not commit directly to `main`. Create a branch for your work, push it, and
open a pull request. Keep changes focused on a single concern so reviews stay
small and fast.

## Standing rules

These rules are non-negotiable and are enforced in review (and, where noted,
by tooling).

1. **No em-dashes or en-dashes.** Do not use the em-dash (U+2014) or en-dash
   (U+2013) anywhere - not in code, comments, docs, JSON, or YAML. Use
   " - " (space hyphen space) or restructure the sentence. For numeric ranges
   use a plain hyphen (for example "2-5").

2. **`reposync-core` must not depend on Tauri.** The `crates/reposync-core`
   crate must not depend on `tauri` or any `tauri-*` crate, directly or
   transitively. Core stays portable and Tauri-free; the Tauri integration
   lives only in `src-tauri`.

## Project layout

The frontend lives at the repository root (`package.json`, `vite.config.ts`,
`tsconfig*.json`, `index.html`, `src/**`). The `src-tauri` directory is a
sibling that holds the desktop shell, and shared portable logic lives in
`crates/reposync-core`.

## Execution plan

Work is organized into efforts. See [EXECUTION.md](EXECUTION.md) for the
execution plan and how the efforts fit together.
