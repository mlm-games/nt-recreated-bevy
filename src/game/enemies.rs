//! Enemy spawning (per-floor packs + bosses) and AI behaviors.
//! AI mirrors the GPL Nuclear-Throne-Mobile rebuild reference.

use bevy::prelude::*;
use rand::RngExt;

use crate::game::components::*;
use crate::game::content::*;
use crate::game::world::*;
use game_utils_bevy::juice::Juice;
use game_utils_bevy::screen_effects::ScreenEffects;
use game_utils_bevy::vfx::VfxSpawner;

pub fn spawn_enemy_at(
    commands: &mut Commands,
    catalog: &AssetCatalog,
    asset_server: &AssetServer,
    kind: EnemyKind,
    pos: Vec2,
    difficulty: f32,
    scarier_face: bool,
    heavy_heart: bool,
) {
    spawn_enemy(
        commands,
        catalog,
        asset_server,
        kind,
        pos,
        difficulty,
        scarier_face,
        heavy_heart,
    );
}

/// Applies deferred spawns queued by systems without asset handles
/// (campfire Throne II, future wave directors).
pub fn flush_pending_enemy_spawns(
    mut commands: Commands,
    catalog: Res<AssetCatalog>,
    asset_server: Res<AssetServer>,
    pending: Query<(Entity, &PendingEnemySpawn)>,
) {
    for (entity, spawn) in pending.iter() {
        spawn_enemy_at(
            &mut commands,
            &catalog,
            &asset_server,
            spawn.kind,
            spawn.pos,
            spawn.difficulty,
            false,
            false,
        );
        commands.entity(entity).despawn();
    }
}

pub fn random_spawn_pos(rng: &mut impl RngExt, min_from_center: f32) -> Vec2 {
    loop {
        let x = rng.random_range(-ARENA_W / 2.0 + 80.0..ARENA_W / 2.0 - 80.0);
        let y = rng.random_range(-ARENA_H / 2.0 + 80.0..ARENA_H / 2.0 - 80.0);
        let p = Vec2::new(x, y);
        if p.length() >= min_from_center {
            return p;
        }
    }
}

pub fn spawn_enemy(
    commands: &mut Commands,
    catalog: &AssetCatalog,
    asset_server: &AssetServer,
    kind: EnemyKind,
    pos: Vec2,
    difficulty: f32,
    scarier_face: bool,
    heavy_heart: bool,
) {
    let def = enemy_def(kind);
    let hp = (def.hp as f32 * difficulty).round().max(1.0) as i32;
    let hp = if scarier_face {
        (hp as f32 * 0.8).floor() as i32
    } else {
        hp
    };
    let speed = def.speed * (0.9 + 0.02 * difficulty);
    let weapon_chance = if heavy_heart {
        def.weapon_chance + 9
    } else {
        def.weapon_chance
    };

    let (sprite, strip) = crate::game::anim::sprite_anim(catalog, asset_server, def.sprite);
    let hurt = crate::game::anim::derive_hurt_path(def.sprite);
    let walk = crate::game::anim::derive_walk_path(def.sprite);
    let mut ec = commands.spawn((
        GameCleanup,
        LevelCleanup,
        Enemy {
            kind,
            score: def.score,
            touch_damage: def.touch_damage,
            rad_drop: def.rad_drop,
            drop_chance: def.drop_chance,
            weapon_chance,
        },
        EnemySprites {
            idle: def.sprite,
            walk,
            hurt,
        },
        EnemyBrain {
            speed,
            accel: def.accel,
            preferred_range: def.preferred_range,
            shoot_range: def.shoot_range,
            attack: Timer::from_seconds(
                def.attack_cooldown * rand::rng().random_range(0.5..1.5),
                TimerMode::Once,
            ),
            burst_left: 0,
            burst_timer: ready_timer(),
            dash: 0.0,
            strafe_dir: if rand::rng().random_bool(0.5) {
                1.0
            } else {
                -1.0
            },
            strafe_timer: Timer::from_seconds(rand::rng().random_range(0.8..1.6), TimerMode::Once),
            melee: ready_timer(),
        },
        Health {
            hp,
            max: hp,
            invuln: ready_timer(),
        },
        Team::Enemy,
        Hitbox { radius: def.radius },
        Velocity(Vec2::ZERO),
        sprite,
        Transform::from_translation(pos.extend(10.0)),
    ));
    if let Some(strip) = strip {
        ec.insert(strip);
    }

    if def.boss {
        ec.insert(BossBrain::new(kind, pos));
    }

    match kind {
        EnemyKind::IdpdVan => {
            ec.insert((IdpdVanBrain::default(), IdpdShieldUnit));
        }
        EnemyKind::IdpdShield => {
            ec.insert(IdpdShieldUnit);
        }
        _ => {}
    }

    let e = ec.id();

    Juice::pop_in(commands, e, 0.18);
}

