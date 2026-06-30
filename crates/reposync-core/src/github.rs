//! github - owned by E-10 (the unauthenticated GitHub metadata client).
//!
//! Enriches each tracked repo with its GitHub host metadata - description,
//! default branch, latest release (tag/date/URL), topics, and the archived flag -
//! and caches it in `repo_remote_meta` with ETag conditional requests, a ~24h
//! refresh clock, and rate-limit backoff. V1 ships the UNAUTHENTICATED path only;
//! the optional PAT plugs in behind the [`TokenProvider`] seam in V1.1.
//!
//! DESIGN: the HTTP boundary is the [`Transport`] seam, so the cache decision, the
//! 200/304 write path, the backoff decision, the URL parse, and the JSON -> DB
//! mapping are all testable as pure logic against a fake transport + an injected
//! clock, with NO live GitHub calls and NO HTTP-mock dependency. The production
//! [`ReqwestTransport`] (reqwest + RUSTLS, no OpenSSL) is the only place the
//! network stack appears and is not exercised by unit tests.
//!
//! NOTE (AC1 deviation): the spec names "octocrab on reqwest", but octocrab is
//! built on hyper, not reqwest, so that pairing is contradictory. This effort uses
//! reqwest + rustls directly (satisfying AC2 and the no-OpenSSL hard rule with full
//! control); the transport seam keeps the choice swappable.
//!
//! Tauri-free; sqlx RUNTIME query API; unix-seconds timestamps (no chrono).

use sqlx::{Row, SqlitePool};

use crate::error::AppError;

/// The refresh window: a repo fetched within this many seconds is served from
/// cache without a network call (the ~24h clock, AC3).
pub const REFRESH_WINDOW_SECS: i64 = 24 * 60 * 60;

/// Back off when `X-RateLimit-Remaining` is at or below this percent of the limit
/// (AC4). Named so the threshold is one obvious constant.
pub const RATE_LIMIT_BACKOFF_PERCENT: i64 = 10;

/// The GitHub REST API base. Centralized so the production transport has one
/// place to build URLs.
const GITHUB_API_BASE: &str = "https://api.github.com";

/// The User-Agent GitHub requires on every request.
const USER_AGENT: &str = "RepoSync";

// =============================================================================
// Data types.
// =============================================================================

/// A GitHub repo's coordinates, parsed from `repos.remote_origin_url`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoCoords {
    pub owner: String,
    pub name: String,
}

/// The repo-RESOURCE metadata mapped from a GitHub repo API response into the
/// `repo_remote_meta` shape (E-02). The latest-release fields are modeled separately as a
/// [`ReleaseState`] / [`GhRelease`], because the release sub-fetch can fail independently
/// of the repo fetch (BL-NI-15a).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GhMetadata {
    pub description: Option<String>,
    pub default_branch: Option<String>,
    /// Topics, serialized to a JSON-array string for `topics_json`.
    pub topics_json: Option<String>,
    pub is_archived: bool,
}

/// The rate-limit budget read from a response's `X-RateLimit-*` headers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RateLimit {
    pub remaining: i64,
    pub limit: i64,
    /// `X-RateLimit-Reset`: the unix second at which the window resets. `0` when the
    /// header is absent. Carried so a rate-limited outcome can surface an honest
    /// `AppError::RateLimited { reset_at }` (and the future refresh-pass orchestrator
    /// can time its resume) rather than guessing the reset.
    pub reset_at: i64,
}

/// A latest-release object from `/releases/latest`. Its fields are nullable in the API,
/// so they stay `Option`; `published_at` is unix seconds (converted from the ISO string).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GhRelease {
    pub tag: Option<String>,
    pub published_at: Option<i64>,
    pub url: Option<String>,
}

/// The conclusive state of the `/releases/latest` sub-fetch within a 200 repo fetch
/// (BL-NI-15a). A SINGLE typed value (Codex review): the release payload lives ONLY inside
/// `Found`, so an "unknown yet carrying a release" state is unrepresentable - the edge
/// cannot mistakenly persist an untrusted release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReleaseState {
    /// `/releases/latest` returned a release (200): authoritative, write it.
    Found(GhRelease),
    /// `/releases/latest` returned 404 - the repo has no release: authoritative, so a
    /// stale cached release is cleared.
    NoRelease,
    /// The sub-fetch FAILED (network / parse / rate-limit / any other status) while the
    /// repo fetch itself succeeded: the cached release fields are PRESERVED, never
    /// overwritten with a spurious `None`.
    Unknown,
}

/// One transport fetch result. The transport classifies the HTTP outcome; the
/// caller ([`refresh_one`]) decides what to persist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FetchOutcome {
    /// 200: fresh metadata, the new ETag, the observed commit SHA, the budget, and
    /// whether the latest-release sub-fetch was conclusive (BL-NI-15a).
    Modified {
        metadata: GhMetadata,
        etag: Option<String>,
        observed_sha: Option<String>,
        rate_limit: RateLimit,
        release: ReleaseState,
    },
    /// 304 Not Modified: the cached metadata is still current; only the budget.
    NotModified { rate_limit: RateLimit },
    /// A transport/connectivity failure (the cached row must be left intact).
    NetworkLost,
    /// 404: the repo is not found on GitHub.
    NotFound,
    /// Rate-limited: a 403 with remaining 0, or the budget hit the backoff floor.
    /// Carries the budget so the reset time reaches the caller (an honest
    /// `AppError::RateLimited { reset_at }`), not just the bare outcome.
    RateLimited { rate_limit: RateLimit },
}

/// The result of [`refresh_one`] for one repo. The network failures are
/// engine-level outcomes E-05 maps to `AppError` variants later; `AppError` here
/// is reserved for DB failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefreshOutcome {
    /// Served from cache (inside the refresh window); no network call.
    Cached,
    /// 200: metadata refreshed and persisted.
    Updated,
    /// 304: cached metadata still current; `last_fetched_at` bumped.
    NotModified,
    /// Not a GitHub repo (`host_type` != github or an unparseable URL); skipped.
    Skipped,
    /// Engine-level failures, surfaced for E-05 to wrap.
    NetworkLost,
    NotFound,
    RateLimited,
}

/// The full result of [`refresh_one`]: the [`RefreshOutcome`] plus the rate-limit budget
/// observed when the network was actually reached (`Some` on a 200/304; `None` when served
/// from cache, skipped, or on a transport failure that carries no budget). The deferred
/// refresh-pass orchestrator feeds `rate_limit` to [`should_backoff`] to decide whether to
/// pause the remaining repos (BL-NI-15c); designing the orchestration cadence itself is the
/// edge-wiring effort's job.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefreshReport {
    pub outcome: RefreshOutcome,
    pub rate_limit: Option<RateLimit>,
    /// A TRANSIENT per-cycle hint: `true` only when THIS 200 refresh's latest-release
    /// sub-fetch was `Unknown` (BL-NI-15a/c), so the repo metadata refreshed but the cached
    /// release could not be confirmed. It is NOT persisted - the cache-hit / 304 / failure
    /// paths report `false` because they did not attempt a release fetch, NOT because the
    /// release is proven fresh. A DURABLE cross-cycle release-freshness boundary (a
    /// `release_last_checked_at` / `release_etag` column the cache/304 paths derive staleness
    /// from, so a flaky release endpoint actually retries despite the repo's 24h window) is
    /// the deferred BL-NI-15b work; until it lands, treat this as a within-pass hint only.
    pub release_stale: bool,
}

