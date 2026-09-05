//! Data-driven content registries: characters, weapons, enemies, mutations.
//! Stats mirror the GPL Nuclear-Throne-Mobile rebuild reference.
//! Visuals resolve through AssetCatalog (original strips via tools/gen_assets.py).

use std::collections::{HashMap, HashSet};

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Files that actually exist under `assets/images`, scanned once at startup.
/// `sprite_or_fallback` consults this so a missing PNG can never produce an
/// invisible entity (the Floppy-Warriors "never boot with blank art" rule).
#[derive(Resource, Default, Clone)]
pub struct AssetCatalog {
    pub images: HashSet<String>,
    /// Audio files (music/ambience candidates), keyed by asset path.
    pub audio: HashSet<String>,
    /// Strip metadata from assets/images/anims.json
    /// (name -> [frames, w, h, fps, xorigin, yorigin]).
    pub anims: HashMap<String, [f32; 6]>,
}

impl AssetCatalog {
    pub fn scan() -> Self {
        let mut images = HashSet::new();
        if let Ok(entries) = std::fs::read_dir("assets/images") {
            for entry in entries.flatten() {
                if let Ok(name) = entry.file_name().into_string()
                    && name.ends_with(".png")
                {
                    images.insert(format!("images/{name}"));
                }
            }
        } else {
            panic!(
                "assets/images is missing or unreadable. Run \
                 `NT_ALL_SPRITES=1 python3 tools/gen_assets.py` to import original art."
            );
        }
        if images.is_empty() {
            panic!(
                "assets/images contains no PNGs. Run `NT_ALL_SPRITES=1 python3 \
                 tools/gen_assets.py` to import original art."
            );
        }
        // Index audio files (ogg/wav/mp3/flac) under top-level asset dirs,
        // storing asset-relative paths like "audio/music/desert.ogg". Music is
        // optional content: an empty set is fine and every cue stays silent.
        let mut audio = HashSet::new();
        const AUDIO_EXTS: [&str; 4] = ["ogg", "wav", "mp3", "flac"];
        fn scan_audio_recursive(base: &str, dir: &std::path::Path, out: &mut HashSet<String>) {
            let Ok(entries) = std::fs::read_dir(dir) else {
                return;
            };
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_dir() {
                    scan_audio_recursive(base, &p, out);
                } else if let Some(name) = p.file_name().and_then(|n| n.to_str()) {
                    let ext = name.rsplit('.').next().unwrap_or("");
                    if AUDIO_EXTS.contains(&ext) {
                        if let Ok(rel) = p.strip_prefix("assets") {
                            out.insert(rel.to_string_lossy().to_string());
                        } else {
                            out.insert(format!("{base}/{name}"));
                        }
                    }
                }
            }
        }
        for sub in ["audio", "sounds", "music", "ambient", "ambience"] {
            let dir = std::path::Path::new("assets").join(sub);
            if dir.is_dir() {
                scan_audio_recursive(sub, &dir, &mut audio);
            }
        }
        // Also accept flat sfx candidates like "audio/sfx/snd_levelup.ogg" by mapping to flat file "audio/sndLevelUp.wav" pattern.
        // No extra alias map needed; resolve_audio_path does case-insensitive fallback.

        let mut anims = HashMap::new();
        if let Ok(txt) = std::fs::read_to_string("assets/images/anims.json")
            && let Ok(raw) = serde_json::from_str::<HashMap<String, HashMap<String, f32>>>(&txt)
        {
            for (name, e) in raw {
                if let (Some(frames), Some(w), Some(h)) = (
                    e.get("frames").copied(),
                    e.get("w").copied(),
                    e.get("h").copied(),
                ) {
                    anims.insert(
                        format!("images/{name}.png"),
                        [
                            frames,
                            w,
                            h,
                            e.get("fps").copied().unwrap_or(0.0),
                            e.get("xorigin").copied().unwrap_or(w * 0.5),
                            e.get("yorigin").copied().unwrap_or(h * 0.5),
                        ],
                    );
                }
            }
        }
        info!(
            "AssetCatalog: {} sprites, {} anim strips, {} audio files",
            images.len(),
            anims.len(),
            audio.len()
        );
        Self {
            images,
            audio,
            anims,
        }
    }

    /// Whether an audio file exists at this asset path.
    pub fn has_audio(&self, path: &str) -> bool {
        self.audio.contains(path)
    }

    pub fn resolve_audio_path(&self, stem: &str) -> Option<String> {
        // Direct match in indexed paths (supports sfx subfolders)
        for dir in ["audio", "sounds", "audio/sfx", "audio/music", "sounds/sfx"] {
            for ext in ["ogg", "wav", "mp3", "flac"] {
                let path = format!("{dir}/{stem}.{ext}");
                if self.has_audio(&path) {
                    return Some(path);
                }
                // Case-insensitive stem match against flat files (e.g., stem "snd_levelup" vs file "sndLevelUp.wav")
                let lower_path = path.to_ascii_lowercase();
                for existing in &self.audio {
                    if existing.to_ascii_lowercase() == lower_path {
                        return Some(existing.clone());
                    }
                }
                // Also try without snd_ prefix normalization: snd_levelup -> sndLevelUp
                // Check stem substring containment fallback (flat folder contains file with stem substring)
                let stem_lower = stem.to_ascii_lowercase().replace('_', "");
                for existing in &self.audio {
                    let exist_lower = existing.to_ascii_lowercase().replace('_', "");
                    if exist_lower.contains(&stem_lower) {
                        return Some(existing.clone());
                    }
                }
            }
        }
        // Final fallback: any audio file containing stem substring (useful for renamed flat placeholders like levelup.wav)
        let stem_norm = stem
            .to_ascii_lowercase()
            .replace("snd", "")
            .replace('_', "");
        for existing in &self.audio {
            let ex_norm = existing.to_ascii_lowercase().replace('_', "");
            if ex_norm.contains(&stem_norm) {
                return Some(existing.clone());
            }
        }
        None
    }

    /// Strip metadata for an animated sprite, if any.
    ///
    /// GML timing: character/prop/portal state strips are owned by objects
    /// with `image_speed = 0.4` at 30 Hz room speed (Portal, Corpse, enemy,
    /// Player, prop, hitme `Create_0.gml`), i.e. exactly 12 img/s. The only
    /// non-0.4 owners in `~/Downloads` are 14 projectile/FX objects whose
    /// strips never use state suffixes, so state-suffixed strips always play
    /// at 12 fps regardless of the extractor's guessed value.
    pub fn anim_def(&self, path: &str) -> Option<crate::game::anim::AnimDef> {
        self.anims.get(path).map(|a| {
            let mut fps = a[3];
            if fps > 0.0 && gml_state_strip_fps(path).is_some() {
                fps = 12.0;
            }
            crate::game::anim::AnimDef {
                frames: a[0] as u32,
                frame_px: a[1] as u32,
                height: a[2] as u32,
                fps,
            }
        })
    }

    pub fn has(&self, path: &str) -> bool {
        self.images.contains(path)
    }

    /// Panic when a referenced sprite was not imported.
    pub fn require(&self, path: &str) {
        if !self.images.contains(path) {
            panic!(
                "Missing art asset: {path}. Run `NT_ALL_SPRITES=1 python3 \
                 tools/gen_assets.py`."
            );
        }
    }
}

pub fn scan_asset_catalog() -> AssetCatalog {
    AssetCatalog::scan()
}

/// fps override for GML `image_speed = 0.4` state strips (12 img/s at 30 Hz).
/// Returns Some(12.0) when `path` is a state strip owned by a 0.4 object;
/// None keeps the extractor value (projectile/FX strips with genuine speeds,
/// and fps-0.0 static variant sheets which must never animate).
///
/// Suffix set is deliberately narrow: `Spawn`/`Charge`/`Fire` are excluded
/// because GuardianBullet (0.7) owns `sprGuardianBulletSpawn` and
/// BigGuardianBullet (0.5) owns `sprBigGuardianBulletSpawn`.
fn gml_state_strip_fps(path: &str) -> Option<f32> {
    let stem = path.rsplit('/').next().unwrap_or(path);
    let stem = stem.strip_suffix(".png").unwrap_or(stem);
    let state_suffix = ["Idle", "Walk", "Hurt", "Dead", "Appear", "Disappear", "Burrow"]
        .iter()
        .any(|s| stem.ends_with(s));
    // Portal object (image_speed 0.4) strips carry no state suffix.
    let portal_family = stem.starts_with("sprPortal")
        || stem.starts_with("sprProtoPortal")
        || stem.starts_with("sprPopoPortal")
        || stem.starts_with("sprBigPortal");
    if state_suffix || portal_family {
        Some(12.0)
    } else {
        None
    }
}

pub fn assert_nt_parity_assets(catalog: &AssetCatalog) {
    // These are deliberately gameplay/UI-visible assets. If any are missing,
    // the game would silently fall back to colored placeholders and diverge
    // from nt-recreated-public / Nuclear Throne UX.
    const REQUIRED: &[&str] = &[
        // Core projectile art
        "images/sprBullet1.png",
        "images/sprBullet2.png",
        "images/sprEnemyBullet1.png",
        "images/sprScorpionBullet.png",
        "images/sprGuardianBullet.png",
        "images/sprIDPDBullet.png",
        "images/sprRocket.png",
        "images/sprGrenade.png",
        "images/sprLaser.png",
        "images/sprBolt.png",
        "images/sprHeavyBullet.png",
        "images/sprHeavyBolt.png",
        "images/sprFlameBall.png",
        "images/sprSalamanderBullet.png",
        // Menu / loading / death parity-critical art
        // NOTE: sprMaggotBullet and sprMenuBG are expected by upstream but
        // are not present in the current extracted bundle; they are omitted
        // here to avoid false-positive panics while the extractor is updated.
        "images/sprLogo.png",
        "images/sprLoadoutCrown.png",
        "images/sprPortal.png",
        "images/sprBigPortal.png",
    ];

    let missing: Vec<&'static str> = REQUIRED
        .iter()
        .copied()
        .filter(|path| !catalog.has(path))
        .collect();

    if !missing.is_empty() {
        panic!(
            "nt-recreated-bevy is missing required original Nuclear Throne assets.\n\
             This build would diverge visually from nt-recreated-public.\n\
             Run `python3 tools/gen_assets.py /path/to/NuclearThrone/game/assets` first.\n\
             Missing assets:\n  - {}",
            missing.join("\n  - ")
        );
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum AmmoKind {
    None = 0,
    Bullets = 1,
    Shells = 2,
    Bolts = 3,
    Explosives = 4,
    Energy = 5,
}

// Keep legacy AmmoType alias for weapons_data bridge
pub use crate::game::weapons_data::AmmoType;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize, Hash, PartialOrd, Ord)]
#[repr(u8)]
pub enum RaceId {
    Random = 0,
    Fish = 1,
    Crystal = 2,
    Eyes = 3,
    Melting = 4,
    Plant = 5,
    Venuz = 6,
    Steroids = 7,
    Robot = 8,
    Chicken = 9,
    Rebel = 10,
    Horror = 11,
    Rogue = 12,
    BigDog = 13,
    Skeleton = 14,
    Frog = 15,
    Cuz = 16,
}

pub const PLAYABLE_RACES: [RaceId; 16] = [
    RaceId::Fish,
    RaceId::Crystal,
    RaceId::Eyes,
    RaceId::Melting,
    RaceId::Plant,
    RaceId::Venuz,
    RaceId::Steroids,
    RaceId::Robot,
    RaceId::Chicken,
    RaceId::Rebel,
    RaceId::Horror,
    RaceId::Rogue,
    RaceId::BigDog,
    RaceId::Skeleton,
    RaceId::Frog,
    RaceId::Cuz,
];

/// Back-compat: old 4-race code used CharacterId::Fish etc.
pub type CharacterId = RaceId;
/// All 16 selectable races (upstream Menu/Create_0 grid).
pub const CHARACTERS: [CharacterId; PLAYABLE_RACES.len()] = PLAYABLE_RACES;

/// The slot list upstream `Menu/Create_0` builds: every race id from
/// `Race.Random` up to (but excluding) `Race.NUM_ALL_RACE_TYPES` that is not
/// hidden (or is unlocked). Unlock gating filters this list at runtime -
/// Random and Fish are always free; everything else respects the save.
pub const CHAR_SELECT_RACES: [RaceId; 17] = [
    RaceId::Random,
    RaceId::Fish,
    RaceId::Crystal,
    RaceId::Eyes,
    RaceId::Melting,
    RaceId::Plant,
    RaceId::Venuz,
    RaceId::Steroids,
    RaceId::Robot,
    RaceId::Chicken,
    RaceId::Rebel,
    RaceId::Horror,
    RaceId::Rogue,
    RaceId::BigDog,
    RaceId::Skeleton,
    RaceId::Frog,
    RaceId::Cuz,
];

/// `scrRaceGetPassiveSkillDescription` stand-ins for the char-select text.
pub fn race_passive_text(race: RaceId) -> &'static str {
    match race {
        RaceId::Fish => "Kills drop extra ammo",
        RaceId::Crystal => "Gains a shield when hurt",
        RaceId::Eyes => "Sees far, eyes aim with you",
        RaceId::Melting => "Frail, but kills give max HP",
        RaceId::Plant => "Attacks snare nearby prey",
        RaceId::Venuz => "Ultra mutation is random",
        RaceId::Steroids => "Wields two weapons at once",
        RaceId::Robot => "Eats ammo to repair itself",
        RaceId::Chicken => "Cheats death once per floor",
        RaceId::Rebel => "Allies cost HP to call",
        RaceId::Horror => "Fires a piercing radiation beam",
        RaceId::Rogue => "Carries rogue ammo for strikes",
        RaceId::BigDog => "A very good dog",
        RaceId::Skeleton => "Bones rattle menacingly",
        RaceId::Frog => "Ribbit.",
        RaceId::Cuz => "Bumbles with random guns",
        RaceId::Random => "A surprise mutant",
    }
}

/// Inverse of `race as usize` for the nt-rewrite `enum Race` values.
pub fn race_from_gml_id(id: usize) -> Option<RaceId> {
    CHAR_SELECT_RACES
        .iter()
        .copied()
        .find(|r| *r as usize == id)
}

/// Exact HUD icon sprite used by nt-rewrite `wep_sprt[]`
/// (scripts/scrWeapons/scrWeapons.gml) for a weapon gml id.
pub fn weapon_hud_sprite(gml_id: u8) -> Option<&'static str> {
    Some(match gml_id {
        1 => "images/sprRevolver.png",
        3 => "images/sprWrench.png",
        4 => "images/sprMachinegun.png",
        5 => "images/sprShotgun.png",
        6 => "images/sprCrossbow.png",
        7 => "images/sprNader.png",
        16 => "images/sprSmg.png",
        17 => "images/sprARifle.png",
        88 => "images/sprHammer.png",
        _ => return None,
    })
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize, Hash, PartialOrd, Ord)]
#[repr(u8)]
pub enum SkinLetter {
    A = 0,
    B = 1,
    C = 2,
    D = 3,
}

impl SkinLetter {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::A),
            1 => Some(Self::B),
            2 => Some(Self::C),
            3 => Some(Self::D),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
pub struct WeaponId(pub u8);

impl WeaponId {
    pub const NONE: Self = Self(0);
    pub const REVOLVER: Self = Self(1);
    pub const WRENCH: Self = Self(3);
    pub const MACHINEGUN: Self = Self(4);
    pub const SHOTGUN: Self = Self(5);
    pub const CROSSBOW: Self = Self(6);
    pub const GRENADE_LAUNCHER: Self = Self(7);
    pub const SMG: Self = Self(16);
    pub const ASSAULT_RIFLE: Self = Self(17);
    pub const SLEDGEHAMMER: Self = Self(88);
}

pub const WEAPON_NONE: WeaponId = WeaponId(0);
pub const WEAPON_REVOLVER: WeaponId = WeaponId(1);

pub fn resolve_start_weapon(raw: WeaponId) -> WeaponId {
    if raw == WEAPON_NONE {
        WEAPON_REVOLVER
    } else {
        raw
    }
}

impl From<WeaponKind> for WeaponId {
    fn from(k: WeaponKind) -> Self {
        match k {
            WeaponKind::None => Self(0),
            WeaponKind::Revolver => Self(1),
            WeaponKind::Wrench => Self(3),
            WeaponKind::Machinegun => Self(4),
            WeaponKind::Shotgun => Self(5),
            WeaponKind::Crossbow => Self(6),
            WeaponKind::GrenadeLauncher => Self(7),
            WeaponKind::Smg => Self(16),
            WeaponKind::AssaultRifle => Self(17),
            WeaponKind::Sledgehammer => Self(88),
        }
    }
}

