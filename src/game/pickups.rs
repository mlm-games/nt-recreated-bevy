//! Pickups: rads, medkits, ammo, weapon drops, and chests. Includes magnet
//! attraction (base range, Eyes passive, Telekinesis active) and collection.

use crate::game::audio::GameAudio;
use crate::game::combat::random_weapon;
use crate::game::components::*;
use crate::game::content::*;
use crate::game::progression;
use bevy::input::gamepad::{Gamepad, GamepadRumbleRequest};
use bevy::prelude::*;
use game_utils_bevy::game_feel::GameFeel;
use game_utils_bevy::juice::Juice;
use game_utils_bevy::screen_effects::{ChromaticAberration, FlashWhite, ScreenEffects, Trauma};
use game_utils_bevy::vfx::VfxSpawner;

impl Toast {
    pub fn show(&mut self, text: &str) {
        self.text = text.to_string();
        self.timer = Timer::from_seconds(2.2, TimerMode::Once);
    }
}

pub fn spawn_pickup(
    commands: &mut Commands,
    kind: PickupKind,
    pos: Vec2,
) {
    spawn_pickup_with_assets(commands, None, kind, pos);
}

pub fn spawn_pickup_with_assets(
    commands: &mut Commands,
    asset_server: Option<&AssetServer>,
    kind: PickupKind,
    pos: Vec2,
) {
    let (path, fallback, size) = match kind {
        PickupKind::Rad(_) => ("images/sprRad.png", Color::srgb(0.25, 1.0, 0.2), 12.0),
        PickupKind::Medkit(_) => ("images/sprHP.png", Color::srgb(1.0, 0.15, 0.15), 16.0),
        PickupKind::Ammo(AmmoKind::Bullets, _) => (
            "images/sprBulletPickup.png",
            Color::srgb(0.25, 0.65, 1.0),
            14.0,
        ),
        PickupKind::Ammo(AmmoKind::Shells, _) => (
            "images/sprShellPickup.png",
            Color::srgb(0.25, 0.65, 1.0),
            14.0,
        ),
        PickupKind::Ammo(AmmoKind::Bolts, _) => (
            "images/sprBoltPickup.png",
            Color::srgb(0.25, 0.65, 1.0),
            14.0,
        ),
        PickupKind::Ammo(AmmoKind::Explosives, _) => (
            "images/sprExploPickup.png",
            Color::srgb(0.25, 0.65, 1.0),
            14.0,
        ),
        PickupKind::Weapon(k) => ("images/sprRevolver.png", weapon_color(k), 20.0),
        PickupKind::Chest => ("images/sprChest.png", Color::srgb(0.85, 0.6, 0.2), 32.0),
    };

    let sprite = if let Some(server) = asset_server {
        sprite_or_fallback(server, path, fallback, Vec2::splat(size))
    } else {
        Sprite {
            color: fallback,
            custom_size: Some(Vec2::splat(size)),
            ..default()
        }
    };

    let e = commands
        .spawn((
            GameCleanup,
            LevelCleanup,
            Pickup { kind },
            sprite,
            Transform::from_translation(pos.extend(8.0)),
        ))
        .id();

    Juice::pop_in(commands, e, 0.14);
}

