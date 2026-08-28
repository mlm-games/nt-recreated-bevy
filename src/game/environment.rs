//! Functional floor surfaces and environmental prop payloads.
//!
//! The ordinary `Prop` component remains the collision / HP representation.
//! This module adds optional behavior components so existing props do not
//! need a large enum field or a migration of every `Prop { ... }` literal.

use bevy::prelude::*;

use crate::game::combat::Explosion;
use crate::game::components::*;
use crate::game::content::{AssetCatalog, enemy_def, sprite_exact};
use crate::game::secret_areas::SecretTriggers;
use game_utils_bevy::hit_flash::HitFlash;
use game_utils_bevy::screen_effects::{ScreenEffects, Trauma};
use game_utils_bevy::vfx::VfxSpawner;

// Functional floor surfaces

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SurfaceKind {
    Cobweb,
    Ice,
}

#[derive(Component, Clone, Copy, Debug)]
pub struct SurfaceZone {
    pub kind: SurfaceKind,
    pub half_size: Vec2,
}

#[derive(Component, Clone, Copy, Debug)]
pub struct SurfacePulse {
    pub speed: f32,
    pub min_alpha: f32,
    pub max_alpha: f32,
    pub phase: f32,
}

impl SurfacePulse {
    pub fn subtle(phase: f32) -> Self {
        Self {
            speed: 1.8,
            min_alpha: 0.55,
            max_alpha: 0.86,
            phase,
        }
    }

    pub fn hazard(phase: f32) -> Self {
        Self {
            speed: 4.2,
            min_alpha: 0.52,
            max_alpha: 0.95,
            phase,
        }
    }
}

/// Axis-aligned zone test used both by the ECS system and unit tests.
pub fn point_in_zone(point: Vec2, center: Vec2, half_size: Vec2) -> bool {
    let delta = (point - center).abs();
    delta.x <= half_size.x && delta.y <= half_size.y
}

/// Select the strongest surface under an actor.
///
/// Cobweb takes priority over ice if generation places both zones on the same
/// cell - this prevents ice compensation from defeating web slowdown.
pub fn surface_at_point(
    point: Vec2,
    zones: impl IntoIterator<Item = (Vec2, SurfaceZone)>,
) -> Option<SurfaceKind> {
    let mut result = None;

    for (center, zone) in zones {
        if !point_in_zone(point, center, zone.half_size) {
            continue;
        }

        match zone.kind {
            SurfaceKind::Cobweb => return Some(SurfaceKind::Cobweb),
            SurfaceKind::Ice => result = Some(SurfaceKind::Ice),
        }
    }

    result
}

/// Post-movement velocity adjustment for a functional floor.
///
/// Runs after ordinary movement:
/// - Cobweb strongly damps existing movement and caps maximum speed.
/// - Ice compensates most of the friction already applied by movement,
///   producing a long glide without rewriting the input system.
pub fn surface_velocity(
    kind: SurfaceKind,
    velocity: Vec2,
    dt: f32,
    base_friction: f32,
    max_speed: f32,
) -> Vec2 {
    match kind {
        SurfaceKind::Cobweb => {
            let retention = 0.72_f32.powf(dt * 60.0);
            let mut next = velocity * retention;
            let cap = max_speed.max(1.0) * 0.52;

            if next.length() > cap {
                next = next.normalize_or_zero() * cap;
            }

            next
        }

        SurfaceKind::Ice => {
            // Undo most-but deliberately not all-of the regular movement
            // friction; the 0.992 term guarantees eventual stillness.
            let friction = base_friction.clamp(0.05, 0.999);
            let compensation = (1.0 / friction).powf(dt * 60.0);
            let retention = 0.992_f32.powf(dt * 60.0);

            let mut next = velocity * compensation * retention;
            let cap = max_speed.max(1.0) * 1.28;

            if next.length() > cap {
                next = next.normalize_or_zero() * cap;
            }

            next
        }
    }
}

