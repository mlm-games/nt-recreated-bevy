use bevy::input::gamepad::{Gamepad, GamepadRumbleRequest};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use rand::RngExt;

use crate::game::audio::GameAudio;
use crate::game::components::*;
use crate::game::content::*;
use crate::game::environment::{PropDeathEffect, spawn_prop_corpse, spawn_prop_death_effect};
use crate::game::input::NtInput;
use crate::game::projectile_archetypes::{BeamSpec, ProjectileArchetype, projectile_archetype};
use crate::game::projectile_art;
use crate::game::secret_areas::SecretTriggers;
use crate::game::weapon_runtime::weapon_runtime_def;
use crate::game::world::*;
use game_utils_bevy::camera_follow::CameraFollow;
use game_utils_bevy::game_feel::{GameFeel, SlowMotion};

#[derive(bevy::ecs::system::SystemParam)]
pub struct MeleeTargets<'w, 's> {
    enemies: Query<
        'w,
        's,
        (
            Entity,
            &'static Transform,
            &'static mut Health,
            &'static Hitbox,
            Option<&'static mut Velocity>,
            Option<&'static mut NextHurt>,
        ),
        (With<Enemy>, Without<Player>),
    >,
    props: Query<
        'w,
        's,
        (
            Entity,
            &'static Transform,
            &'static mut Prop,
            Option<&'static PropDeathEffect>,
            Option<&'static PropSprites>,
            Option<&'static mut NextHurt>,
        ),
        (With<Prop>, Without<Player>),
    >,
    frame: Res<'w, CurrentFrame>,
}
use game_utils_bevy::hit_flash::HitFlash;
use game_utils_bevy::hitstop::HitStop;
use game_utils_bevy::juice::Juice;
use game_utils_bevy::screen_effects::CameraBase;
use game_utils_bevy::screen_effects::{ChromaticAberration, FlashWhite, ScreenEffects, Trauma};
use game_utils_bevy::vfx::VfxSpawner;

pub fn player_move(
    time: Res<Time<Fixed>>,
    mut commands: Commands,
    input: Res<NtInput>,
    mask: Res<FloorMask>,
    mut q: Query<
        (
            Entity,
            &Player,
            &mut Velocity,
            &mut Transform,
            Option<&mut Dash>,
            Option<&PortalSucking>,
        ),
        (With<Player>, Without<Prop>),
    >,
    props: Query<(Entity, &Prop, &Transform), With<Prop>>,
) {
    let Ok((entity, player, mut vel, mut tf, dash, sucking)) = q.single_mut() else {
        return;
    };
    if sucking.is_some() {
        vel.0 = Vec2::ZERO;
        return;
    }

    let dt = time.delta_secs();

    if let Some(mut dash) = dash {
        dash.timer.tick(time.delta());
        vel.0 = dash.dir * 950.0;
        tf.translation += (vel.0 * dt).extend(0.0);

        if dash.timer.just_finished() {
            commands.entity(entity).remove::<Dash>();
        }
    } else {

        if input.move_axis != Vec2::ZERO {

            let dir = input.move_axis.normalize_or_zero();
            vel.0 += dir * player.accel * dt;
        }
        let max_speed = player.speed * player.speed_mult;
        if vel.0.length() > max_speed {
            vel.0 = vel.0.normalize() * max_speed;
        }

        crate::game::components::apply_gml_friction(&mut vel.0, player.friction, dt);
        tf.translation += (vel.0 * dt).extend(0.0);
    }

    resolve_prop_collision(&mut tf.translation, PLAYER_RADIUS, &props);
    mask.resolve_circle(&mut tf.translation, PLAYER_RADIUS);
    clamp_to_arena(&mut tf.translation, PLAYER_RADIUS);
}

pub fn face_aim(mut q: Query<(&AimDir, &Velocity, &mut Sprite), With<Player>>) {
    let Ok((aim, vel, mut sprite)) = q.single_mut() else {
        return;
    };

    let x = if aim.0.length_squared() > 0.001 {
        aim.0.x
    } else {
        vel.0.x
    };
    sprite.flip_x = x < 0.0;
}

pub fn player_aim(
    input: Res<NtInput>,
    windows: Query<&Window, With<PrimaryWindow>>,
    camera_q: Query<(&Camera, &GlobalTransform, Option<&CameraBase>), With<Camera2d>>,
    mut player_q: Query<(&Transform, &mut AimDir), With<Player>>,
    mut follow_q: Query<&mut CameraFollow>,
) {
    let Ok((ptf, mut aim)) = player_q.single_mut() else {
        return;
    };
    let player_pos = ptf.translation.truncate();

    if input.aim_axis != Vec2::ZERO {
        aim.0 = input.aim_axis.normalize_or_zero();
        if let Ok(mut follow) = follow_q.single_mut() {

            const MAX_LOOK: f32 = 48.0;
            follow.set_aim(player_pos + aim.0 * MAX_LOOK);
        }
        return;
    }

    let Ok(window) = windows.single() else {
        return;
    };
    let Ok((camera, cam_gt, cam_base)) = camera_q.single() else {
        return;
    };

    let rest_gt = match cam_base {
        Some(base) => {
            let mut t = Transform::from_translation(base.translation);
            t.rotation = Quat::from_rotation_z(base.rotation);
            GlobalTransform::from(t)
        }
        None => *cam_gt,
    };

    if let Some(cursor) = window.cursor_position()
        && let Ok(world) = camera.viewport_to_world_2d(&rest_gt, cursor)
    {
        let dir = (world - player_pos).normalize_or_zero();
        if dir != Vec2::ZERO {
            aim.0 = dir;
        }
        if let Ok(mut follow) = follow_q.single_mut() {

            const MAX_LOOK: f32 = 48.0;
            let cam_xy = rest_gt.translation().truncate();
            let screen_off = (world - cam_xy).clamp_length_max(MAX_LOOK);
            follow.set_aim(player_pos + screen_off);
        }
    }
}

pub fn weapon_switch(
    mut input: ResMut<NtInput>,
    mut q: Query<&mut Inventory, With<Player>>,
    audio: Res<GameAudio>,
    mut commands: Commands,
) {
    let Ok(mut inv) = q.single_mut() else {
        return;
    };
    let mut switched = false;

    if let Some(slot) = input.take_weapon_slot()
        && slot < inv.weapon_slots
        && inv.weapons[slot] != WeaponId::NONE
        && slot != inv.current
    {
        inv.current = slot;
        switched = true;
    }

    let cycle = input.take_cycle_weapon();
    if cycle != 0 && inv.weapon_slots > 1 {
        let direction = if cycle > 0 { 1 } else { inv.weapon_slots - 1 };

        for step in 1..=inv.weapon_slots {
            let slot = (inv.current + step * direction) % inv.weapon_slots;
            if inv.weapons[slot] != WeaponId::NONE {
                switched |= slot != inv.current;
                inv.current = slot;
                break;
            }
        }
    }

    if switched {
        game_utils_bevy::audio::AudioM::play_sfx_varied(
            &mut commands,
            audio.pickup.clone(),
            0.25,
            0.05,
        );
    }
}

pub fn tick_player_timers(
    time: Res<Time<Fixed>>,
    mut q: Query<(
        &mut Player,
        &mut Health,
        Option<&mut Shield>,
        Option<&mut Telekinesis>,
    )>,
) {
    for (mut player, mut health, shield, telek) in &mut q {
        player.ability_cooldown.tick(time.delta());
        health.invuln.tick(time.delta());
        if let Some(mut s) = shield {
            s.timer.tick(time.delta());
        }
        if let Some(mut t) = telek {
            t.timer.tick(time.delta());
        }
    }
}

pub fn blink_player(time: Res<Time>, mut q: Query<(&Health, &mut Sprite), With<Player>>) {
    let Ok((health, mut sprite)) = q.single_mut() else {
        return;
    };
    if health.invuln.is_finished() {
        sprite.color.set_alpha(1.0);
    } else {
        let t = time.elapsed_secs() * 24.0;
        sprite.color.set_alpha(0.25 + 0.55 * (0.5 + 0.5 * t.sin()));
    }
}

