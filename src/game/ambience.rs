use bevy::audio::{AudioPlayer, AudioSink, PlaybackMode, PlaybackSettings, Volume};
use bevy::prelude::*;
use game_utils_bevy::audio::AudioChannels;

use crate::app::{AppState, Paused};
use crate::game::GameCleanup;
use crate::game::areas::AreaId;
use crate::game::components::*;
use crate::game::content::{AssetCatalog, EnemyKind};

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
    TitleTheme,
}

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

#[derive(Resource, Debug)]
pub struct AmbFilter(pub f32);
impl Default for AmbFilter {
    fn default() -> Self {
        Self(1.0)
    }
}

#[derive(Component)]
pub struct AreaMusicTrack {
    #[allow(dead_code)]
    pub cue: MusicCue,
}

#[derive(Component)]
pub struct AreaAmbienceTrack {
    #[allow(dead_code)]
    pub cue: AmbienceCue,
}

#[derive(Component)]
pub struct AreaAudioFader {
    pub start: f32,
    pub end: f32,
    pub timer: Timer,
    pub despawn_on_finish: bool,
}

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

        AreaId::Loop => MusicCue::Desert,
    }
}

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

        AreaId::Loop => AmbienceCue::DesertWind,
    }
}

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
            "audio/mus1.ogg",
            "audio/mus1b.ogg",
            "audio/music/desert.ogg",
            "audio/music/musDesert.ogg",
            "sounds/music/desert.ogg",
        ],
        MusicCue::Sewers => &[
            "audio/mus2.ogg",
            "audio/music/sewers.ogg",
            "audio/music/musSewers.ogg",
            "sounds/music/sewers.ogg",
        ],
        MusicCue::Scrapyards => &[
            "audio/mus3.ogg",
            "audio/mus3b.ogg",
            "audio/music/scrapyards.ogg",
            "audio/music/musScrapyards.ogg",
            "sounds/music/scrapyards.ogg",
        ],
        MusicCue::CrystalCaves => &[
            "audio/mus4.ogg",
            "audio/music/crystal_caves.ogg",
            "audio/music/crystalcaves.ogg",
            "audio/music/musCrystal.ogg",
            "sounds/music/crystal_caves.ogg",
        ],
        MusicCue::FrozenCity => &[
            "audio/mus5.ogg",
            "audio/mus5b.ogg",
            "audio/music/frozen_city.ogg",
            "audio/music/frozencity.ogg",
            "audio/music/musFrozen.ogg",
            "sounds/music/frozen_city.ogg",
        ],
        MusicCue::Labs => &[
            "audio/mus6.ogg",
            "audio/music/labs.ogg",
            "audio/music/musLabs.ogg",
            "sounds/music/labs.ogg",
        ],
        MusicCue::Palace => &[
            "audio/mus7.ogg",
            "audio/mus7b.ogg",
            "audio/music/palace.ogg",
            "audio/music/musPalace.ogg",
            "sounds/music/palace.ogg",
        ],
        MusicCue::Oasis => &[
            "audio/mus100.ogg",
            "audio/mus100b.ogg",
            "audio/music/oasis.ogg",
            "audio/music/musOasis.ogg",
            "sounds/music/oasis.ogg",
        ],
        MusicCue::PizzaSewers => &[
            "audio/mus101.ogg",
            "audio/music/pizza_sewers.ogg",
            "audio/music/pizzasewers.ogg",
            "audio/music/musPizza.ogg",
            "sounds/music/pizza_sewers.ogg",
        ],
        MusicCue::CursedCaves => &[
            "audio/mus102.ogg",
            "audio/music/cursed_caves.ogg",
            "audio/music/cursedcaves.ogg",
            "audio/music/musCursed.ogg",
            "sounds/music/cursed_caves.ogg",
        ],
        MusicCue::Jungle => &[
            "audio/mus103.ogg",
            "audio/music/jungle.ogg",
            "audio/music/musJungle.ogg",
            "sounds/music/jungle.ogg",
        ],
        MusicCue::Vault => &[
            "audio/mus104.ogg",
            "audio/music/vault.ogg",
            "audio/music/musVault.ogg",
            "sounds/music/vault.ogg",
        ],
        MusicCue::CrownVault => &[
            "audio/mus105.ogg",
            "audio/music/crown_vault.ogg",
            "audio/music/crownvault.ogg",
            "audio/music/musCrownVault.ogg",
            "sounds/music/crown_vault.ogg",
        ],
        MusicCue::Hq => &[
            "audio/mus106.ogg",
            "audio/mus106b.ogg",
            "audio/music/hq.ogg",
            "audio/music/idpd_hq.ogg",
            "audio/music/musHq.ogg",
            "sounds/music/hq.ogg",
        ],
        MusicCue::City => &[
            "audio/mus107.ogg",
            "audio/music/city.ogg",
            "audio/music/yv_mansion.ogg",
            "audio/music/musCity.ogg",
            "sounds/music/city.ogg",
        ],
        MusicCue::Campfire => &[
            "audio/musBoss4Silence.ogg",
            "audio/musboss4silence.ogg",
            "audio/music/campfire.ogg",
            "audio/music/rest.ogg",
            "audio/music/musCampfire.ogg",
            "sounds/music/campfire.ogg",
        ],
        MusicCue::TitleTheme => &[
            "audio/musthemea.ogg",
            "audio/musThemeA.ogg",
            "audio/musthemeb.ogg",
            "audio/musThemeB.ogg",
            "audio/musthemep.ogg",
            "audio/music/title.ogg",
            "audio/music/musThemeA.ogg",
            "sounds/music/title.ogg",
        ],
        MusicCue::BossBigBandit => &[
            "audio/musBoss1.ogg",
            "audio/musboss1.ogg",
            "audio/music/boss_big_bandit.ogg",
            "audio/music/big_bandit.ogg",
            "audio/music/musBossBandit.ogg",
            "sounds/music/boss_big_bandit.ogg",
        ],
        MusicCue::BossBigDog => &[
            "audio/musBoss2.ogg",
            "audio/musboss2.ogg",
            "audio/music/boss_big_dog.ogg",
            "audio/music/big_dog.ogg",
            "audio/music/musBossDog.ogg",
            "sounds/music/boss_big_dog.ogg",
        ],
        MusicCue::BossLilHunter => &[
            "audio/musBoss3.ogg",
            "audio/musboss3.ogg",
            "audio/music/boss_lil_hunter.ogg",
            "audio/music/lil_hunter.ogg",
            "audio/music/musBossHunter.ogg",
            "sounds/music/boss_lil_hunter.ogg",
        ],
        MusicCue::BossThrone => &[
            "audio/musBoss4A.ogg",
            "audio/musBoss4B.ogg",
            "audio/musboss4a.ogg",
            "audio/music/boss_throne.ogg",
            "audio/music/throne.ogg",
            "audio/music/musThrone.ogg",
            "sounds/music/boss_throne.ogg",
        ],
        MusicCue::BossThroneII => &[
            "audio/musBoss5.ogg",
            "audio/musboss5.ogg",
            "audio/musBoss6.ogg",
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
            "audio/amb0.ogg",
            "audio/ambience/desert_wind.ogg",
            "audio/ambient/desert.ogg",
            "sounds/ambience/desert_wind.ogg",
        ],
        AmbienceCue::SewerDrip => &[
            "audio/amb1.ogg",
            "audio/ambience/sewer_drip.ogg",
            "audio/ambient/sewers.ogg",
            "sounds/ambience/sewer_drip.ogg",
        ],
        AmbienceCue::ScrapHum => &[
            "audio/amb2.ogg",
            "audio/ambience/scrap_hum.ogg",
            "audio/ambient/scrapyards.ogg",
            "sounds/ambience/scrap_hum.ogg",
        ],
        AmbienceCue::CrystalHum => &[
            "audio/amb3.ogg",
            "audio/ambience/crystal_hum.ogg",
            "audio/ambient/crystal.ogg",
            "sounds/ambience/crystal_hum.ogg",
        ],
        AmbienceCue::FrozenWind => &[
            "audio/amb4.ogg",
            "audio/ambience/frozen_wind.ogg",
            "audio/ambient/frozen.ogg",
            "sounds/ambience/frozen_wind.ogg",
        ],
        AmbienceCue::LabBuzz => &[
            "audio/amb5.ogg",
            "audio/ambience/lab_buzz.ogg",
            "audio/ambient/labs.ogg",
            "sounds/ambience/lab_buzz.ogg",
        ],
        AmbienceCue::PalaceFire => &[
            "audio/amb6.ogg",
            "audio/ambience/palace_fire.ogg",
            "audio/ambient/palace.ogg",
            "sounds/ambience/palace_fire.ogg",
        ],
        AmbienceCue::OasisBreeze => &[
            "audio/amb0b.ogg",
            "audio/ambience/oasis_breeze.ogg",
            "audio/ambient/oasis.ogg",
            "sounds/ambience/oasis_breeze.ogg",
        ],
        AmbienceCue::JungleBugs => &[
            "audio/amb0c.ogg",
            "audio/ambience/jungle_bugs.ogg",
            "audio/ambient/jungle.ogg",
            "sounds/ambience/jungle_bugs.ogg",
        ],
        AmbienceCue::VaultHum => &[
            "audio/amb101.ogg",
            "audio/ambience/vault_hum.ogg",
            "audio/ambient/vault.ogg",
            "sounds/ambience/vault_hum.ogg",
        ],
        AmbienceCue::HqSirens => &[
            "audio/amb107.ogg",
            "audio/ambience/hq_sirens.ogg",
            "audio/ambient/hq.ogg",
            "sounds/ambience/hq_sirens.ogg",
        ],
        AmbienceCue::CampfireCrackle => &[
            "audio/amb105.ogg",
            "audio/ambience/campfire_crackle.ogg",
            "audio/ambient/campfire.ogg",
            "sounds/ambience/campfire_crackle.ogg",
        ],
        AmbienceCue::CityNoise => &[
            "audio/amb106.ogg",
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

pub fn update_amb_filter(
    time: Res<Time>,
    paused: Res<Paused>,
    spiral: Option<Res<crate::game::vortex::SpiralCtl>>,
    mut filter: ResMut<AmbFilter>,
) {
    let target = if paused.0 || spiral.is_some() {
        0.2
    } else {
        1.0
    };

    let rate = 3.0;
    let dt = time.delta_secs();
    let step = rate * dt;
    if filter.0 < target {
        filter.0 = (filter.0 + step).min(target);
    } else if filter.0 > target {
        filter.0 = (filter.0 - step).max(target);
    }
}

const MUSIC_PAUSE_DIM: f32 = 0.45;

pub fn sync_area_audio(
    mut commands: Commands,
    catalog: Res<AssetCatalog>,
    asset_server: Res<AssetServer>,
    app_state: Res<State<AppState>>,
    run: Res<Run>,
    transition: Res<LoopTransition>,
    mut state: ResMut<AreaAudioState>,
    campfires: Query<(), With<CampfireProp>>,
    bosses: Query<&Enemy, With<BossBrain>>,
    music_entities: Query<Entity, With<AreaMusicTrack>>,
    ambience_entities: Query<Entity, With<AreaAmbienceTrack>>,
) {

    let (wanted_music, wanted_ambience): (Option<MusicCue>, Option<AmbienceCue>) =
        if *app_state.get() != AppState::InGame {
            if *app_state.get() == AppState::Loading {
                (
                    Some(desired_music_cue(&run, &transition, &campfires, &bosses)),
                    Some(desired_ambience_cue(&run, &transition, &campfires)),
                )
            } else if *app_state.get() == AppState::Splash {
                (None, None)
            } else {
                (Some(MusicCue::TitleTheme), None)
            }
        } else if run.game_over {
            (None, None)
        } else {
            (
                Some(desired_music_cue(&run, &transition, &campfires, &bosses)),
                Some(desired_ambience_cue(&run, &transition, &campfires)),
            )
        };

    if state.current_music != wanted_music {

        let fade_secs = if wanted_music.is_none() && run.game_over {
            0.18
        } else {
            MUSIC_FADE_SECS
        };
        for entity in music_entities.iter() {
            commands.entity(entity).insert(AreaAudioFader {
                start: 1.0,
                end: 0.0,
                timer: Timer::from_seconds(fade_secs, TimerMode::Once),
                despawn_on_finish: true,
            });
        }

        let mut spawned = None;
        if let Some(cue) = wanted_music
            && let Some(handle) = pick_audio_handle(&catalog, &asset_server, music_candidates(cue))
        {
            let entity = commands
                .spawn((
                    GameCleanup,
                    AreaMusicTrack { cue },
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
        state.current_music = wanted_music;
    }

    if state.current_ambience != wanted_ambience {
        for entity in ambience_entities.iter() {
            commands.entity(entity).insert(AreaAudioFader {
                start: 1.0,
                end: 0.0,
                timer: Timer::from_seconds(AMBIENCE_FADE_SECS, TimerMode::Once),
                despawn_on_finish: true,
            });
        }

        let mut spawned = None;
        if let Some(cue) = wanted_ambience
            && cue != AmbienceCue::None
            && let Some(handle) =
                pick_audio_handle(&catalog, &asset_server, ambience_candidates(cue))
        {
            let entity = commands
                .spawn((
                    GameCleanup,
                    AreaAmbienceTrack { cue },
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
        state.current_ambience = wanted_ambience;
    }
}

#[allow(clippy::type_complexity)]
pub fn tick_area_audio_fades(
    time: Res<Time>,
    channels: Res<AudioChannels>,
    paused: Res<Paused>,
    amb_filter: Res<AmbFilter>,
    mut commands: Commands,
    mut q: Query<(
        Entity,
        Option<&AreaMusicTrack>,
        Option<&AreaAmbienceTrack>,
        Option<&mut AudioSink>,
        &mut AreaAudioFader,
    )>,
) {
    let music_dim = if paused.0 { MUSIC_PAUSE_DIM } else { 1.0 };
    let music_base = channels.master * channels.music * music_dim;
    let ambience_base = channels.master * channels.music * AMBIENCE_BASE_SCALE * amb_filter.0;

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

pub fn despawn_area_audio(
    mut commands: Commands,
    mut state: ResMut<AreaAudioState>,
    music: Query<Entity, With<AreaMusicTrack>>,
    ambience: Query<Entity, With<AreaAmbienceTrack>>,
) {
    for e in music.iter().chain(ambience.iter()) {
        commands.entity(e).despawn();
    }
    *state = AreaAudioState::default();
}

pub fn sync_area_audio_volumes(
    channels: Res<AudioChannels>,
    paused: Res<Paused>,
    amb_filter: Res<AmbFilter>,
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
    let music_dim = if paused.0 { MUSIC_PAUSE_DIM } else { 1.0 };
    let music_base = channels.master * channels.music * music_dim;
    let ambience_base = channels.master * channels.music * AMBIENCE_BASE_SCALE * amb_filter.0;

    for mut sink in music_q.iter_mut() {
        sink.set_volume(Volume::Linear(music_base.clamp(0.0, 1.0)));
    }

    for mut sink in ambience_q.iter_mut() {
        sink.set_volume(Volume::Linear(ambience_base.clamp(0.0, 1.0)));
    }
}
