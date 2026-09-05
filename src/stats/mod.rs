//! Author-defined stat registries and runtime stat values.
//!
//! `stats` is the second gameplay-definition layer in `voxel-framework`.
//! Like [`crate::content`], it does **not** ship a survival model, fixed body
//! schema, or predefined list of stats. The game author decides whether a game
//! has health, oxygen, stamina, mana, radiation, thirst, ship hull integrity, or
//! no player stats at all.
//!
//! The framework provides three horizontal pieces:
//!
//! - [`StatRegistry`] — author-defined metadata and bounds for stats.
//! - [`StatSet`] — runtime values keyed by [`StatId`], with clamping and simple
//!   mutation and inspection helpers.
//! - [`StatChanged`] plus [`set_stat`], [`add_stat`], [`drain_stat`], and
//!   [`fill_stat`] — optional Bevy-message helpers for systems that want to
//!   react when a stat actually changes.
//!
//! # Example
//!
//! ```rust
//! use bevy::prelude::*;
//! use bevy::ecs::message::MessageWriter;
//! use voxel_framework::stats::{
//!     drain_stat, StatChanged, StatDefinition, StatRegistry, StatSet, VoxelStatsPlugin,
//! };
//!
//! fn register_stats(mut stats: ResMut<StatRegistry>) {
//!     stats
//!         .register(
//!             StatDefinition::new("oxygen", 100.0)
//!                 .named("Suit Oxygen")
//!                 .bounds(0.0, 100.0)
//!                 .tag("survival")
//!                 .attribute("ui_hint", "bar"),
//!         )
//!         .expect("stat id should be unique");
//! }
//!
//! fn drain_oxygen(
//!     registry: Res<StatRegistry>,
//!     mut values: ResMut<StatSet>,
//!     mut changed: MessageWriter<StatChanged>,
//! ) {
//!     values.ensure_registered(&registry);
//!     drain_stat("oxygen", 1.0, &registry, &mut values, &mut changed)
//!         .expect("oxygen should exist");
//! }
//!
//! App::new()
//!     .add_plugins(MinimalPlugins)
//!     .add_plugins(VoxelStatsPlugin)
//!     .add_systems(Startup, register_stats)
//!     .add_systems(Update, drain_oxygen);
//! ```

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;

use bevy::ecs::message::MessageWriter;
use bevy::prelude::{App, Component, Message, Plugin, Resource};
use serde::{Deserialize, Serialize};

use crate::content::ContentValue;

/// Stable identifier for an author-defined stat.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StatId(String);

impl StatId {
    /// Creates a new stat id from any string-like value.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Returns the raw id string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for StatId {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for StatId {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl AsRef<str> for StatId {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for StatId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Author-defined stat metadata and bounds.
#[derive(Debug, Clone, PartialEq)]
pub struct StatDefinition {
    id: StatId,
    display_name: String,
    default_value: f32,
    min_value: f32,
    max_value: f32,
    attributes: HashMap<String, ContentValue>,
    tags: HashSet<String>,
}

/// Serializable snapshot of one author-defined stat definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StatDefinitionSnapshot {
    /// Stable author-defined id.
    pub id: StatId,
    /// Display name intended for UI/docs.
    pub display_name: String,
    /// Value inserted by [`StatSet::ensure_registered`] when the stat is absent.
    pub default_value: f32,
    /// Minimum allowed runtime value.
    pub min_value: f32,
    /// Maximum allowed runtime value.
    pub max_value: f32,
    /// Author-defined attributes in deterministic key order.
    pub attributes: BTreeMap<String, ContentValue>,
    /// Author-defined tags in deterministic lexical order.
    pub tags: Vec<String>,
}

impl StatDefinition {
    /// Creates a stat definition with default bounds `0.0..=default_value`.
    pub fn new(id: impl Into<StatId>, default_value: f32) -> Self {
        let default_value = finite_or_zero(default_value);
        Self {
            id: id.into(),
            display_name: String::new(),
            default_value,
            min_value: 0.0_f32.min(default_value),
            max_value: 0.0_f32.max(default_value),
            attributes: HashMap::new(),
            tags: HashSet::new(),
        }
    }

    /// Returns the stable author-defined id.
    pub fn id(&self) -> &StatId {
        &self.id
    }

    /// Returns the display name intended for UI/docs.
    pub fn display_name(&self) -> &str {
        if self.display_name.is_empty() {
            self.id.as_str()
        } else {
            &self.display_name
        }
    }

    /// Replaces the display name.
    pub fn named(mut self, name: impl Into<String>) -> Self {
        self.display_name = name.into();
        self
    }

    /// Returns the default runtime value.
    pub fn default_value(&self) -> f32 {
        self.default_value
    }

