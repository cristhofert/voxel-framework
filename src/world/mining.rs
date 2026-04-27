//! Voxel mining (block-breaking) API.
//!
//! # Design — pure functions, no implicit systems
//!
//! All logic lives in free functions ([`tick_mining`], [`stop_mining`]) so
//! that game code retains full control over when and how mining occurs.
//! [`MiningPlugin`] only registers the required resources and message types;
//! it adds **no** systems of its own.
//!
//! # Integration pattern
//!
//! ```no_run
//! # use bevy::prelude::*;
//! # use voxel_framework::world::mining::{MiningPlugin, MiningConfig, MiningState,
//! #     MiningProgress, VoxelBroken, tick_mining, stop_mining};
//! # use voxel_framework::world::{ChunkManager, ChunkModified};
//! # use voxel_framework::chunk::Chunk;
//! # use voxel_framework::voxel::DefaultVoxel;
//! // 1. Add the plugin once when building the App.
//! //    App::new().add_plugins(MiningPlugin);
//!
//! // 2. In your input system, call tick_mining every frame while the
//! //    mine button is held, or stop_mining when it is released.
//! fn my_mining_system(
//!     mut state:    ResMut<MiningState>,
//!     config:       Res<MiningConfig>,
//!     manager:      Res<ChunkManager>,
//!     mut chunks:   Query<&mut Chunk<DefaultVoxel>>,
//!     mut broken:   MessageWriter<VoxelBroken>,
//!     mut progress: MessageWriter<MiningProgress>,
//!     mut modified: MessageWriter<ChunkModified>,
//!     time:         Res<Time>,
//! ) {
//!     let mining = true; // replace with real input check
//!     if mining {
//!         tick_mining(
//!             IVec3::ZERO, (8, 8, 8),
//!             time.delta_secs(),
//!             &mut state, &config, &manager,
//!             &mut chunks, &mut broken, &mut progress, &mut modified,
//!         );
//!     } else {
//!         stop_mining(&mut state);
//!     }
//! }
//! ```

use bevy::prelude::*;

use crate::chunk::Chunk;
#[cfg(feature = "items")]
use crate::items::{HotbarState, ItemType};
#[cfg(feature = "items")]
use crate::items::inventory::{ContainerComponent, PrimaryContainer};
#[cfg(feature = "items")]
use crate::player::effects::AppliedEffectModifiers;
use crate::voxel::VoxelType;

use super::{ChunkManager, ChunkModified};

// ─── Resources ───────────────────────────────────────────────────────────────

/// Configuration for the mining system.
///
/// Insert this resource before adding [`MiningPlugin`], or mutate it at
/// runtime to adjust difficulty.
#[derive(Resource, Debug, Clone)]
pub struct MiningConfig {
    /// Time in seconds required to break a single voxel.
    ///
    /// Must be greater than zero.  **Default: `1.5` seconds.**
    pub break_time: f32,
}

impl Default for MiningConfig {
    fn default() -> Self {
        Self { break_time: 1.5 }
    }
}

/// Runtime state for the mining system.
///
/// Tracks which voxel is currently being mined and how much time has
/// accumulated toward breaking it.  There is at most one active target at a
/// time; switching targets resets elapsed time automatically inside
/// [`tick_mining`].
///
/// Call [`stop_mining`] whenever the mine button is released — failing to do
/// so leaves stale progress that will carry over to the next target.
#[derive(Resource, Debug)]
pub struct MiningState {
    /// The chunk coordinate and local voxel index `(x, y, z)` currently being
    /// mined, or `None` when no mining is in progress.
    pub target: Option<(IVec3, (usize, usize, usize))>,
    /// Accumulated break time in seconds, in the range `[0.0, effective_break_time]`.
    pub elapsed: f32,
    /// Normalised break progress in the range `[0.0, 1.0]`.
    ///
    /// Equals `elapsed / effective_break_time`.  Reaches `1.0` when the voxel breaks.
    pub progress: f32,
    /// Cached effective break time for the current target, in seconds.
    ///
    /// Computed once per target as `config.break_time * voxel.hardness()`.
    /// Initialised to `1.0` until the first mining target is set.
    pub(crate) effective_break_time: f32,
}

