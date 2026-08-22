# RepoSync FAQ

A living answer sheet for users and the curious. It describes what RepoSync does
**today**, in its V1 scope. Features marked "later" are planned for V1.1 and are not
promised in this release. As each effort lands, this file grows.

For the deep design rationale see the architecture brief
([docs/internal/v1-architecture-and-decisions.md](internal/v1-architecture-and-decisions.md))
and the original plan ([docs/internal/strategy-and-roadmap.md](internal/strategy-and-roadmap.md)).

---

## What is RepoSync and what problem does it solve?

RepoSync is a small desktop tray utility that keeps a personal library of
cloned-but-not-actively-developed Git repositories fresh and visible. If you have
dozens of repos you cloned for reference, self-hosted apps, templates, or forks you
follow but rarely touch, they quietly go stale. RepoSync watches them on a schedule,
fetches updates safely, surfaces what changed (new commits, new releases, errors),
and keeps an activity log so you can see exactly what it did.

It is a read-and-refresh tool for repos you are *not* actively developing, not a Git
client for repos you work in daily.

## Will it ever change or break my repositories?

No. Safety is the first design rule.

- **Fast-forward only.** Updates use `git pull --ff-only` (or just `fetch`, or a
  check with no fetch, depending on the per-repo mode you choose). RepoSync never
  rebases, never merges with conflicts, and never force-updates.
- **Never destructive.** It does not reset, discard, or overwrite your working tree.
- **Dirty repos are skipped, not touched.** If a repo has uncommitted changes,
  RepoSync leaves it alone and records a "skipped: dirty" entry instead of fetching
  into a working tree you are editing.
- **Diverged repos are surfaced, not forced.** If local and upstream have diverged so
  a fast-forward is impossible, RepoSync reports the state ("behind N", drift) and
  takes no action. Resolving divergence is left to you, deliberately.
- **One git operation per repo at a time.** A per-repo lock means a scheduled check
  and a manual "Check now" can never run two `git` processes in the same working tree
  at once, which avoids index corruption.

The three update modes are `check_only` (read state, no network fetch),
`fetch_only` (fetch refs, do not move HEAD), and `pull_ff_only` (fast-forward HEAD
when it is safe). You pick the mode per repo.

## Does it need my GitHub credentials?

No. V1 talks to GitHub **unauthenticated** for optional enrichment (repo description,
default branch, latest release), using conditional ETag requests so it stays well
within GitHub's unauthenticated rate limit for a personal library checked a few times
a day. There is no login, no token, and no account linking.

For the *fetch* itself, RepoSync shells out to your system `git`, so it inherits
whatever credential helper you already use in your terminal. RepoSync does not store,
read, or manage your Git credentials.

A Personal Access Token stored in the OS keyring (to raise the GitHub rate limit) is a
**later** addition (V1.1), not part of this release. V1 stores no secrets.

## Which platforms are supported?

**Windows is the only supported platform.** It is built, tested, and dogfooded there,
and Windows is the real "done" bar.

**macOS is experimental and unsupported.** The codebase is cross-platform and the macOS
build is kept compiling and bundling in CI so it does not rot, but no one has ever run
it on real Mac hardware, and it is neither signed nor notarized, so Gatekeeper will
refuse to open it without an explicit override. There is no packaged macOS download
today. If you are on macOS you can build from source, but treat the result as
unvalidated: a macOS bug may sit unanswered for want of hardware to reproduce it.
Full support is gated on Mac hardware access and on Apple Developer Program enrollment
for notarization, neither of which exists yet. See the architecture brief for why this
split exists
([v1-architecture-and-decisions.md, Section 2](internal/v1-architecture-and-decisions.md)).

**Linux is not a target.**

## Where is my data stored?