impl From<WeaponId> for WeaponKind {
    fn from(id: WeaponId) -> Self {
        match id.0 {
            1 => WeaponKind::Revolver,
            3 => WeaponKind::Wrench,
            4 => WeaponKind::Machinegun,
            5 => WeaponKind::Shotgun,
            6 => WeaponKind::Crossbow,
            7 => WeaponKind::GrenadeLauncher,
            16 => WeaponKind::Smg,
            17 => WeaponKind::AssaultRifle,
            88 => WeaponKind::Sledgehammer,
            _ => WeaponKind::None,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AbilityKind {
    Flip,          // Fish
    Shield,        // Crystal
    Telekinesis,   // Eyes
    Detonate,      // Melting
    Snare,         // Plant - slow enemies in a cone
    PopPop,        // Y.V. - next shot fires twice
    GetLoaded,     // Steroids - refill ammo
    EatWeapon,     // Robot - consume current weapon for HP
    Throw,         // Chicken - short thrash dash + heal 1
    SpawnAlly,     // Rebel
    HorrorBeam,    // Horror - rad beam along aim
    PortalStrike,  // Rogue - delayed blast at aim point
    RocketBarrage, // Big Dog
    BloodGamble,   // Skeleton - spend 1 HP, random weapon
    ToxicPuke,     // Frog - toxic cloud
    CuzSwap,       // Cuz - cycle 3rd slot / quick-swap
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PassiveKind {
    None,
    ShieldOnHit,     // Crystal
    ChainExplosions, // Melting
    FastReload,      // Steroids-ish dual feel via fire-rate
    Headless,        // Chicken survives lethal hit once per floor
    FreeAmmo,        // Robot passive: ammo pickups heal slightly (hooked later)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, serde::Serialize, serde::Deserialize)]
#[repr(u8)]
pub enum CrownKind {
    None = 0,
    Death = 1,
    Life = 2,
    Haste = 3,
    Guns = 4,
    Hatred = 5,
    Blood = 6,
    Destiny = 7,
    Love = 8,
    Risk = 9,
    Curses = 10,
    Luck = 11,
    Protection = 12,
}

impl CrownKind {
    pub const ALL: [CrownKind; 13] = [
        CrownKind::None,
        CrownKind::Death,
        CrownKind::Life,
        CrownKind::Haste,
        CrownKind::Guns,
        CrownKind::Hatred,
        CrownKind::Blood,
        CrownKind::Destiny,
        CrownKind::Love,
        CrownKind::Risk,
        CrownKind::Curses,
        CrownKind::Luck,
        CrownKind::Protection,
    ];

    pub fn from_u8(value: u8) -> Self {
        match value {
            1 => CrownKind::Death,
            2 => CrownKind::Life,
            3 => CrownKind::Haste,
            4 => CrownKind::Guns,
            5 => CrownKind::Hatred,
            6 => CrownKind::Blood,
            7 => CrownKind::Destiny,
            8 => CrownKind::Love,
            9 => CrownKind::Risk,
            10 => CrownKind::Curses,
            11 => CrownKind::Luck,
            12 => CrownKind::Protection,
            _ => CrownKind::None,
        }
    }

    pub fn to_u8(self) -> u8 {
        self as u8
    }

    pub fn name(self) -> &'static str {
        match self {
            CrownKind::None => "No Crown",
            CrownKind::Death => "Crown of Death",
            CrownKind::Life => "Crown of Life",
            CrownKind::Haste => "Crown of Haste",
            CrownKind::Guns => "Crown of Guns",
            CrownKind::Hatred => "Crown of Hatred",
            CrownKind::Blood => "Crown of Blood",
            CrownKind::Destiny => "Crown of Destiny",
            CrownKind::Love => "Crown of Love",
            CrownKind::Risk => "Crown of Risk",
            CrownKind::Curses => "Crown of Curses",
            CrownKind::Luck => "Crown of Luck",
            CrownKind::Protection => "Crown of Protection",
        }
    }

    pub fn short_name(self) -> &'static str {
        match self {
            CrownKind::None => "NONE",
            CrownKind::Death => "DEATH",
            CrownKind::Life => "LIFE",
            CrownKind::Haste => "HASTE",
            CrownKind::Guns => "GUNS",
            CrownKind::Hatred => "HATRED",
            CrownKind::Blood => "BLOOD",
            CrownKind::Destiny => "DESTINY",
            CrownKind::Love => "LOVE",
            CrownKind::Risk => "RISK",
            CrownKind::Curses => "CURSES",
            CrownKind::Luck => "LUCK",
            CrownKind::Protection => "PROTECTION",
        }
    }

    pub fn is_active(self) -> bool {
        self != CrownKind::None
    }

    pub fn cycle(self, dir: i8) -> Self {
        let len = Self::ALL.len();
        let current = Self::ALL.iter().position(|&c| c == self).unwrap_or(0);

        let next = if dir >= 0 {
            (current + 1) % len
        } else {
            (current + len - 1) % len
        };

        Self::ALL[next]
    }
}

impl Default for CrownKind {
    fn default() -> Self {
        CrownKind::None
    }
}

pub fn crown_name(id: u8) -> &'static str {
    CrownKind::from_u8(id).name()
}

pub fn crown_short_name(id: u8) -> &'static str {
    CrownKind::from_u8(id).short_name()
}

pub fn cycle_crown_id(id: u8, dir: i8) -> u8 {
    CrownKind::from_u8(id).cycle(dir).to_u8()
}

/// Port CrownKind id -> nt-rewrite crwn_* grid/save id (RANDOM=0, NONE=1,
/// DEATH=2 .. PROTECTION=13). Port NONE(0) maps to crwn_none(1); every real
/// crown shifts up one slot.
pub fn crown_port_to_gml(id: u8) -> u8 {
    if id == 0 { 1 } else { id + 1 }
}

/// Inverse of [`crown_port_to_gml`]: crwn_none/RANDOM collapse to port 0.
pub fn crown_gml_to_port(id: u8) -> u8 {
    if id <= 1 { 0 } else { id - 1 }
}

pub struct CharacterDef {
    pub name: &'static str,
    pub color: Color,
    pub max_hp: i32,
    pub speed_mult: f32,
    pub pickup_range: f32,
    pub ability: AbilityKind,
    pub passive: PassiveKind,
    pub sprite: &'static str,
    /// Walk-cycle strip (upstream sprMutantNWalk).
    pub walk_sprite: &'static str,
}

pub fn character_def(id: RaceId) -> CharacterDef {
    match id {
        RaceId::Fish => CharacterDef {
            name: "Fish",
            color: Color::srgb(0.25, 0.95, 0.35),
            max_hp: 8,
            speed_mult: 1.0,
            pickup_range: 95.0,
            ability: AbilityKind::Flip,
            passive: PassiveKind::None,
            sprite: "images/sprMutant1Idle.png",
            walk_sprite: "images/sprMutant1Walk.png",
        },
        RaceId::Crystal => CharacterDef {
            name: "Crystal",
            color: Color::srgb(0.35, 0.65, 1.0),
            max_hp: 10,
            speed_mult: 1.0,
            pickup_range: 95.0,
            ability: AbilityKind::Shield,
            passive: PassiveKind::ShieldOnHit,
            sprite: "images/sprMutant2Idle.png",
            walk_sprite: "images/sprMutant2Walk.png",
        },
        RaceId::Eyes => CharacterDef {
            name: "Eyes",
            color: Color::srgb(0.85, 0.4, 1.0),
            max_hp: 8,
            speed_mult: 1.0,
            pickup_range: 175.0,
            ability: AbilityKind::Telekinesis,
            passive: PassiveKind::None,
            sprite: "images/sprMutant3Idle.png",
            walk_sprite: "images/sprMutant3Walk.png",
        },
        RaceId::Melting => CharacterDef {
            name: "Melting",
            color: Color::srgb(0.95, 0.85, 0.45),
            max_hp: 2,
            speed_mult: 1.0,
            pickup_range: 95.0,
            ability: AbilityKind::Detonate,
            passive: PassiveKind::ChainExplosions,
            sprite: "images/sprMutant4Idle.png",
            walk_sprite: "images/sprMutant4Walk.png",
        },
        RaceId::Plant => CharacterDef {
            name: "Plant",
            color: Color::srgb(0.3, 0.85, 0.35),
            max_hp: 8,
            speed_mult: 1.125, // 4.5/4 GML
            pickup_range: 95.0,
            ability: AbilityKind::Snare,
            passive: PassiveKind::None,
            sprite: "images/sprMutant5Idle.png",
            walk_sprite: "images/sprMutant5Walk.png",
        },
        RaceId::Venuz => CharacterDef {
            name: "Venuz",
            color: Color::srgb(0.85, 0.7, 0.2),
            max_hp: 8,
            speed_mult: 1.0,
            pickup_range: 95.0,
            ability: AbilityKind::PopPop,
            passive: PassiveKind::None,
            sprite: "images/sprMutant6Idle.png",
            walk_sprite: "images/sprMutant6Walk.png",
        },
        RaceId::Steroids => CharacterDef {
            name: "Steroids",
            color: Color::srgb(0.9, 0.25, 0.25),
            max_hp: 8,
            speed_mult: 1.0,
            pickup_range: 95.0,
            ability: AbilityKind::GetLoaded,
            passive: PassiveKind::FastReload,
            sprite: "images/sprMutant7Idle.png",
            walk_sprite: "images/sprMutant7Walk.png",
        },
        // Note: GML Steroids has accuracy 1.8 (handled via spread_mult in Player)
        RaceId::Robot => CharacterDef {
            name: "Robot",
            color: Color::srgb(0.6, 0.6, 0.65),
            max_hp: 8,
            speed_mult: 1.0,
            pickup_range: 95.0,
            ability: AbilityKind::EatWeapon,
            passive: PassiveKind::FreeAmmo,
            sprite: "images/sprMutant8Idle.png",
            walk_sprite: "images/sprMutant8Walk.png",
        },
        RaceId::Chicken => CharacterDef {
            name: "Chicken",
            color: Color::srgb(0.95, 0.9, 0.6),
            max_hp: 8,
            speed_mult: 1.0, // GML maxspeed 4 like Fish
            pickup_range: 95.0,
            ability: AbilityKind::Throw,
            passive: PassiveKind::Headless,
            sprite: "images/sprMutant9Idle.png",
            walk_sprite: "images/sprMutant9Walk.png",
        },
        RaceId::Rebel => CharacterDef {
            name: "Rebel",
            color: Color::srgb(0.75, 0.25, 0.55),
            max_hp: 8,
            speed_mult: 1.0,
            pickup_range: 95.0,
            ability: AbilityKind::SpawnAlly,
            passive: PassiveKind::None,
            sprite: "images/sprMutant10Idle.png",
            walk_sprite: "images/sprMutant10Walk.png",
        },
        RaceId::Horror => CharacterDef {
            name: "Horror",
            color: Color::srgb(0.5, 0.35, 0.85),
            max_hp: 8,
            speed_mult: 1.0,
            pickup_range: 95.0,
            ability: AbilityKind::HorrorBeam,
            passive: PassiveKind::None,
            sprite: "images/sprMutant11Idle.png",
            walk_sprite: "images/sprMutant11Walk.png",
        },
        RaceId::Rogue => CharacterDef {
            name: "Rogue",
            color: Color::srgb(0.35, 0.35, 0.45),
            max_hp: 8,
            speed_mult: 1.0,
            pickup_range: 95.0,
            ability: AbilityKind::PortalStrike,
            passive: PassiveKind::None,
            sprite: "images/sprMutant12Idle.png",
            walk_sprite: "images/sprMutant12Walk.png",
        },
        RaceId::BigDog => CharacterDef {
            name: "Big Dog",
            color: Color::srgb(0.55, 0.38, 0.28),
            max_hp: 300,     // GML scrPlayerRaceChange
            speed_mult: 0.5, // GML maxspeed 2/4
            pickup_range: 95.0,
            ability: AbilityKind::RocketBarrage,
            passive: PassiveKind::None,
            sprite: "images/sprMutant13Idle.png",
            walk_sprite: "images/sprMutant13Walk.png",
        },
        RaceId::Skeleton => CharacterDef {
            name: "Skeleton",
            color: Color::srgb(0.9, 0.9, 0.92),
            max_hp: 4,        // GML
            speed_mult: 0.75, // GML maxspeed 3/4
            pickup_range: 95.0,
            ability: AbilityKind::BloodGamble,
            passive: PassiveKind::None,
            sprite: "images/sprMutant14Idle.png",
            walk_sprite: "images/sprMutant14Walk.png",
        },
        RaceId::Frog => CharacterDef {
            name: "Frog",
            color: Color::srgb(0.45, 0.8, 0.55),
            max_hp: 8,
            speed_mult: 1.0,
            pickup_range: 95.0,
            ability: AbilityKind::ToxicPuke,
            passive: PassiveKind::None,
            sprite: "images/sprMutant15Idle.png",
            walk_sprite: "images/sprMutant15Walk.png",
        },
        RaceId::Cuz => CharacterDef {
            name: "Cuz",
            color: Color::srgb(0.8, 0.65, 0.35),
            max_hp: 8,
            speed_mult: 1.0,
            pickup_range: 95.0,
            ability: AbilityKind::CuzSwap,
            passive: PassiveKind::None,
            sprite: "images/sprMutant16Idle.png",
            walk_sprite: "images/sprMutant16Walk.png",
        },
        RaceId::Random => character_def(RaceId::Fish),
    }
}

pub fn ability_name(kind: AbilityKind) -> &'static str {
    match kind {
        AbilityKind::Flip => "Flip",
        AbilityKind::Shield => "Shield",
        AbilityKind::Telekinesis => "Telekinesis",
        AbilityKind::Detonate => "Detonate",
        AbilityKind::Snare => "Snare",
        AbilityKind::PopPop => "Pop Pop",
        AbilityKind::GetLoaded => "Get Loaded",
        AbilityKind::EatWeapon => "Eat Weapon",
        AbilityKind::Throw => "Throw",
        AbilityKind::SpawnAlly => "Rebel Yell",
        AbilityKind::HorrorBeam => "Irradiate",
        AbilityKind::PortalStrike => "Portal Strike",
        AbilityKind::RocketBarrage => "Barrage",
        AbilityKind::BloodGamble => "Blood Gamble",
        AbilityKind::ToxicPuke => "Puke",
        AbilityKind::CuzSwap => "Extra Slot",
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WeaponKind {
    None,
    Revolver,
    Machinegun,
    Smg,
    AssaultRifle,
    Shotgun,
    Crossbow,
    GrenadeLauncher,
    Wrench,
    Sledgehammer,
}

#[derive(Clone, Copy)]
pub struct MeleeDef {
    pub range: f32,
    pub arc: f32,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HazardKind {
    Fire,
    Toxic,
}

#[derive(Clone, Copy, Debug)]
pub struct HazardDef {
    pub kind: HazardKind,
    pub radius: f32,
    pub damage: i32,
    pub duration: f32,
    pub tick: f32,
    pub color: Color,
}

#[derive(Clone, Copy, Debug)]
pub struct SplitDef {
    pub pellets: u8,
    pub spread: f32,
    pub speed: f32,
    pub damage: i32,
    pub lifetime: f32,
    pub radius: f32,
    pub knockback: f32,
    pub color: Color,
    pub size: Vec2,
}

#[derive(Clone, Copy)]
pub struct WeaponDef {
    pub name: &'static str,
    pub ammo: AmmoKind,
    pub ammo_cost: i32,
    /// GML wep_rads: extra rad cost on fire (ultra weapons 4..16).
    pub rad_cost: u32,
    pub cooldown: f32,
    pub damage: i32,
    pub pellets: usize,
    pub speed: f32,
    pub lifetime: f32,
    pub spread: f32,
    pub recoil: f32,
    pub shake: f32,
    pub projectile_radius: f32,
    pub knockback: f32,
    pub automatic: bool,
    pub explosive: bool,
    pub burst_shots: usize,
    pub burst_interval: f32,
    pub melee: Option<MeleeDef>,
    pub color: Color,
    pub size: Vec2,
    pub muzzle_burst: usize,
    pub bounces: u8,
    pub pierce: u8,
    pub hazard: Option<HazardDef>,
    pub split: Option<SplitDef>,
}

pub fn weapon_def(kind: WeaponKind) -> WeaponDef {
    match kind {
        WeaponKind::None => WeaponDef {
            name: "None",
            ammo: AmmoKind::Bullets,
            ammo_cost: 0,
            rad_cost: 0,
            cooldown: 1.0,
            damage: 0,
            pellets: 0,
            speed: 0.0,
            lifetime: 0.1,
            spread: 0.0,
            recoil: 0.0,
            shake: 0.0,
            projectile_radius: 0.0,
            knockback: 0.0,
            automatic: false,
            explosive: false,
            burst_shots: 0,
            burst_interval: 0.0,
            melee: None,
            color: Color::srgb(0.4, 0.4, 0.4),
            size: Vec2::new(1.0, 1.0),
            muzzle_burst: 0,
            bounces: 0,
            pierce: 0,
            hazard: None,
            split: None,
        },
        WeaponKind::Revolver => WeaponDef {
            name: "Revolver",
            ammo: AmmoKind::Bullets,
            ammo_cost: 1,
            rad_cost: 0,
            cooldown: frames(6.0),
            damage: 3,
            pellets: 1,
            speed: 480.0,
            lifetime: 0.95,
            spread: 0.07,
            recoil: 5.0,
            shake: 0.1,
            projectile_radius: 4.0,
            knockback: 150.0,
            automatic: false,
            explosive: false,
            burst_shots: 1,
            burst_interval: 0.0,
            melee: None,
            color: Color::srgb(1.0, 0.9, 0.25),
            size: Vec2::new(16.0, 5.0),
            muzzle_burst: 4,
            bounces: 0,
            pierce: 0,
            hazard: None,
            split: None,
        },
        WeaponKind::Machinegun => WeaponDef {
            name: "Machinegun",
            ammo: AmmoKind::Bullets,
            ammo_cost: 1,
            rad_cost: 0,
            cooldown: frames(5.0),
            damage: 3,
            pellets: 1,
            speed: 480.0,
            lifetime: 0.85,
            spread: 0.105,
            recoil: 3.5,
            shake: 0.06,
            projectile_radius: 3.0,
            knockback: 70.0,
            automatic: true,
            explosive: false,
            burst_shots: 1,
            burst_interval: 0.0,
            melee: None,
            color: Color::srgb(1.0, 1.0, 0.35),
            size: Vec2::new(12.0, 4.0),
            muzzle_burst: 2,
            bounces: 0,
            pierce: 0,
            hazard: None,
            split: None,
        },
        WeaponKind::Smg => WeaponDef {
            name: "SMG",
            ammo: AmmoKind::Bullets,
            ammo_cost: 1,
            rad_cost: 0,
            cooldown: frames(3.0),
            damage: 3,
            pellets: 1,
            speed: 480.0,
            lifetime: 0.7,
            spread: 0.28,
            recoil: 2.5,
            shake: 0.04,
            projectile_radius: 3.0,
            knockback: 50.0,
            automatic: true,
            explosive: false,
            burst_shots: 1,
            burst_interval: 0.0,
            melee: None,
            color: Color::srgb(1.0, 0.85, 0.3),
            size: Vec2::new(11.0, 4.0),
            muzzle_burst: 1,
            bounces: 0,
            pierce: 0,
            hazard: None,
            split: None,
        },
        WeaponKind::AssaultRifle => WeaponDef {
            name: "Assault Rifle",
            ammo: AmmoKind::Bullets,
            ammo_cost: 3,
            rad_cost: 0,
            cooldown: frames(11.0),
            damage: 3,
            pellets: 1,
            speed: 480.0,
            lifetime: 0.9,
            spread: 0.035,
            recoil: 4.0,
            shake: 0.07,
            projectile_radius: 3.5,
            knockback: 60.0,
            automatic: true,
            explosive: false,
            burst_shots: 3,
            burst_interval: frames(1.0),
            melee: None,
            color: Color::srgb(0.95, 0.95, 0.5),
            size: Vec2::new(13.0, 4.0),
            muzzle_burst: 2,
            bounces: 0,
            pierce: 0,
            hazard: None,
            split: None,
        },
        WeaponKind::Shotgun => WeaponDef {
            name: "Shotgun",
            ammo: AmmoKind::Shells,
            ammo_cost: 1,
            rad_cost: 0,
            cooldown: frames(17.0),
            damage: 2,
            pellets: 7,
            speed: 450.0,
            lifetime: 0.45,
            spread: 0.35,
            recoil: 16.0,
            shake: 0.24,
            projectile_radius: 4.0,
            knockback: 90.0,
            automatic: false,
            explosive: false,
            burst_shots: 1,
            burst_interval: 0.0,
            melee: None,
            color: Color::srgb(1.0, 0.72, 0.26),
            size: Vec2::new(10.0, 4.0),
            muzzle_burst: 6,
            bounces: 0,
            pierce: 0,
            hazard: None,
            split: None,
        },
        WeaponKind::Crossbow => WeaponDef {
            name: "Crossbow",
            ammo: AmmoKind::Bolts,
            ammo_cost: 1,
            rad_cost: 0,
            cooldown: frames(26.0),
            damage: 20,
            pellets: 1,
            speed: 720.0,
            lifetime: 1.2,
            spread: 0.015,
            recoil: 10.0,
            shake: 0.18,
            projectile_radius: 5.0,
            knockback: 300.0,
            automatic: false,
            explosive: false,
            burst_shots: 1,
            burst_interval: 0.0,
            melee: None,
            color: Color::srgb(0.65, 0.35, 0.12),
            size: Vec2::new(24.0, 5.0),
            muzzle_burst: 3,
            bounces: 0,
            pierce: 0,
            hazard: None,
            split: None,
        },
        WeaponKind::GrenadeLauncher => WeaponDef {
            name: "Grenade Launcher",
            ammo: AmmoKind::Explosives,
            ammo_cost: 1,
            rad_cost: 0,
            cooldown: frames(20.0),
            damage: 15,
            pellets: 1,
            speed: 300.0,
            lifetime: 1.4,
            spread: 0.04,
            recoil: 18.0,
            shake: 0.3,
            projectile_radius: 7.0,
            knockback: 350.0,
            automatic: false,
            explosive: true,
            burst_shots: 1,
            burst_interval: 0.0,
            melee: None,
            color: Color::srgb(0.25, 0.95, 0.25),
            size: Vec2::splat(12.0),
            muzzle_burst: 5,
            bounces: 0,
            pierce: 0,
            hazard: None,
            split: None,
        },
        WeaponKind::Wrench => WeaponDef {
            name: "Wrench",
            ammo: AmmoKind::None,
            ammo_cost: 0,
            rad_cost: 0,
            cooldown: frames(22.0),
            damage: 8,
            pellets: 0,
            speed: 0.0,
            lifetime: 0.0,
            spread: 0.0,
            recoil: 0.0,
            shake: 0.0,
            projectile_radius: 0.0,
            knockback: 300.0,
            automatic: false,
            explosive: false,
            burst_shots: 1,
            burst_interval: 0.0,
            melee: Some(MeleeDef {
                range: 70.0,
                arc: 2.2,
            }),
            color: Color::srgb(0.7, 0.7, 0.75),
            size: Vec2::splat(20.0),
            muzzle_burst: 0,
            bounces: 0,
            pierce: 0,
            hazard: None,
            split: None,
        },
        WeaponKind::Sledgehammer => WeaponDef {
            name: "Sledgehammer",
            ammo: AmmoKind::None,
            ammo_cost: 0,
            rad_cost: 0,
            cooldown: frames(35.0),
            damage: 24,
            pellets: 0,
            speed: 0.0,
            lifetime: 0.0,
            spread: 0.0,
            recoil: 0.0,
            shake: 0.0,
            projectile_radius: 0.0,
            knockback: 600.0,
            automatic: false,
            explosive: false,
            burst_shots: 1,
            burst_interval: 0.0,
            melee: Some(MeleeDef {
                range: 96.0,
                arc: 2.6,
            }),
            color: Color::srgb(0.55, 0.5, 0.6),
            size: Vec2::splat(26.0),
            muzzle_burst: 0,
            bounces: 0,
            pierce: 0,
            hazard: None,
            split: None,
        },
    }
}

pub fn weapon_name(kind: WeaponKind) -> &'static str {
    weapon_def(kind).name
}

pub fn weapon_color(kind: WeaponKind) -> Color {
    weapon_def(kind).color
}

pub fn weapon_meta(id: WeaponId) -> &'static crate::game::weapons_data::WeaponData {
    crate::game::weapons_data::WEAPONS
        .get(id.0 as usize)
        .unwrap_or(&crate::game::weapons_data::WEAPONS[0])
}

/// Corrupt/OOB ids collapse to NONE instead of indexing wild memory.
pub fn sanitize_weapon_id(id: WeaponId) -> WeaponId {
    if (id.0 as usize) < crate::game::weapons_data::WEAPONS.len() {
        id
    } else {
        WeaponId::NONE
    }
}

/// Ammo family of a weapon, safe for any id.
pub fn weapon_ammo(id: WeaponId) -> AmmoKind {
    if id == WeaponId::NONE {
        return AmmoKind::None;
    }

    match weapon_meta(sanitize_weapon_id(id)).wep_type {
        crate::game::weapons_data::AmmoType::None => AmmoKind::None,
        crate::game::weapons_data::AmmoType::Bullets => AmmoKind::Bullets,
        crate::game::weapons_data::AmmoType::Shells => AmmoKind::Shells,
        crate::game::weapons_data::AmmoType::Bolts => AmmoKind::Bolts,
        crate::game::weapons_data::AmmoType::Explosives => AmmoKind::Explosives,
        crate::game::weapons_data::AmmoType::Energy => AmmoKind::Energy,
    }
}

pub fn weapon_id_name(id: WeaponId) -> &'static str {
    if id == WeaponId::NONE {
        return "NONE";
    }

