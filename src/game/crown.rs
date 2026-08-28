//! Crown passives: spawn-time application, per-floor ticking behaviors
//! (Life regen, Protection shield, Love allies, Curses bullets), and
//! floor-start bonuses driven by the `FloorStarted` message.

use bevy::prelude::*;
use rand::RngExt;

use crate::game::components::*;
use crate::game::content::{
    AmmoKind, AssetCatalog, CrownKind, WeaponId, ammo_max, ammo_pickup_amount, crown_name,
};
use crate::game::pickups;

/// Apply a crown's one-time passive to the freshly built player components.
pub fn apply_crown_to_spawn(
    crown: CrownKind,
    player: &mut Player,
    health: &mut Health,
    inv: &mut Inventory,
) {
    player.crown = crown;

    match crown {
        CrownKind::None => {}

        CrownKind::Death => {
            health.max = 1;
            health.hp = 1;
            player.drop_mult += 1.0;
        }

        CrownKind::Life => {
            player.medkit_mult *= 1.25;
        }

        CrownKind::Haste => {
            player.fire_rate_mult *= 0.75;
            player.speed_mult *= 1.08;
        }

        CrownKind::Guns => {
            player.drop_mult += 1.5;
        }

        CrownKind::Hatred => {
            player.fire_rate_mult *= 0.9;
            player.spread_mult *= 1.15;
        }

        CrownKind::Blood => {
            player.drop_mult += 0.5;
            player.pickup_range += 32.0;
        }

        CrownKind::Destiny => {
            // Give a deterministic stronger starter if the player enters with no stored weapon.
            if inv.weapons[1] == WeaponId::NONE {
                inv.weapons[1] = WeaponId(17); // Assault Rifle
            }
        }

        CrownKind::Love => {
            player.medkit_mult *= 1.1;
        }

        CrownKind::Risk => {
            player.drop_mult += 2.5;
            player.medkit_mult *= 0.65;
        }

        CrownKind::Curses => {
            player.drop_mult += 1.0;
            player.spread_mult *= 1.08;
        }

        CrownKind::Luck => {
            player.lucky_shot = true;
            player.drop_mult += 0.4;
        }

        CrownKind::Protection => {
            player.shield_on_hit = true;
        }
    }
}

/// Crown of Life: regenerate 1 HP every couple of seconds.
pub fn tick_crown_life(
    time: Res<Time<Fixed>>,
    mut q: Query<(&mut CrownState, &mut Health), With<Player>>,
) {
    for (mut state, mut health) in &mut q {
        if state.crown != CrownKind::Life {
            continue;
        }

        state.life_timer.tick(time.delta());

        if state.life_timer.just_finished() && health.hp < health.max {
            health.hp = (health.hp + 1).min(health.max);
        }
    }
}

/// Crown of Protection: once per floor, gain a brief shield when below half HP.
pub fn tick_crown_protection(
    mut commands: Commands,
    mut q: Query<(Entity, &mut CrownState, &mut Health, Option<&Shield>), With<Player>>,
) {
    for (entity, mut state, mut health, shield) in &mut q {
        if state.crown != CrownKind::Protection {
            continue;
        }

        if health.hp > health.max / 2 {
            state.protection_ready = true;
            continue;
        }

        if !state.protection_ready {
            continue;
        }

        if shield.is_some() {
            continue;
        }

        state.protection_ready = false;
        health.invuln = Timer::from_seconds(0.75, TimerMode::Once);

        commands.entity(entity).insert(Shield {
            timer: Timer::from_seconds(1.25, TimerMode::Once),
        });
    }
}

