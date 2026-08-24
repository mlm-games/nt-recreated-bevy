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
    catalog: &AssetCatalog,
    asset_server: &AssetServer,
    kind: PickupKind,
    pos: Vec2,
) {
    let (path, size) = pickup_sprite(kind, catalog);
    let e = commands
        .spawn((
            GameCleanup,
            LevelCleanup,
            Pickup { kind },
            sprite_exact(catalog, asset_server, &path),
            Transform::from_translation(pos.extend(8.0)),
        ))
        .id();
    Juice::pop_in(commands, e, 0.14);
}

pub fn spawn_chest(
    commands: &mut Commands,
    catalog: &AssetCatalog,
    asset_server: &AssetServer,
    kind: ChestKind,
    pos: Vec2,
) {
    let path = match kind {
        ChestKind::Weapon => "images/sprWeaponChest.png",
        ChestKind::Ammo => "images/sprAmmoChest.png",
        ChestKind::Rad => "images/sprRadChest.png",
    };
    let (sprite, strip) = crate::game::anim::sprite_anim(catalog, asset_server, path);
    let mut ec = commands.spawn((
        GameCleanup,
        LevelCleanup,
        Pickup {
            kind: PickupKind::Chest(kind),
        },
        sprite,
        Transform::from_translation(pos.extend(8.0)),
    ));
    if let Some(strip) = strip {
        ec.insert(strip);
    }
    let e = ec.id();
    Juice::pop_in(commands, e, 0.14);
}

/// Native NT art per pickup kind.
fn pickup_sprite(kind: PickupKind, catalog: &AssetCatalog) -> (String, f32) {
    match kind {
        PickupKind::Rad(_) => ("images/sprRad.png".to_string(), 12.0),
        PickupKind::Medkit(_) => ("images/sprHP.png".to_string(), 16.0),
        PickupKind::Ammo(AmmoKind::Bullets, _) => ("images/sprBulletIcon.png".to_string(), 12.0),
        PickupKind::Ammo(AmmoKind::Shells, _) => ("images/sprShotIcon.png".to_string(), 12.0),
        PickupKind::Ammo(AmmoKind::Bolts, _) => ("images/sprBoltIcon.png".to_string(), 12.0),
        PickupKind::Ammo(AmmoKind::Explosives, _) => ("images/sprExploIcon.png".to_string(), 12.0),
        PickupKind::Ammo(AmmoKind::Energy, _) => ("images/sprEnergyIcon.png".to_string(), 12.0),
        PickupKind::Ammo(AmmoKind::None, _) => ("images/sprRad.png".to_string(), 12.0),
        PickupKind::Weapon(k) => (weapon_id_sprite(k, catalog), 20.0),
        PickupKind::Chest(kind) => match kind {
            ChestKind::Weapon => ("images/sprWeaponChest.png".to_string(), 32.0),
            ChestKind::Ammo => ("images/sprAmmoChest.png".to_string(), 32.0),
            ChestKind::Rad => ("images/sprRadChest.png".to_string(), 32.0),
        },
    }
}

/// World sprite for a dropped weapon.
///
/// Uses the generated registry's exact `wep_sprt` field when that art was
/// imported, falling back to the Revolver so a missing PNG can never crash
/// a drop.
fn weapon_id_sprite(id: WeaponId, catalog: &AssetCatalog) -> String {
    let meta = crate::game::content::weapon_meta(id);
    if !meta.wep_sprt.is_empty() && meta.wep_sprt != "mskNone" {
        let path = format!("images/{}.png", meta.wep_sprt);
        if catalog.has(&path) {
            return path;
        }
    }
    "images/sprRevolver.png".to_string()
}

