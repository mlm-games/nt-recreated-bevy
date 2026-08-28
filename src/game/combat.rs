//! Combat: projectile movement, projectile/world collisions, projectile/entity
//! hits, explosion AoE, contact damage, and death resolution.

use bevy::input::gamepad::{Gamepad, GamepadRumbleRequest};
use bevy::prelude::*;
use rand::RngExt;

use crate::app::{AppState, Paused};
use crate::game::audio::GameAudio;
use crate::game::components::*;
use crate::game::content::*;
use crate::game::environment::{PropDeathEffect, spawn_prop_death_effect};
use crate::game::pickups::spawn_pickup;
use crate::game::secret_areas::SecretTriggers;
use crate::game::world::*;
use crate::save::SaveData;
use game_utils_bevy::game_feel::{GameFeel, SlowMotion};
use game_utils_bevy::hit_flash::HitFlash;
use game_utils_bevy::hitstop::HitStop;
use game_utils_bevy::screen_effects::{ChromaticAberration, FlashWhite, ScreenEffects, Trauma};
use game_utils_bevy::transitions::Transition;
use game_utils_bevy::vfx::VfxSpawner;

/// A pending explosion; applies damage once its short fuse expires so queries
/// never conflict with projectile iteration.
#[derive(Component)]
pub struct Explosion {
    pub timer: Timer,
    pub radius: f32,
    pub damage: i32,
    pub team: Team,
    pub hits_player: bool,
    pub source: Option<DamageSource>,
}

/// Bundled asset handles (Bevy caps systems at 16 flat SystemParams).
#[derive(bevy::ecs::system::SystemParam)]
pub struct RadSpawnCtx<'w> {
    catalog: Res<'w, AssetCatalog>,
    asset_server: Res<'w, AssetServer>,
}

/// Read-only prop-death lookups bundled to stay under the param limit.
#[derive(bevy::ecs::system::SystemParam)]
pub struct PropDeathQueries<'w, 's> {
    entrances: Query<'w, 's, &'static SecretEntrance>,
    snowmen: Query<'w, 's, &'static SnowmanAmbush>,
    gold_barrels: Query<'w, 's, &'static GoldBarrelDrop>,
}

pub fn tick_homing_projectiles(
    time: Res<Time<Fixed>>,
    mut q: Query<(&Team, &Transform, &mut Velocity, &Homing), With<Projectile>>,
    enemies: Query<&Transform, With<Enemy>>,
    player_q: Query<&Transform, (With<Player>, Without<Enemy>)>,
) {
    let dt = time.delta_secs();

    for (team, tf, mut vel, homing) in &mut q {
        let pos = tf.translation.truncate();

        let target = match *team {
            Team::Player => {
                let mut best = None::<(f32, Vec2)>;
                for etf in &enemies {
                    let epos = etf.translation.truncate();
                    let d2 = pos.distance_squared(epos);
                    if d2 > homing.acquire_range * homing.acquire_range {
                        continue;
                    }
                    if best.map(|(bd, _)| d2 < bd).unwrap_or(true) {
                        best = Some((d2, epos));
                    }
                }
                best.map(|(_, p)| p)
            }
            Team::Enemy => player_q.single().ok().map(|tf| tf.translation.truncate()),
        };

        let Some(target_pos) = target else {
            continue;
        };

        let speed = vel.0.length();
        if speed <= 1e-4 {
            continue;
        }

        let current_dir = vel.0.normalize_or_zero();
        let desired_dir = (target_pos - pos).normalize_or_zero();
        let step = (homing.turn_rate * dt).clamp(0.0, 1.0);
        let new_dir = current_dir.lerp(desired_dir, step).normalize_or_zero();

        vel.0 = new_dir * speed;
    }
}

pub fn tick_sticky_projectiles(
    mut q: Query<(&mut Transform, &mut Velocity, &mut Sticky), With<Projectile>>,
    targets: Query<&Transform, Without<Projectile>>,
) {
    for (mut tf, mut vel, sticky) in &mut q {
        if !sticky.armed {
            continue;
        }

        vel.0 = Vec2::ZERO;

        if let Some(target) = sticky.stuck_to
            && let Ok(target_tf) = targets.get(target)
        {
            tf.translation = target_tf.translation + sticky.offset.extend(0.0);
        }
    }
}

pub fn tick_beams(
    time: Res<Time<Fixed>>,
    mut commands: Commands,
    mut secrets: ResMut<SecretTriggers>,
    mut last_damage: ResMut<LastDamageTaken>,
    mut beams: Query<(Entity, &Transform, &mut Beam)>,
    mut targets: Query<
        (
            Entity,
            &Transform,
            &Team,
            &mut Health,
            Option<&mut Velocity>,
        ),
        (Without<Beam>, Without<Projectile>),
    >,
) {
    for (beam_e, beam_tf, mut beam) in beams.iter_mut() {
        beam.timer.tick(time.delta());
        let expired = beam.timer.just_finished();
        beam.tick.tick(time.delta());

        if !expired && !beam.tick.just_finished() {
            continue;
        }

        let center = beam_tf.translation.truncate();
        let half = beam.dir.normalize_or_zero() * (beam.length * 0.5);
        let a = center - half;
        let b = center + half;

        for (target_e, target_tf, target_team, mut health, mut vel) in &mut targets {
            if *target_team == beam.team {
                continue;
            }

            let p = target_tf.translation.truncate();
            if distance_to_segment(p, a, b) > beam.width * 0.5 {
                continue;
            }

            if *target_team == Team::Player && !health.invuln.is_finished() {
                continue;
            }

            health.hp -= beam.damage;

            if let Some(ref mut vel) = vel {
                GameFeel::apply_knockback(&mut vel.0, beam.dir, beam.knockback);
            }

            if *target_team == Team::Player {
                health.invuln = Timer::from_seconds(5.0 / 30.0, TimerMode::Once);
                secrets.mark_damage_taken();
                last_damage.note(Some(HitId::Trap), None);
            }

            HitFlash::apply(&mut commands, target_e, Color::WHITE, 0.08);
        }

        if expired {
            commands.entity(beam_e).despawn();
        }
    }
}

pub fn tick_sentry_turrets(
    time: Res<Time<Fixed>>,
    mut commands: Commands,
    enemies: Query<&Transform, With<Enemy>>,
    mut sentries: Query<(Entity, &Transform, &mut SentryTurret)>,
) {
    for (entity, tf, mut sentry) in &mut sentries {
        sentry.life.tick(time.delta());
        if sentry.life.just_finished() {
            commands.entity(entity).despawn();
            continue;
        }

        sentry.fire.tick(time.delta());
        if !sentry.fire.just_finished() {
            continue;
        }

        let pos = tf.translation.truncate();
        let mut best = None::<(f32, Vec2)>;
        for etf in &enemies {
            let target = etf.translation.truncate();
            let d2 = pos.distance_squared(target);
            if d2 > sentry.range * sentry.range {
                continue;
            }
            if best.map(|(bd, _)| d2 < bd).unwrap_or(true) {
                best = Some((d2, target));
            }
        }

        let Some((_, target)) = best else {
            continue;
        };

        let dir = (target - pos).normalize_or_zero();
        let angle = dir.y.atan2(dir.x);

        commands.spawn((
            GameCleanup,
            LevelCleanup,
            Team::Player,
            Projectile {
                damage: sentry.projectile_damage,
                life: Timer::from_seconds(0.9, TimerMode::Once),
                radius: 4.0,
                knockback: 24.0,
                explosive: false,
                source: None,
            },
            Velocity(dir * sentry.projectile_speed),
            Sprite {
                color: Color::srgb(0.95, 0.9, 0.7),
                custom_size: Some(Vec2::new(8.0, 3.0)),
                ..default()
            },
            Transform::from_translation(pos.extend(14.0))
                .with_rotation(Quat::from_rotation_z(angle)),
        ));
    }
}

fn distance_to_segment(p: Vec2, a: Vec2, b: Vec2) -> f32 {
    let ab = b - a;
    let denom = ab.length_squared();
    if denom <= 1e-6 {
        return p.distance(a);
    }
    let t = ((p - a).dot(ab) / denom).clamp(0.0, 1.0);
    let closest = a + ab * t;
    p.distance(closest)
}

