---
name: RepoSync
description: A resident desktop tray utility that keeps a personal library of consume-only Git repos fresh, visible, and safe, with a transparent audit trail.
---

<!-- Tokens below are provisional, extracted from the HTML prototypes in _LOCAL/gui (Instrument + Signal direction). They are contrast-checked but not yet the implemented source of truth. Re-run /impeccable document in scan mode once the React UI exists to confirm the real tokens and generate the .impeccable/design.json sidecar. -->

# Design System: RepoSync

## 1. Overview

**Creative North Star: "The Quiet Instrument"**

RepoSync is a precise instrument that reports true state at a glance, the way a
well-designed gauge cluster does: dense, legible, trustworthy, and undramatic. It is
**confident and precise** in voice, quiet in footprint (it lives in the tray and never
nags) but exact in content (when you look, it tells you everything that matters, with no
hand-waving).

The committed visual expression is **Instrument + Signal**. Identity comes from two
places, not from chrome: **monospace as the data plane** (repo names, branches, SHAs,
counts, status words all read as machine truth), and a **signature staleness signal** (a
per-repo bar whose fill encodes how far behind a repo is). These two carry the product's
character, so the interface can stay near-flat and quiet without ever feeling generic.

The system commits to **restraint with one accent**. Neutral surfaces carry the
interface; a single confident blue marks where you can act (primary buttons, links,
active navigation, focus); and the saturated status palette is the only color allowed to
shout, because repo state is the most important thing on the screen. Motion is
**functional feedback, not choreography**: a check resolves with a progress cue, a status
crossfade, and a short staggered sweep of the staleness bars, never an orchestrated
entrance.

This system explicitly rejects three failure modes. It is **not the generic AI
dashboard** (gradient text, glassy hero-metric cards, an uppercase eyebrow over every
section, an identical icon-card grid). It is **not the generic professional-SaaS
dashboard** either: differentiation is earned through the monospace register, status
color, and the staleness signal, **not** through a stack of drop-shadowed cards, which is
the safe default the eye reads as "every dashboard." And it is **not the heavy pro-git
client** (commit-graph DAGs, branch/tag/stash trees, dense multi-verb toolbars) built for
repos you actively develop, not a consume-only library. It also refuses the most common
density failure: **light gray small text on gray surfaces**. Density is welcome;
low-contrast text is not.

**Key Characteristics:**
- **Monospace as identity.** The data plane (names, owners, branches, SHAs, counts, status words, timestamps, column headers) is monospace, so the whole surface reads as a precise instrument, not a SaaS app.
- **A signature signal.** The staleness bar is a RepoSync-specific component that encodes lag magnitude, the one thing a status word cannot: not just "behind," but "behind, and a lot."
- **Status-forward color.** Saturated color is reserved for repo state; one blue accent is reserved for interaction. The two never cross.
- **Differentiation before decoration.** Skimmability comes from type, color, and the signal, not from resting shadow. Depth is earned (floating surfaces, one focal region), never a default coat of paint.
- **Colorblind-safe and AA throughout.** Every state is color plus shape plus word; all text meets contrast, secondary and small included.

## 2. Colors

Cool neutral surfaces, a single confident blue for interaction, and a functional status
palette that is the only saturated color allowed to draw the eye. Values below are
provisional (from the prototypes) and contrast-checked against the surface they sit on.

### Primary (interaction accent)
- **Confident Blue** `#2350c8` (tint `#e8eefc`, hairline `#cbd9f8`): primary buttons, links, active navigation, focus rings, hover affordance on destination links. Means "you can act here." **Never** used to convey repo status. ~6.9:1 on white.

### Status (functional, the only saturated color that shouts)
- **Up-to-date Green** `#12813f`: a repo that is current. Paired with a check icon and the word.
- **Behind Indigo** `#5233c4`: a repo behind upstream. Deliberately indigo, not blue, so it never reads as the interaction accent. Paired with a down-arrow icon and the word, and drives the staleness bar fill.
- **Dirty Amber** `#8a5e00`: an uncommitted or detached working tree (skipped, not touched). Paired with a warning icon and the word.
- **Error Red** `#b42318`: auth failure, missing path, deleted upstream, auto-paused after 3 strikes. Paired with an alert icon and the word.
- **Paused/Off Gray** `#5f6773`: checks disabled for this repo. A hollow circle mark, distinct from the filled squares of live states.

