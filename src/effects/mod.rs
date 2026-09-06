//! Author-defined effect registries and active effect state.
//!
//! `effects` is a horizontal gameplay-definition layer. It does **not** ship
//! poison, speed, regeneration, potion, survival, or player-specific behavior.
//! The game author defines effect ids, metadata, tags, stacks, durations, and
//! the systems that interpret active effects.
//!
//! The framework provides three generic pieces:
//!
//! - [`EffectRegistry`] — author-defined metadata for effects.
//! - [`ActiveEffectSet`] — runtime effect instances, usable globally or as a
//!   component on any entity.
//! - [`EffectApplied`] / [`EffectExpired`] plus helper functions for Bevy
//!   systems that want to react to effect lifecycle changes.
//!
//! # Example
//!
//! ```rust
//! use bevy::ecs::message::MessageWriter;
//! use bevy::prelude::*;
//! use voxel_framework::effects::{
//!     ActiveEffectSet, EffectApplied, EffectDefinition, EffectRegistry, VoxelEffectsPlugin,
//!     apply_effect,
//! };
//!
//! fn register_effects(mut effects: ResMut<EffectRegistry>) {
//!     effects
//!         .register(
//!             EffectDefinition::new("low_oxygen")
//!                 .named("Low Oxygen")
//!                 .duration_secs(10.0)
//!                 .max_stacks(3)
//!                 .tag("space_suit"),
//!         )
//!         .expect("effect id should be unique");
//! }
//!
//! fn apply_low_oxygen(
//!     registry: Res<EffectRegistry>,
//!     mut active: ResMut<ActiveEffectSet>,
//!     mut applied: MessageWriter<EffectApplied>,
//! ) {
//!     apply_effect("low_oxygen", &registry, &mut active, &mut applied)
//!         .expect("effect should exist");
//! }
//!
//! App::new()
//!     .add_plugins(MinimalPlugins)
//!     .add_plugins(VoxelEffectsPlugin)
//!     .add_systems(Startup, register_effects)
//!     .add_systems(Update, apply_low_oxygen);
//! ```

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;

use bevy::ecs::message::MessageWriter;
use bevy::prelude::{App, Component, Message, Plugin, Resource};
use serde::{Deserialize, Serialize};

use crate::content::ContentValue;

/// Stable identifier for an author-defined effect.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EffectId(String);

impl EffectId {
    /// Creates a new effect id from any string-like value.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Returns the raw id string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for EffectId {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for EffectId {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl AsRef<str> for EffectId {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for EffectId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Author-defined effect metadata.
#[derive(Debug, Clone, PartialEq)]
pub struct EffectDefinition {
    id: EffectId,
    display_name: String,
    default_duration_secs: Option<f32>,
    max_stacks: u32,
    attributes: HashMap<String, ContentValue>,
    tags: HashSet<String>,
}

/// Serializable snapshot of one author-defined effect definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EffectDefinitionSnapshot {
    /// Stable author-defined id.
    pub id: EffectId,
    /// Display name intended for UI/docs.
    pub display_name: String,
    /// Default duration for newly applied instances. `None` means indefinite.
    pub default_duration_secs: Option<f32>,
    /// Maximum stack count when repeatedly applying this effect.
    pub max_stacks: u32,
    /// Author-defined attributes in deterministic key order.
    pub attributes: BTreeMap<String, ContentValue>,
    /// Author-defined tags in deterministic lexical order.
    pub tags: Vec<String>,
}

impl EffectDefinition {
    /// Creates an indefinite, single-stack effect definition.
    pub fn new(id: impl Into<EffectId>) -> Self {
        Self {
            id: id.into(),
            display_name: String::new(),
            default_duration_secs: None,
            max_stacks: 1,
            attributes: HashMap::new(),
            tags: HashSet::new(),
        }
    }

    /// Returns the stable author-defined id.
    pub fn id(&self) -> &EffectId {
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

    /// Returns the default duration for newly applied instances.
    pub fn default_duration_secs(&self) -> Option<f32> {
        self.default_duration_secs
    }

    /// Sets the default duration for newly applied instances.
    pub fn duration_secs(mut self, duration_secs: f32) -> Self {
        self.default_duration_secs = Some(non_negative_finite_or_zero(duration_secs));
        self
    }

    /// Makes newly applied instances indefinite.
    pub fn indefinite(mut self) -> Self {
        self.default_duration_secs = None;
        self
    }

    /// Returns the maximum stack count for this effect.
    pub fn max_stacks_value(&self) -> u32 {
        self.max_stacks
    }

    /// Sets the maximum stack count. Values below `1` are normalized to `1`.
    pub fn max_stacks(mut self, max_stacks: u32) -> Self {
        self.max_stacks = max_stacks.max(1);
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

    /// Creates a serializable, deterministic snapshot of this effect definition.
    pub fn snapshot(&self) -> EffectDefinitionSnapshot {
        EffectDefinitionSnapshot {
            id: self.id.clone(),
            display_name: self.display_name().to_owned(),
            default_duration_secs: self.default_duration_secs,
            max_stacks: self.max_stacks,
            attributes: ordered_attributes(&self.attributes),
            tags: ordered_tags(&self.tags),
        }
    }
}

/// Errors returned by effect registries and active effect sets.
#[derive(Debug, Clone, PartialEq)]
pub enum EffectError {
    /// An effect with the same id already exists.
    DuplicateId { id: EffectId },
    /// No effect exists for the requested id.
    MissingId { id: EffectId },
}

impl fmt::Display for EffectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateId { id } => write!(f, "effect id `{id}` is already registered"),
            Self::MissingId { id } => write!(f, "effect id `{id}` is not registered"),
        }
    }
}

