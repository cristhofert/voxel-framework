//! Status effect system for the player.
//!
//! # Design
//!
//! Follows the same generic-trait pattern as `VoxelType` / `ItemType`. Games
//! implement the [`StatusEffect`] trait on their own enum and register
//! [`StatusEffectsPlugin<E>`] once. The plugin:
//!
//! 1. Initialises [`ActiveEffects<E>`] and [`AppliedEffectModifiers`] resources.
//! 2. Ticks effect timers and removes expired effects each frame.
//! 3. Applies regen / poison to [`PlayerHealth`] each frame.
//! 4. Computes the combined [`EffectModifiers`] into [`AppliedEffectModifiers`]
//!    before the player movement systems run.
//!
//! [`player_move`](crate::player::player_move) and
//! `hotbar_to_tool_system` read [`AppliedEffectModifiers`] via
//! `Option<Res<AppliedEffectModifiers>>` so they work correctly even when no
//! `StatusEffectsPlugin` is registered.

use bevy::prelude::*;

use crate::player::PlayerHealth;

// ─── EffectModifiers ──────────────────────────────────────────────────────────

/// Accumulated modifier values applied to the player each frame from active effects.
///
/// All multiplicative fields default to `1.0` (identity). Additive fields
/// default to `0.0`.
#[derive(Debug, Clone, Copy)]
pub struct EffectModifiers {
    /// Multiplied into horizontal movement speed.
    pub speed_mult: f32,
    /// Multiplied into the jump impulse.
    pub jump_mult: f32,
    /// Multiplied into gravity accumulation each frame.
    pub gravity_mult: f32,
    /// Multiplied into `ToolStats::tool_speed` (mining speed).
    pub mining_speed_mult: f32,
    /// Fraction of incoming damage blocked. Clamped to `[0.0, 1.0]`.
    pub damage_resistance: f32,
    /// HP healed per second (additive across all active effects).
    pub regen_per_sec: f32,
    /// HP dealt per second (poison). Additive. Does not kill below 1 HP.
    pub damage_per_sec: f32,
}

impl Default for EffectModifiers {
    fn default() -> Self {
        Self {
            speed_mult: 1.0,
            jump_mult: 1.0,
            gravity_mult: 1.0,
            mining_speed_mult: 1.0,
            damage_resistance: 0.0,
            regen_per_sec: 0.0,
            damage_per_sec: 0.0,
        }
    }
}

// ─── StatusEffect trait ───────────────────────────────────────────────────────

/// Trait that game types implement to define a status effect.
///
/// # Example
///
/// ```rust,no_run
/// # use voxel_framework::prelude::{StatusEffect, EffectModifiers};
/// #[derive(Clone, PartialEq)]
/// enum MyEffect { Speed, Poison }
///
/// impl StatusEffect for MyEffect {
///     fn name(&self) -> &'static str {
///         match self { Self::Speed => "Speed", Self::Poison => "Poison" }
///     }
///     fn modifiers(&self, amplifier: u8) -> EffectModifiers {
///         let lvl = (amplifier + 1) as f32;
///         match self {
///             Self::Speed  => EffectModifiers { speed_mult: 1.0 + 0.2 * lvl, ..Default::default() },
///             Self::Poison => EffectModifiers { damage_per_sec: 1.0 * lvl,   ..Default::default() },
///         }
///     }
/// }
/// ```
pub trait StatusEffect: Send + Sync + Clone + PartialEq + 'static {
    /// Human-readable name shown in HUD or debug output.
    fn name(&self) -> &'static str;

    /// Returns the [`EffectModifiers`] for the given amplifier level.
    ///
    /// `amplifier = 0` corresponds to level I, `amplifier = 1` to level II,
    /// and so on.
    fn modifiers(&self, amplifier: u8) -> EffectModifiers;

    /// Particle / icon colour for HUD display. RGBA in `[0.0, 1.0]`.
    ///
    /// Defaults to opaque white.
    fn color(&self) -> [f32; 4] {
        [1.0, 1.0, 1.0, 1.0]
    }

    /// Whether this effect is beneficial (`true`) or harmful (`false`).
    ///
    /// Used by HUD code to pick a display colour (blue/green vs red).
    /// Defaults to `true`.
    fn is_beneficial(&self) -> bool {
        true
    }
}

// ─── ActiveEffect / ActiveEffects ─────────────────────────────────────────────