pub fn move_projectiles(
    time: Res<Time<Fixed>>,
    mut commands: Commands,
    catalog: Res<AssetCatalog>,
    asset_server: Res<AssetServer>,
    mut q: Query<
        (
            Entity,
            &Team,
            &mut Projectile,
            &mut Velocity,
            &mut Transform,
            Option<&mut BouncesLeft>,
            Option<&mut Sticky>,
            Option<&SpawnHazardOnDeath>,
            Option<&SplitOnDeath>,
            Option<&CustomExplosion>,
            Option<&DeploysSentry>,
            Option<&SpawnsWeaponPickup>,
            Option<&PlasmaBurst>,
        ),
        Without<Prop>,
    >,
    mut props: Query<(Entity, &mut Prop, &Transform, Option<&PropDeathEffect>), With<Prop>>,
    entrances: Query<&SecretEntrance>,
    snowmen: Query<&SnowmanAmbush>,
    gold_barrels: Query<&GoldBarrelDrop>,
    mut secrets: ResMut<SecretTriggers>,
) {
    let dt = time.delta_secs();

    for (
        e,
        team,
        mut p,
        mut vel,
        mut tf,
        bounces,
        mut sticky,
        hazard,
        split,
        custom_explosion,
        deploys_sentry,
        spawn_pickup_spec,
        plasma_burst,
    ) in &mut q
    {
        p.life.tick(time.delta());

        // Armed sticky grenades hold position until their fuse (Projectile
        // life) expires, then run the normal terminal path.
        if sticky.as_ref().is_some_and(|s| s.armed) {
            if p.life.just_finished() {
                on_projectile_removed(
                    &mut commands,
                    &catalog,
                    &asset_server,
                    tf.translation.truncate(),
                    *team,
                    p.source,
                    hazard.copied(),
                    split.copied(),
                    vel.0,
                    p.explosive,
                    p.damage,
                    custom_explosion.copied(),
                    deploys_sentry.copied(),
                    spawn_pickup_spec.copied(),
                    plasma_burst.copied(),
                );
                commands.entity(e).despawn();
            }
            continue;
        }

        tf.translation += (vel.0 * dt).extend(0.0);
        let pos = tf.translation.truncate();
        let out = pos.x.abs() > ARENA_W / 2.0 + 80.0 || pos.y.abs() > ARENA_H / 2.0 + 80.0;
        let wall_hit = !out
            && (pos.x.abs() > ARENA_W / 2.0 - p.radius || pos.y.abs() > ARENA_H / 2.0 - p.radius);
        let prop_hit = circle_hits_prop(pos, p.radius, &props);

        if p.life.just_finished() || out {
            on_projectile_removed(
                &mut commands,
                &catalog,
                &asset_server,
                pos,
                *team,
                p.source,
                hazard.copied(),
                split.copied(),
                vel.0,
                p.explosive,
                p.damage,
                custom_explosion.copied(),
                deploys_sentry.copied(),
                spawn_pickup_spec.copied(),
                plasma_burst.copied(),
            );
            commands.entity(e).despawn();
            continue;
        }

        if wall_hit {
            // Sticky grenades attach to walls instead of dying.
            if let Some(mut sticky) = sticky {
                sticky.armed = true;
                sticky.stuck_to = None;
                sticky.offset = Vec2::ZERO;
                vel.0 = Vec2::ZERO;
                continue;
            }

            if let Some(mut bounce) = bounces
                && bounce.0 > 0
            {
                bounce.0 -= 1;
                let normal = if pos.x.abs() > ARENA_W / 2.0 - p.radius {
                    Vec2::new(pos.x.signum(), 0.0)
                } else {
                    Vec2::new(0.0, pos.y.signum())
                };
                vel.0 = crate::game::projectile_math::bounce_velocity(vel.0, normal);
                tf.rotation = Quat::from_rotation_z(vel.0.y.atan2(vel.0.x));
                // Nudge off the wall so we don't re-collide next frame.
                tf.translation += (normal * -p.radius * 0.5).extend(0.0);
                continue;
            }

            if !p.explosive {
                VfxSpawner::spawn_burst(
                    &mut commands,
                    pos,
                    3,
                    Color::srgb(1.0, 0.9, 0.5),
                    (30.0, 90.0),
                );
            }
            on_projectile_removed(
                &mut commands,
                &catalog,
                &asset_server,
                pos,
                *team,
                p.source,
                hazard.copied(),
                split.copied(),
                vel.0,
                p.explosive,
                p.damage,
                custom_explosion.copied(),
                deploys_sentry.copied(),
                spawn_pickup_spec.copied(),
                plasma_burst.copied(),
            );
            commands.entity(e).despawn();
            continue;
        }

        if prop_hit {
            // Sticky grenades stick to the first overlapping prop.
            if sticky.is_some() {
                for (prop_e, prop, prop_tf, _death) in &mut props {
                    let center = prop_tf.translation.truncate();
                    let half = prop.size / 2.0;
                    let closest = Vec2::new(
                        pos.x.clamp(center.x - half.x, center.x + half.x),
                        pos.y.clamp(center.y - half.y, center.y + half.y),
                    );
                    if pos.distance(closest) > p.radius {
                        continue;
                    }
                    if let Some(ref mut sticky) = sticky {
                        sticky.armed = true;
                        sticky.stuck_to = Some(prop_e);
                        sticky.offset = pos - center;
                    }
                    vel.0 = Vec2::ZERO;
                    break;
                }
                continue;
            }

            let mut dead_prop = None;
            for (prop_e, mut prop, prop_tf, death_effect) in &mut props {
                let center = prop_tf.translation.truncate();
                let half = prop.size / 2.0;
                let closest = Vec2::new(
                    pos.x.clamp(center.x - half.x, center.x + half.x),
                    pos.y.clamp(center.y - half.y, center.y + half.y),
                );
                if pos.distance(closest) > p.radius {
                    continue;
                }
                if prop.destructible {
                    prop.hp -= 1;
                    if prop.hp <= 0 {
                        dead_prop = Some((center, prop.explosive, death_effect.copied(), prop_e));
                        commands.entity(prop_e).despawn();
                    }
                }
                break;
            }

            if let Some((center, legacy_explosive, death_effect, prop_e)) = dead_prop {
                // Shared terminal path: bespoke payloads for cars/toxic
                // barrels/mines, legacy barrel boom, or plain debris burst.
                spawn_prop_death_effect(
                    &mut commands,
                    center,
                    death_effect,
                    legacy_explosive,
                    p.source,
                );

                // Destroying a secret entrance queues that secret.
                if let Ok(entrance) = entrances.get(prop_e) {
                    secrets.queue(entrance.target);
                }

                // Snowmen hide a snow bandit + rad (upstream SnowMan Destroy).
                if snowmen.get(prop_e).is_ok() {
                    let mut rng = rand::rng();
                    commands.spawn(PendingEnemySpawn {
                        kind: EnemyKind::Bandit,
                        pos: center
                            + Vec2::new(rng.random_range(-6.0..6.0), rng.random_range(-6.0..6.0)),
                        difficulty: 1.0,
                    });
                    spawn_rad(&mut commands, &catalog, &asset_server, center, 1);
                }

                // Gold barrels drop a gold weapon.
                if gold_barrels.get(prop_e).is_ok() {
                    let weapon = random_gold_weapon(&mut rand::rng());
                    spawn_pickup(
                        &mut commands,
                        &catalog,
                        &asset_server,
                        PickupKind::Weapon(weapon),
                        center + Vec2::new(0.0, -14.0),
                    );
                }
            }

            on_projectile_removed(
                &mut commands,
                &catalog,
                &asset_server,
                pos,
                *team,
                p.source,
                hazard.copied(),
                split.copied(),
                vel.0,
                p.explosive,
                p.damage,
                custom_explosion.copied(),
                deploys_sentry.copied(),
                spawn_pickup_spec.copied(),
                plasma_burst.copied(),
            );
            commands.entity(e).despawn();
        }
    }
}

fn spawn_explosion_with_source_radius(
    commands: &mut Commands,
    pos: Vec2,
    damage: i32,
    source: Option<DamageSource>,
    radius: f32,
    team: Team,
    hits_player: bool,
) {
    commands.spawn((
        GameCleanup,
        LevelCleanup,
        Explosion {
            timer: Timer::from_seconds(0.05, TimerMode::Once),
            radius,
            damage,
            team,
            hits_player,
            source,
        },
        Transform::from_translation(pos.extend(20.0)),
    ));
}

