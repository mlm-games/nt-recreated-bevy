use bevy::prelude::{Color, Vec2};

use crate::game::content::{
    AmmoKind, HazardDef, HazardKind, MeleeDef, SplitDef, WeaponDef, WeaponId, WeaponKind,
    sanitize_weapon_id, weapon_def, weapon_meta,
};
use crate::game::weapons_data::{AmmoType, WeaponData};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)] // exposed via generated/weapons_runtime; consumed by archetypes
pub enum ProjectileKind {
    Bullet,
    Shell,
    Bolt,
    Explosive,
    Energy,
    Melee,
}

#[derive(Clone, Copy, Debug)]
#[allow(dead_code)] // exposed via generated/weapons_runtime; consumed by archetypes
pub struct ExplosionSpec {
    pub radius: f32,
    pub damage: i32,
}

#[derive(Clone, Copy, Debug)]
#[allow(dead_code)] // exposed via generated/weapons_runtime; consumed by archetypes
pub struct MeleeSpec {
    pub range: f32,
    pub arc: f32,
}

#[derive(Clone, Copy, Debug)]
#[allow(dead_code)] // exposed via generated/weapons_runtime; consumed by archetypes
pub struct WeaponRuntime {
    pub projectile_kind: ProjectileKind,
    pub pellets: u8,
    pub spread_deg: f32,
    pub speed: f32,
    pub lifetime_frames: u16,
    pub damage: i32,
    pub recoil: f32,
    pub explosion: Option<ExplosionSpec>,
    pub melee: Option<MeleeSpec>,
    pub cooldown_frames: u16,
    pub automatic: bool,
}

/// Broad runtime class inferred from the generated weapon registry.
///
/// This ensures every valid weapon receives a meaningful runtime even when it
/// does not yet have a bespoke projectile entity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WeaponFamily {
    Empty,
    MeleeLight,
    MeleeHeavy,
    Pistol,
    Automatic,
    BurstRifle,
    Shotgun,
    Slugger,
    Crossbow,
    Splinter,
    Disc,
    Explosive,
    Flame,
    Laser,
    Plasma,
    Lightning,
    Toxic,
    Deployable,
    Novelty,
}

#[allow(dead_code)]
pub fn weapon_family(id: WeaponId) -> WeaponFamily {
    let id = sanitize_weapon_id(id);
    if id == WeaponId::NONE {
        return WeaponFamily::Empty;
    }

    family_for(weapon_meta(id))
}

fn family_for(meta: &WeaponData) -> WeaponFamily {
    let name = meta.wep_name;

    if name.is_empty() {
        return WeaponFamily::Empty;
    }

    // Melee must be checked first because several energy/lightning weapons
    // are melee weapons despite containing ranged-family words.
    if meta.wep_mele {
        if name.contains("HAMMER")
            || name.contains("SHOVEL")
            || name.contains("SLEDGE")
            || name.contains("GUITAR")
            || name.contains("BAT")
        {
            return WeaponFamily::MeleeHeavy;
        }

        return WeaponFamily::MeleeLight;
    }

    if name.contains("SENTRY") {
        return WeaponFamily::Deployable;
    }

    if name.contains("TOXIC") {
        return WeaponFamily::Toxic;
    }

    if name.contains("FLAME")
        || name.contains("FLARE")
        || name == "DRAGON"
        || name.contains("INCINERATOR")
    {
        return WeaponFamily::Flame;
    }

    if name.contains("LIGHTNING") {
        return WeaponFamily::Lightning;
    }

    if name.contains("PLASMA") || name.contains("DEVASTATOR") {
        return WeaponFamily::Plasma;
    }

    if name.contains("LASER") || name.contains("ION CANNON") {
        return WeaponFamily::Laser;
    }

    if name.contains("DISC GUN") {
        return WeaponFamily::Disc;
    }

    if name.contains("FLAK") {
        return WeaponFamily::Explosive;
    }

    if name.contains("SPLINTER") {
        return WeaponFamily::Splinter;
    }

    if name.contains("SLUGGER") {
        return WeaponFamily::Slugger;
    }

    if name.contains("CROSSBOW") || name.ends_with(" BOW") {
        return WeaponFamily::Crossbow;
    }

    if name.contains("SHOTGUN") || name == "WAVE GUN" {
        return WeaponFamily::Shotgun;
    }

    if name.contains("GRENADE")
        || name.contains("BAZOOKA")
        || name.contains("LAUNCHER")
        || name.contains("NUKE")
        || name.contains("JACKHAMMER")
        || name.contains("CLUSTER")
    {
        return WeaponFamily::Explosive;
    }

    if name.contains("RIFLE") {
        return WeaponFamily::BurstRifle;
    }

    if name.contains("MACHINEGUN") || name.contains("MINIGUN") || name.contains("SMG") {
        return WeaponFamily::Automatic;
    }

    if name.contains("PISTOL")
        || name.contains("REVOLVER")
        || name == "SMART GUN"
        || name == "POP GUN"
    {
        return WeaponFamily::Pistol;
    }

    match meta.wep_type {
        AmmoType::None => WeaponFamily::Novelty,
        AmmoType::Bullets => {
            if meta.wep_auto {
                WeaponFamily::Automatic
            } else {
                WeaponFamily::Pistol
            }
        }
        AmmoType::Shells => WeaponFamily::Shotgun,
        AmmoType::Bolts => WeaponFamily::Crossbow,
        AmmoType::Explosives => WeaponFamily::Explosive,
        AmmoType::Energy => {
            if meta.wep_auto {
                WeaponFamily::Laser
            } else {
                WeaponFamily::Plasma
            }
        }
    }
}

/// Upstream Sleep() is frames @30fps. Return seconds for HitStop.
pub fn weapon_sleep_secs(id: WeaponId) -> f32 {
    let fam = weapon_family(id);
    let meta = weapon_meta(sanitize_weapon_id(id));
    let name = meta.wep_name;

    let frames: f32 = match base_weapon_name(name) {
        "REVOLVER" | "PISTOL" => 2.0,
        "MACHINEGUN" | "SMG" => 1.0,
        "ASSAULT RIFLE" => 3.0,
        "SHOTGUN" | "DOUBLE SHOTGUN" | "SAWED-OFF SHOTGUN" => 5.0,
        "SLUGGER" | "HEAVY SLUGGER" => 6.0,
        "CROSSBOW" | "HEAVY CROSSBOW" => 4.0,
        "GRENADE LAUNCHER" | "BAZOOKA" | "NUKE LAUNCHER" => 8.0,
        "SUPER PLASMA CANNON" | "PLASMA CANNON" => 10.0,
        "LIGHTNING HAMMER" | "HAMMER" | "SLEDGEHAMMER" => 8.0,
        "SCREWDRIVER" | "WRENCH" | "GUITAR" => 4.0,
        "LASER RIFLE" | "LASER PISTOL" => 2.0,
        "ENERGY SWORD" | "ENERGY SCREWDRIVER" => 5.0,
        _ => match fam {
            WeaponFamily::Empty => 0.0,
            WeaponFamily::Automatic | WeaponFamily::Pistol => 1.5,
            WeaponFamily::BurstRifle => 3.0,
            WeaponFamily::Shotgun => 5.0,
            WeaponFamily::Slugger | WeaponFamily::Crossbow => 5.0,
            WeaponFamily::Explosive => 7.0,
            WeaponFamily::Plasma | WeaponFamily::Laser => 4.0,
            WeaponFamily::Lightning | WeaponFamily::Flame => 3.0,
            WeaponFamily::MeleeLight => 3.0,
            WeaponFamily::MeleeHeavy => 7.0,
            WeaponFamily::Disc | WeaponFamily::Splinter => 4.0,
            WeaponFamily::Toxic | WeaponFamily::Deployable | WeaponFamily::Novelty => 2.0,
        },
    };

    let mult = if name.starts_with("ULTRA ") {
        1.25
    } else if name.starts_with("CURSED ") {
        1.1
    } else {
        1.0
    };

    (frames * mult / 30.0).clamp(0.0, 0.45)
}

fn base_weapon_name(name: &str) -> &str {
    let stripped = name
        .strip_prefix("ULTRA ")
        .or_else(|| name.strip_prefix("CURSED "))
        .or_else(|| name.strip_prefix("GOLDEN "))
        .unwrap_or(name);
    stripped
}

#[allow(dead_code)]
pub fn weapon_runtime(id: WeaponId) -> WeaponRuntime {
    let id = sanitize_weapon_id(id);

    if id == WeaponId::NONE {
        return WeaponRuntime {
            projectile_kind: ProjectileKind::Melee,
            pellets: 0,
            spread_deg: 0.0,
            speed: 0.0,
            lifetime_frames: 0,
            damage: 0,
            recoil: 0.0,
            explosion: None,
            melee: None,
            cooldown_frames: 1,
            automatic: false,
        };
    }

    let meta = weapon_meta(id);
    let def = weapon_runtime_def(id);

    WeaponRuntime {
        projectile_kind: projectile_kind_for(&def),
        pellets: def.pellets.min(u8::MAX as usize) as u8,
        spread_deg: def.spread,
        speed: def.speed,
        lifetime_frames: seconds_to_frames(def.lifetime),
        damage: def.damage,
        recoil: def.recoil,
        explosion: def.explosive.then_some(ExplosionSpec {
            // The current Projectile component has a boolean explosive flag
            // and uses the common explosion path. Per-weapon radii belong in
            // the subsequent projectile-archetype patch.
            radius: 130.0,
            damage: def.damage,
        }),
        melee: def.melee.map(|melee| MeleeSpec {
            range: melee.range,
            arc: melee.arc,
        }),
        cooldown_frames: meta.wep_load.max(1),
        automatic: meta.wep_auto,
    }
}

