//! github - owned by E-10 (the unauthenticated GitHub metadata client) and
//! extended by E-17 (branch and PR intelligence).
//!
//! Enriches each tracked repo with its GitHub host metadata - description,
//! default branch, latest release (tag/date/URL), topics, the archived flag -
//! AND its pull-request intelligence (open PR count + PRs targeting the default
//! branch). Everything is cached in `repo_remote_meta` with ETag conditional
//! requests, a ~24h refresh clock, and rate-limit backoff. V1 ships the
//! UNAUTHENTICATED path only; the optional PAT plugs in behind the
//! [`TokenProvider`] seam in V1.1.
//!
//! DESIGN: the HTTP boundary is the [`Transport`] seam, so the cache decision, the
//! 200/304 write path, the backoff decision, the URL parse, and the JSON -> DB
//! mapping are all testable as pure logic against a fake transport + an injected
//! clock, with NO live GitHub calls and NO HTTP-mock dependency. The production
//! [`ReqwestTransport`] (reqwest + RUSTLS, no OpenSSL) is the only place the
//! network stack appears and is not exercised by unit tests.
//!
//! E-17 own-cache discipline (BL-NI-15a / BL-NI-15b): the repo resource, the
//! latest-release sub-resource, and the pull-request sub-resource EACH carry their
//! own ETag and last-checked timestamp, so a repo-resource 304 never hides a new
//! release or pull request for the 24h window (BL-NI-15b resolved). A 404/403 on
//! the release sub-resource is authoritative "no release" (it clears); a 404/403
//! on the PR sub-resource is AMBIGUOUS (private / inaccessible) and is treated as
//! Unknown - it PRESERVES the cached counts, never a destructive zero (BL-NI-15a).
//!
//! E-17 request budgeter: [`RateBudgeter`] caps aggregate unauthenticated GitHub
//! usage at [`MAX_REQUESTS_PER_HOUR`] requests per rolling hour (shared with the
//! E-10 enrichment traffic), leaving headroom under the real 60/hour ceiling. The
//! scheduler-driven [`refresh_pass`] refreshes oldest-metadata-first, so a cold
//! 100-repo backfill spreads over several hours by design rather than bursting.
//!
//! Tauri-free; sqlx RUNTIME query API; unix-seconds timestamps (no chrono).

use std::collections::VecDeque;

use sqlx::{Row, SqlitePool};

use crate::error::AppError;

/// The refresh window: a repo fetched within this many seconds is served from
/// cache without a network call (the ~24h clock, AC3).
pub const REFRESH_WINDOW_SECS: i64 = 24 * 60 * 60;

/// Back off when `X-RateLimit-Remaining` is at or below this percent of the limit
/// (AC4). Named so the threshold is one obvious constant.
pub const RATE_LIMIT_BACKOFF_PERCENT: i64 = 10;

/// The hard budget ceiling: aggregate unauthenticated GitHub requests per rolling
/// hour, shared across E-10 enrichment and E-17 PR intelligence (E-17 AC16). Set
/// to HALF the real unauthenticated 60/hour ceiling so a full pass never bursts
/// past it and there is headroom for the manual `repo_refresh_metadata` path.
pub const MAX_REQUESTS_PER_HOUR: usize = 30;

/// The rolling window the budgeter enforces, in seconds (one hour).
pub const RATE_WINDOW_SECS: i64 = 60 * 60;

/// The worst-case network cost of refreshing ONE repo: the repo resource, the
/// latest-release sub-resource, and the pull-request sub-resource. The budgeter
/// only STARTS a repo it can fully fund, so the rolling-hour cap is never exceeded.
pub const MAX_REQUESTS_PER_REPO: usize = 3;

/// The GitHub REST API base. Centralized so the production transport has one
/// place to build URLs.
const GITHUB_API_BASE: &str = "https://api.github.com";

/// The User-Agent GitHub requires on every request.
const USER_AGENT: &str = "RepoSync";

/// The page size for the pulls read: the ONE pulls request per repo per pass reads
/// up to this many open PRs and counts them (open total + default-branch subset)
/// from the body. A repo with more than this many OPEN pull requests undercounts
/// (documented approximation; the precise `Link`-header trick cannot also yield the
/// default-branch subset in one request). GitHub caps `per_page` at 100.
const PULLS_PER_PAGE: usize = 100;

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
/// `repo_remote_meta` shape (E-02). The latest-release and pull-request fields are
/// modeled as their OWN sub-resource fetches ([`ReleaseFetch`] / [`PrFetch`]),
/// because each can fail independently of the repo fetch (BL-NI-15a/b).
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
    /// `AppError::RateLimited { reset_at }` (and the refresh-pass orchestrator can
    /// time its resume) rather than guessing the reset.
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

/// Open pull-request counts for a repo: the total open, and the subset whose base
/// (target) branch is the default branch. E-17 counts, never per-PR detail.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PrCounts {
    pub open: i64,
    pub default_branch: i64,
}

/// One repo-RESOURCE fetch result (the repo GET only - NOT the release or PR
/// sub-resources, which are separate calls with their own ETags). The transport
/// classifies the HTTP outcome; [`refresh_one`] decides what to persist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepoFetch {
    /// 200: fresh metadata, the new ETag, the observed commit SHA, and the budget.
    Modified {
        metadata: GhMetadata,
        etag: Option<String>,
        observed_sha: Option<String>,
        rate_limit: RateLimit,
    },
    /// 304 Not Modified: the cached repo metadata is still current; only the budget.
    NotModified { rate_limit: RateLimit },
    /// A transport/connectivity failure (the cached row must be left intact).
    NetworkLost,
    /// 404: the repo is not found on GitHub.
    NotFound,
    /// Rate-limited: a 403 with remaining 0, or the budget hit the backoff floor.
    RateLimited { rate_limit: RateLimit },
}

/// The conclusive state of the latest-release sub-resource fetch (BL-NI-15a/b). The
/// release payload lives ONLY inside `Modified`, so an "unknown yet carrying a
/// release" state is unrepresentable - the edge cannot persist an untrusted release.
/// This fetch carries its OWN ETag, decoupled from the repo-resource ETag.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReleaseFetch {
    /// 200: a release exists - authoritative, write it (and its new ETag).
    Modified {
        release: GhRelease,
        etag: Option<String>,
        rate_limit: RateLimit,
    },
    /// 304: the cached release is still current; bump only `release_last_checked_at`.
    NotModified { rate_limit: RateLimit },
    /// 404 under the verified auth context - the repo has no release: authoritative,
    /// so a stale cached release is cleared.
    NoRelease { rate_limit: RateLimit },
    /// The sub-fetch FAILED (network / parse / rate-limit / any other status): the
    /// cached release fields are PRESERVED, never overwritten (BL-NI-15a).
    Unknown,
}

/// The conclusive state of the pull-request sub-resource fetch. Its 404/403 handling
/// differs from the release sub-resource: a 404/403 on the pulls endpoint is
/// AMBIGUOUS (private / inaccessible), so it is `Unknown` (preserve the cached
/// counts), never a destructive zero (BL-NI-15a, E-17 AC5). Carries its OWN ETag.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrFetch {
    /// 200: fresh counts - authoritative, write them (and the new ETag).
    Modified {
        counts: PrCounts,
        etag: Option<String>,
        rate_limit: RateLimit,
    },
    /// 304: the cached counts are still current; bump only `pr_last_checked_at`.
    NotModified { rate_limit: RateLimit },
    /// The fetch was ambiguous or failed (404/403/network/parse/other): PRESERVE the
    /// cached counts. A private repo must NEVER read as "0 PRs" (E-17 AC5).
    Unknown,
}

/// The result of [`refresh_one`] for one repo. The network failures are
/// engine-level outcomes E-05 maps to `AppError` variants later; `AppError` here
/// is reserved for DB failures. The outcome reflects the REPO resource; the
/// release/PR sub-resources degrade gracefully via `release_stale` / `pr_stale`.
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

/// The full result of [`refresh_one`]: the [`RefreshOutcome`] plus the rate-limit
/// budget observed when the network was actually reached (`Some` on a 200/304 for
/// any of the three resources; `None` when served from cache, skipped, or on a
/// repo-resource transport failure that carries no budget). The refresh-pass
/// orchestrator feeds `rate_limit` to [`should_backoff`] to decide whether to pause.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefreshReport {
    pub outcome: RefreshOutcome,
    pub rate_limit: Option<RateLimit>,
    /// `true` when THIS pass's latest-release sub-fetch was `Unknown` (BL-NI-15a): the
    /// repo metadata refreshed but the cached release could not be confirmed. With the
    /// decoupled `release_etag` / `release_last_checked_at` (BL-NI-15b), the release
    /// re-checks every window regardless of the repo 200/304, so this is a within-pass
    /// signal, not a permanent block.
    pub release_stale: bool,
    /// `true` when THIS pass's pull-request sub-fetch was `Unknown` (a 404/403 on a
    /// private/inaccessible repo, or a transient failure): the cached PR counts were
    /// PRESERVED, never zeroed (E-17 AC5). The UI renders the last-known counts with
    /// their `pr_last_checked_at` "as of" timestamp.
    pub pr_stale: bool,
    /// How many network requests this refresh actually issued (0 when cached/skipped;
    /// 1 on a repo-resource failure; up to [`MAX_REQUESTS_PER_REPO`] on a 200/304).
    /// The refresh pass charges these against the [`RateBudgeter`].
    pub requests_made: u32,
    /// `true` when this refresh WROTE fresh data the UI cares about: a repo-resource
    /// 200 (`Updated`), a release 200 (`Modified`), or a PR 200 (`Modified`). A 304,
    /// a cache hit, a skip, a preserve-on-`Unknown`, or an authoritative no-release
    /// clear does NOT flip this - the ETags make a steady-state re-check a no-op, so
    /// `changed` is precise, not merely "a network call happened" (finding 3). The
    /// background pass collects the ids of changed repos so the shell can refresh the
    /// open UI exactly for what moved, without an N+1 refetch storm.
    pub changed: bool,
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
///
/// Three methods, one per resource, each ETag-aware under the SAME auth context, so
/// a 304 on any resource is cheap and each sub-resource's freshness is decoupled
/// from the repo-resource ETag (BL-NI-15b).
#[allow(async_fn_in_trait)]
pub trait Transport {
    /// The repo resource GET (`/repos/{owner}/{name}`), sending `repo_etag` as
    /// `If-None-Match`.
    async fn fetch_repo(
        &self,
        coords: &RepoCoords,
        repo_etag: Option<&str>,
        token: Option<&str>,
    ) -> RepoFetch;