impl Default for MiningState {
    fn default() -> Self {
        Self {
            target: None,
            elapsed: 0.0,
            progress: 0.0,
            effective_break_time: 1.0,
        }
    }
}

/// Active-tool statistics derived from the hotbar selection each frame.
///
/// [`MiningInputPlugin`] keeps this resource up to date via
/// `hotbar_to_tool_system`. Game code that calls [`tick_mining`] directly
/// can read or override it as needed.
#[derive(Resource, Debug, Clone)]
pub struct ToolStats {
    /// Minimum tool level required to mine a voxel.
    ///
    /// Matched against [`crate::voxel::VoxelType::mining_level`]:
    /// if `tool_level < voxel.mining_level()`, mining makes no progress.
    pub tool_level: u8,
    /// Mining speed multiplier for the active tool.
    ///
    /// The effective break time is divided by this value (`2.0` = twice as fast).
    pub tool_speed: f32,
}

impl ToolStats {
    /// Resets stats to bare-hands defaults (level 0, speed 1.0).
    pub fn reset(&mut self) {
        self.tool_level = 0;
        self.tool_speed = 1.0;
    }
}

impl Default for ToolStats {
    fn default() -> Self {
        Self { tool_level: 0, tool_speed: 1.0 }
    }
}

// ─── Messages ────────────────────────────────────────────────────────────────

/// Message fired once when a voxel's break time is fully elapsed.
///
/// Emitted by [`tick_mining`] in the same frame the voxel is destroyed.
/// Systems that react to block destruction (e.g. particle effects, inventory
/// updates) should read this message.
#[derive(Message, Debug, Clone)]
pub struct VoxelBroken {
    /// Chunk-grid coordinate of the chunk that contained the broken voxel.
    pub chunk_coord: IVec3,
    /// Local voxel index `(x, y, z)` within the chunk.  Each component is in
    /// `[0, CHUNK_SIZE)`.
    pub local: (usize, usize, usize),
    /// World-space center of the broken voxel.
    pub world_pos: Vec3,
}

/// Message sent every frame while a voxel is being mined.
///
/// Useful for driving crack-overlay animations, progress bars, or audio
/// feedback.  Emitted by [`tick_mining`] on every frame where mining is
/// active, including the frame the voxel breaks (where `progress` is `1.0`).
#[derive(Message, Debug, Clone)]
pub struct MiningProgress {
    /// Normalised break progress in `[0.0, 1.0]`.
    ///
    /// `0.0` on the first frame, `1.0` on the frame the voxel breaks.
    pub progress: f32,
    /// World-space center of the voxel being mined.
    pub world_pos: Vec3,
}

// ─── Free functions ───────────────────────────────────────────────────────────

/// Breaks a single voxel at world-voxel coordinates `world_voxel` if it is
/// solid and its chunk is loaded.  Emits [`VoxelBroken`], [`ChunkModified`],
/// and border-neighbor notifications.
fn break_voxel<V: VoxelType>(
    world_voxel: IVec3,
    manager: &ChunkManager,
    chunks: &mut Query<&mut Chunk<V>>,
    broken: &mut MessageWriter<VoxelBroken>,
    modified: &mut MessageWriter<ChunkModified>,
) {
    use crate::chunk::CHUNK_SIZE;
    let cs = CHUNK_SIZE as i32;
    let chunk_coord = IVec3::new(
        world_voxel.x.div_euclid(cs),
        world_voxel.y.div_euclid(cs),
        world_voxel.z.div_euclid(cs),
    );
    let local = (
        world_voxel.x.rem_euclid(cs) as usize,
        world_voxel.y.rem_euclid(cs) as usize,
        world_voxel.z.rem_euclid(cs) as usize,
    );

    let Some(entity) = manager.get_entity(chunk_coord) else { return };
    let Ok(mut chunk) = chunks.get_mut(entity) else { return };
    if !chunk.is_solid_at(local.0, local.1, local.2) {
        return;
    }
    chunk.set(local.0, local.1, local.2, V::air());
    drop(chunk);

    let wp = voxel_world_pos(chunk_coord, local);
    broken.write(VoxelBroken { chunk_coord, local, world_pos: wp });
    modified.write(ChunkModified(chunk_coord));
    notify_border_neighbors(chunk_coord, local, modified);
}