    /// Returns the minimum runtime value.
    pub fn min_value(&self) -> f32 {
        self.min_value
    }

    /// Returns the maximum runtime value.
    pub fn max_value(&self) -> f32 {
        self.max_value
    }

    /// Returns the absolute span between the minimum and maximum runtime values.
    ///
    /// A stat with equal bounds has a span of `0.0`.
    pub fn span(&self) -> f32 {
        self.max_value - self.min_value
    }

    /// Sets runtime bounds. Reversed bounds are normalized.
    pub fn bounds(mut self, min_value: f32, max_value: f32) -> Self {
        let min_value = finite_or_zero(min_value);
        let max_value = finite_or_zero(max_value);
        self.min_value = min_value.min(max_value);
        self.max_value = min_value.max(max_value);
        self.default_value = self.clamp(self.default_value);
        self
    }

    /// Sets a new default runtime value, clamped into the current bounds.
    pub fn initial_value(mut self, value: f32) -> Self {
        self.default_value = self.clamp(finite_or_zero(value));
        self
    }

    /// Clamps a runtime value to this stat's bounds.
    pub fn clamp(&self, value: f32) -> f32 {
        finite_or_zero(value).clamp(self.min_value, self.max_value)
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

    /// Returns a numeric gameplay attribute by key.
    pub fn number_attribute(&self, key: &str) -> Option<f64> {
        self.get_attribute(key).and_then(ContentValue::as_number)
    }

    /// Returns a boolean gameplay attribute by key.
    pub fn bool_attribute(&self, key: &str) -> Option<bool> {
        self.get_attribute(key).and_then(ContentValue::as_bool)
    }

    /// Returns a text gameplay attribute by key.
    pub fn text_attribute(&self, key: &str) -> Option<&str> {
        self.get_attribute(key).and_then(ContentValue::as_str)
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

    /// Creates a serializable, deterministic snapshot of this stat definition.
    pub fn snapshot(&self) -> StatDefinitionSnapshot {
        StatDefinitionSnapshot {
            id: self.id.clone(),
            display_name: self.display_name().to_owned(),
            default_value: self.default_value,
            min_value: self.min_value,
            max_value: self.max_value,
            attributes: ordered_attributes(&self.attributes),
            tags: ordered_tags(&self.tags),
        }
    }
}

/// Errors returned by stat registries and runtime stat sets.
#[derive(Debug, Clone, PartialEq)]
pub enum StatError {
    /// A stat with the same id already exists.
    DuplicateId { id: StatId },
    /// No stat exists for the requested id.
    MissingId { id: StatId },
}

impl fmt::Display for StatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateId { id } => write!(f, "stat id `{id}` is already registered"),
            Self::MissingId { id } => write!(f, "stat id `{id}` is not registered"),
        }
    }
}

impl std::error::Error for StatError {}

/// Registry of author-defined stat definitions.
#[derive(Debug, Clone, Default, Resource)]
pub struct StatRegistry {
    stats_by_id: HashMap<StatId, StatDefinition>,
}

impl StatRegistry {
    /// Registers a new stat definition.
    pub fn register(&mut self, definition: StatDefinition) -> Result<(), StatError> {
        if self.stats_by_id.contains_key(definition.id()) {
            return Err(StatError::DuplicateId {
                id: definition.id().clone(),
            });
        }

        self.stats_by_id.insert(definition.id().clone(), definition);
        Ok(())
    }

    /// Inserts a definition, replacing any previous definition for the same id.
    pub fn insert_or_replace(&mut self, definition: StatDefinition) {
        self.stats_by_id.insert(definition.id().clone(), definition);
    }

    /// Returns a definition by stat id.
    pub fn get(&self, id: impl AsRef<str>) -> Option<&StatDefinition> {
        let id = StatId::new(id.as_ref());
        self.stats_by_id.get(&id)
    }

    /// Returns a definition by id or an explicit error.
    pub fn require(&self, id: impl AsRef<str>) -> Result<&StatDefinition, StatError> {
        let id = StatId::new(id.as_ref());
        self.stats_by_id.get(&id).ok_or(StatError::MissingId { id })
    }

    /// Returns true if an id exists.
    pub fn contains_id(&self, id: impl AsRef<str>) -> bool {
        self.get(id).is_some()
    }

    /// Number of registered stat definitions.
    pub fn len(&self) -> usize {
        self.stats_by_id.len()
    }

    /// Returns true when no stat definitions are registered.
    pub fn is_empty(&self) -> bool {
        self.stats_by_id.is_empty()
    }

