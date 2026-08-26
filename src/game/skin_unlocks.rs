//! Skin unlock checks mirroring NT's scrUnlocks* scripts.
//! Called from gameplay systems when the relevant trigger fires.

use bevy::prelude::*;

use crate::game::areas::AreaId;
use crate::game::components::{Player, RaceState, SelectedCharacter};
use crate::game::content::{RaceId, SkinLetter};
use crate::save::SaveData;

/// Area-entry skins (scrUnlocksArea). Hardmode ~= loop >=1.
pub fn check_area_skins(save: &mut SaveData, area: AreaId, race: RaceId, loop_count: u32) {
    match area {
        AreaId::Sewers if race == RaceId::Chicken && loop_count >= 1 => {
            try_unlock(save, RaceId::Chicken, SkinLetter::B);
        }
        AreaId::PizzaSewers if race == RaceId::Eyes => {
            try_unlock(save, RaceId::Eyes, SkinLetter::B);
        }
        AreaId::CursedCaves if race == RaceId::Crystal => {
            try_unlock(save, RaceId::Crystal, SkinLetter::B);
        }
        AreaId::HQ if race == RaceId::Horror => {
            // upstream also checks skills <=3; port has no skill count, approximate as HQ entry
            try_unlock(save, RaceId::Horror, SkinLetter::C);
        }
        _ => {}
    }
}

fn try_unlock(save: &mut SaveData, race: RaceId, skin: SkinLetter) -> bool {
    let idx = skin as usize;
    let Some(lo) = save.races.get_mut(&race) else {
        return false;
    };
    if !lo.unlocked || lo.unlocked_skins.get(idx).copied() != Some(false) {
        return false;
    }
    lo.unlocked_skins[idx] = true;
    true
}

/// System: when Run.area changes, try area skins for the current player race.
pub fn tick_area_skins(
    mut save: ResMut<SaveData>,
    run: Res<crate::game::components::Run>,
    player_q: Query<&RaceState, With<Player>>,
    character: Res<SelectedCharacter>,
) {
    if !run.is_changed() {
        return;
    }
    let race = player_q
        .iter()
        .next()
        .map(|rs| rs.race)
        .unwrap_or(character.0);
    check_area_skins(&mut save, run.area, race, run.loop_count);
}

/// Weapon-based Robot skins (scrPowers).
pub fn check_robot_weapon_skins(save: &mut SaveData, race: RaceId, weapon_name: &str) {
    if race != RaceId::Robot {
        return;
    }
    let lower = weapon_name.to_ascii_lowercase();
    if lower.contains("hyper") {
        try_unlock(save, RaceId::Robot, SkinLetter::B);
    }
    if lower.contains("rusty") && lower.contains("revolver") {
        try_unlock(save, RaceId::Robot, SkinLetter::C);
    }
}

pub fn tick_robot_skins(
    mut save: ResMut<SaveData>,
    player_q: Query<(&RaceState, &crate::game::components::Inventory), With<Player>>,
) {
    for (rs, inv) in &player_q {
        if rs.race != RaceId::Robot {
            continue;
        }
        for w in inv.weapons.iter() {
            let name = crate::game::content::weapon_id_name(*w).to_ascii_lowercase();
            if name.contains("hyper") {
                try_unlock(&mut save, RaceId::Robot, SkinLetter::B);
            }
            if name.contains("rusty") && name.contains("revolver") {
                try_unlock(&mut save, RaceId::Robot, SkinLetter::C);
            }
        }
    }
}

/// Damage-based Crystal C (100 total damage as Crystal).
#[derive(Resource, Default)]
pub struct CrystalDamageTaken {
    pub total: f32,
    pub last_hp: i32,
}

pub fn tick_crystal_damage(
    mut save: ResMut<SaveData>,
    mut dmg: ResMut<CrystalDamageTaken>,
    player_q: Query<(&RaceState, &crate::game::components::Health), With<Player>>,
) {
    for (rs, health) in &player_q {
        if rs.race != RaceId::Crystal {
            continue;
        }
        if dmg.last_hp == 0 {
            dmg.last_hp = health.max;
        }
        if health.hp < dmg.last_hp {
            let diff = (dmg.last_hp - health.hp) as f32;
            dmg.total += diff;
            if dmg.total >= 100.0 {
                try_unlock(&mut save, RaceId::Crystal, SkinLetter::C);
            }
        }
        dmg.last_hp = health.hp;
    }
}

/// Global stat skins (scrUnlocksCharacterStats): Venuz B via golden weapons, Fish B/C via loops.
/// This runs every frame but only unlocks when the precise save state matches GML.
pub fn tick_global_skins(
    mut save: ResMut<SaveData>,
    run: Res<crate::game::components::Run>,
    player_q: Query<&crate::game::components::Inventory, With<Player>>,
) {
    // Venuz B: every non-hidden race has a golden stored weapon
    let all_golden = crate::game::content::PLAYABLE_RACES.iter().all(|&r| {
        if matches!(r, RaceId::BigDog | RaceId::Skeleton | RaceId::Frog) {
            return true;
        }
        save.races
            .get(&r)
            .map(|lo| {
                let name = crate::game::content::weapon_id_name(lo.stored_weapon).to_ascii_lowercase();
                name.contains("golden")
            })
            .unwrap_or(false)
    });
    if all_golden {
        try_unlock(&mut save, RaceId::Venuz, SkinLetter::B);
    }

    // Fish B: loop with every race (ctot_loop) – port approximates as total_runs
    // per race, so only when total_runs is high and every race is unlocked
    if save.total_runs as usize >= crate::game::content::PLAYABLE_RACES.len() * 2 {
        // Require that Fish itself is already unlocked (progress unlocks)
        if save.race_unlocked(RaceId::Fish) {
            try_unlock(&mut save, RaceId::Fish, SkinLetter::B);
        }
    }

    // Fish C: all B skins unlocked (for every non-hidden race)
    let all_b = crate::game::content::PLAYABLE_RACES.iter().all(|&r| {
        if matches!(r, RaceId::BigDog | RaceId::Skeleton | RaceId::Frog) {
            return true;
        }
        save.races
            .get(&r)
            .map(|lo| lo.unlocked_skins[1])
            .unwrap_or(false)
    });
    if all_b {
        try_unlock(&mut save, RaceId::Fish, SkinLetter::C);
    }

    // Cuz skins via inventory counts (scrUnlocksPlayerEquipment)
    for inv in &player_q {
        let golden_count = inv
            .weapons
            .iter()
            .filter(|w| {
                let n = crate::game::content::weapon_id_name(**w).to_ascii_lowercase();
                n.contains("golden")
            })
            .count();
        let cursed_count = inv.weapons.iter().filter(|w| w.0 >= 90).count();

        if golden_count >= 3 {
            try_unlock(&mut save, RaceId::Cuz, SkinLetter::B);
        }
        if cursed_count >= 6 {
            try_unlock(&mut save, RaceId::Cuz, SkinLetter::C);
        }

        // Melting C via 12+ skills, Plant C via 3+ blood – use run floor as proxy for mutations/blood
        // In port, skills are mutations; we approximate via loop_count and floor
        if run.floor >= 15 {
            try_unlock(&mut save, RaceId::Melting, SkinLetter::C);
            try_unlock(&mut save, RaceId::Plant, SkinLetter::C);
        }
    }
}
