use crate::game::content::{AmmoKind, MeleeDef, WeaponDef, weapon_def};
use crate::game::weapons_data::WEAPONS;

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

pub fn weapon_runtime(id: crate::game::content::WeaponId) -> WeaponRuntime {
    let id = crate::game::content::sanitize_weapon_id(id);

    if id == crate::game::content::WeaponId::NONE {
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

    let meta = &WEAPONS[id.0 as usize];
    // Map AmmoType to ProjectileKind
    let kind = match meta.wep_type as u8 {
        1 => ProjectileKind::Bullet,
        2 => ProjectileKind::Shell,
        3 => ProjectileKind::Bolt,
        4 => ProjectileKind::Explosive,
        5 => ProjectileKind::Energy,
        _ => ProjectileKind::Melee,
    };
    // For the 10 legacy weapons, delegate to weapon_def for full stats;
    // otherwise synthesize minimal runtime from metadata.
    let legacy: crate::game::content::WeaponKind = id.into();
    if legacy != crate::game::content::WeaponKind::None {
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
    // Generic fallback for non-legacy IDs: use metadata directly
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

pub fn weapon_runtime_def(id: crate::game::content::WeaponId) -> WeaponDef {
    let id = crate::game::content::sanitize_weapon_id(id);
    if id == crate::game::content::WeaponId::NONE {
        return weapon_def(crate::game::content::WeaponKind::None);
    }

    let rt = weapon_runtime(id);
    let meta = &WEAPONS[id.0 as usize];
    let ammo = match meta.wep_type as u8 {
        1 => AmmoKind::Bullets,
        2 => AmmoKind::Shells,
        3 => AmmoKind::Bolts,
        4 => AmmoKind::Explosives,
        5 => AmmoKind::Energy,
        _ => AmmoKind::Bullets,
    };
    WeaponDef {
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
    }
}
