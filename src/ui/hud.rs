//! Author-defined, game-neutral HUD data model and stores.
//!
//! This module deliberately contains no gameplay-domain assumptions. Authors
//! register presentation definitions and publish already-resolved view values.

use std::collections::HashMap;
use std::fmt;

use bevy::asset::AssetPath;
use bevy::ecs::message::MessageWriter;
use bevy::prelude::{Message, Resource};
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
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
    fn public_ecs_types_implement_their_required_bevy_traits() {
        fn assert_resource<T: Resource>() {}
        fn assert_message<T: Message>() {}

        assert_resource::<HudRegistry>();
        assert_resource::<HudValueSet>();
        assert_resource::<HudTheme>();
        assert_message::<HudValueChanged>();
    }
}
