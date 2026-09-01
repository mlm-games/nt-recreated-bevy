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
    mut kind: EnemyKind,
    pos: Vec2,
    difficulty: f32,
    scarier_face: bool,
    heavy_heart: bool,
) {
    // GML Create_0 hooks that mutate spawn before spawn_enemy body
    // Scorpion -> GoldScorpion  Scorpion/Create_0.gml:18 if random(_rand)<1+loops*5 && subarea>1
    // Gatl logic needs Run loops/subarea – caller passes correct kind via world.rs;
    // keep hook as fallback for direct spawns (Blood crown _rand*0.7 handled in world)
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
    let anchor = crate::game::content::sprite_anchor(catalog, def.sprite);
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
            walk: 0.0,
            ammo: match kind {
                EnemyKind::Scorpion | EnemyKind::GoldScorpion => 10,
                _ => 0,
            },
            gunangle: rand::rng().random_range(0.0..std::f32::consts::TAU),
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
        anchor,
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

/// Approximate GML `collision_line(Wall)` for Bandit/Scorpion LOS.
/// Only WallTile blocks (not decor Props like barrels/cactus) — matching
/// GML `collision_line(x,y,target.x,target.y,Wall,0,0)`. The old version
/// checked every Prop and made Bandits blind behind decoration.
fn has_line_of_sight(from: Vec2, to: Vec2, walls: &Query<&WallCell, With<WallTile>>) -> bool {
    let dir = to - from;
    let dist = dir.length();
    if dist < 1.0 {
        return true;
    }
    let steps = (dist / 16.0).ceil().max(8.0) as usize;
    for i in 1..steps {
        let t = i as f32 / steps as f32;
        let p = from + dir * t;
        for cell in walls.iter() {
            let c = Vec2::new(cell.0 as f32 * 16.0 + 8.0, cell.1 as f32 * 16.0 + 8.0);
            let half = Vec2::splat(8.0);
            if p.x >= c.x - half.x
                && p.x <= c.x + half.x
                && p.y >= c.y - half.y
                && p.y <= c.y + half.y
            {
                return false;
            }
        }
    }
    true
}

pub fn enemy_ai(
    time: Res<Time<Fixed>>,
    mut commands: Commands,
    catalog: Res<AssetCatalog>,
    asset_server: Res<AssetServer>,
    mut _trauma: ResMut<game_utils_bevy::screen_effects::Trauma>,
    euphoria: Res<Euphoria>,
    mask: Res<FloorMask>,
    run: Res<Run>,
    player_q: Query<(&Transform, &Player), (With<Player>, Without<Enemy>)>,
    mut enemies: Query<
        (
            Entity,
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
    walls_los: Query<&WallCell, With<WallTile>>,
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
        .map(|(_, _, _, _, tf, _, _)| tf.translation.truncate())
        .collect();

    for (entity, enemy, mut brain, mut vel, mut tf, mut sprite, boss) in &mut enemies {
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
                | EnemyKind::InvLaserCrystal
                | EnemyKind::MaggotSpawn
        );

        // Melee contact cooldown (reference: 30 frames between hits).
        brain.melee.tick(time.delta());

        // --- Exact GML walk (Other_10) + subtractive friction. FixedUpdate==30Hz ---
        if brain.walk > 0.0 {
            let (impulse_f, cap_f) = match enemy.kind {
                EnemyKind::Scorpion | EnemyKind::GoldScorpion => (2.0, 4.0),
                EnemyKind::Bandit | EnemyKind::SnowBandit | EnemyKind::JungleBandit => (0.8, 3.0),
                EnemyKind::Maggot => (0.6, 2.0),
                EnemyKind::Rat | EnemyKind::Ratking => (0.8, 4.0),
                EnemyKind::Gator
                | EnemyKind::BuffGator
                | EnemyKind::Jock
                | EnemyKind::Molefish
                | EnemyKind::Molesarge
                | EnemyKind::BoneFish => (0.8, 3.0),
                EnemyKind::Raven => (0.8, 3.5),
                EnemyKind::Salamander => (2.0, 2.5),
                EnemyKind::Freak | EnemyKind::ExploFreak => (0.55, 4.0),
                EnemyKind::RhinoFreak => (0.8, 1.0),
                EnemyKind::Crab => (1.5, 4.5),
                EnemyKind::Turtle => (1.0, 5.0),
                EnemyKind::FireBaller => (0.6, 2.0),
                EnemyKind::SuperFireBaller => (0.6, 1.5),
                EnemyKind::SnowTank | EnemyKind::GoldSnowtank => (0.6, 1.5),
                EnemyKind::DogGuardian => (0.4, 2.0),
                EnemyKind::JungleFly => (0.8, 3.5),
                EnemyKind::Spider | EnemyKind::InvSpider => (2.0, 4.0),
                EnemyKind::Sniper => (0.8, 1.5),
                _ => (0.4, 4.0),
            };
            // GML uses `direction` while walking (set in Alarm_1). Preserve
            // the direction chosen then: if vel already points elsewhere
            // (e.g., flee `direction = target+180`), keep that bearing.
            // Otherwise fall back to `dir` toward player.
            let walk_dir = if vel.0.length_squared() > 1.0 {
                vel.0.normalize_or_zero()
            } else {
                dir
            };
            gml_motion_add_clamp(&mut vel.0, walk_dir, impulse_f, cap_f, dt);
            brain.walk -= dt * 30.0; // GML walk is in frames
            if brain.walk < 0.0 {
                brain.walk = 0.0;
            }
        }
        // friction 0.4 px/frame everywhere (enemy/Create_0)
        apply_gml_friction(&mut vel.0, 0.4, dt);

        // --- Bandit GML Alarm_1 branch ( Bandit/Alarm_1.gml ) ---
        if matches!(
            enemy.kind,
            EnemyKind::Bandit | EnemyKind::SnowBandit | EnemyKind::JungleBandit
        ) {
            // Drive Bandit entirely via its Alarm_1 timer + walk
            brain.attack.tick(time.delta());
            if brain.attack.just_finished() {
                let los = has_line_of_sight(pos, player_pos, &walls_los);
                if los {
                    if dist > 48.0 {
                        if rng.random::<f32>() < 0.25 {
                            // Shoot EnemyBullet1 speed 4 (120 px/s) spread 20, wkick 4
                            let spread = rng.random_range(-10.0_f32..10.0).to_radians();
                            let base_ang = dir.y.atan2(dir.x);
                            let ang = base_ang + spread;
                            let sdir = Vec2::new(ang.cos(), ang.sin());
                            fire_enemy_bullet(
                                &mut commands,
                                &catalog,
                                &asset_server,
                                &mut rng,
                                entity,
                                enemy,
                                def,
                                pos,
                                sdir,
                                euphoria,
                            );
                            brain.gunangle = base_ang;
                            brain.attack = Timer::from_seconds(
                                (20.0 + rng.random_range(0.0..5.0)) / 30.0,
                                TimerMode::Once,
                            );
                        } else {
                            // Walk random direction around target GML speed 0.4 -> 12 px/s
                            let ang =
                                dir.y.atan2(dir.x) + rng.random_range(-90_f32..90.0).to_radians();
                            let wdir = Vec2::new(ang.cos(), ang.sin());
                            vel.0 = wdir * (0.4 * 30.0);
                            brain.walk = 10.0 + rng.random_range(0.0..10.0);
                            brain.gunangle = dir.y.atan2(dir.x);
                            brain.attack = Timer::from_seconds(
                                (20.0 + rng.random_range(0.0..5.0)) / 30.0,
                                TimerMode::Once,
                            );
                        }
                    } else {
                        // Too close: flee away GML speed 0.4
                        let away = -dir;
                        let ang =
                            away.y.atan2(away.x) + rng.random_range(-10_f32..10.0).to_radians();
                        let wdir = Vec2::new(ang.cos(), ang.sin());
                        vel.0 = wdir * (0.4 * 30.0);
                        brain.walk = 40.0 + rng.random_range(0.0..10.0);
                        brain.gunangle = dir.y.atan2(dir.x);
                        brain.attack = Timer::from_seconds(
                            (20.0 + rng.random_range(0.0..5.0)) / 30.0,
                            TimerMode::Once,
                        );
                    }
                    sprite.flip_x = player_pos.x < pos.x;
                } else if rng.random::<f32>() < 0.25 {
                    let ang = rng.random_range(0.0..std::f32::consts::TAU);
                    let wdir = Vec2::new(ang.cos(), ang.sin());
                    vel.0 = wdir * (0.4 * 30.0);
                    brain.walk = 20.0 + rng.random_range(0.0..10.0);
                    brain.attack = Timer::from_seconds(
                        (brain.walk + 10.0 + rng.random_range(0.0..30.0)) / 30.0,
                        TimerMode::Once,
                    );
                    brain.gunangle = ang;
                    sprite.flip_x = vel.0.x < 0.0;
                }
                // else: GML does nothing - stay still, friction will stop slide
            }
            // Friction already applied above via subtractive block; just move and separate
            {
                if vel.0.length() > 90.0 {
                    vel.0 = vel.0.normalize() * 90.0;
                }
                tf.translation += (vel.0 * dt).extend(0.0);
                resolve_prop_collision(&mut tf.translation, def.radius, &props);
                mask.resolve_circle(&mut tf.translation, def.radius);
                clamp_to_arena(&mut tf.translation, def.radius);
                for other in &positions {
                    let d = pos.distance(*other);
                    if d < def.radius + 14.0 && d > 0.001 {
                        let push = (pos - *other).normalize() * (def.radius + 14.0 - d) * 0.5;
                        tf.translation.x += push.x;
                        tf.translation.y += push.y;
                    }
                }
            }
            continue;
        }

        // --- Scorpion GML Alarm_1 + Alarm_2 + Other_10 ---
        if matches!(enemy.kind, EnemyKind::Scorpion | EnemyKind::GoldScorpion) {
            brain.attack.tick(time.delta());
            brain.burst_timer.tick(time.delta());
            // Burst firing via Alarm_2 (every 2 frames) while ammo>0
            if brain.ammo > 0 && brain.burst_left > 0 {
                if brain.burst_timer.just_finished() {
                    let spread = rng.random_range(-20_f32..20.0).to_radians();
                    let base_ang = brain.gunangle;
                    let ang = base_ang + spread;
                    let sdir = Vec2::new(ang.cos(), ang.sin());
                    let speed = rng.random_range(90.0..120.0); // 3..4 *30
                    let (sprite_b, anchor) =
                        enemy_bullet_sprite(&catalog, &asset_server, enemy.kind, def);
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
                            source: Some(DamageSource::enemy(entity, enemy.kind)),
                        },
                        Velocity(sdir * speed),
                        sprite_b,
                        anchor,
                        Transform::from_translation((pos + sdir * 20.0).extend(15.0))
                            .with_rotation(Quat::from_rotation_z(ang)),
                    ));
                    brain.ammo = brain.ammo.saturating_sub(1);
                    brain.burst_left -= 1;
                    if brain.ammo == 0 || brain.burst_left == 0 {
                        brain.attack = Timer::from_seconds(
                            (40.0 + rng.random_range(0.0..10.0)) / 30.0,
                            TimerMode::Once,
                        );
                        brain.ammo = 10; // reset for next burst? GML ammo=10 resets at Alarm1 start
                    } else {
                        brain.burst_timer = Timer::from_seconds(2.0 / 30.0, TimerMode::Once);
                    }
                }
            } else if brain.attack.just_finished() {
                // Alarm_1 logic
                let target_dir = dir.y.atan2(dir.x);
                // scrWalk(_target_direction+orandom60+180,0,10,20) GML speed 0.4
                let walk_ang = target_dir
                    + rng.random_range(-60_f32..60.0).to_radians()
                    + std::f32::consts::PI;
                let wdir = Vec2::new(walk_ang.cos(), walk_ang.sin());
                vel.0 = wdir * (0.4 * 30.0);
                brain.walk = 10.0 + rng.random_range(0.0..10.0);
                sprite.flip_x = vel.0.x < 0.0;
                // visible check 210
                let los = has_line_of_sight(pos, player_pos, &walls_los);
                if los && dist < 210.0 && rng.random::<f32>() < 0.5 {
                    brain.attack = Timer::from_seconds(
                        (30.0 + rng.random_range(0.0..5.0)) / 30.0,
                        TimerMode::Once,
                    );
                    brain.burst_timer = Timer::from_seconds(1.0 / 30.0, TimerMode::Once);
                    brain.burst_left = 10;
                    brain.ammo = 10;
                    brain.gunangle = target_dir;
                    sprite.flip_x = player_pos.x < pos.x;
                } else {
                    brain.attack = Timer::from_seconds(
                        (30.0 + rng.random_range(0.0..10.0)) / 30.0,
                        TimerMode::Once,
                    );
                }
                if dist < 64.0 {
                    let away = -dir;
                    let ang = away.y.atan2(away.x) + rng.random_range(-10_f32..10.0).to_radians();
                    if dist > 32.0 {
                        let ang2 = ang + std::f32::consts::PI;
                        vel.0 = Vec2::new(ang2.cos(), ang2.sin()) * (0.4 * 30.0);
                    } else {
                        vel.0 = Vec2::new(ang.cos(), ang.sin()) * (0.4 * 30.0);
                    }
                    brain.walk = 40.0;
                }
            }
            // Friction already applied at top; clamp is 4*30 =120 for scorpion GML cap
            if vel.0.length() > 120.0 {
                vel.0 = vel.0.normalize() * 120.0;
            }
            tf.translation += (vel.0 * dt).extend(0.0);
            resolve_prop_collision(&mut tf.translation, def.radius, &props);
            mask.resolve_circle(&mut tf.translation, def.radius);
            clamp_to_arena(&mut tf.translation, def.radius);
            for other in &positions {
                let d = pos.distance(*other);
                if d < def.radius + 14.0 && d > 0.001 {
                    let push = (pos - *other).normalize() * (def.radius + 14.0 - d) * 0.5;
                    tf.translation.x += push.x;
                    tf.translation.y += push.y;
                }
            }
            continue;
        }

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
        // Rhino Freak / Dog Guardian / Turtle: charge attacks.
        if matches!(
            enemy.kind,
            EnemyKind::RhinoFreak | EnemyKind::DogGuardian | EnemyKind::Turtle
        ) && !was_dashing
            && dist < 220.0
            && dist > 40.0
            && brain.melee.is_finished()
        {
            let (dash_time, dash_speed) = if enemy.kind == EnemyKind::Turtle {
                (0.5, 420.0)
            } else {
                (0.42, 700.0)
            };
            brain.dash = dash_time;
            brain.melee = Timer::from_seconds(1.6, TimerMode::Once);
            vel.0 = dir * dash_speed;
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
        // Emplacements never move but still fire below.
        if emplacement {
            vel.0 = Vec2::ZERO;
            sprite.flip_x = dir.x < 0.0;
        } else if dashing {
            // Dashes already set vel directly; just clamp/keep via friction block
            // (friction applied at top block). Nothing to add here.
            tf.translation += (vel.0 * dt).extend(0.0);
        } else if brain.speed > 0.0 {
            // GML-faithful: enemies do NOT constantly home. They move only in
            // bursts when `walk>0` (Other_10: motion_add) and pick a new
            // direction in Alarm[1]. The previous `vel += target*accel` made
            // every grunt slide toward the player nonstop (bug report).
            // Tick the Alarm[1] timer and choose a new walk when it fires.
            // Bandit/Scorpion already continued above, so only generic here.
            brain.attack.tick(time.delta());
            if brain.attack.just_finished() {
                let los = has_line_of_sight(pos, player_pos, &walls_los);
                let base_ang = dir.y.atan2(dir.x);
                // Per-kind walk params sourced from objects/*/{Alarm_1.gml,Other_10.gml}
                // GML walk uses px/frame impulse (`speed`/`motion_add`) and walk frames.
                let (impulse, _cap, far_walk, close_walk, wander_walk) = match enemy.kind {
                    EnemyKind::Maggot => (0.6, 2.0, 8.0..14.0, 12.0..18.0, 10.0..20.0),
                    EnemyKind::Gator | EnemyKind::BuffGator => {
                        (0.8, 3.0, 10.0..14.0, 40.0..50.0, 20.0..30.0)
                    }
                    EnemyKind::Freak | EnemyKind::ExploFreak => {
                        (0.55, 4.0, 18.0..22.0, 12.0..18.0, 10.0..16.0)
                    }
                    EnemyKind::RhinoFreak => (0.8, 1.0, 18.0..22.0, 12.0..18.0, 10.0..16.0),
                    EnemyKind::Spider | EnemyKind::InvSpider => {
                        (2.0, 5.0, 15.0..20.0, 10.0..14.0, 10.0..20.0)
                    }
                    EnemyKind::Crab => (1.5, 4.5, 8.0..14.0, 50.0..60.0, 20.0..30.0),
                    EnemyKind::Turtle => (1.0, 5.0, 40.0..60.0, 40.0..60.0, 40.0..60.0),
                    EnemyKind::Salamander => (2.0, 2.5, 40.0..50.0, 20.0..30.0, 10.0..20.0),
                    EnemyKind::Sniper => (0.8, 1.5, 10.0..14.0, 40.0..50.0, 20.0..30.0),
                    EnemyKind::FireBaller | EnemyKind::SuperFireBaller => {
                        (0.6, 2.0, 8.0..12.0, 10.0..14.0, 10.0..16.0)
                    }
                    EnemyKind::Jock => (0.8, 3.0, 10.0..14.0, 40.0..50.0, 20.0..30.0),
                    EnemyKind::Molefish | EnemyKind::Molesarge => {
                        (0.8, 3.5, 10.0..14.0, 20.0..30.0, 20.0..30.0)
                    }
                    EnemyKind::Raven => (0.8, 3.5, 20.0..30.0, 40.0..50.0, 20.0..30.0),
                    EnemyKind::Rat
                    | EnemyKind::Ratking
                    | EnemyKind::FastRat
                    | EnemyKind::BigRat => (0.8, 4.0, 10.0..16.0, 40.0..50.0, 10.0..25.0),
                    EnemyKind::Wolf => (0.8, 4.0, 10.0..16.0, 20.0..30.0, 12.0..20.0),
                    EnemyKind::Assassin | EnemyKind::MeleeBandit => {
                        (0.8, 4.0, 10.0..14.0, 20.0..28.0, 16.0..24.0)
                    }
                    EnemyKind::Ballguy => (0.6, 3.0, 12.0..18.0, 12.0..18.0, 12.0..18.0),
                    EnemyKind::LightningCrystal => (0.5, 1.5, 10.0..14.0, 10.0..14.0, 10.0..20.0),
                    _ => (0.4, 4.0, 6.0..14.0, 18.0..28.0, 10.0..18.0),
                };
                // Continuous chasers (Maggot, FireBaller, Laser) in OG use
                // unconditional motion_add in Other_10 with the last Alarm
                // direction held for ~30 frames – still bursty, not per-frame homing.
                // So still use walk bursts here.
                if los {
                    if dist > 80.0 {
                        if rng.random::<f32>() < 0.35 {
                            brain.walk = 0.0;
                            vel.0 *= 0.5;
                        } else {
                            let ang = base_ang + rng.random_range(-45_f32..45.0).to_radians();
                            let wdir = Vec2::new(ang.cos(), ang.sin());
                            vel.0 = wdir * (impulse * 30.0);
                            brain.walk = rng.random_range(far_walk);
                            brain.gunangle = base_ang;
                        }
                    } else if dist < 44.0 {
                        let away = -dir;
                        let ang =
                            away.y.atan2(away.x) + rng.random_range(-15_f32..15.0).to_radians();
                        let wdir = Vec2::new(ang.cos(), ang.sin());
                        vel.0 = wdir * (impulse * 30.0);
                        brain.walk = rng.random_range(close_walk);
                        brain.gunangle = base_ang;
                    } else {
                        let ang = base_ang + rng.random_range(-90_f32..90.0).to_radians();
                        let wdir = Vec2::new(ang.cos(), ang.sin());
                        vel.0 = wdir * (impulse * 30.0);
                        brain.walk = rng.random_range(6.0..10.0);
                    }
                    brain.attack =
                        Timer::from_seconds(rng.random_range(0.35..0.75), TimerMode::Once);
                    if player_pos.x < pos.x {
                        sprite.flip_x = true;
                    } else if player_pos.x > pos.x {
                        sprite.flip_x = false;
                    }
                } else if rng.random::<f32>() < 0.4 {
                    let ang = rng.random_range(0.0..std::f32::consts::TAU);
                    let wdir = Vec2::new(ang.cos(), ang.sin());
                    vel.0 = wdir * (impulse * 30.0);
                    brain.walk = rng.random_range(wander_walk);
                    brain.attack = Timer::from_seconds(
                        (brain.walk + 10.0 + rng.random_range(0.0..18.0)) / 30.0,
                        TimerMode::Once,
                    );
                    sprite.flip_x = vel.0.x < 0.0;
                } else {
                    brain.attack = Timer::from_seconds(rng.random_range(0.3..0.6), TimerMode::Once);
                }
            }
            // walk impulse is handled by the top `if brain.walk>0` block on
            // the *next* fixed tick; this tick we just friction-slide.
            // Apply already-ticked friction at top, then translate.
            if vel.0.length() > brain.speed {
                vel.0 = vel.0.normalize() * brain.speed;
            }
            tf.translation += (vel.0 * dt).extend(0.0);
        } else {
            vel.0 = Vec2::ZERO;
        }

        resolve_prop_collision(&mut tf.translation, def.radius, &props);
        mask.resolve_circle(&mut tf.translation, def.radius);
        clamp_to_arena(&mut tf.translation, def.radius);
        // Keep alarm-chosen facing (wander uses vel dir, LOS uses target sign)
        // instead of forcing player direction every frame like the old slide.
        if brain.walk == 0.0 {
            sprite.flip_x = dir.x < 0.0;
        }

        // Cursed-cave veil: invisible variants fade in once the player gets
        // close (upstream InvSpider / InvLaserCrystal visibility flag).
        if matches!(
            enemy.kind,
            EnemyKind::InvSpider | EnemyKind::InvLaserCrystal
        ) {
            let alpha = if dist < 150.0 { 1.0 } else { 0.12 };
            sprite.color.set_alpha(alpha);
        }

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
                    // Upstream loop Labs/Palace: revived freaks rise as
                    // popo-freak police (WantRevivePopoFreak chain).
                    let revived = if run.loop_count >= 1
                        && matches!(
                            run.area,
                            crate::game::areas::AreaId::Labs | crate::game::areas::AreaId::Palace
                        ) {
                        EnemyKind::PopoFreak
                    } else {
                        EnemyKind::Freak
                    };
                    commands.spawn(PendingEnemySpawn {
                        kind: revived,
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

        // MaggotSpawn nests periodically bubble out a Maggot.
        if enemy.kind == EnemyKind::MaggotSpawn {
            brain.attack.tick(time.delta());
            if brain.attack.just_finished() {
                brain.attack = Timer::from_seconds(def.attack_cooldown, TimerMode::Once);
                let ang = rng.random_range(0.0..std::f32::consts::TAU);
                commands.spawn(PendingEnemySpawn {
                    kind: EnemyKind::Maggot,
                    pos: pos + Vec2::new(ang.cos(), ang.sin()) * 24.0,
                    difficulty: 1.0,
                });
            }
        }

        // Firing.
        if def.bullets_per_shot > 0 && dist < brain.shoot_range && !dashing {
            if def.burst {
                if brain.burst_left > 0 {
                    brain.burst_timer.tick(time.delta());
                    if brain.burst_timer.just_finished() {
                        fire_enemy_bullet(
                            &mut commands,
                            &catalog,
                            &asset_server,
                            &mut rng,
                            entity,
                            enemy,
                            def,
                            pos,
                            dir,
                            euphoria,
                        );
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
                        fire_enemy_bullet(
                            &mut commands,
                            &catalog,
                            &asset_server,
                            &mut rng,
                            entity,
                            enemy,
                            def,
                            pos,
                            dir,
                            euphoria,
                        );
                        brain.burst_left -= 1;
                    }
                }
            } else {
                brain.attack.tick(time.delta());
                if brain.attack.just_finished() {
                    fire_enemy_shot(
                        &mut commands,
                        &catalog,
                        &asset_server,
                        &mut rng,
                        entity,
                        enemy,
                        def,
                        pos,
                        dir,
                    );
                    brain.attack = Timer::from_seconds(def.attack_cooldown, TimerMode::Once);
                }
            }
        }
    }
}

fn enemy_bullet_sprite(
    catalog: &AssetCatalog,
    asset_server: &AssetServer,
    kind: EnemyKind,
    _def: EnemyDef,
) -> (Sprite, bevy::sprite::Anchor) {
    let sprite =
        crate::game::projectile_art::enemy_projectile_sprite(asset_server, catalog, kind, None);
    // Resolve anchor from the resolved path (first existing candidate).
    let primary = crate::game::projectile_art::enemy_projectile_path(kind);
    let candidates = [
        primary,
        "images/sprEnemyBullet1.png",
        "images/sprEnemyBullet.png",
        "images/sprBullet1.png",
        "images/sprBullet2.png",
    ];
    let path = crate::game::projectile_art::first_existing(catalog, &candidates);
    let anchor = crate::game::content::sprite_anchor(catalog, path);
    (sprite, anchor)
}

fn fire_enemy_bullet(
    commands: &mut Commands,
    catalog: &AssetCatalog,
    asset_server: &AssetServer,
    rng: &mut impl RngExt,
    owner: Entity,
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
    let (sprite, anchor) = enemy_bullet_sprite(catalog, asset_server, enemy.kind, def);
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
            source: Some(DamageSource::enemy(owner, enemy.kind)),
        },
        Velocity(shot_dir * speed),
        sprite,
        anchor,
        Transform::from_translation((pos + shot_dir * 20.0).extend(15.0))
            .with_rotation(Quat::from_rotation_z(angle)),
    ));
}