pub fn player_ability(
    mut input: ResMut<NtInput>,
    mut commands: Commands,
    mut trauma: ResMut<Trauma>,
    mut chroma: ResMut<ChromaticAberration>,
    mut slow_mo: ResMut<SlowMotion>,
    mut hitstop: ResMut<HitStop>,
    audio: Res<GameAudio>,
    catalog: Res<AssetCatalog>,
    asset_server: Res<AssetServer>,
    gamepads: Query<(Entity, &Gamepad)>,
    mut rumble: MessageWriter<GamepadRumbleRequest>,
    mut q: Query<
        (
            Entity,
            &mut Player,
            &mut Health,
            &mut Velocity,
            &Transform,
            &mut AimDir,
            &mut Inventory,
            &RaceState,
            Option<&mut Shield>,
            Option<&mut Telekinesis>,
        ),
        (With<Player>, Without<Enemy>),
    >,
    mut enemies: Query<(Entity, &Transform, &mut Health), (With<Enemy>, Without<Player>)>,
    mut save: ResMut<crate::save::SaveData>,
    mut dirty: ResMut<SaveDirty>,
    mut toast: ResMut<Toast>,
) {
    let Ok((
        player_e,
        mut player,
        mut health,
        mut vel,
        tf,
        aim,
        mut inv,
        race_state,
        shield,
        telek,
    )) = q.single_mut()
    else {
        return;
    };

    let fire = input.take_ability_pressed();
    if race_state.race == RaceId::Steroids {
        return;
    }
    if !fire || !player.ability_cooldown.is_finished() {
        return;
    }

    let pos = tf.translation.truncate();
    let ability = player.ability;

    let cd_frames: f32 = match ability {
        AbilityKind::Flip => 105.0,
        AbilityKind::Shield => 150.0,
        AbilityKind::Telekinesis => 135.0,
        AbilityKind::Detonate => 165.0,
        AbilityKind::Snare => 120.0,
        AbilityKind::PopPop => 90.0,
        AbilityKind::GetLoaded => 210.0,
        AbilityKind::EatWeapon => 30.0,
        AbilityKind::Throw => 120.0,
        AbilityKind::SpawnAlly => 240.0,
        AbilityKind::HorrorBeam => 150.0,
        AbilityKind::PortalStrike => 180.0,
        AbilityKind::RocketBarrage => 210.0,
        AbilityKind::BloodGamble => 120.0,
        AbilityKind::ToxicPuke => 150.0,
        AbilityKind::CuzSwap => 12.0,
    };
    let cd = cd_frames / 30.0;

    let ability_mult = if player.throne_butt {
        player.ultra_ability_mult * 1.35
    } else {
        player.ultra_ability_mult
    };

    match ability {
        AbilityKind::Flip => {
            let dir = if input.move_axis != Vec2::ZERO {
                input.move_axis.normalize()
            } else {
                aim.0
            };
            commands.entity(player_e).insert(Dash {
                timer: Timer::from_seconds(0.18 * ability_mult.clamp(1.0, 1.6), TimerMode::Once),
                dir,
            });
            health.invuln = Timer::from_seconds(15.0 / 30.0, TimerMode::Once);
            vel.0 = dir * 900.0;
            player.ability_cooldown = Timer::from_seconds(cd, TimerMode::Once);
            ScreenEffects::add_trauma(&mut trauma, 0.12);
            GameFeel::slow_motion(&mut slow_mo, 0.55, 0.2);
            VfxSpawner::spawn_burst(
                &mut commands,
                pos,
                6,
                Color::srgb(0.3, 0.9, 1.0),
                (60.0, 160.0),
            );
            GameFeel::rumble_controller(&mut rumble, &gamepads, 0.2, 0.2, 0.1);
            audio.play_bolt(&mut commands);
        }
        AbilityKind::Shield => {
            let timer = Timer::from_seconds(1.6 * ability_mult.clamp(1.0, 2.0), TimerMode::Once);
            if let Some(mut s) = shield {
                s.timer = timer;
            } else {
                commands.entity(player_e).insert(Shield { timer });
            }
            player.ability_cooldown = Timer::from_seconds(cd, TimerMode::Once);
            ScreenEffects::add_trauma(&mut trauma, 0.08);
            VfxSpawner::spawn_burst(
                &mut commands,
                pos,
                10,
                Color::srgb(0.3, 0.65, 1.0),
                (80.0, 200.0),
            );
            audio.play_pickup(&mut commands);
        }
        AbilityKind::Telekinesis => {
            let timer = Timer::from_seconds(1.4, TimerMode::Once);
            if let Some(mut t) = telek {
                t.timer = timer;
            } else {
                commands.entity(player_e).insert(Telekinesis { timer });
            }
            player.ability_cooldown = Timer::from_seconds(cd, TimerMode::Once);
            VfxSpawner::spawn_burst(
                &mut commands,
                pos,
                14,
                Color::srgb(0.85, 0.4, 1.0),
                (100.0, 260.0),
            );
            audio.play_portal(&mut commands);
        }
        AbilityKind::Detonate => {
            if health.hp <= 1 {
                return;
            }
            health.hp -= 1;
            player.ability_cooldown = Timer::from_seconds(cd, TimerMode::Once);
            let radius = 150.0 * ability_mult.clamp(1.0, 2.0);
            let damage = (3.0 * ability_mult).round() as i32;
            for (_, etf, mut ehealth) in &mut enemies {
                if etf.translation.truncate().distance(pos) < radius {
                    ehealth.hp -= damage;
                }
            }
            ScreenEffects::add_trauma(&mut trauma, 0.5);
            ScreenEffects::chromatic_pulse(&mut chroma, 0.4);
            GameFeel::rumble_controller(&mut rumble, &gamepads, 0.6, 0.8, 0.25);
            VfxSpawner::spawn_burst(
                &mut commands,
                pos,
                40,
                Color::srgb(1.0, 0.5, 0.15),
                (140.0, 420.0),
            );
            hitstop.trigger(0.25, 0.12);
            audio.play_boom(&mut commands);
        }
        AbilityKind::Snare => {
            player.ability_cooldown = Timer::from_seconds(cd, TimerMode::Once);
            commands.spawn((
                LevelCleanup,
                SnareZone {
                    timer: Timer::from_seconds(2.5 * ability_mult.clamp(1.0, 2.0), TimerMode::Once),
                    radius: 110.0 * ability_mult.clamp(1.0, 1.8),
                    slow: (0.35 / ability_mult).clamp(0.12, 0.35),
                },
                Transform::from_translation((pos + aim.0 * 70.0).extend(5.0)),
                Sprite {
                    color: Color::srgba(0.3, 0.9, 0.35, 0.35),
                    custom_size: Some(Vec2::splat(220.0)),
                    ..default()
                },
            ));
            audio.play_pickup(&mut commands);
        }
        AbilityKind::PopPop => {
            player.ability_cooldown = Timer::from_seconds(cd, TimerMode::Once);
            let charges = if player.throne_butt
                || matches!(player.ultra, Some(UltraMutationId::VenuzBack2Bizniz))
            {
                2
            } else {
                1
            };
            commands.entity(player_e).insert(PopPopCharges(charges));
            VfxSpawner::spawn_burst(
                &mut commands,
                pos,
                10,
                Color::srgb(0.95, 0.85, 0.2),
                (80.0, 200.0),
            );
            audio.play_bolt(&mut commands);
        }
        AbilityKind::GetLoaded => {
            player.ability_cooldown = Timer::from_seconds(cd, TimerMode::Once);
            for slot in 0..inv.weapon_slots {
                let w = inv.weapons[slot];
                if w == WeaponId::NONE {
                    continue;
                }
                let kind = weapon_ammo(w);
                if kind == AmmoKind::None {
                    continue;
                }
                let add = match kind {
                    AmmoKind::Bullets => 32,
                    AmmoKind::Shells => 8,
                    AmmoKind::Bolts => 6,
                    AmmoKind::Explosives => 4,
                    AmmoKind::Energy => 10,
                    AmmoKind::None => 0,
                };
                let add = ((add as f32) * ability_mult).round() as i32;
                let slot = inv.ammo_mut(kind);
                *slot = (*slot + add).min(player.ammo_cap(kind));
            }
            VfxSpawner::spawn_burst(
                &mut commands,
                pos,
                12,
                Color::srgb(0.95, 0.3, 0.25),
                (80.0, 200.0),
            );
            audio.play_pickup(&mut commands);
        }
        AbilityKind::EatWeapon => {
            let slot = inv.current;
            let w = inv.weapons[slot];
            if w == WeaponId::NONE {
                return;
            }
            player.ability_cooldown = Timer::from_seconds(cd, TimerMode::Once);
            inv.weapons[slot] = WeaponId::NONE;
            if let Some(next) = (0..inv.weapon_slots).find(|&i| inv.weapons[i] != WeaponId::NONE) {
                inv.current = next;
            }
            let regurgitate = matches!(player.ultra, Some(UltraMutationId::RobotRegurgitate));
            health.hp = (health.hp + if regurgitate { 3 } else { 2 }).min(health.max);
            player.rads = player
                .rads
                .saturating_add(if regurgitate { 40 } else { 20 });

            let unlocked = crate::game::generated::unlocks::check_progress_unlocks(
                &mut save, 0, 0, false, true, false,
            );
            for race in unlocked {
                dirty.0 = true;
                toast.show(&format!(
                    "UNLOCKED {}",
                    character_def(race).name.to_ascii_uppercase()
                ));
            }
            VfxSpawner::spawn_burst(
                &mut commands,
                pos,
                16,
                Color::srgb(0.6, 0.7, 0.75),
                (90.0, 220.0),
            );
            audio.play_pickup(&mut commands);
        }
        AbilityKind::Throw => {
            let dir = if input.move_axis != Vec2::ZERO {
                input.move_axis.normalize()
            } else {
                aim.0
            };

            let slot = inv.current;
            let held = inv.weapons[slot];

            if held == WeaponId::NONE {
                player.ability_cooldown = Timer::from_seconds(cd, TimerMode::Once);
                commands.entity(player_e).insert(Dash {
                    timer: Timer::from_seconds(0.14, TimerMode::Once),
                    dir,
                });
                health.hp = (health.hp + 1).min(health.max);
                health.invuln = Timer::from_seconds(0.35, TimerMode::Once);
                vel.0 = dir * 800.0;
                audio.play_bolt(&mut commands);
                return;
            }

            player.ability_cooldown = Timer::from_seconds(cd, TimerMode::Once);

            inv.weapons[slot] = WeaponId::NONE;
            if let Some(next) = (0..inv.weapon_slots).find(|&i| inv.weapons[i] != WeaponId::NONE) {
                inv.current = next;
            }

            let thrown_def = weapon_runtime_def(held);
            let damage = (thrown_def.damage.max(6)) * 2;

            spawn_player_projectile_with_source(
                &mut commands,
                Some(&catalog),
                Some(&asset_server),
                pos + dir * 18.0,
                dir,
                520.0,
                damage,
                1.2,
                8.0,
                180.0,
                false,
                thrown_def.color,
                Vec2::new(14.0, 6.0),
                1,
                0,
                None,
                None,
                ProjectileArchetype {
                    spawn_weapon_pickup: Some(SpawnsWeaponPickup { weapon: Some(held) }),
                    ..ProjectileArchetype::default()
                },
                Some(DamageSource::player_weapon(player_e, held)),
                Some(held),
            );

            health.invuln = Timer::from_seconds(0.25, TimerMode::Once);
            vel.0 = dir * 220.0;
            audio.play_melee(&mut commands);
        }
        AbilityKind::SpawnAlly => {
            player.ability_cooldown = Timer::from_seconds(cd, TimerMode::Once);
            let ally_count = if matches!(player.ultra, Some(UltraMutationId::RebelRiot)) {
                3
            } else if player.throne_butt {
                2
            } else {
                1
            };
            for i in 0..ally_count {
                let side = Vec2::new(-aim.0.y, aim.0.x)
                    * ((i as f32) - (ally_count as f32 - 1.0) * 0.5)
                    * 22.0;
                let spawn_at = pos + aim.0 * 28.0 + side;
                commands.spawn((
                    LevelCleanup,
                    Ally {
                        life: Timer::from_seconds(
                            12.0 * ability_mult.clamp(1.0, 2.0),
                            TimerMode::Once,
                        ),
                        shoot: Timer::from_seconds(
                            (0.35 / ability_mult).clamp(0.15, 0.35),
                            TimerMode::Repeating,
                        ),
                    },
                    Team::Player,
                    Health {
                        hp: 8,
                        max: 8,
                        invuln: Timer::from_seconds(0.5, TimerMode::Once),
                    },
                    Hitbox { radius: 10.0 },
                    Velocity(Vec2::ZERO),
                    Transform::from_translation(spawn_at.extend(18.0)),
                    Sprite {
                        color: Color::srgb(0.85, 0.25, 0.55),
                        custom_size: Some(Vec2::splat(18.0)),
                        ..default()
                    },
                ));
            }
            audio.play_portal(&mut commands);
        }
        AbilityKind::HorrorBeam => {
            player.ability_cooldown = Timer::from_seconds(cd, TimerMode::Once);
            let dir = aim.0.normalize_or_zero();
            let beam_len = 320.0 * ability_mult.clamp(1.0, 1.8);
            let beam_damage = (4.0 * ability_mult).round() as i32;
            let beam_width = 22.0 * ability_mult.sqrt();
            for (_, etf, mut ehealth) in &mut enemies {
                let to = etf.translation.truncate() - pos;
                let proj = to.dot(dir);
                if proj < 0.0 || proj > beam_len {
                    continue;
                }
                let lateral = (to - dir * proj).length();
                if lateral < beam_width {
                    ehealth.hp -= beam_damage;
                }
            }
            commands.spawn((
                LevelCleanup,
                AbilityHazard,
                HazardCloud {
                    kind: HazardKind::Toxic,
                    radius: 28.0,
                    damage: 1,
                    timer: Timer::from_seconds(0.8, TimerMode::Once),
                    tick: Timer::from_seconds(0.15, TimerMode::Repeating),
                },
                Transform::from_translation((pos + dir * 160.0).extend(6.0)),
                Sprite {
                    color: Color::srgba(0.55, 0.3, 0.95, 0.4),
                    custom_size: Some(Vec2::new(320.0, 36.0)),
                    ..default()
                },
            ));
            ScreenEffects::add_trauma(&mut trauma, 0.18);
            audio.play_bolt(&mut commands);
        }
        AbilityKind::PortalStrike => {
            player.ability_cooldown = Timer::from_seconds(cd, TimerMode::Once);
            let target = pos + aim.0.normalize_or_zero() * 180.0;
            commands.spawn((
                LevelCleanup,
                PortalStrike {
                    timer: Timer::from_seconds(
                        (0.55 / ability_mult).clamp(0.2, 0.55),
                        TimerMode::Once,
                    ),
                    radius: 90.0 * ability_mult.clamp(1.0, 2.0),
                    damage: (8.0 * ability_mult).round() as i32,
                },
                Transform::from_translation(target.extend(8.0)),
                Sprite {
                    color: Color::srgba(0.3, 0.9, 1.0, 0.45),
                    custom_size: Some(Vec2::splat(40.0)),
                    ..default()
                },
            ));
            audio.play_portal(&mut commands);
        }
        AbilityKind::RocketBarrage => {
            player.ability_cooldown = Timer::from_seconds(cd, TimerMode::Once);
            let base = aim.0.normalize_or_zero();
            let rockets = if matches!(player.ultra, Some(UltraMutationId::BigDogHeavyArtillery)) {
                -3..=3
            } else {
                -2..=2
            };
            let rocket_path = "images/sprRocket.png";
            let rocket_sprite = if catalog.has(rocket_path) {
                let mut s = sprite_exact(&catalog, &asset_server, rocket_path);
                s.custom_size = Some(Vec2::splat(10.0));
                s.color = Color::WHITE;
                s
            } else {
                Sprite {
                    color: Color::srgb(1.0, 0.55, 0.15),
                    custom_size: Some(Vec2::splat(10.0)),
                    ..default()
                }
            };
            for i in rockets {
                let ang = (i as f32) * 0.12;
                let dir = Vec2::new(
                    base.x * ang.cos() - base.y * ang.sin(),
                    base.x * ang.sin() + base.y * ang.cos(),
                )
                .normalize_or_zero();
                commands.spawn((
                    LevelCleanup,
                    Projectile {
                        damage: 3,
                        life: Timer::from_seconds(0.9, TimerMode::Once),
                        radius: 6.0,
                        knockback: 40.0,
                        explosive: true,
                        source: Some(DamageSource::player_weapon(
                            player_e,
                            WeaponId::GRENADE_LAUNCHER,
                        )),
                    },
                    Team::Player,
                    Velocity(dir * 420.0),
                    Transform::from_translation(pos.extend(12.0)),
                    rocket_sprite.clone(),
                ));
            }
            ScreenEffects::add_trauma(&mut trauma, 0.25);
            audio.play_boom(&mut commands);
        }
        AbilityKind::BloodGamble => {
            if health.hp <= 1 {
                return;
            }
            player.ability_cooldown = Timer::from_seconds(cd, TimerMode::Once);
            health.hp -= 1;
            let roll = [
                WeaponId::REVOLVER,
                WeaponId::SHOTGUN,
                WeaponId::CROSSBOW,
                WeaponId::MACHINEGUN,
                WeaponId::SMG,
                WeaponId::GRENADE_LAUNCHER,
                WeaponId::ASSAULT_RIFLE,
                WeaponId::WRENCH,
            ];
            let idx = (pos.x.abs() as usize + player.rads as usize) % roll.len();
            let cur = inv.current;
            inv.weapons[cur] = roll[idx];
            VfxSpawner::spawn_burst(
                &mut commands,
                pos,
                14,
                Color::srgb(0.95, 0.95, 0.95),
                (70.0, 180.0),
            );
            audio.play_pickup(&mut commands);
        }
        AbilityKind::ToxicPuke => {
            player.ability_cooldown = Timer::from_seconds(cd, TimerMode::Once);
            let spot = pos + aim.0.normalize_or_zero() * 48.0;
            commands.spawn((
                LevelCleanup,
                AbilityHazard,
                HazardCloud {
                    kind: HazardKind::Toxic,
                    radius: 70.0 * ability_mult.clamp(1.0, 2.0),
                    damage: ((1.0 * ability_mult).ceil() as i32).max(1),
                    timer: Timer::from_seconds(3.0 * ability_mult.clamp(1.0, 1.8), TimerMode::Once),
                    tick: Timer::from_seconds(0.25, TimerMode::Repeating),
                },
                Transform::from_translation(spot.extend(5.0)),
                Sprite {
                    color: Color::srgba(0.35, 0.85, 0.4, 0.4),
                    custom_size: Some(Vec2::splat(140.0)),
                    ..default()
                },
            ));
            audio.play_boom(&mut commands);
        }
        AbilityKind::CuzSwap => {
            let swap_cd = if matches!(player.ultra, Some(UltraMutationId::CuzQuickSwap)) {
                0.15
            } else if player.throne_butt {
                0.25
            } else {
                cd
            };
            player.ability_cooldown = Timer::from_seconds(swap_cd, TimerMode::Once);
            let n = inv.weapon_slots;
            for step in 1..=n {
                let slot = (inv.current + step) % n;
                if inv.weapons[slot] != WeaponId::NONE {
                    inv.current = slot;
                    break;
                }
            }
            audio.play_pickup(&mut commands);
        }
    }
}

