use crate::game::content::{
    AmmoKind, HazardDef, HazardKind, MeleeDef, SplitDef, WeaponDef, WeaponId, WeaponKind,
    sanitize_weapon_id, weapon_def, weapon_meta,
};

#[derive(Clone, Copy, Debug)]
pub enum ProjectileKind {
    Bullet,
    Shell,
    Bolt,
    Explosive,
    Energy,
    Melee,
}

#[derive(Clone, Copy, Debug)]
pub struct ExplosionSpec {
    pub radius: f32,
    pub damage: i32,
}

#[derive(Clone, Copy, Debug)]
pub struct MeleeSpec {
    pub range: f32,
    pub arc: f32,
}

#[derive(Clone, Copy, Debug)]
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
    let kind = match meta.wep_type as u8 {
        1 => ProjectileKind::Bullet,
        2 => ProjectileKind::Shell,
        3 => ProjectileKind::Bolt,
        4 => ProjectileKind::Explosive,
        5 => ProjectileKind::Energy,
        _ => ProjectileKind::Melee,
    };

    let legacy: WeaponKind = id.into();
    if legacy != WeaponKind::None {
        let def = weapon_def(legacy);
        return WeaponRuntime {
            projectile_kind: if def.melee.is_some() {
                ProjectileKind::Melee
            } else {
                kind
            },
            pellets: def.pellets as u8,
            spread_deg: def.spread,
            speed: def.speed,
            lifetime_frames: (def.lifetime * 30.0) as u16,
            damage: def.damage,
            recoil: def.recoil,
            explosion: if def.explosive {
                Some(ExplosionSpec {
                    radius: 130.0,
                    damage: def.damage,
                })
            } else {
                None
            },
            melee: def.melee.map(|m| MeleeSpec {
                range: m.range,
                arc: m.arc,
            }),
            cooldown_frames: meta.wep_load,
            automatic: meta.wep_auto,
        };
    }

    WeaponRuntime {
        projectile_kind: kind,
        pellets: 1,
        spread_deg: 0.07,
        speed: 480.0,
        lifetime_frames: 30,
        damage: 3,
        recoil: 3.0,
        explosion: None,
        melee: if meta.wep_mele {
            Some(MeleeSpec {
                range: 70.0,
                arc: 2.2,
            })
        } else {
            None
        },
        cooldown_frames: meta.wep_load,
        automatic: meta.wep_auto,
    }
}

