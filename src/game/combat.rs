//! Combat: projectile movement, projectile/world collisions, projectile/entity
//! hits, explosion AoE, contact damage, and death resolution.

use bevy::input::gamepad::{Gamepad, GamepadRumbleRequest};
use bevy::prelude::*;
use rand::RngExt;

use crate::app::{AppState, Paused};
use crate::game::audio::GameAudio;
use crate::game::components::*;
use crate::game::content::*;
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

pub fn move_projectiles(
    time: Res<Time<Fixed>>,
    mut commands: Commands,
    mut q: Query<
        (
            Entity,
            &Team,
            &mut Projectile,
            &mut Velocity,
            &mut Transform,
            Option<&mut BouncesLeft>,
            Option<&SpawnHazardOnDeath>,
            Option<&SplitOnDeath>,
        ),
        Without<Prop>,
    >,
    mut props: Query<(Entity, &mut Prop, &Transform), With<Prop>>,
) {
    let dt = time.delta_secs();

    for (e, team, mut p, mut vel, mut tf, bounces, hazard, split) in &mut q {
        p.life.tick(time.delta());
        tf.translation += (vel.0 * dt).extend(0.0);
        let pos = tf.translation.truncate();
        let out = pos.x.abs() > ARENA_W / 2.0 + 80.0 || pos.y.abs() > ARENA_H / 2.0 + 80.0;
        let wall_hit = !out
            && (pos.x.abs() > ARENA_W / 2.0 - p.radius || pos.y.abs() > ARENA_H / 2.0 - p.radius);
        let prop_hit = circle_hits_prop(pos, p.radius, &props);

        if p.life.just_finished() || out {
            on_projectile_removed(
                &mut commands,
                pos,
                *team,
                p.source,
                hazard.copied(),
                split.copied(),
                vel.0,
                p.explosive,
                p.damage,
            );
            commands.entity(e).despawn();
            continue;
        }

        if wall_hit {
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
                pos,
                *team,
                p.source,
                hazard.copied(),
                split.copied(),
                vel.0,
                p.explosive,
                p.damage,
            );
            commands.entity(e).despawn();
            continue;
        }

        if prop_hit {
            let mut dead_prop = None;
            for (prop_e, mut prop, prop_tf) in &mut props {
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
                        dead_prop = Some((center, prop.explosive));
                        commands.entity(prop_e).despawn();
                    }
                }
                break;
            }

            if let Some((center, explosive)) = dead_prop {
                VfxSpawner::spawn_burst(
                    &mut commands,
                    center,
                    10,
                    Color::srgb(0.8, 0.65, 0.4),
                    (60.0, 180.0),
                );
                if explosive {
                    spawn_explosion(&mut commands, center, 6);
                }
            }

            on_projectile_removed(
                &mut commands,
                pos,
                *team,
                p.source,
                hazard.copied(),
                split.copied(),
                vel.0,
                p.explosive,
                p.damage,
            );
            commands.entity(e).despawn();
        }
    }
}

fn spawn_explosion(commands: &mut Commands, pos: Vec2, damage: i32) {
    spawn_explosion_with_source(commands, pos, damage, None);
}

