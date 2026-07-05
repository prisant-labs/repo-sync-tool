//! scheduler - owned by E-08 (interval, jitter, semaphore, per-repo lock).
//!
//! RepoSync's background heartbeat. The model is RESIDENT-ONLY: there is no OS
//! scheduler in V1, so checks happen only while the app runs. One
//! `tokio::time::interval` ticks every minute; on each tick the scheduler asks
//! the DB which repos are due, fans the due set out through a bounded
//! concurrency limit, runs each repo's check through the SHARED E-07
//! decide -> execute path ([`crate::repo::update_now_scheduled`]), then records
//! the outcome and schedules the next check.
//!
//! Two correctness properties are load-bearing:
//!   1. The PER-REPO async mutex composed UNDER a global semaphore, acquired in a
//!      fixed order (per-repo mutex first, THEN the semaphore permit), so a
//!      scheduled check and a manual "check now" never launch two `git` processes
//!      in one working tree at once.
//!   2. No DB transaction is ever held across a network/git call.
//!
//! Everything time-dependent (cadence, jitter, quiet hours, `next_check_at`) is
//! driven by an injected [`Clock`], so the whole engine is testable with a fake
//! clock and zero real-time sleeps.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use sqlx::{Row, SqlitePool};
use tokio::sync::{Mutex as TokioMutex, RwLock, Semaphore};
use tokio::task::JoinSet;

use crate::error::AppError;
use crate::git::SystemGitEngine;
use crate::ipc::RepoId;
use crate::policy::{classify_failure, RepoStatus, RunOutcome};
use crate::repo;

/// The production tick period: one minute, pinned to a named const so the
/// production cadence CANNOT drift. Only the test harness drives ticks directly
/// (via [`Scheduler::tick_once`]) instead of waiting on this interval.
pub const ONE_MINUTE: std::time::Duration = std::time::Duration::from_secs(60);

/// The default global concurrency cap (brief Section 4.7: "a bounded `Semaphore`
/// (default 4)").
pub const DEFAULT_CONCURRENCY: usize = 4;

/// The startup jitter ceiling, in seconds (brief Section 4.7: "stagger with
/// jitter (random 0-30s)").
pub const STARTUP_JITTER_MAX_SECS: i64 = 30;

/// The hard cadence floor in minutes (6h), used only when BOTH the per-repo
/// `check_frequency_min` and the global `settings.global_check_minutes` are
/// non-positive. It matches the schema default for `settings.global_check_minutes`
/// so the effective cadence stays 6h when nothing overrides it. It is NOT the
/// per-repo default: under the INHERIT model a new repo's `check_frequency_min` is
/// 0 (inherit the global), not 360.
pub const DEFAULT_FREQUENCY_MIN: i64 = 360;

// =============================================================================
// Pure decision helpers (no I/O, no clock) - the testable core of the engine.
// =============================================================================

/// Whether `now_min` (local minutes since midnight, 0..=1439) falls inside the
/// configured quiet-hours window.
///
/// The window is half-open `[start, end)` so a check that starts exactly at the
/// `end` minute is allowed. A window whose `start > end` WRAPS past midnight
/// (e.g. 1320 = 22:00 to 420 = 07:00). A zero-width (`start == end`) or
/// unconfigured (either bound `None`) window is never quiet.
pub fn in_quiet_hours(now_min: i64, start: Option<i64>, end: Option<i64>) -> bool {
    let (Some(start), Some(end)) = (start, end) else {
        return false;
    };
    if start == end {
        // A zero-width window means "no quiet period", never quiet.
        return false;
    }
    if start < end {
        // Same-day window: quiet from start (inclusive) to end (exclusive).
        now_min >= start && now_min < end
    } else {
        // Wraps midnight: quiet at/after start OR before end.
        now_min >= start || now_min < end
    }
}

/// The effective check frequency in minutes for a repo: its own
/// `check_frequency_min` when positive (the per-repo override), else the global
/// default, else the hard [`DEFAULT_FREQUENCY_MIN`] floor. A `check_frequency_min`
/// of 0 is the INHERIT sentinel, so an inherit repo resolves to the global
/// `settings.global_check_minutes` passed as `global_default`. A frequency is
/// never allowed to be zero or negative (which would schedule a check in the past
/// or instantly).
pub fn effective_frequency_min(repo_freq_min: i64, global_default: i64) -> i64 {
    if repo_freq_min > 0 {
        repo_freq_min
    } else if global_default > 0 {
        global_default
    } else {
        DEFAULT_FREQUENCY_MIN
    }
}

/// Local minutes-of-day (`0..=1439`) for a UTC `now_unix` shifted by
/// `utc_offset_minutes`. Pure and timezone-database-free: the offset is INJECTED
/// by the edge (never read from the host's configured timezone here), so quiet
/// hours are evaluated purely from the supplied offset and a fixed instant, and a
/// test can pin both. `div_euclid`/`rem_euclid` keep a pre-epoch instant or a
/// large negative offset in range (Rust's `%` would otherwise yield a negative
/// minute for a negative `local_secs`).
pub fn local_minutes_at(now_unix: i64, utc_offset_minutes: i64) -> i64 {
    let local_secs = now_unix + utc_offset_minutes * 60;
    local_secs.div_euclid(60).rem_euclid(1440)
}

/// The next check time (unix seconds) for a repo that just ran at `now_unix`,
/// given its effective frequency in minutes. A non-positive frequency is clamped
/// to [`DEFAULT_FREQUENCY_MIN`] so the next check is always strictly in the
/// future.
pub fn next_check_at(now_unix: i64, freq_min: i64) -> i64 {
    let f = if freq_min > 0 {
        freq_min
    } else {
        DEFAULT_FREQUENCY_MIN
    };
    now_unix + f * 60
}

/// Map the E-07 failure state machine's [`RepoStatus`] verdict to the two
/// persisted columns: `(consecutive_failures, auto_paused)`.
///
///   - [`RepoStatus::Active`]   -> `(0, false)`: success clears the streak.
///   - [`RepoStatus::Retry`]    -> `(n, false)`: the running count, not yet paused.
///   - [`RepoStatus::AutoPaused`] -> `(0, true)`: 3-strikes pause, counter reset
///     (the engine signalled the pause; E-08 sets the flag and resets the count).
///   - [`RepoStatus::PausedOnAuth`] -> `(0, true)`: an auth failure pauses the
///     repo immediately; the schema has a single `auto_paused` flag, so an
///     auth-pause sets it too (the distinct reason lives in the activity row).
fn persist_columns(status: RepoStatus) -> (i64, bool) {
    match status {
        RepoStatus::Active => (0, false),
        RepoStatus::Retry {
            consecutive_failures,
        } => (consecutive_failures, false),
        RepoStatus::AutoPaused => (0, true),
        RepoStatus::PausedOnAuth => (0, true),
    }
}

/// Clamp a raw jitter value into `0..=max` (defensive: the jitter source should
/// already be in range, but the scheduler never staggers by a negative or
/// over-ceiling offset).
fn clamp_jitter(raw: i64, max: i64) -> i64 {
    raw.clamp(0, max)
}

// =============================================================================
// Injected time + randomness seams.
// =============================================================================

/// An injected time source. Carries BOTH unix seconds (for `next_check_at` and
/// timestamps) AND the local minutes-of-day (for quiet hours), so no part of the
/// scheduler reads the wall clock directly. Production reads the system clock;
/// tests use a fake that advances deterministically (AC7).
pub trait Clock: Send + Sync {
    /// Current unix time in whole seconds (UTC epoch).
    fn now_unix(&self) -> i64;
    /// Current LOCAL wall-clock minutes since midnight, in `0..=1439`.
    fn local_minutes_of_day(&self) -> i64;
}

/// An injected randomness source for startup jitter. Production uses a small
/// self-contained PRNG; tests pin it to deterministic values (AC5).
pub trait Jitter: Send + Sync {
    /// A jitter offset in `0..=max_inclusive` seconds.
    fn jitter_secs(&self, max_inclusive: i64) -> i64;
}

/// The production [`Clock`]: unix time from the system clock, local minutes-of-day
/// derived from an injected UTC offset (so reposync-core needs no timezone crate).
///
/// The offset is captured at construction. A DST transition mid-session is NOT
/// re-derived here - that is the spec's flagged open question. The edge that
/// constructs the clock supplies the host's local offset, and a V1.1 enhancement
/// can refresh it.
pub struct SystemClock {
    utc_offset_minutes: i64,
}

impl SystemClock {
    /// A clock in UTC (offset 0). The edge should prefer
    /// [`SystemClock::with_utc_offset_minutes`] with the host's local offset so
    /// quiet hours are evaluated in local time.
    pub fn new() -> SystemClock {
        SystemClock {
            utc_offset_minutes: 0,
        }
    }

    /// A clock whose local time is `utc_offset_minutes` from UTC (e.g. `-480` for
    /// PST). The offset derives the local minutes-of-day used for quiet hours.
    pub fn with_utc_offset_minutes(utc_offset_minutes: i64) -> SystemClock {
        SystemClock { utc_offset_minutes }
    }
}

impl Default for SystemClock {
    fn default() -> Self {
        SystemClock::new()
    }
}

impl Clock for SystemClock {
    fn now_unix(&self) -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0)
    }

    fn local_minutes_of_day(&self) -> i64 {
        local_minutes_at(self.now_unix(), self.utc_offset_minutes)
    }
}

/// The production [`Jitter`]: a tiny self-contained xorshift PRNG seeded from the
/// system clock, so reposync-core needs no `rand` dependency. Staggering needs
/// only a cheap spread to avoid a thundering herd, not cryptographic randomness.
pub struct SystemJitter {
    state: AtomicU64,
}

impl SystemJitter {
    pub fn new() -> SystemJitter {
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x9E37_79B9_7F4A_7C15);
        // Never seed zero (xorshift would stay stuck at zero).
        SystemJitter {
            state: AtomicU64::new(seed | 1),
        }
    }

    fn next_u64(&self) -> u64 {
        // xorshift64*. Relaxed ordering is fine: we want spread, not a strict
        // global sequence, and a rare duplicate under contention only means two
        // repos share a stagger offset (harmless).
        let mut x = self.state.load(Ordering::Relaxed);
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.state.store(x, Ordering::Relaxed);
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
}

