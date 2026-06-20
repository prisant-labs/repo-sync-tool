//! git - owned by E-03 (the git engine boundary).
//!
//! Splits into cheap reads (`inspect`, via git2) and network/mutation
//! operations (`cli`, by shelling out to git). The `GitEngine` trait is the
//! seam both sides implement.
//
// TODO(E-03): implement the GitEngine trait and its inspect/cli backends.

pub mod cli;
pub mod inspect;

/// GitEngine - owned by E-03. Placeholder trait; methods land in E-03.
pub trait GitEngine {}