fn spawn_explosion_with_source(
    commands: &mut Commands,
    pos: Vec2,
    damage: i32,
    source: Option<DamageSource>,
) {
    commands.spawn((
        GameCleanup,
        LevelCleanup,
        Explosion {
            timer: Timer::from_seconds(0.05, TimerMode::Once),
            radius: 130.0,
            damage,
            team: Team::Player,
            hits_player: true,
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

/// Shared terminal path for timeout, wall, prop, and entity hits.
fn on_projectile_removed(
    commands: &mut Commands,
    pos: Vec2,
    team: Team,
    source: Option<DamageSource>,
    hazard: Option<SpawnHazardOnDeath>,
    split: Option<SplitOnDeath>,
    base_dir: Vec2,
    explosive: bool,
    damage: i32,
) {
    if explosive {
        spawn_explosion_with_source(commands, pos, damage, source);
    }
    if let Some(SpawnHazardOnDeath(spec)) = hazard {
        spawn_hazard_cloud(commands, pos, team, spec);
    }
    if let Some(SplitOnDeath(spec)) = split {
        spawn_split_projectiles(commands, pos, team, spec, source, base_dir);
    }
}

/// Weapon / team-tagged hazard clouds only.
/// Ability clouds are handled by `player::tick_hazard_clouds`.
pub fn tick_hazard_clouds(
    time: Res<Time<Fixed>>,
    mut commands: Commands,
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
    mut q: Query<
        (Entity, &mut Explosion, &Transform),
        (Without<Enemy>, Without<Player>, Without<Prop>),
    >,
    mut enemies: Query<(Entity, &Transform, &mut Health), (With<Enemy>, Without<Player>)>,
    mut player_q: Query<(Entity, &Transform, &mut Health, &Player), (With<Player>, Without<Enemy>)>,
    mut props: Query<(Entity, &mut Prop, &Transform), (With<Prop>, Without<Player>)>,
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
            // Explosions destroy props too.
            for (prop_e, mut prop, prop_tf) in &mut props {
                let center = prop_tf.translation.truncate();
                let half = prop.size / 2.0;
                let closest = Vec2::new(
                    pos.x.clamp(center.x - half.x, center.x + half.x),
                    pos.y.clamp(center.y - half.y, center.y + half.y),
                );
                if pos.distance(closest) < boom.radius && prop.destructible {
                    prop.hp -= 1;
                    if prop.hp <= 0 {
                        commands.entity(prop_e).despawn();
                        spawn_prop_destroyed(&mut commands, &mut trauma, &audio, pos);
                    }
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
            HitFlash::apply(&mut commands, player_e, Color::srgb(1.0, 0.3, 0.2), 0.15);
            audio.play_hurt(&mut commands);
        }

        commands.entity(e).despawn();
    }
}

fn spawn_prop_destroyed(
    commands: &mut Commands,
    trauma: &mut Trauma,
    audio: &GameAudio,
    pos: Vec2,
) {
    ScreenEffects::add_trauma(trauma, 0.15);
    VfxSpawner::spawn_burst(
        commands,
        pos,
        10,
        Color::srgb(0.8, 0.65, 0.4),
        (60.0, 180.0),
    );
    let _ = audio;
}

pub fn projectile_hits(
    mut commands: Commands,
    mut trauma: ResMut<Trauma>,
    mut hitstop: ResMut<HitStop>,
    audio: Res<GameAudio>,
    player_state: Query<&Player, With<Player>>,
    mut projectiles: Query<
        (
            Entity,
            &Transform,
            &Team,
            &Projectile,
            &Velocity,
            Option<&mut PiercesLeft>,
            Option<&mut ProjectileHitSet>,
            Option<&SpawnHazardOnDeath>,
            Option<&SplitOnDeath>,
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
    let mut to_despawn: Vec<(
        Entity,
        Vec2,
        Team,
        Option<DamageSource>,
        Option<SpawnHazardOnDeath>,
        Option<SplitOnDeath>,
        Vec2,
        bool,
        i32,
    )> = Vec::new();
    let player = player_state.single().ok();

    for (proj_e, proj_tf, proj_team, proj, proj_vel, pierce, hit_set, hazard, split) in
        &mut projectiles
    {
        let proj_pos = proj_tf.translation.truncate();
        let mut hit = false;
        let mut damaged = false;
        let mut hit_player = false;
        let mut hit_pos = proj_pos;
        let mut hit_target: Option<Entity> = None;

        for (target_e, target_tf, target_team, hitbox, mut health, vel_opt, shield) in &mut targets
        {
            if *target_team == *proj_team {
                continue;
            }

            // Already pierced this entity?
            if let Some(set) = hit_set.as_ref() {
                if set.0.contains(&target_e) {
                    continue;
                }
            }

            let target_pos = target_tf.translation.truncate();
            if proj_pos.distance(target_pos) > proj.radius + hitbox.radius {
                continue;
            }

            hit = true;
            hit_pos = target_pos;
            hit_target = Some(target_e);

            // Shield absorbs — still counts as a "hit" for despawn of non-pierce.
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
                // Invuln: treat as non-damaging contact; pierce should not burn.
                break;
            }

            health.hp -= proj.damage;
            damaged = true;

            if *target_team == Team::Player {
                health.invuln = Timer::from_seconds(5.0 / 30.0, TimerMode::Once);
                hit_player = true;
                audio.play_hurt(&mut commands);
            } else {
                audio.play_hit(&mut commands);
            }

            if let Some(mut vel) = vel_opt {
                let dir = proj_vel.0.normalize_or_zero();
                GameFeel::apply_knockback(&mut vel.0, dir, proj.knockback);
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

        if hit
            && hit_player
            && let Some(p) = &player
            && p.sharp_teeth
        {
            retaliate_sharp_teeth(&mut commands, proj.damage, hit_pos, &mut targets);
        }

        if !hit {
            continue;
        }

        // Record pierce target only when damage landed.
        if damaged {
            if let Some(target_e) = hit_target {
                if let Some(mut set) = hit_set {
                    crate::game::projectile_math::record_hit(&mut set.0, target_e);
                } else {
                    commands
                        .entity(proj_e)
                        .insert(ProjectileHitSet(vec![target_e]));
                }
            }
        }

        let pierce_left_before = pierce.as_ref().map(|p| p.0);
        let (despawn, pierce_left) =
            crate::game::projectile_math::should_despawn_after_hit(damaged, pierce_left_before);
        if let (Some(mut pierce), Some(left)) = (pierce, pierce_left) {
            pierce.0 = left;
        }

        if despawn {
            to_despawn.push((
                proj_e,
                proj_pos,
                *proj_team,
                proj.source,
                hazard.copied(),
                split.copied(),
                proj_vel.0,
                proj.explosive,
                proj.damage,
            ));
        }
    }

    for (e, pos, team, source, hazard, split, dir, explosive, damage) in to_despawn {
        on_projectile_removed(
            &mut commands,
            pos,
            team,
            source,
            hazard,
            split,
            dir,
            explosive,
            damage,
        );
        commands.entity(e).despawn();
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
    ),
    audio: Res<GameAudio>,
    gamepads: Query<(Entity, &Gamepad)>,
    mut rumble: MessageWriter<GamepadRumbleRequest>,
    transition: ResMut<Transition<AppState>>,
    mut player_q: Query<
        (Entity, &Transform, &mut Health, &mut Inventory, &mut Player),
        (With<Player>, Without<Enemy>),
    >,
    mut fire_q: Query<&mut FireCooldown, (With<Player>, Without<Enemy>)>,
    q: Query<
        (Entity, &Transform, &Team, &Health, Option<&Enemy>),
        (Without<Prop>, Without<Player>),
    >,
) {
    let (mut trauma, mut chroma, mut flash, mut hitstop, mut slow_mo) = effects;
    if run.game_over {
        return;
    }

    let Ok((player_e, player_tf, mut phealth, mut pinv, mut player)) = player_q.single_mut() else {
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
        run.total_kills += 1;
        score.0 += enemy.score;

        if score.0 > save.high_score {
            save.high_score = score.0;
            dirty.0 = true;
        }

        ScreenEffects::add_trauma(&mut trauma, 0.25);
        ScreenEffects::chromatic_pulse(&mut chroma, 0.08);
        hitstop.trigger(0.28, 0.055);

        let burst_count = if def.boss { 40 } else { 14 };
        VfxSpawner::spawn_burst(
            &mut commands,
            pos,
            burst_count,
            Color::srgb(0.9, 0.18, 0.1),
            (80.0, 260.0),
        );

        audio.play_hit(&mut commands);

        // Kill effects: Bloodlust heals, Lucky Shot grants ammo, Trigger
        // Fingers shortens the next reload.
        if player.bloodlust && rng.random_range(0..15) == 0 {
            phealth.hp = (phealth.hp + 2).min(phealth.max);
        }
        if player.lucky_shot && rng.random_range(0..10) == 0 {
            give_ammo(&mut pinv);
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
    // Player death (with Strong Spirit / Last Wish revives).
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
            for (slot, kind) in pinv.ammo.iter_mut().zip([
                AmmoKind::Bullets,
                AmmoKind::Shells,
                AmmoKind::Bolts,
                AmmoKind::Explosives,
                AmmoKind::Energy,
            ]) {
                // ammo array is 6 with index 0 unused; zip handles first 5 slots
                let _ = slot;
                let _ = kind;
            }
            // Refill all ammo types properly
            for kind in [
                AmmoKind::Bullets,
                AmmoKind::Shells,
                AmmoKind::Bolts,
                AmmoKind::Explosives,
                AmmoKind::Energy,
            ] {
                *pinv.ammo_mut(kind) = ammo_max(kind);
            }
            HitFlash::apply(&mut commands, player_e, Color::srgb(0.3, 1.0, 0.5), 0.3);
            audio.play_pickup(&mut commands);
            return;
        }

        run.game_over = true;
        commands.entity(player_e).despawn();

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
        dirty.0 = true;
        paused.0 = false;

        // Keep the game running in InGame so the death slow-mo plays; the
        // gameplay gate on `run.game_over` freezes actions. The UI overlay
        // handles retry/quit.
        let _ = transition;
    }
}

fn give_ammo(inv: &mut Inventory) {
    let id = inv.weapons[inv.current];
    if id == WeaponId::NONE {
        let mut rng = rand::rng();
        let kind = random_ammo_kind(&mut rng);
        let slot = inv.ammo_mut(kind);
        let add = ammo_pickup_amount(kind);
        *slot = (*slot + add).min(ammo_max(kind));
        return;
    }
    let def = crate::game::weapon_runtime::weapon_runtime_def(id);
    if def.melee.is_some() {
        return;
    }
    let slot = inv.ammo_mut(def.ammo);
    let add = ammo_pickup_amount(def.ammo);
    *slot = (*slot + add).min(ammo_max(def.ammo));
}

/// Sum of per-weapon ammo need factors (0.1 well-stocked .. 0.75 low).
fn scrub_need(inv: &Inventory) -> f32 {
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
        let cap = ammo_max(def.ammo) as f32;
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
    crate::game::pickups::spawn_pickup(
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

    let need = scrub_need(inv);
    let paw = player.drop_mult;
    let roll = rng.random_range(0.0..100.0);

    if roll < (chance as f32 * (need + paw)) {
        // Health: only when hurt, and only 2/3 of the time.
        if rng.random_range(0..health.max.max(1)) as i32 > health.hp && rng.random_range(0..3) < 2 {
            crate::game::pickups::spawn_pickup(
                commands,
                catalog,
                asset_server,
                PickupKind::Medkit(2),
                pos,
            );
        } else {
            let ammo = random_ammo_kind(&mut rng);
            crate::game::pickups::spawn_pickup(
                commands,
                catalog,
                asset_server,
                PickupKind::Ammo(ammo, ammo_pickup_amount(ammo)),
                pos,
            );
        }
    } else if weapon_chance > 0 && rng.random_range(0.0..100.0) < weapon_chance as f32 {
        let weapon = random_weapon(&mut rng);
        crate::game::pickups::spawn_pickup(
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

pub fn spawn_chest(
    commands: &mut Commands,
    catalog: &AssetCatalog,
    asset_server: &AssetServer,
    pos: Vec2,
) {
    crate::game::pickups::spawn_chest(commands, catalog, asset_server, ChestKind::Weapon, pos);
}
