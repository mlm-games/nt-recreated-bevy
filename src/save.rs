use std::collections::BTreeMap;

use bevy::prelude::*;
use game_utils::save::Versioned;
use serde::{Deserialize, Serialize};

use crate::game::components::RaceLoadout;
use crate::game::content::{RaceId, WeaponId, character_def};

pub const SAVE_VERSION: u32 = 3;

#[derive(Resource, Clone, Serialize, Deserialize)]
pub struct SaveData {
    #[serde(default)]
    pub version: u32,
    pub high_score: u32,
    #[serde(default)]
    pub best_floor: u32,
    #[serde(default)]
    pub total_runs: u32,
    #[serde(default)]
    pub total_kills: u32,
    #[serde(default)]
    pub unlocked_characters: Vec<String>,
    #[serde(default)]
    pub races: BTreeMap<RaceId, RaceLoadout>,
    /// Per-race crown unlocks (nt-rewrite UberCont.crowngot). Ids 0 (RANDOM)
    /// and 1 (NONE) default true; 2..=13 unlock by taking the crown in-run.
    #[serde(default)]
    pub crown_got: BTreeMap<RaceId, [bool; 14]>,
    #[serde(default)]
    pub achievements: BTreeMap<String, bool>,
    #[serde(default)]
    pub unlocked_cheats: bool,
    #[serde(default)]
    pub settings: SettingsData,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SettingsData {
    pub master_volume: f32,
    pub sfx_volume: f32,
    pub music_volume: f32,
    pub language: String,
}

impl Default for SettingsData {
    fn default() -> Self {
        Self {
            master_volume: 1.0,
            sfx_volume: 1.0,
            music_volume: 0.8,
            language: "en".to_string(),
        }
    }
}

impl Default for SaveData {
    fn default() -> Self {
        let mut races = BTreeMap::new();
        for &r in crate::game::content::PLAYABLE_RACES.iter() {
            races.insert(
                r,
                RaceLoadout {
                    unlocked: r == RaceId::Fish,
                    unlocked_skins: [true, true, false, false],
                    stored_weapon: WeaponId(0),
                    start_weapon: WeaponId(0),
                    start_crown: 0,
                },
            );
        }
        Self {
            version: SAVE_VERSION,
            high_score: 0,
            best_floor: 0,
            total_runs: 0,
            total_kills: 0,
            unlocked_characters: vec!["Fish".to_string()],
            races,
            crown_got: BTreeMap::new(),
            achievements: BTreeMap::new(),
            unlocked_cheats: false,
            settings: SettingsData::default(),
        }
    }
}

impl SaveData {
    pub fn race_unlocked(&self, race: RaceId) -> bool {
        if race == RaceId::Random {
            return true;
        }

        if let Some(lo) = self.races.get(&race)
            && lo.unlocked
        {
            return true;
        }

        let name = character_def(race).name;
        race == RaceId::Fish
            || self
                .unlocked_characters
                .iter()
                .any(|s| s.eq_ignore_ascii_case(name))
    }

    pub fn race_loadout_mut(&mut self, race: RaceId) -> &mut RaceLoadout {
        self.races.entry(race).or_insert_with(|| RaceLoadout {
            unlocked: race == RaceId::Fish,
            unlocked_skins: [true, true, false, false],
            stored_weapon: WeaponId(0),
            start_weapon: WeaponId(0),
            start_crown: 0,
        })
    }

    pub fn race_loadout(&self, race: RaceId) -> RaceLoadout {
        let mut lo = self.races.get(&race).cloned().unwrap_or(RaceLoadout {
            unlocked: self.race_unlocked(race),
            unlocked_skins: [true, true, false, false],
            stored_weapon: WeaponId(0),
            start_weapon: WeaponId(0),
            start_crown: 0,
        });

        // Stored weapon without an explicit start weapon is almost always
        // stale/debug data and causes an accidental two-gun run.
        if lo.start_weapon == WeaponId(0) {
            lo.stored_weapon = WeaponId(0);
        }

        lo
    }

    pub fn sanitize_loadouts(&mut self) {
        for lo in self.races.values_mut() {
            // A non-default starting weapon is valid only when it is the
            // weapon actually stored for this race.
            if lo.start_weapon.0 != 0 && lo.start_weapon != lo.stored_weapon {
                lo.start_weapon = WeaponId(0);
            }

            if lo.stored_weapon.0 == 0 {
                lo.start_weapon = WeaponId(0);
            }
        }
    }

