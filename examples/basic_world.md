  basic_world.rs — Detailed walkthrough

  1. Imports (lines 13–20)

  use std::sync::Arc;
  use bevy::prelude::*;
  use bevy::audio::{AudioPlayer, AudioSource, PlaybackSettings};
  use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
  use bevy::asset::RenderAssetUsages;
  use voxel_framework::prelude::*;
  use voxel_framework::audio::{SfxConfig, SfxEvent, SfxRequest};
  use voxel_framework::player::PlayerPlugin;

  - Arc — an atomically reference-counted pointer. It wraps the chunk generator in an
  Arc<dyn Trait> so it can be shared across threads (the generator runs in the background).
  - Extent3d, TextureDimension, TextureFormat — render-pipeline types for building images from code.
  - RenderAssetUsages — tells Bevy whether the image lives in RAM, on the GPU, or both.
  - The framework's prelude::* exposes everything public: VoxelPlugin, DefaultVoxel, ChunkManager, PlayerConfig, etc.

  ---
  2. PendingTextures Resource (lines 23–27)

  #[derive(Resource)]
  struct PendingTextures {
      handles: Vec<Handle<Image>>,
      assembled: bool,
  }

  The problem it solves: Bevy loads assets asynchronously. When you call asset_server.load("stone.png"),
  it hands you back a Handle<Image> — a ticket — but the image isn't available yet. You have to wait for
  the AssetServer to finish loading it before you can assemble the texture array.

  PendingTextures is a Resource (global data in the ECS world) that holds those pending handles plus an
  assembled flag so we don't try to assemble twice.

  ---
  3. load_world_config (lines 31–41)

  fn load_world_config(default_name: &str) -> WorldConfig {
      let args: Vec<String> = std::env::args().collect();
      let name = args.windows(2)
          .find(|w| w[0] == "--world")
          ...
      let path = format!("examples/{}.toml", name);
      WorldConfig::from_file(&path)...
  }

  Parses command-line arguments looking for --world <name>. If none is given, it uses the default
  ("basic_world"). Then it loads examples/basic_world.toml with WorldConfig::from_file().

  WorldConfig is the framework struct holding: world name, spawn position, chunk load radius, generator seed, etc.

  windows(2) is a Rust iterator adapter that iterates the slice in sliding windows of size 2:
  ["--world", "minetest"], ["minetest", "something"], etc. It filters for the window where
  w[0] == "--world" and takes w[1].

  ---
  4. main (lines 43–90)

  The heart of the example. It creates the Bevy App and registers every plugin:

  App::new()
      .add_plugins(DefaultPlugins)                              // ← (1)
      .add_plugins(VoxelPlugin::<DefaultVoxel>::new())          // ← (2)
      .add_plugins(GeneratorPlugin::<DefaultVoxel>::new())      // ← (3)
      .add_plugins(PersistencePlugin::<DefaultVoxel>::with_world(...)) // ← (4)
      .add_plugins(PauseMenuPlugin)                             // ← (5)
      .add_plugins(PlayerPlugin::<DefaultVoxel>::with_world(...))      // ← (6)

  (1) DefaultPlugins — Bevy's full bundle: window, input, renderer, assets, audio, etc.

  (2) VoxelPlugin::<DefaultVoxel> — the framework's batteries-included plugin. Internally it uses
  VoxelCorePlugin and adds skybox, fragments, audio, hotbar, and UI. The generic parameter <DefaultVoxel>
  tells it which voxel type to use.

  (3) GeneratorPlugin::<DefaultVoxel> — wires the chunk generator (on background threads) to the
  ChunkManager. Without this plugin, chunks would come up empty.

  (4) PersistencePlugin::<DefaultVoxel> — saves modified chunks to disk and restores them. Terrain you never edit is not saved.
  with_world("basic_world") uses that folder as the save directory.

  (5) PauseMenuPlugin — the pause menu (Escape). It registers the PauseMenuState that drives
  gameplay_active on the player.

  (6) PlayerPlugin::<DefaultVoxel> — the full player: camera, movement, collision, mining input,
  sprint/crouch, footsteps.

  SfxConfig (lines 55–77)

  .insert_resource({
      let mut sfx = SfxConfig::new();
      sfx.register(SfxEvent::Mining, vec!["audio/impactMining_000.ogg", ...]);
      sfx
  })

  insert_resource inserts a Resource directly. Here we build an SfxConfig and map game events
  (SfxEvent::Mining, SfxEvent::BlockPlace, etc.) to lists of .ogg files. The audio system uses
  round-robin to rotate through the files so it doesn't always play the same sound.

  GeneratorResource (lines 78–80)

  .insert_resource(GeneratorResource::<DefaultVoxel> {
      generator: Arc::new(generator),
  })

  generator was created on line 45: world_config.build_asteroid_generator(). We wrap it in Arc because
  the generator runs on separate threads and Bevy needs it to be Send + Sync.

  ---
  5. play_sfx system (lines 92–106)

  fn play_sfx(
      mut commands: Commands,
      asset_server: Res<AssetServer>,
      sfx_config: Res<SfxConfig>,
      mut sfx_reader: bevy::ecs::message::MessageReader<SfxRequest>,
      mut seed: Local<u64>,
  )

  This system runs every frame (Update). It reads messages from the SfxRequest channel (emitted by the
  framework when a block is broken, one is placed, etc.), and for each one:

  1. Increments seed (a Local<u64> — per-system state that persists across frames)
  2. Asks SfxConfig to pick an audio path with pick_path(&event, seed)
  3. Loads the asset and spawns an entity (AudioPlayer(handle), PlaybackSettings::ONCE)

  PlaybackSettings::ONCE tells Bevy to play the audio once and despawn the entity when it finishes. It's
  the standard Bevy pattern for sound effects.

  ---
  6. setup system (lines 108–153)

  Runs once at startup (Startup):

  ViewerPosition (line 115)
  viewer.position = config.spawn_position;
  The framework needs to know where the player is in order to load chunks around it. ViewerPosition is the
  Resource that tracks that. It's initialized with the spawn position from the config.

  DirectionalLight (lines 117–124)
  commands.spawn((
      DirectionalLight { illuminance: 15_000.0, shadows_enabled: true, ..default() },
      Transform::from_xyz(100.0, 200.0, 100.0).looking_at(Vec3::ZERO, Vec3::Y),
  ));
  The sun. illuminance: 15_000.0 is the intensity in lux (real sunlight is ~100,000 lux; 15,000 gives a
  sunny but not harsh day). looking_at(Vec3::ZERO, Vec3::Y) aims the light at the origin.

  Layer 0 — grey placeholder (lines 130–142)
  let grey_pixel: Vec<u8> = (0..32 * 32)
      .flat_map(|_| [128u8, 128, 128, 255])
      .collect();
  The texture array requires layer 0 to exist (Air uses that index), but Air never renders geometry. We
  generate a 32×32 grey tile programmatically instead of loading a file.

  flat_map(|_| [128u8, 128, 128, 255]) — for each pixel (0..1024), emits 4 RGBA bytes: mid-grey, alpha 255.

  Layers 1–4 (lines 144–148)
  let stone   = asset_server.load("basic_world/textures/stone.png");
  Loads the real textures. At this point it only returns a Handle<Image> — the load is asynchronous.

  PendingTextures (lines 150–153)
  Stores the handles in the resource so assemble_texture_array can wait for them to resolve.

  ---
  7. assemble_texture_array system (lines 158–193)

  let all_ready = pending.handles.iter().all(|h| images.contains(h));
  if !all_ready { return; }

  Runs every frame until all assets are loaded. images.contains(h) returns true once the AssetServer has
  finished loading that image and inserted it into Assets<Image>.

  Once they're ready:
  1. Calls normalise_to_rgba32 on each image (standardizes the format)
  2. Calls build_texture_array(&layer_images) — a framework function that builds a Bevy Texture2DArray with N layers
  3. Inserts ChunkTextureArray { handle } as a resource — the chunk renderer detects it and starts using it

  pending.assembled = true — the check at the top prevents it from running again next frame.

  ---
  8. normalise_to_rgba32 (lines 201–258)

  This is the most technical helper. When Bevy decodes a PNG, the resulting format depends on the PNG type:
  - RGBA PNG → Rgba8UnormSrgb ✅ direct
  - grayscale PNG → R8Unorm ❌ must expand to RGBA
  - 2-channel PNG → Rg8Unorm ❌ same
  - anything else → bright magenta fallback (so it's obvious something failed)

  It also scales to 32×32 with nearest-neighbour if the texture is a different size:
  let sx = (tx * w / TARGET_W) as usize;
  let sy = (ty * h / TARGET_H) as usize;
  Maps each destination pixel to the corresponding source pixel. It's the most basic scaling but good
  enough for voxel pixel art.

  ---
  Full startup flow

  main()
    ├─ Registers plugins
    ├─ [Startup] setup()
    │     ├─ Positions ViewerPosition
    │     ├─ Spawns sunlight
    │     └─ Loads texture handles → PendingTextures
    │
    └─ [Update] (every frame)
          ├─ assemble_texture_array()
          │     └─ (waits) → once ready: builds array → ChunkTextureArray
          │           └─ The ChunkMesh renderer detects it and renders chunks
          └─ play_sfx()
                └─ Consumes SfxRequest → spawns AudioPlayer entities