impl std::error::Error for EffectError {}

/// Registry of author-defined effect definitions.
#[derive(Debug, Clone, Default, Resource)]
pub struct EffectRegistry {
    effects_by_id: HashMap<EffectId, EffectDefinition>,
}

impl EffectRegistry {
    /// Registers a new effect definition.
    pub fn register(&mut self, definition: EffectDefinition) -> Result<(), EffectError> {
        if self.effects_by_id.contains_key(definition.id()) {
            return Err(EffectError::DuplicateId {
                id: definition.id().clone(),
            });
        }

        self.effects_by_id
            .insert(definition.id().clone(), definition);
        Ok(())
    }

    /// Inserts a definition, replacing any previous definition for the same id.
    pub fn insert_or_replace(&mut self, definition: EffectDefinition) {
        self.effects_by_id
            .insert(definition.id().clone(), definition);
    }

    /// Returns a definition by effect id.
    pub fn get(&self, id: impl AsRef<str>) -> Option<&EffectDefinition> {
        let id = EffectId::new(id.as_ref());
        self.effects_by_id.get(&id)
    }

    /// Returns a definition by id or an explicit error.
    pub fn require(&self, id: impl AsRef<str>) -> Result<&EffectDefinition, EffectError> {
        let id = EffectId::new(id.as_ref());
        self.effects_by_id
            .get(&id)
            .ok_or(EffectError::MissingId { id })
    }

    /// Returns true if an id exists.
    pub fn contains_id(&self, id: impl AsRef<str>) -> bool {
        self.get(id).is_some()
    }

    /// Number of registered effect definitions.
    pub fn len(&self) -> usize {
        self.effects_by_id.len()
    }

    /// Returns true when no effect definitions are registered.
    pub fn is_empty(&self) -> bool {
        self.effects_by_id.is_empty()
    }

    /// Iterates definitions in unspecified hash-map order.
    pub fn iter(&self) -> impl Iterator<Item = &EffectDefinition> {
        self.effects_by_id.values()
    }

    /// Returns deterministic serializable snapshots of all registered effects.
    pub fn snapshot(&self) -> Vec<EffectDefinitionSnapshot> {
        let mut effects: Vec<_> = self.iter().map(EffectDefinition::snapshot).collect();
        effects.sort_by(|left, right| left.id.as_str().cmp(right.id.as_str()));
        effects
    }
}

/// Runtime instance of an author-defined effect.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EffectInstance {
    id: EffectId,
    duration_secs: Option<f32>,
    remaining_secs: Option<f32>,
    stacks: u32,
}

/// Serializable snapshot of one active effect instance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EffectInstanceSnapshot {
    /// Stable author-defined id.
    pub id: EffectId,
    /// Initial duration captured when the instance was applied. `None` means indefinite.
    pub duration_secs: Option<f32>,
    /// Remaining duration. `None` means indefinite.
    pub remaining_secs: Option<f32>,
    /// Current stack count.
    pub stacks: u32,
}

impl EffectInstance {
    /// Creates an effect instance with an initial duration and stack count.
    ///
    /// The provided duration is stored as both the original duration and the
    /// current remaining duration. `None` means indefinite.
    pub fn new(id: impl Into<EffectId>, duration_secs: Option<f32>, stacks: u32) -> Self {
        let duration_secs = duration_secs.map(non_negative_finite_or_zero);
        Self::with_runtime_state(id, duration_secs, duration_secs, stacks)
    }

    /// Creates an effect instance with explicit original and remaining durations.
    ///
    /// This is useful when hydrating effect state from author-owned save data or
    /// when forwarding an already-ticked instance. Durations are normalized to
    /// finite, non-negative values; `None` means indefinite.
    pub fn with_runtime_state(
        id: impl Into<EffectId>,
        duration_secs: Option<f32>,
        remaining_secs: Option<f32>,
        stacks: u32,
    ) -> Self {
        Self {
            id: id.into(),
            duration_secs: duration_secs.map(non_negative_finite_or_zero),
            remaining_secs: remaining_secs.map(non_negative_finite_or_zero),
            stacks: stacks.max(1),
        }
    }

