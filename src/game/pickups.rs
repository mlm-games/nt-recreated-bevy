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

pub fn tick_pickup_drag(
    time: Res<Time<Fixed>>,
    mut commands: Commands,
    portal_q: Query<&Transform, (With<Portal>, Without<Pickup>)>,
    mut carried: ResMut<PortalCarriedWeapons>,
    player_q: Query<(&Transform, &Player), With<Player>>,
    mask: Res<FloorMask>,
    mut pickups: Query<(Entity, &mut Transform, &Pickup), (Without<Player>, Without<Portal>)>,
) {
    let Ok((player_tf, player)) = player_q.single() else {
        return;
    };
    let player_pos = player_tf.translation.truncate();
    let portal_pos = portal_q.single().ok().map(|tf| tf.translation.truncate());
    let dt = time.delta_secs();
    let hunger = player.mutations.contains(&MutationId::PlutoniumHunger);

    let loose_range = 32.0 + if hunger { 64.0 } else { 0.0 };

    for (e, mut tf, pickup) in &mut pickups {
        let ppos = tf.translation.truncate();
        match pickup.kind {
            PickupKind::Weapon(w) => {
                if portal_pos.is_some_and(|pp| ppos.distance(pp) < 20.0) {
                    carried.0.push(w);
                    commands.entity(e).try_despawn();
                }
            }
            PickupKind::Rad(_) => {
                if portal_pos.is_none() {
                    continue;
                }

                let dir = (player_pos - ppos).normalize_or_zero();
                tf.translation += (dir * 360.0 * dt).extend(0.0);

                if ppos.distance(portal_pos.unwrap_or(player_pos)) < 20.0 {
                    tf.translation.x = player_pos.x;
                    tf.translation.y = player_pos.y;
                }
            }
            PickupKind::Ammo(..) | PickupKind::Medkit(_) => {
                let in_range = ppos.distance(player_pos) < loose_range;
                if !in_range && portal_pos.is_none() {
                    continue;
                }

                let dir = (player_pos - ppos).normalize_or_zero();
                let delta = dir * 6.0 * 30.0 * dt;
                let nx = Vec2::new(ppos.x + delta.x, ppos.y);
                if mask.is_walkable(nx) {
                    tf.translation.x = nx.x;
                }
                let ny = Vec2::new(tf.translation.x, ppos.y + delta.y);
                if mask.is_walkable(ny) {
                    tf.translation.y = ny.y;
                }

                if portal_pos.is_some_and(|pp| ppos.distance(pp) < 14.0) {
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
    loops: u32,
    hasted: bool,
) -> Entity {
    let (path, _size) = pickup_sprite(kind, catalog);

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

            ec.insert(PickupLifetime {
                timer: Timer::from_seconds(10.0 + rng.random_range(0.0..1.0), TimerMode::Once),
            });
        }
        PickupKind::Medkit(_) | PickupKind::Ammo(..) => {

            let init =
                ((200.0 + rng.random_range(0.0..30.0)) / ((5.0 + loops as f32) / 5.0)).ceil();
            let total_steps = if hasted { init / 3.0 } else { init } + 62.0;
            ec.insert(PickupLifetime {
                timer: Timer::from_seconds(total_steps / 30.0, TimerMode::Once),
            });
        }
        PickupKind::Weapon(_) => {

            ec.insert(WepPickupAmmo(true));

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

fn pickup_sprite(kind: PickupKind, _catalog: &AssetCatalog) -> (String, f32) {
    match kind {
        PickupKind::Rad(_) => ("images/sprRad.png".to_string(), 12.0),
        PickupKind::Medkit(_) => ("images/sprHP.png".to_string(), 16.0),
        PickupKind::Ammo(..) => ("images/sprAmmo.png".to_string(), 12.0),
        PickupKind::Weapon(k) => (weapon_id_sprite(k, _catalog), 20.0),
        PickupKind::Chest(kind) => match kind {
            ChestKind::Weapon => ("images/sprWeaponChest.png".to_string(), 32.0),
            ChestKind::Ammo => ("images/sprAmmoChest.png".to_string(), 32.0),
            ChestKind::Rad => ("images/sprRadChest.png".to_string(), 32.0),
        },
    }
}

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

        if let Some(mut lt) = lifetime {
            lt.timer.tick(time.delta());
            if lt.timer.just_finished() {
                audio.play_pickup_disappear(&mut commands);
                commands.entity(pickup_e).try_despawn();
                continue;
            }
            let ammo_or_hp = matches!(pickup.kind, PickupKind::Ammo(..) | PickupKind::Medkit(_));
            if ammo_or_hp && lt.timer.remaining_secs() < 62.0 / 30.0 {
                let phase = (lt.timer.elapsed_secs() * 30.0) as i32;
                let vis = if (phase / 2) % 2 == 0 {
                    Visibility::Visible
                } else {
                    Visibility::Hidden
                };
                commands.entity(pickup_e).insert(vis);
            } else if lt.timer.remaining_secs() < 1.0
                && let Ok(mut s) = sprites.get_mut(pickup_e)
            {
                let a = 0.35 + 0.65 * (0.5 + 0.5 * (time.elapsed_secs() * 30.0).sin());
                s.color.set_alpha(a);
            }
        }

        let is_chest = matches!(pickup.kind, PickupKind::Chest(_));
        let is_weapon = matches!(pickup.kind, PickupKind::Weapon(_));
        let is_rad = matches!(pickup.kind, PickupKind::Rad(_));
        let is_ammo = matches!(pickup.kind, PickupKind::Ammo(..));
        let is_medkit = matches!(pickup.kind, PickupKind::Medkit(_));
        if is_weapon {
            if telek_active && dist < magnet {
                let dir = (player_pos - pickup_pos).normalize_or_zero();
                pickup_tf.translation += (dir * 900.0 * telek_mult * dt).extend(0.0);
            }
        } else if is_ammo || is_medkit {

        } else if is_rad {

            let has_hunger = player.mutations.contains(&MutationId::PlutoniumHunger);
            let rad_range = 80.0 + if has_hunger { 60.0 } else { 0.0 };
            let magnet_to_player = dist < rad_range || (telek_active && dist < magnet);
            if magnet_to_player {
                let dir = (player_pos - pickup_pos).normalize_or_zero();

                let pull = if telek_active {
                    900.0 * telek_mult
                } else {
                    360.0
                };
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
        } else if is_ammo || is_medkit {

            if dist > 14.0 {
                continue;
            }
        } else if dist > 20.0 {
            continue;
        }

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

                    let weapon = random_weapon(&mut rand::rng());
                    spawn_pickup(
                        &mut commands,
                        &catalog,
                        &asset_server,
                        PickupKind::Weapon(weapon),
                        pickup_pos,
                        0,
                        false,
                    );
                    audio.play_weapon_chest(&mut commands);
                    toast.show(&format!("{}", weapon_id_name(weapon)));
                }
                ChestKind::Ammo => {

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

                    for _ in 0..25 {
                        let ang = rand::rng().random_range(0.0..std::f32::consts::TAU);
                        let d = rand::rng().random_range(6.0..26.0);
                        spawn_pickup(
                            &mut commands,
                            &catalog,
                            &asset_server,
                            PickupKind::Rad(1),
                            pickup_pos + Vec2::new(ang.cos() * d, ang.sin() * d),
                            0,
                            false,
                        );
                    }
                    audio.play_pickup(&mut commands);
                }
            }
            ScreenEffects::add_trauma(&mut trauma, 0.15);
            GameFeel::rumble_controller(&mut rumble, &gamepads, 0.3, 0.4, 0.15);
            continue;
        }

        commands.entity(pickup_e).try_despawn();

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
            PickupKind::Ammo(..) => {

                let ammo = decide_ammo_type(&inv);
                let mut amount = ammo_pickup_amount(ammo);
                if player.crown == crate::game::content::CrownKind::Haste {
                    amount += 1;
                }
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

                let type_name = ammo_type_name(ammo);
                if *slot >= cap {
                    toast.show(&format!("MAX {type_name}"));
                } else {
                    toast.show(&format!("+{gained} {type_name}"));
                }
                audio.play_pickup(&mut commands);
            }
            PickupKind::Weapon(weapon) => {
                commands.spawn((
                    GameCleanup,
                    crate::game::reactive_audio::QueuedReactiveCue(
                        crate::game::reactive_audio::ReactiveCue::WeaponPickup,
                    ),
                ));

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

                if matches!(player.ultra, Some(UltraMutationId::RobotRefinedTaste)) {
                    health.hp = (health.hp + 1).min(health.max);
                }

                Juice::bounce_scale(&mut commands, player_e, 1.3, 0.16);
                audio.play_chest(&mut commands);
                toast.show(&format!("Picked up {}", weapon_id_name(weapon)));
            }
            PickupKind::Chest(_) => {

            }
        }
    }
}

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