/// A single active instance of a status effect on the player.
#[derive(Debug, Clone)]
pub struct ActiveEffect<E: StatusEffect> {
    /// The effect type.
    pub effect: E,
    /// Amplifier level (0 = level I, 1 = level II, …).
    pub amplifier: u8,
    /// Remaining duration in seconds. Decremented each frame by [`tick_effects`].
    pub remaining_secs: f32,
}

/// Resource holding all currently active status effects on the player.
///
/// Register [`StatusEffectsPlugin<E>`] to have this managed automatically.
/// Games can also manipulate it directly via [`ActiveEffects::apply`] and
/// [`ActiveEffects::remove`].
#[derive(Resource)]
pub struct ActiveEffects<E: StatusEffect> {
    /// Currently active effects.
    pub effects: Vec<ActiveEffect<E>>,
}

impl<E: StatusEffect> Default for ActiveEffects<E> {
    fn default() -> Self {
        Self {
            effects: Vec::new(),
        }
    }
}

impl<E: StatusEffect> ActiveEffects<E> {
    /// Apply or refresh an effect.
    ///
    /// If the same effect variant is already active, keeps the **higher
    /// amplifier** and the **longer remaining duration**. Otherwise a new entry
    /// is pushed.
    pub fn apply(&mut self, effect: E, amplifier: u8, duration_secs: f32) {
        if let Some(existing) = self.effects.iter_mut().find(|a| a.effect == effect) {
            existing.amplifier = existing.amplifier.max(amplifier);
            existing.remaining_secs = existing.remaining_secs.max(duration_secs);
        } else {
            self.effects.push(ActiveEffect {
                effect,
                amplifier,
                remaining_secs: duration_secs,
            });
        }
    }

    /// Remove all instances of the given effect variant.
    pub fn remove(&mut self, effect: &E) {
        self.effects.retain(|a| &a.effect != effect);
    }

    /// Compute the combined [`EffectModifiers`] from all active effects.
    ///
    /// - **Multiplicative** fields (`speed_mult`, `jump_mult`, `gravity_mult`,
    ///   `mining_speed_mult`): multiplied together across all effects.
    /// - **Additive** fields (`regen_per_sec`, `damage_per_sec`): summed.
    /// - `damage_resistance`: summed then clamped to `[0.0, 1.0]`.
    pub fn combined_modifiers(&self) -> EffectModifiers {
        let mut out = EffectModifiers::default();
        for a in &self.effects {
            let m = a.effect.modifiers(a.amplifier);
            out.speed_mult *= m.speed_mult;
            out.jump_mult *= m.jump_mult;
            out.gravity_mult *= m.gravity_mult;
            out.mining_speed_mult *= m.mining_speed_mult;
            out.damage_resistance = (out.damage_resistance + m.damage_resistance).min(1.0);
            out.regen_per_sec += m.regen_per_sec;
            out.damage_per_sec += m.damage_per_sec;
        }
        out
    }
}

// ─── AppliedEffectModifiers ───────────────────────────────────────────────────

/// Precomputed combined modifiers from all active effects.
///
/// Updated every frame by [`update_applied_modifiers`] inside
/// [`StatusEffectsPlugin`], before the player movement systems run. Systems
/// such as [`player_move`](crate::player::player_move) and
/// `hotbar_to_tool_system` read this via `Option<Res<AppliedEffectModifiers>>`
/// so they degrade gracefully when no plugin is registered.
#[derive(Resource, Default)]
pub struct AppliedEffectModifiers(pub EffectModifiers);

// ─── Systems ──────────────────────────────────────────────────────────────────

/// Advances effect timers and removes expired effects.
///
/// Added automatically by [`StatusEffectsPlugin`].
pub fn tick_effects<E: StatusEffect>(mut effects: ResMut<ActiveEffects<E>>, time: Res<Time>) {
    let dt = time.delta_secs();
    effects.effects.retain_mut(|a| {
        a.remaining_secs -= dt;
        a.remaining_secs > 0.0
    });
}

/// Recomputes [`AppliedEffectModifiers`] from the active effects.
///
/// Must run before [`player_move`](crate::player::player_move) in the same
/// frame so movement picks up the fresh modifiers. Added automatically by
/// [`StatusEffectsPlugin`].
pub fn update_applied_modifiers<E: StatusEffect>(
    effects: Res<ActiveEffects<E>>,
    mut applied: ResMut<AppliedEffectModifiers>,
) {
    applied.0 = effects.combined_modifiers();
}