    /// Creates an instance from a registered definition's defaults.
    pub fn from_definition(definition: &EffectDefinition) -> Self {
        Self::new(
            definition.id().clone(),
            definition.default_duration_secs(),
            1,
        )
    }

    /// Returns the stable author-defined id.
    pub fn id(&self) -> &EffectId {
        &self.id
    }

    /// Returns the initial duration captured when the instance was applied.
    ///
    /// `None` means the effect is indefinite.
    pub fn duration_secs(&self) -> Option<f32> {
        self.duration_secs
    }

    /// Returns remaining duration. `None` means indefinite.
    pub fn remaining_secs(&self) -> Option<f32> {
        self.remaining_secs
    }

    /// Returns the remaining fraction of the original duration.
    ///
    /// Timed effects return a value clamped to `0.0..=1.0`. Indefinite effects
    /// return `None` because they have no finite denominator.
    pub fn remaining_ratio(&self) -> Option<f32> {
        let duration_secs = self.duration_secs?;
        if duration_secs <= f32::EPSILON {
            return Some(0.0);
        }

        Some((self.remaining_secs.unwrap_or(0.0) / duration_secs).clamp(0.0, 1.0))
    }

    /// Returns the current stack count.
    pub fn stacks(&self) -> u32 {
        self.stacks
    }

    /// Returns true when the instance has a finite duration.
    pub fn is_timed(&self) -> bool {
        self.remaining_secs.is_some()
    }

    /// Returns a serializable snapshot of this effect instance.
    pub fn snapshot(&self) -> EffectInstanceSnapshot {
        EffectInstanceSnapshot {
            id: self.id.clone(),
            duration_secs: self.duration_secs,
            remaining_secs: self.remaining_secs,
            stacks: self.stacks,
        }
    }
}

impl<'de> Deserialize<'de> for EffectInstance {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct SerializedEffectInstance {
            id: EffectId,
            #[serde(default)]
            duration_secs: Option<f32>,
            remaining_secs: Option<f32>,
            stacks: u32,
        }

        let serialized = SerializedEffectInstance::deserialize(deserializer)?;
        let duration_secs = serialized.duration_secs.or(serialized.remaining_secs);
        Ok(Self::with_runtime_state(
            serialized.id,
            duration_secs,
            serialized.remaining_secs,
            serialized.stacks,
        ))
    }
}

/// Active author-defined effect instances.
///
/// An `ActiveEffectSet` can be used as a global [`Resource`] or attached to
/// entities as a [`Component`]. The framework does not assign meaning to the
/// owner: it can be a player, mob, vehicle, machine, region, settlement, or any
/// author-defined subject.
#[derive(Debug, Clone, Default, Component, Resource, Serialize, Deserialize)]
pub struct ActiveEffectSet {
    effects_by_id: HashMap<EffectId, EffectInstance>,
}

impl ActiveEffectSet {
    /// Creates an empty active effect set.
    pub fn new() -> Self {
        Self::default()
    }

    /// Applies an effect by id using registry defaults.
    ///
    /// Re-applying an active effect refreshes its duration from the definition
    /// and increments stacks up to the definition's `max_stacks`.
    pub fn apply(
        &mut self,
        id: impl AsRef<str>,
        registry: &EffectRegistry,
    ) -> Result<EffectInstance, EffectError> {
        let definition = registry.require(id.as_ref())?;
        let next = match self.effects_by_id.get(definition.id()) {
            Some(current) => EffectInstance::new(
                definition.id().clone(),
                definition.default_duration_secs(),
                (current.stacks() + 1).min(definition.max_stacks_value()),
            ),
            None => EffectInstance::from_definition(definition),
        };
        self.effects_by_id
            .insert(definition.id().clone(), next.clone());
        Ok(next)
    }

    /// Applies an explicit instance after resolving and clamping it through the registry.
    pub fn apply_instance(
        &mut self,
        instance: EffectInstance,
        registry: &EffectRegistry,
    ) -> Result<EffectInstance, EffectError> {
        let definition = registry.require(instance.id().as_str())?;
        let next = EffectInstance::with_runtime_state(
            definition.id().clone(),
            instance.duration_secs(),
            instance.remaining_secs(),
            instance.stacks().min(definition.max_stacks_value()),
        );
        self.effects_by_id
            .insert(definition.id().clone(), next.clone());
        Ok(next)
    }