#[allow(dead_code)]
fn seconds_to_frames(seconds: f32) -> u16 {
    (seconds.max(0.0) * 30.0)
        .round()
        .clamp(0.0, u16::MAX as f32) as u16
}

#[allow(dead_code)]
fn projectile_kind_for(def: &WeaponDef) -> ProjectileKind {
    if def.melee.is_some() {
        return ProjectileKind::Melee;
    }

    if def.explosive {
        return ProjectileKind::Explosive;
    }

    match def.ammo {
        AmmoKind::None => ProjectileKind::Melee,
        AmmoKind::Bullets => ProjectileKind::Bullet,
        AmmoKind::Shells => ProjectileKind::Shell,
        AmmoKind::Bolts => ProjectileKind::Bolt,
        AmmoKind::Explosives => ProjectileKind::Explosive,
        AmmoKind::Energy => ProjectileKind::Energy,
    }
}

fn ammo_kind(meta: &WeaponData) -> AmmoKind {
    match meta.wep_type {
        AmmoType::None => AmmoKind::None,
        AmmoType::Bullets => AmmoKind::Bullets,
        AmmoType::Shells => AmmoKind::Shells,
        AmmoType::Bolts => AmmoKind::Bolts,
        AmmoType::Explosives => AmmoKind::Explosives,
        AmmoType::Energy => AmmoKind::Energy,
    }
}

pub fn weapon_runtime_def(id: WeaponId) -> WeaponDef {
    let id = sanitize_weapon_id(id);

    if id == WeaponId::NONE {
        return weapon_def(WeaponKind::None);
    }

    let meta = weapon_meta(id);
    let legacy: WeaponKind = id.into();

    let mut def = if legacy != WeaponKind::None {
        let mut legacy_def = weapon_def(legacy);

        // The hand-authored legacy runtime owns its actual behavior, but the
        // generated registry remains authoritative for identity and timing.
        legacy_def.name = meta.wep_name;
        legacy_def.ammo = ammo_kind(meta);
        legacy_def.ammo_cost = i32::from(meta.wep_cost);
        legacy_def.rad_cost = u32::from(meta.wep_rads);
        legacy_def.cooldown = f32::from(meta.wep_load.max(1)) / 30.0;
        legacy_def.automatic = meta.wep_auto;
        legacy_def
    } else {
        metadata_base_def(meta)
    };

    if legacy == WeaponKind::None {
        apply_family_profile(&mut def, family_for(meta), meta);
    }
    apply_exact_profile(&mut def, meta);
    apply_variant_tuning(&mut def, meta);
    normalize_def(&mut def, meta);

    def
}

fn metadata_base_def(meta: &WeaponData) -> WeaponDef {
    WeaponDef {
        name: meta.wep_name,
        ammo: ammo_kind(meta),
        ammo_cost: i32::from(meta.wep_cost),
        rad_cost: u32::from(meta.wep_rads),
        cooldown: f32::from(meta.wep_load.max(1)) / 30.0,
        damage: 3,
        pellets: 1,
        speed: 480.0,
        lifetime: 1.0,
        spread: 0.07,
        recoil: 3.0,
        shake: 0.08,
        projectile_radius: 4.0,
        knockback: 90.0,
        automatic: meta.wep_auto,
        explosive: false,
        burst_shots: 1,
        burst_interval: 0.0,
        melee: None,
        color: Color::srgb(0.9, 0.9, 0.9),
        size: Vec2::splat(12.0),
        muzzle_burst: 2,
        bounces: 0,
        pierce: 0,
        hazard: None,
        split: None,
    }
}

fn apply_family_profile(def: &mut WeaponDef, family: WeaponFamily, meta: &WeaponData) {
    let cost = i32::from(meta.wep_cost.max(1));
    let area = i32::from(meta.wep_area.max(0));

    match family {
        WeaponFamily::Empty => {}

        WeaponFamily::MeleeLight => {
            set_melee(
                def,
                7 + cost * 2 + area / 5,
                66.0,
                2.05,
                2.5,
                Color::srgb(0.82, 0.84, 0.88),
            );
        }

        WeaponFamily::MeleeHeavy => {
            set_melee(
                def,
                14 + cost * 3 + area / 4,
                80.0,
                2.45,
                5.5,
                Color::srgb(0.88, 0.78, 0.48),
            );
        }

        WeaponFamily::Pistol => {
            set_ranged(
                def,
                3 + cost / 2,
                1,
                560.0,
                0.85,
                0.06,
                3.0,
                4.0,
                75.0,
                Color::srgb(0.95, 0.9, 0.65),
                Vec2::new(10.0, 3.0),
            );
        }

        WeaponFamily::Automatic => {
            set_ranged(
                def,
                3,
                1,
                610.0,
                0.72,
                0.11,
                2.2,
                3.5,
                48.0,
                Color::srgb(1.0, 0.88, 0.5),
                Vec2::new(10.0, 3.0),
            );
        }

        WeaponFamily::BurstRifle => {
            set_ranged(
                def,
                4 + cost / 3,
                1,
                650.0,
                0.82,
                0.055,
                4.0,
                4.0,
                72.0,
                Color::srgb(1.0, 0.9, 0.52),
                Vec2::new(13.0, 3.0),
            );

            if cost >= 3 {
                def.burst_shots = 3;
                def.burst_interval = 2.0 / 30.0;
            }
        }

        WeaponFamily::Shotgun => {
            set_ranged(
                def,
                2,
                (6 + cost * 2) as usize,
                400.0,
                0.34,
                0.31,
                7.0,
                3.5,
                30.0,
                Color::srgb(1.0, 0.82, 0.36),
                Vec2::new(8.0, 3.0),
            );
        }

        WeaponFamily::Slugger => {
            set_ranged(
                def,
                14 + cost * 2,
                1,
                510.0,
                0.7,
                0.07,
                9.0,
                6.0,
                170.0,
                Color::srgb(0.95, 0.83, 0.42),
                Vec2::new(14.0, 5.0),
            );
        }

        WeaponFamily::Crossbow => {
            set_ranged(
                def,
                10 + cost * 2,
                1,
                660.0,
                1.05,
                0.025,
                6.0,
                4.0,
                135.0,
                Color::srgb(0.96, 0.84, 0.45),
                Vec2::new(17.0, 4.0),
            );

            def.pierce = cost.saturating_sub(1).min(5) as u8;
        }

        WeaponFamily::Splinter => {
            set_ranged(
                def,
                3,
                (4 + cost).clamp(4, 10) as usize,
                600.0,
                0.58,
                0.18,
                4.0,
                3.0,
                42.0,
                Color::srgb(0.68, 0.48, 0.32),
                Vec2::new(10.0, 3.0),
            );
        }

        WeaponFamily::Disc => {
            set_ranged(
                def,
                7 + cost,
                1,
                430.0,
                2.4,
                0.025,
                3.5,
                8.0,
                125.0,
                Color::srgb(0.68, 0.94, 1.0),
                Vec2::splat(14.0),
            );

            def.bounces = (5 + cost).clamp(6, 14) as u8;
            def.muzzle_burst = 0;
        }

        WeaponFamily::Explosive => {
            set_explosive(
                def,
                6 + cost * 3,
                1,
                330.0,
                0.9,
                0.07,
                8.0,
                7.0,
                165.0,
                Color::srgb(1.0, 0.57, 0.2),
                Vec2::splat(11.0),
            );
        }

        WeaponFamily::Flame => {
            set_ranged(
                def,
                2 + cost,
                (2 + cost).clamp(2, 8) as usize,
                285.0,
                0.35,
                0.28,
                3.0,
                5.0,
                28.0,
                Color::srgb(1.0, 0.48, 0.14),
                Vec2::new(10.0, 6.0),
            );

            set_fire_hazard(
                def,
                32.0 + cost as f32 * 3.0,
                1 + cost / 3,
                0.75 + cost as f32 * 0.08,
                0.13,
            );
        }

        WeaponFamily::Laser => {
            set_ranged(
                def,
                3 + cost,
                1,
                880.0,
                0.42,
                0.025,
                2.5,
                3.5,
                35.0,
                Color::srgb(1.0, 0.24, 0.2),
                Vec2::new(18.0, 3.0),
            );

            def.pierce = cost.saturating_sub(1).min(5) as u8;
        }

        WeaponFamily::Plasma => {
            set_explosive(
                def,
                7 + cost * 2,
                1,
                300.0,
                1.0,
                0.055,
                6.0,
                8.0,
                135.0,
                Color::srgb(0.3, 1.0, 0.35),
                Vec2::splat(13.0),
            );
        }

        WeaponFamily::Lightning => {
            set_ranged(
                def,
                3 + cost,
                1,
                940.0,
                0.38,
                0.04,
                3.0,
                4.0,
                30.0,
                Color::srgb(0.72, 0.92, 1.0),
                Vec2::new(18.0, 4.0),
            );

            def.pierce = cost.clamp(1, 5) as u8;
        }

        WeaponFamily::Toxic => {
            set_ranged(
                def,
                5 + cost,
                1,
                410.0,
                0.82,
                0.045,
                5.0,
                6.0,
                95.0,
                Color::srgb(0.42, 0.88, 0.38),
                Vec2::new(13.0, 5.0),
            );

            set_toxic_hazard(
                def,
                45.0 + cost as f32 * 4.0,
                1 + cost / 3,
                1.8 + cost as f32 * 0.15,
                0.24,
            );
        }

        WeaponFamily::Deployable => {
            // Until a dedicated sentry entity is added, represent the 24-ammo
            // deployment as a rapid stationary-style volley.
            set_ranged(
                def,
                3,
                8,
                620.0,
                0.72,
                0.2,
                2.0,
                3.5,
                40.0,
                Color::srgb(0.65, 0.7, 0.76),
                Vec2::new(9.0, 3.0),
            );

            def.burst_shots = 3;
            def.burst_interval = 2.0 / 30.0;
        }

        WeaponFamily::Novelty => match meta.wep_type {
            AmmoType::None => {
                set_melee(
                    def,
                    10 + area / 3,
                    70.0,
                    2.1,
                    3.0,
                    Color::srgb(0.9, 0.65, 0.85),
                );
            }
            AmmoType::Bullets => {
                set_ranged(
                    def,
                    3,
                    1,
                    540.0,
                    0.75,
                    0.12,
                    3.0,
                    4.0,
                    55.0,
                    Color::srgb(0.95, 0.62, 0.82),
                    Vec2::new(10.0, 4.0),
                );
            }
            AmmoType::Shells => {
                set_ranged(
                    def,
                    2,
                    6,
                    390.0,
                    0.35,
                    0.32,
                    6.0,
                    4.0,
                    30.0,
                    Color::srgb(0.95, 0.62, 0.82),
                    Vec2::new(8.0, 4.0),
                );
            }
            AmmoType::Bolts => {
                set_ranged(
                    def,
                    9,
                    1,
                    590.0,
                    0.9,
                    0.08,
                    5.0,
                    4.0,
                    100.0,
                    Color::srgb(0.95, 0.62, 0.82),
                    Vec2::new(14.0, 4.0),
                );
            }
            AmmoType::Explosives => {
                set_explosive(
                    def,
                    8,
                    1,
                    320.0,
                    0.8,
                    0.1,
                    7.0,
                    6.0,
                    130.0,
                    Color::srgb(0.95, 0.62, 0.82),
                    Vec2::splat(10.0),
                );
            }
            AmmoType::Energy => {
                set_ranged(
                    def,
                    5,
                    1,
                    760.0,
                    0.48,
                    0.06,
                    3.0,
                    4.0,
                    45.0,
                    Color::srgb(0.95, 0.62, 0.82),
                    Vec2::new(16.0, 4.0),
                );
            }
        },
    }
}

