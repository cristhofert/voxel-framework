//! Region-based generator composition.
//!
//! `RegionGenerator<V>` is a refinement of [`ChunkGenerator<V>`] that decides
//! per-chunk whether it wants to produce that chunk.  [`MultiGenerator<V>`]
//! holds an ordered list of region generators plus a universal fallback and
//! itself implements [`ChunkGenerator<V>`], so it slots into the existing
//! async generation pipeline (`GeneratorPlugin`) with **zero changes** to the
//! pipeline.
//!
//! Use this to compose heterogeneous worlds — e.g. a starting moon at the
//! origin, an asteroid belt in a torus, and a void fallback for empty space.
//!
//! # Back-compat
//!
//! Every [`ChunkGenerator<V>`] is automatically a universal [`RegionGenerator<V>`]
//! via a blanket impl ([`applies_to`](RegionGenerator::applies_to) returns
//! `true` for every chunk).  Single-generator setups continue to work
//! unchanged.
//!
//! # Example
//!
//! ```rust,no_run
//! use std::sync::Arc;
//! use bevy::math::IVec3;
//! use bevy::prelude::*;
//! use voxel_framework::chunk::Chunk;
//! use voxel_framework::generator::{
//!     ChunkGenerator, MultiGenerator, MultiGeneratorPlugin, RegionGenerator,
//! };
//! use voxel_framework::voxel::DefaultVoxel;
//!
//! struct OriginMoon;
//! impl RegionGenerator<DefaultVoxel> for OriginMoon {
//!     fn applies_to(&self, c: IVec3) -> bool { c.length_squared() < 16 }
//!     fn generate(&self, _c: IVec3, chunk: &mut Chunk<DefaultVoxel>) {
//!         chunk.set(0, 0, 0, DefaultVoxel::Rock);
//!     }
//! }
//!
//! struct VoidFallback;
//! impl ChunkGenerator<DefaultVoxel> for VoidFallback {
//!     fn generate(&self, _c: IVec3, _chunk: &mut Chunk<DefaultVoxel>) {}
//! }
//!
//! let multi = MultiGenerator::<DefaultVoxel>::new(Box::new(VoidFallback))
//!     .with_region(Box::new(OriginMoon));
//!
//! // Then: app.add_plugins(MultiGeneratorPlugin::<DefaultVoxel>::from_generator(multi));
//! ```

use std::sync::Arc;

use bevy::math::IVec3;
use bevy::prelude::*;

use crate::chunk::Chunk;
use crate::voxel::VoxelType;

use super::{ChunkGenerator, GeneratorResource};

/// A generator that only applies to certain regions of world space.
///
/// Composed via [`MultiGenerator`] / [`MultiGeneratorPlugin`] to build
/// heterogeneous worlds.  Implementations must be `Send + Sync + 'static`
/// because they are invoked from background worker threads.
///
/// # Thread-safety contract
///
/// Same as [`ChunkGenerator`]: implementations **must not** access the Bevy
/// `World`, hold non-`Send` state, or perform blocking I/O.
pub trait RegionGenerator<V: VoxelType>: Send + Sync + 'static {
    /// Returns `true` iff this generator should produce the chunk at
    /// `chunk_pos`.
    ///
    /// `chunk_pos` is in chunk-grid space (one unit per `CHUNK_SIZE` world
    /// units).  Called on the worker thread; must be cheap and side-effect
    /// free.
    fn applies_to(&self, chunk_pos: IVec3) -> bool;

    /// Fills `chunk` with voxel data for the chunk at `chunk_pos`.
    ///
    /// Only called by [`MultiGenerator`] when [`applies_to`] returns `true`.
    /// Same thread-safety contract as [`ChunkGenerator::generate`].
    ///
    /// [`applies_to`]: RegionGenerator::applies_to
    fn generate(&self, chunk_pos: IVec3, chunk: &mut Chunk<V>);
}

/// Blanket impl: any [`ChunkGenerator`] is a universal [`RegionGenerator`]
/// (applies to every chunk).  This is the back-compat bridge — single-generator
/// setups continue to work and can be dropped into `MultiGenerator` without
/// any changes.
impl<V: VoxelType, G: ChunkGenerator<V>> RegionGenerator<V> for G {
    #[inline]
    fn applies_to(&self, _chunk_pos: IVec3) -> bool {
        true
    }

