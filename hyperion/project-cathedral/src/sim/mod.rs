//! The simulated distributed system: fixture, engine, incident semantics.

pub mod engine;
pub mod fixture;

pub use engine::{Breaker, Engine, Metrics};
pub use fixture::{Fixture, ServiceId};