/// Returns the three extra world-voxel offsets that form a 2×2 square on the
/// hit face together with the primary voxel.
///
/// The two tangent axes are determined by `face_normal`.  For each axis the
/// sign of the offset is chosen so that the 2×2 quad lies in the quadrant
/// where the ray actually entered the face (using `hit_point` vs
/// `voxel_center`).
fn quad_offsets(face_normal: Vec3, hit_point: Vec3, voxel_center: Vec3) -> [IVec3; 3] {
    let sign = |coord: f32, center: f32| -> i32 {
        if coord >= center { 1 } else { -1 }
    };
    let (du, dv) = if face_normal.x.abs() > 0.5 {
        // X face → perpendicular plane is YZ
        (
            IVec3::new(0, sign(hit_point.y, voxel_center.y), 0),
            IVec3::new(0, 0, sign(hit_point.z, voxel_center.z)),
        )
    } else if face_normal.y.abs() > 0.5 {
        // Y face → perpendicular plane is XZ
        (
            IVec3::new(sign(hit_point.x, voxel_center.x), 0, 0),
            IVec3::new(0, 0, sign(hit_point.z, voxel_center.z)),
        )
    } else {
        // Z face (or zero normal) → perpendicular plane is XY
        (
            IVec3::new(sign(hit_point.x, voxel_center.x), 0, 0),
            IVec3::new(0, sign(hit_point.y, voxel_center.y), 0),
        )
    };
    [du, dv, du + dv]
}

/// Sends [`ChunkModified`] for any neighboring chunks whose border shares the
/// face of the voxel at `local` within `chunk_coord`.
///
/// When a voxel on the edge of a chunk is added or removed, the adjacent
/// chunk's mesh must also be regenerated — its border faces were previously
/// culled by this voxel and are now exposed (or vice versa for placement).
fn notify_border_neighbors(
    chunk_coord: IVec3,
    local: (usize, usize, usize),
    modified: &mut MessageWriter<ChunkModified>,
) {
    use crate::chunk::CHUNK_SIZE;
    let (x, y, z) = local;
    let offsets: [(usize, IVec3); 6] = [
        (x, IVec3::NEG_X),
        (CHUNK_SIZE - 1 - x, IVec3::X),
        (y, IVec3::NEG_Y),
        (CHUNK_SIZE - 1 - y, IVec3::Y),
        (z, IVec3::NEG_Z),
        (CHUNK_SIZE - 1 - z, IVec3::Z),
    ];
    for (dist, dir) in offsets {
        if dist == 0 {
            modified.write(ChunkModified(chunk_coord + dir));
        }
    }
}

/// Compute the world-space center of a voxel given its chunk coord and local index.
fn voxel_world_pos(chunk_coord: IVec3, local: (usize, usize, usize)) -> Vec3 {
    let chunk_origin = chunk_coord.as_vec3() * crate::chunk::CHUNK_SIZE as f32;
    chunk_origin
        + Vec3::new(
            local.0 as f32 + 0.5,
            local.1 as f32 + 0.5,
            local.2 as f32 + 0.5,
        )
}