fn spawn_hazard_cloud(commands: &mut Commands, pos: Vec2, team: Team, spec: HazardDef) {
    commands.spawn((
        GameCleanup,
        LevelCleanup,
        team,
        HazardCloud {
            kind: spec.kind,
            radius: spec.radius,
            damage: spec.damage,
            timer: Timer::from_seconds(spec.duration, TimerMode::Once),
            tick: Timer::from_seconds(spec.tick, TimerMode::Repeating),
        },
        Sprite {
            color: spec.color,
            custom_size: Some(Vec2::splat(spec.radius * 2.0)),
            ..default()
        },
        Transform::from_translation(pos.extend(8.0)),
    ));
}

fn spawn_split_projectiles(
    commands: &mut Commands,
    pos: Vec2,
    team: Team,
    split: SplitDef,
    source: Option<DamageSource>,
    base_dir: Vec2,
) {
    let mut rng = rand::rng();
    let samples: Vec<f32> = (0..split.pellets)
        .map(|_| rng.random_range(-1.0f32..1.0))
        .collect();

    for dir in crate::game::projectile_math::split_directions(
        base_dir,
        split.pellets,
        split.spread,
        &samples,
    ) {
        let angle = dir.y.atan2(dir.x);

        commands.spawn((
            GameCleanup,
            LevelCleanup,
            team,
            Projectile {
                damage: split.damage,
                life: Timer::from_seconds(split.lifetime, TimerMode::Once),
                radius: split.radius,
                knockback: split.knockback,
                explosive: false,
                source,
            },
            Velocity(dir * split.speed),
            Sprite {
                color: split.color,
                custom_size: Some(split.size),
                ..default()
            },
            Transform::from_translation(pos.extend(16.0))
                .with_rotation(Quat::from_rotation_z(angle)),
        ));
    }
}

/// Plasma secondary burst: an even ring of children around the impact point.
fn spawn_plasma_children(
    commands: &mut Commands,
    pos: Vec2,
    team: Team,
    plasma: PlasmaBurst,
    source: Option<DamageSource>,
    base_dir: Vec2,
) {
    let base_angle = if base_dir.length_squared() > 0.0 {
        base_dir.y.atan2(base_dir.x)
    } else {
        0.0
    };

    for i in 0..plasma.pellets.max(1) {
        let t = i as f32 / plasma.pellets.max(1) as f32;
        let angle = base_angle + t * std::f32::consts::TAU;
        let dir = Vec2::new(angle.cos(), angle.sin());

        commands.spawn((
            GameCleanup,
            LevelCleanup,
            team,
            Projectile {
                damage: plasma.damage,
                life: Timer::from_seconds(plasma.lifetime, TimerMode::Once),
                radius: plasma.radius,
                knockback: plasma.knockback,
                explosive: false,
                source,
            },
            Velocity(dir * plasma.speed),
            Sprite {
                color: plasma.color,
                custom_size: Some(plasma.size),
                ..default()
            },
            Transform::from_translation(pos.extend(16.0))
                .with_rotation(Quat::from_rotation_z(angle)),
        ));
    }
}

fn spawn_sentry_turret(commands: &mut Commands, pos: Vec2, spec: DeploysSentry) {
    commands.spawn((
        GameCleanup,
        LevelCleanup,
        Team::Player,
        SentryTurret {
            life: Timer::from_seconds(spec.life, TimerMode::Once),
            fire: Timer::from_seconds(spec.fire_interval, TimerMode::Repeating),
            range: spec.range,
            projectile_speed: spec.projectile_speed,
            projectile_damage: spec.projectile_damage,
        },
        Team::Player,
        Sprite {
            color: Color::srgb(0.68, 0.74, 0.8),
            custom_size: Some(Vec2::new(18.0, 14.0)),
            ..default()
        },
        Transform::from_translation(pos.extend(12.0)),
    ));
}

fn spawn_weapon_pickup_from_projectile(
    commands: &mut Commands,
    catalog: &AssetCatalog,
    asset_server: &AssetServer,
    pos: Vec2,
    spec: SpawnsWeaponPickup,
) {
    // Do not spawn pickups outside the playfield.
    if pos.x.abs() > ARENA_W / 2.0 + 32.0 || pos.y.abs() > ARENA_H / 2.0 + 32.0 {
        return;
    }

    let weapon = spec
        .weapon
        .unwrap_or_else(|| random_weapon(&mut rand::rng()));
    spawn_pickup(
        commands,
        catalog,
        asset_server,
        PickupKind::Weapon(weapon),
        pos,
    );
}

/// Shared terminal path for timeout, wall, prop, and entity hits.
#[allow(clippy::type_complexity)]
fn on_projectile_removed(
    commands: &mut Commands,
    catalog: &AssetCatalog,
    asset_server: &AssetServer,
    pos: Vec2,
    team: Team,
    source: Option<DamageSource>,
    hazard: Option<SpawnHazardOnDeath>,
    split: Option<SplitOnDeath>,
    base_dir: Vec2,
    explosive: bool,
    damage: i32,
    custom_explosion: Option<CustomExplosion>,
    deploys_sentry: Option<DeploysSentry>,
    spawn_pickup_spec: Option<SpawnsWeaponPickup>,
    plasma_burst: Option<PlasmaBurst>,
) {
    if let Some(spec) = deploys_sentry {
        spawn_sentry_turret(commands, pos, spec);
    }

    if explosive {
        let radius = custom_explosion.map(|c| c.radius).unwrap_or(130.0);
        spawn_explosion_with_source_radius(commands, pos, damage, source, radius, team, true);
    }

    if let Some(SpawnHazardOnDeath(spec)) = hazard {
        spawn_hazard_cloud(commands, pos, team, spec);
    }

    if let Some(SplitOnDeath(spec)) = split {
        spawn_split_projectiles(commands, pos, team, spec, source, base_dir);
    }

    if let Some(spec) = spawn_pickup_spec {
        spawn_weapon_pickup_from_projectile(commands, catalog, asset_server, pos, spec);
    }

    if let Some(plasma) = plasma_burst {
        spawn_plasma_children(commands, pos, team, plasma, source, base_dir);
    }
}

/// Weapon / team-tagged hazard clouds only.
/// Ability clouds are handled by `player::tick_hazard_clouds`.
pub fn tick_hazard_clouds(
    time: Res<Time<Fixed>>,
    mut commands: Commands,
    mut secrets: ResMut<SecretTriggers>,
    mut last_damage: ResMut<LastDamageTaken>,
    mut clouds: Query<(Entity, &Team, &Transform, &mut HazardCloud), Without<AbilityHazard>>,
    mut targets: Query<
        (Entity, &Transform, &Team, &mut Health),
        (Without<HazardCloud>, Without<Projectile>),
    >,
) {
    for (cloud_e, cloud_team, cloud_tf, mut cloud) in &mut clouds {
        cloud.timer.tick(time.delta());
        cloud.tick.tick(time.delta());

        if cloud.timer.just_finished() {
            commands.entity(cloud_e).despawn();
            continue;
        }
        if !cloud.tick.just_finished() {
            continue;
        }

        let pos = cloud_tf.translation.truncate();
        for (_, target_tf, target_team, mut health) in &mut targets {
            if *target_team == *cloud_team {
                continue;
            }
            if target_tf.translation.truncate().distance(pos) > cloud.radius {
                continue;
            }
            if *target_team == Team::Player && !health.invuln.is_finished() {
                continue;
            }

            health.hp -= cloud.damage;
            if *target_team == Team::Player {
                health.invuln = Timer::from_seconds(5.0 / 30.0, TimerMode::Once);
                // Hazard damage disqualifies the Oasis secret for this floor.
                secrets.mark_damage_taken();
                let hid = match cloud.kind {
                    crate::game::content::HazardKind::Toxic => HitId::Toxic,
                    crate::game::content::HazardKind::Fire => HitId::Fire,
                };
                last_damage.note(Some(hid), None);
            }
        }
    }
}

