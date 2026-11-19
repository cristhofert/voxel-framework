# Design

Keep lifetime ticking in the existing magnet system, before optional collector-dependent spatial work. Zero/multiple collectors skip attraction and pickup, but never expiration. Preserve movement semantics otherwise.

Add `items::drop_visuals::DropVisualPlugin<I>` (new/default, PhantomData), gated by items. It attaches Mesh3d and MeshMaterial3d<StandardMaterial> directly to positioned drops lacking both components. Shared cube mesh (0.25 world units) and per-item unlit material cache use full item equality, not byte IDs. Colors come from ItemType::color, with alpha blending for translucent items. No gameplay mutation, child entities, lights, or external assets. Existing custom meshes/materials are skipped. Asset resources must be provided by DefaultPlugins (or explicitly in headless tests).

The attachment system runs in PostUpdate before TransformSystems::Propagate and VisibilitySystems::VisibilityPropagate; Update-spawned drops are rendered in the same frame. Query all eligible drops, so preexisting drops and late-added transforms work. Cache initialized once and retained across despawns; asset count grows with distinct item variants, not drop count. No promise to dynamically recolor an already rendered drop if its item field is changed by game code.
