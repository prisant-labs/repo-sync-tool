---
name: RepoSync
description: A resident desktop tray utility that keeps a personal library of consume-only Git repos fresh, visible, and safe, with a transparent audit trail.
---

<!-- Canonical design system, reconciled 2026-07-03 to the SHIPPED React UI (the
"Graphite" direction). The source of truth for tokens is src/index.css (oklch,
light + dark) and the component set under src/components; this document describes
that implementation, it does not lead it. Earlier provisional drafts (the
"Instrument + Signal" hex tokens, and the editorial Fraunces direction documented
in docs/internal/v1-architecture-and-decisions.md) are superseded; the draft
mockups were archived to _local/gui/archived-mockups. Principles below are the
durable part and predate the token choice. -->

# Design System: RepoSync

## 1. Overview

**Creative North Star: "The Quiet Instrument"**

RepoSync is a precise instrument that reports true state at a glance, the way a
well-designed gauge cluster does: dense, legible, trustworthy, and undramatic. It is
**confident and precise** in voice, quiet in footprint (it lives in the tray and never
nags) but exact in content (when you look, it tells you everything that matters, with no
hand-waving).

The committed visual expression is **Graphite**: a calm, cool-neutral shadcn/ui base
carrying the interface, with identity coming from three places rather than from chrome.
First, a **monospace data plane**: repo names, branches, SHAs, counts, status words, and
timestamps read as machine truth. Second, a **status taxonomy** that is the only
saturated color allowed to draw the eye, because repo state is the most important thing
on the screen. Third, a **signature lag signal**: a per-repo bar whose fill length
encodes how far behind a repo is, the one thing a status word alone cannot say. These
carry the product's character, so the interface can stay quiet and near-flat without
feeling generic.

The system commits to **restraint with one accent**. Neutral surfaces carry the
interface; a single confident blue (the Graphite accent) marks where you can act (primary
buttons, links, active navigation, focus rings); and the saturated status palette is the
only color allowed to shout. Motion is **functional feedback, not choreography**: a check
resolves with a spinner and a status change, the lag bars animate by transform only,
never an orchestrated entrance.

This system explicitly rejects three failure modes. It is **not the generic AI dashboard**
(gradient text, glassy hero-metric cards, an uppercase eyebrow over every section, an
identical icon-card grid). It is **not the generic professional-SaaS dashboard** either:
differentiation is earned through the monospace register, the status color, and the lag
signal, not through a stack of drop-shadowed cards. And it is **not the heavy pro-git
client** (commit-graph DAGs, branch/tag/stash trees, dense multi-verb toolbars) built for
repos you actively develop. It also refuses the most common density failure: **light gray
small text on gray surfaces**. Density is welcome; low-contrast text is not.

**Key characteristics:**
- **Monospace as identity.** The data plane (names, owners, branches, SHAs, counts, status words, timestamps, column headers) is monospace, so the surface reads as an instrument, not a SaaS app.
- **A signature signal.** The lag bar (`LagSignal`) encodes lag magnitude, the one thing a status word cannot: not just "behind," but "behind, and a lot."
- **Status-forward color.** Saturated color is reserved for repo state; one blue accent is reserved for interaction. The two never cross.
- **Differentiation before decoration.** Skimmability comes from type, color, and the signal, not from resting shadow. Depth is earned (floating surfaces, one focal region), never a default coat of paint.
- **Colorblind-safe and AA throughout.** Every state is color plus shape plus word; all text meets contrast, secondary and small included.

## 2. Colors

The token source of truth is `src/index.css`: oklch values, mapped through
`@theme inline` to Tailwind utilities, for both `:root` (light) and `.dark`. oklch is
used deliberately: its independent lightness / chroma / hue lets the status ramp stay
perceptually consistent across light and dark from a single hue per state. The base is
the shadcn/ui neutral scale; RepoSync adds the accent hue and the six status tokens.

### Interaction accent (one blue, `--primary`)
- **Graphite blue** `oklch(0.52 0.19 264)` light / `oklch(0.6 0.16 264)` dark; focus ring `--ring: oklch(0.62 0.15 264)`. Primary buttons, links, active nav, focus. Means "you can act here." **Never** used to convey repo status.

### Status taxonomy (the only saturated color that shouts)
Each token has a light and a dark value (lighter, slightly less chroma in dark for AA on
the dark surface). Rendered always as color **plus** a lucide icon **plus** a word.

| State | Hue family | Light oklch | Dark oklch | Icon |
|---|---|---|---|---|
| **sync / ahead** | green | `0.53 0.12 150` | `0.75 0.17 152` | check / arrow-up |
| **behind** | violet | `0.47 0.19 293` | `0.72 0.14 300` | arrow-down |
| **dirty** | amber | `0.54 0.1 79` | `0.8 0.13 85` | alert-triangle |
| **failed** | red | `0.53 0.19 27` | `0.7 0.18 24` | x-circle |
| **paused** | neutral | `0.51 0.02 258` | `0.66 0.02 258` | pause-circle |
| **release** | magenta | `0.51 0.19 349` | `0.75 0.17 349` | package |