fn ready_timer() -> Timer {
    let mut t = Timer::from_seconds(0.01, TimerMode::Once);
    t.finish();
    t
}

pub fn enemy_ai(
    time: Res<Time<Fixed>>,
    mut commands: Commands,
    mut _trauma: ResMut<game_utils_bevy::screen_effects::Trauma>,
    euphoria: Res<Euphoria>,
    mask: Res<FloorMask>,
    player_q: Query<(&Transform, &Player), (With<Player>, Without<Enemy>)>,
    mut enemies: Query<
        (
            &Enemy,
            &mut EnemyBrain,
            &mut Velocity,
            &mut Transform,
            &mut Sprite,
            Option<&BossBrain>,
        ),
        (With<Enemy>, Without<Prop>),
    >,
    props: Query<(Entity, &Prop, &Transform), With<Prop>>,
    corpses: Query<(Entity, &Corpse, &Transform), (With<Corpse>, Without<Enemy>)>,
) {
    let Ok((player_tf, player)) = player_q.single() else {
        return;
    };

    let player_pos = player_tf.translation.truncate();
    let dt = time.delta_secs();
    let mut rng = rand::rng();
    // Euphoria mutation or Eyes' Projectile Style ultra slow enemy bullets.
    let euphoria = euphoria.0 || player.euphoria;

    // Pairwise separation to avoid enemy stacking.
    let positions: Vec<Vec2> = enemies
        .iter()
        .map(|(_, _, _, tf, _, _)| tf.translation.truncate())
        .collect();

    for (enemy, mut brain, mut vel, mut tf, mut sprite, boss) in &mut enemies {
        let pos = tf.translation.truncate();
        let to_player = player_pos - pos;
        let dist = to_player.length();
        let dir = to_player.normalize_or_zero();

        let def = enemy_def(enemy.kind);

        // Bosses are handled by `boss_ai`; keeping them in the generic
        // ranged/chase loop double-fires and fights their bespoke phases.
        if boss.is_some() {
            continue;
        }

        // Vans are stationary deployment points, not chasers.
        if enemy.kind == EnemyKind::IdpdVan {
            vel.0 = Vec2::ZERO;
            continue;
        }

        // Emplacements (turrets / crystals) hold position but still fire.
        let emplacement = matches!(
            enemy.kind,
            EnemyKind::Turret
                | EnemyKind::Crystal
                | EnemyKind::LaserCrystal
                | EnemyKind::LightningCrystal
        );

        // Melee contact cooldown (reference: 30 frames between hits).
        brain.melee.tick(time.delta());

        // Assassin / Spider / Melee Bandit: short leap toward the player when in mid range.
        let was_dashing = brain.dash > 0.0;
        if matches!(
            enemy.kind,
            EnemyKind::Assassin | EnemyKind::Spider | EnemyKind::MeleeBandit
        ) && !was_dashing
            && dist < 110.0
            && dist > 36.0
            && brain.melee.is_finished()
        {
            brain.dash = 0.22;
            brain.melee = Timer::from_seconds(1.4, TimerMode::Once);
            vel.0 = dir * 620.0;
        }
        // Rhino Freak / Dog Guardian: heavy charge from further out.
        if matches!(enemy.kind, EnemyKind::RhinoFreak | EnemyKind::DogGuardian)
            && !was_dashing
            && dist < 220.0
            && dist > 40.0
            && brain.melee.is_finished()
        {
            brain.dash = 0.42;
            brain.melee = Timer::from_seconds(1.6, TimerMode::Once);
            vel.0 = dir * 700.0;
        }
        // Raven: nervous hop-repositioning between bursts (flight feel).
        if enemy.kind == EnemyKind::Raven && !was_dashing && brain.melee.is_finished() {
            brain.dash = 0.2;
            brain.melee = Timer::from_seconds(rng.random_range(0.9..1.8), TimerMode::Once);
            let side = Vec2::new(-dir.y, dir.x) * brain.strafe_dir;
            vel.0 = (dir * -0.35 + side).normalize() * 420.0;
        }
        // Guardian: short teleport blink toward its preferred range band.
        if enemy.kind == EnemyKind::Guardian
            && brain.melee.is_finished()
            && dist < 480.0
            && rng.random_bool(0.012)
        {
            let want = def.preferred_range.max(140.0);
            let jump_dir = if dist > want { dir } else { -dir };
            let cand = pos + jump_dir * rng.random_range(70.0..130.0);
            if mask.is_walkable(cand) {
                tf.translation = cand.extend(tf.translation.z);
                vel.0 = Vec2::ZERO;
                VfxSpawner::spawn_burst(
                    &mut commands,
                    pos,
                    8,
                    Color::srgb(0.3, 1.0, 0.45),
                    (40.0, 120.0),
                );
            }
            brain.melee = Timer::from_seconds(2.2, TimerMode::Once);
        }
        // Palace Guardian: shield-bash dash when close.
        if enemy.kind == EnemyKind::PalaceGuardian
            && !was_dashing
            && dist < 80.0
            && dist > 24.0
            && brain.melee.is_finished()
        {
            brain.dash = 0.18;
            brain.melee = Timer::from_seconds(0.9, TimerMode::Once);
            vel.0 = dir * 540.0;
        }
        if brain.dash > 0.0 {
            brain.dash = (brain.dash - dt).max(0.0);
        }

        let dashing = brain.dash > 0.0;
        let speed = if dashing { 600.0 } else { brain.speed };

        let desired = if dashing {
            dir
        } else if enemy.kind == EnemyKind::IdpdShield {
            // Shields advance frontally toward the player.
            dir * 0.85
        } else if brain.preferred_range > 0.0 && dist < brain.preferred_range {
            -dir
        } else {
            dir
        };

        // Ranged enemies strafe perpendicular while in range.
        let mut strafe = Vec2::ZERO;
        brain.strafe_timer.tick(time.delta());
        if brain.strafe_timer.just_finished() {
            brain.strafe_dir *= -1.0;
            brain.strafe_timer = Timer::from_seconds(rng.random_range(0.6..1.4), TimerMode::Once);
        }
        if !dashing && dist < brain.preferred_range + 60.0 {
            let amount = if enemy.kind == EnemyKind::IdpdShield {
                0.25
            } else {
                0.6
            };
            strafe = Vec2::new(-dir.y, dir.x) * brain.strafe_dir * amount;
        }

        let target = (desired + strafe).normalize_or_zero();
        // Emplacements never move but still fire below.
        if emplacement {
            vel.0 = Vec2::ZERO;
            sprite.flip_x = dir.x < 0.0;
        } else if brain.speed > 0.0 {
            vel.0 += target * brain.accel * dt;

            if vel.0.length() > speed {
                vel.0 = vel.0.normalize() * speed;
            }
            vel.0 *= 0.90_f32.powf(dt * 60.0);
            tf.translation += (vel.0 * dt).extend(0.0);
        } else {
            vel.0 = Vec2::ZERO;
        }

        resolve_prop_collision(&mut tf.translation, def.radius, &props);
        mask.resolve_circle(&mut tf.translation, def.radius);
        clamp_to_arena(&mut tf.translation, def.radius);
        sprite.flip_x = dir.x < 0.0;

        // Light separation from other enemies.
        for other in &positions {
            let d = pos.distance(*other);
            if d < def.radius + 14.0 && d > 0.001 {
                let push = (pos - *other).normalize() * (def.radius + 14.0 - d) * 0.5;
                tf.translation.x += push.x;
                tf.translation.y += push.y;
            }
        }

        // Necromancer revive pulse: revive nearest corpse or spawn a Freak.
        if enemy.kind == EnemyKind::Necromancer {
            brain.attack.tick(time.delta());
            if brain.attack.just_finished() {
                brain.attack = Timer::from_seconds(def.attack_cooldown, TimerMode::Once);
                let mut best: Option<(Entity, Vec2)> = None;
                let mut best_d = 160.0;
                for (ce, _corpse, ctf) in &corpses {
                    let d = ctf.translation.truncate().distance(pos);
                    if d < best_d {
                        best_d = d;
                        best = Some((ce, ctf.translation.truncate()));
                    }
                }
                if let Some((ce, cpos)) = best {
                    commands.entity(ce).despawn();
                    commands.spawn(PendingEnemySpawn {
                        kind: EnemyKind::Freak,
                        pos: cpos,
                        difficulty: 1.0,
                    });
                } else if (positions.len() as u32) < 40 {
                    let ang = rng.random_range(0.0..std::f32::consts::TAU);
                    let p = pos + Vec2::new(ang.cos(), ang.sin()) * 40.0;
                    commands.spawn(PendingEnemySpawn {
                        kind: EnemyKind::Freak,
                        pos: p,
                        difficulty: 1.0,
                    });
                }
            }
        }

        // Firing.
        if def.bullets_per_shot > 0 && dist < brain.shoot_range && !dashing {
            if def.burst {
                if brain.burst_left > 0 {
                    brain.burst_timer.tick(time.delta());
                    if brain.burst_timer.just_finished() {
                        fire_enemy_bullet(&mut commands, &mut rng, enemy, def, pos, dir, euphoria);
                        brain.burst_left -= 1;
                        if brain.burst_left == 0 {
                            brain.attack =
                                Timer::from_seconds(def.attack_cooldown, TimerMode::Once);
                        }
                    }
                } else {
                    brain.attack.tick(time.delta());
                    if brain.attack.just_finished() {
                        brain.burst_left = def.bullets_per_shot;
                        brain.burst_timer =
                            Timer::from_seconds(def.burst_interval, TimerMode::Once);
                        fire_enemy_bullet(&mut commands, &mut rng, enemy, def, pos, dir, euphoria);
                        brain.burst_left -= 1;
                    }
                }
            } else {
                brain.attack.tick(time.delta());
                if brain.attack.just_finished() {
                    fire_enemy_shot(&mut commands, &mut rng, enemy, def, pos, dir);
                    brain.attack = Timer::from_seconds(def.attack_cooldown, TimerMode::Once);
                }
            }
        }
    }
}