impl Default for SystemJitter {
    fn default() -> Self {
        SystemJitter::new()
    }
}

impl Jitter for SystemJitter {
    fn jitter_secs(&self, max_inclusive: i64) -> i64 {
        if max_inclusive <= 0 {
            return 0;
        }
        (self.next_u64() % (max_inclusive as u64 + 1)) as i64
    }
}

// =============================================================================
// The per-repo lock map (shared with the manual command path).
// =============================================================================

/// The per-repo async-mutex map (AC3/AC4). Acquiring a repo's mutex serializes
/// ALL git work for that repo, so a scheduled check and a manual "check now"
/// never launch two `git` processes in one working tree. The SAME handle is
/// shared with the manual command path: [`RepoLocks`] is `Clone` and backed by a
/// shared map, so both paths contend the same lock.
///
/// Eviction default (the spec's open question): lazy insert on first use, drop on
/// [`RepoLocks::remove`] (called by the repo-remove path); no periodic sweep.
#[derive(Clone, Default)]
pub struct RepoLocks {
    map: Arc<StdMutex<HashMap<i64, Arc<TokioMutex<()>>>>>,
}

impl RepoLocks {
    /// The mutex for `id`, lazily inserted on first use.
    pub fn lock_handle(&self, id: RepoId) -> Arc<TokioMutex<()>> {
        let mut map = self.map.lock().expect("repo-locks map poisoned");
        map.entry(id.0)
            .or_insert_with(|| Arc::new(TokioMutex::new(())))
            .clone()
    }

    /// Drop a removed repo's mutex entry (the drop-on-remove eviction default).
    /// Safe to call for an absent id.
    pub fn remove(&self, id: RepoId) {
        self.map
            .lock()
            .expect("repo-locks map poisoned")
            .remove(&id.0);
    }

    /// The number of live per-repo mutex entries (diagnostics / tests).
    pub fn len(&self) -> usize {
        self.map.lock().expect("repo-locks map poisoned").len()
    }