pub fn apply_explosions(
    time: Res<Time<Fixed>>,
    mut commands: Commands,
    mut trauma: ResMut<Trauma>,
    mut hitstop: ResMut<HitStop>,
    mut chroma: ResMut<ChromaticAberration>,
    audio: Res<GameAudio>,
    ctx: RadSpawnCtx,
    death_ctx: PropDeathQueries,
    mut secrets: ResMut<SecretTriggers>,
    mut q: Query<
        (Entity, &mut Explosion, &Transform),
        (Without<Enemy>, Without<Player>, Without<Prop>),
    >,
    mut enemies: Query<(Entity, &Transform, &mut Health), (With<Enemy>, Without<Player>)>,
    mut player_q: Query<(Entity, &Transform, &mut Health, &Player), (With<Player>, Without<Enemy>)>,
    mut props: Query<
        (Entity, &mut Prop, &Transform, Option<&PropDeathEffect>),
        (With<Prop>, Without<Player>),
    >,
    walls: Query<(Entity, &WallCell, &Transform), With<WallTile>>,
    mut last_damage: ResMut<LastDamageTaken>,
) {
    for (e, mut boom, tf) in &mut q {
        boom.timer.tick(time.delta());
        if !boom.timer.just_finished() {
            continue;
        }

        let pos = tf.translation.truncate();
        ScreenEffects::add_trauma(&mut trauma, 0.45);
        ScreenEffects::chromatic_pulse(&mut chroma, 0.3);
        hitstop.trigger(0.14, 0.1);
        VfxSpawner::spawn_burst(
            &mut commands,
            pos,
            32,
            Color::srgb(1.0, 0.4, 0.1),
            (130.0, 400.0),
        );
        VfxSpawner::spawn_burst(
            &mut commands,
            pos,
            16,
            Color::srgb(1.0, 0.9, 0.5),
            (60.0, 220.0),
        );
        audio.play_boom(&mut commands);

        if boom.team == Team::Player {
            for (ee, etf, mut health) in &mut enemies {
                if etf.translation.truncate().distance(pos) < boom.radius {
                    health.hp -= boom.damage;
                    HitFlash::apply(&mut commands, ee, Color::WHITE, 0.12);
                    VfxSpawner::spawn_damage_number(
                        &mut commands,
                        boom.damage,
                        etf.translation.truncate(),
                        Color::srgb(1.0, 0.6, 0.2),
                    );
                }
            }
            // Explosions destroy props and can chain their payloads.
            let mut destroyed_props = Vec::new();
            for (prop_e, mut prop, prop_tf, death_effect) in &mut props {
                if !prop.destructible {
                    continue;
                }

                let center = prop_tf.translation.truncate();
                let half = prop.size / 2.0;
                let closest = Vec2::new(
                    pos.x.clamp(center.x - half.x, center.x + half.x),
                    pos.y.clamp(center.y - half.y, center.y + half.y),
                );
                if pos.distance(closest) < boom.radius {
                    prop.hp -= boom.damage.max(1);
                    if prop.hp <= 0 {
                        destroyed_props.push((
                            prop_e,
                            center,
                            prop.explosive,
                            death_effect.copied(),
                        ));
                    }
                }
            }

            for (prop_e, center, legacy_explosive, death_effect) in destroyed_props {
                spawn_prop_death_effect(
                    &mut commands,
                    center,
                    death_effect,
                    legacy_explosive,
                    boom.source,
                );

                // Explosions can also open secret entrances.
                if let Ok(entrance) = death_ctx.entrances.get(prop_e) {
                    secrets.queue(entrance.target);
                }

                // Snowmen hide a snow bandit + rad (upstream SnowMan Destroy).
                if death_ctx.snowmen.get(prop_e).is_ok() {
                    let mut rng = rand::rng();
                    commands.spawn(PendingEnemySpawn {
                        kind: EnemyKind::Bandit,
                        pos: center
                            + Vec2::new(rng.random_range(-6.0..6.0), rng.random_range(-6.0..6.0)),
                        difficulty: 1.0,
                    });
                    spawn_rad(&mut commands, &ctx.catalog, &ctx.asset_server, center, 1);
                }

                // Gold barrels drop a gold weapon.
                if death_ctx.gold_barrels.get(prop_e).is_ok() {
                    let weapon = random_gold_weapon(&mut rand::rng());
                    spawn_pickup(
                        &mut commands,
                        &ctx.catalog,
                        &ctx.asset_server,
                        PickupKind::Weapon(weapon),
                        center + Vec2::new(0.0, -14.0),
                    );
                }

                commands.entity(prop_e).despawn();
            }

            // Explosions chew nearby walls.
            for (_, cell, wtf) in &walls {
                if wtf.translation.truncate().distance(pos) < boom.radius * 0.85 {
                    commands.spawn((
                        GameCleanup,
                        LevelCleanup,
                        PendingWallBreak {
                            cell: (cell.0, cell.1),
                            pos: wtf.translation.truncate(),
                            spawn_floor: true,
                        },
                    ));
                }
            }
        }

        // Friendly fire: explosions can hurt the player too (Boiling Veins
        // clamps the damage so HP can't drop below the threshold).
        if boom.hits_player
            && let Ok((player_e, ptf, mut health, player)) = player_q.single_mut()
            && ptf.translation.truncate().distance(pos) < boom.radius
            && health.invuln.is_finished()
        {
            let mut dmg = boom.damage;
            if player.boiling_veins {
                let floor = player.veins_threshold;
                dmg = if health.hp - dmg < floor {
                    (health.hp - floor).max(0)
                } else {
                    dmg
                };
            }
            health.hp -= dmg;
            health.invuln = Timer::from_seconds(5.0 / 30.0, TimerMode::Once);
            secrets.mark_damage_taken();
            last_damage.note_from_source(boom.source.as_ref());
            HitFlash::apply(&mut commands, player_e, Color::srgb(1.0, 0.3, 0.2), 0.15);
            audio.play_hurt(&mut commands);
        }

        commands.entity(e).despawn();
    }
}

