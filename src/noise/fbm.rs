//! Fractal Brownian Motion (FBM) built on [`super::value_noise`].
//!
//! FBM layers multiple octaves of noise at geometrically increasing frequencies
//! and decreasing amplitudes.  The result combines low-frequency large-scale
//! shapes with high-frequency fine detail, producing the irregular, organic
//! surfaces seen in terrain and asteroid generation.
//!
//! Each octave `i` is evaluated at frequency `frequency × lacunarity^i` and
//! weighted by `amplitude × gain^i`.  The accumulated sum is divided by the
//! total amplitude so the output stays in approximately `[-1.0, 1.0]` regardless
//! of the number of octaves.

use super::value::value_noise;

/// Evaluates Fractal Brownian Motion at `(x, y, z)` by summing `octaves` layers
/// of [`super::value_noise`].
///
/// # Parameters
///
/// - `x`, `y`, `z` — World-space sample coordinates.  The first octave is
///   evaluated at `(x × frequency, y × frequency, z × frequency)`.
/// - `seed` — Base seed for the noise field.  Each octave shifts the seed by a
///   prime offset (`i × 7919`) to ensure octaves are statistically independent.
/// - `octaves` — Number of noise layers.  Typical range: `2..=6`.  More octaves
///   add finer detail at a linear cost in noise evaluations.
/// - `frequency` — Initial sampling frequency.  Higher values produce more
///   surface detail per world unit.  Typical value: `2.5`.
/// - `lacunarity` — Frequency multiplier applied each octave.  A value of `2.0`
///   doubles the frequency per octave, adding one extra detail level per doubling.
///   Typical value: `2.0`.
/// - `gain` — Amplitude multiplier per octave (also called *persistence*).  A
///   value of `0.5` halves each successive octave's contribution, making finer
///   detail progressively subtler.  Typical value: `0.5`.
///
/// # Output range
///
/// Approximately `[-1.0, 1.0]`.  The normalization divides by the sum of all
/// octave amplitudes (a geometric series), so the range is theoretically exact
/// when `gain < 1.0`.  Floating-point rounding may produce values slightly
/// outside `[-1.01, 1.01]` in practice.
///
/// # Determinism guarantee
///
/// Identical to [`super::value_noise`]: same inputs always produce the same
/// output on all platforms.
///
/// # Examples
///
/// ```rust,no_run
/// use voxel_framework::noise::fbm;
///
/// // Sample surface displacement at world position (10.5, 0.0, -3.2).
/// let displacement = fbm(
///     10.5, 0.0, -3.2,
///     /*seed=*/   42,
///     /*octaves=*/ 4,
///     /*frequency=*/ 2.5,
///     /*lacunarity=*/ 2.0,
///     /*gain=*/ 0.5,
/// );
/// assert!((-1.0..=1.0).contains(&displacement));
/// ```
#[allow(clippy::too_many_arguments)] // All parameters are semantically distinct FBM controls; a params struct would harm ergonomics.
pub fn fbm(
    x: f32,
    y: f32,
    z: f32,
    seed: u32,
    octaves: u32,
    frequency: f32,
    lacunarity: f32,
    gain: f32,
) -> f32 {
    let mut sum = 0.0_f32;
    let mut amplitude = 1.0_f32;
    let mut freq = frequency;
    let mut max_amplitude = 0.0_f32;

    for i in 0..octaves {
        let octave_seed = seed.wrapping_add(i * 7919);
        sum += value_noise(x * freq, y * freq, z * freq, octave_seed) * amplitude;
        max_amplitude += amplitude;
        freq *= lacunarity;
        amplitude *= gain;
    }

    // Normalize to [-1, 1]
    if max_amplitude > 0.0 {
        sum / max_amplitude
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fbm_determinism() {
        let a = fbm(1.5, -2.3, 4.7, 42, 4, 2.5, 2.0, 0.5);
        let b = fbm(1.5, -2.3, 4.7, 42, 4, 2.5, 2.0, 0.5);
        assert_eq!(a, b, "fbm must be deterministic");
    }

    #[test]
    fn fbm_range() {
        for i in 0..200 {
            let x = (i as f32) * 0.37 - 37.0;
            let y = (i as f32) * 0.23 + 5.0;
            let z = (i as f32) * 0.53 - 50.0;
            let v = fbm(x, y, z, 0, 4, 2.5, 2.0, 0.5);
            assert!(
                (-1.01..=1.01).contains(&v),
                "fbm({x}, {y}, {z}) = {v} out of expected range"
            );
        }
    }

    #[test]
    fn fbm_different_seeds_differ() {
        let a = fbm(5.0, 5.0, 5.0, 0, 4, 2.5, 2.0, 0.5);
        let b = fbm(5.0, 5.0, 5.0, 999, 4, 2.5, 2.0, 0.5);
        assert_ne!(a, b, "different seeds should produce different values");
    }

    #[test]
    fn fbm_single_octave_equals_value_noise() {
        // With 1 octave, fbm should equal value_noise at the given frequency
        let freq = 2.5_f32;
        let x = 3.14;
        let y = -1.0;
        let z = 7.7;
        let seed = 42;

        let fbm_val = fbm(x, y, z, seed, 1, freq, 2.0, 0.5);
        let noise_val = value_noise(x * freq, y * freq, z * freq, seed);
        assert!(
            (fbm_val - noise_val).abs() < 1e-6,
            "single-octave fbm should match value_noise: {fbm_val} vs {noise_val}"
        );
    }
}
