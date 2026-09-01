//! Boss-specific AI state machines.
//!
//! Intentionally bypasses the generic enemy chase/fire loop: bosses get their
//! own phases so they stop feeling like scaled-up Bandits. Generic `EnemyBrain`
//! still carries the shared melee-contact timer.

use std::time::Duration;

use bevy::prelude::*;
use rand::RngExt;

use crate::game::boss_patterns::{
    dir_from_angle, fan_angles, hyper_orbit_count, lead_target, ring_angles,
};
use crate::game::combat::Explosion;
use crate::game::components::*;
use crate::game::content::*;
use crate::game::world::{clamp_to_arena, resolve_prop_collision};
use game_utils_bevy::screen_effects::{ScreenEffects, Trauma};

pub fn boss_ai(
    time: Res<Time<Fixed>>,
    mut commands: Commands,
    catalog: Res<AssetCatalog>,
    asset_server: Res<AssetServer>,
    run: Res<Run>,
    mut trauma: ResMut<Trauma>,
    throne_room: Res<ThroneRoomState>,
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
        (With<Enemy>, Without<WallTile>),
    >,
    props: Query<(Entity, &Prop, &Transform), (With<Prop>, Without<Enemy>)>,
    walls: Query<(Entity, &WallCell, &Transform), With<WallTile>>,
    // Non-boss children (disjoint from `bosses`) for FrogQueen egg budget.
    children: Query<
        (Entity, &'static Enemy, &'static Transform),
        (With<Enemy>, Without<BossBrain>),
    >,
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
        // GML enemy friction 0.4 for bosses (applied inside big_bandit already)
        if !matches!(enemy.kind, EnemyKind::BigBandit | EnemyKind::BigBanditLoop) {
            apply_gml_friction(&mut vel.0, 0.4, dt);
        }

        match enemy.kind {
            EnemyKind::BigBandit | EnemyKind::BigBanditLoop => big_bandit_ai(
                &mut commands,
                &catalog,
                &asset_server,
                &mut trauma,
                entity,
                &mut boss,
                &mut brain,
                &mut vel,
                &mut tf,
                def,
                pos,
                player_pos,
                dir,
                dt,
                &props,
                &walls,
            ),
            EnemyKind::BigDog | EnemyKind::BigDogLoop => big_dog_ai(
                &mut commands,
                &catalog,
                &asset_server,
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
            EnemyKind::LilHunter | EnemyKind::LilHunterLoop => lil_hunter_ai(
                &mut commands,
                &catalog,
                &asset_server,
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
                &catalog,
                &asset_server,
                &mut trauma,
                &throne_room,
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
            EnemyKind::ThroneII => throne_ii_ai(
                &mut commands,
                &catalog,
                &asset_server,
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
                run.loop_count,
            ),
            EnemyKind::Hyper => hyper_ai(
                &mut commands,
                &catalog,
                &asset_server,
                &mut trauma,
                entity,
                &mut boss,
                &mut vel,
                &mut tf,
                def,
                pos,
                player_pos,
                dt,
                run.loop_count,
            ),
            EnemyKind::Mom => mom_ai(
                &mut commands,
                &catalog,
                &asset_server,
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
            EnemyKind::FrogQueen => frog_queen_ai(
                &mut commands,
                &catalog,
                &asset_server,
                &mut trauma,
                &children,
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
            EnemyKind::Technomancer => technomancer_ai(
                &mut commands,
                &catalog,
                &asset_server,
                &mut trauma,
                entity,
                &mut boss,
                &mut vel,
                def,
                pos,
                player_pos,
            ),
            EnemyKind::Captain => captain_ai(
                &mut commands,
                &catalog,
                &asset_server,
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
                &walls,
            ),
            EnemyKind::OldGuardian => old_guardian_ai(
                &mut commands,
                &catalog,
                &asset_server,
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
            _ => {}
        }

        clamp_to_arena(&mut tf.translation, def.radius);
    }
}

#[allow(clippy::too_many_arguments)]
fn big_bandit_ai(
    commands: &mut Commands,
    catalog: &AssetCatalog,
    asset_server: &AssetServer,
    trauma: &mut ResMut<Trauma>,
    owner: Entity,
    boss: &mut BossBrain,
    brain: &mut EnemyBrain,
    vel: &mut Velocity,
    tf: &mut Transform,
    def: EnemyDef,
    pos: Vec2,
    player_pos: Vec2,
    dir: Vec2,
    dt: f32,
    props: &Query<(Entity, &Prop, &Transform), (With<Prop>, Without<Enemy>)>,
    walls: &Query<(Entity, &WallCell, &Transform), With<WallTile>>,
) {
    let looped = def.name.contains("Loop");
    let kind = if looped {
        EnemyKind::BigBanditLoop
    } else {
        EnemyKind::BigBandit
    };
    // GML BanditBoss fidelity: friction 0.4 every tick, speed caps via gml_motion_add_clamp
    apply_gml_friction(&mut vel.0, 0.4, dt);

    match boss.phase {
        BossPhase::Idle | BossPhase::Cooldown => {
            // Walk like GML Other_10 non-charge: motion_add(direction,0.4) via walk timer sets vel
            // plus continuous friction already applied. Drift handled by BossBrain walk via
            // EnemyBrain.walk – ensure boss moves like trash with gml helpers.
            if brain.walk > 0.0 {
                let face = (boss.target - pos).normalize_or_zero();
                // GML Other_10 when not charging: motion_add(direction,1) + motion_add(gunangle,1) cap 3
                // For idle we approximate with small walk impulse away/toward player
                let move_dir = if vel.0.length_squared() > 0.0 {
                    vel.0.normalize_or_zero()
                } else {
                    -dir
                };
                gml_motion_add_clamp(&mut vel.0, move_dir, 1.0, 3.0, dt);
                // also nudge toward gunangle
                if face.length_squared() > 0.0001 {
                    gml_motion_add_clamp(&mut vel.0, face, 0.5, 3.0, dt);
                }
                brain.walk -= dt * 30.0;
                if brain.walk < 0.0 {
                    brain.walk = 0.0;
                }
            }
            tf.translation += (vel.0 * dt).extend(0.0);
            resolve_prop_collision(&mut tf.translation, def.radius, props);

            if boss.attack_timer.just_finished() {
                let dist = pos.distance(player_pos);
                let los = !crate::game::walls::segment_hits_wall(pos, player_pos, walls);
                let period = if looped {
                    (20.0 + rand::rng().random_range(0.0..50.0)) / 30.0
                } else {
                    (30.0 + rand::rng().random_range(0.0..60.0)) / 30.0
                };
                boss.attack_timer = Timer::from_seconds(period, TimerMode::Once);

                // Alarm_1 chance to start burst (2/3) if LOS + dist>48 + intro done
                let intro_done = boss.pattern_index > 0 || boss.phase == BossPhase::Cooldown;
                // Use pattern_index as chargewait counter (like GML chargewait variable)
                // and also as intro flag: after first cycle pattern_index becomes non-zero
                let should_burst = los
                    && dist > 48.0
                    && dist < 240.0
                    && intro_done
                    && rand::rng().random::<f32>() < 2.0 / 3.0;
                if should_burst {
                    brain.ammo = if looped { 15 } else { 10 };
                    brain.burst_left = brain.ammo as usize;
                    brain.burst_timer = Timer::from_seconds(1.0 / 30.0, TimerMode::Once);
                    brain.gunangle = dir.y.atan2(dir.x);
                    boss.set_phase(BossPhase::Radial, 2.5);
                    boss.attack_timer = Timer::from_seconds(70.0 / 30.0, TimerMode::Once);
                } else {
                    // chargewait path – increment and possibly start Tell (Alarm_3)
                    // pattern_index reused as chargewait counter
                    let mut chargewait = boss.pattern_index.saturating_add(1);
                    if dist < 96.0 {
                        chargewait = chargewait.saturating_add(1);
                    }
                    boss.pattern_index = chargewait;
                    // GML: if chargewait>=2 or during intro then Alarm_3 -> charge
                    let intro_charge = pos.distance(boss.home) < 1.0;
                    if chargewait >= 2 || intro_charge {
                        boss.pattern_index = 0;
                        boss.target = player_pos;
                        brain.gunangle = dir.y.atan2(dir.x);
                        boss.set_phase(BossPhase::Telegraph, 15.0 / 30.0); // Alarm_3 =15f
                        vel.0 *= 0.2;
                        ScreenEffects::add_trauma(trauma, 0.08);
                    }
                }
                // walk step (Alarm_1 always sets walk)
                let away = -dir;
                let ang =
                    away.y.atan2(away.x) + rand::rng().random_range(-90f32..90.0).to_radians();
                vel.0 = Vec2::new(ang.cos(), ang.sin()) * (0.4 * 30.0);
                brain.walk = if dist > 64.0 {
                    40.0
                } else {
                    10.0 + rand::rng().random_range(0.0..10.0)
                };
            }
        }

        BossPhase::Radial => {
            // Burst = Alarm_2 every 4 frames while ammo > 0
            brain.burst_timer.tick(Duration::from_secs_f32(dt));
            brain.walk = 0.0;
            if brain.ammo > 0 && brain.burst_timer.just_finished() {
                let spread = rand::rng().random_range(-15f32..15.0).to_radians();
                let ang = brain.gunangle + spread;
                let sdir = Vec2::new(ang.cos(), ang.sin());
                // bullet speed 8 px/frame = 240 px/s
                fire_projectile(
                    commands,
                    catalog,
                    asset_server,
                    owner,
                    pos + sdir * 20.0,
                    sdir,
                    Team::Enemy,
                    240.0,
                    3,
                    3.2,
                    4.5,
                    120.0,
                    Color::srgb(1.0, 0.28, 0.08),
                    8.0,
                    Some(kind),
                );
                // recoil
                gml_motion_add_clamp(&mut vel.0, -sdir, 1.0, 5.0, dt);
                brain.ammo -= 1;
                brain.burst_left = brain.burst_left.saturating_sub(1);
                if looped && brain.ammo == 7 {
                    brain.gunangle = dir.y.atan2(dir.x); // mid-burst re-aim
                }
                brain.burst_timer = Timer::from_seconds(4.0 / 30.0, TimerMode::Once);
            }
            if brain.ammo == 0 {
                boss.set_phase(
                    BossPhase::Cooldown,
                    (60.0 + rand::rng().random_range(0.0..10.0)) / 30.0,
                );
                boss.attack_timer = Timer::from_seconds(
                    (60.0 + rand::rng().random_range(0.0..10.0)) / 30.0,
                    TimerMode::Once,
                );
            }
            tf.translation += (vel.0 * dt).extend(0.0);
            resolve_prop_collision(&mut tf.translation, def.radius, props);
        }

        BossPhase::Telegraph => {
            // sprBanditBossTell - hold still 15f
            vel.0 *= 0.5_f32.powf(dt * 30.0);
            tf.translation += (vel.0 * dt).extend(0.0);
            if boss.phase_timer.just_finished() {
                let charge_dir = (boss.target - pos).normalize_or_zero();
                brain.gunangle = charge_dir.y.atan2(charge_dir.x);
                // seed direction for Other_10 charge
                vel.0 = charge_dir * (2.0 * 30.0);
                boss.set_phase(BossPhase::Charging, 0.55); // ~ charge duration
                ScreenEffects::add_trauma(trauma, 0.18);
            }
        }

        BossPhase::Charging => {
            // Other_10 charge: motion_add(direction,2) + motion_add(gunangle,2) cap 5
            let move_dir = vel.0.normalize_or_zero();
            let gun = Vec2::new(brain.gunangle.cos(), brain.gunangle.sin());
            gml_motion_add_clamp(&mut vel.0, move_dir, 2.0, 5.0, dt);
            gml_motion_add_clamp(&mut vel.0, gun, 2.0, 5.0, dt);
            let before = tf.translation.truncate();
            tf.translation += (vel.0 * dt).extend(0.0);
            crate::game::walls::queue_wall_breaks_along_segment(
                commands,
                walls,
                before,
                tf.translation.truncate(),
                def.radius * 0.9,
            );
            resolve_prop_collision(&mut tf.translation, def.radius, props);
            if boss.phase_timer.just_finished() {
                boss.set_phase(BossPhase::Cooldown, 0.55);
                vel.0 *= 0.15;
                tf.scale = Vec3::ONE;
                // No recovery ring – not in base GML BanditBoss (removed divergence)
                boss.pattern_index = 0;
            } else {
                tf.scale = Vec3::splat(1.06);
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
    catalog: &AssetCatalog,
    asset_server: &AssetServer,
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
    props: &Query<(Entity, &Prop, &Transform), (With<Prop>, Without<Enemy>)>,
) {
    // Big Dog is heavy: it drifts toward home and the player, but never chases.
    let looped = def.name.contains("Loop");
    let kind = if looped {
        EnemyKind::BigDogLoop
    } else {
        EnemyKind::BigDog
    };
    let desired_home = (boss.home - pos).normalize_or_zero() * 0.35;
    let desired_player = dir * 0.25;
    vel.0 += (desired_home + desired_player).normalize_or_zero() * def.accel * 0.18 * dt;
    limit_velocity(vel, if boss.enraged { 85.0 } else { 60.0 });
    tf.translation += (vel.0 * dt).extend(0.0);
    resolve_prop_collision(&mut tf.translation, def.radius, props);

    if boss.attack_timer.just_finished() {
        let base = (player_pos - pos).to_angle();
        let count = if boss.enraged {
            if looped { 9 } else { 7 }
        } else if looped {
            7
        } else {
            5
        };
        for angle in fan_angles(base, count, 0.12) {
            let shot_dir = dir_from_angle(angle);
            fire_projectile(
                commands,
                catalog,
                asset_server,
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
                Some(kind),
            );
        }
    }

    if boss.special_timer.just_finished() {
        boss.pattern_index += 1;
        if boss.pattern_index % 3 == 0 {
            big_dog_stomp(
                commands,
                catalog,
                asset_server,
                trauma,
                owner,
                pos,
                boss.enraged,
                looped,
            );
        } else {
            big_dog_rotating_salvo(
                commands,
                catalog,
                asset_server,
                owner,
                pos,
                boss.pattern_index,
                boss.enraged,
                looped,
            );
        }
    }
}

fn big_dog_rotating_salvo(
    commands: &mut Commands,
    catalog: &AssetCatalog,
    asset_server: &AssetServer,
    owner: Entity,
    pos: Vec2,
    pattern_index: usize,
    enraged: bool,
    looped: bool,
) {
    let count = if enraged {
        if looped { 24 } else { 18 }
    } else if looped {
        18
    } else {
        14
    };
    let phase = pattern_index as f32 * 0.23;
    let kind = if looped {
        EnemyKind::BigDogLoop
    } else {
        EnemyKind::BigDog
    };

    for angle in ring_angles(count, phase) {
        let dir = dir_from_angle(angle);
        fire_projectile(
            commands,
            catalog,
            asset_server,
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
            Some(kind),
        );
    }
}

fn big_dog_stomp(
    commands: &mut Commands,
    catalog: &AssetCatalog,
    asset_server: &AssetServer,
    trauma: &mut ResMut<Trauma>,
    owner: Entity,
    pos: Vec2,
    enraged: bool,
    looped: bool,
) {
    ScreenEffects::add_trauma(trauma, if enraged || looped { 0.35 } else { 0.24 });

    let bigdog_kind = if looped {
        EnemyKind::BigDogLoop
    } else {
        EnemyKind::BigDog
    };
    commands.spawn((
        GameCleanup,
        LevelCleanup,
        Explosion {
            timer: Timer::from_seconds(0.03, TimerMode::Once),
            radius: if enraged { 155.0 } else { 120.0 },
            damage: if enraged { 8 } else { 6 },
            team: Team::Enemy,
            hits_player: true,
            source: Some(DamageSource::enemy(owner, bigdog_kind)),
        },
        Transform::from_translation(pos.extend(20.0)),
    ));

    fire_ring_with_kind(
        commands,
        catalog,
        asset_server,
        owner,
        pos,
        Team::Enemy,
        if looped {
            22
        } else if enraged {
            20
        } else {
            14
        },
        0.0,
        if enraged { 230.0 } else { 180.0 },
        2,
        2.1,
        4.0,
        Color::srgb(1.0, 0.35, 0.08),
        8.0,
        Some(bigdog_kind),
    );
}

#[allow(clippy::too_many_arguments)]
fn lil_hunter_ai(
    commands: &mut Commands,
    catalog: &AssetCatalog,
    asset_server: &AssetServer,
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
    props: &Query<(Entity, &Prop, &Transform), (With<Prop>, Without<Enemy>)>,
) {
    let looped = def.name.contains("Loop");

    match boss.phase {
        BossPhase::Idle | BossPhase::Cooldown => {
            // Keep a skirmish range.
            let desired = if pos.distance(player_pos) < 150.0 {
                -dir
            } else {
                dir
            };

            vel.0 += desired * def.accel * 0.7 * dt;
            limit_velocity(
                vel,
                if boss.enraged {
                    if looped { 210.0 } else { 185.0 }
                } else if looped {
                    165.0
                } else {
                    145.0
                },
            );
            tf.translation += (vel.0 * dt).extend(0.0);
            resolve_prop_collision(&mut tf.translation, def.radius, props);

            if boss.attack_timer.just_finished() {
                lil_hunter_burst(
                    commands,
                    catalog,
                    asset_server,
                    owner,
                    pos,
                    player_pos,
                    player_vel,
                    boss.enraged,
                    looped,
                );
            }

            if boss.special_timer.just_finished() {
                boss.target = player_pos + player_vel * 0.35;
                boss.set_phase(BossPhase::Telegraph, if looped { 0.18 } else { 0.25 });
                vel.0 *= 0.2;
            }
        }

        BossPhase::Telegraph => {
            tf.scale =
                Vec3::splat(1.0 + (boss.phase_timer.elapsed_secs() * 20.0).sin().abs() * 0.12);
            vel.0 *= 0.70_f32.powf(dt * crate::app::NT_SIM_HZ as f32);

            if boss.phase_timer.just_finished() {
                boss.set_phase(
                    BossPhase::Jumping,
                    if boss.enraged {
                        if looped { 0.30 } else { 0.36 }
                    } else if looped {
                        0.36
                    } else {
                        0.44
                    },
                );
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
                let lil_kind = if looped {
                    EnemyKind::LilHunterLoop
                } else {
                    EnemyKind::LilHunter
                };
                commands.spawn((
                    GameCleanup,
                    LevelCleanup,
                    Explosion {
                        timer: Timer::from_seconds(0.02, TimerMode::Once),
                        radius: if boss.enraged { 95.0 } else { 75.0 },
                        damage: if boss.enraged { 6 } else { 4 },
                        team: Team::Enemy,
                        hits_player: true,
                        source: Some(DamageSource::enemy(owner, lil_kind)),
                    },
                    Transform::from_translation(land_pos.extend(20.0)),
                ));

                fire_ring_with_kind(
                    commands,
                    catalog,
                    asset_server,
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
                    Some(lil_kind),
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
                    catalog,
                    asset_server,
                    owner,
                    tf.translation.truncate(),
                    player_pos,
                    player_vel,
                    true,
                    def.name.contains("Loop"),
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
    catalog: &AssetCatalog,
    asset_server: &AssetServer,
    owner: Entity,
    pos: Vec2,
    player_pos: Vec2,
    player_vel: Vec2,
    enraged: bool,
    looped: bool,
) {
    let aim = lead_target(pos, player_pos, player_vel, 260.0);
    let base = aim.to_angle();
    let count = if enraged {
        if looped { 7 } else { 5 }
    } else if looped {
        5
    } else {
        3
    };

    for angle in fan_angles(base, count, 0.11) {
        let dir = dir_from_angle(angle);
        let kind = if looped {
            EnemyKind::LilHunterLoop
        } else {
            EnemyKind::LilHunter
        };
        fire_projectile(
            commands,
            catalog,
            asset_server,
            owner,
            pos + dir * 20.0,
            dir,
            Team::Enemy,
            if enraged {
                if looped { 340.0 } else { 280.0 }
            } else if looped {
                300.0
            } else {
                245.0
            },
            3,
            2.3,
            4.0,
            4.0,
            Color::srgb(0.6, 0.95, 1.0),
            7.0,
            Some(kind),
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn throne_ai(
    commands: &mut Commands,
    catalog: &AssetCatalog,
    asset_server: &AssetServer,
    trauma: &mut ResMut<Trauma>,
    throne_room: &ThroneRoomState,
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
            throne_cross_rings(
                commands,
                catalog,
                asset_server,
                owner,
                pos,
                boss.pattern_index,
                boss.enraged,
            );
        } else {
            throne_aimed_spread(
                commands,
                catalog,
                asset_server,
                owner,
                pos,
                player_pos,
                boss.enraged,
            );
        }
    }

    if boss.special_timer.just_finished() {
        boss.set_phase(BossPhase::Radial, 0.45);
        throne_radial_burst(
            commands,
            catalog,
            asset_server,
            owner,
            pos,
            boss.pattern_index,
            boss.enraged,
        );
        ScreenEffects::add_trauma(trauma, if boss.enraged { 0.34 } else { 0.22 });
    }

    if throne_room.player_on_carpet && boss.special_timer.just_finished() {
        let dir = Vec2::new(0.0, -1.0);
        spawn_enemy_beam(commands, pos + dir * 40.0, dir, 520.0, 28.0, 5, 0.35);
        ScreenEffects::add_trauma(trauma, 0.2);
    }
}

fn throne_aimed_spread(
    commands: &mut Commands,
    catalog: &AssetCatalog,
    asset_server: &AssetServer,
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
            catalog,
            asset_server,
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
            Some(EnemyKind::Throne),
        );
    }
}

fn throne_cross_rings(
    commands: &mut Commands,
    catalog: &AssetCatalog,
    asset_server: &AssetServer,
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
            catalog,
            asset_server,
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
            Some(EnemyKind::Throne),
        );
    }
}

fn throne_radial_burst(
    commands: &mut Commands,
    catalog: &AssetCatalog,
    asset_server: &AssetServer,
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
            catalog,
            asset_server,
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
            Some(EnemyKind::Throne),
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
                source: None,
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

// Throne II - circling orb boss (split orbs / laser orbs / static stars)

#[allow(clippy::too_many_arguments)]
fn throne_ii_ai(
    commands: &mut Commands,
    catalog: &AssetCatalog,
    asset_server: &AssetServer,
    trauma: &mut ResMut<Trauma>,
    owner: Entity,
    boss: &mut BossBrain,
    vel: &mut Velocity,
    tf: &mut Transform,
    def: EnemyDef,
    pos: Vec2,
    player_pos: Vec2,
    dir_to_player: Vec2,
    dt: f32,
    loop_count: u32,
) {
    // Circle the arena around home, reversing direction every few patterns.
    let reverse = (boss.pattern_index / 3) % 2 == 1;
    let ang_speed = if reverse { -1.15 } else { 1.15 };
    let angle = boss.phase_timer.elapsed_secs() * ang_speed + boss.pattern_index as f32 * 0.2;
    let target = crate::game::boss_patterns::orbit_point(boss.home, 120.0, angle);

    let desired = (target - pos).normalize_or_zero() * 0.85 + dir_to_player * 0.15;
    vel.0 += desired.normalize_or_zero() * def.accel * 0.22 * dt;
    limit_velocity(vel, def.speed.max(90.0));
    tf.translation += (vel.0 * dt).extend(0.0);

    // Enrage transition: faster cadence + a named phase for the pulse VFX.
    if boss.enraged
        && !matches!(
            boss.phase,
            BossPhase::Enraged | BossPhase::Radial | BossPhase::Beam
        )
    {
        boss.set_phase(BossPhase::Enraged, 0.35);
        boss.attack_timer = Timer::from_seconds(0.6, TimerMode::Repeating);
        ScreenEffects::add_trauma(trauma, 0.38);
    }

    match boss.phase {
        BossPhase::Idle | BossPhase::Cooldown | BossPhase::Enraged => {
            if boss.attack_timer.just_finished() {
                boss.pattern_index += 1;
                match boss.pattern_index % 3 {
                    0 => {
                        throne_ii_split_orbs(
                            commands,
                            catalog,
                            asset_server,
                            owner,
                            pos,
                            player_pos,
                            loop_count,
                            boss.enraged,
                        );
                    }
                    1 => {
                        throne_ii_laser_orbs(
                            commands,
                            catalog,
                            asset_server,
                            owner,
                            pos,
                            loop_count,
                            boss.enraged,
                        );
                    }
                    _ => {
                        boss.set_phase(BossPhase::Radial, if boss.enraged { 0.55 } else { 0.7 });
                        vel.0 *= 0.2;
                    }
                }
            }

            if boss.special_timer.just_finished() {
                throne_ii_split_orbs(
                    commands,
                    catalog,
                    asset_server,
                    owner,
                    pos,
                    player_pos,
                    loop_count,
                    true,
                );
                ScreenEffects::add_trauma(trauma, 0.18);
            }
        }

        BossPhase::Radial => {
            // Static star phase.
            vel.0 *= 0.85_f32.powf(dt * crate::app::NT_SIM_HZ as f32);
            if boss.phase_timer.just_finished() {
                throne_ii_star_burst(
                    commands,
                    catalog,
                    asset_server,
                    owner,
                    pos,
                    loop_count,
                    boss.enraged,
                );
                ScreenEffects::add_trauma(trauma, 0.22);
                boss.set_phase(BossPhase::Cooldown, 0.45);
            }
        }

        _ => boss.set_phase(BossPhase::Idle, 0.1),
    }
}

#[allow(clippy::too_many_arguments)]
fn throne_ii_split_orbs(
    commands: &mut Commands,
    catalog: &AssetCatalog,
    asset_server: &AssetServer,
    owner: Entity,
    pos: Vec2,
    player_pos: Vec2,
    loop_count: u32,
    enraged: bool,
) {
    let count = 3 + loop_count as usize + usize::from(enraged);
    let aim = (player_pos - pos).normalize_or_zero();
    let base = aim.to_angle();

    for angle in fan_angles(base, count, 0.18) {
        let dir = dir_from_angle(angle);
        commands.spawn((
            GameCleanup,
            LevelCleanup,
            Team::Enemy,
            Projectile {
                damage: 12,
                life: Timer::from_seconds(0.55, TimerMode::Once),
                radius: 10.0,
                knockback: 80.0,
                explosive: false,
                source: Some(DamageSource::enemy(owner, EnemyKind::ThroneII)),
            },
            Velocity(dir * 140.0),
            SplitOnDeath(crate::game::content::SplitDef {
                pellets: (8u8).saturating_add(loop_count as u8).min(14),
                spread: std::f32::consts::TAU,
                speed: 220.0,
                damage: 5,
                lifetime: 1.6,
                radius: 4.0,
                knockback: 40.0,
                color: Color::srgb(0.35, 1.0, 0.5),
                size: Vec2::splat(7.0),
            }),
            crate::game::projectile_art::generic_enemy_bullet_sprite(asset_server, catalog, None),
            Transform::from_translation((pos + dir * 28.0).extend(16.0)),
        ));
    }
}

#[allow(clippy::too_many_arguments)]
fn throne_ii_laser_orbs(
    commands: &mut Commands,
    catalog: &AssetCatalog,
    asset_server: &AssetServer,
    owner: Entity,
    pos: Vec2,
    loop_count: u32,
    enraged: bool,
) {
    let count = 5 + loop_count as usize + usize::from(enraged);
    for i in 0..count {
        let angle = i as f32 * (std::f32::consts::TAU / count as f32);
        let dir = dir_from_angle(angle);

        commands.spawn((
            GameCleanup,
            LevelCleanup,
            Team::Enemy,
            Projectile {
                damage: 12,
                life: Timer::from_seconds(0.35, TimerMode::Once),
                radius: 9.0,
                knockback: 60.0,
                explosive: false,
                source: Some(DamageSource::enemy(owner, EnemyKind::ThroneII)),
            },
            Velocity(dir * 120.0),
            crate::game::projectile_art::generic_enemy_bullet_sprite(asset_server, catalog, None),
            Transform::from_translation((pos + dir * 24.0).extend(16.0)),
        ));

        // Bright orbs fire a random-direction beam from their travel lane.
        let beam_dir = dir_from_angle(angle + 1.7);
        spawn_enemy_beam(
            commands,
            pos + dir * 80.0 + beam_dir * 260.0,
            beam_dir,
            if enraged { 620.0 } else { 520.0 },
            16.0,
            2,
            if enraged { 0.55 } else { 0.45 },
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn throne_ii_star_burst(
    commands: &mut Commands,
    catalog: &AssetCatalog,
    asset_server: &AssetServer,
    owner: Entity,
    pos: Vec2,
    loop_count: u32,
    enraged: bool,
) {
    let points = 10 + loop_count as usize * 2 + usize::from(enraged);
    for angle in crate::game::boss_patterns::star_angles(points, 0.15) {
        let dir = dir_from_angle(angle);
        fire_projectile(
            commands,
            catalog,
            asset_server,
            owner,
            pos + dir * 22.0,
            dir,
            Team::Enemy,
            260.0,
            5,
            1.8,
            4.0,
            50.0,
            Color::srgb(0.4, 1.0, 0.55),
            7.0,
            Some(EnemyKind::ThroneII),
        );
    }
}

// Hyper Crystal - contact flunky core with orbiting laser crystals

#[allow(clippy::too_many_arguments)]
fn hyper_ai(
    commands: &mut Commands,
    catalog: &AssetCatalog,
    asset_server: &AssetServer,
    trauma: &mut ResMut<Trauma>,
    owner: Entity,
    boss: &mut BossBrain,
    vel: &mut Velocity,
    tf: &mut Transform,
    def: EnemyDef,
    pos: Vec2,
    player_pos: Vec2,
    dt: f32,
    loop_count: u32,
) {
    // Slow drift; the huge touch damage is the threat.
    let desired = (boss.home - pos) * 0.4 + (player_pos - pos) * 0.1;
    if desired.length_squared() > 1.0 {
        vel.0 += desired.normalize_or_zero() * def.accel * 0.12 * dt;
    }
    limit_velocity(vel, def.speed.max(35.0));
    tf.translation += (vel.0 * dt).extend(0.0);

    // Rearm orbit ring periodically.
    if boss.attack_timer.just_finished() {
        boss.pattern_index += 1;
        hyper_ensure_orbit(commands, owner, pos, loop_count, boss.enraged);
    }

    // Search phase when the player keeps distance.
    if boss.special_timer.just_finished() && pos.distance(player_pos) > 220.0 {
        hyper_search_detonate(
            commands,
            trauma,
            owner,
            player_pos,
            loop_count,
            boss.enraged,
        );
        boss.set_phase(BossPhase::Cooldown, 0.8);
    }
}

fn hyper_search_detonate(
    commands: &mut Commands,
    trauma: &mut ResMut<Trauma>,
    owner: Entity,
    player_pos: Vec2,
    loop_count: u32,
    enraged: bool,
) {
    ScreenEffects::add_trauma(trauma, 0.3);
    let lasers = 7 + loop_count as usize * 2 + usize::from(enraged);

    // Explosion at the player's cover.
    commands.spawn((
        GameCleanup,
        LevelCleanup,
        Explosion {
            timer: Timer::from_seconds(0.03, TimerMode::Once),
            radius: 90.0,
            damage: 6,
            team: Team::Enemy,
            hits_player: true,
            source: Some(DamageSource::enemy(owner, EnemyKind::Hyper)),
        },
        Transform::from_translation(player_pos.extend(20.0)),
    ));

    for angle in ring_angles(lasers, 0.0) {
        let dir = dir_from_angle(angle);
        spawn_enemy_beam(
            commands,
            player_pos + dir * 210.0,
            dir,
            420.0,
            12.0,
            2,
            0.28,
        );
    }
}

fn hyper_ensure_orbit(
    commands: &mut Commands,
    owner: Entity,
    pos: Vec2,
    loop_count: u32,
    enraged: bool,
) {
    let n = hyper_orbit_count(loop_count) + usize::from(enraged);

    for i in 0..n {
        let angle = i as f32 / n as f32 * std::f32::consts::TAU;
        let radius = 70.0 + (i % 3) as f32 * 12.0;

        commands.spawn((
            GameCleanup,
            LevelCleanup,
            Team::Enemy,
            Health {
                hp: 6,
                max: 6,
                invuln: short_ready_timer(),
            },
            Hitbox { radius: 9.0 },
            Velocity(Vec2::ZERO),
            Sprite {
                color: Color::srgb(1.0, 0.28, 0.38),
                custom_size: Some(Vec2::splat(14.0)),
                ..default()
            },
            Transform::from_translation((pos + dir_from_angle(angle) * radius).extend(14.0)),
            HyperOrbitCrystal {
                owner,
                angle,
                radius,
                angular_speed: 1.15 + (i as f32) * 0.04,
                fire_timer: Timer::from_seconds(1.4 + (i % 3) as f32 * 0.35, TimerMode::Repeating),
            },
        ));
    }
}

/// Orbit positioning around the core plus periodic beam fire; crystals free
/// themselves (slow drift) when their core dies.
pub fn tick_hyper_orbit_crystals(
    time: Res<Time<Fixed>>,
    mut commands: Commands,
    mut q: Query<(
        Entity,
        &mut Transform,
        &mut Velocity,
        &mut HyperOrbitCrystal,
    )>,
    cores: Query<&Transform, (With<Enemy>, Without<HyperOrbitCrystal>)>,
) {
    let dt = time.delta_secs();

    for (_entity, mut tf, mut vel, mut crystal) in q.iter_mut() {
        let Ok(core_tf) = cores.get(crystal.owner) else {
            // Core dead: become a drifting free crystal.
            vel.0 *= 0.9;
            continue;
        };

        crystal.angle += crystal.angular_speed * dt;
        let center = core_tf.translation.truncate();
        tf.translation =
            (center + dir_from_angle(crystal.angle) * crystal.radius).extend(tf.translation.z);
        vel.0 = Vec2::ZERO;

        crystal.fire_timer.tick(time.delta());
        if !crystal.fire_timer.just_finished() {
            continue;
        }

        let origin = tf.translation.truncate();
        let aim = dir_from_angle(crystal.angle + std::f32::consts::FRAC_PI_2);
        spawn_enemy_beam(
            &mut commands,
            origin + aim * 210.0,
            aim,
            420.0,
            12.0,
            2,
            0.28,
        );
    }
}

// Mom - loop Sewers boss: toxic rings, hazard clouds, and Frog Egg broods

#[allow(clippy::too_many_arguments)]
fn mom_ai(
    commands: &mut Commands,
    catalog: &AssetCatalog,
    asset_server: &AssetServer,
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
    props: &Query<(Entity, &Prop, &Transform), (With<Prop>, Without<Enemy>)>,
) {
    // Drift and keep mid range.
    let desired = if pos.distance(player_pos) < 120.0 {
        -dir
    } else {
        dir
    };
    vel.0 += desired * def.accel * 0.5 * dt;
    limit_velocity(vel, def.speed.max(50.0));
    tf.translation += (vel.0 * dt).extend(0.0);
    resolve_prop_collision(&mut tf.translation, def.radius, props);

    if boss.attack_timer.just_finished() {
        // Toxic ring around Mom.
        fire_ring_with_kind(
            commands,
            catalog,
            asset_server,
            owner,
            pos,
            Team::Enemy,
            10 + usize::from(boss.enraged) * 4,
            boss.pattern_index as f32 * 0.11,
            90.0,
            2,
            2.5,
            5.0,
            Color::srgb(0.4, 1.0, 0.35),
            9.0,
            Some(EnemyKind::Mom),
        );
        boss.pattern_index += 1;
        ScreenEffects::add_trauma(trauma, 0.12);
    }

    if boss.special_timer.just_finished() {
        boss.set_phase(BossPhase::Spawning, 0.4);
        // Lay three eggs in a triangle around Mom.
        for i in 0..3 {
            let a = i as f32 * std::f32::consts::TAU / 3.0 + boss.pattern_index as f32 * 0.4;
            commands.spawn(PendingEnemySpawn {
                kind: EnemyKind::FrogEgg,
                pos: pos + Vec2::new(a.cos(), a.sin()) * 48.0,
                difficulty: difficulty_for_loop(boss.enraged),
            });
        }
        ScreenEffects::add_trauma(trauma, 0.18);
    }

    if matches!(boss.phase, BossPhase::Spawning) && boss.phase_timer.just_finished() {
        boss.set_phase(BossPhase::Idle, 0.1);
    }
}

/// Frog Queen - Pizza Sewers secret boss (upstream FrogQueen / Ball Mama).
/// Alternates aimed MomProjectile volleys with FrogEgg clusters; keeps an
/// egg budget of 8 on screen (upstream Exploder + SuperFrog*2 < 8 check).
fn frog_queen_ai(
    commands: &mut Commands,
    catalog: &AssetCatalog,
    asset_server: &AssetServer,
    trauma: &mut ResMut<Trauma>,
    children: &Query<
        (Entity, &'static Enemy, &'static Transform),
        (With<Enemy>, Without<BossBrain>),
    >,
    owner: Entity,
    boss: &mut BossBrain,
    vel: &mut Velocity,
    tf: &mut Transform,
    def: EnemyDef,
    pos: Vec2,
    player_pos: Vec2,
    dir: Vec2,
    dt: f32,
    props: &Query<(Entity, &Prop, &Transform), (With<Prop>, Without<Enemy>)>,
) {
    // Relentless chase at loop-scaled speed.
    vel.0 += dir * def.accel * 0.6 * dt;
    limit_velocity(vel, def.speed);
    tf.translation += (vel.0 * dt).extend(0.0);
    resolve_prop_collision(&mut tf.translation, def.radius, props);

    // Egg budget.
    let egg_count = children
        .iter()
        .filter(|(_, e, _)| e.kind == EnemyKind::FrogEgg)
        .count();

    if boss.attack_timer.just_finished() {
        // Aimed MomProjectile: speed 4/frame = 120px/s, orandom(30) jitter.
        let jitter = rand::rng().random_range(-0.26f32..0.26);
        let aim = Vec2::new(
            dir.x * jitter.cos() - dir.y * jitter.sin(),
            dir.x * jitter.sin() + dir.y * jitter.cos(),
        );
        fire_fan_with_kind(
            commands,
            catalog,
            asset_server,
            owner,
            pos,
            aim.normalize_or_zero(),
            Team::Enemy,
            1,
            0.0,
            def.projectile_speed,
            def.projectile_damage,
            def.projectile_lifetime,
            def.projectile_radius,
            def.projectile_color,
            def.projectile_size,
            Some(EnemyKind::FrogQueen),
        );
        ScreenEffects::add_trauma(trauma, 0.12);
    }

    if boss.special_timer.just_finished() && egg_count < 8 {
        boss.set_phase(BossPhase::Spawning, 0.35);
        // Cluster of two eggs flanking the queen.
        for side in [-1.0f32, 1.0] {
            let offset = Vec2::new(side * 40.0, -20.0);
            commands.spawn(PendingEnemySpawn {
                kind: EnemyKind::FrogEgg,
                pos: pos + offset,
                difficulty: difficulty_for_loop(boss.enraged),
            });
        }
        ScreenEffects::add_trauma(trauma, 0.15);
    }

    if matches!(boss.phase, BossPhase::Spawning) && boss.phase_timer.just_finished() {
        boss.set_phase(BossPhase::Idle, 0.1);
    }
}

// Technomancer - loop Labs boss: stationary summon engine

fn technomancer_ai(
    commands: &mut Commands,
    _catalog: &AssetCatalog,
    _asset_server: &AssetServer,
    trauma: &mut ResMut<Trauma>,
    _owner: Entity,
    boss: &mut BossBrain,
    vel: &mut Velocity,
    def: EnemyDef,
    pos: Vec2,
    _player_pos: Vec2,
) {
    let _ = def;
    // Stationary revive engine: periodically raises reinforcements.
    if boss.attack_timer.just_finished() {
        boss.pattern_index += 1;
        let kind = if boss.pattern_index % 2 == 0 {
            EnemyKind::Necromancer
        } else {
            EnemyKind::Freak
        };
        let ang = boss.pattern_index as f32 * 1.7;
        commands.spawn(PendingEnemySpawn {
            kind,
            pos: pos + Vec2::new(ang.cos(), ang.sin()) * 90.0,
            difficulty: difficulty_for_loop(boss.enraged),
        });
        ScreenEffects::add_trauma(trauma, 0.1);
    }

    if boss.special_timer.just_finished() {
        // Burst of Freaks in a ring; wider when enraged.
        let n = if boss.enraged { 4 } else { 2 };
        for i in 0..n {
            let a = i as f32 * (std::f32::consts::TAU / n as f32);
            commands.spawn(PendingEnemySpawn {
                kind: EnemyKind::Freak,
                pos: pos + Vec2::new(a.cos(), a.sin()) * 110.0,
                difficulty: 1.15,
            });
        }
        ScreenEffects::add_trauma(trauma, 0.22);
    }

    vel.0 = Vec2::ZERO;
}

// Captain - IDPD HQ boss: fan volleys, wall charges, and teleports

#[allow(clippy::too_many_arguments)]
fn captain_ai(
    commands: &mut Commands,
    catalog: &AssetCatalog,
    asset_server: &AssetServer,
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
    props: &Query<(Entity, &Prop, &Transform), (With<Prop>, Without<Enemy>)>,
    walls: &Query<(Entity, &WallCell, &Transform), With<WallTile>>,
) {
    match boss.phase {
        BossPhase::Idle | BossPhase::Cooldown => {
            let desired = if pos.distance(player_pos) < 100.0 {
                -dir
            } else {
                dir
            };
            vel.0 += desired * def.accel * 0.65 * dt;
            limit_velocity(vel, def.speed);
            tf.translation += (vel.0 * dt).extend(0.0);
            resolve_prop_collision(&mut tf.translation, def.radius, props);

            if boss.attack_timer.just_finished() {
                fire_fan_with_kind(
                    commands,
                    catalog,
                    asset_server,
                    owner,
                    pos,
                    dir,
                    Team::Enemy,
                    def.bullets_per_shot.max(5),
                    def.fan_spread,
                    def.projectile_speed,
                    def.projectile_damage,
                    def.projectile_lifetime,
                    def.projectile_radius,
                    def.projectile_color,
                    def.projectile_size,
                    Some(EnemyKind::Captain),
                );
            }

            if boss.special_timer.just_finished() && pos.distance(player_pos) < 560.0 {
                boss.target = player_pos;
                boss.set_phase(BossPhase::Telegraph, 0.22);
                vel.0 *= 0.25;
            }
        }

        BossPhase::Telegraph => {
            vel.0 *= 0.8_f32.powf(dt * crate::app::NT_SIM_HZ as f32);
            tf.scale =
                Vec3::splat(1.0 + (boss.phase_timer.elapsed_secs() * 20.0).sin().abs() * 0.08);

            if boss.phase_timer.just_finished() {
                tf.scale = Vec3::ONE;
                if boss.pattern_index % 2 == 0 {
                    // Charge through the player's last position.
                    boss.set_phase(BossPhase::Charging, 0.35);
                    vel.0 = (boss.target - pos).normalize_or_zero() * 720.0;
                    ScreenEffects::add_trauma(trauma, 0.14);
                } else {
                    // Teleport past the player.
                    boss.set_phase(BossPhase::Teleport, 0.05);
                    tf.translation = (player_pos + dir * 90.0).extend(tf.translation.z);
                    ScreenEffects::add_trauma(trauma, 0.2);
                    boss.set_phase(BossPhase::Cooldown, 0.4);
                }
                boss.pattern_index += 1;
            }
        }

        BossPhase::Charging => {
            let before = tf.translation.truncate();
            tf.translation += (vel.0 * dt).extend(0.0);
            let after = tf.translation.truncate();
            crate::game::walls::queue_wall_breaks_along_segment(
                commands,
                walls,
                before,
                after,
                def.radius * 0.9,
            );

            if boss.phase_timer.just_finished() {
                vel.0 *= 0.15;
                boss.set_phase(BossPhase::Cooldown, 0.45);
                fire_ring_with_kind(
                    commands,
                    catalog,
                    asset_server,
                    owner,
                    tf.translation.truncate(),
                    Team::Enemy,
                    12,
                    boss.pattern_index as f32 * 0.17,
                    140.0,
                    3,
                    2.0,
                    4.0,
                    Color::srgb(0.4, 0.7, 1.0),
                    7.0,
                    Some(EnemyKind::Captain),
                );
            }
        }

        _ => {
            boss.set_phase(BossPhase::Idle, 0.1);
            tf.scale = Vec3::ONE;
        }
    }
}

// Old Guardian - Crown Vault boss: slow advance, aimed fans + radial bursts

#[allow(clippy::too_many_arguments)]
fn old_guardian_ai(
    commands: &mut Commands,
    catalog: &AssetCatalog,
    asset_server: &AssetServer,
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
    props: &Query<(Entity, &Prop, &Transform), (With<Prop>, Without<Enemy>)>,
) {
    // Slow advance toward the intruder.
    let desired = if pos.distance(player_pos) < 90.0 {
        -dir
    } else {
        dir
    };
    vel.0 += desired * def.accel * 0.55 * dt;
    limit_velocity(vel, def.speed);
    tf.translation += (vel.0 * dt).extend(0.0);
    resolve_prop_collision(&mut tf.translation, def.radius, props);

    if boss.attack_timer.just_finished() {
        fire_fan_with_kind(
            commands,
            catalog,
            asset_server,
            owner,
            pos,
            dir,
            Team::Enemy,
            def.bullets_per_shot.max(4),
            def.fan_spread,
            def.projectile_speed,
            def.projectile_damage,
            def.projectile_lifetime,
            def.projectile_radius,
            def.projectile_color,
            def.projectile_size,
            Some(EnemyKind::OldGuardian),
        );
    }

    if boss.special_timer.just_finished() {
        fire_ring_with_kind(
            commands,
            catalog,
            asset_server,
            owner,
            pos,
            Team::Enemy,
            10 + usize::from(boss.enraged) * 4,
            boss.pattern_index as f32 * 0.19,
            120.0,
            3,
            2.2,
            4.0,
            Color::srgb(0.95, 0.9, 0.55),
            8.0,
            Some(EnemyKind::OldGuardian),
        );
        boss.pattern_index += 1;
        ScreenEffects::add_trauma(trauma, 0.16);
    }
}

/// Spawn difficulty used by bosses that call reinforcements mid-fight.
fn difficulty_for_loop(enraged: bool) -> f32 {
    1.0 + if enraged { 0.25 } else { 0.0 }
}

// Shared enemy beam helper

#[allow(clippy::too_many_arguments)]
fn spawn_enemy_beam(
    commands: &mut Commands,
    center: Vec2,
    dir: Vec2,
    length: f32,
    width: f32,
    damage: i32,
    duration: f32,
) {
    let angle = dir.to_angle();
    commands.spawn((
        GameCleanup,
        LevelCleanup,
        Beam {
            team: Team::Enemy,
            dir,
            length,
            width,
            damage,
            knockback: 40.0,
            timer: Timer::from_seconds(duration, TimerMode::Once),
            tick: Timer::from_seconds(0.06, TimerMode::Repeating),
            source: None,
        },
        Sprite {
            color: Color::srgba(0.55, 1.0, 0.6, 0.65),
            custom_size: Some(Vec2::new(length, width)),
            ..default()
        },
        Transform::from_translation(center.extend(17.0))
            .with_rotation(Quat::from_rotation_z(angle)),
    ));
}

fn short_ready_timer() -> Timer {
    let mut t = Timer::from_seconds(0.01, TimerMode::Once);
    t.finish();
    t
}

#[allow(clippy::too_many_arguments)]
fn fire_fan(
    commands: &mut Commands,
    catalog: &AssetCatalog,
    asset_server: &AssetServer,
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
    // Back-compat wrapper: defaults to Bandit when caller doesn't specify a kind.
    fire_fan_with_kind(
        commands,
        catalog,
        asset_server,
        owner,
        pos,
        dir,
        team,
        count,
        spread,
        speed,
        damage,
        lifetime,
        radius,
        color,
        size,
        None,
    );
}

#[allow(clippy::too_many_arguments)]
fn fire_fan_with_kind(
    commands: &mut Commands,
    catalog: &AssetCatalog,
    asset_server: &AssetServer,
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
    _color: Color,
    _size: f32,
    enemy_kind: Option<EnemyKind>,
) {
    let base = dir.to_angle();
    for angle in fan_angles(base, count, spread) {
        let shot_dir = dir_from_angle(angle);
        fire_projectile(
            commands,
            catalog,
            asset_server,
            owner,
            pos + shot_dir * 20.0,
            shot_dir,
            team,
            speed,
            damage,
            lifetime,
            radius,
            120.0,
            _color,
            _size,
            enemy_kind,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn fire_ring(
    commands: &mut Commands,
    catalog: &AssetCatalog,
    asset_server: &AssetServer,
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
    fire_ring_with_kind(
        commands,
        catalog,
        asset_server,
        owner,
        pos,
        team,
        count,
        phase,
        speed,
        damage,
        lifetime,
        radius,
        color,
        size,
        None,
    );
}

#[allow(clippy::too_many_arguments)]
fn fire_ring_with_kind(
    commands: &mut Commands,
    catalog: &AssetCatalog,
    asset_server: &AssetServer,
    owner: Entity,
    pos: Vec2,
    team: Team,
    count: usize,
    phase: f32,
    speed: f32,
    damage: i32,
    lifetime: f32,
    radius: f32,
    _color: Color,
    _size: f32,
    enemy_kind: Option<EnemyKind>,
) {
    for angle in ring_angles(count, phase) {
        let dir = dir_from_angle(angle);
        fire_projectile(
            commands,
            catalog,
            asset_server,
            owner,
            pos + dir * 22.0,
            dir,
            team,
            speed,
            damage,
            lifetime,
            radius,
            100.0,
            _color,
            _size,
            enemy_kind,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn fire_projectile(
    commands: &mut Commands,
    catalog: &AssetCatalog,
    asset_server: &AssetServer,
    owner: Entity,
    pos: Vec2,
    dir: Vec2,
    team: Team,
    speed: f32,
    damage: i32,
    lifetime: f32,
    radius: f32,
    knockback: f32,
    _color: Color,
    _size: f32,
    enemy_kind: Option<EnemyKind>,
) {
    let angle = dir.y.atan2(dir.x);
    let source = if team == Team::Enemy {
        let kind = enemy_kind.unwrap_or(EnemyKind::Bandit);
        Some(DamageSource {
            owner,
            team,
            hit_id: HitId::from_enemy_kind(kind),
            enemy_kind: Some(kind),
        })
    } else {
        Some(DamageSource {
            owner,
            team,
            hit_id: HitId::Weapon(WeaponId::NONE),
            enemy_kind: None,
        })
    };
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
            source,
        },
        Velocity(dir * speed),
        {
            let sprite = if let Some(kind) = enemy_kind {
                if matches!(
                    kind,
                    EnemyKind::BigDog
                        | EnemyKind::BigDogLoop
                        | EnemyKind::Jock
                        | EnemyKind::SnowTank
                        | EnemyKind::GoldSnowtank
                ) {
                    crate::game::projectile_art::sprite_from_projectile_path(
                        asset_server,
                        catalog,
                        &[
                            "images/sprBigDogMissile.png",
                            "images/sprJockRocket.png",
                            "images/sprRocket.png",
                        ],
                        None,
                    )
                } else if matches!(
                    kind,
                    EnemyKind::Guardian
                        | EnemyKind::ExploGuardian
                        | EnemyKind::DogGuardian
                        | EnemyKind::Crystal
                        | EnemyKind::LaserCrystal
                        | EnemyKind::Turtle
                        | EnemyKind::OldGuardian
                        | EnemyKind::Throne
                        | EnemyKind::ThroneII
                ) {
                    crate::game::projectile_art::sprite_from_projectile_path(
                        asset_server,
                        catalog,
                        &[
                            "images/sprGuardianBullet.png",
                            "images/sprHorrorBullet.png",
                            "images/sprEnemyBullet1.png",
                        ],
                        None,
                    )
                } else {
                    crate::game::projectile_art::enemy_projectile_sprite(
                        asset_server,
                        catalog,
                        kind,
                        None,
                    )
                }
            } else {
                crate::game::projectile_art::generic_enemy_bullet_sprite(
                    asset_server,
                    catalog,
                    None,
                )
            };
            sprite
        },
        Transform::from_translation(pos.extend(15.0)).with_rotation(Quat::from_rotation_z(angle)),
    ));
}

fn limit_velocity(vel: &mut Velocity, max: f32) {
    if vel.0.length() > max {
        vel.0 = vel.0.normalize_or_zero() * max;
    }
}