fn apply_exact_profile(def: &mut WeaponDef, meta: &WeaponData) {
    // Golden/Ultra/Cursed weapons inherit the normal family's exact profile.
    let name = base_weapon_name(meta.wep_name);

    match name {
        "REVOLVER" => {
            set_ranged(
                def,
                3,
                1,
                560.0,
                0.82,
                0.07,
                3.0,
                4.0,
                75.0,
                Color::srgb(0.95, 0.9, 0.65),
                Vec2::new(10.0, 3.0),
            );
        }

        "TRIPLE MACHINEGUN" => {
            set_ranged(
                def,
                3,
                3,
                610.0,
                0.7,
                0.2,
                4.0,
                3.5,
                45.0,
                Color::srgb(1.0, 0.86, 0.45),
                Vec2::new(10.0, 3.0),
            );
        }

        "WRENCH" => {
            set_melee(def, 8, 68.0, 2.1, 3.5, Color::srgb(0.76, 0.78, 0.82));
        }

        "MACHINEGUN" => {
            set_ranged(
                def,
                3,
                1,
                610.0,
                0.72,
                0.105,
                2.6,
                3.5,
                48.0,
                Color::srgb(1.0, 0.86, 0.45),
                Vec2::new(10.0, 3.0),
            );
        }

        "SHOTGUN" => {
            set_ranged(
                def,
                2,
                7,
                410.0,
                0.34,
                0.35,
                7.0,
                3.5,
                28.0,
                Color::srgb(1.0, 0.8, 0.3),
                Vec2::new(8.0, 3.0),
            );
        }

        "CROSSBOW" => {
            set_ranged(
                def,
                20,
                1,
                720.0,
                1.0,
                0.025,
                6.0,
                4.0,
                130.0,
                Color::srgb(0.95, 0.84, 0.45),
                Vec2::new(17.0, 4.0),
            );
        }

        "GRENADE LAUNCHER" => {
            // GML Grenade/Create_0: damage 15, speed 10 (300 px/s), alarm0 60 (2.0s), friction 0.1→0.4@6f, bounce speed*=0.6
            set_explosive(
                def,
                15,
                1,
                300.0,
                2.0,
                0.052,
                10.0,
                4.0,
                200.0,
                Color::srgb(1.0, 0.58, 0.2),
                Vec2::splat(6.0),
            );
            def.bounces = 4; // GML move_bounce_solid(true) repeatedly until fuse
        }

        "DOUBLE SHOTGUN" => {
            set_ranged(
                def,
                2,
                14,
                410.0,
                0.34,
                0.52,
                12.0,
                3.5,
                32.0,
                Color::srgb(1.0, 0.78, 0.27),
                Vec2::new(8.0, 3.0),
            );
        }

        "MINIGUN" => {
            set_ranged(
                def,
                3,
                1,
                640.0,
                0.68,
                0.23,
                1.8,
                3.0,
                38.0,
                Color::srgb(1.0, 0.88, 0.48),
                Vec2::new(10.0, 3.0),
            );
        }

        "AUTO SHOTGUN" => {
            set_ranged(
                def,
                2,
                6,
                430.0,
                0.32,
                0.26,
                5.0,
                3.5,
                24.0,
                Color::srgb(1.0, 0.8, 0.3),
                Vec2::new(8.0, 3.0),
            );
        }

        "AUTO CROSSBOW" => {
            set_ranged(
                def,
                20,
                1,
                720.0,
                1.0,
                0.085,
                4.0,
                4.0,
                100.0,
                Color::srgb(0.95, 0.84, 0.45),
                Vec2::new(15.0, 4.0),
            );
        }

        "SUPER CROSSBOW" => {
            set_ranged(
                def,
                20,
                5,
                720.0,
                1.05,
                0.17,
                14.0,
                4.5,
                130.0,
                Color::srgb(1.0, 0.9, 0.55),
                Vec2::new(18.0, 5.0),
            );

            def.pierce = 1;
        }

        "SHOVEL" => {
            set_melee(def, 16, 84.0, 2.55, 7.0, Color::srgb(0.84, 0.77, 0.48));

            def.pellets = 3;
        }

        "BAZOOKA" => {
            // GML Rocket/Create_0: damage 20, speed max 12 (360 px/s)
            set_explosive(
                def,
                20,
                1,
                250.0,
                1.4,
                0.05,
                13.0,
                7.0,
                220.0,
                Color::srgb(1.0, 0.45, 0.13),
                Vec2::new(14.0, 8.0),
            );
        }

        "STICKY LAUNCHER" => {
            set_explosive(
                def,
                8,
                1,
                350.0,
                1.7,
                0.055,
                7.0,
                7.0,
                145.0,
                Color::srgb(1.0, 0.6, 0.22),
                Vec2::splat(10.0),
            );
        }

        "SMG" => {
            set_ranged(
                def,
                3,
                1,
                590.0,
                0.65,
                0.28,
                1.7,
                3.5,
                35.0,
                Color::srgb(1.0, 0.85, 0.47),
                Vec2::new(9.0, 3.0),
            );
        }

        "ASSAULT RIFLE" => {
            set_ranged(
                def,
                3,
                1,
                650.0,
                0.78,
                0.06,
                4.0,
                4.0,
                65.0,
                Color::srgb(1.0, 0.88, 0.47),
                Vec2::new(12.0, 3.0),
            );

            def.burst_shots = 3;
            def.burst_interval = 2.0 / 30.0;
        }

        "DISC GUN" => {
            // GML Disc/Create_0: damage 6, speed 5 (150 px/s)
            set_ranged(
                def,
                6,
                1,
                420.0,
                2.2,
                0.035,
                3.0,
                8.0,
                120.0,
                Color::srgb(0.7, 0.95, 1.0),
                Vec2::splat(14.0),
            );

            def.bounces = 6;
            def.muzzle_burst = 0;
        }

        "SUPER DISC GUN" => {
            // GML Disc damage 6 (5x volley)
            set_ranged(
                def,
                6,
                1,
                460.0,
                2.8,
                0.02,
                4.0,
                10.0,
                180.0,
                Color::srgb(0.75, 1.0, 1.0),
                Vec2::splat(18.0),
            );

            def.bounces = 12;
            def.muzzle_burst = 0;
        }

        "LASER PISTOL" => {
            // GML Laser/Create_0: damage 2 (instant hitscan in GML, fast pierce here)
            set_ranged(
                def,
                2,
                1,
                850.0,
                0.42,
                0.018,
                2.5,
                3.5,
                30.0,
                Color::srgb(1.0, 0.22, 0.18),
                Vec2::new(18.0, 3.0),
            );

            def.pierce = 1;
        }

        "LASER RIFLE" => {
            // GML Laser damage 2
            set_ranged(
                def,
                2,
                1,
                900.0,
                0.48,
                0.05,
                3.5,
                3.5,
                35.0,
                Color::srgb(1.0, 0.2, 0.16),
                Vec2::new(21.0, 3.0),
            );

            def.pierce = 2;
        }

        "LASER MINIGUN" => {
            // GML Laser damage 2
            set_ranged(
                def,
                2,
                1,
                820.0,
                0.38,
                0.21,
                1.6,
                3.0,
                24.0,
                Color::srgb(1.0, 0.22, 0.17),
                Vec2::new(15.0, 3.0),
            );

            def.pierce = 1;
        }

        "SLUGGER" => {
            // GML Slug/Create_0: damage 22, speed 16 (480 px/s)
            set_ranged(
                def,
                22,
                1,
                500.0,
                0.66,
                0.085,
                9.0,
                6.0,
                170.0,
                Color::srgb(0.95, 0.82, 0.42),
                Vec2::new(14.0, 5.0),
            );
        }

        "GATLING SLUGGER" => {
            // GML fires Slug (damage 22) at 18 px/frame
            set_ranged(
                def,
                22,
                1,
                560.0,
                0.62,
                0.105,
                5.0,
                6.0,
                135.0,
                Color::srgb(0.95, 0.8, 0.38),
                Vec2::new(14.0, 5.0),
            );
        }

        "ASSAULT SLUGGER" => {
            set_ranged(
                def,
                14,
                1,
                520.0,
                0.62,
                0.08,
                8.0,
                6.0,
                150.0,
                Color::srgb(0.96, 0.81, 0.4),
                Vec2::new(14.0, 5.0),
            );

            def.burst_shots = 3;
            def.burst_interval = 3.0 / 30.0;
        }

        "ENERGY SWORD" => {
            set_melee(def, 12, 82.0, 2.5, 4.0, Color::srgb(0.25, 0.86, 1.0));
        }

        "SUPER SLUGGER" => {
            // GML SuperSlugger fires 5x Slug (damage 22 each)
            set_ranged(
                def,
                22,
                5,
                560.0,
                0.68,
                0.18,
                15.0,
                6.0,
                160.0,
                Color::srgb(1.0, 0.86, 0.44),
                Vec2::new(14.0, 5.0),
            );
        }

        "HYPER RIFLE" => {
            set_ranged(
                def,
                3,
                5,
                880.0,
                0.55,
                0.045,
                2.0,
                3.0,
                40.0,
                Color::srgb(1.0, 0.95, 0.6),
                Vec2::new(16.0, 3.0),
            );
        }

        "SCREWDRIVER" => {
            set_melee(def, 6, 58.0, 1.35, 1.7, Color::srgb(0.82, 0.84, 0.87));
        }

        "ENERGY SCREWDRIVER" => {
            set_melee(def, 9, 64.0, 1.5, 2.0, Color::srgb(0.25, 0.86, 1.0));
        }

        "BLOOD LAUNCHER" => {
            // GML BloodGrenade damage 10
            set_explosive(
                def,
                10,
                1,
                330.0,
                0.85,
                0.105,
                7.0,
                7.0,
                150.0,
                Color::srgb(0.9, 0.16, 0.18),
                Vec2::splat(11.0),
            );
        }

        "BLOOD CANNON" => {
            // GML BloodBall damage 45
            set_explosive(
                def,
                45,
                1,
                320.0,
                0.62,
                0.08,
                10.0,
                8.0,
                220.0,
                Color::srgb(0.9, 0.18, 0.18),
                Vec2::splat(12.0),
            );

            set_split(
                def,
                6,
                0.7,
                380.0,
                2,
                0.34,
                3.0,
                30.0,
                Color::srgb(1.0, 0.3, 0.3),
                Vec2::new(7.0, 3.0),
            );
        }

        "SPLINTER GUN" => {
            // GML Splinter damage 4
            set_ranged(
                def,
                4,
                5,
                620.0,
                0.58,
                0.18,
                4.0,
                3.0,
                38.0,
                Color::srgb(0.68, 0.47, 0.3),
                Vec2::new(10.0, 3.0),
            );
        }

        "SPLINTER PISTOL" => {
            // GML Splinter damage 4
            set_ranged(
                def,
                4,
                4,
                580.0,
                0.52,
                0.09,
                3.0,
                3.0,
                34.0,
                Color::srgb(0.68, 0.47, 0.3),
                Vec2::new(9.0, 3.0),
            );
        }

        "SUPER SPLINTER GUN" => {
            set_ranged(
                def,
                4,
                10,
                650.0,
                0.62,
                0.26,
                7.0,
                3.5,
                45.0,
                Color::srgb(0.72, 0.5, 0.32),
                Vec2::new(11.0, 3.0),
            );
        }

        "TOXIC BOW" => {
            // GML ToxicBolt damage 16
            set_ranged(
                def,
                16,
                1,
                620.0,
                1.0,
                0.02,
                6.0,
                4.5,
                120.0,
                Color::srgb(0.42, 0.9, 0.37),
                Vec2::new(17.0, 4.0),
            );

            set_toxic_hazard(def, 46.0, 1, 2.0, 0.24);
        }

        "SENTRY GUN" => {
            set_ranged(
                def,
                3,
                8,
                620.0,
                0.72,
                0.2,
                2.0,
                3.5,
                40.0,
                Color::srgb(0.62, 0.68, 0.76),
                Vec2::new(9.0, 3.0),
            );

            def.burst_shots = 3;
            def.burst_interval = 2.0 / 30.0;
        }

        "WAVE GUN" => {
            set_ranged(
                def,
                3,
                9,
                430.0,
                0.5,
                0.5,
                7.0,
                4.0,
                42.0,
                Color::srgb(0.55, 0.86, 1.0),
                Vec2::new(10.0, 4.0),
            );
        }

        "PLASMA GUN" => {
            // GML PlasmaBall damage 4
            set_explosive(
                def,
                4,
                1,
                300.0,
                1.1,
                0.07,
                5.0,
                8.0,
                120.0,
                Color::srgb(0.3, 1.0, 0.36),
                Vec2::splat(13.0),
            );
        }

        "PLASMA RIFLE" => {
            // GML PlasmaBall damage 4
            set_explosive(
                def,
                4,
                1,
                330.0,
                1.0,
                0.05,
                4.0,
                7.0,
                105.0,
                Color::srgb(0.28, 1.0, 0.34),
                Vec2::splat(12.0),
            );
        }

        "PLASMA MINIGUN" => {
            // GML PlasmaBall damage 4
            set_explosive(
                def,
                4,
                1,
                350.0,
                0.85,
                0.13,
                2.5,
                6.0,
                75.0,
                Color::srgb(0.28, 1.0, 0.34),
                Vec2::splat(10.0),
            );
        }

        "PLASMA CANNON" | "DEVASTATOR" => {
            // GML PlasmaBig damage 15
            set_explosive(
                def,
                15,
                1,
                260.0,
                1.45,
                0.035,
                14.0,
                11.0,
                260.0,
                Color::srgb(0.35, 1.0, 0.4),
                Vec2::splat(18.0),
            );
        }

        "ENERGY HAMMER" => {
            set_melee(def, 24, 92.0, 2.7, 8.0, Color::srgb(0.25, 0.85, 1.0));
        }

        "JACKHAMMER" => {
            set_explosive(
                def,
                6,
                1,
                360.0,
                0.48,
                0.15,
                4.0,
                5.0,
                85.0,
                Color::srgb(1.0, 0.58, 0.2),
                Vec2::splat(9.0),
            );
        }

        "FLAK CANNON" => {
            set_explosive(
                def,
                8,
                1,
                340.0,
                0.65,
                0.07,
                10.0,
                7.0,
                200.0,
                Color::srgb(1.0, 0.72, 0.3),
                Vec2::splat(11.0),
            );

            set_split(
                def,
                6,
                0.55,
                420.0,
                3,
                0.32,
                3.0,
                50.0,
                Color::srgb(1.0, 0.88, 0.55),
                Vec2::new(8.0, 3.0),
            );
        }

        "SUPER FLAK CANNON" => {
            set_explosive(
                def,
                10,
                1,
                360.0,
                0.7,
                0.05,
                12.0,
                8.0,
                240.0,
                Color::srgb(1.0, 0.66, 0.22),
                Vec2::splat(12.0),
            );

            set_split(
                def,
                10,
                0.8,
                460.0,
                3,
                0.36,
                3.0,
                55.0,
                Color::srgb(1.0, 0.9, 0.6),
                Vec2::new(8.0, 3.0),
            );
        }

        "CHICKEN SWORD" => {
            set_melee(def, 6, 72.0, 2.1, 2.5, Color::srgb(0.92, 0.92, 0.96));
        }

        "NUKE LAUNCHER" => {
            // GML Nuke damage 50
            set_explosive(
                def,
                50,
                1,
                220.0,
                1.7,
                0.035,
                18.0,
                12.0,
                320.0,
                Color::srgb(1.0, 0.38, 0.1),
                Vec2::splat(20.0),
            );
        }

        "ION CANNON" => {
            set_ranged(
                def,
                18,
                1,
                820.0,
                0.7,
                0.02,
                9.0,
                7.0,
                150.0,
                Color::srgb(0.52, 0.85, 1.0),
                Vec2::new(25.0, 6.0),
            );

            def.pierce = 5;
        }

        "QUADRUPLE MACHINEGUN" => {
            set_ranged(
                def,
                3,
                4,
                650.0,
                0.72,
                0.16,
                6.0,
                3.5,
                45.0,
                Color::srgb(1.0, 0.88, 0.48),
                Vec2::new(10.0, 3.0),
            );
        }

        "FLAMETHROWER" => {
            set_ranged(
                def,
                2,
                5,
                250.0,
                0.22,
                0.35,
                1.2,
                5.0,
                18.0,
                Color::srgb(1.0, 0.55, 0.15),
                Vec2::new(10.0, 6.0),
            );

            set_fire_hazard(def, 34.0, 1, 0.8, 0.12);
        }

        "DRAGON" => {
            set_ranged(
                def,
                3,
                7,
                280.0,
                0.28,
                0.42,
                2.0,
                6.0,
                25.0,
                Color::srgb(1.0, 0.4, 0.08),
                Vec2::new(12.0, 7.0),
            );

            set_fire_hazard(def, 40.0, 1, 1.0, 0.12);
        }

        "FLARE GUN" => {
            set_ranged(
                def,
                8,
                1,
                300.0,
                1.1,
                0.12,
                6.0,
                6.0,
                90.0,
                Color::srgb(1.0, 0.32, 0.1),
                Vec2::splat(10.0),
            );

            set_fire_hazard(def, 42.0, 2, 1.5, 0.18);
        }

        "HYPER LAUNCHER" => {
            set_explosive(
                def,
                7,
                2,
                480.0,
                0.65,
                0.05,
                6.0,
                6.0,
                100.0,
                Color::srgb(1.0, 0.5, 0.16),
                Vec2::splat(9.0),
            );
        }

        "LASER CANNON" => {
            set_ranged(
                def,
                18,
                1,
                980.0,
                0.7,
                0.012,
                10.0,
                6.0,
                130.0,
                Color::srgb(1.0, 0.16, 0.12),
                Vec2::new(28.0, 5.0),
            );

            def.pierce = 6;
        }

        "RUSTY REVOLVER" => {
            set_ranged(
                def,
                2,
                1,
                510.0,
                0.8,
                0.12,
                3.0,
                4.0,
                55.0,
                Color::srgb(0.67, 0.48, 0.31),
                Vec2::new(9.0, 3.0),
            );
        }

        "LIGHTNING PISTOL" => {
            set_ranged(
                def,
                4,
                1,
                920.0,
                0.35,
                0.06,
                2.5,
                4.0,
                25.0,
                Color::srgb(0.75, 0.9, 1.0),
                Vec2::new(18.0, 4.0),
            );

            def.pierce = 1;
        }

        "LIGHTNING RIFLE" => {
            set_ranged(
                def,
                6,
                1,
                980.0,
                0.42,
                0.05,
                4.0,
                4.0,
                35.0,
                Color::srgb(0.7, 0.95, 1.0),
                Vec2::new(22.0, 4.0),
            );

            def.pierce = 2;
        }

        "LIGHTNING SHOTGUN" => {
            set_ranged(
                def,
                3,
                8,
                860.0,
                0.26,
                0.3,
                7.0,
                3.0,
                20.0,
                Color::srgb(0.75, 0.95, 1.0),
                Vec2::new(14.0, 3.0),
            );

            def.pierce = 1;
        }

        "LIGHTNING SMG" => {
            set_ranged(
                def,
                3,
                1,
                900.0,
                0.32,
                0.2,
                2.0,
                3.0,
                18.0,
                Color::srgb(0.72, 0.93, 1.0),
                Vec2::new(14.0, 3.0),
            );

            def.pierce = 1;
        }

        "LIGHTNING CANNON" => {
            set_ranged(
                def,
                15,
                1,
                520.0,
                1.1,
                0.08,
                10.0,
                9.0,
                140.0,
                Color::srgb(0.65, 0.9, 1.0),
                Vec2::splat(17.0),
            );

            def.pierce = 4;
        }

        "LIGHTNING HAMMER" => {
            // This exact profile prevents the old numeric-ID error where this
            // weapon inherited a flamethrower profile.
            set_melee(def, 22, 90.0, 2.7, 7.0, Color::srgb(0.65, 0.9, 1.0));

            def.pierce = 0;
            def.hazard = None;
        }

        "SAWED-OFF SHOTGUN" => {
            set_ranged(
                def,
                2,
                20,
                390.0,
                0.28,
                0.78,
                14.0,
                3.0,
                35.0,
                Color::srgb(1.0, 0.78, 0.28),
                Vec2::new(8.0, 3.0),
            );
        }

        "SMART GUN" => {
            set_ranged(
                def,
                4,
                1,
                650.0,
                0.78,
                0.025,
                2.0,
                4.0,
                60.0,
                Color::srgb(0.45, 0.85, 1.0),
                Vec2::new(12.0, 3.0),
            );
        }

        "HEAVY CROSSBOW" => {
            set_ranged(
                def,
                50,
                1,
                480.0,
                1.3,
                0.015,
                10.0,
                6.0,
                250.0,
                Color::srgb(0.84, 0.72, 0.3),
                Vec2::new(24.0, 6.0),
            );

            def.pierce = 5;
        }

        "HEAVY AUTO CROSSBOW" => {
            set_ranged(
                def,
                50,
                1,
                480.0,
                1.0,
                0.12,
                6.5,
                5.0,
                140.0,
                Color::srgb(1.0, 0.88, 0.5),
                Vec2::new(16.0, 5.0),
            );

            def.pierce = 2;
        }

        "BLOOD HAMMER" => {
            set_melee(def, 14, 80.0, 2.35, 5.0, Color::srgb(0.9, 0.12, 0.15));
        }

        "POP GUN" => {
            set_ranged(
                def,
                2,
                1,
                560.0,
                0.52,
                0.07,
                2.0,
                3.0,
                20.0,
                Color::srgb(1.0, 0.72, 0.85),
                Vec2::new(8.0, 3.0),
            );
        }

        "POP RIFLE" => {
            set_ranged(
                def,
                2,
                1,
                610.0,
                0.6,
                0.08,
                3.0,
                3.0,
                30.0,
                Color::srgb(1.0, 0.72, 0.85),
                Vec2::new(10.0, 3.0),
            );

            def.burst_shots = 3;
            def.burst_interval = 2.0 / 30.0;
        }

        "TOXIC LAUNCHER" => {
            set_explosive(
                def,
                7,
                1,
                340.0,
                0.7,
                0.05,
                9.0,
                7.0,
                170.0,
                Color::srgb(0.45, 0.9, 0.4),
                Vec2::splat(11.0),
            );

            set_toxic_hazard(def, 56.0, 1, 2.4, 0.25);
        }

        "FLAME CANNON" => {
            set_ranged(
                def,
                9,
                1,
                300.0,
                0.45,
                0.14,
                8.0,
                7.0,
                110.0,
                Color::srgb(1.0, 0.5, 0.18),
                Vec2::splat(12.0),
            );

            set_fire_hazard(def, 48.0, 1, 1.1, 0.15);
        }

        "FLAME SHOTGUN" => {
            set_ranged(
                def,
                2,
                6,
                300.0,
                0.24,
                0.42,
                4.0,
                4.0,
                22.0,
                Color::srgb(1.0, 0.55, 0.18),
                Vec2::new(9.0, 5.0),
            );

            set_fire_hazard(def, 32.0, 1, 0.8, 0.12);
        }

        "DOUBLE FLAME SHOTGUN" => {
            set_ranged(
                def,
                2,
                14,
                300.0,
                0.24,
                0.5,
                6.5,
                4.0,
                25.0,
                Color::srgb(1.0, 0.55, 0.2),
                Vec2::new(9.0, 5.0),
            );

            set_fire_hazard(def, 34.0, 1, 0.9, 0.12);
        }

        "AUTO FLAME SHOTGUN" => {
            set_ranged(
                def,
                2,
                6,
                300.0,
                0.22,
                0.35,
                3.5,
                4.0,
                20.0,
                Color::srgb(1.0, 0.58, 0.2),
                Vec2::new(9.0, 5.0),
            );

            set_fire_hazard(def, 30.0, 1, 0.75, 0.12);
        }

        "CLUSTER LAUNCHER" => {
            set_explosive(
                def,
                8,
                1,
                310.0,
                0.72,
                0.14,
                9.0,
                7.0,
                170.0,
                Color::srgb(1.0, 0.62, 0.22),
                Vec2::splat(11.0),
            );

            set_split(
                def,
                6,
                0.75,
                340.0,
                3,
                0.4,
                3.0,
                45.0,
                Color::srgb(1.0, 0.78, 0.38),
                Vec2::splat(7.0),
            );
        }

        "GRENADE SHOTGUN" => {
            set_explosive(
                def,
                4,
                4,
                360.0,
                0.5,
                0.3,
                9.0,
                5.0,
                70.0,
                Color::srgb(1.0, 0.62, 0.22),
                Vec2::splat(8.0),
            );
        }

        "AUTO GRENADE SHOTGUN" => {
            set_explosive(
                def,
                4,
                3,
                360.0,
                0.46,
                0.26,
                6.0,
                5.0,
                65.0,
                Color::srgb(1.0, 0.62, 0.22),
                Vec2::splat(8.0),
            );
        }

        "GRENADE RIFLE" => {
            set_explosive(
                def,
                5,
                1,
                410.0,
                0.65,
                0.08,
                6.0,
                5.0,
                85.0,
                Color::srgb(1.0, 0.61, 0.2),
                Vec2::splat(9.0),
            );

            def.burst_shots = 3;
            def.burst_interval = 2.0 / 30.0;
        }

        "ROGUE RIFLE" => {
            set_ranged(
                def,
                4,
                1,
                700.0,
                0.75,
                0.04,
                4.0,
                4.0,
                70.0,
                Color::srgb(0.45, 0.8, 1.0),
                Vec2::new(14.0, 3.0),
            );

            def.burst_shots = 3;
            def.burst_interval = 2.0 / 30.0;
        }

        "PARTY GUN" => {
            set_ranged(
                def,
                3,
                5,
                480.0,
                0.7,
                0.4,
                3.0,
                4.0,
                35.0,
                Color::srgb(1.0, 0.38, 0.86),
                Vec2::splat(8.0),
            );
        }

        "DOUBLE MINIGUN" => {
            set_ranged(
                def,
                3,
                2,
                650.0,
                0.68,
                0.25,
                3.0,
                3.0,
                38.0,
                Color::srgb(1.0, 0.88, 0.48),
                Vec2::new(10.0, 3.0),
            );
        }

        "GATLING BAZOOKA" => {
            set_explosive(
                def,
                10,
                1,
                280.0,
                1.2,
                0.2,
                8.0,
                7.0,
                180.0,
                Color::srgb(1.0, 0.46, 0.13),
                Vec2::new(13.0, 7.0),
            );
        }

        "SEEKER SHOTGUN" => {
            // No homing component yet: long-lived mildly-spread bolt volley
            // stands in until the projectile-archetype patch.
            set_ranged(
                def,
                9,
                3,
                520.0,
                1.9,
                0.09,
                7.0,
                4.0,
                90.0,
                Color::srgb(1.0, 0.55, 0.85),
                Vec2::new(11.0, 4.0),
            );

            def.pierce = 1;
        }

        "ERASER" => {
            set_ranged(
                def,
                3,
                10,
                700.0,
                0.35,
                0.08,
                7.0,
                3.0,
                40.0,
                Color::srgb(0.9, 0.95, 1.0),
                Vec2::new(14.0, 3.0),
            );
        }

        "HEAVY REVOLVER" => {
            // GML HeavyBullet damage 7
            set_ranged(
                def,
                7,
                1,
                720.0,
                0.95,
                0.02,
                9.0,
                4.5,
                140.0,
                Color::srgb(1.0, 0.9, 0.4),
                Vec2::new(12.0, 4.0),
            );
        }

        "HEAVY MACHINEGUN" => {
            // GML HeavyBullet damage 7
            set_ranged(
                def,
                7,
                1,
                700.0,
                0.85,
                0.07,
                4.5,
                4.0,
                70.0,
                Color::srgb(1.0, 0.88, 0.35),
                Vec2::new(12.0, 3.5),
            );
        }

        "SLEDGEHAMMER" => {
            // GML Sledge: Slash damage 24, HeavySlash sprite
            set_melee(def, 24, 80.0, 2.45, 6.0, Color::srgb(0.88, 0.78, 0.48));
        }

        "GUITAR" | "ELECTRIC GUITAR" => {
            // GML Guitar: Slash damage 26 (+electric flag for electric)
            set_melee(def, 26, 80.0, 2.45, 6.0, Color::srgb(0.9, 0.7, 0.3));
        }

        "BLACK SWORD" => {
            // GML BlackSword: 12 normal, 80 MegaSlash when dying
            set_melee(def, 12, 66.0, 2.1, 4.0, Color::srgb(0.2, 0.2, 0.25));
        }

        "HEAVY SLUGGER" => {
            // GML HeavySlug damage 60, speed 13 (390 px/s)
            set_ranged(
                def,
                60,
                1,
                390.0,
                0.7,
                0.07,
                34.0,
                14.0,
                320.0,
                Color::srgb(1.0, 0.85, 0.4),
                Vec2::new(16.0, 6.0),
            );
        }

        "HEAVY CROSSBOW" | "HEAVY AUTO CROSSBOW" => {
            // GML HeavyBolt damage 50, speed 16 (480 px/s)
            set_ranged(
                def,
                50,
                1,
                480.0,
                1.1,
                0.025,
                50.0,
                6.0,
                220.0,
                Color::srgb(1.0, 0.9, 0.5),
                Vec2::new(18.0, 5.0),
            );
            def.pierce = 5;
        }

        "ULTRA REVOLVER" => {
            // GML UltraBullet damage 18, speed 24 (720 px/s)
            set_ranged(
                def,
                18,
                1,
                720.0,
                0.8,
                0.05,
                12.0,
                6.0,
                160.0,
                Color::srgb(0.95, 0.4, 1.0),
                Vec2::new(12.0, 4.0),
            );
        }

        "ULTRA SHOTGUN" => {
            // GML 9x UltraShell damage 6
            set_ranged(
                def,
                6,
                9,
                450.0,
                0.34,
                0.38,
                44.0,
                5.0,
                90.0,
                Color::srgb(0.95, 0.5, 1.0),
                Vec2::new(8.0, 3.0),
            );
        }

        "SUPER PLASMA CANNON" => {
            // GML PlasmaHuge damage 25, speed 1.5 (45 px/s)
            set_explosive(
                def,
                25,
                1,
                120.0,
                2.5,
                0.02,
                40.0,
                15.0,
                400.0,
                Color::srgb(0.4, 1.0, 0.5),
                Vec2::splat(24.0),
            );
        }

        _ => {}
    }
}