#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub enum WeaponLabelRole {
    Name,
    GaugeBg,
    Gauge,
    Prompt,
}

#[derive(Component)]
pub struct WeaponLabelPart {
    role: WeaponLabelRole,
}

#[derive(Resource, Default)]
pub struct WeaponLabelTarget(pub Option<Entity>);

fn ammo_gauge_paths(kind: AmmoKind) -> Option<(&'static str, &'static str)> {
    match kind {
        AmmoKind::Bullets => Some(("images/sprBulletIcon.png", "images/sprBulletIconBG.png")),
        AmmoKind::Shells => Some(("images/sprShotIcon.png", "images/sprShotIconBG.png")),
        AmmoKind::Bolts => Some(("images/sprBoltIcon.png", "images/sprBoltIconBG.png")),
        AmmoKind::Explosives => Some(("images/sprExploIcon.png", "images/sprExploIconBG.png")),
        AmmoKind::Energy => Some(("images/sprEnergyIcon.png", "images/sprEnergyIconBG.png")),
        AmmoKind::None => None,
    }
}

fn gauge_frame(fill: f32) -> usize {

    (7.0 - (7.0 * fill.clamp(0.0, 1.0)).ceil()).clamp(0.0, 7.0) as usize
}

#[allow(clippy::too_many_arguments)]
pub fn sync_weapon_label(
    mut commands: Commands,
    catalog: Res<AssetCatalog>,
    asset_server: Res<AssetServer>,
    font: Res<crate::app::UiFont>,
    player_q: Query<(&Transform, &Inventory, &Player), With<Player>>,
    weapon_q: Query<
        (Entity, &Transform, &Pickup),
        (Without<Player>, Without<Portal>, Without<WeaponLabelPart>),
    >,
    mut target: ResMut<WeaponLabelTarget>,
    mut parts: Query<
        (Entity, &WeaponLabelPart, &mut Transform),
        (Without<Pickup>, Without<Player>),
    >,
) {
    let Some((player_tf, inv, player)) = player_q.single().ok() else {
        return;
    };
    let player_pos = player_tf.translation.truncate();

    let mut best: Option<(Entity, WeaponId, Vec2)> = None;
    for (e, tf, pickup) in &weapon_q {
        let PickupKind::Weapon(w) = pickup.kind else {
            continue;
        };
        let p = tf.translation.truncate();
        let d = player_pos.distance(p);
        if d < 18.0 && best.is_none_or(|(_, _, bp)| d < player_pos.distance(bp)) {
            best = Some((e, w, p));
        }
    }

    if best.map(|(e, _, _)| e) != target.0 {
        for (e, _, _) in &parts {
            commands.entity(e).try_despawn();
        }
        target.0 = None;
        if let Some((gun_e, weapon, gun_pos)) = best {
            commands.spawn((
                GameCleanup,
                LevelCleanup,
                WeaponLabelPart {
                    role: WeaponLabelRole::Name,
                },
                Text2d::new(weapon_id_name(weapon).to_string()),
                TextFont {
                    font: font.0.clone().into(),
                    font_size: FontSize::Px(8.0),
                    ..default()
                },
                TextColor(Color::WHITE),
                TextLayout::justify(Justify::Center),
                Transform::from_translation((gun_pos + Vec2::new(0.0, 31.0)).extend(14.0)),
            ));

            let wtype = weapon_ammo(weapon);
            if let Some((icon, bg)) = ammo_gauge_paths(wtype)
                && catalog.has(icon)
                && catalog.has(bg)
            {
                let cap = player.ammo_cap(wtype).max(1) as f32;
                let fill = inv.ammo_of(wtype) as f32 / cap;
                let mut bg_sprite =
                    crate::game::content::sprite_exact_frame(&catalog, &asset_server, bg, 2);
                bg_sprite.color = Color::WHITE;
                commands.spawn((
                    GameCleanup,
                    LevelCleanup,
                    WeaponLabelPart {
                        role: WeaponLabelRole::GaugeBg,
                    },
                    bg_sprite,
                    Transform::from_translation((gun_pos + Vec2::new(14.0, 21.0)).extend(14.0)),
                ));
                let mut fg_sprite = crate::game::content::sprite_exact_frame(
                    &catalog,
                    &asset_server,
                    icon,
                    gauge_frame(fill),
                );
                fg_sprite.color = Color::WHITE;
                commands.spawn((
                    GameCleanup,
                    LevelCleanup,
                    WeaponLabelPart {
                        role: WeaponLabelRole::Gauge,
                    },
                    fg_sprite,
                    Transform::from_translation((gun_pos + Vec2::new(14.0, 21.0)).extend(14.5)),
                ));
            }

            commands.spawn((
                GameCleanup,
                LevelCleanup,
                WeaponLabelPart {
                    role: WeaponLabelRole::Prompt,
                },
                Text2d::new("E".to_string()),
                TextFont {
                    font: font.0.clone().into(),
                    font_size: FontSize::Px(8.0),
                    ..default()
                },
                TextColor(Color::srgb(1.0, 0.9, 0.4)),
                TextLayout::justify(Justify::Center),
                Transform::from_translation(gun_pos.extend(14.0)),
            ));
            target.0 = Some(gun_e);
        }
        return;
    }

    let Some((_, weapon, gun_pos)) = best else {
        return;
    };
    let wtype = weapon_ammo(weapon);
    let cap = player.ammo_cap(wtype).max(1) as f32;
    let fill = inv.ammo_of(wtype) as f32 / cap;
    for (e, part, mut tf) in &mut parts {
        match part.role {
            WeaponLabelRole::Name => {
                tf.translation.x = gun_pos.x;
                tf.translation.y = gun_pos.y + 31.0;
            }
            WeaponLabelRole::GaugeBg => {
                tf.translation.x = gun_pos.x + 14.0;
                tf.translation.y = gun_pos.y + 21.0;
            }
            WeaponLabelRole::Gauge => {
                tf.translation.x = gun_pos.x + 14.0;
                tf.translation.y = gun_pos.y + 21.0;
                if let Some((icon, _)) = ammo_gauge_paths(wtype) {
                    let mut s = crate::game::content::sprite_exact_frame(
                        &catalog,
                        &asset_server,
                        icon,
                        gauge_frame(fill),
                    );
                    s.color = Color::WHITE;
                    commands.entity(e).insert(s);
                }
            }
            WeaponLabelRole::Prompt => {
                tf.translation.x = gun_pos.x;
                tf.translation.y = gun_pos.y;
            }
        }
    }
}

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

