//! GENERATED FROM toarch7/nt-recreated-public@06a2e3e
//! Do not edit by hand.
//! Source: scrUnlocks.gml / scrLoadoutMenuInit.gml — handles July 17 2026 NTT skin unlock logic.

use crate::game::content::{RaceId, SkinLetter};
use crate::save::SaveData;

pub fn is_race_unlocked(save: &SaveData, race: RaceId) -> bool {
    if race == RaceId::Fish {
        return true;
    }
    save.races.get(&race).map(|r| r.unlocked).unwrap_or(false)
}

pub fn is_skin_unlocked(save: &SaveData, race: RaceId, skin: SkinLetter) -> bool {
    save.races
        .get(&race)
        .map(|r| r.unlocked_skins[skin as usize])
        .unwrap_or(false)
}

// TODO: port July 17 2026 hidden NTT skin unlockability (scrRaces + scrUnlocks + scrFire)