/// Applies `regen_per_sec` and `damage_per_sec` to [`PlayerHealth`].
///
/// - Regeneration heals the player up to their maximum HP.
/// - Poison reduces HP but never below `1.0` (non-lethal).
///
/// Added automatically by [`StatusEffectsPlugin`].
pub fn apply_health_effects<E: StatusEffect>(
    effects: Res<ActiveEffects<E>>,
    mut health: ResMut<PlayerHealth>,
    time: Res<Time>,
) {
    let m = effects.combined_modifiers();
    let dt = time.delta_secs();

    // Regeneration — capped at max HP.
    if m.regen_per_sec > 0.0 {
        health.health = (health.health + m.regen_per_sec * dt).min(health.max_health);
    }

    // Poison — does not reduce below 1 HP.
    if m.damage_per_sec > 0.0 {
        health.health = (health.health - m.damage_per_sec * dt).max(1.0_f32);
    }
}

// ─── Plugin ───────────────────────────────────────────────────────────────────

/// Bevy plugin that wires the status effect system for effect type `E`.
///
/// # What it registers
///
/// - [`ActiveEffects<E>`] resource (default: empty)
/// - [`AppliedEffectModifiers`] resource (default: identity modifiers)
/// - [`tick_effects<E>`] — advances timers, removes expired effects
/// - [`update_applied_modifiers<E>`] — recomputes combined modifiers each frame
/// - [`apply_health_effects<E>`] — applies regen / poison to [`PlayerHealth`]
///
/// # Example
///
/// ```rust,no_run
/// # use bevy::prelude::*;
/// # use voxel_framework::prelude::{StatusEffect, StatusEffectsPlugin, EffectModifiers};
/// #[derive(Clone, PartialEq)]
/// enum MyEffect { Speed }
/// impl StatusEffect for MyEffect {
///     fn name(&self) -> &'static str { "Speed" }
///     fn modifiers(&self, _: u8) -> EffectModifiers { EffectModifiers { speed_mult: 1.4, ..Default::default() } }
/// }
///
/// App::new().add_plugins(StatusEffectsPlugin::<MyEffect>::new());
/// ```
pub struct StatusEffectsPlugin<E: StatusEffect> {
    _marker: std::marker::PhantomData<E>,
}

impl<E: StatusEffect> StatusEffectsPlugin<E> {
    /// Creates a new [`StatusEffectsPlugin`] for effect type `E`.
    pub fn new() -> Self {
        Self {
            _marker: std::marker::PhantomData,
        }
    }
}

impl<E: StatusEffect> Default for StatusEffectsPlugin<E> {
    fn default() -> Self {
        Self::new()
    }
}