    /// crowngot row with scrInit defaults: RANDOM(0) and NONE(1) always got.
    fn crown_row(&self, race: RaceId) -> [bool; 14] {
        const BASE: [bool; 14] = [
            true, true, false, false, false, false, false, false, false, false, false, false,
            false, false,
        ];
        self.crown_got.get(&race).copied().unwrap_or(BASE)
    }

    /// scr_loadout_race_is_crown_unlocked: cgot[race] && crowngot[race, id].
    pub fn crown_unlocked(&self, race: RaceId, crown: u8) -> bool {
        if crown as usize >= 14 || race == RaceId::Random {
            return false;
        }
        self.race_unlocked(race) && self.crown_row(race)[crown as usize]
    }

    /// scr_loadout_race_get_unlocked_crowns_count > 0 (ids above NONE only).
    pub fn any_crown_unlocked(&self, race: RaceId) -> bool {
        if race == RaceId::Random {
            return false;
        }
        let row = self.crown_row(race);
        (2..14).any(|i| row[i])
    }

    /// scr_loadout_race_unlock_crown + set_start_crown auto-equip.
    pub fn unlock_crown(&mut self, race: RaceId, crown: u8) {
        if (crown as usize) >= 14 || race == RaceId::Random || !self.race_unlocked(race) {
            return;
        }
        let row = self.crown_got.entry(race).or_insert({
            let mut r = [false; 14];
            r[0] = true;
            r[1] = true;
            r
        });
        if row[crown as usize] {
            return;
        }
        row[crown as usize] = true;
        // Loadout storage is port-id space (scr_loadout_race_set_start_crown
        // equivalent); crown ids here are GML crwn_* ids.
        self.race_loadout_mut(race).start_crown = crate::game::content::crown_gml_to_port(crown);
    }
}

impl Versioned for SaveData {
    fn version(&self) -> u32 {
        self.version
    }

    fn set_version(&mut self, version: u32) {
        self.version = version;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_settings_files_receive_defaults_for_new_fields() {
        let settings: SettingsData = serde_json::from_str(r#"{"master_volume":0.5}"#)
            .expect("partial settings should deserialize");

        assert_eq!(settings.master_volume, 0.5);
        assert_eq!(settings.sfx_volume, 1.0);
        assert_eq!(settings.music_volume, 0.8);
        assert_eq!(settings.language, "en");
    }

    /// scrInit defaults: RANDOM(0)/NONE(1) always got, real crowns locked.
    #[test]
    fn crown_defaults_match_scrInit() {
        let save = SaveData::default();
        assert!(save.crown_unlocked(RaceId::Fish, 0));
        assert!(save.crown_unlocked(RaceId::Fish, 1));
        assert!(!save.crown_unlocked(RaceId::Fish, 2));
        assert!(!save.any_crown_unlocked(RaceId::Fish));
    }

    /// Locked race denies everything (scr_loadout_race_is_crown_unlocked's
    /// cgot[race] gate).
    #[test]
    fn crown_unlock_requires_race_unlocked() {
        let mut save = SaveData::default();
        save.unlock_crown(RaceId::Rogue, crate::game::content::crown_port_to_gml(2));
        assert!(!save.crown_unlocked(RaceId::Rogue, crate::game::content::crown_port_to_gml(2)));
        assert!(!save.any_crown_unlocked(RaceId::Rogue));

        // Unlocking the race makes the same call succeed and auto-equip.
        save.race_loadout_mut(RaceId::Rogue).unlocked = true;
        let haste_gml = crate::game::content::crown_port_to_gml(3);
        save.unlock_crown(RaceId::Rogue, haste_gml);
        assert!(save.crown_unlocked(RaceId::Rogue, haste_gml));
        assert!(save.any_crown_unlocked(RaceId::Rogue));
        assert_eq!(save.race_loadout(RaceId::Rogue).start_crown, 3);
    }

    /// Old saves without the field get scrInit defaults via serde(default).
    #[test]
    fn legacy_saves_get_crown_defaults() {
        let json = serde_json::json!({
            "high_score": 7,
            "total_runs": 1,
            "total_kills": 2,
            "unlocked_characters": ["Fish"],
            "settings": {},
        });
        let save: SaveData = serde_json::from_value(json).expect("legacy save");
        assert!(save.crown_got.is_empty());
        assert!(save.crown_unlocked(RaceId::Fish, 1));
        assert!(!save.crown_unlocked(RaceId::Fish, 5));
    }
}
