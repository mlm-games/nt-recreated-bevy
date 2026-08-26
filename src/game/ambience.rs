//! Area music + ambience: cue selection, looping track entities, crossfades,
//! and volume-bus sync.
//!
//! Silent-failure design: every cue maps to candidate paths checked against
//! the asset catalog; a missing file simply means that cue stays quiet until
//! matching audio is dropped into the pack.

use bevy::audio::{AudioPlayer, AudioSink, PlaybackMode, PlaybackSettings, Volume};
use bevy::prelude::*;
use game_utils_bevy::audio::AudioChannels;

use crate::game::GameCleanup;
use crate::game::areas::AreaId;
use crate::game::components::*;
use crate::game::content::{AssetCatalog, EnemyKind};

/// High-level music selection cue.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum MusicCue {
    Desert,
    Sewers,
    Scrapyards,
    CrystalCaves,
    FrozenCity,
    Labs,
    Palace,

    Oasis,
    PizzaSewers,
    CursedCaves,
    Jungle,
    Vault,
    CrownVault,
    Hq,
    City,
    Campfire,

    BossBigBandit,
    BossBigDog,
    BossLilHunter,
    BossThrone,
    BossThroneII,
}

/// High-level ambience selection cue.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum AmbienceCue {
    None,
    DesertWind,
    SewerDrip,
    ScrapHum,
    CrystalHum,
    FrozenWind,
    LabBuzz,
    PalaceFire,
    OasisBreeze,
    JungleBugs,
    VaultHum,
    HqSirens,
    CampfireCrackle,
    CityNoise,
}

#[derive(Resource, Debug, Default)]
pub struct AreaAudioState {
    pub current_music: Option<MusicCue>,
    pub current_ambience: Option<AmbienceCue>,
    pub music_entity: Option<Entity>,
    pub ambience_entity: Option<Entity>,
}

/// Marker + identity for the currently playing music track.
#[derive(Component)]
pub struct AreaMusicTrack {
    #[allow(dead_code)] // retained for future intensity-layer queries
    pub cue: MusicCue,
}

/// Marker + identity for the currently playing ambience loop.
#[derive(Component)]
pub struct AreaAmbienceTrack {
    #[allow(dead_code)] // retained for future intensity-layer queries
    pub cue: AmbienceCue,
}

#[derive(Component)]
pub struct AreaAudioFader {
    pub start: f32,
    pub end: f32,
    pub timer: Timer,
    pub despawn_on_finish: bool,
}

/// Map route/secret area to its base music cue.
pub fn music_for_area(area: AreaId) -> MusicCue {
    match area {
        AreaId::Desert => MusicCue::Desert,
        AreaId::Sewers => MusicCue::Sewers,
        AreaId::Scrapyards => MusicCue::Scrapyards,
        AreaId::CrystalCaves => MusicCue::CrystalCaves,
        AreaId::FrozenCity => MusicCue::FrozenCity,
        AreaId::Labs => MusicCue::Labs,
        AreaId::Palace => MusicCue::Palace,

        AreaId::Oasis => MusicCue::Oasis,
        AreaId::PizzaSewers => MusicCue::PizzaSewers,
        AreaId::CursedCaves => MusicCue::CursedCaves,
        AreaId::Jungle => MusicCue::Jungle,
        AreaId::Vault => MusicCue::Vault,
        AreaId::CrownVault => MusicCue::CrownVault,
        AreaId::HQ => MusicCue::Hq,
        AreaId::City => MusicCue::City,
        AreaId::Campfire => MusicCue::Campfire,
        // Post-loop limbo slot; never produced by the current router.
        AreaId::Loop => MusicCue::Desert,
    }
}