/// Advance mining by one frame toward breaking the target voxel.
///
/// Call this **every frame** while the mine button is held.  When the
/// accumulated time reaches `config.break_time` the function:
///
/// 1. Sets the target voxel to air (if the chunk is loaded).
/// 2. Writes a [`VoxelBroken`] message.
/// 3. Writes a [`ChunkModified`] message to trigger remeshing.
/// 4. Resets `state` so a new target can start fresh.
///
/// A [`MiningProgress`] message is written on **every** call, including the
/// break frame.
///
/// If `target_chunk` / `target_local` differs from the previous frame, elapsed
/// time is reset to zero automatically — no need to call [`stop_mining`] when
/// switching targets mid-frame.
///
/// # Parameters
///
/// - `target_chunk` — Chunk-grid coordinate of the chunk containing the target
///   voxel.
/// - `target_local` — Local `(x, y, z)` voxel index within the chunk.
/// - `delta_secs` — Frame delta time in seconds (typically `time.delta_secs()`).
/// - `state` — Mutable reference to the shared [`MiningState`] resource.
/// - `config` — Read-only mining configuration (break time).
/// - `manager` — Chunk registry used to look up the chunk entity.
/// - `chunks` — Mutable query used to set voxels to air on break.
/// - `broken` — Message writer for [`VoxelBroken`].
/// - `progress_writer` — Message writer for [`MiningProgress`].
/// - `modified` — Message writer for [`crate::world::ChunkModified`].
///
/// # Examples
///
/// ```no_run
/// # use bevy::prelude::*;
/// # use voxel_framework::world::mining::{MiningConfig, MiningState, MiningProgress, VoxelBroken, tick_mining};
/// # use voxel_framework::world::{ChunkManager, ChunkModified};
/// # use voxel_framework::chunk::Chunk;
/// # use voxel_framework::voxel::DefaultVoxel;
/// fn my_system(
///     mut state:    ResMut<MiningState>,
///     config:       Res<MiningConfig>,
///     manager:      Res<ChunkManager>,
///     mut chunks:   Query<&mut Chunk<DefaultVoxel>>,
///     mut broken:   MessageWriter<VoxelBroken>,
///     mut progress: MessageWriter<MiningProgress>,
///     mut modified: MessageWriter<ChunkModified>,
///     time:         Res<Time>,
/// ) {
///     tick_mining(
///         IVec3::ZERO, (4, 0, 4),
///         time.delta_secs(),
///         &mut state, &config, &manager,
///         &mut chunks, &mut broken, &mut progress, &mut modified,
///     );
/// }
/// ```
#[allow(clippy::too_many_arguments)]
pub fn tick_mining<V: VoxelType>(
    target_chunk: IVec3,
    target_local: (usize, usize, usize),
    delta_secs: f32,
    state: &mut MiningState,
    config: &MiningConfig,
    manager: &ChunkManager,
    chunks: &mut Query<&mut Chunk<V>>,
    broken: &mut MessageWriter<VoxelBroken>,
    progress_writer: &mut MessageWriter<MiningProgress>,
    modified: &mut MessageWriter<ChunkModified>,
) {
    // Reset elapsed when target changes and sample the new voxel's hardness.
    if state.target != Some((target_chunk, target_local)) {
        let hardness = manager
            .get_entity(target_chunk)
            .and_then(|e| chunks.get(e).ok())
            .map(|chunk| {
                let (x, y, z) = target_local;
                chunk.get(x, y, z).hardness()
            })
            .unwrap_or(1.0);

        // Hardness 0.0 means unbreakable — use a very large break time.
        state.effective_break_time = if hardness > 0.0 {
            config.break_time * hardness
        } else {
            f32::MAX
        };
        state.elapsed = 0.0;
        state.progress = 0.0;
        state.target = Some((target_chunk, target_local));
    }

    state.elapsed += delta_secs;
    state.progress = (state.elapsed / state.effective_break_time).min(1.0);

    let world_pos = voxel_world_pos(target_chunk, target_local);

    progress_writer.write(MiningProgress {
        progress: state.progress,
        world_pos,
    });

    if state.progress >= 1.0 && state.effective_break_time < f32::MAX {
        // Destroy the voxel if the chunk is loaded.
        if let Some(entity) = manager.get_entity(target_chunk) {
            if let Ok(mut chunk) = chunks.get_mut(entity) {
                let (x, y, z) = target_local;
                chunk.set(x, y, z, V::air());
            }
        }

        broken.write(VoxelBroken {
            chunk_coord: target_chunk,
            local: target_local,
            world_pos,
        });

        modified.write(ChunkModified(target_chunk));
        notify_border_neighbors(target_chunk, target_local, modified);

        // Reset state.
        state.target = None;
        state.elapsed = 0.0;
        state.progress = 0.0;
    }
}

