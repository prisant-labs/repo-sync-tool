---
effort: E-14
title: Desktop Notifications - implementation plan
status: ready
---

# E-14 - Desktop Notifications - Implementation Plan

## Ordered steps

1. **Decision function (test-first).** In `reposync-core` (e.g. `notify.rs`), write the pure `decide(event, settings, now) -> Option<Notification>` covering release / failure / auth x toggle on-off x quiet-hours in/out. Write the unit-test matrix first, then the function.
2. **Coalescing.** Add a cycle-level reducer: given the per-repo outcomes of one scheduler cycle, produce the bounded set of notifications (one summary + optional per-failure).
3. **Plugin wiring.** Add `tauri-plugin-notification` to `src-tauri`. In the scheduler's check-completion path, call `decide`/the reducer and raise each resulting toast via the plugin.
4. **Event emit.** For each raised toast, emit the typed `notification:fired` event (E-06 payload).
5. **Quiet hours.** Ensure the decision consults the injected clock + quiet-hours settings; the scheduler already has an injected clock (E-08), reuse it.
6. **Click action.** Best-effort: set the toast action to focus the app / open the repo where supported.
7. **Verify.** Unit tests green for the decision matrix + coalescing; manual smoke on Windows that a real toast appears for a release and a failure, and is suppressed in quiet hours and when toggled off.

## Test strategy

- The decision function and the coalescing reducer are pure and live in `reposync-core`: exhaustive unit tests, no plugin, no UI (the seam principle). The plugin call and the OS toast are the only untested-by-unit pieces; cover them with a manual Windows smoke. macOS toast behavior is deferred to the staged Mac pass.

## Files touched

- `crates/reposync-core/src/notify.rs` (new: decision + coalescing; Tauri-free).
- `src-tauri/src/` scheduler/check-completion path (call the decision; raise the toast; emit the event).
- `src-tauri/Cargo.toml` + capabilities (add `tauri-plugin-notification`).

## Risks

- Notification permission/availability differs per OS and Windows focus-assist can silently swallow toasts; the `notification:fired` event gives the UI a reliable mirror regardless.
- Over-notifying is the main failure mode; the coalescing reducer and the unit matrix guard it.

## Definition of done

- All six ACs met; the decision + coalescing are pure and exhaustively unit-tested; a Windows smoke shows real toasts under the right conditions and silence under the wrong ones; local gate green.