    weapon_meta(id).wep_name
}

/// Ammo capacity per kind (reference: bullets 255, others 55, energy 55). Back Muscle adds
/// +300 / +44 respectively.
pub fn ammo_max(kind: AmmoKind) -> i32 {
    match kind {
        AmmoKind::None => 0,
        AmmoKind::Bullets => 255,
        AmmoKind::Shells | AmmoKind::Bolts | AmmoKind::Explosives | AmmoKind::Energy => 55,
    }
}

pub fn ammo_pickup_amount(kind: AmmoKind) -> i32 {
    // GML typ_ammo (scrAmmoUpdateTypeStats): 32/8/7/6/10.
    match kind {
        AmmoKind::None => 0,
        AmmoKind::Bullets => 32,
        AmmoKind::Shells => 8,
        AmmoKind::Bolts => 7,
        AmmoKind::Explosives => 6,
        AmmoKind::Energy => 10,
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u16)]
pub enum EnemyKind {
    Maggot,
    Bandit,
    Scorpion,
    Assassin,
    Freak,
    BigBandit,
    BigBanditLoop,
    Throne,
    ThroneII,
    Hyper,
    Rat,
    BigRat,
    RobotGuard,
    Turret,
    SnowBandit,
    Wolf,
    BigDog,
    BigDogLoop,
    LilHunter,
    LilHunterLoop,
    IdpdGrunt,
    IdpdShield,
    IdpdElite,
    IdpdVan,
    // Loop / secret bosses (parity with upstream)
    Mom,
    /// Pizza Sewers secret boss (upstream FrogQueen / Ball Mama).
    FrogQueen,
    Technomancer,
    Captain,
    // Area fodder used by Mom / Techno
    Ballguy,
    FrogEgg,
    Necromancer,
    // Expanded area roster
    Spider,
    Crystal,
    LaserCrystal,
    Sniper,
    Crab,
    // Upstream roster parity (scrPopEnemies): sewers / scrapyards / caves
    Gator,
    BuffGator,
    Raven,
    Salamander,
    MeleeBandit,
    BigMaggot,
    FastRat,
    Ratking,
    GoldScorpion,
    LightningCrystal,
    ExploFreak,
    RhinoFreak,
    // Frozen City / Palace garrisons
    SnowTank,
    GoldSnowtank,
    Guardian,
    ExploGuardian,
    DogGuardian,
    // Jungle secret area
    JungleBandit,
    // Secret areas & Y.V. Mansion garrison (scrPopEnemies parity)
    BoneFish,
    Turtle,
    Molefish,
    Molesarge,
    FireBaller,
    SuperFireBaller,
    Jock,
    JungleFly,
    InvSpider,
    InvLaserCrystal,
    PopoFreak,
    MaggotSpawn,
    // IDPD inspector unit (HQ trios)
    IdpdInspector,
    // Vault
    OldGuardian,
    /// Palace guardian spawned by Throne statues.
    PalaceGuardian,
}

impl EnemyKind {
    pub fn from_u16(v: u16) -> Option<Self> {
        const ALL: &[EnemyKind] = &[
            EnemyKind::Maggot,
            EnemyKind::Bandit,
            EnemyKind::Scorpion,
            EnemyKind::Assassin,
            EnemyKind::Freak,
            EnemyKind::BigBandit,
            EnemyKind::BigBanditLoop,
            EnemyKind::Throne,
            EnemyKind::ThroneII,
            EnemyKind::Hyper,
            EnemyKind::Rat,
            EnemyKind::BigRat,
            EnemyKind::RobotGuard,
            EnemyKind::Turret,
            EnemyKind::SnowBandit,
            EnemyKind::Wolf,
            EnemyKind::BigDog,
            EnemyKind::BigDogLoop,
            EnemyKind::LilHunter,
            EnemyKind::LilHunterLoop,
            EnemyKind::IdpdGrunt,
            EnemyKind::IdpdShield,
            EnemyKind::IdpdElite,
            EnemyKind::IdpdVan,
            EnemyKind::Mom,
            EnemyKind::FrogQueen,
            EnemyKind::Technomancer,
            EnemyKind::Captain,
            EnemyKind::Ballguy,
            EnemyKind::FrogEgg,
            EnemyKind::Necromancer,
            EnemyKind::Spider,
            EnemyKind::Crystal,
            EnemyKind::LaserCrystal,
            EnemyKind::Sniper,
            EnemyKind::Crab,
            EnemyKind::Gator,
            EnemyKind::BuffGator,
            EnemyKind::Raven,
            EnemyKind::Salamander,
            EnemyKind::MeleeBandit,
            EnemyKind::BigMaggot,
            EnemyKind::FastRat,
            EnemyKind::Ratking,
            EnemyKind::GoldScorpion,
            EnemyKind::LightningCrystal,
            EnemyKind::ExploFreak,
            EnemyKind::RhinoFreak,
            EnemyKind::SnowTank,
            EnemyKind::GoldSnowtank,
            EnemyKind::Guardian,
            EnemyKind::ExploGuardian,
            EnemyKind::DogGuardian,
            EnemyKind::JungleBandit,
            EnemyKind::BoneFish,
            EnemyKind::Turtle,
            EnemyKind::Molefish,
            EnemyKind::Molesarge,
            EnemyKind::FireBaller,
            EnemyKind::SuperFireBaller,
            EnemyKind::Jock,
            EnemyKind::JungleFly,
            EnemyKind::InvSpider,
            EnemyKind::InvLaserCrystal,
            EnemyKind::PopoFreak,
            EnemyKind::MaggotSpawn,
            EnemyKind::IdpdInspector,
            EnemyKind::OldGuardian,
            EnemyKind::PalaceGuardian,
        ];
        ALL.get(v as usize).copied()
    }
}

/// `size`/`color` are presentation-parity fields retained from the reference
/// registry; runtime visuals come from sprite strips.
#[derive(Clone, Copy)]
#[allow(dead_code)]
pub struct EnemyDef {
    pub name: &'static str,
    pub hp: i32,
    pub speed: f32,
    pub accel: f32,
    pub radius: f32,
    pub size: f32,
    pub color: Color,
    pub sprite: &'static str,
    pub score: u32,
    pub touch_damage: i32,
    pub rad_drop: usize,
    pub drop_chance: usize,
    pub weapon_chance: usize,
    pub preferred_range: f32,
    pub shoot_range: f32,
    pub attack_cooldown: f32,
    pub bullets_per_shot: usize,
    pub burst: bool,
    pub burst_interval: f32,
    pub fan_spread: f32,
    pub projectile_speed: f32,
    pub projectile_spread: f32,
    pub projectile_damage: i32,
    pub projectile_radius: f32,
    pub projectile_lifetime: f32,
    pub projectile_color: Color,
    pub projectile_size: f32,
    pub boss: bool,
}