// =============================================================================
// The auth seam (AC5).
// =============================================================================

/// Supplies the optional GitHub token. The V1 implementation ([`NoToken`]) always
/// returns `None`, so the client structurally runs the unauthenticated path. The
/// keyring-backed PAT plugs in as a second impl in V1.1 with no client rewrite.
pub trait TokenProvider {
    fn token(&self) -> Option<String>;
}

/// The V1 [`TokenProvider`]: always `None` (the unauthenticated path is the only
/// V1 path).
#[derive(Debug, Clone, Default)]
pub struct NoToken;

impl TokenProvider for NoToken {
    fn token(&self) -> Option<String> {
        // TODO(V1.1): a keyring-backed TokenProvider reads the PAT from Windows
        // Credential Manager / macOS Keychain and flips settings.github_token_present.
        None
    }
}

// =============================================================================
// The transport seam (the mockable HTTP boundary).
// =============================================================================

/// The HTTP boundary. The production impl ([`ReqwestTransport`]) talks to GitHub
/// over reqwest+rustls; tests use a fake returning canned outcomes, so the cache
/// and write logic is exercised with no network.
#[allow(async_fn_in_trait)]
pub trait Transport {
    async fn fetch(
        &self,
        coords: &RepoCoords,
        etag: Option<&str>,
        token: Option<&str>,
    ) -> FetchOutcome;
}

// =============================================================================
// Pure logic (no I/O) - the testable core.
// =============================================================================

/// Parse a repo's GitHub `owner/name` from its stored `remote_origin_url`, given
/// its `host_type`. Returns `None` for a non-GitHub host_type or any URL form that
/// does not resolve. Handles HTTPS (`https://github.com/owner/name(.git)`) and SSH
/// (`git@github.com:owner/name(.git)`, `ssh://git@github.com/owner/name`) forms.
pub fn parse_github_coords(remote_origin_url: &str, host_type: &str) -> Option<RepoCoords> {
    if host_type != "github" {
        return None;
    }
    let url = remote_origin_url.trim();
    // Extract (host, path) from the supported URL shapes, then require the host to
    // be EXACTLY github.com - never a substring of a look-alike host (notgithub.com,
    // evilgithub.com, github.com.evil.com) nor a path segment of some other host
    // (example.com/github.com/...), all of which the old substring match accepted.
    let (host, path) = if let Some((_scheme, rest)) = url.split_once("://") {
        // scheme://[user@]host[:port]/owner/name  (https, http, ssh, git)
        let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
        let host_port = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
        let host = host_port.split_once(':').map_or(host_port, |(h, _)| h);
        (host, path)
    } else if let Some((user_host, path)) = url.split_once(':') {
        // scp-like: [user@]host:owner/name  (no scheme)
        let host = user_host.rsplit_once('@').map_or(user_host, |(_, h)| h);
        (host, path)
    } else {
        return None;
    };
    if host != "github.com" {
        return None;
    }
    let path = path.trim_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    let path = path.trim_end_matches('/');
    let mut parts = path.split('/');
    let owner = parts.next()?;
    let name = parts.next()?;
    if !is_valid_path_segment(owner) || !is_valid_path_segment(name) {
        return None;
    }
    Some(RepoCoords {
        owner: owner.to_string(),
        name: name.to_string(),
    })
}

/// Whether a GitHub owner/repo path segment is well-formed: non-empty, not a
/// dot-only traversal segment, and only ASCII alphanumerics plus `-`, `_`, `.`.
fn is_valid_path_segment(s: &str) -> bool {
    !s.is_empty()
        && s != "."
        && s != ".."
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
}

/// Whether `year` is a Gregorian leap year.
fn is_leap_year(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

/// The number of days in `(year, month)`; `0` for an out-of-range month.
fn days_in_month(year: i64, month: i64) -> i64 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 0,
    }
}

/// Days since 1970-01-01 for a civil `(year, month, day)`, via Howard Hinnant's
/// `days_from_civil` algorithm (no chrono). `None` for an out-of-range month or a
/// day beyond the month's real length, so impossible dates (Feb 31, or Feb 29 in a
/// non-leap year) are rejected rather than silently normalized.
fn days_from_civil(y: i64, m: i64, d: i64) -> Option<i64> {
    if !(1..=12).contains(&m) || d < 1 || d > days_in_month(y, m) {
        return None;
    }
    let y = if m <= 2 { y - 1 } else { y };
    let era = (if y >= 0 { y } else { y - 399 }) / 400;
    let yoe = y - era * 400; // [0, 399]
    let mp = if m > 2 { m - 3 } else { m + 9 }; // Mar = 0 .. Feb = 11
    let doy = (153 * mp + 2) / 5 + d - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    Some(era * 146097 + doe - 719468)
}

/// Parse an ISO-8601 / RFC-3339 UTC timestamp (`2024-01-15T10:30:00Z`) to unix
/// seconds, for the `latest_release_at` INTEGER column. `None` on any unparseable
/// form. Only the UTC (`Z`) form GitHub emits is supported; no chrono dependency.
fn iso8601_to_unix(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    if b.len() < 19 {
        return None;
    }
    // Byte-check the fixed separators (panic-safe, unlike string slicing).
    if b[4] != b'-'
        || b[7] != b'-'
        || !matches!(b[10], b'T' | b't' | b' ')
        || b[13] != b':'
        || b[16] != b':'
    {
        return None;
    }
    let year: i64 = s.get(0..4)?.parse().ok()?;
    let month: i64 = s.get(5..7)?.parse().ok()?;
    let day: i64 = s.get(8..10)?.parse().ok()?;
    let hour: i64 = s.get(11..13)?.parse().ok()?;
    let min: i64 = s.get(14..16)?.parse().ok()?;
    let sec: i64 = s.get(17..19)?.parse().ok()?;
    if hour > 23 || min > 59 || sec > 60 {
        return None;
    }
    // The timezone must be UTC: a trailing "Z" (optionally preceded by a
    // fractional-seconds part ".<digits>"). A numeric offset (+HH:MM) is rejected
    // rather than silently read as UTC, and trailing junk or a missing zone is
    // rejected too - GitHub emits the Z form.
    if !is_utc_zone(s.get(19..)?) {
        return None;
    }
    let days = days_from_civil(year, month, day)?;
    Some(days * 86_400 + hour * 3_600 + min * 60 + sec)
}

/// Whether `tz` is an accepted UTC zone suffix: `Z`/`z`, optionally preceded by a
/// fractional-seconds part (`.<digits>`). Rejects numeric offsets and any junk.
fn is_utc_zone(tz: &str) -> bool {
    if matches!(tz, "Z" | "z") {
        return true;
    }
    if let Some(frac) = tz.strip_prefix('.') {
        if let Some(digits) = frac.strip_suffix(['Z', 'z']) {
            return !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit());
        }
    }
    false
}

