use crate::game::content::{AmmoKind, EnemyKind, WeaponId, weapon_ammo, weapon_meta};
use crate::game::weapon_runtime::{weapon_family, WeaponFamily};

/// Resolve the best projectile sprite for a player weapon.
/// Returns an asset path like "images/sprBullet1.png".
pub fn player_projectile_path(id: WeaponId) -> &'static str {
    let meta = weapon_meta(crate::game::content::sanitize_weapon_id(id));
    let name = meta.wep_name.to_ascii_uppercase();
    // Strip prefixes for variant handling is done inside family mapping,
    // but we also check raw name for DISC/BOUNCER etc.
    let fam = weapon_family(id);

    if name.contains("DISC") {
        return "images/sprDisc.png";
    }
    if name.contains("BOUNCER") {
        return "images/sprBouncerBullet.png";
    }
    if name.contains("BOLT") || name.contains("CROSSBOW") || name.contains("SPLINTER") {
        return "images/sprBolt.png";
    }
    if name.contains("FLAK") {
        return "images/sprFlakBullet.png";
    }
    if name.contains("ROCKET") || name.contains("BAZOOKA") || name.contains("NUKE") {
        return "images/sprRocket.png";
    }
    if name.contains("GRENADE") || name.contains("NAD") {
        // sticky / blood / cluster variants have their own sprites but fall back to grenade
        if name.contains("STICKY") {
            return "images/sprStickyGrenade.png";
        }
        if name.contains("BLOOD") {
            return "images/sprBloodGrenade.png";
        }
        return "images/sprGrenade.png";
    }
    if name.contains("PLASMA") || name.contains("DEVASTATOR") {
        return "images/sprPlasmaBall.png";
    }
    if name.contains("LASER") {
        return "images/sprLaser.png";
    }
    if matches!(fam, WeaponFamily::Flame) {
        return "images/sprFlameBall.png";
    }
    if name.contains("LIGHTNING") {
        return "images/sprEnemyLightning.png";
    }
    // Fallback by ammo family
    match weapon_ammo(id) {
        AmmoKind::Shells => "images/sprBullet2.png",
        AmmoKind::Bolts => "images/sprBolt.png",
        AmmoKind::Explosives => "images/sprGrenade.png",
        AmmoKind::Energy => "images/sprPlasmaBall.png",
        _ => "images/sprBullet1.png",
    }
}

pub fn enemy_projectile_path(kind: EnemyKind) -> &'static str {
    match kind {
        EnemyKind::Scorpion | EnemyKind::GoldScorpion => "images/sprScorpionBullet.png",
        EnemyKind::Jock => "images/sprJockRocket.png",
        EnemyKind::SnowTank => "images/sprRocket.png",
        EnemyKind::GoldSnowtank => "images/sprGoldTankRocket.png",
        EnemyKind::Guardian => "images/sprGuardianBullet.png",
        EnemyKind::ExploGuardian => "images/sprExploGuardianBullet.png",
        EnemyKind::DogGuardian => "images/sprHeavyBullet.png",
        EnemyKind::Crystal | EnemyKind::LaserCrystal | EnemyKind::InvLaserCrystal | EnemyKind::LightningCrystal => {
            "images/sprGuardianBullet.png"
        }
        EnemyKind::FireBaller | EnemyKind::SuperFireBaller => "images/sprFlameBall.png",
        EnemyKind::Turtle => "images/sprGuardianBullet.png",
        EnemyKind::Sniper => "images/sprBullet1.png",
        EnemyKind::Bandit | EnemyKind::JungleBandit | EnemyKind::SnowBandit | EnemyKind::MeleeBandit => {
            "images/sprEnemyBullet1.png"
        }
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
        EnemyKind::IdpdGrunt | EnemyKind::IdpdShield | EnemyKind::IdpdElite | EnemyKind::IdpdInspector => {
            "images/sprIDPDBullet.png"
        }
        EnemyKind::Maggot | EnemyKind::BigMaggot | EnemyKind::MaggotSpawn => "images/sprMaggotBullet.png",
        // Fallback
        _ => "images/sprEnemyBullet1.png",
    }
}

/// Pick first path that exists in catalog, fallback to first.
pub fn first_existing<'a>(catalog: &crate::game::content::AssetCatalog, paths: &[&'a str]) -> &'a str {
    for p in paths {
        if catalog.has(p) {
            return p;
        }
    }
    paths.first().copied().unwrap_or("images/sprBullet1.png")
}

/// Helper to resolve ordered candidates for a weapon.
pub fn player_projectile_candidates(id: WeaponId) -> Vec<&'static str> {
    let primary = player_projectile_path(id);
    let mut out = vec![primary];
    // Add ammo family fallbacks
    match weapon_ammo(id) {
        AmmoKind::Shells => out.push("images/sprBullet2.png"),
        AmmoKind::Bolts => out.push("images/sprBolt.png"),
        AmmoKind::Explosives => out.push("images/sprGrenade.png"),
        AmmoKind::Energy => out.push("images/sprPlasmaBall.png"),
        _ => out.push("images/sprBullet1.png"),
    }
    out.push("images/sprBullet1.png");
    out
}
