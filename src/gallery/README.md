# Component gallery

The gallery renders the real components in `src/` against fixture data, so a design
can be judged on the code that ships instead of on a hand-drawn prototype. It is a
second Vite entry. It does not touch the app entry (`index.html`, `src/main.tsx`) and
it is never bundled into the Tauri app.

## What it contains

- **Primitives**: every component in `src/components/ui/`, in its variants and states.
- **Product components**: the shared parts in `src/components/` (status badge, filter
  chip, lag signal, empty state, async panel, dialogs, and similar).
- **Screens**: Dashboard, Repos, Activity, Settings, the repository detail drawer and
  the whole app shell, each rendered from the real screen component.

The control bar switches light and dark (the `.dark` class the app uses), the
`data-look` attribute on the gallery root, and the data state (populated, empty,
loading). Everything switches in-page, because the built file has no server and no
query string. Drawers, dialogs and toasts stay inside their cell, because each
overlay cell has a CSS `transform`, which contains `position: fixed`.

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

## The rule

**Every new shared component, and every new variant or state of an existing one,
must be added to the gallery in the same change.** Add a cell to `sections.tsx`, and
a fixture to `fixtures.ts` if it needs data. A component that is not in the gallery
cannot be reviewed there, and the gallery stops being the source of truth.