#[allow(clippy::type_complexity)]
pub fn projectile_hits(
    mut commands: Commands,
    catalog: Res<AssetCatalog>,
    asset_server: Res<AssetServer>,
    mut trauma: ResMut<Trauma>,
    mut hitstop: ResMut<HitStop>,
    audio: Res<GameAudio>,
    mut secrets: ResMut<SecretTriggers>,
    mut last_damage: ResMut<LastDamageTaken>,
    player_state: Query<&Player, With<Player>>,
    mut projectiles: Query<
        (
            Entity,
            &mut Transform,
            &Team,
            &Projectile,
            &mut Velocity,
            Option<&mut PiercesLeft>,
            Option<&mut ProjectileHitSet>,
            Option<&mut Sticky>,
            Option<&mut ChainLightning>,
            Option<&SpawnHazardOnDeath>,
            Option<&SplitOnDeath>,
            Option<&CustomExplosion>,
            Option<&DeploysSentry>,
            Option<&SpawnsWeaponPickup>,
            Option<&PlasmaBurst>,
        ),
        Without<Hitbox>,
    >,
    mut targets: Query<
        (
            Entity,
            &Transform,
            &Team,
            &Hitbox,
            &mut Health,
            Option<&mut Velocity>,
            Option<&Shield>,
        ),
        Without<Projectile>,
    >,
) {
    let player = player_state.single().ok();

    for (
        proj_e,
        proj_tf,
        proj_team,
        proj,
        mut proj_vel,
        pierce,
        mut hit_set,
        mut sticky,
        chain,
        hazard,
        split,
        custom_explosion,
        deploys_sentry,
        spawn_pickup_spec,
        plasma_burst,
    ) in projectiles.iter_mut()
    {
        // Armed sticky grenades do not deal contact damage; they wait for the
        // fuse handled in move_projectiles.
        if sticky.as_ref().is_some_and(|s| s.armed) {
            continue;
        }

        let proj_pos = proj_tf.translation.truncate();
        let mut hit = false;
        let mut damaged = false;
        let mut hit_player = false;
        let mut hit_pos = proj_pos;
        let mut hit_target = None::<Entity>;

        for (target_e, target_tf, target_team, hitbox, mut health, vel_opt, shield) in
            targets.iter_mut()
        {
            if *target_team == *proj_team {
                continue;
            }

            if let Some(set) = hit_set.as_ref()
                && set.0.contains(&target_e)
            {
                continue;
            }

            let target_pos = target_tf.translation.truncate();
            if proj_pos.distance(target_pos) > proj.radius + hitbox.radius {
                continue;
            }

            // Sticky grenades attach instead of dealing immediate damage.
            if let Some(ref mut sticky) = sticky
                && !sticky.armed
            {
                sticky.armed = true;
                sticky.stuck_to = Some(target_e);
                sticky.offset = proj_pos - target_pos;
                proj_vel.0 = Vec2::ZERO;
                break;
            }

            hit = true;
            hit_pos = target_pos;
            hit_target = Some(target_e);

            if *target_team == Team::Player
                && let Some(shield) = shield
                && !shield.timer.is_finished()
            {
                VfxSpawner::spawn_burst(
                    &mut commands,
                    target_pos,
                    8,
                    Color::srgb(0.3, 0.65, 1.0),
                    (60.0, 160.0),
                );
                audio.play_hit(&mut commands);
                break;
            }

            if *target_team == Team::Player && !health.invuln.is_finished() {
                break;
            }

            health.hp -= proj.damage;
            damaged = true;

            if *target_team == Team::Player {
                health.invuln = Timer::from_seconds(5.0 / 30.0, TimerMode::Once);
                hit_player = true;
                secrets.mark_damage_taken();
                last_damage.note_from_source(proj.source.as_ref());
                audio.play_hurt(&mut commands);
            } else {
                audio.play_hit(&mut commands);
            }

            if let Some(mut vel) = vel_opt {
                GameFeel::apply_knockback(
                    &mut vel.0,
                    proj_vel.0.normalize_or_zero(),
                    proj.knockback,
                );
            }

            HitFlash::apply(&mut commands, target_e, Color::WHITE, 0.1);
            ScreenEffects::add_trauma(&mut trauma, 0.08);
            VfxSpawner::spawn_damage_number(
                &mut commands,
                proj.damage,
                target_pos,
                Color::srgb(1.0, 0.92, 0.35),
            );

            if proj.explosive {
                hitstop.trigger(0.12, 0.09);
            }
            break;
        }

        if !hit {
            continue;
        }

        if hit_player
            && let Some(p) = &player
            && p.sharp_teeth
        {
            retaliate_sharp_teeth(&mut commands, proj.damage, hit_pos, &mut targets);
        }

        if damaged {
            if let Some(target_e) = hit_target {
                if let Some(ref mut set) = hit_set {
                    crate::game::projectile_math::record_hit(&mut set.0, target_e);
                } else {
                    commands
                        .entity(proj_e)
                        .try_insert(ProjectileHitSet(vec![target_e]));
                }
            }
        }

        let terminal = |commands: &mut Commands| {
            on_projectile_removed(
                commands,
                &catalog,
                &asset_server,
                hit_pos,
                *proj_team,
                proj.source,
                hazard.copied(),
                split.copied(),
                proj_vel.0,
                proj.explosive,
                proj.damage,
                custom_explosion.copied(),
                deploys_sentry.copied(),
                spawn_pickup_spec.copied(),
                plasma_burst.copied(),
            );
            commands.entity(proj_e).despawn();
        };

        // Lightning weapons jump between distinct targets instead of piercing.
        if damaged && let Some(ref chain) = chain {
            chain_to_nearby_targets(
                &mut commands,
                &mut targets,
                *proj_team,
                proj,
                hit_target,
                hit_pos,
                chain.range,
                chain.jumps_left,
                chain.falloff,
            );
            terminal(&mut commands);
            continue;
        }

        let pierce_left_before = pierce.as_ref().map(|p| p.0);
        let (despawn, pierce_left) =
            crate::game::projectile_math::should_despawn_after_hit(damaged, pierce_left_before);
        if let (Some(mut p), Some(left)) = (pierce, pierce_left) {
            p.0 = left;
        }

        if despawn {
            terminal(&mut commands);
        }
    }
}

/// Chain lightning: hop from the just-hit target to the nearest unvisited
/// enemy within range, applying falloff damage and zap VFX per jump.
#[allow(clippy::type_complexity)]
fn chain_to_nearby_targets(
    commands: &mut Commands,
    targets: &mut Query<
        (
            Entity,
            &Transform,
            &Team,
            &Hitbox,
            &mut Health,
            Option<&mut Velocity>,
            Option<&Shield>,
        ),
        Without<Projectile>,
    >,
    _proj_team: Team,
    proj: &Projectile,
    first_target: Option<Entity>,
    first_pos: Vec2,
    range: f32,
    jumps: u8,
    falloff: f32,
) {
    let Some(first_e) = first_target else {
        return;
    };

    let mut visited: Vec<Entity> = vec![first_e];
    let mut current_pos = first_pos;
    let mut damage = proj.damage.max(1);

    for _ in 0..jumps {
        // Find nearest unvisited enemy to current point.
        let mut best: Option<(Entity, Vec2, f32)> = None;
        let mut snapshot: Vec<(Entity, Vec2)> = Vec::new();
        for (target_e, target_tf, target_team, ..) in targets.iter() {
            if *target_team != Team::Enemy || visited.contains(&target_e) {
                continue;
            }
            let pos = target_tf.translation.truncate();
            let d2 = current_pos.distance_squared(pos);
            if d2 > range * range {
                continue;
            }
            snapshot.push((target_e, pos));
            if best.map(|(_, _, bd)| d2 < bd).unwrap_or(true) {
                best = Some((target_e, pos, d2));
            }
        }

        let Some((next_e, next_pos, _)) = best else {
            break;
        };

        damage = ((damage as f32) * falloff).round().max(1.0) as i32;

        for (target_e, _tf, _team, _hb, mut health, vel_opt, _) in targets.iter_mut() {
            if target_e != next_e {
                continue;
            }
            health.hp -= damage;
            if let Some(mut vel) = vel_opt {
                GameFeel::apply_knockback(
                    &mut vel.0,
                    (next_pos - current_pos).normalize_or_zero(),
                    proj.knockback * 0.5,
                );
            }
            HitFlash::apply(commands, target_e, Color::srgb(0.7, 0.95, 1.0), 0.08);
            VfxSpawner::spawn_damage_number(
                commands,
                damage,
                next_pos,
                Color::srgb(0.7, 0.95, 1.0),
            );
            VfxSpawner::spawn_burst(
                commands,
                (current_pos + next_pos) * 0.5,
                6,
                Color::srgb(0.75, 0.95, 1.0),
                (30.0, 90.0),
            );
            break;
        }

        visited.push(next_e);
        current_pos = next_pos;
        let _ = snapshot;
    }
}

/// Sharp Teeth: incoming damage is dealt back (x2) to all enemies on screen.
fn retaliate_sharp_teeth(
    commands: &mut Commands,
    damage: i32,
    center: Vec2,
    targets: &mut Query<
        (
            Entity,
            &Transform,
            &Team,
            &Hitbox,
            &mut Health,
            Option<&mut Velocity>,
            Option<&Shield>,
        ),
        Without<Projectile>,
    >,
) {
    for (ee, etf, team, _, mut health, _, _) in targets.iter_mut() {
        if *team != Team::Enemy {
            continue;
        }
        if etf.translation.truncate().distance(center) > 900.0 {
            continue;
        }
        health.hp -= damage * 2;
        HitFlash::apply(commands, ee, Color::srgb(1.0, 0.4, 0.4), 0.12);
    }
}

