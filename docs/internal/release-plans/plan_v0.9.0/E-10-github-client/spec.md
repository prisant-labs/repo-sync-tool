---
effort: E-10
tracking-issue: 12
title: GitHub Metadata Client
status: ready
tier: SHOULD
scope: V1 (non-GUI)
depends_on: [E-02]
source: docs/internal/v1-architecture-and-decisions.md (Sections 6, 4.4, 4.2, 4.9)
---

# E-10 - GitHub Metadata Client

## Task Summary

> Agents keep this block current as work proceeds.

- **State:** done - built test-first on build/e-01-foundation: a direct `reqwest`+rustls client (NOT octocrab; see the AC1 resolution), gate green, OpenSSL-free verified, 230 core tests.
- **Resolution (HTTP library, jp-ratified 2026-06-25):** the spec's "octocrab on reqwest" was impossible - octocrab is built on hyper, not reqwest (verified against octocrab's docs). Resolved to a direct `reqwest`+rustls client, honoring the brief's Section 4.2 no-OpenSSL HTTP-stack decision (the authoritative choice); octocrab in brief Section 6 was a convenience pick that conflicts with it. The `Transport` seam keeps octocrab a localized swap if the GitHub surface ever outgrows V1's ~6 fields.
- **Next:** the thin `repo_refresh_metadata` command shell (E-06 / src-tauri) and the proactive rate-limit backoff-across-a-pass wiring ride with the scheduler/command integration. `last_remote_sha` is left None in V1 (a precise default-branch HEAD sha needs a separate /commits call; the seam already persists it).
- **Blockers:** none.

## Context

This effort delivers the cheap-delight half of the tray's value: enriching each tracked repo with its GitHub host metadata - description, default branch, latest release (tag/date/URL), topics, and the archived flag. It is the SHOULD-tier item the scope ledger keeps for V1, and it is kept deliberately narrow: only the **unauthenticated** path ships in V1. The keyring-backed PAT flow is CUT to V1.1.

Two design constraints from the brief make this both cheap and well-behaved. First, the client uses `reqwest` with **rustls**, not OpenSSL, so there is no platform TLS or native-deps divergence to manage; this mirrors the same rustls-over-OpenSSL choice the git engine makes for `git2`. Second, all caching and rate-limit discipline is a network concern, not a UI concern: the client reads and writes `repo_remote_meta` (`etag`, `last_remote_sha`, `last_fetched_at`, and the metadata fields) and never touches a screen. It can be built and tested entirely behind the frozen IPC contract.

The load-bearing forward-compatibility move is the **auth seam**. The brief's full design stores an optional PAT in the OS keychain and records only a `github_token_present` boolean in `settings`. V1 does not build that vault. Instead the client takes its token from behind a small `TokenProvider` (or equivalent) abstraction whose V1 implementation always returns `None`, so the client always runs the unauthenticated path. Wiring the keyring later is then a localized change - a second `TokenProvider` impl - with no rewrite of the fetch, cache, or backoff logic.

This effort owns the metadata fetch, the ETag conditional-request caching against `repo_remote_meta`, the rate-limit backoff, and the explicit PAT-seam-stubbed-for-V1.1 extension point. It does NOT own the `repo_remote_meta` schema (E-02), the `repo_refresh_metadata` command wiring into Tauri (E-06 defines the IPC type; the command shell lives in `src-tauri`), or the keyring vault (CUT to V1.1).

## In scope

- An `octocrab`-based client built on `reqwest` with **rustls** (no OpenSSL), fetching per-repo: `description`, default branch, latest release (`latest_release_tag`/`latest_release_at`/`latest_release_url`), `topics`, and the `is_archived` flag.
- A `TokenProvider` (or equivalent) seam supplying the auth token; the V1 implementation always returns `None`, so the client runs the unauthenticated path. This is the localized extension point the keyring PAT plugs into in V1.1.
- ETag / `If-None-Match` conditional requests: the HTTP ETag is cached in `repo_remote_meta.etag` (E-02 AC9), `last_fetched_at` drives the ~24h refresh clock so a repo is not re-queried before its refresh window elapses, and `last_remote_sha` records the observed commit SHA.
- Rate-limit handling: read `X-RateLimit-Remaining` from responses and back off when remaining is at or below 10% of the limit.
- The write path into `repo_remote_meta`: on a `200`, update the metadata fields plus `etag`/`last_remote_sha`/`last_fetched_at`; on a `304 Not Modified`, refresh only `last_fetched_at` (the cached metadata is still current).
- The metadata-fetch entry point that the future `repo_refresh_metadata(id) -> RepoDetail` command calls into; this effort provides the core function, the command shell is thin wiring.

## Out of scope