    /// Removes an active effect by id.
    pub fn remove(&mut self, id: impl AsRef<str>) -> Option<EffectInstance> {
        let id = EffectId::new(id.as_ref());
        self.effects_by_id.remove(&id)
    }

    /// Advances finite durations and returns expired instances.
    ///
    /// Indefinite effects are not affected. Non-finite or negative deltas are
    /// normalized to `0.0`.
    pub fn tick(&mut self, delta_secs: f32) -> Vec<EffectInstance> {
        let delta_secs = non_negative_finite_or_zero(delta_secs);
        if delta_secs == 0.0 {
            return Vec::new();
        }

        let mut expired_ids = Vec::new();
        for (id, instance) in &mut self.effects_by_id {
            let Some(remaining_secs) = &mut instance.remaining_secs else {
                continue;
            };
            *remaining_secs = non_negative_finite_or_zero(*remaining_secs - delta_secs);
            if *remaining_secs <= f32::EPSILON {
                expired_ids.push(id.clone());
            }
        }

        let mut expired = Vec::with_capacity(expired_ids.len());
        for id in expired_ids {
            if let Some(instance) = self.effects_by_id.remove(&id) {
                expired.push(instance);
            }
        }
        expired.sort_by(|left, right| left.id().as_str().cmp(right.id().as_str()));
        expired
    }

    /// Returns true when an effect id is active.
    pub fn has(&self, id: impl AsRef<str>) -> bool {
        self.get(id).is_some()
    }

    /// Returns true when an effect id is active.
    ///
    /// This is a read-side alias for [`ActiveEffectSet::has`].
    pub fn is_active(&self, id: impl AsRef<str>) -> bool {
        self.has(id)
    }

    /// Returns an active effect by id.
    pub fn get(&self, id: impl AsRef<str>) -> Option<&EffectInstance> {
        let id = EffectId::new(id.as_ref());
        self.effects_by_id.get(&id)
    }

    /// Returns the current stack count for an active effect id, or `0` when inactive.
    pub fn stacks(&self, id: impl AsRef<str>) -> u32 {
        self.get(id).map_or(0, EffectInstance::stacks)
    }

    /// Returns the remaining duration for an active effect id.
    ///
    /// `None` means the effect is inactive or indefinite.
    pub fn remaining_secs(&self, id: impl AsRef<str>) -> Option<f32> {
        self.get(id).and_then(EffectInstance::remaining_secs)
    }

    /// Returns the remaining fraction for an active timed effect id.
    ///
    /// `None` means the effect is inactive or indefinite.
    pub fn remaining_ratio(&self, id: impl AsRef<str>) -> Option<f32> {
        self.get(id).and_then(EffectInstance::remaining_ratio)
    }

    /// Number of active effects.
    pub fn len(&self) -> usize {
        self.effects_by_id.len()
    }

    /// Returns true when no effects are active.
    pub fn is_empty(&self) -> bool {
        self.effects_by_id.is_empty()
    }

    /// Iterates active effects in unspecified hash-map order.
    pub fn iter(&self) -> impl Iterator<Item = &EffectInstance> {
        self.effects_by_id.values()
    }

    /// Returns deterministic serializable snapshots of all active effects.
    pub fn snapshot(&self) -> Vec<EffectInstanceSnapshot> {
        let mut effects: Vec<_> = self.iter().map(EffectInstance::snapshot).collect();
        effects.sort_by(|left, right| left.id.as_str().cmp(right.id.as_str()));
        effects
    }
}

/// Message emitted by [`apply_effect`] and [`apply_effect_instance`].
///
/// The framework does not interpret the effect id. Systems in the consuming
/// game decide whether an applied effect changes stats, UI, audio, physics,
/// abilities, AI, or nothing at all.
#[derive(Message, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EffectApplied {
    /// Stable author-defined effect id.
    pub id: EffectId,
    /// Previous active instance, if the effect was already active.
    pub previous: Option<EffectInstance>,
    /// Current active instance after applying.
    pub current: EffectInstance,
}

/// Message emitted by [`tick_effects`] when finite effects expire.
#[derive(Message, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EffectExpired {
    /// Stable author-defined effect id.
    pub id: EffectId,
    /// Runtime instance removed from the active set.
    pub expired: EffectInstance,
}

/// Applies an effect and emits [`EffectApplied`].
pub fn apply_effect(
    id: impl AsRef<str>,
    registry: &EffectRegistry,
    effects: &mut ActiveEffectSet,
    applied: &mut MessageWriter<EffectApplied>,
) -> Result<EffectInstance, EffectError> {
    let definition = registry.require(id.as_ref())?;
    let previous = effects.get(definition.id().as_str()).cloned();
    let current = effects.apply(definition.id().as_str(), registry)?;
    applied.write(EffectApplied {
        id: definition.id().clone(),
        previous,
        current: current.clone(),
    });
    Ok(current)
}