/// Map a GitHub REPO JSON into the [`GhMetadata`] / `repo_remote_meta` shape (AC1):
/// `topics` -> `topics_json` (a JSON-array string), plus description / default_branch /
/// archived. The latest release is mapped separately by [`map_release`].
pub fn map_metadata(repo_json: &serde_json::Value) -> GhMetadata {
    let str_field = |v: &serde_json::Value, key: &str| -> Option<String> {
        v.get(key).and_then(|x| x.as_str()).map(|s| s.to_string())
    };
    let is_archived = repo_json
        .get("archived")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let topics_json = repo_json
        .get("topics")
        .and_then(|t| t.as_array())
        .map(|arr| {
            let topics: Vec<String> = arr
                .iter()
                .filter_map(|x| x.as_str().map(|s| s.to_string()))
                .collect();
            serde_json::to_string(&topics).unwrap_or_else(|_| "[]".to_string())
        });
    GhMetadata {
        description: str_field(repo_json, "description"),
        default_branch: str_field(repo_json, "default_branch"),
        topics_json,
        is_archived,
    }
}

/// Map a `/releases/latest` JSON body into a [`GhRelease`]: `tag_name` -> `tag`,
/// `published_at` (ISO-8601) -> unix `published_at`, `html_url` -> `url`.
pub fn map_release(release_json: &serde_json::Value) -> GhRelease {
    let str_field = |key: &str| -> Option<String> {
        release_json
            .get(key)
            .and_then(|x| x.as_str())
            .map(|s| s.to_string())
    };
    GhRelease {
        tag: str_field("tag_name"),
        published_at: str_field("published_at")
            .as_deref()
            .and_then(iso8601_to_unix),
        url: str_field("html_url"),
    }
}

/// Whether a repo fetched at `last_fetched_at` is still inside the refresh window
/// at `now` (so it is served from cache with no network call, AC3). `None`
/// (never fetched) is always out of the window.
pub fn is_within_refresh_window(last_fetched_at: Option<i64>, now: i64) -> bool {
    match last_fetched_at {
        None => false,
        Some(last) => now - last < REFRESH_WINDOW_SECS,
    }
}

/// Whether to back off given the rate-limit budget (AC4): remaining at or below
/// [`RATE_LIMIT_BACKOFF_PERCENT`] of the limit. A non-positive limit never backs
/// off (no budget information).
pub fn should_backoff(rate_limit: &RateLimit) -> bool {
    if rate_limit.limit <= 0 {
        return false;
    }
    // The backoff floor is RATE_LIMIT_BACKOFF_PERCENT of the limit (integer). The
    // saturating mul avoids overflow when a missing header defaults remaining high.
    let floor = rate_limit.limit.saturating_mul(RATE_LIMIT_BACKOFF_PERCENT) / 100;
    rate_limit.remaining <= floor
}

// =============================================================================
// The refresh entry point (orchestration over the seams + the DB).
// =============================================================================

/// Refresh one repo's GitHub metadata: the core function the future
/// `repo_refresh_metadata(id) -> RepoDetail` command wraps (the command shell is
/// E-06/src-tauri, not this effort).
///
/// Flow: resolve the repo's coords (skip non-GitHub); if it is inside the refresh
/// window, serve from cache (no network); else fetch with the stored ETag as
/// `If-None-Match` and the token from the seam. On 200, persist the metadata +
/// etag + observed sha + `last_fetched_at`; on 304, bump `last_fetched_at` only
/// (cached metadata intact, AC6); on a network/not-found/rate-limited outcome,
/// return it without corrupting the cached row. `AppError` only on a DB failure.
pub async fn refresh_one<T: Transport, P: TokenProvider>(
    pool: &SqlitePool,
    transport: &T,
    tokens: &P,
    repo_id: i64,
    now: i64,
) -> Result<RefreshReport, AppError> {
    // 1. Resolve the repo's coords; a missing id is NotFound, a non-GitHub repo
    //    (or unparseable URL) is a clean skip - never a network call.
    let repo_row = sqlx::query("SELECT remote_origin_url, host_type FROM repos WHERE id = ?")
        .bind(repo_id)
        .fetch_optional(pool)
        .await?;
    let Some(repo_row) = repo_row else {
        return Err(AppError::NotFound {
            entity: format!("repo {repo_id}"),
        });
    };
    let remote: Option<String> = repo_row.try_get("remote_origin_url")?;
    let host_type: String = repo_row.try_get("host_type")?;
    let Some(coords) = remote
        .as_deref()
        .and_then(|u| parse_github_coords(u, &host_type))
    else {
        return Ok(RefreshReport {
            outcome: RefreshOutcome::Skipped,
            rate_limit: None,
            release_stale: false,
        });
    };

    // 2. Read the cached ETag + last_fetched_at (a repo may have no meta row yet).
    let meta_row =
        sqlx::query("SELECT etag, last_fetched_at FROM repo_remote_meta WHERE repo_id = ?")
            .bind(repo_id)
            .fetch_optional(pool)
            .await?;
    let (etag, last_fetched_at): (Option<String>, Option<i64>) = match &meta_row {
        Some(r) => (r.try_get("etag")?, r.try_get("last_fetched_at")?),
        None => (None, None),
    };

    // 3. Inside the ~24h refresh window -> serve from cache, no network (AC3).
    if is_within_refresh_window(last_fetched_at, now) {
        return Ok(RefreshReport {
            outcome: RefreshOutcome::Cached,
            rate_limit: None,
            release_stale: false,
        });
    }

    // 4. Fetch with the stored ETag as If-None-Match and the seam's token (V1: None).
    let token = tokens.token();
    let outcome = transport
        .fetch(&coords, etag.as_deref(), token.as_deref())
        .await;

    // 5. Persist per the outcome (AC6): a 200 rewrites everything; a 304 bumps only
    //    last_fetched_at; a network/not-found/rate-limited result leaves the row.
    match outcome {
        FetchOutcome::Modified {
            metadata,
            etag: new_etag,
            observed_sha,
            rate_limit,
            release,
        } => {
            // All metadata writes for a 200 run in ONE transaction so the freshness
            // markers (last_fetched_at, etag) cannot advance unless the release write is
            // also durable (BL-NI-15a atomicity). Otherwise a release-write failure could
            // leave the repo marked freshly-fetched with a stale release, which the 24h
            // window would then hide.
            let mut tx = pool.begin().await?;
            // default_branch lives on repos; the rest on repo_remote_meta.
            sqlx::query(
                "UPDATE repos SET default_branch = COALESCE(?, default_branch) WHERE id = ?",
            )
            .bind(&metadata.default_branch)
            .bind(repo_id)
            .execute(&mut *tx)
            .await?;
            // The repo-RESOURCE columns are always authoritative on a 200. The release
            // columns are written separately (next) so a failed latest-release sub-fetch
            // never erases the cached release (BL-NI-15a).
            sqlx::query(
                "INSERT INTO repo_remote_meta \
                 (repo_id, description, topics_json, is_archived, last_remote_sha, \
                  last_fetched_at, etag) \
                 VALUES (?, ?, ?, ?, ?, ?, ?) \
                 ON CONFLICT(repo_id) DO UPDATE SET \
                  description = excluded.description, topics_json = excluded.topics_json, \
                  is_archived = excluded.is_archived, last_remote_sha = excluded.last_remote_sha, \
                  last_fetched_at = excluded.last_fetched_at, etag = excluded.etag",
            )
            .bind(repo_id)
            .bind(&metadata.description)
            .bind(&metadata.topics_json)
            .bind(metadata.is_archived as i64)
            .bind(&observed_sha)
            .bind(now)
            .bind(&new_etag)
            .execute(&mut *tx)
            .await?;
            // Release columns, by the conclusive state of the sub-fetch (BL-NI-15a):
            // Found writes the release, NoRelease clears a deleted one, Unknown preserves
            // the cached fields and is surfaced as release_stale.
            let release_stale = match &release {
                ReleaseState::Found(rel) => {
                    sqlx::query(
                        "UPDATE repo_remote_meta SET latest_release_tag = ?, \
                         latest_release_at = ?, latest_release_url = ? WHERE repo_id = ?",
                    )
                    .bind(&rel.tag)
                    .bind(rel.published_at)
                    .bind(&rel.url)
                    .bind(repo_id)
                    .execute(&mut *tx)
                    .await?;
                    false
                }
                ReleaseState::NoRelease => {
                    sqlx::query(
                        "UPDATE repo_remote_meta SET latest_release_tag = NULL, \
                         latest_release_at = NULL, latest_release_url = NULL WHERE repo_id = ?",
                    )
                    .bind(repo_id)
                    .execute(&mut *tx)
                    .await?;
                    false
                }
                ReleaseState::Unknown => true,
            };
            tx.commit().await?;
            Ok(RefreshReport {
                outcome: RefreshOutcome::Updated,
                rate_limit: Some(rate_limit),
                release_stale,
            })
        }
        FetchOutcome::NotModified { rate_limit } => {
            sqlx::query(
                "INSERT INTO repo_remote_meta (repo_id, last_fetched_at) VALUES (?, ?) \
                 ON CONFLICT(repo_id) DO UPDATE SET last_fetched_at = excluded.last_fetched_at",
            )
            .bind(repo_id)
            .bind(now)
            .execute(pool)
            .await?;
            Ok(RefreshReport {
                outcome: RefreshOutcome::NotModified,
                rate_limit: Some(rate_limit),
                release_stale: false,
            })
        }
        FetchOutcome::NetworkLost => Ok(RefreshReport {
            outcome: RefreshOutcome::NetworkLost,
            rate_limit: None,
            release_stale: false,
        }),
        FetchOutcome::NotFound => Ok(RefreshReport {
            outcome: RefreshOutcome::NotFound,
            rate_limit: None,
            release_stale: false,
        }),
        FetchOutcome::RateLimited { rate_limit } => Ok(RefreshReport {
            outcome: RefreshOutcome::RateLimited,
            // Carry the budget (incl. reset_at) so the edge can raise an honest
            // AppError::RateLimited { reset_at } and the future refresh-pass
            // orchestrator can time its resume.
            rate_limit: Some(rate_limit),
            release_stale: false,
        }),
    }
}

