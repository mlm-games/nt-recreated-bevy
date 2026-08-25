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
use rand::RngExt;

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
    let (path, _size) = pickup_sprite(kind, catalog);
    // Upstream pickup image speeds: Rad spins at 0.4/frame (12 fps @30) from
    // a random start frame; HP/ammo strips are static (image_speed 0).
    let mut rng = rand::rng();
    let mut ec = commands.spawn((
        GameCleanup,
        LevelCleanup,
        Pickup { kind },
        sprite_exact(catalog, asset_server, &path),
        Transform::from_translation(pos.extend(8.0)),
    ));
    match kind {
        PickupKind::Rad(_) => {
            if let Some(def) = catalog.anim_def(&path) {
                let mut anim = crate::game::anim::SpriteAnim::new(&leak(path), def);
                anim.timer = Timer::from_seconds(1.0 / 12.0, TimerMode::Repeating);
                anim.frame = rng.random_range(0..def.frames.max(1));
                ec.insert(anim);
            }
            ec.insert(PickupLifetime {
                timer: Timer::from_seconds(5.0 + rng.random_range(0.0..1.0), TimerMode::Once),
            });
        }
        PickupKind::Medkit(_) | PickupKind::Ammo(..) => {
            ec.insert(PickupLifetime {
                timer: Timer::from_seconds(6.67 + rng.random_range(0.0..1.0), TimerMode::Once),
            });
        }
        PickupKind::Weapon(_) => {
            // WepPickup: random resting angle + a small pop with spin.
            let ang = rng.random_range(0.0..std::f32::consts::TAU);
            ec.insert(GroundPhysics {
                vel: Vec2::new(ang.cos(), ang.sin()) * rng.random_range(15.0..45.0),
                rotspeed: rng.random_range(0.7..1.0)
                    * if rng.random_bool(0.5) { 1.0 } else { -1.0 },
            });
            ec.insert(Transform::from_translation(pos.extend(8.0)).with_rotation(
                Quat::from_rotation_z(rng.random_range(0.0..std::f32::consts::TAU)),
            ));
        }
        PickupKind::Chest(_) => {}
    }
    let e = ec.id();
    Juice::pop_in(commands, e, 0.14);
}

/// Leak pickup sprite paths so `SpriteAnim` can hold them as &'static str.
fn leak(path: String) -> &'static str {
    Box::leak(path.into_boxed_str())
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
    mut pickups: Query<
        (
            Entity,
            &mut Transform,
            &Pickup,
            Option<&mut GroundPhysics>,
            Option<&mut PickupLifetime>,
        ),
        Without<Player>,
    >,
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

    for (pickup_e, mut pickup_tf, pickup, ground, mut lifetime) in &mut pickups {
        let pickup_pos = pickup_tf.translation.truncate();
        let dist = player_pos.distance(pickup_pos);

        // Ground items slide with friction and spin while moving (WepPickup
        // Step_0: image_angle += rotspeed * speed * 2, friction 0.4).
        if let Some(mut gp) = ground {
            let speed = gp.vel.length();
            if speed > 0.5 {
                pickup_tf.translation += (gp.vel * dt).extend(0.0);
                pickup_tf.rotate_z(gp.rotspeed * speed * dt * 2.0);
                gp.vel *= 0.4_f32.powf(dt * 30.0);
            } else {
                gp.vel = Vec2::ZERO;
            }
        }

        // Rad/HP/ammo blink out and despawn after their upstream lifetime.
        if let Some(mut lt) = lifetime {
            lt.timer.tick(time.delta());
            if lt.timer.just_finished() {
                audio.play_pickup_disappear(&mut commands);
                commands.entity(pickup_e).despawn();
                continue;
            }
            if lt.timer.remaining_secs() < 1.0
                && let Ok(mut s) = sprites.get_mut(pickup_e)
            {
                let a = 0.35 + 0.65 * (0.5 + 0.5 * (time.elapsed_secs() * 30.0).sin());
                s.color.set_alpha(a);
            }
        }

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

        // Chests (upstream Collision_Player): grant loot INSTANTLY, swap to
        // the open-corpse sprite (stays on the floor), never despawn here.
        if let PickupKind::Chest(chest) = pickup.kind {
            open_chest(
                &mut commands,
                &catalog,
                &asset_server,
                &mut anims,
                &mut sprites,
                pickup_e,
                chest,
            );
            match chest {
                ChestKind::Weapon => {
                    // WeaponChest/Collision_Player: spawn the ground weapon at
                    // the chest, sndWeaponChest.
                    let weapon = random_weapon(&mut rand::rng());
                    spawn_pickup(
                        &mut commands,
                        &catalog,
                        &asset_server,
                        PickupKind::Weapon(weapon),
                        pickup_pos,
                    );
                    audio.play_weapon_chest(&mut commands);
                    toast.show(&format!("{}", weapon_id_name(weapon)));
                }
                ChestKind::Ammo => {
                    // AmmoChest/Collision_Player: scrAmmoDecideType x2 direct
                    // give, sndAmmoChest.
                    let ammo = decide_ammo_type(&inv);
                    let amount = ammo_pickup_amount(ammo) * 2;
                    let cap = player.ammo_cap(ammo);
                    let slot = inv.ammo_mut(ammo);
                    let gained = (*slot + amount).min(cap) - *slot;
                    *slot += gained;
                    VfxSpawner::spawn_damage_number(
                        &mut commands,
                        gained,
                        player_pos,
                        Color::srgb(0.35, 0.7, 1.0),
                    );
                    audio.play_ammo_chest(&mut commands);
                    toast.show("Ammo refilled");
                }
                ChestKind::Rad => {
                    // RadChest: hp=0 -> corpse + raddrop 25 rads burst.
                    for _ in 0..25 {
                        let ang = rand::rng().random_range(0.0..std::f32::consts::TAU);
                        let d = rand::rng().random_range(6.0..26.0);
                        spawn_pickup(
                            &mut commands,
                            &catalog,
                            &asset_server,
                            PickupKind::Rad(1),
                            pickup_pos + Vec2::new(ang.cos() * d, ang.sin() * d),
                        );
                    }
                    audio.play_pickup(&mut commands);
                }
            }
            ScreenEffects::add_trauma(&mut trauma, 0.15);
            GameFeel::rumble_controller(&mut rumble, &gamepads, 0.3, 0.4, 0.15);
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
                let cap = player.ammo_cap(ammo);
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
                    &player,
                );

                // Fish ultra — Confiscate: weapon pickups grant extra ammo.
                if matches!(player.ultra, Some(UltraMutationId::FishConfiscate)) {
                    let kind = weapon_ammo(weapon);
                    if kind != AmmoKind::None {
                        let add = ammo_pickup_amount(kind) * 2;
                        let slot = inv.ammo_mut(kind);
                        *slot = (*slot + add).min(player.ammo_cap(kind));
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
                // Handled above: chests grant instantly and stay as corpses.
            }
        }
    }
}

