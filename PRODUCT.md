# Product

## Users

Technically competent developers who keep a personal library of 5 to 100+ cloned
Git repositories they *consume* rather than contribute to: self-hosted tools they
run locally, reference repos read for samples, templates, and rarely-touched forks.
They are comfortable with Git on the command line but do not want to babysit
`git fetch` across dozens of folders. Their context when using RepoSync is ambient,
not focused: the app lives in the system tray, runs all day, and is glanced at
between other work. The job to be done is awareness, "what in my library is stale,
what changed, what broke", answered at a glance and acted on in one click, without
thinking about Git plumbing.

This is explicitly the consumer-repo user, not the active-development user. RepoSync
is a read-and-refresh tool for repos you are *not* working in daily, never a Git
client for repos you are.

## Product Purpose

RepoSync makes the silent staleness of a cloned-repo library visible, and keeps that
library fresh safely, on a schedule, with a transparent audit trail. It is a resident
desktop tray utility (Tauri v2 + Rust core + React/TypeScript shell, SQLite-backed,
local-first) with a richer main window for management and review: Dashboard, Repos
(list + detail), Activity, Summaries, Settings, plus a compact tray popup.

Success looks like a user who adds 30 repos, sees each repo's state at a glance
(clean/dirty, ahead/behind, last fetch, new release, error), trusts that nothing was
done to their working tree that they did not ask for, and can audit exactly what ran.
The product wins by being read-mostly, predictable, and honest: fast-forward-only by
default, dirty repos skipped with a stated reason, every git invocation logged with
its raw command and output. It is open source (MIT), with no telemetry, no account,
and no cloud sync.

## Brand Personality

**Confident and precise.** RepoSync touches the user's repositories on a schedule, so
it has to read as something that knows exactly what it is doing and shows its work. The
voice is engineer-grade and exact: real state, real numbers, real command output, no
hand-waving. It earns trust through transparency and accuracy rather than reassurance
or polish.

The surface is quiet in *footprint* (it idles in the tray, it does not nag) but dense
and exact in *content* (when you look, it tells you everything that matters, precisely).
Confident is not loud; it is the calm of a tool that is sure of its facts. Three words:
**precise, transparent, trustworthy.**

## Anti-references

RepoSync should not look or feel like either of these (both ruled out after reviewing
mockups in `_local/gui/anti-refs.html`):

- **The generic AI dashboard.** Gradient hero text, glassy stat cards with oversized
  hero numbers, an uppercase tracked eyebrow over every section, an identical
  icon-card grid. It looks produced but buries the one thing that matters (which repos
  need attention) under decoration. RepoSync should read as intentional, not templated.
- **The heavy pro-git client.** Commit-graph DAGs, branch/tag/stash trees, an inspector
  pane, a toolbar of a dozen git verbs (GitKraken, Tower, SourceTree). That machinery is
  for repos you actively develop; it is the opposite of a consume-only, glance-and-go
  library. RepoSync is not a Git client and should not cosplay as one.

These fail in opposite directions, "too much decoration, too little signal" versus "too
much git machinery", which together fence the design in from both sides.

Product anti-positioning (from the strategy doc) reinforces this: not a Git client for
active development, not a CI/deployment tool, not an IDE workspace manager, not a process
manager, not multi-user/team-shared in V1.

## Design Principles

1. **State obvious at a glance.** The list view must let a user instantly read each
   repo's clean/dirty, ahead/behind, last checked, and error state without drilling in.
   If a state matters, it is legible in the row, not hidden one click away.
2. **Transparency is the trust mechanism.** The product earns trust by showing exactly
   what it did, raw command, exit code, stdout/stderr, timestamp. Never imply an action
   without the receipt available. Prefer showing the real thing over summarizing it away.
3. **Never hide risk behind vague language.** Risky Git behavior must look risky in the
   UI. Safe defaults are presented plainly; anything that could surprise the working tree
   is labeled clearly and made harder to reach than the safe path.
4. **Confidence through precision, not decoration.** Density and accuracy beat flourish.
   Every pixel earns its place by carrying information; no gradient, hero metric, or
   decorative card stands in for substance. Quiet in footprint, exact in content.
5. **Every automation has a manual equivalent and an opt-out.** Scheduled behavior is a
   convenience, not a cage. Anything the scheduler does, the user can trigger, pause, or
   disable by hand, and a repo's settings survive being disabled.

## Accessibility & Inclusion

- **Target: WCAG 2.1 AA**, with colorblind-safe status as a hard requirement specific to
  this product. Because the entire value proposition is "repo state at a glance" carried
  largely by color (clean/dirty, ahead/behind, error), **status must never be encoded by
  hue alone**: every state pairs color with an icon and a text label, so it survives
  grayscale, color blindness, and low-contrast displays.
- **Contrast.** Body text meets at least 4.5:1; large text at least 3:1. No muted-gray
  body text on tinted near-white surfaces.
- **Reduced motion.** Every animation (status transitions, scheduler/sync indicators,
  reveals) has a `prefers-reduced-motion: reduce` alternative, typically a crossfade or
  an instant change. Sync/activity indicators must not rely on motion to convey state.
- **Keyboard.** The main window is fully operable by keyboard; primary actions (check
  now, open folder/terminal/editor/remote, enable/disable) are reachable without a mouse.
