//! Projectile archetypes: special behaviors beyond bounce/pierce/hazard/split
//! (homing, sticky, chain lightning, sentry deployment, custom explosions,
//! beams, HP-ammo, weapon-pickup payloads).
//!
//! Deliberately keyed by generated-registry NAME and kept OUTSIDE `WeaponDef`
//! so the runtime registry stays identity/timing/presentation only. Golden /
//! Ultra / Cursed variants inherit their base family's archetype.

use bevy::prelude::*;

use crate::game::components::{
    BloodAmmo, ChainLightning, CustomExplosion, DeploysSentry, Homing, SpawnsWeaponPickup, Sticky,
};
use crate::game::content::WeaponId;
use crate::game::weapons_data::WEAPONS;

/// Beam archetype used by Ion Cannon / Laser Cannon.
///
/// This is a firing-mode override, not a projectile component.
#[derive(Clone, Copy, Debug)]
pub struct BeamSpec {
    pub length: f32,
    pub width: f32,
    pub damage: i32,
    pub knockback: f32,
    pub duration: f32,
    pub tick: f32,
    pub color: Color,
}

/// Plasma family secondary-burst profile.
#[derive(Clone, Copy, Debug)]
pub struct PlasmaBurstSpec {
    pub pellets: u8,
    pub speed: f32,
    pub damage: i32,
    pub lifetime: f32,
    pub radius: f32,
    pub knockback: f32,
    pub color: Color,
    pub size: Vec2,
}

/// One archetype bundle per weapon.
///
/// This deliberately sits outside `WeaponDef` so the generated runtime registry
/// stays stable and fully metadata-driven.
#[derive(Clone, Copy, Debug, Default)]
pub struct ProjectileArchetype {
    pub sticky: Option<Sticky>,
    pub homing: Option<Homing>,
    pub chain_lightning: Option<ChainLightning>,
    pub deploys_sentry: Option<DeploysSentry>,
    pub custom_explosion: Option<CustomExplosion>,
    pub blood_ammo: Option<BloodAmmo>,
    /// `Some(weapon)` => always drop that weapon.
    /// `None` inside `Some(...)` means roll a random weapon.
    pub spawn_weapon_pickup: Option<SpawnsWeaponPickup>,
    pub beam: Option<BeamSpec>,
    pub plasma_burst: Option<PlasmaBurstSpec>,
    pub hits_all_teams: bool,
}

/// Strip GOLDEN/ULTRA/CURSED so variants inherit their family's archetype.
pub fn base_weapon_name(name: &str) -> &str {
    name.strip_prefix("GOLDEN ")
        .or_else(|| name.strip_prefix("ULTRA "))
        .or_else(|| name.strip_prefix("CURSED "))
        .unwrap_or(name)
}

