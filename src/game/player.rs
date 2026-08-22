//! Player: movement, mouse aim (with camera lookahead), weapon switching,
//! firing (ranged + melee), and the character active ability.

use bevy::input::gamepad::{Gamepad, GamepadRumbleRequest};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use rand::RngExt;

use crate::game::audio::GameAudio;
use crate::game::components::*;
use crate::game::content::*;
use crate::game::weapon_runtime::weapon_runtime_def;
use crate::game::world::*;
use game_utils_bevy::camera_follow::CameraFollow;
use game_utils_bevy::game_feel::{GameFeel, SlowMotion};
use game_utils_bevy::hit_flash::HitFlash;
use game_utils_bevy::hitstop::HitStop;
use game_utils_bevy::juice::Juice;
use game_utils_bevy::screen_effects::{ChromaticAberration, FlashWhite, ScreenEffects, Trauma};
use game_utils_bevy::vfx::VfxSpawner;

pub fn player_move(
    time: Res<Time<Fixed>>,
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    mask: Res<FloorMask>,
    mut q: Query<
        (
            Entity,
            &Player,
            &mut Velocity,
            &mut Transform,
            Option<&mut Dash>,
        ),
        (With<Player>, Without<Prop>),
    >,
    props: Query<(Entity, &Prop, &Transform), With<Prop>>,
) {
    let Ok((entity, player, mut vel, mut tf, dash)) = q.single_mut() else {
        return;
    };

    let dt = time.delta_secs();

    if let Some(mut dash) = dash {
        dash.timer.tick(time.delta());
        vel.0 = dash.dir * 950.0;
        tf.translation += (vel.0 * dt).extend(0.0);

        if dash.timer.just_finished() {
            commands.entity(entity).remove::<Dash>();
        }
    } else {
        let mut input = Vec2::ZERO;
        if keys.pressed(KeyCode::KeyW) || keys.pressed(KeyCode::ArrowUp) {
            input.y += 1.0;
        }
        if keys.pressed(KeyCode::KeyS) || keys.pressed(KeyCode::ArrowDown) {
            input.y -= 1.0;
        }
        if keys.pressed(KeyCode::KeyA) || keys.pressed(KeyCode::ArrowLeft) {
            input.x -= 1.0;
        }
        if keys.pressed(KeyCode::KeyD) || keys.pressed(KeyCode::ArrowRight) {
            input.x += 1.0;
        }

        if input != Vec2::ZERO {
            vel.0 += input.normalize() * player.accel * dt;
        }
        let max_speed = player.speed * player.speed_mult;
        if vel.0.length() > max_speed {
            vel.0 = vel.0.normalize() * max_speed;
        }
        vel.0 *= player.friction.powf(dt * 60.0);
        tf.translation += (vel.0 * dt).extend(0.0);
    }

    // Order: props (walls) first, then snap onto floor mask, then outer AABB.
    resolve_prop_collision(&mut tf.translation, PLAYER_RADIUS, &props);
    clamp_to_arena(&mut tf.translation, PLAYER_RADIUS);
}

pub fn face_aim(mut q: Query<(&AimDir, &mut Sprite), With<Player>>) {
    let Ok((aim, mut sprite)) = q.single_mut() else {
        return;
    };
    // NT faces aim; flip X when aiming left
    sprite.flip_x = aim.0.x < 0.0;
}

pub fn player_aim(
    windows: Query<&Window, With<PrimaryWindow>>,
    camera_q: Query<(&Camera, &GlobalTransform), With<Camera2d>>,
    mut player_q: Query<(&Transform, &mut AimDir), With<Player>>,
    mut follow_q: Query<&mut CameraFollow>,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    let Ok((camera, cam_gt)) = camera_q.single() else {
        return;
    };
    let Ok((ptf, mut aim)) = player_q.single_mut() else {
        return;
    };

    if let Some(cursor) = window.cursor_position()
        && let Ok(world) = camera.viewport_to_world_2d(cam_gt, cursor)
    {
        let dir = (world - ptf.translation.truncate()).normalize_or_zero();
        if dir != Vec2::ZERO {
            aim.0 = dir;
        }
        if let Ok(mut follow) = follow_q.single_mut() {
            follow.set_aim(world);
        }
    }
}

