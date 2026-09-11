//! Author-defined, game-neutral HUD data model and stores.
//!
//! This module deliberately contains no gameplay-domain assumptions. Authors
//! register presentation definitions and publish already-resolved view values.

use std::collections::{HashMap, HashSet};
use std::fmt;

use bevy::asset::{AssetPath, LoadState};
use bevy::ecs::message::MessageWriter;
use bevy::prelude::*;
use serde::{Deserialize, Deserializer, Serialize};

/// Maximum number of retained children supported by a segments presentation.
pub const MAX_HUD_SEGMENTS: u32 = 64;

macro_rules! hud_id {
    ($name:ident, $description:literal) => {
        #[doc = $description]
        #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        pub struct $name(String);

        impl $name {
            /// Creates an id from any string-like value.
            pub fn new(id: impl Into<String>) -> Self {
                Self(id.into())
            }

            /// Returns the raw id string.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl From<&str> for $name {
            fn from(value: &str) -> Self {
                Self::new(value)
            }
        }

        impl From<String> for $name {
            fn from(value: String) -> Self {
                Self::new(value)
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                self.as_str()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.as_str())
            }
        }
    };
}

hud_id!(
    HudElementId,
    "Stable identifier for an author-defined HUD element."
);
hud_id!(
    HudValueId,
    "Stable identifier for an author-published HUD value."
);

/// A finite scalar with normalized, non-degenerate bounds.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct HudScalar {
    value: f32,
    min: f32,
    max: f32,
}

impl HudScalar {
    /// Creates a scalar, normalizing reversed bounds.
    ///
    /// Values outside the range remain intact; [`HudScalar::clamped_ratio`]
    /// performs the presentation clamp. Equal bounds are rejected because they
    /// cannot define a proportion.
    pub fn new(value: f32, min: f32, max: f32) -> Result<Self, HudValueError> {
        for (field, candidate) in [
            (HudScalarField::Value, value),
            (HudScalarField::Min, min),
            (HudScalarField::Max, max),
        ] {
            if !candidate.is_finite() {
                return Err(HudValueError::NonFinite { field });
            }
        }

        let (min, max) = if min <= max { (min, max) } else { (max, min) };
        if min == max {
            return Err(HudValueError::InvalidRange { min, max });
        }

        Ok(Self { value, min, max })
    }

    /// Returns the unmodified published value.
    pub fn value(self) -> f32 {
        self.value
    }

    /// Returns the normalized lower bound.
    pub fn min(self) -> f32 {
        self.min
    }

    /// Returns the normalized upper bound.
    pub fn max(self) -> f32 {
        self.max
    }

    /// Returns the value resolved to a bounded `0.0..=1.0` proportion.
    pub fn clamped_ratio(self) -> f32 {
        // Finite `f32` endpoints can still overflow their subtraction in
        // `f32` (for example, `f32::MIN..=f32::MAX`). Resolve in `f64` so the
        // validated finite inputs always produce a finite bounded ratio.
        (((self.value as f64 - self.min as f64) / (self.max as f64 - self.min as f64))
            .clamp(0.0, 1.0)) as f32
    }
}

#[derive(Deserialize)]
struct HudScalarRepresentation {
    value: f32,
    min: f32,
    max: f32,
}

impl<'de> Deserialize<'de> for HudScalar {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let scalar = HudScalarRepresentation::deserialize(deserializer)?;
        Self::new(scalar.value, scalar.min, scalar.max).map_err(serde::de::Error::custom)
    }
}

/// Scalar field associated with a validation failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HudScalarField {
    /// Published scalar value.
    Value,
    /// Lower range bound.
    Min,
    /// Upper range bound.
    Max,
}

impl fmt::Display for HudScalarField {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Value => "value",
            Self::Min => "min",
            Self::Max => "max",
        })
    }
}

/// Errors returned while constructing a HUD value.
#[derive(Debug, Clone, PartialEq)]
pub enum HudValueError {
    /// A scalar field was NaN or infinite.
    NonFinite { field: HudScalarField },
    /// Equal bounds cannot define a scalar proportion.
    InvalidRange { min: f32, max: f32 },
}

impl fmt::Display for HudValueError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonFinite { field } => write!(f, "HUD scalar {field} must be finite"),
            Self::InvalidRange { min, max } => {
                write!(
                    f,
                    "HUD scalar range `{min}..={max}` must have a nonzero span"
                )
            }
        }
    }
}

impl std::error::Error for HudValueError {}

/// Closed set of author-published, screen-ready HUD values.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum HudValue {
    /// Display-ready text.
    Text(String),
    /// Finite scalar with explicit bounds.
    Scalar(HudScalar),
    /// Unsigned count used by repeated segments or text.
    Count(u32),
    /// Boolean state, including conditional visibility bindings.
    Bool(bool),
    /// Owned path to an image asset.
    Icon(AssetPath<'static>),
}

impl HudValue {
    /// Returns this value's closed kind.
    pub fn kind(&self) -> HudValueKind {
        match self {
            Self::Text(_) => HudValueKind::Text,
            Self::Scalar(_) => HudValueKind::Scalar,
            Self::Count(_) => HudValueKind::Count,
            Self::Bool(_) => HudValueKind::Bool,
            Self::Icon(_) => HudValueKind::Icon,
        }
    }

    /// Creates an icon value while forcing the asset path to be owned.
    pub fn icon(path: impl Into<AssetPath<'static>>) -> Self {
        Self::Icon(path.into().into_owned())
    }
}

impl From<String> for HudValue {
    fn from(value: String) -> Self {
        Self::Text(value)
    }
}

impl From<&str> for HudValue {
    fn from(value: &str) -> Self {
        Self::Text(value.to_owned())
    }
}

impl From<HudScalar> for HudValue {
    fn from(value: HudScalar) -> Self {
        Self::Scalar(value)
    }
}

impl From<u32> for HudValue {
    fn from(value: u32) -> Self {
        Self::Count(value)
    }
}

impl From<bool> for HudValue {
    fn from(value: bool) -> Self {
        Self::Bool(value)
    }
}

impl From<AssetPath<'static>> for HudValue {
    fn from(value: AssetPath<'static>) -> Self {
        Self::Icon(value.into_owned())
    }
}

/// Discriminant for the closed [`HudValue`] vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HudValueKind {
    /// Display-ready text.
    Text,
    /// Finite scalar with explicit bounds.
    Scalar,
    /// Unsigned count.
    Count,
    /// Boolean state.
    Bool,
    /// Owned image asset path.
    Icon,
}

/// Text presentation bound to one published value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HudText {
    value: HudValueId,
    prefix: String,
    suffix: String,
    precision: Option<u8>,
}

impl HudText {
    /// Creates a text presentation bound to `value`.
    pub fn new(value: impl Into<HudValueId>) -> Self {
        Self {
            value: value.into(),
            prefix: String::new(),
            suffix: String::new(),
            precision: None,
        }
    }

    /// Sets text prepended to the resolved value.
    pub fn prefix(mut self, prefix: impl Into<String>) -> Self {
        self.prefix = prefix.into();
        self
    }

    /// Sets text appended to the resolved value.
    pub fn suffix(mut self, suffix: impl Into<String>) -> Self {
        self.suffix = suffix.into();
        self
    }

    /// Sets decimal precision used when formatting scalar values.
    pub fn precision(mut self, precision: u8) -> Self {
        self.precision = Some(precision);
        self
    }

    /// Returns the bound value id.
    pub fn value_id(&self) -> &HudValueId {
        &self.value
    }

    /// Returns the configured prefix.
    pub fn prefix_value(&self) -> &str {
        &self.prefix
    }

    /// Returns the configured suffix.
    pub fn suffix_value(&self) -> &str {
        &self.suffix
    }

    /// Returns the configured scalar precision, if any.
    pub fn precision_value(&self) -> Option<u8> {
        self.precision
    }
}

/// Continuous bar presentation bound to a scalar value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HudBar {
    value: HudValueId,
}

impl HudBar {
    /// Creates a continuous bar bound to `value`.
    pub fn new(value: impl Into<HudValueId>) -> Self {
        Self {
            value: value.into(),
        }
    }

    /// Returns the bound scalar value id.
    pub fn value_id(&self) -> &HudValueId {
        &self.value
    }
}

/// Static or value-bound icon source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum HudIconSource {
    /// Fixed image asset path.
    Asset(AssetPath<'static>),
    /// Id of an author-published [`HudValue::Icon`].
    Bound(HudValueId),
}

/// Icon presentation using an owned asset path or a bound icon value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HudIcon {
    source: HudIconSource,
}

impl HudIcon {
    /// Creates an icon with an owned fixed asset path.
    pub fn asset(path: impl Into<AssetPath<'static>>) -> Self {
        Self {
            source: HudIconSource::Asset(path.into().into_owned()),
        }
    }

    /// Creates an icon bound to an author-published icon value.
    pub fn bound(value: impl Into<HudValueId>) -> Self {
        Self {
            source: HudIconSource::Bound(value.into()),
        }
    }

    /// Returns the fixed or value-bound icon source.
    pub fn source(&self) -> &HudIconSource {
        &self.source
    }
}

/// Bounded repeated-segment presentation bound to a count value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HudSegments {
    value: HudValueId,
    capacity: u32,
    filled_asset: Option<AssetPath<'static>>,
    empty_asset: Option<AssetPath<'static>>,
}

impl HudSegments {
    /// Creates a repeated-segment presentation with a bounded child capacity.
    ///
    /// Capacity is validated when the containing definition is registered.
    pub fn new(value: impl Into<HudValueId>, capacity: u32) -> Self {
        Self {
            value: value.into(),
            capacity,
            filled_asset: None,
            empty_asset: None,
        }
    }

    /// Sets the owned asset path used for filled segments.
    pub fn filled_asset(mut self, path: impl Into<AssetPath<'static>>) -> Self {
        self.filled_asset = Some(path.into().into_owned());
        self
    }

    /// Sets the owned asset path used for empty segments.
    pub fn empty_asset(mut self, path: impl Into<AssetPath<'static>>) -> Self {
        self.empty_asset = Some(path.into().into_owned());
        self
    }

    /// Returns the bound count value id.
    pub fn value_id(&self) -> &HudValueId {
        &self.value
    }

    /// Returns the configured segment capacity.
    pub fn capacity(&self) -> u32 {
        self.capacity
    }

    /// Returns the filled-segment asset path, if configured.
    pub fn filled_asset_path(&self) -> Option<&AssetPath<'static>> {
        self.filled_asset.as_ref()
    }

    /// Returns the empty-segment asset path, if configured.
    pub fn empty_asset_path(&self) -> Option<&AssetPath<'static>> {
        self.empty_asset.as_ref()
    }
}

/// Supported retained HUD presentation kinds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum HudPresentation {
    /// Text resolved from text, scalar, count, or boolean values.
    Text(HudText),
    /// Continuous bar resolved from a scalar value.
    Bar(HudBar),
    /// Fixed or value-bound image icon.
    Icon(HudIcon),
    /// Bounded repeated segments resolved from a count value.
    Segments(HudSegments),
}

impl HudPresentation {
    /// Returns the dynamic value binding, if this presentation has one.
    pub fn value_id(&self) -> Option<&HudValueId> {
        match self {
            Self::Text(text) => Some(text.value_id()),
            Self::Bar(bar) => Some(bar.value_id()),
            Self::Icon(HudIcon {
                source: HudIconSource::Bound(value),
            }) => Some(value),
            Self::Icon(HudIcon {
                source: HudIconSource::Asset(_),
            }) => None,
            Self::Segments(segments) => Some(segments.value_id()),
        }
    }

    /// Returns whether this presentation can consume a value kind.
    pub fn accepts_kind(&self, kind: HudValueKind) -> bool {
        match self {
            Self::Text(_) => matches!(
                kind,
                HudValueKind::Text
                    | HudValueKind::Scalar
                    | HudValueKind::Count
                    | HudValueKind::Bool
            ),
            Self::Bar(_) => kind == HudValueKind::Scalar,
            Self::Icon(HudIcon {
                source: HudIconSource::Bound(_),
            }) => kind == HudValueKind::Icon,
            Self::Icon(HudIcon {
                source: HudIconSource::Asset(_),
            }) => false,
            Self::Segments(_) => kind == HudValueKind::Count,
        }
    }
}

/// One of the nine supported screen anchors.
#[derive(
    Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub enum HudAnchor {
    #[default]
    /// Top-left screen corner.
    TopLeft,
    /// Horizontal center of the top edge.
    TopCenter,
    /// Top-right screen corner.
    TopRight,
    /// Vertical center of the left edge.
    CenterLeft,
    /// Screen center.
    Center,
    /// Vertical center of the right edge.
    CenterRight,
    /// Bottom-left screen corner.
    BottomLeft,
    /// Horizontal center of the bottom edge.
    BottomCenter,
    /// Bottom-right screen corner.
    BottomRight,
}

/// Direction in which elements sharing an anchor are stacked.
#[derive(
    Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub enum HudStackDirection {
    /// Lay out elements from left to right.
    Horizontal,
    #[default]
    /// Lay out elements from top to bottom.
    Vertical,
}

/// Bounded placement vocabulary for one HUD element.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct HudPlacement {
    /// Screen location used as the element's layout anchor.
    pub anchor: HudAnchor,
    /// Logical-pixel offset from the anchor.
    pub offset: [f32; 2],
    /// Direction used to stack elements sharing the anchor.
    pub stack_direction: HudStackDirection,
}

impl HudPlacement {
    /// Creates default placement at `anchor`.
    pub fn new(anchor: HudAnchor) -> Self {
        Self {
            anchor,
            ..Self::default()
        }
    }

    /// Sets a logical-pixel offset from the anchor.
    pub fn offset(mut self, x: f32, y: f32) -> Self {
        self.offset = [x, y];
        self
    }

    /// Sets the stacking direction for this anchor group.
    pub fn stack_direction(mut self, direction: HudStackDirection) -> Self {
        self.stack_direction = direction;
        self
    }
}

impl Default for HudPlacement {
    fn default() -> Self {
        Self {
            anchor: HudAnchor::TopLeft,
            offset: [16.0, 16.0],
            stack_direction: HudStackDirection::Vertical,
        }
    }
}

