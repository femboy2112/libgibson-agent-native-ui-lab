//! Read-only git intelligence layer.
//!
//! Bounded-by-policy: every scan carries an explicit cap, every expensive
//! computation is lazy with a bounded cache, and nothing in this module ever
//! mutates the user's repository.

pub mod blame;
pub mod diff;
pub mod filelog;
pub mod history;
pub mod metrics;
pub mod repo;

pub use repo::Repo;