`behind` is deliberately **violet** (hue ~293), not blue, so it never reads as the
interaction accent (hue 264) and so it stays distinct from `failed` red and `sync` green.
`paused` is a near-neutral (very low chroma), so a disabled repo reads as "off," not as a
status that shouts. `release` is a signal color for "a new upstream release exists," not a
repo state.

### Neutrals (shadcn neutral scale, oklch)
Cool, near-hueless grays. `--background` white (`oklch(1 0 0)`) / near-black in dark;
layered surfaces `--card`, `--sidebar` (recessed), `--muted`, `--popover`; `--border` /
`--input` hairlines; `--muted-foreground` for secondary text. Radius scale from
`--radius: 0.625rem`.

### Named rules
**The Status-Owns-Saturation Rule.** Saturated color belongs to repo status and nothing
else. The blue accent is for interaction only. If a status color and the accent would
collide, the status shifts hue (behind is violet, not blue); the accent never yields, and
the two color languages never mix.

**The No-Gray-on-Gray Rule.** Text is contrast-checked against the surface it actually
sits on. Light gray small text on a gray surface is prohibited. Secondary and small text
still meet WCAG AA; "muted" means lower in the hierarchy, pushed toward ink, never toward
the background.

## 3. Typography

**UI / chrome font:** the default sans stack (a clean technical sans; no exotic display
face). **Data / identity font:** the default monospace stack (`font-mono`). The
personality comes from the two-register split and from precision, not from font-pairing
flourish. (Named faces are intentionally not pinned; the register split is what is
load-bearing, not a specific typeface.)

### Hierarchy (a deliberately tight, dense ramp)
- **View title / eyebrow** mono, uppercase, tracked (the topbar breadcrumb).
- **Page heading** sans, ~2xl bold (a screen title like "Dashboard").
- **Identity / body** mono ~13-14px: repo names, primary cell text.
- **Cell / help** mono or sans ~12px: dense cell text, inline help.
- **Meta / lag label / path** mono ~11px: secondary metadata under the primary line.
- **Column header / field label** mono ~10-11px, uppercase, tracked.

Hierarchy is carried by **register (mono vs sans), weight, and color**, not by large size
jumps. Counts use tabular figures where alignment matters.

### Named rule
**The Two-Register Rule.** The UI has exactly two type registers. **Mono is the
data/identity plane** (repo names, owners, branches, SHAs, counts, status words,
timestamps, paths, raw command output, column headers). **Sans is the human/chrome plane**
(navigation, buttons, page titles, help text, prose, error explanations). Never mono for
prose; never sans for a SHA. When the app offers a **choice** (an update mode, a policy),
the primary label is the human-readable sans name; the machine identifier (the config
enum) may sit beside it as a secondary mono tag, never as the only label.

## 4. Elevation & Differentiation

Near-flat by intent, never undifferentiated. The fix for flatness is not decorative shadow
but differentiation through the data-design system plus layered neutral surfaces. Depth is
earned, not a default coat of paint.

Surface layers (all neutral tokens): the recessed **sidebar** (`--sidebar`), the app
**background**, the raised **card** (`--card`) that holds tables and panels, and **muted**
insets (`bg-muted`) for table-header bands and wells. Separation is done with hairline
borders and tonal steps, not resting shadows.

Three depth levels, used sparingly:
- **Data plane (flat).** The repo table and readouts sit flat on the card, separated by hairlines and tonal steps. Skimmability comes from type, color, and the lag bar.
- **Floating surfaces (real shadow).** Things that genuinely float (the drawer, dialogs, dropdowns, toasts, a hover-lifted row) get a soft real shadow, because they are above.
- **Focal region (one, earned).** On a screen with a single most-important action (the "behind, fast-forward available" panel in the detail drawer) that one region takes a status tint (`bg-status-*/12`) and a status-tinted border so the eye lands on it first. One per screen, at most.

### Named rules
**The Differentiation-Before-Decoration Rule.** When a layout reads flat and hard to skim,
the first move is more type, color, or signal contrast, not a drop shadow.

**The Earned-Depth Rule.** Resting decorative shadows are prohibited on flat data planes.
A shadow must mean "this floats" or "this is the one thing to do here," or it does not
appear.

## 5. Components

The React implementation is the canonical source (`src/components`, `src/screens`). The
primitives are hand-written shadcn/ui new-york parts (`button`, `card`, `badge`, `input`,
`switch`, plus `drawer`, `dialog`, `toast`); the product-specific parts are below.

