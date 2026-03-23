//! 3-D value noise via deterministic integer hashing and trilinear interpolation.
//!
//! # Algorithm
//!
//! Given a floating-point coordinate `(x, y, z)`:
//!
//! 1. The integer part of each axis is used to index into a virtual infinite
//!    lattice.  [`hash3`] maps each of the eight surrounding lattice corners to
//!    a pseudo-random float in `[0.0, 1.0]`.
//! 2. A quintic smoothstep (Ken Perlin's fade function: `6t⁵ − 15t⁴ + 10t³`)
//!    is applied to the fractional part of each axis so that first and second
//!    derivatives are zero at cell boundaries, eliminating visible grid artifacts.
//! 3. Trilinear interpolation blends the eight corner values.
//! 4. The result is remapped from `[0.0, 1.0]` to `[-1.0, 1.0]`.
//!
//! The hash function uses a MurmurHash3-style bit finalizer seeded with an
//! integer `seed`, so independent noise fields at the same coordinates are
//! produced by passing different seeds.

/// Maps three integer lattice coordinates and a `seed` to a pseudo-random float
/// in `[0.0, 1.0]`.
///
/// The function XORs each coordinate into an accumulator using the golden-ratio
/// constant `0x9e3779b9` (from Knuth's multiplicative hashing) and then applies
/// the MurmurHash3 bit finalizer to spread the bits.  The lower 24 bits of the
/// result are divided by `0x00FF_FFFF` to produce the output float, keeping the
/// mantissa precision high without bias from floating-point rounding.
///
/// # Determinism guarantee
///
/// Given the same `(x, y, z, seed)`, this function always returns the same value
/// on all platforms and compiler versions.  No thread-local state or external
/// entropy is used.
///
/// # Output range
///
/// `[0.0, 1.0]` (inclusive on both ends for integer inputs).
#[inline]
pub fn hash3(x: i32, y: i32, z: i32, seed: u32) -> f32 {
    let mut h = seed;
    h ^= x as u32;
    h = h.wrapping_mul(0x9e3779b9);
    h ^= y as u32;
    h = h.wrapping_mul(0x9e3779b9);
    h ^= z as u32;
    // MurmurHash3 finalizer
    h ^= h >> 16;
    h = h.wrapping_mul(0x85ebca6b);
    h ^= h >> 13;
    h = h.wrapping_mul(0xc2b2ae35);
    h ^= h >> 16;
    (h & 0x00FF_FFFF) as f32 / 0x00FF_FFFF as f32
}

/// Ken Perlin's smoothstep (fade) function: `6t^5 - 15t^4 + 10t^3`.
///
/// Maps `[0, 1]` to `[0, 1]` with zero first and second derivatives at the
/// endpoints, producing smooth interpolation without visible grid artifacts.
#[inline]
fn fade(t: f32) -> f32 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

/// Linearly interpolates between `a` and `b` by factor `t`.
#[inline]
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + t * (b - a)
}

/// Evaluates 3-D value noise at `(x, y, z)` with the given `seed`.
///
/// Hashes the eight corners of the unit cube surrounding `(x, y, z)` using
/// [`hash3`] and trilinearly interpolates between them after applying the quintic
/// fade function to the fractional coordinates.  The result is remapped to
/// `[-1.0, 1.0]`.
///
/// Pass different `seed` values to produce statistically independent noise fields
/// at the same coordinate, which is useful for mineral vein generation where
/// multiple independent fields are needed simultaneously.
///
/// # Output range
///
/// `[-1.0, 1.0]`.  The extremes are only approached (not reached) for typical
/// non-integer inputs due to the smooth interpolation.
///
/// # Determinism guarantee
///
/// Identical to [`hash3`]: same inputs, same output on all platforms.
#[inline]
pub fn value_noise(x: f32, y: f32, z: f32, seed: u32) -> f32 {
    let xi = x.floor() as i32;
    let yi = y.floor() as i32;
    let zi = z.floor() as i32;

    let xf = fade(x - x.floor());
    let yf = fade(y - y.floor());
    let zf = fade(z - z.floor());

    // Sample the 8 corners of the unit cube
    let c000 = hash3(xi, yi, zi, seed);
    let c100 = hash3(xi + 1, yi, zi, seed);
    let c010 = hash3(xi, yi + 1, zi, seed);
    let c110 = hash3(xi + 1, yi + 1, zi, seed);
    let c001 = hash3(xi, yi, zi + 1, seed);
    let c101 = hash3(xi + 1, yi, zi + 1, seed);
    let c011 = hash3(xi, yi + 1, zi + 1, seed);
    let c111 = hash3(xi + 1, yi + 1, zi + 1, seed);

    // Trilinear interpolation
    let x00 = lerp(c000, c100, xf);
    let x10 = lerp(c010, c110, xf);
    let x01 = lerp(c001, c101, xf);
    let x11 = lerp(c011, c111, xf);

    let y0 = lerp(x00, x10, yf);
    let y1 = lerp(x01, x11, yf);

    let result = lerp(y0, y1, zf);

    // Map from [0, 1] to [-1, 1]
    result * 2.0 - 1.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash3_determinism() {
        let a = hash3(10, -20, 30, 42);
        let b = hash3(10, -20, 30, 42);
        assert_eq!(a, b, "hash3 must be deterministic");
    }

    #[test]
    fn hash3_range() {
        for x in -10..10 {
            for y in -10..10 {
                let v = hash3(x, y, 0, 0);
                assert!(
                    (0.0..=1.0).contains(&v),
                    "hash3({x}, {y}, 0, 0) = {v} out of [0, 1]"
                );
            }
        }
    }

    #[test]
    fn hash3_distribution() {
        // Verify that not all values are the same
        let mut values = std::collections::HashSet::new();
        for x in 0..20 {
            let v = hash3(x, 0, 0, 0);
            // Quantize to check variety
            values.insert((v * 1000.0) as i32);
        }
        assert!(
            values.len() > 5,
            "hash3 should produce varied output, got only {} distinct values",
            values.len()
        );
    }

    #[test]
    fn value_noise_determinism() {
        let a = value_noise(1.5, -2.3, 4.7, 42);
        let b = value_noise(1.5, -2.3, 4.7, 42);
        assert_eq!(a, b, "value_noise must be deterministic");
    }

    #[test]
    fn value_noise_range() {
        for i in 0..100 {
            let x = (i as f32) * 0.37 - 18.5;
            let y = (i as f32) * 0.23 + 5.0;
            let z = (i as f32) * 0.53 - 10.0;
            let v = value_noise(x, y, z, 0);
            assert!(
                (-1.0..=1.0).contains(&v),
                "value_noise({x}, {y}, {z}, 0) = {v} out of [-1, 1]"
            );
        }
    }
}