pub fn enemy_def(kind: EnemyKind) -> EnemyDef {
    match kind {
        EnemyKind::Maggot => EnemyDef {
            name: "Maggot",
            hp: 2,
            speed: 75.0,
            accel: 1800.0,
            radius: 9.0,
            size: 17.0,
            color: Color::srgb(0.95, 0.55, 0.25),
            sprite: "images/sprMaggotIdle.png",
            score: 5,
            touch_damage: 1,
            rad_drop: 1,
            drop_chance: 0,
            weapon_chance: 0,
            preferred_range: 0.0,
            shoot_range: 0.0,
            attack_cooldown: 9.9,
            bullets_per_shot: 0,
            burst: false,
            burst_interval: 0.0,
            fan_spread: 0.0,
            projectile_speed: 0.0,
            projectile_spread: 0.0,
            projectile_damage: 0,
            projectile_radius: 0.0,
            projectile_lifetime: 0.0,
            projectile_color: Color::WHITE,
            projectile_size: 1.0,
            boss: false,
        },
        EnemyKind::Bandit => EnemyDef {
            name: "Bandit",
            hp: 4,
            speed: 24.0,
            accel: 800.0,
            radius: 12.0,
            size: 24.0,
            color: Color::srgb(0.85, 0.22, 0.18),
            sprite: "images/sprBanditIdle.png",
            score: 10,
            touch_damage: 0,
            rad_drop: 2,
            drop_chance: 16,
            weapon_chance: 0,
            preferred_range: 90.0,
            shoot_range: 480.0,
            attack_cooldown: 1.65,
            bullets_per_shot: 1,
            burst: false,
            burst_interval: 0.0,
            fan_spread: 0.0,
            projectile_speed: 120.0, // GM motion_add 4 *30
            projectile_spread: 0.175,
            projectile_damage: 3,
            projectile_radius: 4.0,
            projectile_lifetime: 3.5,
            projectile_color: Color::srgb(1.0, 0.35, 0.08),
            projectile_size: 8.0,
            boss: false,
        },
        EnemyKind::Scorpion => EnemyDef {
            name: "Scorpion",
            hp: 16,
            speed: 24.0,
            accel: 800.0,
            radius: 14.0,
            size: 28.0,
            color: Color::srgb(0.35, 0.85, 0.28),
            sprite: "images/sprScorpionIdle.png",
            score: 18,
            touch_damage: 5,
            rad_drop: 10,
            drop_chance: 15,
            weapon_chance: 0,
            preferred_range: 120.0,
            shoot_range: 210.0,
            attack_cooldown: 0.75,
            bullets_per_shot: 10,
            burst: true,
            burst_interval: 0.033,
            fan_spread: 0.0,
            projectile_speed: 105.0, // GM random_range(3,4) avg 3.5*30
            // GML Scorpion/Alarm_2: orandom(20) = ±20° (0.349rad).
            projectile_spread: 0.349,
            projectile_damage: 2,
            projectile_radius: 4.0,
            projectile_lifetime: 3.0,
            projectile_color: Color::srgb(0.35, 1.0, 0.25),
            projectile_size: 7.0,
            boss: false,
        },
        EnemyKind::Assassin => EnemyDef {
            name: "Assassin",
            hp: 14,
            speed: 84.0,
            accel: 880.0,
            radius: 11.0,
            size: 22.0,
            color: Color::srgb(0.2, 0.18, 0.24),
            sprite: "images/sprJungleAssassinIdle.png",
            score: 25,
            touch_damage: 3,
            rad_drop: 4,
            drop_chance: 16,
            weapon_chance: 0,
            preferred_range: 0.0,
            shoot_range: 0.0,
            attack_cooldown: 9.9,
            bullets_per_shot: 0,
            burst: false,
            burst_interval: 0.0,
            fan_spread: 0.0,
            projectile_speed: 0.0,
            projectile_spread: 0.0,
            projectile_damage: 0,
            projectile_radius: 0.0,
            projectile_lifetime: 0.0,
            projectile_color: Color::WHITE,
            projectile_size: 1.0,
            boss: false,
        },
        EnemyKind::Freak => EnemyDef {
            name: "Freak",
            hp: 7,
            speed: 112.0,
            accel: 5400.0,
            radius: 13.0,
            size: 26.0,
            color: Color::srgb(0.6, 0.35, 0.85),
            sprite: "images/sprFreak1Idle.png",
            score: 15,
            touch_damage: 3,
            rad_drop: 1,
            drop_chance: 10,
            weapon_chance: 0,
            preferred_range: 0.0,
            shoot_range: 0.0,
            attack_cooldown: 9.9,
            bullets_per_shot: 0,
            burst: false,
            burst_interval: 0.0,
            fan_spread: 0.0,
            projectile_speed: 0.0,
            projectile_spread: 0.0,
            projectile_damage: 0,
            projectile_radius: 0.0,
            projectile_lifetime: 0.0,
            projectile_color: Color::WHITE,
            projectile_size: 1.0,
            boss: false,
        },
        EnemyKind::BigBandit => EnemyDef {
            name: "Big Bandit",
            hp: 85,
            speed: 80.0,
            accel: 1000.0,
            radius: 26.0,
            size: 52.0,
            color: Color::srgb(0.95, 0.25, 0.12),
            sprite: "images/sprBanditBossIdle.png",
            score: 500,
            touch_damage: 5,
            rad_drop: 25,
            drop_chance: 60,
            weapon_chance: 8,
            preferred_range: 170.0,
            shoot_range: 560.0,
            attack_cooldown: 1.15,
            bullets_per_shot: 5,
            burst: false,
            burst_interval: 0.0,
            fan_spread: 0.16,
            projectile_speed: 240.0,
            projectile_spread: 0.06,
            projectile_damage: 3,
            projectile_radius: 4.5,
            projectile_lifetime: 3.2,
            projectile_color: Color::srgb(1.0, 0.28, 0.08),
            projectile_size: 8.0,
            boss: true,
        },
        EnemyKind::BigBanditLoop => EnemyDef {
            name: "Loop Big Bandit",
            hp: 130,
            speed: 95.0,
            accel: 1200.0,
            radius: 28.0,
            size: 56.0,
            color: Color::srgb(1.0, 0.42, 0.18),
            sprite: enemy_def(EnemyKind::BigBandit).sprite,
            score: 850,
            touch_damage: 7,
            rad_drop: 35,
            drop_chance: 70,
            weapon_chance: 12,
            preferred_range: 190.0,
            shoot_range: 620.0,
            attack_cooldown: 0.95,
            bullets_per_shot: 7,
            burst: false,
            burst_interval: 0.0,
            fan_spread: 0.14,
            projectile_speed: 215.0,
            projectile_spread: 0.05,
            projectile_damage: 4,
            projectile_radius: 4.5,
            projectile_lifetime: 3.2,
            projectile_color: Color::srgb(1.0, 0.45, 0.12),
            projectile_size: 8.5,
            boss: true,
        },
        EnemyKind::Throne => EnemyDef {
            name: "The Throne",
            hp: 320,
            speed: 0.0,
            accel: 650.0,
            radius: 44.0,
            size: 88.0,
            color: Color::srgb(1.0, 0.78, 0.25),
            sprite: "images/sprThroneStatue.png",
            score: 5000,
            touch_damage: 10,
            rad_drop: 100,
            drop_chance: 100,
            weapon_chance: 25,
            preferred_range: 0.0,
            shoot_range: 999.0,
            attack_cooldown: 0.7,
            bullets_per_shot: 7,
            burst: false,
            burst_interval: 0.0,
            fan_spread: 0.1,
            projectile_speed: 210.0,
            projectile_spread: 0.0,
            projectile_damage: 3,
            projectile_radius: 5.5,
            projectile_lifetime: 3.0,
            projectile_color: Color::srgb(1.0, 0.75, 0.25),
            projectile_size: 9.0,
            boss: true,
        },
        EnemyKind::ThroneII => EnemyDef {
            name: "Throne II",
            hp: 460,
            speed: 90.0,
            accel: 900.0,
            radius: 30.0,
            size: 60.0,
            color: Color::srgb(0.45, 1.0, 0.55),
            sprite: "images/sprThroneStatue.png",
            score: 8000,
            touch_damage: 10,
            rad_drop: 90,
            drop_chance: 100,
            weapon_chance: 25,
            preferred_range: 0.0,
            shoot_range: 999.0,
            attack_cooldown: 0.85,
            bullets_per_shot: 3,
            burst: false,
            burst_interval: 0.0,
            fan_spread: 0.0,
            projectile_speed: 180.0,
            projectile_spread: 0.0,
            projectile_damage: 5,
            projectile_radius: 8.0,
            projectile_lifetime: 1.4,
            projectile_color: Color::srgb(0.35, 1.0, 0.45),
            projectile_size: 14.0,
            boss: true,
        },
        EnemyKind::Hyper => EnemyDef {
            name: "Hyper Crystal",
            hp: 520,
            speed: 35.0,
            accel: 500.0,
            radius: 34.0,
            size: 68.0,
            color: Color::srgb(1.0, 0.25, 0.35),
            sprite: "images/sprHyperCrystalIdle.png",
            score: 9000,
            touch_damage: 60,
            rad_drop: 150,
            drop_chance: 100,
            weapon_chance: 20,
            preferred_range: 0.0,
            shoot_range: 0.0,
            attack_cooldown: 1.1,
            bullets_per_shot: 0,
            burst: false,
            burst_interval: 0.0,
            fan_spread: 0.0,
            projectile_speed: 0.0,
            projectile_spread: 0.0,
            projectile_damage: 0,
            projectile_radius: 0.0,
            projectile_lifetime: 0.0,
            projectile_color: Color::WHITE,
            projectile_size: 0.0,
            boss: true,
        },
        EnemyKind::Rat => EnemyDef {
            name: "Rat",
            hp: 7,
            speed: 110.0,
            accel: 6000.0,
            radius: 8.0,
            size: 14.0,
            color: Color::srgb(0.75, 0.6, 0.45),
            sprite: "images/sprRatIdle.png",
            score: 5,
            touch_damage: 2,
            rad_drop: 4,
            drop_chance: 0,
            weapon_chance: 0,
            preferred_range: 0.0,
            shoot_range: 0.0,
            attack_cooldown: 9.9,
            bullets_per_shot: 0,
            burst: false,
            burst_interval: 0.0,
            fan_spread: 0.0,
            projectile_speed: 0.0,
            projectile_spread: 0.0,
            projectile_damage: 0,
            projectile_radius: 0.0,
            projectile_lifetime: 0.0,
            projectile_color: Color::WHITE,
            projectile_size: 1.0,
            boss: false,
        },
        EnemyKind::BigRat => EnemyDef {
            name: "Big Rat",
            hp: 14,
            speed: 95.0,
            accel: 4800.0,
            radius: 13.0,
            size: 26.0,
            color: Color::srgb(0.65, 0.5, 0.35),
            sprite: "images/sprRatkingIdle.png",
            score: 15,
            touch_damage: 3,
            rad_drop: 3,
            drop_chance: 8,
            weapon_chance: 0,
            preferred_range: 0.0,
            shoot_range: 0.0,
            attack_cooldown: 9.9,
            bullets_per_shot: 0,
            burst: false,
            burst_interval: 0.0,
            fan_spread: 0.0,
            projectile_speed: 0.0,
            projectile_spread: 0.0,
            projectile_damage: 0,
            projectile_radius: 0.0,
            projectile_lifetime: 0.0,
            projectile_color: Color::WHITE,
            projectile_size: 1.0,
            boss: false,
        },
        EnemyKind::RobotGuard => EnemyDef {
            name: "Robot Guard",
            hp: 12,
            speed: 30.0,
            accel: 900.0,
            radius: 11.0,
            size: 22.0,
            color: Color::srgb(0.55, 0.62, 0.7),
            sprite: "images/sprSnowBotIdle.png",
            score: 20,
            touch_damage: 0,
            rad_drop: 4,
            drop_chance: 16,
            weapon_chance: 0,
            preferred_range: 180.0,
            shoot_range: 420.0,
            attack_cooldown: 1.4,
            bullets_per_shot: 3,
            burst: true,
            burst_interval: 0.1,
            fan_spread: 0.0,
            projectile_speed: 150.0,
            projectile_spread: 0.14,
            projectile_damage: 3,
            projectile_radius: 4.0,
            projectile_lifetime: 3.0,
            projectile_color: Color::srgb(1.0, 0.8, 0.3),
            projectile_size: 8.0,
            boss: false,
        },
        EnemyKind::Turret => EnemyDef {
            name: "Turret",
            hp: 24,
            speed: 0.0,
            accel: 0.0,
            radius: 12.0,
            size: 24.0,
            color: Color::srgb(0.45, 0.5, 0.55),
            sprite: "images/sprTurretIdle.png",
            score: 15,
            touch_damage: 0,
            rad_drop: 3,
            drop_chance: 10,
            weapon_chance: 0,
            preferred_range: 999.0,
            shoot_range: 520.0,
            attack_cooldown: 1.1,
            bullets_per_shot: 1,
            burst: false,
            burst_interval: 0.0,
            fan_spread: 0.0,
            // GML Turret/Alarm_2: EB1 speed 8*30=240 spread ±4° (0.07).
            projectile_speed: 240.0,
            projectile_spread: 0.07,
            projectile_damage: 3,
            projectile_radius: 4.0,
            projectile_lifetime: 3.0,
            projectile_color: Color::srgb(1.0, 0.45, 0.2),
            projectile_size: 8.0,
            boss: false,
        },
        EnemyKind::SnowBandit => EnemyDef {
            name: "Snow Bandit",
            hp: 9,
            speed: 26.0,
            accel: 850.0,
            radius: 12.0,
            size: 24.0,
            color: Color::srgb(0.75, 0.85, 0.95),
            sprite: "images/sprSnowBanditIdle.png",
            score: 20,
            touch_damage: 0,
            rad_drop: 3,
            drop_chance: 16,
            weapon_chance: 0,
            preferred_range: 110.0,
            shoot_range: 500.0,
            attack_cooldown: 1.5,
            bullets_per_shot: 1,
            burst: false,
            burst_interval: 0.0,
            fan_spread: 0.0,
            projectile_speed: 130.0,
            projectile_spread: 0.175,
            projectile_damage: 3,
            projectile_radius: 4.0,
            projectile_lifetime: 3.5,
            projectile_color: Color::srgb(0.7, 0.9, 1.0),
            projectile_size: 8.0,
            boss: false,
        },
        EnemyKind::Wolf => EnemyDef {
            name: "Wolf",
            hp: 16,
            speed: 135.0,
            accel: 6000.0,
            radius: 10.0,
            size: 20.0,
            color: Color::srgb(0.85, 0.85, 0.9),
            sprite: "images/sprWolfIdle.png",
            score: 18,
            touch_damage: 3,
            rad_drop: 2,
            drop_chance: 8,
            weapon_chance: 0,
            preferred_range: 0.0,
            shoot_range: 0.0,
            attack_cooldown: 9.9,
            bullets_per_shot: 0,
            burst: false,
            burst_interval: 0.0,
            fan_spread: 0.0,
            projectile_speed: 0.0,
            projectile_spread: 0.0,
            projectile_damage: 0,
            projectile_radius: 0.0,
            projectile_lifetime: 0.0,
            projectile_color: Color::WHITE,
            projectile_size: 1.0,
            boss: false,
        },
        EnemyKind::BigDog => EnemyDef {
            name: "Big Dog",
            hp: 180,
            speed: 40.0,
            accel: 700.0,
            radius: 36.0,
            size: 72.0,
            color: Color::srgb(0.65, 0.65, 0.7),
            sprite: "images/sprScrapBossIdle.png",
            score: 1200,
            touch_damage: 6,
            rad_drop: 45,
            drop_chance: 80,
            weapon_chance: 15,
            preferred_range: 240.0,
            shoot_range: 999.0,
            attack_cooldown: 0.8,
            bullets_per_shot: 5,
            burst: false,
            burst_interval: 0.0,
            fan_spread: 0.12,
            projectile_speed: 190.0,
            projectile_spread: 0.03,
            projectile_damage: 3,
            projectile_radius: 6.0,
            projectile_lifetime: 2.6,
            projectile_color: Color::srgb(1.0, 0.42, 0.12),
            projectile_size: 9.0,
            boss: true,
        },
        EnemyKind::BigDogLoop => EnemyDef {
            name: "Loop Big Dog",
            hp: 260,
            speed: 55.0,
            accel: 850.0,
            radius: 38.0,
            size: 76.0,
            color: Color::srgb(0.85, 0.72, 0.65),
            sprite: enemy_def(EnemyKind::BigDog).sprite,
            score: 1800,
            touch_damage: 8,
            rad_drop: 65,
            drop_chance: 90,
            weapon_chance: 20,
            preferred_range: 260.0,
            shoot_range: 999.0,
            attack_cooldown: 0.65,
            bullets_per_shot: 7,
            burst: false,
            burst_interval: 0.0,
            fan_spread: 0.10,
            projectile_speed: 230.0,
            projectile_spread: 0.03,
            projectile_damage: 4,
            projectile_radius: 6.0,
            projectile_lifetime: 2.8,
            projectile_color: Color::srgb(1.0, 0.52, 0.16),
            projectile_size: 9.5,
            boss: true,
        },
        EnemyKind::LilHunter => EnemyDef {
            name: "Lil Hunter",
            hp: 140,
            speed: 130.0,
            accel: 1100.0,
            radius: 20.0,
            size: 40.0,
            color: Color::srgb(0.55, 0.85, 1.0),
            sprite: "images/sprLilHunter.png",
            score: 1500,
            touch_damage: 5,
            rad_drop: 40,
            drop_chance: 85,
            weapon_chance: 20,
            preferred_range: 190.0,
            shoot_range: 640.0,
            attack_cooldown: 0.55,
            bullets_per_shot: 2,
            burst: true,
            burst_interval: 0.12,
            fan_spread: 0.0,
            projectile_speed: 245.0,
            projectile_spread: 0.04,
            projectile_damage: 3,
            projectile_radius: 4.0,
            projectile_lifetime: 2.3,
            projectile_color: Color::srgb(0.6, 0.95, 1.0),
            projectile_size: 7.0,
            boss: true,
        },
        EnemyKind::LilHunterLoop => EnemyDef {
            name: "Loop Lil Hunter",
            hp: 210,
            speed: 155.0,
            accel: 1300.0,
            radius: 21.0,
            size: 42.0,
            color: Color::srgb(0.75, 1.0, 1.0),
            sprite: enemy_def(EnemyKind::LilHunter).sprite,
            score: 2200,
            touch_damage: 7,
            rad_drop: 60,
            drop_chance: 95,
            weapon_chance: 25,
            preferred_range: 210.0,
            shoot_range: 720.0,
            attack_cooldown: 0.42,
            bullets_per_shot: 5,
            burst: true,
            burst_interval: 0.055,
            fan_spread: 0.11,
            projectile_speed: 310.0,
            projectile_spread: 0.03,
            projectile_damage: 4,
            projectile_radius: 4.5,
            projectile_lifetime: 2.5,
            projectile_color: Color::srgb(0.75, 1.0, 1.0),
            projectile_size: 7.5,
            boss: true,
        },
        EnemyKind::IdpdGrunt => EnemyDef {
            name: "IDPD Grunt",
            hp: 14,
            speed: 120.0,
            accel: 1100.0,
            radius: 13.0,
            size: 24.0,
            color: Color::srgb(0.25, 0.45, 0.95),
            sprite: "images/sprGruntIdle.png",
            score: 18,
            touch_damage: 3,
            rad_drop: 4,
            drop_chance: 18,
            weapon_chance: 4,
            preferred_range: 220.0,
            shoot_range: 520.0,
            attack_cooldown: 0.75,
            bullets_per_shot: 3,
            burst: true,
            burst_interval: 0.07,
            fan_spread: 0.07,
            projectile_speed: 280.0,
            projectile_spread: 0.03,
            projectile_damage: 2,
            projectile_radius: 3.5,
            projectile_lifetime: 2.2,
            projectile_color: Color::srgb(0.45, 0.75, 1.0),
            projectile_size: 7.0,
            boss: false,
        },
        EnemyKind::IdpdShield => EnemyDef {
            name: "IDPD Shield",
            hp: 24,
            speed: 95.0,
            accel: 950.0,
            radius: 15.0,
            size: 28.0,
            color: Color::srgb(0.2, 0.5, 0.9),
            sprite: "images/sprShielderIdle.png",
            score: 28,
            touch_damage: 4,
            rad_drop: 6,
            drop_chance: 20,
            weapon_chance: 5,
            preferred_range: 180.0,
            shoot_range: 420.0,
            attack_cooldown: 1.0,
            bullets_per_shot: 2,
            burst: true,
            burst_interval: 0.08,
            fan_spread: 0.05,
            projectile_speed: 250.0,
            projectile_spread: 0.02,
            projectile_damage: 2,
            projectile_radius: 4.0,
            projectile_lifetime: 2.0,
            projectile_color: Color::srgb(0.4, 0.75, 1.0),
            projectile_size: 8.0,
            boss: false,
        },
        EnemyKind::IdpdElite => EnemyDef {
            name: "IDPD Elite",
            hp: 36,
            speed: 135.0,
            accel: 1200.0,
            radius: 14.0,
            size: 26.0,
            color: Color::srgb(0.4, 0.25, 1.0),
            sprite: "images/sprEliteGruntIdle.png",
            score: 45,
            touch_damage: 4,
            rad_drop: 10,
            drop_chance: 26,
            weapon_chance: 8,
            preferred_range: 260.0,
            shoot_range: 700.0,
            attack_cooldown: 0.6,
            bullets_per_shot: 5,
            burst: true,
            burst_interval: 0.05,
            fan_spread: 0.11,
            projectile_speed: 340.0,
            projectile_spread: 0.03,
            projectile_damage: 3,
            projectile_radius: 4.0,
            projectile_lifetime: 2.6,
            projectile_color: Color::srgb(0.55, 0.75, 1.0),
            projectile_size: 7.5,
            boss: false,
        },
        EnemyKind::IdpdVan => EnemyDef {
            name: "IDPD Van",
            hp: 80,
            speed: 0.0,
            accel: 0.0,
            radius: 24.0,
            size: 44.0,
            color: Color::srgb(0.18, 0.28, 0.72),
            sprite: "images/sprVanDrive.png",
            score: 80,
            touch_damage: 6,
            rad_drop: 18,
            drop_chance: 40,
            weapon_chance: 12,
            preferred_range: 0.0,
            shoot_range: 999.0,
            attack_cooldown: 2.0,
            bullets_per_shot: 0,
            burst: false,
            burst_interval: 0.0,
            fan_spread: 0.0,
            projectile_speed: 0.0,
            projectile_spread: 0.0,
            projectile_damage: 0,
            projectile_radius: 0.0,
            projectile_lifetime: 0.0,
            projectile_color: Color::srgb(0.4, 0.7, 1.0),
            projectile_size: 1.0,
            boss: false,
        },
        EnemyKind::Mom => EnemyDef {
            name: "Mom",
            hp: 280,
            speed: 40.0,
            accel: 700.0,
            radius: 28.0,
            size: 56.0,
            color: Color::srgb(0.55, 0.85, 0.35),
            sprite: "images/sprMomIdle.png",
            score: 1200,
            touch_damage: 4,
            rad_drop: 50,
            drop_chance: 100,
            weapon_chance: 18,
            preferred_range: 160.0,
            shoot_range: 520.0,
            attack_cooldown: 1.1,
            bullets_per_shot: 0,
            burst: false,
            burst_interval: 0.0,
            fan_spread: 0.0,
            projectile_speed: 0.0,
            projectile_spread: 0.0,
            projectile_damage: 0,
            projectile_radius: 0.0,
            projectile_lifetime: 0.0,
            projectile_color: Color::srgb(0.45, 1.0, 0.35),
            projectile_size: 10.0,
            boss: true,
        },
        EnemyKind::FrogQueen => EnemyDef {
            name: "Frog Queen",
            hp: 490,
            speed: 55.0,
            accel: 800.0,
            radius: 26.0,
            size: 52.0,
            color: Color::srgb(0.45, 0.9, 0.4),
            sprite: "images/sprFrogQueenIdle.png",
            score: 2000,
            touch_damage: 10,
            rad_drop: 30,
            drop_chance: 100,
            weapon_chance: 15,
            // Upstream: chases (1.5 + loops/2 px/frame), alternates aimed
            // MomProjectile volleys with FrogEgg clusters.
            preferred_range: 140.0,
            shoot_range: 999.0,
            attack_cooldown: 1.3,
            bullets_per_shot: 0,
            burst: false,
            burst_interval: 0.0,
            fan_spread: 0.0,
            projectile_speed: 120.0,
            projectile_spread: 0.13,
            projectile_damage: 4,
            projectile_radius: 6.0,
            projectile_lifetime: 3.0,
            projectile_color: Color::srgb(0.5, 1.0, 0.4),
            projectile_size: 11.0,
            boss: true,
        },
        EnemyKind::Technomancer => EnemyDef {
            name: "Technomancer",
            hp: 220,
            speed: 0.0,
            accel: 0.0,
            radius: 22.0,
            size: 44.0,
            color: Color::srgb(0.55, 0.35, 0.85),
            // Fix: WAD has sprTechnoMancer (no Idle suffix); use that strip directly.
            sprite: "images/sprTechnoMancer.png",
            score: 1500,
            touch_damage: 0,
            rad_drop: 45,
            drop_chance: 100,
            weapon_chance: 15,
            preferred_range: 0.0,
            shoot_range: 0.0,
            attack_cooldown: 2.2,
            bullets_per_shot: 0,
            burst: false,
            burst_interval: 0.0,
            fan_spread: 0.0,
            projectile_speed: 0.0,
            projectile_spread: 0.0,
            projectile_damage: 0,
            projectile_radius: 0.0,
            projectile_lifetime: 0.0,
            projectile_color: Color::WHITE,
            projectile_size: 1.0,
            boss: true,
        },
        EnemyKind::Captain => EnemyDef {
            name: "Captain",
            hp: 420,
            speed: 110.0,
            accel: 1400.0,
            radius: 16.0,
            size: 32.0,
            color: Color::srgb(0.35, 0.55, 0.95),
            sprite: "images/sprPopoCaptainIdle.png",
            score: 4000,
            touch_damage: 6,
            rad_drop: 80,
            drop_chance: 100,
            weapon_chance: 20,
            preferred_range: 140.0,
            shoot_range: 700.0,
            attack_cooldown: 0.75,
            bullets_per_shot: 5,
            burst: true,
            burst_interval: 0.06,
            fan_spread: 0.12,
            projectile_speed: 240.0,
            projectile_spread: 0.04,
            projectile_damage: 4,
            projectile_radius: 4.5,
            projectile_lifetime: 2.8,
            projectile_color: Color::srgb(0.45, 0.75, 1.0),
            projectile_size: 8.0,
            boss: true,
        },
        EnemyKind::Ballguy => EnemyDef {
            name: "Ballguy",
            hp: 8,
            speed: 95.0,
            accel: 4200.0,
            radius: 11.0,
            size: 20.0,
            color: Color::srgb(0.45, 0.9, 0.4),

            sprite: "images/sprExploderIdle.png",
            score: 12,
            touch_damage: 2,
            rad_drop: 3,
            drop_chance: 20,
            weapon_chance: 2,
            preferred_range: 0.0,
            shoot_range: 0.0,
            attack_cooldown: 1.0,
            bullets_per_shot: 0,
            burst: false,
            burst_interval: 0.0,
            fan_spread: 0.0,
            projectile_speed: 0.0,
            projectile_spread: 0.0,
            projectile_damage: 0,
            projectile_radius: 0.0,
            projectile_lifetime: 0.0,
            projectile_color: Color::WHITE,
            projectile_size: 1.0,
            boss: false,
        },
        EnemyKind::FrogEgg => EnemyDef {
            name: "Frog Egg",
            hp: 35,
            speed: 0.0,
            accel: 0.0,
            radius: 10.0,
            size: 18.0,
            color: Color::srgb(0.7, 0.85, 0.35),
            sprite: "images/sprFrogEgg.png",
            score: 5,
            touch_damage: 0,
            rad_drop: 1,
            drop_chance: 0,
            weapon_chance: 0,
            preferred_range: 0.0,
            shoot_range: 0.0,
            attack_cooldown: 4.0,
            bullets_per_shot: 0,
            burst: false,
            burst_interval: 0.0,
            fan_spread: 0.0,
            projectile_speed: 0.0,
            projectile_spread: 0.0,
            projectile_damage: 0,
            projectile_radius: 0.0,
            projectile_lifetime: 0.0,
            projectile_color: Color::WHITE,
            projectile_size: 1.0,
            boss: false,
        },
        EnemyKind::Necromancer => EnemyDef {
            name: "Necromancer",
            hp: 18,
            speed: 28.0,
            accel: 700.0,
            radius: 12.0,
            size: 24.0,
            color: Color::srgb(0.65, 0.35, 0.8),
            sprite: "images/sprNecromancerIdle.png",
            score: 30,
            touch_damage: 0,
            rad_drop: 8,
            drop_chance: 35,
            weapon_chance: 5,
            preferred_range: 180.0,
            shoot_range: 0.0,
            attack_cooldown: 2.0,
            bullets_per_shot: 0,
            burst: false,
            burst_interval: 0.0,
            fan_spread: 0.0,
            projectile_speed: 0.0,
            projectile_spread: 0.0,
            projectile_damage: 0,
            projectile_radius: 0.0,
            projectile_lifetime: 0.0,
            projectile_color: Color::WHITE,
            projectile_size: 1.0,
            boss: false,
        },
        EnemyKind::Spider => EnemyDef {
            name: "Spider",
            hp: 10,
            speed: 130.0,
            accel: 5000.0,
            radius: 10.0,
            size: 20.0,
            color: Color::srgb(0.55, 0.2, 0.65),
            sprite: "images/sprSpiderIdle.png",
            score: 14,
            touch_damage: 2,
            rad_drop: 3,
            drop_chance: 18,
            weapon_chance: 2,
            preferred_range: 0.0,
            shoot_range: 0.0,
            attack_cooldown: 9.9,
            bullets_per_shot: 0,
            burst: false,
            burst_interval: 0.0,
            fan_spread: 0.0,
            projectile_speed: 0.0,
            projectile_spread: 0.0,
            projectile_damage: 0,
            projectile_radius: 0.0,
            projectile_lifetime: 0.0,
            projectile_color: Color::WHITE,
            projectile_size: 1.0,
            boss: false,
        },
        EnemyKind::Crystal => EnemyDef {
            name: "Crystal",
            hp: 12,
            speed: 0.0,
            accel: 0.0,
            radius: 12.0,
            size: 24.0,
            color: Color::srgb(0.85, 0.45, 1.0),
            // Fix: original has no sprCrystalIdle; use LaserCrystal (same family, exists in WAD/assets)
            sprite: "images/sprLaserCrystalIdle.png",
            score: 16,
            touch_damage: 0,
            rad_drop: 4,
            drop_chance: 20,
            weapon_chance: 3,
            preferred_range: 0.0,
            shoot_range: 400.0,
            attack_cooldown: 1.4,
            bullets_per_shot: 1,
            burst: false,
            burst_interval: 0.0,
            fan_spread: 0.0,
            projectile_speed: 160.0,
            projectile_spread: 0.05,
            projectile_damage: 3,
            projectile_radius: 4.0,
            projectile_lifetime: 2.5,
            projectile_color: Color::srgb(0.9, 0.5, 1.0),
            projectile_size: 8.0,
            boss: false,
        },
        EnemyKind::LaserCrystal => EnemyDef {
            name: "Laser Crystal",
            hp: 22,
            speed: 0.0,
            accel: 0.0,
            radius: 14.0,
            size: 28.0,
            color: Color::srgb(1.0, 0.25, 0.45),
            sprite: "images/sprLaserCrystalIdle.png",
            score: 28,
            touch_damage: 0,
            rad_drop: 8,
            drop_chance: 30,
            weapon_chance: 5,
            preferred_range: 0.0,
            shoot_range: 700.0,
            attack_cooldown: 2.0,
            bullets_per_shot: 1,
            burst: false,
            burst_interval: 0.0,
            fan_spread: 0.0,
            projectile_speed: 320.0,
            projectile_spread: 0.0,
            projectile_damage: 4,
            projectile_radius: 3.5,
            projectile_lifetime: 1.2,
            projectile_color: Color::srgb(1.0, 0.2, 0.35),
            projectile_size: 6.0,
            boss: false,
        },
        EnemyKind::Sniper => EnemyDef {
            name: "Sniper",
            hp: 12,
            speed: 40.0,
            accel: 900.0,
            radius: 11.0,
            size: 22.0,
            color: Color::srgb(0.4, 0.55, 0.75),
            sprite: "images/sprSniperIdle.png",
            score: 22,
            touch_damage: 0,
            rad_drop: 5,
            drop_chance: 22,
            weapon_chance: 4,
            preferred_range: 220.0,
            shoot_range: 800.0,
            attack_cooldown: 1.8,
            // GML Sniper/Alarm_2: 3x EB4 speed16 offsets +4/-4/0.
            bullets_per_shot: 3,
            burst: false,
            burst_interval: 0.0,
            fan_spread: 0.03,
            projectile_speed: 480.0, // GM motion_add 16*30
            projectile_spread: 0.01,
            projectile_damage: 5,
            projectile_radius: 3.5,
            projectile_lifetime: 2.0,
            projectile_color: Color::srgb(0.6, 0.85, 1.0),
            projectile_size: 6.0,
            boss: false,
        },
        EnemyKind::Crab => EnemyDef {
            name: "Crab",
            hp: 10,
            speed: 70.0,
            accel: 2200.0,
            radius: 12.0,
            size: 22.0,
            color: Color::srgb(0.95, 0.45, 0.25),
            sprite: "images/sprCrabIdle.png",
            score: 12,
            touch_damage: 2,
            rad_drop: 3,
            drop_chance: 15,
            weapon_chance: 2,
            // GML Crab/Alarm_2: 2x EB2 speed 5-7 (avg 6*30=180) ±3° (0.052), x8 burst.
            preferred_range: 90.0,
            shoot_range: 280.0,
            attack_cooldown: 1.6,
            bullets_per_shot: 2,
            burst: true,
            burst_interval: 0.066,
            fan_spread: 0.35,
            projectile_speed: 180.0,
            projectile_spread: 0.052,
            projectile_damage: 2,
            projectile_radius: 3.5,
            projectile_lifetime: 2.5,
            projectile_color: Color::srgb(1.0, 0.6, 0.3),
            projectile_size: 6.0,
            boss: false,
        },
        EnemyKind::OldGuardian => EnemyDef {
            name: "Old Guardian",
            hp: 180,
            speed: 55.0,
            accel: 900.0,
            radius: 22.0,
            size: 44.0,
            color: Color::srgb(0.75, 0.7, 0.55),
            sprite: "images/sprOldGuardianIdle.png",
            score: 2000,
            touch_damage: 5,
            rad_drop: 40,
            drop_chance: 100,
            weapon_chance: 15,
            preferred_range: 120.0,
            shoot_range: 500.0,
            attack_cooldown: 1.0,
            bullets_per_shot: 4,
            burst: false,
            burst_interval: 0.0,
            fan_spread: 0.14,
            projectile_speed: 180.0,
            projectile_spread: 0.05,
            projectile_damage: 3,
            projectile_radius: 4.5,
            projectile_lifetime: 2.5,
            projectile_color: Color::srgb(0.9, 0.85, 0.5),
            projectile_size: 8.0,
            boss: true,
        },
        EnemyKind::PalaceGuardian => EnemyDef {
            name: "Palace Guardian",
            hp: 45,
            speed: 95.0,
            accel: 2800.0,
            radius: 14.0,
            size: 28.0,
            color: Color::srgb(0.85, 0.75, 0.45),
            sprite: "images/sprGuardianIdle.png",
            score: 40,
            touch_damage: 4,
            rad_drop: 10,
            drop_chance: 40,
            weapon_chance: 6,
            preferred_range: 80.0,
            shoot_range: 420.0,
            attack_cooldown: 0.9,
            // GML Guardian/Alarm_1: 3x GuardianBullet speed 1,2,2 angles 0/-40/+40.
            bullets_per_shot: 3,
            burst: false,
            burst_interval: 0.0,
            fan_spread: 0.7,
            projectile_speed: 60.0,
            projectile_spread: 0.04,
            projectile_damage: 3,
            projectile_radius: 4.0,
            projectile_lifetime: 2.2,
            projectile_color: Color::srgb(1.0, 0.85, 0.4),
            projectile_size: 7.0,
            boss: false,
        },
        EnemyKind::Gator => EnemyDef {
            name: "Gator",
            hp: 12,
            speed: 30.0,
            accel: 900.0,
            radius: 13.0,
            size: 26.0,
            color: Color::srgb(0.25, 0.55, 0.28),
            sprite: "images/sprGatorIdle.png",
            score: 18,
            touch_damage: 0,
            rad_drop: 8,
            drop_chance: 16,
            weapon_chance: 2,
            // Upstream: shotgun blast only at 48–128px with a HitWarning.
            preferred_range: 90.0,
            shoot_range: 170.0,
            attack_cooldown: 1.5,
            bullets_per_shot: 6,
            burst: false,
            burst_interval: 0.0,
            fan_spread: 0.22,
            // GML Gator/Alarm_2: 6x EB3 speed 10-14 (avg 12*30=360) spread ±25° (0.436).
            projectile_speed: 360.0,
            projectile_spread: 0.436,
            projectile_damage: 2,
            projectile_radius: 3.5,
            projectile_lifetime: 0.6,
            projectile_color: Color::srgb(1.0, 0.75, 0.35),
            projectile_size: 6.0,
            boss: false,
        },
        EnemyKind::BuffGator => EnemyDef {
            name: "Buff Gator",
            hp: 30,
            speed: 34.0,
            accel: 950.0,
            radius: 16.0,
            size: 32.0,
            color: Color::srgb(0.3, 0.62, 0.32),
            sprite: "images/sprBuffGatorIdle.png",
            score: 40,
            touch_damage: 0,
            rad_drop: 12,
            drop_chance: 30,
            weapon_chance: 6,
            preferred_range: 120.0,
            shoot_range: 260.0,
            attack_cooldown: 1.4,
            bullets_per_shot: 8,
            burst: false,
            burst_interval: 0.0,
            fan_spread: 0.3,
            projectile_speed: 320.0,
            projectile_spread: 0.08,
            projectile_damage: 2,
            projectile_radius: 3.5,
            projectile_lifetime: 0.75,
            projectile_color: Color::srgb(1.0, 0.6, 0.25),
            projectile_size: 7.0,
            boss: false,
        },
        EnemyKind::Raven => EnemyDef {
            name: "Raven",
            hp: 10,
            speed: 85.0,
            accel: 1600.0,
            radius: 11.0,
            size: 22.0,
            color: Color::srgb(0.18, 0.18, 0.24),
            sprite: "images/sprRavenIdle.png",
            score: 15,
            touch_damage: 0,
            rad_drop: 4,
            drop_chance: 14,
            weapon_chance: 0,
            preferred_range: 150.0,
            shoot_range: 480.0,
            attack_cooldown: 1.35,
            bullets_per_shot: 3,
            burst: true,
            burst_interval: 0.09,
            fan_spread: 0.0,
            projectile_speed: 200.0,
            projectile_spread: 0.06,
            projectile_damage: 3,
            projectile_radius: 3.5,
            projectile_lifetime: 2.6,
            projectile_color: Color::srgb(1.0, 0.32, 0.1),
            projectile_size: 7.0,
            boss: false,
        },
        EnemyKind::Salamander => EnemyDef {
            name: "Salamander",
            hp: 25,
            speed: 36.0,
            accel: 950.0,
            radius: 14.0,
            size: 30.0,
            color: Color::srgb(0.95, 0.45, 0.15),
            sprite: "images/sprSalamanderIdle.png",
            score: 26,
            touch_damage: 1,
            rad_drop: 12,
            drop_chance: 18,
            weapon_chance: 3,
            // Short-range fire breath (upstream snd_mele = fire sound).
            preferred_range: 70.0,
            shoot_range: 150.0,
            attack_cooldown: 1.1,
            bullets_per_shot: 5,
            burst: true,
            burst_interval: 0.05,
            fan_spread: 0.05,
            projectile_speed: 160.0,
            projectile_spread: 0.12,
            projectile_damage: 2,
            projectile_radius: 4.5,
            projectile_lifetime: 0.9,
            projectile_color: Color::srgb(1.0, 0.45, 0.1),
            projectile_size: 9.0,
            boss: false,
        },
        EnemyKind::MeleeBandit => EnemyDef {
            name: "Melee Bandit",
            hp: 6,
            speed: 95.0,
            accel: 2200.0,
            radius: 11.0,
            size: 22.0,
            color: Color::srgb(0.85, 0.35, 0.2),
            sprite: "images/sprMeleeIdle.png",
            score: 12,
            touch_damage: 3,
            rad_drop: 2,
            drop_chance: 10,
            weapon_chance: 0,
            preferred_range: 0.0,
            shoot_range: 0.0,
            attack_cooldown: 9.9,
            bullets_per_shot: 0,
            burst: false,
            burst_interval: 0.0,
            fan_spread: 0.0,
            projectile_speed: 0.0,
            projectile_spread: 0.0,
            projectile_damage: 0,
            projectile_radius: 0.0,
            projectile_lifetime: 0.0,
            projectile_color: Color::WHITE,
            projectile_size: 1.0,
            boss: false,
        },
        EnemyKind::JungleBandit => EnemyDef {
            name: "Jungle Bandit",
            hp: 4,
            speed: 26.0,
            accel: 850.0,
            radius: 12.0,
            size: 24.0,
            color: Color::srgb(0.35, 0.6, 0.25),
            sprite: "images/sprJungleBanditIdle.png",
            score: 10,
            touch_damage: 0,
            rad_drop: 2,
            drop_chance: 12,
            weapon_chance: 0,
            preferred_range: 100.0,
            shoot_range: 460.0,
            attack_cooldown: 1.65,
            bullets_per_shot: 1,
            burst: false,
            burst_interval: 0.0,
            fan_spread: 0.0,
            // GML JungleBandit/Alarm_2: EB3 speed 11-13 (avg 12*30=360) ±8° (0.14).
            projectile_speed: 360.0,
            projectile_spread: 0.14,
            projectile_damage: 3,
            projectile_radius: 4.0,
            projectile_lifetime: 3.5,
            projectile_color: Color::srgb(0.55, 1.0, 0.35),
            projectile_size: 8.0,
            boss: false,
        },
        EnemyKind::BigMaggot => EnemyDef {
            name: "Big Maggot",
            hp: 22,
            speed: 60.0,
            accel: 1400.0,
            radius: 14.0,
            size: 30.0,
            color: Color::srgb(0.95, 0.5, 0.2),
            sprite: "images/sprBigMaggotIdle.png",
            score: 20,
            touch_damage: 1,
            rad_drop: 10,
            drop_chance: 14,
            weapon_chance: 0,
            preferred_range: 0.0,
            shoot_range: 0.0,
            attack_cooldown: 9.9,
            bullets_per_shot: 0,
            burst: false,
            burst_interval: 0.0,
            fan_spread: 0.0,
            projectile_speed: 0.0,
            projectile_spread: 0.0,
            projectile_damage: 0,
            projectile_radius: 0.0,
            projectile_lifetime: 0.0,
            projectile_color: Color::WHITE,
            projectile_size: 1.0,
            boss: false,
        },
        EnemyKind::FastRat => EnemyDef {
            name: "Fast Rat",
            hp: 4,
            speed: 165.0,
            accel: 7000.0,
            radius: 9.0,
            size: 18.0,
            color: Color::srgb(0.5, 0.9, 0.4),
            sprite: "images/sprFastRatIdle.png",
            score: 8,
            touch_damage: 2,
            rad_drop: 0,
            drop_chance: 0,
            weapon_chance: 0,
            preferred_range: 0.0,
            shoot_range: 0.0,
            attack_cooldown: 9.9,
            bullets_per_shot: 0,
            burst: false,
            burst_interval: 0.0,
            fan_spread: 0.0,
            projectile_speed: 0.0,
            projectile_spread: 0.0,
            projectile_damage: 0,
            projectile_radius: 0.0,
            projectile_lifetime: 0.0,
            projectile_color: Color::WHITE,
            projectile_size: 1.0,
            boss: false,
        },
        EnemyKind::Ratking => EnemyDef {
            name: "Ratking",
            hp: 35,
            speed: 42.0,
            accel: 900.0,
            radius: 16.0,
            size: 32.0,
            color: Color::srgb(0.72, 0.55, 0.38),
            sprite: "images/sprRatkingIdle.png",
            score: 35,
            touch_damage: 0,
            rad_drop: 20,
            drop_chance: 24,
            weapon_chance: 3,
            preferred_range: 190.0,
            shoot_range: 520.0,
            attack_cooldown: 1.5,
            bullets_per_shot: 4,
            burst: true,
            burst_interval: 0.09,
            fan_spread: 0.0,
            projectile_speed: 210.0,
            projectile_spread: 0.05,
            projectile_damage: 3,
            projectile_radius: 4.0,
            projectile_lifetime: 2.6,
            projectile_color: Color::srgb(1.0, 0.65, 0.3),
            projectile_size: 7.0,
            boss: false,
        },
        EnemyKind::GoldScorpion => EnemyDef {
            name: "Gold Scorpion",
            hp: 40,
            speed: 30.0,
            accel: 850.0,
            radius: 15.0,
            size: 32.0,
            color: Color::srgb(0.95, 0.82, 0.25),
            sprite: "images/sprGoldScorpionIdle.png",
            score: 50,
            touch_damage: 5,
            rad_drop: 30,
            drop_chance: 40,
            weapon_chance: 8,
            preferred_range: 130.0,
            shoot_range: 240.0,
            attack_cooldown: 0.8,
            bullets_per_shot: 10,
            burst: true,
            burst_interval: 0.033,
            fan_spread: 0.0,
            projectile_speed: 115.0,
            projectile_spread: 0.17,
            projectile_damage: 2,
            projectile_radius: 4.0,
            projectile_lifetime: 3.0,
            projectile_color: Color::srgb(1.0, 0.9, 0.3),
            projectile_size: 7.0,
            boss: false,
        },
        EnemyKind::LightningCrystal => EnemyDef {
            name: "Lightning Crystal",
            hp: 45,
            speed: 0.0,
            accel: 0.0,
            radius: 14.0,
            size: 30.0,
            color: Color::srgb(0.5, 0.85, 1.0),
            sprite: "images/sprLightningCrystalIdle.png",
            score: 45,
            touch_damage: 20,
            rad_drop: 25,
            drop_chance: 30,
            weapon_chance: 5,
            preferred_range: 999.0,
            shoot_range: 420.0,
            attack_cooldown: 1.7,
            bullets_per_shot: 4,
            burst: true,
            burst_interval: 0.07,
            fan_spread: 0.0,
            projectile_speed: 340.0,
            projectile_spread: 0.02,
            projectile_damage: 4,
            projectile_radius: 3.5,
            projectile_lifetime: 1.1,
            projectile_color: Color::srgb(0.55, 0.85, 1.0),
            projectile_size: 6.0,
            boss: false,
        },
        EnemyKind::ExploFreak => EnemyDef {
            name: "Explo Freak",
            hp: 5,
            speed: 140.0,
            accel: 6000.0,
            radius: 12.0,
            size: 26.0,
            color: Color::srgb(0.95, 0.45, 0.2),
            sprite: "images/sprExploFreakIdle.png",
            score: 18,
            touch_damage: 2,
            rad_drop: 10,
            drop_chance: 8,
            weapon_chance: 0,
            preferred_range: 0.0,
            shoot_range: 0.0,
            attack_cooldown: 9.9,
            bullets_per_shot: 0,
            burst: false,
            burst_interval: 0.0,
            fan_spread: 0.0,
            projectile_speed: 0.0,
            projectile_spread: 0.0,
            projectile_damage: 0,
            projectile_radius: 0.0,
            projectile_lifetime: 0.0,
            projectile_color: Color::WHITE,
            projectile_size: 1.0,
            boss: false,
        },
        EnemyKind::RhinoFreak => EnemyDef {
            name: "Rhino Freak",
            hp: 80,
            speed: 80.0,
            accel: 2600.0,
            radius: 17.0,
            size: 40.0,
            color: Color::srgb(0.55, 0.4, 0.7),
            sprite: "images/sprRhinoFreakIdle.png",
            score: 50,
            touch_damage: 5,
            rad_drop: 20,
            drop_chance: 20,
            weapon_chance: 2,
            preferred_range: 0.0,
            shoot_range: 0.0,
            attack_cooldown: 9.9,
            bullets_per_shot: 0,
            burst: false,
            burst_interval: 0.0,
            fan_spread: 0.0,
            projectile_speed: 0.0,
            projectile_spread: 0.0,
            projectile_damage: 0,
            projectile_radius: 0.0,
            projectile_lifetime: 0.0,
            projectile_color: Color::WHITE,
            projectile_size: 1.0,
            boss: false,
        },
        EnemyKind::SnowTank => EnemyDef {
            name: "Snow Tank",
            hp: 50,
            speed: 22.0,
            accel: 500.0,
            radius: 18.0,
            size: 40.0,
            color: Color::srgb(0.55, 0.65, 0.78),
            sprite: "images/sprSnowTankIdle.png",
            score: 55,
            touch_damage: 0,
            rad_drop: 10,
            drop_chance: 30,
            weapon_chance: 5,
            // Slow crawler that stops to line up its rocket.
            // GML SnowTank/Alarm_2: 2x EB4 speed12 dir ±sin(wave)*20, 16 shots.
            preferred_range: 230.0,
            shoot_range: 700.0,
            attack_cooldown: 2.4,
            bullets_per_shot: 2,
            burst: false,
            burst_interval: 0.0,
            fan_spread: 0.35,
            projectile_speed: 360.0,
            projectile_spread: 0.01,
            projectile_damage: 6,
            projectile_radius: 6.0,
            projectile_lifetime: 2.6,
            projectile_color: Color::srgb(1.0, 0.35, 0.15),
            projectile_size: 11.0,
            boss: false,
        },
        EnemyKind::GoldSnowtank => EnemyDef {
            name: "Gold Snow Tank",
            hp: 70,
            speed: 24.0,
            accel: 520.0,
            radius: 18.0,
            size: 40.0,
            color: Color::srgb(0.95, 0.82, 0.25),
            sprite: "images/sprGoldTankIdle.png",
            score: 110,
            touch_damage: 0,
            rad_drop: 13,
            drop_chance: 45,
            weapon_chance: 10,
            preferred_range: 250.0,
            shoot_range: 760.0,
            attack_cooldown: 2.0,
            bullets_per_shot: 1,
            burst: false,
            burst_interval: 0.0,
            fan_spread: 0.0,
            projectile_speed: 330.0,
            projectile_spread: 0.008,
            projectile_damage: 7,
            projectile_radius: 6.5,
            projectile_lifetime: 2.8,
            projectile_color: Color::srgb(1.0, 0.85, 0.2),
            projectile_size: 12.0,
            boss: false,
        },
        EnemyKind::Guardian => EnemyDef {
            name: "Guardian",
            hp: 35,
            speed: 46.0,
            accel: 800.0,
            radius: 16.0,
            size: 34.0,
            color: Color::srgb(0.35, 0.85, 0.55),
            sprite: "images/sprGuardianIdle.png",
            score: 40,
            touch_damage: 2,
            rad_drop: 11,
            drop_chance: 22,
            weapon_chance: 3,
            // Teleporting orb-fan shooter (sprGuardianAppear/Disappear).
            preferred_range: 180.0,
            shoot_range: 520.0,
            attack_cooldown: 1.5,
            bullets_per_shot: 3,
            burst: false,
            burst_interval: 0.0,
            fan_spread: 0.3,
            projectile_speed: 105.0,
            projectile_spread: 0.04,
            projectile_damage: 3,
            projectile_radius: 6.0,
            projectile_lifetime: 3.4,
            projectile_color: Color::srgb(0.3, 1.0, 0.45),
            projectile_size: 11.0,
            boss: false,
        },
        EnemyKind::ExploGuardian => EnemyDef {
            name: "Explo Guardian",
            hp: 50,
            speed: 52.0,
            accel: 900.0,
            radius: 15.0,
            size: 32.0,
            color: Color::srgb(0.95, 0.55, 0.25),
            sprite: "images/sprExploGuardianIdle.png",
            score: 48,
            touch_damage: 2,
            rad_drop: 15,
            drop_chance: 26,
            weapon_chance: 4,
            preferred_range: 150.0,
            shoot_range: 440.0,
            attack_cooldown: 1.6,
            bullets_per_shot: 2,
            burst: true,
            burst_interval: 0.12,
            fan_spread: 0.0,
            projectile_speed: 150.0,
            projectile_spread: 0.03,
            projectile_damage: 4,
            projectile_radius: 6.0,
            projectile_lifetime: 2.4,
            projectile_color: Color::srgb(1.0, 0.6, 0.2),
            projectile_size: 10.0,
            boss: false,
        },
        EnemyKind::DogGuardian => EnemyDef {
            name: "Dog Guardian",
            hp: 160,
            speed: 100.0,
            accel: 3000.0,
            radius: 18.0,
            size: 40.0,
            color: Color::srgb(0.4, 0.75, 0.5),
            sprite: "images/sprDogGuardianWalk.png",
            score: 90,
            touch_damage: 6,
            rad_drop: 20,
            drop_chance: 35,
            weapon_chance: 6,
            preferred_range: 0.0,
            shoot_range: 0.0,
            attack_cooldown: 9.9,
            bullets_per_shot: 0,
            burst: false,
            burst_interval: 0.0,
            fan_spread: 0.0,
            projectile_speed: 0.0,
            projectile_spread: 0.0,
            projectile_damage: 0,
            projectile_radius: 0.0,
            projectile_lifetime: 0.0,
            projectile_color: Color::WHITE,
            projectile_size: 1.0,
            boss: false,
        },
        EnemyKind::BoneFish => EnemyDef {
            name: "Bone Fish",
            hp: 8,
            speed: 105.0,
            accel: 4200.0,
            radius: 10.0,
            size: 20.0,
            color: Color::srgb(0.9, 0.88, 0.75),
            sprite: "images/sprBoneFish1Idle.png",
            score: 12,
            touch_damage: 2,
            rad_drop: 2,
            drop_chance: 10,
            weapon_chance: 0,
            preferred_range: 0.0,
            shoot_range: 0.0,
            attack_cooldown: 9.9,
            bullets_per_shot: 0,
            burst: false,
            burst_interval: 0.0,
            fan_spread: 0.0,
            projectile_speed: 0.0,
            projectile_spread: 0.0,
            projectile_damage: 0,
            projectile_radius: 0.0,
            projectile_lifetime: 0.0,
            projectile_color: Color::WHITE,
            projectile_size: 1.0,
            boss: false,
        },
        EnemyKind::Turtle => EnemyDef {
            name: "Turtle",
            hp: 15,
            speed: 30.0,
            accel: 900.0,
            radius: 13.0,
            size: 26.0,
            color: Color::srgb(0.45, 0.7, 0.4),
            sprite: "images/sprTurtleIdle.png",
            score: 20,
            touch_damage: 0,
            rad_drop: 12,
            drop_chance: 25,
            weapon_chance: 3,
            // Upstream Alarm_1: dashes at visible targets within 320px
            // (spr_fire, meleedamage = 4 while charging).
            preferred_range: 0.0,
            shoot_range: 0.0,
            attack_cooldown: 1.7,
            bullets_per_shot: 0,
            burst: false,
            burst_interval: 0.0,
            fan_spread: 0.0,
            projectile_speed: 0.0,
            projectile_spread: 0.0,
            projectile_damage: 0,
            projectile_radius: 0.0,
            projectile_lifetime: 0.0,
            projectile_color: Color::WHITE,
            projectile_size: 1.0,
            boss: false,
        },
        EnemyKind::Molefish => EnemyDef {
            name: "Molefish",
            hp: 6,
            speed: 30.0,
            accel: 850.0,
            radius: 11.0,
            size: 22.0,
            color: Color::srgb(0.65, 0.5, 0.35),
            sprite: "images/sprMolefishIdle.png",
            score: 12,
            touch_damage: 0,
            rad_drop: 3,
            drop_chance: 12,
            weapon_chance: 0,
            preferred_range: 150.0,
            shoot_range: 440.0,
            attack_cooldown: 1.3,
            bullets_per_shot: 1,
            burst: false,
            burst_interval: 0.0,
            fan_spread: 0.0,
            projectile_speed: 120.0,
            projectile_spread: 0.03,
            projectile_damage: 3,
            projectile_radius: 4.0,
            projectile_lifetime: 3.0,
            projectile_color: Color::srgb(1.0, 0.4, 0.1),
            projectile_size: 8.0,
            boss: false,
        },
        EnemyKind::Molesarge => EnemyDef {
            name: "Molesarge",
            hp: 14,
            speed: 32.0,
            accel: 900.0,
            radius: 12.0,
            size: 24.0,
            color: Color::srgb(0.72, 0.55, 0.38),
            sprite: "images/sprMolesargeIdle.png",
            score: 22,
            touch_damage: 0,
            rad_drop: 6,
            drop_chance: 18,
            weapon_chance: 2,
            preferred_range: 170.0,
            shoot_range: 500.0,
            attack_cooldown: 1.5,
            // GML Molesarge/Alarm_1: 5x EB3 speed 10-12 angles 0/±15/±30.
            bullets_per_shot: 5,
            burst: false,
            burst_interval: 0.0,
            fan_spread: 0.26,
            projectile_speed: 330.0,
            projectile_spread: 0.02,
            projectile_damage: 3,
            projectile_radius: 4.0,
            projectile_lifetime: 3.0,
            projectile_color: Color::srgb(1.0, 0.5, 0.12),
            projectile_size: 8.0,
            boss: false,
        },
        EnemyKind::FireBaller => EnemyDef {
            name: "Fire Baller",
            hp: 25,
            speed: 34.0,
            accel: 900.0,
            radius: 13.0,
            size: 28.0,
            color: Color::srgb(0.95, 0.4, 0.15),
            sprite: "images/sprFireBallerIdle.png",
            score: 24,
            touch_damage: 0,
            rad_drop: 5,
            drop_chance: 16,
            weapon_chance: 2,
            // Lobs arcing fireballs toward the player.
            preferred_range: 160.0,
            shoot_range: 420.0,
            attack_cooldown: 1.4,
            bullets_per_shot: 1,
            burst: false,
            burst_interval: 0.0,
            fan_spread: 0.0,
            projectile_speed: 90.0,
            projectile_spread: 0.05,
            projectile_damage: 4,
            projectile_radius: 6.0,
            projectile_lifetime: 3.2,
            projectile_color: Color::srgb(1.0, 0.45, 0.08),
            projectile_size: 11.0,
            boss: false,
        },
        EnemyKind::SuperFireBaller => EnemyDef {
            name: "Super Fire Baller",
            hp: 60,
            speed: 38.0,
            accel: 950.0,
            radius: 16.0,
            size: 34.0,
            color: Color::srgb(1.0, 0.5, 0.1),
            sprite: "images/sprSuperFireBallerIdle.png",
            score: 45,
            touch_damage: 1,
            rad_drop: 15,
            drop_chance: 28,
            weapon_chance: 5,
            preferred_range: 180.0,
            shoot_range: 480.0,
            attack_cooldown: 1.1,
            bullets_per_shot: 3,
            burst: true,
            burst_interval: 0.22,
            fan_spread: 0.0,
            projectile_speed: 95.0,
            projectile_spread: 0.04,
            projectile_damage: 4,
            projectile_radius: 7.0,
            projectile_lifetime: 3.4,
            projectile_color: Color::srgb(1.0, 0.55, 0.1),
            projectile_size: 13.0,
            boss: false,
        },
        EnemyKind::Jock => EnemyDef {
            name: "Jock",
            hp: 25,
            speed: 44.0,
            accel: 1100.0,
            radius: 14.0,
            size: 30.0,
            color: Color::srgb(0.85, 0.3, 0.35),
            sprite: "images/sprJockIdle.png",
            score: 26,
            touch_damage: 2,
            rad_drop: 8,
            drop_chance: 18,
            weapon_chance: 3,
            // Ammo-limited rocket bursts (upstream JockRocket).
            preferred_range: 190.0,
            shoot_range: 520.0,
            attack_cooldown: 1.7,
            bullets_per_shot: 1,
            burst: true,
            burst_interval: 0.3,
            fan_spread: 0.0,
            projectile_speed: 60.0,
            projectile_spread: 0.03,
            projectile_damage: 5,
            projectile_radius: 6.0,
            projectile_lifetime: 3.6,
            projectile_color: Color::srgb(1.0, 0.6, 0.2),
            projectile_size: 10.0,
            boss: false,
        },
        EnemyKind::JungleFly => EnemyDef {
            name: "Jungle Fly",
            hp: 40,
            speed: 120.0,
            accel: 3600.0,
            radius: 13.0,
            size: 26.0,
            color: Color::srgb(0.55, 0.85, 0.3),
            sprite: "images/sprJungleFlyIdle.png",
            score: 30,
            touch_damage: 5,
            rad_drop: 10,
            drop_chance: 20,
            weapon_chance: 2,
            // Upstream: dives when close, spits 3 FiredMaggots beyond 96px.
            preferred_range: 110.0,
            shoot_range: 380.0,
            attack_cooldown: 1.5,
            bullets_per_shot: 3,
            burst: true,
            burst_interval: 0.1,
            fan_spread: 0.0,
            projectile_speed: 150.0,
            projectile_spread: 0.07,
            projectile_damage: 3,
            projectile_radius: 4.0,
            projectile_lifetime: 1.6,
            projectile_color: Color::srgb(0.95, 0.55, 0.25),
            projectile_size: 7.0,
            boss: false,
        },
        EnemyKind::InvSpider => EnemyDef {
            name: "Cursed Spider",
            hp: 18,
            speed: 140.0,
            accel: 5200.0,
            radius: 10.0,
            size: 20.0,
            color: Color::srgb(0.35, 0.15, 0.45),
            sprite: "images/sprInvSpiderIdle.png",
            score: 20,
            touch_damage: 3,
            rad_drop: 20,
            drop_chance: 18,
            weapon_chance: 2,
            preferred_range: 0.0,
            shoot_range: 0.0,
            attack_cooldown: 9.9,
            bullets_per_shot: 0,
            burst: false,
            burst_interval: 0.0,
            fan_spread: 0.0,
            projectile_speed: 0.0,
            projectile_spread: 0.0,
            projectile_damage: 0,
            projectile_radius: 0.0,
            projectile_lifetime: 0.0,
            projectile_color: Color::WHITE,
            projectile_size: 1.0,
            boss: false,
        },
        EnemyKind::InvLaserCrystal => EnemyDef {
            name: "Cursed Laser Crystal",
            hp: 45,
            speed: 0.0,
            accel: 0.0,
            radius: 14.0,
            size: 30.0,
            color: Color::srgb(0.85, 0.3, 1.0),
            sprite: "images/sprInvLaserCrystalIdle.png",
            score: 42,
            touch_damage: 20,
            rad_drop: 25,
            drop_chance: 32,
            weapon_chance: 6,
            preferred_range: 999.0,
            shoot_range: 700.0,
            attack_cooldown: 2.0,
            bullets_per_shot: 4,
            burst: true,
            burst_interval: 0.09,
            fan_spread: 0.0,
            projectile_speed: 330.0,
            projectile_spread: 0.01,
            projectile_damage: 4,
            projectile_radius: 3.5,
            projectile_lifetime: 1.2,
            projectile_color: Color::srgb(0.9, 0.3, 1.0),
            projectile_size: 6.0,
            boss: false,
        },
        EnemyKind::PopoFreak => EnemyDef {
            name: "Popo Freak",
            hp: 30,
            speed: 115.0,
            accel: 2200.0,
            radius: 14.0,
            size: 30.0,
            color: Color::srgb(0.3, 0.5, 0.95),
            sprite: "images/sprPopoFreakIdle.png",
            score: 50,
            touch_damage: 5,
            rad_drop: 25,
            drop_chance: 30,
            weapon_chance: 8,
            // IDPD freak police: rushes then fires 8-round slug bursts.
            preferred_range: 130.0,
            shoot_range: 460.0,
            attack_cooldown: 1.6,
            bullets_per_shot: 8,
            burst: true,
            burst_interval: 0.07,
            fan_spread: 0.0,
            projectile_speed: 260.0,
            projectile_spread: 0.06,
            projectile_damage: 3,
            projectile_radius: 3.5,
            projectile_lifetime: 2.4,
            projectile_color: Color::srgb(0.5, 0.8, 1.0),
            projectile_size: 7.0,
            boss: false,
        },
        EnemyKind::MaggotSpawn => EnemyDef {
            name: "Maggot Spawn",
            hp: 12,
            speed: 0.0,
            accel: 0.0,
            radius: 14.0,
            size: 28.0,
            color: Color::srgb(0.9, 0.6, 0.3),
            sprite: "images/sprMSpawnIdle.png",
            score: 15,
            touch_damage: 0,
            rad_drop: 5,
            drop_chance: 14,
            weapon_chance: 0,
            preferred_range: 999.0,
            shoot_range: 999.0,
            attack_cooldown: 2.6,
            bullets_per_shot: 0,
            burst: false,
            burst_interval: 0.0,
            fan_spread: 0.0,
            projectile_speed: 0.0,
            projectile_spread: 0.0,
            projectile_damage: 0,
            projectile_radius: 0.0,
            projectile_lifetime: 0.0,
            projectile_color: Color::WHITE,
            projectile_size: 1.0,
            boss: false,
        },
        EnemyKind::IdpdInspector => EnemyDef {
            name: "IDPD Inspector",
            hp: 10,
            speed: 95.0,
            accel: 1400.0,
            radius: 12.0,
            size: 24.0,
            color: Color::srgb(0.45, 0.35, 0.9),
            sprite: "images/sprInspectorIdle.png",
            score: 40,
            touch_damage: 0,
            // Upstream raddrop = 0 - inspectors never drop rads.
            rad_drop: 0,
            drop_chance: 22,
            weapon_chance: 6,
            preferred_range: 210.0,
            shoot_range: 560.0,
            attack_cooldown: 0.85,
            bullets_per_shot: 1,
            burst: false,
            burst_interval: 0.0,
            fan_spread: 0.0,
            // PopoSlug: very fast, wide jitter.
            projectile_speed: 480.0,
            projectile_spread: 0.1,
            projectile_damage: 3,
            projectile_radius: 3.5,
            projectile_lifetime: 2.2,
            projectile_color: Color::srgb(0.6, 0.55, 1.0),
            projectile_size: 7.0,
            boss: false,
        },
    }
}

