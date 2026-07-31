# RepoSync security model

What RepoSync trusts, what it does not, and what an attacker would have to do. This is a living document: it describes the posture as shipped today, including the parts that are still weak.

Report a vulnerability privately through the process in [SECURITY.md](../SECURITY.md). Do not open a public issue.

## What makes this app worth thinking about

Most desktop utilities are only as dangerous as the user who clicks. RepoSync is different in one specific way: **it runs `git` autonomously, on a timer, inside directories whose contents it did not create.**

That is the whole threat model in one sentence. A repository directory is not inert data. A `.git/config` can point `core.pager`, `credential.helper`, or `core.sshCommand` at an arbitrary executable, and Git hooks are executable files that Git runs on your behalf. If you clone something hostile and RepoSync fetches it sixty seconds later without you present, "I did not run anything" is not true.

Everything below follows from taking that seriously.

## Trust boundaries

RepoSync has four, listed from most to least trusted.

| Boundary | Trust | Why |
|---|---|---|
| The Rust core (`crates/reposync-core`) | Trusted | All product logic. Tauri-free and CI-enforced to stay that way, so it has no WebView, no IPC, and no display surface to attack. |
| The Tauri shell (`src-tauri`) | Trusted | Owns the runtime, the IPC surface, the tray, windows, and the OS plugin edges. |
| The WebView frontend (`src/`) | **Semi-trusted** | It is our code, but it is the surface a supply-chain compromise or an injection would land on. The controls below assume it can be turned against us. |
| Watched repository directories | **Untrusted** | Contents are attacker-influenced by definition. This is the primary attack surface. |

## Controls, and what each one actually buys

### Git execution is argv-safe and config-hardened

Every Git invocation goes through one chokepoint, `run_git` in `crates/reposync-core/src/git/cli.rs`. Nothing else in the codebase spawns `git`.

- Arguments are passed as an argv vector, never through a shell, so repository paths and ref names cannot inject commands regardless of what characters they contain.
- Operations are bounded by timeouts, so a hostile or hung remote cannot wedge the scheduler indefinitely.
- Git configuration that would let a repository nominate an executable is constrained rather than inherited wholesale.

**Residual risk, stated plainly:** repository-local Git configuration is not fully neutralized. `credential.helper` and `core.sshCommand` remain the open edge, tracked in the backlog. Do not register a repository you would not be willing to run code from.

### The WebView cannot reach the network

The production Content-Security-Policy in `src-tauri/tauri.conf.json`:

```
default-src 'self'; script-src 'self'; connect-src ipc: http://ipc.localhost;
object-src 'none'; base-uri 'self'; form-action 'self'; frame-ancestors 'none'
```

`connect-src` is the load-bearing line. The frontend can talk to the Tauri IPC bridge and to nothing else. No `fetch`, no WebSocket, no beacon to any external host. Injected script cannot pull a second-stage payload, and it cannot exfiltrate over the network.

`script-src 'self'` blocks inline and remote script. `object-src 'none'` and `frame-ancestors 'none'` remove plugin and framing vectors.

`style-src` still permits `'unsafe-inline'`, and `style-src-attr` permits inline style attributes, because the UI sets `style={}` in a handful of places (group color swatches, the lag and signal bars). This is a real, if minor, loosening: CSS injection can exfiltrate limited information through selectors and background URLs. Tightening it means removing every inline style binding first.

A separate, stricter `devCsp` applies only under `tauri dev`, where Vite requires `'unsafe-eval'` and a WebSocket to the dev server. **The shipped app never uses `devCsp`.** If you are verifying the CSP, verify a production build.

### The WebView cannot navigate away

A CSP cannot restrict top-level navigation. No fetch directive governs `window.location`, and CSP3's `navigate-to` was dropped from the spec with no browser support, so adding it would be an ignored no-op.

So navigation is guarded separately, in Rust, by an `on_navigation` allowlist in the shell. It permits only the app's own origin (`tauri://localhost`, and on Windows the host `tauri.localhost`) plus the Vite dev server, and only when `tauri::is_dev()` is true. Everything else is denied.

The dev-server allowance is gated on `tauri::is_dev()` rather than `cfg!(debug_assertions)` deliberately: debug assertions are on for a `tauri build --debug` bundle, which is a shipped artifact, so keying on them would ship the dev allowance.

### The updater verifies before it applies

Updates are verified against a committed minisign public key before installation, and a signature failure aborts and keeps the current version. Updates are never silent; the user confirms.

**Current state:** the updater ships **dark**, configured with a placeholder public key, so it does not fetch or apply anything. It activates when the production key is generated and custodied. See the status table in the [README](../README.md).

### Data stays local

State is a local SQLite database in the OS-appropriate per-user data directory. There is no telemetry, no account, and no server. Every SQL statement is parameterized. Outbound network traffic is limited to your own Git remotes and, optionally, the public GitHub API for release and pull-request counts, unauthenticated and under a hard request budget.

## Known weaknesses

Listed because a security document that only lists strengths is marketing.

| Weakness | Impact | Status |
|---|---|---|
| Plugin capabilities are over-granted | `src-tauri/capabilities/default.json` grants `notification:default`, `autostart:default`, `updater:default`, `process:default`, and `dialog:allow-open` to the WebView. The `default` permission sets are broader than the specific commands the UI actually calls. | Open, trim pending |
| App commands are not capability-gated | Tauri capabilities gate the plugin and core surface, not custom commands. All app commands are reachable from the WebView. A compromised frontend could point `editor_command` or `terminal_command` at an existing executable via `settings_set`, then trigger it. The CSP and navigation guard are backstops, not a fix. | Open (BL-NI-53) |
| Activity log is not redacted | Raw Git stdout and stderr are persisted, so if authenticated GitHub access is added later, a token could be retained. The size half of this is closed: each captured stream is capped at 16 KiB by a single shared function at the write sink, with an explicit truncation marker, so attacker-controlled Git output can no longer exhaust disk. Redaction remains open, and deliberately so: what is acceptable to retain, for how long, and what promise to make is a maintainer decision, not an inferred default. | Partly open (BL-NI-54) |
| Repository-local Git config partly trusted | `credential.helper` and `core.sshCommand` are not neutralized. | Open |
| Two high-severity Rust advisories are waived | `RUSTSEC-2026-0194` and `RUSTSEC-2026-0195`, both `quick-xml 0.39.4`, fixed in `>= 0.41.0`. They reach the graph via `quick-xml -> plist -> tauri-utils`, which is Tauri's own config and bundling tooling; RepoSync parses no XML or plist input, so neither DoS-class advisory has a reachable path, and the version is chosen by a framework we do not control. CI now fails on any advisory that is NOT explicitly waived in `.cargo/audit.toml`, and re-checks weekly so a newly published advisory against an unchanged lockfile still turns the build red. Each waiver carries a reachability argument, an owner, and a review date. | Waived, review by 2026-10-31 |
| Installers are unsigned | No Authenticode or Apple signing. Users cannot verify artifact provenance, and Windows shows an unknown-publisher warning. | Open, human-gated |

## If you are auditing this

Start at `run_git` in `crates/reposync-core/src/git/cli.rs`. It is the highest-value target in the codebase and the place where autonomous execution meets untrusted input. After that, read `src-tauri/src/commands/mod.rs` for the full reachable command surface, and `src-tauri/tauri.conf.json` for the CSP as actually shipped.

The IPC seam itself (`src/lib/bindings.ts`) is generated from the Rust types and CI-gated against drift, so it can be read as an accurate description of the surface rather than an aspirational one.