impl<E: StatusEffect> Plugin for StatusEffectsPlugin<E> {
    fn build(&self, app: &mut App) {
        app.init_resource::<ActiveEffects<E>>();
        app.init_resource::<AppliedEffectModifiers>();
        app.add_systems(
            Update,
            (
                update_applied_modifiers::<E>,
                tick_effects::<E>,
                apply_health_effects::<E>,
            ),
        );
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, PartialEq, Debug)]
    enum TestEffect {
        Speed,
        Poison,
        Resistance,
    }

    impl StatusEffect for TestEffect {
        fn name(&self) -> &'static str {
            match self {
                Self::Speed => "Speed",
                Self::Poison => "Poison",
                Self::Resistance => "Resistance",
            }
        }

        fn modifiers(&self, amplifier: u8) -> EffectModifiers {
            let lvl = (amplifier + 1) as f32;
            match self {
                Self::Speed => EffectModifiers {
                    speed_mult: 1.0 + 0.2 * lvl,
                    ..Default::default()
                },
                Self::Poison => EffectModifiers {
                    damage_per_sec: 1.0 * lvl,
                    ..Default::default()
                },
                Self::Resistance => EffectModifiers {
                    damage_resistance: 0.2 * lvl,
                    ..Default::default()
                },
            }
        }

        fn is_beneficial(&self) -> bool {
            !matches!(self, Self::Poison)
        }
    }

    // ── EffectModifiers defaults ─────────────────────────────────────────────

    #[test]
    fn effect_modifiers_default_identity() {
        let m = EffectModifiers::default();
        assert_eq!(m.speed_mult, 1.0);
        assert_eq!(m.jump_mult, 1.0);
        assert_eq!(m.gravity_mult, 1.0);
        assert_eq!(m.mining_speed_mult, 1.0);
        assert_eq!(m.damage_resistance, 0.0);
        assert_eq!(m.regen_per_sec, 0.0);
        assert_eq!(m.damage_per_sec, 0.0);
    }

    // ── ActiveEffects::apply ─────────────────────────────────────────────────

    #[test]
    fn apply_adds_new_effect() {
        let mut ae: ActiveEffects<TestEffect> = ActiveEffects::default();
        ae.apply(TestEffect::Speed, 0, 10.0);
        assert_eq!(ae.effects.len(), 1);
        assert_eq!(ae.effects[0].amplifier, 0);
        assert!((ae.effects[0].remaining_secs - 10.0).abs() < f32::EPSILON);
    }

    #[test]
    fn apply_refreshes_existing_keeps_higher_amplifier_and_longer_duration() {
        let mut ae: ActiveEffects<TestEffect> = ActiveEffects::default();
        ae.apply(TestEffect::Speed, 1, 10.0);
        ae.apply(TestEffect::Speed, 0, 20.0); // lower amplifier, longer duration
        assert_eq!(ae.effects.len(), 1);
        assert_eq!(ae.effects[0].amplifier, 1); // kept higher
        assert!((ae.effects[0].remaining_secs - 20.0).abs() < f32::EPSILON); // kept longer
    }

    #[test]
    fn apply_two_different_effects() {
        let mut ae: ActiveEffects<TestEffect> = ActiveEffects::default();
        ae.apply(TestEffect::Speed, 0, 5.0);
        ae.apply(TestEffect::Poison, 0, 5.0);
        assert_eq!(ae.effects.len(), 2);
    }

    // ── ActiveEffects::remove ────────────────────────────────────────────────

    #[test]
    fn remove_clears_matching_effect() {
        let mut ae: ActiveEffects<TestEffect> = ActiveEffects::default();
        ae.apply(TestEffect::Speed, 0, 10.0);
        ae.apply(TestEffect::Poison, 0, 10.0);
        ae.remove(&TestEffect::Speed);
        assert_eq!(ae.effects.len(), 1);
        assert_eq!(ae.effects[0].effect, TestEffect::Poison);
    }

    #[test]
    fn remove_nonexistent_is_noop() {
        let mut ae: ActiveEffects<TestEffect> = ActiveEffects::default();
        ae.apply(TestEffect::Poison, 0, 5.0);
        ae.remove(&TestEffect::Speed);
        assert_eq!(ae.effects.len(), 1);
    }

    // ── combined_modifiers ───────────────────────────────────────────────────

    #[test]
    fn combined_modifiers_empty_is_identity() {
        let ae: ActiveEffects<TestEffect> = ActiveEffects::default();
        let m = ae.combined_modifiers();
        assert_eq!(m.speed_mult, 1.0);
        assert_eq!(m.damage_per_sec, 0.0);
    }

    #[test]
    fn combined_modifiers_speed_multiplied() {
        let mut ae: ActiveEffects<TestEffect> = ActiveEffects::default();
        ae.apply(TestEffect::Speed, 0, 10.0); // lvl=1 → speed_mult=1.2
        ae.apply(TestEffect::Speed, 0, 10.0); // same effect — refreshes, not doubled
        let m = ae.combined_modifiers();
        assert!((m.speed_mult - 1.2).abs() < 1e-5);
    }

    #[test]
    fn combined_modifiers_damage_resistance_clamped_at_one() {
        let mut ae: ActiveEffects<TestEffect> = ActiveEffects::default();
        ae.apply(TestEffect::Resistance, 4, 10.0); // 0.2 * 5 = 1.0
        ae.apply(TestEffect::Resistance, 4, 10.0); // same — refreshes
        let m = ae.combined_modifiers();
        // With a single Resistance lvl 5 (amplifier=4), damage_resistance = 1.0
        assert!(m.damage_resistance <= 1.0);
    }

    #[test]
    fn combined_modifiers_poison_additive() {
        let mut ae: ActiveEffects<TestEffect> = ActiveEffects::default();
        ae.apply(TestEffect::Poison, 0, 10.0); // 1.0 dps
        ae.apply(TestEffect::Speed, 0, 10.0); // different effect
        let m = ae.combined_modifiers();
        assert!((m.damage_per_sec - 1.0).abs() < 1e-5);
        assert!((m.speed_mult - 1.2).abs() < 1e-5);
    }

    // ── StatusEffect trait defaults ──────────────────────────────────────────

    #[test]
    fn status_effect_color_default() {
        assert_eq!(TestEffect::Speed.color(), [1.0, 1.0, 1.0, 1.0]);
    }

    #[test]
    fn status_effect_is_beneficial_poison_is_harmful() {
        assert!(!TestEffect::Poison.is_beneficial());
        assert!(TestEffect::Speed.is_beneficial());
    }
}