    /// Iterates definitions in unspecified hash-map order.
    pub fn iter(&self) -> impl Iterator<Item = &StatDefinition> {
        self.stats_by_id.values()
    }

    /// Returns deterministic serializable snapshots of all registered stats.
    pub fn snapshot(&self) -> Vec<StatDefinitionSnapshot> {
        let mut stats: Vec<_> = self.iter().map(StatDefinition::snapshot).collect();
        stats.sort_by(|left, right| left.id.as_str().cmp(right.id.as_str()));
        stats
    }
}

/// Runtime stat values keyed by author-defined [`StatId`].
///
/// A `StatSet` can be used as the global stats [`Resource`] initialized by
/// [`VoxelStatsPlugin`] or attached directly to entities as a [`Component`].
/// The framework does not assign a meaning to the owner entity: it can be a
/// player, mob, vehicle, machine, colony, or any author-defined subject.
#[derive(Debug, Clone, Default, Component, Resource, Serialize, Deserialize)]
pub struct StatSet {
    values: HashMap<StatId, f32>,
}

impl StatSet {
    /// Creates an empty stat set.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a stat set initialized with every registered stat default.
    pub fn from_registry(registry: &StatRegistry) -> Self {
        let mut values = Self::new();
        values.ensure_registered(registry);
        values
    }

    /// Inserts any missing registered stats with their configured defaults.
    pub fn ensure_registered(&mut self, registry: &StatRegistry) {
        for definition in registry.iter() {
            self.values
                .entry(definition.id().clone())
                .or_insert_with(|| definition.default_value());
        }
    }

    /// Sets a stat value after resolving and clamping it through the registry.
    pub fn set(
        &mut self,
        id: impl AsRef<str>,
        value: f32,
        registry: &StatRegistry,
    ) -> Result<f32, StatError> {
        let definition = registry.require(id.as_ref())?;
        let value = definition.clamp(value);
        self.values.insert(definition.id().clone(), value);
        Ok(value)
    }

    /// Adds a delta to a stat and returns the clamped value.
    pub fn add(
        &mut self,
        id: impl AsRef<str>,
        delta: f32,
        registry: &StatRegistry,
    ) -> Result<f32, StatError> {
        let definition = registry.require(id.as_ref())?;
        let current = self.get(id.as_ref(), registry)?;
        self.set(
            definition.id().as_str(),
            current + finite_or_zero(delta),
            registry,
        )
    }

    /// Removes from a stat and returns the clamped value.
    pub fn drain(
        &mut self,
        id: impl AsRef<str>,
        amount: f32,
        registry: &StatRegistry,
    ) -> Result<f32, StatError> {
        self.add(id, -finite_or_zero(amount).abs(), registry)
    }

    /// Adds to a stat and returns the clamped value.
    pub fn fill(
        &mut self,
        id: impl AsRef<str>,
        amount: f32,
        registry: &StatRegistry,
    ) -> Result<f32, StatError> {
        self.add(id, finite_or_zero(amount).abs(), registry)
    }

    /// Returns the current stat value, falling back to the registered default
    /// when the stat is not yet stored in this set.
    pub fn get(&self, id: impl AsRef<str>, registry: &StatRegistry) -> Result<f32, StatError> {
        let definition = registry.require(id.as_ref())?;
        Ok(self
            .values
            .get(definition.id())
            .copied()
            .unwrap_or_else(|| definition.default_value()))
    }

    /// Returns the current value normalized to `0.0..=1.0` across the
    /// registered stat bounds.
    ///
    /// This is only an inspection helper: it does not imply HUD, rules, or
    /// effects behavior. A stat whose minimum and maximum are equal normalizes
    /// to `1.0` to avoid division by zero.
    pub fn normalized(
        &self,
        id: impl AsRef<str>,
        registry: &StatRegistry,
    ) -> Result<f32, StatError> {
        let definition = registry.require(id.as_ref())?;
        let value = definition.clamp(self.get(definition.id().as_str(), registry)?);
        Ok(normalize_value(definition, value))
    }

    /// Returns true when the current value is at or below an absolute threshold.
    pub fn is_at_or_below(
        &self,
        id: impl AsRef<str>,
        threshold: f32,
        registry: &StatRegistry,
    ) -> Result<bool, StatError> {
        let definition = registry.require(id.as_ref())?;
        Ok(self.get(definition.id().as_str(), registry)? <= finite_or_zero(threshold))
    }

    /// Returns true when the current value is at or above an absolute threshold.
    pub fn is_at_or_above(
        &self,
        id: impl AsRef<str>,
        threshold: f32,
        registry: &StatRegistry,
    ) -> Result<bool, StatError> {
        let definition = registry.require(id.as_ref())?;
        Ok(self.get(definition.id().as_str(), registry)? >= finite_or_zero(threshold))
    }

