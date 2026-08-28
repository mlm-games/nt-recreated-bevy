//! Loadout menu - renders per-race stored weapon / crown / skin selection
//! from SaveData::races. Currently stubbed; upstream logic in scrLoadoutMenuInit.

use crate::game::components::RaceLoadout;
use crate::game::content::{RaceId, SkinLetter, WeaponId};
use crate::save::SaveData;

pub struct LoadoutViewModel {
    pub races: Vec<RaceCardVm>,
    pub selected_race: RaceId,
    pub selected_skin: SkinLetter,
    pub start_weapon: WeaponId,
    pub stored_weapon: WeaponId,
    pub crown: u8,
}

pub struct RaceCardVm {
    pub race: RaceId,
    pub loadout: RaceLoadout,
    pub name: String,
}

pub fn build_loadout_vm(save: &SaveData, selected: RaceId) -> LoadoutViewModel {
    let races = crate::game::content::PLAYABLE_RACES
        .iter()
        .map(|&r| RaceCardVm {
            race: r,
            loadout: save.race_loadout(r),
            name: crate::game::content::character_def(r).name.to_string(),
        })
        .collect();
    let lo = save.race_loadout(selected);
    LoadoutViewModel {
        races,
        selected_race: selected,
        selected_skin: SkinLetter::A,
        start_weapon: lo.start_weapon,
        stored_weapon: lo.stored_weapon,
        crown: lo.start_crown,
    }
}

pub fn loadout_summary(save: &SaveData, race: RaceId) -> String {
    let lo = save.race_loadout(race);
    let def = crate::game::content::character_def(race);
    format!(
        "{} | start {} | stored {} | crown {} | ability {}",
        def.name,
        crate::game::content::weapon_id_name(lo.start_weapon),
        crate::game::content::weapon_id_name(lo.stored_weapon),
        lo.start_crown,
        crate::game::content::ability_name(def.ability),
    )
}