fn apply_variant_tuning(def: &mut WeaponDef, meta: &WeaponData) {
    let name = meta.wep_name;

    if meta.wep_gold || name.starts_with("GOLDEN ") {
        def.color = Color::srgb(1.0, 0.82, 0.2);
        def.muzzle_burst = def.muzzle_burst.saturating_add(1);

        // The generated registry already owns the faster/slower reload values.
        // Only family-specific golden projectile differences belong here.
        match base_weapon_name(name) {
            "SHOTGUN" => {
                def.pellets = def.pellets.saturating_add(1);
            }
            "DISC GUN" => {
                def.bounces = def.bounces.saturating_add(2);
                def.speed *= 1.08;
            }
            "SPLINTER GUN" => {
                def.pellets = def.pellets.saturating_add(1);
            }
            _ => {}
        }
    }

    if name.starts_with("ULTRA ") {
        def.color = Color::srgb(0.95, 0.4, 1.0);
        // Explicit ULTRA profiles already carry GML Ultra projectile damages
        // (UltraBullet 18, UltraShell 6, etc.) - do not re-multiply those.
        let explicit_ultra = matches!(
            name,
            "ULTRA REVOLVER" | "ULTRA SHOTGUN" | "ULTRA CROSSBOW" | "ULTRA GRENADE LAUNCHER"
        );
        if !explicit_ultra {
            def.damage = ((def.damage as f32) * 1.35).round() as i32;
        }
        def.recoil *= 1.2;
        def.shake *= 1.25;

        if def.melee.is_none() {
            def.projectile_radius *= 1.15;
            def.knockback *= 1.2;
        }
    }

    if name.starts_with("CURSED ") {
        def.color = Color::srgb(0.7, 0.28, 0.9);
        def.damage = ((def.damage as f32) * 1.2).round() as i32;
        def.shake *= 1.15;
    }

    if name.contains("BLOOD") {
        def.color = Color::srgb(0.9, 0.14, 0.18);
    }
}

