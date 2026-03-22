//! Procedural noise functions for world generation.
//!
//! Provides a pure-Rust, dependency-free noise library with 3D value noise
//! and Fractal Brownian Motion (FBM). These are pure math functions with no
//! Bevy dependency.

mod value;
mod fbm;

pub use value::{hash3, value_noise};
pub use fbm::fbm;