pub fn collect_pickups(
    time: Res<Time<Fixed>>,
    mut commands: Commands,
    catalog: Res<AssetCatalog>,
    asset_server: Res<AssetServer>,
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
            &RaceState,
            Option<&Telekinesis>,
        ),
        (With<Player>, Without<Pickup>),
    >,
    mut pickups: Query<(Entity, &mut Transform, &Pickup), Without<Player>>,
    mut anims: Query<&mut crate::game::anim::SpriteAnim>,
    mut sprites: Query<&mut Sprite>,
    mut toast: ResMut<Toast>,
) {
    let Ok((player_e, player_tf, mut player, mut health, mut inv, race_state, telek)) =
        player_q.single_mut()
    else {
        return;
    };

    let player_pos = player_tf.translation.truncate();
    let dt = time.delta_secs();

    // Telekinesis massively extends the magnet range while active.
    let telek_active = telek.is_some_and(|t| !t.timer.is_finished());
    let telek_mult = if telek_active {
        player.ultra_ability_mult
    } else {
        1.0
    };
    let magnet = if telek_active {
        player.pickup_range + 500.0 * telek_mult
    } else {
        player.pickup_range
    };

    for (pickup_e, mut pickup_tf, pickup) in &mut pickups {
        let pickup_pos = pickup_tf.translation.truncate();
        let dist = player_pos.distance(pickup_pos);

        // Chests never fly to the player (upstream: open on contact).
        let is_chest = matches!(pickup.kind, PickupKind::Chest(_));
        if !is_chest && dist < magnet {
            let dir = (player_pos - pickup_pos).normalize_or_zero();
            let pull = if telek_active {
                900.0 * telek_mult
            } else {
                460.0
            };
            pickup_tf.translation += (dir * pull * dt).extend(0.0);
        }

        if dist > 20.0 {
            continue;
        }

        // Chests: play the open strip first; loot grants when it finishes
        // (tick_chest_opening). The entity stops being a Pickup so it can't
        // re-trigger while opening.
        if let PickupKind::Chest(chest) = pickup.kind {
            let open_path = match chest {
                ChestKind::Weapon => "images/sprWeaponChestOpen.png",
                ChestKind::Ammo => "images/sprAmmoChestOpen.png",
                ChestKind::Rad => "images/sprRadChestOpen.png",
            };
            let closed_path = match chest {
                ChestKind::Weapon => "images/sprWeaponChest.png",
                ChestKind::Ammo => "images/sprAmmoChest.png",
                ChestKind::Rad => "images/sprRadChest.png",
            };
            let path = if catalog.has(open_path) {
                open_path
            } else {
                closed_path
            };
            if let Some(def) = catalog.anim_def(path) {
                if let Ok(mut anim) = anims.get_mut(pickup_e) {
                    anim.set_path(path, def, true);
                    anim.frame = 0;
                    anim.finished = false;
                    anim.timer = Timer::from_seconds(1.0 / def.fps.max(0.1), TimerMode::Repeating);
                } else {
                    commands
                        .entity(pickup_e)
                        .insert(crate::game::anim::SpriteAnim::oneshot(path, def));
                }
                if let Ok(mut sprite) = sprites.get_mut(pickup_e) {
                    sprite.image = asset_server.load(path.to_string());
                    sprite.rect = Some(Rect::new(0.0, 0.0, def.frame_px as f32, def.height as f32));
                }
            }
            commands.entity(pickup_e).remove::<Pickup>();
            commands.entity(pickup_e).insert(ChestOpening {
                kind: chest,
                timer: Timer::from_seconds(0.35, TimerMode::Once),
                granted: false,
            });
            commands.spawn((
                GameCleanup,
                crate::game::reactive_audio::QueuedReactiveCue(
                    crate::game::reactive_audio::ReactiveCue::ChestOpen,
                ),
            ));
            ScreenEffects::add_trauma(&mut trauma, 0.15);
            GameFeel::rumble_controller(&mut rumble, &gamepads, 0.3, 0.4, 0.15);
            audio.play_chest(&mut commands);
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
                    race_state.race,
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
                        AmmoKind::None => 0,
                        AmmoKind::Bullets => 8,
                        AmmoKind::Shells
                        | AmmoKind::Bolts
                        | AmmoKind::Explosives
                        | AmmoKind::Energy => 2,
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

                // Robot FreeAmmo: ammo pickups restore HP; ultras heal more.
                if player.free_ammo && gained > 0 {
                    let heal = match player.ultra {
                        Some(
                            UltraMutationId::RobotRefinedTaste | UltraMutationId::RobotRegurgitate,
                        ) => 2,
                        _ => 1,
                    };
                    health.hp = (health.hp + heal).min(health.max);
                    VfxSpawner::spawn_damage_number(
                        &mut commands,
                        heal,
                        player_pos,
                        Color::srgb(0.55, 0.85, 0.95),
                    );
                }

                VfxSpawner::spawn_damage_number(
                    &mut commands,
                    gained,
                    player_pos,
                    Color::srgb(0.35, 0.7, 1.0),
                );
                audio.play_pickup(&mut commands);
            }
            PickupKind::Weapon(weapon) => {
                commands.spawn((
                    GameCleanup,
                    crate::game::reactive_audio::QueuedReactiveCue(
                        crate::game::reactive_audio::ReactiveCue::WeaponPickup,
                    ),
                ));
                equip_weapon(
                    &mut commands,
                    &catalog,
                    &asset_server,
                    &mut inv,
                    weapon,
                    player_pos,
                );

                // Fish ultra — Confiscate: weapon pickups grant extra ammo.
                if matches!(player.ultra, Some(UltraMutationId::FishConfiscate)) {
                    let kind = weapon_ammo(weapon);
                    if kind != AmmoKind::None {
                        let add = ammo_pickup_amount(kind) * 2;
                        let slot = inv.ammo_mut(kind);
                        *slot = (*slot + add).min(ammo_max(kind));
                        VfxSpawner::spawn_damage_number(
                            &mut commands,
                            add,
                            player_pos,
                            Color::srgb(0.9, 0.82, 0.25),
                        );
                    }
                }

                // Robot ultra — Refined Taste: new hardware heals.
                if matches!(player.ultra, Some(UltraMutationId::RobotRefinedTaste)) {
                    health.hp = (health.hp + 1).min(health.max);
                }

                Juice::bounce_scale(&mut commands, player_e, 1.3, 0.16);
                audio.play_chest(&mut commands);
                toast.show(&format!("Picked up {}", weapon_id_name(weapon)));
            }
            PickupKind::Chest(_) => {
                // Handled above via ChestOpening; unreachable here because
                // chests convert before the despawn.
            }
        }
    }
}