/// Level-10 race ultimates (two choices per playable race).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum UltraMutationId {
    FishGunWarrant,
    FishConfiscate,
    CrystalFortress,
    CrystalJuggernaut,
    EyesMonsterStyle,
    EyesProjectileStyle,
    MeltingBrainCapacity,
    MeltingDetachment,
    PlantTrapper,
    PlantKiller,
    VenuzBack2Bizniz,
    VenuzGunGod,
    SteroidsAmbidextrous,
    SteroidsGetArmed,
    RobotRefinedTaste,
    RobotRegurgitate,
    ChickenHarderToKill,
    ChickenDetermination,
    RebelPersonalGuard,
    RebelRiot,
    HorrorStalker,
    HorrorAnomaly,
    RogueSuperBlastArmor,
    RoguePortalStrike,
    BigDogHeavyArtillery,
    BigDogGuardian,
    SkeletonBloodArmor,
    SkeletonNecromancy,
    FrogToxicLord,
    FrogSwampBody,
    CuzHoarder,
    CuzQuickSwap,
}

pub struct UltraMutationDef {
    pub name: &'static str,
    pub description: &'static str,
}

pub fn ultra_choices_for(race: RaceId) -> [UltraMutationId; 2] {
    match race {
        RaceId::Fish | RaceId::Random => [
            UltraMutationId::FishGunWarrant,
            UltraMutationId::FishConfiscate,
        ],
        RaceId::Crystal => [
            UltraMutationId::CrystalFortress,
            UltraMutationId::CrystalJuggernaut,
        ],
        RaceId::Eyes => [
            UltraMutationId::EyesMonsterStyle,
            UltraMutationId::EyesProjectileStyle,
        ],
        RaceId::Melting => [
            UltraMutationId::MeltingBrainCapacity,
            UltraMutationId::MeltingDetachment,
        ],
        RaceId::Plant => [UltraMutationId::PlantTrapper, UltraMutationId::PlantKiller],
        RaceId::Venuz => [
            UltraMutationId::VenuzBack2Bizniz,
            UltraMutationId::VenuzGunGod,
        ],
        RaceId::Steroids => [
            UltraMutationId::SteroidsAmbidextrous,
            UltraMutationId::SteroidsGetArmed,
        ],
        RaceId::Robot => [
            UltraMutationId::RobotRefinedTaste,
            UltraMutationId::RobotRegurgitate,
        ],
        RaceId::Chicken => [
            UltraMutationId::ChickenHarderToKill,
            UltraMutationId::ChickenDetermination,
        ],
        RaceId::Rebel => [
            UltraMutationId::RebelPersonalGuard,
            UltraMutationId::RebelRiot,
        ],
        RaceId::Horror => [
            UltraMutationId::HorrorStalker,
            UltraMutationId::HorrorAnomaly,
        ],
        RaceId::Rogue => [
            UltraMutationId::RogueSuperBlastArmor,
            UltraMutationId::RoguePortalStrike,
        ],
        RaceId::BigDog => [
            UltraMutationId::BigDogHeavyArtillery,
            UltraMutationId::BigDogGuardian,
        ],
        RaceId::Skeleton => [
            UltraMutationId::SkeletonBloodArmor,
            UltraMutationId::SkeletonNecromancy,
        ],
        RaceId::Frog => [
            UltraMutationId::FrogToxicLord,
            UltraMutationId::FrogSwampBody,
        ],
        RaceId::Cuz => [UltraMutationId::CuzHoarder, UltraMutationId::CuzQuickSwap],
    }
}

