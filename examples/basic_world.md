  basic_world.rs — Explicación detallada                                                                                                         
                                                                                                                                                 
  1. Imports (líneas 13–20)                                                                                                                      
                                                                                                                                                 
  use std::sync::Arc;                                                                                                                            
  use bevy::prelude::*;                                                                                                                          
  use bevy::audio::{AudioPlayer, AudioSource, PlaybackSettings};                                                                                 
  use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};                                                                
  use bevy::asset::RenderAssetUsages;                                                                                                            
  use voxel_framework::prelude::*;                                                                                                               
  use voxel_framework::audio::{SfxConfig, SfxEvent, SfxRequest};                                                                               
  use voxel_framework::player::PlayerPlugin;                                                                                                     
                                                                                                                                                 
  - Arc — puntero de referencia contada atómica. Se usa para envolver el generador de chunks en un Arc<dyn Trait> y poder compartirlo entre      
  threads (el generador corre en background).                                                                                                    
  - Extent3d, TextureDimension, TextureFormat — tipos del render pipeline para crear imágenes desde código.                                      
  - RenderAssetUsages — le dice a Bevy si la imagen vive en RAM, GPU, o ambas.                                                                   
  - El prelude::* del framework expone todo lo público: VoxelPlugin, DefaultVoxel, ChunkManager, PlayerConfig, etc.                              
                                                                                                                                                 
  ---                                                                                                                                            
  2. PendingTextures Resource (líneas 23–27)                                                                                                     
                                                                                                                                               
  #[derive(Resource)]
  struct PendingTextures {                                                                                                                       
      handles: Vec<Handle<Image>>,
      assembled: bool,                                                                                                                           
  }                                                                                                                                            

  El problema que resuelve: Bevy carga assets de forma asíncrona. Cuando llamás asset_server.load("stone.png"), te devuelve un Handle<Image> — un
   ticket — pero la imagen no está disponible todavía. Necesitás esperar a que el AssetServer la termine de cargar antes de poder armar el
  texture array.                                                                                                                                 
                                                                                                                                               
  PendingTextures es un Resource (datos globales en el mundo ECS) que guarda esos handles pendientes y el flag assembled para no intentar        
  ensamblar dos veces.
                                                                                                                                                 
  ---                                                                                                                                          
  3. load_world_config (líneas 31–41)
                                                                                                                                                 
  fn load_world_config(default_name: &str) -> WorldConfig {
      let args: Vec<String> = std::env::args().collect();                                                                                        
      let name = args.windows(2)                                                                                                               
          .find(|w| w[0] == "--world")                                                                                                           
          ...                                                                                                                                  
      let path = format!("examples/{}.toml", name);                                                                                              
      WorldConfig::from_file(&path)...                                                                                                         
  }                                                                                                                                              
                                                                                                                                               
  Parsea los argumentos de línea de comando buscando --world <nombre>. Si no hay, usa el default ("basic_world"). Luego carga                    
  examples/basic_world.toml con WorldConfig::from_file().                                                                                      
                                                                                                                                                 
  WorldConfig es el struct del framework que tiene: nombre del mundo, posición de spawn, radio de carga de chunks, semilla del generador, etc.   
   
  windows(2) es un iterator adapter de Rust que itera el slice en ventanas deslizantes de tamaño 2: ["--world", "minetest"], ["minetest",        
  "algo"], etc. Filtra por la ventana donde w[0] == "--world" y toma w[1].                                                                     
                                                                                                                                                 
  ---                                                             
  4. main (líneas 43–90)
                        
  El corazón del ejemplo. Crea la App de Bevy y registra todos los plugins:
                                                                                                                                                 
  App::new()
      .add_plugins(DefaultPlugins)                              // ← (1)                                                                         
      .add_plugins(VoxelPlugin::<DefaultVoxel>::new())          // ← (2)                                                                         
      .add_plugins(GeneratorPlugin::<DefaultVoxel>::new())      // ← (3)
      .add_plugins(PersistencePlugin::<DefaultVoxel>::with_world(...)) // ← (4)                                                                  
      .add_plugins(PauseMenuPlugin)                             // ← (5)                                                                         
      .add_plugins(PlayerPlugin::<DefaultVoxel>::with_world(...))      // ← (6)                                                                  
                                                                                                                                                 
  (1) DefaultPlugins — El bundle completo de Bevy: ventana, input, renderer, assets, audio, etc.                                                 
                                                                                                                                                 
  (2) VoxelPlugin::<DefaultVoxel> — El plugin batteries-included del framework. Internamente usa VoxelCorePlugin y agrega skybox, fragmentos,    
  audio, hotbar, UI. El parámetro genérico <DefaultVoxel> le dice qué tipo de voxel va a usar.
                                                                                                                                                 
  (3) GeneratorPlugin::<DefaultVoxel> — Conecta el generador de chunks (en background threads) al ChunkManager. Sin este plugin, los chunks      
  aparecerían vacíos.
                                                                                                                                                 
  (4) PersistencePlugin::<DefaultVoxel> — Guarda y carga chunks modificados en disco. with_world("basic_world") usa esa carpeta como directorio  
  de save.
                                                                                                                                                 
  (5) PauseMenuPlugin — El menú de pausa (Escape). Registra el estado PauseMenuState que gatilla gameplay_active en el player.                   
  
  (6) PlayerPlugin::<DefaultVoxel> — El jugador completo: cámara, movimiento, colisión, mining input, sprint/crouch, footsteps.                  
                                                                  
  SfxConfig (líneas 55–77)                                                                                                                       
                                                                  
  .insert_resource({                                                                                                                             
      let mut sfx = SfxConfig::new();                             
      sfx.register(SfxEvent::Mining, vec!["audio/impactMining_000.ogg", ...]);                                                                   
      sfx                                                                                                                                        
  })                                                                                                                                             
                                                                                                                                                 
  insert_resource inserta un Resource directamente. Acá se crea un SfxConfig y se mapean eventos de juego (SfxEvent::Mining,                     
  SfxEvent::BlockPlace, etc.) a listas de archivos .ogg. El sistema de audio usa round-robin para rotar entre los archivos y no repetir siempre
  el mismo sonido.                                                                                                                               
                                                                  
  GeneratorResource (líneas 78–80)

  .insert_resource(GeneratorResource::<DefaultVoxel> {
      generator: Arc::new(generator),                                                                                                            
  })
                                                                                                                                                 
  generator fue creado en la línea 45: world_config.build_asteroid_generator(). Lo envuelve en Arc porque el generador corre en threads separados
   y Bevy necesita que sea Send + Sync.
                                                                                                                                                 
  ---                                                             
  5. play_sfx system (líneas 92–106)

  fn play_sfx(
      mut commands: Commands,                                                                                                                    
      asset_server: Res<AssetServer>,
      sfx_config: Res<SfxConfig>,                                                                                                                
      mut sfx_reader: bevy::ecs::message::MessageReader<SfxRequest>,
      mut seed: Local<u64>,                                                                                                                      
  )
                                                                                                                                                 
  Este system corre cada frame (Update). Lee mensajes del canal SfxRequest (emitidos por el framework cuando se rompe un bloque, se coloca uno,  
  etc.), y para cada uno:
                                                                                                                                                 
  1. Incrementa seed (es un Local<u64> — estado local del sistema, persiste entre frames)                                                        
  2. Pide a SfxConfig que elija un path de audio con pick_path(&event, seed)
  3. Carga el asset y spawnea una entidad (AudioPlayer(handle), PlaybackSettings::ONCE)                                                          
                                                                                                                                                 
  PlaybackSettings::ONCE le dice a Bevy que reproduzca el audio una vez y destruya la entidad cuando termina. Es el patrón estándar de Bevy para 
  efectos de sonido.                                                                                                                             
                                                                                                                                                 
  ---                                                             
  6. setup system (líneas 108–153)
                                                                                                                                                 
  Corre una sola vez al inicio (Startup):
                                                                                                                                                 
  ViewerPosition (línea 115)                                                                                                                     
  viewer.position = config.spawn_position;                                                                                                       
  El framework necesita saber dónde está el jugador para cargar chunks alrededor. ViewerPosition es el Resource que maneja eso. Se inicializa con
   la posición de spawn del config.                                                                                                              
                                                                                                                                                 
  DirectionalLight (líneas 117–124)                                                                                                              
  commands.spawn((                                                                                                                               
      DirectionalLight { illuminance: 15_000.0, shadows_enabled: true, ..default() },
      Transform::from_xyz(100.0, 200.0, 100.0).looking_at(Vec3::ZERO, Vec3::Y),                                                                  
  ));                                                                                                                                            
  El sol. illuminance: 15_000.0 es la intensidad en lux (el sol real es ~100.000 lux, 15.000 da un día soleado pero no agresivo).                
  looking_at(Vec3::ZERO, Vec3::Y) orienta la luz hacia el origen.                                                                                
                                                                                                                                                 
  Layer 0 — placeholder gris (líneas 130–142)                     
  let grey_pixel: Vec<u8> = (0..32 * 32)                                                                                                         
      .flat_map(|_| [128u8, 128, 128, 255])                       
      .collect();                                                                                                                                
  El texture array necesita que el layer 0 exista (Air usa ese índice), pero Air nunca renderiza geometría. Se genera programáticamente un tile
  32×32 gris en lugar de cargar un archivo.                                                                                                      
                                                                                                                                                 
  flat_map(|_| [128u8, 128, 128, 255]) — por cada píxel (0..1024), emite 4 bytes RGBA: gris medio, alpha 255.                                    
                                                                                                                                                 
  Layers 1–4 (líneas 144–148)                                     
  let stone   = asset_server.load("basic_world/textures/stone.png");                                                                             
  Carga las texturas reales. En este punto solo devuelve Handle<Image> — la carga es asíncrona.
                                                                                                                                                 
  PendingTextures (líneas 150–153)                                                                                                               
  Guarda los handles en el resource para que assemble_texture_array pueda esperar su resolución.                                                 
                                                                                                                                                 
  ---                                                             
  7. assemble_texture_array system (líneas 158–193)                                                                                              
                                                                                                                                                 
  let all_ready = pending.handles.iter().all(|h| images.contains(h));
  if !all_ready { return; }                                                                                                                      
                                                                  
  Corre cada frame hasta que todos los assets estén cargados. images.contains(h) devuelve true cuando el AssetServer terminó de cargar esa imagen
   y la insertó en Assets<Image>.                                 
                                                                                                                                                 
  Una vez listos:                                                                                                                                
  1. Llama normalise_to_rgba32 en cada imagen (estandariza el formato)
  2. Llama build_texture_array(&layer_images) — función del framework que arma un Texture2DArray de Bevy con N layers                            
  3. Inserta ChunkTextureArray { handle } como resource — el chunk renderer lo detecta y empieza a usarlo            
                                                                                                                                                 
  pending.assembled = true — el check del principio evita que corra de nuevo la próxima frame.                                                   
                                                                                                                                                 
  ---                                                                                                                                            
  8. normalise_to_rgba32 (líneas 201–258)                                                                                                        
                                                                  
  Este es el helper más técnico. Cuando Bevy decodifica un PNG, el formato resultante depende del tipo de PNG:
  - PNG RGBA → Rgba8UnormSrgb ✅ directo                                                                                                         
  - PNG grayscale → R8Unorm ❌ hay que expandir a RGBA                                                                                           
  - PNG con 2 canales → Rg8Unorm ❌ ídem                                                                                                         
  - Cualquier otro → fallback magenta brillante (para que sea obvio que algo falló)                                                              
                                                                                                                                                 
  Además escala a 32×32 con nearest-neighbour si la textura tiene otro tamaño:                                                                   
  let sx = (tx * w / TARGET_W) as usize;                                                                                                         
  let sy = (ty * h / TARGET_H) as usize;                                                                                                         
  Mapea cada pixel destino al pixel fuente correspondiente. Es la escala más básica pero suficiente para pixel art voxel.                        
                                                                  
  ---                                                                                                                                            
  Flujo completo de arranque                                      
                                                                                                                                                 
  main()                                                          
    ├─ Registra plugins                                                                                                                          
    ├─ [Startup] setup()
    │     ├─ Posiciona ViewerPosition                                                                                                            
    │     ├─ Spawna luz solar                                                                                                                    
    │     └─ Carga handles de textura → PendingTextures
    │                                                                                                                                            
    └─ [Update] (cada frame)                                      
          ├─ assemble_texture_array()                                                                                                            
          │     └─ (espera) → cuando listo: arma array → ChunkTextureArray
          │           └─ El ChunkMesh renderer lo detecta y renderiza chunks                                                                     
          └─ play_sfx()                                                                                                                          
                └─ Consume SfxRequest → spawna AudioPlayer entities                                                                              
                                 