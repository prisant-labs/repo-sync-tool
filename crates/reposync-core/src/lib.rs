//! reposync-core - the Tauri-free logic crate for RepoSync.
//!
//! This crate holds all product behavior: the git engine, scheduler, update
//! policy, persistence, activity log, summary engine, GitHub client, and the
//! IPC payload types and error taxonomy shared with the Tauri shell.
//!
//! Load-bearing invariant (E-01 AC2): this crate must NOT depend on `tauri` or
//! any `tauri-*` crate, even transitively. That is what keeps the product logic
//! headlessly testable and makes the macOS port a thin edge.
//!
//! In E-01 every module below is a compiling stub with no logic. Each module
//! names its owning effort; later efforts fill them in.

pub mod activity;
pub mod error;
pub mod git;
pub mod github;
pub mod ipc;
pub mod paths;
pub mod policy;
pub mod repo;
pub mod scheduler;
pub mod summary;

#[cfg(test)]
mod tests {
    /// Placeholder test so the workspace test gate runs from day one (E-01 AC6).
    /// Real unit tests replace this as each effort lands. The body is intentionally
    /// empty: a constant assertion such as `assert!(true)` trips
    /// `clippy::assertions_on_constants` under `-D warnings`.
    #[test]
    fn skeleton_compiles() {}
}