/// Default visual values used by the retained HUD renderer.
#[derive(Debug, Clone, PartialEq, Resource, Serialize, Deserialize)]
pub struct HudTheme {
    /// Default text size in logical pixels.
    pub font_size: f32,
    /// Default linear RGBA text color.
    pub text_color: [f32; 4],
    /// Default linear RGBA background color.
    pub background_color: [f32; 4],
    /// Default linear RGBA foreground color.
    pub foreground_color: [f32; 4],
    /// Default logical-pixel padding in left, right, top, bottom order.
    pub padding: [f32; 4],
    /// Default logical-pixel gap between retained children.
    pub gap: f32,
    /// Default bar width and height in logical pixels.
    pub bar_size: [f32; 2],
    /// Default icon width and height in logical pixels.
    pub icon_size: [f32; 2],
    /// Default segment width and height in logical pixels.
    pub segment_size: [f32; 2],
    /// Default UI z-index; loading and debug overlays use higher values.
    pub z_index: i32,
}

impl Default for HudTheme {
    fn default() -> Self {
        Self {
            font_size: 18.0,
            text_color: [1.0, 1.0, 1.0, 1.0],
            background_color: [0.0, 0.0, 0.0, 0.65],
            foreground_color: [0.2, 0.8, 1.0, 1.0],
            padding: [8.0, 8.0, 6.0, 6.0],
            gap: 6.0,
            bar_size: [220.0, 18.0],
            icon_size: [32.0, 32.0],
            segment_size: [18.0, 18.0],
            z_index: 50,
        }
    }
}

/// Narrow optional overrides corresponding to [`HudTheme`] fields.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct HudStyleOverride {
    /// Optional text size override in logical pixels.
    pub font_size: Option<f32>,
    /// Optional linear RGBA text-color override.
    pub text_color: Option<[f32; 4]>,
    /// Optional linear RGBA background-color override.
    pub background_color: Option<[f32; 4]>,
    /// Optional linear RGBA foreground-color override.
    pub foreground_color: Option<[f32; 4]>,
    /// Optional logical-pixel padding override in left, right, top, bottom order.
    pub padding: Option<[f32; 4]>,
    /// Optional logical-pixel child-gap override.
    pub gap: Option<f32>,
    /// Optional logical-pixel bar size override.
    pub bar_size: Option<[f32; 2]>,
    /// Optional logical-pixel icon size override.
    pub icon_size: Option<[f32; 2]>,
    /// Optional logical-pixel segment size override.
    pub segment_size: Option<[f32; 2]>,
    /// Optional UI z-index override.
    pub z_index: Option<i32>,
}

/// Author-defined presentation and placement for one HUD element.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HudElementDefinition {
    id: HudElementId,
    presentation: HudPresentation,
    placement: HudPlacement,
    order: i32,
    visible_when: Option<HudValueId>,
    fallback: Option<HudValue>,
    style: HudStyleOverride,
}

impl HudElementDefinition {
    /// Creates a definition with default placement, order, visibility, and style.
    pub fn new(id: impl Into<HudElementId>, presentation: HudPresentation) -> Self {
        Self {
            id: id.into(),
            presentation,
            placement: HudPlacement::default(),
            order: 0,
            visible_when: None,
            fallback: None,
            style: HudStyleOverride::default(),
        }
    }

    /// Sets the element's screen placement.
    pub fn placement(mut self, placement: HudPlacement) -> Self {
        self.placement = placement;
        self
    }

    /// Sets signed ordering within the anchor group.
    pub fn order(mut self, order: i32) -> Self {
        self.order = order;
        self
    }

    /// Makes visibility conditional on a published boolean value.
    pub fn visible_when(mut self, value: impl Into<HudValueId>) -> Self {
        self.visible_when = Some(value.into());
        self
    }

    /// Sets a compatible value used when the primary binding cannot resolve.
    pub fn fallback(mut self, value: impl Into<HudValue>) -> Self {
        self.fallback = Some(value.into());
        self
    }

    /// Sets narrow visual overrides for this element.
    pub fn style(mut self, style: HudStyleOverride) -> Self {
        self.style = style;
        self
    }

    /// Returns the stable element id.
    pub fn id(&self) -> &HudElementId {
        &self.id
    }

    /// Returns the element presentation.
    pub fn presentation(&self) -> &HudPresentation {
        &self.presentation
    }

    /// Returns the element placement.
    pub fn placement_value(&self) -> HudPlacement {
        self.placement
    }

    /// Returns signed ordering within the anchor group.
    pub fn order_value(&self) -> i32 {
        self.order
    }

    /// Returns the optional boolean visibility binding.
    pub fn visibility_value_id(&self) -> Option<&HudValueId> {
        self.visible_when.as_ref()
    }

    /// Returns the optional compatible fallback value.
    pub fn fallback_value(&self) -> Option<&HudValue> {
        self.fallback.as_ref()
    }

    /// Returns this element's narrow style overrides.
    pub fn style_override(&self) -> &HudStyleOverride {
        &self.style
    }

    /// Creates a serializable, definition-only inspection snapshot.
    pub fn snapshot(&self) -> HudElementSnapshot {
        HudElementSnapshot {
            id: self.id.clone(),
            presentation: self.presentation.clone(),
            placement: self.placement,
            order: self.order,
            visible_when: self.visible_when.clone(),
            fallback: self.fallback.clone(),
            style: self.style.clone(),
        }
    }
}

/// Serializable inspection snapshot of one HUD element definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HudElementSnapshot {
    /// Stable element id.
    pub id: HudElementId,
    /// Presentation definition.
    pub presentation: HudPresentation,
    /// Screen placement.
    pub placement: HudPlacement,
    /// Signed ordering within the anchor group.
    pub order: i32,
    /// Optional boolean visibility binding.
    pub visible_when: Option<HudValueId>,
    /// Optional compatible fallback value.
    pub fallback: Option<HudValue>,
    /// Narrow visual overrides.
    pub style: HudStyleOverride,
}

/// Serializable inspection snapshot of one published HUD value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HudValueSnapshot {
    /// Stable published value id.
    pub id: HudValueId,
    /// Screen-ready published value.
    pub value: HudValue,
}

/// Definition validation reason returned through [`HudRegistryError`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HudDefinitionError {
    /// A fallback value cannot be consumed by the presentation.
    InvalidFallbackKind {
        /// Human-readable set of accepted kinds.
        expected: &'static str,
        /// Actual fallback kind.
        actual: HudValueKind,
    },
    /// A fixed icon cannot use a value fallback because it has no binding.
    StaticIconFallback,
    /// Segment capacity is zero or exceeds [`MAX_HUD_SEGMENTS`].
    InvalidSegmentCapacity {
        /// Rejected capacity.
        capacity: u32,
        /// Maximum supported capacity.
        maximum: u32,
    },
    /// At least one placement offset is NaN or infinite.
    NonFinitePlacement,
    /// A style field contains a non-finite or disallowed negative value.
    InvalidStyleValue {
        /// Name of the rejected field.
        field: &'static str,
    },
}

impl fmt::Display for HudDefinitionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidFallbackKind { expected, actual } => {
                write!(
                    f,
                    "fallback kind {actual:?} is incompatible; expected {expected}"
                )
            }
            Self::StaticIconFallback => {
                f.write_str("a static icon has no value binding and cannot use a fallback")
            }
            Self::InvalidSegmentCapacity { capacity, maximum } => write!(
                f,
                "segment capacity {capacity} must be within 1..={maximum}"
            ),
            Self::NonFinitePlacement => f.write_str("HUD placement offsets must be finite"),
            Self::InvalidStyleValue { field } => {
                write!(
                    f,
                    "HUD style field `{field}` must be finite and non-negative"
                )
            }
        }
    }
}

/// Errors returned by [`HudRegistry`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HudRegistryError {
    /// An element already exists under this id.
    DuplicateId {
        /// Duplicate element id.
        id: HudElementId,
    },
    /// No element exists under this id.
    MissingId {
        /// Missing element id.
        id: HudElementId,
    },
    /// The definition violates a model invariant.
    InvalidDefinition {
        /// Invalid element id.
        id: HudElementId,
        /// Specific invariant violation.
        reason: HudDefinitionError,
    },
}

impl fmt::Display for HudRegistryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateId { id } => write!(f, "HUD element id `{id}` is already registered"),
            Self::MissingId { id } => write!(f, "HUD element id `{id}` is not registered"),
            Self::InvalidDefinition { id, reason } => {
                write!(f, "HUD element `{id}` is invalid: {reason}")
            }
        }
    }
}

impl std::error::Error for HudRegistryError {}

/// Author-defined HUD element registry with revision-based change tracking.
#[derive(Debug, Clone, Default, Resource)]
pub struct HudRegistry {
    elements_by_id: HashMap<HudElementId, HudElementDefinition>,
    revision: u64,
}

impl HudRegistry {
    /// Creates an empty registry at revision zero.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a valid definition under a previously unused id.
    ///
    /// Duplicate ids return [`HudRegistryError::DuplicateId`] without changing
    /// the registry or its revision. Use [`HudRegistry::insert_or_replace`] for
    /// intentional replacement.
    pub fn register(&mut self, definition: HudElementDefinition) -> Result<(), HudRegistryError> {
        if self.elements_by_id.contains_key(definition.id()) {
            return Err(HudRegistryError::DuplicateId {
                id: definition.id().clone(),
            });
        }
        validate_definition(&definition)?;
        self.elements_by_id
            .insert(definition.id().clone(), definition);
        bump_revision(&mut self.revision);
        Ok(())
    }

    /// Inserts a valid definition or replaces a different definition with the same id.
    ///
    /// An identical definition is a no-op. The returned value is the previous
    /// different definition when replacement occurred; new insertion and an
    /// identical no-op both return `None`.
    pub fn insert_or_replace(
        &mut self,
        definition: HudElementDefinition,
    ) -> Result<Option<HudElementDefinition>, HudRegistryError> {
        validate_definition(&definition)?;
        if self.elements_by_id.get(definition.id()) == Some(&definition) {
            return Ok(None);
        }
        let previous = self
            .elements_by_id
            .insert(definition.id().clone(), definition);
        bump_revision(&mut self.revision);
        Ok(previous)
    }

    /// Removes and returns a definition, or reports a missing id.
    pub fn remove(
        &mut self,
        id: impl AsRef<str>,
    ) -> Result<HudElementDefinition, HudRegistryError> {
        let id = HudElementId::new(id.as_ref());
        let definition = self
            .elements_by_id
            .remove(&id)
            .ok_or_else(|| HudRegistryError::MissingId { id: id.clone() })?;
        bump_revision(&mut self.revision);
        Ok(definition)
    }

    /// Returns a definition by element id.
    pub fn get(&self, id: impl AsRef<str>) -> Option<&HudElementDefinition> {
        self.elements_by_id.get(&HudElementId::new(id.as_ref()))
    }

    /// Returns a definition by id or an explicit missing-id error.
    pub fn require(&self, id: impl AsRef<str>) -> Result<&HudElementDefinition, HudRegistryError> {
        let id = HudElementId::new(id.as_ref());
        self.elements_by_id
            .get(&id)
            .ok_or(HudRegistryError::MissingId { id })
    }

    /// Returns whether an element id is registered.
    pub fn contains_id(&self, id: impl AsRef<str>) -> bool {
        self.get(id).is_some()
    }

    /// Returns the number of registered definitions.
    pub fn len(&self) -> usize {
        self.elements_by_id.len()
    }

    /// Returns whether the registry contains no definitions.
    pub fn is_empty(&self) -> bool {
        self.elements_by_id.is_empty()
    }

    /// Returns the current monotonic change revision.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Iterates definitions in unspecified hash-map order.
    pub fn iter(&self) -> impl Iterator<Item = &HudElementDefinition> {
        self.elements_by_id.values()
    }

    /// Returns deterministic id-ordered inspection snapshots.
    pub fn snapshot(&self) -> Vec<HudElementSnapshot> {
        let mut elements: Vec<_> = self.iter().map(HudElementDefinition::snapshot).collect();
        elements.sort_by(|left, right| left.id.cmp(&right.id));
        elements
    }
}

/// Author-published screen-ready HUD values with revision-based change tracking.
#[derive(Debug, Clone, Default, Resource)]
pub struct HudValueSet {
    values_by_id: HashMap<HudValueId, HudValue>,
    revision: u64,
}

impl HudValueSet {
    /// Creates an empty value set at revision zero.
    pub fn new() -> Self {
        Self::default()
    }

    /// Inserts or replaces a value. Equal replacement is a no-op.
    ///
    /// Returns the previous different value on replacement. New insertion and
    /// an identical no-op both return `None`; use [`set_hud_value`] when the
    /// caller needs an explicit changed flag and message emission.
    pub fn insert_or_replace(
        &mut self,
        id: impl Into<HudValueId>,
        value: impl Into<HudValue>,
    ) -> Option<HudValue> {
        self.set_internal(id.into(), value.into()).0
    }

    /// Removes and returns a value, or returns `None` without changing revision.
    pub fn remove(&mut self, id: impl AsRef<str>) -> Option<HudValue> {
        let removed = self.values_by_id.remove(&HudValueId::new(id.as_ref()));
        if removed.is_some() {
            bump_revision(&mut self.revision);
        }
        removed
    }

    /// Returns a published value by id.
    pub fn get(&self, id: impl AsRef<str>) -> Option<&HudValue> {
        self.values_by_id.get(&HudValueId::new(id.as_ref()))
    }

    /// Returns whether a value id is present.
    pub fn contains_id(&self, id: impl AsRef<str>) -> bool {
        self.get(id).is_some()
    }

    /// Returns the number of published values.
    pub fn len(&self) -> usize {
        self.values_by_id.len()
    }

    /// Returns whether no values are published.
    pub fn is_empty(&self) -> bool {
        self.values_by_id.is_empty()
    }

    /// Returns the current monotonic change revision.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Iterates ids and values in unspecified hash-map order.
    pub fn iter(&self) -> impl Iterator<Item = (&HudValueId, &HudValue)> {
        self.values_by_id.iter()
    }

    /// Returns deterministic id-ordered inspection snapshots.
    pub fn snapshot(&self) -> Vec<HudValueSnapshot> {
        let mut values: Vec<_> = self
            .iter()
            .map(|(id, value)| HudValueSnapshot {
                id: id.clone(),
                value: value.clone(),
            })
            .collect();
        values.sort_by(|left, right| left.id.cmp(&right.id));
        values
    }

    fn set_internal(&mut self, id: HudValueId, value: HudValue) -> (Option<HudValue>, bool) {
        if self.values_by_id.get(&id) == Some(&value) {
            return (None, false);
        }
        let previous = self.values_by_id.insert(id, value);
        bump_revision(&mut self.revision);
        (previous, true)
    }
}