    /// The latest-release sub-resource GET (`/releases/latest`), sending
    /// `release_etag` as `If-None-Match`. A 404 is authoritative "no release".
    async fn fetch_release(
        &self,
        coords: &RepoCoords,
        release_etag: Option<&str>,
        token: Option<&str>,
    ) -> ReleaseFetch;

    /// The open pull-request sub-resource GET (`/pulls?state=open`), sending
    /// `pr_etag` as `If-None-Match`. `default_branch` (when known) drives the
    /// default-branch subset count. A 404/403 is Unknown (preserve), never zero.
    async fn fetch_pulls(
        &self,
        coords: &RepoCoords,
        default_branch: Option<&str>,
        pr_etag: Option<&str>,
        token: Option<&str>,
    ) -> PrFetch;
}

// =============================================================================
// The rate budgeter (E-17 AC16) - pure, injected clock.
// =============================================================================

/// A hard request budgeter: caps aggregate unauthenticated GitHub usage at
/// `max_per_window` requests per rolling `window_secs` window (default
/// [`MAX_REQUESTS_PER_HOUR`] / [`RATE_WINDOW_SECS`]). Pure over an injected `now`,
/// so a rolling-hour window is deterministically testable with no wall-clock waits.
///
/// The refresh pass consults [`RateBudgeter::can_spend`] before STARTING a repo and
/// [`RateBudgeter::record`]s the requests it actually issued, so a cold 100-repo
/// backfill spreads over several hours and never bursts past the ceiling (AC16).
#[derive(Debug, Clone)]
pub struct RateBudgeter {
    max_per_window: usize,
    window_secs: i64,
    /// Timestamps (unix seconds) of the requests inside the current window, oldest
    /// at the front. Pruned lazily on each query.
    events: VecDeque<i64>,
}

impl RateBudgeter {
    /// A budgeter with the production limits (30 requests per rolling hour).
    pub fn new() -> RateBudgeter {
        RateBudgeter::with_limits(MAX_REQUESTS_PER_HOUR, RATE_WINDOW_SECS)
    }

    /// A budgeter with explicit limits (for tests / tuning).
    pub fn with_limits(max_per_window: usize, window_secs: i64) -> RateBudgeter {
        RateBudgeter {
            max_per_window,
            window_secs,
            events: VecDeque::new(),
        }
    }

    /// Drop events that have aged out of the rolling window at `now`.
    fn prune(&mut self, now: i64) {
        while let Some(&front) = self.events.front() {
            if front <= now - self.window_secs {
                self.events.pop_front();
            } else {
                break;
            }
        }
    }

    /// Remaining budget in the rolling window at `now`.
    pub fn remaining(&mut self, now: i64) -> usize {
        self.prune(now);
        self.max_per_window.saturating_sub(self.events.len())
    }

    /// Whether at least `count` more requests fit in the window at `now`.
    pub fn can_spend(&mut self, now: i64, count: usize) -> bool {
        self.remaining(now) >= count
    }

    /// Record `count` requests issued at `now` (called after they are made).
    pub fn record(&mut self, now: i64, count: usize) {
        for _ in 0..count {
            self.events.push_back(now);
        }
    }
}

