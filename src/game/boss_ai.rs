//! Boss-specific AI state machines.
//!
//! Intentionally bypasses the generic enemy chase/fire loop: bosses get their
//! own phases so they stop feeling like scaled-up Bandits. Generic `EnemyBrain`
//! still carries the shared melee-contact timer.

use bevy::prelude::*;

use crate::game::boss_patterns::{dir_from_angle, fan_angles, lead_target, ring_angles};
use crate::game::combat::Explosion;
use crate::game::components::*;
use crate::game::content::*;
use crate::game::world::{clamp_to_arena, resolve_prop_collision};
use game_utils_bevy::screen_effects::{ScreenEffects, Trauma};

pub fn boss_ai(
    time: Res<Time<Fixed>>,
    mut commands: Commands,
    mut trauma: ResMut<Trauma>,
    player_q: Query<(&Transform, &Velocity), (With<Player>, Without<Enemy>)>,
    mut bosses: Query<
        (
            Entity,
            &Enemy,
            &mut BossBrain,
            &mut EnemyBrain,
            &mut Velocity,
            &mut Transform,
            &mut Health,
            &mut Sprite,
        ),
        With<Enemy>,
    >,
    props: Query<(Entity, &Prop, &Transform), With<Prop>>,
) {
    let Ok((player_tf, player_vel)) = player_q.single() else {
        return;
    };

    let player_pos = player_tf.translation.truncate();
    let player_velocity = player_vel.0;
    let dt = time.delta_secs();

    for (entity, enemy, mut boss, mut brain, mut vel, mut tf, mut health, mut sprite) in
        bosses.iter_mut()
    {
        let def = enemy_def(enemy.kind);
        if !def.boss {
            continue;
        }

        let pos = tf.translation.truncate();
        let to_player = player_pos - pos;
        let dir = to_player.normalize_or_zero();
        sprite.flip_x = dir.x < 0.0;

        boss.enraged = health.hp <= (health.max / 2).max(1);
        boss.phase_timer.tick(time.delta());
        boss.attack_timer.tick(time.delta());
        boss.special_timer.tick(time.delta());
        brain.melee.tick(time.delta());

        match enemy.kind {
            EnemyKind::BigBandit => big_bandit_ai(
                &mut commands,
                &mut trauma,
                entity,
                &mut boss,
                &mut vel,
                &mut tf,
                def,
                pos,
                player_pos,
                dir,
                dt,
                &props,
            ),
            EnemyKind::BigDog => big_dog_ai(
                &mut commands,
                &mut trauma,
                entity,
                &mut boss,
                &mut vel,
                &mut tf,
                def,
                pos,
                player_pos,
                dir,
                dt,
                &props,
            ),
            EnemyKind::LilHunter => lil_hunter_ai(
                &mut commands,
                &mut trauma,
                entity,
                &mut boss,
                &mut vel,
                &mut tf,
                &mut health,
                def,
                pos,
                player_pos,
                player_velocity,
                dir,
                dt,
                &props,
            ),
            EnemyKind::Throne => throne_ai(
                &mut commands,
                &mut trauma,
                entity,
                &mut boss,
                &mut vel,
                &mut tf,
                def,
                pos,
                player_pos,
                dir,
                dt,
            ),
            _ => {}
        }

        clamp_to_arena(&mut tf.translation, def.radius);
    }
}