pub fn player_fire(
    time: Res<Time<Fixed>>,
    mut input: ResMut<NtInput>,
    mut commands: Commands,
    mut trauma: ResMut<Trauma>,
    mut hitstop: ResMut<HitStop>,
    audio: Res<GameAudio>,
    catalog: Res<AssetCatalog>,
    asset_server: Res<AssetServer>,
    mut toast: ResMut<Toast>,
    mut player_q: Query<
        (
            Entity,
            &Transform,
            &AimDir,
            &mut Player,
            &mut Health,
            &RaceState,
            Option<&PortalSucking>,
        ),
        With<Player>,
    >,
    mut fire_q: Query<(&mut FireCooldown, &mut Inventory, &mut Velocity), With<Player>>,
    mut vis_q: Query<&mut WeaponVisual>,
    mut pop_q: Query<&mut PopPopCharges>,
    mut targets: MeleeTargets,
    gamepads: Query<(Entity, &Gamepad)>,
    mut rumble: MessageWriter<GamepadRumbleRequest>,
) {
    let Ok((player_ent, tf, aim, mut player, mut health, race_state, sucking)) =
        player_q.single_mut()
    else {
        return;
    };
    if sucking.is_some() {
        return;
    }
    let Ok((mut cooldown, mut inv, mut vel)) = fire_q.single_mut() else {
        return;
    };

    let is_steroids = race_state.race == RaceId::Steroids;

    cooldown.timer.tick(time.delta());
    cooldown.burst_timer.tick(time.delta());
    cooldown.timer_b.tick(time.delta());
    cooldown.burst_timer_b.tick(time.delta());

    let fire_held = input.fire_held;
    let fire_pressed = input.take_fire_pressed();
    let spec_held = input.spec_held;
    let spec_pressed = input.take_spec_pressed();

    let primary_id = inv.weapons[inv.current];
    let primary_def = weapon_runtime_def(primary_id);

    // Second slot mirrors the other live slot.
    let second_slot = steroids_secondary_slot(inv.current, inv.weapon_slots);
    let secondary_id = if is_steroids {
        inv.weapons[second_slot]
    } else {
        WeaponId::NONE
    };
    let secondary_def = weapon_runtime_def(secondary_id);

    if cooldown.burst_left > 0 && cooldown.burst_timer.is_finished() && primary_id != WeaponId::NONE
    {
        fire_burst_volley(
            &mut commands,
            &mut trauma,
            &mut hitstop,
            &audio,
            &mut rumble,
            &gamepads,
            &catalog,
            &asset_server,
            &mut pop_q,
            player_ent,
            tf,
            aim,
            &*player,
            primary_id,
            &primary_def,
        );
        cooldown.burst_left -= 1;
        cooldown.burst_timer = Timer::from_seconds(primary_def.burst_interval, TimerMode::Once);
    }
    if is_steroids
        && cooldown.burst_left_b > 0
        && cooldown.burst_timer_b.is_finished()
        && secondary_id != WeaponId::NONE
    {
        fire_burst_volley(
            &mut commands,
            &mut trauma,
            &mut hitstop,
            &audio,
            &mut rumble,
            &gamepads,
            &catalog,
            &asset_server,
            &mut pop_q,
            player_ent,
            tf,
            aim,
            &*player,
            secondary_id,
            &secondary_def,
        );
        cooldown.burst_left_b -= 1;
        cooldown.burst_timer_b = Timer::from_seconds(secondary_def.burst_interval, TimerMode::Once);
    }

    let primary_intent = if primary_def.automatic || is_steroids {
        fire_held
    } else {
        fire_pressed
    };

    let secondary_intent = is_steroids && spec_held && secondary_id != WeaponId::NONE;

    if primary_id != WeaponId::NONE && primary_intent && cooldown.timer.is_finished() {
        fire_one_gun(
            &mut commands,
            &mut trauma,
            &mut hitstop,
            &audio,
            &mut rumble,
            &gamepads,
            &catalog,
            &asset_server,
            &mut toast,
            &mut pop_q,
            &mut targets,
            &mut vis_q,
            player_ent,
            tf,
            aim,
            &mut player,
            &mut inv,
            &mut health,
            &mut vel,
            &mut cooldown,
            primary_id,
            &primary_def,
            0,
        );
    }

    if secondary_intent && cooldown.timer_b.is_finished() {

        let secondary_def = weapon_runtime_def(secondary_id);
        fire_one_gun(
            &mut commands,
            &mut trauma,
            &mut hitstop,
            &audio,
            &mut rumble,
            &gamepads,
            &catalog,
            &asset_server,
            &mut toast,
            &mut pop_q,
            &mut targets,
            &mut vis_q,
            player_ent,
            tf,
            aim,
            &mut player,
            &mut inv,
            &mut health,
            &mut vel,
            &mut cooldown,
            secondary_id,
            &secondary_def,
            1,
        );
    }

    let _ = spec_pressed;
}