pub fn contact_damage(
    mut commands: Commands,
    mut trauma: ResMut<Trauma>,
    mut flash: ResMut<FlashWhite>,
    audio: Res<GameAudio>,
    gamepads: Query<(Entity, &Gamepad)>,
    mut rumble: MessageWriter<GamepadRumbleRequest>,
    mut secrets: ResMut<SecretTriggers>,
    mut last_damage: ResMut<LastDamageTaken>,
    mut player_q: Query<
        (Entity, &Transform, &mut Health, &mut Velocity, &Player),
        (With<Player>, Without<Enemy>),
    >,
    mut enemies: Query<
        (&Transform, &Enemy, &mut EnemyBrain, &mut Health, &Hitbox),
        (With<Enemy>, Without<Player>),
    >,
) {
    let Ok((player_e, player_tf, mut health, mut player_vel, player)) = player_q.single_mut()
    else {
        return;
    };

    // Contact damage must respect the same invulnerability window as
    // projectile and explosion damage.
    if !health.invuln.is_finished() {
        return;
    }

    let player_pos = player_tf.translation.truncate();
    let mut took_damage = 0;

    for (enemy_tf, enemy, mut brain, ehealth, enemy_hitbox) in &mut enemies {
        if !brain.melee.is_finished() {
            continue;
        }
        if player_pos.distance(enemy_tf.translation.truncate())
            >= PLAYER_RADIUS + enemy_hitbox.radius
        {
            continue;
        }

        // Gamma Guts: weak enemies are vaporized on contact instead of meleeing.
        if player.gamma_guts && ehealth.hp <= 6 {
            continue;
        }

        let damage = if brain.dash > 0.0 {
            10
        } else {
            enemy.touch_damage
        };
        if damage <= 0 {
            continue;
        }

        health.hp -= damage;
        took_damage = damage;
        health.invuln = Timer::from_seconds(5.0 / 30.0, TimerMode::Once);
        brain.melee = Timer::from_seconds(0.5, TimerMode::Once);
        secrets.mark_damage_taken();
        last_damage.note(Some(HitId::Contact), Some(enemy.kind));

        let away = (player_pos - enemy_tf.translation.truncate()).normalize_or_zero();
        GameFeel::apply_knockback(&mut player_vel.0, away, 240.0);

        HitFlash::apply(&mut commands, player_e, Color::srgb(1.0, 0.15, 0.1), 0.18);
        ScreenEffects::add_trauma(&mut trauma, 0.35);
        GameFeel::rumble_controller(&mut rumble, &gamepads, 0.2, 0.8, 0.16);
        audio.play_hurt(&mut commands);

        VfxSpawner::spawn_burst(
            &mut commands,
            player_pos,
            12,
            Color::srgb(1.0, 0.1, 0.08),
            (80.0, 220.0),
        );

        // Crystal's passive: brief shield after taking a hit.
        if player.shield_on_hit {
            commands.entity(player_e).insert(Shield {
                timer: Timer::from_seconds(0.7, TimerMode::Once),
            });
        }
        break;
    }

    if took_damage > 0 && player.sharp_teeth {
        for (enemy_tf, _, _, mut ehealth, _) in &mut enemies {
            if enemy_tf.translation.truncate().distance(player_pos) <= 900.0 {
                ehealth.hp -= took_damage * 2;
            }
        }
    }
}

/// Gamma Guts: the player's aura deals 6 damage to any enemy touching it
/// (enemies with <= 6 HP are killed on contact instead of meleeing).
pub fn gamma_guts_aura(
    mut commands: Commands,
    player_q: Query<(&Transform, &Player), (With<Player>, Without<Enemy>)>,
    mut enemies: Query<(Entity, &Transform, &mut Health), (With<Enemy>, Without<Player>)>,
) {
    let Ok((ptf, player)) = player_q.single() else {
        return;
    };
    if !player.gamma_guts {
        return;
    }
    let ppos = ptf.translation.truncate();
    for (e, etf, mut health) in &mut enemies {
        if etf.translation.truncate().distance(ppos) >= 60.0 {
            continue;
        }
        if !health.invuln.is_finished() {
            continue;
        }
        health.hp -= 6;
        health.invuln = Timer::from_seconds(5.0 / 30.0, TimerMode::Once);
        HitFlash::apply(&mut commands, e, Color::srgb(0.4, 1.0, 0.4), 0.1);
    }
}

