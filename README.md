# RepoSync

**A desktop tray utility that keeps a library of cloned Git repositories fresh and visible, without ever touching your work.**

If you keep dozens of repos cloned locally that you *use* rather than *develop* - tools, templates, references, self-hosted services, forks you follow - they go stale silently. You do not find out that a dependency shipped a new version, or that a reference repo moved on without you, until it matters. Keeping current by hand means remembering to `git fetch` across dozens of folders, and doing it in bulk risks clobbering local changes you forgot you made.

RepoSync lives in your system tray, checks those repos on a schedule, tells you which ones need attention, and updates them only when it can do so losslessly.

It is deliberately **not** a Git client. No commit graph, no branch tree, no staging area. If you want to commit, branch, or rebase in a repo, that repo has graduated out of RepoSync's job.

## What it does

- **Registers repos** individually or by scanning a parent folder.
- **Tracks state** per repo: branch, upstream, dirty or clean, ahead and behind counts, and how long since the last local commit.
- **Checks on a schedule** you control, globally or per repo, with quiet hours and bounded concurrency.
- **Updates safely, or refuses to.** Read-only checking, fetch-only, or a guarded fast-forward-only pull.
- **Pauses repos that keep failing** after three strikes, instead of retrying forever.
- **Enriches GitHub repos** with latest-release and open-pull-request counts, unauthenticated, under a hard request budget.
- **Groups repos** into colored labels you define, and filters by them.
- **Records every operation** in an activity log, and rolls the day up into a summary.
- **Integrates natively**: system tray with a full menu, desktop notifications, launch on login, and open-in folder / terminal / editor / remote.

## The safety model

This is the part worth reading before installing anything that runs `git` on a timer. RepoSync is built to never do three things:

1. **It never mutates your working tree unless the update is a clean, lossless fast-forward.** No merges, no rebases, no resets. Ever.
2. **It never pulls over local changes.** A dirty repo is skipped, and told to you with the reason.
3. **It never hides risk behind vague language.** Anything that could surprise your working tree is labeled plainly and is harder to reach than the safe path.

A repo that is dirty, detached, divergent, or otherwise outside policy is reported, not "fixed."

RepoSync is local-first: your data stays in a local SQLite database, there is **no telemetry**, and **no account is required**. The only network calls are to your own Git remotes and, optionally, to the public GitHub API for release and pull-request counts.

## Status: public beta, and honest about it

RepoSync is in **public beta**. It works, it is actively developed, and it is used daily by its author, but it is **not yet packaged to the standard a stranger should expect.** Read this table before installing.

| | Status |
|---|---|
| **Windows** | The genuinely supported target. Built, tested, and dogfooded. |
| **macOS** | **Experimental and unsupported.** Compiles and bundles in CI so the codebase does not rot, but it has **never been run on real Mac hardware by anyone**, and it is neither signed nor notarized. No packaged macOS download is published. Build from source if you want to try it, and expect Gatekeeper to block it. |
| **Linux** | Not a target. |
| **Installers** | **Unsigned.** Windows will show a SmartScreen "unknown publisher" warning on first run. There is no Authenticode certificate yet. |
| **Auto-updater** | Wired but deliberately **dark**. It ships with a placeholder signing key and will not fetch or apply updates. **Update by downloading a new release manually; the in-app updater will not notice one.** |
| **Latest release** | [v0.9.0](https://github.com/prisant-labs/repo-sync-tool/releases/tag/v0.9.0), marked pre-release. Windows artifacts only. |

None of that is an oversight. Code signing, Apple enrollment, and updater key custody are deliberate, human-gated steps that cost money or legal identity and have not happened yet. Shipping an unsigned binary while claiming otherwise would be the real problem. When they land, this table changes.

## Install

Download the Windows installer from the [Releases page](https://github.com/prisant-labs/repo-sync-tool/releases). Both NSIS (`.exe`) and MSI artifacts are produced; the NSIS installer is the one the updater path is built around.

**Expect a SmartScreen warning on first run.** Windows will say "Windows protected your PC" and name an unknown publisher. Click **More info**, then **Run anyway**. That warning is the honest consequence of an unsigned installer, and it will keep appearing until an Authenticode certificate exists. If that is not acceptable to you, build from source instead, which is the better choice anyway if you are the sort of person who reads the code first.

On **macOS** there is nothing to download, and macOS is unsupported. If you build from source, the result is unsigned and un-notarized, so macOS will refuse to open it on the grounds that it cannot verify the developer. Use Apple's per-app exception: try to open it, then go to **System Settings > Privacy & Security** and choose **Open Anyway** for RepoSync specifically. Do not disable Gatekeeper, and do not apply a blanket quarantine override. If macOS instead says the app is **damaged**, that is a different message and it should be believed rather than worked around: RepoSync has never been run on Mac hardware by anyone, so a broken bundle is a live possibility and we cannot tell you which of the two you will see.

## Build from source

You need a stable Rust toolchain (the channel is pinned in `rust-toolchain.toml`, so `rustup` picks it up), pnpm with a current Node LTS, and the C toolchain plus platform WebView dependencies that [Tauri v2 requires](https://v2.tauri.app/start/prerequisites/).

```sh
pnpm install
pnpm tauri dev     # run it
pnpm tauri build   # produce installers
```

## Documentation

| Doc | Read it when |
|---|---|
| [User guide](docs/user-guide.md) | You are setting RepoSync up, or want a feature explained in depth. Includes the settings reference and troubleshooting. |
| [FAQ](docs/faq.md) | You want a quick, plain answer about behavior, safety, platforms, or data. |
| [Architecture](docs/architecture.md) | You want to know what the pieces are and how they fit together. |
| [Security model](docs/security-model.md) | You want the threat model, the controls actually in place, and an honest list of the weaknesses that remain open. |
| [Design rationale](docs/explanation.md) | You want to know *why* it is shaped this way before changing it. |
| [Backlog](docs/backlog.md) | You have an idea, or want to know what is deliberately out of scope. |
| [Contributing](CONTRIBUTING.md) | You want to open a pull request. Start here for the quality gates. |

## How it is built

A Tauri v2 desktop shell (Rust) with a React 19 and TypeScript frontend. The architecture has one load-bearing rule: **all product logic lives in `crates/reposync-core`, which is Tauri-free and CI-enforced to stay that way.** The core compiles and tests with no Tauri, no WebView, and no display server, which is why the scheduler, policy engine, git parsing, and persistence are all testable in plain `cargo test` on headless CI.

The two halves talk over a single typed IPC seam, `src/lib/bindings.ts`, generated from the Rust types by `tauri-specta` and gated in CI against drift, so the frontend cannot silently disagree with the backend.

## Contributing

Contributions are welcome. Read [CONTRIBUTING.md](CONTRIBUTING.md) first: it covers the prerequisites, the quality gates every pull request must pass, and two non-negotiable standing rules (no em-dashes anywhere, and `reposync-core` must never depend on Tauri).

Please also read the [Code of Conduct](CODE_OF_CONDUCT.md).

## Security

Do **not** open a public issue for a security problem. Report it privately through GitHub's security advisory form, as described in [SECURITY.md](SECURITY.md).

## License

[MIT](LICENSE)