fn normalize_def(def: &mut WeaponDef, meta: &WeaponData) {
    def.name = meta.wep_name;
    def.ammo = ammo_kind(meta);
    def.ammo_cost = i32::from(meta.wep_cost);
    def.rad_cost = u32::from(meta.wep_rads);
    def.cooldown = f32::from(meta.wep_load.max(1)) / 30.0;
    def.automatic = meta.wep_auto;

    def.damage = def.damage.max(0);
    def.lifetime = def.lifetime.max(1.0 / 30.0);
    def.spread = def.spread.max(0.0);
    def.recoil = def.recoil.max(0.0);
    def.shake = def.shake.max(0.0);
    def.projectile_radius = def.projectile_radius.max(0.0);
    def.knockback = def.knockback.max(0.0);
    def.burst_shots = def.burst_shots.max(1);
    def.burst_interval = def.burst_interval.max(0.0);

    if def.melee.is_some() {
        def.speed = 0.0;
        def.pellets = 0;
        def.projectile_radius = 0.0;
        def.bounces = 0;
        def.pierce = 0;
        def.hazard = None;
        def.split = None;
    } else {
        def.speed = def.speed.max(1.0);
        def.pellets = def.pellets.max(1);
        def.projectile_radius = def.projectile_radius.max(1.0);
    }
}

