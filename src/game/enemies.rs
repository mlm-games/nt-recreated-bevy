//! Enemy spawning (per-floor packs + bosses) and AI behaviors.
//! AI mirrors the GPL Nuclear-Throne-Mobile rebuild reference.

use bevy::prelude::*;
use rand::RngExt;

use crate::game::components::*;
use crate::game::content::*;
use crate::game::world::*;
use game_utils_bevy::juice::Juice;

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
    spawn_enemy(commands, catalog, asset_server, kind, pos, difficulty, scarier_face, heavy_heart);
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

    let (sprite, strip) =
        crate::game::anim::sprite_anim(catalog, asset_server, def.sprite);
    let mut ec = commands
        .spawn((
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
                telegraph: 0.0,
                dash: 0.0,
                dash_cooldown: Timer::from_seconds(
                    1.2 + rand::rng().random_range(0.0..0.6),
                    TimerMode::Once,
                ),
                strafe_dir: if rand::rng().random_bool(0.5) {
                    1.0
                } else {
                    -1.0
                },
                strafe_timer: Timer::from_seconds(
                    rand::rng().random_range(0.8..1.6),
                    TimerMode::Once,
                ),
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
    mut trauma: ResMut<game_utils_bevy::screen_effects::Trauma>,
    euphoria: Res<Euphoria>,
    mask: Res<FloorMask>,
    player_q: Query<(&Transform, &Player), (With<Player>, Without<Enemy>)>,
    mut enemies: Query<
        (&Enemy, &mut EnemyBrain, &mut Velocity, &mut Transform, &mut Sprite),
        (With<Enemy>, Without<Prop>),
    >,
    props: Query<(Entity, &Prop, &Transform), With<Prop>>,
) {
    let Ok((player_tf, _player)) = player_q.single() else {
        return;
    };

    let player_pos = player_tf.translation.truncate();
    let dt = time.delta_secs();
    let mut rng = rand::rng();
    let euphoria = euphoria.0;

    // Pairwise separation to avoid enemy stacking.
    let positions: Vec<Vec2> = enemies
        .iter()
        .map(|(_, _, _, tf, _)| tf.translation.truncate())
        .collect();

    for (enemy, mut brain, mut vel, mut tf, mut sprite) in &mut enemies {
        let pos = tf.translation.truncate();
        let to_player = player_pos - pos;
        let dist = to_player.length();
        let dir = to_player.normalize_or_zero();

        let def = enemy_def(enemy.kind);

        // Melee contact cooldown (reference: 30 frames between hits).
        brain.melee.tick(time.delta());

        // Big Bandit charge: telegraph -> dash -> cooldown.
        if enemy.kind == EnemyKind::BigBandit {
            if brain.dash > 0.0 {
                brain.dash -= dt;
                if brain.dash <= 0.0 {
                    brain.dash = 0.0;
                    brain.telegraph = 0.0;
                    brain.dash_cooldown = Timer::from_seconds(
                        1.2 + rand::rng().random_range(0.0..0.6),
                        TimerMode::Once,
                    );
                }
            } else if brain.telegraph > 0.0 {
                brain.telegraph -= dt;
                if brain.telegraph <= 0.0 {
                    brain.telegraph = 0.0;
                    brain.dash = 0.33;
                    screen_effects::add_charge_trauma(&mut trauma);
                }
            } else {
                brain.dash_cooldown.tick(time.delta());
                if brain.dash_cooldown.just_finished() && dist < def.shoot_range {
                    brain.telegraph = 0.25;
                }
            }
        }

        let dashing = brain.dash > 0.0;
        let speed = if dashing { 600.0 } else { brain.speed };

        let desired = if dashing {
            dir
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
            strafe = Vec2::new(-dir.y, dir.x) * brain.strafe_dir * 0.6;
        }

        let target = (desired + strafe).normalize_or_zero();
        vel.0 += target * brain.accel * dt;

        if vel.0.length() > speed {
            vel.0 = vel.0.normalize() * speed;
        }
        vel.0 *= 0.90_f32.powf(dt * 60.0);
        tf.translation += (vel.0 * dt).extend(0.0);

        resolve_prop_collision(&mut tf.translation, def.radius, &props);
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

mod screen_effects {
    use super::*;
    pub fn add_charge_trauma(trauma: &mut ResMut<game_utils_bevy::screen_effects::Trauma>) {
        game_utils_bevy::screen_effects::ScreenEffects::add_trauma(trauma, 0.12);
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
            explosive: false,
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
                explosive: false,
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