#[allow(clippy::too_many_arguments)]
fn big_bandit_ai(
    commands: &mut Commands,
    trauma: &mut ResMut<Trauma>,
    owner: Entity,
    boss: &mut BossBrain,
    vel: &mut Velocity,
    tf: &mut Transform,
    def: EnemyDef,
    pos: Vec2,
    player_pos: Vec2,
    dir: Vec2,
    dt: f32,
    props: &Query<(Entity, &Prop, &Transform), With<Prop>>,
) {
    match boss.phase {
        BossPhase::Idle | BossPhase::Cooldown => {
            // Slow drift toward mid-range.
            let desired = if pos.distance(player_pos) < 120.0 {
                -dir
            } else {
                dir
            };
            vel.0 += desired * def.accel * 0.55 * dt;
            limit_velocity(vel, 135.0);
            tf.translation += (vel.0 * dt).extend(0.0);
            resolve_prop_collision(&mut tf.translation, def.radius, props);

            if boss.attack_timer.just_finished() {
                fire_fan(
                    commands,
                    owner,
                    pos,
                    dir,
                    Team::Enemy,
                    5,
                    0.16,
                    165.0,
                    3,
                    3.2,
                    4.5,
                    Color::srgb(1.0, 0.28, 0.08),
                    8.0,
                );
            }

            if boss.special_timer.just_finished() && pos.distance(player_pos) < 520.0 {
                boss.target = player_pos;
                boss.set_phase(BossPhase::Telegraph, 0.38);
                vel.0 *= 0.25;
                ScreenEffects::add_trauma(trauma, 0.08);
            }
        }

        BossPhase::Telegraph => {
            vel.0 *= 0.80_f32.powf(dt * 60.0);
            tf.scale =
                Vec3::splat(1.0 + (boss.phase_timer.elapsed_secs() * 18.0).sin().abs() * 0.08);

            if boss.phase_timer.just_finished() {
                boss.set_phase(BossPhase::Charging, 0.42);
                let charge_dir = (boss.target - pos).normalize_or_zero();
                vel.0 = charge_dir * 680.0;
                ScreenEffects::add_trauma(trauma, 0.18);
            }
        }

        BossPhase::Charging => {
            tf.scale = Vec3::splat(1.06);
            tf.translation += (vel.0 * dt).extend(0.0);
            resolve_prop_collision(&mut tf.translation, def.radius, props);

            if boss.phase_timer.just_finished() {
                boss.set_phase(BossPhase::Cooldown, 0.55);
                boss.special_timer = Timer::from_seconds(2.2, TimerMode::Repeating);
                vel.0 *= 0.15;
                tf.scale = Vec3::ONE;

                // Recovery blast.
                fire_ring(
                    commands,
                    owner,
                    tf.translation.truncate(),
                    Team::Enemy,
                    10,
                    boss.pattern_index as f32 * 0.13,
                    135.0,
                    2,
                    1.9,
                    3.5,
                    Color::srgb(1.0, 0.35, 0.1),
                    7.0,
                );
                boss.pattern_index += 1;
            }
        }

        _ => {
            boss.set_phase(BossPhase::Idle, 0.1);
            tf.scale = Vec3::ONE;
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn big_dog_ai(
    commands: &mut Commands,
    trauma: &mut ResMut<Trauma>,
    owner: Entity,
    boss: &mut BossBrain,
    vel: &mut Velocity,
    tf: &mut Transform,
    def: EnemyDef,
    pos: Vec2,
    player_pos: Vec2,
    dir: Vec2,
    dt: f32,
    props: &Query<(Entity, &Prop, &Transform), With<Prop>>,
) {
    // Big Dog is heavy: it drifts toward home and the player, but never chases.
    let desired_home = (boss.home - pos).normalize_or_zero() * 0.35;
    let desired_player = dir * 0.25;
    vel.0 += (desired_home + desired_player).normalize_or_zero() * def.accel * 0.18 * dt;
    limit_velocity(vel, if boss.enraged { 85.0 } else { 60.0 });
    tf.translation += (vel.0 * dt).extend(0.0);
    resolve_prop_collision(&mut tf.translation, def.radius, props);

    if boss.attack_timer.just_finished() {
        let base = (player_pos - pos).to_angle();
        let count = if boss.enraged { 7 } else { 5 };
        for angle in fan_angles(base, count, 0.12) {
            let shot_dir = dir_from_angle(angle);
            fire_projectile(
                commands,
                owner,
                pos + shot_dir * 26.0,
                shot_dir,
                Team::Enemy,
                190.0,
                3,
                2.6,
                6.0,
                4.5,
                Color::srgb(1.0, 0.42, 0.12),
                9.0,
            );
        }
    }

    if boss.special_timer.just_finished() {
        boss.pattern_index += 1;
        if boss.pattern_index % 3 == 0 {
            big_dog_stomp(commands, trauma, owner, pos, boss.enraged);
        } else {
            big_dog_rotating_salvo(commands, owner, pos, boss.pattern_index, boss.enraged);
        }
    }
}

fn big_dog_rotating_salvo(
    commands: &mut Commands,
    owner: Entity,
    pos: Vec2,
    pattern_index: usize,
    enraged: bool,
) {
    let count = if enraged { 18 } else { 14 };
    let phase = pattern_index as f32 * 0.23;

    for angle in ring_angles(count, phase) {
        let dir = dir_from_angle(angle);
        fire_projectile(
            commands,
            owner,
            pos + dir * 34.0,
            dir,
            Team::Enemy,
            if enraged { 210.0 } else { 165.0 },
            2,
            2.2,
            5.0,
            4.0,
            Color::srgb(1.0, 0.55, 0.18),
            8.0,
        );
    }
}

fn big_dog_stomp(
    commands: &mut Commands,
    trauma: &mut ResMut<Trauma>,
    owner: Entity,
    pos: Vec2,
    enraged: bool,
) {
    ScreenEffects::add_trauma(trauma, if enraged { 0.35 } else { 0.24 });

    commands.spawn((
        GameCleanup,
        LevelCleanup,
        Explosion {
            timer: Timer::from_seconds(0.03, TimerMode::Once),
            radius: if enraged { 155.0 } else { 120.0 },
            damage: if enraged { 8 } else { 6 },
            team: Team::Enemy,
            hits_player: true,
            source: Some(DamageSource {
                owner,
                team: Team::Enemy,
                hit_id: HitId::Enemy(0),
            }),
        },
        Transform::from_translation(pos.extend(20.0)),
    ));

    fire_ring(
        commands,
        owner,
        pos,
        Team::Enemy,
        if enraged { 20 } else { 14 },
        0.0,
        if enraged { 230.0 } else { 180.0 },
        2,
        2.1,
        4.0,
        Color::srgb(1.0, 0.35, 0.08),
        8.0,
    );
}

#[allow(clippy::too_many_arguments)]
fn lil_hunter_ai(
    commands: &mut Commands,
    trauma: &mut ResMut<Trauma>,
    owner: Entity,
    boss: &mut BossBrain,
    vel: &mut Velocity,
    tf: &mut Transform,
    health: &mut Health,
    def: EnemyDef,
    pos: Vec2,
    player_pos: Vec2,
    player_vel: Vec2,
    dir: Vec2,
    dt: f32,
    props: &Query<(Entity, &Prop, &Transform), With<Prop>>,
) {
    match boss.phase {
        BossPhase::Idle | BossPhase::Cooldown => {
            // Keep a skirmish range.
            let desired = if pos.distance(player_pos) < 150.0 {
                -dir
            } else {
                dir
            };

            vel.0 += desired * def.accel * 0.7 * dt;
            limit_velocity(vel, if boss.enraged { 185.0 } else { 145.0 });
            tf.translation += (vel.0 * dt).extend(0.0);
            resolve_prop_collision(&mut tf.translation, def.radius, props);

            if boss.attack_timer.just_finished() {
                lil_hunter_burst(commands, owner, pos, player_pos, player_vel, boss.enraged);
            }

            if boss.special_timer.just_finished() {
                boss.target = player_pos + player_vel * 0.35;
                boss.set_phase(BossPhase::Telegraph, 0.25);
                vel.0 *= 0.2;
            }
        }

        BossPhase::Telegraph => {
            tf.scale =
                Vec3::splat(1.0 + (boss.phase_timer.elapsed_secs() * 20.0).sin().abs() * 0.12);
            vel.0 *= 0.70_f32.powf(dt * 60.0);

            if boss.phase_timer.just_finished() {
                boss.set_phase(BossPhase::Jumping, if boss.enraged { 0.36 } else { 0.44 });
                let jump_dir = (boss.target - pos).normalize_or_zero();
                vel.0 = jump_dir * if boss.enraged { 620.0 } else { 520.0 };
                health.invuln = Timer::from_seconds(0.18, TimerMode::Once);
                tf.translation.z = 24.0;
            }
        }

        BossPhase::Jumping => {
            tf.translation += (vel.0 * dt).extend(0.0);
            tf.translation.z = 24.0 + (boss.phase_timer.elapsed_secs() * 32.0).sin().abs() * 20.0;

            if boss.phase_timer.just_finished() {
                boss.set_phase(BossPhase::Landing, 0.18);
                tf.translation.z = 10.0;
                vel.0 *= 0.1;
                ScreenEffects::add_trauma(trauma, 0.28);

                let land_pos = tf.translation.truncate();
                commands.spawn((
                    GameCleanup,
                    LevelCleanup,
                    Explosion {
                        timer: Timer::from_seconds(0.02, TimerMode::Once),
                        radius: if boss.enraged { 95.0 } else { 75.0 },
                        damage: if boss.enraged { 6 } else { 4 },
                        team: Team::Enemy,
                        hits_player: true,
                        source: Some(DamageSource {
                            owner,
                            team: Team::Enemy,
                            hit_id: HitId::Enemy(0),
                        }),
                    },
                    Transform::from_translation(land_pos.extend(20.0)),
                ));

                fire_ring(
                    commands,
                    owner,
                    land_pos,
                    Team::Enemy,
                    if boss.enraged { 14 } else { 10 },
                    boss.pattern_index as f32 * 0.17,
                    170.0,
                    2,
                    1.7,
                    4.0,
                    Color::srgb(0.65, 0.9, 1.0),
                    7.0,
                );
                boss.pattern_index += 1;
            }
        }

        BossPhase::Landing => {
            if boss.phase_timer.just_finished() {
                boss.set_phase(BossPhase::Cooldown, 0.35);
                tf.scale = Vec3::ONE;
                // Post-land double burst.
                lil_hunter_burst(
                    commands,
                    owner,
                    tf.translation.truncate(),
                    player_pos,
                    player_vel,
                    true,
                );
            }
        }

        _ => {
            boss.set_phase(BossPhase::Idle, 0.1);
            tf.scale = Vec3::ONE;
            tf.translation.z = 10.0;
        }
    }
}

fn lil_hunter_burst(
    commands: &mut Commands,
    owner: Entity,
    pos: Vec2,
    player_pos: Vec2,
    player_vel: Vec2,
    enraged: bool,
) {
    let aim = lead_target(pos, player_pos, player_vel, 260.0);
    let base = aim.to_angle();
    let count = if enraged { 5 } else { 3 };

    for angle in fan_angles(base, count, 0.13) {
        let dir = dir_from_angle(angle);
        fire_projectile(
            commands,
            owner,
            pos + dir * 20.0,
            dir,
            Team::Enemy,
            if enraged { 280.0 } else { 245.0 },
            3,
            2.3,
            4.0,
            4.0,
            Color::srgb(0.6, 0.95, 1.0),
            7.0,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn throne_ai(
    commands: &mut Commands,
    trauma: &mut ResMut<Trauma>,
    owner: Entity,
    boss: &mut BossBrain,
    vel: &mut Velocity,
    tf: &mut Transform,
    def: EnemyDef,
    pos: Vec2,
    player_pos: Vec2,
    dir: Vec2,
    dt: f32,
) {
    // Throne stays central and only drifts back toward its home.
    let to_home = boss.home - pos;
    vel.0 += to_home.normalize_or_zero() * def.accel * 0.15 * dt;
    limit_velocity(vel, 70.0);
    tf.translation += (vel.0 * dt).extend(0.0);
    tf.translation.x = tf.translation.x.clamp(-ARENA_W * 0.25, ARENA_W * 0.25);
    tf.translation.y = tf.translation.y.clamp(-ARENA_H * 0.25, ARENA_H * 0.25);

    if boss.attack_timer.just_finished() {
        boss.pattern_index += 1;

        if boss.pattern_index % 4 == 0 {
            boss.set_phase(BossPhase::Beam, 0.3);
            throne_beam_lanes(commands, pos, dir, boss.enraged);
            ScreenEffects::add_trauma(trauma, if boss.enraged { 0.26 } else { 0.18 });
        } else if boss.pattern_index % 2 == 0 {
            throne_cross_rings(commands, owner, pos, boss.pattern_index, boss.enraged);
        } else {
            throne_aimed_spread(commands, owner, pos, player_pos, boss.enraged);
        }
    }

    if boss.special_timer.just_finished() {
        boss.set_phase(BossPhase::Radial, 0.45);
        throne_radial_burst(commands, owner, pos, boss.pattern_index, boss.enraged);
        ScreenEffects::add_trauma(trauma, if boss.enraged { 0.34 } else { 0.22 });
    }
}

fn throne_aimed_spread(
    commands: &mut Commands,
    owner: Entity,
    pos: Vec2,
    player_pos: Vec2,
    enraged: bool,
) {
    let base = (player_pos - pos).to_angle();
    let count = if enraged { 9 } else { 7 };
    for angle in fan_angles(base, count, 0.10) {
        let dir = dir_from_angle(angle);
        fire_projectile(
            commands,
            owner,
            pos + dir * 44.0,
            dir,
            Team::Enemy,
            if enraged { 250.0 } else { 210.0 },
            3,
            3.0,
            5.5,
            5.0,
            Color::srgb(1.0, 0.75, 0.25),
            9.0,
        );
    }
}

fn throne_cross_rings(
    commands: &mut Commands,
    owner: Entity,
    pos: Vec2,
    index: usize,
    enraged: bool,
) {
    let phase = index as f32 * 0.09;
    let count = if enraged { 24 } else { 18 };

    for angle in ring_angles(count, phase) {
        let dir = dir_from_angle(angle);
        fire_projectile(
            commands,
            owner,
            pos + dir * 42.0,
            dir,
            Team::Enemy,
            if enraged { 230.0 } else { 185.0 },
            2,
            3.2,
            4.5,
            4.5,
            Color::srgb(1.0, 0.55, 0.15),
            8.0,
        );
    }
}

fn throne_radial_burst(
    commands: &mut Commands,
    owner: Entity,
    pos: Vec2,
    index: usize,
    enraged: bool,
) {
    let count = if enraged { 32 } else { 24 };
    let phase = index as f32 * 0.21;

    for angle in ring_angles(count, phase) {
        let dir = dir_from_angle(angle);
        fire_projectile(
            commands,
            owner,
            pos + dir * 50.0,
            dir,
            Team::Enemy,
            if enraged { 270.0 } else { 220.0 },
            2,
            2.8,
            4.5,
            4.0,
            Color::srgb(1.0, 0.38, 0.12),
            8.0,
        );
    }
}

/// Four beam lanes on the aim axis and its perpendiculars. Team-safe damage
/// flows through `Beam.team` inside `tick_beams`.
fn throne_beam_lanes(commands: &mut Commands, pos: Vec2, dir_to_player: Vec2, enraged: bool) {
    let base = dir_to_player.to_angle();
    let angles = [
        base,
        base + std::f32::consts::FRAC_PI_2,
        base - std::f32::consts::FRAC_PI_2,
        base + std::f32::consts::PI,
    ];

    for angle in angles {
        let dir = dir_from_angle(angle);
        let length = if enraged { 680.0 } else { 560.0 };
        let width = if enraged { 30.0 } else { 24.0 };
        commands.spawn((
            GameCleanup,
            LevelCleanup,
            Beam {
                team: Team::Enemy,
                dir,
                length,
                width,
                damage: if enraged { 5 } else { 4 },
                knockback: 120.0,
                timer: Timer::from_seconds(if enraged { 0.32 } else { 0.24 }, TimerMode::Once),
                tick: Timer::from_seconds(0.08, TimerMode::Repeating),
            },
            Sprite {
                color: Color::srgba(1.0, 0.65, 0.18, 0.65),
                custom_size: Some(Vec2::new(length, width)),
                ..default()
            },
            Transform::from_translation((pos + dir * 260.0).extend(18.0))
                .with_rotation(Quat::from_rotation_z(angle)),
        ));
    }
}

#[allow(clippy::too_many_arguments)]
fn fire_fan(
    commands: &mut Commands,
    owner: Entity,
    pos: Vec2,
    dir: Vec2,
    team: Team,
    count: usize,
    spread: f32,
    speed: f32,
    damage: i32,
    lifetime: f32,
    radius: f32,
    color: Color,
    size: f32,
) {
    let base = dir.to_angle();
    for angle in fan_angles(base, count, spread) {
        let shot_dir = dir_from_angle(angle);
        fire_projectile(
            commands,
            owner,
            pos + shot_dir * 20.0,
            shot_dir,
            team,
            speed,
            damage,
            lifetime,
            radius,
            120.0,
            color,
            size,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn fire_ring(
    commands: &mut Commands,
    owner: Entity,
    pos: Vec2,
    team: Team,
    count: usize,
    phase: f32,
    speed: f32,
    damage: i32,
    lifetime: f32,
    radius: f32,
    color: Color,
    size: f32,
) {
    for angle in ring_angles(count, phase) {
        let dir = dir_from_angle(angle);
        fire_projectile(
            commands,
            owner,
            pos + dir * 22.0,
            dir,
            team,
            speed,
            damage,
            lifetime,
            radius,
            100.0,
            color,
            size,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn fire_projectile(
    commands: &mut Commands,
    owner: Entity,
    pos: Vec2,
    dir: Vec2,
    team: Team,
    speed: f32,
    damage: i32,
    lifetime: f32,
    radius: f32,
    knockback: f32,
    color: Color,
    size: f32,
) {
    let angle = dir.y.atan2(dir.x);
    commands.spawn((
        GameCleanup,
        LevelCleanup,
        team,
        Projectile {
            damage,
            life: Timer::from_seconds(lifetime, TimerMode::Once),
            radius,
            knockback,
            explosive: false,
            source: Some(DamageSource {
                owner,
                team,
                hit_id: HitId::Enemy(0),
            }),
        },
        Velocity(dir * speed),
        Sprite {
            color,
            custom_size: Some(Vec2::splat(size)),
            ..default()
        },
        Transform::from_translation(pos.extend(15.0)).with_rotation(Quat::from_rotation_z(angle)),
    ));
}

fn limit_velocity(vel: &mut Velocity, max: f32) {
    if vel.0.length() > max {
        vel.0 = vel.0.normalize_or_zero() * max;
    }
}