fn fire_enemy_bullet(
    commands: &mut Commands,
    rng: &mut impl RngExt,
    enemy: &Enemy,
    def: EnemyDef,
    pos: Vec2,
    dir: Vec2,
    euphoria: bool,
) {
    let base = dir.y.atan2(dir.x);
    let angle = base + rng.random_range(-def.projectile_spread..def.projectile_spread);
    let shot_dir = Vec2::new(angle.cos(), angle.sin());
    let speed = def.projectile_speed * if euphoria { 0.8 } else { 1.0 };
    commands.spawn((
        GameCleanup,
        LevelCleanup,
        Team::Enemy,
        Projectile {
            damage: def.projectile_damage,
            life: Timer::from_seconds(def.projectile_lifetime, TimerMode::Once),
            radius: def.projectile_radius,
            knockback: 150.0,
            explosive: explosive_kind(enemy.kind),
            source: Some(DamageSource {
                owner: Entity::PLACEHOLDER,
                team: Team::Enemy,
                hit_id: HitId::Enemy(0),
            }),
        },
        Velocity(shot_dir * speed),
        Sprite {
            color: def.projectile_color,
            custom_size: Some(Vec2::splat(def.projectile_size)),
            ..default()
        },
        Transform::from_translation((pos + shot_dir * 20.0).extend(15.0)),
    ));
    let _ = enemy;
}