/// Map route/secret area to ambience cue.
pub fn ambience_for_area(area: AreaId) -> AmbienceCue {
    match area {
        AreaId::Desert => AmbienceCue::DesertWind,
        AreaId::Sewers => AmbienceCue::SewerDrip,
        AreaId::Scrapyards => AmbienceCue::ScrapHum,
        AreaId::CrystalCaves => AmbienceCue::CrystalHum,
        AreaId::FrozenCity => AmbienceCue::FrozenWind,
        AreaId::Labs => AmbienceCue::LabBuzz,
        AreaId::Palace => AmbienceCue::PalaceFire,

        AreaId::Oasis => AmbienceCue::OasisBreeze,
        AreaId::PizzaSewers => AmbienceCue::SewerDrip,
        AreaId::CursedCaves => AmbienceCue::CrystalHum,
        AreaId::Jungle => AmbienceCue::JungleBugs,
        AreaId::Vault => AmbienceCue::VaultHum,
        AreaId::CrownVault => AmbienceCue::VaultHum,
        AreaId::HQ => AmbienceCue::HqSirens,
        AreaId::City => AmbienceCue::CityNoise,
        AreaId::Campfire => AmbienceCue::CampfireCrackle,
        // Post-loop limbo slot; never produced by the current router.
        AreaId::Loop => AmbienceCue::DesertWind,
    }
}

/// Boss themes override area music. Loop variants reuse their family theme;
/// unknown bosses fall through to area music.
pub fn boss_music_for_kind(kind: EnemyKind) -> Option<MusicCue> {
    match kind {
        EnemyKind::BigBandit | EnemyKind::BigBanditLoop => Some(MusicCue::BossBigBandit),
        EnemyKind::BigDog | EnemyKind::BigDogLoop => Some(MusicCue::BossBigDog),
        EnemyKind::LilHunter | EnemyKind::LilHunterLoop => Some(MusicCue::BossLilHunter),
        EnemyKind::Throne => Some(MusicCue::BossThrone),
        EnemyKind::ThroneII => Some(MusicCue::BossThroneII),
        _ => None,
    }
}

fn music_candidates(cue: MusicCue) -> &'static [&'static str] {
    match cue {
        MusicCue::Desert => &[
            "audio/music/desert.ogg",
            "audio/music/musDesert.ogg",
            "sounds/music/desert.ogg",
        ],
        MusicCue::Sewers => &[
            "audio/music/sewers.ogg",
            "audio/music/musSewers.ogg",
            "sounds/music/sewers.ogg",
        ],
        MusicCue::Scrapyards => &[
            "audio/music/scrapyards.ogg",
            "audio/music/musScrapyards.ogg",
            "sounds/music/scrapyards.ogg",
        ],
        MusicCue::CrystalCaves => &[
            "audio/music/crystal_caves.ogg",
            "audio/music/crystalcaves.ogg",
            "audio/music/musCrystal.ogg",
            "sounds/music/crystal_caves.ogg",
        ],
        MusicCue::FrozenCity => &[
            "audio/music/frozen_city.ogg",
            "audio/music/frozencity.ogg",
            "audio/music/musFrozen.ogg",
            "sounds/music/frozen_city.ogg",
        ],
        MusicCue::Labs => &[
            "audio/music/labs.ogg",
            "audio/music/musLabs.ogg",
            "sounds/music/labs.ogg",
        ],
        MusicCue::Palace => &[
            "audio/music/palace.ogg",
            "audio/music/musPalace.ogg",
            "sounds/music/palace.ogg",
        ],
        MusicCue::Oasis => &[
            "audio/music/oasis.ogg",
            "audio/music/musOasis.ogg",
            "sounds/music/oasis.ogg",
        ],
        MusicCue::PizzaSewers => &[
            "audio/music/pizza_sewers.ogg",
            "audio/music/pizzasewers.ogg",
            "audio/music/musPizza.ogg",
            "sounds/music/pizza_sewers.ogg",
        ],
        MusicCue::CursedCaves => &[
            "audio/music/cursed_caves.ogg",
            "audio/music/cursedcaves.ogg",
            "audio/music/musCursed.ogg",
            "sounds/music/cursed_caves.ogg",
        ],
        MusicCue::Jungle => &[
            "audio/music/jungle.ogg",
            "audio/music/musJungle.ogg",
            "sounds/music/jungle.ogg",
        ],
        MusicCue::Vault => &[
            "audio/music/vault.ogg",
            "audio/music/musVault.ogg",
            "sounds/music/vault.ogg",
        ],
        MusicCue::CrownVault => &[
            "audio/music/crown_vault.ogg",
            "audio/music/crownvault.ogg",
            "audio/music/musCrownVault.ogg",
            "sounds/music/crown_vault.ogg",
        ],
        MusicCue::Hq => &[
            "audio/music/hq.ogg",
            "audio/music/idpd_hq.ogg",
            "audio/music/musHq.ogg",
            "sounds/music/hq.ogg",
        ],
        MusicCue::City => &[
            "audio/music/city.ogg",
            "audio/music/yv_mansion.ogg",
            "audio/music/musCity.ogg",
            "sounds/music/city.ogg",
        ],
        MusicCue::Campfire => &[
            "audio/music/campfire.ogg",
            "audio/music/rest.ogg",
            "audio/music/musCampfire.ogg",
            "sounds/music/campfire.ogg",
        ],
        MusicCue::BossBigBandit => &[
            "audio/music/boss_big_bandit.ogg",
            "audio/music/big_bandit.ogg",
            "audio/music/musBossBandit.ogg",
            "sounds/music/boss_big_bandit.ogg",
        ],
        MusicCue::BossBigDog => &[
            "audio/music/boss_big_dog.ogg",
            "audio/music/big_dog.ogg",
            "audio/music/musBossDog.ogg",
            "sounds/music/boss_big_dog.ogg",
        ],
        MusicCue::BossLilHunter => &[
            "audio/music/boss_lil_hunter.ogg",
            "audio/music/lil_hunter.ogg",
            "audio/music/musBossHunter.ogg",
            "sounds/music/boss_lil_hunter.ogg",
        ],
        MusicCue::BossThrone => &[
            "audio/music/boss_throne.ogg",
            "audio/music/throne.ogg",
            "audio/music/musThrone.ogg",
            "sounds/music/boss_throne.ogg",
        ],
        MusicCue::BossThroneII => &[
            "audio/music/boss_throne_ii.ogg",
            "audio/music/throne_ii.ogg",
            "audio/music/musThrone2.ogg",
            "sounds/music/boss_throne_ii.ogg",
        ],
    }
}