#[allow(clippy::type_complexity)]
pub fn apply_surface_effects(
    time: Res<Time<Fixed>>,
    zones: Query<(&Transform, &SurfaceZone)>,
    mut actors: Query<
        (
            &Transform,
            &mut Velocity,
            Option<&Player>,
            Option<&Enemy>,
            Option<&Dash>,
        ),
        (Without<SurfaceZone>, Without<Projectile>),
    >,
) {
    let dt = time.delta_secs();

    for (tf, mut velocity, player, enemy, dash) in &mut actors {
        // Character dashes carry explicit velocity that should be preserved.
        if dash.is_some() {
            continue;
        }

        let point = tf.translation.truncate();
        let Some(surface) = surface_at_point(
            point,
            zones
                .iter()
                .map(|(ztf, zone)| (ztf.translation.truncate(), *zone)),
        ) else {
            continue;
        };

        let (friction, max_speed) = if let Some(player) = player {
            (player.friction, player.speed * player.speed_mult)
        } else if let Some(enemy) = enemy {
            let def = enemy_def(enemy.kind);
            (0.84, def.speed.max(40.0))
        } else {
            (0.84, 240.0)
        };

        velocity.0 = surface_velocity(surface, velocity.0, dt, friction, max_speed);
    }
}

// Environmental hazards

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EnvironmentHazardKind {
    Fire,
    Toxic,
}

impl EnvironmentHazardKind {
    #[allow(dead_code)] // stable HitId mapping for future damage attribution
    pub fn hit_id(self) -> HitId {
        match self {
            EnvironmentHazardKind::Fire => HitId::Fire,
            EnvironmentHazardKind::Toxic => HitId::Toxic,
        }
    }