### Neutral
- **Ink** `#12151b`: primary text and near-black emphasis. ~16:1 on white.
- **Ink-2** `#2b313c`: strong secondary text, control labels.
- **Muted** `#545d6b`: secondary text (owners, help, timestamps). ~5.9:1. Muted means lower in hierarchy, not lower contrast.
- **Faint** `#5f6773`: tertiary text (column headers, path detail, placeholders). ~5.7:1, still clears AA. (The prototype's original `#838b96` was ~3.2:1 and is retired.)
- **Page** `#e6e9ef`: the cool recessed field the app body sits in.
- **Surface / Surface-2** `#ffffff` / `#f6f7fa`: the card/panel and the slightly stepped header/toolbar band that separates regions without a shadow.
- **Hairline** `#e0e4ea` / `#eaedf1`: 1px borders and dividers that do the separation work shadows would otherwise do.

### Named Rules
**The Status-Owns-Saturation Rule.** Saturated color belongs to repo status and nothing
else. The blue accent is for interaction only. If a status color and the accent would
collide (a "behind" blue vs the accent blue), the status shifts hue to indigo; the accent
never yields, and the two color languages never mix.

**The No-Gray-on-Gray Rule.** Text is contrast-checked against the surface it actually
sits on. Light gray small text on a gray surface is prohibited. Secondary and small text
still meet WCAG AA (4.5:1); when in doubt, push muted text toward ink, never toward the
background. Every text token above is stated with its measured ratio for this reason.

## 3. Typography

**UI / Chrome Font:** a single clean, technical sans for the human-facing chrome
(`[sans to be chosen at implementation]`).
**Data / Identity Font:** a monospace for the data plane
(`[mono to be chosen at implementation]`; the prototypes use Cascadia Code / SF Mono).

**Character:** engineer-grade and neutral. The personality comes from the two-register
split and from precision, not from font-pairing flourish.

### Hierarchy (a deliberately tight, dense ramp)
- **View title** mono ~15px, uppercase, tracked: the window/section title ("REPOS").
- **Detail title** sans ~20px: the repo name as a page heading in detail view.
- **Identity / body** mono ~13px: repo names, primary cell text.
- **Cell / branch / help** mono or sans ~12px: dense cell text, branches, inline help.
- **Meta / staleness label / path** mono ~11px: secondary metadata under the primary line.
- **Column header / field label** mono ~10px, uppercase, tracked: table headers and form labels.

The ramp is tight on purpose (a dense instrument register). Hierarchy is carried by
**register (mono vs sans), weight, color, and the staleness signal**, not by large
size jumps. Counts use **tabular numerals** so columns of numbers align.

### Named Rules
**The Two-Register Rule.** The UI has exactly two type registers. **Mono is the
data/identity plane**: repo names, owners, branches, SHAs, counts, status words,
timestamps, paths, raw commands and output, and column headers. **Sans is the
human/chrome plane**: navigation, buttons, section and page titles, help text, prose, and
error explanations. Mono says "this is machine state you can trust"; sans says "this is
the app talking to you." Never mono for prose; never sans for a SHA. When the app
offers a **choice** to the user (an update mode, a policy option), the primary label is
the human-readable sans name; the machine identifier (the config enum) may sit beside it
as a secondary mono tag, but never stands in as the only label.

## 4. Elevation & Differentiation

Near-flat by intent, never undifferentiated. The earlier failure mode was pure flatness
that gave the eye nothing to grab; the fix is **not** decorative shadow (which drifts
toward the generic professional dashboard) but differentiation through the data-design
system: the monospace register, status color, and the staleness signal do the work
elevation would otherwise do. Depth is earned, not a default coat of paint.

Three levels, used sparingly:
- **Data plane (flat).** The repo table and readouts sit flat on the surface, separated by hairlines and tonal steps. Skimmability comes from type, color, and the staleness bar.
- **Floating surfaces (real shadow).** Things that genuinely sit above the page (the tray popover, dropdowns, dialogs, a hover-lifted row) get a soft real shadow, because they *are* above.
- **Focal region (one, earned).** On a screen with a single most-important action (a "behind, fast-forward available" panel), that one region may take a slightly stronger surface and a status tint so the eye lands on it first. One per screen, at most.

### Named Rules
**The Differentiation-Before-Decoration Rule.** When a layout reads flat and hard to
skim, the first move is more type, color, or signal contrast, not a drop shadow. Shadow
is reserved for genuinely floating surfaces and the single focal region.

**The Earned-Depth Rule.** Resting decorative shadows are prohibited on flat data planes.
A shadow must mean "this floats" or "this is the one thing to do here," or it does not
appear.

## 5. Components

Documented from the prototypes (`repos-list-instrument.html`, `repo-detail` in the
Instrument lane). These are the canonical primitives; the React implementation should
match their behavior.

- **Status mark.** A small colored square (rounded 2px) plus a mono uppercase word. Live states are filled squares (green/indigo/amber/red); paused is a hollow circle in gray. Color plus shape plus word makes every state colorblind-safe. Replaces the earlier rounded "pill."
- **Staleness bar (signature).** A 6px track with a fill whose width encodes lag (empty is fresh/current, fuller is more behind) and whose color is the state color, with a mono label beneath ("current", "14 behind", "uncommitted, skipped", "paused after 3 strikes"). The one bespoke, RepoSync-specific component. On a check, bars retract to zero and grow back in a short stagger.
- **Destination links.** Two icon links beside each repo name: a folder (open the local clone, always present) and an external-link (open the web repo, present only when a remote exists). Muted at rest, accent tint on hover. See the Destinations-Are-Permanent rule below.
- **Data-table row.** The heart of the Repos view: checkbox, identity (name + owner + destination links, path beneath), branch, status mark, staleness bar, last-checked, and hover-revealed operations.
- **Buttons.** Primary (solid accent), secondary (hairline border), icon-ghost. Active scale of 0.975 for tactile feedback.
- **Search + filter chips.** Mono search field; filter chips where the active chip is solid ink.
- **Option cards.** For a choice the user must understand to pick well (update mode, dirty-tree handling, branch eligibility): a stacked list of cards, each with a human-readable name, an optional secondary mono enum tag, and a one-line plain-language consequence that is **always visible** (not revealed only on selection). Unavailable/advanced options (merge, rebase) render as disabled caution wells, never as live cards. This replaces segmented controls and bare dropdowns for any consequential setting. Cards stack in a single vertical column capped to a comfortable reading width (about half the container), never full-bleed and never in a multi-column grid, so a mutually-exclusive group reads as one linear top-to-bottom choice.
- **Activity row.** Expandable; the raw command, stdout, exit code, and duration live in a dark monospace well, the one dark surface in the app, reserved for literal terminal output.
- **Floating surfaces.** Tray popover, dropdowns, dialogs: the only surfaces with a resting shadow, because they float.

## 6. Do's and Don'ts

### Do:
- **Do** carry identity in the monospace data plane and the staleness signal, so the interface reads as an instrument without needing decorative chrome.
- **Do** reserve saturated color for repo status and the blue accent for interaction; keep the two color languages strictly separate.
- **Do** encode every repo state as color plus shape plus word, so it survives grayscale and color blindness. Never rely on hue alone.
- **Do** keep destination links (local clone, web repo) always visible; hide only operations (check, pause) until hover.
- **Do** show every option's meaning up front for a consequential choice: render mutually-exclusive settings as option cards whose plain-language consequence is always visible, so the user can compare before selecting. Lead with the human label; the config enum is a secondary annotation.
- **Do** meet WCAG AA on all text, secondary and small included; push muted text toward ink until it clears 4.5:1.
- **Do** use monospace for the data plane and tabular numerals for counts.
- **Do** keep motion to functional feedback (a check resolving, the staleness sweep, hover and focus), and ship a `prefers-reduced-motion` fallback for each.

### Don't:
- **Don't** reach for drop-shadowed cards to create hierarchy; that is the generic professional-dashboard reflex. Differentiate with type, color, and the signal first; earn depth only for floating surfaces and one focal region.
- **Don't** hide an option's meaning behind interaction. Never surface a raw internal enum (`check_only`, `pull_ff_only`) as the primary label of a choice, and never make the user click a segment or open a dropdown just to learn what an option does. For consequential settings, all choices and their consequences are visible at once.
- **Don't** build the generic AI dashboard: no gradient text, no glassy hero-metric cards, no uppercase tracked eyebrow over every section, no identical icon-card grid.
- **Don't** build the heavy pro-git client: no commit-graph DAGs, no branch/tag/stash trees, no dense multi-verb git toolbars. RepoSync is not a Git client.
- **Don't** stack light gray small text on gray surfaces. Gray-on-gray low-contrast text is prohibited; "muted" means lower hierarchy, not lower legibility.
- **Don't** use the brand blue to signal repo status, or status colors for interactive controls.
- **Don't** use mono for prose or sans for machine truth; the two-register split is load-bearing.
- **Don't** animate layout properties (width, height, top, left) or add choreographed entrances; this is a background utility, not a landing page. The staleness sweep uses transform only.