#[allow(clippy::too_many_arguments)]
fn fire_burst_volley(
    commands: &mut Commands,
    trauma: &mut Trauma,
    hitstop: &mut HitStop,
    audio: &GameAudio,
    rumble: &mut MessageWriter<GamepadRumbleRequest>,
    gamepads: &Query<(Entity, &Gamepad)>,
    catalog: &AssetCatalog,
    asset_server: &AssetServer,
    pop_q: &mut Query<&mut PopPopCharges>,
    player_ent: Entity,
    tf: &Transform,
    aim: &AimDir,
    player: &Player,
    weapon_id: WeaponId,
    def: &WeaponDef,
) {
    spawn_pellets(
        commands,
        trauma,
        hitstop,
        audio,
        rumble,
        gamepads,
        catalog,
        asset_server,
        player_ent,
        tf,
        aim,
        player,
        weapon_id,
        def,
    );
    if let Ok(mut charges) = pop_q.get_mut(player_ent) {
        if charges.0 > 0 {
            charges.0 -= 1;
            spawn_pellets(
                commands,
                trauma,
                hitstop,
                audio,
                rumble,
                gamepads,
                catalog,
                asset_server,
                player_ent,
                tf,
                aim,
                player,
                weapon_id,
                def,
            );
            if charges.0 == 0 {
                commands.entity(player_ent).remove::<PopPopCharges>();
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn fire_one_gun(
    commands: &mut Commands,
    trauma: &mut Trauma,
    hitstop: &mut HitStop,
    audio: &GameAudio,
    rumble: &mut MessageWriter<GamepadRumbleRequest>,
    gamepads: &Query<(Entity, &Gamepad)>,
    catalog: &AssetCatalog,
    asset_server: &AssetServer,
    toast: &mut Toast,
    pop_q: &mut Query<&mut PopPopCharges>,
    targets: &mut MeleeTargets,
    vis_q: &mut Query<&mut WeaponVisual>,
    player_ent: Entity,
    tf: &Transform,
    aim: &AimDir,
    player: &mut Player,
    inv: &mut Inventory,
    health: &mut Health,
    vel: &mut Velocity,
    cooldown: &mut FireCooldown,
    weapon_id: WeaponId,
    def: &WeaponDef,
    visual_slot: u8,
) {
    let archetype = projectile_archetype(weapon_id);

    if def.rad_cost > 0 && player.rads < def.rad_cost {
        toast.show("NOT ENOUGH RADS");
        audio.play_ultra_empty(commands);
        for mut wv in vis_q.iter_mut() {
            if wv.owner == player_ent && wv.slot == visual_slot {
                wv.wkick = -2.0;
            }
        }
        return;
    }

    if def.melee.is_none() {
        match pay_fire_cost(inv, health, def.ammo, def.ammo_cost, archetype.blood_ammo) {
            AmmoPayment::Paid => {}
            AmmoPayment::Blood(cost) => {
                VfxSpawner::spawn_damage_number(
                    commands,
                    cost,
                    tf.translation.truncate(),
                    Color::srgb(1.0, 0.35, 0.35),
                );
            }
            AmmoPayment::Failed => {
                if inv.ammo_of(def.ammo) > 0 {
                    toast.show("NOT ENOUGH AMMO");
                } else {
                    toast.show("EMPTY");
                }
                audio.play_empty(commands);
                for mut wv in vis_q.iter_mut() {
                    if wv.owner == player_ent && wv.slot == visual_slot {
                        wv.wkick = -2.0;
                    }
                }
                return;
            }
        }
        if def.rad_cost > 0 {
            player.rads = player.rads.saturating_sub(def.rad_cost);
        }
    }

    let stress_bonus = if player.stress {
        (1.0 - health.hp as f32 / health.max.max(1) as f32).max(0.0)
    } else {
        0.0
    };
    let cd = def.cooldown * player.fire_rate_mult / (1.0 + stress_bonus);

    let timer = if visual_slot == 0 {
        &mut cooldown.timer
    } else {
        &mut cooldown.timer_b
    };
    *timer = Timer::from_seconds(cd.max(0.03), TimerMode::Once);

    if let Some(melee) = def.melee {
        melee_attack(
            commands,
            trauma,
            hitstop,
            audio,
            rumble,
            gamepads,
            player_ent,
            tf,
            aim,
            player,
            vel,
            def,
            melee,
            targets,
            catalog,
            asset_server,
        );
        return;
    }

    spawn_pellets(
        commands,
        trauma,
        hitstop,
        audio,
        rumble,
        gamepads,
        catalog,
        asset_server,
        player_ent,
        tf,
        aim,
        player,
        weapon_id,
        def,
    );
    vel.0 -= aim.0.normalize_or_zero() * def.recoil * 18.0;
    for mut wv in vis_q.iter_mut() {
        if wv.owner == player_ent && wv.slot == visual_slot {
            wv.wkick = -def.recoil.max(2.0) * 1.5;
        }
    }
    if player.recycle_gland
        && def.ammo == AmmoKind::Bullets
        && def.melee.is_none()
        && rand::rng().random_range(0..5) == 0
    {
        let slot = inv.ammo_mut(AmmoKind::Bullets);
        *slot = (*slot + 1).min(player.ammo_cap(AmmoKind::Bullets));
    }

    if let Ok(mut charges) = pop_q.get_mut(player_ent) {
        if charges.0 > 0 {
            charges.0 -= 1;
            spawn_pellets(
                commands,
                trauma,
                hitstop,
                audio,
                rumble,
                gamepads,
                catalog,
                asset_server,
                player_ent,
                tf,
                aim,
                player,
                weapon_id,
                def,
            );
            if charges.0 == 0 {
                commands.entity(player_ent).remove::<PopPopCharges>();
            }
        }
    }

    if def.burst_shots > 1 {
        if visual_slot == 0 {
            cooldown.burst_left = def.burst_shots - 1;
            cooldown.burst_timer = Timer::from_seconds(def.burst_interval, TimerMode::Once);
        } else {
            cooldown.burst_left_b = def.burst_shots - 1;
            cooldown.burst_timer_b = Timer::from_seconds(def.burst_interval, TimerMode::Once);
        }
    }
}

fn apply_weapon_mutation_mods(
    def: &mut WeaponDef,
    archetype: &mut ProjectileArchetype,
    player: &Player,
) {
    def.damage = ((def.damage as f32) * player.ultra_damage_mult).round() as i32;

    if player.laser_brain && def.ammo == AmmoKind::Energy && def.melee.is_none() {
        def.damage = ((def.damage as f32) * 1.35).round() as i32;
        def.speed *= 1.15;
        def.size *= 1.15;
        def.projectile_radius *= 1.15;
    }

    if player.shotgun_shoulders && def.ammo == AmmoKind::Shells && def.melee.is_none() {
        def.bounces = def.bounces.max(5);
        def.lifetime *= 1.25;
    }

    if player.bolt_marrow && def.ammo == AmmoKind::Bolts && def.melee.is_none() {
        archetype.homing = Some(archetype.homing.unwrap_or(Homing {
            turn_rate: 7.0,
            acquire_range: 420.0,
        }));
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_pellets(
    commands: &mut Commands,
    trauma: &mut Trauma,
    hitstop: &mut HitStop,
    audio: &GameAudio,
    rumble: &mut MessageWriter<GamepadRumbleRequest>,
    gamepads: &Query<(Entity, &Gamepad)>,
    catalog: &AssetCatalog,
    asset_server: &AssetServer,
    player_ent: Entity,
    tf: &Transform,
    aim: &AimDir,
    player: &Player,
    id: WeaponId,
    def: &WeaponDef,
) {
    let sleep = crate::game::weapon_runtime::weapon_sleep_secs(id);
    if sleep > 0.0 {
        hitstop.trigger((sleep * 8.0).clamp(0.15, 0.85), sleep);
    }
    ScreenEffects::add_trauma(trauma, def.shake);
    GameFeel::rumble_controller(rumble, gamepads, 0.08, def.shake, 0.07);

    let kind: WeaponKind = id.into();

    let legacy_fallback = matches!(
        kind,
        WeaponKind::Revolver
            | WeaponKind::Machinegun
            | WeaponKind::Smg
            | WeaponKind::AssaultRifle
            | WeaponKind::Shotgun
            | WeaponKind::Crossbow
            | WeaponKind::GrenadeLauncher
    );
    if legacy_fallback {
        match kind {
            WeaponKind::Revolver => {
                audio.play_shoot(commands);
            }
            WeaponKind::Machinegun | WeaponKind::Smg | WeaponKind::AssaultRifle => {
                audio.play_machine(commands);
            }
            WeaponKind::Shotgun => audio.play_shotgun(commands),
            WeaponKind::Crossbow => audio.play_bolt(commands),
            WeaponKind::GrenadeLauncher => audio.play_explode(commands),
            _ => {
                audio.play_weapon_fire(commands, def.name);
            }
        }
    } else {
        audio.play_weapon_fire(commands, def.name);
    }

    let muzzle = tf.translation.truncate() + aim.0 * 24.0;
    if def.muzzle_burst > 0 {
        VfxSpawner::spawn_burst(
            commands,
            muzzle,
            def.muzzle_burst,
            Color::srgb(1.0, 0.85, 0.25),
            (40.0, 120.0),
        );
    }

    let mut archetype = projectile_archetype(id);
    let mut def = *def;
    apply_weapon_mutation_mods(&mut def, &mut archetype, player);

    if let Some(beam) = archetype.beam {
        spawn_beam_shot(
            commands,
            muzzle,
            aim.0.normalize_or_zero(),
            beam,
            Some(DamageSource::player_weapon(player_ent, id)),
        );
        return;
    }

    if let Some(sentry) = archetype.deploys_sentry {
        spawn_player_projectile_with_source(
            commands,
            Some(catalog),
            Some(asset_server),
            muzzle,
            aim.0.normalize_or_zero(),
            260.0,
            0,
            0.9,
            6.0,
            0.0,
            false,
            def.color,
            Vec2::splat(10.0),
            0,
            0,
            None,
            None,
            ProjectileArchetype {
                deploys_sentry: Some(sentry),
                ..ProjectileArchetype::default()
            },
            Some(DamageSource::player_weapon(player_ent, id)),
            Some(id),
        );
        return;
    }

    let mut rng = rand::rng();
    let spread = def.spread * player.spread_mult * player.accuracy;

    let pierce = if archetype.chain_lightning.is_some() {
        0
    } else {
        def.pierce
    };
    for _ in 0..def.pellets {
        let base_angle = aim.0.y.atan2(aim.0.x);
        let angle = base_angle + rng.random_range(-spread..spread);
        let dir = Vec2::new(angle.cos(), angle.sin());
        let speed = if def.ammo == AmmoKind::Shells {
            rng.random_range(360.0..540.0)
        } else {
            def.speed
        };
        spawn_player_projectile_with_source(
            commands,
            Some(catalog),
            Some(asset_server),
            muzzle,
            dir,
            speed,
            def.damage,
            def.lifetime,
            def.projectile_radius,
            def.knockback * player.knockback_mult,
            def.explosive,
            def.color,
            def.size,
            def.bounces,
            pierce,
            def.hazard,
            def.split,
            archetype,
            Some(DamageSource::player_weapon(player_ent, id)),
            Some(id),
        );
    }

    if def.ammo == AmmoKind::Bullets && def.melee.is_none() {
        let shell_path = "images/sprBulletShell.png";
        if catalog.has(shell_path) {

            let shells = if matches!(id.0, 2 | 83 | 49) {
                def.pellets
            } else {
                1
            };
            for _ in 0..shells {
                let mut rng2 = rand::rng();
                let shell_sprite = sprite_exact(catalog, asset_server, shell_path);
                let anchor = crate::game::content::sprite_anchor(catalog, shell_path);
                let right = if aim.0.x >= 0.0 { 1.0 } else { -1.0 };
                let shell_angle = aim.0.y.atan2(aim.0.x)
                    + right * 100_f32.to_radians()
                    + rng2.random_range(-25_f32..25.0).to_radians();
                let shell_dir = Vec2::new(shell_angle.cos(), shell_angle.sin());
                let shell_speed = rng2.random_range(50.0..90.0);
                let shell_vel = shell_dir * shell_speed + Vec2::new(0.0, -20.0);
                commands.spawn((
                    GameCleanup,
                    LevelCleanup,
                    shell_sprite,
                    anchor,
                    Transform::from_translation(muzzle.extend(9.0)),
                    GroundPhysics {
                        vel: shell_vel,
                        rotspeed: rng2.random_range(-12.0..12.0),
                    },
                    PickupLifetime {
                        timer: Timer::from_seconds(1.4, TimerMode::Once),
                    },
                ));
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn melee_attack(
    commands: &mut Commands,
    trauma: &mut Trauma,
    hitstop: &mut HitStop,
    audio: &GameAudio,
    rumble: &mut MessageWriter<GamepadRumbleRequest>,
    gamepads: &Query<(Entity, &Gamepad)>,
    player_ent: Entity,
    tf: &Transform,
    aim: &AimDir,
    player: &Player,
    _vel: &mut Velocity,
    def: &WeaponDef,
    melee: MeleeDef,
    targets: &mut MeleeTargets,
    catalog: &AssetCatalog,
    asset_server: &AssetServer,
) {
    let melee_def = melee;

    let range = melee_def.range * player.melee_range_mult;
    ScreenEffects::add_trauma(trauma, def.shake.max(0.12));
    audio.play_melee(commands);

    let player_pos = tf.translation.truncate();
    let aim_angle = aim.0.y.atan2(aim.0.x);
    let mut hit_any = false;
    let mut hit_wall = false;

    for (ee, etf, mut ehealth, ebox, mut evel, nexthurt) in targets.enemies.iter_mut() {
        let offset = etf.translation.truncate() - player_pos;
        let dist = offset.length();
        if dist > range + ebox.radius {
            continue;
        }
        let angle = offset.y.atan2(offset.x);
        let diff = (angle - aim_angle).rem_euclid(std::f32::consts::TAU);
        if diff > melee_def.arc && diff < std::f32::consts::TAU - melee_def.arc {
            continue;
        }

        if nexthurt.as_ref().is_some_and(|nh| nh.0 > targets.frame.0) {
            continue;
        }
        ehealth.hp -= def.damage;
        if let Some(mut nh) = nexthurt {
            nh.0 = targets.frame.0 + 5;
        }
        if let Some(vel) = evel.as_mut() {
            GameFeel::apply_knockback(&mut vel.0, offset.normalize_or_zero(), def.knockback);
        }
        HitFlash::apply(commands, ee, Color::WHITE, 0.12);
        VfxSpawner::spawn_damage_number(
            commands,
            def.damage,
            etf.translation.truncate(),
            Color::srgb(1.0, 0.95, 0.6),
        );
        hit_any = true;
    }

    let mut dead_props: Vec<(
        Entity,
        Vec2,
        bool,
        Option<PropDeathEffect>,
        Option<PropSprites>,
    )> = Vec::new();
    for (pe, ptf, mut prop, death, sprites, nexthurt) in targets.props.iter_mut() {
        if !prop.destructible {
            continue;
        }
        let center = ptf.translation.truncate();
        let half = prop.size * 0.5;

        let offset = center - player_pos;
        let dist = (offset.length() - half.length()).max(0.0);
        if dist > range {
            continue;
        }
        let angle = offset.y.atan2(offset.x);
        let diff = (angle - aim_angle).rem_euclid(std::f32::consts::TAU);
        if diff > melee_def.arc && diff < std::f32::consts::TAU - melee_def.arc {
            continue;
        }
        if nexthurt.as_ref().is_some_and(|nh| nh.0 > targets.frame.0) {
            continue;
        }
        prop.hp -= def.damage.max(1);
        if let Some(mut nh) = nexthurt {
            nh.0 = targets.frame.0 + 5;
        }
        audio.play_hit(commands);
        hit_any = true;
        if prop.hp <= 0 {
            dead_props.push((pe, center, prop.explosive, death.copied(), sprites.copied()));
        }
    }
    for (pe, center, explosive, death, sprites) in dead_props {
        if let Some(ps) = sprites {
            crate::game::environment::spawn_prop_corpse(
                commands,
                catalog,
                asset_server,
                center,
                &ps,
            );
        }
        crate::game::environment::spawn_prop_death_effect(commands, center, death, explosive, None);
        commands.entity(pe).try_despawn();
    }

    if hit_any {
        hitstop.trigger(0.4, 0.1);
        ScreenEffects::add_trauma(trauma, 0.3);
        GameFeel::rumble_controller(rumble, gamepads, 0.5, 0.7, 0.2);
        audio.play_hit(commands);
    } else {

        let tip = player_pos + aim.0.normalize_or_zero() * range;
        let _ = tip;
        let _ = &mut hit_wall;
    }

    let angle = aim_angle;
    let swing_len = range * 1.35;
    commands.spawn((
        GameCleanup,
        LevelCleanup,
        SwingFx {
            timer: Timer::from_seconds(0.14, TimerMode::Once),
        },
        Sprite {
            color: Color::srgba(1.0, 1.0, 1.0, 0.45),
            custom_size: Some(Vec2::new(swing_len, 30.0)),
            ..default()
        },
        Transform::from_translation((player_pos + aim.0 * range * 0.6).extend(25.0))
            .with_rotation(Quat::from_rotation_z(angle)),
    ));
    Juice::pop_in(commands, player_ent, 0.08);
}

enum AmmoPayment {
    Paid,
    Blood(i32),
    Failed,
}

fn pay_fire_cost(
    inv: &mut Inventory,
    health: &mut Health,
    ammo: AmmoKind,
    amount: i32,
    blood: Option<crate::game::components::BloodAmmo>,
) -> AmmoPayment {
    if amount <= 0 {
        return AmmoPayment::Paid;
    }

    let slot = inv.ammo_mut(ammo);
    if *slot >= amount {
        *slot -= amount;
        return AmmoPayment::Paid;
    }

    if let Some(blood) = blood
        && health.hp > blood.hp_cost
    {
        health.hp -= blood.hp_cost;
        return AmmoPayment::Blood(blood.hp_cost);
    }

    AmmoPayment::Failed
}

#[allow(clippy::too_many_arguments)]
fn spawn_beam_shot(
    commands: &mut Commands,
    pos: Vec2,
    dir: Vec2,
    spec: BeamSpec,
    source: Option<DamageSource>,
) {
    let angle = dir.y.atan2(dir.x);
    let center = pos + dir * (spec.length * 0.5);

    commands.spawn((
        GameCleanup,
        LevelCleanup,
        Team::Player,
        crate::game::components::Beam {
            team: Team::Player,
            dir: dir.normalize_or_zero(),
            length: spec.length,
            width: spec.width,
            damage: spec.damage,
            knockback: spec.knockback,
            timer: Timer::from_seconds(spec.duration, TimerMode::Once),
            tick: Timer::from_seconds(spec.tick, TimerMode::Repeating),
            source,
        },
        Sprite {
            color: spec.color,
            custom_size: Some(Vec2::new(spec.length, spec.width)),
            ..default()
        },
        Transform::from_translation(center.extend(18.0))
            .with_rotation(Quat::from_rotation_z(angle)),
    ));
}

pub fn spawn_player_projectile(
    commands: &mut Commands,
    pos: Vec2,
    dir: Vec2,
    speed: f32,
    damage: i32,
    lifetime: f32,
    radius: f32,
    knockback: f32,
    explosive: bool,
    color: Color,
    size: Vec2,
) {

    spawn_player_projectile_with_source(
        commands,
        None,
        None,
        pos,
        dir,
        speed,
        damage,
        lifetime,
        radius,
        knockback,
        explosive,
        color,
        size,
        0,
        0,
        None,
        None,
        ProjectileArchetype::default(),
        None,
        None,
    )
}

pub fn spawn_player_projectile_with_source(
    commands: &mut Commands,
    catalog: Option<&AssetCatalog>,
    asset_server: Option<&AssetServer>,
    pos: Vec2,
    dir: Vec2,
    speed: f32,
    damage: i32,
    lifetime: f32,
    radius: f32,
    knockback: f32,
    explosive: bool,
    color: Color,
    size: Vec2,
    bounces: u8,
    pierce: u8,
    hazard: Option<HazardDef>,
    split: Option<SplitDef>,
    archetype: ProjectileArchetype,
    source: Option<DamageSource>,
    weapon: Option<WeaponId>,
) {
    let angle = dir.y.atan2(dir.x);
    let (sprite, anchor, anim_opt) =
        if let (Some(cat), Some(srv), Some(w)) = (catalog, asset_server, weapon) {
            let candidates = projectile_art::player_projectile_candidates(w);
            let path = projectile_art::first_existing(cat, &candidates);
            if cat.has(path) {

                let frames = cat.anims.get(path).map(|m| m[0] as usize).unwrap_or(1);
                let mut s = if frames == 2 {
                    crate::game::content::sprite_exact_frame(cat, srv, path, 1)
                } else {
                    sprite_exact(cat, srv, path)
                };

                s.custom_size = None;
                s.color = Color::WHITE;
                let a = crate::game::content::sprite_anchor(cat, path);
                let anim = crate::game::projectile_art::projectile_anim(cat, path);
                (s, a, anim)
            } else {
                (
                    Sprite {
                        color,
                        custom_size: Some(size),
                        ..default()
                    },
                    bevy::sprite::Anchor::CENTER,
                    None,
                )
            }
        } else {
            (
                Sprite {
                    color,
                    custom_size: Some(size),
                    ..default()
                },
                bevy::sprite::Anchor::CENTER,
                None,
            )
        };
    let mut ec = commands.spawn((
        GameCleanup,
        LevelCleanup,
        Team::Player,
        Projectile {
            damage,
            life: Timer::from_seconds(lifetime, TimerMode::Once),
            radius,
            knockback,
            explosive,
            source,
        },
        Velocity(dir * speed),
        sprite,
        anchor,
        Transform::from_translation(pos.extend(16.0)).with_rotation(Quat::from_rotation_z(angle)),
    ));
    if let Some(anim) = anim_opt {
        ec.insert(anim);
    }

    if bounces > 0 {
        ec.insert(BouncesLeft(bounces));
        if let Some(w) = weapon {
            if crate::game::content::weapon_ammo(w) == AmmoKind::Shells {
                ec.insert(ShellWallBounce(5.0));
            }
        }
    }
    if let Some(w) = weapon {
        if w.0 == 7 || w.0 == 44 {

            ec.insert(ProjectileFriction(0.1));
            ec.insert(crate::game::components::GrenadeFuse {
                smoke_armed: false,
                friction_switched: false,
                alarm1: Timer::from_seconds(6.0 / 30.0, TimerMode::Once),
            });
        } else if crate::game::content::weapon_ammo(w) == AmmoKind::Shells {
            ec.insert(ProjectileFriction(0.6));
            ec.insert(ShellBonus {
                timer: Timer::from_seconds(2.0 / 30.0, TimerMode::Once),
                bonus: 1,
            });
        }
    }
    if pierce > 0 || archetype.chain_lightning.is_some() {
        ec.insert(PiercesLeft(pierce));
        ec.insert(ProjectileHitSet::default());
    }
    if let Some(spec) = hazard {
        ec.insert(SpawnHazardOnDeath(spec));
        if spec.kind == HazardKind::Fire {
            ec.insert(FlameTrail {
                timer: Timer::from_seconds(0.12, TimerMode::Repeating),
                spec,
            });
        }
    }
    if let Some(spec) = split {
        ec.insert(SplitOnDeath(spec));
    }
    if let Some(homing) = archetype.homing {
        ec.insert(homing);
    }
    if let Some(sticky) = archetype.sticky {
        ec.insert(sticky);
    }
    if let Some(chain) = archetype.chain_lightning {

        ec.remove::<PiercesLeft>();
        ec.insert(chain);
    }
    if let Some(sentry) = archetype.deploys_sentry {
        ec.insert(sentry);
    }
    if let Some(custom) = archetype.custom_explosion {
        ec.insert(custom);
    }
    if let Some(blood) = archetype.blood_ammo {
        ec.insert(blood);
    }
    if let Some(pickup) = archetype.spawn_weapon_pickup {
        ec.insert(pickup);
    }
    if let Some(plasma) = archetype.plasma_burst {
        ec.insert(crate::game::components::PlasmaBurst {
            pellets: plasma.pellets,
            speed: plasma.speed,
            damage: plasma.damage,
            lifetime: plasma.lifetime,
            radius: plasma.radius,
            knockback: plasma.knockback,
            color: plasma.color,
            size: plasma.size,
        });
    }
    if archetype.hits_all_teams {
        ec.insert(HitsAllTeams);

        ec.insert(SpawnGrace(Timer::from_seconds(2.0 / 30.0, TimerMode::Once)));
    }

    let e = ec.id();
    if explosive {
        Juice::shake(commands, e, 1.2, lifetime);
    }
}

pub fn hammerhead_chew(
    time: Res<Time<Fixed>>,
    mut commands: Commands,
    catalog: Res<AssetCatalog>,
    asset_server: Res<AssetServer>,
    mut cooldown: Local<f32>,
    mut budget: ResMut<HammerheadBudget>,
    player_q: Query<(Entity, &Transform, &Player, &Velocity), With<Player>>,
    mut props: Query<
        (
            Entity,
            &mut Prop,
            &Transform,
            Option<&PropDeathEffect>,
            Option<&PropSprites>,
        ),
        Without<WallTile>,
    >,
    walls: Query<(Entity, &WallCell, &Transform), With<WallTile>>,
    entrances: Query<&SecretEntrance>,
    mut secrets: ResMut<SecretTriggers>,
) {
    *cooldown -= time.delta_secs();
    if *cooldown > 0.0 {
        return;
    }

    let Ok((player_entity, player_tf, player, vel)) = player_q.single() else {
        return;
    };
    if !player.hammerhead {
        return;
    }
    if vel.0.length_squared() < 40.0 * 40.0 {
        return;
    }

    let pos = player_tf.translation.truncate();
    let push = vel.0.normalize_or_zero();
    let probe = pos + push * (PLAYER_RADIUS + 6.0);

    if budget.remaining > 0 {
        for (_, cell, wtf) in &walls {
            let wpos = wtf.translation.truncate();
            if wpos.distance(probe) > crate::game::world::WALL_PX * 0.85 {
                continue;
            }

            if (wpos - pos).dot(push) < 0.0 {
                continue;
            }
            budget.remaining -= 1;
            *cooldown = 0.08;
            commands.spawn((
                GameCleanup,
                LevelCleanup,
                PendingWallBreak {
                    cell: (cell.0, cell.1),
                    pos: wpos,
                    spawn_floor: true,
                },
            ));
            return;
        }
    }

    for (prop_e, mut prop, prop_tf, death_effect, sprites) in &mut props {
        if !prop.destructible {
            continue;
        }
        let center = prop_tf.translation.truncate();
        let half = prop.size / 2.0;
        let closest = Vec2::new(
            pos.x.clamp(center.x - half.x, center.x + half.x),
            pos.y.clamp(center.y - half.y, center.y + half.y),
        );
        if pos.distance(closest) > PLAYER_RADIUS + 6.0 {
            continue;
        }

        *cooldown = 0.25;
        prop.hp -= 1;
        if prop.hp <= 0 {
            if let Some(ps) = sprites.copied() {
                spawn_prop_corpse(&mut commands, &catalog, &asset_server, center, &ps);
            }
            spawn_prop_death_effect(
                &mut commands,
                center,
                death_effect.copied(),
                prop.explosive,
                Some(DamageSource {
                    owner: player_entity,
                    team: Team::Player,
                    hit_id: HitId::Other(301),
                    enemy_kind: None,
                }),
            );
            if let Ok(entrance) = entrances.get(prop_e) {
                secrets.queue(entrance.target);
            }
            commands.entity(prop_e).try_despawn();
        }
        return;
    }
}

pub fn move_swing_fx(
    time: Res<Time<Fixed>>,
    mut commands: Commands,
    mut q: Query<(Entity, &mut SwingFx, &mut Sprite)>,
) {
    for (e, mut fx, mut sprite) in &mut q {
        fx.timer.tick(time.delta());
        let t = fx.timer.fraction();
        sprite.color.set_alpha(0.45 * (1.0 - t));
        if fx.timer.just_finished() {
            commands.entity(e).despawn();
        }
    }
}

pub fn tick_snare_zones(
    time: Res<Time<Fixed>>,
    mut commands: Commands,
    mut zones: Query<(Entity, &Transform, &mut SnareZone)>,
    enemies: Query<(Entity, &Transform), (With<Enemy>, Without<Slowed>)>,
) {
    for (e, ztf, mut zone) in &mut zones {
        zone.timer.tick(time.delta());
        if zone.timer.just_finished() {
            commands.entity(e).despawn();
            continue;
        }
        let z = ztf.translation.truncate();
        for (ee, etf) in &enemies {
            if etf.translation.truncate().distance(z) <= zone.radius {
                commands.entity(ee).insert(Slowed {
                    timer: Timer::from_seconds(0.4, TimerMode::Once),
                    factor: zone.slow,
                });
            }
        }
    }
}

pub fn tick_slowed(
    time: Res<Time<Fixed>>,
    mut commands: Commands,
    mut q: Query<(Entity, &mut Slowed, &mut Velocity), With<Enemy>>,
) {
    for (e, mut s, mut vel) in &mut q {
        s.timer.tick(time.delta());
        vel.0 *= s.factor;
        if s.timer.just_finished() {
            commands.entity(e).remove::<Slowed>();
        }
    }
}

pub fn tick_portal_strikes(
    time: Res<Time<Fixed>>,
    mut commands: Commands,
    mut trauma: ResMut<Trauma>,
    audio: Res<GameAudio>,
    mut q: Query<(Entity, &Transform, &mut PortalStrike)>,
    mut enemies: Query<(&Transform, &mut Health), With<Enemy>>,
) {
    for (e, tf, mut strike) in &mut q {
        strike.timer.tick(time.delta());
        if !strike.timer.just_finished() {
            continue;
        }
        let pos = tf.translation.truncate();
        for (etf, mut h) in &mut enemies {
            if etf.translation.truncate().distance(pos) <= strike.radius {
                h.hp -= strike.damage;
            }
        }
        ScreenEffects::add_trauma(&mut trauma, 0.4);
        audio.play_boom(&mut commands);
        VfxSpawner::spawn_burst(
            &mut commands,
            pos,
            28,
            Color::srgb(0.3, 0.9, 1.0),
            (120.0, 360.0),
        );
        commands.entity(e).despawn();
    }
}

pub fn tick_hazard_clouds(
    time: Res<Time<Fixed>>,
    mut commands: Commands,
    mut q: Query<(Entity, &Transform, &mut HazardCloud), (With<AbilityHazard>, Without<Team>)>,
    mut enemies: Query<(&Transform, &mut Health), With<Enemy>>,
) {
    for (e, tf, mut cloud) in &mut q {
        cloud.timer.tick(time.delta());
        cloud.tick.tick(time.delta());
        if cloud.timer.just_finished() {
            commands.entity(e).despawn();
            continue;
        }
        if !cloud.tick.just_finished() {
            continue;
        }
        let pos = tf.translation.truncate();
        for (etf, mut h) in &mut enemies {
            if etf.translation.truncate().distance(pos) <= cloud.radius {
                h.hp -= cloud.damage;
            }
        }
    }
}

pub fn ally_ai(
    time: Res<Time<Fixed>>,
    mut commands: Commands,
    catalog: Res<AssetCatalog>,
    asset_server: Res<AssetServer>,
    mut allies: Query<(Entity, &mut Ally, &mut Transform, &mut Velocity), Without<Enemy>>,
    enemies: Query<&Transform, With<Enemy>>,
    audio: Res<GameAudio>,
) {
    for (e, mut ally, mut tf, mut vel) in &mut allies {
        ally.life.tick(time.delta());
        ally.shoot.tick(time.delta());
        if ally.life.just_finished() {
            commands.entity(e).despawn();
            continue;
        }
        let pos = tf.translation.truncate();
        let mut best = None::<(f32, Vec2)>;
        for etf in &enemies {
            let p = etf.translation.truncate();
            let d = p.distance_squared(pos);
            if best.map(|(bd, _)| d < bd).unwrap_or(true) {
                best = Some((d, p));
            }
        }
        if let Some((_, target)) = best {
            let dir = (target - pos).normalize_or_zero();
            vel.0 = dir * 140.0;
            tf.translation += (vel.0 * time.delta_secs()).extend(0.0);
            if ally.shoot.just_finished() {
                let ally_path = "images/sprAllyBullet.png";
                let (ally_sprite, ally_anchor) = if catalog.has(ally_path) {
                    let mut s = sprite_exact(&catalog, &asset_server, ally_path);
                    s.custom_size = Some(Vec2::splat(6.0));
                    s.color = Color::WHITE;
                    let a = crate::game::content::sprite_anchor(&catalog, ally_path);
                    (s, a)
                } else {
                    (
                        Sprite {
                            color: Color::srgb(1.0, 0.7, 0.85),
                            custom_size: Some(Vec2::splat(6.0)),
                            ..default()
                        },
                        bevy::sprite::Anchor::CENTER,
                    )
                };
                commands.spawn((
                    GameCleanup,
                    LevelCleanup,
                    Projectile {
                        damage: 2,
                        life: Timer::from_seconds(0.7, TimerMode::Once),
                        radius: 4.0,
                        knockback: 20.0,
                        explosive: false,
                        source: Some(DamageSource {
                            owner: e,
                            team: Team::Player,
                            hit_id: HitId::Other(1),
                            enemy_kind: None,
                        }),
                    },
                    Team::Player,
                    Velocity(dir * 380.0),
                    Transform::from_translation(pos.extend(12.0)),
                    ally_sprite,
                    ally_anchor,
                ));
                audio.play_bolt(&mut commands);
            }
        }
    }
}

pub fn ensure_weapon_visual(
    mut commands: Commands,
    catalog: Res<AssetCatalog>,
    asset_server: Res<AssetServer>,
    player_q: Query<
        (Entity, &Inventory, &Transform, &AimDir, &RaceState),
        (With<Player>, Without<WeaponVisualOwner>),
    >,
) {
    let Ok((player_e, inv, tf, aim, race_state)) = player_q.single() else {
        return;
    };
    let id = inv.weapons[inv.current];
    if id == WeaponId::NONE {
        return;
    }
    let dual = race_state.race == RaceId::Steroids
        && inv.weapon_slots > 1
        && inv.weapons[(inv.current + 1) % inv.weapon_slots] != WeaponId::NONE;
    commands.entity(player_e).insert(WeaponVisualOwner);
    spawn_gun_visual(
        &mut commands,
        &catalog,
        &asset_server,
        player_e,
        tf,
        aim,
        id,
        0,
        Vec2::ZERO,
    );
    if dual {
        let second = inv.weapons[(inv.current + 1) % inv.weapon_slots];

        let perp = Vec2::new(-aim.0.y, aim.0.x).normalize_or_zero() * 8.0;
        spawn_gun_visual(
            &mut commands,
            &catalog,
            &asset_server,
            player_e,
            tf,
            aim,
            second,
            1,
            perp,
        );
    }
}

fn spawn_gun_visual(
    commands: &mut Commands,
    catalog: &AssetCatalog,
    asset_server: &AssetServer,
    player_e: Entity,
    tf: &Transform,
    aim: &AimDir,
    id: WeaponId,
    slot: u8,
    extra_offset: Vec2,
) {
    let path = weapon_world_sprite(id, catalog);
    let (mut spr, _) = crate::game::anim::sprite_anim(catalog, asset_server, &path);
    spr.custom_size = spr.custom_size.or(Some(Vec2::new(24.0, 12.0)));
    let angle = aim.0.y.atan2(aim.0.x);
    let pos = tf.translation.truncate() + aim.0 * 14.0 + extra_offset;
    let anchor = crate::game::content::sprite_anchor(catalog, &path);
    commands.spawn((
        GameCleanup,
        WeaponVisual {
            owner: player_e,
            wkick: 0.0,
            wep_id: id,
            slot,
        },
        spr,
        anchor,
        Transform::from_translation(pos.extend(21.0 - slot as f32 * 0.5))
            .with_rotation(Quat::from_rotation_z(angle)),
    ));
}

pub fn tick_weapon_visuals(
    time: Res<Time<Fixed>>,
    catalog: Res<AssetCatalog>,
    asset_server: Res<AssetServer>,
    mut commands: Commands,
    player_q: Query<
        (
            Entity,
            &Transform,
            &AimDir,
            &Inventory,
            &RaceState,
            Option<&PortalSucking>,
        ),
        With<Player>,
    >,
    mut vis_q: Query<
        (
            Entity,
            &mut WeaponVisual,
            &mut Transform,
            &mut Sprite,
            &mut bevy::sprite::Anchor,
        ),
        Without<Player>,
    >,
) {
    let dt = time.delta_secs();
    let Ok((player_e, ptf, aim, inv, race_state, sucking)) = player_q.single() else {
        for (e, _, _, _, _) in &vis_q {
            commands.entity(e).despawn();
        }
        return;
    };
    if sucking.is_some() {
        for (e, _, _, _, _) in &vis_q {
            commands.entity(e).despawn();
        }
        commands.entity(player_e).remove::<WeaponVisualOwner>();
        return;
    }
    let dual = race_state.race == RaceId::Steroids && inv.weapon_slots > 1;
    let want_slots: usize = if dual { 2 } else { 1 };

    let mut seen = [false; 2];
    for (_e, wv, _, _, _) in vis_q.iter() {
        if wv.owner == player_e && (wv.slot as usize) < 2 {
            seen[wv.slot as usize] = true;
        }
    }
    if dual {
        let second = inv.weapons[(inv.current + 1) % inv.weapon_slots];
        if second == WeaponId::NONE && seen[1] {
            for (e, wv, _, _, _) in vis_q.iter() {
                if wv.owner == player_e && wv.slot == 1 {
                    commands.entity(e).despawn();
                }
            }
            seen[1] = false;
        } else if second != WeaponId::NONE && !seen[1] {
            let perp = Vec2::new(-aim.0.y, aim.0.x).normalize_or_zero() * 8.0;
            spawn_gun_visual(
                &mut commands,
                &catalog,
                &asset_server,
                player_e,
                ptf,
                aim,
                second,
                1,
                perp,
            );
            seen[1] = true;
        }
    } else if seen[1] {
        for (e, wv, _, _, _) in vis_q.iter() {
            if wv.owner == player_e && wv.slot == 1 {
                commands.entity(e).despawn();
            }
        }
    }
    let _ = want_slots;
    for (_e, mut wv, mut tf, mut sprite, mut anchor) in &mut vis_q {
        if wv.owner != player_e {
            continue;
        }
        let slot_idx = if dual {
            (inv.current + wv.slot as usize) % inv.weapon_slots
        } else {
            if wv.slot != 0 {
                continue;
            }
            inv.current
        };
        let id = inv.weapons[slot_idx];
        wv.wkick *= 0.6_f32.powf(dt * crate::app::NT_SIM_HZ as f32);
        if wv.wkick.abs() < 0.15 {
            wv.wkick = 0.0;
        }
        if wv.wep_id != id {
            wv.wep_id = id;
            let path = weapon_world_sprite(id, &catalog);
            sprite.image = asset_server.load(path.clone());
            if let Some(def) = catalog.anim_def(&path) {
                sprite.rect = Some(Rect::new(0.0, 0.0, def.frame_px as f32, def.height as f32));
            } else {
                sprite.rect = None;
            }
            *anchor = crate::game::content::sprite_anchor(&catalog, &path);
        }
        let angle = aim.0.y.atan2(aim.0.x);
        let forward = aim.0.normalize_or_zero();
        let perp = Vec2::new(-forward.y, forward.x);
        let side = if wv.slot == 1 { perp * 8.0 } else { Vec2::ZERO };
        let hold = ptf.translation.truncate() + forward * (12.0 - wv.wkick) + side;
        tf.translation = hold.extend(21.0 - wv.slot as f32 * 0.5);
        tf.rotation = Quat::from_rotation_z(angle);
        sprite.flip_y = aim.0.x < 0.0;
    }
}

fn steroids_secondary_slot(current: usize, slots: usize) -> usize {
    if slots > 1 {
        (current + 1) % slots
    } else {
        current
    }
}

fn weapon_world_sprite(id: WeaponId, catalog: &AssetCatalog) -> String {

    let meta = crate::game::content::weapon_meta(id);
    let stem = meta.wep_sprt;
    if stem.is_empty() || stem == "mskNone" {
        return "images/sprRevolver.png".to_string();
    }
    let path = format!("images/{stem}.png");
    if !catalog.has(&path) {
        panic!(
            "Missing weapon art: {path} for {} (id {}). Run `NT_ALL_SPRITES=1 python3 tools/gen_assets.py`.",
            meta.wep_name, id.0
        );
    }
    path
}

#[cfg(test)]
mod steroids_dual_tests {
    use super::steroids_secondary_slot;

    #[test]
    fn secondary_is_the_other_live_slot() {
        assert_eq!(steroids_secondary_slot(0, 2), 1);
        assert_eq!(steroids_secondary_slot(1, 2), 0);
    }

    #[test]
    fn single_slot_has_no_secondary() {
        assert_eq!(steroids_secondary_slot(0, 1), 0);
    }

    #[test]
    fn swap_exchanges_roles() {

        let a_primary = 0;
        let a_secondary = steroids_secondary_slot(a_primary, 2);
        let b_primary = 1;
        let b_secondary = steroids_secondary_slot(b_primary, 2);
        assert_eq!(a_primary, b_secondary);
        assert_eq!(a_secondary, b_primary);
    }
}