    /// Returns true when the normalized value is at or below `threshold`.
    ///
    /// `threshold` is clamped to `0.0..=1.0`; for example, `0.15` means 15%.
    pub fn is_at_or_below_percent(
        &self,
        id: impl AsRef<str>,
        threshold: f32,
        registry: &StatRegistry,
    ) -> Result<bool, StatError> {
        Ok(self.normalized(id, registry)? <= normalized_threshold(threshold))
    }

    /// Returns true when the normalized value is at or above `threshold`.
    ///
    /// `threshold` is clamped to `0.0..=1.0`; for example, `0.85` means 85%.
    pub fn is_at_or_above_percent(
        &self,
        id: impl AsRef<str>,
        threshold: f32,
        registry: &StatRegistry,
    ) -> Result<bool, StatError> {
        Ok(self.normalized(id, registry)? >= normalized_threshold(threshold))
    }

    /// Returns true when this stat value is at or below its minimum bound.
    pub fn is_min(&self, id: impl AsRef<str>, registry: &StatRegistry) -> Result<bool, StatError> {
        let definition = registry.require(id.as_ref())?;
        Ok(self.get(definition.id().as_str(), registry)? <= definition.min_value())
    }

    /// Returns true when this stat value is at or above its maximum bound.
    pub fn is_max(&self, id: impl AsRef<str>, registry: &StatRegistry) -> Result<bool, StatError> {
        let definition = registry.require(id.as_ref())?;
        Ok(self.get(definition.id().as_str(), registry)? >= definition.max_value())
    }

    /// Number of explicitly stored stat values.
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// Returns true when no values are explicitly stored.
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Iterates explicitly stored values in unspecified hash-map order.
    pub fn iter(&self) -> impl Iterator<Item = (&StatId, f32)> {
        self.values.iter().map(|(id, value)| (id, *value))
    }

    /// Returns deterministic serializable snapshots of the explicitly stored values.
    pub fn snapshot(&self) -> Vec<StatValueSnapshot> {
        let mut values: Vec<_> = self
            .iter()
            .map(|(id, value)| StatValueSnapshot {
                id: id.clone(),
                value,
            })
            .collect();
        values.sort_by(|left, right| left.id.as_str().cmp(right.id.as_str()));
        values
    }
}

/// Serializable snapshot of one stored runtime stat value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StatValueSnapshot {
    /// Stable author-defined id.
    pub id: StatId,
    /// Current clamped runtime value.
    pub value: f32,
}

/// Message emitted by stat helper functions when a runtime stat value changes.
///
/// The framework does not interpret the stat id. Systems in the consuming game
/// decide whether a change means UI refresh, damage, an alarm, an achievement,
/// or nothing at all.
#[derive(Message, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StatChanged {
    /// Stable author-defined stat id.
    pub id: StatId,
    /// Value before the helper mutation.
    pub previous: f32,
    /// Value after clamping and applying the helper mutation.
    pub current: f32,
}

/// Sets a stat and emits [`StatChanged`] when the clamped value differs.
pub fn set_stat(
    id: impl AsRef<str>,
    value: f32,
    registry: &StatRegistry,
    values: &mut StatSet,
    changed: &mut MessageWriter<StatChanged>,
) -> Result<f32, StatError> {
    let definition = registry.require(id.as_ref())?;
    let previous = values.get(definition.id().as_str(), registry)?;
    let current = values.set(definition.id().as_str(), value, registry)?;
    write_stat_changed(definition, previous, current, changed);
    Ok(current)
}

/// Adds a delta to a stat and emits [`StatChanged`] when the clamped value differs.
pub fn add_stat(
    id: impl AsRef<str>,
    delta: f32,
    registry: &StatRegistry,
    values: &mut StatSet,
    changed: &mut MessageWriter<StatChanged>,
) -> Result<f32, StatError> {
    let definition = registry.require(id.as_ref())?;
    let previous = values.get(definition.id().as_str(), registry)?;
    set_stat(
        definition.id().as_str(),
        previous + finite_or_zero(delta),
        registry,
        values,
        changed,
    )
}

/// Removes from a stat and emits [`StatChanged`] when the clamped value differs.
pub fn drain_stat(
    id: impl AsRef<str>,
    amount: f32,
    registry: &StatRegistry,
    values: &mut StatSet,
    changed: &mut MessageWriter<StatChanged>,
) -> Result<f32, StatError> {
    add_stat(id, -finite_or_zero(amount).abs(), registry, values, changed)
}

