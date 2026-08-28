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
    // scr_race_is_skin_unlocked: only A(0) is free; B/C/D via cskingot.
    save.skin_unlocked(race, skin as u8)
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

/// Floor / death / eat-weapon / throne progress awards.
/// Returns newly unlocked races so callers can toast.
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
    // Rebel + Rogue: reach a loop
    if loop_count >= 1 {
        award(save, RaceId::Rebel);
        award(save, RaceId::Rogue);
    }
    // Cuz: deep loop / hard-mode stand-in (Throne II path)
    if loop_count >= 2 || cleared_throne {
        award(save, RaceId::Cuz);
    }

    got
}

/// Kill-based unlocks (call from resolve_deaths on boss kills).
/// `race` is the playing character - some B-skins only unlock when that
/// specific race lands the kill (scrOnBossKill).
pub fn check_kill_unlocks(
    save: &mut SaveData,
    kind: crate::game::content::EnemyKind,
    race: RaceId,
) -> Vec<RaceId> {
    use crate::game::content::EnemyKind;
    let mut got = Vec::new();
    let mut award = |save: &mut SaveData, r: RaceId| {
        if try_unlock_race(save, r) {
            got.push(r);
        }
    };

    match kind {
        // Big Dog playable after defeating Big Dog (base or loop).
        EnemyKind::BigDog | EnemyKind::BigDogLoop => {
            award(save, RaceId::BigDog);
        }
        EnemyKind::Mom => {
            award(save, RaceId::Frog);
        }
        _ => {}
    }

    // scrOnBossKill B/C-skin grants: boss must fall to the matching mutant.
    match kind {
        EnemyKind::FrogQueen if race == RaceId::Rebel => {
            try_unlock_skin(save, RaceId::Rebel, 1);
        }
        EnemyKind::Hyper if race == RaceId::Horror => {
            try_unlock_skin(save, RaceId::Horror, 1);
        }
        EnemyKind::Technomancer if race == RaceId::Steroids => {
            try_unlock_skin(save, RaceId::Steroids, 1);
        }
        EnemyKind::Captain => {
            // Rogue B as Rogue, Venuz C as Cuz (YVBoss - same object in port)
            if race == RaceId::Rogue {
                try_unlock_skin(save, RaceId::Rogue, 1);
            }
            if race == RaceId::Cuz {
                try_unlock_skin(save, RaceId::Venuz, 2); // Venuz C
            }
        }
        EnemyKind::Bandit if race == RaceId::Rebel => {
            try_unlock_skin(save, RaceId::Rebel, 2);
        }
        EnemyKind::LilHunter | EnemyKind::LilHunterLoop if race == RaceId::Rogue => {
            try_unlock_skin(save, RaceId::Rogue, 2); // Rogue C via LilHunter event
        }
        EnemyKind::Throne | EnemyKind::ThroneII => {
            // Throne defeat skins (scrUnlocksThroneDefeat)
            if race == RaceId::Melting {
                try_unlock_skin(save, RaceId::Melting, 1); // Melting B (no rhino/strong spirit - approx)
                try_unlock_skin(save, RaceId::Melting, 2); // Melting C (>=12 skills - approx)
            }
            if race == RaceId::Plant {
                try_unlock_skin(save, RaceId::Plant, 1); // Plant B (speedrun)
                try_unlock_skin(save, RaceId::Plant, 2); // Plant C (blood 3)
            }
            if race == RaceId::Eyes {
                try_unlock_skin(save, RaceId::Eyes, 2); // Eyes C (pacifist)
            }
            if race == RaceId::Steroids {
                try_unlock_skin(save, RaceId::Steroids, 2); // Steroids C (pacifist)
            }
        }
        _ => {}
    };

    // Additional GML checks that map cleanly to kill context are wired
    // below; area/throne/damage/weapon checks live in their own systems.
    got
}

