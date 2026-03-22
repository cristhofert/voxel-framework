//! Fractal Brownian Motion (FBM) built on 3D value noise.

use super::value::value_noise;

/// Computes Fractal Brownian Motion by layering multiple octaves of value noise.
///
/// Each octave samples the noise at a higher frequency and lower amplitude than
/// the previous one, producing natural-looking detail at multiple scales.
///
/// # Parameters
///
/// - `x`, `y`, `z`: World-space coordinates (will be scaled by `frequency`).
/// - `seed`: Base seed for the noise field.
/// - `octaves`: Number of noise layers to accumulate (typically 3-6).
/// - `frequency`: Initial sampling frequency (higher = more detail per unit distance).
/// - `lacunarity`: Frequency multiplier per octave (typically 2.0).
/// - `gain`: Amplitude multiplier per octave (typically 0.5, also called "persistence").
///
/// # Returns
///
/// A value normalized to approximately `[-1.0, 1.0]`.
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