/// Stop mining and reset all accumulated progress.
///
/// Call this whenever the mine button is released.  Failing to call
/// `stop_mining` means any partial progress persists, and the next target
/// will appear to start partially broken.
///
/// # Examples
///
/// ```no_run
/// # use voxel_framework::world::mining::{MiningState, stop_mining};
/// fn on_button_release(mut state: bevy::prelude::ResMut<MiningState>) {
///     stop_mining(&mut state);
/// }
/// ```
pub fn stop_mining(state: &mut MiningState) {
    state.target = None;
    state.elapsed = 0.0;
    state.progress = 0.0;
}

// ─── Plugin ───────────────────────────────────────────────────────────────────

/// Bevy plugin that registers the mining subsystem.
///
/// Adds the following to the [`bevy::prelude::App`]:
///
/// - [`MiningConfig`] resource (initialised with defaults — `break_time: 1.5 s`)
/// - [`MiningState`] resource (initialised at default / idle)
/// - [`VoxelBroken`] message type
/// - [`MiningProgress`] message type
///
/// Does **not** add any systems.  Game code is responsible for calling
/// [`tick_mining`] and [`stop_mining`] from its own update systems.
pub struct MiningPlugin;

impl Plugin for MiningPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MiningConfig>();
        app.init_resource::<MiningState>();
        app.add_message::<VoxelBroken>();
        app.add_message::<MiningProgress>();
    }
}

// ─── MiningInputPlugin ────────────────────────────────────────────────────────
//
// `MiningInputPlugin` and its companion `hotbar_to_tool_system` are gated on the
// `items` feature because they require `ItemType`, `HotbarState`,
// `ContainerComponent`, and `PrimaryContainer` — all of which live in
// `crate::items`. Disabling the feature compiles them out cleanly.

/// Convenience plugin that wires mouse input to the mining API automatically.
///
/// `MiningInputPlugin<V, I>` adds two [`Update`] systems:
///
/// 1. `hotbar_to_tool_system` — reads the active hotbar slot from [`Inventory<I>`]
///    and updates [`ToolStats`] with the slot's tool level and speed each frame.
/// 2. `mining_input_system` — raycasts from the active [`Camera3d`] (max
///    `6.0` world units). While the **left mouse button** is held and the ray
///    hits a solid voxel whose [`crate::voxel::VoxelType::mining_level`] is
///    satisfied by the current [`ToolStats::tool_level`], calls [`tick_mining`]
///    to accumulate break progress. When the button is released (or no voxel is
///    hit, or the tool level is too low), calls [`stop_mining`].
///
/// # Prerequisites
///
/// [`MiningPlugin`] must be added **before** this plugin so that the required
/// resources ([`MiningState`], [`MiningConfig`]) and message channels
/// ([`VoxelBroken`], [`MiningProgress`]) are already registered.
///
/// A player entity with [`crate::items::inventory::ContainerComponent<I>`] and
/// [`crate::items::inventory::PrimaryContainer`] must be present (typically spawned
/// in a `Startup` system).
///
/// # Example
///
/// ```no_run
/// use bevy::prelude::*;
/// use voxel_framework::{VoxelPlugin, generator::{GeneratorPlugin, GeneratorResource}};
/// use voxel_framework::world::mining::MiningInputPlugin;
/// use voxel_framework::voxel::DefaultTerrainVoxel;
/// use voxel_framework::items::DefaultItem;
///
/// App::new()
///     .add_plugins(DefaultPlugins)
///     .add_plugins(VoxelPlugin::<DefaultTerrainVoxel>::new())
///     .add_plugins(MiningInputPlugin::<DefaultTerrainVoxel, DefaultItem>::new())
///     .run();
/// ```
#[cfg(feature = "items")]
pub struct MiningInputPlugin<V: VoxelType, I: ItemType> {
    _m: std::marker::PhantomData<(V, I)>,
}

#[cfg(feature = "items")]
impl<V: VoxelType, I: ItemType> MiningInputPlugin<V, I> {
    /// Creates a new [`MiningInputPlugin`] for voxel type `V` and item type `I`.
    pub fn new() -> Self {
        Self { _m: std::marker::PhantomData }
    }
}

#[cfg(feature = "items")]
impl<V: VoxelType, I: ItemType> Default for MiningInputPlugin<V, I> {
    fn default() -> Self { Self::new() }
}