/// Grants the chest loot once its open strip has played (ChestOpening).
#[allow(clippy::too_many_arguments)]
pub fn tick_chest_opening(
    time: Res<Time<Fixed>>,
    mut commands: Commands,
    catalog: Res<AssetCatalog>,
    asset_server: Res<AssetServer>,
    mut trauma: ResMut<Trauma>,
    mut flash: ResMut<FlashWhite>,
    mut _chroma: ResMut<ChromaticAberration>,
    audio: Res<GameAudio>,
    mut player_q: Query<
        (
            Entity,
            &Transform,
            &mut Player,
            &mut Health,
            &mut Inventory,
            &RaceState,
        ),
        With<Player>,
    >,
    mut q: Query<(Entity, &mut ChestOpening, &Transform)>,
    mut toast: ResMut<Toast>,
) {
    let Ok((player_e, player_tf, mut player, mut health, mut inv, race_state)) =
        player_q.single_mut()
    else {
        return;
    };
    let player_pos = player_tf.translation.truncate();

    for (e, mut opening, tf) in &mut q {
        opening.timer.tick(time.delta());
        if !opening.timer.just_finished() || opening.granted {
            continue;
        }
        opening.granted = true;
        let pos = tf.translation.truncate();
        VfxSpawner::spawn_burst(
            &mut commands,
            pos,
            24,
            Color::srgb(1.0, 0.8, 0.3),
            (100.0, 300.0),
        );
        match opening.kind {
            ChestKind::Weapon => {
                let weapon = random_weapon(&mut rand::rng());
                equip_weapon(
                    &mut commands,
                    &catalog,
                    &asset_server,
                    &mut inv,
                    weapon,
                    player_pos,
                );
                toast.show(&format!("Opened chest: {}", weapon_id_name(weapon)));
            }
            ChestKind::Ammo => {
                let mut total = 0i32;
                for ammo in [
                    AmmoKind::Bullets,
                    AmmoKind::Shells,
                    AmmoKind::Bolts,
                    AmmoKind::Explosives,
                    AmmoKind::Energy,
                ] {
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
                    if *slot < cap {
                        *slot = (*slot + ammo_pickup_amount(ammo) * 3).min(cap);
                        total += 1;
                    }
                }
                if player.free_ammo && total > 0 {
                    health.hp = (health.hp + 2).min(health.max);
                }
                toast.show(if total > 0 {
                    "Ammo refilled"
                } else {
                    "Ammo already full"
                });
            }
            ChestKind::Rad => {
                player.rads += 30;
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
                    race_state.race,
                );
            }
        }
        commands.entity(e).despawn();
    }
}

fn first_empty_weapon_slot(inv: &Inventory) -> Option<usize> {
    (0..inv.weapon_slots).find(|&i| inv.weapons[i] == WeaponId::NONE)
}

fn spawn_dropped_weapon(
    commands: &mut Commands,
    catalog: &AssetCatalog,
    asset_server: &AssetServer,
    weapon: WeaponId,
    pos: Vec2,
) {
    spawn_pickup(
        commands,
        catalog,
        asset_server,
        PickupKind::Weapon(weapon),
        pos + Vec2::new(0.0, 24.0),
    );
}

/// Equips a weapon NT-style: slot-aware for Cuz (3 slots). If an empty slot exists,
/// fill it and switch to it; otherwise drop the current weapon.
fn equip_weapon(
    commands: &mut Commands,
    catalog: &AssetCatalog,
    asset_server: &AssetServer,
    inv: &mut Inventory,
    weapon: WeaponId,
    player_pos: Vec2,
) {
    if let Some(empty) = first_empty_weapon_slot(inv) {
        inv.weapons[empty] = weapon;
        inv.current = empty;
        let def = crate::game::weapon_runtime::weapon_runtime_def(weapon);
        if def.melee.is_none() {
            let slot = inv.ammo_mut(def.ammo);
            let add = ammo_pickup_amount(def.ammo) * 2;
            *slot = (*slot + add).min(ammo_max(def.ammo));
        }
        return;
    }

    let dropped = inv.weapons[inv.current];
    if dropped != WeaponId::NONE {
        spawn_dropped_weapon(commands, catalog, asset_server, dropped, player_pos);
    }
    inv.weapons[inv.current] = weapon;

    let def = crate::game::weapon_runtime::weapon_runtime_def(weapon);
    if def.melee.is_none() {
        let slot = inv.ammo_mut(def.ammo);
        let add = ammo_pickup_amount(def.ammo) * 2;
        *slot = (*slot + add).min(ammo_max(def.ammo));
    }
}

pub fn tick_toast(time: Res<Time<Fixed>>, mut toast: ResMut<Toast>) {
    if toast.timer.duration().is_zero() {
        return;
    }
    toast.timer.tick(time.delta());
    if toast.timer.is_finished() {
        toast.text.clear();
    }
}