fn ambience_candidates(cue: AmbienceCue) -> &'static [&'static str] {
    match cue {
        AmbienceCue::None => &[],
        AmbienceCue::DesertWind => &[
            "audio/ambience/desert_wind.ogg",
            "audio/ambient/desert.ogg",
            "sounds/ambience/desert_wind.ogg",
        ],
        AmbienceCue::SewerDrip => &[
            "audio/ambience/sewer_drip.ogg",
            "audio/ambient/sewers.ogg",
            "sounds/ambience/sewer_drip.ogg",
        ],
        AmbienceCue::ScrapHum => &[
            "audio/ambience/scrap_hum.ogg",
            "audio/ambient/scrapyards.ogg",
            "sounds/ambience/scrap_hum.ogg",
        ],
        AmbienceCue::CrystalHum => &[
            "audio/ambience/crystal_hum.ogg",
            "audio/ambient/crystal.ogg",
            "sounds/ambience/crystal_hum.ogg",
        ],
        AmbienceCue::FrozenWind => &[
            "audio/ambience/frozen_wind.ogg",
            "audio/ambient/frozen.ogg",
            "sounds/ambience/frozen_wind.ogg",
        ],
        AmbienceCue::LabBuzz => &[
            "audio/ambience/lab_buzz.ogg",
            "audio/ambient/labs.ogg",
            "sounds/ambience/lab_buzz.ogg",
        ],
        AmbienceCue::PalaceFire => &[
            "audio/ambience/palace_fire.ogg",
            "audio/ambient/palace.ogg",
            "sounds/ambience/palace_fire.ogg",
        ],
        AmbienceCue::OasisBreeze => &[
            "audio/ambience/oasis_breeze.ogg",
            "audio/ambient/oasis.ogg",
            "sounds/ambience/oasis_breeze.ogg",
        ],
        AmbienceCue::JungleBugs => &[
            "audio/ambience/jungle_bugs.ogg",
            "audio/ambient/jungle.ogg",
            "sounds/ambience/jungle_bugs.ogg",
        ],
        AmbienceCue::VaultHum => &[
            "audio/ambience/vault_hum.ogg",
            "audio/ambient/vault.ogg",
            "sounds/ambience/vault_hum.ogg",
        ],
        AmbienceCue::HqSirens => &[
            "audio/ambience/hq_sirens.ogg",
            "audio/ambient/hq.ogg",
            "sounds/ambience/hq_sirens.ogg",
        ],
        AmbienceCue::CampfireCrackle => &[
            "audio/ambience/campfire_crackle.ogg",
            "audio/ambient/campfire.ogg",
            "sounds/ambience/campfire_crackle.ogg",
        ],
        AmbienceCue::CityNoise => &[
            "audio/ambience/city_noise.ogg",
            "audio/ambient/city.ogg",
            "sounds/ambience/city_noise.ogg",
        ],
    }
}