pub fn resolve_deaths(
    mut commands: Commands,
    catalog: Res<AssetCatalog>,
    asset_server: Res<AssetServer>,
    mut score: ResMut<Score>,
    mut save: ResMut<SaveData>,
    mut dirty: ResMut<SaveDirty>,
    mut run: ResMut<Run>,
    mut paused: ResMut<Paused>,
    effects: (
        ResMut<Trauma>,
        ResMut<ChromaticAberration>,
        ResMut<FlashWhite>,
        ResMut<HitStop>,
        ResMut<SlowMotion>,
        ResMut<LoopTransition>,
        ResMut<Toast>,
        ResMut<LastDamageTaken>,
        ResMut<ThroneRoomState>,
    ),
    audio: Res<GameAudio>,
    gamepads: Query<(Entity, &Gamepad)>,
    mut rumble: MessageWriter<GamepadRumbleRequest>,
    transition: ResMut<Transition<AppState>>,
    mut player_q: Query<
        (
            Entity,
            &Transform,
            &mut Health,
            &mut Inventory,
            &mut Player,
            &mut RaceState,
            Option<&mut Sprite>,
        ),
        (With<Player>, Without<Enemy>),
    >,
    mut fire_q: Query<&mut FireCooldown, (With<Player>, Without<Enemy>)>,
    q: Query<
        (Entity, &Transform, &Team, &Health, Option<&Enemy>),
        (Without<Prop>, Without<Player>),
    >,
) {
    let (
        mut trauma,
        mut chroma,
        mut flash,
        mut hitstop,
        mut slow_mo,
        mut loop_transition,
        mut toast,
        mut last_damage,
        mut throne_room,
    ) = effects;
    if run.game_over {
        return;
    }

    let Ok((
        player_e,
        player_tf,
        mut phealth,
        mut pinv,
        mut player,
        mut race_state,
        mut player_sprite,
    )) = player_q.single_mut()
    else {
        return;
    };

    let mut rng = rand::rng();

    for (e, tf, team, health, enemy) in &q {
        if *team != Team::Enemy || health.hp > 0 {
            continue;
        }

        let enemy = enemy.copied().unwrap_or(Enemy {
            kind: EnemyKind::Maggot,
            score: 1,
            touch_damage: 1,
            rad_drop: 1,
            drop_chance: 0,
            weapon_chance: 0,
        });
        let def = enemy_def(enemy.kind);
        let pos = tf.translation.truncate();

        commands.entity(e).despawn();
        if !def.boss && !matches!(enemy.kind, EnemyKind::IdpdVan | EnemyKind::FrogEgg) {
            commands.spawn((
                GameCleanup,
                LevelCleanup,
                Corpse {
                    kind: enemy.kind,
                    life: Timer::from_seconds(12.0, TimerMode::Once),
                    pos,
                },
                Sprite {
                    color: Color::srgba(0.35, 0.1, 0.1, 0.85),
                    custom_size: Some(Vec2::splat(def.size * 0.7)),
                    ..default()
                },
                Transform::from_translation(pos.extend(-5.0)),
            ));
        }

        // Loop-transition hooks must run before normal drop handling so the
        // interlude starts while loot still pops.
        let player_pos_now = player_tf.translation.truncate();
        match enemy.kind {
            EnemyKind::Throne => {
                if throne_room.loop_eligible {
                    crate::game::loop_transition::begin_throne_campfire(
                        &mut commands,
                        &mut loop_transition,
                        &mut toast,
                        &mut trauma,
                        player_pos_now,
                    );
                } else {
                    // Sit ending - run over, no loop (generators still up).
                    run.game_over = true;
                    toast.show("THE NUCLEAR THRONE");
                    ScreenEffects::flash_white(&mut flash, 0.2);
                    ScreenEffects::add_trauma(&mut trauma, 0.5);
                    GameFeel::slow_motion(&mut slow_mo, 0.25, 2.0);
                }
            }
            EnemyKind::ThroneII => {
                loop_transition.throne_ii_defeated();
                crate::game::loop_transition::mark_throne_ii_defeated(&mut toast, &mut trauma);
            }
            _ => {}
        }

        run.total_kills += 1;
        score.0 += enemy.score;

        if score.0 > save.high_score {
            save.high_score = score.0;
            dirty.0 = true;
        }

        ScreenEffects::add_trauma(&mut trauma, 0.25);
        ScreenEffects::chromatic_pulse(&mut chroma, 0.08);
        hitstop.trigger(0.28, 0.055);
        if def.boss {
            hitstop.trigger(0.28, 0.18);
            GameFeel::slow_motion(&mut slow_mo, 0.35, 0.55);
            ScreenEffects::flash_white(&mut flash, 0.12);
            toast.show(&format!(
                "{} DEFEATED",
                enemy_def(enemy.kind).name.to_ascii_uppercase()
            ));
        }

        // Kill-gated unlocks (Big Dog, Frog/Mom, …); some B-skins require
        // the killing race (scrOnBossKill).
        let newly = crate::game::generated::unlocks::check_kill_unlocks(
            &mut save,
            enemy.kind,
            race_state.race,
        );
        if !newly.is_empty() {
            dirty.0 = true;
            for r in newly {
                toast.show(&format!(
                    "{} UNLOCKED",
                    crate::game::content::character_def(r)
                        .name
                        .to_ascii_uppercase()
                ));
            }
        }

        let burst_count = if def.boss {
            40 + run.loop_count as usize * 6
        } else {
            14
        };
        let boom_radius = if enemy.kind == EnemyKind::Throne {
            130.0 + run.loop_count as f32 * 18.0
        } else {
            0.0
        };
        VfxSpawner::spawn_burst(
            &mut commands,
            pos,
            burst_count,
            Color::srgb(0.9, 0.18, 0.1),
            (80.0, 260.0),
        );
        if boom_radius > 0.0 {
            commands.spawn((
                GameCleanup,
                LevelCleanup,
                Explosion {
                    timer: Timer::from_seconds(0.05, TimerMode::Once),
                    radius: boom_radius,
                    damage: 6 + run.loop_count as i32 * 2,
                    team: Team::Enemy,
                    hits_player: true,
                    source: Some(DamageSource::enemy(e, enemy.kind)),
                },
                Transform::from_translation(pos.extend(20.0)),
            ));
        }

        audio.play_hit(&mut commands);

        // Kind-specific death effects (upstream Exploder / ExploFreak).
        match enemy.kind {
            // Exploder (Ballguy): bursts into a ring of bullets.
            EnemyKind::Ballguy => {
                for i in 0..8 {
                    let ang = (i as f32) * std::f32::consts::TAU / 8.0;
                    let d = Vec2::new(ang.cos(), ang.sin());
                    commands.spawn((
                        GameCleanup,
                        LevelCleanup,
                        Team::Enemy,
                        Projectile {
                            damage: 2,
                            life: Timer::from_seconds(0.9, TimerMode::Once),
                            radius: 3.5,
                            knockback: 120.0,
                            explosive: false,
                            source: Some(DamageSource::enemy(e, enemy.kind)),
                        },
                        Velocity(d * 190.0),
                        Sprite {
                            color: Color::srgb(1.0, 0.75, 0.25),
                            custom_size: Some(Vec2::splat(7.0)),
                            ..default()
                        },
                        Transform::from_translation(pos.extend(15.0)),
                    ));
                }
            }
            // Explo Freak: detonates on death.
            EnemyKind::ExploFreak => {
                commands.spawn((
                    GameCleanup,
                    LevelCleanup,
                    Explosion {
                        timer: Timer::from_seconds(0.05, TimerMode::Once),
                        radius: 46.0,
                        damage: 5,
                        team: Team::Enemy,
                        hits_player: true,
                        source: Some(DamageSource {
                            owner: e,
                            team: Team::Enemy,
                            hit_id: HitId::Explosion(WeaponId::NONE),
                            enemy_kind: Some(enemy.kind),
                        }),
                    },
                    Transform::from_translation(pos.extend(20.0)),
                ));
                VfxSpawner::spawn_burst(
                    &mut commands,
                    pos,
                    18,
                    Color::srgb(1.0, 0.5, 0.15),
                    (60.0, 220.0),
                );
                ScreenEffects::add_trauma(&mut trauma, 0.12);
            }
            _ => {}
        }

        // Kill effects: Bloodlust heals, Lucky Shot grants ammo, Trigger
        // Fingers shortens the next reload.
        if player.bloodlust && rng.random_range(0..15) == 0 {
            phealth.hp = (phealth.hp + 2).min(phealth.max);
        }
        if player.lucky_shot && rng.random_range(0..10) == 0 {
            give_ammo(&mut pinv, &player);
        }
        // Trigger Fingers: a kill cuts the in-progress reload by 40%.
        if player.mutations.contains(&MutationId::TriggerFingers)
            && let Ok(mut fc) = fire_q.single_mut()
        {
            if !fc.timer.is_finished() {
                fc.timer = Timer::from_seconds(fc.timer.remaining_secs() * 0.6, TimerMode::Once);
            }
            if fc.burst_left > 0 {
                fc.burst_timer =
                    Timer::from_seconds(fc.burst_timer.remaining_secs() * 0.6, TimerMode::Once);
            }
        }

        if def.boss {
            audio.play_boom(&mut commands);
            GameFeel::rumble_controller(&mut rumble, &gamepads, 0.8, 1.0, 0.4);
            GameFeel::slow_motion(&mut slow_mo, 0.35, 0.6);
            for _ in 0..enemy.rad_drop.min(24) {
                spawn_rad(
                    &mut commands,
                    &catalog,
                    &asset_server,
                    pos + random_offset(),
                    1,
                );
            }
            // Boss drops a chest with a weapon plus two drops.
            spawn_chest(
                &mut commands,
                &catalog,
                &asset_server,
                pos + random_offset() * 3.0,
            );
            for _ in 0..2 {
                maybe_spawn_drop(
                    &mut commands,
                    &catalog,
                    &asset_server,
                    pos,
                    enemy.drop_chance,
                    enemy.weapon_chance,
                    &player,
                    &pinv,
                    &phealth,
                );
            }
        } else {
            // Melting's passive: enemy deaths chain-explode (does not hurt the
            // player).
            if player.chain_explosions {
                commands.spawn((
                    GameCleanup,
                    LevelCleanup,
                    Explosion {
                        timer: Timer::from_seconds(0.05, TimerMode::Once),
                        radius: 100.0,
                        damage: 3,
                        team: Team::Player,
                        hits_player: false,
                        source: None,
                    },
                    Transform::from_translation(pos.extend(20.0)),
                ));
            }
            for _ in 0..enemy.rad_drop {
                spawn_rad(
                    &mut commands,
                    &catalog,
                    &asset_server,
                    pos + random_offset(),
                    1,
                );
            }
            maybe_spawn_drop(
                &mut commands,
                &catalog,
                &asset_server,
                pos,
                enemy.drop_chance,
                enemy.weapon_chance,
                &player,
                &pinv,
                &phealth,
            );
        }
    }

    // Chicken headless soak: one lethal hit per floor buffered to 1 HP.
    if phealth.hp <= 0 && player.headless_ready && !run.game_over {
        player.headless_ready = false;
        phealth.hp = 1;
        phealth.invuln = Timer::from_seconds(1.5, TimerMode::Once);
        HitFlash::apply(&mut commands, player_e, Color::srgb(1.0, 0.95, 0.6), 0.25);
        audio.play_pickup(&mut commands);
        VfxSpawner::spawn_burst(
            &mut commands,
            player_tf.translation.truncate(),
            12,
            Color::srgb(1.0, 0.95, 0.6),
            (60.0, 160.0),
        );
        return;
    }
    // Player death (Strong Spirit / Last Wish / Melting→Skeleton).
    if phealth.hp <= 0 && !run.game_over {
        if player.strong_spirit_ready {
            player.strong_spirit_ready = false;
            phealth.hp = phealth.max;
            phealth.invuln = Timer::from_seconds(1.0, TimerMode::Once);
            HitFlash::apply(&mut commands, player_e, Color::srgb(0.3, 1.0, 0.5), 0.3);
            audio.play_pickup(&mut commands);
            return;
        }
        if !player.last_wish_used {
            player.last_wish_used = true;
            phealth.hp = phealth.max;
            for kind in [
                AmmoKind::Bullets,
                AmmoKind::Shells,
                AmmoKind::Bolts,
                AmmoKind::Explosives,
                AmmoKind::Energy,
            ] {
                *pinv.ammo_mut(kind) = player.ammo_cap(kind);
            }
            HitFlash::apply(&mut commands, player_e, Color::srgb(0.3, 1.0, 0.5), 0.3);
            audio.play_pickup(&mut commands);
            return;
        }

        // Melting → Skeleton: die within ~96px of a living Necromancer.
        if race_state.race == RaceId::Melting {
            let ppos = player_tf.translation.truncate();
            let near_necro = q.iter().any(|(_, ntf, team, health, enemy)| {
                *team == Team::Enemy
                    && health.hp > 0
                    && enemy
                        .map(|e| e.kind == EnemyKind::Necromancer)
                        .unwrap_or(false)
                    && ntf.translation.truncate().distance(ppos) <= 96.0
            });
            if near_necro {
                let unlocked = crate::game::generated::unlocks::try_unlock_skeleton(&mut save);
                if unlocked {
                    dirty.0 = true;
                }

                let sk = crate::game::content::character_def(RaceId::Skeleton);
                race_state.race = RaceId::Skeleton;
                player.ability = sk.ability;
                player.chain_explosions = false;
                player.shield_on_hit = false;
                player.headless_ready = false;
                // Skeleton is frail.
                phealth.max = sk.max_hp.max(2);
                phealth.hp = phealth.max;
                phealth.invuln = Timer::from_seconds(1.25, TimerMode::Once);
                player.ability_cooldown = Timer::from_seconds(0.0, TimerMode::Once);

                if let Some(ref mut spr) = player_sprite {
                    spr.color = sk.color;
                }

                toast.show(if unlocked {
                    "SKELETON UNLOCKED"
                } else {
                    "BACK FROM THE DEAD"
                });
                HitFlash::apply(&mut commands, player_e, Color::srgb(0.95, 0.95, 1.0), 0.35);
                ScreenEffects::flash_white(&mut flash, 0.08);
                ScreenEffects::add_trauma(&mut trauma, 0.35);
                audio.play_portal(&mut commands);
                VfxSpawner::spawn_burst(
                    &mut commands,
                    ppos,
                    28,
                    Color::srgb(0.9, 0.9, 1.0),
                    (80.0, 220.0),
                );
                return;
            }
        }

        run.game_over = true;
        commands.entity(player_e).despawn();
        commands.spawn((
            GameCleanup,
            crate::game::reactive_audio::QueuedReactiveCue(
                crate::game::reactive_audio::ReactiveCue::PlayerDeath,
            ),
        ));

        ScreenEffects::add_trauma(&mut trauma, 0.8);
        ScreenEffects::chromatic_pulse(&mut chroma, 0.7);
        ScreenEffects::flash_white(&mut flash, 0.06);
        hitstop.trigger(0.1, 0.25);
        GameFeel::slow_motion(&mut slow_mo, 0.3, 1.4);
        GameFeel::rumble_controller(&mut rumble, &gamepads, 0.9, 1.0, 0.6);
        audio.play_death(&mut commands);

        let pos = player_tf.translation.truncate();
        VfxSpawner::spawn_burst(
            &mut commands,
            pos,
            60,
            Color::srgb(0.2, 1.0, 0.25),
            (120.0, 460.0),
        );
        VfxSpawner::spawn_burst(
            &mut commands,
            pos,
            40,
            Color::srgb(1.0, 0.9, 0.4),
            (100.0, 380.0),
        );

        if save.best_floor < run.floor {
            save.best_floor = run.floor;
        }
        save.total_runs += 1;
        save.total_kills = save.total_kills.saturating_add(run.total_kills);
        crate::game::generated::unlocks::check_progress_unlocks(
            &mut save,
            run.floor,
            run.loop_count,
            true,
            false,
            false,
        );
        dirty.0 = true;
        paused.0 = false;
        toast.show(&format!("KILLED BY {}", last_damage.source_name));

        // Keep the game running in InGame so the death slow-mo plays; the
        // gameplay gate on `run.game_over` freezes actions. The UI overlay
        // handles retry/quit.
        let _ = transition;
    }
}