Locally, on your machine, under the OS application-data directory. On Windows that is
`%LOCALAPPDATA%\RepoSync\` (the Local profile, never a roaming or OneDrive-synced
folder, to avoid database corruption). Two things live there:

- **A SQLite database** holding your repo list, cached git state, cached GitHub
  metadata, the activity log, and your settings.
- **A `logs/` directory** of rotating diagnostic logs, one file per day. These record
  what RepoSync did in the background and why anything failed, because a tray app has
  nowhere on screen to tell you. They contain local paths and error text, which can
  include git's own error output. Fourteen days are kept, capped at 32 MiB total,
  oldest deleted first.

Neither is sent anywhere. If you attach a log to a bug report, it is worth reading
first, since it names paths on your machine.

**Settings -> Diagnostics** shows both locations, opens the `logs/` folder for you, and
reports whether logging is actually running and under what retention. It also flags the
conditions worth knowing about: a data folder that ended up inside a OneDrive-synced
tree, a database that had to be recovered at startup, and background checks whose
results could not be saved.

There is **no telemetry, no crash reporting, and no cloud sync.** RepoSync makes no
outbound network calls beyond the Git fetches and the optional unauthenticated GitHub
metadata lookups that you configure. The activity log shows the exact commands it ran,
so nothing is hidden: select any row in **Activity** for the full receipt, including
git's own output on both streams, the exit code, and how long it took. The list can be
narrowed to checks or updates and to successes or failures, and that filtering runs over
your whole history rather than only the entries currently on screen.

## Is it safe to point at many repos?

Yes. The scheduler is built for a sizable personal library without thrashing your
disk or the network:

- **Bounded concurrency.** At most a few repos are checked at once (default 4), not
  all of them at once.
- **Jitter on startup.** Due repos are staggered by a random short delay so they do
  not all fire the instant the app launches.
- **Quiet hours.** You can define hours during which no scheduled checks run and no
  notifications are raised. Nothing is lost to the silence: a check that came due
  inside the window simply runs on the first cycle after it, and a notification that
  a cycle produced just as the window opened is held and delivered then rather than
  discarded.
- **Default cadence.** Repos are checked every 6 hours by default; you can change the
  frequency.
- **3-strikes auto-pause.** A repo that fails repeatedly is automatically paused after
  three consecutive failures, so a broken remote or expired credential does not retry
  forever or bury the rest of your library in noise. You can re-enable it once fixed.

## Why not just a cron job that runs `git pull`?

A script can fetch, but it cannot *show* you what happened or stop itself when things
go wrong. RepoSync adds the parts a one-line cron job lacks:

- **Visibility.** A list and per-repo detail of local-vs-remote state, latest release,
  and recent commits, plus a tray presence that flags when something needs attention.
- **Safety rails.** Fast-forward-only, dirty-repo skipping, per-repo locking, and the
  3-strikes auto-pause described above.
- **Honest error states.** Auth failure, missing local path, deleted upstream, git not
  installed, and similar are distinct, actionable states rather than a silent failure
  or a wall of stderr.
- **An activity log.** An append-only record of every operation, including the raw
  command, output, exit code, and duration, so you can audit exactly what ran.
- **A daily summary.** A plain-language read-out of what changed across your library.

In short, a cron job answers "did it run"; RepoSync answers "what changed, what
broke, and what did it actually do."

## Is it open source? What license?

Yes. RepoSync is open source under the **MIT License** (see
[LICENSE](../LICENSE)). It is a community contribution and a personal utility, not a
commercial product. There is no paid tier, no account, and no telemetry.

## How is it built?

RepoSync is a [Tauri v2](https://v2.tauri.app/) desktop app: a Rust backend with a
React + TypeScript frontend rendered in the OS-native WebView, with state kept in
SQLite. The design keeps all the actual logic (git engine, scheduler, database,
policy) in a Tauri-free Rust core so it stays testable and portable, with a thin Tauri
shell on top.

It is developed by a single maintainer working through AI coding agents, under an
explicit human/agent governance contract
([EXECUTION.md](../EXECUTION.md)). The full architecture is in
[docs/internal/v1-architecture-and-decisions.md](internal/v1-architecture-and-decisions.md).

---

*Have a question this does not answer? Open an issue. This FAQ grows with the project.*
