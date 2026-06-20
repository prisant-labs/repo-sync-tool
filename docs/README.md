# RepoSync documentation

RepoSync is a cross-platform (Windows-first) desktop tray utility that keeps a
personal library of cloned-but-not-actively-developed Git repositories fresh and
visible. It is an open-source (MIT) personal utility: local-first, no telemetry,
no required account.

This folder holds the public, contributor-facing documentation. The docs are
**living**: each is accurate as of the current build and grows as efforts land.
Start here, then follow the links into whichever doc answers your question.

## The living docs

| Doc | What it is | Read it when |
|---|---|---|
| [faq.md](faq.md) | Plain-language answers about what RepoSync does today, in V1 scope. | You are a user or just curious about behavior, safety, platforms, and data. |
| [architecture.md](architecture.md) | The system and its parts: component map, the IPC seam, persistence, tech stack, CI gates. | You want to know *what* the pieces are and how they fit. |
| [explanation.md](explanation.md) | The design rationale: why RepoSync is shaped the way it is. | You want to know *why* a decision was made before you change it. |
| [backlog.md](backlog.md) | Deferred work, V1.1 cut features, human-only decisions, open technical questions, and review findings. | You have a new idea, or you are looking for what is deliberately out of scope. |

## Source-of-truth docs (deeper, internal)

The living docs above are seeds that link into these for full depth. Treat these
as authoritative when the two disagree, and update the living doc to match.

- [internal/v1-architecture-and-decisions.md](internal/v1-architecture-and-decisions.md) - the deep architecture and decisions brief.
- [internal/strategy-and-roadmap.md](internal/strategy-and-roadmap.md) - the original plan; Section 4.2 is the authoritative database schema.
- [../AGENTS/efforts/README.md](../AGENTS/efforts/README.md) - the V1 execution plan: ratified decisions, scope ledger (MUST / SHOULD / CUT), the effort index E-01..E-12, the dependency graph, sequencing, and descope triggers. This is the single source of truth for build status.
- [../EXECUTION.md](../EXECUTION.md) - the autonomy and governance contract: the agent/human boundary and the CI gates that must be green before any merge.

## Living-docs and review cadence

These docs only stay useful if they move with the code. The process is short and
non-negotiable:

1. **Update docs in the same change.** Every effort that changes architecture,
   the IPC contract, scope, or user-facing behavior updates the relevant living
   doc(s) in the *same* change. A doc that lags the code is treated as a bug, not
   a follow-up.
2. **Capture everything else in the backlog.** New ideas, deferred work, and
   review findings land in [backlog.md](backlog.md). When a backlog item moves
   into a build slice, give it an effort and remove it from the backlog.
3. **Adversarial review per substantive effort.** Each substantive effort gets an
   adversarial review pass: a Claude self-review plus a Codex second-opinion
   review. Confirmed findings are fixed in place; deferred findings go to
   [backlog.md](backlog.md) with a status.

The rule of thumb: if a reader of these docs would be surprised by the running
code, a living doc is out of date and fixing it is part of the work, not extra.
