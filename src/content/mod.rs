//! Author-defined content registries for voxel games.
//!
//! `content` is the first gameplay-definition layer in `voxel-framework`.
//! It does **not** ship a game preset and it does not impose a closed taxonomy
//! of block categories. Instead, it gives the game author a Bevy resource where
//! block definitions can be registered, queried, tagged, and annotated with
//! custom attributes.
//!
//! The framework still needs a few technical contracts to drive engine systems
//! such as meshing, rendering, and collision. Those live in
//! [`BlockEngineProperties`]. Everything else — gameplay categories, hardness
//! semantics, factions, rarity, economy values, magic conductivity, oxygen
//! rules, and so on — belongs to game-authored [`ContentValue`] attributes or
//! author-defined tags.
//!
//! # Example
//!
//! ```rust
//! use bevy::prelude::*;
//! use voxel_framework::content::{
//!     BlockDefinition, BlockRegistry, ContentValue, VoxelContentPlugin,
//! };
//! use voxel_framework::voxel::DefaultTerrainVoxel;
//!
//! fn register_blocks(mut blocks: ResMut<BlockRegistry<DefaultTerrainVoxel>>) {
//!     blocks
//!         .register(
//!             BlockDefinition::new("moon_regolith", DefaultTerrainVoxel::Sand)
//!                 .named("Moon Regolith")
//!                 .tag("terrain")
//!                 .attribute("hardness", 0.8)
//!                 .attribute("dusty", true)
//!                 .attribute("drops", ContentValue::strings(["moon_regolith"])),
//!         )
//!         .expect("block id and voxel value should be unique");
//! }
//!
//! App::new()
//!     .add_plugins(MinimalPlugins)
//!     .add_plugins(VoxelContentPlugin::<DefaultTerrainVoxel>::new())
//!     .add_systems(Startup, register_blocks);
//! ```

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::hash::Hash;
use std::marker::PhantomData;

use bevy::prelude::{App, Plugin, Resource};
use serde::{Deserialize, Serialize};

use crate::voxel::{DefaultVoxel, VoxelType};

/// Stable identifier for author-defined content.
///
/// `ContentId` is intentionally just a string newtype. The framework does not
/// prescribe a namespace scheme; game authors can use simple ids
/// (`"stone"`) or namespaced ids (`"my_game:stone"`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ContentId(String);

impl ContentId {
    /// Creates a new content id from any string-like value.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Returns the raw id string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for ContentId {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for ContentId {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl AsRef<str> for ContentId {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for ContentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Author-defined attribute value attached to content definitions.
///
/// This type is deliberately broad but small. Use it for gameplay metadata
/// that the framework should store and expose without interpreting globally.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ContentValue {
    /// Boolean flag, e.g. `flammable = true`.
    Bool(bool),
    /// Signed integer, e.g. `fuel_value = 20`.
    Integer(i64),
    /// Floating-point number, e.g. `hardness = 1.25`.
    Float(f64),
    /// Text value, e.g. `required_tool = "pickaxe"`.
    Text(String),
    /// Ordered list of text values, e.g. drops, groups, or recipe ids.
    TextList(Vec<String>),
    /// Ordered list of numeric values.
    NumberList(Vec<f64>),
    /// RGBA color represented as four linear floats.
    Color([f32; 4]),
}

impl ContentValue {
    /// Convenience constructor for string lists.
    pub fn strings(values: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self::TextList(values.into_iter().map(Into::into).collect())
    }

    /// Returns this value as a boolean when it stores one.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(value) => Some(*value),
            _ => None,
        }
    }

    /// Returns this value as a signed integer when it stores one.
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Self::Integer(value) => Some(*value),
            _ => None,
        }
    }

    /// Returns this value as a float when it stores one.
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Float(value) => Some(*value),
            _ => None,
        }
    }

    /// Returns this value as text when it stores text.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::Text(value) => Some(value.as_str()),
            _ => None,
        }
    }

    /// Returns this value as a text list when it stores one.
    pub fn as_text_list(&self) -> Option<&[String]> {
        match self {
            Self::TextList(value) => Some(value.as_slice()),
            _ => None,
        }
    }
}

impl From<bool> for ContentValue {
    fn from(value: bool) -> Self {
        Self::Bool(value)
    }
}

impl From<i64> for ContentValue {
    fn from(value: i64) -> Self {
        Self::Integer(value)
    }
}

