//! Pure-Rust, dependency-free noise primitives for procedural world generation.
//!
//! This module provides two layers of noise:
//!
//! - **[`value_noise`]** — 3-D value noise via trilinear interpolation over a
//!   hash lattice.  Returns values in `[-1.0, 1.0]`.  Single-frequency; useful
//!   as a building block for higher-level noise.
//! - **[`fbm`]** — Fractal Brownian Motion built on `value_noise`.  Sums
//!   multiple octaves of noise at geometrically increasing frequencies and
//!   decreasing amplitudes, producing natural-looking detail at multiple scales.
//!   Returns values in approximately `[-1.0, 1.0]`.
//!
//! [`hash3`] is also re-exported as a standalone hash primitive suitable for
//! deterministic pseudo-random placement (e.g. asteroid center positions).
//!
//! # Why no external dependency
//!
//! Common noise crates (noise, fastnoise-lite, etc.) change APIs frequently and
//! carry their own versioning concerns.  The generator needs only value noise and
//! FBM; embedding the ~100-line implementation avoids version drift, keeps the
//! output bit-identical across compiler versions, and makes the algorithm fully
//! auditable within this codebase.

mod value;
mod fbm;

/// Deterministic integer lattice hash — see [`value::hash3`] for full docs.
pub use value::hash3;
/// 3-D value noise with trilinear interpolation — see [`value::value_noise`] for full docs.
pub use value::value_noise;
/// Fractal Brownian Motion over value noise — see [`fbm::fbm`] for full docs.
pub use fbm::fbm;