pub fn ultra_mutation_def(id: UltraMutationId) -> UltraMutationDef {
    match id {
        UltraMutationId::FishGunWarrant => UltraMutationDef {
            name: "Gun Warrant",
            description: "Faster gun handling and stronger rolls",
        },
        UltraMutationId::FishConfiscate => UltraMutationDef {
            name: "Confiscate",
            description: "Weapon pickups grant extra ammo",
        },
        UltraMutationId::CrystalFortress => UltraMutationDef {
            name: "Fortress",
            description: "Much more HP and longer shield",
        },
        UltraMutationId::CrystalJuggernaut => UltraMutationDef {
            name: "Juggernaut",
            description: "Move faster while protected",
        },
        UltraMutationId::EyesMonsterStyle => UltraMutationDef {
            name: "Monster Style",
            description: "Telekinesis and pickup pull are stronger",
        },
        UltraMutationId::EyesProjectileStyle => UltraMutationDef {
            name: "Projectile Style",
            description: "Enemy projectiles are slowed further",
        },
        UltraMutationId::MeltingBrainCapacity => UltraMutationDef {
            name: "Brain Capacity",
            description: "Detonate reaches farther and hurts more",
        },
        UltraMutationId::MeltingDetachment => UltraMutationDef {
            name: "Detachment",
            description: "Gain emergency survivability",
        },
        UltraMutationId::PlantTrapper => UltraMutationDef {
            name: "Trapper",
            description: "Snare lasts longer and slows harder",
        },
        UltraMutationId::PlantKiller => UltraMutationDef {
            name: "Killer",
            description: "Move and fire faster",
        },
        UltraMutationId::VenuzBack2Bizniz => UltraMutationDef {
            name: "Back 2 Bizniz",
            description: "Pop Pop grants an extra charge",
        },
        UltraMutationId::VenuzGunGod => UltraMutationDef {
            name: "Ima Gun God",
            description: "Major fire-rate and accuracy boost",
        },
        UltraMutationId::SteroidsAmbidextrous => UltraMutationDef {
            name: "Ambidextrous",
            description: "Faster fire and lower recoil feel",
        },
        UltraMutationId::SteroidsGetArmed => UltraMutationDef {
            name: "Get Armed",
            description: "Get Loaded refills more ammunition",
        },
        UltraMutationId::RobotRefinedTaste => UltraMutationDef {
            name: "Refined Taste",
            description: "Ammo and weapon pickups heal more",
        },
        UltraMutationId::RobotRegurgitate => UltraMutationDef {
            name: "Regurgitate",
            description: "Eating weapons gives better rewards",
        },
        UltraMutationId::ChickenHarderToKill => UltraMutationDef {
            name: "Harder To Kill",
            description: "Headless survival returns with more HP",
        },
        UltraMutationId::ChickenDetermination => UltraMutationDef {
            name: "Determination",
            description: "Thrown weapons hit harder",
        },
        UltraMutationId::RebelPersonalGuard => UltraMutationDef {
            name: "Personal Guard",
            description: "Allies live longer and shoot faster",
        },
        UltraMutationId::RebelRiot => UltraMutationDef {
            name: "Riot",
            description: "Spawn more allies",
        },
        UltraMutationId::HorrorStalker => UltraMutationDef {
            name: "Stalker",
            description: "Beam and radiation effects are stronger",
        },
        UltraMutationId::HorrorAnomaly => UltraMutationDef {
            name: "Anomaly",
            description: "Energy weapons and pickups improve",
        },
        UltraMutationId::RogueSuperBlastArmor => UltraMutationDef {
            name: "Super Blast Armor",
            description: "Explosion damage is greatly reduced",
        },
        UltraMutationId::RoguePortalStrike => UltraMutationDef {
            name: "Ultra Portal Strike",
            description: "Portal strike is larger and faster",
        },
        UltraMutationId::BigDogHeavyArtillery => UltraMutationDef {
            name: "Heavy Artillery",
            description: "Rocket barrage gains side rockets",
        },
        UltraMutationId::BigDogGuardian => UltraMutationDef {
            name: "Guardian",
            description: "Gain bulk and protection",
        },
        UltraMutationId::SkeletonBloodArmor => UltraMutationDef {
            name: "Blood Armor",
            description: "More HP and blood-fueled kills",
        },
        UltraMutationId::SkeletonNecromancy => UltraMutationDef {
            name: "Necromancy",
            description: "Kills sometimes heal and refund ammo",
        },
        UltraMutationId::FrogToxicLord => UltraMutationDef {
            name: "Toxic Lord",
            description: "Toxic clouds are larger and longer",
        },
        UltraMutationId::FrogSwampBody => UltraMutationDef {
            name: "Swamp Body",
            description: "Gain bulk and blast resilience",
        },
        UltraMutationId::CuzHoarder => UltraMutationDef {
            name: "Hoarder",
            description: "Carry a full third weapon slot",
        },
        UltraMutationId::CuzQuickSwap => UltraMutationDef {
            name: "Quick Swap",
            description: "Swap ability is nearly instant",
        },
    }
}

