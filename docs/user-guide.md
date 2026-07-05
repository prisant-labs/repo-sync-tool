# RepoSync User Guide

This is the complete guide to using RepoSync: what it is, how to install it, and
what every feature does and why it exists. It is written for the person it is
built for: a developer who keeps a library of Git repositories they *use* but do
not actively develop, tools, dotfiles, reference repos, forks they cloned once
and mostly forgot about.

If you already know the mental model and just want the settings list, jump to
[Settings reference](#12-settings-reference). Everything else teaches the "why"
so the buttons make sense the first time you see them.

---

## 1. What RepoSync is, and the problem it solves

Say you have 40 folders of cloned repositories on disk: a handful of CLI tools
you run locally, a pile of reference implementations you read but never edit, a
few forks you keep around "just in case," your dotfiles repo, a couple of
libraries you vendor by hand. None of these are things you are actively
developing. You are not going to open a pull request against them this week.
But you do care whether your copy is still current, and whether the upstream
project is even still alive.

That is a different question from the one every Git client answers. A Git
client (GitKraken, Tower, SourceTree, the CLI itself) is built around the
question "what do I want to do to this repo right now": stage, commit, branch,
rebase, push. RepoSync is built around a quieter, more ambient question that
nothing else answers well:

> **Is my copy current, and is the project still active?**

"Is my copy current" is a freshness question: has the remote moved since you
last looked, and if so, can you catch up safely. "Is the project still active"
is a different question again: has anything happened upstream at all, recently,
that you'd want to know about, a new release, a burst of open pull requests, a
project that has clearly gone quiet.

A cloned-repo library goes stale silently. Nothing tells you that a tool you
depend on shipped a new version, that a reference repo moved on without you, or
that a manual `git pull` three months ago left a working tree dirty and
un-fast-forwardable ever since. Keeping current the manual way means
remembering to `git fetch` across dozens of folders, and doing that in bulk
risks clobbering local changes you forgot you made.

RepoSync is a resident desktop tray utility that makes that staleness visible
and keeps your library fresh safely, on a schedule, with a receipt for
everything it did. It lives in your system tray, checks your repos on its own
cadence, and gives you a real window (Dashboard, Repos, Activity, Settings)
when you want to look closer. It is deliberately **not** a Git client: no
commit graphs, no branch trees, no staging area. If you find yourself wanting
to commit, branch, or rebase in a repo, that repo has graduated out of
RepoSync's job and into your regular Git workflow.

Three things it is built to never do, because they would betray the "consume,
don't develop" mental model:

- It never mutates your working tree unless the update can happen with a clean,
  lossless fast-forward.
- It never silently pulls over local changes, it skips a dirty repo and tells
  you why.
- It never hides risk behind vague language, anything that could surprise your
  working tree is labeled clearly and harder to reach than the safe path.

## 2. Install and first run

**Platform status for this release.** RepoSync v0.9.0 is Windows-first: the
Windows build is the fully supported target, packaged as an installer (both MSI
and NSIS installer artifacts come out of the build; the auto-updater path in
particular is built around the NSIS installer, see
[section 11](#11-auto-update-honest-about-where-it-stands)). macOS is kept
**compiling and bundling in CI** so the codebase does not rot on that platform,
but it is not distributed as a signed, ready-to-run build in v0.9.0. If you are
on macOS today, you can build from source, but there is no packaged download
for you yet.

**This is a private, unsigned build.** RepoSync is an open-source (MIT)
personal utility, not a commercial product, and this release ships privately
before a later public flip. Two practical consequences:

- The Windows installer is **not code-signed**. Windows SmartScreen will warn
  "Windows protected your PC" / "unknown publisher" the first time you run it.
  This is expected and documented, not a sign anything is wrong; you'll need to
  click "More info" then "Run anyway." Code signing is planned as a fast-follow,
  not shipped yet.
- Because the repository is private while it stays this way, the in-app
  auto-updater cannot yet reach its update server (more in
  [section 11](#11-auto-update-honest-about-where-it-stands)). It is fully
  built and will start working the moment the project goes public, with no
  reinstall required.

**First run.** The first time you launch RepoSync, your library is empty and
the Dashboard and Repos screens both show a plain "No repositories yet" empty
state with an **Add repositories** button. There are two ways to add repos, both
reachable from that same button (also available as "Add repos" in the
top-right of Dashboard and Repos at any time):

- **Add a single path.** Paste or type the folder path of one repository you
  already have cloned and click "Add this path." RepoSync verifies it is a real
  Git working tree and starts tracking it immediately.
- **Scan a folder.** Point RepoSync at a parent folder (say, the folder that
  holds all forty of your tool clones) and click "Scan." It walks the folder
  tree (bounded, so it won't run away on a huge disk) and shows you every Git
  repository it found, each with a checkbox. Anything already tracked is shown
  greyed out and marked "tracked" so you don't double-add it. Check the ones
  you want and click "Add N selected."

As soon as repos are added, the Dashboard fills in: a row of stat tiles (repos
under watch, needing attention, updated today, new releases) and a "Needs
attention" list. Nothing is checked instantly on add, RepoSync's scheduler picks
each new repo up on its normal cadence, but you can always trigger a check by
hand (see the next section) rather than wait.

## 3. The core loop: check vs. update, and why fast-forward-only is the safety guarantee

Everything RepoSync does to a repo falls into one of two operations, and the
distinction between them is the single most important thing to understand
about how the tool stays safe to run unattended.

**Check is read-only.** A check runs `git fetch` (asking the remote "what have
you got that I don't") and compares your local branch against its upstream.
That's it. A check can tell you that you're 14 commits behind, or that your
tree has uncommitted changes, or that a fetch failed, but it **never touches
your working tree.** You can let RepoSync check every repo in your library on
autopilot forever and never worry that it silently changed a file you were
mid-edit on. This is why checking is the thing the scheduler does constantly
and safely in the background.

**Update is fast-forward-only, and that restriction is the safety guarantee.**
When RepoSync updates a repo, it runs the equivalent of `git pull --ff-only`.
A fast-forward is the one kind of pull that is provably lossless: it only
happens when your local branch and the remote's history do not disagree,
your branch is simply a strict prefix of a longer history, so "catching up"
means moving your branch pointer forward with nothing to merge and nothing to
lose. If a true merge would be required, RepoSync's update refuses outright
rather than guess how to reconcile the two histories for you. That refusal is
the point: **the default update path is architecturally incapable of
rewriting your history or discarding your commits**, because a fast-forward is
the only move where doing that is impossible. In v0.9.0, merge and rebase pull
modes exist in the interface as visibly disabled option cards labeled "Not
available in this release", they are not silently missing, you can see exactly
what riskier options would exist and that they are deliberately withheld.

**Dirty repos are skipped, and told why.** If a repo has uncommitted local
changes, RepoSync will not pull over them, full stop. The repo shows as
**Dirty** with an explanation ("uncommitted changes were found"), and the fix
is yours to make: commit, stash, or discard the changes yourself, then check
again. RepoSync will never guess whether your uncommitted edit was intentional.

Put together: **checking is unconditionally safe** (it never writes to your
tree), and **updating is safe by construction** (it only ever fast-forwards, and
refuses anything else, and never touches a dirty tree at all). Every automation
RepoSync runs on a schedule has this same manual equivalent available from the
repo detail view, and every automated repo can be paused or disabled by hand at
any time, its settings survive being turned off.

## 4. Reading the dashboard: the status taxonomy

Repo state in RepoSync is always shown as **color plus icon plus word**, never
color alone, so it reads the same in grayscale, for colorblind users, and at a
glance. There are six states a repo can be in:

| State | What it means | What to do |
|---|---|---|
| **In sync** (green, check mark) | Your local branch matches its upstream exactly. Nothing to do. | Nothing. This is the common, quiet case. |
| **Ahead** (green, up arrow, "N ahead") | You have local commits the remote does not have yet. | Nothing from RepoSync's side, it never pushes for you; push it yourself when you're ready. |
| **Behind** (violet, down arrow, "N behind") | The remote has commits you don't. | If the tree is clean, fast-forward it (one click from the repo's detail panel, or wait for the scheduler). |
| **Dirty** (amber, warning triangle) | The working tree has uncommitted local changes; any pending pull was skipped. | Commit, stash, or discard the changes yourself, then check again. |
| **Failed** (red, X circle) | The last check or update hit an error, an auth failure, a missing path, a deleted upstream branch, or (see below) a history that can't fast-forward. | Open the repo's detail panel; it shows the exact error code and a specific remediation instead of a generic failure. |
| **Paused** (neutral grey, pause icon) | Scheduled checks are off for this repo, either you turned it off, or RepoSync auto-paused it after three consecutive failures. | Resume it from the detail panel once whatever was wrong is fixed. Paused is a first-class state, not a greyed-out row, and the repo's settings are preserved while paused. |

Priority when more than one thing is true (for example a repo that is both
dirty and behind): **paused beats failed beats dirty beats behind beats ahead
beats in sync.** A dirty-and-behind repo shows as Dirty, because the thing you
need to act on first is the uncommitted change, not the fast-forward that is
waiting behind it.

**A history that has truly diverged** (both your branch and the remote have
commits the other lacks, so a fast-forward is mathematically impossible) does
not get its own color. It surfaces as **Failed**, carrying the specific message
"the branch has diverged and cannot fast-forward", because from RepoSync's
read-mostly point of view a diverged history is a stop-and-look-at-it
condition exactly like any other error it won't silently paper over. You
resolve it yourself in your normal Git tooling; RepoSync is never going to
attempt a merge on your behalf.

**The Repos list** shows every tracked repo as a row: identity (name, host,
and any release/PR signal, see the next section), its status badge, a **lag
signal** bar, a "last checked" time, and a one-click "check now" button. The
lag signal is the one thing a status word by itself can't say: an empty bar
reads as current, and it fills further the more commits behind a repo is (up to
a visual cap), so at a glance you can tell "a little behind" from "wildly
behind" without reading the exact number. Status filter chips above the list
(All, Behind, Dirty, Failed, Paused, Ahead, In sync) let you narrow a big
library down to just what needs attention, and a name filter box narrows
further by typing.

**The Dashboard's "Needs attention" list** is a rolled-up view of exactly the
repos that need you: anything dirty, failed, or behind. Each item shows the
repo's true status color and icon (not a single blanket warning glyph) plus a
short detail line, and clicking it opens that repo's detail drawer directly.
When nothing needs attention, the Dashboard shows a calm "All clear" message
instead of an empty list, so a quiet library reads as intentional, not broken.

## 5. Branch and PR intelligence: how alive is the upstream project?

Freshness (in sync / behind / ahead) answers "is my copy current." RepoSync
also answers the second question from section 1, "is the project still
active," with a small set of signals that ride alongside status but are never
confused with it:

- **Open pull-request count**, and how many of those target the project's
  default branch. A tool with 40 open PRs against `main` is a very different
  kind of "active" than one with none.
- **Latest release**, the tag and how long ago it shipped.
- **Last local commit recency**, how long ago the commit your checkout is
  sitting on was actually made, which is a different fact from "when did
  RepoSync last look" (`last checked`).

You'll see these as small chips next to a repo's name in the Repos list (a
package icon with the release tag, a pull-request icon with the count) and as
a dedicated "Branch & PR intelligence" block in the repo's detail drawer,
alongside the ahead/behind counts already described above.

**This is rendered in a distinct visual register, on purpose.** DESIGN.md calls
the six status colors above the "status taxonomy," and reserves them
exclusively for repo freshness. Release and PR information is rendered in a
separate **magenta signal color** that never appears in the status taxonomy,
specifically so a "new release" badge can never be mistaken for a sync-status
color. A repo can be quietly "in sync" (green) and still be flagged with an
active magenta release badge, those are two independent facts, not competing
answers to the same question.

**It's unauthenticated, and RepoSync budgets its GitHub calls on purpose.**
This intelligence comes from GitHub's public API without you signing in or
providing a token, GitHub allows 60 unauthenticated requests per hour per
caller. RepoSync self-imposes a tighter cap, at most 30 requests per rolling
hour, and spends that budget oldest-data-first across your whole library.
That means a big library (100+ repos) doesn't hit a wall: on first sync it
simply takes RepoSync several hours to work through everyone once, spreading
the load by design rather than bursting through the ceiling and getting
rate-limited. Once each repo has a fresh copy of its PR/release data, ordinary
refreshes are cheap (a conditional request that costs nothing if nothing
changed), so steady-state usage stays low.

**Being rate-limited or offline never shows a fake zero.** If GitHub can't be
reached, or the budget is temporarily spent, or a repo's remote isn't even on
GitHub, RepoSync keeps showing you the last values it actually knew, stamped
with an honest "as of `<time>`" so you can see how stale that number is. A
private or currently-inaccessible GitHub repo shows "not yet checked" for its
PR count, never a fabricated "0 open PRs", because "we don't know" and "there
are genuinely none" are different facts and RepoSync refuses to conflate them.
A repo whose remote isn't GitHub at all (a self-hosted GitLab instance, say)
simply shows PR/release data as unavailable; its local intelligence
(ahead/behind, dirty, recency) still works fully, since that never depended on
GitHub in the first place.

## 6. Groups and filtering: organizing a large library

Say you track 40 tools and want to tell "work-relevant CLI tools" apart from
"dotfiles" apart from "stuff I'm just curious about." **Groups** are
user-defined, colored labels you attach to repos, many-to-many, a repo can
belong to as many groups as make sense, and a group can hold as many repos as
you like.

- **Create a group** from the "+" next to "Groups" in the left sidebar: give
  it a name and pick one of eight preset colors.
- **Assign repos to a group** from that repo's detail drawer (open any repo,
  scroll to the "Groups" section, and flip the switch next to each group you
  want it in). There's no bulk-assign in this release, you assign per repo,
  from the drawer.
- **Rename or delete a group** by hovering its row in the sidebar; a delete
  asks for confirmation inline before it takes effect. Deleting a group only
  removes the label, it never removes or affects the repos that were in it.
- **Filter by group** by clicking a group in the sidebar. That switches you to
  the Repos view with a "Filtered to `<group name>`" banner and a repo count;
  "Clear filter" (or clicking "All repositories") returns you to the full list.
  Group filtering combines with the status chips and the name search box
  described in section 4, so you can, for example, look at just the Dirty
  repos inside your "Work tools" group.

## 7. Staying current automatically: cadence and scheduling

RepoSync checks your repos on a schedule so you don't have to remember to. Two
settings control the cadence, and they're designed so the common case (one
global rhythm for everything) needs zero per-repo configuration, while any
repo that genuinely needs a different rhythm can have one.

- **Global cadence** (Settings > Schedule > "Global cadence") is the number of
  minutes between automatic checks, applied to every repo that hasn't been
  given its own override. Out of the box this is every 360 minutes (6 hours).
  Change it and every inheriting repo re-cadences immediately, you don't have
  to wait out the old schedule or restart the app.
- **Per-repo cadence override** lives in each repo's detail drawer, under
  "Check cadence." It's presented as two always-visible option cards rather
  than a bare number field: **"Inherit global"** (the sentinel value `0`, this
  repo just follows whatever the global setting is set to) or **"Custom
  interval"** (a positive number of minutes that overrides the global setting
  for this one repo only). The drawer always shows the effective cadence in
  plain language ("every 360 min") so you never have to do the arithmetic
  yourself. A repo you care about watching closely can check every 15 minutes
  while the rest of your library checks every 6 hours, with no other setting
  affected.

**Quiet hours** (Settings > Schedule) let you suppress scheduled activity
during a daily window, for example overnight. Scheduled checks still happen
during quiet hours in the sense that the underlying work still runs, quiet
hours specifically affect notifications (section 9), not the checks
themselves.

**The app must be running for scheduled checks to happen.** RepoSync has no
OS-level background service in this release, so if the app (or its tray icon)
isn't running, nothing checks itself. This is a deliberate, stated tradeoff:
the tray icon plus [Launch on login](#10-launch-on-login) (section 10) are how
you make sure it's always there rather than something to work around.

**How results surface.** You never have to go looking for what the scheduler
found: the Dashboard's stat tiles and "Needs attention" list update live, the
Activity screen logs every check and update it ran, and (unless you're in
quiet hours) new releases and failures raise a desktop notification, all
covered in the next few sections.

## 8. Living in the tray

RepoSync is meant to run continuously in the background, so it treats the
system tray as its home, not the main window.

- **Closing the main window doesn't quit the app.** The close button hides the
  window; RepoSync keeps running and keeps checking in the tray. **Quit** (from
  the tray menu) is the only thing that actually exits.
- **Left-click the tray icon** to show and focus the main window.
- **Right-click the tray icon** for the full native menu:
  - **Show RepoSync**, show and focus the main window.
  - **Check All Now**, trigger an immediate check of every enabled repo, right
    from the tray, without opening the window.
  - **Pause all / Resume all**, a global toggle for scheduled checking; the
    menu item's own label reflects whichever state you're currently in.
  - **Open recent**, a submenu of your most recently active repos, each item
    opens that repo's folder directly. (Note: this submenu is populated once
    when the app launches, so a repo that becomes "recent" after launch won't
    appear there until you restart, a small known limitation in this release.)
  - **Settings**, opens the main window directly on the Settings screen.
  - **Quit**, a clean shutdown, the only way to fully exit.
- **If you've enabled Launch on login** (section 10), that autostart launch
  opens hidden, straight to the tray, no window pops up in your face just
  because the machine booted.

## 9. Notifications: ambient awareness without nagging

RepoSync can raise a native OS toast for the two events actually worth
interrupting you for: **a new release appeared**, or **a check or update
failed**. Both are independent toggles in Settings > Notifications
("Notify on new release" and "Notify on failure"), both on by default.

**Coalescing keeps a big sweep from becoming a wall of toasts.** If a scheduled
cycle touches many repos at once, say ten repos updated and one failed in the
same pass, RepoSync raises a small, bounded number of notifications for that
cycle rather than one per repo. You get told what happened without being
buried in it.

**Quiet hours suppress the toast, never the work.** If you've configured a
quiet-hours window (section 7), no notification pops up during it, but checks
and updates still run on schedule underneath; nothing is silently skipped, only
the interruption is withheld. Anything that happened during quiet hours is
still fully visible the next time you open the app, in the Dashboard and the
Activity log.

## 10. Launch on login

**Launch on login** (Settings > System) makes RepoSync start automatically
when you sign in to Windows, so a resident tray utility is actually resident
rather than something you have to remember to open every morning. It is
**off by default**, opt-in, and per-user (no elevation, no admin install
required, it only affects your own login).

Toggle it on or off from Settings; RepoSync also double-checks on every launch
that the OS's actual autostart registration still matches your saved setting,
and quietly corrects it if it's drifted (for instance if a prior registration
attempt didn't fully take). As noted in section 8, an autostart-triggered
launch starts hidden in the tray rather than popping a window open the moment
you log in.

## 11. Auto-update: honest about where it stands

RepoSync includes a full in-app updater, and it's worth being straightforward
about exactly what it does and doesn't do in this release.

**What's built and working:** RepoSync can check for a new version on launch
(gated by a Settings toggle, on by default) and via a manual "Check for
updates" button in Settings > Updates at any time. If an update is found, you
see the new version number and release notes and choose to install, **nothing
ever installs silently or without your confirmation.** When you do confirm,
the downloaded update is verified against a cryptographic signature (minisign)
embedded in the app before it's applied; if that signature doesn't check out,
the install aborts and your current version is left untouched, full stop. A
manifest that offers an equal-or-older version is treated as "up to date," so
a stale or rolled-back release listing can't push you backward. None of this
touches telemetry or an account, the update channel is a public GitHub
Release manifest, checked anonymously.

**What's honestly not live yet:** this v0.9.0 build ships with the updater
**disabled in practice**, "dark" is the internal term for it, because two
things haven't happened yet: the repository is still private (so the update
manifest can't actually be downloaded even by an unauthenticated request), and
the production cryptographic signing key that would authorize real update
artifacts hasn't been generated and installed yet, that step is deliberately
a human-only action, not something automated. Until both of those land,
"Check for updates" will honestly report **"Could not reach the update
server"**, phrased gently rather than as an alarming error, because from the
app's point of view that's exactly what's happening: the endpoint isn't
reachable yet.

The mechanism itself is fully built and end-to-end tested against a local
test channel, so once the project goes public and the signing key is in
place, updating will simply start working for existing installs with no
reinstall and no code change required on your end.

## 12. Settings reference

All settings live on the Settings screen, grouped into cards. Changes are
staged as a draft, "Unsaved changes" appears at the bottom until you click
**Save changes** (or **Reset** to discard the draft back to what's saved).

**Schedule**
- *Global cadence*: minutes between automatic checks for any repo that hasn't
  been given its own override (see section 7).
- *Quiet hours*: a toggle plus a start/end time (your local clock) during
  which notifications are withheld (see sections 7 and 9).

**Notifications**
- *Notify on new release*: raise a toast when an upstream release appears.
- *Notify on failure*: raise a toast when a check or update fails.

**System**
- *Launch on login*: see section 10.
- *Activity retention*: how many days of Activity-log history to keep before
  RepoSync automatically prunes older rows (default 90 days).
- *Git executable*: leave blank to use whatever `git` is on your PATH, or set
  an explicit path if you want RepoSync to use a specific install. See
  [Troubleshooting](#15-troubleshooting) for what happens when git can't be
  found at all.
- *Editor command*: the command RepoSync runs for "Open in editor" (see
  section 13).
- *Terminal command*: the command RepoSync runs for "Open in terminal."

**Updates** (see section 11)
- *RepoSync version*: the version you're currently running, read-only.
- *Check for updates on launch*: on by default; controls only the automatic
  on-launch check, the manual "Check for updates" button always works
  regardless of this toggle.
- *Check for updates* button: runs the check immediately and shows the result
  inline.

**Integrations**
- *GitHub token*: shows whether a GitHub personal access token is present,
  read-only in this screen. In v0.9.0, RepoSync's GitHub access is always
  unauthenticated (see section 5); authenticated access via a token, which
  would lift the request budget considerably, is prepared as a seam in the
  code but is not a feature you can turn on yet in this release.

## 13. Opening repos in your tools

Every repo's detail drawer has an "Open in" row with one-click actions that
take you from "RepoSync noticed something" straight into your normal
workflow, no need to go find the folder yourself:

- **Folder**, opens the repo's local path in File Explorer.
- **Terminal**, opens a terminal in the repo's folder, using whatever you've
  set as your Terminal command in Settings (or a sensible default if you
  haven't). Windows Terminal is detected and used with the right working
  directory when it's installed.
- **Editor**, opens the repo's folder in whatever you've set as your Editor
  command in Settings (defaults to a command like `code` for VS Code if left
  blank).
- **Remote**, opens the repo's GitHub remote in your browser. This button only
  appears when the repo actually has a recognized remote URL to open.

These went through a hardening pass after their initial build (path handling
on Windows, validating remote URLs before anything is opened rather than
executing them blindly, and safer process launching for the editor and
terminal commands), so what you have in this release is the corrected
version, not the original rough cut.

## 14. Privacy and data

RepoSync is an open-source (MIT-licensed) personal utility, not a commercial
product, and its data posture follows from that directly:

- **No telemetry, no crash reporting, no account, no cloud sync.** Nothing
  about your usage or your repo list ever leaves your machine as analytics.
  This isn't a "not yet implemented" gap, it's a deliberate, permanent default.
- **GitHub access is unauthenticated.** RepoSync's release/PR intelligence
  (section 5) talks to GitHub's public API without you signing in or handing
  over credentials; see section 5 for the rate-budget mechanics that make this
  workable for a large library.
- **Everything lives locally in a SQLite database** in your Windows app-data
  folder. There's no server component and nothing to configure for storage.
- **If your app-data folder is itself inside a synced folder (like OneDrive),**
  RepoSync warns you: a cloud-sync agent rewriting the database's WAL files
  mid-write is a real corruption hazard, and the app flags this rather than
  silently risking it.
- If a startup database migration ever fails, RepoSync doesn't lose your data
  silently: it moves the old database aside, starts a fresh one, and shows a
  dismissible banner in the main window telling you exactly where the previous
  database was preserved so you (or a future recovery) can find it.

## 15. Troubleshooting

**"Git was not found" / a repo shows Failed with a git-related error.**
RepoSync looks for `git` in this order: an explicit path you've set in
Settings, then your PATH, then a list of well-known Windows install locations
(Program Files, the winget and Scoop shim folders). If none of those resolve
to a working `git`, the app still launches normally, you can browse your
existing repo list, but checks and updates on any repo will fail with the
message **"Git was not found. Install Git for Windows or set the path in
Settings."** Installing Git for Windows (or pointing the Git executable
setting at wherever it actually lives) and running a check again resolves it.
A git that's present but older than version 2.30 produces a similar clear,
non-blocking warning rather than a confusing failure.

**A repo's release/PR numbers look stale, or say "as of some time ago."**
That's by design, not a bug, see section 5: RepoSync is honoring its own
GitHub rate budget or riding out a temporary network problem, and it always
shows you the last real values it had along with how old they are, rather
than either erroring loudly or guessing a fake current number.

**A repo won't fast-forward, and there's no button to make it.** If a repo's
local branch and its remote have each gained commits the other doesn't have
(a true divergence), a fast-forward is mathematically impossible, there's no
safe automatic move for RepoSync to make. It shows as **Failed** with "the
branch has diverged and cannot fast-forward" (section 4). This is intentional:
resolving a real divergence means deciding how to reconcile two histories,
which is exactly the kind of judgment call RepoSync leaves to you and your
normal Git tooling, not something it will guess at on your behalf.

**A database recovery banner appeared.** See section 14, this means a startup
migration failed and RepoSync started fresh rather than risk your data; the
banner tells you exactly where your previous database was preserved. It's
dismissible once you've noted where the backup lives.

**The database says it's locked / busy.** This usually means another
RepoSync process (or something else holding the same SQLite file, such as a
sync client mid-write) has it open. Close any other running copy of RepoSync
and retry.

## 16. Keyboard and accessibility

RepoSync's main window is built to be fully usable without a mouse, not as an
afterthought:

- Repo rows in the Repos list and items in the Dashboard's "Needs attention"
  list are real focusable controls: Tab to them, then **Enter** or **Space**
  opens that repo's detail drawer.
- The detail drawer and every dialog (Add repositories, group create/rename)
  trap focus while open and close on **Escape**, and return focus to whatever
  you had focused before you opened them.
- Every primary action (check now, the "Open in" buttons, enable/disable,
  fast-forward, retry) is a normal button reachable by keyboard, not a
  mouse-only affordance.
- Status is never carried by color alone anywhere in the app: every state is
  color plus a distinct icon plus a text word, so the app is usable in
  grayscale and by colorblind users without losing any information.
- Motion (the lag-signal bar filling, the drawer sliding in, a spinner during
  a check) is functional feedback only, kept minimal and never required to
  understand what happened.