/// Crown of Love: periodically spawn a friendly ally.
pub fn tick_crown_love(
    time: Res<Time<Fixed>>,
    mut commands: Commands,
    mut q: Query<(&mut CrownState, &Transform), With<Player>>,
) {
    for (mut state, tf) in &mut q {
        if state.crown != CrownKind::Love {
            continue;
        }

        state.love_timer.tick(time.delta());

        if !state.love_timer.just_finished() {
            continue;
        }

        let pos = tf.translation.truncate() + Vec2::new(40.0, 0.0);

        commands.spawn((
            GameCleanup,
            LevelCleanup,
            Ally {
                life: Timer::from_seconds(18.0, TimerMode::Once),
                shoot: Timer::from_seconds(0.45, TimerMode::Repeating),
            },
            Team::Player,
            Health {
                hp: 10,
                max: 10,
                invuln: Timer::from_seconds(0.35, TimerMode::Once),
            },
            Hitbox { radius: 10.0 },
            Velocity(Vec2::ZERO),
            Sprite {
                color: Color::srgb(1.0, 0.35, 0.65),
                custom_size: Some(Vec2::splat(18.0)),
                ..default()
            },
            Transform::from_translation(pos.extend(18.0)),
        ));
    }
}

/// Crown of Curses: periodic hostile bullets rain on the player.
pub fn tick_crown_curses(
    time: Res<Time<Fixed>>,
    mut commands: Commands,
    mut q: Query<(&mut CrownState, &Transform), With<Player>>,
) {
    for (mut state, tf) in &mut q {
        if state.crown != CrownKind::Curses {
            continue;
        }

        state.curses_timer.tick(time.delta());

        if !state.curses_timer.just_finished() {
            continue;
        }

        let pos = tf.translation.truncate();
        let mut rng = rand::rng();

        for _ in 0..4 {
            let dir = Vec2::from_angle(rng.random_range(0.0..std::f32::consts::TAU));
            commands.spawn((
                GameCleanup,
                LevelCleanup,
                Projectile {
                    damage: 2,
                    life: Timer::from_seconds(0.75, TimerMode::Once),
                    radius: 4.0,
                    knockback: 10.0,
                    explosive: false,
                    source: None,
                },
                Team::Enemy,
                Velocity(dir * 230.0),
                Sprite {
                    color: Color::srgb(0.55, 0.25, 0.85),
                    custom_size: Some(Vec2::splat(7.0)),
                    ..default()
                },
                Transform::from_translation((pos + dir * 80.0).extend(13.0)),
            ));
        }
    }
}

/// Floor-start bonuses for Destiny / Risk / Guns, fired by `FloorStarted`.
pub fn crown_floor_start_bonus(
    mut events: MessageReader<FloorStarted>,
    mut commands: Commands,
    catalog: Res<AssetCatalog>,
    asset_server: Res<AssetServer>,
    mut q: Query<(&Player, &mut CrownState, &mut Inventory, &Transform), With<Player>>,
) {
    let mut started: Option<FloorStarted> = None;
    for event in events.read() {
        started = Some(*event);
    }
    let Some(start) = started else {
        return;
    };

    for (player, mut state, mut inv, tf) in &mut q {
        match player.crown {
            CrownKind::Destiny => {
                // One stronger weapon per run: only the first floor start.
                if !state.destiny_ready {
                    continue;
                }
                state.destiny_ready = false;

                let pool = [
                    WeaponId::ASSAULT_RIFLE,
                    WeaponId::CROSSBOW,
                    WeaponId::GRENADE_LAUNCHER,
                    WeaponId(38),  // Flak Cannon
                    WeaponId(58),  // Lightning Rifle
                    WeaponId(72),  // Toxic Launcher
                    WeaponId(104), // Super Disc
                ];

                let idx = ((tf.translation.x.abs() as usize)
                    + player.level as usize * 3
                    + start.floor as usize)
                    % pool.len();
                let slot = inv.current.min(inv.weapon_slots.saturating_sub(1));
                inv.weapons[slot] = pool[idx];
            }

            CrownKind::Risk => {
                // Risk front-loads ammo pressure: give a one-time small refill
                // on floor start, then rely on the lower medkit tradeoff.
                for ammo in [
                    AmmoKind::Bullets,
                    AmmoKind::Shells,
                    AmmoKind::Bolts,
                    AmmoKind::Explosives,
                    AmmoKind::Energy,
                ] {
                    let slot = inv.ammo_mut(ammo);
                    *slot = (*slot + ammo_pickup_amount(ammo)).min(player.ammo_cap(ammo));
                }
            }

            CrownKind::Guns => {
                // Extra weapon drop at floor start - but not inside secret
                // reward areas where weapons already rain.
                if crate::game::secret_areas::is_secret_area(start.area) {
                    continue;
                }
                pickups::spawn_pickup(
                    &mut commands,
                    &catalog,
                    &asset_server,
                    PickupKind::Weapon(WeaponId::ASSAULT_RIFLE),
                    tf.translation.truncate() + Vec2::new(36.0, -28.0),
                );
            }

            _ => {}
        }
    }
}