/// Adds to a stat and emits [`StatChanged`] when the clamped value differs.
pub fn fill_stat(
    id: impl AsRef<str>,
    amount: f32,
    registry: &StatRegistry,
    values: &mut StatSet,
    changed: &mut MessageWriter<StatChanged>,
) -> Result<f32, StatError> {
    add_stat(id, finite_or_zero(amount).abs(), registry, values, changed)
}

/// Horizontal plugin that initializes author-defined stat resources.
///
/// This plugin does not register default stats. The game author defines the
/// stats their game needs and decides how systems interpret them.
pub struct VoxelStatsPlugin;

impl Plugin for VoxelStatsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<StatRegistry>();
        app.init_resource::<StatSet>();
        app.add_message::<StatChanged>();
    }
}

fn write_stat_changed(
    definition: &StatDefinition,
    previous: f32,
    current: f32,
    changed: &mut MessageWriter<StatChanged>,
) {
    if previous == current {
        return;
    }

    changed.write(StatChanged {
        id: definition.id().clone(),
        previous,
        current,
    });
}

fn finite_or_zero(value: f32) -> f32 {
    if value.is_finite() { value } else { 0.0 }
}

fn normalize_value(definition: &StatDefinition, value: f32) -> f32 {
    let span = definition.span();
    if span <= f32::EPSILON {
        return 1.0;
    }

    ((definition.clamp(value) - definition.min_value()) / span).clamp(0.0, 1.0)
}

fn normalized_threshold(threshold: f32) -> f32 {
    finite_or_zero(threshold).clamp(0.0, 1.0)
}

fn ordered_attributes(
    attributes: &HashMap<String, ContentValue>,
) -> BTreeMap<String, ContentValue> {
    attributes
        .iter()
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect()
}

fn ordered_tags(tags: &HashSet<String>) -> Vec<String> {
    let mut tags: Vec<_> = tags.iter().cloned().collect();
    tags.sort();
    tags
}

#[cfg(test)]
mod tests {
    use bevy::ecs::message::{MessageReader, MessageWriter};
    use bevy::prelude::{
        App, Component, IntoScheduleConfigs, MinimalPlugins, Query, Res, ResMut, Resource, Startup,
        Update, With,
    };

    use super::*;

    #[test]
    fn stat_definition_keeps_author_defined_metadata() {
        let stat = StatDefinition::new("oxygen", 100.0)
            .named("Suit Oxygen")
            .bounds(0.0, 100.0)
            .tag("survival")
            .attribute("ui_hint", "bar")
            .attribute("critical_threshold", 15.0);

        assert_eq!(stat.id().as_str(), "oxygen");
        assert_eq!(stat.display_name(), "Suit Oxygen");
        assert_eq!(stat.default_value(), 100.0);
        assert_eq!(stat.min_value(), 0.0);
        assert_eq!(stat.max_value(), 100.0);
        assert!(stat.has_tag("survival"));
        assert_eq!(stat.text_attribute("ui_hint"), Some("bar"));
        assert_eq!(stat.number_attribute("critical_threshold"), Some(15.0));
    }

    #[test]
    fn stat_definition_normalizes_bounds_and_clamps_default() {
        let stat = StatDefinition::new("temperature", 50.0).bounds(10.0, -10.0);

        assert_eq!(stat.min_value(), -10.0);
        assert_eq!(stat.max_value(), 10.0);
        assert_eq!(stat.span(), 20.0);
        assert_eq!(stat.default_value(), 10.0);
        assert_eq!(stat.clamp(99.0), 10.0);
        assert_eq!(stat.clamp(-99.0), -10.0);
    }

    #[test]
    fn stat_definition_initial_value_is_clamped_to_bounds() {
        let stat = StatDefinition::new("radiation", 0.0)
            .bounds(0.0, 100.0)
            .initial_value(250.0);

        assert_eq!(stat.default_value(), 100.0);
    }

    #[test]
    fn registry_rejects_duplicate_ids_and_requires_existing_stats() {
        let mut registry = StatRegistry::default();
        registry
            .register(StatDefinition::new("oxygen", 100.0))
            .expect("first stat should register");

        assert!(matches!(
            registry.register(StatDefinition::new("oxygen", 50.0)),
            Err(StatError::DuplicateId { .. })
        ));
        assert!(registry.require("oxygen").is_ok());
        assert!(matches!(
            registry.require("missing"),
            Err(StatError::MissingId { .. })
        ));
    }