    pub fn color(self) -> Color {
        match self {
            EnvironmentHazardKind::Fire => Color::srgba(1.0, 0.43, 0.10, 0.36),
            EnvironmentHazardKind::Toxic => Color::srgba(0.30, 0.88, 0.30, 0.36),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct EnvironmentHazardSpec {
    pub kind: EnvironmentHazardKind,
    pub radius: f32,
    pub damage: i32,
    pub duration: f32,
    pub tick: f32,
    pub hurts_player: bool,
    pub hurts_enemies: bool,
}

impl EnvironmentHazardSpec {
    pub fn toxic_barrel() -> Self {
        Self {
            kind: EnvironmentHazardKind::Toxic,
            radius: 62.0,
            damage: 1,
            duration: 3.0,
            tick: 0.24,
            hurts_player: true,
            hurts_enemies: true,
        }
    }

    pub fn fire_trap() -> Self {
        Self {
            kind: EnvironmentHazardKind::Fire,
            radius: 42.0,
            damage: 2,
            duration: 9_999.0,
            tick: 0.38,
            hurts_player: true,
            hurts_enemies: true,
        }
    }

    pub fn mine_fire() -> Self {
        Self {
            kind: EnvironmentHazardKind::Fire,
            radius: 48.0,
            damage: 1,
            duration: 0.9,
            tick: 0.18,
            hurts_player: true,
            hurts_enemies: true,
        }
    }
}

#[derive(Component)]
pub struct EnvironmentHazard {
    pub spec: EnvironmentHazardSpec,
    pub life: Timer,
    pub damage_tick: Timer,
}

impl EnvironmentHazard {
    pub fn new(spec: EnvironmentHazardSpec) -> Self {
        Self {
            life: Timer::from_seconds(spec.duration.max(0.01), TimerMode::Once),
            damage_tick: Timer::from_seconds(spec.tick.max(0.01), TimerMode::Repeating),
            spec,
        }
    }
}

pub fn spawn_environment_hazard(commands: &mut Commands, pos: Vec2, spec: EnvironmentHazardSpec) {
    commands.spawn((
        GameCleanup,
        LevelCleanup,
        EnvironmentHazard::new(spec),
        SurfacePulse::hazard(pos.x * 0.013 + pos.y * 0.009),
        Sprite {
            color: spec.kind.color(),
            custom_size: Some(Vec2::splat(spec.radius * 2.0)),
            ..default()
        },
        Transform::from_translation(pos.extend(7.0)),
    ));
}

#[allow(clippy::type_complexity)]
pub fn tick_environment_hazards(
    time: Res<Time<Fixed>>,
    mut commands: Commands,
    mut secrets: ResMut<SecretTriggers>,
    mut hazards: Query<(Entity, &Transform, &mut EnvironmentHazard)>,
    mut targets: Query<
        (Entity, &Transform, &Team, &mut Health, Option<&Player>),
        (
            Without<EnvironmentHazard>,
            Without<Projectile>,
            Without<Explosion>,
        ),
    >,
) {
    for (hazard_entity, hazard_tf, mut hazard) in hazards.iter_mut() {
        hazard.life.tick(time.delta());
        hazard.damage_tick.tick(time.delta());

        if hazard.life.just_finished() {
            commands.entity(hazard_entity).despawn();
            continue;
        }

        if !hazard.damage_tick.just_finished() {
            continue;
        }

        let center = hazard_tf.translation.truncate();

        for (target_entity, target_tf, team, mut health, player) in targets.iter_mut() {
            let is_player = *team == Team::Player;

            if is_player && !hazard.spec.hurts_player {
                continue;
            }
            if !is_player && !hazard.spec.hurts_enemies {
                continue;
            }
            if target_tf.translation.truncate().distance(center) > hazard.spec.radius {
                continue;
            }
            if is_player && !health.invuln.is_finished() {
                continue;
            }

            // Boiling Veins protects low-health players from fire hazards.
            if is_player
                && hazard.spec.kind == EnvironmentHazardKind::Fire
                && let Some(player) = player
                && player.boiling_veins
                && health.hp <= player.veins_threshold
            {
                continue;
            }

            health.hp -= hazard.spec.damage;

            if is_player {
                health.invuln = Timer::from_seconds(5.0 / 30.0, TimerMode::Once);
                secrets.mark_damage_taken();
            }

            HitFlash::apply(&mut commands, target_entity, hazard.spec.kind.color(), 0.08);
            VfxSpawner::spawn_damage_number(
                &mut commands,
                hazard.spec.damage,
                target_tf.translation.truncate(),
                hazard.spec.kind.color(),
            );
        }
    }
}

// Prop terminal payloads

#[derive(Clone, Copy, Debug)]
pub struct ExplosionPayload {
    pub radius: f32,
    pub damage: i32,
}

#[derive(Component, Clone, Copy, Debug, Default)]
pub struct PropDeathEffect {
    pub explosion: Option<ExplosionPayload>,
    pub hazard: Option<EnvironmentHazardSpec>,
}

impl PropDeathEffect {
    pub fn toxic_barrel() -> Self {
        Self {
            explosion: Some(ExplosionPayload {
                radius: 95.0,
                damage: 5,
            }),
            hazard: Some(EnvironmentHazardSpec::toxic_barrel()),
        }
    }

    pub fn car() -> Self {
        Self {
            explosion: Some(ExplosionPayload {
                radius: 155.0,
                damage: 8,
            }),
            hazard: None,
        }
    }

    pub fn mine() -> Self {
        Self {
            explosion: Some(ExplosionPayload {
                radius: 118.0,
                damage: 7,
            }),
            hazard: Some(EnvironmentHazardSpec::mine_fire()),
        }
    }

    pub fn legacy_barrel() -> Self {
        Self {
            explosion: Some(ExplosionPayload {
                radius: 110.0,
                damage: 6,
            }),
            hazard: None,
        }
    }
}

/// Shared terminal path for props destroyed by bullets, explosions, melee,
/// Hammerhead, or a future chain reaction.
pub fn spawn_prop_death_effect(
    commands: &mut Commands,
    pos: Vec2,
    explicit: Option<PropDeathEffect>,
    legacy_explosive: bool,
    source: Option<DamageSource>,
) {
    let effect = explicit.or_else(|| legacy_explosive.then_some(PropDeathEffect::legacy_barrel()));

    let Some(effect) = effect else {
        VfxSpawner::spawn_burst(
            commands,
            pos,
            8,
            Color::srgb(0.78, 0.65, 0.42),
            (50.0, 150.0),
        );
        return;
    };

    if let Some(explosion) = effect.explosion {
        // Neutral-hazard convention: Team::Player so the enemy-side pass sees
        // it, hits_player=true so the player-side pass also sees it.
        commands.spawn((
            GameCleanup,
            LevelCleanup,
            Explosion {
                timer: Timer::from_seconds(0.04, TimerMode::Once),
                radius: explosion.radius,
                damage: explosion.damage,
                team: Team::Player,
                hits_player: true,
                source,
            },
            Transform::from_translation(pos.extend(20.0)),
        ));

        VfxSpawner::spawn_burst(
            commands,
            pos,
            24,
            Color::srgb(1.0, 0.52, 0.16),
            (100.0, 360.0),
        );
    }

    if let Some(hazard) = effect.hazard {
        spawn_environment_hazard(commands, pos, hazard);
    }
}

// Mines

#[derive(Component, Clone, Copy, Debug)]
pub struct ProximityMine {
    pub trigger_radius: f32,
    pub payload: PropDeathEffect,
}

impl Default for ProximityMine {
    fn default() -> Self {
        Self {
            trigger_radius: 54.0,
            payload: PropDeathEffect::mine(),
        }
    }
}

pub fn tick_proximity_mines(
    mut commands: Commands,
    mut trauma: ResMut<Trauma>,
    mines: Query<(Entity, &Transform, &ProximityMine), With<Prop>>,
    targets: Query<(&Transform, &Team), Without<ProximityMine>>,
) {
    for (mine_entity, mine_tf, mine) in mines.iter() {
        let center = mine_tf.translation.truncate();

        let triggered = targets.iter().any(|(target_tf, team)| {
            matches!(*team, Team::Player | Team::Enemy)
                && target_tf.translation.truncate().distance(center) <= mine.trigger_radius
        });

        if !triggered {
            continue;
        }

        spawn_prop_death_effect(&mut commands, center, Some(mine.payload), false, None);

        ScreenEffects::add_trauma(&mut trauma, 0.20);
        commands.entity(mine_entity).despawn();
    }
}

// Presentation helpers

pub fn animate_environment(time: Res<Time>, mut q: Query<(&SurfacePulse, &mut Sprite)>) {
    let now = time.elapsed_secs();

    for (pulse, mut sprite) in &mut q {
        let wave = 0.5 + 0.5 * (now * pulse.speed + pulse.phase).sin();
        let alpha = pulse.min_alpha + (pulse.max_alpha - pulse.min_alpha) * wave;

        sprite.color.set_alpha(alpha);
    }
}

/// Load the first available image, otherwise create a visible colored
/// fallback. Unlike `sprite_exact`, intentionally tolerant: environment art is
/// optional and exact pack names can vary.
pub fn sprite_from_candidates(
    catalog: &AssetCatalog,
    asset_server: &AssetServer,
    candidates: &[&str],
    fallback_color: Color,
    fallback_size: Vec2,
) -> Sprite {
    for path in candidates {
        if catalog.has(path) {
            return sprite_exact(catalog, asset_server, path);
        }
    }

    Sprite {
        color: fallback_color,
        custom_size: Some(fallback_size),
        ..default()
    }
}

/// Safety test for environmental spawns generated near arena edges.
#[allow(dead_code)]
pub fn valid_environment_position(pos: Vec2, radius: f32) -> bool {
    pos.x.abs() <= ARENA_W * 0.5 - radius && pos.y.abs() <= ARENA_H * 0.5 - radius
}

// Unit tests
