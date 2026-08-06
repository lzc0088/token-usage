//! Data-collection layer. See docs/design.md §4 and docs/plan.md §M1.
//!
//! `tokscale` is the single source of token statistics. This module resolves
//! the binary (three-tier strategy), spawns it, and tolerantly parses its JSON.

pub mod anchor;
pub mod project_snapshot;
pub mod runtime;
pub mod scheduler;
pub mod tokscale;
pub mod watcher;
pub mod workspace;

pub use tokscale::{Period, TokscaleError};