    #[test]
    fn stat_set_uses_defaults_and_clamps_mutations() {
        let mut registry = StatRegistry::default();
        registry
            .register(StatDefinition::new("oxygen", 100.0).bounds(0.0, 100.0))
            .expect("stat should register");

        let mut values = StatSet::new();
        assert_eq!(values.get("oxygen", &registry), Ok(100.0));
        assert!(values.is_empty());

        assert_eq!(values.drain("oxygen", 125.0, &registry), Ok(0.0));
        assert_eq!(values.get("oxygen", &registry), Ok(0.0));
        assert_eq!(values.fill("oxygen", 250.0, &registry), Ok(100.0));
        assert_eq!(values.add("oxygen", -40.0, &registry), Ok(60.0));
    }

    #[test]
    fn stat_set_normalizes_current_value_against_registered_bounds() {
        let mut registry = StatRegistry::default();
        registry
            .register(StatDefinition::new("oxygen", 100.0).bounds(0.0, 100.0))
            .expect("stat should register");

        let mut values = StatSet::new();

        assert_eq!(values.normalized("oxygen", &registry), Ok(1.0));
        assert_eq!(values.set("oxygen", 25.0, &registry), Ok(25.0));
        assert_eq!(values.normalized("oxygen", &registry), Ok(0.25));
        assert_eq!(values.set("oxygen", -25.0, &registry), Ok(0.0));
        assert_eq!(values.normalized("oxygen", &registry), Ok(0.0));
        assert_eq!(values.set("oxygen", f32::INFINITY, &registry), Ok(0.0));
        assert_eq!(values.normalized("oxygen", &registry), Ok(0.0));
    }

    #[test]
    fn stat_set_normalization_handles_equal_bounds_without_dividing_by_zero() {
        let mut registry = StatRegistry::default();
        registry
            .register(StatDefinition::new("locked", 5.0).bounds(5.0, 5.0))
            .expect("stat should register");

        let mut values = StatSet::new();

        assert_eq!(
            registry.require("locked").map(StatDefinition::span),
            Ok(0.0)
        );
        assert_eq!(values.normalized("locked", &registry), Ok(1.0));
        assert_eq!(values.set("locked", f32::NAN, &registry), Ok(5.0));
        assert_eq!(values.normalized("locked", &registry), Ok(1.0));
    }

    #[test]
    fn stat_set_supports_absolute_threshold_checks() {
        let mut registry = StatRegistry::default();
        registry
            .register(StatDefinition::new("oxygen", 100.0).bounds(0.0, 100.0))
            .expect("stat should register");

        let mut values = StatSet::new();
        values
            .set("oxygen", 15.0, &registry)
            .expect("oxygen exists");

        assert_eq!(values.is_at_or_below("oxygen", 15.0, &registry), Ok(true));
        assert_eq!(values.is_at_or_below("oxygen", 14.0, &registry), Ok(false));
        assert_eq!(values.is_at_or_above("oxygen", 15.0, &registry), Ok(true));
        assert_eq!(values.is_at_or_above("oxygen", 16.0, &registry), Ok(false));
        assert_eq!(
            values.is_at_or_below("oxygen", f32::INFINITY, &registry),
            Ok(false)
        );
    }

    #[test]
    fn stat_set_supports_normalized_threshold_checks() {
        let mut registry = StatRegistry::default();
        registry
            .register(StatDefinition::new("oxygen", 100.0).bounds(0.0, 100.0))
            .expect("stat should register");

        let mut values = StatSet::new();
        values
            .set("oxygen", 15.0, &registry)
            .expect("oxygen exists");

        assert_eq!(
            values.is_at_or_below_percent("oxygen", 0.15, &registry),
            Ok(true)
        );
        assert_eq!(
            values.is_at_or_below_percent("oxygen", 0.14, &registry),
            Ok(false)
        );
        assert_eq!(
            values.is_at_or_above_percent("oxygen", 0.15, &registry),
            Ok(true)
        );
        assert_eq!(
            values.is_at_or_above_percent("oxygen", 0.16, &registry),
            Ok(false)
        );
        assert_eq!(
            values.is_at_or_above_percent("oxygen", f32::INFINITY, &registry),
            Ok(true)
        );
    }

    #[test]
    fn stat_read_helpers_return_missing_id_errors() {
        let registry = StatRegistry::default();
        let values = StatSet::new();

        assert!(matches!(
            values.normalized("missing", &registry),
            Err(StatError::MissingId { .. })
        ));
        assert!(matches!(
            values.is_at_or_below("missing", 1.0, &registry),
            Err(StatError::MissingId { .. })
        ));
        assert!(matches!(
            values.is_at_or_below_percent("missing", 0.5, &registry),
            Err(StatError::MissingId { .. })
        ));
    }