/// Message emitted when a helper changes or removes a published value.
#[derive(Message, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HudValueChanged {
    /// Stable id of the changed value.
    pub id: HudValueId,
    /// Previous value, or `None` for first publication.
    pub previous: Option<HudValue>,
    /// Current value, or `None` after removal.
    pub current: Option<HudValue>,
}

/// Current resolution problem for one HUD element.
///
/// Diagnostics describe author input without attaching game meaning to ids or
/// values. The renderer emits them only when an element enters a different
/// failure state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HudDiagnosticKind {
    /// A required published value is absent.
    MissingValue {
        /// Missing value id.
        value: HudValueId,
        /// How the element uses the missing value.
        binding: HudBindingKind,
    },
    /// A published value has a kind the binding cannot consume.
    MismatchedValue {
        /// Mismatched value id.
        value: HudValueId,
        /// How the element uses the mismatched value.
        binding: HudBindingKind,
        /// Actual published kind.
        actual: HudValueKind,
    },
    /// A presentation received no consumable value after validation.
    InvalidValue {
        /// How the element attempted to use the value.
        binding: HudBindingKind,
        /// Actual kind when a value was present.
        actual: Option<HudValueKind>,
    },
    /// A definition bypassed registry validation or became otherwise invalid.
    InvalidDefinition {
        /// Validation failure.
        reason: HudDefinitionError,
    },
    /// A merged theme/style field cannot be rendered safely.
    InvalidStyle {
        /// Name of the invalid resolved field.
        field: &'static str,
    },
    /// A segment count exceeds its registered capacity.
    ExcessiveSegments {
        /// Published segment count.
        count: u32,
        /// Definition capacity.
        capacity: u32,
    },
    /// A configured image path is not currently available.
    AssetUnavailable {
        /// Owned unavailable path.
        path: AssetPath<'static>,
    },
}

/// Role of a published value in HUD resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HudBindingKind {
    /// Value consumed by the element presentation.
    Presentation,
    /// Boolean value controlling element visibility.
    Visibility,
}

/// Message emitted when an element enters a new resolution failure state.
#[derive(Message, Debug, Clone, PartialEq, Eq)]
pub struct HudDiagnostic {
    /// Element whose resolution failed.
    pub element: HudElementId,
    /// Newly active failure state.
    pub kind: HudDiagnosticKind,
}

/// Public scheduling boundary for author publication and retained HUD updates.
///
/// Put systems that call [`set_hud_value`] or [`remove_hud_value`] in
/// [`HudSystemSet::Publish`] when their changes must be visible in the same
/// update. Reconciliation and rendering are ordered after that set.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HudSystemSet {
    /// Author-owned systems publish screen-ready values here.
    Publish,
    /// Retained entity structure is reconciled after publication.
    Reconcile,
    /// Changed resolved state is applied after deferred reconciliation.
    Render,
}

/// Explicit, domain-neutral retained renderer for author-defined HUD elements.
///
/// The plugin is intentionally independent from [`crate::ui::UiPlugin`] and
/// every convenience bundle. It can run with `MinimalPlugins`, initializes an
/// empty HUD, and performs no asset I/O unless a registered presentation
/// actually references an image path and an [`AssetServer`] is available.
pub struct VoxelHudPlugin;

impl Plugin for VoxelHudPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<HudRegistry>()
            .init_resource::<HudValueSet>()
            .init_resource::<HudTheme>()
            .init_resource::<HudRuntimeState>()
            .init_resource::<HudDiagnosticTracker>()
            .init_resource::<HudAssetCache>()
            .add_message::<HudValueChanged>()
            .add_message::<HudDiagnostic>()
            .configure_sets(
                Update,
                (
                    HudSystemSet::Publish,
                    HudSystemSet::Reconcile,
                    HudSystemSet::Render,
                )
                    .chain(),
            )
            .configure_sets(
                PostStartup,
                (
                    HudSystemSet::Publish,
                    HudSystemSet::Reconcile,
                    HudSystemSet::Render,
                )
                    .chain(),
            )
            .add_systems(
                PostStartup,
                (
                    reconcile_hud_entities.in_set(HudSystemSet::Reconcile),
                    ApplyDeferred,
                    render_hud_entities.in_set(HudSystemSet::Render),
                )
                    .chain(),
            )
            .add_systems(
                Update,
                (
                    reconcile_hud_entities.in_set(HudSystemSet::Reconcile),
                    ApplyDeferred,
                    render_hud_entities.in_set(HudSystemSet::Render),
                )
                    .chain(),
            );
    }
}

#[derive(Component)]
struct HudCanvas;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct HudAnchorKey {
    anchor: HudAnchor,
    direction: HudStackDirection,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
struct HudAnchorRoot(HudAnchorKey);

#[derive(Component, Debug, Clone, PartialEq, Eq)]
struct HudElementRoot(HudElementId);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HudTreeKind {
    Text,
    Bar,
    Icon,
    Segments(u32),
}

impl HudTreeKind {
    fn from_presentation(presentation: &HudPresentation) -> Self {
        match presentation {
            HudPresentation::Text(_) => Self::Text,
            HudPresentation::Bar(_) => Self::Bar,
            HudPresentation::Icon(_) => Self::Icon,
            HudPresentation::Segments(segments) => Self::Segments(segments.capacity()),
        }
    }
}

#[derive(Component, Debug, Clone, PartialEq)]
struct HudDefinitionFingerprint {
    definition: HudElementSnapshot,
    tree: HudTreeKind,
}

#[derive(Component, Debug, Default, Clone, PartialEq)]
struct HudResolvedFingerprint(Option<ResolvedHudElement>);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HudChildKind {
    Text,
    BarTrack,
    BarFill,
    Icon,
    Segment(u32),
}

#[derive(Component, Debug, Clone, PartialEq, Eq)]
struct HudChildIdentity {
    element: HudElementId,
    kind: HudChildKind,
}

#[derive(Component, Debug, Default, Clone, PartialEq)]
struct HudCachedImage {
    path: Option<AssetPath<'static>>,
    handle: Option<Handle<Image>>,
}

#[derive(Resource, Debug, Default)]
struct HudRuntimeState {
    registry_revision: Option<u64>,
    value_revision: Option<u64>,
    render_all: bool,
    pending_assets: bool,
    asset_server_present: bool,
}

#[derive(Resource, Debug, Default)]
struct HudAssetCache {
    handles: HashMap<AssetPath<'static>, Handle<Image>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HudAxisAnchor {
    Start,
    Center,
    End,
}

#[derive(Debug, Clone, PartialEq)]
struct ResolvedHudLayout {
    anchor: HudAnchor,
    horizontal: HudAxisAnchor,
    vertical: HudAxisAnchor,
    offset: [f32; 2],
    stack_direction: HudStackDirection,
    order: i32,
}

#[derive(Debug, Clone, PartialEq)]
struct ResolvedHudStyle {
    font_size: f32,
    text_color: [f32; 4],
    background_color: [f32; 4],
    foreground_color: [f32; 4],
    padding: [f32; 4],
    gap: f32,
    bar_size: [f32; 2],
    icon_size: [f32; 2],
    segment_size: [f32; 2],
    z_index: i32,
}

#[derive(Debug, Clone, PartialEq)]
enum ResolvedHudPresentation {
    Text(String),
    Bar {
        ratio: f32,
    },
    Icon {
        path: Option<AssetPath<'static>>,
    },
    Segments {
        filled: u32,
        capacity: u32,
        filled_asset: Option<AssetPath<'static>>,
        empty_asset: Option<AssetPath<'static>>,
    },
}

#[derive(Debug, Clone, PartialEq)]
struct ResolvedHudElement {
    id: HudElementId,
    visible: bool,
    used_fallback: bool,
    layout: ResolvedHudLayout,
    style: ResolvedHudStyle,
    presentation: ResolvedHudPresentation,
}

#[derive(Resource, Debug, Default)]
struct HudDiagnosticTracker {
    active_by_element: HashMap<HudElementId, HudDiagnosticKind>,
}

impl HudDiagnosticTracker {
    fn transition(
        &mut self,
        element: &HudElementId,
        current: Option<HudDiagnosticKind>,
    ) -> Option<HudDiagnostic> {
        match current {
            Some(kind) if self.active_by_element.get(element) == Some(&kind) => None,
            Some(kind) => {
                self.active_by_element.insert(element.clone(), kind.clone());
                Some(HudDiagnostic {
                    element: element.clone(),
                    kind,
                })
            }
            None => {
                self.active_by_element.remove(element);
                None
            }
        }
    }