// =============================================================================
// The production transport (reqwest + RUSTLS). Not exercised by unit tests; the
// seam is faked there. AC2 hygiene is enforced by the cargo-tree gate.
// =============================================================================

/// The production [`Transport`]: GitHub REST over reqwest configured with RUSTLS
/// (no OpenSSL/native-tls). Does the conditional repo GET (sending `If-None-Match`
/// and the optional token), then the latest-release GET on a 200, mapping the
/// result via [`map_metadata`] and reading the ETag + rate-limit headers.
pub struct ReqwestTransport {
    client: reqwest::Client,
}

impl ReqwestTransport {
    /// Build the client with the required User-Agent. reqwest resolves to rustls
    /// (the crate pins `default-features = false` + `rustls-tls-webpki-roots`), so
    /// no OpenSSL is pulled.
    pub fn new() -> Result<ReqwestTransport, AppError> {
        let client = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .build()
            .map_err(|e| AppError::Unexpected {
                context: format!("failed to build the GitHub HTTP client: {e}"),
            })?;
        Ok(ReqwestTransport { client })
    }

    fn rate_limit_from(headers: &reqwest::header::HeaderMap) -> RateLimit {
        let read = |name: &str, default: i64| -> i64 {
            headers
                .get(name)
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<i64>().ok())
                .unwrap_or(default)
        };
        RateLimit {
            // Default remaining high so a missing header never trips backoff.
            remaining: read("x-ratelimit-remaining", i64::MAX),
            limit: read("x-ratelimit-limit", 60),
            // `0` when the header is absent (the unauthenticated path still sends it).
            reset_at: read("x-ratelimit-reset", 0),
        }
    }
}

impl Transport for ReqwestTransport {
    async fn fetch(
        &self,
        coords: &RepoCoords,
        etag: Option<&str>,
        token: Option<&str>,
    ) -> FetchOutcome {
        let repo_url = format!("{GITHUB_API_BASE}/repos/{}/{}", coords.owner, coords.name);
        let mut req = self
            .client
            .get(&repo_url)
            .header("Accept", "application/vnd.github+json");
        if let Some(tag) = etag {
            req = req.header("If-None-Match", tag);
        }
        if let Some(tok) = token {
            req = req.header("Authorization", format!("Bearer {tok}"));
        }

        let resp = match req.send().await {
            Ok(r) => r,
            Err(_) => return FetchOutcome::NetworkLost,
        };
        let rate_limit = Self::rate_limit_from(resp.headers());
        let status = resp.status();

        if status == reqwest::StatusCode::NOT_MODIFIED {
            return FetchOutcome::NotModified { rate_limit };
        }
        if status == reqwest::StatusCode::NOT_FOUND {
            return FetchOutcome::NotFound;
        }
        if status == reqwest::StatusCode::FORBIDDEN && rate_limit.remaining <= 0 {
            return FetchOutcome::RateLimited { rate_limit };
        }
        if !status.is_success() {
            return FetchOutcome::NetworkLost;
        }

        let new_etag = resp
            .headers()
            .get("etag")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());
        let repo_json: serde_json::Value = match resp.json().await {
            Ok(v) => v,
            Err(_) => return FetchOutcome::NetworkLost,
        };

