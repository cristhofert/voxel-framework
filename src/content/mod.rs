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

#[cfg(feature = "items")]
use crate::items::{DefaultItem, ItemType};
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

    /// Returns this value as a number when it stores an integer or float.
    pub fn as_number(&self) -> Option<f64> {
        match self {
            Self::Integer(value) => Some(*value as f64),
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

    /// Returns this value as a number list when it stores one.
    pub fn as_number_list(&self) -> Option<&[f64]> {
        match self {
            Self::NumberList(value) => Some(value.as_slice()),
            _ => None,
        }
    }

    /// Returns this value as an RGBA color when it stores one.
    pub fn as_color(&self) -> Option<[f32; 4]> {
        match self {
            Self::Color(value) => Some(*value),
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

    /// Returns a boolean gameplay attribute by key.
    pub fn bool_attribute(&self, key: &str) -> Option<bool> {
        self.get_attribute(key).and_then(ContentValue::as_bool)
    }

    /// Returns an integer gameplay attribute by key.
    pub fn integer_attribute(&self, key: &str) -> Option<i64> {
        self.get_attribute(key).and_then(ContentValue::as_i64)
    }

    /// Returns a numeric gameplay attribute by key.
    ///
    /// Both integer and float attributes are accepted.
    pub fn number_attribute(&self, key: &str) -> Option<f64> {
        self.get_attribute(key).and_then(ContentValue::as_number)
    }

    /// Returns a text gameplay attribute by key.
    pub fn text_attribute(&self, key: &str) -> Option<&str> {
        self.get_attribute(key).and_then(ContentValue::as_str)
    }

    /// Returns a text-list gameplay attribute by key.
    pub fn text_list_attribute(&self, key: &str) -> Option<&[String]> {
        self.get_attribute(key).and_then(ContentValue::as_text_list)
    }

    /// Returns a number-list gameplay attribute by key.
    pub fn number_list_attribute(&self, key: &str) -> Option<&[f64]> {
        self.get_attribute(key)
            .and_then(ContentValue::as_number_list)
    }

    /// Returns a color gameplay attribute by key.
    pub fn color_attribute(&self, key: &str) -> Option<[f32; 4]> {
        self.get_attribute(key).and_then(ContentValue::as_color)
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
    /// No block exists for the requested id.
    MissingId { id: ContentId },
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
            Self::MissingId { id } => {
                write!(f, "content id `{id}` is not registered")
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

    /// Returns a definition by id or an explicit error.
    pub fn require(
        &self,
        id: impl AsRef<str>,
    ) -> Result<&BlockDefinition<V>, ContentRegistryError<V>> {
        let id = ContentId::new(id.as_ref());
        self.blocks_by_id
            .get(&id)
            .ok_or(ContentRegistryError::MissingId { id })
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

// ─── Item content ────────────────────────────────────────────────────────────

/// Author-defined item content associated with a concrete item value.
///
/// Available when the `items` feature is enabled. Like blocks, item
/// definitions are data: the framework stores identity, display information,
/// tags, and arbitrary attributes without imposing game-specific semantics.
#[cfg(feature = "items")]
#[derive(Debug, Clone, PartialEq)]
pub struct ItemDefinition<I: ItemType> {
    id: ContentId,
    item: I,
    display_name: String,
    color: [f32; 4],
    attributes: HashMap<String, ContentValue>,
    tags: HashSet<String>,
}

#[cfg(feature = "items")]
impl<I: ItemType> ItemDefinition<I> {
    /// Creates an item definition for the given id and item value.
    pub fn new(id: impl Into<ContentId>, item: I) -> Self {
        Self {
            id: id.into(),
            item,
            display_name: item.name().to_owned(),
            color: item.color(),
            attributes: HashMap::new(),
            tags: HashSet::new(),
        }
    }

    /// Returns the stable author-defined id.
    pub fn id(&self) -> &ContentId {
        &self.id
    }

    /// Returns the item value this definition describes.
    pub fn item(&self) -> I {
        self.item
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

    /// Returns the fallback display color.
    pub fn color(&self) -> [f32; 4] {
        self.color
    }

    /// Replaces the fallback display color.
    pub fn color_override(mut self, color: [f32; 4]) -> Self {
        self.color = color;
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

    /// Returns a boolean gameplay attribute by key.
    pub fn bool_attribute(&self, key: &str) -> Option<bool> {
        self.get_attribute(key).and_then(ContentValue::as_bool)
    }

    /// Returns an integer gameplay attribute by key.
    pub fn integer_attribute(&self, key: &str) -> Option<i64> {
        self.get_attribute(key).and_then(ContentValue::as_i64)
    }

    /// Returns a numeric gameplay attribute by key.
    ///
    /// Both integer and float attributes are accepted.
    pub fn number_attribute(&self, key: &str) -> Option<f64> {
        self.get_attribute(key).and_then(ContentValue::as_number)
    }

    /// Returns a text gameplay attribute by key.
    pub fn text_attribute(&self, key: &str) -> Option<&str> {
        self.get_attribute(key).and_then(ContentValue::as_str)
    }

    /// Returns a text-list gameplay attribute by key.
    pub fn text_list_attribute(&self, key: &str) -> Option<&[String]> {
        self.get_attribute(key).and_then(ContentValue::as_text_list)
    }

    /// Returns a number-list gameplay attribute by key.
    pub fn number_list_attribute(&self, key: &str) -> Option<&[f64]> {
        self.get_attribute(key)
            .and_then(ContentValue::as_number_list)
    }

    /// Returns a color gameplay attribute by key.
    pub fn color_attribute(&self, key: &str) -> Option<[f32; 4]> {
        self.get_attribute(key).and_then(ContentValue::as_color)
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

/// Errors returned by [`ItemRegistry::register`].
#[cfg(feature = "items")]
#[derive(Debug, Clone, PartialEq)]
pub enum ItemRegistryError<I: ItemType> {
    /// An item with the same id already exists.
    DuplicateId { id: ContentId },
    /// The item value is already associated with a different item id.
    DuplicateItem { item: I, existing_id: ContentId },
    /// No item exists for the requested id.
    MissingId { id: ContentId },
}

#[cfg(feature = "items")]
impl<I: ItemType + fmt::Debug> fmt::Display for ItemRegistryError<I> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateId { id } => {
                write!(f, "content id `{id}` is already registered")
            }
            Self::DuplicateItem { item, existing_id } => {
                write!(
                    f,
                    "item value `{item:?}` is already registered as `{existing_id}`"
                )
            }
            Self::MissingId { id } => {
                write!(f, "content id `{id}` is not registered")
            }
        }
    }
}

#[cfg(feature = "items")]
impl<I: ItemType + fmt::Debug> std::error::Error for ItemRegistryError<I> {}

/// Registry of author-defined item content.
#[cfg(feature = "items")]
#[derive(Debug, Clone, Resource)]
pub struct ItemRegistry<I: ItemType> {
    items_by_id: HashMap<ContentId, ItemDefinition<I>>,
    ids_by_item: HashMap<I, ContentId>,
}

#[cfg(feature = "items")]
impl<I: ItemType> Default for ItemRegistry<I> {
    fn default() -> Self {
        Self {
            items_by_id: HashMap::new(),
            ids_by_item: HashMap::new(),
        }
    }
}

#[cfg(feature = "items")]
impl<I: ItemType> ItemRegistry<I> {
    /// Registers a new item definition.
    pub fn register(&mut self, definition: ItemDefinition<I>) -> Result<(), ItemRegistryError<I>> {
        if self.items_by_id.contains_key(definition.id()) {
            return Err(ItemRegistryError::DuplicateId {
                id: definition.id().clone(),
            });
        }

        if let Some(existing_id) = self.ids_by_item.get(&definition.item()) {
            return Err(ItemRegistryError::DuplicateItem {
                item: definition.item(),
                existing_id: existing_id.clone(),
            });
        }

        self.ids_by_item
            .insert(definition.item(), definition.id().clone());
        self.items_by_id.insert(definition.id().clone(), definition);
        Ok(())
    }

    /// Inserts a definition, replacing any previous definition for the same id
    /// or item value.
    pub fn insert_or_replace(&mut self, definition: ItemDefinition<I>) {
        if let Some(old) = self.items_by_id.remove(definition.id()) {
            self.ids_by_item.remove(&old.item());
        }
        if let Some(old_id) = self.ids_by_item.remove(&definition.item()) {
            self.items_by_id.remove(&old_id);
        }

        self.ids_by_item
            .insert(definition.item(), definition.id().clone());
        self.items_by_id.insert(definition.id().clone(), definition);
    }

    /// Returns a definition by content id.
    pub fn get(&self, id: impl AsRef<str>) -> Option<&ItemDefinition<I>> {
        let id = ContentId::new(id.as_ref());
        self.items_by_id.get(&id)
    }

    /// Returns a definition by id or an explicit error.
    pub fn require(&self, id: impl AsRef<str>) -> Result<&ItemDefinition<I>, ItemRegistryError<I>> {
        let id = ContentId::new(id.as_ref());
        self.items_by_id
            .get(&id)
            .ok_or(ItemRegistryError::MissingId { id })
    }

    /// Returns a definition by item value.
    pub fn get_by_item(&self, item: I) -> Option<&ItemDefinition<I>> {
        self.ids_by_item
            .get(&item)
            .and_then(|id| self.items_by_id.get(id))
    }

    /// Returns true if an id exists.
    pub fn contains_id(&self, id: impl AsRef<str>) -> bool {
        self.get(id).is_some()
    }

    /// Number of registered item definitions.
    pub fn len(&self) -> usize {
        self.items_by_id.len()
    }

    /// Returns true when no definitions are registered.
    pub fn is_empty(&self) -> bool {
        self.items_by_id.is_empty()
    }

    /// Iterates definitions in unspecified hash-map order.
    pub fn iter(&self) -> impl Iterator<Item = &ItemDefinition<I>> {
        self.items_by_id.values()
    }
}

/// Horizontal plugin that initializes author-defined item content.
///
/// This is separate from [`VoxelContentPlugin`] so item content can remain
/// generic over `I: ItemType` without turning the block registry into a
/// convenience bundle.
#[cfg(feature = "items")]
pub struct ItemContentPlugin<I: ItemType = DefaultItem> {
    _marker: PhantomData<I>,
}

#[cfg(feature = "items")]
impl<I: ItemType> ItemContentPlugin<I> {
    /// Creates a new item-content plugin for item type `I`.
    pub fn new() -> Self {
        Self {
            _marker: PhantomData,
        }
    }
}

#[cfg(feature = "items")]
impl<I: ItemType> Default for ItemContentPlugin<I> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "items")]
impl<I: ItemType> Plugin for ItemContentPlugin<I> {
    fn build(&self, app: &mut App) {
        app.init_resource::<ItemRegistry<I>>();
    }
}

// ─── Drop content ────────────────────────────────────────────────────────────

/// Count range for an author-defined block drop.
#[cfg(feature = "items")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DropCount {
    /// Minimum count, inclusive.
    pub min: u32,
    /// Maximum count, inclusive.
    pub max: u32,
}

#[cfg(feature = "items")]
impl DropCount {
    /// Creates a fixed-count drop.
    pub fn fixed(count: u32) -> Self {
        Self {
            min: count,
            max: count,
        }
    }

    /// Creates a drop range.
    ///
    /// The values are normalized so `range(3, 1)` becomes `1..=3` instead of
    /// panicking in game setup code.
    pub fn range(min: u32, max: u32) -> Self {
        Self {
            min: min.min(max),
            max: min.max(max),
        }
    }
}

/// Author-defined item drop attached to a block definition.
#[cfg(feature = "items")]
#[derive(Debug, Clone, PartialEq)]
pub struct BlockDropDefinition<I: ItemType> {
    item: I,
    count: DropCount,
    chance: f32,
    attributes: HashMap<String, ContentValue>,
}

#[cfg(feature = "items")]
impl<I: ItemType> BlockDropDefinition<I> {
    /// Creates a fixed-count guaranteed drop for an item.
    pub fn fixed(item: I, count: u32) -> Self {
        Self::new(item, DropCount::fixed(count))
    }

    /// Creates a guaranteed drop with the provided count range.
    pub fn new(item: I, count: DropCount) -> Self {
        Self {
            item,
            count,
            chance: 1.0,
            attributes: HashMap::new(),
        }
    }

    /// Returns the item value this drop produces.
    pub fn item(&self) -> I {
        self.item
    }

    /// Returns the count range.
    pub fn count(&self) -> DropCount {
        self.count
    }

    /// Returns the chance in `0.0..=1.0`.
    pub fn chance(&self) -> f32 {
        self.chance
    }

    /// Sets the chance in `0.0..=1.0`.
    pub fn with_chance(mut self, chance: f32) -> Self {
        self.chance = chance.clamp(0.0, 1.0);
        self
    }

    /// Adds or replaces an author-defined gameplay attribute.
    pub fn attribute(mut self, key: impl Into<String>, value: impl Into<ContentValue>) -> Self {
        self.attributes.insert(key.into(), value.into());
        self
    }

    /// Returns a gameplay attribute by key.
    pub fn get_attribute(&self, key: &str) -> Option<&ContentValue> {
        self.attributes.get(key)
    }

    /// Returns a boolean gameplay attribute by key.
    pub fn bool_attribute(&self, key: &str) -> Option<bool> {
        self.get_attribute(key).and_then(ContentValue::as_bool)
    }

    /// Returns an integer gameplay attribute by key.
    pub fn integer_attribute(&self, key: &str) -> Option<i64> {
        self.get_attribute(key).and_then(ContentValue::as_i64)
    }

    /// Returns a numeric gameplay attribute by key.
    ///
    /// Both integer and float attributes are accepted.
    pub fn number_attribute(&self, key: &str) -> Option<f64> {
        self.get_attribute(key).and_then(ContentValue::as_number)
    }

    /// Returns a text gameplay attribute by key.
    pub fn text_attribute(&self, key: &str) -> Option<&str> {
        self.get_attribute(key).and_then(ContentValue::as_str)
    }

    /// Returns a text-list gameplay attribute by key.
    pub fn text_list_attribute(&self, key: &str) -> Option<&[String]> {
        self.get_attribute(key).and_then(ContentValue::as_text_list)
    }

    /// Returns a number-list gameplay attribute by key.
    pub fn number_list_attribute(&self, key: &str) -> Option<&[f64]> {
        self.get_attribute(key)
            .and_then(ContentValue::as_number_list)
    }

    /// Returns a color gameplay attribute by key.
    pub fn color_attribute(&self, key: &str) -> Option<[f32; 4]> {
        self.get_attribute(key).and_then(ContentValue::as_color)
    }

    /// Returns all gameplay attributes.
    pub fn attributes(&self) -> &HashMap<String, ContentValue> {
        &self.attributes
    }
}

/// Errors returned by block-drop registry validation helpers.
#[cfg(feature = "items")]
#[derive(Debug, Clone, PartialEq)]
pub enum BlockDropRegistryError<I: ItemType> {
    /// A drop is attached to a block id that is not registered.
    UnknownBlockId { id: ContentId },
    /// A drop produces an item value that is not registered.
    UnknownItem { block_id: ContentId, item: I },
}

#[cfg(feature = "items")]
impl<I: ItemType + fmt::Debug> fmt::Display for BlockDropRegistryError<I> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownBlockId { id } => {
                write!(f, "drop table references unknown block id `{id}`")
            }
            Self::UnknownItem { block_id, item } => {
                write!(
                    f,
                    "drop table for block `{block_id}` references unknown item `{item:?}`"
                )
            }
        }
    }
}

#[cfg(feature = "items")]
impl<I: ItemType + fmt::Debug> std::error::Error for BlockDropRegistryError<I> {}

/// Registry that associates block content ids with author-defined item drops.
#[cfg(feature = "items")]
#[derive(Debug, Clone, Resource)]
pub struct BlockDropRegistry<I: ItemType> {
    drops_by_block_id: HashMap<ContentId, Vec<BlockDropDefinition<I>>>,
}

#[cfg(feature = "items")]
impl<I: ItemType> Default for BlockDropRegistry<I> {
    fn default() -> Self {
        Self {
            drops_by_block_id: HashMap::new(),
        }
    }
}

#[cfg(feature = "items")]
impl<I: ItemType> BlockDropRegistry<I> {
    /// Adds one drop definition for a block id.
    pub fn register_drop(&mut self, block_id: impl Into<ContentId>, drop: BlockDropDefinition<I>) {
        self.drops_by_block_id
            .entry(block_id.into())
            .or_default()
            .push(drop);
    }

    /// Returns all drops registered for a block id.
    pub fn get(&self, block_id: impl AsRef<str>) -> &[BlockDropDefinition<I>] {
        let id = ContentId::new(block_id.as_ref());
        self.drops_by_block_id
            .get(&id)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// Returns true when at least one drop exists for a block id.
    pub fn contains_block_id(&self, block_id: impl AsRef<str>) -> bool {
        let id = ContentId::new(block_id.as_ref());
        self.drops_by_block_id.contains_key(&id)
    }

    /// Returns the number of drops registered for one block id.
    pub fn len_for(&self, block_id: impl AsRef<str>) -> usize {
        self.get(block_id).len()
    }

    /// Iterates block ids and their drop lists in unspecified hash-map order.
    pub fn iter(&self) -> impl Iterator<Item = (&ContentId, &[BlockDropDefinition<I>])> {
        self.drops_by_block_id
            .iter()
            .map(|(id, drops)| (id, drops.as_slice()))
    }

    /// Returns all drops registered for a voxel value by first resolving it
    /// through a [`BlockRegistry`].
    pub fn get_for_voxel<V: VoxelType>(
        &self,
        voxel: V,
        blocks: &BlockRegistry<V>,
    ) -> &[BlockDropDefinition<I>] {
        let Some(block) = blocks.get_by_voxel(voxel) else {
            return &[];
        };
        self.get(block.id().as_str())
    }

    /// Validates that every drop references a registered block id.
    pub fn validate_blocks<V: VoxelType>(
        &self,
        blocks: &BlockRegistry<V>,
    ) -> Result<(), BlockDropRegistryError<I>> {
        for block_id in self.drops_by_block_id.keys() {
            if !blocks.contains_id(block_id.as_str()) {
                return Err(BlockDropRegistryError::UnknownBlockId {
                    id: block_id.clone(),
                });
            }
        }
        Ok(())
    }

    /// Validates that every drop produces a registered item value.
    pub fn validate_items(&self, items: &ItemRegistry<I>) -> Result<(), BlockDropRegistryError<I>> {
        for (block_id, drops) in &self.drops_by_block_id {
            for drop in drops {
                if items.get_by_item(drop.item()).is_none() {
                    return Err(BlockDropRegistryError::UnknownItem {
                        block_id: block_id.clone(),
                        item: drop.item(),
                    });
                }
            }
        }
        Ok(())
    }

    /// Number of block ids with at least one registered drop.
    pub fn len(&self) -> usize {
        self.drops_by_block_id.len()
    }

    /// Returns true when no block drops are registered.
    pub fn is_empty(&self) -> bool {
        self.drops_by_block_id.is_empty()
    }
}

/// Horizontal plugin that initializes author-defined block-drop content.
#[cfg(feature = "items")]
pub struct BlockDropContentPlugin<I: ItemType = DefaultItem> {
    _marker: PhantomData<I>,
}

#[cfg(feature = "items")]
impl<I: ItemType> BlockDropContentPlugin<I> {
    /// Creates a new block-drop content plugin for item type `I`.
    pub fn new() -> Self {
        Self {
            _marker: PhantomData,
        }
    }
}

#[cfg(feature = "items")]
impl<I: ItemType> Default for BlockDropContentPlugin<I> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "items")]
impl<I: ItemType> Plugin for BlockDropContentPlugin<I> {
    fn build(&self, app: &mut App) {
        app.init_resource::<BlockDropRegistry<I>>();
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
    #[cfg(feature = "items")]
    use crate::items::DefaultItem;
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
        assert_eq!(definition.number_attribute("hardness"), Some(0.8));
        assert_eq!(definition.bool_attribute("dusty"), Some(true));
        assert_eq!(
            definition.text_list_attribute("drops"),
            Some(&["regolith".to_owned()][..])
        );
    }

    #[test]
    fn content_value_number_accepts_integers_and_floats() {
        assert_eq!(ContentValue::from(5).as_number(), Some(5.0));
        assert_eq!(ContentValue::from(0.75).as_number(), Some(0.75));
        assert_eq!(ContentValue::from("not_a_number").as_number(), None);
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
        assert_eq!(
            registry.require("wood").expect("wood should exist").voxel(),
            DefaultTerrainVoxel::Wood
        );
        assert!(matches!(
            registry.require("missing"),
            Err(ContentRegistryError::MissingId { .. })
        ));
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

    #[cfg(feature = "items")]
    #[test]
    fn item_definition_keeps_author_defined_attributes_and_tags() {
        let definition = ItemDefinition::new("dream_glass_shard", DefaultItem::Crystal)
            .named("Dream Glass Shard")
            .color_override([0.45, 0.75, 1.0, 1.0])
            .tag("ritual_material")
            .attribute("mana_conductivity", 12.0);

        assert_eq!(definition.id().as_str(), "dream_glass_shard");
        assert_eq!(definition.item(), DefaultItem::Crystal);
        assert_eq!(definition.display_name(), "Dream Glass Shard");
        assert_eq!(definition.color(), [0.45, 0.75, 1.0, 1.0]);
        assert!(definition.has_tag("ritual_material"));
        assert_eq!(definition.number_attribute("mana_conductivity"), Some(12.0));
    }

    #[cfg(feature = "items")]
    #[test]
    fn item_registry_rejects_duplicate_ids_and_duplicate_items() {
        let mut registry = ItemRegistry::<DefaultItem>::default();
        registry
            .register(ItemDefinition::new("sand_item", DefaultItem::Sand))
            .expect("first item should register");

        let duplicate_id =
            registry.register(ItemDefinition::new("sand_item", DefaultItem::Crystal));
        assert!(matches!(
            duplicate_id,
            Err(ItemRegistryError::DuplicateId { .. })
        ));

        let duplicate_item =
            registry.register(ItemDefinition::new("another_sand", DefaultItem::Sand));
        assert!(matches!(
            duplicate_item,
            Err(ItemRegistryError::DuplicateItem { .. })
        ));

        assert!(registry.require("sand_item").is_ok());
        assert!(matches!(
            registry.require("missing"),
            Err(ItemRegistryError::MissingId { .. })
        ));
    }

    #[cfg(feature = "items")]
    #[test]
    fn block_drop_registry_resolves_drops_by_block_id_and_voxel() {
        let mut blocks = BlockRegistry::<DefaultTerrainVoxel>::default();
        blocks
            .register(BlockDefinition::new(
                "moon_regolith",
                DefaultTerrainVoxel::Sand,
            ))
            .expect("block should register");

        let mut drops = BlockDropRegistry::<DefaultItem>::default();
        drops.register_drop(
            "moon_regolith",
            BlockDropDefinition::new(DefaultItem::Sand, DropCount::range(3, 1)).with_chance(1.4),
        );

        let by_id = drops.get("moon_regolith");
        assert_eq!(by_id.len(), 1);
        assert_eq!(by_id[0].item(), DefaultItem::Sand);
        assert_eq!(by_id[0].count(), DropCount { min: 1, max: 3 });
        assert_eq!(by_id[0].chance(), 1.0);
        assert!(drops.contains_block_id("moon_regolith"));
        assert_eq!(drops.len_for("moon_regolith"), 1);

        let by_voxel = drops.get_for_voxel(DefaultTerrainVoxel::Sand, &blocks);
        assert_eq!(by_voxel.len(), 1);
        assert_eq!(by_voxel[0].item(), DefaultItem::Sand);
        assert!(
            drops
                .get_for_voxel(DefaultTerrainVoxel::Stone, &blocks)
                .is_empty()
        );
    }

    #[cfg(feature = "items")]
    #[test]
    fn block_drop_registry_validates_block_and_item_references() {
        let mut blocks = BlockRegistry::<DefaultTerrainVoxel>::default();
        blocks
            .register(BlockDefinition::new(
                "moon_regolith",
                DefaultTerrainVoxel::Sand,
            ))
            .expect("block should register");

        let mut items = ItemRegistry::<DefaultItem>::default();
        items
            .register(ItemDefinition::new("moon_regolith", DefaultItem::Sand))
            .expect("item should register");

        let mut drops = BlockDropRegistry::<DefaultItem>::default();
        drops.register_drop(
            "moon_regolith",
            BlockDropDefinition::fixed(DefaultItem::Sand, 1),
        );

        assert!(drops.validate_blocks(&blocks).is_ok());
        assert!(drops.validate_items(&items).is_ok());

        drops.register_drop(
            "missing_block",
            BlockDropDefinition::fixed(DefaultItem::Sand, 1),
        );
        assert!(matches!(
            drops.validate_blocks(&blocks),
            Err(BlockDropRegistryError::UnknownBlockId { .. })
        ));

        drops.register_drop(
            "moon_regolith",
            BlockDropDefinition::fixed(DefaultItem::Crystal, 1),
        );
        assert!(matches!(
            drops.validate_items(&items),
            Err(BlockDropRegistryError::UnknownItem { .. })
        ));
    }

    #[cfg(feature = "items")]
    #[test]
    fn item_and_drop_plugins_initialize_empty_registries_without_defaults() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(ItemContentPlugin::<DefaultItem>::new());
        app.add_plugins(BlockDropContentPlugin::<DefaultItem>::new());

        assert!(
            app.world()
                .resource::<ItemRegistry<DefaultItem>>()
                .is_empty(),
            "item content plugin must not register game-specific default items"
        );
        assert!(
            app.world()
                .resource::<BlockDropRegistry<DefaultItem>>()
                .is_empty(),
            "block-drop content plugin must not register game-specific default drops"
        );
    }
}