impl From<i32> for ContentValue {
    fn from(value: i32) -> Self {
        Self::Integer(value.into())
    }
}

impl From<u32> for ContentValue {
    fn from(value: u32) -> Self {
        Self::Integer(value.into())
    }
}

impl From<f32> for ContentValue {
    fn from(value: f32) -> Self {
        Self::Float(value.into())
    }
}

impl From<f64> for ContentValue {
    fn from(value: f64) -> Self {
        Self::Float(value)
    }
}

impl From<&str> for ContentValue {
    fn from(value: &str) -> Self {
        Self::Text(value.to_owned())
    }
}

impl From<String> for ContentValue {
    fn from(value: String) -> Self {
        Self::Text(value)
    }
}

impl From<[f32; 4]> for ContentValue {
    fn from(value: [f32; 4]) -> Self {
        Self::Color(value)
    }
}

/// Engine-facing block contracts derived from or attached to a voxel value.
///
/// These are not gameplay categories. They are the minimum technical facts the
/// framework may need for systems such as rendering, meshing, collision, and
/// visibility. Game-specific meaning belongs in [`BlockDefinition::attributes`]
/// and [`BlockDefinition::tags`].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BlockEngineProperties {
    /// Whether this block should emit voxel mesh geometry.
    pub emits_mesh: bool,
    /// Whether this block participates in collision queries.
    pub collidable: bool,
    /// Visibility/occlusion hint in the range `0.0..=1.0`.
    pub opacity: f32,
    /// Default RGBA color.
    pub color: [f32; 4],
    /// Texture-array layer for each face: +Y, -Y, +X, -X, +Z, -Z.
    pub texture_layers: [u32; 6],
}

impl BlockEngineProperties {
    /// Creates engine properties with explicit values.
    pub fn new(
        emits_mesh: bool,
        collidable: bool,
        opacity: f32,
        color: [f32; 4],
        texture_layers: [u32; 6],
    ) -> Self {
        Self {
            emits_mesh,
            collidable,
            opacity,
            color,
            texture_layers,
        }
    }

    /// Derives engine properties from an existing [`VoxelType`] value.
    pub fn from_voxel<V: VoxelType>(voxel: V) -> Self {
        Self {
            emits_mesh: voxel.is_solid(),
            collidable: voxel.is_solid(),
            opacity: voxel.alpha(),
            color: voxel.color(),
            texture_layers: [
                voxel.texture_layer(0),
                voxel.texture_layer(1),
                voxel.texture_layer(2),
                voxel.texture_layer(3),
                voxel.texture_layer(4),
                voxel.texture_layer(5),
            ],
        }
    }

    /// A fully empty technical block: no mesh, no collider, no opacity.
    pub fn empty() -> Self {
        Self::new(false, false, 0.0, [0.0, 0.0, 0.0, 0.0], [0; 6])
    }
}

/// Author-defined block content associated with a concrete voxel value.
#[derive(Debug, Clone, PartialEq)]
pub struct BlockDefinition<V: VoxelType> {
    id: ContentId,
    voxel: V,
    display_name: String,
    engine: BlockEngineProperties,
    attributes: HashMap<String, ContentValue>,
    tags: HashSet<String>,
}

impl<V: VoxelType> BlockDefinition<V> {
    /// Creates a block definition for the given id and voxel value.
    pub fn new(id: impl Into<ContentId>, voxel: V) -> Self {
        Self {
            id: id.into(),
            voxel,
            display_name: voxel.name().to_owned(),
            engine: BlockEngineProperties::from_voxel(voxel),
            attributes: HashMap::new(),
            tags: HashSet::new(),
        }
    }

    /// Returns the stable author-defined id.
    pub fn id(&self) -> &ContentId {
        &self.id
    }

    /// Returns the voxel value this definition describes.
    pub fn voxel(&self) -> V {
        self.voxel
    }

    /// Returns the display name intended for UI/docs.
    pub fn display_name(&self) -> &str {
        &self.display_name
    }

    /// Replaces the display name.
    pub fn named(mut self, name: impl Into<String>) -> Self {
        self.display_name = name.into();
        self
    }

    /// Returns engine-facing technical properties.
    pub fn engine(&self) -> BlockEngineProperties {
        self.engine
    }

    /// Replaces engine-facing technical properties.
    pub fn engine_properties(mut self, engine: BlockEngineProperties) -> Self {
        self.engine = engine;
        self
    }

