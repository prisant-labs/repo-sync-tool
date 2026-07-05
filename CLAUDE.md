# CLAUDE.md - RepoSync

`AGENTS.md` at the repo root is authoritative: project map, build/verify commands, hard
conventions, the shell-crate chokepoint rule, doc-update cadence, and canon pointers. Read it
first. This file only adds what is Claude-specific and does not repeat AGENTS.md content.

## Claude-specific notes

- **Session logs** go in `_LOCAL/session-logs/` (gitignored), not in the tracked doc tree. Use
  the `jp-wrap-session` skill to close a session; `jp-continue-session` to resume one.
- **Adversarial review:** each substantive effort gets a Claude self-review plus a Codex
  second-opinion review, per the cadence in `docs/README.md`. Confirmed findings are fixed in
  place; deferred findings go to `docs/backlog.md` with a status.
- **Design work** builds toward `DESIGN.md`'s Graphite/oklch system, with `src/index.css` as the
  token source of truth. Superseded draft mockups (the earlier "Instrument + Signal" hex-token
  direction) are archived under `_local/gui/archived-mockups/`, not deleted; do not resurrect
  them as current guidance.
- **Subagent delegation** (when orchestrating multi-agent work): Fable/Opus plan and make crux
  decisions (scheduler/locking/migrations, security-sensitive code, CI redesign); Sonnet does
  standard feature and doc work; Haiku runs mechanical gate sweeps and doc syncs. Serialize any
  subagent work that touches `src-tauri/**` (the shell-crate chokepoint in AGENTS.md);
  `reposync-core`-only and frontend-only subagents can run in parallel.