impl Default for RateBudgeter {
    fn default() -> Self {
        RateBudgeter::new()
    }
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

/// Map a `/pulls?state=open` JSON array into [`PrCounts`] (E-17 AC3): `open` is the
/// number of returned pull requests (each entry is one open PR); `default_branch` is
/// the subset whose `base.ref` equals `default_branch` (when the default branch is
/// known). A non-array body yields `(0, 0)` - the transport only maps a 200, so an
/// empty array is a legitimate "genuinely zero open PRs".
pub fn map_pull_counts(pulls_json: &serde_json::Value, default_branch: Option<&str>) -> PrCounts {
    let arr = pulls_json.as_array();
    let open = arr.map(|a| a.len() as i64).unwrap_or(0);
    let default_branch_count = match (arr, default_branch) {
        (Some(a), Some(db)) => a
            .iter()
            .filter(|pr| {
                pr.get("base")
                    .and_then(|b| b.get("ref"))
                    .and_then(|r| r.as_str())
                    == Some(db)
            })
            .count() as i64,
        _ => 0,
    };
    PrCounts {
        open,
        default_branch: default_branch_count,
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

/// Which of a repo's three GitHub resources are due for a network re-check at `now`.
/// Each of the repo resource, the latest-release sub-resource, and the PR sub-resource
/// carries its OWN last-checked timestamp (BL-NI-15b), so the decision is
/// RESOURCE-AWARE: a fresh repo window never hides a stale (or never-fetched)
/// sub-resource. See [`due_resources`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DueResources {
    pub repo: bool,
    pub release: bool,
    pub pr: bool,
}

impl DueResources {
    /// `true` when at least one resource needs a network re-check (so the refresh is
    /// not a pure cache hit).
    pub fn any(&self) -> bool {
        self.repo || self.release || self.pr
    }
}

/// Decide which of a repo's three GitHub resources are due for a network re-check at
/// `now`, given each resource's OWN last-checked timestamp. A resource is due when
/// `force` is set (the manual-refresh path always re-checks), or when its last-checked
/// is missing (never fetched) or older than the refresh window.
///
/// This is the fix for the repo-level cache masking a stale sub-resource (finding 1):
/// a repo enriched by E-10 with a fresh `last_fetched_at` but a NULL
/// `pr_last_checked_at` / `release_last_checked_at` is STILL due for its PR / release
/// fetch, instead of being served wholesale from the repo cache and never fetching
/// PR / release data until the 24h repo window elapses.
pub fn due_resources(
    last_fetched_at: Option<i64>,
    release_last_checked_at: Option<i64>,
    pr_last_checked_at: Option<i64>,
    now: i64,
    force: bool,
) -> DueResources {
    DueResources {
        repo: force || !is_within_refresh_window(last_fetched_at, now),
        release: force || !is_within_refresh_window(release_last_checked_at, now),
        pr: force || !is_within_refresh_window(pr_last_checked_at, now),
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

/// The most conservative (lowest `remaining`) budget among those observed this pass,
/// so the caller's [`should_backoff`] fires on the tightest of the repo/release/PR
/// reads. `None` when no network budget was observed.
fn worst_budget(budgets: &[RateLimit]) -> Option<RateLimit> {
    budgets.iter().copied().min_by_key(|b| b.remaining)
}

// =============================================================================
// The refresh entry point (orchestration over the seams + the DB).
// =============================================================================

/// Refresh one repo's GitHub metadata AND its branch/PR intelligence: the core
/// function the `repo_refresh_metadata(id) -> RepoDetail` command wraps (the command
/// shell is E-06/src-tauri).
///
/// Flow: resolve the repo's coords (skip non-GitHub); decide which of the three
/// resources (repo, latest-release, PR) are DUE using each resource's OWN last-checked
/// timestamp (BL-NI-15b / finding 1) - so a fresh repo window never masks a stale or
/// never-fetched sub-resource. If nothing is due, serve from cache (no network).
/// Otherwise fetch each DUE resource with its own stored ETag as `If-None-Match`; the
/// repo resource is fetched only when it is itself due, so a fresh repo whose PR
/// sub-resource is due re-checks the PR without re-fetching the repo. A 404/403 on the
/// release is authoritative (clears); a 404/403 on the pulls is Unknown (preserves the
/// cached counts, never a destructive zero - BL-NI-15a).
///
/// `force` re-checks EVERY resource regardless of its window (the manual
/// `repo_refresh_metadata` path), so a user Refresh always re-fetches. `AppError` only
/// on a DB failure.
pub async fn refresh_one<T: Transport, P: TokenProvider>(
    pool: &SqlitePool,
    transport: &T,
    tokens: &P,
    repo_id: i64,
    now: i64,
    force: bool,
) -> Result<RefreshReport, AppError> {
    // 1. Resolve the repo's coords; a missing id is NotFound, a non-GitHub repo
    //    (or unparseable URL) is a clean skip - never a network call.
    let repo_row =
        sqlx::query("SELECT remote_origin_url, host_type, default_branch FROM repos WHERE id = ?")
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
    let cached_default_branch: Option<String> = repo_row.try_get("default_branch")?;
    let Some(coords) = remote
        .as_deref()
        .and_then(|u| parse_github_coords(u, &host_type))
    else {
        return Ok(RefreshReport {
            outcome: RefreshOutcome::Skipped,
            rate_limit: None,
            release_stale: false,
            pr_stale: false,
            requests_made: 0,
            changed: false,
        });
    };

    // 2. Read the cached ETags + last-checked timestamps (a repo may have no meta row
    //    yet). Each resource carries its OWN ETag AND its OWN last-checked (BL-NI-15b):
    //    repo, release, and PR, so their freshness is decided independently.
    let meta_row = sqlx::query(
        "SELECT etag, last_fetched_at, release_etag, release_last_checked_at, \
         pr_etag, pr_last_checked_at \
         FROM repo_remote_meta WHERE repo_id = ?",
    )
    .bind(repo_id)
    .fetch_optional(pool)
    .await?;
    #[allow(clippy::type_complexity)]
    let (
        repo_etag,
        last_fetched_at,
        release_etag,
        release_last_checked_at,
        pr_etag,
        pr_last_checked_at,
    ): (
        Option<String>,
        Option<i64>,
        Option<String>,
        Option<i64>,
        Option<String>,
        Option<i64>,
    ) = match &meta_row {
        Some(r) => (
            r.try_get("etag")?,
            r.try_get("last_fetched_at")?,
            r.try_get("release_etag")?,
            r.try_get("release_last_checked_at")?,
            r.try_get("pr_etag")?,
            r.try_get("pr_last_checked_at")?,
        ),
        None => (None, None, None, None, None, None),
    };

    // 3. Resource-aware staleness (finding 1): decide which of repo / release / PR are
    //    due from each resource's OWN last-checked. A fresh repo window no longer
    //    short-circuits the whole refresh to Cached when a sub-resource is stale or
    //    never fetched. If NOTHING is due (and not forced), serve from cache, no
    //    network (AC3).
    let due = due_resources(
        last_fetched_at,
        release_last_checked_at,
        pr_last_checked_at,
        now,
        force,
    );
    if !due.any() {
        return Ok(RefreshReport {
            outcome: RefreshOutcome::Cached,
            rate_limit: None,
            release_stale: false,
            pr_stale: false,
            requests_made: 0,
            changed: false,
        });
    }

    let token = tokens.token();
    let mut requests_made: u32 = 0;
    let mut changed = false;

    // Collect the budgets observed across the reads issued this pass; the report
    // surfaces the MOST conservative one so the edge's `should_backoff` fires on the
    // tightest.
    let mut budgets: Vec<RateLimit> = Vec::with_capacity(MAX_REQUESTS_PER_REPO);

    // 4. The repo resource is fetched ONLY when it is itself due. A fresh repo whose
    //    PR / release sub-resource is due skips the repo fetch entirely (no wasted
    //    round-trip) and serves the repo columns from cache, then still runs the due
    //    sub-fetch(es) below. A repo-resource FAILURE short-circuits: no sub-fetches,
    //    cache left intact.
    let (outcome, effective_default_branch): (RefreshOutcome, Option<String>) = if due.repo {
        let repo_fetch = transport
            .fetch_repo(&coords, repo_etag.as_deref(), token.as_deref())
            .await;
        requests_made += 1;
        match repo_fetch {
            RepoFetch::NetworkLost => {
                return Ok(RefreshReport {
                    outcome: RefreshOutcome::NetworkLost,
                    rate_limit: None,
                    release_stale: false,
                    pr_stale: false,
                    requests_made,
                    changed: false,
                });
            }
            RepoFetch::NotFound => {
                return Ok(RefreshReport {
                    outcome: RefreshOutcome::NotFound,
                    rate_limit: None,
                    release_stale: false,
                    pr_stale: false,
                    requests_made,
                    changed: false,
                });
            }
            RepoFetch::RateLimited { rate_limit } => {
                return Ok(RefreshReport {
                    outcome: RefreshOutcome::RateLimited,
                    // Carry the budget (incl. reset_at) so the edge can raise an honest
                    // AppError::RateLimited { reset_at } and the pass can time its resume.
                    rate_limit: Some(rate_limit),
                    release_stale: false,
                    pr_stale: false,
                    requests_made,
                    changed: false,
                });
            }
            // 4a. A 200 repo fetch rewrites the repo-RESOURCE columns (description /
            //     topics / archived / sha / etag / last_fetched_at) in ONE transaction
            //     so the freshness markers cannot advance without the resource write
            //     being durable. The release/PR columns are written SEPARATELY (below),
            //     each with its own ETag, so a failed sub-fetch never erases them
            //     (BL-NI-15a).
            RepoFetch::Modified {
                metadata,
                etag: new_etag,
                observed_sha,
                rate_limit,
            } => {
                let effective_db = metadata
                    .default_branch
                    .clone()
                    .or_else(|| cached_default_branch.clone());
                let mut tx = pool.begin().await?;
                sqlx::query(
                    "UPDATE repos SET default_branch = COALESCE(?, default_branch) WHERE id = ?",
                )
                .bind(&metadata.default_branch)
                .bind(repo_id)
                .execute(&mut *tx)
                .await?;
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
                tx.commit().await?;
                budgets.push(rate_limit);
                changed = true;
                (RefreshOutcome::Updated, effective_db)
            }
            // 4b. A 304 bumps only last_fetched_at (cached repo metadata intact, AC6).
            //     The release/PR sub-fetches STILL run when due (BL-NI-15b decoupling),
            //     so a repo-304 never suppresses a new release or pull request.
            RepoFetch::NotModified { rate_limit } => {
                sqlx::query(
                    "INSERT INTO repo_remote_meta (repo_id, last_fetched_at) VALUES (?, ?) \
                     ON CONFLICT(repo_id) DO UPDATE SET last_fetched_at = excluded.last_fetched_at",
                )
                .bind(repo_id)
                .bind(now)
                .execute(pool)
                .await?;
                budgets.push(rate_limit);
                (RefreshOutcome::NotModified, cached_default_branch.clone())
            }
        }
    } else {
        // The repo resource is still fresh: serve its columns from cache (no network),
        // but a sub-resource is due, so the due sub-fetch(es) below still run. The
        // outcome reflects the REPO resource, so it is Cached even though this refresh
        // may issue sub-resource requests (finding 1).
        (RefreshOutcome::Cached, cached_default_branch.clone())
    };

    // 5. Latest-release sub-resource, fetched only when DUE, with its OWN ETag
    //    (BL-NI-15b). Decoupled from the repo-resource ETag, so it re-checks on its own
    //    window regardless of the repo result.
    let release_stale = if due.release {
        let release_fetch = transport
            .fetch_release(&coords, release_etag.as_deref(), token.as_deref())
            .await;
        requests_made += 1;
        match release_fetch {
            ReleaseFetch::Modified {
                release,
                etag,
                rate_limit,
            } => {
                budgets.push(rate_limit);
                sqlx::query(
                    "UPDATE repo_remote_meta SET latest_release_tag = ?, latest_release_at = ?, \
                     latest_release_url = ?, release_etag = ?, release_last_checked_at = ? \
                     WHERE repo_id = ?",
                )
                .bind(&release.tag)
                .bind(release.published_at)
                .bind(&release.url)
                .bind(&etag)
                .bind(now)
                .bind(repo_id)
                .execute(pool)
                .await?;
                changed = true;
                false
            }
            ReleaseFetch::NotModified { rate_limit } => {
                budgets.push(rate_limit);
                sqlx::query(
                    "UPDATE repo_remote_meta SET release_last_checked_at = ? WHERE repo_id = ?",
                )
                .bind(now)
                .bind(repo_id)
                .execute(pool)
                .await?;
                false
            }
            ReleaseFetch::NoRelease { rate_limit } => {
                budgets.push(rate_limit);
                sqlx::query(
                    "UPDATE repo_remote_meta SET latest_release_tag = NULL, \
                     latest_release_at = NULL, latest_release_url = NULL, release_etag = NULL, \
                     release_last_checked_at = ? WHERE repo_id = ?",
                )
                .bind(now)
                .bind(repo_id)
                .execute(pool)
                .await?;
                false
            }
            // Unknown: preserve the cached release fields (BL-NI-15a); do NOT advance
            // release_last_checked_at, so the "as of" reflects the last real confirmation.
            ReleaseFetch::Unknown => true,
        }
    } else {
        false
    };

    // 6. Pull-request sub-resource, fetched only when DUE, with its OWN ETag. A 404/403
    //    is Unknown (preserve the cached counts), NEVER a destructive zero (BL-NI-15a,
    //    AC5).
    let pr_stale = if due.pr {
        let pr_fetch = transport
            .fetch_pulls(
                &coords,
                effective_default_branch.as_deref(),
                pr_etag.as_deref(),
                token.as_deref(),
            )
            .await;
        requests_made += 1;
        match pr_fetch {
            PrFetch::Modified {
                counts,
                etag,
                rate_limit,
            } => {
                budgets.push(rate_limit);
                sqlx::query(
                    "UPDATE repo_remote_meta SET open_pr_count = ?, default_branch_pr_count = ?, \
                     pr_etag = ?, pr_last_checked_at = ? WHERE repo_id = ?",
                )
                .bind(counts.open)
                .bind(counts.default_branch)
                .bind(&etag)
                .bind(now)
                .bind(repo_id)
                .execute(pool)
                .await?;
                changed = true;
                false
            }
            PrFetch::NotModified { rate_limit } => {
                budgets.push(rate_limit);
                sqlx::query("UPDATE repo_remote_meta SET pr_last_checked_at = ? WHERE repo_id = ?")
                    .bind(now)
                    .bind(repo_id)
                    .execute(pool)
                    .await?;
                false
            }
            // Unknown: preserve the cached counts; do NOT advance pr_last_checked_at.
            PrFetch::Unknown => true,
        }
    } else {
        false
    };

    Ok(RefreshReport {
        outcome,
        rate_limit: worst_budget(&budgets),
        release_stale,
        pr_stale,
        requests_made,
        changed,
    })
}

/// A shared [`RateBudgeter`] handle: the ONE rolling-hour GitHub request budget that
/// BOTH the background [`refresh_pass`] and the manual `repo_refresh_metadata` command
/// spend against (finding 2). Tauri-free (`Arc<tokio::sync::Mutex<_>>`, the same
/// primitive the scheduler's shared handles use); the shell owns the concrete handle
/// in its `AppState` and hands a clone to the background loop.
pub type SharedBudgeter = std::sync::Arc<tokio::sync::Mutex<RateBudgeter>>;

/// A fresh [`SharedBudgeter`] with the production limits ([`RateBudgeter::new`]).
pub fn shared_budgeter() -> SharedBudgeter {
    std::sync::Arc::new(tokio::sync::Mutex::new(RateBudgeter::new()))
}

/// The result of one budgeted single-repo refresh ([`refresh_one_budgeted`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BudgetedRefresh {
    /// The rolling-hour budget could not fully fund this repo, so NOTHING was
    /// attempted (no network call, no overspend). The repo keeps its last-known
    /// values, which the UI renders with their "as of <time>" staleness marker -
    /// never an error (E-17 degradation: budget exhaustion is not a failure state).
    BudgetExhausted,
    /// The refresh ran; carries its [`RefreshReport`].
    Refreshed(RefreshReport),
}

/// The ONE budgeted entry point BOTH the background [`refresh_pass`] and the manual
/// `repo_refresh_metadata` command route through, so a manual refresh can never race
/// the background pass into overspending the unauthenticated 60/hour ceiling
/// (finding 2). It holds the SHARED budgeter lock for the duration of this ONE repo's
/// refresh: the `can_spend` gate, the network calls, and the `record` all happen under
/// one lock hold, so two callers cannot both pass the gate and overspend, and they
/// cannot double-fetch the SAME repo concurrently (the lock serializes them - no
/// separate per-repo metadata lock is needed). The lock is released between repos, so
/// a manual refresh is never blocked for more than one repo's fetch.
///
/// `force` (the manual path) re-checks every resource regardless of its window
/// (finding 1). The budget gate reserves the WORST case ([`MAX_REQUESTS_PER_REPO`]) so
/// a repo is only started when it can be fully funded, but only the requests actually
/// issued are recorded, so the rolling-hour accounting stays exact.
pub async fn refresh_one_budgeted<T: Transport, P: TokenProvider>(
    pool: &SqlitePool,
    transport: &T,
    tokens: &P,
    budgeter: &SharedBudgeter,
    repo_id: i64,
    now: i64,
    force: bool,
) -> Result<BudgetedRefresh, AppError> {
    let mut budget = budgeter.lock().await;
    if !budget.can_spend(now, MAX_REQUESTS_PER_REPO) {
        return Ok(BudgetedRefresh::BudgetExhausted);
    }
    let report = refresh_one(pool, transport, tokens, repo_id, now, force).await?;
    budget.record(now, report.requests_made as usize);
    Ok(BudgetedRefresh::Refreshed(report))
}

/// The result of one budgeted [`refresh_pass`] over the tracked GitHub repos.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PassReport {
    /// How many repos this pass actually refreshed (called [`refresh_one`] on).
    pub attempted: usize,
    /// Total network requests issued this pass (charged against the budgeter).
    pub requests_made: usize,
    /// `true` when the pass stopped because the rolling-hour budget was exhausted
    /// (the remaining repos keep their last-known values and are picked up next pass).
    pub budget_exhausted: bool,
    /// `true` when a repo returned a rate-limited outcome and the pass backed off.
    pub rate_limited: bool,
    /// The ids of the repos whose GitHub metadata actually CHANGED this pass - a repo
    /// whose fetch wrote fresh data (a repo/release/PR 200), NOT a 304/cached no-op
    /// (finding 3). The shell emits ONE coalesced `repo:metadata-refreshed` for the
    /// whole pass (so the aggregate list refetches exactly once, never an N+1 storm -
    /// the Phase-3 F3 batching discipline) plus a per-repo `repo:state-changed` for
    /// each (so an open repo-detail drawer refreshes for the repo that moved).
    pub changed_repo_ids: Vec<i64>,
}

/// One budgeted refresh pass over the tracked GitHub repos, oldest-metadata-first
/// (round-robin), driven by the scheduler tick (E-17 AC16). Refreshes repos with any
/// DUE resource (repo, release, or PR - each on its own window, so a fresh repo whose
/// PR sub-resource is stale is NOT stranded, finding 1), spending at most
/// [`MAX_REQUESTS_PER_REPO`] per repo through the shared [`SharedBudgeter`] and never
/// starting a repo it cannot fully fund, so aggregate usage stays under
/// [`MAX_REQUESTS_PER_HOUR`] in any rolling hour EVEN when a manual refresh interleaves
/// (finding 2). On budget exhaustion or a rate-limited outcome the pass stops; the
/// remaining repos keep their last-known values (the UI renders them with an "as of"
/// timestamp, never an error).
pub async fn refresh_pass<T: Transport, P: TokenProvider>(
    pool: &SqlitePool,
    transport: &T,
    tokens: &P,
    budgeter: &SharedBudgeter,
    now: i64,
) -> Result<PassReport, AppError> {
    // Oldest-metadata-first across ALL THREE resources: sort by the earliest of the
    // repo/release/PR last-checked timestamps (a never-fetched resource COALESCEs to 0
    // and sorts first), so the most-stale repo is refreshed first and a cold 100-repo
    // backfill spreads across passes.
    let rows = sqlx::query(
        "SELECT r.id AS id, \
                m.last_fetched_at AS lf, \
                m.release_last_checked_at AS rlc, \
                m.pr_last_checked_at AS plc \
         FROM repos r LEFT JOIN repo_remote_meta m ON m.repo_id = r.id \
         WHERE r.host_type = 'github' \
         ORDER BY MIN(COALESCE(m.last_fetched_at, 0), \
                      COALESCE(m.release_last_checked_at, 0), \
                      COALESCE(m.pr_last_checked_at, 0)) ASC, \
                  r.id ASC",
    )
    .fetch_all(pool)
    .await?;

    let mut report = PassReport::default();
    for row in &rows {
        let id: i64 = row.try_get("id")?;
        let lf: Option<i64> = row.try_get("lf")?;
        let rlc: Option<i64> = row.try_get("rlc")?;
        let plc: Option<i64> = row.try_get("plc")?;
        // Resource-aware skip (finding 1): a repo whose repo-resource window is fresh is
        // STILL visited when its release or PR sub-resource is due, so a repo enriched
        // by E-10 (fresh last_fetched_at, NULL PR columns) is not stranded. Only a repo
        // with NOTHING due is skipped (no network).
        if !due_resources(lf, rlc, plc, now, false).any() {
            continue;
        }
        match refresh_one_budgeted(pool, transport, tokens, budgeter, id, now, false).await? {
            // Never START a repo we cannot fully fund, so the rolling-hour cap is never
            // exceeded. Stopping here leaves the repo for the next pass (its last-known
            // values stay put with their staleness timestamp - never an error).
            BudgetedRefresh::BudgetExhausted => {
                report.budget_exhausted = true;
                break;
            }
            BudgetedRefresh::Refreshed(rep) => {
                report.attempted += 1;
                report.requests_made += rep.requests_made as usize;
                if rep.changed {
                    report.changed_repo_ids.push(id);
                }
                if matches!(rep.outcome, RefreshOutcome::RateLimited) {
                    // Back off for the rest of this pass; the reset is in rep.rate_limit.
                    report.rate_limited = true;
                    break;
                }
            }
        }
    }
    Ok(report)
}

// =============================================================================
// The production transport (reqwest + RUSTLS). Not exercised by unit tests; the
// seam is faked there. AC2 hygiene is enforced by the cargo-tree gate.
// =============================================================================

/// The production [`Transport`]: GitHub REST over reqwest configured with RUSTLS
/// (no OpenSSL/native-tls). Does the conditional repo GET, the release GET, and the
/// pulls GET (each sending its own `If-None-Match` and the optional token under the
/// SAME auth context), reading the ETag + rate-limit headers each time.
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

    fn etag_of(headers: &reqwest::header::HeaderMap) -> Option<String> {
        headers
            .get("etag")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string())
    }
}