    /// Adds or replaces an author-defined gameplay attribute.
    pub fn attribute(mut self, key: impl Into<String>, value: impl Into<ContentValue>) -> Self {
        self.attributes.insert(key.into(), value.into());
        self
    }

    /// Adds an author-defined tag.
    pub fn tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.insert(tag.into());
        self
    }

    /// Returns a gameplay attribute by key.
    pub fn get_attribute(&self, key: &str) -> Option<&ContentValue> {
        self.attributes.get(key)
    }

    /// Returns all gameplay attributes.
    pub fn attributes(&self) -> &HashMap<String, ContentValue> {
        &self.attributes
    }

    /// Returns true when the author-defined tag exists.
    pub fn has_tag(&self, tag: &str) -> bool {
        self.tags.contains(tag)
    }

    /// Returns all author-defined tags.
    pub fn tags(&self) -> &HashSet<String> {
        &self.tags
    }
}

/// Errors returned by [`BlockRegistry::register`].
#[derive(Debug, Clone, PartialEq)]
pub enum ContentRegistryError<V: VoxelType> {
    /// A block with the same id already exists.
    DuplicateId { id: ContentId },
    /// The voxel value is already associated with a different block id.
    DuplicateVoxel { voxel: V, existing_id: ContentId },
}

impl<V: VoxelType + fmt::Debug> fmt::Display for ContentRegistryError<V> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateId { id } => {
                write!(f, "content id `{id}` is already registered")
            }
            Self::DuplicateVoxel { voxel, existing_id } => {
                write!(
                    f,
                    "voxel value `{voxel:?}` is already registered as `{existing_id}`"
                )
            }
        }
    }
}

impl<V: VoxelType + fmt::Debug> std::error::Error for ContentRegistryError<V> {}

/// Registry of author-defined block content.
///
/// The registry is intentionally a plain Bevy resource. Add
/// [`VoxelContentPlugin`] to initialize it, then populate it from startup
/// systems or your own setup code.
#[derive(Debug, Clone, Resource)]
pub struct BlockRegistry<V: VoxelType> {
    blocks_by_id: HashMap<ContentId, BlockDefinition<V>>,
    ids_by_voxel: HashMap<V, ContentId>,
}

impl<V: VoxelType> Default for BlockRegistry<V> {
    fn default() -> Self {
        Self {
            blocks_by_id: HashMap::new(),
            ids_by_voxel: HashMap::new(),
        }
    }
}

impl<V: VoxelType> BlockRegistry<V> {
    /// Registers a new block definition.
    ///
    /// Use [`BlockRegistry::insert_or_replace`] when your setup intentionally
    /// overwrites an existing definition.
    pub fn register(
        &mut self,
        definition: BlockDefinition<V>,
    ) -> Result<(), ContentRegistryError<V>> {
        if self.blocks_by_id.contains_key(definition.id()) {
            return Err(ContentRegistryError::DuplicateId {
                id: definition.id().clone(),
            });
        }

        if let Some(existing_id) = self.ids_by_voxel.get(&definition.voxel()) {
            return Err(ContentRegistryError::DuplicateVoxel {
                voxel: definition.voxel(),
                existing_id: existing_id.clone(),
            });
        }

        self.ids_by_voxel
            .insert(definition.voxel(), definition.id().clone());
        self.blocks_by_id
            .insert(definition.id().clone(), definition);
        Ok(())
    }

    /// Inserts a definition, replacing any previous definition for the same id
    /// or voxel value.
    pub fn insert_or_replace(&mut self, definition: BlockDefinition<V>) {
        if let Some(old) = self.blocks_by_id.remove(definition.id()) {
            self.ids_by_voxel.remove(&old.voxel());
        }
        if let Some(old_id) = self.ids_by_voxel.remove(&definition.voxel()) {
            self.blocks_by_id.remove(&old_id);
        }

        self.ids_by_voxel
            .insert(definition.voxel(), definition.id().clone());
        self.blocks_by_id
            .insert(definition.id().clone(), definition);
    }

    /// Returns a definition by content id.
    pub fn get(&self, id: impl AsRef<str>) -> Option<&BlockDefinition<V>> {
        let id = ContentId::new(id.as_ref());
        self.blocks_by_id.get(&id)
    }

    /// Returns a definition by voxel value.
    pub fn get_by_voxel(&self, voxel: V) -> Option<&BlockDefinition<V>> {
        self.ids_by_voxel
            .get(&voxel)
            .and_then(|id| self.blocks_by_id.get(id))
    }