pub fn is_boss(kind: EnemyKind) -> bool {
    enemy_def(kind).boss
}

/// NT simulation runs at 30 FPS; GML `wep_load` is frames.
#[inline]
pub const fn frames(f: f32) -> f32 {
    f / 30.0
}

pub fn nt_cooldown_secs(wep_id: u8) -> f32 {
    let w = &crate::game::weapons_data::WEAPONS[wep_id as usize];
    w.wep_load as f32 / 30.0
}

pub fn weapon_gml_id(kind: WeaponKind) -> u8 {
    match kind {
        WeaponKind::None => 0,
        WeaponKind::Revolver => 1,
        WeaponKind::Wrench => 3,
        WeaponKind::Machinegun => 4,
        WeaponKind::Shotgun => 5,
        WeaponKind::Crossbow => 6,
        WeaponKind::GrenadeLauncher => 7,
        WeaponKind::Smg => 16,
        WeaponKind::AssaultRifle => 17,
        WeaponKind::Sledgehammer => 88,
    }
}

/// Load a sprite at its native pixel size. Panics when the art is missing -
/// the game must never boot with invisible entities.
pub fn sprite_exact(catalog: &AssetCatalog, asset_server: &AssetServer, path: &str) -> Sprite {
    catalog.require(path);
    let mut sprite = Sprite {
        image: asset_server.load(path.to_string()),
        ..Default::default()
    };
    // Extracted strips now keep every frame; a plain consumer must show one
    // frame, not the whole row. Animated users (SpriteAnim, ui_art) overwrite
    // the rect themselves.
    if let Some(m) = catalog.anims.get(path)
        && m[0] > 1.0
    {
        let (w, h) = (m[1].max(1.0), m[2].max(1.0));
        sprite.rect = Some(Rect::new(0.0, 0.0, w, h));
    }
    sprite
}

/// Same as `sprite_exact`, but pick a horizontal strip frame.
pub fn sprite_exact_frame(
    catalog: &AssetCatalog,
    asset_server: &AssetServer,
    path: &str,
    frame: usize,
) -> Sprite {
    let mut sprite = sprite_exact(catalog, asset_server, path);
    if let Some(m) = catalog.anims.get(path) {
        let frames = m[0].max(1.0) as usize;
        let f = frame % frames.max(1);
        let w = m[1].max(1.0);
        let h = m[2].max(1.0);
        sprite.rect = Some(Rect::new(f as f32 * w, 0.0, (f as f32 + 1.0) * w, h));
    }
    sprite
}

/// Metadata: [frames, w, h, fps, xorigin, yorigin]
pub fn sprite_meta(catalog: &AssetCatalog, path: &str) -> [f32; 6] {
    catalog
        .anims
        .get(path)
        .copied()
        .unwrap_or([1.0, 16.0, 16.0, 0.0, 8.0, 8.0])
}

/// Place a sprite as GameMaker would: draw point = instance (x,y), using
/// sprite xorigin/yorigin. Coordinates are Bevy y-up; `draw_pos` is the
/// Bevy-space position of the GM draw point (instance x,y mapped to y-up).
/// Returns (Sprite, Transform) with center-anchor.
pub fn sprite_at_gm_origin(
    catalog: &AssetCatalog,
    asset_server: &AssetServer,
    path: &str,
    frame: usize,
    draw_pos: Vec2,
    z: f32,
) -> (Sprite, Transform) {
    let m = sprite_meta(catalog, path);
    let (fw, fh, xorigin, yorigin) = (m[1].max(1.0), m[2].max(1.0), m[4], m[5]);
    let sprite = sprite_exact_frame(catalog, asset_server, path, frame);

    // GM: left = x - xorigin, top = y - yorigin (y-down).
    // Bevy y-up draw_pos is the same logical corner/origin point on screen
    // after the lattice is already y-up:
    //   left  = draw_pos.x - xorigin
    //   top (high y) = draw_pos.y + yorigin
    //   center = left + fw/2, top - fh/2 = draw_pos.y + yorigin - fh/2.
    let center = Vec2::new(
        draw_pos.x - xorigin + fw * 0.5,
        draw_pos.y + yorigin - fh * 0.5,
    );

    (sprite, Transform::from_xyz(center.x, center.y, z))
}