#[allow(clippy::too_many_arguments)]
fn set_ranged(
    def: &mut WeaponDef,
    damage: i32,
    pellets: usize,
    speed: f32,
    lifetime: f32,
    spread: f32,
    recoil: f32,
    radius: f32,
    knockback: f32,
    color: Color,
    size: Vec2,
) {
    def.damage = damage;
    def.pellets = pellets;
    def.speed = speed;
    def.lifetime = lifetime;
    def.spread = spread;
    def.recoil = recoil;
    def.shake = (recoil / 50.0).clamp(0.03, 0.5);
    def.projectile_radius = radius;
    def.knockback = knockback;
    def.explosive = false;
    def.melee = None;
    def.color = color;
    def.size = size;
    def.muzzle_burst = ((recoil / 2.0).round() as usize).clamp(1, 8);
    def.bounces = 0;
    def.pierce = 0;
    def.hazard = None;
    def.split = None;
}

#[allow(clippy::too_many_arguments)]
fn set_explosive(
    def: &mut WeaponDef,
    damage: i32,
    pellets: usize,
    speed: f32,
    lifetime: f32,
    spread: f32,
    recoil: f32,
    radius: f32,
    knockback: f32,
    color: Color,
    size: Vec2,
) {
    set_ranged(
        def, damage, pellets, speed, lifetime, spread, recoil, radius, knockback, color, size,
    );

    def.explosive = true;
    def.shake = (recoil / 35.0).clamp(0.1, 0.65);
}