fn give_ammo(inv: &mut Inventory, player: &Player) {
    let id = inv.weapons[inv.current];
    if id == WeaponId::NONE {
        let mut rng = rand::rng();
        let kind = random_ammo_kind(&mut rng);
        let slot = inv.ammo_mut(kind);
        let add = ammo_pickup_amount(kind);
        *slot = (*slot + add).min(player.ammo_cap(kind));
        return;
    }
    let def = crate::game::weapon_runtime::weapon_runtime_def(id);
    if def.melee.is_some() {
        return;
    }
    let slot = inv.ammo_mut(def.ammo);
    let add = ammo_pickup_amount(def.ammo);
    *slot = (*slot + add).min(player.ammo_cap(def.ammo));
}

/// Sum of per-weapon ammo need factors (0.1 well-stocked .. 0.75 low).
fn scrub_need(inv: &Inventory, player: &Player) -> f32 {
    let mut need = 0.0;
    for w in inv.weapons.iter().take(inv.weapon_slots) {
        if *w == WeaponId::NONE {
            continue;
        }
        let def = crate::game::weapon_runtime::weapon_runtime_def(*w);
        if def.melee.is_some() {
            need += 0.5;
            continue;
        }
        let cap = player.ammo_cap(def.ammo) as f32;
        let am = inv_ammo(inv, def.ammo) as f32;
        if am < cap * 0.2 {
            need += 0.75;
        } else if am > cap * 0.6 {
            need += 0.1;
        } else {
            need += 0.5;
        }
    }
    need
}

fn inv_ammo(inv: &Inventory, kind: AmmoKind) -> i32 {
    inv.ammo_of(kind)
}

pub fn spawn_rad(
    commands: &mut Commands,
    catalog: &AssetCatalog,
    asset_server: &AssetServer,
    pos: Vec2,
    amount: u32,
) {
    spawn_pickup(
        commands,
        catalog,
        asset_server,
        PickupKind::Rad(amount),
        pos,
    );
}

fn random_offset() -> Vec2 {
    let mut rng = rand::rng();
    let a = rng.random_range(0.0..std::f32::consts::TAU);
    let d = rng.random_range(0.0..22.0);
    Vec2::new(a.cos(), a.sin()) * d
}

/// scrDrop-equivalent: `chance` is a per-mille chance scaled by ammo need and
/// Rabbit Paw; if it lands, drop a medkit (when hurt) or ammo. Otherwise, roll
/// the weapon chance.
/// scrDrop-equivalent: `chance` is a per-mille chance scaled by ammo need and
/// Rabbit Paw; if it lands, drop a medkit (when hurt) or ammo. Otherwise, roll
/// the weapon chance.
pub fn maybe_spawn_drop(
    commands: &mut Commands,
    catalog: &AssetCatalog,
    asset_server: &AssetServer,
    pos: Vec2,
    chance: usize,
    weapon_chance: usize,
    player: &Player,
    inv: &Inventory,
    health: &Health,
) {
    let mut rng = rand::rng();

    let need = scrub_need(inv, player);
    let paw = player.drop_mult;
    let roll = rng.random_range(0.0..100.0);

    if roll < (chance as f32 * (need + paw)) {
        // Health: only when hurt, and only 2/3 of the time.
        if rng.random_range(0..health.max.max(1)) as i32 > health.hp && rng.random_range(0..3) < 2 {
            spawn_pickup(commands, catalog, asset_server, PickupKind::Medkit(2), pos);
        } else {
            let ammo = random_ammo_kind(&mut rng);
            spawn_pickup(
                commands,
                catalog,
                asset_server,
                PickupKind::Ammo(ammo, ammo_pickup_amount(ammo)),
                pos,
            );
        }
    } else if weapon_chance > 0 && rng.random_range(0.0..100.0) < weapon_chance as f32 {
        let weapon = random_weapon(&mut rng);
        spawn_pickup(
            commands,
            catalog,
            asset_server,
            PickupKind::Weapon(weapon),
            pos,
        );
    }
}

fn random_ammo_kind(rng: &mut impl rand::RngExt) -> AmmoKind {
    match rng.random_range(0..5) {
        0 => AmmoKind::Bullets,
        1 => AmmoKind::Shells,
        2 => AmmoKind::Bolts,
        3 => AmmoKind::Explosives,
        _ => AmmoKind::Energy,
    }
}

pub fn random_weapon(rng: &mut impl rand::RngExt) -> WeaponId {
    // Map from legacy 8-weapon pool to WeaponId
    match rng.random_range(0..8) {
        0 => WeaponId::MACHINEGUN,
        1 => WeaponId(5),
        2 => WeaponId::CROSSBOW,
        3 => WeaponId::GRENADE_LAUNCHER,
        4 => WeaponId::SMG,
        5 => WeaponId::ASSAULT_RIFLE,
        6 => WeaponId::WRENCH,
        _ => WeaponId::SLEDGEHAMMER,
    }
}

/// Random gold weapon (gold barrels / mansion drops).
pub fn random_gold_weapon(rng: &mut impl rand::RngExt) -> WeaponId {
    let gold: Vec<WeaponId> = crate::game::weapons_data::WEAPONS
        .iter()
        .filter(|w| w.wep_gold)
        .map(|w| WeaponId(w.id))
        .collect();
    if gold.is_empty() {
        return random_weapon(rng);
    }
    gold[rng.random_range(0..gold.len())]
}

pub fn spawn_chest(
    commands: &mut Commands,
    catalog: &AssetCatalog,
    asset_server: &AssetServer,
    pos: Vec2,
) {
    crate::game::pickups::spawn_chest(commands, catalog, asset_server, ChestKind::Weapon, pos);
}
