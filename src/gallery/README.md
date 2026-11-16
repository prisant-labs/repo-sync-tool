# Component gallery

The gallery renders the real components in `src/` against fixture data, so a design
can be judged on the code that ships instead of on a hand-drawn prototype. It is a
second Vite entry. It does not touch the app entry (`index.html`, `src/main.tsx`) and
it is never bundled into the Tauri app.

## What it contains

- **Primitives**: every component in `src/components/ui/`, in its variants and states.
- **Product components**: the shared parts in `src/components/` (status badge, filter
  chip, lag signal, empty state, async panel, dialogs, and similar).
- **Recipes**: the small shared components in `src/components/recipes/` that several
  screens reuse, one section per recipe with every variant shown: `OptionCard` and
  `RadioDot` (choice cards), `GroupChip` and `GroupDot` (group name chips, in a
  `removable` and a `compact` variant), and `SettingsField` with `NumberInput`,
  `TextInput` and `TimeInput`. The key-value rows in `repo-detail.tsx` and
  `diagnostics-card.tsx` are deliberately not a recipe: they are two different
  layouts, and the diagnostics row's own comment explains why.
- **Screens**: Dashboard, Repos, Activity, Settings, the repository detail drawer and
  the whole app shell, each rendered from the real screen component.

The control bar switches light and dark (the `.dark` class the app uses), the look
(Today, Look A, Look B) and the data state (populated, empty, loading). `.dark` and
`data-look` always sit on the same element, which is what the override blocks in
`src/index.css` are written for. "Today" sets no `data-look` at all, exactly like the
shipped app. Everything switches in-page, because the built file has no server and no
query string. Drawers, dialogs and toasts stay inside their cell, because each
overlay cell has a CSS `transform`, which contains `position: fixed`.

## Comparing looks

The **Compare looks** section (group "Looks") draws the same real components three times,
one column per look, each column its own theme root. It follows the Dark switch. The
Look selector is disabled there because the section shows all three.

- **Today**: no `data-look`. The control. The shipped app renders like this column.
- **Look A (graphite)**: today's colours with tighter type (meta weight 500, cell
  line-height 1.4, meta line-height 1.35, micro raised to 11px). Changes no colour, so
  most of the difference is in the type specimen at the foot of each column: no real
  component reads the `cell`, `meta`, `label` or `micro` classes yet.
- **Look B (slate)**: cool-tinted neutrals, a dark navigation rail in both themes,
  stronger borders and secondary text, 6px corners, heavier weights.

Measured contrast, from the token values in `src/index.css` (the test in
`src/lib/contrast.test.ts` recomputes these and fails if a row here is stale).
"Secondary text" is the worst of `muted-foreground` on page, card, well, popover and
the rail's surfaces. Look B in the light theme measures the rail against its dark
palette.

| Look | Theme | Secondary text | Border on card | Card on page | Button label on accent |
|---|---|---|---|---|---|
| Today | light | 4.71 | 1.37 | 1.10 | 5.49 |
| Today | dark | 5.83 | 1.32 | 1.10 | 4.98 |
| Look A | light | 4.71 | 1.37 | 1.10 | 5.49 |
| Look A | dark | 5.83 | 1.32 | 1.10 | 4.98 |
| Look B | light | 7.39 | 2.00 | 1.23 | 7.24 |
| Look B | dark | 8.09 | 1.97 | 1.19 | 5.04 |

Text needs 4.5:1. Borders and surface steps are not gated; they are listed so the
difference between the looks can be read as a number as well as seen.

## How it gets data

`stub-ipc.ts` installs `window.__TAURI_INTERNALS__` before any component runs, and
answers each command from `fixtures.ts`. The fixtures are typed against
`src/lib/bindings.ts`, so a changed IPC type fails `pnpm typecheck` here. Paths and
names are invented; keep it that way.

## Build and open

```sh
pnpm gallery:build   # writes dist-gallery/gallery.html, one self-contained file
pnpm gallery:dev     # dev server on port 1422 (never 1420, which Tauri owns); open /gallery.html
```

Double-click `dist-gallery/gallery.html` to open the build. `dist-gallery/` is
gitignored.

## Screenshot check

Every pull request runs the `gallery screenshots` job in `.github/workflows/ci.yml`. It
builds the gallery at the PR head and at the merge base, photographs every section in
light and dark (default "Today" look, populated data), compares the two sets pixel by
pixel, and writes a table of the sections that changed to the job summary. The base, head
and diff PNGs are attached as the `gallery-screenshots` artifact.

The job reports and does not fail on pixel differences. It fails only if the head gallery
does not build or the script errors. If the merge base has no `gallery:build` script, it
says "No base gallery" and passes.

The tools are in `tools/gallery-shots/`. The script drives the gallery's own controls, so
a new section is photographed without any change there. Output is made repeatable with a
fixed 1440 pixel viewport, a frozen clock, no animations and a wait for fonts.

To run it locally, with the base and head each built into their own copy of the gallery:

```sh
pnpm exec playwright install chromium-headless-shell   # once
pnpm gallery:build
cp dist-gallery/gallery.html /tmp/base.html            # keep the "before" build
# ...make your change, then:
pnpm gallery:build
node tools/gallery-shots/shoot.mjs /tmp/base.html /tmp/shots/base
node tools/gallery-shots/shoot.mjs dist-gallery/gallery.html /tmp/shots/head
node tools/gallery-shots/compare.mjs /tmp/shots/base /tmp/shots/head /tmp/shots/diff
```

Shooting the same build twice must report zero changed pixels. Screenshots taken on
Windows and on the CI's Linux runner differ in font rendering, so compare only runs from
the same machine.

## The rule

**Every new shared component, and every new variant or state of an existing one,
must be added to the gallery in the same change.** Add a cell to `sections.tsx`, and
a fixture to `fixtures.ts` if it needs data. A component that is not in the gallery
cannot be reviewed there, and the gallery stops being the source of truth.
