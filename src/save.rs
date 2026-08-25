use std::collections::BTreeMap;

use bevy::prelude::*;
use game_utils::save::Versioned;
use serde::{Deserialize, Serialize};

use crate::game::components::RaceLoadout;
use crate::game::content::{RaceId, WeaponId, character_def};

pub const SAVE_VERSION: u32 = 2;

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
        self.races.get(&race).cloned().unwrap_or(RaceLoadout {
            unlocked: self.race_unlocked(race),
            unlocked_skins: [true, true, false, false],
            stored_weapon: WeaponId(0),
            start_weapon: WeaponId(0),
            start_crown: 0,
        })
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
}