- The keyring-backed PAT vault (Windows Credential Manager / macOS Keychain) and the `github_token_present` settings boolean - CUT to V1.1. V1 ships the seam stubbed to `None` only.
- The `repo_remote_meta` table definition and the `SqlitePool` (E-02); this effort reads and writes the table E-02 owns.
- The `repo_refresh_metadata` IPC payload type and `tauri-specta` codegen (E-06); this effort exposes a core function the command wraps.
- The scheduler cadence that decides *when* metadata is refreshed (E-08); this effort honors the ~24h clock when called but does not own the tick.
- The `AppError` variants for rate-limited / network-lost / not-found (E-05); this effort returns engine-level results that E-05 later wraps.
- Any UI surfacing of release lines, topics, or the archived badge (UI surface, out of these efforts).

## Contract / deliverables

1. An `octocrab` client on `reqwest`+rustls fetches description, default branch, latest release tag/date/URL, topics, and archived flag for a given repo's host coordinates.
2. A `TokenProvider` seam supplies the token; the V1 impl returns `None` so every request runs unauthenticated.
3. Requests send `If-None-Match` using the stored ETag and a ~24h refresh clock suppresses re-queries inside the window.
4. `X-RateLimit-Remaining` is honored: under 10% of the limit the client backs off rather than continuing to spend the budget.
5. A `200` updates the `repo_remote_meta` metadata fields plus `etag`/`last_remote_sha`/`last_fetched_at`; a `304` refreshes `last_fetched_at` only.
6. The metadata-fetch function is the seam the future `repo_refresh_metadata` command calls; no client rewrite is needed to add the keyring PAT later.

## Acceptance criteria

- [ ] AC1: The client fetches description, default branch, latest release (tag/date/URL), topics, and the archived flag via a direct **`reqwest`+rustls** client. (RESOLUTION, jp-ratified 2026-06-25: the spec originally said `octocrab`, which is impossible - octocrab is built on hyper, not reqwest. Resolved to reqwest+rustls-direct per the Task Summary; the `Transport` seam keeps octocrab swappable.) The GitHub-API-to-DB-column mapping is explicit: `published_at` -> `latest_release_at`, `html_url` -> `latest_release_url`, `topics` -> `topics_json`. Source: brief Section 6 (GitHub metadata client row) and Section 4.4 (`repo_refresh_metadata`).
- [ ] AC2: The HTTP stack is `reqwest` with **rustls** (no OpenSSL), so there is no platform TLS divergence. Source: brief Section 4.2 (what is shared: "`reqwest` with rustls (not OpenSSL)").
- [ ] AC3: Conditional requests use ETag / `If-None-Match` with the HTTP ETag cached in `repo_remote_meta.etag` (E-02 AC9); `last_fetched_at` drives the ~24h refresh clock and `last_remote_sha` records the observed commit SHA. Source: brief Section 6 (GitHub metadata client row) and Section 4.5 (`repo_remote_meta` fields).
- [ ] AC4: The client reads `X-RateLimit-Remaining` and backs off when remaining is at or below 10% of the limit. Source: brief Section 6 (GitHub metadata client row).
- [ ] AC5: V1 ships the **unauthenticated** path only; the optional PAT is supplied behind a `TokenProvider` seam whose V1 impl returns `None`, and the keyring vault is deferred to V1.1. Source: scope ledger (E-10 "unauthenticated path"; keyring PAT "CUT to V1.1") and brief Section 4.9 (`github_token_present` boolean, token in keychain).
- [ ] AC6: A `200` writes the metadata fields plus `etag`/`last_remote_sha`/`last_fetched_at`; a `304 Not Modified` updates `last_fetched_at` only and leaves cached metadata intact. Source: brief Section 6 (caching is a network concern) and Section 4.5 (`repo_remote_meta`).

## Dependencies

- Upstream: E-02 (the `repo_remote_meta` table, the `SqlitePool`, and the `paths` seam).
- Downstream: E-06 (the `repo_refresh_metadata` IPC type wraps this function), E-11 (the daily summary reads enriched release metadata this client caches).

## V1.1 extension points

- The keyring-backed PAT flow plugs in as a second `TokenProvider` impl that reads the token from the OS keychain (Windows Credential Manager / macOS Keychain) and flips `settings.github_token_present` to `true`; the fetch, cache, and backoff logic is untouched. Source: brief Section 4.9 and scope ledger.
- The authenticated rate limit (5000/hour vs the unauthenticated 60/hour) becomes available once the PAT seam returns a token; the same `X-RateLimit-Remaining` backoff logic applies against the higher ceiling.
- Non-GitHub hosts (GitLab, self-hosted) could extend behind the same metadata-fetch seam if `host_type` ever grows beyond GitHub.

## Open questions

- Exact mapping from a repo's stored coordinates (`remote_origin_url` / `host_type` in `repos`) to GitHub owner/name for the `octocrab` call: confirm the parse handles SSH and HTTPS remote URL forms and skips non-GitHub `host_type` rows cleanly. Flag any URL form the parser cannot resolve.
- Whether the ~24h refresh clock is a fixed constant or reads a settings value; default to a constant in V1 and lift it to settings only if a need appears. Flag at scaffold time.
- Whether a `403` with `X-RateLimit-Remaining: 0` should surface as a distinct "rate-limited" state to the caller now or wait for the E-05 taxonomy; default to returning an engine-level rate-limited result that E-05 maps later.