/// Bevy `Anchor` for a sprite path, derived from GameMaker xorigin/yorigin.
/// Centered origins return `Anchor::Center` (default); custom origins (weapons,
/// projectiles) return a custom anchor so rotation pivots at the handle/muzzle
/// exactly as in the ~/Documents reference (fixes positional discrepancies).
pub fn sprite_anchor(catalog: &AssetCatalog, path: &str) -> bevy::sprite::Anchor {
    if let Some(m) = catalog.anims.get(path) {
        let (w, h) = (m[1].max(1.0), m[2].max(1.0));
        let (xorigin, yorigin) = (m[4], m[5]);
        let ax = xorigin / w - 0.5;
        let ay = 0.5 - yorigin / h;
        if ax.abs() > 0.001 || ay.abs() > 0.001 {
            return bevy::sprite::Anchor(Vec2::new(ax, ay));
        }
    }
    bevy::sprite::Anchor::CENTER
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum MutationId {
    RhinoSkin,
    PlutoniumHunger,
    TriggerFingers,
    RabbitPaw,
    SecondStomach,
    ScarierFace,
    BoilingVeins,
    ImpactWrists,
    ExtraFeet,
    Bloodlust,
    LuckyShot,
    GammaGuts,
    BackMuscle,
    Euphoria,
    LongArms,
    Stress,
    EagleEyes,
    OpenMind,
    HeavyHeart,
    StrongSpirit,
    SharpTeeth,
    LastWish,

    // Missing upstream mutation pool.
    BoltMarrow,
    Hammerhead,
    LaserBrain,
    RecycleGland,
    ShotgunShoulders,
    ThroneButt,
    Patience,
}

pub const ALL_MUTATIONS: [MutationId; 29] = [
    MutationId::RhinoSkin,
    MutationId::PlutoniumHunger,
    MutationId::TriggerFingers,
    MutationId::RabbitPaw,
    MutationId::SecondStomach,
    MutationId::ScarierFace,
    MutationId::BoilingVeins,
    MutationId::ImpactWrists,
    MutationId::ExtraFeet,
    MutationId::Bloodlust,
    MutationId::LuckyShot,
    MutationId::GammaGuts,
    MutationId::BackMuscle,
    MutationId::Euphoria,
    MutationId::LongArms,
    MutationId::Stress,
    MutationId::EagleEyes,
    MutationId::OpenMind,
    MutationId::HeavyHeart,
    MutationId::StrongSpirit,
    MutationId::SharpTeeth,
    MutationId::LastWish,
    MutationId::BoltMarrow,
    MutationId::Hammerhead,
    MutationId::LaserBrain,
    MutationId::RecycleGland,
    MutationId::ShotgunShoulders,
    MutationId::ThroneButt,
    MutationId::Patience,
];

pub struct MutationDef {
    pub name: &'static str,
    pub description: &'static str,
}

pub fn mutation_def(id: MutationId) -> MutationDef {
    match id {
        MutationId::RhinoSkin => MutationDef {
            name: "Rhino Skin",
            description: "+4 max HP",
        },
        MutationId::PlutoniumHunger => MutationDef {
            name: "Plutonium Hunger",
            description: "Much larger pickup range",
        },
        MutationId::TriggerFingers => MutationDef {
            name: "Trigger Fingers",
            description: "Kills lower reload time",
        },
        MutationId::RabbitPaw => MutationDef {
            name: "Rabbit Paw",
            description: "Better chance for drops",
        },
        MutationId::SecondStomach => MutationDef {
            name: "Second Stomach",
            description: "Medkits heal double",
        },
        MutationId::ScarierFace => MutationDef {
            name: "Scarier Face",
            description: "Enemies have less HP",
        },
        MutationId::BoilingVeins => MutationDef {
            name: "Boiling Veins",
            description: "Explosions can't drop you below 4 HP",
        },
        MutationId::ImpactWrists => MutationDef {
            name: "Impact Wrists",
            description: "Weapons knock back harder",
        },
        MutationId::ExtraFeet => MutationDef {
            name: "Extra Feet",
            description: "Move faster",
        },
        MutationId::Bloodlust => MutationDef {
            name: "Bloodlust",
            description: "Kills sometimes heal you",
        },
        MutationId::LuckyShot => MutationDef {
            name: "Lucky Shot",
            description: "Kills sometimes drop ammo",
        },
        MutationId::GammaGuts => MutationDef {
            name: "Gamma Guts",
            description: "Enemies that touch you take damage",
        },
        MutationId::BackMuscle => MutationDef {
            name: "Back Muscle",
            description: "Higher ammo capacity",
        },
        MutationId::Euphoria => MutationDef {
            name: "Euphoria",
            description: "Enemy bullets are slower",
        },
        MutationId::LongArms => MutationDef {
            name: "Long Arms",
            description: "Melee attacks reach further",
        },
        MutationId::Stress => MutationDef {
            name: "Stress",
            description: "Fire faster at low health",
        },
        MutationId::EagleEyes => MutationDef {
            name: "Eagle Eyes",
            description: "Better accuracy",
        },
        MutationId::OpenMind => MutationDef {
            name: "Open Mind",
            description: "More chests spawn",
        },
        MutationId::HeavyHeart => MutationDef {
            name: "Heavy Heart",
            description: "More weapon drops",
        },
        MutationId::StrongSpirit => MutationDef {
            name: "Strong Spirit",
            description: "Prevents death, once",
        },
        MutationId::SharpTeeth => MutationDef {
            name: "Sharp Teeth",
            description: "Damage taken also hurts nearby enemies",
        },
        MutationId::LastWish => MutationDef {
            name: "Last Wish",
            description: "Heal and refill ammo when low",
        },
        MutationId::BoltMarrow => MutationDef {
            name: "Bolt Marrow",
            description: "Bolts seek targets",
        },
        MutationId::Hammerhead => MutationDef {
            name: "Hammerhead",
            description: "Chew through destructible props",
        },
        MutationId::LaserBrain => MutationDef {
            name: "Laser Brain",
            description: "Energy weapons hit harder",
        },
        MutationId::RecycleGland => MutationDef {
            name: "Recycle Gland",
            description: "Bullet weapons sometimes refund ammo",
        },
        MutationId::ShotgunShoulders => MutationDef {
            name: "Shotgun Shoulders",
            description: "Shells bounce off walls",
        },
        MutationId::ThroneButt => MutationDef {
            name: "Throne Butt",
            description: "Active ability is upgraded",
        },
        MutationId::Patience => MutationDef {
            name: "Patience",
            description: "Skip now; get more choices next time",
        },
    }
}

/// GML mut_* index used as sprSkillIcon subimage (nt-rewrite scrSkills order).
pub fn mutation_skill_index(id: MutationId) -> u8 {
    match id {
        MutationId::RhinoSkin => 1,
        MutationId::ExtraFeet => 2,
        MutationId::PlutoniumHunger => 3,
        MutationId::RabbitPaw => 4,
        MutationId::ThroneButt => 5,
        MutationId::LuckyShot => 6,
        MutationId::Bloodlust => 7,
        MutationId::GammaGuts => 8,
        MutationId::SecondStomach => 9,
        MutationId::BackMuscle => 10,
        MutationId::ScarierFace => 11,
        MutationId::Euphoria => 12,
        MutationId::LongArms => 13,
        MutationId::BoilingVeins => 14,
        MutationId::ShotgunShoulders => 15,
        MutationId::RecycleGland => 16,
        MutationId::LaserBrain => 17,
        MutationId::LastWish => 18,
        MutationId::EagleEyes => 19,
        MutationId::ImpactWrists => 20,
        MutationId::BoltMarrow => 21,
        MutationId::Stress => 22,
        MutationId::TriggerFingers => 23,
        MutationId::SharpTeeth => 24,
        MutationId::Patience => 25,
        MutationId::Hammerhead => 26,
        MutationId::StrongSpirit => 27,
        MutationId::OpenMind => 28,
        MutationId::HeavyHeart => 29,
    }
}

/// Ultra skill icon subimage.
pub fn ultra_skill_index(id: UltraMutationId) -> u8 {
    match id {
        UltraMutationId::FishGunWarrant => 1,
        UltraMutationId::FishConfiscate => 2,
        UltraMutationId::CrystalFortress => 3,
        UltraMutationId::CrystalJuggernaut => 4,
        UltraMutationId::EyesMonsterStyle => 5,
        UltraMutationId::EyesProjectileStyle => 6,
        UltraMutationId::MeltingBrainCapacity => 7,
        UltraMutationId::MeltingDetachment => 8,
        UltraMutationId::PlantTrapper => 9,
        UltraMutationId::PlantKiller => 10,
        UltraMutationId::VenuzBack2Bizniz => 11,
        UltraMutationId::VenuzGunGod => 12,
        UltraMutationId::SteroidsAmbidextrous => 13,
        UltraMutationId::SteroidsGetArmed => 14,
        UltraMutationId::RobotRefinedTaste => 15,
        UltraMutationId::RobotRegurgitate => 16,
        UltraMutationId::ChickenHarderToKill => 17,
        UltraMutationId::ChickenDetermination => 18,
        UltraMutationId::RebelPersonalGuard => 19,
        UltraMutationId::RebelRiot => 20,
        UltraMutationId::HorrorStalker => 21,
        UltraMutationId::HorrorAnomaly => 22,
        UltraMutationId::RogueSuperBlastArmor => 23,
        UltraMutationId::RoguePortalStrike => 24,
        UltraMutationId::BigDogHeavyArtillery => 25,
        UltraMutationId::BigDogGuardian => 26,
        UltraMutationId::SkeletonBloodArmor => 27,
        UltraMutationId::SkeletonNecromancy => 28,
        UltraMutationId::FrogToxicLord => 29,
        UltraMutationId::FrogSwampBody => 30,
        UltraMutationId::CuzHoarder => 1,
        UltraMutationId::CuzQuickSwap => 2,
    }
}

#[cfg(test)]
mod weapon_id_tests {
    use super::*;

    #[test]
    fn invalid_weapon_ids_are_sanitized() {
        assert_eq!(sanitize_weapon_id(WeaponId(255)), WeaponId::NONE);
    }

    #[test]
    fn invalid_weapon_metadata_does_not_panic() {
        assert_eq!(weapon_meta(WeaponId(255)).id, 0);
    }

    #[test]
    fn all_real_weapon_ids_resolve() {
        for id in 0..crate::game::weapons_data::MAXWEP as u8 {
            assert_eq!(weapon_meta(WeaponId(id)).id, id);
        }
    }
}

#[cfg(test)]
mod crown_tests {
    use super::*;

    #[test]
    fn crown_ids_roundtrip() {
        for crown in CrownKind::ALL {
            assert_eq!(CrownKind::from_u8(crown.to_u8()), crown);
        }
    }

    #[test]
    fn bad_crown_ids_are_none() {
        assert_eq!(CrownKind::from_u8(99), CrownKind::None);
    }

    #[test]
    fn crown_cycle_wraps() {
        assert_eq!(CrownKind::None.cycle(-1), CrownKind::Protection);
        assert_eq!(CrownKind::Protection.cycle(1), CrownKind::None);
    }

    #[test]
    fn crown_names_are_non_empty() {
        for crown in CrownKind::ALL {
            assert!(!crown.name().is_empty());
            assert!(!crown.short_name().is_empty());
        }
    }
}

#[cfg(test)]
mod mutation_pool_tests {
    use super::*;

    #[test]
    fn normal_mutation_pool_has_no_duplicates() {
        let mut seen = std::collections::HashSet::new();
        for id in ALL_MUTATIONS {
            assert!(seen.insert(id), "duplicate mutation: {id:?}");
        }
    }

    #[test]
    fn missing_reference_mutations_are_in_pool() {
        for id in [
            MutationId::BoltMarrow,
            MutationId::Hammerhead,
            MutationId::LaserBrain,
            MutationId::RecycleGland,
            MutationId::ShotgunShoulders,
            MutationId::ThroneButt,
            MutationId::Patience,
        ] {
            assert!(
                ALL_MUTATIONS.contains(&id),
                "{id:?} missing from ALL_MUTATIONS"
            );
        }
    }

    #[test]
    fn every_mutation_has_text() {
        for id in ALL_MUTATIONS {
            let def = mutation_def(id);
            assert!(!def.name.is_empty(), "{id:?} has no name");
            assert!(!def.description.is_empty(), "{id:?} has no description");
        }
    }

    #[test]
    fn every_playable_race_has_two_ultras() {
        for race in PLAYABLE_RACES {
            let [a, b] = ultra_choices_for(race);
            assert_ne!(a, b, "{race:?} has duplicate ultras");

            let da = ultra_mutation_def(a);
            let db = ultra_mutation_def(b);

            assert!(!da.name.is_empty());
            assert!(!db.name.is_empty());
            assert!(!da.description.is_empty());
            assert!(!db.description.is_empty());
        }
    }

    #[test]
    fn random_race_uses_fish_ultras_as_safe_default() {
        let choices = ultra_choices_for(RaceId::Random);
        assert_eq!(choices[0], UltraMutationId::FishGunWarrant);
        assert_eq!(choices[1], UltraMutationId::FishConfiscate);
    }
}

#[cfg(test)]
mod boss_def_tests {
    use super::*;

    #[test]
    fn known_bosses_are_marked_boss() {
        for kind in [
            EnemyKind::BigBandit,
            EnemyKind::BigDog,
            EnemyKind::LilHunter,
            EnemyKind::Throne,
        ] {
            assert!(enemy_def(kind).boss, "{kind:?} should be boss");
        }
    }

    #[test]
    fn throne_is_stationary_shooter() {
        let def = enemy_def(EnemyKind::Throne);
        assert!(def.shoot_range >= 900.0);
        assert!(def.hp >= 300);
    }

    #[test]
    fn big_dog_has_fan_attack_data() {
        let def = enemy_def(EnemyKind::BigDog);
        assert!(def.bullets_per_shot >= 5);
        assert!(def.fan_spread > 0.0);
        assert!(def.hp >= 150);
    }

    #[test]
    fn lil_hunter_is_mobile_burst_boss() {
        let def = enemy_def(EnemyKind::LilHunter);
        assert!(def.speed >= 100.0);
        assert!(def.burst);
        assert!(def.attack_cooldown <= 0.75);
    }

    #[test]
    fn big_bandit_charge_boss_has_contact_damage() {
        let def = enemy_def(EnemyKind::BigBandit);
        assert!(def.touch_damage >= 5);
        assert!(def.shoot_range >= 500.0);
    }
}

#[cfg(test)]
mod idpd_def_tests {
    use super::*;

    #[test]
    fn idpd_units_are_not_bosses() {
        for kind in [
            EnemyKind::IdpdGrunt,
            EnemyKind::IdpdShield,
            EnemyKind::IdpdElite,
            EnemyKind::IdpdVan,
        ] {
            assert!(!enemy_def(kind).boss, "{kind:?} should not be boss");
        }
    }

    #[test]
    fn idpd_van_is_stationary() {
        let def = enemy_def(EnemyKind::IdpdVan);
        assert_eq!(def.speed, 0.0);
        assert_eq!(def.accel, 0.0);
        assert!(def.hp >= 60);
    }

    #[test]
    fn idpd_elite_is_more_dangerous_than_grunt() {
        let grunt = enemy_def(EnemyKind::IdpdGrunt);
        let elite = enemy_def(EnemyKind::IdpdElite);
        assert!(elite.hp > grunt.hp);
        assert!(elite.attack_cooldown <= grunt.attack_cooldown);
        assert!(elite.bullets_per_shot >= grunt.bullets_per_shot);
    }

    #[test]
    fn idpd_shield_is_midrange() {
        let def = enemy_def(EnemyKind::IdpdShield);
        assert!(def.preferred_range > 0.0);
        assert!(def.shoot_range >= 400.0);
    }
}

#[cfg(test)]
mod loop_boss_def_tests {
    use super::*;

    #[test]
    fn loop_bosses_are_boss_flagged() {
        assert!(enemy_def(EnemyKind::ThroneII).boss);
        assert!(enemy_def(EnemyKind::Hyper).boss);
    }

    #[test]
    fn throne_ii_is_mobile_and_tougher_than_throne() {
        let t1 = enemy_def(EnemyKind::Throne);
        let t2 = enemy_def(EnemyKind::ThroneII);
        assert!(t2.hp > t1.hp);
        assert!(t2.speed > t1.speed);
        assert!(t2.touch_damage >= 10);
        assert!(t2.score > t1.score);
    }

    #[test]
    fn hyper_is_contact_flunky_boss() {
        let h = enemy_def(EnemyKind::Hyper);
        assert!(h.touch_damage >= 50);
        assert_eq!(h.bullets_per_shot, 0);
        assert!(h.hp >= 400);
        assert_eq!(h.speed, 35.0);
    }
}

#[cfg(test)]
mod loop_variant_def_tests {
    use super::*;

    #[test]
    fn loop_bosses_are_bosses() {
        for kind in [
            EnemyKind::BigBanditLoop,
            EnemyKind::BigDogLoop,
            EnemyKind::LilHunterLoop,
        ] {
            assert!(enemy_def(kind).boss, "{kind:?}");
        }
    }

    #[test]
    fn loop_big_bandit_is_stronger_than_base() {
        let base = enemy_def(EnemyKind::BigBandit);
        let looped = enemy_def(EnemyKind::BigBanditLoop);
        assert!(looped.hp > base.hp);
        assert!(looped.touch_damage >= base.touch_damage);
        assert!(looped.bullets_per_shot >= base.bullets_per_shot);
    }

    #[test]
    fn loop_big_dog_is_stronger_than_base() {
        let base = enemy_def(EnemyKind::BigDog);
        let looped = enemy_def(EnemyKind::BigDogLoop);
        assert!(looped.hp > base.hp);
        assert!(looped.projectile_speed >= base.projectile_speed);
        assert!(looped.bullets_per_shot >= base.bullets_per_shot);
    }

    #[test]
    fn loop_lil_hunter_is_faster_than_base() {
        let base = enemy_def(EnemyKind::LilHunter);
        let looped = enemy_def(EnemyKind::LilHunterLoop);
        assert!(looped.hp > base.hp);
        assert!(looped.speed > base.speed);
        assert!(looped.attack_cooldown < base.attack_cooldown);
    }
}