- **StatusBadge.** Color + a lucide icon + a mono word (`text-status-*` on the icon and label). Live states carry their own icon (check, arrow-down, alert-triangle, x-circle); paused is a pause-circle in the neutral token. Color plus shape plus word makes every state colorblind-safe. Ahead/behind fold the count into the label ("14 behind").
- **LagSignal (signature).** A 1.5px track with a fill whose width encodes lag (empty is current, fuller is more behind) in the state color, with a mono label beneath ("current", "14 behind", "uncommitted, skipped", "watching paused"). The one bespoke RepoSync component. Animated with `transform: scaleX` only.
- **Repo table row.** The heart of the Repos view: a CSS grid of identity (name + host + release), StatusBadge, LagSignal, last-checked, and a hover/right-aligned check action. The whole row is a button that opens the detail drawer; the check button stops propagation.
- **Detail drawer** (`RepoDetailPanel`). A right slide-over (`Drawer`) over `repo_get`: a state-specific **focal** panel (fast-forward CTA when behind, retry when failed, resume when paused), quick actions, an "Open in" row, the update-policy **option cards**, latest release, and a "where it lives" facts table.
- **Option cards.** For a choice the user must understand to pick well (update mode, dirty handling, branch policy): a single vertical column of cards, each with a human-readable name, an optional secondary mono enum tag, and a one-line plain-language consequence that is **always visible** (not revealed only on selection). Unavailable/advanced options (merge, rebase) render as disabled wells, never live cards. Capped to a comfortable reading width, never a multi-column grid, so a mutually-exclusive group reads as one linear top-to-bottom choice. Replaces segmented controls and bare dropdowns for any consequential setting.
- **Buttons.** Primary (solid accent), secondary (subtle fill), outline (hairline border), icon-ghost. `[&_svg]:size-4`; sizes default / sm / icon.
- **Search + filter chips.** A mono-ish search field; status filter chips carry the state color and a live count, the active chip in the accent.
- **AsyncPanel.** The loading / error / empty boundary every screen renders through; the error state surfaces the backend's own remediation text (from `IpcError`).
- **Toast.** Transient action feedback (ok / info / error), bottom-right, auto-dismissing; every mutation toasts.
- **Floating surfaces.** Drawer, dialog, toast, dropdowns: the only surfaces with a resting shadow, because they float.

## 6. Motion

Functional feedback only, transform/opacity only, never layout properties. The lag bar
fills via `scaleX`; the drawer slides via `translateX`; busy actions use `animate-spin`;
status and surfaces cross-fade. No choreographed entrances, this is a background utility.
A `prefers-reduced-motion` fallback is owed for each animated affordance.

## 7. Do's and Don'ts

### Do:
- **Do** carry identity in the monospace data plane and the lag signal, so the interface reads as an instrument without decorative chrome.
- **Do** reserve saturated color for repo status and the blue accent for interaction; keep the two color languages strictly separate.
- **Do** encode every repo state as color plus shape plus word, so it survives grayscale and color blindness. Never rely on hue alone.
- **Do** keep destination/open actions discoverable; lead a repo row with identity and status.
- **Do** show every option's meaning up front for a consequential choice: render mutually-exclusive settings as option cards whose plain-language consequence is always visible. Lead with the human label; the config enum is a secondary annotation.
- **Do** meet WCAG AA on all text, secondary and small included; push muted text toward ink until it clears 4.5:1.
- **Do** use monospace for the data plane and tabular figures for counts.
- **Do** keep motion to functional feedback (a check resolving, the lag sweep, hover and focus), transform/opacity only, with a `prefers-reduced-motion` fallback.

### Don't:
- **Don't** reach for drop-shadowed cards to create hierarchy; that is the generic professional-dashboard reflex. Differentiate with type, color, and the signal first; earn depth only for floating surfaces and one focal region.
- **Don't** hide an option's meaning behind interaction. Never surface a raw internal enum (`check_only`, `pull_ff_only`) as the primary label of a choice, and never make the user click a segment or open a dropdown just to learn what an option does.
- **Don't** build the generic AI dashboard: no gradient text, no glassy hero-metric cards, no uppercase tracked eyebrow over every section, no identical icon-card grid.
- **Don't** build the heavy pro-git client: no commit-graph DAGs, no branch/tag/stash trees, no dense multi-verb git toolbars. RepoSync is not a Git client.
- **Don't** stack light gray small text on gray surfaces. Gray-on-gray low-contrast text is prohibited; "muted" means lower hierarchy, not lower legibility.
- **Don't** use the accent blue to signal repo status, or status colors for interactive controls.
- **Don't** use mono for prose or sans for machine truth; the two-register split is load-bearing.
- **Don't** animate layout properties (width, height, top, left) or add choreographed entrances; the lag sweep and drawer use transform only.