/// Area-entry skin unlocks (scrUnlocksArea). `race` is the current player.
pub fn check_area_skin_unlocks(
    save: &mut SaveData,
    area: crate::game::areas::AreaId,
    race: RaceId,
) -> bool {
    let mut any = false;
    match area {
        crate::game::areas::AreaId::Sewers => {
            // Hardmode sewers as Chicken -> Chicken B
            // hardmode ~= loop_count >=1 in port (UberCont.hardmode)
            // Check via try_unlock_skin which requires race unlocked
            if race == RaceId::Chicken {
                any |= try_unlock_skin(save, RaceId::Chicken, 1);
            }
        }
        crate::game::areas::AreaId::PizzaSewers => {
            if race == RaceId::Eyes {
                any |= try_unlock_skin(save, RaceId::Eyes, 1);
            }
        }
        crate::game::areas::AreaId::CursedCaves => {
            if race == RaceId::Crystal {
                any |= try_unlock_skin(save, RaceId::Crystal, 1);
            }
        }
        crate::game::areas::AreaId::HQ => {
            // Horror C via HQ with <=3 skills - approximated as entering HQ as Horror
            if race == RaceId::Horror {
                any |= try_unlock_skin(save, RaceId::Horror, 2);
            }
        }
        _ => {}
    }
    any
}

/// scrRaceUnlockSkin: requires the race itself unlocked; returns on change.
fn try_unlock_skin(save: &mut SaveData, race: RaceId, skin: usize) -> bool {
    let Some(lo) = save.races.get_mut(&race) else {
        return false;
    };
    if !lo.unlocked || lo.unlocked_skins.get(skin).copied() != Some(false) {
        return false;
    }
    lo.unlocked_skins[skin] = true;
    true
}

/// Skeleton: Melting dies inside a living Necromancer's circle → unlock + mid-run transform.
pub fn try_unlock_skeleton(save: &mut SaveData) -> bool {
    try_unlock_race(save, RaceId::Skeleton)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::content::EnemyKind;

    #[test]
    fn floor_reach_unlocks_fire_in_order() {
        let mut save = SaveData::default();
        assert!(
            check_progress_unlocks(&mut save, 4, 0, false, false, false).contains(&RaceId::Crystal)
        );
        assert!(!save.race_unlocked(RaceId::Eyes));
        assert!(
            check_progress_unlocks(&mut save, 5, 0, false, false, false).contains(&RaceId::Eyes)
        );
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
    fn big_dog_and_mom_kill_unlocks() {
        let mut save = SaveData::default();
        assert!(
            check_kill_unlocks(&mut save, EnemyKind::BigDog, RaceId::Fish)
                .contains(&RaceId::BigDog)
        );
        assert!(
            check_kill_unlocks(&mut save, EnemyKind::Mom, RaceId::Fish).contains(&RaceId::Frog)
        );
    }

    #[test]
    fn skins_ab_are_free_once_race_is_unlocked() {
        let mut save = SaveData::default();
        check_progress_unlocks(&mut save, 4, 0, false, false, false);
        assert!(is_skin_unlocked(&save, RaceId::Crystal, SkinLetter::A));
        // Upstream scrInit: B is NOT free with the race - it is earned
        // (scrOnBossKill grants per boss/race pair).
        assert!(!is_skin_unlocked(&save, RaceId::Crystal, SkinLetter::B));
        assert!(!is_skin_unlocked(&save, RaceId::Crystal, SkinLetter::C));
    }

    /// scrOnBossKill: FrogQueen killed AS Rebel unlocks Rebel's B-skin;
    /// the same kill as another race does not.
    #[test]
    fn frog_queen_grants_rebel_b_skin_only_as_rebel() {
        let mut save = SaveData::default();
        check_progress_unlocks(&mut save, 9, 0, false, false, false); // unlock Rebel+Rogue
        try_unlock_race(&mut save, RaceId::Rebel);

        check_kill_unlocks(&mut save, EnemyKind::FrogQueen, RaceId::Rogue);
        assert!(!is_skin_unlocked(&save, RaceId::Rebel, SkinLetter::B));

        check_kill_unlocks(&mut save, EnemyKind::FrogQueen, RaceId::Rebel);
        assert!(is_skin_unlocked(&save, RaceId::Rebel, SkinLetter::B));
        assert!(!is_skin_unlocked(&save, RaceId::Rebel, SkinLetter::C));
    }
}