fn set_melee(def: &mut WeaponDef, damage: i32, range: f32, arc: f32, recoil: f32, color: Color) {
    def.damage = damage;
    def.pellets = 0;
    def.speed = 0.0;
    def.lifetime = 0.12;
    def.spread = 0.0;
    def.recoil = recoil;
    def.shake = (recoil / 45.0).clamp(0.04, 0.45);
    def.projectile_radius = 0.0;
    def.knockback = recoil * 18.0;
    def.explosive = false;
    def.burst_shots = 1;
    def.burst_interval = 0.0;
    def.melee = Some(MeleeDef { range, arc });
    def.color = color;
    def.size = Vec2::new(range, 5.0);
    def.muzzle_burst = 0;
    def.bounces = 0;
    def.pierce = 0;
    def.hazard = None;
    def.split = None;
}

fn set_fire_hazard(def: &mut WeaponDef, radius: f32, damage: i32, duration: f32, tick: f32) {
    def.hazard = Some(HazardDef {
        kind: HazardKind::Fire,
        radius,
        damage,
        duration,
        tick,
        color: Color::srgba(1.0, 0.48, 0.12, 0.3),
    });
}

fn set_toxic_hazard(def: &mut WeaponDef, radius: f32, damage: i32, duration: f32, tick: f32) {
    def.hazard = Some(HazardDef {
        kind: HazardKind::Toxic,
        radius,
        damage,
        duration,
        tick,
        color: Color::srgba(0.34, 0.9, 0.34, 0.34),
    });
}

#[allow(clippy::too_many_arguments)]
fn set_split(
    def: &mut WeaponDef,
    pellets: u8,
    spread: f32,
    speed: f32,
    damage: i32,
    lifetime: f32,
    radius: f32,
    knockback: f32,
    color: Color,
    size: Vec2,
) {
    def.split = Some(SplitDef {
        pellets,
        spread,
        speed,
        damage,
        lifetime,
        radius,
        knockback,
        color,
        size,
    });
}

#[cfg(test)]
mod tests {
    #[test]
    fn dbg_nuke_values() {
        let meta_n = crate::game::weapons_data::WEAPONS
            .iter()
            .find(|m| m.wep_name == "NUKE LAUNCHER")
            .unwrap();
        let meta_g = crate::game::weapons_data::WEAPONS
            .iter()
            .find(|m| m.wep_name == "GRENADE LAUNCHER")
            .unwrap();
        let nuke = super::weapon_runtime_def(crate::game::content::WeaponId(meta_n.id));
        let gren = super::weapon_runtime_def(crate::game::content::WeaponId(meta_g.id));
        println!(
            "nuke id={} dmg={} radius={}",
            meta_n.id, nuke.damage, nuke.projectile_radius
        );
        println!(
            "gren id={} dmg={} radius={}",
            meta_g.id, gren.damage, gren.projectile_radius
        );
    }
    use super::*;
    use crate::game::weapons_data::{MAXWEP, WEAPONS};

    fn id_by_name(name: &str) -> WeaponId {
        let meta = WEAPONS
            .iter()
            .find(|meta| meta.wep_name == name)
            .unwrap_or_else(|| panic!("missing generated weapon {name:?}"));

        WeaponId(meta.id)
    }

    #[test]
    fn every_generated_weapon_has_a_runtime() {
        for meta in WEAPONS.iter().skip(1) {
            let id = WeaponId(meta.id);
            let def = weapon_runtime_def(id);

            assert_eq!(def.name, meta.wep_name, "weapon id {}", meta.id);
            assert_eq!(
                def.ammo_cost,
                i32::from(meta.wep_cost),
                "weapon {} ({})",
                meta.id,
                meta.wep_name,
            );
            assert_eq!(
                def.automatic, meta.wep_auto,
                "weapon {} ({})",
                meta.id, meta.wep_name,
            );
            assert!(
                def.cooldown > 0.0,
                "weapon {} ({}) has zero cooldown",
                meta.id,
                meta.wep_name,
            );
            assert!(
                def.damage >= 0,
                "weapon {} ({}) has negative damage",
                meta.id,
                meta.wep_name,
            );

            if meta.wep_mele {
                assert!(
                    def.melee.is_some(),
                    "melee weapon {} ({}) has no melee runtime",
                    meta.id,
                    meta.wep_name,
                );
            } else {
                assert!(
                    def.pellets > 0,
                    "ranged weapon {} ({}) has no projectiles",
                    meta.id,
                    meta.wep_name,
                );
                assert!(
                    def.speed > 0.0,
                    "ranged weapon {} ({}) has no projectile speed",
                    meta.id,
                    meta.wep_name,
                );
            }
        }
    }

    #[test]
    fn registry_covers_ids_zero_through_maxwep() {
        assert_eq!(WEAPONS.len(), MAXWEP + 1);

        for (index, meta) in WEAPONS.iter().enumerate() {
            assert_eq!(usize::from(meta.id), index);
        }
    }

    #[test]
    fn every_nonempty_weapon_is_classified() {
        for meta in WEAPONS.iter().skip(1) {
            let family = weapon_family(WeaponId(meta.id));
            assert_ne!(
                family,
                WeaponFamily::Empty,
                "weapon {} ({}) was not classified",
                meta.id,
                meta.wep_name,
            );
        }
    }

    #[test]
    fn generated_metadata_remains_authoritative() {
        for meta in WEAPONS.iter().skip(1) {
            let def = weapon_runtime_def(WeaponId(meta.id));
            let expected_cooldown = f32::from(meta.wep_load.max(1)) / 30.0;

            assert!(
                (def.cooldown - expected_cooldown).abs() < f32::EPSILON,
                "cooldown mismatch for {} ({})",
                meta.id,
                meta.wep_name,
            );
            assert_eq!(def.ammo, ammo_kind(meta));
            assert_eq!(def.ammo_cost, i32::from(meta.wep_cost));
            assert_eq!(def.automatic, meta.wep_auto);
        }
    }

    #[test]
    fn triple_machinegun_has_three_projectiles() {
        let def = weapon_runtime_def(id_by_name("TRIPLE MACHINEGUN"));

        assert_eq!(def.pellets, 3);
        assert_eq!(def.ammo, AmmoKind::Bullets);
        assert!(def.automatic);
    }

    #[test]
    fn double_shotgun_has_fourteen_pellets() {
        let def = weapon_runtime_def(id_by_name("DOUBLE SHOTGUN"));

        assert_eq!(def.pellets, 14);
        assert_eq!(def.ammo, AmmoKind::Shells);
    }

    #[test]
    fn super_crossbow_has_five_bolts() {
        let def = weapon_runtime_def(id_by_name("SUPER CROSSBOW"));

        assert_eq!(def.pellets, 5);
        assert_eq!(def.ammo, AmmoKind::Bolts);
    }

    #[test]
    fn assault_rifle_is_a_three_shot_burst() {
        let def = weapon_runtime_def(id_by_name("ASSAULT RIFLE"));

        assert_eq!(def.burst_shots, 3);
        assert!(def.burst_interval > 0.0);
    }

    #[test]
    fn disc_guns_bounce() {
        let disc = weapon_runtime_def(id_by_name("DISC GUN"));
        let super_disc = weapon_runtime_def(id_by_name("SUPER DISC GUN"));

        assert!(disc.bounces >= 6);
        assert!(super_disc.bounces > disc.bounces);
    }

    #[test]
    fn slugger_is_not_a_generic_shotgun() {
        let def = weapon_runtime_def(id_by_name("SLUGGER"));

        assert_eq!(def.pellets, 1);
        assert!(def.damage >= 16);
        assert!(def.knockback >= 150.0);
    }

    #[test]
    fn super_slugger_fires_five_slugs() {
        let def = weapon_runtime_def(id_by_name("SUPER SLUGGER"));

        assert_eq!(def.pellets, 5);
        assert!(def.damage >= 16);
    }

    #[test]
    fn splinter_variants_have_distinct_counts() {
        let pistol = weapon_runtime_def(id_by_name("SPLINTER PISTOL"));
        let gun = weapon_runtime_def(id_by_name("SPLINTER GUN"));
        let super_gun = weapon_runtime_def(id_by_name("SUPER SPLINTER GUN"));

        assert_eq!(pistol.pellets, 4);
        assert_eq!(gun.pellets, 5);
        assert!(super_gun.pellets > gun.pellets);
    }