pub fn ammo_type_name(kind: AmmoKind) -> &'static str {
    match kind {
        AmmoKind::None => "NONE",
        AmmoKind::Bullets => "BULLETS",
        AmmoKind::Shells => "SHELLS",
        AmmoKind::Bolts => "BOLTS",
        AmmoKind::Explosives => "EXPLOSIVES",
        AmmoKind::Energy => "ENERGY",
    }
}

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

    let e = spawn_pickup(
        commands,
        catalog,
        asset_server,
        PickupKind::Weapon(weapon),
        pos + Vec2::new(0.0, 24.0),
        0,
        false,
    );
    commands.entity(e).insert(WepPickupAmmo(false));
}

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
    match weapon_pickup_grant(has_ammo, def.melee.is_some(), player.crown, second_stomach) {
        WeaponPickupGrant::Nothing => {}
        WeaponPickupGrant::Heal(heal) => {
            health.hp = (health.hp + heal).min(health.max);
            VfxSpawner::spawn_damage_number(commands, heal, player_pos, Color::srgb(0.3, 1.0, 0.3));
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

        let closest = Vec2::new(
            player_pos.x.clamp(center.x - half.x, center.x + half.x),
            player_pos.y.clamp(center.y - half.y, center.y + half.y),
        );
        if player_pos.distance(closest) > crate::game::components::PLAYER_RADIUS + 2.0 {

            if player_pos.distance(center) > half.x + crate::game::components::PLAYER_RADIUS + 4.0 {
                continue;
            }
        }

        commands.entity(e).try_despawn();
        for _ in 0..25 {
            let ang = rand::rng().random_range(0.0..std::f32::consts::TAU);
            let d = rand::rng().random_range(6.0..26.0);
            spawn_pickup(
                &mut commands,
                &catalog,
                &asset_server,
                PickupKind::Rad(1),
                center + Vec2::new(ang.cos() * d, ang.sin() * d),
                0,
                false,
            );
        }

        audio.play_boom(&mut commands);

        game_utils_bevy::vfx::VfxSpawner::spawn_burst(
            &mut commands,
            center,
            8,
            Color::srgb(0.55, 0.55, 0.60),
            (40.0, 120.0),
        );
    }
}

