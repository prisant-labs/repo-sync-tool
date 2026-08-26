---
effort: E-14
title: Desktop Notifications
tracking-issue: 16
status: ready
tier: SHOULD
scope: V1 (integration)
depends_on: [E-08, E-09, E-10, E-02]
source: docs/internal/v1-architecture-and-decisions.md (deliverable 11 "Notifications"; cross-platform table row "Notifications - shared API, plugin abstracts OS"; event surface `notification:fired`)
---

# E-14 - Desktop Notifications

## Task Summary

> Agents keep this block current as work proceeds.

- **State:** DONE and SHIPPED in v0.9.0 (2026-07-05). The pure firing decision (`decide`) and per-cycle coalescing (`coalesce`) live in `reposync-core/src/notify.rs`, built test-first over the release/failure/auth x toggle x quiet-hours matrix and adversarially reviewed (release identity preserved, no double-report, typed `LocalMinute`); the auth-toggle policy is filed as BL-NI-17. The edge (AC5) is `src-tauri/src/notify.rs`: it raises via `tauri-plugin-notification` and emits `notification:fired`, sourcing its `LocalMinute` from the same offset-aware clock the scheduler uses.
- **Amended 2026-08-21 (BL-NI-80).** AC3's enforcement now has TWO points, and the distinction matters to anyone reading `passes_gate` alone. The core gate still refuses an event inside the window. The EDGE additionally decides, before consuming the scheduler's per-cycle buffer, whether firing is allowed at all: `CycleNotifications::drain_if` takes the verdict as an argument and returns the events only when it approves, so a refusal HOLDS them for a later tick instead of destroying them. This closed a real defect - a cycle starting at 21:59 and finishing at 22:00 previously lost every notification it produced, because `coalesce` filters rather than queues and the buffer had already been drained. AC3's wording is unchanged and better served: "no toast is raised during quiet hours" now means withheld rather than discarded. There is deliberately no unconditional drain left in the module; the ordering is enforced by the type rather than by convention, because the two-statement version passed every test in both the correct and the broken order.
- **Not covered by this effort's gate:** `updates.rs` raises the launch-time "update available" toast directly, outside `decide`/`coalesce`/`passes_gate` entirely, so no setting suppresses it (BL-NI-82, ruled: leave as-is for now). Quiet hours also suppresses the toast for a manual "Refresh metadata" the user just clicked (BL-NI-83, ruled: no change, AC3 is unqualified and the drawer shows the new tag on screen anyway).
- **Blockers:** none.

## Context

OS-native toasts for the three events the brief names: **a new release**, **a check/update failure**, and **an auth failure**. Cross-platform behavior is abstracted by `tauri-plugin-notification` (one API; the plugin maps to Windows toast vs macOS Notification Center), so the only platform-specific piece is the plugin call. The **firing decision** (whether to notify, given the event and the settings) is plain logic and is UI-independent and headlessly testable.

Notifications are gated by two settings (`notify_on_release`, `notify_on_failure`, both default on) and suppressed during quiet hours. The typed `notification:fired` event already exists in the frozen contract (E-06); this effort fills in the emit site and the decision logic.

## In scope

- A firing rule evaluated when a check or update completes: if a **new release** was detected and `notify_on_release` is on, toast it; if the op **failed** (fetch/update error or auth failure) and `notify_on_failure` is on, toast it.
- **Quiet hours suppression**: no toasts are raised during the configured quiet-hours window (the work still happens and is logged; only the toast is withheld).
- **Coalescing / rate-limiting**: a scheduler cycle that touches many repos raises at most a small bounded number of toasts (e.g. one summary toast "5 repos updated, 1 failed") rather than one per repo, to avoid notification spam.
- Emit the typed `notification:fired` event for each toast so the frontend can mirror it.
- Best-effort click action (focus the app / open the relevant repo) where the plugin supports it.

## Out of scope

- The settings **toggle UI** (the settings screen renders `notify_on_release` / `notify_on_failure`; storage is E-02).
- The `notification:fired` **payload definition** (frozen in E-06).
- The daily summary (E-11) and the activity log (E-09), which are separate surfaces; this effort consumes their signals, it does not produce them.

## Contract / deliverables

1. A firing-decision function: `(event, settings, clock) -> Option<Notification>`, pure and unit-tested against the matrix of (release/failure/auth) x (toggle on/off) x (inside/outside quiet hours).
2. The emit site in the scheduler/check-completion path that calls the function and raises the toast via `tauri-plugin-notification`, then emits `notification:fired`.
3. Coalescing so a multi-repo cycle raises a bounded number of toasts.

## Acceptance criteria

- [x] AC1: A completed check that detects a new release raises exactly one toast when `notify_on_release` is on, and none when it is off. Source: brief deliverable 11; settings `notify_on_release`. **Done in core** (`decide` returns one payload; the edge raises the OS toast).
- [x] AC2: A failed check/update or an auth failure raises a toast when `notify_on_failure` is on, and none when off. Source: brief deliverable 11; settings `notify_on_failure`. **Done in core** (auth shares the failure toggle per this AC; separate auth policy = BL-NI-17).
- [x] AC3: No toast is raised during quiet hours; the underlying work still runs and is logged. Source: brief Section on quiet hours; settings `quiet_hours_start/end`. **Done in core** (`passes_gate` reuses the scheduler's `in_quiet_hours` via `is_quiet`; the gate withholds only the toast), **with a second enforcement point at the edge since 2026-08-21** so a withheld toast is held for a later tick rather than destroyed (BL-NI-80; see the Task Summary amendment).
- [x] AC4: A scheduler cycle over many repos raises a bounded (coalesced) number of toasts, not one per repo. Source: notification-spam avoidance (derived; brief deliverable 11 "another surface to QA"). **Done in core** (`coalesce`: per-kind individual toasts up to the cap + one overflow summary, bounded by `2 * MAX_INDIVIDUAL_TOASTS + 1`).
- [ ] AC5: Each raised toast emits the `notification:fired` event. Source: brief event surface `notification:fired` (E-06 contract). **Deferred edge** - the core returns the ready `NotificationFiredPayload`; the `src-tauri` raise + emit is the edge-wiring effort.
- [x] AC6: The firing decision is a pure function unit-tested across the toggle/quiet-hours/event matrix, with no plugin or UI dependency. Source: the seam principle (logic is Tauri-free and testable). **Done in core** (`reposync-core/src/notify.rs`; 10 tests; `cargo tree` confirms no tauri).

## Dependencies

- Upstream: E-08 (scheduler check-completion is the trigger point + the cycle boundary for coalescing), E-10 (new-release detection), E-09 (failure records), E-02 (settings toggles + quiet hours). The plugin call sits in `src-tauri`; the decision logic sits in `reposync-core`.
- Downstream: the settings screen renders the toggles (out of scope here).

## V1.1 extension points

- Per-repo notification preferences (currently global toggles).
- Richer notification actions (inline "open", "pause this repo") as the plugin/OS support allows.
- Notification grouping/threads on platforms that support it.

## Open questions

- **Tier (flag for jp):** marked SHOULD - failures and releases are also visible in the UI, so toasts are delight, not the only surface. Promote to MUST if "silent failures" are considered unacceptable for the resident utility.
- Exact coalescing policy (one summary toast per cycle vs per-category). Default: one summary toast for the cycle plus, optionally, individual toasts for failures.
- Whether quiet-hours-suppressed notifications are dropped or queued to the next window. Default: dropped (the daily summary covers the recap).