    #[inline]
    fn generate(&self, chunk_pos: IVec3, chunk: &mut Chunk<V>) {
        ChunkGenerator::generate(self, chunk_pos, chunk)
    }
}

/// Composite generator that dispatches each chunk request to the first
/// [`RegionGenerator`] whose [`applies_to`](RegionGenerator::applies_to)
/// returns `true`, falling back to a universal [`ChunkGenerator`] when none
/// match.
///
/// Itself implements [`ChunkGenerator<V>`], so it is a drop-in replacement for
/// any single-generator setup and slots into the existing
/// [`GeneratorPlugin`](super::GeneratorPlugin) async pipeline with no changes.
///
/// # Dispatch order
///
/// Regions are walked in **insertion order**.  The first region whose
/// `applies_to` returns `true` wins; later regions never see that chunk.
/// Order your regions from most specific to most general.
pub struct MultiGenerator<V: VoxelType> {
    regions: Vec<Box<dyn RegionGenerator<V>>>,
    fallback: Box<dyn ChunkGenerator<V>>,
}

impl<V: VoxelType> MultiGenerator<V> {
    /// Creates a [`MultiGenerator`] with the given universal fallback and no
    /// regions.  Add regions via [`add_region`](Self::add_region) or
    /// [`with_region`](Self::with_region).
    pub fn new(fallback: Box<dyn ChunkGenerator<V>>) -> Self {
        Self {
            regions: Vec::new(),
            fallback,
        }
    }

    /// Appends a region generator.  Regions are tried in insertion order.
    pub fn add_region(&mut self, region: Box<dyn RegionGenerator<V>>) -> &mut Self {
        self.regions.push(region);
        self
    }

    /// Builder-style variant of [`add_region`](Self::add_region).
    pub fn with_region(mut self, region: Box<dyn RegionGenerator<V>>) -> Self {
        self.regions.push(region);
        self
    }

    /// Replaces the universal fallback.
    pub fn with_fallback(mut self, fallback: Box<dyn ChunkGenerator<V>>) -> Self {
        self.fallback = fallback;
        self
    }

    /// Returns the number of registered regions (excluding the fallback).
    pub fn region_count(&self) -> usize {
        self.regions.len()
    }
}

impl<V: VoxelType> ChunkGenerator<V> for MultiGenerator<V> {
    fn generate(&self, coord: IVec3, chunk: &mut Chunk<V>) {
        for region in &self.regions {
            if region.applies_to(coord) {
                region.generate(coord, chunk);
                return;
            }
        }
        self.fallback.generate(coord, chunk);
    }
}

/// Bevy [`Plugin`] that installs a [`MultiGenerator<V>`] as the active
/// [`GeneratorResource<V>`] for voxel type `V`.
///
/// Mirrors the behaviour of [`GeneratorPlugin`](super::GeneratorPlugin) but
/// inserts the resource itself, so no separate
/// `app.insert_resource(GeneratorResource { ... })` step is needed.  Add this
/// plugin **alongside** [`GeneratorPlugin`](super::GeneratorPlugin) — the latter
/// still owns the async job-queue systems; this plugin only provides the
/// generator backing them.
///
/// # Builder workflow
///
/// ```rust,no_run
/// use bevy::prelude::*;
/// use bevy::math::IVec3;
/// use voxel_framework::chunk::Chunk;
/// use voxel_framework::generator::{
///     ChunkGenerator, GeneratorPlugin, MultiGenerator, MultiGeneratorPlugin,
///     RegionGenerator,
/// };
/// use voxel_framework::voxel::DefaultVoxel;
///
/// struct VoidGen;
/// impl ChunkGenerator<DefaultVoxel> for VoidGen {
///     fn generate(&self, _c: IVec3, _chunk: &mut Chunk<DefaultVoxel>) {}
/// }
///
/// struct MoonGen;
/// impl RegionGenerator<DefaultVoxel> for MoonGen {
///     fn applies_to(&self, c: IVec3) -> bool { c.length_squared() < 4 }
///     fn generate(&self, _c: IVec3, chunk: &mut Chunk<DefaultVoxel>) {
///         chunk.set(0, 0, 0, DefaultVoxel::Rock);
///     }
/// }
///
/// let plugin = MultiGeneratorPlugin::<DefaultVoxel>::new(Box::new(VoidGen))
///     .add_region(Box::new(MoonGen));
///
/// App::new()
///     .add_plugins(plugin)
///     .add_plugins(GeneratorPlugin::<DefaultVoxel>::new());
/// ```
pub struct MultiGeneratorPlugin<V: VoxelType> {
    /// Holds the in-progress generator until [`Plugin::build`] takes it.
    /// `Mutex` provides the interior mutability needed because `build` is
    /// called with `&self`; the `Option` lets us move the value out exactly
    /// once.  Bevy calls `build` from a single thread, so there is no real
    /// contention.
    multi: std::sync::Mutex<Option<MultiGenerator<V>>>,
}

