use crate::game::content::{RaceId, SkinLetter, character_def};
use crate::save::SaveData;

pub fn is_race_unlocked(save: &SaveData, race: RaceId) -> bool {
    match race {
        RaceId::Fish | RaceId::Random => true,
        _ => save.race_unlocked(race),
    }
}

pub fn is_skin_unlocked(save: &SaveData, race: RaceId, skin: SkinLetter) -> bool {

    save.skin_unlocked(race, skin as u8)
}

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

    if floor >= 4 {
        award(save, RaceId::Crystal);
    }

    if floor >= 5 {
        award(save, RaceId::Eyes);
    }

    if died {
        award(save, RaceId::Melting);
    }

    if floor >= 7 {
        award(save, RaceId::Plant);
    }

    if floor >= 9 {
        award(save, RaceId::Venuz);
    }

    if floor >= 11 {
        award(save, RaceId::Chicken);
    }

    if floor >= 12 {
        award(save, RaceId::Steroids);
    }

    if ate_weapon {
        award(save, RaceId::Robot);
    }

    if floor >= 15 || cleared_throne {
        award(save, RaceId::Horror);
    }

    if loop_count >= 1 {
        award(save, RaceId::Rebel);
        award(save, RaceId::Rogue);
    }

    if loop_count >= 2 || cleared_throne {
        award(save, RaceId::Cuz);
    }

    got
}

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

        EnemyKind::BigDog | EnemyKind::BigDogLoop => {
            award(save, RaceId::BigDog);
        }
        EnemyKind::Mom => {
            award(save, RaceId::Frog);
        }
        _ => {}
    }

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

            if race == RaceId::Rogue {
                try_unlock_skin(save, RaceId::Rogue, 1);
            }
            if race == RaceId::Cuz {
                try_unlock_skin(save, RaceId::Venuz, 2);
            }
        }
        EnemyKind::Bandit if race == RaceId::Rebel => {
            try_unlock_skin(save, RaceId::Rebel, 2);
        }
        EnemyKind::LilHunter | EnemyKind::LilHunterLoop if race == RaceId::Rogue => {
            try_unlock_skin(save, RaceId::Rogue, 2);
        }
        EnemyKind::Throne | EnemyKind::ThroneII => {

            if race == RaceId::Melting {
                try_unlock_skin(save, RaceId::Melting, 1);
                try_unlock_skin(save, RaceId::Melting, 2);
            }
            if race == RaceId::Plant {
                try_unlock_skin(save, RaceId::Plant, 1);
                try_unlock_skin(save, RaceId::Plant, 2);
            }
            if race == RaceId::Eyes {
                try_unlock_skin(save, RaceId::Eyes, 2);
            }
            if race == RaceId::Steroids {
                try_unlock_skin(save, RaceId::Steroids, 2);
            }
        }
        _ => {}
    };

    got
}

pub fn check_area_skin_unlocks(
    save: &mut SaveData,
    area: crate::game::areas::AreaId,
    race: RaceId,
) -> bool {
    let mut any = false;
    match area {
        crate::game::areas::AreaId::Sewers => {

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

            if race == RaceId::Horror {
                any |= try_unlock_skin(save, RaceId::Horror, 2);
            }
        }
        _ => {}
    }
    any
}

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

        assert!(!is_skin_unlocked(&save, RaceId::Crystal, SkinLetter::B));
        assert!(!is_skin_unlocked(&save, RaceId::Crystal, SkinLetter::C));
    }

    #[test]
    fn frog_queen_grants_rebel_b_skin_only_as_rebel() {
        let mut save = SaveData::default();
        check_progress_unlocks(&mut save, 9, 0, false, false, false);
        try_unlock_race(&mut save, RaceId::Rebel);

        check_kill_unlocks(&mut save, EnemyKind::FrogQueen, RaceId::Rogue);
        assert!(!is_skin_unlocked(&save, RaceId::Rebel, SkinLetter::B));

        check_kill_unlocks(&mut save, EnemyKind::FrogQueen, RaceId::Rebel);
        assert!(is_skin_unlocked(&save, RaceId::Rebel, SkinLetter::B));
        assert!(!is_skin_unlocked(&save, RaceId::Rebel, SkinLetter::C));
    }
}