pub fn weapon_switch(
    keys: Res<ButtonInput<KeyCode>>,
    mut q: Query<&mut Inventory, With<Player>>,
    audio: Res<GameAudio>,
    mut commands: Commands,
) {
    let Ok(mut inv) = q.single_mut() else {
        return;
    };
    let mut switched = false;
    if keys.just_pressed(KeyCode::Digit1) && inv.weapons[0] != WeaponId::NONE {
        inv.current = 0;
        switched = true;
    }
    if keys.just_pressed(KeyCode::Digit2)
        && inv.weapon_slots > 1
        && inv.weapons[1] != WeaponId::NONE
    {
        inv.current = 1;
        switched = true;
    }
    if keys.just_pressed(KeyCode::Digit3)
        && inv.weapon_slots > 2
        && inv.weapons[2] != WeaponId::NONE
    {
        inv.current = 2;
        switched = true;
    }
    // Scroll wheel could be added later
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
    keys: Res<ButtonInput<KeyCode>>,
    mut commands: Commands,
    mut trauma: ResMut<Trauma>,
    mut chroma: ResMut<ChromaticAberration>,
    mut slow_mo: ResMut<SlowMotion>,
    mut hitstop: ResMut<HitStop>,
    audio: Res<GameAudio>,
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
            Option<&mut Shield>,
            Option<&mut Telekinesis>,
        ),
        (With<Player>, Without<Enemy>),
    >,
    mut enemies: Query<(Entity, &Transform, &mut Health), (With<Enemy>, Without<Player>)>,
) {
    let Ok((player_e, mut player, mut health, mut vel, tf, aim, shield, telek)) = q.single_mut()
    else {
        return;
    };

    let fire = keys.just_pressed(KeyCode::KeyE) || keys.just_pressed(KeyCode::ShiftLeft);
    if !fire || !player.ability_cooldown.is_finished() {
        return;
    }

    player.ability_cooldown = Timer::from_seconds(6.0, TimerMode::Once);
    let pos = tf.translation.truncate();

    match player.ability {
        AbilityKind::Flip => {
            let mut dir = Vec2::ZERO;
            if keys.pressed(KeyCode::KeyW) || keys.pressed(KeyCode::ArrowUp) {
                dir.y += 1.0;
            }
            if keys.pressed(KeyCode::KeyS) || keys.pressed(KeyCode::ArrowDown) {
                dir.y -= 1.0;
            }
            if keys.pressed(KeyCode::KeyA) || keys.pressed(KeyCode::ArrowLeft) {
                dir.x -= 1.0;
            }
            if keys.pressed(KeyCode::KeyD) || keys.pressed(KeyCode::ArrowRight) {
                dir.x += 1.0;
            }
            let dir = if dir != Vec2::ZERO {
                dir.normalize()
            } else {
                aim.0
            };
            commands.entity(player_e).insert(Dash {
                timer: Timer::from_seconds(0.18, TimerMode::Once),
                dir,
            });
            health.invuln = Timer::from_seconds(15.0 / 30.0, TimerMode::Once);
            vel.0 = dir * 900.0;
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
            let timer = Timer::from_seconds(1.6, TimerMode::Once);
            if let Some(mut s) = shield {
                s.timer = timer;
            } else {
                commands.entity(player_e).insert(Shield { timer });
            }
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
                player.ability_cooldown.finish();
                return;
            }
            health.hp -= 1;
            let radius = 150.0;
            for (_, etf, mut ehealth) in &mut enemies {
                if etf.translation.truncate().distance(pos) < radius {
                    ehealth.hp -= 3;
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
    }
}

pub fn player_fire(
    time: Res<Time<Fixed>>,
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    mut commands: Commands,
    mut trauma: ResMut<Trauma>,
    mut flash: ResMut<FlashWhite>,
    mut hitstop: ResMut<HitStop>,
    audio: Res<GameAudio>,
    player_q: Query<(Entity, &Transform, &AimDir, &Player, &Health), With<Player>>,
    mut fire_q: Query<(&mut FireCooldown, &mut Inventory, &mut Velocity), With<Player>>,
    mut enemies: Query<
        (
            Entity,
            &Transform,
            &mut Health,
            &Hitbox,
            Option<&mut Velocity>,
        ),
        (With<Enemy>, Without<Player>),
    >,
    gamepads: Query<(Entity, &Gamepad)>,
    mut rumble: MessageWriter<GamepadRumbleRequest>,
) {
    let Ok((player_ent, tf, aim, player, health)) = player_q.single() else {
        return;
    };
    let Ok((mut cooldown, mut inv, mut vel)) = fire_q.single_mut() else {
        return;
    };

    cooldown.timer.tick(time.delta());
    cooldown.burst_timer.tick(time.delta());

    let weapon_id = inv.weapons[inv.current];
    let def = weapon_runtime_def(weapon_id);

    // Empty slot: nothing to fire.
    if weapon_id == WeaponId::NONE {
        cooldown.timer = Timer::from_seconds(0.0, TimerMode::Once);
        cooldown.burst_left = 0;
        return;
    }

    // Continue an in-progress burst (Assault Rifle).
    if cooldown.burst_left > 0 && cooldown.burst_timer.is_finished() {
        spawn_pellets(
            &mut commands,
            &mut trauma,
            &audio,
            &mut rumble,
            &gamepads,
            player_ent,
            tf,
            aim,
            player,
            weapon_id,
            &def,
        );
        cooldown.burst_left -= 1;
        cooldown.burst_timer = Timer::from_seconds(def.burst_interval, TimerMode::Once);
        return;
    }

    let intent = if def.automatic {
        mouse.pressed(MouseButton::Left) || keys.pressed(KeyCode::Space)
    } else {
        mouse.just_pressed(MouseButton::Left) || keys.just_pressed(KeyCode::Space)
    };

    if !intent || !cooldown.timer.is_finished() {
        return;
    }

    if def.melee.is_none() && !consume_ammo(&mut inv, def.ammo, def.ammo_cost) {
        return;
    }

    // Stress: fire rate scales with missing health (up to +100% at 1 HP).
    let stress_bonus = if player.stress {
        (1.0 - health.hp as f32 / health.max.max(1) as f32).max(0.0)
    } else {
        0.0
    };
    let cd = def.cooldown * player.fire_rate_mult / (1.0 + stress_bonus);
    cooldown.timer = Timer::from_seconds(cd.max(0.03), TimerMode::Once);

    if let Some(melee) = def.melee {
        melee_attack(
            &mut commands,
            &mut trauma,
            &mut hitstop,
            &audio,
            &mut rumble,
            &gamepads,
            player_ent,
            tf,
            aim,
            player,
            &mut vel,
            &def,
            melee,
            &mut enemies,
        );
        return;
    }

    spawn_pellets(
        &mut commands,
        &mut trauma,
        &audio,
        &mut rumble,
        &gamepads,
        player_ent,
        tf,
        aim,
        player,
        weapon_id,
        &def,
    );

    // Queue the rest of the burst.
    if def.burst_shots > 1 {
        cooldown.burst_left = def.burst_shots - 1;
        cooldown.burst_timer = Timer::from_seconds(def.burst_interval, TimerMode::Once);
    }
}

/// Fires one volley of the current weapon (all pellets for one trigger pull).
#[allow(clippy::too_many_arguments)]
fn spawn_pellets(
    commands: &mut Commands,
    trauma: &mut Trauma,
    audio: &GameAudio,
    rumble: &mut MessageWriter<GamepadRumbleRequest>,
    gamepads: &Query<(Entity, &Gamepad)>,
    player_ent: Entity,
    tf: &Transform,
    aim: &AimDir,
    player: &Player,
    id: WeaponId,
    def: &WeaponDef,
) {
    ScreenEffects::add_trauma(trauma, def.shake);
    GameFeel::rumble_controller(rumble, gamepads, 0.08, def.shake, 0.07);

    let kind: WeaponKind = id.into();
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
            if def.explosive {
                audio.play_explode(commands);
            } else if def.melee.is_none() {
                audio.play_shoot(commands);
            }
        }
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

    let mut rng = rand::rng();
    let spread = def.spread * player.spread_mult;
    for _ in 0..def.pellets {
        let base_angle = aim.0.y.atan2(aim.0.x);
        let angle = base_angle + rng.random_range(-spread..spread);
        let dir = Vec2::new(angle.cos(), angle.sin());
        spawn_player_projectile_with_source(
            commands,
            muzzle,
            dir,
            def.speed,
            def.damage,
            def.lifetime,
            def.projectile_radius,
            def.knockback * player.knockback_mult,
            def.explosive,
            def.color,
            def.size,
            Some(DamageSource {
                owner: player_ent,
                team: Team::Player,
                hit_id: HitId::Weapon(id),
            }),
        );
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
    enemies: &mut Query<
        (
            Entity,
            &Transform,
            &mut Health,
            &Hitbox,
            Option<&mut Velocity>,
        ),
        (With<Enemy>, Without<Player>),
    >,
) {
    let melee_def = melee;
    let range = melee_def.range * player.melee_range_mult;
    ScreenEffects::add_trauma(trauma, def.shake.max(0.12));
    audio.play_melee(commands);

    let player_pos = tf.translation.truncate();
    let aim_angle = aim.0.y.atan2(aim.0.x);
    let mut hit_any = false;

    for (ee, etf, mut ehealth, ebox, mut evel) in enemies.iter_mut() {
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

        ehealth.hp -= def.damage;
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

    if hit_any {
        hitstop.trigger(0.4, 0.1);
        ScreenEffects::add_trauma(trauma, 0.3);
        GameFeel::rumble_controller(rumble, gamepads, 0.5, 0.7, 0.2);
        audio.play_hit(commands);
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

fn consume_ammo(inv: &mut Inventory, ammo: AmmoKind, amount: i32) -> bool {
    if amount <= 0 {
        return true;
    }
    let slot = inv.ammo_mut(ammo);
    if *slot >= amount {
        *slot -= amount;
        true
    } else {
        false
    }
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
        commands, pos, dir, speed, damage, lifetime, radius, knockback, explosive, color, size,
        None,
    )
}

pub fn spawn_player_projectile_with_source(
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
    source: Option<DamageSource>,
) {
    let angle = dir.y.atan2(dir.x);
    let e = commands
        .spawn((
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
            Sprite {
                color,
                custom_size: Some(size),
                ..default()
            },
            Transform::from_translation(pos.extend(16.0))
                .with_rotation(Quat::from_rotation_z(angle)),
        ))
        .id();

    if explosive {
        Juice::shake(commands, e, 1.2, lifetime);
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