fn archetyped(name: &str) -> ProjectileArchetype {
    match name {
        "SENTRY GUN" => ProjectileArchetype {
            deploys_sentry: Some(DeploysSentry {
                life: 14.0,
                fire_interval: 0.18,
                range: 360.0,
                projectile_speed: 640.0,
                projectile_damage: 3,
            }),
            ..default()
        },

        "SMART GUN" | "SEEKER PISTOL" | "SEEKER SHOTGUN" => ProjectileArchetype {
            homing: Some(Homing {
                turn_rate: if name == "SMART GUN" { 8.0 } else { 5.5 },
                acquire_range: 420.0,
            }),
            ..default()
        },

        "STICKY LAUNCHER" => ProjectileArchetype {
            sticky: Some(Sticky::default()),
            custom_explosion: Some(CustomExplosion { radius: 170.0 }),
            ..default()
        },

        "NUKE LAUNCHER" => ProjectileArchetype {
            custom_explosion: Some(CustomExplosion { radius: 340.0 }),
            ..default()
        },

        "ION CANNON" => ProjectileArchetype {
            beam: Some(BeamSpec {
                length: 760.0,
                width: 28.0,
                damage: 18,
                knockback: 120.0,
                duration: 0.2,
                tick: 1.0 / 30.0,
                color: Color::srgb(0.52, 0.85, 1.0),
            }),
            ..default()
        },

        "LASER CANNON" => ProjectileArchetype {
            beam: Some(BeamSpec {
                length: 820.0,
                width: 24.0,
                damage: 18,
                knockback: 100.0,
                duration: 0.18,
                tick: 1.0 / 30.0,
                color: Color::srgb(1.0, 0.18, 0.14),
            }),
            ..default()
        },

        "BLOOD LAUNCHER" => ProjectileArchetype {
            blood_ammo: Some(BloodAmmo { hp_cost: 1 }),
            ..default()
        },

        "BLOOD CANNON" => ProjectileArchetype {
            blood_ammo: Some(BloodAmmo { hp_cost: 2 }),
            ..default()
        },

        "GUN GUN" => ProjectileArchetype {
            spawn_weapon_pickup: Some(SpawnsWeaponPickup { weapon: None }),
            ..default()
        },

        "LIGHTNING PISTOL" | "LIGHTNING SMG" => ProjectileArchetype {
            chain_lightning: Some(ChainLightning {
                jumps_left: 1,
                range: 170.0,
                falloff: 0.8,
            }),
            ..default()
        },

        "LIGHTNING RIFLE" => ProjectileArchetype {
            chain_lightning: Some(ChainLightning {
                jumps_left: 2,
                range: 190.0,
                falloff: 0.75,
            }),
            ..default()
        },

        "LIGHTNING SHOTGUN" => ProjectileArchetype {
            chain_lightning: Some(ChainLightning {
                jumps_left: 2,
                range: 155.0,
                falloff: 0.8,
            }),
            ..default()
        },

        "LIGHTNING CANNON" => ProjectileArchetype {
            chain_lightning: Some(ChainLightning {
                jumps_left: 4,
                range: 220.0,
                falloff: 0.7,
            }),
            ..default()
        },

        "PLASMA GUN" => ProjectileArchetype {
            plasma_burst: Some(PlasmaBurstSpec {
                pellets: 4,
                speed: 300.0,
                damage: 2,
                lifetime: 0.32,
                radius: 3.0,
                knockback: 24.0,
                color: Color::srgb(0.35, 1.0, 0.42),
                size: Vec2::splat(7.0),
            }),
            ..default()
        },

        "PLASMA RIFLE" => ProjectileArchetype {
            plasma_burst: Some(PlasmaBurstSpec {
                pellets: 5,
                speed: 320.0,
                damage: 2,
                lifetime: 0.34,
                radius: 3.0,
                knockback: 24.0,
                color: Color::srgb(0.3, 1.0, 0.38),
                size: Vec2::splat(7.0),
            }),
            ..default()
        },

        "PLASMA MINIGUN" => ProjectileArchetype {
            plasma_burst: Some(PlasmaBurstSpec {
                pellets: 3,
                speed: 330.0,
                damage: 1,
                lifetime: 0.26,
                radius: 2.5,
                knockback: 16.0,
                color: Color::srgb(0.32, 1.0, 0.4),
                size: Vec2::splat(6.0),
            }),
            ..default()
        },

        "PLASMA CANNON" | "SUPER PLASMA CANNON" => ProjectileArchetype {
            plasma_burst: Some(PlasmaBurstSpec {
                pellets: 8,
                speed: 360.0,
                damage: 3,
                lifetime: 0.38,
                radius: 4.0,
                knockback: 40.0,
                color: Color::srgb(0.36, 1.0, 0.45),
                size: Vec2::splat(8.0),
            }),
            ..default()
        },

        "DEVASTATOR" => ProjectileArchetype {
            plasma_burst: Some(PlasmaBurstSpec {
                pellets: 10,
                speed: 380.0,
                damage: 4,
                lifetime: 0.42,
                radius: 4.0,
                knockback: 48.0,
                color: Color::srgb(0.38, 1.0, 0.48),
                size: Vec2::splat(9.0),
            }),
            ..default()
        },

        "DISC GUN" | "SUPER DISC GUN" | "GOLDEN DISC GUN" => ProjectileArchetype {
            hits_all_teams: true,
            ..default()
        },

        "BOUNCER SMG" | "BOUNCER SHOTGUN" => ProjectileArchetype {
            hits_all_teams: true,
            ..default()
        },

        "CROSSBOW"
        | "HEAVY CROSSBOW"
        | "AUTO CROSSBOW"
        | "SUPER CROSSBOW"
        | "HEAVY AUTO CROSSBOW"
        | "ULTRA CROSSBOW" => ProjectileArchetype {
            sticky: Some(Sticky::default()),
            ..default()
        },

        "SPLINTER GUN" | "SPLINTER PISTOL" | "SUPER SPLINTER GUN" => ProjectileArchetype {
            sticky: Some(Sticky::default()),
            ..default()
        },

        "TOXIC BOW" => ProjectileArchetype {
            sticky: Some(Sticky::default()),
            ..default()
        },

        _ => {
            // Fallback: any name containing DISC or BOUNCER gets friendly-fire (future weapons)
            if name.contains("DISC") || name.contains("BOUNCER") {
                ProjectileArchetype {
                    hits_all_teams: true,
                    ..default()
                }
            } else {
                ProjectileArchetype::default()
            }
        }
    }
}