        // The latest release is a separate endpoint; a 404 means "no releases".
        let release_url = format!(
            "{GITHUB_API_BASE}/repos/{}/{}/releases/latest",
            coords.owner, coords.name
        );
        // The release sub-request MUST use the SAME auth context as the repo request
        // (Codex review): otherwise, once the V1.1 PAT lands, a private repo could be
        // fetched WITH the token but its release endpoint hit WITHOUT it, and that 404
        // ("inaccessible", not "no release") would be misread as NoRelease and wrongly
        // CLEAR the cached release. Sending the same token makes a 404 authoritative,
        // because the repo was proven accessible under that same context.
        let mut release_req = self
            .client
            .get(&release_url)
            .header("Accept", "application/vnd.github+json");
        if let Some(tok) = token {
            release_req = release_req.header("Authorization", format!("Bearer {tok}"));
        }
        let release: ReleaseState = match release_req.send().await {
            // 200: a release exists - parse it. A parse failure is NOT authoritative, so
            // it is Unknown (preserve any cached release) rather than a spurious "none".
            Ok(r) if r.status().is_success() => match r.json::<serde_json::Value>().await {
                Ok(v) => ReleaseState::Found(map_release(&v)),
                Err(_) => ReleaseState::Unknown,
            },
            // 404 under the same verified auth context: GitHub confirmed the repo has no
            // latest release - authoritative None.
            Ok(r) if r.status() == reqwest::StatusCode::NOT_FOUND => ReleaseState::NoRelease,
            // Any other status (incl. a rate-limited 403) or a transport error: do not
            // trust it - preserve the cached release fields (BL-NI-15a). Sending the
            // release endpoint its own conditional/rate-limit handling is the deferred
            // wiring work (BL-NI-15b).
            _ => ReleaseState::Unknown,
        };

        let metadata = map_metadata(&repo_json);
        // The observed SHA (last_remote_sha) is the default branch HEAD; a precise
        // value needs a separate commits call. V1 leaves it None when not cheaply
        // available; a commits-based fill is a documented refinement (backlog).
        let observed_sha: Option<String> = None;