/// Applies an explicit effect instance and emits [`EffectApplied`].
pub fn apply_effect_instance(
    instance: EffectInstance,
    registry: &EffectRegistry,
    effects: &mut ActiveEffectSet,
    applied: &mut MessageWriter<EffectApplied>,
) -> Result<EffectInstance, EffectError> {
    let definition = registry.require(instance.id().as_str())?;
    let previous = effects.get(definition.id().as_str()).cloned();
    let current = effects.apply_instance(instance, registry)?;
    applied.write(EffectApplied {
        id: definition.id().clone(),
        previous,
        current: current.clone(),
    });
    Ok(current)
}

/// Advances finite effect durations and emits [`EffectExpired`] for each expired effect.
pub fn tick_effects(
    delta_secs: f32,
    effects: &mut ActiveEffectSet,
    expired: &mut MessageWriter<EffectExpired>,
) -> Vec<EffectInstance> {
    let expired_instances = effects.tick(delta_secs);
    for instance in &expired_instances {
        expired.write(EffectExpired {
            id: instance.id().clone(),
            expired: instance.clone(),
        });
    }
    expired_instances
}

/// Horizontal plugin that initializes author-defined effect resources.
///
/// This plugin does not register default effects. The game author defines the
/// effects their game needs and decides how systems interpret them.
pub struct VoxelEffectsPlugin;

impl Plugin for VoxelEffectsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<EffectRegistry>();
        app.init_resource::<ActiveEffectSet>();
        app.add_message::<EffectApplied>();
        app.add_message::<EffectExpired>();
    }
}