/// Swaps a chest to its open-corpse art (frozen on the last frame) and marks
/// it opened. Upstream: spr_dead = sprXxxOpen, ChestOpen plays at 0.4 and
/// freezes on image_number-1; RadChest's corpse is sprRadChestCorpse.
fn open_chest(
    commands: &mut Commands,
    catalog: &AssetCatalog,
    asset_server: &AssetServer,
    anims: &mut Query<&mut crate::game::anim::SpriteAnim>,
    sprites: &mut Query<&mut Sprite>,
    e: Entity,
    kind: ChestKind,
) {
    let (path, last_frame) = match kind {
        ChestKind::Weapon => ("images/sprWeaponChestOpen.png", 0),
        ChestKind::Ammo => ("images/sprAmmoChestOpen.png", 0),
        // RadChest spr_dead is the corpse strip; freeze on its last frame.
        ChestKind::Rad => ("images/sprRadChestCorpse.png", 2),
    };
    let path = if catalog.has(path) { path } else { "" };
    if !path.is_empty()
        && let Some(def) = catalog.anim_def(path)
    {
        let frame = last_frame.min(def.frames.saturating_sub(1)) as f32;
        let fw = def.frame_px as f32;
        let fh = def.height as f32;
        if let Ok(mut anim) = anims.get_mut(e) {
            anim.set_path(path, def, true);
            anim.frame = frame as u32;
            anim.finished = true;
        } else {
            let mut a = crate::game::anim::SpriteAnim::oneshot(path, def);
            a.frame = frame as u32;
            a.finished = true;
            commands.entity(e).insert(a);
        }
        if let Ok(mut sprite) = sprites.get_mut(e) {
            sprite.image = asset_server.load(path.to_string());
            sprite.rect = Some(Rect::new(frame * fw, 0.0, (frame + 1.0) * fw, fh));
        }
    }
    commands.entity(e).remove::<Pickup>();
    commands.entity(e).insert(OpenedChest);
}

/// scrAmmoDecideType: primary weapon's type first (while not full), then the
/// stored weapon's, else a random type.
fn decide_ammo_type(inv: &Inventory) -> AmmoKind {
    let types = [
        weapon_ammo(inv.weapons[inv.current]),
        weapon_ammo(inv.weapons[1.min(inv.weapon_slots - 1)]),
    ];
    for ty in types {
        if ty != AmmoKind::None && inv.ammo_of(ty) < ammo_max(ty) {
            return ty;
        }
    }
    match rand::rng().random_range(1..=5) {
        1 => AmmoKind::Bullets,
        2 => AmmoKind::Shells,
        3 => AmmoKind::Bolts,
        4 => AmmoKind::Explosives,
        _ => AmmoKind::Energy,
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
    player: &Player,
) {
    if let Some(empty) = first_empty_weapon_slot(inv) {
        inv.weapons[empty] = weapon;
        inv.current = empty;
        let def = crate::game::weapon_runtime::weapon_runtime_def(weapon);
        if def.melee.is_none() {
            let slot = inv.ammo_mut(def.ammo);
            let add = ammo_pickup_amount(def.ammo) * 2;
            *slot = (*slot + add).min(player.ammo_cap(def.ammo));
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
        *slot = (*slot + add).min(player.ammo_cap(def.ammo));
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
