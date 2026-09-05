//! Pickups: rads, medkits, ammo, weapon drops, and chests. Includes magnet
//! attraction (base range, Eyes passive, Telekinesis active) and collection.

use crate::game::audio::GameAudio;
use crate::game::combat::random_weapon;
use crate::game::components::*;
use crate::game::content::*;
use crate::game::input::NtInput;
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

/// GML portal level-end pickup behavior (runs before `collect_pickups`):
/// - `WepPickup/Collision_Portal`: weapons touching the portal become
///   persistent (carried to the next floor via `PortalCarriedWeapons`).
/// - `Rad/Step_0`: while a Portal exists rads target the player regardless
///   of distance (`mp_potential_step 12` = 360px/s); rads touching the
///   portal are pulled onto the player so normal collection grants them.
pub fn portal_pickup_carry(
    time: Res<Time<Fixed>>,
    mut commands: Commands,
    portal_q: Query<&Transform, (With<Portal>, Without<Pickup>)>,
    mut carried: ResMut<PortalCarriedWeapons>,
    player_q: Query<&Transform, With<Player>>,
    mut pickups: Query<(Entity, &mut Transform, &Pickup), (Without<Player>, Without<Portal>)>,
) {
    let Ok(portal_tf) = portal_q.single() else {
        return;
    };
    let Ok(player_tf) = player_q.single() else {
        return;
    };
    let portal_pos = portal_tf.translation.truncate();
    let player_pos = player_tf.translation.truncate();
    let dt = time.delta_secs();

    for (e, mut tf, pickup) in &mut pickups {
        let ppos = tf.translation.truncate();
        match pickup.kind {
            PickupKind::Weapon(w) => {
                if ppos.distance(portal_pos) < 20.0 {
                    carried.0.push(w);
                    commands.entity(e).despawn();
                }
            }
            PickupKind::Rad(_) => {
                // Global magnet while the portal is open.
                let dir = (player_pos - ppos).normalize_or_zero();
                tf.translation += (dir * 360.0 * dt).extend(0.0);
                // Portal touch: pull onto the player so `collect_pickups`
                // grants it next tick (mirrors place_meeting Portal collect).
                if ppos.distance(portal_pos) < 20.0 {
                    tf.translation.x = player_pos.x;
                    tf.translation.y = player_pos.y;
                }
            }
            _ => {}
        }
    }
}

