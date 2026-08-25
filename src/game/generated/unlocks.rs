//! GENERATED FROM toarch7/nt-recreated-public@06a2e3e
//! Do not edit by hand.
//! Unlock table + award hooks (canonical NT conditions).
use crate::game::content::{RaceId, SkinLetter, character_def};
use crate::save::SaveData;

pub fn is_race_unlocked(save: &SaveData, race: RaceId) -> bool {
    match race {
        RaceId::Fish | RaceId::Random => true,
        _ => save.race_unlocked(race),
    }
}

pub fn is_skin_unlocked(save: &SaveData, race: RaceId, skin: SkinLetter) -> bool {
    if skin as u8 <= 1 {
        // A/B skins come free once the race itself is unlocked.
        return is_race_unlocked(save, race);
    }
    save.races
        .get(&race)
        .map(|r| r.unlocked_skins[skin as usize])
        .unwrap_or(false)
}

/// Marks a race unlocked in the save; returns true when this changed state.
pub fn try_unlock_race(save: &mut SaveData, race: RaceId) -> bool {
    if is_race_unlocked(save, race) {
        return false;
    }
    let name = character_def(race).name.to_string();
    save.race_loadout_mut(race).unlocked = true;
    if !save
        .unlocked_characters
        .iter()
        .any(|s| s.eq_ignore_ascii_case(&name))
    {
        save.unlocked_characters.push(name);
    }
    true
}

/// Canonical NT unlock awards; call on floor enter / death / special events.
/// Returns the races newly unlocked by this event so callers can toast.
pub fn check_progress_unlocks(
    save: &mut SaveData,
    floor: u32,
    loop_count: u32,
    died: bool,
    ate_weapon: bool,
    cleared_throne: bool,
) -> Vec<RaceId> {
    let mut got = Vec::new();
    let mut award = |save: &mut SaveData, r: RaceId| {
        if try_unlock_race(save, r) {
            got.push(r);
        }
    };

    // Crystal: reach 2-1 (global floor 4)
    if floor >= 4 {
        award(save, RaceId::Crystal);
    }
    // Eyes: reach 3-1 (floor 5)
    if floor >= 5 {
        award(save, RaceId::Eyes);
    }
    // Melting: die once
    if died {
        award(save, RaceId::Melting);
    }
    // Plant: reach 3-3 (floor 7)
    if floor >= 7 {
        award(save, RaceId::Plant);
    }
    // Y.V.: reach 5-1 (floor 9)
    if floor >= 9 {
        award(save, RaceId::Venuz);
    }
    // Chicken: reach 5-3 (floor 11)
    if floor >= 11 {
        award(save, RaceId::Chicken);
    }
    // Steroids: reach 6-1 (floor 12)
    if floor >= 12 {
        award(save, RaceId::Steroids);
    }
    // Robot: eat a weapon
    if ate_weapon {
        award(save, RaceId::Robot);
    }
    // Horror: reach 7-3 / the Throne (floor 15)
    if floor >= 15 || cleared_throne {
        award(save, RaceId::Horror);
    }
    // Rebel + Rogue: reach a loop (Rogue's full condition is an IDPD loop
    // escape; reaching any loop is the interim proxy).
    if loop_count >= 1 {
        award(save, RaceId::Rebel);
        award(save, RaceId::Rogue);
    }
    // Skeleton / Frog / BigDog / Cuz stay achievement-gated for later.

    got
}

// Bespoke unlock hooks (call sites outside this helper):
// - Robot eat-weapon path passes `ate_weapon: true`
// - Throne II death passes `cleared_throne: true`

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn floor_reach_unlocks_fire_in_order() {
        let mut save = SaveData::default();

        assert!(check_progress_unlocks(&mut save, 4, 0, false, false, false).contains(&RaceId::Crystal));
        assert!(!save.race_unlocked(RaceId::Eyes));
        assert!(check_progress_unlocks(&mut save, 5, 0, false, false, false).contains(&RaceId::Eyes));

        // Reaching the same floor again awards nothing new.
        assert!(check_progress_unlocks(&mut save, 5, 0, false, false, false).is_empty());
    }

    #[test]
    fn death_and_eat_weapon_awards() {
        let mut save = SaveData::default();
        assert!(
            check_progress_unlocks(&mut save, 1, 0, true, false, false).contains(&RaceId::Melting)
        );
        assert!(
            check_progress_unlocks(&mut save, 1, 0, false, true, false).contains(&RaceId::Robot)
        );
    }

    #[test]
    fn loop_reach_awards_rebel_and_rogue() {
        let mut save = SaveData::default();
        let got = check_progress_unlocks(&mut save, 16, 1, false, false, false);
        assert!(got.contains(&RaceId::Rebel));
        assert!(got.contains(&RaceId::Rogue));
    }

    #[test]
    fn skins_ab_are_free_once_race_is_unlocked() {
        let mut save = SaveData::default();
        check_progress_unlocks(&mut save, 4, 0, false, false, false);
        assert!(is_skin_unlocked(&save, RaceId::Crystal, SkinLetter::A));
        assert!(is_skin_unlocked(&save, RaceId::Crystal, SkinLetter::B));
        // C/D stay locked until their own conditions fire.
        assert!(!is_skin_unlocked(&save, RaceId::Crystal, SkinLetter::C));
    }
}