    #[test]
    fn lightning_weapons_pierce() {
        for name in [
            "LIGHTNING PISTOL",
            "LIGHTNING RIFLE",
            "LIGHTNING SHOTGUN",
            "LIGHTNING SMG",
            "LIGHTNING CANNON",
        ] {
            let def = weapon_runtime_def(id_by_name(name));

            assert!(def.pierce > 0, "{name}");
            assert_eq!(def.ammo, AmmoKind::Energy, "{name}");
        }
    }

    #[test]
    fn lightning_hammer_is_melee_not_fire() {
        let def = weapon_runtime_def(id_by_name("LIGHTNING HAMMER"));

        assert!(def.melee.is_some());
        assert!(def.hazard.is_none());
        assert_eq!(def.pellets, 0);
        assert_eq!(
            weapon_family(id_by_name("LIGHTNING HAMMER")),
            WeaponFamily::MeleeHeavy
        );
    }

    #[test]
    fn flame_family_leaves_fire() {
        for name in [
            "FLAMETHROWER",
            "DRAGON",
            "FLARE GUN",
            "FLAME CANNON",
            "FLAME SHOTGUN",
            "DOUBLE FLAME SHOTGUN",
            "AUTO FLAME SHOTGUN",
        ] {
            let def = weapon_runtime_def(id_by_name(name));
            let hazard = def
                .hazard
                .unwrap_or_else(|| panic!("{name} has no fire hazard"));

            assert!(
                matches!(hazard.kind, HazardKind::Fire),
                "{name} has the wrong hazard",
            );
        }
    }

    #[test]
    fn toxic_family_leaves_toxic_hazards() {
        for name in ["TOXIC BOW", "TOXIC LAUNCHER"] {
            let def = weapon_runtime_def(id_by_name(name));
            let hazard = def
                .hazard
                .unwrap_or_else(|| panic!("{name} has no toxic hazard"));

            assert!(matches!(hazard.kind, HazardKind::Toxic));
        }
    }

    #[test]
    fn flak_variants_split() {
        let flak = weapon_runtime_def(id_by_name("FLAK CANNON"));
        let super_flak = weapon_runtime_def(id_by_name("SUPER FLAK CANNON"));

        assert!(flak.explosive);
        assert!(super_flak.explosive);

        let normal_split = flak.split.expect("flak split");
        let super_split = super_flak.split.expect("super flak split");

        assert!(super_split.pellets > normal_split.pellets);
    }

    #[test]
    fn blood_cannon_splits() {
        let def = weapon_runtime_def(id_by_name("BLOOD CANNON"));

        assert!(def.explosive);
        assert!(def.split.is_some());
    }

    #[test]
    fn cluster_launcher_splits() {
        let def = weapon_runtime_def(id_by_name("CLUSTER LAUNCHER"));

        assert!(def.explosive);
        assert!(def.split.is_some());
    }

    #[test]
    fn plasma_family_is_explosive() {
        for name in [
            "PLASMA GUN",
            "PLASMA RIFLE",
            "PLASMA CANNON",
            "PLASMA MINIGUN",
            "DEVASTATOR",
        ] {
            let def = weapon_runtime_def(id_by_name(name));
            assert!(def.explosive, "{name}");
        }
    }

    #[test]
    fn nuke_launcher_is_not_generic_grenade_damage() {
        let grenade = weapon_runtime_def(id_by_name("GRENADE LAUNCHER"));
        let nuke = weapon_runtime_def(id_by_name("NUKE LAUNCHER"));

        assert!(nuke.explosive);
        // Legacy GRENADE LAUNCHER keeps its hand-authored damage (15), so
        // compare with headroom instead of the profile-table value.
        assert!(nuke.damage >= 40);
        assert!(nuke.damage > grenade.damage * 2);
        assert!(nuke.projectile_radius > grenade.projectile_radius);
    }

    #[test]
    fn heavy_crossbows_pierce() {
        for name in ["HEAVY CROSSBOW", "HEAVY AUTO CROSSBOW"] {
            let def = weapon_runtime_def(id_by_name(name));

            assert!(def.pierce > 0, "{name}");
            assert_eq!(def.ammo, AmmoKind::Bolts, "{name}");
        }
    }

    #[test]
    fn golden_weapons_inherit_base_family() {
        let revolver = weapon_runtime_def(id_by_name("REVOLVER"));
        let golden = weapon_runtime_def(id_by_name("GOLDEN REVOLVER"));

        assert_eq!(
            weapon_family(id_by_name("REVOLVER")),
            weapon_family(id_by_name("GOLDEN REVOLVER")),
        );
        assert_eq!(revolver.pellets, golden.pellets);
        assert_eq!(revolver.damage, golden.damage);
    }

    #[test]
    fn golden_shotgun_gets_bonus_pellet() {
        let normal = weapon_runtime_def(id_by_name("SHOTGUN"));
        let golden = weapon_runtime_def(id_by_name("GOLDEN SHOTGUN"));

        assert_eq!(golden.pellets, normal.pellets + 1);
    }

    #[test]
    fn empty_weapon_remains_inert() {
        let def = weapon_runtime_def(WeaponId::NONE);
        let runtime = weapon_runtime(WeaponId::NONE);

        assert_eq!(def.damage, 0);
        assert_eq!(def.pellets, 0);
        assert_eq!(runtime.damage, 0);
        assert_eq!(runtime.pellets, 0);
    }

    #[test]
    fn invalid_weapon_id_sanitizes_to_empty() {
        let def = weapon_runtime_def(WeaponId(u8::MAX));

        assert_eq!(def.damage, 0);
        assert_eq!(def.pellets, 0);
    }

    #[test]
    fn shell_weapons_multi_pellet_or_heavy_slug() {
        // Every Shells-typed weapon either fires a wide volley or a single
        // heavy slug (Slugger family) - never a lone weak pea.
        for meta in WEAPONS.iter().skip(1) {
            if meta.wep_type != AmmoType::Shells || meta.wep_mele {
                continue;
            }
            let def = weapon_runtime_def(WeaponId(meta.id));
            let slug = def.pellets == 1;
            let ok = if slug {
                // Heavy slug OR a splitting payload (Flak fires one shell that
                // bursts into shrapnel on death).
                def.damage >= 10 || def.split.is_some()
            } else {
                def.pellets >= 5
            };
            assert!(
                ok,
                "shell weapon {} ({}): pellets={} damage={}",
                meta.id, meta.wep_name, def.pellets, def.damage,
            );
        }
    }

    #[test]
    fn explosive_type_weapons_boom_or_burn() {
        // Every Explosives-ammo weapon is flagged explosive OR carries an
        // elemental hazard - except the PARTY GUN, which is deliberately inert
        // confetti.
        for meta in WEAPONS.iter().skip(1) {
            if meta.wep_type != AmmoType::Explosives {
                continue;
            }
            if meta.wep_name == "PARTY GUN" {
                continue;
            }
            let def = weapon_runtime_def(WeaponId(meta.id));
            assert!(
                def.explosive || def.hazard.is_some(),
                "explosive weapon {} ({}) neither booms nor burns",
                meta.id,
                meta.wep_name,
            );
        }
    }

    #[test]
    fn seeker_shotgun_is_long_lived_volley() {
        let def = weapon_runtime_def(id_by_name("SEEKER SHOTGUN"));

        assert!(def.lifetime >= 1.5, "seeker stand-in must outlive bullets");
        assert!(def.pierce >= 1);
        assert_eq!(def.ammo, AmmoKind::Bolts);
    }

    #[test]
    fn eraser_is_dense_fast_shrapnel() {
        let def = weapon_runtime_def(id_by_name("ERASER"));

        assert!(def.pellets >= 8);
        assert!(def.speed >= 650.0);
        assert_eq!(def.ammo, AmmoKind::Shells);
    }

    #[test]
    fn incinerator_is_flame_not_plain_bullet() {
        // Registry types it as Bullets; the name-based family must override
        // that into the flame class with fire residue.
        let family = weapon_family(id_by_name("INCINERATOR"));
        let def = weapon_runtime_def(id_by_name("INCINERATOR"));

        assert_eq!(family, WeaponFamily::Flame);
        assert!(matches!(def.hazard.map(|h| h.kind), Some(HazardKind::Fire)));
    }

    #[test]
    fn weapon_sleep_heavy_gt_auto() {
        let heavy = weapon_sleep_secs(id_by_name("SLEDGEHAMMER"));
        let auto = weapon_sleep_secs(id_by_name("SMG"));
        assert!(heavy > auto, "heavy sleep {heavy} should be > auto {auto}");
        let shotgun = weapon_sleep_secs(id_by_name("SHOTGUN"));
        assert!(
            shotgun > auto,
            "shotgun sleep {shotgun} should be > auto {auto}"
        );
    }

    #[test]
    fn most_weapons_are_specialized_not_generic() {
        // The old fixed fallback was damage=3 / pellets=1 / no specials.
        // After the family+profile pass, only a small minority may remain
        // fully generic.
        let mut genericish = 0;
        for meta in WEAPONS.iter().skip(1) {
            let def = weapon_runtime_def(WeaponId(meta.id));
            if def.damage == 3
                && def.pellets <= 1
                && def.bounces == 0
                && def.pierce == 0
                && def.hazard.is_none()
                && def.split.is_none()
                && !def.explosive
                && def.melee.is_none()
            {
                genericish += 1;
                println!("generic: {} {}", meta.id, meta.wep_name);
            }
        }
        assert!(
            genericish <= 30,
            "too many generic weapons remain: {genericish}"
        );
    }
}