        FetchOutcome::Modified {
            metadata,
            etag: new_etag,
            observed_sha,
            rate_limit,
            release,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use tempfile::TempDir;

    // --- parse_github_coords (AC1) ------------------------------------------

    #[test]
    fn parse_coords_https_form() {
        assert_eq!(
            parse_github_coords("https://github.com/rust-lang/rust", "github"),
            Some(RepoCoords {
                owner: "rust-lang".into(),
                name: "rust".into()
            })
        );
    }

    #[test]
    fn parse_coords_https_with_dot_git_suffix() {
        assert_eq!(
            parse_github_coords("https://github.com/owner/name.git", "github"),
            Some(RepoCoords {
                owner: "owner".into(),
                name: "name".into()
            })
        );
    }

    #[test]
    fn parse_coords_ssh_scp_form() {
        assert_eq!(
            parse_github_coords("git@github.com:owner/name.git", "github"),
            Some(RepoCoords {
                owner: "owner".into(),
                name: "name".into()
            })
        );
    }

    #[test]
    fn parse_coords_ssh_url_form() {
        assert_eq!(
            parse_github_coords("ssh://git@github.com/owner/name", "github"),
            Some(RepoCoords {
                owner: "owner".into(),
                name: "name".into()
            })
        );
    }

    #[test]
    fn parse_coords_skips_non_github_host_type() {
        assert_eq!(
            parse_github_coords("https://github.com/owner/name", "unknown"),
            None,
            "a non-github host_type is skipped even if the URL looks like github"
        );
    }

    #[test]
    fn parse_coords_rejects_unparseable() {
        assert_eq!(parse_github_coords("not a url", "github"), None);
        assert_eq!(
            parse_github_coords("https://gitlab.com/owner/name", "github"),
            None,
            "a non-github.com host does not resolve"
        );
        assert_eq!(
            parse_github_coords("https://github.com/owner", "github"),
            None,
            "a URL without owner/name does not resolve"
        );
    }

    #[test]
    fn parse_coords_rejects_look_alike_hosts() {
        // Codex E-10 review: the host must be EXACTLY github.com, never a substring
        // of a look-alike host nor a path segment of some other host - otherwise a
        // non-GitHub remote gets enriched with unrelated GitHub metadata.
        for url in [
            "git@notgithub.com:owner/name.git",
            "https://example.com/github.com/owner/name",
            "https://github.com.evil.com/owner/name",
            "https://evilgithub.com/owner/name",
            "git@github.com.evil.com:owner/name",
        ] {
            assert_eq!(
                parse_github_coords(url, "github"),
                None,
                "a look-alike host must be rejected: {url}"
            );
        }
    }

    #[test]
    fn parse_coords_ssh_url_with_port() {
        assert_eq!(
            parse_github_coords("ssh://git@github.com:22/owner/name", "github"),
            Some(RepoCoords {
                owner: "owner".into(),
                name: "name".into()
            }),
            "an ssh url with an explicit port resolves (the port is stripped)"
        );
    }

    #[test]
    fn parse_coords_rejects_invalid_segments() {
        // A query/fragment or a path-traversal segment must not resolve.
        assert_eq!(
            parse_github_coords("https://github.com/owner/name?x=1", "github"),
            None,
            "a query suffix on the name is rejected"
        );
        assert_eq!(
            parse_github_coords("https://github.com/../etc/passwd", "github"),
            None,
            "a path-traversal segment is rejected"
        );
    }

    // --- iso8601_to_unix + days_from_civil ----------------------------------

    #[test]
    fn iso8601_epoch_is_zero() {
        assert_eq!(iso8601_to_unix("1970-01-01T00:00:00Z"), Some(0));
    }

    #[test]
    fn iso8601_known_timestamp() {
        // 2024-01-15T10:30:00Z = 1705314600 (verified against a unix epoch table).
        assert_eq!(iso8601_to_unix("2024-01-15T10:30:00Z"), Some(1_705_314_600));
    }

    #[test]
    fn iso8601_rejects_garbage() {
        assert_eq!(iso8601_to_unix("not-a-date"), None);
        assert_eq!(iso8601_to_unix(""), None);
        assert_eq!(iso8601_to_unix("2024-13-01T00:00:00Z"), None);
    }

    #[test]
    fn iso8601_rejects_invalid_calendar_dates() {
        // Codex E-10 review: impossible dates must be rejected, not normalized.
        assert_eq!(
            iso8601_to_unix("2023-02-29T00:00:00Z"),
            None,
            "Feb 29 in a non-leap year does not exist"
        );
        assert_eq!(
            iso8601_to_unix("2024-02-31T00:00:00Z"),
            None,
            "Feb 31 never exists"
        );
        assert_eq!(
            iso8601_to_unix("2024-04-31T00:00:00Z"),
            None,
            "April has 30 days"
        );
        assert_eq!(iso8601_to_unix("2024-00-10T00:00:00Z"), None, "month 0");
        assert_eq!(iso8601_to_unix("2024-01-00T00:00:00Z"), None, "day 0");
    }

    #[test]
    fn iso8601_accepts_valid_leap_day() {
        // Feb 29 2024 (a leap year) is valid: 2024-02-29T00:00:00Z = 1709164800.
        assert_eq!(iso8601_to_unix("2024-02-29T00:00:00Z"), Some(1_709_164_800));
    }

    #[test]
    fn iso8601_requires_utc_z_and_rejects_offsets_and_junk() {
        // Codex E-10 review: a numeric offset must NOT be silently read as UTC, and
        // trailing junk / a missing timezone must be rejected.
        assert_eq!(
            iso8601_to_unix("2024-01-15T10:30:00+02:00"),
            None,
            "a +02:00 offset is rejected, not silently treated as UTC"
        );
        assert_eq!(
            iso8601_to_unix("2024-01-15T10:30:00Zgarbage"),
            None,
            "trailing junk after Z is rejected"
        );
        assert_eq!(
            iso8601_to_unix("2024-01-15T10:30:00"),
            None,
            "a missing timezone is rejected"
        );
        // A fractional-seconds UTC form is accepted (the fraction is truncated).
        assert_eq!(
            iso8601_to_unix("2024-01-15T10:30:00.000Z"),
            Some(1_705_314_600),
            "fractional seconds before Z are accepted"
        );
    }

    // --- map_metadata (AC1) -------------------------------------------------

    #[test]
    fn map_metadata_full_mapping() {
        let repo = serde_json::json!({
            "description": "a repo",
            "default_branch": "main",
            "archived": true,
            "topics": ["rust", "cli"]
        });
        let m = map_metadata(&repo);
        assert_eq!(m.description.as_deref(), Some("a repo"));
        assert_eq!(m.default_branch.as_deref(), Some("main"));
        assert!(m.is_archived);
        // topics serialize to a JSON-array string.
        let topics: Vec<String> = serde_json::from_str(m.topics_json.as_deref().unwrap()).unwrap();
        assert_eq!(topics, vec!["rust", "cli"]);
    }

    #[test]
    fn map_release_maps_tag_published_and_url() {
        let release = serde_json::json!({
            "tag_name": "v1.2.3",
            "published_at": "2024-01-15T10:30:00Z",
            "html_url": "https://github.com/o/n/releases/tag/v1.2.3"
        });
        let r = map_release(&release);
        assert_eq!(r.tag.as_deref(), Some("v1.2.3"));
        assert_eq!(r.published_at, Some(1_705_314_600));
        assert_eq!(
            r.url.as_deref(),
            Some("https://github.com/o/n/releases/tag/v1.2.3")
        );
    }

    #[test]
    fn map_release_missing_fields_are_none() {
        let r = map_release(&serde_json::json!({}));
        assert_eq!(r.tag, None);
        assert_eq!(r.published_at, None);
        assert_eq!(r.url, None);
    }

    // --- is_within_refresh_window (AC3) -------------------------------------

    #[test]
    fn refresh_window_logic() {
        let now = 1_000_000;
        assert!(
            !is_within_refresh_window(None, now),
            "never fetched -> out of window"
        );
        assert!(
            is_within_refresh_window(Some(now - REFRESH_WINDOW_SECS + 1), now),
            "fetched < 24h ago -> inside window (serve cache)"
        );
        assert!(
            !is_within_refresh_window(Some(now - REFRESH_WINDOW_SECS), now),
            "fetched exactly 24h ago -> out of window (refresh)"
        );
        assert!(
            !is_within_refresh_window(Some(now - REFRESH_WINDOW_SECS - 1), now),
            "fetched > 24h ago -> out of window"
        );
    }

    // --- should_backoff (AC4) -----------------------------------------------

    #[test]
    fn backoff_at_or_below_ten_percent() {
        // limit 60 (unauthenticated): 10% = 6.
        assert!(
            !should_backoff(&RateLimit {
                remaining: 7,
                limit: 60,
                reset_at: 0
            }),
            "above 10% -> no backoff"
        );
        assert!(
            should_backoff(&RateLimit {
                remaining: 6,
                limit: 60,
                reset_at: 0
            }),
            "at 10% -> backoff"
        );
        assert!(
            should_backoff(&RateLimit {
                remaining: 0,
                limit: 60,
                reset_at: 0
            }),
            "exhausted -> backoff"
        );
        assert!(
            !should_backoff(&RateLimit {
                remaining: 0,
                limit: 0,
                reset_at: 0
            }),
            "no limit info -> no backoff"
        );
    }

    #[test]
    fn rate_limit_from_parses_remaining_limit_and_reset() {
        // The transport parses X-RateLimit-Reset into reset_at (alongside
        // remaining/limit) so a rate-limited outcome carries an honest reset time.
        let mut h = reqwest::header::HeaderMap::new();
        h.insert("x-ratelimit-remaining", "0".parse().unwrap());
        h.insert("x-ratelimit-limit", "60".parse().unwrap());
        h.insert("x-ratelimit-reset", "1700000000".parse().unwrap());
        let rl = ReqwestTransport::rate_limit_from(&h);
        assert_eq!(rl.remaining, 0);
        assert_eq!(rl.limit, 60);
        assert_eq!(
            rl.reset_at, 1_700_000_000,
            "reset is parsed from the header"
        );
    }

    #[tokio::test]
    async fn refresh_rate_limited_surfaces_the_budget_and_reset() {
        // A rate-limited fetch must carry the budget (incl. reset_at) back to the
        // caller so the edge can raise an honest AppError::RateLimited { reset_at },
        // not just the bare outcome.
        let tmp = TempDir::new().unwrap();
        let pool = fresh_pool(tmp.path()).await;
        let id = seed_github_repo(&pool).await;
        let t = FakeTransport::new(FetchOutcome::RateLimited {
            rate_limit: RateLimit {
                remaining: 0,
                limit: 60,
                reset_at: 1_700_000_000,
            },
        });
        let out = refresh_one(&pool, &t, &NoToken, id, 5000).await.unwrap();
        assert_eq!(out.outcome, RefreshOutcome::RateLimited);
        assert_eq!(
            out.rate_limit,
            Some(RateLimit {
                remaining: 0,
                limit: 60,
                reset_at: 1_700_000_000
            }),
            "the rate-limited outcome carries the budget + reset back to the caller"
        );
    }

    // --- TokenProvider seam (AC5) -------------------------------------------

    #[test]
    fn v1_token_provider_is_always_none() {
        assert_eq!(NoToken.token(), None, "V1 runs the unauthenticated path");
    }

    #[test]
    fn token_provider_seam_is_real() {
        // A stub provider returning a token proves the seam is real (the keyring
        // PAT is a second impl in V1.1, not a client rewrite).
        struct StubToken;
        impl TokenProvider for StubToken {
            fn token(&self) -> Option<String> {
                Some("ghp_stub".into())
            }
        }
        assert_eq!(StubToken.token().as_deref(), Some("ghp_stub"));
    }

    // --- refresh_one orchestration (AC3, AC6) against a fake transport -------

    struct FakeTransport {
        outcome: std::cell::RefCell<Option<FetchOutcome>>,
        calls: std::cell::RefCell<u32>,
        last_etag: std::cell::RefCell<Option<String>>,
    }
    impl FakeTransport {
        fn new(outcome: FetchOutcome) -> FakeTransport {
            FakeTransport {
                outcome: std::cell::RefCell::new(Some(outcome)),
                calls: std::cell::RefCell::new(0),
                last_etag: std::cell::RefCell::new(None),
            }
        }
    }
    impl Transport for FakeTransport {
        async fn fetch(
            &self,
            _coords: &RepoCoords,
            etag: Option<&str>,
            _token: Option<&str>,
        ) -> FetchOutcome {
            *self.calls.borrow_mut() += 1;
            *self.last_etag.borrow_mut() = etag.map(|s| s.to_string());
            self.outcome.borrow().clone().expect("outcome set")
        }
    }

    async fn fresh_pool(dir: &std::path::Path) -> SqlitePool {
        let pool = db::open_pool(&dir.join("gh-test.db"))
            .await
            .expect("open_pool");
        db::run_migrations(&pool).await.expect("migrations");
        pool
    }

    async fn seed_github_repo(pool: &SqlitePool) -> i64 {
        sqlx::query(
            "INSERT INTO repos (local_name, local_path, remote_origin_url, host_type, created_at) \
             VALUES ('r', 'r', 'https://github.com/owner/name', 'github', 0)",
        )
        .execute(pool)
        .await
        .unwrap()
        .last_insert_rowid()
    }

    fn sample_metadata() -> GhMetadata {
        GhMetadata {
            description: Some("desc".into()),
            default_branch: Some("main".into()),
            topics_json: Some("[\"a\"]".into()),
            is_archived: false,
        }
    }

    #[tokio::test]
    async fn refresh_skips_non_github_repo() {
        let tmp = TempDir::new().unwrap();
        let pool = fresh_pool(tmp.path()).await;
        let id = sqlx::query(
            "INSERT INTO repos (local_name, local_path, host_type, created_at) \
             VALUES ('r', 'r', 'unknown', 0)",
        )
        .execute(&pool)
        .await
        .unwrap()
        .last_insert_rowid();
        let t = FakeTransport::new(FetchOutcome::NotFound);
        let out = refresh_one(&pool, &t, &NoToken, id, 1000).await.unwrap();
        assert_eq!(out.outcome, RefreshOutcome::Skipped);
        assert_eq!(
            *t.calls.borrow(),
            0,
            "a non-github repo never hits the transport"
        );
    }

    #[tokio::test]
    async fn refresh_200_writes_all_metadata_columns() {
        let tmp = TempDir::new().unwrap();
        let pool = fresh_pool(tmp.path()).await;
        let id = seed_github_repo(&pool).await;
        let t = FakeTransport::new(FetchOutcome::Modified {
            metadata: sample_metadata(),
            etag: Some("\"abc\"".into()),
            observed_sha: Some("deadbeef".into()),
            rate_limit: RateLimit {
                remaining: 59,
                limit: 60,
                reset_at: 0,
            },
            release: ReleaseState::Found(GhRelease {
                tag: Some("v1".into()),
                published_at: Some(1000),
                url: Some("https://github.com/owner/name/releases/tag/v1".into()),
            }),
        });
        let out = refresh_one(&pool, &t, &NoToken, id, 5000).await.unwrap();
        assert_eq!(out.outcome, RefreshOutcome::Updated);
        assert!(!out.release_stale, "a Found release is not stale");

        let row = sqlx::query(
            "SELECT m.description, m.topics_json, m.latest_release_tag, m.latest_release_at, \
                    m.latest_release_url, m.is_archived, m.last_remote_sha, m.last_fetched_at, \
                    m.etag, r.default_branch \
             FROM repo_remote_meta m JOIN repos r ON r.id = m.repo_id WHERE m.repo_id = ?",
        )
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            row.try_get::<Option<String>, _>("description")
                .unwrap()
                .as_deref(),
            Some("desc")
        );
        assert_eq!(
            row.try_get::<Option<String>, _>("default_branch")
                .unwrap()
                .as_deref(),
            Some("main")
        );
        assert_eq!(
            row.try_get::<Option<String>, _>("latest_release_tag")
                .unwrap()
                .as_deref(),
            Some("v1")
        );
        assert_eq!(
            row.try_get::<Option<i64>, _>("latest_release_at").unwrap(),
            Some(1000)
        );
        assert_eq!(
            row.try_get::<Option<String>, _>("etag").unwrap().as_deref(),
            Some("\"abc\"")
        );
        assert_eq!(
            row.try_get::<Option<String>, _>("last_remote_sha")
                .unwrap()
                .as_deref(),
            Some("deadbeef")
        );
        assert_eq!(
            row.try_get::<Option<i64>, _>("last_fetched_at").unwrap(),
            Some(5000)
        );
    }