fn non_negative_finite_or_zero(value: f32) -> f32 {
    if value.is_finite() {
        value.max(0.0)
    } else {
        0.0
    }
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
    fn effect_definition_keeps_author_defined_metadata() {
        let effect = EffectDefinition::new("low_oxygen")
            .named("Low Oxygen")
            .duration_secs(10.0)
            .max_stacks(3)
            .tag("space_suit")
            .attribute("ui_hint", "warning")
            .attribute("severity", 2.0);

        assert_eq!(effect.id().as_str(), "low_oxygen");
        assert_eq!(effect.display_name(), "Low Oxygen");
        assert_eq!(effect.default_duration_secs(), Some(10.0));
        assert_eq!(effect.max_stacks_value(), 3);
        assert!(effect.has_tag("space_suit"));
        assert_eq!(effect.text_attribute("ui_hint"), Some("warning"));
        assert_eq!(effect.number_attribute("severity"), Some(2.0));
    }

    #[test]
    fn effect_definition_normalizes_duration_and_stack_bounds() {
        let effect = EffectDefinition::new("glitch")
            .duration_secs(f32::INFINITY)
            .max_stacks(0);

        assert_eq!(effect.default_duration_secs(), Some(0.0));
        assert_eq!(effect.max_stacks_value(), 1);
    }

    #[test]
    fn registry_rejects_duplicate_ids_and_requires_existing_effects() {
        let mut registry = EffectRegistry::default();
        registry
            .register(EffectDefinition::new("low_oxygen"))
            .expect("first effect should register");

        assert!(matches!(
            registry.register(EffectDefinition::new("low_oxygen")),
            Err(EffectError::DuplicateId { .. })
        ));
        assert!(registry.require("low_oxygen").is_ok());
        assert!(matches!(
            registry.require("missing"),
            Err(EffectError::MissingId { .. })
        ));
    }

    #[test]
    fn active_effect_set_applies_refreshes_and_caps_stacks() {
        let mut registry = EffectRegistry::default();
        registry
            .register(
                EffectDefinition::new("low_oxygen")
                    .duration_secs(10.0)
                    .max_stacks(2),
            )
            .expect("effect should register");
        let mut effects = ActiveEffectSet::new();

        let first = effects
            .apply("low_oxygen", &registry)
            .expect("effect should exist");
        assert_eq!(first.duration_secs(), Some(10.0));
        assert_eq!(first.remaining_secs(), Some(10.0));
        assert_eq!(first.remaining_ratio(), Some(1.0));
        assert_eq!(first.stacks(), 1);

        effects.tick(4.0);
        assert_eq!(effects.remaining_secs("low_oxygen"), Some(6.0));
        assert_eq!(effects.remaining_ratio("low_oxygen"), Some(0.6));
        let second = effects
            .apply("low_oxygen", &registry)
            .expect("effect should exist");
        assert_eq!(second.duration_secs(), Some(10.0));
        assert_eq!(second.remaining_secs(), Some(10.0));
        assert_eq!(second.remaining_ratio(), Some(1.0));
        assert_eq!(second.stacks(), 2);

        let third = effects
            .apply("low_oxygen", &registry)
            .expect("effect should exist");
        assert_eq!(third.stacks(), 2);
    }

    #[test]
    fn apply_instance_clamps_stacks_to_definition() {
        let mut registry = EffectRegistry::default();
        registry
            .register(EffectDefinition::new("shielded").max_stacks(3))
            .expect("effect should register");
        let mut effects = ActiveEffectSet::new();

        let applied = effects
            .apply_instance(EffectInstance::new("shielded", Some(5.0), 99), &registry)
            .expect("effect should exist");

        assert_eq!(applied.stacks(), 3);
        assert_eq!(applied.duration_secs(), Some(5.0));
        assert_eq!(applied.remaining_secs(), Some(5.0));
    }

    #[test]
    fn apply_instance_preserves_original_duration_for_ticked_instances() {
        let mut registry = EffectRegistry::default();
        registry
            .register(EffectDefinition::new("shielded").duration_secs(10.0))
            .expect("effect should register");

        let mut source = ActiveEffectSet::new();
        source.apply("shielded", &registry).expect("effect exists");
        source.tick(4.0);
        let ticked = source
            .get("shielded")
            .expect("effect should still be active")
            .clone();

        let mut target = ActiveEffectSet::new();
        let applied = target
            .apply_instance(ticked, &registry)
            .expect("effect should exist");

        assert_eq!(applied.duration_secs(), Some(10.0));
        assert_eq!(applied.remaining_secs(), Some(6.0));
        assert_eq!(applied.remaining_ratio(), Some(0.6));
    }

    #[test]
    fn active_effect_set_ticks_and_expires_timed_effects_only() {
        let mut registry = EffectRegistry::default();
        registry
            .register(EffectDefinition::new("timed").duration_secs(2.0))
            .expect("effect should register");
        registry
            .register(EffectDefinition::new("indefinite"))
            .expect("effect should register");

        let mut effects = ActiveEffectSet::new();
        effects.apply("timed", &registry).expect("timed exists");
        effects
            .apply("indefinite", &registry)
            .expect("indefinite exists");

        assert!(effects.tick(f32::NAN).is_empty());
        assert_eq!(
            effects
                .get("timed")
                .expect("timed should still be active")
                .duration_secs(),
            Some(2.0)
        );
        assert_eq!(
            effects
                .get("timed")
                .expect("timed should still be active")
                .remaining_secs(),
            Some(2.0)
        );
        assert_eq!(effects.remaining_ratio("timed"), Some(1.0));
        assert_eq!(effects.remaining_ratio("indefinite"), None);
        assert!(effects.tick(1.0).is_empty());
        assert_eq!(
            effects
                .get("timed")
                .expect("timed should still be active")
                .remaining_secs(),
            Some(1.0)
        );
        assert_eq!(effects.remaining_ratio("timed"), Some(0.5));

        let expired = effects.tick(1.0);
        assert_eq!(expired.len(), 1);
        assert_eq!(expired[0].id().as_str(), "timed");
        assert!(!effects.has("timed"));
        assert!(effects.has("indefinite"));
    }

    #[test]
    fn active_effect_read_helpers_cover_finite_indefinite_inactive_and_missing() {
        let mut registry = EffectRegistry::default();
        registry
            .register(
                EffectDefinition::new("timed")
                    .duration_secs(8.0)
                    .max_stacks(3),
            )
            .expect("effect should register");
        registry
            .register(EffectDefinition::new("indefinite").max_stacks(2))
            .expect("effect should register");

        let mut effects = ActiveEffectSet::new();
        assert!(!effects.is_active("timed"));
        assert_eq!(effects.stacks("timed"), 0);
        assert_eq!(effects.remaining_secs("timed"), None);
        assert_eq!(effects.remaining_ratio("timed"), None);
        assert_eq!(effects.stacks("missing"), 0);
        assert_eq!(effects.remaining_secs("missing"), None);
        assert_eq!(effects.remaining_ratio("missing"), None);

        effects.apply("timed", &registry).expect("timed exists");
        effects.apply("timed", &registry).expect("timed exists");
        effects
            .apply("indefinite", &registry)
            .expect("indefinite exists");
        effects.tick(2.0);

        assert!(effects.is_active("timed"));
        assert_eq!(effects.stacks("timed"), 2);
        assert_eq!(effects.remaining_secs("timed"), Some(6.0));
        assert_eq!(effects.remaining_ratio("timed"), Some(0.75));
        assert!(effects.is_active("indefinite"));
        assert_eq!(effects.stacks("indefinite"), 1);
        assert_eq!(effects.remaining_secs("indefinite"), None);
        assert_eq!(effects.remaining_ratio("indefinite"), None);

        effects.remove("timed");
        assert!(!effects.is_active("timed"));
        assert_eq!(effects.stacks("timed"), 0);
        assert_eq!(effects.remaining_secs("timed"), None);
        assert_eq!(effects.remaining_ratio("timed"), None);
    }

    #[test]
    fn remaining_ratio_clamps_to_original_duration_bounds() {
        let expired = EffectInstance::with_runtime_state("expired", Some(10.0), Some(-2.0), 1);
        let overfull = EffectInstance::with_runtime_state("overfull", Some(10.0), Some(12.0), 1);
        let instant = EffectInstance::new("instant", Some(0.0), 1);
        let indefinite = EffectInstance::new("indefinite", None, 1);

        assert_eq!(expired.remaining_ratio(), Some(0.0));
        assert_eq!(overfull.remaining_ratio(), Some(1.0));
        assert_eq!(instant.remaining_ratio(), Some(0.0));
        assert_eq!(indefinite.remaining_ratio(), None);
    }

    #[test]
    fn effect_instance_deserializes_legacy_runtime_state_without_duration() {
        let instance: EffectInstance =
            serde_json::from_str(r#"{"id":"legacy_timed","remaining_secs":6.0,"stacks":2}"#)
                .expect("legacy effect instance should deserialize");

        assert_eq!(instance.id().as_str(), "legacy_timed");
        assert_eq!(instance.duration_secs(), Some(6.0));
        assert_eq!(instance.remaining_secs(), Some(6.0));
        assert_eq!(instance.remaining_ratio(), Some(1.0));
        assert_eq!(instance.stacks(), 2);
    }

    #[test]
    fn active_effect_set_removes_effects() {
        let mut registry = EffectRegistry::default();
        registry
            .register(EffectDefinition::new("low_oxygen"))
            .expect("effect should register");
        let mut effects = ActiveEffectSet::new();
        effects
            .apply("low_oxygen", &registry)
            .expect("effect should exist");

        let removed = effects
            .remove("low_oxygen")
            .expect("effect should be active");
        assert_eq!(removed.id().as_str(), "low_oxygen");
        assert!(effects.is_empty());
        assert!(effects.remove("low_oxygen").is_none());
    }

    #[test]
    fn snapshots_are_serializable_and_deterministic() {
        let mut registry = EffectRegistry::default();
        registry
            .register(
                EffectDefinition::new("low_oxygen")
                    .duration_secs(10.0)
                    .tag("space_suit")
                    .tag("survival")
                    .attribute("ui_hint", "warning"),
            )
            .expect("effect should register");
        registry
            .register(EffectDefinition::new("magnetic_boots").indefinite())
            .expect("effect should register");

        let definition_snapshot = registry.snapshot();
        assert_eq!(definition_snapshot[0].id.as_str(), "low_oxygen");
        assert_eq!(definition_snapshot[1].id.as_str(), "magnetic_boots");
        assert_eq!(
            definition_snapshot[0].tags,
            vec!["space_suit".to_owned(), "survival".to_owned()]
        );

        let mut effects = ActiveEffectSet::new();
        effects
            .apply("low_oxygen", &registry)
            .expect("effect should exist");
        effects
            .apply("magnetic_boots", &registry)
            .expect("effect should exist");
        let active_snapshot = effects.snapshot();
        assert_eq!(active_snapshot[0].id.as_str(), "low_oxygen");
        assert_eq!(active_snapshot[0].duration_secs, Some(10.0));
        assert_eq!(active_snapshot[0].remaining_secs, Some(10.0));
        assert_eq!(active_snapshot[1].id.as_str(), "magnetic_boots");
        assert_eq!(active_snapshot[1].duration_secs, None);
        assert_eq!(active_snapshot[1].remaining_secs, None);

        serde_json::to_string(&definition_snapshot).expect("definitions should serialize");
        serde_json::to_string(&active_snapshot).expect("active effects should serialize");
    }

    #[test]
    fn plugin_initializes_empty_resources_without_defaults() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(VoxelEffectsPlugin);

        assert!(app.world().resource::<EffectRegistry>().is_empty());
        assert!(app.world().resource::<ActiveEffectSet>().is_empty());
    }

    #[derive(Resource, Default)]
    struct ObservedApplied(Vec<EffectApplied>);

    #[derive(Resource, Default)]
    struct ObservedExpired(Vec<EffectExpired>);

    #[derive(Resource, Default)]
    struct ObservedErrors(Vec<EffectError>);

    #[derive(Component)]
    struct EffectSubject;

    fn register_test_effect(mut registry: ResMut<EffectRegistry>) {
        registry
            .register(
                EffectDefinition::new("low_oxygen")
                    .duration_secs(1.0)
                    .max_stacks(2),
            )
            .expect("effect should register");
    }

    fn record_applied(
        mut reader: MessageReader<EffectApplied>,
        mut observed: ResMut<ObservedApplied>,
    ) {
        observed.0.extend(reader.read().cloned());
    }

    fn record_expired(
        mut reader: MessageReader<EffectExpired>,
        mut observed: ResMut<ObservedExpired>,
    ) {
        observed.0.extend(reader.read().cloned());
    }

    fn apply_test_effect(
        registry: Res<EffectRegistry>,
        mut effects: ResMut<ActiveEffectSet>,
        mut applied: MessageWriter<EffectApplied>,
    ) {
        apply_effect("low_oxygen", &registry, &mut effects, &mut applied)
            .expect("effect should exist");
    }

    fn apply_missing_effect(
        registry: Res<EffectRegistry>,
        mut effects: ResMut<ActiveEffectSet>,
        mut applied: MessageWriter<EffectApplied>,
        mut errors: ResMut<ObservedErrors>,
    ) {
        let error = apply_effect("missing", &registry, &mut effects, &mut applied)
            .expect_err("missing effect should error");
        errors.0.push(error);
    }

    fn tick_test_effects(
        mut effects: ResMut<ActiveEffectSet>,
        mut expired: MessageWriter<EffectExpired>,
    ) {
        tick_effects(1.0, &mut effects, &mut expired);
    }

    fn apply_query_effect(
        registry: Res<EffectRegistry>,
        mut subjects: Query<&mut ActiveEffectSet, With<EffectSubject>>,
        mut applied: MessageWriter<EffectApplied>,
    ) {
        for mut effects in &mut subjects {
            apply_effect("low_oxygen", &registry, &mut effects, &mut applied)
                .expect("effect should exist");
        }
    }

    #[test]
    fn effect_helpers_emit_applied_messages() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(VoxelEffectsPlugin);
        app.init_resource::<ObservedApplied>();
        app.add_systems(Startup, register_test_effect);
        app.add_systems(Update, (apply_test_effect, record_applied).chain());

        app.update();

        let observed = app.world().resource::<ObservedApplied>();
        assert_eq!(observed.0.len(), 1);
        assert_eq!(observed.0[0].id.as_str(), "low_oxygen");
        assert_eq!(observed.0[0].previous, None);
        assert_eq!(observed.0[0].current.stacks(), 1);
    }

    #[test]
    fn effect_helpers_emit_expired_messages() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(VoxelEffectsPlugin);
        app.init_resource::<ObservedExpired>();
        app.add_systems(Startup, register_test_effect);
        app.add_systems(
            Update,
            (apply_test_effect, tick_test_effects, record_expired).chain(),
        );

        app.update();

        let observed = app.world().resource::<ObservedExpired>();
        assert_eq!(observed.0.len(), 1);
        assert_eq!(observed.0[0].id.as_str(), "low_oxygen");
        assert!(app.world().resource::<ActiveEffectSet>().is_empty());
    }

    #[test]
    fn helpers_return_missing_id_without_emitting_messages() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(VoxelEffectsPlugin);
        app.init_resource::<ObservedApplied>();
        app.init_resource::<ObservedErrors>();
        app.add_systems(Startup, register_test_effect);
        app.add_systems(Update, (apply_missing_effect, record_applied).chain());

        app.update();

        assert!(app.world().resource::<ObservedApplied>().0.is_empty());
        assert_eq!(
            app.world().resource::<ObservedErrors>().0,
            vec![EffectError::MissingId {
                id: EffectId::new("missing"),
            }]
        );
    }

    #[test]
    fn active_effect_set_can_be_attached_to_bevy_entities_as_component() {
        let mut registry = EffectRegistry::default();
        registry
            .register(EffectDefinition::new("low_oxygen").duration_secs(5.0))
            .expect("effect should register");

        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        let entity = app.world_mut().spawn(ActiveEffectSet::new()).id();
        app.world_mut()
            .get_mut::<ActiveEffectSet>(entity)
            .expect("entity should have effect set")
            .apply("low_oxygen", &registry)
            .expect("effect should exist");

        let effects = app
            .world()
            .get::<ActiveEffectSet>(entity)
            .expect("entity should have effect set");
        assert!(effects.has("low_oxygen"));
    }

    #[test]
    fn helpers_can_mutate_query_effect_sets() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(VoxelEffectsPlugin);
        app.init_resource::<ObservedApplied>();
        app.add_systems(Startup, register_test_effect);
        let entity = app
            .world_mut()
            .spawn((EffectSubject, ActiveEffectSet::new()))
            .id();
        app.add_systems(Update, (apply_query_effect, record_applied).chain());

        app.update();

        let effects = app
            .world()
            .get::<ActiveEffectSet>(entity)
            .expect("entity should have effect set");
        assert!(effects.has("low_oxygen"));
        assert_eq!(app.world().resource::<ObservedApplied>().0.len(), 1);
    }
}