    /// Returns true if an id exists.
    pub fn contains_id(&self, id: impl AsRef<str>) -> bool {
        self.get(id).is_some()
    }

    /// Number of registered block definitions.
    pub fn len(&self) -> usize {
        self.blocks_by_id.len()
    }

    /// Returns true when no definitions are registered.
    pub fn is_empty(&self) -> bool {
        self.blocks_by_id.is_empty()
    }

    /// Iterates definitions in unspecified hash-map order.
    pub fn iter(&self) -> impl Iterator<Item = &BlockDefinition<V>> {
        self.blocks_by_id.values()
    }
}

/// Horizontal plugin that initializes author-defined content registries.
///
/// This plugin does not compose game presets or default content. It only
/// installs the resources that game authors populate.
pub struct VoxelContentPlugin<V: VoxelType = DefaultVoxel> {
    _marker: PhantomData<V>,
}

impl<V: VoxelType> VoxelContentPlugin<V> {
    /// Creates a new content plugin for voxel type `V`.
    pub fn new() -> Self {
        Self {
            _marker: PhantomData,
        }
    }
}

impl<V: VoxelType> Default for VoxelContentPlugin<V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<V: VoxelType> Plugin for VoxelContentPlugin<V> {
    fn build(&self, app: &mut App) {
        app.init_resource::<BlockRegistry<V>>();
    }
}

#[cfg(test)]
mod tests {
    use bevy::prelude::{App, MinimalPlugins};

    use super::*;
    use crate::voxel::DefaultTerrainVoxel;

    #[test]
    fn block_definition_keeps_author_defined_attributes_and_tags() {
        let definition = BlockDefinition::new("regolith", DefaultTerrainVoxel::Sand)
            .named("Regolith")
            .tag("terrain")
            .attribute("hardness", 0.8)
            .attribute("dusty", true)
            .attribute("drops", ContentValue::strings(["regolith"]));

        assert_eq!(definition.id().as_str(), "regolith");
        assert_eq!(definition.voxel(), DefaultTerrainVoxel::Sand);
        assert_eq!(definition.display_name(), "Regolith");
        assert!(definition.has_tag("terrain"));
        assert_eq!(
            definition
                .get_attribute("hardness")
                .and_then(ContentValue::as_f64),
            Some(0.8)
        );
        assert_eq!(
            definition
                .get_attribute("dusty")
                .and_then(ContentValue::as_bool),
            Some(true)
        );
        assert_eq!(
            definition
                .get_attribute("drops")
                .and_then(ContentValue::as_text_list),
            Some(&["regolith".to_owned()][..])
        );
    }

    #[test]
    fn registry_rejects_duplicate_ids_and_duplicate_voxels() {
        let mut registry = BlockRegistry::<DefaultTerrainVoxel>::default();
        registry
            .register(BlockDefinition::new("stone", DefaultTerrainVoxel::Stone))
            .expect("first block should register");

        let duplicate_id =
            registry.register(BlockDefinition::new("stone", DefaultTerrainVoxel::Dirt));
        assert!(matches!(
            duplicate_id,
            Err(ContentRegistryError::DuplicateId { .. })
        ));

        let duplicate_voxel = registry.register(BlockDefinition::new(
            "another_stone",
            DefaultTerrainVoxel::Stone,
        ));
        assert!(matches!(
            duplicate_voxel,
            Err(ContentRegistryError::DuplicateVoxel { .. })
        ));
    }

    #[test]
    fn registry_can_lookup_by_id_and_voxel() {
        let mut registry = BlockRegistry::<DefaultTerrainVoxel>::default();
        registry
            .register(BlockDefinition::new("wood", DefaultTerrainVoxel::Wood).tag("organic"))
            .expect("block should register");

        assert_eq!(registry.len(), 1);
        assert!(registry.contains_id("wood"));
        assert!(
            registry
                .get_by_voxel(DefaultTerrainVoxel::Wood)
                .expect("wood voxel should resolve")
                .has_tag("organic")
        );
    }

    #[test]
    fn content_plugin_initializes_empty_registry_without_defaults() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(VoxelContentPlugin::<DefaultTerrainVoxel>::new());

        assert!(
            app.world()
                .resource::<BlockRegistry<DefaultTerrainVoxel>>()
                .is_empty(),
            "content plugin must not register game-specific default blocks"
        );
    }
}