    /// Whether no per-repo mutex entries are live.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

// =============================================================================
// The scheduler seams (faked in unit tests, DB/git-backed in production).
// =============================================================================

/// One repo the due-query selected, carrying the fields the per-job pipeline
/// needs: the id, its effective check frequency (for `next_check_at`), and the
/// prior consecutive-failure count (the input to the E-07 failure state machine,
/// read BEFORE this run).
#[derive(Debug, Clone, Copy)]
pub struct DueRepo {
    pub id: RepoId,
    pub check_frequency_min: i64,
    pub consecutive_failures: i64,
}

/// The due-query result: the candidate repos plus the configured quiet-hours
/// window from `settings`. The scheduler applies the clock-driven quiet-hours
/// gate to the candidates, so the gate stays testable with a fake clock.
#[derive(Debug, Clone)]
pub struct DueSelection {
    /// `(quiet_hours_start, quiet_hours_end)` as minutes since midnight, or
    /// `(None, None)` when unconfigured.
    pub quiet_hours: (Option<i64>, Option<i64>),
    pub candidates: Vec<DueRepo>,
}

/// The due-query seam: candidate repos (`enabled = 1` AND `auto_paused = 0` AND
/// `next_check_at <= now`) plus the quiet-hours window. Faked in unit tests;
/// DB-backed in production.
pub trait DueQuery: Send + Sync {
    /// `+ Send` is required because the scheduler spawns the per-tick pipeline onto
    /// the multi-threaded runtime; every seam future must cross threads.
    fn select_due(
        &self,
        now_unix: i64,
    ) -> impl std::future::Future<Output = Result<DueSelection, AppError>> + Send;
}

/// The per-job runner seam: run one repo's scheduled check/update through the
/// SHARED E-07 decide -> execute path and return the classified [`RunOutcome`].
/// Faked in unit tests; the production impl calls
/// [`crate::repo::update_now_scheduled`] so the safety rules and the git
/// execution live in exactly one place.
///
/// The engine is passed IN per job (not owned by the runner) so every job in a
/// tick uses the engine the scheduler resolved from the live handle at tick
/// start (BL-NI-23 / finding 6): a re-probe that swaps a new git in is picked up
/// on the next tick without a restart, and the runner never holds a stale clone.
pub trait JobRunner: Send + Sync {
    fn run(
        &self,
        id: RepoId,
        git: SystemGitEngine,
    ) -> impl std::future::Future<Output = RunOutcome> + Send;
}

/// The outcome-writer seam: persist `next_check_at` + the failure-counter /
/// auto-pause columns for one finished job, in a SHORT transaction that never
/// spans the git call. Faked in unit tests; DB-backed in production.
pub trait OutcomeWriter: Send + Sync {
    fn record(
        &self,
        repo: &DueRepo,
        now_unix: i64,
        status: RepoStatus,
    ) -> impl std::future::Future<Output = Result<(), AppError>> + Send;
}

// =============================================================================
// The live git-engine seam (BL-NI-23 / finding 6).
// =============================================================================

/// The shared, swappable git-engine handle. The edge owns ONE of these in its
/// managed state; when the user fixes a broken/missing git path in Settings it
/// re-probes and swaps the inner engine (BL-NI-19 / BL-NI-26). The resident
/// scheduler reads the CURRENT engine through the SAME handle each cycle, so a
/// live re-probe is picked up by the background loop WITHOUT a restart. `None`
/// means "no usable git right now" (git absent at startup, or an explicit
/// override that does not resolve).
pub type SharedGitEngine = Arc<RwLock<Option<SystemGitEngine>>>;

/// A source of the currently-live git engine, read fresh at the START of each
/// scheduler cycle. Returning `None` makes the scheduler SKIP the whole cycle
/// (no due query, no jobs, no writes), so an absent git never advances
/// `next_check_at` or accrues per-repo failures - the moment git appears, every
/// due repo is checked on the next tick. This seam is what keeps the core
/// Tauri-free: the edge injects the live handle; the core never names Tauri.
pub trait GitEngineSource: Send + Sync {
    /// The engine currently in effect, or `None` when no usable git is available.
    /// Boxed (not RPITIT) so the scheduler can hold `Arc<dyn GitEngineSource>`
    /// without a fourth generic parameter.
    fn current(&self) -> Pin<Box<dyn Future<Output = Option<SystemGitEngine>> + Send>>;
}

/// The production [`GitEngineSource`]: reads the current engine out of the shared,
/// swappable [`SharedGitEngine`] handle. Cloning the engine out under a short read
/// guard (the engine is cheap to clone - it wraps a path + availability state)
/// means the loop never holds the lock across git work and always sees the latest
/// swapped-in engine.
pub struct SharedGitEngineSource {
    handle: SharedGitEngine,
}

impl SharedGitEngineSource {
    pub fn new(handle: SharedGitEngine) -> SharedGitEngineSource {
        SharedGitEngineSource { handle }
    }
}

impl GitEngineSource for SharedGitEngineSource {
    fn current(&self) -> Pin<Box<dyn Future<Output = Option<SystemGitEngine>> + Send>> {
        let handle = self.handle.clone();
        Box::pin(async move { handle.read().await.clone() })
    }
}

// =============================================================================
// The scheduler.
// =============================================================================

/// The background scheduler (AC1): a tick over the due-query that fans each due
/// repo through the per-repo-mutex + global-semaphore composition into the shared
/// update path, then records the outcome and the next check time.
pub struct Scheduler<Q, J, W> {
    clock: Arc<dyn Clock>,
    jitter: Arc<dyn Jitter>,
    /// The live git-engine source, read at the start of each cycle so the loop
    /// picks up a re-probe and skips cleanly when git is absent (BL-NI-23).
    git_source: Arc<dyn GitEngineSource>,
    due_query: Q,
    job_runner: Arc<J>,
    outcome_writer: Arc<W>,
    semaphore: Arc<Semaphore>,
    locks: RepoLocks,
    /// Deduplicates the "no git engine" skip log so a long-resident git-less
    /// session emits one clear line per absence episode, not one every minute.
    git_absent_logged: AtomicBool,
}

impl<Q, J, W> Scheduler<Q, J, W>
where
    Q: DueQuery,
    J: JobRunner + Send + Sync + 'static,
    W: OutcomeWriter + Send + Sync + 'static,
{
    /// Build a scheduler with a fresh per-repo lock map and a global semaphore of
    /// `concurrency` permits (clamped to at least 1).
    pub fn new(
        clock: Arc<dyn Clock>,
        jitter: Arc<dyn Jitter>,
        git_source: Arc<dyn GitEngineSource>,
        due_query: Q,
        job_runner: J,
        outcome_writer: W,
        concurrency: usize,
    ) -> Scheduler<Q, J, W> {
        Scheduler {
            clock,
            jitter,
            git_source,
            due_query,
            job_runner: Arc::new(job_runner),
            outcome_writer: Arc::new(outcome_writer),
            semaphore: Arc::new(Semaphore::new(concurrency.max(1))),
            locks: RepoLocks::default(),
            git_absent_logged: AtomicBool::new(false),
        }
    }

    /// The shared per-repo lock map, so the manual command path can contend the
    /// same locks as the scheduler (the property that keeps a manual and a
    /// scheduled git op on one repo from colliding).
    pub fn locks(&self) -> RepoLocks {
        self.locks.clone()
    }

    /// Run ONE steady-state tick: select due repos, apply the quiet-hours gate,
    /// and fan the due set out through the concurrency composition with NO jitter.
    /// Returns the number of repos run. Tests drive ticks via this method instead
    /// of the timer.
    pub async fn tick_once(&self) -> Result<usize, AppError> {
        self.run_due(false).await
    }

    /// The startup pass: like a tick, but each due repo is staggered by a random
    /// `0..=30s` jitter (AC5: only startup and a newly-added repo's first schedule
    /// are jittered; steady-state ticks are not).
    pub async fn start(&self) -> Result<usize, AppError> {
        self.run_due(true).await
    }

    async fn run_due(&self, startup: bool) -> Result<usize, AppError> {
        // Live git gate (BL-NI-23 / finding 6): read the CURRENT engine from the
        // shared handle each cycle. If no usable git is available right now, skip
        // the WHOLE cycle - no due query, no jobs, no `next_check_at` writes - so
        // an absent git never advances a repo's schedule or accrues failures. The
        // instant git appears (a settings re-probe swaps in an engine), the next
        // tick runs the still-due repos. One clear log line per absence episode
        // (deduped so a long-resident git-less session does not spam every tick).
        let Some(engine) = self.git_source.current().await else {
            if !self.git_absent_logged.swap(true, Ordering::SeqCst) {
                eprintln!(
                    "scheduler: no usable git engine; skipping scheduled checks \
                     until git is available (set a valid git path in Settings)"
                );
            }
            return Ok(0);
        };
        // Git is back (or was always present): clear the dedup flag so the next
        // absence logs afresh.
        self.git_absent_logged.store(false, Ordering::SeqCst);

        let now = self.clock.now_unix();
        let now_min = self.clock.local_minutes_of_day();
        let selection = self.due_query.select_due(now).await?;
        let (quiet_start, quiet_end) = selection.quiet_hours;
        if in_quiet_hours(now_min, quiet_start, quiet_end) {
            // Inside quiet hours: nothing is selected this tick. The gate is the
            // due-query predicate re-evaluated next tick - there is no deferred
            // queue, so an in-window repo simply becomes selectable the first tick
            // `now` is outside the window.
            return Ok(0);
        }
        let due = selection.candidates;
        let count = due.len();
        self.spawn_and_join(due, startup, engine).await;
        Ok(count)
    }

    async fn spawn_and_join(&self, due: Vec<DueRepo>, startup: bool, engine: SystemGitEngine) {
        let mut set = JoinSet::new();
        for repo in due {
            let offset = if startup {
                clamp_jitter(
                    self.jitter.jitter_secs(STARTUP_JITTER_MAX_SECS),
                    STARTUP_JITTER_MAX_SECS,
                )
            } else {
                0
            };
            let locks = self.locks.clone();
            let sem = self.semaphore.clone();
            let jr = self.job_runner.clone();
            let ow = self.outcome_writer.clone();
            // Every job in this tick runs against the engine resolved from the live
            // handle at tick start (cheap to clone), so a mid-tick swap can never
            // leave one job on a stale engine and another on a new one.
            let git = engine.clone();
            // Each job reads its OWN completion time from the clock (AC6: next_check_at
            // "after each job"), so a slow/queued job does not inherit the stale
            // tick-start time.
            let clock = self.clock.clone();
            set.spawn(async move {
                // Startup stagger (production only; tests pin jitter to 0 so no
                // real sleep runs).
                if offset > 0 {
                    tokio::time::sleep(Duration::from_secs(offset as u64)).await;
                }
                run_job(repo, clock, locks, sem, jr, ow, git).await;
            });
        }
        while set.join_next().await.is_some() {}
    }

    /// The production resident loop (AC1): run the startup pass (jittered), then
    /// tick at [`ONE_MINUTE`] for the process lifetime. Never returns. The edge
    /// spawns this on the runtime.
    pub async fn run(&self) {
        if let Err(e) = self.start().await {
            eprintln!("scheduler: startup pass failed: {e}");
        }
        let mut interval = tokio::time::interval(ONE_MINUTE);
        // The first tick fires immediately; the startup pass already ran, so
        // consume it before entering the steady cadence.
        interval.tick().await;
        loop {
            interval.tick().await;
            if let Err(e) = self.tick_once().await {
                eprintln!("scheduler: tick failed: {e}");
            }
        }
    }
}

/// Run one repo's job under the FIXED lock-acquisition order (AC3/AC4): the
/// per-repo mutex FIRST, THEN a global semaphore permit, THEN the git work. The
/// permit is released as soon as the git work finishes (before the DB write), and
/// the per-repo mutex is held until the outcome is recorded, so a manual check on
/// the same repo waits for the whole job. Releasing in reverse order (permit, then
/// mutex) is what keeps a repo blocked on its mutex from holding a global permit.
async fn run_job<J, W>(
    repo: DueRepo,
    clock: Arc<dyn Clock>,
    locks: RepoLocks,
    semaphore: Arc<Semaphore>,
    job_runner: Arc<J>,
    outcome_writer: Arc<W>,
    git: SystemGitEngine,
) where
    J: JobRunner,
    W: OutcomeWriter,
{
    // 1. Per-repo mutex FIRST (serializes all git work for this repo, so a manual
    //    "check now" on the same repo waits behind a scheduled check).
    let lock = locks.lock_handle(repo.id);
    let guard = lock.lock_owned().await;
    // 2. Global semaphore permit SECOND (caps total concurrent git work). The
    //    fixed order matters: a job blocked on its per-repo mutex above is NOT
    //    holding a permit, so it cannot starve the global cap.
    let permit = semaphore
        .acquire_owned()
        .await
        .expect("scheduler semaphore is never closed");
    // 3. The git work, through the SHARED E-07 decide -> execute path, with NO DB
    //    transaction open across it. The engine is the one resolved from the live
    //    handle at tick start (BL-NI-23), so it is always the current git.
    let outcome = job_runner.run(repo.id, git).await;
    // Release the global permit as soon as the git work is done - the DB write
    // below needs no git slot. Releasing the permit before the mutex is the
    // reverse of the acquisition order.
    drop(permit);
    // next_check_at is computed from the job's COMPLETION time (AC6: "after each
    // job"), read here from the injected clock - NOT from the tick-start time,
    // which would schedule a slow repo's next check in the past and busy-loop it.
    let completed = clock.now_unix();
    // 4. Classify via the E-07 failure state machine (using the prior count read
    //    with the due repo) and persist the outcome (a short txn).
    let status = classify_failure(repo.consecutive_failures, outcome);
    if let Err(e) = outcome_writer.record(&repo, completed, status).await {
        eprintln!(
            "scheduler: failed to record outcome for repo {}: {e}",
            repo.id.0
        );
    }
    // 5. Release the per-repo mutex LAST (after the outcome is recorded).
    drop(guard);
}

// =============================================================================
// Production seam implementations (DB + git backed).
// =============================================================================

/// The production [`DueQuery`]: selects due repos (`enabled = 1` AND
/// `auto_paused = 0` AND due by `next_check_at`) and reads the quiet-hours window
/// from `settings`, via the sqlx runtime query API. A `next_check_at` of NULL
/// means "never scheduled", which is due immediately (a newly-added repo).
pub struct DbDueQuery {
    pool: SqlitePool,
}

impl DbDueQuery {
    pub fn new(pool: SqlitePool) -> DbDueQuery {
        DbDueQuery { pool }
    }
}

impl DueQuery for DbDueQuery {
    async fn select_due(&self, now_unix: i64) -> Result<DueSelection, AppError> {
        // Seed + read the singleton quiet-hours window (idempotent seed, matching
        // store::settings_get).
        sqlx::query("INSERT OR IGNORE INTO settings (id) VALUES (1)")
            .execute(&self.pool)
            .await?;
        let s = sqlx::query("SELECT quiet_hours_start, quiet_hours_end FROM settings WHERE id = 1")
            .fetch_one(&self.pool)
            .await?;
        let quiet_hours = (
            s.try_get::<Option<i64>, _>("quiet_hours_start")?,
            s.try_get::<Option<i64>, _>("quiet_hours_end")?,
        );

        let rows = sqlx::query(
            "SELECT r.id AS id, r.check_frequency_min AS check_frequency_min, \
                s.consecutive_failures AS consecutive_failures \
             FROM repos r \
             JOIN repo_local_state s ON s.repo_id = r.id \
             WHERE r.enabled = 1 AND s.auto_paused = 0 \
               AND (s.next_check_at IS NULL OR s.next_check_at <= ?) \
             ORDER BY r.id ASC",
        )
        .bind(now_unix)
        .fetch_all(&self.pool)
        .await?;

        let mut candidates = Vec::with_capacity(rows.len());
        for row in &rows {
            candidates.push(DueRepo {
                id: RepoId(row.try_get("id")?),
                check_frequency_min: row.try_get("check_frequency_min")?,
                consecutive_failures: row.try_get("consecutive_failures")?,
            });
        }
        Ok(DueSelection {
            quiet_hours,
            candidates,
        })
    }
}

/// The production [`OutcomeWriter`]: persists `next_check_at` and the
/// failure-counter / auto-pause columns in one short UPDATE (no transaction held
/// across any network call - the git work already finished in the job runner).
pub struct DbOutcomeWriter {
    pool: SqlitePool,
}

impl DbOutcomeWriter {
    pub fn new(pool: SqlitePool) -> DbOutcomeWriter {
        DbOutcomeWriter { pool }
    }
}

impl OutcomeWriter for DbOutcomeWriter {
    async fn record(
        &self,
        repo: &DueRepo,
        now_unix: i64,
        status: RepoStatus,
    ) -> Result<(), AppError> {
        // Read the LIVE global cadence so a repo whose check_frequency_min is 0
        // (the inherit sentinel) follows settings.global_check_minutes. Seed the
        // singleton the same idempotent way store::settings_get and the due-query
        // do, then read just the one column. Passing the compile-time
        // DEFAULT_FREQUENCY_MIN here (the old bug, BL-NI-20) made the global
        // control a no-op.
        sqlx::query("INSERT OR IGNORE INTO settings (id) VALUES (1)")
            .execute(&self.pool)
            .await?;
        let global_default = sqlx::query("SELECT global_check_minutes FROM settings WHERE id = 1")
            .fetch_one(&self.pool)
            .await?
            .try_get::<i64, _>("global_check_minutes")?;
        let freq = effective_frequency_min(repo.check_frequency_min, global_default);
        let next = next_check_at(now_unix, freq);
        let (consecutive_failures, auto_paused) = persist_columns(status);
        sqlx::query(
            "UPDATE repo_local_state \
             SET next_check_at = ?, consecutive_failures = ?, auto_paused = ? \
             WHERE repo_id = ?",
        )
        .bind(next)
        .bind(consecutive_failures)
        .bind(if auto_paused { 1_i64 } else { 0_i64 })
        .bind(repo.id.0)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

/// Recompute `next_check_at` for every INHERIT-mode repo after the global cadence
/// changed, so a lowered or raised `settings.global_check_minutes` takes effect on
/// already-scheduled repos WITHOUT waiting out their stale timestamps
/// (BL-NI-25 / finding 4). Called by the edge's `settings_set` handler when (and
/// only when) the global cadence actually changed.
///
/// Invariant: for each ENABLED, non-auto-paused repo whose `check_frequency_min`
/// is `0` (the inherit sentinel) that has completed at least one check
/// (`last_checked_at IS NOT NULL`), the next check becomes
/// `max(now, last_checked_at + new_global_minutes * 60)`:
///   - a LOWERED cadence can put that time in the past, which `max(now, ..)`
///     clamps to `now` so the repo becomes due on the very NEXT tick (never
///     scheduled retroactively in the past, which would busy-loop it);
///   - a RAISED cadence pushes the next check further out from the last check, so
///     the longer interval takes effect immediately.
///
/// A repo with a POSITIVE per-repo override is untouched (its cadence did not
/// change). A never-checked repo (`last_checked_at IS NULL`) is left as-is: it is
/// already due now, so there is nothing to bring forward. Disabled and
/// auto-paused repos are left alone - they are not being scheduled, so
/// rescheduling them would only pre-empt the enable/unpause path. This mirrors
/// [`next_check_at`]'s minute->second cadence and future-clamp, anchored on the
/// last completed check instead of a fresh completion time.
///
/// Returns the number of repo rows rescheduled.
pub async fn reschedule_inherit_repos(
    pool: &SqlitePool,
    now_unix: i64,
    new_global_minutes: i64,
) -> Result<u64, AppError> {
    // The global cadence is validated `>= 1` before it is persisted; clamp
    // defensively so a bad value can never schedule a check in the past.
    let freq_secs = new_global_minutes.max(1) * 60;
    let res = sqlx::query(
        "UPDATE repo_local_state \
         SET next_check_at = MAX(?, last_checked_at + ?) \
         WHERE last_checked_at IS NOT NULL \
           AND auto_paused = 0 \
           AND repo_id IN ( \
               SELECT id FROM repos WHERE check_frequency_min = 0 AND enabled = 1 \
           )",
    )
    .bind(now_unix)
    .bind(freq_secs)
    .execute(pool)
    .await?;
    Ok(res.rows_affected())
}

/// The production [`JobRunner`]: runs each repo through the SHARED
/// [`crate::repo::update_now_scheduled`] path (decide via E-07, execute via E-03),
/// returning the classified [`RunOutcome`]. A hard error maps to a transient
/// network failure (the retry path), except an auth error which pauses - the same
/// auth-vs-transient split the in-Ok-path classification makes. The git engine is
/// supplied per job by the scheduler (read from the live handle each cycle), not
/// owned here, so a re-probe is honored without a restart (BL-NI-23).
pub struct UpdateNowJobRunner {
    pool: SqlitePool,
}

impl UpdateNowJobRunner {
    pub fn new(pool: SqlitePool) -> UpdateNowJobRunner {
        UpdateNowJobRunner { pool }
    }
}

impl JobRunner for UpdateNowJobRunner {
    async fn run(&self, id: RepoId, git: SystemGitEngine) -> RunOutcome {
        // `git` is the engine the scheduler read from the live handle this cycle,
        // so a re-probe (settings fixing the git path) is honored without a
        // restart - the runner never holds a stale clone (BL-NI-23).
        match repo::update_now_scheduled(&self.pool, &git, id).await {
            Ok(scheduled) => scheduled.run_outcome,
            Err(AppError::AuthFailed) => RunOutcome::AuthFailure,
            Err(_) => RunOutcome::NetworkFailure,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicI64;
    use tokio::sync::Barrier;

    // --- quiet hours (the due-query's clock-driven gate) ----------------------

    #[test]
    fn quiet_hours_none_configured_is_never_quiet() {
        // No window, or a half-open (malformed) window, is never quiet.
        assert!(!in_quiet_hours(600, None, None));
        assert!(!in_quiet_hours(600, Some(60), None));
        assert!(!in_quiet_hours(600, None, Some(60)));
    }

    #[test]
    fn quiet_hours_same_day_window_is_half_open() {
        // 09:00 (540) to 17:00 (1020): quiet at/after start, up to but not
        // including end.
        let (start, end) = (Some(540), Some(1020));
        assert!(in_quiet_hours(540, start, end), "start is inclusive");
        assert!(in_quiet_hours(700, start, end), "inside the window");
        assert!(!in_quiet_hours(1020, start, end), "end is exclusive");
        assert!(!in_quiet_hours(539, start, end), "before the window");
        assert!(!in_quiet_hours(1021, start, end), "after the window");
    }

    #[test]
    fn quiet_hours_wraps_midnight() {
        // 22:00 (1320) to 07:00 (420): quiet late at night AND early morning,
        // active during the day.
        let (start, end) = (Some(1320), Some(420));
        assert!(in_quiet_hours(1320, start, end), "start is inclusive");
        assert!(in_quiet_hours(1439, start, end), "just before midnight");
        assert!(in_quiet_hours(0, start, end), "midnight");
        assert!(in_quiet_hours(419, start, end), "just before end");
        assert!(!in_quiet_hours(420, start, end), "end is exclusive");
        assert!(!in_quiet_hours(700, start, end), "midday is active");
    }

    #[test]
    fn quiet_hours_zero_width_is_never_quiet() {
        // start == end is a zero-width window: there is no quiet minute.
        let (start, end) = (Some(600), Some(600));
        assert!(!in_quiet_hours(600, start, end));
        assert!(!in_quiet_hours(599, start, end));
        assert!(!in_quiet_hours(601, start, end));
    }

    // --- local minute-of-day + non-UTC quiet-window gate (BL-NI-21c) ----------

    #[test]
    fn local_minutes_at_derives_from_injected_offset_only() {
        // The local minute-of-day depends ONLY on the injected offset and the UTC
        // instant, never on the host's configured timezone (there is no timezone
        // read here). Pinned instants make the arithmetic exact and boundary-safe.
        // Epoch (00:00 UTC) is minute 0 in UTC.
        assert_eq!(local_minutes_at(0, 0), 0);
        // 23:59 UTC (86340s) is minute 1439 - the top boundary.
        assert_eq!(local_minutes_at(86_340, 0), 1439);
        // A negative offset wraps backward across midnight without going negative:
        // epoch at UTC-8 (PST, -480) is 16:00 the previous local day (minute 960).
        assert_eq!(local_minutes_at(0, -480), 960);
        // A whole-day offset is a no-op on the minute-of-day (rem_euclid keeps it
        // in range even when local_secs goes negative).
        assert_eq!(local_minutes_at(0, -1440), 0);
        // A positive offset wraps forward past midnight: 23:59 UTC + 2h = 01:59.
        assert_eq!(local_minutes_at(86_340, 120), 119);
    }

    #[test]
    fn non_utc_quiet_window_gate_uses_local_time_not_utc() {
        // A user in PST (UTC-8) with a 22:00..07:00 quiet window that spans
        // midnight. Pick a single UTC instant that is 23:00 LOCAL (inside the quiet
        // window) but 07:00 UTC (outside it). The gate must decide from the LOCAL
        // minute, so the SAME instant is quiet in PST and active in UTC - proving
        // the decision follows the injected offset, not the wall/UTC clock.
        let (start, end) = (Some(1320), Some(420)); // 22:00 .. 07:00, wraps midnight
        let pst = -480; // UTC-8
                        // 111_600s = 1970-01-02 07:00:00 UTC. In PST that is 1970-01-01 23:00.
        let now_unix = 111_600;
        assert_eq!(local_minutes_at(now_unix, pst), 1380, "23:00 local (PST)");
        assert_eq!(local_minutes_at(now_unix, 0), 420, "07:00 UTC");
        assert!(
            in_quiet_hours(local_minutes_at(now_unix, pst), start, end),
            "23:00 PST is inside the 22:00..07:00 quiet window"
        );
        assert!(
            !in_quiet_hours(local_minutes_at(now_unix, 0), start, end),
            "the same instant read as 07:00 UTC is OUTSIDE the window (end is exclusive)"
        );

        // The production SystemClock derives its local minute the same way (offset
        // injected via `with_utc_offset_minutes`), so it never reads the machine's
        // timezone. Verify the clock delegates to the pure helper. Bracket the live
        // read with two now_unix samples so a minute boundary crossing between reads
        // cannot make this flaky: the live minute must equal the helper at one end.
        let clock = SystemClock::with_utc_offset_minutes(pst);
        let before = clock.now_unix();
        let live = clock.local_minutes_of_day();
        let after = clock.now_unix();
        assert!(
            live == local_minutes_at(before, pst) || live == local_minutes_at(after, pst),
            "SystemClock's local minute is the injected-offset computation, not a tz read"
        );

        // A midnight boundary inside the wrap window: local minute 0 is quiet.
        assert!(
            in_quiet_hours(local_minutes_at(28_800, pst), start, end),
            "00:00 local (PST) is inside 22:00..07:00"
        );
    }

    // --- effective frequency (per-repo override vs global default) ------------

    #[test]
    fn effective_frequency_prefers_repo_override() {
        // A positive per-repo frequency wins over the global default.
        assert_eq!(effective_frequency_min(120, 360), 120);
    }

    #[test]
    fn effective_frequency_falls_back_to_global_then_floor() {
        // A non-positive per-repo frequency falls back to the global default.
        assert_eq!(effective_frequency_min(0, 360), 360);
        assert_eq!(effective_frequency_min(-5, 360), 360);
        // A non-positive global too falls back to the hard floor.
        assert_eq!(effective_frequency_min(0, 0), DEFAULT_FREQUENCY_MIN);
    }

    // --- next_check_at math ---------------------------------------------------

    #[test]
    fn next_check_at_adds_frequency_minutes() {
        // 6h (360 min) past a fixed instant.
        assert_eq!(next_check_at(1_000_000, 360), 1_000_000 + 360 * 60);
        // A per-repo override of 30 min.
        assert_eq!(next_check_at(1_000_000, 30), 1_000_000 + 30 * 60);
    }

    #[test]
    fn next_check_at_guards_nonpositive_frequency() {
        // A zero/negative frequency must never schedule a check in the past or
        // now; it clamps to the default floor.
        assert_eq!(
            next_check_at(1_000_000, 0),
            1_000_000 + DEFAULT_FREQUENCY_MIN * 60
        );
        assert_eq!(
            next_check_at(1_000_000, -1),
            1_000_000 + DEFAULT_FREQUENCY_MIN * 60
        );
    }

    // --- failure-counter / auto-pause persistence mapping ---------------------

    #[test]
    fn persist_active_resets_counter_and_unpauses() {
        assert_eq!(persist_columns(RepoStatus::Active), (0, false));
    }

    #[test]
    fn persist_retry_keeps_running_count_unpaused() {
        assert_eq!(
            persist_columns(RepoStatus::Retry {
                consecutive_failures: 2
            }),
            (2, false)
        );
    }

    #[test]
    fn persist_auto_paused_sets_pause_and_resets_counter() {
        // The 3-strikes auto-pause: flag set, counter reset (the engine signalled
        // the pause; E-08 persists auto_paused = 1 and resets the count).
        assert_eq!(persist_columns(RepoStatus::AutoPaused), (0, true));
    }

    #[test]
    fn persist_paused_on_auth_sets_pause() {
        // An auth failure pauses immediately; the single auto_paused flag is set.
        assert_eq!(persist_columns(RepoStatus::PausedOnAuth), (0, true));
    }

    // --- jitter clamp ---------------------------------------------------------

    #[test]
    fn clamp_jitter_bounds_to_range() {
        assert_eq!(clamp_jitter(15, 30), 15, "in range passes through");
        assert_eq!(clamp_jitter(0, 30), 0, "zero is allowed");
        assert_eq!(clamp_jitter(30, 30), 30, "ceiling is allowed");
        assert_eq!(clamp_jitter(40, 30), 30, "over ceiling clamps down");
        assert_eq!(clamp_jitter(-5, 30), 0, "negative clamps up to zero");
    }

    // =========================================================================
    // Orchestration test harness (fakes for the four seams).
    // =========================================================================

    /// A deterministic [`Clock`]: both fields are set by the test and never
    /// advance on their own.
    struct FakeClock {
        now: AtomicI64,
        minutes: AtomicI64,
    }
    impl FakeClock {
        fn new(now: i64, minutes: i64) -> Arc<FakeClock> {
            Arc::new(FakeClock {
                now: AtomicI64::new(now),
                minutes: AtomicI64::new(minutes),
            })
        }
    }
    impl Clock for FakeClock {
        fn now_unix(&self) -> i64 {
            self.now.load(Ordering::SeqCst)
        }
        fn local_minutes_of_day(&self) -> i64 {
            self.minutes.load(Ordering::SeqCst)
        }
    }

    /// A [`Clock`] whose `now_unix` ADVANCES by `step` on each call, so a test can
    /// tell tick-start time (the first read) from job-completion time (a later
    /// read). `local_minutes_of_day` is fixed and does not advance.
    struct AdvancingClock {
        now: AtomicI64,
        step: i64,
        minutes: i64,
    }
    impl AdvancingClock {
        fn new(start: i64, step: i64, minutes: i64) -> Arc<AdvancingClock> {
            Arc::new(AdvancingClock {
                now: AtomicI64::new(start),
                step,
                minutes,
            })
        }
    }
    impl Clock for AdvancingClock {
        fn now_unix(&self) -> i64 {
            // fetch_add returns the PRE-increment value, so the first call yields
            // `start`, the next `start + step`, and so on.
            self.now.fetch_add(self.step, Ordering::SeqCst)
        }
        fn local_minutes_of_day(&self) -> i64 {
            self.minutes
        }
    }

    /// A [`Jitter`] that returns a fixed value and COUNTS how many times it was
    /// consulted (so a test can prove startup jitters each repo and a steady tick
    /// jitters none).
    struct FakeJitter {
        calls: AtomicI64,
        value: i64,
    }
    impl FakeJitter {
        fn new(value: i64) -> Arc<FakeJitter> {
            Arc::new(FakeJitter {
                calls: AtomicI64::new(0),
                value,
            })
        }
        fn calls(&self) -> i64 {
            self.calls.load(Ordering::SeqCst)
        }
    }
    impl Jitter for FakeJitter {
        fn jitter_secs(&self, _max_inclusive: i64) -> i64 {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.value
        }
    }

    /// A [`Jitter`] that PANICS if consulted - used to prove a steady-state tick
    /// never jitters.
    struct PanicJitter;
    impl Jitter for PanicJitter {
        fn jitter_secs(&self, _max_inclusive: i64) -> i64 {
            panic!("jitter must not be consulted on a steady-state tick");
        }
    }

    /// Shared instrumentation: a logical clock plus concurrency counters, an
    /// event log, and the git/select/db call windows used by the no-lock test.
    #[derive(Clone, Default)]
    struct Instrument {
        tick: Arc<AtomicI64>,
        in_flight: Arc<AtomicI64>,
        max_in_flight: Arc<AtomicI64>,
        git_windows: Arc<StdMutex<Vec<(i64, i64)>>>,
        select_windows: Arc<StdMutex<Vec<(i64, i64)>>>,
        db_windows: Arc<StdMutex<Vec<(i64, i64)>>>,
        events: Arc<StdMutex<Vec<String>>>,
    }
    impl Instrument {
        fn stamp(&self) -> i64 {
            self.tick.fetch_add(1, Ordering::SeqCst)
        }
        fn enter_git(&self, id: i64) -> i64 {
            let f = self.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
            self.max_in_flight.fetch_max(f, Ordering::SeqCst);
            self.events.lock().unwrap().push(format!("enter:{id}"));
            self.stamp()
        }
        fn exit_git(&self, id: i64, start: i64) {
            self.in_flight.fetch_sub(1, Ordering::SeqCst);
            let end = self.stamp();
            self.events.lock().unwrap().push(format!("exit:{id}"));
            self.git_windows.lock().unwrap().push((start, end));
        }
        fn max(&self) -> i64 {
            self.max_in_flight.load(Ordering::SeqCst)
        }
        fn events(&self) -> Vec<String> {
            self.events.lock().unwrap().clone()
        }
    }

    /// A [`JobRunner`] that records a git window (via [`Instrument`]), optionally
    /// blocks on a barrier so multiple jobs are provably in-flight at once, and
    /// returns a per-id outcome (default [`RunOutcome::Success`]).
    struct FakeJobRunner {
        inst: Instrument,
        gate: Option<Arc<Barrier>>,
        outcomes: HashMap<i64, RunOutcome>,
    }
    impl FakeJobRunner {
        fn new(inst: Instrument) -> FakeJobRunner {
            FakeJobRunner {
                inst,
                gate: None,
                outcomes: HashMap::new(),
            }
        }
        fn with_gate(inst: Instrument, gate: Arc<Barrier>) -> FakeJobRunner {
            FakeJobRunner {
                inst,
                gate: Some(gate),
                outcomes: HashMap::new(),
            }
        }
        fn with_outcomes(inst: Instrument, outcomes: HashMap<i64, RunOutcome>) -> FakeJobRunner {
            FakeJobRunner {
                inst,
                gate: None,
                outcomes,
            }
        }
    }
    impl JobRunner for FakeJobRunner {
        async fn run(&self, id: RepoId, _git: SystemGitEngine) -> RunOutcome {
            // The fake drives git through the instrument, not the real engine, so
            // the injected engine is ignored - it only exists to prove the
            // scheduler passes the live one through (BL-NI-23).
            let start = self.inst.enter_git(id.0);
            if let Some(b) = &self.gate {
                b.wait().await;
            }
            let outcome = self
                .outcomes
                .get(&id.0)
                .copied()
                .unwrap_or(RunOutcome::Success);
            self.inst.exit_git(id.0, start);
            outcome
        }
    }

    /// A [`GitEngineSource`] that always reports git PRESENT via a fabricated
    /// engine (no `git --version` subprocess, no host-git dependency), so the tick
    /// gate passes for the orchestration tests whose fake job runner never touches
    /// the real engine. The absent/re-probe path is covered separately through the
    /// production [`SharedGitEngineSource`] seam
    /// (`scheduler_skips_when_git_absent_then_runs_after_live_swap`).
    struct FakeEngineSource;
    impl FakeEngineSource {
        fn present() -> Arc<FakeEngineSource> {
            Arc::new(FakeEngineSource)
        }
    }
    impl GitEngineSource for FakeEngineSource {
        fn current(&self) -> Pin<Box<dyn Future<Output = Option<SystemGitEngine>> + Send>> {
            Box::pin(async move { Some(SystemGitEngine::fabricated_for_test()) })
        }
    }

    /// A [`DueQuery`] that returns a fixed selection.
    struct FakeDueQuery {
        selection: DueSelection,
    }
    impl DueQuery for FakeDueQuery {
        async fn select_due(&self, _now_unix: i64) -> Result<DueSelection, AppError> {
            Ok(self.selection.clone())
        }
    }

    /// A [`DueQuery`] that also stamps a "select transaction" window, to prove the
    /// select txn closes before any git call.
    struct InstrumentedDueQuery {
        inst: Instrument,
        selection: DueSelection,
    }
    impl DueQuery for InstrumentedDueQuery {
        async fn select_due(&self, _now_unix: i64) -> Result<DueSelection, AppError> {
            let s = self.inst.stamp();
            let sel = self.selection.clone();
            let e = self.inst.stamp();
            self.inst.select_windows.lock().unwrap().push((s, e));
            Ok(sel)
        }
    }

    /// An [`OutcomeWriter`] that records each `(id, now, status)` and optionally
    /// stamps a "db transaction" window.
    #[derive(Clone)]
    struct FakeOutcomeWriter {
        inst: Option<Instrument>,
        recorded: Arc<StdMutex<Vec<(i64, i64, RepoStatus)>>>,
    }
    impl FakeOutcomeWriter {
        fn new() -> FakeOutcomeWriter {
            FakeOutcomeWriter {
                inst: None,
                recorded: Arc::new(StdMutex::new(Vec::new())),
            }
        }
        fn instrumented(inst: Instrument) -> FakeOutcomeWriter {
            FakeOutcomeWriter {
                inst: Some(inst),
                recorded: Arc::new(StdMutex::new(Vec::new())),
            }
        }
        fn recorded(&self) -> Vec<(i64, i64, RepoStatus)> {
            self.recorded.lock().unwrap().clone()
        }
    }
    impl OutcomeWriter for FakeOutcomeWriter {
        async fn record(
            &self,
            repo: &DueRepo,
            now_unix: i64,
            status: RepoStatus,
        ) -> Result<(), AppError> {
            match &self.inst {
                Some(inst) => {
                    let s = inst.stamp();
                    self.recorded
                        .lock()
                        .unwrap()
                        .push((repo.id.0, now_unix, status));
                    let e = inst.stamp();
                    inst.db_windows.lock().unwrap().push((s, e));
                }
                None => {
                    self.recorded
                        .lock()
                        .unwrap()
                        .push((repo.id.0, now_unix, status));
                }
            }
            Ok(())
        }
    }

    fn due(id: i64) -> DueRepo {
        DueRepo {
            id: RepoId(id),
            // 0 = inherit the global cadence (the default for a newly-added repo
            // under the INHERIT model). The fake outcome writer ignores this, so
            // the value only documents the model for these orchestration tests.
            check_frequency_min: 0,
            consecutive_failures: 0,
        }
    }
    fn selection(candidates: Vec<DueRepo>) -> DueSelection {
        DueSelection {
            quiet_hours: (None, None),
            candidates,
        }
    }

    // =========================================================================
    // Concurrency: global cap, per-repo serialization, acquisition order.
    // =========================================================================

    #[tokio::test(flavor = "multi_thread", worker_threads = 8)]
    async fn global_semaphore_caps_concurrent_jobs() {
        // With a semaphore of `cap`, at most `cap` git jobs run at once. A barrier
        // of size `cap` lets each wave assemble (proving they overlap up to the
        // cap) and trips; the (cap+1)th can only start once a permit frees, so the
        // observed max in-flight is exactly `cap`. A broken cap would let more than
        // `cap` enter and the max would exceed `cap`.
        let cap = 2usize;
        let inst = Instrument::default();
        let barrier = Arc::new(Barrier::new(cap));
        let runner = FakeJobRunner::with_gate(inst.clone(), barrier);
        // cap*2 DISTINCT repos so per-repo mutexes never serialize them.
        let dq = FakeDueQuery {
            selection: selection((1..=(cap as i64 * 2)).map(due).collect()),
        };
        let sched = Scheduler::new(
            FakeClock::new(1000, 600),
            FakeJitter::new(0),
            FakeEngineSource::present(),
            dq,
            runner,
            FakeOutcomeWriter::new(),
            cap,
        );
        let n = sched.tick_once().await.expect("tick");
        assert_eq!(n, cap * 2);
        assert_eq!(
            inst.max(),
            cap as i64,
            "no more than `cap` git jobs may run concurrently"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn different_repos_run_concurrently() {
        // Two DIFFERENT repos must be able to run at once: a barrier of 2 only
        // trips if both are in-flight together, so a passing test proves overlap.
        let inst = Instrument::default();
        let barrier = Arc::new(Barrier::new(2));
        let runner = FakeJobRunner::with_gate(inst.clone(), barrier);
        let dq = FakeDueQuery {
            selection: selection(vec![due(1), due(2)]),
        };
        let sched = Scheduler::new(
            FakeClock::new(1000, 600),
            FakeJitter::new(0),
            FakeEngineSource::present(),
            dq,
            runner,
            FakeOutcomeWriter::new(),
            4,
        );
        sched.tick_once().await.expect("tick");
        assert_eq!(
            inst.max(),
            2,
            "two different repos run concurrently (the barrier of 2 trips)"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn same_repo_serializes_via_per_repo_mutex() {
        // Two jobs on the SAME repo must NOT overlap: the per-repo mutex serializes
        // them, so the event log is two complete, non-interleaved runs.
        let inst = Instrument::default();
        let runner = FakeJobRunner::new(inst.clone());
        let dq = FakeDueQuery {
            selection: selection(vec![due(7), due(7)]), // same id twice
        };
        let sched = Scheduler::new(
            FakeClock::new(1000, 600),
            FakeJitter::new(0),
            FakeEngineSource::present(),
            dq,
            runner,
            FakeOutcomeWriter::new(),
            4,
        );
        sched.tick_once().await.expect("tick");
        let ev = inst.events();
        assert_eq!(
            ev,
            vec!["enter:7", "exit:7", "enter:7", "exit:7"],
            "two jobs on one repo must serialize (no interleave), got {ev:?}"
        );
    }

    #[tokio::test]
    async fn blocked_on_mutex_holds_no_global_permit() {
        // The acquisition-order guarantee (per-repo mutex FIRST, then the
        // semaphore permit): a job blocked on its per-repo mutex holds NO global
        // permit. If the order were swapped (permit first), the blocked job would
        // hold a permit and `available_permits()` would drop below `cap`, failing
        // this assertion.
        let cap = 2usize;
        let inst = Instrument::default();
        let runner = Arc::new(FakeJobRunner::new(inst.clone()));
        let writer = Arc::new(FakeOutcomeWriter::new());
        let sem = Arc::new(Semaphore::new(cap));
        let locks = RepoLocks::default();
        let id = RepoId(1);

        // The TEST holds the repo's mutex, so a job for it must block on the mutex.
        let held = locks.lock_handle(id);
        let guard = held.lock_owned().await;

        let task = {
            let (locks, sem, runner, writer) =
                (locks.clone(), sem.clone(), runner.clone(), writer.clone());
            tokio::spawn(async move {
                run_job(
                    DueRepo {
                        id,
                        check_frequency_min: 0,
                        consecutive_failures: 0,
                    },
                    FakeClock::new(1000, 600),
                    locks,
                    sem,
                    runner,
                    writer,
                    SystemGitEngine::fabricated_for_test(),
                )
                .await;
            })
        };

        // Let the spawned task run up to its first await (the contended mutex).
        for _ in 0..64 {
            tokio::task::yield_now().await;
        }
        assert_eq!(
            sem.available_permits(),
            cap,
            "a job blocked on its per-repo mutex must hold NO global permit (mutex is acquired before the permit)"
        );
        assert_eq!(
            inst.max(),
            0,
            "the blocked job must not have entered the git work"
        );

        // Release the mutex; the job proceeds and finishes cleanly.
        drop(guard);
        task.await.expect("job task joins");
        assert_eq!(
            sem.available_permits(),
            cap,
            "the global permit is released after the job finishes"
        );
        assert_eq!(inst.events(), vec!["enter:1", "exit:1"]);
    }

    #[tokio::test]
    async fn no_db_transaction_is_held_across_the_git_call() {
        // The git call must run with NO DB transaction open: the select txn closes
        // before the git call starts, and the outcome-write txn opens only after
        // the git call ends. Logical-clock stamps make the windows comparable.
        let inst = Instrument::default();
        let runner = FakeJobRunner::new(inst.clone());
        let writer = FakeOutcomeWriter::instrumented(inst.clone());
        let dq = InstrumentedDueQuery {
            inst: inst.clone(),
            selection: selection(vec![due(1)]),
        };
        let sched = Scheduler::new(
            FakeClock::new(1000, 600),
            FakeJitter::new(0),
            FakeEngineSource::present(),
            dq,
            runner,
            writer,
            4,
        );
        sched.tick_once().await.expect("tick");

        let git = inst.git_windows.lock().unwrap().clone();
        let sel = inst.select_windows.lock().unwrap().clone();
        let db = inst.db_windows.lock().unwrap().clone();
        assert_eq!(git.len(), 1, "one git call");
        assert_eq!(sel.len(), 1, "one select");
        assert_eq!(db.len(), 1, "one db write");
        let (g0, g1) = git[0];
        let (_s0, s1) = sel[0];
        let (d0, d1) = db[0];
        assert!(
            s1 <= g0,
            "the due-query txn must close before the git call (select ..{s1}, git {g0}..)"
        );
        assert!(
            g1 <= d0,
            "the outcome-write txn must open only after the git call (git ..{g1}, db {d0}..{d1})"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn manual_op_and_scheduled_job_on_same_repo_serialize() {
        // BL-NI-21(a): a manual `repo_check_now` / `repo_update_now` racing a
        // scheduled check on the SAME repo must serialize on the shared per-repo
        // mutex, so no two `git` processes ever run in one working tree.
        //
        // The manual command path holds `state.locks.lock_handle(id).lock_owned()`
        // across its whole check (src-tauri `commands/mod.rs`: `repo_check_now`
        // line 57, `repo_update_now` line 174). `state.locks` IS the value returned
        // by `Scheduler::locks()` and handed to `AppState` in `lib.rs`. This test
        // obtains the lock handle the SAME way AppState does (via `sched.locks()`),
        // simulating the manual critical section, and proves a scheduled tick on
        // that repo blocks until the manual op releases - then runs exactly once,
        // never overlapping.
        let inst = Instrument::default();
        let runner = FakeJobRunner::new(inst.clone());
        let dq = FakeDueQuery {
            selection: selection(vec![due(7)]),
        };
        let sched = Arc::new(Scheduler::new(
            FakeClock::new(1000, 600),
            FakeJitter::new(0),
            FakeEngineSource::present(),
            dq,
            runner,
            FakeOutcomeWriter::new(),
            4,
        ));

        // The manual command path's lock, obtained exactly as AppState obtains it.
        let locks = sched.locks();
        let manual = locks.lock_handle(RepoId(7));
        let manual_guard = manual.lock_owned().await;

        // Start a scheduled tick for repo 7 while the manual op holds the lock.
        let tick = {
            let sched = sched.clone();
            tokio::spawn(async move { sched.tick_once().await })
        };

        // Let the scheduled job reach its per-repo mutex acquisition and block.
        for _ in 0..64 {
            tokio::task::yield_now().await;
        }
        assert_eq!(
            inst.max(),
            0,
            "the scheduled job must NOT enter git work while the manual op holds the repo lock"
        );
        assert!(
            inst.events().is_empty(),
            "no git entry/exit while blocked on the shared per-repo mutex, got {:?}",
            inst.events()
        );

        // Release the manual op; the scheduled job now proceeds - exactly once.
        drop(manual_guard);
        let ran = tick
            .await
            .expect("tick task joins")
            .expect("tick completes ok");
        assert_eq!(
            ran, 1,
            "the scheduled tick runs the one due repo once the manual op releases"
        );
        assert_eq!(
            inst.events(),
            vec!["enter:7", "exit:7"],
            "the scheduled job runs to completion once unblocked"
        );
        assert_eq!(
            inst.max(),
            1,
            "manual and scheduled never run two git ops on one repo at once"
        );
    }

    // =========================================================================
    // Quiet hours gating (clock-driven).
    // =========================================================================

    #[tokio::test]
    async fn quiet_hours_suppresses_the_whole_tick() {
        // Window 22:00..07:00; now = 23:00 (1380) is inside, so nothing runs.
        let inst = Instrument::default();
        let runner = FakeJobRunner::new(inst.clone());
        let dq = FakeDueQuery {
            selection: DueSelection {
                quiet_hours: (Some(1320), Some(420)),
                candidates: vec![due(1), due(2)],
            },
        };
        let sched = Scheduler::new(
            FakeClock::new(1000, 1380),
            FakeJitter::new(0),
            FakeEngineSource::present(),
            dq,
            runner,
            FakeOutcomeWriter::new(),
            4,
        );
        let n = sched.tick_once().await.expect("tick");
        assert_eq!(n, 0, "inside quiet hours, the tick selects nothing");
        assert_eq!(inst.max(), 0, "no git work starts during quiet hours");
    }

    #[tokio::test]
    async fn outside_quiet_hours_runs_due_repos() {
        // Same window, now = 10:00 (600) is outside, so the due repos run.
        let inst = Instrument::default();
        let runner = FakeJobRunner::new(inst.clone());
        let dq = FakeDueQuery {
            selection: DueSelection {
                quiet_hours: (Some(1320), Some(420)),
                candidates: vec![due(1), due(2)],
            },
        };
        let sched = Scheduler::new(
            FakeClock::new(1000, 600),
            FakeJitter::new(0),
            FakeEngineSource::present(),
            dq,
            runner,
            FakeOutcomeWriter::new(),
            4,
        );
        let n = sched.tick_once().await.expect("tick");
        assert_eq!(n, 2, "outside quiet hours, the due repos run");
    }

    // =========================================================================
    // Jitter: startup staggers each repo; steady ticks never jitter.
    // =========================================================================

    #[tokio::test]
    async fn steady_tick_never_jitters() {
        // PanicJitter panics if consulted; a clean steady tick proves it is not.
        let runner = FakeJobRunner::new(Instrument::default());
        let dq = FakeDueQuery {
            selection: selection(vec![due(1), due(2)]),
        };
        let sched = Scheduler::new(
            FakeClock::new(1000, 600),
            Arc::new(PanicJitter),
            FakeEngineSource::present(),
            dq,
            runner,
            FakeOutcomeWriter::new(),
            4,
        );
        sched
            .tick_once()
            .await
            .expect("a steady tick must not consult the jitter source");
    }

    #[tokio::test]
    async fn startup_pulls_jitter_per_due_repo() {
        // The startup pass staggers EACH due repo: the jitter source is consulted
        // once per repo. Value 0 means no real sleep, so the test stays fast.
        let jitter = FakeJitter::new(0);
        let runner = FakeJobRunner::new(Instrument::default());
        let dq = FakeDueQuery {
            selection: selection(vec![due(1), due(2), due(3)]),
        };
        let sched = Scheduler::new(
            FakeClock::new(1000, 600),
            jitter.clone(),
            FakeEngineSource::present(),
            dq,
            runner,
            FakeOutcomeWriter::new(),
            4,
        );
        sched.start().await.expect("startup pass");
        assert_eq!(
            jitter.calls(),
            3,
            "startup staggers each of the 3 due repos with its own jitter offset"
        );
    }

    // =========================================================================
    // Failure-counter / auto-pause persistence is driven by the E-07 state
    // machine with the prior count read from the due repo.
    // =========================================================================

    #[tokio::test]
    async fn third_consecutive_network_failure_signals_auto_pause() {
        // A repo with 2 prior failures that fails again is the 3rd strike: the
        // scheduler must classify it AutoPaused and the writer must persist that.
        let inst = Instrument::default();
        let mut outcomes = HashMap::new();
        outcomes.insert(5i64, RunOutcome::NetworkFailure);
        let runner = FakeJobRunner::with_outcomes(inst.clone(), outcomes);
        let writer = FakeOutcomeWriter::new();
        let writer_handle = writer.clone();
        let repo = DueRepo {
            id: RepoId(5),
            check_frequency_min: 0,
            consecutive_failures: 2,
        };
        let dq = FakeDueQuery {
            selection: DueSelection {
                quiet_hours: (None, None),
                candidates: vec![repo],
            },
        };
        let sched = Scheduler::new(
            FakeClock::new(1000, 600),
            FakeJitter::new(0),
            FakeEngineSource::present(),
            dq,
            runner,
            writer,
            4,
        );
        sched.tick_once().await.expect("tick");
        let rec = writer_handle.recorded();
        assert_eq!(rec.len(), 1);
        assert_eq!(
            rec[0].2,
            RepoStatus::AutoPaused,
            "2 prior + this failure = 3 strikes -> AutoPaused"
        );
    }

    #[tokio::test]
    async fn success_records_active_status() {
        // A success resets the streak to Active regardless of prior failures.
        let runner = FakeJobRunner::new(Instrument::default()); // default outcome Success
        let writer = FakeOutcomeWriter::new();
        let writer_handle = writer.clone();
        let repo = DueRepo {
            id: RepoId(9),
            check_frequency_min: 0,
            consecutive_failures: 2,
        };
        let dq = FakeDueQuery {
            selection: DueSelection {
                quiet_hours: (None, None),
                candidates: vec![repo],
            },
        };
        let sched = Scheduler::new(
            FakeClock::new(1000, 600),
            FakeJitter::new(0),
            FakeEngineSource::present(),
            dq,
            runner,
            writer,
            4,
        );
        sched.tick_once().await.expect("tick");
        assert_eq!(
            writer_handle.recorded()[0].2,
            RepoStatus::Active,
            "a success resets to Active even after prior failures"
        );
    }

    #[tokio::test]
    async fn next_check_at_uses_job_completion_time() {
        // AC6: next_check_at is recomputed "after each job" from the injected
        // clock. The recorded timestamp must be the COMPLETION time (a later clock
        // read), not tick-start, so a slow repo on a short cadence does not
        // schedule its next check in the past and busy-loop. The advancing clock
        // returns 1000 at tick start and 1100 at job completion.
        let clock = AdvancingClock::new(1000, 100, 600);
        let writer = FakeOutcomeWriter::new();
        let writer_handle = writer.clone();
        let dq = FakeDueQuery {
            selection: selection(vec![due(1)]),
        };
        let sched = Scheduler::new(
            clock,
            FakeJitter::new(0),
            FakeEngineSource::present(),
            dq,
            FakeJobRunner::new(Instrument::default()),
            writer,
            4,
        );
        sched.tick_once().await.expect("tick");
        let rec = writer_handle.recorded();
        assert_eq!(rec.len(), 1);
        assert_eq!(
            rec[0].1, 1100,
            "next_check_at must derive from job-completion time (1100), not tick-start (1000)"
        );
    }

    // =========================================================================
    // The production due-query predicate against a real migrated pool: only
    // enabled, non-auto-paused, due (or never-scheduled) repos are selected.
    // =========================================================================

    #[tokio::test]
    async fn db_due_query_excludes_disabled_paused_and_future() {
        async fn insert(
            pool: &SqlitePool,
            name: &str,
            enabled: i64,
            auto_paused: i64,
            next_check_at: Option<i64>,
        ) -> i64 {
            let id = sqlx::query(
                "INSERT INTO repos (local_name, local_path, created_at, enabled) \
                 VALUES (?, ?, 0, ?)",
            )
            .bind(name)
            .bind(name)
            .bind(enabled)
            .execute(pool)
            .await
            .unwrap()
            .last_insert_rowid();
            sqlx::query(
                "INSERT INTO repo_local_state \
                 (repo_id, auto_paused, next_check_at, consecutive_failures) \
                 VALUES (?, ?, ?, 0)",
            )
            .bind(id)
            .bind(auto_paused)
            .bind(next_check_at)
            .execute(pool)
            .await
            .unwrap();
            id
        }

        let tmp = tempfile::TempDir::new().unwrap();
        let pool = crate::db::open_pool(&tmp.path().join("due.db"))
            .await
            .unwrap();
        crate::db::run_migrations(&pool).await.unwrap();

        let now = 1_000_000i64;
        // Selected: enabled, not paused, past-due OR never-scheduled (NULL).
        let due = insert(&pool, "due", 1, 0, Some(now - 1)).await;
        let null_due = insert(&pool, "never-scheduled", 1, 0, None).await;
        // Excluded: disabled, auto-paused, and not-yet-due.
        insert(&pool, "disabled", 0, 0, Some(now - 1)).await;
        insert(&pool, "paused", 1, 1, Some(now - 1)).await;
        insert(&pool, "future", 1, 0, Some(now + 10_000)).await;

        let selection = DbDueQuery::new(pool.clone()).select_due(now).await.unwrap();
        let mut got: Vec<i64> = selection.candidates.iter().map(|c| c.id.0).collect();
        got.sort();
        let mut want = vec![due, null_due];
        want.sort();
        assert_eq!(
            got, want,
            "only enabled, non-paused, due (or never-scheduled) repos are selected"
        );
        assert_eq!(
            selection.quiet_hours,
            (None, None),
            "default settings carry no quiet-hours window"
        );
    }

    // =========================================================================
    // The production outcome-writer computes next_check_at from the LIVE global
    // cadence for an inherit (check_frequency_min = 0) repo, and honours a
    // positive per-repo override (BL-NI-20: the global control used to be a
    // no-op because the writer passed the compile-time DEFAULT_FREQUENCY_MIN).
    // =========================================================================

    #[tokio::test]
    async fn db_outcome_writer_uses_global_cadence_for_inherit_repo() {
        async fn insert_repo(pool: &SqlitePool, name: &str, freq_min: i64) -> i64 {
            let id = sqlx::query(
                "INSERT INTO repos (local_name, local_path, created_at, check_frequency_min) \
                 VALUES (?, ?, 0, ?)",
            )
            .bind(name)
            .bind(name)
            .bind(freq_min)
            .execute(pool)
            .await
            .unwrap()
            .last_insert_rowid();
            sqlx::query(
                "INSERT INTO repo_local_state (repo_id, consecutive_failures, auto_paused) \
                 VALUES (?, 0, 0)",
            )
            .bind(id)
            .execute(pool)
            .await
            .unwrap();
            id
        }

        async fn read_next_check_at(pool: &SqlitePool, id: i64) -> Option<i64> {
            sqlx::query("SELECT next_check_at FROM repo_local_state WHERE repo_id = ?")
                .bind(id)
                .fetch_one(pool)
                .await
                .unwrap()
                .try_get("next_check_at")
                .unwrap()
        }

        let tmp = tempfile::TempDir::new().unwrap();
        let pool = crate::db::open_pool(&tmp.path().join("cadence.db"))
            .await
            .unwrap();
        crate::db::run_migrations(&pool).await.unwrap();

        // Set a DISTINCTIVE global cadence (120m) so a regression to the 360m
        // const would be visible. Seed the singleton the same idempotent way the
        // writer does, then override the one column.
        sqlx::query("INSERT OR IGNORE INTO settings (id) VALUES (1)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("UPDATE settings SET global_check_minutes = 120 WHERE id = 1")
            .execute(&pool)
            .await
            .unwrap();

        let inherit = insert_repo(&pool, "inherit", 0).await;
        let overridden = insert_repo(&pool, "override", 45).await;

        let writer = DbOutcomeWriter::new(pool.clone());
        let now = 1_000_000i64;

        // An inherit repo (check_frequency_min = 0) schedules from the LIVE global
        // cadence (120m), NOT the compile-time DEFAULT_FREQUENCY_MIN (360m).
        writer
            .record(
                &DueRepo {
                    id: RepoId(inherit),
                    check_frequency_min: 0,
                    consecutive_failures: 0,
                },
                now,
                RepoStatus::Active,
            )
            .await
            .unwrap();
        assert_eq!(
            read_next_check_at(&pool, inherit).await,
            Some(now + 120 * 60),
            "an inherit repo must schedule from the live global cadence (120m), not the 360m const"
        );

        // A positive per-repo override (45m) wins over the global cadence.
        writer
            .record(
                &DueRepo {
                    id: RepoId(overridden),
                    check_frequency_min: 45,
                    consecutive_failures: 0,
                },
                now,
                RepoStatus::Active,
            )
            .await
            .unwrap();
        assert_eq!(
            read_next_check_at(&pool, overridden).await,
            Some(now + 45 * 60),
            "a positive per-repo override wins over the global cadence"
        );
    }

    // =========================================================================
    // Reschedule on global-cadence change (BL-NI-25 / finding 4): lowering or
    // raising settings.global_check_minutes re-cadences already-scheduled INHERIT
    // repos immediately; explicit-override, paused, disabled, and never-checked
    // repos are left untouched.
    // =========================================================================

    #[tokio::test]
    async fn reschedule_recomputes_only_inherit_repos_on_cadence_change() {
        async fn insert_repo(pool: &SqlitePool, name: &str, freq_min: i64, enabled: i64) -> i64 {
            sqlx::query(
                "INSERT INTO repos (local_name, local_path, created_at, check_frequency_min, enabled) \
                 VALUES (?, ?, 0, ?, ?)",
            )
            .bind(name)
            .bind(name)
            .bind(freq_min)
            .bind(enabled)
            .execute(pool)
            .await
            .unwrap()
            .last_insert_rowid()
        }
        async fn set_state(
            pool: &SqlitePool,
            id: i64,
            last_checked_at: Option<i64>,
            next_check_at: Option<i64>,
            auto_paused: i64,
        ) {
            sqlx::query(
                "INSERT INTO repo_local_state \
                 (repo_id, last_checked_at, next_check_at, auto_paused, consecutive_failures) \
                 VALUES (?, ?, ?, ?, 0)",
            )
            .bind(id)
            .bind(last_checked_at)
            .bind(next_check_at)
            .bind(auto_paused)
            .execute(pool)
            .await
            .unwrap();
        }
        async fn next_of(pool: &SqlitePool, id: i64) -> Option<i64> {
            sqlx::query("SELECT next_check_at FROM repo_local_state WHERE repo_id = ?")
                .bind(id)
                .fetch_one(pool)
                .await
                .unwrap()
                .try_get("next_check_at")
                .unwrap()
        }

        let tmp = tempfile::TempDir::new().unwrap();
        let pool = crate::db::open_pool(&tmp.path().join("resched.db"))
            .await
            .unwrap();
        crate::db::run_migrations(&pool).await.unwrap();

        let now = 1_000_000i64;
        let old_next = now + 320 * 60; // scheduled under the old 360m cadence
                                       // Enabled, inherit, last checked 40m ago: the ONLY repo that reschedules.
        let inherit = insert_repo(&pool, "inherit", 0, 1).await;
        set_state(&pool, inherit, Some(now - 40 * 60), Some(old_next), 0).await;
        // Positive per-repo override: its cadence did not change, so untouched.
        let overridden = insert_repo(&pool, "override", 90, 1).await;
        set_state(
            &pool,
            overridden,
            Some(now - 40 * 60),
            Some(now + 50 * 60),
            0,
        )
        .await;
        // Auto-paused inherit repo: not being scheduled, so untouched.
        let paused = insert_repo(&pool, "paused", 0, 1).await;
        set_state(&pool, paused, Some(now - 40 * 60), Some(old_next), 1).await;
        // Disabled inherit repo: not being scheduled, so untouched.
        let disabled = insert_repo(&pool, "disabled", 0, 0).await;
        set_state(&pool, disabled, Some(now - 40 * 60), Some(old_next), 0).await;
        // Never-checked inherit repo: already due, so left as-is (not pushed out).
        let fresh = insert_repo(&pool, "fresh", 0, 1).await;
        set_state(&pool, fresh, None, None, 0).await;

        // LOWER the global cadence to 30m: inherit's next = max(now, (now-40m)+30m)
        // = max(now, now-10m) = now, so it becomes due on the next tick.
        let n = reschedule_inherit_repos(&pool, now, 30).await.unwrap();
        assert_eq!(
            n, 1,
            "only the enabled, non-paused, checked, inherit repo is rescheduled"
        );
        assert_eq!(
            next_of(&pool, inherit).await,
            Some(now),
            "a lowered cadence landing in the past clamps to now (due next tick)"
        );
        assert_eq!(
            next_of(&pool, overridden).await,
            Some(now + 50 * 60),
            "a positive per-repo override is untouched"
        );
        assert_eq!(
            next_of(&pool, paused).await,
            Some(old_next),
            "an auto-paused repo is untouched"
        );
        assert_eq!(
            next_of(&pool, disabled).await,
            Some(old_next),
            "a disabled repo is untouched"
        );
        assert_eq!(
            next_of(&pool, fresh).await,
            None,
            "a never-checked repo stays due (unchanged)"
        );

        // RAISE the global cadence to 600m: recomputed from the last check, so
        // inherit's next = max(now, (now-40m)+600m) = now + 560m.
        let n2 = reschedule_inherit_repos(&pool, now, 600).await.unwrap();
        assert_eq!(n2, 1);
        assert_eq!(
            next_of(&pool, inherit).await,
            Some(now + 560 * 60),
            "a raised cadence pushes the next check out from the last completed check"
        );
    }

    // =========================================================================
    // Live git gate (BL-NI-23 / finding 6): with no git the whole cycle skips
    // (no jobs, no next_check_at writes); a re-probe that swaps an engine into the
    // SAME shared handle is picked up on the very next tick with no restart. Runs
    // through the REAL SharedGitEngineSource seam, so this is the production path.
    // =========================================================================

    #[tokio::test]
    async fn scheduler_skips_when_git_absent_then_runs_after_live_swap() {
        // The shared, swappable handle starts EMPTY: git absent at startup.
        let handle: SharedGitEngine = Arc::new(RwLock::new(None));
        let inst = Instrument::default();
        let runner = FakeJobRunner::new(inst.clone());
        let writer = FakeOutcomeWriter::new();
        let writer_handle = writer.clone();
        let dq = FakeDueQuery {
            selection: selection(vec![due(1), due(2)]),
        };
        let sched = Scheduler::new(
            FakeClock::new(1000, 600),
            FakeJitter::new(0),
            Arc::new(SharedGitEngineSource::new(handle.clone())),
            dq,
            runner,
            writer,
            4,
        );

        // Git absent: the tick skips the WHOLE cycle - no jobs, no outcome writes,
        // so no repo's next_check_at is advanced while git is missing.
        let ran = sched.tick_once().await.expect("tick with no git");
        assert_eq!(ran, 0, "with no git, the tick selects and runs nothing");
        assert_eq!(inst.max(), 0, "no git work starts when git is absent");
        assert!(
            writer_handle.recorded().is_empty(),
            "a skipped cycle writes no next_check_at (repos stay due)"
        );

        // A settings re-probe swaps a live engine into the SAME handle (no restart).
        *handle.write().await = Some(SystemGitEngine::fabricated_for_test());

        // The very next tick picks up the swapped-in engine and runs the due repos.
        let ran2 = sched.tick_once().await.expect("tick after swap");
        assert_eq!(
            ran2, 2,
            "once git is available, the still-due repos run on the next tick"
        );
        assert_eq!(
            writer_handle.recorded().len(),
            2,
            "both repos' outcomes are recorded once git is live"
        );
    }
}