pub fn spawn_pickup(
    commands: &mut Commands,
    catalog: &AssetCatalog,
    asset_server: &AssetServer,
    kind: PickupKind,
    pos: Vec2,
) -> Entity {
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
                let mut anim = crate::game::anim::SpriteAnim::new(path.clone(), def);
                anim.timer = Timer::from_seconds(1.0 / 12.0, TimerMode::Repeating);
                anim.frame = rng.random_range(0..def.frames.max(1));
                ec.insert(anim);
            }
            // GML Rad Alarm 300 =10s ; Bevy 5s drifted – fix to 10s
            ec.insert(PickupLifetime {
                timer: Timer::from_seconds(10.0 + rng.random_range(0.0..1.0), TimerMode::Once),
            });
        }
        PickupKind::Medkit(_) | PickupKind::Ammo(..) => {
            // GML HP Ammo Alarm 400 =13.3s
            ec.insert(PickupLifetime {
                timer: Timer::from_seconds(13.33 + rng.random_range(0.0..1.0), TimerMode::Once),
            });
        }
        PickupKind::Weapon(_) => {
            // GML scrWeaponPickupCreate(has_ammo=true) for fresh/chest drops:
            // one ammo bonus, consumed on first touch. Swap drops override to
            // dry in spawn_dropped_weapon.
            ec.insert(WepPickupAmmo(true));
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
    e
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
    mut input: ResMut<NtInput>,
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
            Option<&WepPickupAmmo>,
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
    let interact_pressed = input.take_interact_pressed();

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

    // Nearest weapon for press-to-pick (original: instance_nearest + press_pick)
    let mut nearest_weapon: Option<(Entity, f32)> = None;
    for (e, tf, pickup, _, _, _) in pickups.iter() {
        if matches!(pickup.kind, PickupKind::Weapon(_)) {
            let d = player_pos.distance(tf.translation.truncate());
            if d < 28.0 && nearest_weapon.is_none_or(|(_, bd)| d < bd) {
                nearest_weapon = Some((e, d));
            }
        }
    }

    for (pickup_e, mut pickup_tf, pickup, ground, mut lifetime, wep_ammo) in &mut pickups {
        let pickup_pos = pickup_tf.translation.truncate();
        let dist = player_pos.distance(pickup_pos);

        // Ground items slide with friction and spin while moving (WepPickup
        // Step_0: image_angle += rotspeed * speed * 2, friction 0.4).
        if let Some(mut gp) = ground {
            let speed = gp.vel.length();
            if speed > 0.5 {
                pickup_tf.translation += (gp.vel * dt).extend(0.0);
                pickup_tf.rotate_z(gp.rotspeed * speed * dt * 2.0);
                gp.vel *= 0.4_f32.powf(dt * crate::app::NT_SIM_HZ as f32);
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

        // Chests never fly to the player (upstream: open on contact / shock).
        // Weapons: only telekinesis pulls them to the player (original
        // WepPickup has no player magnet); portal drag + carry live in
        // progression::portal_attract and portal_pickup_carry.
        let is_chest = matches!(pickup.kind, PickupKind::Chest(_));
        let is_weapon = matches!(pickup.kind, PickupKind::Weapon(_));
        let is_rad = matches!(pickup.kind, PickupKind::Rad(_));
        if is_weapon {
            if telek_active && dist < magnet {
                let dir = (player_pos - pickup_pos).normalize_or_zero();
                pickup_tf.translation += (dir * 900.0 * telek_mult * dt).extend(0.0);
            }
        } else if is_rad {
            // GML rad range 80 (+60 plutonium hunger). Portal-global magnet
            // lives in portal_pickup_carry (runs before this system).
            let has_hunger = player
                .mutations
                .contains(&MutationId::PlutoniumHunger);
            let rad_range = 80.0 + if has_hunger { 60.0 } else { 0.0 };
            let magnet_to_player = dist < rad_range || (telek_active && dist < magnet);
            if magnet_to_player {
                let dir = (player_pos - pickup_pos).normalize_or_zero();
                // GML mp_potential_step 12px/step = 360px/s.
                let pull = if telek_active { 900.0 * telek_mult } else { 360.0 };
                pickup_tf.translation += (dir * pull * dt).extend(0.0);
            }
        } else if !is_chest && dist < magnet {
            let dir = (player_pos - pickup_pos).normalize_or_zero();
            let pull = if telek_active {
                900.0 * telek_mult
            } else {
                460.0
            };
            pickup_tf.translation += (dir * pull * dt).extend(0.0);
        }

        // Weapons require press-to-pick (original: press_pick + nearest check)
        if is_weapon {
            if dist > 28.0 {
                continue;
            }
            if nearest_weapon.is_none_or(|(e, _)| e != pickup_e) {
                continue;
            }
            if !interact_pressed {
                continue;
            }
        } else if dist > 20.0 {
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
                progression::try_recharge_strong_spirit(&mut player, &health);
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
                    progression::try_recharge_strong_spirit(&mut player, &health);
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
                // GML one-shot `ammo` flag: fresh drops grant once, swap drops
                // (dry) grant nothing. Bevy has no autopick pickups, so the
                // GotWeapon toast below always shows (GML `!autopick` branch).
                let has_ammo = wep_ammo.is_some_and(|f| f.0);
                equip_weapon(
                    &mut commands,
                    &catalog,
                    &asset_server,
                    &mut inv,
                    weapon,
                    player_pos,
                    &player,
                    &mut health,
                    has_ammo,
                );

                // Fish ultra - Confiscate: weapon pickups grant extra ammo.
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

                // Robot ultra - Refined Taste: new hardware heals.
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
/// Shock path reuses the same corpse swap (loot spawned by caller).
pub fn open_chest_shock(
    commands: &mut Commands,
    catalog: &AssetCatalog,
    asset_server: &AssetServer,
    anims: &mut Query<&mut crate::game::anim::SpriteAnim>,
    sprites: &mut Query<&mut Sprite>,
    e: Entity,
    kind: ChestKind,
) {
    open_chest(commands, catalog, asset_server, anims, sprites, e, kind);
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
    // GML Player swap drops pass no ammo flag: re-pickup grants nothing.
    let e = spawn_pickup(
        commands,
        catalog,
        asset_server,
        PickupKind::Weapon(weapon),
        pos + Vec2::new(0.0, 24.0),
    );
    commands.entity(e).insert(WepPickupAmmo(false));
}

/// Equips a weapon NT-style: slot-aware for Cuz (3 slots). If an empty slot exists,
/// fill it and switch to it; otherwise drop the current weapon.
/// `has_ammo` is the pickup's one-shot GML `ammo` flag: fresh/chest drops grant
/// one `2x` ammo bonus with a small popup, swap drops grant nothing.
fn equip_weapon(
    commands: &mut Commands,
    catalog: &AssetCatalog,
    asset_server: &AssetServer,
    inv: &mut Inventory,
    weapon: WeaponId,
    player_pos: Vec2,
    player: &Player,
    health: &mut Health,
    has_ammo: bool,
) {
    if let Some(empty) = first_empty_weapon_slot(inv) {
        inv.weapons[empty] = weapon;
        inv.current = empty;
        grant_pickup_ammo(commands, inv, weapon, player_pos, player, health, has_ammo);
        return;
    }

    let dropped = inv.weapons[inv.current];
    if dropped != WeaponId::NONE {
        spawn_dropped_weapon(commands, catalog, asset_server, dropped, player_pos);
    }
    inv.weapons[inv.current] = weapon;

    grant_pickup_ammo(commands, inv, weapon, player_pos, player, health, has_ammo);
}

/// GML `Collision_WepPickup` tail: `if other.ammo && type != None`.
/// Crown of Protection converts the bonus to healing
/// (`1 + second stomach`), otherwise `2x` ammo with a small amount popup.
fn grant_pickup_ammo(
    commands: &mut Commands,
    inv: &mut Inventory,
    weapon: WeaponId,
    player_pos: Vec2,
    player: &Player,
    health: &mut Health,
    has_ammo: bool,
) {
    let def = crate::game::weapon_runtime::weapon_runtime_def(weapon);
    let second_stomach = player.mutations.contains(&MutationId::SecondStomach);
    match weapon_pickup_grant(
        has_ammo,
        def.melee.is_some(),
        player.crown,
        second_stomach,
    ) {
        WeaponPickupGrant::Nothing => {}
        WeaponPickupGrant::Heal(heal) => {
            health.hp = (health.hp + heal).min(health.max);
            VfxSpawner::spawn_damage_number(
                commands,
                heal,
                player_pos,
                Color::srgb(0.3, 1.0, 0.3),
            );
        }
        WeaponPickupGrant::Ammo => {
            let slot = inv.ammo_mut(def.ammo);
            let add = ammo_pickup_amount(def.ammo) * 2;
            let gained = add.min(player.ammo_cap(def.ammo) - *slot).max(0);
            *slot += gained;
            VfxSpawner::spawn_damage_number(
                commands,
                gained,
                player_pos,
                Color::srgb(0.35, 0.7, 1.0),
            );
        }
    }
}

/// Pure GML `other.ammo` truth table (unit-tested below): dry or melee
/// pickups grant nothing; Protection crown heals instead of granting ammo.
#[derive(Debug, PartialEq, Eq)]
enum WeaponPickupGrant {
    Nothing,
    Heal(i32),
    Ammo,
}

fn weapon_pickup_grant(
    has_ammo: bool,
    melee: bool,
    crown: CrownKind,
    second_stomach: bool,
) -> WeaponPickupGrant {
    if !has_ammo || melee {
        return WeaponPickupGrant::Nothing;
    }
    if crown == CrownKind::Protection {
        return WeaponPickupGrant::Heal(1 + i32::from(second_stomach));
    }
    WeaponPickupGrant::Ammo
}

#[cfg(test)]
mod weapon_pickup_grant_tests {
    use super::*;

    #[test]
    fn dry_swap_drops_grant_nothing() {
        assert_eq!(
            weapon_pickup_grant(false, false, CrownKind::None, false),
            WeaponPickupGrant::Nothing
        );
    }

    #[test]
    fn melee_never_grants_ammo() {
        assert_eq!(
            weapon_pickup_grant(true, true, CrownKind::None, false),
            WeaponPickupGrant::Nothing
        );
    }

    #[test]
    fn fresh_ranged_drop_grants_ammo() {
        assert_eq!(
            weapon_pickup_grant(true, false, CrownKind::None, false),
            WeaponPickupGrant::Ammo
        );
    }

    #[test]
    fn protection_crown_heals_instead() {
        assert_eq!(
            weapon_pickup_grant(true, false, CrownKind::Protection, false),
            WeaponPickupGrant::Heal(1)
        );
        assert_eq!(
            weapon_pickup_grant(true, false, CrownKind::Protection, true),
            WeaponPickupGrant::Heal(2)
        );
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

/// Rad chest walk-contact - upstream `RadChest/Collision_Player.gml`:
///
/// `if !scrChestOpened() { hp = 0 }` → `Destroy_0` drops 25 rads.
/// Bullets also set `hp -= damage` via `hitme`. Bevy keeps both paths.
pub fn tick_rad_container_contact(
    mut commands: Commands,
    catalog: Res<AssetCatalog>,
    asset_server: Res<AssetServer>,
    audio: Res<GameAudio>,
    player_q: Query<&Transform, With<Player>>,
    mut rad_q: Query<(Entity, &Transform, &Prop), With<RadChestContainer>>,
) {
    let Ok(player_tf) = player_q.single() else {
        return;
    };
    let player_pos = player_tf.translation.truncate();
    for (e, tf, prop) in &mut rad_q {
        let center = tf.translation.truncate();
        let half = prop.size * 0.5;
        // AABB contact - mirrors `place_meeting(x,y,Player)` in GML (16x16 msk)
        let closest = Vec2::new(
            player_pos.x.clamp(center.x - half.x, center.x + half.x),
            player_pos.y.clamp(center.y - half.y, center.y + half.y),
        );
        if player_pos.distance(closest) > crate::game::components::PLAYER_RADIUS + 2.0 {
            // also allow simple radius check for center overlap
            if player_pos.distance(center) > half.x + crate::game::components::PLAYER_RADIUS + 4.0 {
                continue;
            }
        }
        // Open on contact - same payload as bullet destroy
        commands.entity(e).despawn();
        for _ in 0..25 {
            let ang = rand::rng().random_range(0.0..std::f32::consts::TAU);
            let d = rand::rng().random_range(6.0..26.0);
            spawn_pickup(
                &mut commands,
                &catalog,
                &asset_server,
                PickupKind::Rad(1),
                center + Vec2::new(ang.cos() * d, ang.sin() * d),
            );
        }
        // Upstream Destroy spawns 4 Smoke + ExploderExplo + sndEXPChest
        audio.play_boom(&mut commands);
        // small VFX burst to mirror Smoke
        game_utils_bevy::vfx::VfxSpawner::spawn_burst(
            &mut commands,
            center,
            8,
            Color::srgb(0.55, 0.55, 0.60),
            (40.0, 120.0),
        );
    }
}