pub fn projectile_archetype(id: WeaponId) -> ProjectileArchetype {
    let Some(meta) = WEAPONS.get(id.0 as usize) else {
        return ProjectileArchetype::default();
    };

    if meta.wep_name.is_empty() {
        return ProjectileArchetype::default();
    }

    archetyped(base_weapon_name(meta.wep_name))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id_by_name(name: &str) -> WeaponId {
        let meta = WEAPONS
            .iter()
            .find(|meta| meta.wep_name == name)
            .unwrap_or_else(|| panic!("missing generated weapon {name:?}"));

        WeaponId(meta.id)
    }

    #[test]
    fn sentry_gun_deploys_sentry() {
        let a = projectile_archetype(id_by_name("SENTRY GUN"));
        assert!(a.deploys_sentry.is_some());
        assert!(a.beam.is_none());
    }

    #[test]
    fn smart_and_seeker_guns_home() {
        assert!(
            projectile_archetype(id_by_name("SMART GUN"))
                .homing
                .is_some()
        );
        assert!(
            projectile_archetype(id_by_name("SEEKER SHOTGUN"))
                .homing
                .is_some()
        );
    }

    #[test]
    fn sticky_launcher_is_sticky() {
        let a = projectile_archetype(id_by_name("STICKY LAUNCHER"));
        assert!(a.sticky.is_some());
        assert!(a.custom_explosion.is_some());
    }

    #[test]
    fn nuke_has_custom_radius() {
        let a = projectile_archetype(id_by_name("NUKE LAUNCHER"));
        assert_eq!(a.custom_explosion.unwrap().radius, 340.0);
    }

    #[test]
    fn ion_and_laser_cannons_are_beams() {
        assert!(
            projectile_archetype(id_by_name("ION CANNON"))
                .beam
                .is_some()
        );
        assert!(
            projectile_archetype(id_by_name("LASER CANNON"))
                .beam
                .is_some()
        );
    }

    #[test]
    fn blood_weapons_use_hp() {
        assert!(
            projectile_archetype(id_by_name("BLOOD LAUNCHER"))
                .blood_ammo
                .is_some()
        );
        assert!(
            projectile_archetype(id_by_name("BLOOD CANNON"))
                .blood_ammo
                .is_some()
        );
    }

    #[test]
    fn gun_gun_spawns_random_pickup() {
        let a = projectile_archetype(id_by_name("GUN GUN"));
        assert!(a.spawn_weapon_pickup.is_some());
        assert!(a.spawn_weapon_pickup.unwrap().weapon.is_none());
    }

    #[test]
    fn lightning_family_chains() {
        for name in [
            "LIGHTNING PISTOL",
            "LIGHTNING RIFLE",
            "LIGHTNING SHOTGUN",
            "LIGHTNING SMG",
            "LIGHTNING CANNON",
        ] {
            let a = projectile_archetype(id_by_name(name));
            assert!(a.chain_lightning.is_some(), "{name}");
        }
    }

    #[test]
    fn lightning_hammer_is_not_chain_projectile() {
        let a = projectile_archetype(id_by_name("LIGHTNING HAMMER"));
        assert!(a.chain_lightning.is_none());
        assert!(a.beam.is_none());
    }

    #[test]
    fn golden_variants_inherit_base_archetype() {
        // GOLDEN NUKE LAUNCHER exists in the registry and must inherit the
        // base nuke's big blast.
        let normal = projectile_archetype(id_by_name("NUKE LAUNCHER"));
        let golden = projectile_archetype(id_by_name("GOLDEN NUKE LAUNCHER"));
        assert_eq!(
            normal.custom_explosion.map(|c| c.radius),
            golden.custom_explosion.map(|c| c.radius),
        );

        // Unrelated golden weapons stay inert.
        assert!(
            projectile_archetype(id_by_name("GOLDEN REVOLVER"))
                .chain_lightning
                .is_none()
        );
    }

    #[test]
    fn empty_id_has_no_archetype() {
        let a = projectile_archetype(WeaponId::NONE);
        assert!(a.sticky.is_none());
        assert!(a.homing.is_none());
        assert!(a.chain_lightning.is_none());
        assert!(a.deploys_sentry.is_none());
        assert!(a.custom_explosion.is_none());
        assert!(a.blood_ammo.is_none());
        assert!(a.spawn_weapon_pickup.is_none());
        assert!(a.beam.is_none());
        assert!(a.plasma_burst.is_none());
    }

    /// Sweep: every registry name whose family implies a behavior gets exactly
    /// that behavior, and nothing else accidentally lights up.
    #[test]
    fn archetype_assignment_matches_registry_names() {
        for meta in WEAPONS.iter().skip(1) {
            let id = WeaponId(meta.id);
            let a = projectile_archetype(id);
            let base = base_weapon_name(meta.wep_name);

            let wants_sticky = base.contains("STICKY")
                || base.contains("CROSSBOW")
                || base.contains("SPLINTER")
                || base.contains("TOXIC BOW");
            let wants_homing = base.contains("SMART GUN") || base.contains("SEEKER");
            let wants_chain = base.contains("LIGHTNING") && !base.contains("HAMMER");
            let wants_blood = base.contains("BLOOD LAUNCHER") || base.contains("BLOOD CANNON");
            let wants_sentry = base.contains("SENTRY");
            let wants_nuke = base.contains("NUKE");
            let wants_pickup = base.contains("GUN GUN");
            let wants_beam = base.contains("ION CANNON") || base.contains("LASER CANNON");

            assert_eq!(a.sticky.is_some(), wants_sticky, "{}", meta.wep_name);
            assert_eq!(a.homing.is_some(), wants_homing, "{}", meta.wep_name);
            assert_eq!(
                a.chain_lightning.is_some(),
                wants_chain,
                "{}",
                meta.wep_name
            );
            assert_eq!(a.blood_ammo.is_some(), wants_blood, "{}", meta.wep_name);
            assert_eq!(
                a.deploys_sentry.is_some(),
                wants_sentry,
                "{}",
                meta.wep_name
            );
            assert_eq!(
                a.custom_explosion.is_some(),
                wants_nuke || base.contains("STICKY"),
                "{}",
                meta.wep_name
            );
            assert_eq!(
                a.spawn_weapon_pickup.is_some(),
                wants_pickup,
                "{}",
                meta.wep_name
            );
            assert_eq!(a.beam.is_some(), wants_beam, "{}", meta.wep_name);

            let wants_plasma = base.contains("PLASMA") || base.contains("DEVASTATOR");
            assert_eq!(a.plasma_burst.is_some(), wants_plasma, "{}", meta.wep_name);

            if wants_nuke {
                assert!(
                    a.custom_explosion.unwrap().radius >= 180.0,
                    "{} nuke radius too small",
                    meta.wep_name
                );
            }
        }
    }

    /// Compile-time guard: the lookup only relies on stable registry fields.
    #[test]
    fn lookup_handles_out_of_range_ids() {
        let a = projectile_archetype(WeaponId(u8::MAX));
        assert!(a.sticky.is_none() && a.homing.is_none() && a.beam.is_none());
    }
}