    #[test]
    fn ensure_registered_inserts_missing_defaults_without_overwriting_existing_values() {
        let mut registry = StatRegistry::default();
        registry
            .register(StatDefinition::new("oxygen", 100.0))
            .expect("stat should register");
        registry
            .register(StatDefinition::new("mana", 50.0))
            .expect("stat should register");

        let mut values = StatSet::new();
        values
            .set("oxygen", 25.0, &registry)
            .expect("oxygen exists");
        values.ensure_registered(&registry);

        assert_eq!(values.get("oxygen", &registry), Ok(25.0));
        assert_eq!(values.get("mana", &registry), Ok(50.0));
        assert_eq!(values.len(), 2);
    }

    #[test]
    fn stat_set_from_registry_inserts_registered_defaults() {
        let mut registry = StatRegistry::default();
        registry
            .register(StatDefinition::new("oxygen", 100.0))
            .expect("stat should register");
        registry
            .register(StatDefinition::new("radiation", 0.0).bounds(0.0, 1_000.0))
            .expect("stat should register");

        let values = StatSet::from_registry(&registry);

        assert_eq!(values.get("oxygen", &registry), Ok(100.0));
        assert_eq!(values.get("radiation", &registry), Ok(0.0));
        assert_eq!(values.len(), 2);
    }

    #[test]
    fn snapshots_are_serializable_and_deterministic() {
        let mut registry = StatRegistry::default();
        registry
            .register(
                StatDefinition::new("oxygen", 100.0)
                    .tag("survival")
                    .tag("space_suit")
                    .attribute("ui_hint", "bar"),
            )
            .expect("stat should register");

        let definition_snapshot = registry.snapshot();
        assert_eq!(
            definition_snapshot[0].tags,
            vec!["space_suit".to_owned(), "survival".to_owned()]
        );

        let mut values = StatSet::new();
        values
            .set("oxygen", 75.0, &registry)
            .expect("oxygen exists");
        let value_snapshot = values.snapshot();
        assert_eq!(value_snapshot[0].id.as_str(), "oxygen");
        assert_eq!(value_snapshot[0].value, 75.0);

        serde_json::to_string(&definition_snapshot).expect("definitions should serialize");
        serde_json::to_string(&value_snapshot).expect("values should serialize");
    }

    #[test]
    fn stats_plugin_initializes_empty_resources_without_defaults() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(VoxelStatsPlugin);