    fn retain_elements(&mut self, ids: &HashSet<HudElementId>) {
        self.active_by_element.retain(|id, _| ids.contains(id));
    }
}

#[derive(Debug, Default)]
struct ResolvedHud {
    elements: Vec<ResolvedHudElement>,
    diagnostics: Vec<HudDiagnostic>,
}

/// Publishes a value and emits [`HudValueChanged`] only when stored state changes.
pub fn set_hud_value(
    id: impl Into<HudValueId>,
    value: impl Into<HudValue>,
    values: &mut HudValueSet,
    changed: &mut MessageWriter<HudValueChanged>,
) -> bool {
    let id = id.into();
    let value = value.into();
    let (previous, did_change) = values.set_internal(id.clone(), value.clone());
    if did_change {
        changed.write(HudValueChanged {
            id,
            previous,
            current: Some(value),
        });
    }
    did_change
}

/// Removes a value and emits [`HudValueChanged`] only when it existed.
pub fn remove_hud_value(
    id: impl AsRef<str>,
    values: &mut HudValueSet,
    changed: &mut MessageWriter<HudValueChanged>,
) -> bool {
    let id = HudValueId::new(id.as_ref());
    let Some(previous) = values.remove(id.as_str()) else {
        return false;
    };
    changed.write(HudValueChanged {
        id,
        previous: Some(previous),
        current: None,
    });
    true
}

const HUD_ANCHORS: [HudAnchor; 9] = [
    HudAnchor::TopLeft,
    HudAnchor::TopCenter,
    HudAnchor::TopRight,
    HudAnchor::CenterLeft,
    HudAnchor::Center,
    HudAnchor::CenterRight,
    HudAnchor::BottomLeft,
    HudAnchor::BottomCenter,
    HudAnchor::BottomRight,
];
const HUD_STACK_DIRECTIONS: [HudStackDirection; 2] =
    [HudStackDirection::Horizontal, HudStackDirection::Vertical];

fn hud_anchor_keys() -> impl Iterator<Item = HudAnchorKey> {
    HUD_ANCHORS.into_iter().flat_map(|anchor| {
        HUD_STACK_DIRECTIONS
            .into_iter()
            .map(move |direction| HudAnchorKey { anchor, direction })
    })
}

fn reconcile_hud_entities(
    mut commands: Commands,
    registry: Res<HudRegistry>,
    theme: Res<HudTheme>,
    mut runtime: ResMut<HudRuntimeState>,
    canvases: Query<(Entity, Option<&Children>), With<HudCanvas>>,
    anchors: Query<(Entity, &HudAnchorRoot, Option<&Children>, Option<&ChildOf>)>,
    roots: Query<(
        Entity,
        &HudElementRoot,
        &HudDefinitionFingerprint,
        Option<&Children>,
    )>,
    child_identities: Query<&HudChildIdentity>,
) {
    let structure_missing = canvases.iter().count() != 1 || anchors.iter().count() != 18;
    if runtime.registry_revision == Some(registry.revision())
        && !theme.is_changed()
        && !structure_missing
    {
        return;
    }

    let mut canvas_entities: Vec<_> = canvases
        .iter()
        .map(|(entity, children)| (entity, children.map(|children| children.to_vec())))
        .collect();
    canvas_entities.sort_by_key(|(entity, _)| entity.to_bits());
    let canvas_children = canvas_entities
        .first()
        .and_then(|(_, children)| children.clone())
        .unwrap_or_default();
    let canvas = canvas_entities
        .first()
        .map(|(entity, _)| *entity)
        .unwrap_or_else(|| {
            commands
                .spawn((
                    HudCanvas,
                    Node {
                        position_type: PositionType::Absolute,
                        width: Val::Percent(100.0),
                        height: Val::Percent(100.0),
                        ..default()
                    },
                ))
                .id()
        });
    for (duplicate, children) in canvas_entities.into_iter().skip(1) {
        for child in children.into_iter().flatten() {
            commands.entity(canvas).add_child(child);
        }
        commands.entity(duplicate).despawn();
    }

    let mut anchors_by_kind: HashMap<HudAnchorKey, Vec<(Entity, Vec<Entity>, Option<Entity>)>> =
        HashMap::new();
    for (entity, anchor, children, parent) in &anchors {
        anchors_by_kind.entry(anchor.0).or_default().push((
            entity,
            children.map_or_else(Vec::new, |children| children.to_vec()),
            parent.map(ChildOf::parent),
        ));
    }

    let mut anchor_entities = HashMap::new();
    let mut anchor_children = HashMap::new();
    for key in hud_anchor_keys() {
        let mut candidates = anchors_by_kind.remove(&key).unwrap_or_default();
        candidates.sort_by_key(|(entity, _, _)| entity.to_bits());
        let anchor_entity = candidates
            .first()
            .map(|(entity, _, _)| *entity)
            .unwrap_or_else(|| {
                commands
                    .spawn((HudAnchorRoot(key), anchor_node(key), ChildOf(canvas)))
                    .id()
            });
        anchor_children.insert(
            key,
            candidates
                .first()
                .map(|(_, children, _)| children.clone())
                .unwrap_or_default(),
        );

        if candidates.first().and_then(|(_, _, parent)| *parent) != Some(canvas) {
            commands.entity(canvas).add_child(anchor_entity);
        }
        for (duplicate, children, _) in candidates.into_iter().skip(1) {
            for child in children {
                commands.entity(anchor_entity).add_child(child);
            }
            commands.entity(duplicate).despawn();
        }
        anchor_entities.insert(key, anchor_entity);
    }
    let ordered_anchors: Vec<_> = hud_anchor_keys().map(|key| anchor_entities[&key]).collect();
    if canvas_children != ordered_anchors {
        commands.entity(canvas).replace_children(&ordered_anchors);
    }

    let mut root_candidates: HashMap<
        HudElementId,
        Vec<(Entity, HudDefinitionFingerprint, Vec<Entity>)>,
    > = HashMap::new();
    for (entity, root, fingerprint, children) in &roots {
        root_candidates.entry(root.0.clone()).or_default().push((
            entity,
            fingerprint.clone(),
            children.map_or_else(Vec::new, |children| children.to_vec()),
        ));
    }
    for candidates in root_candidates.values_mut() {
        candidates.sort_by_key(|(entity, _, _)| entity.to_bits());
    }

    let mut definitions: Vec<_> = registry.iter().collect();
    definitions.sort_by(|left, right| {
        left.placement
            .anchor
            .cmp(&right.placement.anchor)
            .then_with(|| left.order.cmp(&right.order))
            .then_with(|| left.id.cmp(&right.id))
    });
    let active_ids: HashSet<_> = definitions
        .iter()
        .map(|definition| definition.id().clone())
        .collect();

    for (id, candidates) in &root_candidates {
        if !active_ids.contains(id) {
            for (entity, _, _) in candidates {
                commands.entity(*entity).despawn();
            }
        } else {
            for (entity, _, _) in candidates.iter().skip(1) {
                commands.entity(*entity).despawn();
            }
        }
    }

    let mut roots_by_id = HashMap::new();
    for definition in &definitions {
        let id = definition.id().clone();
        let tree = HudTreeKind::from_presentation(definition.presentation());
        let snapshot = definition.snapshot();
        let root = if let Some((entity, old_fingerprint, children)) = root_candidates
            .get(&id)
            .and_then(|candidates| candidates.first())
        {
            reconcile_hud_tree(
                &mut commands,
                *entity,
                &id,
                old_fingerprint.tree,
                tree,
                children,
                &child_identities,
            );
            if old_fingerprint.definition != snapshot || old_fingerprint.tree != tree {
                commands.entity(*entity).insert(HudDefinitionFingerprint {
                    definition: snapshot,
                    tree,
                });
            }
            *entity
        } else {
            let entity = commands
                .spawn((
                    HudElementRoot(id.clone()),
                    HudDefinitionFingerprint {
                        definition: snapshot,
                        tree,
                    },
                    HudResolvedFingerprint::default(),
                    Node::default(),
                    UiTransform::default(),
                    BackgroundColor(Color::NONE),
                    Visibility::Hidden,
                    ZIndex(0),
                ))
                .id();
            spawn_hud_tree(&mut commands, entity, &id, tree);
            entity
        };
        roots_by_id.insert(id, root);
    }

    let mut desired_by_anchor: HashMap<HudAnchorKey, Vec<Entity>> = HashMap::new();
    for definition in definitions {
        let placement = definition.placement_value();
        desired_by_anchor
            .entry(HudAnchorKey {
                anchor: placement.anchor,
                direction: placement.stack_direction,
            })
            .or_default()
            .push(roots_by_id[definition.id()]);
    }
    for key in hud_anchor_keys() {
        let desired = desired_by_anchor.remove(&key).unwrap_or_default();
        if anchor_children[&key] != desired {
            commands
                .entity(anchor_entities[&key])
                .replace_children(&desired);
        }
    }

    runtime.registry_revision = Some(registry.revision());
    runtime.render_all = true;
}

fn anchor_node(key: HudAnchorKey) -> Node {
    let horizontal = match key.anchor {
        HudAnchor::TopLeft | HudAnchor::CenterLeft | HudAnchor::BottomLeft => HudAxisAnchor::Start,
        HudAnchor::TopCenter | HudAnchor::Center | HudAnchor::BottomCenter => HudAxisAnchor::Center,
        HudAnchor::TopRight | HudAnchor::CenterRight | HudAnchor::BottomRight => HudAxisAnchor::End,
    };
    let vertical = match key.anchor {
        HudAnchor::TopLeft | HudAnchor::TopCenter | HudAnchor::TopRight => HudAxisAnchor::Start,
        HudAnchor::CenterLeft | HudAnchor::Center | HudAnchor::CenterRight => HudAxisAnchor::Center,
        HudAnchor::BottomLeft | HudAnchor::BottomCenter | HudAnchor::BottomRight => {
            HudAxisAnchor::End
        }
    };
    let axis_to_align = |axis| match axis {
        HudAxisAnchor::Start => AlignItems::FlexStart,
        HudAxisAnchor::Center => AlignItems::Center,
        HudAxisAnchor::End => AlignItems::FlexEnd,
    };
    let axis_to_justify = |axis| match axis {
        HudAxisAnchor::Start => JustifyContent::FlexStart,
        HudAxisAnchor::Center => JustifyContent::Center,
        HudAxisAnchor::End => JustifyContent::FlexEnd,
    };
    let (align_items, justify_content) = match key.direction {
        HudStackDirection::Horizontal => (axis_to_align(vertical), axis_to_justify(horizontal)),
        HudStackDirection::Vertical => (axis_to_align(horizontal), axis_to_justify(vertical)),
    };
    Node {
        position_type: PositionType::Absolute,
        width: Val::Percent(100.0),
        height: Val::Percent(100.0),
        flex_direction: match key.direction {
            HudStackDirection::Horizontal => FlexDirection::Row,
            HudStackDirection::Vertical => FlexDirection::Column,
        },
        align_items,
        justify_content,
        ..default()
    }
}

fn reconcile_hud_tree(
    commands: &mut Commands,
    root: Entity,
    id: &HudElementId,
    old: HudTreeKind,
    new: HudTreeKind,
    children: &[Entity],
    child_identities: &Query<&HudChildIdentity>,
) {
    match (old, new) {
        (HudTreeKind::Segments(_), HudTreeKind::Segments(capacity)) => {
            let mut segments = Vec::new();
            let mut malformed = false;
            for child in children {
                match child_identities.get(*child) {
                    Ok(HudChildIdentity {
                        element,
                        kind: HudChildKind::Segment(index),
                    }) if element == id => segments.push((*index, *child)),
                    _ => malformed = true,
                }
            }
            segments.sort_by_key(|(index, entity)| (*index, entity.to_bits()));
            let unique = segments.windows(2).all(|pair| pair[0].0 != pair[1].0);
            if malformed || !unique {
                commands.entity(root).despawn_children();
                spawn_hud_tree(commands, root, id, new);
                return;
            }

            let mut desired = Vec::with_capacity(capacity as usize);
            for index in 0..capacity {
                if let Some((_, entity)) = segments.iter().find(|(existing, _)| *existing == index)
                {
                    desired.push(*entity);
                } else {
                    desired.push(spawn_segment(commands, root, id, index));
                }
            }
            for (index, entity) in segments {
                if index >= capacity {
                    commands.entity(entity).despawn();
                }
            }
            commands.entity(root).replace_children(&desired);
        }
        (HudTreeKind::Text, HudTreeKind::Text)
        | (HudTreeKind::Bar, HudTreeKind::Bar)
        | (HudTreeKind::Icon, HudTreeKind::Icon) => {}
        _ => {
            commands.entity(root).despawn_children();
            spawn_hud_tree(commands, root, id, new);
        }
    }
}

fn spawn_hud_tree(commands: &mut Commands, root: Entity, id: &HudElementId, tree: HudTreeKind) {
    match tree {
        HudTreeKind::Text => {
            commands.spawn((
                HudChildIdentity {
                    element: id.clone(),
                    kind: HudChildKind::Text,
                },
                Text::new(""),
                TextFont::default(),
                TextColor(Color::WHITE),
                Node::default(),
                ChildOf(root),
            ));
        }
        HudTreeKind::Bar => {
            let track = commands
                .spawn((
                    HudChildIdentity {
                        element: id.clone(),
                        kind: HudChildKind::BarTrack,
                    },
                    Node::default(),
                    BackgroundColor(Color::NONE),
                    ChildOf(root),
                ))
                .id();
            commands.spawn((
                HudChildIdentity {
                    element: id.clone(),
                    kind: HudChildKind::BarFill,
                },
                Node::default(),
                BackgroundColor(Color::NONE),
                ChildOf(track),
            ));
        }
        HudTreeKind::Icon => {
            commands.spawn((
                HudChildIdentity {
                    element: id.clone(),
                    kind: HudChildKind::Icon,
                },
                HudCachedImage::default(),
                Node::default(),
                ImageNode::default(),
                ChildOf(root),
            ));
        }
        HudTreeKind::Segments(capacity) => {
            for index in 0..capacity {
                spawn_segment(commands, root, id, index);
            }
        }
    }
}

fn spawn_segment(commands: &mut Commands, root: Entity, id: &HudElementId, index: u32) -> Entity {
    commands
        .spawn((
            HudChildIdentity {
                element: id.clone(),
                kind: HudChildKind::Segment(index),
            },
            HudCachedImage::default(),
            Node::default(),
            BackgroundColor(Color::NONE),
            ImageNode::default(),
            ChildOf(root),
        ))
        .id()
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn render_hud_entities(
    registry: Res<HudRegistry>,
    values: Res<HudValueSet>,
    theme: Res<HudTheme>,
    asset_server: Option<Res<AssetServer>>,
    mut asset_cache: ResMut<HudAssetCache>,
    mut runtime: ResMut<HudRuntimeState>,
    mut tracker: ResMut<HudDiagnosticTracker>,
    mut diagnostics: MessageWriter<HudDiagnostic>,
    mut roots: Query<
        (
            &HudElementRoot,
            &mut HudResolvedFingerprint,
            &mut Node,
            &mut UiTransform,
            &mut BackgroundColor,
            &mut Visibility,
            &mut ZIndex,
        ),
        Without<HudChildIdentity>,
    >,
    mut children: Query<
        (
            &HudChildIdentity,
            &mut Node,
            Option<&mut Text>,
            Option<&mut TextFont>,
            Option<&mut TextColor>,
            Option<&mut BackgroundColor>,
            Option<&mut ImageNode>,
            Option<&mut HudCachedImage>,
        ),
        Without<HudElementRoot>,
    >,
) {
    let asset_server_present = asset_server.is_some();
    if !runtime.render_all
        && runtime.value_revision == Some(values.revision())
        && !runtime.pending_assets
        && runtime.asset_server_present == asset_server_present
    {
        return;
    }

    let mut pending_assets = false;
    let mut touched_assets = HashSet::new();
    let resolved = resolve_hud(
        &registry,
        &values,
        &theme,
        |path| {
            touched_assets.insert(path.clone().into_owned());
            asset_server.as_ref().is_some_and(|server| {
                asset_cache.ensure_available(path, server, &mut pending_assets)
            })
        },
        &mut tracker,
    );
    asset_cache
        .handles
        .retain(|path, _| touched_assets.contains(path));
    for diagnostic in resolved.diagnostics.iter().cloned() {
        diagnostics.write(diagnostic);
    }

    let resolved_by_id: HashMap<_, _> = resolved
        .elements
        .iter()
        .map(|element| (element.id.clone(), element))
        .collect();
    let mut changed_elements = HashSet::new();
    for (
        root,
        mut fingerprint,
        mut node,
        mut transform,
        mut background,
        mut visibility,
        mut z_index,
    ) in &mut roots
    {
        let Some(element) = resolved_by_id.get(&root.0).copied() else {
            continue;
        };
        if fingerprint.0.as_ref() == Some(element) {
            continue;
        }

        replace_if_changed(&mut node, hud_root_node(element));
        replace_if_changed(&mut transform, hud_root_transform(&element.layout));
        replace_if_changed(
            &mut background,
            BackgroundColor(root_background_color(element)),
        );
        replace_if_changed(
            &mut visibility,
            if element.visible {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            },
        );
        replace_if_changed(&mut z_index, ZIndex(element.style.z_index));
        fingerprint.0 = Some(element.clone());
        changed_elements.insert(root.0.clone());
    }

    for (identity, mut node, text, font, text_color, background, image, cached_image) in
        &mut children
    {
        if !changed_elements.contains(&identity.element) {
            continue;
        }
        let Some(element) = resolved_by_id.get(&identity.element).copied() else {
            continue;
        };
        render_hud_child(
            identity.kind,
            element,
            &asset_cache,
            &mut node,
            text,
            font,
            text_color,
            background,
            image,
            cached_image,
        );
    }

    runtime.value_revision = Some(values.revision());
    runtime.render_all = false;
    runtime.pending_assets = pending_assets;
    runtime.asset_server_present = asset_server_present;
}

impl HudAssetCache {
    fn ensure_available(
        &mut self,
        path: &AssetPath<'static>,
        asset_server: &AssetServer,
        pending: &mut bool,
    ) -> bool {
        let handle = self
            .handles
            .entry(path.clone().into_owned())
            .or_insert_with(|| asset_server.load::<Image>(path.clone().into_owned()));
        match asset_server.load_state(handle.id()) {
            LoadState::Loaded => true,
            LoadState::NotLoaded | LoadState::Loading => {
                *pending = true;
                false
            }
            LoadState::Failed(_) => false,
        }
    }

    fn handle(&self, path: &AssetPath<'static>) -> Option<Handle<Image>> {
        self.handles.get(path).cloned()
    }
}

#[allow(clippy::too_many_arguments)]
fn render_hud_child(
    kind: HudChildKind,
    element: &ResolvedHudElement,
    asset_cache: &HudAssetCache,
    node: &mut Mut<Node>,
    mut text: Option<Mut<Text>>,
    mut font: Option<Mut<TextFont>>,
    mut text_color: Option<Mut<TextColor>>,
    mut background: Option<Mut<BackgroundColor>>,
    mut image: Option<Mut<ImageNode>>,
    mut cached_image: Option<Mut<HudCachedImage>>,
) {
    match (kind, &element.presentation) {
        (HudChildKind::Text, ResolvedHudPresentation::Text(value)) => {
            replace_if_changed(node, Node::default());
            if let Some(text) = text.as_mut() {
                replace_if_changed(text, Text::new(value.clone()));
            }
            if let Some(font) = font.as_mut() {
                replace_if_changed(font, TextFont::from_font_size(element.style.font_size));
            }
            if let Some(text_color) = text_color.as_mut() {
                replace_if_changed(
                    text_color,
                    TextColor(color_from_array(element.style.text_color)),
                );
            }
        }
        (HudChildKind::BarTrack, ResolvedHudPresentation::Bar { .. }) => {
            replace_if_changed(
                node,
                Node {
                    width: Val::Px(element.style.bar_size[0]),
                    height: Val::Px(element.style.bar_size[1]),
                    ..default()
                },
            );
            if let Some(background) = background.as_mut() {
                replace_if_changed(
                    background,
                    BackgroundColor(color_from_array(element.style.background_color)),
                );
            }
        }
        (HudChildKind::BarFill, ResolvedHudPresentation::Bar { ratio }) => {
            replace_if_changed(
                node,
                Node {
                    width: Val::Px(element.style.bar_size[0] * ratio),
                    height: Val::Percent(100.0),
                    ..default()
                },
            );
            if let Some(background) = background.as_mut() {
                replace_if_changed(
                    background,
                    BackgroundColor(color_from_array(element.style.foreground_color)),
                );
            }
        }
        (HudChildKind::Icon, ResolvedHudPresentation::Icon { path }) => {
            replace_if_changed(
                node,
                Node {
                    width: Val::Px(element.style.icon_size[0]),
                    height: Val::Px(element.style.icon_size[1]),
                    ..default()
                },
            );
            render_image(path.as_ref(), asset_cache, &mut image, &mut cached_image);
        }
        (
            HudChildKind::Segment(index),
            ResolvedHudPresentation::Segments {
                filled,
                filled_asset,
                empty_asset,
                ..
            },
        ) => {
            replace_if_changed(
                node,
                Node {
                    width: Val::Px(element.style.segment_size[0]),
                    height: Val::Px(element.style.segment_size[1]),
                    ..default()
                },
            );
            let is_filled = index < *filled;
            let path = if is_filled {
                filled_asset.as_ref()
            } else {
                empty_asset.as_ref()
            };
            render_image(path, asset_cache, &mut image, &mut cached_image);
            if let Some(background) = background.as_mut() {
                let color = if path.is_some() {
                    Color::NONE
                } else if is_filled {
                    color_from_array(element.style.foreground_color)
                } else {
                    color_from_array(element.style.background_color)
                };
                replace_if_changed(background, BackgroundColor(color));
            }
        }
        _ => {}
    }
}

fn render_image(
    path: Option<&AssetPath<'static>>,
    asset_cache: &HudAssetCache,
    image: &mut Option<Mut<ImageNode>>,
    cached: &mut Option<Mut<HudCachedImage>>,
) {
    let handle = path.and_then(|path| asset_cache.handle(path));
    let next = HudCachedImage {
        path: path.cloned(),
        handle: handle.clone(),
    };
    let changed = cached.as_ref().is_none_or(|cached| **cached != next);
    if changed && let Some(image) = image.as_mut() {
        **image = handle
            .map(ImageNode::new)
            .unwrap_or_else(ImageNode::default);
    }
    if let Some(cached) = cached.as_mut() {
        replace_if_changed(cached, next);
    }
}

fn hud_root_node(element: &ResolvedHudElement) -> Node {
    let mut node = Node {
        flex_direction: FlexDirection::Row,
        padding: UiRect {
            left: Val::Px(element.style.padding[0]),
            right: Val::Px(element.style.padding[1]),
            top: Val::Px(element.style.padding[2]),
            bottom: Val::Px(element.style.padding[3]),
        },
        row_gap: Val::Px(element.style.gap),
        column_gap: Val::Px(element.style.gap),
        ..default()
    };
    match element.layout.horizontal {
        HudAxisAnchor::Start | HudAxisAnchor::Center => {
            node.margin.left = Val::Px(element.layout.offset[0]);
        }
        HudAxisAnchor::End => node.margin.right = Val::Px(element.layout.offset[0]),
    }
    match element.layout.vertical {
        HudAxisAnchor::Start | HudAxisAnchor::Center => {
            node.margin.top = Val::Px(element.layout.offset[1]);
        }
        HudAxisAnchor::End => node.margin.bottom = Val::Px(element.layout.offset[1]),
    }
    node
}

fn hud_root_transform(_layout: &ResolvedHudLayout) -> UiTransform {
    UiTransform::default()
}

fn root_background_color(element: &ResolvedHudElement) -> Color {
    match element.presentation {
        ResolvedHudPresentation::Bar { .. } | ResolvedHudPresentation::Segments { .. } => {
            Color::NONE
        }
        _ => color_from_array(element.style.background_color),
    }
}

fn color_from_array(color: [f32; 4]) -> Color {
    Color::linear_rgba(color[0], color[1], color[2], color[3])
}

fn replace_if_changed<T: PartialEq>(current: &mut Mut<T>, next: T) {
    if **current != next {
        **current = next;
    }
}

fn resolve_hud(
    registry: &HudRegistry,
    values: &HudValueSet,
    theme: &HudTheme,
    mut asset_available: impl FnMut(&AssetPath<'static>) -> bool,
    tracker: &mut HudDiagnosticTracker,
) -> ResolvedHud {
    let mut definitions: Vec<_> = registry.iter().collect();
    definitions.sort_by(|left, right| {
        left.placement
            .anchor
            .cmp(&right.placement.anchor)
            .then_with(|| left.order.cmp(&right.order))
            .then_with(|| left.id.cmp(&right.id))
    });

    let active_ids: HashSet<_> = definitions
        .iter()
        .map(|definition| definition.id().clone())
        .collect();
    tracker.retain_elements(&active_ids);

    let mut resolved = ResolvedHud::default();
    for definition in definitions {
        let (element, failure) =
            resolve_hud_element(definition, values, theme, &mut asset_available);
        if let Some(diagnostic) = tracker.transition(definition.id(), failure) {
            resolved.diagnostics.push(diagnostic);
        }
        resolved.elements.push(element);
    }
    resolved
}

fn resolve_hud_element(
    definition: &HudElementDefinition,
    values: &HudValueSet,
    theme: &HudTheme,
    asset_available: &mut impl FnMut(&AssetPath<'static>) -> bool,
) -> (ResolvedHudElement, Option<HudDiagnosticKind>) {
    let safe_presentation = safe_presentation(definition.presentation());

    if let Err(HudRegistryError::InvalidDefinition { reason, .. }) = validate_definition(definition)
    {
        return (
            ResolvedHudElement {
                id: definition.id().clone(),
                visible: false,
                used_fallback: false,
                layout: safe_default_layout(),
                style: safe_default_style(),
                presentation: safe_presentation,
            },
            Some(HudDiagnosticKind::InvalidDefinition { reason }),
        );
    }

    let layout = resolve_layout(definition);
    let style = match resolve_style(theme, definition.style_override()) {
        Ok(style) => style,
        Err(field) => {
            return (
                ResolvedHudElement {
                    id: definition.id().clone(),
                    visible: false,
                    used_fallback: false,
                    layout,
                    style: safe_default_style(),
                    presentation: safe_presentation,
                },
                Some(HudDiagnosticKind::InvalidStyle { field }),
            );
        }
    };

    match resolve_visibility(definition, values) {
        Ok(false) => {
            return (
                ResolvedHudElement {
                    id: definition.id().clone(),
                    visible: false,
                    used_fallback: false,
                    layout,
                    style,
                    presentation: safe_presentation,
                },
                None,
            );
        }
        Err(failure) => {
            return (
                ResolvedHudElement {
                    id: definition.id().clone(),
                    visible: false,
                    used_fallback: false,
                    layout,
                    style,
                    presentation: safe_presentation,
                },
                Some(failure),
            );
        }
        Ok(true) => {}
    }

    match resolve_presentation(definition, values, asset_available) {
        PresentationResolution::Visible {
            presentation,
            used_fallback,
            failure,
        } => (
            ResolvedHudElement {
                id: definition.id().clone(),
                visible: true,
                used_fallback,
                layout,
                style,
                presentation,
            },
            failure,
        ),
        PresentationResolution::Hidden { failure } => (
            ResolvedHudElement {
                id: definition.id().clone(),
                visible: false,
                used_fallback: false,
                layout,
                style,
                presentation: safe_presentation,
            },
            Some(failure),
        ),
    }
}

enum PresentationResolution {
    Visible {
        presentation: ResolvedHudPresentation,
        used_fallback: bool,
        failure: Option<HudDiagnosticKind>,
    },
    Hidden {
        failure: HudDiagnosticKind,
    },
}

fn resolve_presentation(
    definition: &HudElementDefinition,
    values: &HudValueSet,
    asset_available: &mut impl FnMut(&AssetPath<'static>) -> bool,
) -> PresentationResolution {
    let presentation = definition.presentation();
    let Some(value_id) = presentation.value_id() else {
        return match resolve_with_value(presentation, None, asset_available) {
            Ok(presentation) => PresentationResolution::Visible {
                presentation,
                used_fallback: false,
                failure: None,
            },
            Err(failure) => PresentationResolution::Hidden { failure },
        };
    };

    let primary = match values.get(value_id.as_str()) {
        Some(value) if presentation.accepts_kind(value.kind()) => {
            resolve_with_value(presentation, Some(value), asset_available)
        }
        Some(value) => Err(HudDiagnosticKind::MismatchedValue {
            value: value_id.clone(),
            binding: HudBindingKind::Presentation,
            actual: value.kind(),
        }),
        None => Err(HudDiagnosticKind::MissingValue {
            value: value_id.clone(),
            binding: HudBindingKind::Presentation,
        }),
    };

    match primary {
        Ok(presentation) => PresentationResolution::Visible {
            presentation,
            used_fallback: false,
            failure: None,
        },
        Err(primary_failure) => {
            let fallback = definition.fallback_value().and_then(|fallback| {
                presentation
                    .accepts_kind(fallback.kind())
                    .then(|| resolve_with_value(presentation, Some(fallback), asset_available))
            });
            match fallback {
                Some(Ok(presentation)) => PresentationResolution::Visible {
                    presentation,
                    used_fallback: true,
                    failure: Some(primary_failure),
                },
                Some(Err(fallback_failure)) => PresentationResolution::Hidden {
                    failure: fallback_failure,
                },
                None => PresentationResolution::Hidden {
                    failure: primary_failure,
                },
            }
        }
    }
}

fn resolve_with_value(
    presentation: &HudPresentation,
    value: Option<&HudValue>,
    asset_available: &mut impl FnMut(&AssetPath<'static>) -> bool,
) -> Result<ResolvedHudPresentation, HudDiagnosticKind> {
    match (presentation, value) {
        (
            HudPresentation::Text(text),
            Some(
                value @ (HudValue::Text(_)
                | HudValue::Scalar(_)
                | HudValue::Count(_)
                | HudValue::Bool(_)),
            ),
        ) => Ok(ResolvedHudPresentation::Text(format_text_value(
            text, value,
        ))),
        (HudPresentation::Bar(_), Some(HudValue::Scalar(scalar))) => {
            Ok(ResolvedHudPresentation::Bar {
                ratio: scalar.clamped_ratio(),
            })
        }
        (
            HudPresentation::Icon(HudIcon {
                source: HudIconSource::Asset(path),
            }),
            None,
        )
        | (
            HudPresentation::Icon(HudIcon {
                source: HudIconSource::Bound(_),
            }),
            Some(HudValue::Icon(path)),
        ) => {
            if asset_available(path) {
                Ok(ResolvedHudPresentation::Icon {
                    path: Some(path.clone().into_owned()),
                })
            } else {
                Err(HudDiagnosticKind::AssetUnavailable {
                    path: path.clone().into_owned(),
                })
            }
        }
        (HudPresentation::Segments(segments), Some(HudValue::Count(count))) => {
            if *count > segments.capacity() {
                return Err(HudDiagnosticKind::ExcessiveSegments {
                    count: *count,
                    capacity: segments.capacity(),
                });
            }
            for path in [segments.filled_asset_path(), segments.empty_asset_path()]
                .into_iter()
                .flatten()
            {
                if !asset_available(path) {
                    return Err(HudDiagnosticKind::AssetUnavailable {
                        path: path.clone().into_owned(),
                    });
                }
            }
            Ok(ResolvedHudPresentation::Segments {
                filled: *count,
                capacity: segments.capacity(),
                filled_asset: segments.filled_asset_path().cloned(),
                empty_asset: segments.empty_asset_path().cloned(),
            })
        }
        (_, value) => Err(HudDiagnosticKind::InvalidValue {
            binding: HudBindingKind::Presentation,
            actual: value.map(HudValue::kind),
        }),
    }
}

fn format_text_value(text: &HudText, value: &HudValue) -> String {
    let value = match value {
        HudValue::Text(value) => value.clone(),
        HudValue::Scalar(scalar) => match text.precision_value() {
            Some(precision) => format!("{:.*}", usize::from(precision), scalar.value()),
            None => scalar.value().to_string(),
        },
        HudValue::Count(value) => value.to_string(),
        HudValue::Bool(value) => value.to_string(),
        HudValue::Icon(_) => String::new(),
    };
    format!("{}{}{}", text.prefix_value(), value, text.suffix_value())
}

fn resolve_visibility(
    definition: &HudElementDefinition,
    values: &HudValueSet,
) -> Result<bool, HudDiagnosticKind> {
    let Some(value_id) = definition.visibility_value_id() else {
        return Ok(true);
    };
    match values.get(value_id.as_str()) {
        Some(HudValue::Bool(visible)) => Ok(*visible),
        Some(value) => Err(HudDiagnosticKind::MismatchedValue {
            value: value_id.clone(),
            binding: HudBindingKind::Visibility,
            actual: value.kind(),
        }),
        None => Err(HudDiagnosticKind::MissingValue {
            value: value_id.clone(),
            binding: HudBindingKind::Visibility,
        }),
    }
}

fn resolve_layout(definition: &HudElementDefinition) -> ResolvedHudLayout {
    let placement = definition.placement_value();
    let (horizontal, vertical) = match placement.anchor {
        HudAnchor::TopLeft => (HudAxisAnchor::Start, HudAxisAnchor::Start),
        HudAnchor::TopCenter => (HudAxisAnchor::Center, HudAxisAnchor::Start),
        HudAnchor::TopRight => (HudAxisAnchor::End, HudAxisAnchor::Start),
        HudAnchor::CenterLeft => (HudAxisAnchor::Start, HudAxisAnchor::Center),
        HudAnchor::Center => (HudAxisAnchor::Center, HudAxisAnchor::Center),
        HudAnchor::CenterRight => (HudAxisAnchor::End, HudAxisAnchor::Center),
        HudAnchor::BottomLeft => (HudAxisAnchor::Start, HudAxisAnchor::End),
        HudAnchor::BottomCenter => (HudAxisAnchor::Center, HudAxisAnchor::End),
        HudAnchor::BottomRight => (HudAxisAnchor::End, HudAxisAnchor::End),
    };
    ResolvedHudLayout {
        anchor: placement.anchor,
        horizontal,
        vertical,
        offset: placement.offset,
        stack_direction: placement.stack_direction,
        order: definition.order_value(),
    }
}

fn safe_default_layout() -> ResolvedHudLayout {
    ResolvedHudLayout {
        anchor: HudAnchor::TopLeft,
        horizontal: HudAxisAnchor::Start,
        vertical: HudAxisAnchor::Start,
        offset: HudPlacement::default().offset,
        stack_direction: HudStackDirection::Vertical,
        order: 0,
    }
}

fn resolve_style(
    theme: &HudTheme,
    style: &HudStyleOverride,
) -> Result<ResolvedHudStyle, &'static str> {
    let resolved = ResolvedHudStyle {
        font_size: style.font_size.unwrap_or(theme.font_size),
        text_color: style.text_color.unwrap_or(theme.text_color),
        background_color: style.background_color.unwrap_or(theme.background_color),
        foreground_color: style.foreground_color.unwrap_or(theme.foreground_color),
        padding: style.padding.unwrap_or(theme.padding),
        gap: style.gap.unwrap_or(theme.gap),
        bar_size: style.bar_size.unwrap_or(theme.bar_size),
        icon_size: style.icon_size.unwrap_or(theme.icon_size),
        segment_size: style.segment_size.unwrap_or(theme.segment_size),
        z_index: style.z_index.unwrap_or(theme.z_index),
    };
    validate_resolved_style(&resolved)?;
    Ok(resolved)
}

fn validate_resolved_style(style: &ResolvedHudStyle) -> Result<(), &'static str> {
    for (field, values) in [
        ("padding", style.padding.as_slice()),
        ("bar_size", style.bar_size.as_slice()),
        ("icon_size", style.icon_size.as_slice()),
        ("segment_size", style.segment_size.as_slice()),
    ] {
        if values
            .iter()
            .any(|value| !value.is_finite() || *value < 0.0)
        {
            return Err(field);
        }
    }
    for (field, value) in [("font_size", style.font_size), ("gap", style.gap)] {
        if !value.is_finite() || value < 0.0 {
            return Err(field);
        }
    }
    for (field, values) in [
        ("text_color", style.text_color.as_slice()),
        ("background_color", style.background_color.as_slice()),
        ("foreground_color", style.foreground_color.as_slice()),
    ] {
        if values.iter().any(|value| !value.is_finite()) {
            return Err(field);
        }
    }
    Ok(())
}

fn safe_default_style() -> ResolvedHudStyle {
    let theme = HudTheme::default();
    ResolvedHudStyle {
        font_size: theme.font_size,
        text_color: theme.text_color,
        background_color: theme.background_color,
        foreground_color: theme.foreground_color,
        padding: theme.padding,
        gap: theme.gap,
        bar_size: theme.bar_size,
        icon_size: theme.icon_size,
        segment_size: theme.segment_size,
        z_index: theme.z_index,
    }
}

fn safe_presentation(presentation: &HudPresentation) -> ResolvedHudPresentation {
    match presentation {
        HudPresentation::Text(_) => ResolvedHudPresentation::Text(String::new()),
        HudPresentation::Bar(_) => ResolvedHudPresentation::Bar { ratio: 0.0 },
        HudPresentation::Icon(_) => ResolvedHudPresentation::Icon { path: None },
        HudPresentation::Segments(segments) => ResolvedHudPresentation::Segments {
            filled: 0,
            capacity: segments.capacity().min(MAX_HUD_SEGMENTS),
            filled_asset: None,
            empty_asset: None,
        },
    }
}

fn validate_definition(definition: &HudElementDefinition) -> Result<(), HudRegistryError> {
    let invalid = |reason| HudRegistryError::InvalidDefinition {
        id: definition.id().clone(),
        reason,
    };

    if !definition.placement.offset.into_iter().all(f32::is_finite) {
        return Err(invalid(HudDefinitionError::NonFinitePlacement));
    }

    if let HudPresentation::Segments(segments) = definition.presentation() {
        if !(1..=MAX_HUD_SEGMENTS).contains(&segments.capacity()) {
            return Err(invalid(HudDefinitionError::InvalidSegmentCapacity {
                capacity: segments.capacity(),
                maximum: MAX_HUD_SEGMENTS,
            }));
        }
    }

    if let Some(fallback) = definition.fallback_value() {
        if matches!(
            definition.presentation(),
            HudPresentation::Icon(HudIcon {
                source: HudIconSource::Asset(_),
            })
        ) {
            return Err(invalid(HudDefinitionError::StaticIconFallback));
        }
        if !definition.presentation().accepts_kind(fallback.kind()) {
            return Err(invalid(HudDefinitionError::InvalidFallbackKind {
                expected: expected_kinds(definition.presentation()),
                actual: fallback.kind(),
            }));
        }
    }

    validate_style(&definition.style).map_err(invalid)
}

fn expected_kinds(presentation: &HudPresentation) -> &'static str {
    match presentation {
        HudPresentation::Text(_) => "text, scalar, count, or bool",
        HudPresentation::Bar(_) => "scalar",
        HudPresentation::Icon(_) => "icon",
        HudPresentation::Segments(_) => "count",
    }
}

fn validate_style(style: &HudStyleOverride) -> Result<(), HudDefinitionError> {
    validate_non_negative_scalar("font_size", style.font_size)?;
    validate_non_negative_scalar("gap", style.gap)?;
    validate_non_negative_array(
        "padding",
        style.padding.as_ref().map(|value| value.as_slice()),
    )?;
    validate_non_negative_array(
        "bar_size",
        style.bar_size.as_ref().map(|value| value.as_slice()),
    )?;
    validate_non_negative_array(
        "icon_size",
        style.icon_size.as_ref().map(|value| value.as_slice()),
    )?;
    validate_non_negative_array(
        "segment_size",
        style.segment_size.as_ref().map(|value| value.as_slice()),
    )?;
    validate_finite_array(
        "text_color",
        style.text_color.as_ref().map(|value| value.as_slice()),
    )?;
    validate_finite_array(
        "background_color",
        style
            .background_color
            .as_ref()
            .map(|value| value.as_slice()),
    )?;
    validate_finite_array(
        "foreground_color",
        style
            .foreground_color
            .as_ref()
            .map(|value| value.as_slice()),
    )?;
    Ok(())
}

fn validate_non_negative_scalar(
    field: &'static str,
    value: Option<f32>,
) -> Result<(), HudDefinitionError> {
    if value.is_some_and(|value| !value.is_finite() || value < 0.0) {
        return Err(HudDefinitionError::InvalidStyleValue { field });
    }
    Ok(())
}

fn validate_non_negative_array(
    field: &'static str,
    values: Option<&[f32]>,
) -> Result<(), HudDefinitionError> {
    if values.is_some_and(|values| {
        values
            .iter()
            .any(|value| !value.is_finite() || *value < 0.0)
    }) {
        return Err(HudDefinitionError::InvalidStyleValue { field });
    }
    Ok(())
}

fn validate_finite_array(
    field: &'static str,
    values: Option<&[f32]>,
) -> Result<(), HudDefinitionError> {
    if values.is_some_and(|values| values.iter().any(|value| !value.is_finite())) {
        return Err(HudDefinitionError::InvalidStyleValue { field });
    }
    Ok(())
}

fn bump_revision(revision: &mut u64) {
    *revision = revision.saturating_add(1);
}

#[cfg(test)]
mod tests {
    use bevy::ecs::message::{MessageReader, MessageWriter};
    use bevy::prelude::*;

    use super::*;

    fn text_definition(id: &str, value: &str) -> HudElementDefinition {
        HudElementDefinition::new(id, HudPresentation::Text(HudText::new(value)))
    }

    #[test]
    fn ids_keep_string_ergonomics_and_ordering() {
        let id = HudElementId::new("status");
        assert_eq!(id.as_str(), "status");
        assert_eq!(id.to_string(), "status");
        assert_eq!(HudValueId::from(String::from("value")).as_ref(), "value");
        assert!(HudElementId::from("a") < HudElementId::from("b"));
    }

    #[test]
    fn scalar_rejects_non_finite_and_degenerate_ranges() {
        assert_eq!(
            HudScalar::new(f32::NAN, 0.0, 1.0),
            Err(HudValueError::NonFinite {
                field: HudScalarField::Value
            })
        );
        assert_eq!(
            HudScalar::new(0.0, f32::NEG_INFINITY, 1.0),
            Err(HudValueError::NonFinite {
                field: HudScalarField::Min
            })
        );
        assert_eq!(
            HudScalar::new(0.0, 0.0, f32::INFINITY),
            Err(HudValueError::NonFinite {
                field: HudScalarField::Max
            })
        );
        assert_eq!(
            HudScalar::new(0.0, 1.0, 1.0),
            Err(HudValueError::InvalidRange { min: 1.0, max: 1.0 })
        );

        let error = serde_json::from_str::<HudScalar>(r#"{"value":0.0,"min":1.0,"max":1.0}"#)
            .expect_err("deserialization must preserve the scalar invariant");
        assert!(error.to_string().contains("nonzero span"));
    }

    #[test]
    fn scalar_normalizes_reversed_bounds_and_clamps_ratio() {
        let scalar = HudScalar::new(15.0, 10.0, 0.0).expect("range should normalize");
        assert_eq!((scalar.min(), scalar.max()), (0.0, 10.0));
        assert_eq!(scalar.value(), 15.0);
        assert_eq!(scalar.clamped_ratio(), 1.0);

        let deserialized =
            serde_json::from_str::<HudScalar>(r#"{"value":-5.0,"min":0.0,"max":-10.0}"#)
                .expect("valid reversed bounds should normalize during deserialization");
        assert_eq!((deserialized.min(), deserialized.max()), (-10.0, 0.0));
        assert_eq!(deserialized.clamped_ratio(), 0.5);

        let huge = HudScalar::new(0.0, f32::MIN, f32::MAX).expect("finite range is valid");
        assert!(huge.clamped_ratio().is_finite());
        assert_eq!(huge.clamped_ratio(), 0.5);
    }

    #[test]
    fn presentation_builders_preserve_owned_author_data() {
        let text = HudText::new("temperature")
            .prefix("T: ")
            .suffix(" C")
            .precision(1);
        assert_eq!(text.value_id().as_str(), "temperature");
        assert_eq!(text.prefix_value(), "T: ");
        assert_eq!(text.suffix_value(), " C");
        assert_eq!(text.precision_value(), Some(1));

        let path = String::from("textures/hud/charge.png");
        let icon = HudIcon::asset(path);
        assert!(
            matches!(icon.source(), HudIconSource::Asset(asset) if asset.to_string() == "textures/hud/charge.png")
        );

        let icon_value = HudValue::icon(String::from("textures/hud/dynamic.png"));
        let serialized = serde_json::to_string(&icon_value).expect("icon value should serialize");
        assert_eq!(
            serde_json::from_str::<HudValue>(&serialized).expect("icon value should deserialize"),
            icon_value
        );

        let segments = HudSegments::new("charges", 8)
            .filled_asset(String::from("textures/hud/full.png"))
            .empty_asset(String::from("textures/hud/empty.png"));
        assert_eq!(segments.capacity(), 8);
        assert!(segments.filled_asset_path().is_some());
        assert!(segments.empty_asset_path().is_some());
    }

    #[test]
    fn registry_rejects_invalid_definition_kinds_and_bounds() {
        let mut registry = HudRegistry::new();
        let invalid_fallback =
            HudElementDefinition::new("bar", HudPresentation::Bar(HudBar::new("level")))
                .fallback(3_u32);
        assert!(matches!(
            registry.register(invalid_fallback),
            Err(HudRegistryError::InvalidDefinition {
                reason: HudDefinitionError::InvalidFallbackKind { .. },
                ..
            })
        ));

        let excessive = HudElementDefinition::new(
            "segments",
            HudPresentation::Segments(HudSegments::new("count", MAX_HUD_SEGMENTS + 1)),
        );
        assert!(matches!(
            registry.register(excessive),
            Err(HudRegistryError::InvalidDefinition {
                reason: HudDefinitionError::InvalidSegmentCapacity { .. },
                ..
            })
        ));

        let zero_capacity = HudElementDefinition::new(
            "zero_segments",
            HudPresentation::Segments(HudSegments::new("count", 0)),
        );
        assert!(matches!(
            registry.register(zero_capacity),
            Err(HudRegistryError::InvalidDefinition {
                reason: HudDefinitionError::InvalidSegmentCapacity { capacity: 0, .. },
                ..
            })
        ));

        let invalid_placement = text_definition("placement", "value")
            .placement(HudPlacement::default().offset(f32::NAN, 0.0));
        assert!(matches!(
            registry.register(invalid_placement),
            Err(HudRegistryError::InvalidDefinition {
                reason: HudDefinitionError::NonFinitePlacement,
                ..
            })
        ));

        for (id, style) in [
            (
                "negative_size",
                HudStyleOverride {
                    icon_size: Some([-1.0, 1.0]),
                    ..default()
                },
            ),
            (
                "infinite_gap",
                HudStyleOverride {
                    gap: Some(f32::INFINITY),
                    ..default()
                },
            ),
            (
                "nan_color",
                HudStyleOverride {
                    text_color: Some([1.0, 1.0, f32::NAN, 1.0]),
                    ..default()
                },
            ),
        ] {
            assert!(matches!(
                registry.register(text_definition(id, "value").style(style)),
                Err(HudRegistryError::InvalidDefinition {
                    reason: HudDefinitionError::InvalidStyleValue { .. },
                    ..
                })
            ));
        }
        assert_eq!(registry.revision(), 0);
    }

    #[test]
    fn registry_duplicate_error_precedes_revalidation_and_invalid_replace_is_atomic() {
        let mut registry = HudRegistry::new();
        registry
            .register(text_definition("status", "status_text"))
            .expect("initial definition registers");

        let invalid_duplicate = HudElementDefinition::new(
            "status",
            HudPresentation::Segments(HudSegments::new("count", 0)),
        );
        assert!(matches!(
            registry.register(invalid_duplicate.clone()),
            Err(HudRegistryError::DuplicateId { .. })
        ));
        assert_eq!(registry.revision(), 1);

        assert!(matches!(
            registry.insert_or_replace(invalid_duplicate),
            Err(HudRegistryError::InvalidDefinition { .. })
        ));
        assert!(matches!(
            registry.require("status").unwrap().presentation(),
            HudPresentation::Text(_)
        ));
        assert_eq!(registry.revision(), 1);
    }

    #[test]
    fn registry_tracks_only_real_definition_changes() {
        let mut registry = HudRegistry::new();
        let first = text_definition("status", "status_text");
        registry
            .register(first.clone())
            .expect("definition registers");
        assert_eq!(registry.revision(), 1);
        assert!(matches!(
            registry.register(first.clone()),
            Err(HudRegistryError::DuplicateId { .. })
        ));
        assert_eq!(registry.revision(), 1);

        assert_eq!(registry.insert_or_replace(first).unwrap(), None);
        assert_eq!(registry.revision(), 1);
        let replacement = text_definition("status", "other").order(7);
        assert!(registry.insert_or_replace(replacement).unwrap().is_some());
        assert_eq!(registry.require("status").unwrap().order_value(), 7);
        assert_eq!(registry.revision(), 2);

        registry.remove("status").expect("definition exists");
        assert_eq!(registry.revision(), 3);
        assert!(matches!(
            registry.remove("status"),
            Err(HudRegistryError::MissingId { .. })
        ));
        assert_eq!(registry.revision(), 3);
    }

    #[test]
    fn registry_snapshot_is_identity_ordered_and_serializable() {
        let mut registry = HudRegistry::new();
        registry
            .register(text_definition("zeta", "z"))
            .expect("zeta registers");
        registry
            .register(text_definition("alpha", "a"))
            .expect("alpha registers");
        let snapshot = registry.snapshot();
        assert_eq!(snapshot[0].id.as_str(), "alpha");
        assert_eq!(snapshot[1].id.as_str(), "zeta");
        let serialized =
            serde_json::to_string(&snapshot).expect("element snapshots should serialize");

        let mut equivalent = HudRegistry::new();
        equivalent
            .register(text_definition("alpha", "a"))
            .expect("alpha registers");
        equivalent
            .register(text_definition("zeta", "z"))
            .expect("zeta registers");
        assert_eq!(
            serialized,
            serde_json::to_string(&equivalent.snapshot()).unwrap()
        );
    }

    #[test]
    fn value_set_tracks_real_changes_removal_and_ordered_snapshots() {
        let mut values = HudValueSet::new();
        assert_eq!(values.insert_or_replace("zeta", 2_u32), None);
        assert_eq!(values.revision(), 1);
        assert_eq!(values.insert_or_replace("zeta", 2_u32), None);
        assert_eq!(values.revision(), 1);
        assert_eq!(values.insert_or_replace("alpha", "ready"), None);
        assert_eq!(values.revision(), 2);
        assert_eq!(values.get("zeta"), Some(&HudValue::Count(2)));

        let snapshot = values.snapshot();
        assert_eq!(snapshot[0].id.as_str(), "alpha");
        assert_eq!(snapshot[1].id.as_str(), "zeta");
        let serialized =
            serde_json::to_string(&snapshot).expect("value snapshots should serialize");

        let mut equivalent = HudValueSet::new();
        equivalent.insert_or_replace("alpha", "ready");
        equivalent.insert_or_replace("zeta", 2_u32);
        assert_eq!(
            serialized,
            serde_json::to_string(&equivalent.snapshot()).unwrap()
        );

        assert_eq!(values.remove("missing"), None);
        assert_eq!(values.revision(), 2);
        assert_eq!(values.remove("zeta"), Some(HudValue::Count(2)));
        assert_eq!(values.revision(), 3);
    }

    #[derive(Resource, Default)]
    struct ObservedChanges(Vec<HudValueChanged>);

    fn publish_values(
        mut values: ResMut<HudValueSet>,
        mut changed: MessageWriter<HudValueChanged>,
    ) {
        assert!(set_hud_value("status", "ready", &mut values, &mut changed));
        assert!(!set_hud_value("status", "ready", &mut values, &mut changed));
        assert!(set_hud_value("status", "active", &mut values, &mut changed));
        assert!(remove_hud_value("status", &mut values, &mut changed));
        assert!(!remove_hud_value("status", &mut values, &mut changed));
    }

    fn record_changes(
        mut reader: MessageReader<HudValueChanged>,
        mut observed: ResMut<ObservedChanges>,
    ) {
        observed.0.extend(reader.read().cloned());
    }

    #[test]
    fn publication_helpers_emit_only_real_change_messages() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.init_resource::<HudValueSet>();
        app.init_resource::<ObservedChanges>();
        app.add_message::<HudValueChanged>();
        app.add_systems(Update, (publish_values, record_changes).chain());

        app.update();

        let observed = app.world().resource::<ObservedChanges>();
        assert_eq!(observed.0.len(), 3);
        assert_eq!(observed.0[0].previous, None);
        assert_eq!(observed.0[0].current, Some(HudValue::Text("ready".into())));
        assert_eq!(observed.0[1].previous, Some(HudValue::Text("ready".into())));
        assert_eq!(observed.0[2].current, None);
    }

    #[test]
    fn resolution_formats_supported_text_values_without_coercing_kinds() {
        let scalar = HudScalar::new(12.345, 0.0, 20.0).unwrap();
        let text = HudText::new("value").prefix("<").suffix(">").precision(2);
        assert_eq!(
            format_text_value(&text, &HudValue::Scalar(scalar)),
            "<12.35>"
        );
        assert_eq!(format_text_value(&text, &HudValue::Count(7)), "<7>");
        assert_eq!(format_text_value(&text, &HudValue::Bool(true)), "<true>");
        assert_eq!(
            format_text_value(&text, &HudValue::Text("ready".into())),
            "<ready>"
        );
        assert_eq!(
            resolve_with_value(
                &HudPresentation::Text(text),
                Some(&HudValue::icon("textures/not-text.png")),
                &mut |_| true,
            ),
            Err(HudDiagnosticKind::InvalidValue {
                binding: HudBindingKind::Presentation,
                actual: Some(HudValueKind::Icon),
            })
        );
    }

    #[test]
    fn resolution_handles_all_presentations_and_bounds_numeric_output() {
        let mut registry = HudRegistry::new();
        for definition in [
            text_definition("text", "label"),
            HudElementDefinition::new("bar", HudPresentation::Bar(HudBar::new("ratio"))),
            HudElementDefinition::new(
                "icon",
                HudPresentation::Icon(HudIcon::asset("textures/icon.png")),
            ),
            HudElementDefinition::new(
                "segments",
                HudPresentation::Segments(HudSegments::new("count", 5)),
            ),
        ] {
            registry.register(definition).unwrap();
        }
        let mut values = HudValueSet::new();
        values.insert_or_replace("label", "ready");
        values.insert_or_replace("ratio", HudScalar::new(15.0, 0.0, 10.0).unwrap());
        values.insert_or_replace("count", 3_u32);

        let mut tracker = HudDiagnosticTracker::default();
        let resolved = resolve_hud(
            &registry,
            &values,
            &HudTheme::default(),
            |_| true,
            &mut tracker,
        );
        assert_eq!(resolved.diagnostics, Vec::<HudDiagnostic>::new());
        assert_eq!(resolved.elements.len(), 4);
        assert!(resolved.elements.iter().all(|element| element.visible));
        assert!(resolved.elements.iter().any(|element| {
            matches!(element.presentation, ResolvedHudPresentation::Text(ref value) if value == "ready")
        }));
        assert!(resolved.elements.iter().any(|element| {
            matches!(element.presentation, ResolvedHudPresentation::Bar { ratio } if ratio == 1.0)
        }));
        assert!(resolved.elements.iter().any(|element| {
            matches!(
                element.presentation,
                ResolvedHudPresentation::Segments {
                    filled: 3,
                    capacity: 5,
                    ..
                }
            )
        }));
        assert!(resolved.elements.iter().any(|element| {
            matches!(element.presentation, ResolvedHudPresentation::Icon { path: Some(ref path) } if path.to_string() == "textures/icon.png")
        }));
    }

    #[test]
    fn resolution_merges_style_maps_anchors_and_orders_ties_by_id() {
        let mut registry = HudRegistry::new();
        for id in ["zeta", "alpha"] {
            registry
                .register(
                    text_definition(id, id)
                        .order(4)
                        .placement(
                            HudPlacement::new(HudAnchor::BottomRight)
                                .offset(12.0, 8.0)
                                .stack_direction(HudStackDirection::Horizontal),
                        )
                        .style(HudStyleOverride {
                            font_size: Some(24.0),
                            gap: Some(3.0),
                            ..default()
                        }),
                )
                .unwrap();
        }
        let mut values = HudValueSet::new();
        values.insert_or_replace("zeta", "z");
        values.insert_or_replace("alpha", "a");

        let mut tracker = HudDiagnosticTracker::default();
        let resolved = resolve_hud(
            &registry,
            &values,
            &HudTheme::default(),
            |_| true,
            &mut tracker,
        );
        assert_eq!(resolved.elements[0].id.as_str(), "alpha");
        assert_eq!(resolved.elements[1].id.as_str(), "zeta");
        let first = &resolved.elements[0];
        assert_eq!(first.layout.horizontal, HudAxisAnchor::End);
        assert_eq!(first.layout.vertical, HudAxisAnchor::End);
        assert_eq!(first.layout.offset, [12.0, 8.0]);
        assert_eq!(first.layout.stack_direction, HudStackDirection::Horizontal);
        assert_eq!(first.style.font_size, 24.0);
        assert_eq!(first.style.gap, 3.0);
        assert_eq!(first.style.bar_size, HudTheme::default().bar_size);
    }

    #[test]
    fn missing_and_mismatched_values_use_only_compatible_fallbacks() {
        let fallback =
            HudElementDefinition::new("fallback", HudPresentation::Bar(HudBar::new("missing")))
                .fallback(HudScalar::new(1.0, 0.0, 4.0).unwrap());
        let hidden =
            HudElementDefinition::new("hidden", HudPresentation::Bar(HudBar::new("wrong")));
        let mut registry = HudRegistry::new();
        registry.register(fallback).unwrap();
        registry.register(hidden).unwrap();
        let mut values = HudValueSet::new();
        values.insert_or_replace("wrong", "not a scalar");

        let mut tracker = HudDiagnosticTracker::default();
        let resolved = resolve_hud(
            &registry,
            &values,
            &HudTheme::default(),
            |_| true,
            &mut tracker,
        );
        let fallback = resolved
            .elements
            .iter()
            .find(|element| element.id.as_str() == "fallback")
            .unwrap();
        assert!(fallback.visible);
        assert!(fallback.used_fallback);
        assert!(matches!(
            fallback.presentation,
            ResolvedHudPresentation::Bar { ratio } if ratio == 0.25
        ));
        let hidden = resolved
            .elements
            .iter()
            .find(|element| element.id.as_str() == "hidden")
            .unwrap();
        assert!(!hidden.visible);
        assert!(!hidden.used_fallback);
        assert_eq!(resolved.diagnostics.len(), 2);
        assert!(
            resolved.diagnostics.iter().any(|diagnostic| matches!(
                diagnostic.kind,
                HudDiagnosticKind::MissingValue { .. }
            ))
        );
        assert!(resolved.diagnostics.iter().any(|diagnostic| matches!(
            diagnostic.kind,
            HudDiagnosticKind::MismatchedValue { .. }
        )));
    }

    #[test]
    fn excessive_segments_are_never_clamped_and_may_use_a_bounded_fallback() {
        let definition = HudElementDefinition::new(
            "charges",
            HudPresentation::Segments(HudSegments::new("count", 4)),
        )
        .fallback(2_u32);
        let mut registry = HudRegistry::new();
        registry.register(definition).unwrap();
        let mut values = HudValueSet::new();
        values.insert_or_replace("count", 99_u32);

        let mut tracker = HudDiagnosticTracker::default();
        let resolved = resolve_hud(
            &registry,
            &values,
            &HudTheme::default(),
            |_| true,
            &mut tracker,
        );
        assert!(resolved.elements[0].visible);
        assert!(resolved.elements[0].used_fallback);
        assert!(matches!(
            resolved.elements[0].presentation,
            ResolvedHudPresentation::Segments {
                filled: 2,
                capacity: 4,
                ..
            }
        ));
        assert!(matches!(
            resolved.diagnostics[0].kind,
            HudDiagnosticKind::ExcessiveSegments {
                count: 99,
                capacity: 4
            }
        ));
    }

    #[test]
    fn visibility_and_asset_failures_resolve_to_hidden_or_available_fallback_state() {
        let conditional = text_definition("conditional", "label").visible_when("show");
        let icon =
            HudElementDefinition::new("icon", HudPresentation::Icon(HudIcon::bound("icon_path")))
                .fallback(HudValue::icon("textures/fallback.png"));
        let mut registry = HudRegistry::new();
        registry.register(conditional).unwrap();
        registry.register(icon).unwrap();
        let mut values = HudValueSet::new();
        values.insert_or_replace("label", "ready");
        values.insert_or_replace("show", false);
        values.insert_or_replace("icon_path", HudValue::icon("textures/missing.png"));

        let mut tracker = HudDiagnosticTracker::default();
        let resolved = resolve_hud(
            &registry,
            &values,
            &HudTheme::default(),
            |path| path.to_string() == "textures/fallback.png",
            &mut tracker,
        );
        let conditional = resolved
            .elements
            .iter()
            .find(|element| element.id.as_str() == "conditional")
            .unwrap();
        assert!(!conditional.visible);
        let icon = resolved
            .elements
            .iter()
            .find(|element| element.id.as_str() == "icon")
            .unwrap();
        assert!(icon.visible);
        assert!(icon.used_fallback);
        assert!(matches!(
            icon.presentation,
            ResolvedHudPresentation::Icon { path: Some(ref path) }
                if path.to_string() == "textures/fallback.png"
        ));
        assert_eq!(resolved.diagnostics.len(), 1);
        assert!(matches!(
            resolved.diagnostics[0].kind,
            HudDiagnosticKind::AssetUnavailable { .. }
        ));

        let unavailable_fallback = resolve_hud(
            &registry,
            &values,
            &HudTheme::default(),
            |_| false,
            &mut tracker,
        );
        let icon = unavailable_fallback
            .elements
            .iter()
            .find(|element| element.id.as_str() == "icon")
            .unwrap();
        assert!(!icon.visible);
        assert!(!icon.used_fallback);
        assert!(matches!(
            unavailable_fallback.diagnostics[0].kind,
            HudDiagnosticKind::AssetUnavailable { ref path }
                if path.to_string() == "textures/fallback.png"
        ));
    }

    #[test]
    fn invalid_definition_and_theme_inputs_hide_without_panicking() {
        let invalid_definition = HudElementDefinition::new(
            "invalid",
            HudPresentation::Segments(HudSegments::new("count", 0)),
        );
        let values = HudValueSet::new();
        let (resolved, failure) = resolve_hud_element(
            &invalid_definition,
            &values,
            &HudTheme::default(),
            &mut |_| true,
        );
        assert!(!resolved.visible);
        assert!(matches!(
            failure,
            Some(HudDiagnosticKind::InvalidDefinition { .. })
        ));

        let invalid_placement = text_definition("invalid-placement", "label")
            .placement(HudPlacement::new(HudAnchor::BottomRight).offset(f32::NAN, f32::INFINITY));
        let (resolved, failure) = resolve_hud_element(
            &invalid_placement,
            &values,
            &HudTheme::default(),
            &mut |_| true,
        );
        assert!(!resolved.visible);
        assert_eq!(resolved.layout.offset, HudPlacement::default().offset);
        assert!(resolved.layout.offset.into_iter().all(f32::is_finite));
        assert!(matches!(
            failure,
            Some(HudDiagnosticKind::InvalidDefinition {
                reason: HudDefinitionError::NonFinitePlacement
            })
        ));

        let invalid_override = text_definition("invalid-style", "label").style(HudStyleOverride {
            gap: Some(-1.0),
            ..default()
        });
        let (resolved, failure) = resolve_hud_element(
            &invalid_override,
            &values,
            &HudTheme::default(),
            &mut |_| true,
        );
        assert!(!resolved.visible);
        assert_eq!(resolved.style, safe_default_style());
        assert!(matches!(
            failure,
            Some(HudDiagnosticKind::InvalidDefinition {
                reason: HudDefinitionError::InvalidStyleValue { field: "gap" }
            })
        ));

        let definition = text_definition("theme", "label");
        let mut values = HudValueSet::new();
        values.insert_or_replace("label", "ready");
        let invalid_theme = HudTheme {
            font_size: f32::NAN,
            ..default()
        };
        let (resolved, failure) =
            resolve_hud_element(&definition, &values, &invalid_theme, &mut |_| true);
        assert!(!resolved.visible);
        assert_eq!(
            failure,
            Some(HudDiagnosticKind::InvalidStyle { field: "font_size" })
        );
        assert!(resolved.style.font_size.is_finite());
    }

    #[test]
    fn diagnostics_emit_only_on_failure_transitions_and_reset_after_recovery() {
        let mut registry = HudRegistry::new();
        registry
            .register(text_definition("status", "label"))
            .unwrap();
        let mut values = HudValueSet::new();
        let mut tracker = HudDiagnosticTracker::default();

        let first = resolve_hud(
            &registry,
            &values,
            &HudTheme::default(),
            |_| true,
            &mut tracker,
        );
        let unchanged = resolve_hud(
            &registry,
            &values,
            &HudTheme::default(),
            |_| true,
            &mut tracker,
        );
        assert_eq!(first.diagnostics.len(), 1);
        assert!(unchanged.diagnostics.is_empty());

        values.insert_or_replace("label", "ready");
        let recovered = resolve_hud(
            &registry,
            &values,
            &HudTheme::default(),
            |_| true,
            &mut tracker,
        );
        assert!(recovered.diagnostics.is_empty());
        assert!(recovered.elements[0].visible);

        values.remove("label");
        let failed_again = resolve_hud(
            &registry,
            &values,
            &HudTheme::default(),
            |_| true,
            &mut tracker,
        );
        assert_eq!(failed_again.diagnostics.len(), 1);
    }

    #[test]
    fn public_ecs_types_implement_their_required_bevy_traits() {
        fn assert_resource<T: Resource>() {}
        fn assert_message<T: Message>() {}

        assert_resource::<HudRegistry>();
        assert_resource::<HudValueSet>();
        assert_resource::<HudTheme>();
        assert_message::<HudValueChanged>();
        assert_message::<HudDiagnostic>();
    }

    fn root_for(app: &mut App, id: &str) -> Entity {
        let mut query = app.world_mut().query::<(Entity, &HudElementRoot)>();
        let matches: Vec<_> = query
            .iter(app.world())
            .filter_map(|(entity, root)| (root.0.as_str() == id).then_some(entity))
            .collect();
        assert_eq!(matches.len(), 1, "expected exactly one root for {id}");
        matches[0]
    }

    fn children_for(app: &mut App, id: &str) -> Vec<(HudChildKind, Entity)> {
        let mut query = app.world_mut().query::<(Entity, &HudChildIdentity)>();
        let mut matches: Vec<_> = query
            .iter(app.world())
            .filter_map(|(entity, child)| {
                (child.element.as_str() == id).then_some((child.kind, entity))
            })
            .collect();
        matches.sort_by_key(|(kind, entity)| (format!("{kind:?}"), entity.to_bits()));
        matches
    }

    #[test]
    fn retained_renderer_preserves_roots_and_reconciles_only_changed_children() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, VoxelHudPlugin));
        app.world_mut()
            .resource_mut::<HudRegistry>()
            .register(text_definition("status", "status_value"))
            .unwrap();
        app.world_mut()
            .resource_mut::<HudValueSet>()
            .insert_or_replace("status_value", "ready");

        app.update();
        let root = root_for(&mut app, "status");
        let text_child = children_for(&mut app, "status")
            .into_iter()
            .find_map(|(kind, entity)| (kind == HudChildKind::Text).then_some(entity))
            .unwrap();
        assert_eq!(app.world().get::<Text>(text_child).unwrap().0, "ready");

        app.world_mut()
            .resource_mut::<HudValueSet>()
            .insert_or_replace("status_value", "updated");
        app.update();
        assert_eq!(root_for(&mut app, "status"), root);
        assert_eq!(app.world().get::<Text>(text_child).unwrap().0, "updated");

        app.world_mut()
            .resource_mut::<HudRegistry>()
            .insert_or_replace(text_definition("status", "status_value").order(5).style(
                HudStyleOverride {
                    font_size: Some(24.0),
                    ..default()
                },
            ))
            .unwrap();
        app.update();
        assert_eq!(root_for(&mut app, "status"), root);
        assert_eq!(
            children_for(&mut app, "status"),
            vec![(HudChildKind::Text, text_child)]
        );

        app.world_mut()
            .resource_mut::<HudRegistry>()
            .insert_or_replace(HudElementDefinition::new(
                "status",
                HudPresentation::Segments(HudSegments::new("charges", 3)),
            ))
            .unwrap();
        app.world_mut()
            .resource_mut::<HudValueSet>()
            .insert_or_replace("charges", 2_u32);
        app.update();
        assert_eq!(root_for(&mut app, "status"), root);
        assert!(app.world().get_entity(text_child).is_err());
        let first_segments = children_for(&mut app, "status");
        assert_eq!(first_segments.len(), 3);

        app.world_mut()
            .resource_mut::<HudRegistry>()
            .insert_or_replace(HudElementDefinition::new(
                "status",
                HudPresentation::Segments(HudSegments::new("charges", 5)),
            ))
            .unwrap();
        app.update();
        assert_eq!(root_for(&mut app, "status"), root);
        let resized = children_for(&mut app, "status");
        assert_eq!(resized.len(), 5);
        for (kind, entity) in first_segments {
            assert!(resized.contains(&(kind, entity)));
        }

        app.world_mut()
            .resource_mut::<HudRegistry>()
            .remove("status")
            .unwrap();
        app.update();
        let mut query = app.world_mut().query::<&HudElementRoot>();
        assert_eq!(query.iter(app.world()).count(), 0);
        assert!(app.world().get_entity(root).is_err());
    }

    #[test]
    fn bar_render_transition_mutates_fill_without_rebuilding_tree() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, VoxelHudPlugin));
        app.world_mut()
            .resource_mut::<HudRegistry>()
            .register(HudElementDefinition::new(
                "energy",
                HudPresentation::Bar(HudBar::new("energy-value")),
            ))
            .unwrap();
        app.world_mut()
            .resource_mut::<HudValueSet>()
            .insert_or_replace("energy-value", HudScalar::new(25.0, 0.0, 100.0).unwrap());
        app.update();

        let root = root_for(&mut app, "energy");
        let fill = children_for(&mut app, "energy")
            .into_iter()
            .find_map(|(kind, entity)| (kind == HudChildKind::BarFill).then_some(entity))
            .unwrap();
        assert_eq!(app.world().get::<Node>(fill).unwrap().width, Val::Px(55.0));

        app.world_mut()
            .resource_mut::<HudValueSet>()
            .insert_or_replace("energy-value", HudScalar::new(75.0, 0.0, 100.0).unwrap());
        app.update();

        assert_eq!(root_for(&mut app, "energy"), root);
        assert!(children_for(&mut app, "energy").contains(&(HudChildKind::BarFill, fill)));
        assert_eq!(app.world().get::<Node>(fill).unwrap().width, Val::Px(165.0));
    }

    #[test]
    fn reconciliation_orders_ties_and_reparents_without_replacing_roots() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, VoxelHudPlugin));
        for id in ["zeta", "alpha"] {
            app.world_mut()
                .resource_mut::<HudRegistry>()
                .register(
                    text_definition(id, id)
                        .order(7)
                        .placement(HudPlacement::new(HudAnchor::BottomRight)),
                )
                .unwrap();
            app.world_mut()
                .resource_mut::<HudValueSet>()
                .insert_or_replace(id, id);
        }
        app.update();

        let alpha = root_for(&mut app, "alpha");
        let zeta = root_for(&mut app, "zeta");
        let mut anchors = app
            .world_mut()
            .query::<(Entity, &HudAnchorRoot, &Children)>();
        let bottom_right = anchors
            .iter(app.world())
            .find(|(_, anchor, _)| {
                anchor.0
                    == HudAnchorKey {
                        anchor: HudAnchor::BottomRight,
                        direction: HudStackDirection::Vertical,
                    }
            })
            .unwrap();
        assert_eq!(bottom_right.2.as_ref(), &[alpha, zeta]);

        app.world_mut()
            .resource_mut::<HudRegistry>()
            .insert_or_replace(
                text_definition("zeta", "zeta").order(7).placement(
                    HudPlacement::new(HudAnchor::TopLeft)
                        .offset(12.0, 8.0)
                        .stack_direction(HudStackDirection::Horizontal),
                ),
            )
            .unwrap();
        app.update();
        assert_eq!(root_for(&mut app, "zeta"), zeta);
        let zeta_parent = app.world().get::<ChildOf>(zeta).unwrap().parent();
        let anchor = app.world().get::<HudAnchorRoot>(zeta_parent).unwrap();
        assert_eq!(anchor.0.anchor, HudAnchor::TopLeft);
        assert_eq!(anchor.0.direction, HudStackDirection::Horizontal);
        let node = app.world().get::<Node>(zeta).unwrap();
        assert_eq!(node.margin.left, Val::Px(12.0));
        assert_eq!(node.margin.top, Val::Px(8.0));
    }

    #[derive(Resource, Default)]
    struct PublishOnce(bool);

    fn register_startup_hud(mut registry: ResMut<HudRegistry>) {
        registry
            .register(text_definition("same-frame", "same-frame-value"))
            .unwrap();
    }

    fn publish_same_frame(
        mut once: ResMut<PublishOnce>,
        mut values: ResMut<HudValueSet>,
        mut changed: MessageWriter<HudValueChanged>,
    ) {
        if !once.0 {
            set_hud_value("same-frame-value", "visible now", &mut values, &mut changed);
            once.0 = true;
        }
    }

    #[test]
    fn publish_set_updates_startup_definition_in_the_same_app_update() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, VoxelHudPlugin));
        app.init_resource::<PublishOnce>();
        app.add_systems(Startup, register_startup_hud);
        app.add_systems(Update, publish_same_frame.in_set(HudSystemSet::Publish));

        app.update();

        let child = children_for(&mut app, "same-frame")
            .into_iter()
            .find_map(|(kind, entity)| (kind == HudChildKind::Text).then_some(entity))
            .unwrap();
        assert_eq!(app.world().get::<Text>(child).unwrap().0, "visible now");
        let root = root_for(&mut app, "same-frame");
        assert_eq!(
            *app.world().get::<Visibility>(root).unwrap(),
            Visibility::Inherited
        );
    }

    #[test]
    fn unchanged_and_unrelated_values_do_not_mutate_retained_text() {
        #[derive(Resource, Default)]
        struct ChangedTexts(Vec<Vec<String>>);

        fn record_changed_texts(
            changed: Query<&HudChildIdentity, Changed<Text>>,
            mut observed: ResMut<ChangedTexts>,
        ) {
            let mut ids: Vec<_> = changed
                .iter()
                .map(|identity| identity.element.as_str().to_owned())
                .collect();
            ids.sort();
            observed.0.push(ids);
        }

        let mut app = App::new();
        app.add_plugins((MinimalPlugins, VoxelHudPlugin));
        app.init_resource::<ChangedTexts>();
        app.add_systems(Update, record_changed_texts.after(HudSystemSet::Render));
        for id in ["alpha", "beta"] {
            app.world_mut()
                .resource_mut::<HudRegistry>()
                .register(text_definition(id, id))
                .unwrap();
            app.world_mut()
                .resource_mut::<HudValueSet>()
                .insert_or_replace(id, id);
        }
        app.update();

        app.world_mut()
            .resource_mut::<HudValueSet>()
            .insert_or_replace("alpha", "changed");
        app.update();
        assert_eq!(
            app.world().resource::<ChangedTexts>().0.last().unwrap(),
            &["alpha"]
        );

        app.update();
        assert!(
            app.world()
                .resource::<ChangedTexts>()
                .0
                .last()
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn missing_asset_server_hides_images_without_starting_asset_io() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, VoxelHudPlugin));
        app.world_mut()
            .resource_mut::<HudRegistry>()
            .register(HudElementDefinition::new(
                "icon",
                HudPresentation::Icon(HudIcon::asset("textures/icon.png")),
            ))
            .unwrap();
        let mut cursor = app
            .world()
            .resource::<Messages<HudDiagnostic>>()
            .get_cursor_current();

        app.update();

        let root = root_for(&mut app, "icon");
        assert_eq!(
            *app.world().get::<Visibility>(root).unwrap(),
            Visibility::Hidden
        );
        assert!(app.world().resource::<HudAssetCache>().handles.is_empty());
        let diagnostics: Vec<_> = cursor
            .read(app.world().resource::<Messages<HudDiagnostic>>())
            .cloned()
            .collect();
        assert_eq!(diagnostics.len(), 1);
        assert!(matches!(
            diagnostics[0].kind,
            HudDiagnosticKind::AssetUnavailable { .. }
        ));

        app.update();
        assert_eq!(
            cursor
                .read(app.world().resource::<Messages<HudDiagnostic>>())
                .count(),
            0
        );
    }
}
