---
effort: E-10
plan_for: spec.md
status: ready
---

# E-10 Implementation Plan

## Approach

Build the client inside out: first the auth seam (so the unauthenticated path is structurally the only path V1 can take), then the bare unauthenticated fetch, then the ETag cache layer over `repo_remote_meta`, then the rate-limit backoff, last the function the `repo_refresh_metadata` command will call. Keep every piece pure-ish and headlessly testable: the HTTP boundary is mockable, the cache decisions are functions over `repo_remote_meta` rows and a clock, and the backoff decision is a function over a rate-limit header. Pin `reqwest` to rustls from the first commit so the no-OpenSSL property is never accidentally lost. The durable property to protect is that adding the keyring PAT in V1.1 is a single new `TokenProvider` impl, not a client rewrite.

## Steps

1. **Auth seam first.** Define a `TokenProvider` trait (or equivalent) in `crates/reposync-core/src/github.rs` with a single method returning `Option<Token>`. Provide the V1 impl that always returns `None`. The client takes a `TokenProvider` so the request-building code branches on "token present" exactly once, in one place; in V1 that branch always goes unauthenticated (AC5). Document with a `// TODO(V1.1): keyring-backed TokenProvider` marker at the seam.
2. **Client construction (rustls).** Build the `octocrab` client on a `reqwest` client configured with **rustls** and no OpenSSL feature (AC2). Centralize client construction so the TLS backend is set in one place and a CI/dependency check can assert no OpenSSL is pulled. Plumb the `TokenProvider` into client/request construction.
3. **Unauthenticated metadata fetch.** Implement the core fetch for one repo: resolve its GitHub owner/name from the stored `repos.remote_origin_url`/`host_type`, then pull description, default branch, latest release (`tag`/`published_at`/`html_url`), topics, and the `archived` flag (AC1). Skip rows whose `host_type` is not GitHub cleanly. Map the response into the `repo_remote_meta` field shape E-02 defined, with the explicit GitHub-API-to-DB-column mapping: `published_at` -> `latest_release_at`, `html_url` -> `latest_release_url`, `topics` -> `topics_json`.
4. **ETag cache layer.** Before issuing a request, read the repo's `repo_remote_meta` row: if `last_fetched_at` is within the ~24h refresh window, return the cached metadata without a network call (the refresh clock, AC3). Otherwise issue the request with `If-None-Match` set to the HTTP ETag stored in `repo_remote_meta.etag` (E-02 AC9). The ETag is read from and written to `repo_remote_meta.etag`; `last_remote_sha` records the observed commit SHA, not the conditional-request token.
5. **Response handling and writes.** On `200`: update the metadata fields plus `etag`/`last_remote_sha`/`last_fetched_at` (AC6). On `304 Not Modified`: update `last_fetched_at` only and keep the cached metadata (AC6). On a non-cacheable error (network lost, not found): return an engine-level result without corrupting the cached row, leaving E-05 to wrap it later.
6. **Rate-limit backoff.** Parse `X-RateLimit-Remaining` and the limit from every response. When remaining is at or below 10% of the limit, back off: stop issuing further requests in the current pass and surface a back-off signal rather than spending the remaining budget (AC4). Keep the threshold a named constant. A `403` with remaining `0` is treated as the rate-limited terminal state.
7. **Expose the refresh entry point.** Provide the single `async fn` that fetches-and-caches one repo's metadata and returns the data `repo_refresh_metadata(id) -> RepoDetail` needs. This function is the seam the Tauri command wraps; `reposync-core` stays Tauri-free (no `tauri` import). Wiring the keyring PAT later swaps only the `TokenProvider` impl (AC5).
8. **Verify.** Run the tests below on Windows; confirm a clean `cargo build` pulls no OpenSSL and `cargo tree -p reposync-core | grep -i tauri` stays empty.

## Test strategy

- **Fetch mapping test.** Feed a recorded/mocked GitHub JSON response through the mapper and assert description, default branch, latest release tag/date/URL, topics, and archived flag land in the `repo_remote_meta` shape correctly.
- **TokenProvider seam test.** Assert the V1 provider returns `None` and that request construction takes the unauthenticated branch; assert a stub provider returning a token would take the authenticated branch (proves the seam is real without building the vault).
- **ETag / 304 test.** With a mocked HTTP layer, assert that the ETag stored in `repo_remote_meta.etag` is sent as `If-None-Match`, a `304` updates `last_fetched_at` only and leaves metadata untouched, and a `200` rewrites the metadata plus `etag`/`last_remote_sha`/`last_fetched_at`.
- **Refresh-clock test.** With an injected clock, assert a repo inside the ~24h window is served from cache with no network call, and one outside the window issues a request.
- **Rate-limit backoff test.** Drive responses with descending `X-RateLimit-Remaining`; assert no backoff above 10% and a backoff signal at or below 10%, and that a `403` with remaining `0` surfaces the rate-limited result.
- **TLS-backend guard.** A dependency-hygiene assertion (in CI, alongside the E-01 tauri gate) that the resolved tree uses rustls and pulls no OpenSSL for `reposync-core`.
- All HTTP-touching tests run against a mocked transport (no live GitHub calls in CI), in plain `cargo test`, consistent with the headless-core rule.

## Files / modules touched

- `crates/reposync-core/src/github.rs` (the real client replacing the E-01 stub: `TokenProvider` seam, `octocrab`/rustls construction, fetch, cache, backoff, the refresh entry point).
- `crates/reposync-core/Cargo.toml` (`octocrab`, `reqwest` with rustls and **without** the default OpenSSL TLS, `serde`; confirm no `tauri`).
- The `repo_remote_meta` read/write helpers (in `github.rs` or a small DB helper it calls) against the E-02 `SqlitePool`.
- `src-tauri/src/commands/` (thin `repo_refresh_metadata` wrapper over the core function) - wiring only, lands with or after E-06.
- No migration is needed for ETag storage: E-02's `repo_remote_meta` already provides the dedicated `etag TEXT` column (E-02 AC9); this effort reads and writes it.

## Risks and mitigations

- **`reqwest` defaulting to native-TLS/OpenSSL.** The whole no-platform-divergence property hinges on rustls. Mitigate by disabling default features and selecting rustls explicitly, and by the CI TLS-backend guard so a future dependency bump cannot silently re-introduce OpenSSL.
- **Keeping ETag storage distinct from the SHA.** The HTTP ETag is stored in and read from the dedicated `repo_remote_meta.etag` column (E-02 AC9); `last_remote_sha` records the observed commit SHA. Do not overload `last_remote_sha` with the conditional-request token: the `etag` column is the only place the `If-None-Match` value lives.
- **Unauthenticated 60/hour limit biting during testing or a large library.** The ~24h refresh clock plus ETag `304`s keep real request volume low; the rate-limit backoff is the hard backstop. The PAT (V1.1) lifts the ceiling but is deliberately out of V1.
- **`octocrab` version / rustls feature churn.** Pin exact versions (consistent with the brief's "pin all Tauri-related crates" posture extended to the HTTP stack) and re-check at the quarterly dependency review.
- **Remote URL parsing edge cases.** SSH vs HTTPS vs non-GitHub `host_type`. Mitigate with explicit unit tests over URL forms and a clean skip for non-GitHub rows; flag unresolved forms (spec open question).

## Definition of done

All six acceptance criteria checked, the mocked-transport tests green in `cargo test` on Windows and in CI, `reposync-core` still has no `tauri` in its dependency tree and pulls no OpenSSL, the `TokenProvider` seam proven to make the keyring PAT a localized V1.1 change, and the branch ready for self-merge per `EXECUTION.md`.