/// Kinds whose projectiles detonate on impact (tank rockets, explo orbs).
fn explosive_kind(kind: EnemyKind) -> bool {
    matches!(
        kind,
        EnemyKind::SnowTank | EnemyKind::GoldSnowtank | EnemyKind::ExploGuardian | EnemyKind::Jock
    )
}

fn fire_enemy_shot(
    commands: &mut Commands,
    catalog: &AssetCatalog,
    asset_server: &AssetServer,
    rng: &mut impl RngExt,
    owner: Entity,
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
        let (sprite, anchor) = enemy_bullet_sprite(catalog, asset_server, enemy.kind, def);
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
                source: Some(DamageSource::enemy(owner, enemy.kind)),
            },
            Velocity(shot_dir * def.projectile_speed),
            sprite,
            anchor,
            Transform::from_translation((pos + shot_dir * 20.0).extend(15.0))
                .with_rotation(Quat::from_rotation_z(angle)),
        ));
    }
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

/// Frog Eggs sit for their attack timer (upstream alarm[1] = 120 frames),
/// then burst into an 8-way acid ring.
pub fn tick_frog_eggs(
    time: Res<Time<Fixed>>,
    mut commands: Commands,
    catalog: Res<AssetCatalog>,
    asset_server: Res<AssetServer>,
    _run: Res<Run>,
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
        // Upstream FrogEgg Alarm_1: repeat 8 → AcidStreak at 45° steps.
        let (acid_sprite, acid_anchor) = {
            let s = crate::game::projectile_art::sprite_from_projectile_path(
                &asset_server,
                &catalog,
                &["images/sprAcidStreak.png", "images/sprEnemyBullet1.png"],
                None,
            );
            let a = crate::game::content::sprite_anchor(&catalog, "images/sprAcidStreak.png");
            (s, a)
        };
        for i in 0..8 {
            let ang = (i as f32) * std::f32::consts::TAU / 8.0;
            let d = Vec2::new(ang.cos(), ang.sin());
            commands.spawn((
                GameCleanup,
                LevelCleanup,
                Team::Enemy,
                Projectile {
                    damage: 3,
                    life: Timer::from_seconds(1.1, TimerMode::Once),
                    radius: 4.0,
                    knockback: 100.0,
                    explosive: false,
                    source: Some(DamageSource::enemy(e, enemy.kind)),
                },
                Velocity(d * 240.0),
                acid_sprite.clone(),
                acid_anchor,
                Transform::from_translation(pos.extend(15.0)),
            ));
        }
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

#[cfg(test)]
mod double_hp_check {
    use super::*;
    use crate::game::content::{EnemyKind, enemy_def};
    use crate::game::world::difficulty_multiplier;
    #[test]
    fn bandit_hp_is_4_not_8() {
        let def = enemy_def(EnemyKind::Bandit);
        let diff = difficulty_multiplier(1);
        let hp = (def.hp as f32 * diff).round() as i32;
        assert_eq!(hp, 4, "bandit hp should be 4, got {}", hp);
        println!("bandit hp {} diff {}", hp, diff);
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