#[cfg(test)]
mod ammo_label_parity_tests {
    use super::*;
    use bevy::asset::AssetPlugin;
    use bevy::time::TimeUpdateStrategy;

    fn harness() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()));
        app.init_asset::<Image>();
        app.insert_resource(TimeUpdateStrategy::FixedTimesteps(1));
        app.insert_resource(Time::<Fixed>::from_hz(crate::app::NT_SIM_HZ));
        app.init_resource::<Toast>();
        app.init_resource::<PortalCarriedWeapons>();
        app.init_resource::<WeaponLabelTarget>();
        app.insert_resource(crate::app::UiFont(Handle::default()));
        let mut mask = FloorMask::default();
        for cx in -2..=8 {
            mask.cells.insert((cx, 0));
        }
        app.insert_resource(mask);
        let mut catalog = AssetCatalog::default();
        for p in [
            "images/sprAmmo.png",
            "images/sprBulletIcon.png",
            "images/sprBulletIconBG.png",
            "images/sprRevolver.png",
        ] {
            catalog.images.insert(p.to_string());
        }
        app.insert_resource(catalog);
        app.add_systems(FixedUpdate, (tick_pickup_drag, sync_weapon_label));
        app
    }

    fn spawn_player(app: &mut App, pos: Vec2) {
        app.world_mut().spawn((
            Player::default(),
            Velocity(Vec2::ZERO),
            Inventory {
                weapons: [WeaponId::NONE; MAX_WEAPON_SLOTS],
                weapon_slots: 2,
                current: 0,
                ammo: [0; MAX_AMMO_TYPES],
            },
            Transform::from_translation(pos.extend(20.0)),
        ));
    }

    #[test]
    fn ammo_drifts_at_gml_rate_not_vacuum() {
        let mut app = harness();
        spawn_player(&mut app, Vec2::ZERO);

        let near = app
            .world_mut()
            .spawn((
                Pickup {
                    kind: PickupKind::Ammo(AmmoKind::None, 0),
                },
                Transform::from_translation(Vec2::new(25.0, 0.0).extend(8.0)),
            ))
            .id();

        let far = app
            .world_mut()
            .spawn((
                Pickup {
                    kind: PickupKind::Ammo(AmmoKind::None, 0),
                },
                Transform::from_translation(Vec2::new(60.0, 0.0).extend(8.0)),
            ))
            .id();
        let dist = |app: &App, e: Entity| {
            app.world()
                .get::<Transform>(e)
                .map(|tf| tf.translation.truncate().distance(Vec2::ZERO))
                .unwrap_or(f32::MAX)
        };
        for _ in 0..2 {
            app.update();
        }

        let d = dist(&app, near);
        assert!(d < 25.0 && d > 5.0, "ammo drift wrong: {d}");
        assert!(
            (dist(&app, far) - 60.0).abs() < 0.01,
            "out-of-range ammo must not move"
        );
    }

    #[test]
    fn ammo_uses_shared_box_sprite_and_loop_lifetime() {
        #[derive(Resource, Default)]
        struct SpawnedAmmo(Option<Entity>);
        fn spawn_test_ammo(
            mut commands: Commands,
            catalog: Res<AssetCatalog>,
            asset_server: Res<AssetServer>,
            mut out: ResMut<SpawnedAmmo>,
        ) {
            out.0 = Some(spawn_pickup(
                &mut commands,
                &catalog,
                &asset_server,
                PickupKind::Ammo(AmmoKind::Shells, 99),
                Vec2::ZERO,
                0,
                false,
            ));
        }
        let mut app = harness();
        app.init_resource::<SpawnedAmmo>();
        app.add_systems(Startup, spawn_test_ammo);
        app.update();
        let asset_server = app.world().resource::<AssetServer>().clone();
        let e = app.world().resource::<SpawnedAmmo>().0.unwrap();

        let sprite = app.world().get::<Sprite>(e).unwrap();
        assert_eq!(
            sprite.image,
            asset_server.load::<Image>("images/sprAmmo.png"),
            "ammo must render the shared sprAmmo box"
        );

        let secs = app
            .world()
            .get::<PickupLifetime>(e)
            .unwrap()
            .timer
            .duration()
            .as_secs_f32();
        assert!(
            (8.7..9.8).contains(&secs),
            "loop-0 ammo lifetime wrong: {secs}"
        );
        assert_eq!(ammo_pickup_amount(AmmoKind::Energy), 10);
    }

    #[test]
    fn weapon_label_shows_name_and_gauge_on_overlap() {
        let mut app = harness();
        spawn_player(&mut app, Vec2::ZERO);
        app.world_mut().spawn((
            Pickup {
                kind: PickupKind::Weapon(WeaponId::REVOLVER),
            },
            GroundPhysics {
                vel: Vec2::ZERO,
                rotspeed: 0.8,
            },
            Transform::from_translation(Vec2::new(10.0, 0.0).extend(8.0)),
        ));
        for _ in 0..3 {
            app.update();
        }

        let parts: Vec<(WeaponLabelRole, Vec2)> = app
            .world_mut()
            .query::<(&WeaponLabelPart, &Transform)>()
            .iter(app.world())
            .map(|(part, tf)| (part.role, tf.translation.truncate()))
            .collect();
        let names = parts
            .iter()
            .filter(|(role, _)| *role == WeaponLabelRole::Name)
            .count();
        let prompts = parts
            .iter()
            .filter(|(role, _)| *role == WeaponLabelRole::Prompt)
            .count();
        let gauges = parts
            .iter()
            .filter(|(role, _)| {
                *role == WeaponLabelRole::Gauge || *role == WeaponLabelRole::GaugeBg
            })
            .count();
        for (role, pos) in &parts {
            if *role == WeaponLabelRole::Name {
                assert!((pos.y - 31.0).abs() < 0.01);
            }
        }

        let texts: Vec<(WeaponLabelRole, String)> = app
            .world_mut()
            .query::<(&WeaponLabelPart, &Text2d)>()
            .iter(app.world())
            .map(|(part, txt)| (part.role, txt.0.clone()))
            .collect();
        let name_ok = texts.iter().any(|(role, txt)| {
            *role == WeaponLabelRole::Name && txt == weapon_id_name(WeaponId::REVOLVER)
        });
        assert_eq!(names, 1, "exactly one name label expected");
        assert_eq!(prompts, 1, "interact prompt expected");
        assert_eq!(gauges, 2, "bg + fill gauge expected");
        assert!(name_ok, "label must show the weapon name");

        {
            let mut players = app
                .world_mut()
                .query_filtered::<&mut Transform, With<Player>>();
            for mut tf in players.iter_mut(app.world_mut()) {
                tf.translation.x = 200.0;
            }
        }
        for _ in 0..3 {
            app.update();
        }
        let left = app
            .world_mut()
            .query::<&WeaponLabelPart>()
            .iter(app.world())
            .count();
        assert_eq!(left, 0, "label must hide away from guns");
    }
}
