//! Project Theseus library surface.
//!
//! The binary (`src/main.rs`) is a thin shell over these modules, which are
//! exposed so the adversarial integration tests can drive the real quotient
//! pipeline and the real RGB renderer without spawning a subprocess.

pub mod model;
pub mod visual;