fn pick_audio_handle(
    catalog: &AssetCatalog,
    asset_server: &AssetServer,
    candidates: &[&str],
) -> Option<Handle<AudioSource>> {
    for &path in candidates {
        if catalog.has_audio(path) {
            return Some(asset_server.load(path.to_string()));
        }
    }
    None
}

fn desired_music_cue(
    run: &Run,
    transition: &LoopTransition,
    campfires: &Query<(), With<CampfireProp>>,
    bosses: &Query<&Enemy, With<BossBrain>>,
) -> MusicCue {
    if transition.campfire_active || !campfires.is_empty() {
        return MusicCue::Campfire;
    }

    for boss in bosses.iter() {
        if let Some(cue) = boss_music_for_kind(boss.kind) {
            return cue;
        }
    }

    music_for_area(run.area)
}

fn desired_ambience_cue(
    run: &Run,
    transition: &LoopTransition,
    campfires: &Query<(), With<CampfireProp>>,
) -> AmbienceCue {
    if transition.campfire_active || !campfires.is_empty() {
        return AmbienceCue::CampfireCrackle;
    }

    ambience_for_area(run.area)
}

const MUSIC_FADE_SECS: f32 = 1.2;
const AMBIENCE_FADE_SECS: f32 = 0.8;
const AMBIENCE_BASE_SCALE: f32 = 0.55;

/// Swap music/ambience entities when the resolved cues change.
///
/// Outgoing tracks receive a fade-out fader that despawns them; incoming
/// tracks start silent and fade in. Missing audio keeps the cue "selected"
/// but spawns nothing, so adding a file later just works.
pub fn sync_area_audio(
    mut commands: Commands,
    catalog: Res<AssetCatalog>,
    asset_server: Res<AssetServer>,
    run: Res<Run>,
    transition: Res<LoopTransition>,
    mut state: ResMut<AreaAudioState>,
    campfires: Query<(), With<CampfireProp>>,
    bosses: Query<&Enemy, With<BossBrain>>,
    music_entities: Query<Entity, With<AreaMusicTrack>>,
    ambience_entities: Query<Entity, With<AreaAmbienceTrack>>,
) {
    let wanted_music = desired_music_cue(&run, &transition, &campfires, &bosses);
    let wanted_ambience = desired_ambience_cue(&run, &transition, &campfires);

    if state.current_music != Some(wanted_music) {
        // Fade out whatever is currently playing.
        for entity in music_entities.iter() {
            commands.entity(entity).insert(AreaAudioFader {
                start: 1.0,
                end: 0.0,
                timer: Timer::from_seconds(MUSIC_FADE_SECS, TimerMode::Once),
                despawn_on_finish: true,
            });
        }

        let mut spawned = None;
        if let Some(handle) =
            pick_audio_handle(&catalog, &asset_server, music_candidates(wanted_music))
        {
            let entity = commands
                .spawn((
                    GameCleanup,
                    AreaMusicTrack { cue: wanted_music },
                    AudioPlayer::new(handle),
                    PlaybackSettings {
                        mode: PlaybackMode::Loop,
                        volume: Volume::SILENT,
                        ..default()
                    },
                    AreaAudioFader {
                        start: 0.0,
                        end: 1.0,
                        timer: Timer::from_seconds(MUSIC_FADE_SECS, TimerMode::Once),
                        despawn_on_finish: false,
                    },
                ))
                .id();
            spawned = Some(entity);
        }

        state.music_entity = spawned;
        state.current_music = Some(wanted_music);
    }

    if state.current_ambience != Some(wanted_ambience) {
        for entity in ambience_entities.iter() {
            commands.entity(entity).insert(AreaAudioFader {
                start: 1.0,
                end: 0.0,
                timer: Timer::from_seconds(AMBIENCE_FADE_SECS, TimerMode::Once),
                despawn_on_finish: true,
            });
        }

        let mut spawned = None;
        if wanted_ambience != AmbienceCue::None
            && let Some(handle) = pick_audio_handle(
                &catalog,
                &asset_server,
                ambience_candidates(wanted_ambience),
            )
        {
            let entity = commands
                .spawn((
                    GameCleanup,
                    AreaAmbienceTrack {
                        cue: wanted_ambience,
                    },
                    AudioPlayer::new(handle),
                    PlaybackSettings {
                        mode: PlaybackMode::Loop,
                        volume: Volume::SILENT,
                        ..default()
                    },
                    AreaAudioFader {
                        start: 0.0,
                        end: 1.0,
                        timer: Timer::from_seconds(AMBIENCE_FADE_SECS, TimerMode::Once),
                        despawn_on_finish: false,
                    },
                ))
                .id();
            spawned = Some(entity);
        }

        state.ambience_entity = spawned;
        state.current_ambience = Some(wanted_ambience);
    }
}