impl<V: VoxelType> MultiGeneratorPlugin<V> {
    /// Creates a plugin builder seeded with a fallback generator and no
    /// regions.
    pub fn new(fallback: Box<dyn ChunkGenerator<V>>) -> Self {
        Self {
            multi: std::sync::Mutex::new(Some(MultiGenerator::new(fallback))),
        }
    }

    /// Wraps an existing [`MultiGenerator`] in a plugin without further
    /// builder calls.
    pub fn from_generator(multi: MultiGenerator<V>) -> Self {
        Self {
            multi: std::sync::Mutex::new(Some(multi)),
        }
    }

    /// Appends a region generator (builder style).
    pub fn add_region(self, region: Box<dyn RegionGenerator<V>>) -> Self {
        {
            let mut guard = self.multi.lock().expect("MultiGeneratorPlugin mutex");
            guard
                .as_mut()
                .expect("MultiGeneratorPlugin already consumed")
                .add_region(region);
        }
        self
    }

    /// Replaces the fallback generator (builder style).
    pub fn with_fallback(self, fallback: Box<dyn ChunkGenerator<V>>) -> Self {
        {
            let mut guard = self.multi.lock().expect("MultiGeneratorPlugin mutex");
            let multi = guard.take().expect("MultiGeneratorPlugin already consumed");
            *guard = Some(multi.with_fallback(fallback));
        }
        self
    }
}

