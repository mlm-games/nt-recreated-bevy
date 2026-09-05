use bevy::prelude::*;

use crate::game::content::{AssetCatalog, EnemyKind, WeaponId, weapon_meta};
use crate::game::weapons_data::AmmoType;

pub fn player_projectile_path(id: WeaponId) -> &'static str {
    let id = crate::game::content::sanitize_weapon_id(id);
    if id == WeaponId::NONE {
        return "images/sprBullet1.png";
    }

    match id.0 {

        1 => "images/sprBullet1.png",

        2 => "images/sprBullet1.png",

        3 => "images/sprSlash.png",

        4 => "images/sprBullet1.png",

        5 => "images/sprBullet2.png",

        6 => "images/sprBolt.png",

        7 => "images/sprGrenade.png",

        8 => "images/sprBullet2.png",

        9 => "images/sprBullet1.png",

        10 => "images/sprBullet2.png",

        11 => "images/sprBolt.png",

        12 => "images/sprBolt.png",

        13 => "images/sprSlash.png",

        14 => "images/sprRocket.png",

        15 => "images/sprStickyGrenade.png",

        16 => "images/sprBullet1.png",

        17 => "images/sprBullet1.png",

        18 => "images/sprDisc.png",

        19 => "images/sprLaser.png",

        20 => "images/sprLaser.png",

        21 => "images/sprSlugBullet.png",

        22 => "images/sprSlugBullet.png",

        23 => "images/sprSlugBullet.png",

        24 => "images/sprEnergySlash.png",

        25 => "images/sprSlugBullet.png",

        26 => "images/sprBullet1.png",

        27 => "images/sprShank.png",

        28 => "images/sprLaser.png",

        29 => "images/sprBloodGrenade.png",

        30 => "images/sprSplinter.png",

        31 => "images/sprToxicBolt.png",

        32 => "images/sprBullet1.png",

        33 => "images/sprBullet2.png",

        34 => "images/sprPlasmaBall.png",

        35 => "images/sprPlasmaBallBig.png",

        36 => "images/sprEnergyHammer.png",

        37 => "images/sprShank.png",

        38 => "images/sprFlakBullet.png",

        39 => "images/sprBullet1.png",

        40 => "images/sprSlash.png",

        41 => "images/sprBullet1.png",

        42 => "images/sprBullet2.png",

        43 => "images/sprBoltGold.png",

        44 => "images/sprGoldGrenade.png",

        45 => "images/sprLaser.png",

        46 => "images/sprSlash.png",

        47 => "images/sprNuke.png",

        48 => "images/sprPlasmaBall.png",

        49 => "images/sprBullet1.png",

        50 => "images/sprTrapFire.png",

        51 => "images/sprTrapFire.png",

        52 => "images/sprFlare.png",

        53 => "images/sprEnergySlash.png",

        54 => "images/sprPopoNade.png",

        55 => "images/sprLaser.png",

        56 => "images/sprBullet1.png",

        57 => "images/sprLightning.png",

        58 => "images/sprLightning.png",

        59 => "images/sprLightning.png",

        60 => "images/sprSuperFlakBullet.png",

        61 => "images/sprBullet2.png",

        62 => "images/sprSplinter.png",

        63 => "images/sprSplinter.png",

        64 => "images/sprLightning.png",

        65 => "images/sprBullet1.png",

        66 => "images/sprHeavyBolt.png",

        67 => "images/sprBloodSlash.png",

        68 => "images/sprLightningBall.png",

        69 => "images/sprBullet2.png",

        70 => "images/sprPlasmaBall.png",

        71 => "images/sprBullet2.png",

        72 => "images/sprToxicGrenade.png",

        73 => "images/sprFlameBall.png",

        74 => "images/sprLightningSlash.png",

        75 => "images/sprFireShell.png",

        76 => "images/sprFireShell.png",

        77 => "images/sprFireShell.png",

        78 => "images/sprClusterNade.png",

        79 => "images/sprMininade.png",

        80 => "images/sprMininade.png",

        81 => "images/sprBullet1.png",

        82 => "images/sprConfettiBall.png",

        83 => "images/sprBullet1.png",

        84 => "images/sprRocket.png",

        85 => "images/sprMininade.png",

        86 => "images/sprUltraBullet.png",

        87 => "images/sprLaser.png",

        88 => "images/sprSlash.png",

        89 => "images/sprHeavyBullet.png",

        90 => "images/sprHeavyBullet.png",

        91 => "images/sprHeavySlug.png",

        92 => "images/sprUltraSlash.png",

        93 => "images/sprUltraShell.png",

        94 => "images/sprUltraBolt.png",

        95 => "images/sprUltraGrenade.png",

        96 => "images/sprPlasmaBall.png",

        97 => "images/sprPlasmaBall.png",

        98 => "images/sprPlasmaBall.png",

        99 => "images/sprSlugBullet.png",

        100 => "images/sprSplinter.png",

        101 => "images/sprShank.png",

        102 => "images/sprGoldRocket.png",

        103 => "images/sprBullet1.png",

        104 => "images/sprDisc.png",

        105 => "images/sprHeavyBolt.png",

        106 => "images/sprHeavyBullet.png",

        107 => "images/sprBloodBall.png",

        108 => "images/sprSlash.png",

        109 => "images/sprRocket.png",

        110 => "images/sprFireShell.png",

        111 => "images/sprPlasmaBallHuge.png",

        112 => "images/sprSeeker.png",

        113 => "images/sprSeeker.png",

        114 => "images/sprBullet2.png",

        115 => "images/sprSlash.png",

        116 => "images/sprBouncerBullet.png",

        117 => "images/sprBouncerBullet.png",

        118 => "images/sprSlugBullet.png",

        119 => "images/sprRocket.png",

        120 => "images/sprScorpionBullet.png",

        121 => "images/sprSlash.png",

        122 => "images/sprGoldNuke.png",

        123 => "images/sprGoldDisc.png",

        124 => "images/sprHeavyNade.png",

        125 => "images/sprBullet1.png",

        126 => "images/sprBullet1.png",

        127 => "images/sprScorpionBullet.png",

        128 => "images/sprSlash.png",
        _ => {

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

    let mut sprite = crate::game::content::sprite_exact(catalog, asset_server, path);
    sprite.color = Color::WHITE;
    sprite.custom_size = custom_size;

    if let Some(m) = catalog.anims.get(path) {
        let frames = m[0] as usize;
        if frames == 2 {
            let w = m[1].max(1.0);
            let h = m[2].max(1.0);
            sprite.rect = Some(Rect::new(w, 0.0, w * 2.0, h));
        }
    }
    sprite
}

pub fn projectile_anim(
    catalog: &AssetCatalog,
    path: &'static str,
) -> Option<crate::game::anim::SpriteAnim> {
    let def = catalog.anim_def(path)?;
    if def.frames <= 2 {
        return None;
    }
    let mut anim = crate::game::anim::SpriteAnim::new(path, def);
    if anim.def.fps <= 0.0 {
        anim.def.fps = 12.0;
        anim.timer = Timer::from_seconds(1.0 / 12.0, TimerMode::Repeating);
    }
    Some(anim)
}

pub fn sprite_and_anim_from_projectile_path(
    asset_server: &AssetServer,
    catalog: &AssetCatalog,
    candidates: &[&'static str],
    custom_size: Option<Vec2>,
) -> (Sprite, Option<crate::game::anim::SpriteAnim>, &'static str) {
    let path = first_existing(catalog, candidates);
    let sprite = sprite_from_projectile_path(asset_server, catalog, candidates, custom_size);
    let anim = projectile_anim(catalog, path);
    (sprite, anim, path)
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

pub fn player_projectile_candidates(id: WeaponId) -> Vec<&'static str> {
    let primary = player_projectile_path(id);
    let mut out = vec![primary];

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