/// Kinds whose projectiles detonate on impact (tank rockets, explo orbs).
fn explosive_kind(kind: EnemyKind) -> bool {
    matches!(
        kind,
        EnemyKind::SnowTank | EnemyKind::GoldSnowtank | EnemyKind::ExploGuardian
    )
}

fn fire_enemy_shot(
    commands: &mut Commands,
    rng: &mut impl RngExt,
    enemy: &Enemy,
    def: EnemyDef,
    pos: Vec2,
    dir: Vec2,
) {
    let base = dir.y.atan2(dir.x);
    let total = def.bullets_per_shot;
    for i in 0..total {
        let offset = if total > 1 {
            (i as f32 - (total as f32 - 1.0) * 0.5) * def.fan_spread
        } else {
            0.0
        };
        let angle = base + offset + rng.random_range(-0.06..0.06);
        let shot_dir = Vec2::new(angle.cos(), angle.sin());
        commands.spawn((
            GameCleanup,
            LevelCleanup,
            Team::Enemy,
            Projectile {
                damage: def.projectile_damage,
                life: Timer::from_seconds(def.projectile_lifetime, TimerMode::Once),
                radius: def.projectile_radius,
                knockback: 150.0,
                explosive: explosive_kind(enemy.kind),
                source: Some(DamageSource {
                    owner: Entity::PLACEHOLDER,
                    team: Team::Enemy,
                    hit_id: HitId::Enemy(0),
                }),
            },
            Velocity(shot_dir * def.projectile_speed),
            Sprite {
                color: def.projectile_color,
                custom_size: Some(Vec2::splat(def.projectile_size)),
                ..default()
            },
            Transform::from_translation((pos + shot_dir * 20.0).extend(15.0)),
        ));
    }
    let _ = enemy;
}
/// Big Bandit bursts in once enough of the floor's trash is dead, charging
/// from a wall-adjacent cell near the player (upstream BanditBoss behaviour).
pub fn tick_delayed_boss_spawns(
    mut commands: Commands,
    catalog: Res<AssetCatalog>,
    asset_server: Res<AssetServer>,
    run: Res<Run>,
    mask: Res<FloorMask>,
    mut trauma: ResMut<game_utils_bevy::screen_effects::Trauma>,
    mut hitstop: ResMut<game_utils_bevy::hitstop::HitStop>,
    mut toast: ResMut<Toast>,
    pending: Query<(Entity, &PendingDelayedBoss)>,
    enemies: Query<&Enemy, With<Enemy>>,
    player_q: Query<&Transform, With<Player>>,
    walls: Query<(Entity, &WallCell, &Transform, Option<&ScreenEnd>), With<WallTile>>,
) {
    let Ok((marker_e, pending_boss)) = pending.single() else {
        return;
    };

    let living_trash = enemies
        .iter()
        .filter(|e| !crate::game::content::is_boss(e.kind))
        .count() as u32;
    let killed = pending_boss.initial_trash.saturating_sub(living_trash);
    if killed < pending_boss.kills_needed() {
        return;
    }

    let Ok(player_tf) = player_q.single() else {
        return;
    };
    let player_pos = player_tf.translation.truncate();

    // Prefer a wall cell roughly 180px from the player (side walls), with a
    // bonus for screen-end walls (outer ring).
    let mut best_wall: Option<(Vec2, (i32, i32))> = None;
    let mut best_score = f32::MAX;
    if pending_boss.from_wall {
        for (_, cell, tf, screen_end) in &walls {
            let p = tf.translation.truncate();
            let d = p.distance(player_pos);
            if d < 120.0 || d > 260.0 {
                continue;
            }
            let mut score = (d - 180.0).abs() + (p.y - player_pos.y).abs() * 0.25;
            if screen_end.is_some() {
                score -= 20.0;
            }
            if score < best_score {
                best_score = score;
                best_wall = Some((p, (cell.0, cell.1)));
            }
        }
    }

    let spawn_pos = if let Some((p, _)) = best_wall {
        p
    } else {
        // Fallback: walkable floor a few tiles from player.
        let mut rng = rand::rng();
        let mut best = mask.random_floor_pos(&mut rng, 120.0);
        for _ in 0..32 {
            let ang = rng.random_range(0.0..std::f32::consts::TAU);
            let cand =
                player_pos + Vec2::new(ang.cos(), ang.sin()) * rng.random_range(140.0..240.0);
            if mask.is_walkable(cand) {
                best = cand;
                break;
            }
        }
        best
    };

    commands.entity(marker_e).despawn();
    ScreenEffects::add_trauma(&mut trauma, 0.3);

    // Carve a hole so Bandit doesn't suffocate in the wall.
    if let Some((p, cell)) = best_wall {
        for (dx, dy) in [(-1, 0), (1, 0), (0, -1), (0, 1), (0, 0)] {
            commands.spawn((
                GameCleanup,
                LevelCleanup,
                PendingWallBreak {
                    cell: (cell.0 + dx, cell.1 + dy),
                    pos: p,
                    spawn_floor: true,
                },
            ));
        }
    }

    spawn_enemy_at(
        &mut commands,
        &catalog,
        &asset_server,
        pending_boss.kind,
        spawn_pos,
        difficulty_multiplier(run.floor),
        false,
        false,
    );

    commands.spawn((
        GameCleanup,
        BossIntro {
            timer: Timer::from_seconds(1.1, TimerMode::Once),
        },
    ));
    toast.show("BIG BANDIT");
    hitstop.trigger(0.2, 0.15);
}

