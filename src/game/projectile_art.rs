use bevy::prelude::*;

use crate::game::content::{AssetCatalog, EnemyKind, WeaponId, weapon_meta};
use crate::game::weapons_data::AmmoType;

/// Resolve the best projectile sprite for a player weapon.
/// Returns an asset path like "images/sprBullet1.png".
/// Mapping is 1:1 from `scrFire.gml` projectile objects to their
/// `spriteId` (see `objects/<Projectile>/<Projectile>.yy`).
pub fn player_projectile_path(id: WeaponId) -> &'static str {
    let id = crate::game::content::sanitize_weapon_id(id);
    if id == WeaponId::NONE {
        return "images/sprBullet1.png";
    }
    // Exact per-weapon mapping derived from `scrFire.gml` switch.
    // Burst wrappers (Burst/HyperBurst/etc.) are resolved to their inner
    // projectile sprite so the art matches the on-screen bullet.
    match id.0 {
        // 1 REVOLVER -> Bullet1
        1 => "images/sprBullet1.png",
        // 2 TRIPLE MACHINEGUN -> Bullet1 x3
        2 => "images/sprBullet1.png",
        // 3 WRENCH -> Slash (melee)
        3 => "images/sprSlash.png",
        // 4 MACHINEGUN -> Bullet1
        4 => "images/sprBullet1.png",
        // 5 SHOTGUN -> Bullet2
        5 => "images/sprBullet2.png",
        // 6 CROSSBOW -> Bolt
        6 => "images/sprBolt.png",
        // 7 GRENADE LAUNCHER -> Grenade
        7 => "images/sprGrenade.png",
        // 8 DOUBLE SHOTGUN -> Bullet2 x14
        8 => "images/sprBullet2.png",
        // 9 MINIGUN -> Bullet1
        9 => "images/sprBullet1.png",
        // 10 AUTO SHOTGUN -> Bullet2
        10 => "images/sprBullet2.png",
        // 11 AUTO CROSSBOW -> Bolt
        11 => "images/sprBolt.png",
        // 12 SUPER CROSSBOW -> Bolt x5
        12 => "images/sprBolt.png",
        // 13 SHOVEL -> Slash
        13 => "images/sprSlash.png",
        // 14 BAZOOKA -> Rocket
        14 => "images/sprRocket.png",
        // 15 STICKY LAUNCHER -> Grenade (sprStickyGrenade)
        15 => "images/sprStickyGrenade.png",
        // 16 SMG -> Bullet1
        16 => "images/sprBullet1.png",
        // 17 ASSAULT RIFLE -> Burst -> Bullet1
        17 => "images/sprBullet1.png",
        // 18 DISC GUN -> Disc
        18 => "images/sprDisc.png",
        // 19 LASER PISTOL -> Laser
        19 => "images/sprLaser.png",
        // 20 LASER RIFLE -> Laser
        20 => "images/sprLaser.png",
        // 21 SLUGGER -> Slug
        21 => "images/sprSlugBullet.png",
        // 22 GATLING SLUGGER -> Slug
        22 => "images/sprSlugBullet.png",
        // 23 ASSAULT SLUGGER -> SlugBurst -> Slug
        23 => "images/sprSlugBullet.png",
        // 24 ENERGY SWORD -> EnergySlash (melee)
        24 => "images/sprEnergySlash.png",
        // 25 SUPER SLUGGER -> Slug x5
        25 => "images/sprSlugBullet.png",
        // 26 HYPER RIFLE -> HyperBurst -> Bullet1
        26 => "images/sprBullet1.png",
        // 27 SCREWDRIVER -> Shank (melee)
        27 => "images/sprShank.png",
        // 28 LASER MINIGUN -> Laser
        28 => "images/sprLaser.png",
        // 29 BLOOD LAUNCHER -> BloodGrenade
        29 => "images/sprBloodGrenade.png",
        // 30 SPLINTER GUN -> Splinter
        30 => "images/sprSplinter.png",
        // 31 TOXIC BOW -> ToxicBolt
        31 => "images/sprToxicBolt.png",
        // 32 SENTRY GUN -> SentryGun entity (no bullet)
        32 => "images/sprBullet1.png",
        // 33 WAVE GUN -> WaveBurst -> Bullet2 x9? actually Bullet2
        33 => "images/sprBullet2.png",
        // 34 PLASMA GUN -> PlasmaBall
        34 => "images/sprPlasmaBall.png",
        // 35 PLASMA CANNON -> PlasmaBig
        35 => "images/sprPlasmaBallBig.png",
        // 36 ENERGY HAMMER -> EnergyHammerSlash (melee)
        36 => "images/sprEnergyHammer.png",
        // 37 JACKHAMMER -> SawBurst -> Shank
        37 => "images/sprShank.png",
        // 38 FLAK CANNON -> FlakBullet
        38 => "images/sprFlakBullet.png",
        // 39 GOLDEN REVOLVER -> Bullet1
        39 => "images/sprBullet1.png",
        // 40 GOLDEN WRENCH -> Slash
        40 => "images/sprSlash.png",
        // 41 GOLDEN MACHINEGUN -> Bullet1
        41 => "images/sprBullet1.png",
        // 42 GOLDEN SHOTGUN -> Bullet2
        42 => "images/sprBullet2.png",
        // 43 GOLDEN CROSSBOW -> BoltGold
        43 => "images/sprBoltGold.png",
        // 44 GOLDEN GRENADE LAUNCHER -> GoldGrenade
        44 => "images/sprGoldGrenade.png",
        // 45 GOLDEN LASER PISTOL -> Laser
        45 => "images/sprLaser.png",
        // 46 CHICKEN SWORD -> Slash
        46 => "images/sprSlash.png",
        // 47 NUKE LAUNCHER -> Nuke
        47 => "images/sprNuke.png",
        // 48 ION CANNON -> IonBurst (energy)
        48 => "images/sprPlasmaBall.png",
        // 49 QUADRUPLE MACHINEGUN -> Bullet1 x4
        49 => "images/sprBullet1.png",
        // 50 FLAMETHROWER -> FlameBurst -> Flame (sprTrapFire)
        50 => "images/sprTrapFire.png",
        // 51 DRAGON -> DragonBurst -> Flame
        51 => "images/sprTrapFire.png",
        // 52 FLARE GUN -> Flare
        52 => "images/sprFlare.png",
        // 53 ENERGY SCREWDRIVER -> EnergyShank (melee)
        53 => "images/sprEnergySlash.png",
        // 54 HYPER LAUNCHER -> HyperGrenade (sprPopoNade)
        54 => "images/sprPopoNade.png",
        // 55 LASER CANNON -> LaserCannon (laser)
        55 => "images/sprLaser.png",
        // 56 RUSTY REVOLVER -> Bullet1
        56 => "images/sprBullet1.png",
        // 57 LIGHTNING PISTOL -> Lightning
        57 => "images/sprLightning.png",
        // 58 LIGHTNING RIFLE -> Lightning
        58 => "images/sprLightning.png",
        // 59 LIGHTNING SHOTGUN -> Lightning x8
        59 => "images/sprLightning.png",
        // 60 SUPER FLAK CANNON -> SuperFlakBullet
        60 => "images/sprSuperFlakBullet.png",
        // 61 SAWED-OFF SHOTGUN -> Bullet2 x20
        61 => "images/sprBullet2.png",
        // 62 SPLINTER PISTOL -> Splinter x4
        62 => "images/sprSplinter.png",
        // 63 SUPER SPLINTER GUN -> SplinterBurst -> Splinter
        63 => "images/sprSplinter.png",
        // 64 LIGHTNING SMG -> Lightning
        64 => "images/sprLightning.png",
        // 65 SMART GUN -> Bullet1 (homing)
        65 => "images/sprBullet1.png",
        // 66 HEAVY CROSSBOW -> HeavyBolt
        66 => "images/sprHeavyBolt.png",
        // 67 BLOOD HAMMER -> BloodSlash (melee)
        67 => "images/sprBloodSlash.png",
        // 68 LIGHTNING CANNON -> LightningBall
        68 => "images/sprLightningBall.png",
        // 69 POP GUN -> Bullet2
        69 => "images/sprBullet2.png",
        // 70 PLASMA RIFLE -> PlasmaBall
        70 => "images/sprPlasmaBall.png",
        // 71 POP RIFLE -> PopBurst -> Bullet2
        71 => "images/sprBullet2.png",
        // 72 TOXIC LAUNCHER -> ToxicGrenade
        72 => "images/sprToxicGrenade.png",
        // 73 FLAME CANNON -> FlameBall
        73 => "images/sprFlameBall.png",
        // 74 LIGHTNING HAMMER -> LightningSlash (melee)
        74 => "images/sprLightningSlash.png",
        // 75 FLAME SHOTGUN -> FlameShell
        75 => "images/sprFireShell.png",
        // 76 DOUBLE FLAME SHOTGUN -> FlameShell x14
        76 => "images/sprFireShell.png",
        // 77 AUTO FLAME SHOTGUN -> FlameShell
        77 => "images/sprFireShell.png",
        // 78 CLUSTER LAUNCHER -> ClusterNade
        78 => "images/sprClusterNade.png",
        // 79 GRENADE SHOTGUN -> SmallGrenade (sprMininade)
        79 => "images/sprMininade.png",
        // 80 GRENADE RIFLE -> NadeBurst -> SmallGrenade
        80 => "images/sprMininade.png",
        // 81 ROGUE RIFLE -> IDPDBurst -> Bullet1
        81 => "images/sprBullet1.png",
        // 82 PARTY GUN -> ConfettiBall
        82 => "images/sprConfettiBall.png",
        // 83 DOUBLE MINIGUN -> Bullet1 x2
        83 => "images/sprBullet1.png",
        // 84 GATLING BAZOOKA -> Rocket
        84 => "images/sprRocket.png",
        // 85 AUTO GRENADE SHOTGUN -> SmallGrenade
        85 => "images/sprMininade.png",
        // 86 ULTRA REVOLVER -> UltraBullet
        86 => "images/sprUltraBullet.png",
        // 87 ULTRA LASER PISTOL -> Laser x5
        87 => "images/sprLaser.png",
        // 88 SLEDGEHAMMER -> Slash (heavy)
        88 => "images/sprSlash.png",
        // 89 HEAVY REVOLVER -> HeavyBullet
        89 => "images/sprHeavyBullet.png",
        // 90 HEAVY MACHINEGUN -> HeavyBullet
        90 => "images/sprHeavyBullet.png",
        // 91 HEAVY SLUGGER -> HeavySlug
        91 => "images/sprHeavySlug.png",
        // 92 ULTRA SHOVEL -> UltraSlash
        92 => "images/sprUltraSlash.png",
        // 93 ULTRA SHOTGUN -> UltraShell
        93 => "images/sprUltraShell.png",
        // 94 ULTRA CROSSBOW -> UltraBolt
        94 => "images/sprUltraBolt.png",
        // 95 ULTRA GRENADE LAUNCHER -> UltraGrenade
        95 => "images/sprUltraGrenade.png",
        // 96 PLASMA MINIGUN -> PlasmaBall
        96 => "images/sprPlasmaBall.png",
        // 97 DEVASTATOR -> Devastator (plasma)
        97 => "images/sprPlasmaBall.png",
        // 98 GOLDEN PLASMA GUN -> PlasmaBall
        98 => "images/sprPlasmaBall.png",
        // 99 GOLDEN SLUGGER -> Slug
        99 => "images/sprSlugBullet.png",
        // 100 GOLDEN SPLINTER GUN -> Splinter
        100 => "images/sprSplinter.png",
        // 101 GOLDEN SCREWDRIVER -> Shank
        101 => "images/sprShank.png",
        // 102 GOLDEN BAZOOKA -> GoldRocket
        102 => "images/sprGoldRocket.png",
        // 103 GOLDEN ASSAULT RIFLE -> Burst -> Bullet1
        103 => "images/sprBullet1.png",
        // 104 SUPER DISC GUN -> Disc (5 discs)
        104 => "images/sprDisc.png",
        // 105 HEAVY AUTO CROSSBOW -> HeavyBolt
        105 => "images/sprHeavyBolt.png",
        // 106 HEAVY ASSAULT RIFLE -> HeavyBurst -> HeavyBullet
        106 => "images/sprHeavyBullet.png",
        // 107 BLOOD CANNON -> BloodBall
        107 => "images/sprBloodBall.png",
        // 108 DOG SPIN ATTACK -> melee
        108 => "images/sprSlash.png",
        // 109 DOG MISSILE -> Rocket (fallback)
        109 => "images/sprRocket.png",
        // 110 INCINERATOR -> FlameShell x3
        110 => "images/sprFireShell.png",
        // 111 SUPER PLASMA CANNON -> PlasmaHuge
        111 => "images/sprPlasmaBallHuge.png",
        // 112 SEEKER PISTOL -> Seeker
        112 => "images/sprSeeker.png",
        // 113 SEEKER SHOTGUN -> Seeker x6
        113 => "images/sprSeeker.png",
        // 114 ERASER -> Bullet2 x17
        114 => "images/sprBullet2.png",
        // 115 GUITAR -> Slash
        115 => "images/sprSlash.png",
        // 116 BOUNCER SMG -> BouncerBullet
        116 => "images/sprBouncerBullet.png",
        // 117 BOUNCER SHOTGUN -> BouncerBullet (sprBouncerShell for some pellets)
        117 => "images/sprBouncerBullet.png",
        // 118 HYPER SLUGGER -> HyperSlug -> sprSlugBullet
        118 => "images/sprSlugBullet.png",
        // 119 SUPER BAZOOKA -> Rocket x5
        119 => "images/sprRocket.png",
        // 120 FROG PISTOL -> EnemyBullet2 (sprScorpionBullet)
        120 => "images/sprScorpionBullet.png",
        // 121 BLACK SWORD -> Slash (MegaSlash when low)
        121 => "images/sprSlash.png",
        // 122 GOLDEN NUKE LAUNCHER -> GoldNuke
        122 => "images/sprGoldNuke.png",
        // 123 GOLDEN DISC GUN -> GoldDisc
        123 => "images/sprGoldDisc.png",
        // 124 HEAVY GRENADE LAUNCHER -> HeavyGrenade (sprHeavyNade)
        124 => "images/sprHeavyNade.png",
        // 125 GUN GUN -> GunGun pickup (melee-ish)
        125 => "images/sprBullet1.png",
        // 126 EGGPLANT -> unknown (fallback)
        126 => "images/sprBullet1.png",
        // 127 GOLDEN FROG PISTOL -> EnemyBullet2
        127 => "images/sprScorpionBullet.png",
        // 128 ELECTRIC GUITAR -> Slash (electric)
        128 => "images/sprSlash.png",
        _ => {
            // Fallback by ammo family for any future ids
            let meta = weapon_meta(id);
            match meta.wep_type {
                AmmoType::Shells => "images/sprBullet2.png",
                AmmoType::Bolts => "images/sprBolt.png",
                AmmoType::Explosives => "images/sprGrenade.png",
                AmmoType::Energy => "images/sprPlasmaBall.png",
                AmmoType::None => "images/sprSlash.png",
                AmmoType::Bullets => "images/sprBullet1.png",
            }
        }
    }
}