    #[tokio::test]
    async fn refresh_inside_window_serves_cache_without_network() {
        let tmp = TempDir::new().unwrap();
        let pool = fresh_pool(tmp.path()).await;
        let id = seed_github_repo(&pool).await;
        // Seed a recent fetch (last_fetched_at = now-100, inside the 24h window).
        sqlx::query(
            "INSERT INTO repo_remote_meta (repo_id, last_fetched_at, etag) VALUES (?, ?, '\"e\"')",
        )
        .bind(id)
        .bind(9900_i64)
        .execute(&pool)
        .await
        .unwrap();
        let t = FakeTransport::new(FetchOutcome::NetworkLost);
        let out = refresh_one(&pool, &t, &NoToken, id, 10000).await.unwrap();
        assert_eq!(out.outcome, RefreshOutcome::Cached);
        assert_eq!(
            *t.calls.borrow(),
            0,
            "inside the refresh window, no network call"
        );
    }

    #[tokio::test]
    async fn refresh_sends_stored_etag_and_304_bumps_only_last_fetched_at() {
        let tmp = TempDir::new().unwrap();
        let pool = fresh_pool(tmp.path()).await;
        let id = seed_github_repo(&pool).await;
        // Seed an OLD fetch (outside the window) with a stored etag + metadata.
        sqlx::query(
            "INSERT INTO repo_remote_meta (repo_id, description, last_fetched_at, etag) \
             VALUES (?, 'cached desc', ?, '\"stored-etag\"')",
        )
        .bind(id)
        .bind(1_i64)
        .execute(&pool)
        .await
        .unwrap();
        let t = FakeTransport::new(FetchOutcome::NotModified {
            rate_limit: RateLimit {
                remaining: 50,
                limit: 60,
                reset_at: 0,
            },
        });
        let now = 1 + REFRESH_WINDOW_SECS + 10;
        let out = refresh_one(&pool, &t, &NoToken, id, now).await.unwrap();
        assert_eq!(out.outcome, RefreshOutcome::NotModified);
        assert_eq!(
            t.last_etag.borrow().as_deref(),
            Some("\"stored-etag\""),
            "the stored ETag is sent as If-None-Match"
        );
        let row = sqlx::query(
            "SELECT description, last_fetched_at FROM repo_remote_meta WHERE repo_id = ?",
        )
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            row.try_get::<Option<String>, _>("description")
                .unwrap()
                .as_deref(),
            Some("cached desc"),
            "a 304 leaves the cached metadata intact"
        );
        assert_eq!(
            row.try_get::<Option<i64>, _>("last_fetched_at").unwrap(),
            Some(now),
            "a 304 bumps only last_fetched_at"
        );
    }

    #[tokio::test]
    async fn refresh_network_error_does_not_corrupt_cache() {
        let tmp = TempDir::new().unwrap();
        let pool = fresh_pool(tmp.path()).await;
        let id = seed_github_repo(&pool).await;
        sqlx::query(
            "INSERT INTO repo_remote_meta (repo_id, description, last_fetched_at) VALUES (?, 'keep', 1)",
        )
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
        let t = FakeTransport::new(FetchOutcome::NetworkLost);
        let now = 1 + REFRESH_WINDOW_SECS + 10;
        let out = refresh_one(&pool, &t, &NoToken, id, now).await.unwrap();
        assert_eq!(out.outcome, RefreshOutcome::NetworkLost);
        let desc: Option<String> =
            sqlx::query("SELECT description FROM repo_remote_meta WHERE repo_id = ?")
                .bind(id)
                .fetch_one(&pool)
                .await
                .unwrap()
                .try_get("description")
                .unwrap();
        assert_eq!(
            desc.as_deref(),
            Some("keep"),
            "a network error leaves the cached row intact"
        );
    }

    #[tokio::test]
    async fn refresh_200_release_unknown_preserves_cached_release() {
        // BL-NI-15a: a 200 repo fetch whose latest-release SUB-fetch failed
        // (ReleaseInfo::Unknown) must NOT erase the previously-cached release - it
        // preserves the release fields while still refreshing the repo-resource columns.
        let tmp = TempDir::new().unwrap();
        let pool = fresh_pool(tmp.path()).await;
        let id = seed_github_repo(&pool).await;
        sqlx::query(
            "INSERT INTO repo_remote_meta \
             (repo_id, description, latest_release_tag, latest_release_at, latest_release_url, last_fetched_at) \
             VALUES (?, 'old desc', 'v0', 500, 'https://github.com/owner/name/releases/tag/v0', 1)",
        )
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
        let t = FakeTransport::new(FetchOutcome::Modified {
            metadata: GhMetadata {
                description: Some("fresh desc".into()),
                default_branch: Some("main".into()),
                topics_json: Some("[]".into()),
                is_archived: false,
            },
            etag: Some("\"new\"".into()),
            observed_sha: None,
            rate_limit: RateLimit {
                remaining: 59,
                limit: 60,
                reset_at: 0,
            },
            release: ReleaseState::Unknown,
        });
        let now = 1 + REFRESH_WINDOW_SECS + 10;
        let out = refresh_one(&pool, &t, &NoToken, id, now).await.unwrap();
        assert_eq!(out.outcome, RefreshOutcome::Updated);
        assert!(
            out.release_stale,
            "an Unknown release sub-fetch is surfaced as stale (BL-NI-15c)"
        );
        let row = sqlx::query(
            "SELECT description, latest_release_tag, latest_release_at \
             FROM repo_remote_meta WHERE repo_id = ?",
        )
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            row.try_get::<Option<String>, _>("latest_release_tag")
                .unwrap()
                .as_deref(),
            Some("v0"),
            "an Unknown release sub-fetch preserves the cached release tag"
        );
        assert_eq!(
            row.try_get::<Option<i64>, _>("latest_release_at").unwrap(),
            Some(500),
            "an Unknown release sub-fetch preserves the cached release timestamp"
        );
        assert_eq!(
            row.try_get::<Option<String>, _>("description")
                .unwrap()
                .as_deref(),
            Some("fresh desc"),
            "the repo-resource columns still refresh on a 200"
        );
    }

    #[tokio::test]
    async fn refresh_200_release_known_none_clears_cached_release() {
        // BL-NI-15a: a CONFIRMED no-release (404 -> ReleaseInfo::Known with None fields)
        // legitimately clears a stale cached release (e.g. the release was deleted
        // upstream). Known + None is authoritative; only Unknown preserves.
        let tmp = TempDir::new().unwrap();
        let pool = fresh_pool(tmp.path()).await;
        let id = seed_github_repo(&pool).await;
        sqlx::query(
            "INSERT INTO repo_remote_meta \
             (repo_id, latest_release_tag, latest_release_at, last_fetched_at) \
             VALUES (?, 'v0', 500, 1)",
        )
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
        let t = FakeTransport::new(FetchOutcome::Modified {
            metadata: GhMetadata {
                description: Some("d".into()),
                default_branch: None,
                topics_json: None,
                is_archived: false,
            },
            etag: None,
            observed_sha: None,
            rate_limit: RateLimit {
                remaining: 59,
                limit: 60,
                reset_at: 0,
            },
            release: ReleaseState::NoRelease,
        });
        let now = 1 + REFRESH_WINDOW_SECS + 10;
        let out = refresh_one(&pool, &t, &NoToken, id, now).await.unwrap();
        assert!(
            !out.release_stale,
            "a confirmed NoRelease is authoritative, not stale"
        );
        let tag: Option<String> =
            sqlx::query("SELECT latest_release_tag FROM repo_remote_meta WHERE repo_id = ?")
                .bind(id)
                .fetch_one(&pool)
                .await
                .unwrap()
                .try_get("latest_release_tag")
                .unwrap();
        assert_eq!(
            tag, None,
            "a confirmed no-release clears the stale cached release"
        );
    }

    #[tokio::test]
    async fn refresh_surfaces_rate_limit_budget_for_backoff() {
        // BL-NI-15c: refresh_one surfaces the observed rate-limit budget so the deferred
        // refresh-pass orchestrator can call should_backoff and pause. A near-exhausted
        // budget on a 200 must reach the caller (the old code discarded it).
        let tmp = TempDir::new().unwrap();
        let pool = fresh_pool(tmp.path()).await;
        let id = seed_github_repo(&pool).await;
        let t = FakeTransport::new(FetchOutcome::Modified {
            metadata: sample_metadata(),
            etag: None,
            observed_sha: None,
            // 3 of 60 remaining = 5%, below the 10% backoff floor.
            rate_limit: RateLimit {
                remaining: 3,
                limit: 60,
                reset_at: 0,
            },
            release: ReleaseState::NoRelease,
        });
        let out = refresh_one(&pool, &t, &NoToken, id, 5000).await.unwrap();
        assert_eq!(out.outcome, RefreshOutcome::Updated);
        let budget = out
            .rate_limit
            .expect("a 200 surfaces the observed rate-limit budget");
        assert!(
            should_backoff(&budget),
            "a near-exhausted budget reaches the caller so the orchestrator backs off"
        );
    }

    #[tokio::test]
    async fn refresh_from_cache_surfaces_no_budget() {
        // A cache hit makes no network call, so there is no budget to report (None).
        let tmp = TempDir::new().unwrap();
        let pool = fresh_pool(tmp.path()).await;
        let id = seed_github_repo(&pool).await;
        sqlx::query("INSERT INTO repo_remote_meta (repo_id, last_fetched_at) VALUES (?, 9900)")
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();
        let t = FakeTransport::new(FetchOutcome::NetworkLost);
        let out = refresh_one(&pool, &t, &NoToken, id, 10000).await.unwrap();
        assert_eq!(out.outcome, RefreshOutcome::Cached);
        assert_eq!(out.rate_limit, None, "a cache hit reports no budget");
    }
}
