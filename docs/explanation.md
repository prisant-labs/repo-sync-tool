# Explanation: why RepoSync is shaped the way it is

This is the design-rationale narrative for contributors who want to understand
the *shape* of the system, not just its parts. It explains the "why" behind the
decisions you will run into when you read the code or the architecture. It is
deliberately discursive; for the precise structures it points at the
source-of-truth docs:

- Architecture and decisions, in depth:
  [`docs/internal/v1-architecture-and-decisions.md`](internal/v1-architecture-and-decisions.md)
- Original strategy and the authoritative schema (Section 4.2):
  [`docs/internal/strategy-and-roadmap.md`](internal/strategy-and-roadmap.md)
- The work breakdown (efforts E-01..E-12, scope ledger, dependency graph):
  [`AGENTS/efforts/README.md`](../AGENTS/efforts/README.md)
- The build-and-ship governance contract:
  [`EXECUTION.md`](../EXECUTION.md)

This is a living document. It is accurate as of the current build state and is
meant to grow as each effort lands.

## The problem: cloned repos go stale in silence

A developer accumulates dozens of cloned-but-not-actively-developed repositories:
self-hosted tools they run locally, reference repos read for samples, templates,
rarely-touched forks. They *consume* these repos rather than contribute to them.
Nothing tells them when one falls behind upstream, gets a new release, or has
drifted into a dirty or detached state. The only way to know is to `cd` into each
folder and run `git fetch`, which nobody does across 30+ directories.

RepoSync exists to make that staleness visible and to keep the library fresh
safely, on a schedule, without the user thinking about Git plumbing. That single
job (see strategy doc Section 1.3) is the lens for every decision below.

## Why a background tray utility

The job is *ambient awareness*, not a task you sit down to do. A CLI you have to
remember to run does not solve "repos going stale in silence" - it just moves the
forgetting one level up. A resident tray utility runs all day, checks on a
cadence, and surfaces what needs attention without being asked. The tray is the
product's primary surface precisely because the value is passive: you glance, you
see state, you move on.

This also sets a hard constraint: a thing that runs all day must be cheap. Idle
memory and bundle size are first-class targets (the Windows bars are under 150 MB
idle and under a 30 MB bundle), which directly drives the stack choice below.

## Why Tauri v2 + a Rust core (not Electron, not a pure CLI)

Three properties had to hold at once: a small always-on footprint, native Git
behavior, and logic that can be tested without a GUI. Tauri v2 with a Rust core
hits all three; the alternatives each give up one.

- **Versus Electron.** Electron bundles a full Chromium per app. For a utility
  that idles in the tray all day, that is the wrong footprint. Tauri renders the
  UI in the OS-provided WebView (WebView2 on Windows, WKWebView on macOS) instead
  of shipping a browser, which is what keeps the installer small and idle memory
  low (strategy doc Section 3.1). The same property is also our largest
  cross-platform rendering risk - see the Windows-first section.
- **Versus a pure CLI.** A CLI cannot be the ambient surface the job needs, and it
  cannot show repo state "at a glance." The tray *is* the point.
- **Why Rust for the core.** The work is filesystem traversal, subprocess
  execution, SQLite I/O, and timed concurrent background jobs. That is exactly the
  domain where Rust's type system catches the concurrency bugs that would
  otherwise show up as flaky scheduled-update failures. Git operations shell out
  to the system `git` binary so the app inherits the user's existing credential
  helpers (no auth flow to build); `git2` is reserved for cheap read-only
  inspection. The CLI-vs-`git2` boundary is sharp on purpose: anything that hits
  the network or mutates the working tree shells out to `git`; only cheap reads
  use `git2` (strategy doc risk 8.7).

## Why a Tauri-free core and a thin shell, enforced by a CI gate

The workspace splits into two crates: `crates/reposync-core`, which holds all the
product logic and imports no Tauri, and `src-tauri`, a thin shell of
`#[tauri::command]` wrappers, tray code, and window lifecycle. The load-bearing
rule is that **`reposync-core` must never import `tauri`, even transitively**, and
that rule is checked in CI (`cargo tree -p reposync-core` must show no `tauri`;
see [`EXECUTION.md`](../EXECUTION.md)).

A naming convention is documentation; a CI gate is a guarantee. The gate is worth
the cost because two distinct benefits ride on it:

- **Testability.** A Tauri-free core compiles and tests with plain `cargo test`,
  with no running app, no WebView, and no display server - on any OS, including
  headless CI. The scheduler, git parsing, policy engine, and migrations are all
  unit- and integration-testable in isolation. If this logic lived in `src-tauri`,
  every test would drag in the GUI host, and the most important code would become
  the hardest to test.
- **The macOS port is a thin edge, not a fork.** Because every platform
  difference is confined to a `paths` module, `tray.rs` plus per-OS assets, and
  the CI bundling config, the macOS port touches a small, enumerable set of files.
  There is no `#[cfg]` scattered through business logic; `reposync-core` is
  `#[cfg]`-free. The crate that holds the actual product behavior is byte-identical
  on both platforms. That is what makes "Windows-first, macOS later" cheap to
  finish rather than a rewrite (architecture brief Section 4.2 and 4.3).

Without the gate, a single convenient `use tauri::...` in the core would silently
re-couple the logic to the GUI host and quietly erase both benefits. The gate
turns "we agreed not to" into "the build will not let us."

## Why the typed IPC seam is frozen early

There is exactly one seam between the two halves of the system: the typed IPC
contract (effort E-06). Payload structs and the error type live in
`reposync-core::ipc` (Tauri-free); the `#[tauri::command]` signatures live in
`src-tauri`; `tauri-specta` generates the TypeScript bindings from the Rust types.
The Rust types are the single source of truth, so the contract cannot drift
between backend and frontend without the TypeScript build breaking (architecture
brief Section 4.4).

