//! Data-driven content registries: characters, weapons, enemies, mutations.
//! All visuals are placeholder colored sprites; no external assets.
//! Stats mirror the GPL Nuclear-Throne-Mobile rebuild reference.

use bevy::prelude::*;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AmmoKind {
    Bullets,
    Shells,
    Bolts,
    Explosives,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CharacterId {
    Fish,
    Crystal,
    Eyes,
    Melting,
}

pub const CHARACTERS: [CharacterId; 4] = [
    CharacterId::Fish,
    CharacterId::Crystal,
    CharacterId::Eyes,
    CharacterId::Melting,
];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AbilityKind {
    Flip,
    Shield,
    Telekinesis,
    Detonate,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PassiveKind {
    None,
    ShieldOnHit,
    ChainExplosions,
}

pub struct CharacterDef {
    pub name: &'static str,
    pub color: Color,
    pub max_hp: i32,
    pub speed_mult: f32,
    pub pickup_range: f32,
    pub ability: AbilityKind,
    pub passive: PassiveKind,
    pub sprite: &'static str,
}

pub fn character_def(id: CharacterId) -> CharacterDef {
    match id {
        CharacterId::Fish => CharacterDef {
            name: "Fish",
            color: Color::srgb(0.25, 0.95, 0.35),
            max_hp: 8,
            speed_mult: 1.0,
            pickup_range: 95.0,
            ability: AbilityKind::Flip,
            passive: PassiveKind::None,
            sprite: "images/sprMutant1Idle.png",
        },
        CharacterId::Crystal => CharacterDef {
            name: "Crystal",
            color: Color::srgb(0.35, 0.65, 1.0),
            max_hp: 10,
            speed_mult: 1.0,
            pickup_range: 95.0,
            ability: AbilityKind::Shield,
            passive: PassiveKind::ShieldOnHit,
            sprite: "images/sprMutant2Idle.png",
        },
        CharacterId::Eyes => CharacterDef {
            name: "Eyes",
            color: Color::srgb(0.85, 0.4, 1.0),
            max_hp: 8,
            speed_mult: 1.0,
            pickup_range: 175.0,
            ability: AbilityKind::Telekinesis,
            passive: PassiveKind::None,
            sprite: "images/sprMutant3Idle.png",
        },
        CharacterId::Melting => CharacterDef {
            name: "Melting",
            color: Color::srgb(0.95, 0.85, 0.45),
            max_hp: 2,
            speed_mult: 1.0,
            pickup_range: 95.0,
            ability: AbilityKind::Detonate,
            passive: PassiveKind::ChainExplosions,
            sprite: "images/sprMutant4Idle.png",
        },
    }
}

pub fn ability_name(kind: AbilityKind) -> &'static str {
    match kind {
        AbilityKind::Flip => "Flip",
        AbilityKind::Shield => "Shield",
        AbilityKind::Telekinesis => "Telekinesis",
        AbilityKind::Detonate => "Detonate",
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WeaponKind {
    None,
    Revolver,
    Machinegun,
    Smg,
    AssaultRifle,
    Shotgun,
    Crossbow,
    GrenadeLauncher,
    Wrench,
    Sledgehammer,
}

#[derive(Clone, Copy)]
pub struct MeleeDef {
    pub range: f32,
    pub arc: f32,
}

#[derive(Clone, Copy)]
pub struct WeaponDef {
    pub name: &'static str,
    pub ammo: AmmoKind,
    pub ammo_cost: i32,
    pub cooldown: f32,
    pub damage: i32,
    pub pellets: usize,
    pub speed: f32,
    pub lifetime: f32,
    pub spread: f32,
    pub recoil: f32,
    pub shake: f32,
    pub projectile_radius: f32,
    pub knockback: f32,
    pub automatic: bool,
    pub explosive: bool,
    pub burst_shots: usize,
    pub burst_interval: f32,
    pub melee: Option<MeleeDef>,
    pub color: Color,
    pub size: Vec2,
    pub muzzle_burst: usize,
}

pub fn weapon_def(kind: WeaponKind) -> WeaponDef {
    match kind {
        WeaponKind::None => WeaponDef {
            name: "None",
            ammo: AmmoKind::Bullets,
            ammo_cost: 0,
            cooldown: 1.0,
            damage: 0,
            pellets: 0,
            speed: 0.0,
            lifetime: 0.1,
            spread: 0.0,
            recoil: 0.0,
            shake: 0.0,
            projectile_radius: 0.0,
            knockback: 0.0,
            automatic: false,
            explosive: false,
            burst_shots: 0,
            burst_interval: 0.0,
            melee: None,
            color: Color::srgb(0.4, 0.4, 0.4),
            size: Vec2::new(1.0, 1.0),
            muzzle_burst: 0,
        },
        WeaponKind::Revolver => WeaponDef {
            name: "Revolver",
            ammo: AmmoKind::Bullets,
            ammo_cost: 1,
            cooldown: frames(6.0),
            damage: 3,
            pellets: 1,
            speed: 960.0,
            lifetime: 0.95,
            spread: 0.07,
            recoil: 5.0,
            shake: 0.1,
            projectile_radius: 4.0,
            knockback: 150.0,
            automatic: false,
            explosive: false,
            burst_shots: 1,
            burst_interval: 0.0,
            melee: None,
            color: Color::srgb(1.0, 0.9, 0.25),
            size: Vec2::new(16.0, 5.0),
            muzzle_burst: 4,
        },
        WeaponKind::Machinegun => WeaponDef {
            name: "Machinegun",
            ammo: AmmoKind::Bullets,
            ammo_cost: 1,
            cooldown: frames(5.0),
            damage: 3,
            pellets: 1,
            speed: 960.0,
            lifetime: 0.85,
            spread: 0.105,
            recoil: 3.5,
            shake: 0.06,
            projectile_radius: 3.0,
            knockback: 70.0,
            automatic: true,
            explosive: false,
            burst_shots: 1,
            burst_interval: 0.0,
            melee: None,
            color: Color::srgb(1.0, 1.0, 0.35),
            size: Vec2::new(12.0, 4.0),
            muzzle_burst: 2,
        },
        WeaponKind::Smg => WeaponDef {
            name: "SMG",
            ammo: AmmoKind::Bullets,
            ammo_cost: 1,
            cooldown: frames(3.0),
            damage: 3,
            pellets: 1,
            speed: 960.0,
            lifetime: 0.7,
            spread: 0.28,
            recoil: 2.5,
            shake: 0.04,
            projectile_radius: 3.0,
            knockback: 50.0,
            automatic: true,
            explosive: false,
            burst_shots: 1,
            burst_interval: 0.0,
            melee: None,
            color: Color::srgb(1.0, 0.85, 0.3),
            size: Vec2::new(11.0, 4.0),
            muzzle_burst: 1,
        },
        WeaponKind::AssaultRifle => WeaponDef {
            name: "Assault Rifle",
            ammo: AmmoKind::Bullets,
            ammo_cost: 3,
            cooldown: frames(11.0),
            damage: 3,
            pellets: 1,
            speed: 960.0,
            lifetime: 0.9,
            spread: 0.035,
            recoil: 4.0,
            shake: 0.07,
            projectile_radius: 3.5,
            knockback: 60.0,
            automatic: true,
            explosive: false,
            burst_shots: 3,
            burst_interval: frames(1.0),
            melee: None,
            color: Color::srgb(0.95, 0.95, 0.5),
            size: Vec2::new(13.0, 4.0),
            muzzle_burst: 2,
        },
        WeaponKind::Shotgun => WeaponDef {
            name: "Shotgun",
            ammo: AmmoKind::Shells,
            ammo_cost: 1,
            cooldown: frames(17.0),
            damage: 2,
            pellets: 7,
            speed: 900.0,
            lifetime: 0.45,
            spread: 0.35,
            recoil: 16.0,
            shake: 0.24,
            projectile_radius: 4.0,
            knockback: 90.0,
            automatic: false,
            explosive: false,
            burst_shots: 1,
            burst_interval: 0.0,
            melee: None,
            color: Color::srgb(1.0, 0.72, 0.26),
            size: Vec2::new(10.0, 4.0),
            muzzle_burst: 6,
        },
        WeaponKind::Crossbow => WeaponDef {
            name: "Crossbow",
            ammo: AmmoKind::Bolts,
            ammo_cost: 1,
            cooldown: frames(26.0),
            damage: 20,
            pellets: 1,
            speed: 1440.0,
            lifetime: 1.2,
            spread: 0.015,
            recoil: 10.0,
            shake: 0.18,
            projectile_radius: 5.0,
            knockback: 300.0,
            automatic: false,
            explosive: false,
            burst_shots: 1,
            burst_interval: 0.0,
            melee: None,
            color: Color::srgb(0.65, 0.35, 0.12),
            size: Vec2::new(24.0, 5.0),
            muzzle_burst: 3,
        },
        WeaponKind::GrenadeLauncher => WeaponDef {
            name: "Grenade Launcher",
            ammo: AmmoKind::Explosives,
            ammo_cost: 1,
            cooldown: frames(20.0),
            damage: 15,
            pellets: 1,
            speed: 600.0,
            lifetime: 1.4,
            spread: 0.04,
            recoil: 18.0,
            shake: 0.3,
            projectile_radius: 7.0,
            knockback: 350.0,
            automatic: false,
            explosive: true,
            burst_shots: 1,
            burst_interval: 0.0,
            melee: None,
            color: Color::srgb(0.25, 0.95, 0.25),
            size: Vec2::splat(12.0),
            muzzle_burst: 5,
        },
        WeaponKind::Wrench => WeaponDef {
            name: "Wrench",
            ammo: AmmoKind::Bullets,
            ammo_cost: 0,
            cooldown: frames(22.0),
            damage: 8,
            pellets: 0,
            speed: 0.0,
            lifetime: 0.0,
            spread: 0.0,
            recoil: 0.0,
            shake: 0.0,
            projectile_radius: 0.0,
            knockback: 300.0,
            automatic: false,
            explosive: false,
            burst_shots: 1,
            burst_interval: 0.0,
            melee: Some(MeleeDef {
                range: 70.0,
                arc: 2.2,
            }),
            color: Color::srgb(0.7, 0.7, 0.75),
            size: Vec2::splat(20.0),
            muzzle_burst: 0,
        },
        WeaponKind::Sledgehammer => WeaponDef {
            name: "Sledgehammer",
            ammo: AmmoKind::Bullets,
            ammo_cost: 0,
            cooldown: frames(35.0),
            damage: 24,
            pellets: 0,
            speed: 0.0,
            lifetime: 0.0,
            spread: 0.0,
            recoil: 0.0,
            shake: 0.0,
            projectile_radius: 0.0,
            knockback: 600.0,
            automatic: false,
            explosive: false,
            burst_shots: 1,
            burst_interval: 0.0,
            melee: Some(MeleeDef {
                range: 96.0,
                arc: 2.6,
            }),
            color: Color::srgb(0.55, 0.5, 0.6),
            size: Vec2::splat(26.0),
            muzzle_burst: 0,
        },
    }
}

pub fn weapon_name(kind: WeaponKind) -> &'static str {
    weapon_def(kind).name
}

pub fn weapon_color(kind: WeaponKind) -> Color {
    weapon_def(kind).color
}

/// Ammo capacity per kind (reference: bullets 255, others 55). Back Muscle adds
/// +300 / +44 respectively.
pub fn ammo_max(kind: AmmoKind) -> i32 {
    match kind {
        AmmoKind::Bullets => 255,
        AmmoKind::Shells | AmmoKind::Bolts | AmmoKind::Explosives => 55,
    }
}

pub fn ammo_pickup_amount(kind: AmmoKind) -> i32 {
    match kind {
        AmmoKind::Bullets => 32,
        AmmoKind::Shells => 8,
        AmmoKind::Bolts => 7,
        AmmoKind::Explosives => 6,
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EnemyKind {
    Maggot,
    Bandit,
    Scorpion,
    Assassin,
    Freak,
    BigBandit,
    Throne,
}

#[derive(Clone, Copy)]
pub struct EnemyDef {
    pub name: &'static str,
    pub hp: i32,
    pub speed: f32,
    pub accel: f32,
    pub radius: f32,
    pub size: f32,
    pub color: Color,
    pub sprite: &'static str,
    pub score: u32,
    pub touch_damage: i32,
    pub rad_drop: usize,
    pub drop_chance: usize,
    pub weapon_chance: usize,
    pub preferred_range: f32,
    pub shoot_range: f32,
    pub attack_cooldown: f32,
    pub bullets_per_shot: usize,
    pub burst: bool,
    pub burst_interval: f32,
    pub fan_spread: f32,
    pub projectile_speed: f32,
    pub projectile_spread: f32,
    pub projectile_damage: i32,
    pub projectile_radius: f32,
    pub projectile_lifetime: f32,
    pub projectile_color: Color,
    pub projectile_size: f32,
    pub boss: bool,
}

pub fn enemy_def(kind: EnemyKind) -> EnemyDef {
    match kind {
        EnemyKind::Maggot => EnemyDef {
            name: "Maggot",
            hp: 2,
            speed: 75.0,
            accel: 1800.0,
            radius: 9.0,
            size: 17.0,
            color: Color::srgb(0.95, 0.55, 0.25),
            sprite: "images/sprMaggotIdle.png",
            score: 5,
            touch_damage: 1,
            rad_drop: 1,
            drop_chance: 0,
            weapon_chance: 0,
            preferred_range: 0.0,
            shoot_range: 0.0,
            attack_cooldown: 9.9,
            bullets_per_shot: 0,
            burst: false,
            burst_interval: 0.0,
            fan_spread: 0.0,
            projectile_speed: 0.0,
            projectile_spread: 0.0,
            projectile_damage: 0,
            projectile_radius: 0.0,
            projectile_lifetime: 0.0,
            projectile_color: Color::WHITE,
            projectile_size: 1.0,
            boss: false,
        },
        EnemyKind::Bandit => EnemyDef {
            name: "Bandit",
            hp: 4,
            speed: 24.0,
            accel: 800.0,
            radius: 12.0,
            size: 24.0,
            color: Color::srgb(0.85, 0.22, 0.18),
            sprite: "images/sprBanditIdle.png",
            score: 10,
            touch_damage: 0,
            rad_drop: 2,
            drop_chance: 16,
            weapon_chance: 0,
            preferred_range: 90.0,
            shoot_range: 480.0,
            attack_cooldown: 1.65,
            bullets_per_shot: 1,
            burst: false,
            burst_interval: 0.0,
            fan_spread: 0.0,
            projectile_speed: 240.0,
            projectile_spread: 0.175,
            projectile_damage: 3,
            projectile_radius: 4.0,
            projectile_lifetime: 3.5,
            projectile_color: Color::srgb(1.0, 0.35, 0.08),
            projectile_size: 8.0,
            boss: false,
        },
        EnemyKind::Scorpion => EnemyDef {
            name: "Scorpion",
            hp: 16,
            speed: 24.0,
            accel: 800.0,
            radius: 14.0,
            size: 28.0,
            color: Color::srgb(0.35, 0.85, 0.28),
            sprite: "images/sprScorpionIdle.png",
            score: 18,
            touch_damage: 5,
            rad_drop: 10,
            drop_chance: 15,
            weapon_chance: 0,
            preferred_range: 120.0,
            shoot_range: 210.0,
            attack_cooldown: 0.75,
            bullets_per_shot: 10,
            burst: true,
            burst_interval: 0.033,
            fan_spread: 0.0,
            projectile_speed: 210.0,
            projectile_spread: 0.175,
            projectile_damage: 2,
            projectile_radius: 4.0,
            projectile_lifetime: 3.0,
            projectile_color: Color::srgb(0.35, 1.0, 0.25),
            projectile_size: 7.0,
            boss: false,
        },
        EnemyKind::Assassin => EnemyDef {
            name: "Assassin",
            hp: 14,
            speed: 168.0,
            accel: 880.0,
            radius: 11.0,
            size: 22.0,
            color: Color::srgb(0.2, 0.18, 0.24),
            sprite: "images/sprAssassinIdle.png",
            score: 25,
            touch_damage: 3,
            rad_drop: 4,
            drop_chance: 16,
            weapon_chance: 0,
            preferred_range: 0.0,
            shoot_range: 0.0,
            attack_cooldown: 9.9,
            bullets_per_shot: 0,
            burst: false,
            burst_interval: 0.0,
            fan_spread: 0.0,
            projectile_speed: 0.0,
            projectile_spread: 0.0,
            projectile_damage: 0,
            projectile_radius: 0.0,
            projectile_lifetime: 0.0,
            projectile_color: Color::WHITE,
            projectile_size: 1.0,
            boss: false,
        },
        EnemyKind::Freak => EnemyDef {
            name: "Freak",
            hp: 7,
            speed: 225.0,
            accel: 5400.0,
            radius: 13.0,
            size: 26.0,
            color: Color::srgb(0.6, 0.35, 0.85),
            sprite: "images/sprFreak1Idle.png",
            score: 15,
            touch_damage: 3,
            rad_drop: 1,
            drop_chance: 10,
            weapon_chance: 0,
            preferred_range: 0.0,
            shoot_range: 0.0,
            attack_cooldown: 9.9,
            bullets_per_shot: 0,
            burst: false,
            burst_interval: 0.0,
            fan_spread: 0.0,
            projectile_speed: 0.0,
            projectile_spread: 0.0,
            projectile_damage: 0,
            projectile_radius: 0.0,
            projectile_lifetime: 0.0,
            projectile_color: Color::WHITE,
            projectile_size: 1.0,
            boss: false,
        },
        EnemyKind::BigBandit => EnemyDef {
            name: "Big Bandit",
            hp: 100,
            speed: 24.0,
            accel: 800.0,
            radius: 26.0,
            size: 56.0,
            color: Color::srgb(0.55, 0.1, 0.1),
            sprite: "images/sprBanditBossIdle.png",
            score: 100,
            touch_damage: 0,
            rad_drop: 30,
            drop_chance: 200,
            weapon_chance: 0,
            preferred_range: 180.0,
            shoot_range: 240.0,
            attack_cooldown: 1.1,
            bullets_per_shot: 10,
            burst: true,
            burst_interval: 0.067,
            fan_spread: 0.0,
            projectile_speed: 480.0,
            projectile_spread: 0.131,
            projectile_damage: 3,
            projectile_radius: 5.0,
            projectile_lifetime: 2.5,
            projectile_color: Color::srgb(1.0, 0.5, 0.1),
            projectile_size: 10.0,
            boss: true,
        },
        EnemyKind::Throne => EnemyDef {
            name: "Throne",
            hp: 900,
            speed: 40.0,
            accel: 220.0,
            radius: 34.0,
            size: 72.0,
            color: Color::srgb(0.15, 0.25, 0.45),
            sprite: "images/sprThroneIdle.png",
            score: 200,
            touch_damage: 5,
            rad_drop: 40,
            drop_chance: 200,
            weapon_chance: 0,
            preferred_range: 260.0,
            shoot_range: 460.0,
            attack_cooldown: 2.1,
            bullets_per_shot: 12,
            burst: false,
            burst_interval: 0.0,
            fan_spread: std::f32::consts::TAU,
            projectile_speed: 210.0,
            projectile_spread: 0.175,
            projectile_damage: 2,
            projectile_radius: 6.0,
            projectile_lifetime: 3.5,
            projectile_color: Color::srgb(0.5, 0.7, 1.0),
            projectile_size: 9.0,
            boss: true,
        },
    }
}

pub fn is_boss(kind: EnemyKind) -> bool {
    enemy_def(kind).boss
}

/// NT simulation runs at 30 FPS; GML `wep_load` is frames.
#[inline]
pub const fn frames(f: f32) -> f32 {
    f / 30.0
}

pub fn nt_cooldown_secs(wep_id: u8) -> f32 {
    let w = &crate::game::weapons_data::WEAPONS[wep_id as usize];
    w.wep_load as f32 / 30.0
}

pub fn weapon_gml_id(kind: WeaponKind) -> u8 {
    match kind {
        WeaponKind::None => 0,
        WeaponKind::Revolver => 1,
        WeaponKind::Wrench => 3,
        WeaponKind::Machinegun => 4,
        WeaponKind::Shotgun => 5,
        WeaponKind::Crossbow => 6,
        WeaponKind::GrenadeLauncher => 7,
        WeaponKind::Smg => 16,
        WeaponKind::AssaultRifle => 17,
        WeaponKind::Sledgehammer => 88,
    }
}

pub fn sprite_or_fallback(
    asset_server: &AssetServer,
    path: &'static str,
    fallback_color: Color,
    size: Vec2,
) -> Sprite {
    let looks_like_og = path.starts_with("images/spr");
    if looks_like_og {
        Sprite {
            image: asset_server.load(path),
            // NT art is pre-colored; do not multiply with fallback tint
            color: Color::WHITE,
            custom_size: Some(size),
            ..Default::default()
        }
    } else {
        Sprite {
            color: fallback_color,
            custom_size: Some(size),
            ..Default::default()
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MutationId {
    RhinoSkin,
    PlutoniumHunger,
    TriggerFingers,
    RabbitPaw,
    SecondStomach,
    ScarierFace,
    BoilingVeins,
    ImpactWrists,
    ExtraFeet,
    Bloodlust,
    LuckyShot,
    GammaGuts,
    BackMuscle,
    Euphoria,
    LongArms,
    Stress,
    EagleEyes,
    OpenMind,
    HeavyHeart,
    StrongSpirit,
    SharpTeeth,
    LastWish,
}

pub const ALL_MUTATIONS: [MutationId; 22] = [
    MutationId::RhinoSkin,
    MutationId::PlutoniumHunger,
    MutationId::TriggerFingers,
    MutationId::RabbitPaw,
    MutationId::SecondStomach,
    MutationId::ScarierFace,
    MutationId::BoilingVeins,
    MutationId::ImpactWrists,
    MutationId::ExtraFeet,
    MutationId::Bloodlust,
    MutationId::LuckyShot,
    MutationId::GammaGuts,
    MutationId::BackMuscle,
    MutationId::Euphoria,
    MutationId::LongArms,
    MutationId::Stress,
    MutationId::EagleEyes,
    MutationId::OpenMind,
    MutationId::HeavyHeart,
    MutationId::StrongSpirit,
    MutationId::SharpTeeth,
    MutationId::LastWish,
];

pub struct MutationDef {
    pub name: &'static str,
    pub description: &'static str,
}

pub fn mutation_def(id: MutationId) -> MutationDef {
    match id {
        MutationId::RhinoSkin => MutationDef {
            name: "Rhino Skin",
            description: "+4 max HP",
        },
        MutationId::PlutoniumHunger => MutationDef {
            name: "Plutonium Hunger",
            description: "Much larger pickup range",
        },
        MutationId::TriggerFingers => MutationDef {
            name: "Trigger Fingers",
            description: "Kills lower reload time",
        },
        MutationId::RabbitPaw => MutationDef {
            name: "Rabbit Paw",
            description: "Better chance for drops",
        },
        MutationId::SecondStomach => MutationDef {
            name: "Second Stomach",
            description: "Medkits heal double",
        },
        MutationId::ScarierFace => MutationDef {
            name: "Scarier Face",
            description: "Enemies have less HP",
        },
        MutationId::BoilingVeins => MutationDef {
            name: "Boiling Veins",
            description: "Explosions can't drop you below 4 HP",
        },
        MutationId::ImpactWrists => MutationDef {
            name: "Impact Wrists",
            description: "Weapons knock back harder",
        },
        MutationId::ExtraFeet => MutationDef {
            name: "Extra Feet",
            description: "Move faster",
        },
        MutationId::Bloodlust => MutationDef {
            name: "Bloodlust",
            description: "Kills sometimes heal you",
        },
        MutationId::LuckyShot => MutationDef {
            name: "Lucky Shot",
            description: "Kills sometimes drop ammo",
        },
        MutationId::GammaGuts => MutationDef {
            name: "Gamma Guts",
            description: "Enemies that touch you take damage",
        },
        MutationId::BackMuscle => MutationDef {
            name: "Back Muscle",
            description: "Higher ammo capacity",
        },
        MutationId::Euphoria => MutationDef {
            name: "Euphoria",
            description: "Enemy bullets are slower",
        },
        MutationId::LongArms => MutationDef {
            name: "Long Arms",
            description: "Melee attacks reach further",
        },
        MutationId::Stress => MutationDef {
            name: "Stress",
            description: "Fire faster at low health",
        },
        MutationId::EagleEyes => MutationDef {
            name: "Eagle Eyes",
            description: "Better accuracy",
        },
        MutationId::OpenMind => MutationDef {
            name: "Open Mind",
            description: "More chests spawn",
        },
        MutationId::HeavyHeart => MutationDef {
            name: "Heavy Heart",
            description: "More weapon drops",
        },
        MutationId::StrongSpirit => MutationDef {
            name: "Strong Spirit",
            description: "Prevents death, once",
        },
        MutationId::SharpTeeth => MutationDef {
            name: "Sharp Teeth",
            description: "Damage taken also hurts nearby enemies",
        },
        MutationId::LastWish => MutationDef {
            name: "Last Wish",
            description: "Heal and refill ammo when low",
        },
    }
}