pub fn enemy_projectile_path(kind: EnemyKind) -> &'static str {
    match kind {
        EnemyKind::Scorpion | EnemyKind::GoldScorpion => "images/sprScorpionBullet.png",
        EnemyKind::Jock => "images/sprJockRocket.png",
        EnemyKind::SnowTank => "images/sprRocket.png",
        EnemyKind::GoldSnowtank => "images/sprGoldTankRocket.png",
        EnemyKind::Guardian => "images/sprGuardianBullet.png",
        EnemyKind::ExploGuardian => "images/sprHorrorBullet.png",
        EnemyKind::DogGuardian => "images/sprHeavyBullet.png",
        EnemyKind::Crystal
        | EnemyKind::LaserCrystal
        | EnemyKind::InvLaserCrystal
        | EnemyKind::LightningCrystal => "images/sprGuardianBullet.png",
        EnemyKind::FireBaller | EnemyKind::SuperFireBaller => "images/sprFlameBall.png",
        EnemyKind::Turtle => "images/sprGuardianBullet.png",
        EnemyKind::Sniper => "images/sprBullet1.png",
        EnemyKind::Bandit
        | EnemyKind::JungleBandit
        | EnemyKind::SnowBandit
        | EnemyKind::MeleeBandit => "images/sprEnemyBullet1.png",
        EnemyKind::Rat | EnemyKind::BigRat | EnemyKind::FastRat | EnemyKind::Ratking => {
            "images/sprEnemyBullet1.png"
        }
        EnemyKind::Gator | EnemyKind::BuffGator => "images/sprEnemyBullet1.png",
        EnemyKind::Raven => "images/sprEnemyBullet1.png",
        EnemyKind::Spider | EnemyKind::InvSpider => "images/sprEnemyBullet1.png",
        EnemyKind::Salamander => "images/sprSalamanderBullet.png",
        EnemyKind::Freak | EnemyKind::RhinoFreak | EnemyKind::ExploFreak | EnemyKind::PopoFreak => {
            "images/sprEnemyBullet1.png"
        }
        EnemyKind::IdpdGrunt
        | EnemyKind::IdpdShield
        | EnemyKind::IdpdElite
        | EnemyKind::IdpdInspector => "images/sprIDPDBullet.png",
        EnemyKind::Maggot | EnemyKind::BigMaggot | EnemyKind::MaggotSpawn => {
            "images/sprMaggotBullet.png"
        }
        _ => "images/sprEnemyBullet1.png",
    }
}

