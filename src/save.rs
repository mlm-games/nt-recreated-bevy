use std::collections::BTreeMap;

use bevy::prelude::*;
use game_utils::save::Versioned;
use serde::{Deserialize, Serialize};

use crate::game::components::RaceLoadout;
use crate::game::content::{RaceId, WeaponId};

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
    pub settings: SettingsData,
}

#[derive(Clone, Serialize, Deserialize)]
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
        // Fish unlocked by default; others locked until achievements — keep old unlocked_characters compat
        races.insert(
            RaceId::Fish,
            RaceLoadout {
                unlocked: true,
                unlocked_skins: [true, false, false, false],
                stored_weapon: WeaponId(0),
                start_weapon: WeaponId(0),
                start_crown: 0,
            },
        );
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
    pub fn race_loadout_mut(&mut self, race: RaceId) -> &mut RaceLoadout {
        self.races.entry(race).or_insert_with(|| RaceLoadout {
            unlocked: race == RaceId::Fish,
            unlocked_skins: [race == RaceId::Fish, false, false, false],
            stored_weapon: WeaponId(0),
            start_weapon: WeaponId(0),
            start_crown: 0,
        })
    }

    pub fn race_loadout(&self, race: RaceId) -> RaceLoadout {
        self.races
            .get(&race)
            .cloned()
            .unwrap_or(RaceLoadout {
                unlocked: race == RaceId::Fish,
                unlocked_skins: [race == RaceId::Fish, false, false, false],
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