pub fn collect_pickups(
    time: Res<Time>,
    mut commands: Commands,
    mut trauma: ResMut<Trauma>,
    mut flash: ResMut<FlashWhite>,
    mut chroma: ResMut<ChromaticAberration>,
    audio: Res<GameAudio>,
    gamepads: Query<(Entity, &Gamepad)>,
    mut rumble: MessageWriter<GamepadRumbleRequest>,
    mut player_q: Query<
        (
            Entity,
            &Transform,
            &mut Player,
            &mut Health,
            &mut Inventory,
            Option<&Telekinesis>,
        ),
        (With<Player>, Without<Pickup>),
    >,
    mut pickups: Query<(Entity, &mut Transform, &Pickup), Without<Player>>,
    mut toast: ResMut<Toast>,
) {
    let Ok((player_e, player_tf, mut player, mut health, mut inv, telek)) = player_q.single_mut()
    else {
        return;
    };

    let player_pos = player_tf.translation.truncate();
    let dt = time.delta_secs();

    // Telekinesis massively extends the magnet range while active.
    let telek_active = telek.is_some_and(|t| !t.timer.is_finished());
    let magnet = if telek_active {
        player.pickup_range + 500.0
    } else {
        player.pickup_range
    };

    for (pickup_e, mut pickup_tf, pickup) in &mut pickups {
        let pickup_pos = pickup_tf.translation.truncate();
        let dist = player_pos.distance(pickup_pos);

        if dist < magnet {
            let dir = (player_pos - pickup_pos).normalize_or_zero();
            let pull = if telek_active { 900.0 } else { 460.0 };
            pickup_tf.translation += (dir * pull * dt).extend(0.0);
        }

        if dist > 20.0 {
            continue;
        }

        commands.entity(pickup_e).despawn();

        match pickup.kind {
            PickupKind::Rad(amount) => {
                player.rads += amount;
                ScreenEffects::chromatic_pulse(&mut chroma, 0.04);
                audio.play_pickup(&mut commands);
                progression::check_level_up(
                    &mut commands,
                    &mut trauma,
                    &mut flash,
                    &mut player,
                    &mut health,
                    &mut inv,
                    &mut toast,
                    &audio,
                    player_pos,
                );
            }
            PickupKind::Medkit(amount) => {
                let heal = (amount as f32 * player.medkit_mult).round() as i32;
                health.hp = (health.hp + heal).min(health.max);
                VfxSpawner::spawn_damage_number(
                    &mut commands,
                    heal,
                    player_pos,
                    Color::srgb(0.3, 1.0, 0.3),
                );
                audio.play_pickup(&mut commands);
            }
            PickupKind::Ammo(ammo, amount) => {
                let fish_bonus = if player.ability == AbilityKind::Flip {
                    match ammo {
                        AmmoKind::Bullets => 8,
                        AmmoKind::Shells | AmmoKind::Bolts | AmmoKind::Explosives => 2,
                    }
                } else {
                    0
                };
                let cap = ammo_max(ammo)
                    + if player.back_muscle > 0 {
                        match ammo {
                            AmmoKind::Bullets => (300 * player.back_muscle) as i32,
                            _ => (44 * player.back_muscle) as i32,
                        }
                    } else {
                        0
                    };
                let slot = inv.ammo_mut(ammo);
                let gained = (amount + fish_bonus).min(cap - *slot).max(0);
                *slot += gained;
                VfxSpawner::spawn_damage_number(
                    &mut commands,
                    gained,
                    player_pos,
                    Color::srgb(0.35, 0.7, 1.0),
                );
                audio.play_pickup(&mut commands);
            }
            PickupKind::Weapon(weapon) => {
                equip_weapon(&mut commands, &mut inv, weapon, player_pos);
                ScreenEffects::flash_white(&mut flash, 0.04);
                Juice::bounce_scale(&mut commands, player_e, 1.3, 0.16);
                audio.play_chest(&mut commands);
                toast.show(&format!("Picked up {}", weapon_name(weapon)));
            }
            PickupKind::Chest => {
                let weapon = random_weapon(&mut rand::rng());
                equip_weapon(&mut commands, &mut inv, weapon, player_pos);
                ScreenEffects::add_trauma(&mut trauma, 0.15);
                ScreenEffects::flash_white(&mut flash, 0.05);
                GameFeel::rumble_controller(&mut rumble, &gamepads, 0.3, 0.4, 0.15);
                audio.play_chest(&mut commands);
                VfxSpawner::spawn_burst(
                    &mut commands,
                    player_pos,
                    24,
                    Color::srgb(1.0, 0.8, 0.3),
                    (100.0, 300.0),
                );
                toast.show(&format!("Opened chest: {}", weapon_name(weapon)));
            }
        }
    }
}

/// Equips a weapon NT-style: if the backup slot is empty, the current weapon
/// moves there; otherwise the current weapon is dropped on the ground.
fn equip_weapon(
    commands: &mut Commands,
    inv: &mut Inventory,
    weapon: WeaponKind,
    player_pos: Vec2,
) {
    let other = 1 - inv.current;
    if inv.weapons[other] != WeaponKind::None {
        let dropped = inv.weapons[inv.current];
        if dropped != WeaponKind::None {
            spawn_pickup(commands, PickupKind::Weapon(dropped), player_pos);
        }
    } else {
        inv.weapons[other] = inv.weapons[inv.current];
    }
    inv.weapons[inv.current] = weapon;

    let def = weapon_def(weapon);
    if def.melee.is_none() {
        let slot = inv.ammo_mut(def.ammo);
        let add = ammo_pickup_amount(def.ammo) * 2;
        *slot = (*slot + add).min(ammo_max(def.ammo));
    }
}

pub fn tick_toast(time: Res<Time>, mut toast: ResMut<Toast>) {
    if toast.timer.duration().is_zero() {
        return;
    }
    toast.timer.tick(time.delta());
    if toast.timer.is_finished() {
        toast.text.clear();
    }
}