pub fn crown_name_for_toast(crown: CrownKind) -> &'static str {
    crown_name(crown.to_u8())
}

/// Crown Vault pedestal: touching it swaps the player's active crown.
pub fn tick_crown_pedestal(
    mut commands: Commands,
    mut toast: ResMut<Toast>,
    mut save: ResMut<crate::save::SaveData>,
    selected: Res<crate::game::SelectedCharacter>,
    mut q_player: Query<
        (
            &Transform,
            &mut Player,
            &mut Health,
            &mut Inventory,
            &mut CrownState,
        ),
        With<Player>,
    >,
    pedestals: Query<(Entity, &Transform, &CrownPedestal)>,
) {
    let Ok((ptf, mut player, mut health, mut inv, mut state)) = q_player.single_mut() else {
        return;
    };
    let p = ptf.translation.truncate();
    for (e, tf, ped) in &pedestals {
        if tf.translation.truncate().distance(p) > 28.0 {
            continue;
        }
        apply_crown_to_spawn(ped.kind, &mut player, &mut health, &mut inv);
        *state = CrownState::new(ped.kind);
        // scrCrownUnlock persists per-race crowngot and auto-equips. The
        // save/grid use GML crwn_* ids; CrownKind is the port numbering.
        save.unlock_crown(
            selected.0,
            crate::game::content::crown_port_to_gml(ped.kind.to_u8()),
        );
        toast.show(&format!(
            "{} TAKEN",
            crown_name_for_toast(ped.kind).to_ascii_uppercase()
        ));
        commands.entity(e).despawn();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_player() -> Player {
        Player {
            pickup_range: 80.0,
            ..Default::default()
        }
    }

    fn base_health() -> Health {
        Health {
            hp: 8,
            max: 8,
            invuln: Timer::from_seconds(0.0, TimerMode::Once),
        }
    }

    fn base_inv() -> Inventory {
        Inventory {
            weapons: [WeaponId::REVOLVER, WeaponId::NONE, WeaponId::NONE],
            weapon_slots: 2,
            current: 0,
            ammo: [0; MAX_AMMO_TYPES],
        }
    }

    #[test]
    fn crown_death_sets_one_hp() {
        let mut p = base_player();
        let mut h = base_health();
        let mut inv = base_inv();

        apply_crown_to_spawn(CrownKind::Death, &mut p, &mut h, &mut inv);

        assert_eq!(p.crown, CrownKind::Death);
        assert_eq!(h.max, 1);
        assert_eq!(h.hp, 1);
    }

    #[test]
    fn crown_haste_reduces_fire_cooldown_multiplier() {
        let mut p = base_player();
        let mut h = base_health();
        let mut inv = base_inv();

        apply_crown_to_spawn(CrownKind::Haste, &mut p, &mut h, &mut inv);

        assert!(p.fire_rate_mult < 1.0);
    }

    #[test]
    fn crown_luck_enables_lucky_shot() {
        let mut p = base_player();
        let mut h = base_health();
        let mut inv = base_inv();

        apply_crown_to_spawn(CrownKind::Luck, &mut p, &mut h, &mut inv);

        assert!(p.lucky_shot);
        assert_eq!(p.crown, CrownKind::Luck);
    }

    #[test]
    fn crown_destiny_fills_empty_stored_slot() {
        let mut p = base_player();
        let mut h = base_health();
        let mut inv = base_inv();

        assert_eq!(inv.weapons[1], WeaponId::NONE);
        apply_crown_to_spawn(CrownKind::Destiny, &mut p, &mut h, &mut inv);
        assert_ne!(inv.weapons[1], WeaponId::NONE);
    }
}