/// Frog Eggs laid by Mom sit for their attack timer, then hatch into Ballguys.
pub fn tick_frog_eggs(
    time: Res<Time<Fixed>>,
    mut commands: Commands,
    catalog: Res<AssetCatalog>,
    asset_server: Res<AssetServer>,
    run: Res<Run>,
    mut q: Query<(Entity, &Enemy, &mut EnemyBrain, &Transform), With<Enemy>>,
) {
    for (e, enemy, mut brain, tf) in &mut q {
        if enemy.kind != EnemyKind::FrogEgg {
            continue;
        }
        brain.attack.tick(time.delta());
        if !brain.attack.just_finished() {
            continue;
        }
        let pos = tf.translation.truncate();
        commands.entity(e).despawn();
        spawn_enemy_at(
            &mut commands,
            &catalog,
            &asset_server,
            EnemyKind::Ballguy,
            pos,
            difficulty_multiplier(run.floor),
            false,
            false,
        );
    }
}

pub fn tick_boss_intro(
    time: Res<Time<Fixed>>,
    mut commands: Commands,
    mut q: Query<(Entity, &mut BossIntro)>,
) {
    for (e, mut intro) in &mut q {
        intro.timer.tick(time.delta());
        if intro.timer.just_finished() {
            commands.entity(e).despawn();
        }
    }
}

pub fn tick_corpses(
    time: Res<Time<Fixed>>,
    mut commands: Commands,
    mut q: Query<(Entity, &mut Corpse)>,
) {
    for (e, mut c) in &mut q {
        c.life.tick(time.delta());
        if c.life.just_finished() {
            commands.entity(e).despawn();
        }
    }
}