#[allow(clippy::type_complexity)]
pub fn tick_area_audio_fades(
    time: Res<Time>,
    channels: Res<AudioChannels>,
    mut commands: Commands,
    mut q: Query<(
        Entity,
        Option<&AreaMusicTrack>,
        Option<&AreaAmbienceTrack>,
        Option<&mut AudioSink>,
        &mut AreaAudioFader,
    )>,
) {
    let music_base = channels.master * channels.music;
    let ambience_base = channels.master * channels.music * AMBIENCE_BASE_SCALE;

    for (entity, music, ambience, mut sink, mut fade) in &mut q {
        fade.timer.tick(time.delta());

        let t = if fade.timer.duration().is_zero() {
            1.0
        } else {
            (fade.timer.elapsed_secs() / fade.timer.duration().as_secs_f32()).clamp(0.0, 1.0)
        };

        let gain = fade.start + (fade.end - fade.start) * t;
        let base = if music.is_some() {
            music_base
        } else if ambience.is_some() {
            ambience_base
        } else {
            channels.master
        };

        if let Some(ref mut sink) = sink {
            sink.set_volume(Volume::Linear((base * gain).clamp(0.0, 1.0)));
        }

        if fade.timer.just_finished() {
            if fade.despawn_on_finish {
                commands.entity(entity).despawn();
            } else {
                commands.entity(entity).remove::<AreaAudioFader>();
                if let Some(ref mut sink) = sink {
                    sink.set_volume(Volume::Linear(base.clamp(0.0, 1.0)));
                }
            }
        }
    }
}

/// Live volume-bus sync so slider moves affect playing tracks immediately.
pub fn sync_area_audio_volumes(
    channels: Res<AudioChannels>,
    mut music_q: Query<
        &mut AudioSink,
        (
            With<AreaMusicTrack>,
            Without<AreaAmbienceTrack>,
            Without<AreaAudioFader>,
        ),
    >,
    mut ambience_q: Query<
        &mut AudioSink,
        (
            With<AreaAmbienceTrack>,
            Without<AreaMusicTrack>,
            Without<AreaAudioFader>,
        ),
    >,
) {
    let music_base = channels.master * channels.music;
    let ambience_base = channels.master * channels.music * AMBIENCE_BASE_SCALE;

    for mut sink in music_q.iter_mut() {
        sink.set_volume(Volume::Linear(music_base.clamp(0.0, 1.0)));
    }

    for mut sink in ambience_q.iter_mut() {
        sink.set_volume(Volume::Linear(ambience_base.clamp(0.0, 1.0)));
    }
}