impl Transport for ReqwestTransport {
    async fn fetch_repo(
        &self,
        coords: &RepoCoords,
        repo_etag: Option<&str>,
        token: Option<&str>,
    ) -> RepoFetch {
        let repo_url = format!("{GITHUB_API_BASE}/repos/{}/{}", coords.owner, coords.name);
        let mut req = self
            .client
            .get(&repo_url)
            .header("Accept", "application/vnd.github+json");
        if let Some(tag) = repo_etag {
            req = req.header("If-None-Match", tag);
        }
        if let Some(tok) = token {
            req = req.header("Authorization", format!("Bearer {tok}"));
        }

        let resp = match req.send().await {
            Ok(r) => r,
            Err(_) => return RepoFetch::NetworkLost,
        };
        let rate_limit = Self::rate_limit_from(resp.headers());
        let status = resp.status();

        if status == reqwest::StatusCode::NOT_MODIFIED {
            return RepoFetch::NotModified { rate_limit };
        }
        if status == reqwest::StatusCode::NOT_FOUND {
            return RepoFetch::NotFound;
        }
        if status == reqwest::StatusCode::FORBIDDEN && rate_limit.remaining <= 0 {
            return RepoFetch::RateLimited { rate_limit };
        }
        if !status.is_success() {
            return RepoFetch::NetworkLost;
        }

        let new_etag = Self::etag_of(resp.headers());
        let repo_json: serde_json::Value = match resp.json().await {
            Ok(v) => v,
            Err(_) => return RepoFetch::NetworkLost,
        };
        let metadata = map_metadata(&repo_json);
        // The observed SHA (last_remote_sha) is the default branch HEAD; a precise
        // value needs a separate commits call. V1 leaves it None when not cheaply
        // available; a commits-based fill is a documented refinement (backlog).
        RepoFetch::Modified {
            metadata,
            etag: new_etag,
            observed_sha: None,
            rate_limit,
        }
    }

    async fn fetch_release(
        &self,
        coords: &RepoCoords,
        release_etag: Option<&str>,
        token: Option<&str>,
    ) -> ReleaseFetch {
        let url = format!(
            "{GITHUB_API_BASE}/repos/{}/{}/releases/latest",
            coords.owner, coords.name
        );
        // The release request MUST use the SAME auth context as the repo request
        // (Codex E-10 review): once the V1.1 PAT lands, a private repo fetched WITH the
        // token whose release endpoint is hit WITHOUT it would misread an inaccessible
        // 404 as NoRelease and wrongly CLEAR the cached release. Sending the same token
        // makes a 404 authoritative.
        let mut req = self
            .client
            .get(&url)
            .header("Accept", "application/vnd.github+json");
        if let Some(tag) = release_etag {
            req = req.header("If-None-Match", tag);
        }
        if let Some(tok) = token {
            req = req.header("Authorization", format!("Bearer {tok}"));
        }

        let resp = match req.send().await {
            Ok(r) => r,
            Err(_) => return ReleaseFetch::Unknown,
        };
        let rate_limit = Self::rate_limit_from(resp.headers());
        let status = resp.status();

        if status == reqwest::StatusCode::NOT_MODIFIED {
            return ReleaseFetch::NotModified { rate_limit };
        }
        // 404 under the verified auth context: GitHub confirmed no latest release.
        if status == reqwest::StatusCode::NOT_FOUND {
            return ReleaseFetch::NoRelease { rate_limit };
        }
        // Any other non-success (incl. a rate-limited 403): do not trust it - preserve.
        if !status.is_success() {
            return ReleaseFetch::Unknown;
        }
        let new_etag = Self::etag_of(resp.headers());
        match resp.json::<serde_json::Value>().await {
            Ok(v) => ReleaseFetch::Modified {
                release: map_release(&v),
                etag: new_etag,
                rate_limit,
            },
            // A parse failure is NOT authoritative: Unknown (preserve), not a spurious none.
            Err(_) => ReleaseFetch::Unknown,
        }
    }

