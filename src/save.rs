use std::collections::BTreeMap;

use bevy::prelude::*;
use game_utils::save::Versioned;
use serde::{Deserialize, Serialize};

use crate::game::components::RaceLoadout;
use crate::game::content::{RaceId, WeaponId, character_def};

pub const SAVE_VERSION: u32 = 4;

#[derive(Resource, Clone, Serialize, Deserialize)]
pub struct SaveData {
    // Old saves fill defaults via serde.
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

    // Crowns 0-1 start open, rest unlock in-run.
    #[serde(default)]
    pub crown_got: BTreeMap<RaceId, [bool; 14]>,
    #[serde(default)]
    pub achievements: BTreeMap<String, bool>,
    #[serde(default)]
    pub unlocked_cheats: bool,
    #[serde(default)]
    pub settings: SettingsData,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(default)]
pub struct SettingsData {
    pub master_volume: f32,
    pub sfx_volume: f32,
    pub music_volume: f32,
    pub ambience_volume: f32,
    pub language: String,
    #[serde(default = "default_volume_3dsound")]
    pub volume_3dsound: bool,
    #[serde(default = "default_f")]
    pub screenshake: f32,
    #[serde(default = "default_f")]
    pub freezeframes: f32,
    #[serde(default)]
    pub bloom: bool,
    #[serde(default = "default_true")]
    pub particles: bool,
    #[serde(default = "default_true")]
    pub show_hud: bool,
    #[serde(default = "default_true")]
    pub show_timer: bool,
    #[serde(default = "default_true")]
    pub show_area: bool,
    #[serde(default = "default_true")]
    pub boss_intros: bool,
    #[serde(default = "default_true")]
    pub auto_pause: bool,
    #[serde(default = "default_true")]
    pub pause_button: bool,
    #[serde(default)]
    pub achievements_popup: bool,
    #[serde(default = "default_true")]
    pub vsync: bool,
    #[serde(default = "default_true")]
    pub fullscreen: bool,
    #[serde(default)]
    pub widescreen: bool,
    #[serde(default)]
    pub crosshair: u8,
    #[serde(default)]
    pub sideart: u8,
    #[serde(default = "default_pixel_mode")]
    pub pixel_mode: u8,
    #[serde(default)]
    pub gamepad_enabled: bool,
    #[serde(default)]
    pub gamepad_type: u8,
    #[serde(default)]
    pub aim_assist: bool,
    #[serde(default)]
    pub auto_aim: bool,
    #[serde(default)]
    pub volume_controls: bool,
    #[serde(default)]
    pub split_fire: bool,
    #[serde(default)]
    pub fixed_sight: bool,
    #[serde(default = "default_controls_scale")]
    pub controls_scale: f32,
    #[serde(default = "default_true")]
    pub show_tutorial: bool,
    #[serde(default)]
    pub player_color_hex: String,
    #[serde(default)]
    pub profile_name: String,
    #[serde(default)]
    pub cprefs_eyes: bool,
    #[serde(default)]
    pub cprefs_melting: bool,
    #[serde(default)]
    pub cprefs_plant: bool,
    #[serde(default)]
    pub cprefs_yv: bool,
    #[serde(default)]
    pub cprefs_steroids: bool,
    #[serde(default)]
    pub cprefs_horror: bool,
    #[serde(default)]
    pub cprefs_rogue: bool,
    #[serde(default)]
    pub cprefs_skeleton: bool,
}

fn default_volume_3dsound() -> bool {
    true
}
fn default_f() -> f32 {
    1.0
}
fn default_true() -> bool {
    true
}
fn default_pixel_mode() -> u8 {
    1
}
fn default_controls_scale() -> f32 {
    0.5
}