/// Pick first path that exists in catalog, fallback to first.
pub fn first_existing<'a>(
    catalog: &crate::game::content::AssetCatalog,
    paths: &[&'a str],
) -> &'a str {
    for p in paths {
        if catalog.has(p) {
            return p;
        }
    }
    paths.first().copied().unwrap_or("images/sprBullet1.png")
}

pub fn sprite_from_projectile_path(
    asset_server: &AssetServer,
    catalog: &AssetCatalog,
    candidates: &[&'static str],
    custom_size: Option<Vec2>,
) -> Sprite {
    let path = first_existing(catalog, candidates);

    Sprite {
        image: asset_server.load(path),
        color: Color::WHITE,
        custom_size,
        ..default()
    }
}

pub fn player_projectile_sprite(
    asset_server: &AssetServer,
    catalog: &AssetCatalog,
    weapon: WeaponId,
    custom_size: Option<Vec2>,
) -> Sprite {
    let candidates = player_projectile_candidates(weapon);
    sprite_from_projectile_path(asset_server, catalog, &candidates, custom_size)
}

pub fn enemy_projectile_sprite(
    asset_server: &AssetServer,
    catalog: &AssetCatalog,
    kind: EnemyKind,
    custom_size: Option<Vec2>,
) -> Sprite {
    let primary = enemy_projectile_path(kind);
    sprite_from_projectile_path(
        asset_server,
        catalog,
        &[
            primary,
            "images/sprEnemyBullet1.png",
            "images/sprBullet1.png",
        ],
        custom_size,
    )
}

pub fn generic_enemy_bullet_sprite(
    asset_server: &AssetServer,
    catalog: &AssetCatalog,
    custom_size: Option<Vec2>,
) -> Sprite {
    sprite_from_projectile_path(
        asset_server,
        catalog,
        &["images/sprEnemyBullet1.png", "images/sprBullet1.png"],
        custom_size,
    )
}

pub fn generic_player_bullet_sprite(
    asset_server: &AssetServer,
    catalog: &AssetCatalog,
    custom_size: Option<Vec2>,
) -> Sprite {
    sprite_from_projectile_path(
        asset_server,
        catalog,
        &["images/sprBullet1.png"],
        custom_size,
    )
}

pub fn plasma_child_sprite(
    asset_server: &AssetServer,
    catalog: &AssetCatalog,
    size: Vec2,
) -> Sprite {
    let path = if size.x >= 14.0 || size.y >= 14.0 {
        "images/sprPlasmaBallBig.png"
    } else {
        "images/sprPlasmaBall.png"
    };

    sprite_from_projectile_path(
        asset_server,
        catalog,
        &[path, "images/sprPlasmaBall.png", "images/sprBullet1.png"],
        None,
    )
}

/// Helper to resolve ordered candidates for a weapon.
pub fn player_projectile_candidates(id: WeaponId) -> Vec<&'static str> {
    let primary = player_projectile_path(id);
    let mut out = vec![primary];
    // Gold variants already resolved; add ammo family fallbacks for robustness
    // if the exact art was not imported (e.g. missing sprBoltGold).
    let meta = weapon_meta(crate::game::content::sanitize_weapon_id(id));
    match meta.wep_type {
        AmmoType::Shells => out.push("images/sprBullet2.png"),
        AmmoType::Bolts => out.push("images/sprBolt.png"),
        AmmoType::Explosives => out.push("images/sprGrenade.png"),
        AmmoType::Energy => out.push("images/sprPlasmaBall.png"),
        AmmoType::None => out.push("images/sprSlash.png"),
        AmmoType::Bullets => out.push("images/sprBullet1.png"),
    }
    out.push("images/sprBullet1.png");
    out
}