    async fn fetch_pulls(
        &self,
        coords: &RepoCoords,
        default_branch: Option<&str>,
        pr_etag: Option<&str>,
        token: Option<&str>,
    ) -> PrFetch {
        let url = format!(
            "{GITHUB_API_BASE}/repos/{}/{}/pulls?state=open&per_page={PULLS_PER_PAGE}",
            coords.owner, coords.name
        );
        let mut req = self
            .client
            .get(&url)
            .header("Accept", "application/vnd.github+json");
        if let Some(tag) = pr_etag {
            req = req.header("If-None-Match", tag);
        }
        if let Some(tok) = token {
            req = req.header("Authorization", format!("Bearer {tok}"));
        }

        let resp = match req.send().await {
            Ok(r) => r,
            Err(_) => return PrFetch::Unknown,
        };
        let rate_limit = Self::rate_limit_from(resp.headers());
        let status = resp.status();

        if status == reqwest::StatusCode::NOT_MODIFIED {
            return PrFetch::NotModified { rate_limit };
        }
        // A 404/403/any-other under the unauthenticated context is AMBIGUOUS
        // (private / inaccessible): Unknown (preserve the cached counts), NEVER a
        // destructive zero (BL-NI-15a, E-17 AC5).
        if !status.is_success() {
            return PrFetch::Unknown;
        }
        let new_etag = Self::etag_of(resp.headers());
        match resp.json::<serde_json::Value>().await {
            Ok(v) => PrFetch::Modified {
                counts: map_pull_counts(&v, default_branch),
                etag: new_etag,
                rate_limit,
            },
            Err(_) => PrFetch::Unknown,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use std::cell::{Cell, RefCell};
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
        assert_eq!(iso8601_to_unix("2024-02-29T00:00:00Z"), Some(1_709_164_800));
    }

    #[test]
    fn iso8601_requires_utc_z_and_rejects_offsets_and_junk() {
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

    // --- map_pull_counts (E-17 AC3) -----------------------------------------

    #[test]
    fn map_pull_counts_counts_open_and_default_branch_subset() {
        // Three open PRs: two target main (the default branch), one targets a feature
        // branch. open = 3, default_branch = 2.
        let pulls = serde_json::json!([
            { "number": 1, "base": { "ref": "main" } },
            { "number": 2, "base": { "ref": "feature-x" } },
            { "number": 3, "base": { "ref": "main" } }
        ]);
        let counts = map_pull_counts(&pulls, Some("main"));
        assert_eq!(counts.open, 3, "three open pull requests");
        assert_eq!(counts.default_branch, 2, "two target the default branch");
    }

    #[test]
    fn map_pull_counts_empty_array_is_genuine_zero() {
        // A 200 with an empty array is a legitimate "zero open PRs" (the transport only
        // maps a 200; a 404/403 is Unknown upstream, never reaching the mapper).
        let counts = map_pull_counts(&serde_json::json!([]), Some("main"));
        assert_eq!(counts, PrCounts::default());
    }

    #[test]
    fn map_pull_counts_unknown_default_branch_yields_zero_subset() {
        // Without a known default branch, the subset count is 0 but the open total
        // still maps.
        let pulls = serde_json::json!([{ "number": 1, "base": { "ref": "main" } }]);
        let counts = map_pull_counts(&pulls, None);
        assert_eq!(counts.open, 1);
        assert_eq!(counts.default_branch, 0);
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

    // --- RateBudgeter (E-17 AC16) -------------------------------------------

    #[test]
    fn budgeter_caps_requests_in_the_rolling_window() {
        let mut b = RateBudgeter::with_limits(30, 3600);
        assert_eq!(b.remaining(1000), 30, "fresh budget is the full cap");
        b.record(1000, 30);
        assert_eq!(b.remaining(1000), 0, "30 recorded -> none left");
        assert!(!b.can_spend(1000, 1), "cannot spend past the cap");
        // Advance past the window: the events age out and the budget refills.
        assert_eq!(b.remaining(1000 + 3600), 30, "the window rolled over");
    }

    #[test]
    fn budgeter_prunes_only_events_older_than_the_window() {
        let mut b = RateBudgeter::with_limits(30, 3600);
        b.record(0, 10); // 10 at t=0
        b.record(1800, 10); // 10 at t=1800
                            // At t=3600 the t=0 events age out (0 <= 3600-3600), the t=1800 stay.
        assert_eq!(
            b.remaining(3600),
            20,
            "only the oldest window of events is pruned"
        );
    }

    // --- TokenProvider seam (AC5) -------------------------------------------

    #[test]
    fn v1_token_provider_is_always_none() {
        assert_eq!(NoToken.token(), None, "V1 runs the unauthenticated path");
    }

    #[test]
    fn token_provider_seam_is_real() {
        struct StubToken;
        impl TokenProvider for StubToken {
            fn token(&self) -> Option<String> {
                Some("ghp_stub".into())
            }
        }
        assert_eq!(StubToken.token().as_deref(), Some("ghp_stub"));
    }

    // --- refresh_one orchestration against a fake transport -------

    /// A fully configurable fake transport: each of the three seam methods returns a
    /// cloned canned outcome and records its call count + the ETag it was sent, so a
    /// test can prove the stored ETag is forwarded and each sub-resource is (or is not)
    /// hit. Defaults: repo Modified with sample metadata, release Unknown, PR Unknown.
    struct FakeTransport {
        repo: RefCell<RepoFetch>,
        release: RefCell<ReleaseFetch>,
        pulls: RefCell<PrFetch>,
        repo_calls: Cell<u32>,
        release_calls: Cell<u32>,
        pulls_calls: Cell<u32>,
        last_repo_etag: RefCell<Option<String>>,
        last_release_etag: RefCell<Option<String>>,
        last_pr_etag: RefCell<Option<String>>,
        last_pr_default_branch: RefCell<Option<String>>,
    }

    impl FakeTransport {
        fn new(repo: RepoFetch, release: ReleaseFetch, pulls: PrFetch) -> FakeTransport {
            FakeTransport {
                repo: RefCell::new(repo),
                release: RefCell::new(release),
                pulls: RefCell::new(pulls),
                repo_calls: Cell::new(0),
                release_calls: Cell::new(0),
                pulls_calls: Cell::new(0),
                last_repo_etag: RefCell::new(None),
                last_release_etag: RefCell::new(None),
                last_pr_etag: RefCell::new(None),
                last_pr_default_branch: RefCell::new(None),
            }
        }
        /// A repo-only fake: the repo outcome under test, sub-resources set to
        /// Unknown (preserve; no sub-resource assertion in that test).
        fn repo_only(repo: RepoFetch) -> FakeTransport {
            FakeTransport::new(repo, ReleaseFetch::Unknown, PrFetch::Unknown)
        }
    }

    impl Transport for FakeTransport {
        async fn fetch_repo(
            &self,
            _coords: &RepoCoords,
            etag: Option<&str>,
            _token: Option<&str>,
        ) -> RepoFetch {
            self.repo_calls.set(self.repo_calls.get() + 1);
            *self.last_repo_etag.borrow_mut() = etag.map(|s| s.to_string());
            self.repo.borrow().clone()
        }
        async fn fetch_release(
            &self,
            _coords: &RepoCoords,
            etag: Option<&str>,
            _token: Option<&str>,
        ) -> ReleaseFetch {
            self.release_calls.set(self.release_calls.get() + 1);
            *self.last_release_etag.borrow_mut() = etag.map(|s| s.to_string());
            self.release.borrow().clone()
        }
        async fn fetch_pulls(
            &self,
            _coords: &RepoCoords,
            default_branch: Option<&str>,
            etag: Option<&str>,
            _token: Option<&str>,
        ) -> PrFetch {
            self.pulls_calls.set(self.pulls_calls.get() + 1);
            *self.last_pr_etag.borrow_mut() = etag.map(|s| s.to_string());
            *self.last_pr_default_branch.borrow_mut() = default_branch.map(|s| s.to_string());
            self.pulls.borrow().clone()
        }
    }

    fn healthy_budget() -> RateLimit {
        RateLimit {
            remaining: 59,
            limit: 60,
            reset_at: 0,
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
            "INSERT INTO repos (local_name, local_path, remote_origin_url, host_type, default_branch, created_at) \
             VALUES ('r', 'r', 'https://github.com/owner/name', 'github', 'main', 0)",
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

    async fn read_meta<T>(pool: &SqlitePool, id: i64, col: &str) -> T
    where
        T: for<'r> sqlx::Decode<'r, sqlx::Sqlite> + sqlx::Type<sqlx::Sqlite> + Send + Unpin,
    {
        let sql = format!("SELECT {col} AS v FROM repo_remote_meta WHERE repo_id = ?");
        sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(id)
            .fetch_one(pool)
            .await
            .unwrap()
            .try_get::<T, _>("v")
            .unwrap()
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
        let t = FakeTransport::repo_only(RepoFetch::NotFound);
        let out = refresh_one(&pool, &t, &NoToken, id, 1000, false)
            .await
            .unwrap();
        assert_eq!(out.outcome, RefreshOutcome::Skipped);
        assert_eq!(out.requests_made, 0);
        assert_eq!(
            t.repo_calls.get(),
            0,
            "a non-github repo never hits the transport"
        );
    }

    #[tokio::test]
    async fn refresh_200_writes_all_metadata_release_and_pr_columns() {
        // A 200 repo fetch plus a Found release plus 200 PR counts writes every column,
        // including the E-17 pull-request columns and each resource's own ETag.
        let tmp = TempDir::new().unwrap();
        let pool = fresh_pool(tmp.path()).await;
        let id = seed_github_repo(&pool).await;
        let t = FakeTransport::new(
            RepoFetch::Modified {
                metadata: sample_metadata(),
                etag: Some("\"repo\"".into()),
                observed_sha: Some("deadbeef".into()),
                rate_limit: healthy_budget(),
            },
            ReleaseFetch::Modified {
                release: GhRelease {
                    tag: Some("v1".into()),
                    published_at: Some(1000),
                    url: Some("https://github.com/owner/name/releases/tag/v1".into()),
                },
                etag: Some("\"rel\"".into()),
                rate_limit: healthy_budget(),
            },
            PrFetch::Modified {
                counts: PrCounts {
                    open: 4,
                    default_branch: 2,
                },
                etag: Some("\"pr\"".into()),
                rate_limit: healthy_budget(),
            },
        );
        let out = refresh_one(&pool, &t, &NoToken, id, 5000, false)
            .await
            .unwrap();
        assert_eq!(out.outcome, RefreshOutcome::Updated);
        assert!(!out.release_stale && !out.pr_stale);
        assert_eq!(out.requests_made, 3, "repo + release + pulls");

        assert_eq!(
            read_meta::<Option<String>>(&pool, id, "description")
                .await
                .as_deref(),
            Some("desc")
        );
        assert_eq!(
            read_meta::<Option<String>>(&pool, id, "latest_release_tag")
                .await
                .as_deref(),
            Some("v1")
        );
        assert_eq!(
            read_meta::<Option<String>>(&pool, id, "etag")
                .await
                .as_deref(),
            Some("\"repo\"")
        );
        assert_eq!(
            read_meta::<Option<String>>(&pool, id, "release_etag")
                .await
                .as_deref(),
            Some("\"rel\"")
        );
        assert_eq!(
            read_meta::<Option<i64>>(&pool, id, "open_pr_count").await,
            Some(4)
        );
        assert_eq!(
            read_meta::<Option<i64>>(&pool, id, "default_branch_pr_count").await,
            Some(2)
        );
        assert_eq!(
            read_meta::<Option<String>>(&pool, id, "pr_etag")
                .await
                .as_deref(),
            Some("\"pr\"")
        );
        assert_eq!(
            read_meta::<Option<i64>>(&pool, id, "pr_last_checked_at").await,
            Some(5000)
        );
        assert_eq!(
            read_meta::<Option<i64>>(&pool, id, "release_last_checked_at").await,
            Some(5000)
        );
        assert_eq!(
            t.last_pr_default_branch.borrow().as_deref(),
            Some("main"),
            "the pulls fetch is told the default branch for the subset count"
        );
    }

    #[tokio::test]
    async fn refresh_inside_window_serves_cache_without_network() {
        let tmp = TempDir::new().unwrap();
        let pool = fresh_pool(tmp.path()).await;
        let id = seed_github_repo(&pool).await;
        // All THREE resources fresh (within the window at now=10000), so NOTHING is due
        // and the refresh is a pure cache hit - no repo, release, or PR network call.
        sqlx::query(
            "INSERT INTO repo_remote_meta \
             (repo_id, last_fetched_at, etag, release_last_checked_at, pr_last_checked_at) \
             VALUES (?, ?, '\"e\"', ?, ?)",
        )
        .bind(id)
        .bind(9900_i64)
        .bind(9900_i64)
        .bind(9900_i64)
        .execute(&pool)
        .await
        .unwrap();
        let t = FakeTransport::repo_only(RepoFetch::NetworkLost);
        let out = refresh_one(&pool, &t, &NoToken, id, 10000, false)
            .await
            .unwrap();
        assert_eq!(out.outcome, RefreshOutcome::Cached);
        assert_eq!(out.requests_made, 0);
        assert!(!out.changed, "a pure cache hit changed nothing");
        assert_eq!(t.repo_calls.get(), 0, "inside the window, no network call");
        assert_eq!(t.release_calls.get(), 0);
        assert_eq!(t.pulls_calls.get(), 0);
    }

    #[tokio::test]
    async fn refresh_sends_each_stored_etag_and_304_bumps_only_last_fetched_at() {
        // The repo, release, and PR ETags are each forwarded as If-None-Match, and a
        // repo 304 leaves the cached metadata intact while still hitting the sub-resources.
        let tmp = TempDir::new().unwrap();
        let pool = fresh_pool(tmp.path()).await;
        let id = seed_github_repo(&pool).await;
        sqlx::query(
            "INSERT INTO repo_remote_meta \
             (repo_id, description, last_fetched_at, etag, release_etag, pr_etag) \
             VALUES (?, 'cached desc', ?, '\"repo-e\"', '\"rel-e\"', '\"pr-e\"')",
        )
        .bind(id)
        .bind(1_i64)
        .execute(&pool)
        .await
        .unwrap();
        let t = FakeTransport::new(
            RepoFetch::NotModified {
                rate_limit: RateLimit {
                    remaining: 50,
                    limit: 60,
                    reset_at: 0,
                },
            },
            ReleaseFetch::NotModified {
                rate_limit: healthy_budget(),
            },
            PrFetch::NotModified {
                rate_limit: healthy_budget(),
            },
        );
        let now = 1 + REFRESH_WINDOW_SECS + 10;
        let out = refresh_one(&pool, &t, &NoToken, id, now, false)
            .await
            .unwrap();
        assert_eq!(out.outcome, RefreshOutcome::NotModified);
        assert_eq!(t.last_repo_etag.borrow().as_deref(), Some("\"repo-e\""));
        assert_eq!(t.last_release_etag.borrow().as_deref(), Some("\"rel-e\""));
        assert_eq!(t.last_pr_etag.borrow().as_deref(), Some("\"pr-e\""));
        assert_eq!(
            read_meta::<Option<String>>(&pool, id, "description")
                .await
                .as_deref(),
            Some("cached desc"),
            "a 304 leaves the cached metadata intact"
        );
        assert_eq!(
            read_meta::<Option<i64>>(&pool, id, "last_fetched_at").await,
            Some(now),
            "a 304 bumps only last_fetched_at"
        );
    }

    #[tokio::test]
    async fn refresh_repo_304_still_refreshes_pull_requests() {
        // BL-NI-15b: a repo-resource 304 must NOT suppress a NEW pull-request refresh -
        // the PR fetch has its own ETag and runs regardless of the repo result.
        let tmp = TempDir::new().unwrap();
        let pool = fresh_pool(tmp.path()).await;
        let id = seed_github_repo(&pool).await;
        sqlx::query(
            "INSERT INTO repo_remote_meta (repo_id, open_pr_count, last_fetched_at, etag) \
             VALUES (?, 1, ?, '\"repo-e\"')",
        )
        .bind(id)
        .bind(1_i64)
        .execute(&pool)
        .await
        .unwrap();
        let t = FakeTransport::new(
            RepoFetch::NotModified {
                rate_limit: healthy_budget(),
            },
            ReleaseFetch::Unknown,
            PrFetch::Modified {
                counts: PrCounts {
                    open: 5,
                    default_branch: 3,
                },
                etag: Some("\"pr-new\"".into()),
                rate_limit: healthy_budget(),
            },
        );
        let now = 1 + REFRESH_WINDOW_SECS + 10;
        let out = refresh_one(&pool, &t, &NoToken, id, now, false)
            .await
            .unwrap();
        assert_eq!(out.outcome, RefreshOutcome::NotModified);
        assert_eq!(
            read_meta::<Option<i64>>(&pool, id, "open_pr_count").await,
            Some(5),
            "a repo-304 does NOT suppress a new PR count (BL-NI-15b decoupling)"
        );
    }

    #[tokio::test]
    async fn refresh_pr_404_or_403_preserves_cached_counts() {
        // E-17 AC5 / BL-NI-15a: an ambiguous 404/403 on the pulls endpoint is Unknown -
        // it PRESERVES the cached counts, never a destructive zero.
        let tmp = TempDir::new().unwrap();
        let pool = fresh_pool(tmp.path()).await;
        let id = seed_github_repo(&pool).await;
        sqlx::query(
            "INSERT INTO repo_remote_meta \
             (repo_id, open_pr_count, default_branch_pr_count, pr_etag, last_fetched_at) \
             VALUES (?, 7, 3, '\"pr-e\"', 1)",
        )
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
        let t = FakeTransport::new(
            RepoFetch::Modified {
                metadata: sample_metadata(),
                etag: Some("\"repo\"".into()),
                observed_sha: None,
                rate_limit: healthy_budget(),
            },
            ReleaseFetch::Unknown,
            // Unknown models the 404/403 the transport maps for the pulls endpoint.
            PrFetch::Unknown,
        );
        let now = 1 + REFRESH_WINDOW_SECS + 10;
        let out = refresh_one(&pool, &t, &NoToken, id, now, false)
            .await
            .unwrap();
        assert!(out.pr_stale, "an Unknown PR fetch is surfaced as stale");
        assert_eq!(
            read_meta::<Option<i64>>(&pool, id, "open_pr_count").await,
            Some(7),
            "a private/inaccessible repo must NOT read as 0 PRs - the cached count is preserved"
        );
        assert_eq!(
            read_meta::<Option<i64>>(&pool, id, "default_branch_pr_count").await,
            Some(3),
            "the default-branch subset count is preserved too"
        );
    }

    #[tokio::test]
    async fn refresh_pr_304_bumps_pr_last_checked_only() {
        // A PR-endpoint 304 bumps only pr_last_checked_at and leaves the counts intact.
        // pr_last_checked_at is seeded stale (=1, out of the window at `now`) so the PR
        // sub-resource is DUE and actually re-checked (finding 1 gating).
        let tmp = TempDir::new().unwrap();
        let pool = fresh_pool(tmp.path()).await;
        let id = seed_github_repo(&pool).await;
        sqlx::query(
            "INSERT INTO repo_remote_meta \
             (repo_id, open_pr_count, default_branch_pr_count, pr_etag, pr_last_checked_at, last_fetched_at) \
             VALUES (?, 2, 1, '\"pr-e\"', 1, 1)",
        )
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
        let t = FakeTransport::new(
            RepoFetch::NotModified {
                rate_limit: healthy_budget(),
            },
            ReleaseFetch::Unknown,
            PrFetch::NotModified {
                rate_limit: healthy_budget(),
            },
        );
        let now = 1 + REFRESH_WINDOW_SECS + 10;
        refresh_one(&pool, &t, &NoToken, id, now, false)
            .await
            .unwrap();
        assert_eq!(
            read_meta::<Option<i64>>(&pool, id, "open_pr_count").await,
            Some(2),
            "a PR 304 leaves the counts intact"
        );
        assert_eq!(
            read_meta::<Option<i64>>(&pool, id, "pr_last_checked_at").await,
            Some(now),
            "a PR 304 bumps only pr_last_checked_at"
        );
    }

    #[tokio::test]
    async fn refresh_network_error_does_not_corrupt_cache_or_hit_subresources() {
        let tmp = TempDir::new().unwrap();
        let pool = fresh_pool(tmp.path()).await;
        let id = seed_github_repo(&pool).await;
        sqlx::query(
            "INSERT INTO repo_remote_meta (repo_id, description, open_pr_count, last_fetched_at) \
             VALUES (?, 'keep', 9, 1)",
        )
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
        let t = FakeTransport::repo_only(RepoFetch::NetworkLost);
        let now = 1 + REFRESH_WINDOW_SECS + 10;
        let out = refresh_one(&pool, &t, &NoToken, id, now, false)
            .await
            .unwrap();
        assert_eq!(out.outcome, RefreshOutcome::NetworkLost);
        assert_eq!(
            t.release_calls.get(),
            0,
            "no sub-fetch after a repo failure"
        );
        assert_eq!(t.pulls_calls.get(), 0);
        assert_eq!(
            read_meta::<Option<String>>(&pool, id, "description")
                .await
                .as_deref(),
            Some("keep"),
            "a network error leaves the cached row intact"
        );
        assert_eq!(
            read_meta::<Option<i64>>(&pool, id, "open_pr_count").await,
            Some(9),
            "a network error leaves the cached PR count intact"
        );
    }

    #[tokio::test]
    async fn refresh_200_release_unknown_preserves_cached_release() {
        // BL-NI-15a: a 200 repo fetch whose release sub-fetch failed (Unknown) must NOT
        // erase the previously-cached release; it preserves the release fields.
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
        let t = FakeTransport::new(
            RepoFetch::Modified {
                metadata: GhMetadata {
                    description: Some("fresh desc".into()),
                    default_branch: Some("main".into()),
                    topics_json: Some("[]".into()),
                    is_archived: false,
                },
                etag: Some("\"new\"".into()),
                observed_sha: None,
                rate_limit: healthy_budget(),
            },
            ReleaseFetch::Unknown,
            PrFetch::Unknown,
        );
        let now = 1 + REFRESH_WINDOW_SECS + 10;
        let out = refresh_one(&pool, &t, &NoToken, id, now, false)
            .await
            .unwrap();
        assert_eq!(out.outcome, RefreshOutcome::Updated);
        assert!(out.release_stale, "an Unknown release sub-fetch is stale");
        assert_eq!(
            read_meta::<Option<String>>(&pool, id, "latest_release_tag")
                .await
                .as_deref(),
            Some("v0"),
            "an Unknown release sub-fetch preserves the cached release tag"
        );
        assert_eq!(
            read_meta::<Option<i64>>(&pool, id, "latest_release_at").await,
            Some(500),
            "an Unknown release sub-fetch preserves the cached release timestamp"
        );
        assert_eq!(
            read_meta::<Option<String>>(&pool, id, "description")
                .await
                .as_deref(),
            Some("fresh desc"),
            "the repo-resource columns still refresh on a 200"
        );
    }

    #[tokio::test]
    async fn refresh_200_release_no_release_clears_cached_release() {
        // BL-NI-15a: a CONFIRMED no-release (404 -> NoRelease) legitimately clears a
        // stale cached release. NoRelease is authoritative; only Unknown preserves.
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
        let t = FakeTransport::new(
            RepoFetch::Modified {
                metadata: sample_metadata(),
                etag: None,
                observed_sha: None,
                rate_limit: healthy_budget(),
            },
            ReleaseFetch::NoRelease {
                rate_limit: healthy_budget(),
            },
            PrFetch::Unknown,
        );
        let now = 1 + REFRESH_WINDOW_SECS + 10;
        let out = refresh_one(&pool, &t, &NoToken, id, now, false)
            .await
            .unwrap();
        assert!(!out.release_stale, "a confirmed NoRelease is not stale");
        assert_eq!(
            read_meta::<Option<String>>(&pool, id, "latest_release_tag").await,
            None,
            "a confirmed no-release clears the stale cached release"
        );
    }

    #[tokio::test]
    async fn refresh_rate_limited_surfaces_the_budget_and_reset() {
        let tmp = TempDir::new().unwrap();
        let pool = fresh_pool(tmp.path()).await;
        let id = seed_github_repo(&pool).await;
        let t = FakeTransport::repo_only(RepoFetch::RateLimited {
            rate_limit: RateLimit {
                remaining: 0,
                limit: 60,
                reset_at: 1_700_000_000,
            },
        });
        let out = refresh_one(&pool, &t, &NoToken, id, 5000, false)
            .await
            .unwrap();
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
        assert_eq!(t.release_calls.get(), 0, "no sub-fetch after a rate limit");
    }

    #[tokio::test]
    async fn refresh_surfaces_rate_limit_budget_for_backoff() {
        // A near-exhausted budget on any read must reach the caller so should_backoff
        // fires. Here the PR read carries the tightest budget.
        let tmp = TempDir::new().unwrap();
        let pool = fresh_pool(tmp.path()).await;
        let id = seed_github_repo(&pool).await;
        let t = FakeTransport::new(
            RepoFetch::Modified {
                metadata: sample_metadata(),
                etag: None,
                observed_sha: None,
                rate_limit: healthy_budget(),
            },
            ReleaseFetch::NoRelease {
                rate_limit: healthy_budget(),
            },
            PrFetch::Modified {
                counts: PrCounts::default(),
                etag: None,
                // 3 of 60 remaining = 5%, below the 10% backoff floor.
                rate_limit: RateLimit {
                    remaining: 3,
                    limit: 60,
                    reset_at: 0,
                },
            },
        );
        let out = refresh_one(&pool, &t, &NoToken, id, 5000, false)
            .await
            .unwrap();
        assert_eq!(out.outcome, RefreshOutcome::Updated);
        let budget = out
            .rate_limit
            .expect("a 200 surfaces the observed rate-limit budget");
        assert!(
            should_backoff(&budget),
            "the tightest budget reaches the caller so the orchestrator backs off"
        );
    }

    #[tokio::test]
    async fn refresh_from_cache_surfaces_no_budget() {
        let tmp = TempDir::new().unwrap();
        let pool = fresh_pool(tmp.path()).await;
        let id = seed_github_repo(&pool).await;
        // All three resources fresh, so nothing is due -> a true cache hit, no budget.
        sqlx::query(
            "INSERT INTO repo_remote_meta \
             (repo_id, last_fetched_at, release_last_checked_at, pr_last_checked_at) \
             VALUES (?, 9900, 9900, 9900)",
        )
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
        let t = FakeTransport::repo_only(RepoFetch::NetworkLost);
        let out = refresh_one(&pool, &t, &NoToken, id, 10000, false)
            .await
            .unwrap();
        assert_eq!(out.outcome, RefreshOutcome::Cached);
        assert_eq!(out.rate_limit, None, "a cache hit reports no budget");
    }

    // --- refresh_pass + the budgeter (E-17 AC16) ----------------------------

    /// A fake that always 200s the repo + release + pulls with a healthy budget, so a
    /// cold backfill is bounded only by the request budgeter, never a rate limit.
    struct AlwaysOkTransport;
    impl Transport for AlwaysOkTransport {
        async fn fetch_repo(
            &self,
            _c: &RepoCoords,
            _e: Option<&str>,
            _t: Option<&str>,
        ) -> RepoFetch {
            RepoFetch::Modified {
                metadata: GhMetadata {
                    default_branch: Some("main".into()),
                    ..Default::default()
                },
                etag: Some("\"e\"".into()),
                observed_sha: None,
                rate_limit: RateLimit {
                    remaining: 59,
                    limit: 60,
                    reset_at: 0,
                },
            }
        }
        async fn fetch_release(
            &self,
            _c: &RepoCoords,
            _e: Option<&str>,
            _t: Option<&str>,
        ) -> ReleaseFetch {
            ReleaseFetch::NoRelease {
                rate_limit: RateLimit {
                    remaining: 59,
                    limit: 60,
                    reset_at: 0,
                },
            }
        }
        async fn fetch_pulls(
            &self,
            _c: &RepoCoords,
            _db: Option<&str>,
            _e: Option<&str>,
            _t: Option<&str>,
        ) -> PrFetch {
            PrFetch::Modified {
                counts: PrCounts {
                    open: 1,
                    default_branch: 1,
                },
                etag: Some("\"pr\"".into()),
                rate_limit: RateLimit {
                    remaining: 59,
                    limit: 60,
                    reset_at: 0,
                },
            }
        }
    }

    #[tokio::test]
    async fn budgeted_100_repo_backfill_never_exceeds_the_ceiling_and_reaches_full_coverage() {
        // E-17 AC16: a 100-repo library reaches full PR-intelligence coverage without
        // ever exceeding 60 requests in any rolling hour, with no rate-limit error, via
        // the request budgeter + oldest-metadata-first refresh, over an injected clock.
        let tmp = TempDir::new().unwrap();
        let pool = fresh_pool(tmp.path()).await;
        for i in 0..100 {
            sqlx::query(
                "INSERT INTO repos (local_name, local_path, remote_origin_url, host_type, created_at) \
                 VALUES (?, ?, ?, 'github', 0)",
            )
            .bind(format!("repo-{i}"))
            .bind(format!("C:/repos/repo-{i}"))
            .bind(format!("https://github.com/owner/repo-{i}"))
            .execute(&pool)
            .await
            .unwrap();
        }

        let transport = AlwaysOkTransport;
        let budgeter = shared_budgeter(); // 30 / hour, the shared handle
                                          // Record (now, requests_made) for each productive pass so we can prove the
                                          // rolling-hour ceiling was respected.
        let mut history: Vec<(i64, usize)> = Vec::new();
        let mut now: i64 = 1_000_000;
        // Advance the clock by 30 min between passes so two consecutive passes fall in
        // one rolling hour - the budgeter must still hold the line.
        let step = 1800;
        for _ in 0..40 {
            let report = refresh_pass(&pool, &transport, &NoToken, &budgeter, now)
                .await
                .unwrap();
            assert!(!report.rate_limited, "no rate-limit error must surface");
            if report.requests_made > 0 {
                history.push((now, report.requests_made));
            }
            now += step;
        }

        // Full coverage: every repo has a last_fetched_at (its metadata was refreshed).
        let covered: i64 = sqlx::query(
            "SELECT COUNT(*) AS c FROM repo_remote_meta WHERE last_fetched_at IS NOT NULL",
        )
        .fetch_one(&pool)
        .await
        .unwrap()
        .try_get("c")
        .unwrap();
        assert_eq!(
            covered, 100,
            "the cold 100-repo backfill reached full coverage"
        );

        // Rolling-hour ceiling: for every pass start t, the requests in [t, t+3600)
        // must never exceed 60 (in fact the 30/hour budgeter keeps it at or under 30).
        for &(t, _) in &history {
            let in_window: usize = history
                .iter()
                .filter(|(u, _)| *u >= t && *u < t + RATE_WINDOW_SECS)
                .map(|(_, made)| *made)
                .sum();
            assert!(
                in_window <= 60,
                "no rolling hour may exceed 60 requests; window at {t} had {in_window}"
            );
            assert!(
                in_window <= MAX_REQUESTS_PER_HOUR,
                "the 30/hour budgeter must hold; window at {t} had {in_window}"
            );
        }
    }

    // --- finding 1: resource-aware staleness ---------------------------------

    #[tokio::test]
    async fn fresh_repo_window_still_fetches_a_due_pr_subresource() {
        // Finding 1 regression: a repo enriched by E-10 (fresh last_fetched_at, fresh
        // release, but NULL pr_last_checked_at) must STILL fetch its PR sub-resource,
        // instead of being served wholesale from the repo cache. The repo resource and
        // the release are fresh, so ONLY the PR fetch fires - no wasted repo re-fetch.
        let tmp = TempDir::new().unwrap();
        let pool = fresh_pool(tmp.path()).await;
        let id = seed_github_repo(&pool).await;
        sqlx::query(
            "INSERT INTO repo_remote_meta \
             (repo_id, last_fetched_at, etag, release_last_checked_at) \
             VALUES (?, 9900, '\"repo-e\"', 9900)",
        )
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
        let t = FakeTransport::new(
            RepoFetch::NetworkLost,
            ReleaseFetch::Unknown,
            PrFetch::Modified {
                counts: PrCounts {
                    open: 4,
                    default_branch: 2,
                },
                etag: Some("\"pr\"".into()),
                rate_limit: healthy_budget(),
            },
        );
        let out = refresh_one(&pool, &t, &NoToken, id, 10000, false)
            .await
            .unwrap();
        assert_eq!(
            out.outcome,
            RefreshOutcome::Cached,
            "the repo resource is still served from cache (it was fresh)"
        );
        assert_eq!(
            out.requests_made, 1,
            "only the due PR sub-resource is fetched"
        );
        assert!(out.changed, "the PR fetch wrote fresh counts");
        assert_eq!(
            t.repo_calls.get(),
            0,
            "the fresh repo resource is NOT re-fetched"
        );
        assert_eq!(
            t.release_calls.get(),
            0,
            "the fresh release is NOT re-fetched"
        );
        assert_eq!(t.pulls_calls.get(), 1, "the due PR sub-resource IS fetched");
        assert_eq!(
            read_meta::<Option<i64>>(&pool, id, "open_pr_count").await,
            Some(4),
            "a fresh repo window no longer masks a due PR fetch (finding 1)"
        );
    }

    #[tokio::test]
    async fn force_rechecks_every_resource_inside_the_window() {
        // Finding 1 force path: a manual refresh (force=true) re-checks all three
        // resources even when every window is fresh, so a user Refresh always re-fetches.
        let tmp = TempDir::new().unwrap();
        let pool = fresh_pool(tmp.path()).await;
        let id = seed_github_repo(&pool).await;
        sqlx::query(
            "INSERT INTO repo_remote_meta \
             (repo_id, last_fetched_at, release_last_checked_at, pr_last_checked_at) \
             VALUES (?, 9900, 9900, 9900)",
        )
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
        let t = FakeTransport::new(
            RepoFetch::NotModified {
                rate_limit: healthy_budget(),
            },
            ReleaseFetch::NotModified {
                rate_limit: healthy_budget(),
            },
            PrFetch::NotModified {
                rate_limit: healthy_budget(),
            },
        );
        let out = refresh_one(&pool, &t, &NoToken, id, 10000, true)
            .await
            .unwrap();
        assert_eq!(out.requests_made, 3, "force re-checks all three resources");
        assert_eq!(t.repo_calls.get(), 1);
        assert_eq!(t.release_calls.get(), 1);
        assert_eq!(t.pulls_calls.get(), 1);
    }

    // --- finding 2: manual + background share ONE budget ---------------------

    #[tokio::test]
    async fn manual_and_background_share_one_budget_and_never_overspend() {
        // Finding 2: the manual `repo_refresh_metadata` path and the background pass
        // spend against the SAME budget, so a manual refresh cannot race the pass into
        // overspending the ceiling. Shared budget of 5 funds ONE full repo (3) with
        // headroom, but not a second (6).
        let tmp = TempDir::new().unwrap();
        let pool = fresh_pool(tmp.path()).await;
        let a = seed_github_repo(&pool).await;
        sqlx::query(
            "INSERT INTO repos (local_name, local_path, remote_origin_url, host_type, default_branch, created_at) \
             VALUES ('b', 'b', 'https://github.com/owner/b', 'github', 'main', 0)",
        )
        .execute(&pool)
        .await
        .unwrap();

        let transport = AlwaysOkTransport;
        let budget: SharedBudgeter =
            std::sync::Arc::new(tokio::sync::Mutex::new(RateBudgeter::with_limits(5, 3600)));
        let now = 1_000_000;

        // Manual refresh of A (force) spends 3 against the SHARED budget.
        let manual = refresh_one_budgeted(&pool, &transport, &NoToken, &budget, a, now, true)
            .await
            .unwrap();
        assert!(matches!(manual, BudgetedRefresh::Refreshed(_)));
        assert_eq!(
            budget.lock().await.remaining(now),
            2,
            "the manual refresh spent 3 of the shared 5"
        );

        // A background pass now sees only 2 budget left - not enough to START B (needs
        // 3), so it stops WITHOUT overspending the shared ceiling.
        let report = refresh_pass(&pool, &transport, &NoToken, &budget, now)
            .await
            .unwrap();
        assert!(
            report.budget_exhausted,
            "the shared budget stops the background pass"
        );
        assert_eq!(
            report.attempted, 0,
            "B is never started - it cannot be fully funded from the shared budget"
        );
        assert_eq!(
            budget.lock().await.remaining(now),
            2,
            "no overspend past the shared ceiling"
        );
    }

    #[tokio::test]
    async fn budget_exhaustion_yields_a_non_error_last_known_state() {
        // Finding 2 degradation: when the shared budget is exhausted, a refresh returns
        // BudgetExhausted (the caller shows last-known "as of" values), NOT an error and
        // NOT an overspend.
        let tmp = TempDir::new().unwrap();
        let pool = fresh_pool(tmp.path()).await;
        let id = seed_github_repo(&pool).await;
        let transport = AlwaysOkTransport;
        // A zero budget: nothing can be funded.
        let budget: SharedBudgeter =
            std::sync::Arc::new(tokio::sync::Mutex::new(RateBudgeter::with_limits(0, 3600)));
        let out = refresh_one_budgeted(&pool, &transport, &NoToken, &budget, id, 1000, true)
            .await
            .unwrap();
        assert_eq!(out, BudgetedRefresh::BudgetExhausted);
    }

    // --- finding 3: the pass reports which repos CHANGED ----------------------

    /// A fake that 304s every resource - a steady-state re-check where nothing moved.
    struct AllNotModifiedTransport;
    impl Transport for AllNotModifiedTransport {
        async fn fetch_repo(
            &self,
            _c: &RepoCoords,
            _e: Option<&str>,
            _t: Option<&str>,
        ) -> RepoFetch {
            RepoFetch::NotModified {
                rate_limit: healthy_budget(),
            }
        }
        async fn fetch_release(
            &self,
            _c: &RepoCoords,
            _e: Option<&str>,
            _t: Option<&str>,
        ) -> ReleaseFetch {
            ReleaseFetch::NotModified {
                rate_limit: healthy_budget(),
            }
        }
        async fn fetch_pulls(
            &self,
            _c: &RepoCoords,
            _db: Option<&str>,
            _e: Option<&str>,
            _t: Option<&str>,
        ) -> PrFetch {
            PrFetch::NotModified {
                rate_limit: healthy_budget(),
            }
        }
    }

    #[tokio::test]
    async fn pass_reports_changed_ids_for_fresh_data_and_none_when_nothing_moves() {
        // Finding 3: a cold pass writes fresh data and reports EVERY repo as changed (so
        // the shell emits one coalesced list refresh + a per-repo drawer signal); an
        // immediate re-pass with nothing due reports NO changed repos.
        let tmp = TempDir::new().unwrap();
        let pool = fresh_pool(tmp.path()).await;
        let a = seed_github_repo(&pool).await;
        let b = sqlx::query(
            "INSERT INTO repos (local_name, local_path, remote_origin_url, host_type, default_branch, created_at) \
             VALUES ('b', 'b', 'https://github.com/owner/b', 'github', 'main', 0)",
        )
        .execute(&pool)
        .await
        .unwrap()
        .last_insert_rowid();

        let transport = AlwaysOkTransport;
        let budget = shared_budgeter();
        let now = 1_000_000;
        let report = refresh_pass(&pool, &transport, &NoToken, &budget, now)
            .await
            .unwrap();
        assert_eq!(report.attempted, 2);
        let mut changed = report.changed_repo_ids.clone();
        changed.sort_unstable();
        let mut expected = vec![a, b];
        expected.sort_unstable();
        assert_eq!(
            changed, expected,
            "a cold pass reports every repo as changed"
        );

        // A second pass at the SAME instant: every resource is inside its window, so
        // nothing is due and nothing is reported as changed.
        let report2 = refresh_pass(&pool, &transport, &NoToken, &budget, now)
            .await
            .unwrap();
        assert_eq!(report2.attempted, 0);
        assert!(
            report2.changed_repo_ids.is_empty(),
            "a pass with nothing due reports no changed repos - no needless UI refresh"
        );
    }

    #[tokio::test]
    async fn pass_with_only_304s_reports_no_changed_repos() {
        // Finding 3 precision: a due pass where every resource 304s (nothing actually
        // moved) reports NO changed repos, so the ETag steady state does not spam the UI
        // with refreshes.
        let tmp = TempDir::new().unwrap();
        let pool = fresh_pool(tmp.path()).await;
        let id = seed_github_repo(&pool).await;
        // Seed an existing meta row whose windows have all elapsed (so it is DUE) but
        // whose ETags are set (so the transport 304s).
        sqlx::query(
            "INSERT INTO repo_remote_meta \
             (repo_id, last_fetched_at, etag, release_etag, release_last_checked_at, \
              pr_etag, pr_last_checked_at) \
             VALUES (?, 1, '\"r\"', '\"rel\"', 1, '\"pr\"', 1)",
        )
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
        let transport = AllNotModifiedTransport;
        let budget = shared_budgeter();
        let now = 1 + REFRESH_WINDOW_SECS + 10;
        let report = refresh_pass(&pool, &transport, &NoToken, &budget, now)
            .await
            .unwrap();
        assert_eq!(report.attempted, 1, "the due repo was refreshed");
        assert_eq!(
            report.requests_made, 3,
            "all three resources were re-checked"
        );
        assert!(
            report.changed_repo_ids.is_empty(),
            "a 304-only pass changed nothing, so it reports no changed repos"
        );
    }
}