impl Default for SettingsData {
    fn default() -> Self {
        Self {
            master_volume: 1.0,
            sfx_volume: 1.0,
            music_volume: 0.8,
            ambience_volume: 1.0,
            language: "en".to_string(),
            volume_3dsound: true,
            screenshake: 1.0,
            freezeframes: 1.0,
            bloom: true,
            particles: true,
            show_hud: true,
            show_timer: false,
            show_area: true,
            boss_intros: true,
            auto_pause: true,
            pause_button: true,
            achievements_popup: true,
            vsync: false,
            fullscreen: true,
            widescreen: false,
            crosshair: 0,
            sideart: 0,
            pixel_mode: 1,
            gamepad_enabled: false,
            gamepad_type: 0,
            aim_assist: false,
            auto_aim: false,
            volume_controls: false,
            split_fire: false,
            fixed_sight: false,
            controls_scale: 0.5,
            show_tutorial: true,
            player_color_hex: String::new(),
            profile_name: String::new(),
            cprefs_eyes: true,
            cprefs_melting: true,
            cprefs_plant: false,
            cprefs_yv: true,
            cprefs_steroids: true,
            cprefs_horror: true,
            cprefs_rogue: true,
            cprefs_skeleton: false,
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
                    unlocked_skins: [true, false, false, false],
                    preferred_skin: 0,
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
            unlocked_skins: [true, false, false, false],
            preferred_skin: 0,
            stored_weapon: WeaponId(0),
            start_weapon: WeaponId(0),
            start_crown: 0,
        })
    }

    pub fn race_loadout(&self, race: RaceId) -> RaceLoadout {
        let mut lo = self.races.get(&race).cloned().unwrap_or(RaceLoadout {
            unlocked: self.race_unlocked(race),
            unlocked_skins: [true, false, false, false],
            preferred_skin: 0,
            stored_weapon: WeaponId(0),
            start_weapon: WeaponId(0),
            start_crown: 0,
        });

        // Drop stored gun with no start gun.
        if lo.start_weapon == WeaponId(0) {
            lo.stored_weapon = WeaponId(0);
        }

        // Convert port crown ids to GML ids.
        if lo.start_crown != 0 {
            let gml = crate::game::content::crown_port_to_gml(lo.start_crown);
            if !self.crown_unlocked(race, gml) {
                lo.start_crown = 0;
            }
        }

        lo
    }

    pub fn sanitize_loadouts(&mut self) {
        for lo in self.races.values_mut() {

            if lo.start_weapon.0 != 0 && lo.start_weapon != lo.stored_weapon {
                lo.start_weapon = WeaponId(0);
            }

            if lo.stored_weapon.0 == 0 {
                lo.start_weapon = WeaponId(0);
            }
        }

        let mut to_reset = Vec::new();
        for (race, lo) in self.races.iter() {
            if lo.start_crown != 0 {
                let gml = crate::game::content::crown_port_to_gml(lo.start_crown);
                if !self.crown_unlocked(*race, gml) {
                    to_reset.push(*race);
                }
            }
        }
        for race in to_reset {
            if let Some(lo) = self.races.get_mut(&race) {
                lo.start_crown = 0;
            }
        }
    }

    fn crown_row(&self, race: RaceId) -> [bool; 14] {
        const BASE: [bool; 14] = [
            true, true, false, false, false, false, false, false, false, false, false, false,
            false, false,
        ];
        self.crown_got.get(&race).copied().unwrap_or(BASE)
    }

    pub fn crown_unlocked(&self, race: RaceId, crown: u8) -> bool {
        if crown as usize >= 14 || race == RaceId::Random {
            return false;
        }
        self.race_unlocked(race) && self.crown_row(race)[crown as usize]
    }

    pub fn any_crown_unlocked(&self, race: RaceId) -> bool {
        if race == RaceId::Random {
            return false;
        }
        let row = self.crown_row(race);
        (2..14).any(|i| row[i])
    }

    pub fn skin_unlocked(&self, race: RaceId, skin: u8) -> bool {
        if skin == 0 {
            return true;
        }
        race != RaceId::Random
            && self.races.get(&race).is_some_and(|lo| {
                lo.unlocked_skins
                    .get(skin as usize)
                    .copied()
                    .unwrap_or(false)
            })
    }

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

    #[test]
    fn crown_defaults_match_scrInit() {
        let save = SaveData::default();
        assert!(save.crown_unlocked(RaceId::Fish, 0));
        assert!(save.crown_unlocked(RaceId::Fish, 1));
        assert!(!save.crown_unlocked(RaceId::Fish, 2));
        assert!(!save.any_crown_unlocked(RaceId::Fish));
    }

    #[test]
    fn crown_unlock_requires_race_unlocked() {
        let mut save = SaveData::default();
        save.unlock_crown(RaceId::Rogue, crate::game::content::crown_port_to_gml(2));
        assert!(!save.crown_unlocked(RaceId::Rogue, crate::game::content::crown_port_to_gml(2)));
        assert!(!save.any_crown_unlocked(RaceId::Rogue));

        save.race_loadout_mut(RaceId::Rogue).unlocked = true;
        let haste_gml = crate::game::content::crown_port_to_gml(3);
        save.unlock_crown(RaceId::Rogue, haste_gml);
        assert!(save.crown_unlocked(RaceId::Rogue, haste_gml));
        assert!(save.any_crown_unlocked(RaceId::Rogue));
        assert_eq!(save.race_loadout(RaceId::Rogue).start_crown, 3);
    }

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

    #[test]
    fn skin_unlock_defaults_match_scrInit() {
        let mut save = SaveData::default();
        assert!(save.skin_unlocked(RaceId::Fish, 0));
        assert!(!save.skin_unlocked(RaceId::Fish, 1));
        assert!(save.skin_unlocked(RaceId::Random, 0));

        save.race_loadout_mut(RaceId::Fish).unlocked_skins[2] = true;
        assert!(save.skin_unlocked(RaceId::Fish, 2));
        assert!(!save.skin_unlocked(RaceId::Fish, 3));
    }

    #[test]
    fn preferred_skin_persists_and_defaults() {
        let mut save = SaveData::default();
        save.race_loadout_mut(RaceId::Fish).preferred_skin = 2;
        assert_eq!(save.race_loadout(RaceId::Fish).preferred_skin, 2);
        assert_eq!(
            save.race_loadout(RaceId::Crystal).preferred_skin,
            0,
            "other races keep their own pick"
        );

        let legacy: SaveData = serde_json::from_value(serde_json::json!({
            "high_score": 0,
            "races": { "Fish": { "unlocked": true } },
        }))
        .expect("legacy RaceLoadout without preferred_skin");
        assert_eq!(
            legacy.races[&RaceId::Fish].preferred_skin,
            0,
            "serde(default) fills the new field"
        );
    }
}