pub fn weapon_runtime_def(id: WeaponId) -> WeaponDef {
    let id = sanitize_weapon_id(id);
    if id == WeaponId::NONE {
        return weapon_def(WeaponKind::None);
    }

    let rt = weapon_runtime(id);
    let meta = weapon_meta(id);

    let ammo = match meta.wep_type as u8 {
        1 => AmmoKind::Bullets,
        2 => AmmoKind::Shells,
        3 => AmmoKind::Bolts,
        4 => AmmoKind::Explosives,
        5 => AmmoKind::Energy,
        _ => AmmoKind::Bullets,
    };

    let mut def = WeaponDef {
        name: meta.wep_name,
        ammo,
        ammo_cost: meta.wep_cost as i32,
        cooldown: rt.cooldown_frames as f32 / 30.0,
        damage: rt.damage,
        pellets: rt.pellets as usize,
        speed: rt.speed,
        lifetime: rt.lifetime_frames as f32 / 30.0,
        spread: rt.spread_deg,
        recoil: rt.recoil,
        shake: 0.08,
        projectile_radius: 4.0,
        knockback: 90.0,
        automatic: rt.automatic,
        explosive: rt.explosion.is_some(),
        burst_shots: 1,
        burst_interval: 0.0,
        melee: rt.melee.map(|m| MeleeDef {
            range: m.range,
            arc: m.arc,
        }),
        color: bevy::prelude::Color::srgb(0.9, 0.9, 0.9),
        size: bevy::prelude::Vec2::splat(12.0),
        muzzle_burst: 2,
        bounces: 0,
        pierce: 0,
        hazard: None,
        split: None,
    };

    match id.0 {
        // AUTO CROSSBOW
        11 => {
            def.damage = 6;
            def.speed = 560.0;
            def.lifetime = 1.1;
            def.spread = 0.05;
            def.recoil = 4.0;
            def.projectile_radius = 4.0;
            def.knockback = 100.0;
            def.color = bevy::prelude::Color::srgb(0.95, 0.85, 0.45);
            def.size = bevy::prelude::Vec2::new(14.0, 4.0);
            def.muzzle_burst = 2;
        }
        // SUPER CROSSBOW
        12 => {
            def.damage = 14;
            def.speed = 620.0;
            def.lifetime = 1.15;
            def.spread = 0.03;
            def.recoil = 8.0;
            def.projectile_radius = 5.0;
            def.knockback = 180.0;
            def.color = bevy::prelude::Color::srgb(1.0, 0.9, 0.55);
            def.size = bevy::prelude::Vec2::new(18.0, 5.0);
            def.muzzle_burst = 4;
        }
        // DISC GUN
        18 => {
            def.damage = 7;
            def.speed = 420.0;
            def.lifetime = 2.2;
            def.spread = 0.02;
            def.recoil = 3.0;
            def.projectile_radius = 8.0;
            def.knockback = 120.0;
            def.color = bevy::prelude::Color::srgb(0.7, 0.95, 1.0);
            def.size = bevy::prelude::Vec2::splat(14.0);
            def.muzzle_burst = 0;
            def.bounces = 6;
        }
        // HYPER RIFLE
        26 => {
            def.damage = 3;
            def.speed = 880.0;
            def.lifetime = 0.55;
            def.spread = 0.01;
            def.recoil = 2.0;
            def.projectile_radius = 3.0;
            def.knockback = 40.0;
            def.color = bevy::prelude::Color::srgb(1.0, 0.95, 0.6);
            def.size = bevy::prelude::Vec2::new(16.0, 3.0);
            def.muzzle_burst = 2;
        }
        // LASER MINIGUN
        28 => {
            def.damage = 2;
            def.speed = 760.0;
            def.lifetime = 0.5;
            def.spread = 0.1;
            def.recoil = 1.6;
            def.projectile_radius = 3.0;
            def.knockback = 30.0;
            def.color = bevy::prelude::Color::srgb(0.5, 1.0, 0.6);
            def.size = bevy::prelude::Vec2::new(12.0, 3.0);
            def.muzzle_burst = 1;
        }
        // FLAK CANNON
        38 => {
            def.damage = 8;
            def.speed = 340.0;
            def.lifetime = 0.65;
            def.spread = 0.04;
            def.recoil = 10.0;
            def.projectile_radius = 7.0;
            def.knockback = 200.0;
            def.explosive = true;
            def.color = bevy::prelude::Color::srgb(1.0, 0.72, 0.3);
            def.size = bevy::prelude::Vec2::splat(11.0);
            def.muzzle_burst = 5;
            def.split = Some(SplitDef {
                pellets: 6,
                spread: 0.55,
                speed: 420.0,
                damage: 3,
                lifetime: 0.32,
                radius: 3.0,
                knockback: 50.0,
                color: bevy::prelude::Color::srgb(1.0, 0.88, 0.55),
                size: bevy::prelude::Vec2::new(8.0, 3.0),
            });
        }
        // LIGHTNING PISTOL
        57 => {
            def.damage = 4;
            def.speed = 920.0;
            def.lifetime = 0.35;
            def.spread = 0.0;
            def.recoil = 2.5;
            def.projectile_radius = 4.0;
            def.knockback = 25.0;
            def.color = bevy::prelude::Color::srgb(0.75, 0.9, 1.0);
            def.size = bevy::prelude::Vec2::new(18.0, 4.0);
            def.muzzle_burst = 2;
            def.pierce = 1;
        }
        // LIGHTNING RIFLE
        58 => {
            def.damage = 6;
            def.speed = 980.0;
            def.lifetime = 0.42;
            def.spread = 0.0;
            def.recoil = 4.0;
            def.projectile_radius = 4.0;
            def.knockback = 35.0;
            def.color = bevy::prelude::Color::srgb(0.7, 0.95, 1.0);
            def.size = bevy::prelude::Vec2::new(22.0, 4.0);
            def.muzzle_burst = 3;
            def.pierce = 2;
        }
        // LIGHTNING SHOTGUN
        59 => {
            def.damage = 3;
            def.pellets = 7;
            def.speed = 860.0;
            def.lifetime = 0.26;
            def.spread = 0.3;
            def.recoil = 7.0;
            def.projectile_radius = 3.0;
            def.knockback = 20.0;
            def.color = bevy::prelude::Color::srgb(0.75, 0.95, 1.0);
            def.size = bevy::prelude::Vec2::new(14.0, 3.0);
            def.muzzle_burst = 4;
            def.pierce = 1;
        }
        // SUPER FLAK CANNON
        60 => {
            def.damage = 10;
            def.speed = 360.0;
            def.lifetime = 0.7;
            def.spread = 0.03;
            def.recoil = 12.0;
            def.projectile_radius = 8.0;
            def.knockback = 240.0;
            def.explosive = true;
            def.color = bevy::prelude::Color::srgb(1.0, 0.66, 0.22);
            def.size = bevy::prelude::Vec2::splat(12.0);
            def.muzzle_burst = 6;
            def.split = Some(SplitDef {
                pellets: 10,
                spread: 0.8,
                speed: 460.0,
                damage: 3,
                lifetime: 0.36,
                radius: 3.0,
                knockback: 55.0,
                color: bevy::prelude::Color::srgb(1.0, 0.9, 0.6),
                size: bevy::prelude::Vec2::new(8.0, 3.0),
            });
        }
        // TOXIC LAUNCHER
        72 => {
            def.damage = 7;
            def.speed = 340.0;
            def.lifetime = 0.7;
            def.spread = 0.03;
            def.recoil = 9.0;
            def.projectile_radius = 7.0;
            def.knockback = 170.0;
            def.explosive = true;
            def.color = bevy::prelude::Color::srgb(0.45, 0.9, 0.4);
            def.size = bevy::prelude::Vec2::splat(11.0);
            def.muzzle_burst = 4;
            def.hazard = Some(HazardDef {
                kind: HazardKind::Toxic,
                radius: 56.0,
                damage: 1,
                duration: 2.4,
                tick: 0.25,
                color: bevy::prelude::Color::srgba(0.35, 0.9, 0.35, 0.35),
            });
        }
        // FLAME CANNON
        73 => {
            def.damage = 9;
            def.speed = 300.0;
            def.lifetime = 0.45;
            def.spread = 0.14;
            def.recoil = 8.0;
            def.projectile_radius = 7.0;
            def.knockback = 110.0;
            def.color = bevy::prelude::Color::srgb(1.0, 0.5, 0.18);
            def.size = bevy::prelude::Vec2::splat(12.0);
            def.muzzle_burst = 6;
            def.hazard = Some(HazardDef {
                kind: HazardKind::Fire,
                radius: 48.0,
                damage: 1,
                duration: 1.1,
                tick: 0.15,
                color: bevy::prelude::Color::srgba(1.0, 0.45, 0.12, 0.32),
            });
        }
        // FLAMETHROWER
        74 => {
            def.damage = 2;
            def.pellets = 5;
            def.speed = 250.0;
            def.lifetime = 0.22;
            def.spread = 0.35;
            def.recoil = 1.2;
            def.projectile_radius = 5.0;
            def.knockback = 18.0;
            def.color = bevy::prelude::Color::srgb(1.0, 0.55, 0.15);
            def.size = bevy::prelude::Vec2::new(10.0, 6.0);
            def.muzzle_burst = 4;
            def.hazard = Some(HazardDef {
                kind: HazardKind::Fire,
                radius: 34.0,
                damage: 1,
                duration: 0.8,
                tick: 0.12,
                color: bevy::prelude::Color::srgba(1.0, 0.5, 0.15, 0.28),
            });
        }
        // DOUBLE FLAME SHOTGUN
        76 => {
            def.damage = 2;
            def.pellets = 12;
            def.speed = 300.0;
            def.lifetime = 0.24;
            def.spread = 0.5;
            def.recoil = 6.5;
            def.projectile_radius = 4.0;
            def.knockback = 25.0;
            def.color = bevy::prelude::Color::srgb(1.0, 0.55, 0.2);
            def.size = bevy::prelude::Vec2::new(9.0, 5.0);
            def.muzzle_burst = 6;
            def.hazard = Some(HazardDef {
                kind: HazardKind::Fire,
                radius: 34.0,
                damage: 1,
                duration: 0.9,
                tick: 0.12,
                color: bevy::prelude::Color::srgba(1.0, 0.5, 0.15, 0.28),
            });
        }
        // AUTO FLAME SHOTGUN
        77 => {
            def.damage = 2;
            def.pellets = 6;
            def.speed = 300.0;
            def.lifetime = 0.22;
            def.spread = 0.42;
            def.recoil = 3.5;
            def.projectile_radius = 4.0;
            def.knockback = 20.0;
            def.color = bevy::prelude::Color::srgb(1.0, 0.58, 0.2);
            def.size = bevy::prelude::Vec2::new(9.0, 5.0);
            def.muzzle_burst = 4;
            def.hazard = Some(HazardDef {
                kind: HazardKind::Fire,
                radius: 30.0,
                damage: 1,
                duration: 0.75,
                tick: 0.12,
                color: bevy::prelude::Color::srgba(1.0, 0.5, 0.15, 0.26),
            });
        }
        // SUPER DISC GUN
        104 => {
            def.damage = 10;
            def.speed = 460.0;
            def.lifetime = 2.8;
            def.spread = 0.01;
            def.recoil = 4.0;
            def.projectile_radius = 10.0;
            def.knockback = 180.0;
            def.color = bevy::prelude::Color::srgb(0.75, 1.0, 1.0);
            def.size = bevy::prelude::Vec2::splat(18.0);
            def.muzzle_burst = 0;
            def.bounces = 12;
        }
        // HEAVY AUTO CROSSBOW
        105 => {
            def.damage = 10;
            def.speed = 520.0;
            def.lifetime = 1.0;
            def.spread = 0.07;
            def.recoil = 6.5;
            def.projectile_radius = 5.0;
            def.knockback = 140.0;
            def.color = bevy::prelude::Color::srgb(1.0, 0.88, 0.5);
            def.size = bevy::prelude::Vec2::new(16.0, 5.0);
            def.muzzle_burst = 3;
        }
        // BLOOD CANNON
        107 => {
            def.damage = 10;
            def.speed = 320.0;
            def.lifetime = 0.62;
            def.spread = 0.08;
            def.recoil = 10.0;
            def.projectile_radius = 8.0;
            def.knockback = 220.0;
            def.explosive = true;
            def.color = bevy::prelude::Color::srgb(0.9, 0.18, 0.18);
            def.size = bevy::prelude::Vec2::splat(12.0);
            def.muzzle_burst = 6;
            def.split = Some(SplitDef {
                pellets: 6,
                spread: 0.7,
                speed: 380.0,
                damage: 2,
                lifetime: 0.34,
                radius: 3.0,
                knockback: 30.0,
                color: bevy::prelude::Color::srgb(1.0, 0.3, 0.3),
                size: bevy::prelude::Vec2::new(7.0, 3.0),
            });
        }
        _ => {}
    }

    def
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disc_gun_bounces() {
        let def = weapon_runtime_def(WeaponId(18));
        assert!(def.bounces > 0);
        assert_eq!(def.ammo, AmmoKind::Bolts);
    }

    #[test]
    fn lightning_rifle_pierces() {
        let def = weapon_runtime_def(WeaponId(58));
        assert!(def.pierce > 0);
        assert_eq!(def.ammo, AmmoKind::Energy);
    }

    #[test]
    fn toxic_launcher_spawns_hazard() {
        let def = weapon_runtime_def(WeaponId(72));
        assert!(def.hazard.is_some());
    }

    #[test]
    fn super_flak_splits() {
        let def = weapon_runtime_def(WeaponId(60));
        assert!(def.split.is_some());
    }
}