Freezing this seam early is what lets the two halves proceed independently. UI/UX
decisions govern *rendering*; they never govern what data exists or how git, the
DB, and the scheduler behave. Once the contract is frozen, the frontend can stub
against real generated types while the backend is still being built, and the
backend can be finished and tested without a single final UI decision. This is the
core reason the entire V1 backend (efforts E-02..E-12) is buildable now: it all
sits on one side of the seam, depending on the frozen contract at most, never on a
finished screen (efforts README, "The seam principle").

The biggest avoidable failure mode for a single-developer, agent-driven build is
silent contract drift: a renamed Rust field producing a runtime `undefined` the
type checker never caught. Generating the contract instead of hand-writing it
makes that class of bug a compile error. The version pin is deliberate caution:
`tauri-specta`/`specta` are release-candidate, so they are exact-pinned, with a
hand-maintained-bindings fallback documented as the contingency, not the plan
(architecture brief Section 4.4).

## Why Windows-first with a true dual-platform architecture

The end goal is genuinely dual-platform; that is not in question. The tension is
verification capacity, not ambition. The project is built by one developer working
through AI agents on a Windows-only machine. Agents can *write* macOS code freely -
Rust and TypeScript do not care which OS they compile for - but no one on the
project can *see, run, sign, or judge* a macOS build before users do.

That gap is real and specific (architecture brief Section 2): macOS code signing
and notarization require a Mac plus a paid Apple Developer account and cannot run
on Windows at all; the WKWebView engine can render the UI differently from
WebView2 and that divergence ships unobserved; the tray/menu-bar UX, the single
most important surface, behaves differently on macOS and cannot be clicked even
once. Certifying any of that locally is impossible.

So the resolution keeps macOS first-class without pretending it can be validated
today (the ratified Option 3):

- Build platform-agnostic, with every unavoidable platform difference pushed
  behind a thin, named seam.
- **Windows is the real GA bar:** launches, human-validated by the developer, and
  signed-or-documented.
- **macOS degrades to "compiles + bundles in CI"** until real Mac access exists.
  CI is the *honesty* mechanism (it stops silent bit-rot), explicitly downgraded
  from the *acceptance* mechanism. Acceptance criteria are written per-platform so
  the team never signs its name to a macOS runtime claim it cannot check.

This keeps the first ship real and on the developer's own machine, keeps macOS
alive and cheap to finish later, and stops the project from certifying things no
one can see.

## Why fast-forward-only fetches and a 3-strikes auto-pause

The first strategic principle is "default to safe behavior over clever behavior"
(strategy doc Section 2). For a tool that touches the user's repositories on a
schedule, the cardinal sin is surprising the user's working tree.

- **Fast-forward-only.** V1 ships `check_only`, `fetch_only` (the default), and
  `pull_ff_only` - and nothing that can rewrite history or auto-merge. A
  fast-forward can only advance a branch to commits it does not yet have; it
  cannot create a merge commit, cannot rebase, and refuses rather than falling
  through to a merge when it is not possible. Mutating modes (`pull_standard`,
  `pull_rebase`) are deliberately deferred so a scheduled job can never silently
  rewrite work the user has not pushed (strategy doc Section 5, risk 8.3). The
  user must explicitly upgrade a repo from `fetch_only`; dirty repos are skipped
  by default and the UI says why.
- **3-strikes auto-pause.** Failures are handled to avoid two opposite mistakes:
  hammering a broken repo, and hiding the breakage. A fast-forward that is not
  possible surfaces clearly instead of auto-merging; an auth failure pauses that
  one repo rather than retry-looping; a transient network error simply waits for
  the next cycle. After three consecutive failures, the repo auto-pauses and
  requires a manual acknowledgement (strategy doc Section 5). This stops RepoSync
  from quietly retrying a repo with revoked credentials forever while keeping the
  rest of the library running.

Both behaviors reduce to the same promise: RepoSync is read-mostly and predictable,
and it never does something risky behind vague UI language.

## Why the pure engines are built test-first

The update-policy engine (E-07) and the scheduler (E-08) are pure functions of
their inputs: policy is `(repo state, policy) -> action or skip-with-reason`, and
the scheduler's `next_check_at` math, jitter, quiet-hours, and concurrency are all
deterministic given an injected clock and randomness source. These two engines
encode the safety story from the section above, and a wrong decision here is the
kind of bug that surprises a user's repo.

Pure, deterministic, and safety-critical is exactly the profile where test-first
pays off, so it is mandatory for E-07 and E-08 (efforts README; E-07 and E-08
plans). The policy engine's decision table is written as one failing assertion per
cell (each fixture state under each mode) *before* the implementation; the
scheduler's timing and "no DB lock held across a network call" guarantees are
written as failing tests first too. Writing the test first forces the contract to
be stated precisely before any code can rationalize around it, which is the whole
point when the contract is "do not corrupt the user's working tree."

## A note on the build method

RepoSync is built by one developer driving AI coding agents, with adversarial
review folded in (this architecture itself came out of a multi-agent analysis of
the original plan). That method shapes the engineering: tight CI gates substitute
for a second pair of hands on routine changes, strong types catch what review
would, and every meaningful decision is written down in `docs/internal/` so the
project can be reconstructed from the repo alone (strategy doc risk 8.10).

The agent/human boundary is explicit rather than implicit: anything that spends
money, asserts a legal identity, publishes to the world, or cannot be cleanly
undone stays with the human; everything upstream of that line is agent-autonomous,
gated by CI ([`EXECUTION.md`](../EXECUTION.md)). The same instinct that makes the
core Tauri-free and the IPC seam generated also makes this project legible to the
next contributor, human or agent: clear seams, enforced rules, and written reasons.