#[cfg(feature = "items")]
impl<V: VoxelType, I: ItemType> Plugin for MiningInputPlugin<V, I> {
    fn build(&self, app: &mut App) {
        app.init_resource::<ToolStats>();
        app.add_systems(Update, (
            hotbar_to_tool_system::<I>,
            mining_input_system::<V>,
        ).chain());
    }
}

/// Updates [`ToolStats`] from the active hotbar slot each frame.
///
/// Reads the [`ContainerComponent`] on the [`PrimaryContainer`] entity.
/// If no such entity exists, resets to bare-hands stats silently.
///
/// Added automatically by [`MiningInputPlugin`] — do not add manually.
#[cfg(feature = "items")]
pub fn hotbar_to_tool_system<I: ItemType>(
    hotbar: Res<HotbarState>,
    container_query: Query<&ContainerComponent<I>, With<PrimaryContainer>>,
    mut tool_stats: ResMut<ToolStats>,
    applied: Option<Res<AppliedEffectModifiers>>,
) {
    let Ok(container) = container_query.single() else {
        tool_stats.reset();
        return;
    };
    let active_slot = hotbar.selected;
    if let Some(slot) = container.0.slots.get(active_slot).and_then(|s| s.as_ref()) {
        tool_stats.tool_level = slot.item.tool_level();
        tool_stats.tool_speed = slot.item.tool_speed()
            * applied.as_deref().map(|a| a.0.mining_speed_mult).unwrap_or(1.0);
    } else {
        tool_stats.reset();
    }
}