        assert!(app.world().resource::<StatRegistry>().is_empty());
        assert!(app.world().resource::<StatSet>().is_empty());
    }

    #[derive(Resource, Default)]
    struct ObservedStatChanges(Vec<StatChanged>);

    #[derive(Resource, Default)]
    struct ObservedStatErrors(Vec<StatError>);

    #[derive(Component)]
    struct StatSubject;

    fn register_oxygen_stat(mut registry: ResMut<StatRegistry>, mut values: ResMut<StatSet>) {
        registry
            .register(StatDefinition::new("oxygen", 100.0).bounds(0.0, 100.0))
            .expect("oxygen should register");
        values.ensure_registered(&registry);
    }

    fn record_stat_changes(
        mut reader: MessageReader<StatChanged>,
        mut observed: ResMut<ObservedStatChanges>,
    ) {
        observed.0.extend(reader.read().cloned());
    }

    fn drain_oxygen_with_helper(
        registry: Res<StatRegistry>,
        mut values: ResMut<StatSet>,
        mut changed: MessageWriter<StatChanged>,
    ) {
        drain_stat("oxygen", 25.0, &registry, &mut values, &mut changed)
            .expect("oxygen should exist");
    }

    fn set_oxygen_to_same_value_with_helper(
        registry: Res<StatRegistry>,
        mut values: ResMut<StatSet>,
        mut changed: MessageWriter<StatChanged>,
    ) {
        set_stat("oxygen", 100.0, &registry, &mut values, &mut changed)
            .expect("oxygen should exist");
    }

    fn fill_oxygen_past_max_with_helper(
        registry: Res<StatRegistry>,
        mut values: ResMut<StatSet>,
        mut changed: MessageWriter<StatChanged>,
    ) {
        set_stat("oxygen", 90.0, &registry, &mut values, &mut changed)
            .expect("oxygen should exist");
        fill_stat("oxygen", 50.0, &registry, &mut values, &mut changed)
            .expect("oxygen should exist");
    }

    fn add_missing_stat_with_helper(
        registry: Res<StatRegistry>,
        mut values: ResMut<StatSet>,
        mut changed: MessageWriter<StatChanged>,
        mut errors: ResMut<ObservedStatErrors>,
    ) {
        let error = add_stat("missing", 1.0, &registry, &mut values, &mut changed)
            .expect_err("missing stat should return an error");
        errors.0.push(error);
    }

    fn drain_subject_oxygen_with_query(
        registry: Res<StatRegistry>,
        mut subjects: Query<&mut StatSet, With<StatSubject>>,
        mut changed: MessageWriter<StatChanged>,
    ) {
        for mut values in &mut subjects {
            drain_stat("oxygen", 25.0, &registry, &mut values, &mut changed)
                .expect("oxygen should exist");
        }
    }

    #[test]
    fn stat_helpers_emit_changed_message_when_value_changes() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(VoxelStatsPlugin);
        app.init_resource::<ObservedStatChanges>();
        app.add_systems(Startup, register_oxygen_stat);
        app.add_systems(
            Update,
            (drain_oxygen_with_helper, record_stat_changes).chain(),
        );

        app.update();

        let observed = app.world().resource::<ObservedStatChanges>();
        assert_eq!(
            observed.0,
            vec![StatChanged {
                id: StatId::new("oxygen"),
                previous: 100.0,
                current: 75.0,
            }]
        );
    }

    #[test]
    fn stat_helpers_do_not_emit_when_clamped_value_is_unchanged() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(VoxelStatsPlugin);
        app.init_resource::<ObservedStatChanges>();
        app.add_systems(Startup, register_oxygen_stat);
        app.add_systems(
            Update,
            (set_oxygen_to_same_value_with_helper, record_stat_changes).chain(),
        );

        app.update();

        let observed = app.world().resource::<ObservedStatChanges>();
        assert!(observed.0.is_empty());
        assert_eq!(
            app.world()
                .resource::<StatSet>()
                .get("oxygen", app.world().resource::<StatRegistry>()),
            Ok(100.0)
        );
    }

    #[test]
    fn stat_helpers_emit_clamped_final_value() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(VoxelStatsPlugin);
        app.init_resource::<ObservedStatChanges>();
        app.add_systems(Startup, register_oxygen_stat);
        app.add_systems(
            Update,
            (fill_oxygen_past_max_with_helper, record_stat_changes).chain(),
        );

        app.update();

        let observed = app.world().resource::<ObservedStatChanges>();
        assert_eq!(
            observed.0,
            vec![
                StatChanged {
                    id: StatId::new("oxygen"),
                    previous: 100.0,
                    current: 90.0,
                },
                StatChanged {
                    id: StatId::new("oxygen"),
                    previous: 90.0,
                    current: 100.0,
                },
            ]
        );
    }

    #[test]
    fn stat_helpers_return_missing_id_without_emitting_message() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(VoxelStatsPlugin);
        app.init_resource::<ObservedStatChanges>();
        app.init_resource::<ObservedStatErrors>();
        app.add_systems(Startup, register_oxygen_stat);
        app.add_systems(
            Update,
            (add_missing_stat_with_helper, record_stat_changes).chain(),
        );

        app.update();

        let observed = app.world().resource::<ObservedStatChanges>();
        assert!(observed.0.is_empty());

        let errors = app.world().resource::<ObservedStatErrors>();
        assert_eq!(
            errors.0,
            vec![StatError::MissingId {
                id: StatId::new("missing"),
            }]
        );
    }

    #[test]
    fn stat_set_can_be_attached_to_bevy_entities_as_component() {
        let mut registry = StatRegistry::default();
        registry
            .register(StatDefinition::new("oxygen", 100.0).bounds(0.0, 100.0))
            .expect("oxygen should register");

        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        let entity = app
            .world_mut()
            .spawn((StatSubject, StatSet::from_registry(&registry)))
            .id();

        let values = app
            .world()
            .get::<StatSet>(entity)
            .expect("entity should have stat set");

        assert_eq!(values.get("oxygen", &registry), Ok(100.0));
    }

    #[test]
    fn stat_helpers_can_mutate_query_stat_sets() {
        let mut registry = StatRegistry::default();
        registry
            .register(StatDefinition::new("oxygen", 100.0).bounds(0.0, 100.0))
            .expect("oxygen should register");
        let initial_values = StatSet::from_registry(&registry);

        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(VoxelStatsPlugin);
        app.insert_resource(registry);
        app.init_resource::<ObservedStatChanges>();
        let entity = app.world_mut().spawn((StatSubject, initial_values)).id();
        app.add_systems(
            Update,
            (drain_subject_oxygen_with_query, record_stat_changes).chain(),
        );

        app.update();

        let registry = app.world().resource::<StatRegistry>();
        let values = app
            .world()
            .get::<StatSet>(entity)
            .expect("entity should have stat set");
        assert_eq!(values.get("oxygen", registry), Ok(75.0));

        let observed = app.world().resource::<ObservedStatChanges>();
        assert_eq!(
            observed.0,
            vec![StatChanged {
                id: StatId::new("oxygen"),
                previous: 100.0,
                current: 75.0,
            }]
        );
    }
}