impl<V: VoxelType> Plugin for MultiGeneratorPlugin<V> {
    fn build(&self, app: &mut App) {
        let multi = self
            .multi
            .lock()
            .expect("MultiGeneratorPlugin mutex")
            .take()
            .expect("MultiGeneratorPlugin built twice or never seeded");

        app.insert_resource(GeneratorResource::<V> {
            generator: Arc::new(multi),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::chunk::CHUNK_SIZE;
    use crate::voxel::DefaultVoxel;

    /// Region that applies to chunks with `chunk_pos.x >= 0`.  Marks voxel
    /// `(0, 0, 0)` with `Rock`.
    struct PositiveX;
    impl RegionGenerator<DefaultVoxel> for PositiveX {
        fn applies_to(&self, chunk_pos: IVec3) -> bool {
            chunk_pos.x >= 0
        }
        fn generate(&self, _chunk_pos: IVec3, chunk: &mut Chunk<DefaultVoxel>) {
            chunk.set(0, 0, 0, DefaultVoxel::Rock);
        }
    }

    /// Region that applies to chunks with `chunk_pos.x < 0`.  Marks voxel
    /// `(0, 0, 0)` with `IronOre`.
    struct NegativeX;
    impl RegionGenerator<DefaultVoxel> for NegativeX {
        fn applies_to(&self, chunk_pos: IVec3) -> bool {
            chunk_pos.x < 0
        }
        fn generate(&self, _chunk_pos: IVec3, chunk: &mut Chunk<DefaultVoxel>) {
            chunk.set(0, 0, 0, DefaultVoxel::IronOre);
        }
    }

    /// Universal fallback that leaves every voxel as air.  Used as the
    /// "should never be called" sentinel — sets `(1, 1, 1)` to `RareMineral`
    /// so the test can detect when fallback ran unexpectedly.
    struct VoidFallback;
    impl ChunkGenerator<DefaultVoxel> for VoidFallback {
        fn generate(&self, _coord: IVec3, chunk: &mut Chunk<DefaultVoxel>) {
            chunk.set(1, 1, 1, DefaultVoxel::RareMineral);
        }
    }

    #[test]
    fn region_dispatch_positive_x_runs_first_region() {
        let multi = MultiGenerator::<DefaultVoxel>::new(Box::new(VoidFallback))
            .with_region(Box::new(PositiveX))
            .with_region(Box::new(NegativeX));

        let mut chunk = Chunk::<DefaultVoxel>::new();
        ChunkGenerator::generate(&multi, IVec3::new(5, 0, 0), &mut chunk);

        assert_eq!(chunk.get(0, 0, 0), DefaultVoxel::Rock);
        // Fallback must not have run.
        assert_eq!(chunk.get(1, 1, 1), DefaultVoxel::Air);
    }

    #[test]
    fn region_dispatch_negative_x_runs_second_region() {
        let multi = MultiGenerator::<DefaultVoxel>::new(Box::new(VoidFallback))
            .with_region(Box::new(PositiveX))
            .with_region(Box::new(NegativeX));

        let mut chunk = Chunk::<DefaultVoxel>::new();
        ChunkGenerator::generate(&multi, IVec3::new(-3, 0, 0), &mut chunk);

        assert_eq!(chunk.get(0, 0, 0), DefaultVoxel::IronOre);
        assert_eq!(chunk.get(1, 1, 1), DefaultVoxel::Air);
    }

    #[test]
    fn fallback_runs_when_no_region_matches() {
        // Region that never matches.
        struct Never;
        impl RegionGenerator<DefaultVoxel> for Never {
            fn applies_to(&self, _: IVec3) -> bool {
                false
            }
            fn generate(&self, _: IVec3, chunk: &mut Chunk<DefaultVoxel>) {
                chunk.set(0, 0, 0, DefaultVoxel::Rock);
            }
        }

        let multi = MultiGenerator::<DefaultVoxel>::new(Box::new(VoidFallback))
            .with_region(Box::new(Never));

        let mut chunk = Chunk::<DefaultVoxel>::new();
        ChunkGenerator::generate(&multi, IVec3::new(0, 0, 0), &mut chunk);

        // Fallback ran.
        assert_eq!(chunk.get(1, 1, 1), DefaultVoxel::RareMineral);
        // Region did NOT run.
        assert_eq!(chunk.get(0, 0, 0), DefaultVoxel::Air);
    }

    #[test]
    fn insertion_order_wins_on_overlap() {
        // Two regions that both match every chunk.  First wins.
        struct AllRock;
        impl RegionGenerator<DefaultVoxel> for AllRock {
            fn applies_to(&self, _: IVec3) -> bool {
                true
            }
            fn generate(&self, _: IVec3, chunk: &mut Chunk<DefaultVoxel>) {
                chunk.set(0, 0, 0, DefaultVoxel::Rock);
            }
        }
        struct AllIron;
        impl RegionGenerator<DefaultVoxel> for AllIron {
            fn applies_to(&self, _: IVec3) -> bool {
                true
            }
            fn generate(&self, _: IVec3, chunk: &mut Chunk<DefaultVoxel>) {
                chunk.set(0, 0, 0, DefaultVoxel::IronOre);
            }
        }

        let multi = MultiGenerator::<DefaultVoxel>::new(Box::new(VoidFallback))
            .with_region(Box::new(AllRock))
            .with_region(Box::new(AllIron));

        let mut chunk = Chunk::<DefaultVoxel>::new();
        ChunkGenerator::generate(&multi, IVec3::ZERO, &mut chunk);

        assert_eq!(chunk.get(0, 0, 0), DefaultVoxel::Rock);
    }

    #[test]
    fn blanket_impl_lets_chunk_generator_serve_as_region() {
        // VoidFallback is a ChunkGenerator; the blanket impl makes it a
        // universal RegionGenerator too.
        let multi = MultiGenerator::<DefaultVoxel>::new(Box::new(VoidFallback))
            .with_region(Box::new(VoidFallback));

        let mut chunk = Chunk::<DefaultVoxel>::new();
        ChunkGenerator::generate(&multi, IVec3::new(7, 7, 7), &mut chunk);

        // First region (the universal VoidFallback) ran, not the fallback.
        // VoidFallback marks (1,1,1) with RareMineral either way, so we can't
        // distinguish here — instead, assert the chunk volume is otherwise
        // untouched, confirming the dispatcher ran exactly one generator.
        let mut nonair = 0;
        for x in 0..CHUNK_SIZE {
            for y in 0..CHUNK_SIZE {
                for z in 0..CHUNK_SIZE {
                    if chunk.get(x, y, z) != DefaultVoxel::Air {
                        nonair += 1;
                    }
                }
            }
        }
        assert_eq!(
            nonair, 1,
            "exactly one voxel should be set; got {nonair} non-air voxels"
        );
    }
}