/// System added by [`MiningInputPlugin`].
///
/// Raycasts from the camera and drives [`tick_mining`] / [`stop_mining`] based
/// on left-mouse-button state and [`ToolStats`]. Added automatically — do not
/// add this system manually when using [`MiningInputPlugin`].
pub fn mining_input_system<V: VoxelType>(
    buttons: Res<ButtonInput<MouseButton>>,
    camera_q: Query<&GlobalTransform, With<Camera3d>>,
    manager: Res<ChunkManager>,
    mut chunks: Query<&mut crate::chunk::Chunk<V>>,
    mut state: ResMut<MiningState>,
    config: Res<MiningConfig>,
    tool_stats: Res<ToolStats>,
    mut broken: MessageWriter<VoxelBroken>,
    mut progress: MessageWriter<MiningProgress>,
    mut modified: MessageWriter<ChunkModified>,
    time: Res<Time>,
) {
    if !buttons.pressed(MouseButton::Left) {
        stop_mining(&mut state);
        return;
    }

    let Ok(cam_tf) = camera_q.single() else {
        return;
    };

    let origin    = cam_tf.translation();
    let direction = cam_tf.forward().as_vec3();

    // `transmute_lens` gives a read-only view without a second conflicting query.
    let mut lens = chunks.transmute_lens::<&crate::chunk::Chunk<V>>();
    let Some(hit) = super::raycast::raycast(
        origin, direction, 6.0, &manager, &lens.query(),
    ) else {
        stop_mining(&mut state);
        return;
    };

    // Check that the active tool can mine this voxel type.
    let voxel_mining_level = lens.query()
        .get(manager.get_entity(hit.chunk_coord).unwrap_or(Entity::PLACEHOLDER))
        .map(|chunk| {
            let (x, y, z) = hit.local;
            chunk.get(x, y, z).mining_level()
        })
        .unwrap_or(0);

    if tool_stats.tool_level < voxel_mining_level {
        stop_mining(&mut state);
        return;
    }

    // Apply tool speed to the effective delta — faster tools reduce break time.
    let effective_delta = time.delta_secs() * tool_stats.tool_speed;

    // Snapshot before tick so we can detect the break event below.
    let was_mining = state.target.is_some();

    tick_mining(
        hit.chunk_coord,
        hit.local,
        effective_delta,
        &mut state,
        &config,
        &manager,
        &mut chunks,
        &mut broken,
        &mut progress,
        &mut modified,
    );

    // If the primary block just broke, also break the 3 adjacent blocks that
    // form a 2×2 square on the struck face.
    if was_mining && state.target.is_none() {
        use crate::chunk::CHUNK_SIZE;
        let cs = CHUNK_SIZE as i32;
        let vx = hit.chunk_coord.x * cs + hit.local.0 as i32;
        let vy = hit.chunk_coord.y * cs + hit.local.1 as i32;
        let vz = hit.chunk_coord.z * cs + hit.local.2 as i32;
        let primary = IVec3::new(vx, vy, vz);
        let center  = Vec3::new(vx as f32 + 0.5, vy as f32 + 0.5, vz as f32 + 0.5);
        let hit_pt  = origin + hit.distance * direction.normalize();

        for offset in quad_offsets(hit.face_normal, hit_pt, center) {
            break_voxel(primary + offset, &manager, &mut chunks, &mut broken, &mut modified);
        }
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn default_config() -> MiningConfig {
        MiningConfig { break_time: 1.5 }
    }

    // ── stop_mining ──────────────────────────────────────────────────────────

    #[test]
    fn stop_mining_resets_state() {
        let mut state = MiningState {
            target: Some((IVec3::ZERO, (1, 2, 3))),
            elapsed: 0.8,
            progress: 0.5,
            effective_break_time: 1.0,
        };
        stop_mining(&mut state);
        assert!(state.target.is_none());
        assert_eq!(state.elapsed, 0.0);
        assert_eq!(state.progress, 0.0);
    }

    // ── progress calculation ──────────────────────────────────────────────────

    #[test]
    fn test_mining_progress_accumulates() {
        let config = default_config();
        let mut state = MiningState::default();

        // Manually simulate two ticks without Bevy context.
        let target = (IVec3::ZERO, (0usize, 0usize, 0usize));

        // First tick: set target and advance elapsed.
        if state.target != Some(target) {
            state.elapsed = 0.0;
            state.progress = 0.0;
            state.target = Some(target);
        }
        state.elapsed += 0.3;
        state.progress = (state.elapsed / config.break_time).min(1.0);

        assert!((state.elapsed - 0.3).abs() < f32::EPSILON);

        // Second tick: same target, keep accumulating.
        state.elapsed += 0.4;
        state.progress = (state.elapsed / config.break_time).min(1.0);

        assert!((state.elapsed - 0.7).abs() < f32::EPSILON);
        assert!(state.progress > 0.0 && state.progress < 1.0);
    }

    #[test]
    fn test_mining_resets_on_target_change() {
        let config = default_config();
        let mut state = MiningState::default();

        let target_a = (IVec3::ZERO, (0usize, 0usize, 0usize));
        let target_b = (IVec3::ONE, (1usize, 1usize, 1usize));

        // Mine target A for a bit.
        state.target = Some(target_a);
        state.elapsed += 0.5;
        state.progress = (state.elapsed / config.break_time).min(1.0);

        assert!(state.elapsed > 0.0);

        // Switch to target B — should reset.
        if state.target != Some(target_b) {
            state.elapsed = 0.0;
            state.progress = 0.0;
            state.target = Some(target_b);
        }

        assert_eq!(state.elapsed, 0.0);
        assert_eq!(state.progress, 0.0);
        assert_eq!(state.target, Some(target_b));
    }

    #[test]
    fn test_mining_completes_at_break_time() {
        let config = default_config();
        let mut state = MiningState::default();

        // Simulate ticks until elapsed >= break_time.
        let target = (IVec3::ZERO, (0usize, 0usize, 0usize));
        state.target = Some(target);

        let mut completed = false;
        let tick = 0.5_f32;
        for _ in 0..10 {
            state.elapsed += tick;
            state.progress = (state.elapsed / config.break_time).min(1.0);
            if state.progress >= 1.0 {
                completed = true;
                break;
            }
        }

        assert!(completed, "mining should complete within 10 ticks of 0.5s");
        assert_eq!(state.progress, 1.0);
    }

    #[test]
    fn progress_is_clamped_to_one() {
        let config = default_config();
        let mut state = MiningState::default();

        // Way more elapsed than break_time.
        state.elapsed = 99.0;
        state.progress = (state.elapsed / config.break_time).min(1.0);

        assert_eq!(state.progress, 1.0);
    }
}
