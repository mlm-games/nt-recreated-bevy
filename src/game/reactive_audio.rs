//! Reactive one-shot audio and combat-intensity overlays.

use bevy::audio::{AudioPlayer, AudioSink, AudioSource, PlaybackMode, PlaybackSettings, Volume};
use bevy::prelude::*;

use crate::game::areas::AreaId;
use crate::game::components::*;
use crate::game::content::AssetCatalog;
use crate::game::idpd::is_idpd_kind;
use crate::menus::UiAction;
use game_utils_bevy::audio::AudioChannels;
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum ReactiveCue {
    // Gameplay / progression
    LevelUp,
    MutationChosen,
    UltraChosen,
    BossAppear,
    BossDefeated,
    PlayerCritical,
    PlayerDeath,
    PortalOpen,
    PortalEnter,
    SecretFound,
    WeaponPickup,
    ChestOpen,
    LoopComplete,
    ThroneRises,
    IdpdIncoming,

    // Kill feedback
    Kill,
    KillStreak,

    // UI
    UiClick,
    UiBack,
    UiConfirm,
    UiCycle,
}

#[derive(bevy::ecs::message::Message, Clone, Copy, Debug)]
pub struct ReactiveAudioRequest {
    pub cue: ReactiveCue,
}

impl ReactiveAudioRequest {
    pub fn new(cue: ReactiveCue) -> Self {
        Self { cue }
    }
}

/// Deferred cue for systems at (or near) the system-parameter cap: spawn this
/// component and `flush_queued_cues` converts it into a request message.
#[derive(Component, Clone, Copy, Debug)]
pub struct QueuedReactiveCue(pub ReactiveCue);

pub fn cue_candidates(cue: ReactiveCue) -> &'static [&'static str] {
    match cue {
        ReactiveCue::LevelUp => &[
            "audio/sfx/snd_levelup.ogg",
            "audio/sfx/snd_mutation.ogg",
            "sounds/snd_levelup.ogg",
        ],
        ReactiveCue::MutationChosen => &[
            "audio/sfx/snd_mutation_chosen.ogg",
            "audio/sfx/snd_mutation.ogg",
            "sounds/snd_mutation.ogg",
        ],
        ReactiveCue::UltraChosen => &[
            "audio/sfx/snd_ultra_chosen.ogg",
            "audio/sfx/snd_mutation_chosen.ogg",
            "audio/sfx/snd_levelup.ogg",
        ],
        ReactiveCue::BossAppear => &[
            "audio/sfx/snd_boss_appear.ogg",
            "audio/sfx/snd_boss_intro.ogg",
            "sounds/snd_boss_intro.ogg",
        ],
        ReactiveCue::BossDefeated => &[
            "audio/sfx/snd_boss_dead.ogg",
            "audio/sfx/snd_boss_defeated.ogg",
            "sounds/snd_boss_dead.ogg",
        ],
        ReactiveCue::PlayerCritical => &[
            "audio/sfx/snd_hurt_critical.ogg",
            "audio/sfx/snd_low_health.ogg",
            "audio/sfx/snd_hurt.ogg",
        ],
        ReactiveCue::PlayerDeath => &[
            "audio/sfx/snd_player_dead.ogg",
            "audio/sfx/snd_death.ogg",
            "sounds/snd_death.ogg",
        ],
        ReactiveCue::PortalOpen => &[
            "audio/sfx/snd_portal_open.ogg",
            "audio/sfx/snd_portal.ogg",
            "sounds/snd_portal.ogg",
        ],
        ReactiveCue::PortalEnter => &[
            "audio/sfx/snd_portal_enter.ogg",
            "audio/sfx/snd_portal.ogg",
            "sounds/snd_portal.ogg",
        ],
        ReactiveCue::SecretFound => &["audio/sfx/snd_secret.ogg", "audio/sfx/snd_secret_found.ogg"],
        ReactiveCue::WeaponPickup => &[
            "audio/sfx/snd_weapon_pickup.ogg",
            "audio/sfx/snd_pickup.ogg",
        ],
        ReactiveCue::ChestOpen => &["audio/sfx/snd_chest_open.ogg", "audio/sfx/snd_pickup.ogg"],
        ReactiveCue::LoopComplete => &[
            "audio/sfx/snd_loop_complete.ogg",
            "audio/sfx/snd_levelup.ogg",
        ],
        ReactiveCue::ThroneRises => &[
            "audio/sfx/snd_throne_rises.ogg",
            "audio/sfx/snd_boss_intro.ogg",
        ],
        ReactiveCue::IdpdIncoming => {
            &["audio/sfx/snd_idpd_incoming.ogg", "audio/sfx/snd_alarm.ogg"]
        }
        ReactiveCue::Kill => &["audio/sfx/snd_kill.ogg", "audio/sfx/snd_hit.ogg"],
        ReactiveCue::KillStreak => &["audio/sfx/snd_streak.ogg", "audio/sfx/snd_levelup.ogg"],
        ReactiveCue::UiClick => &["audio/sfx/ui_click.ogg", "audio/sfx/snd_ui_click.ogg"],
        ReactiveCue::UiBack => &["audio/sfx/ui_back.ogg", "audio/sfx/snd_ui_back.ogg"],
        ReactiveCue::UiConfirm => &["audio/sfx/ui_confirm.ogg", "audio/sfx/snd_ui_confirm.ogg"],
        ReactiveCue::UiCycle => &["audio/sfx/ui_cycle.ogg", "audio/sfx/snd_ui_cycle.ogg"],
    }
}

pub fn cue_base_volume(cue: ReactiveCue) -> f32 {
    match cue {
        ReactiveCue::PlayerDeath => 1.0,
        ReactiveCue::BossAppear | ReactiveCue::BossDefeated | ReactiveCue::ThroneRises => 0.95,
        ReactiveCue::LoopComplete => 0.95,
        ReactiveCue::LevelUp
        | ReactiveCue::MutationChosen
        | ReactiveCue::UltraChosen
        | ReactiveCue::SecretFound
        | ReactiveCue::IdpdIncoming => 0.85,
        ReactiveCue::PortalOpen | ReactiveCue::PortalEnter | ReactiveCue::PlayerCritical => 0.75,
        ReactiveCue::KillStreak => 0.70,
        ReactiveCue::WeaponPickup | ReactiveCue::ChestOpen | ReactiveCue::UiBack => 0.58,
        ReactiveCue::UiConfirm => 0.58,
        ReactiveCue::UiClick => 0.48,
        ReactiveCue::UiCycle => 0.40,
        ReactiveCue::Kill => 0.32,
    }
}

pub fn cue_throttle_seconds(cue: ReactiveCue) -> f32 {
    match cue {
        ReactiveCue::Kill => 0.12,
        ReactiveCue::UiCycle => 0.06,
        ReactiveCue::WeaponPickup | ReactiveCue::ChestOpen => 0.25,
        ReactiveCue::PlayerCritical => 1.5,
        ReactiveCue::KillStreak => 2.5,
        _ => 0.38,
    }
}

pub fn throttle_allows(cue: ReactiveCue, last_fired: Option<f32>, now: f32) -> bool {
    let Some(last) = last_fired else {
        return true;
    };

    now - last >= cue_throttle_seconds(cue).max(1.0 / 30.0)
}

fn first_existing_audio(
    catalog: &AssetCatalog,
    candidates: &'static [&'static str],
) -> Option<&'static str> {
    candidates.iter().copied().find(|p| catalog.has_audio(p))
}

#[derive(Resource, Default)]
pub struct ReactiveAudioState {
    last_fired: HashMap<ReactiveCue, f32>,
    known_bosses: HashSet<Entity>,
    last_player_level: Option<u32>,
    last_player_hp: Option<i32>,
    low_hp_armed: bool,
    kill_streak: u32,
    kill_streak_last: f32,
}

impl ReactiveAudioState {
    pub fn reset(&mut self) {
        self.last_fired.clear();
        self.known_bosses.clear();
        self.last_player_level = None;
        self.last_player_hp = None;
        self.low_hp_armed = true;
        self.kill_streak = 0;
        self.kill_streak_last = 0.0;
    }

    fn mark_fired(&mut self, cue: ReactiveCue, now: f32) {
        self.last_fired.insert(cue, now);
    }

    fn last_fired(&self, cue: ReactiveCue) -> Option<f32> {
        self.last_fired.get(&cue).copied()
    }

    /// Returns true on every tenth kill inside the 3.5s streak window.
    fn note_kill(&mut self, now: f32) -> bool {
        if now - self.kill_streak_last > 3.5 {
            self.kill_streak = 0;
        }

        self.kill_streak_last = now;
        self.kill_streak += 1;

        self.kill_streak % 10 == 0
    }
}

pub fn reset_reactive_audio_state(mut state: ResMut<ReactiveAudioState>) {
    state.reset();
}

/// Converts deferred [`QueuedReactiveCue`] components into request messages so
/// parameter-capped systems can still fire stingers via `commands`.
pub fn flush_queued_cues(
    mut commands: Commands,
    queued: Query<(Entity, &QueuedReactiveCue)>,
    mut writer: MessageWriter<ReactiveAudioRequest>,
) {
    for (entity, queued) in queued.iter() {
        writer.write(ReactiveAudioRequest::new(queued.0));
        commands.entity(entity).despawn();
    }
}

pub fn play_reactive_audio_requests(
    mut commands: Commands,
    time: Res<Time>,
    asset_server: Res<AssetServer>,
    catalog: Res<AssetCatalog>,
    channels: Res<AudioChannels>,
    mut state: ResMut<ReactiveAudioState>,
    mut requests: MessageReader<ReactiveAudioRequest>,
) {
    let now = time.elapsed_secs();
    let bus = channels.master.clamp(0.0, 1.0) * channels.sfx.clamp(0.0, 1.0);

    if bus <= 0.0 {
        requests.clear();
        return;
    }

    for request in requests.read() {
        if !throttle_allows(request.cue, state.last_fired(request.cue), now) {
            continue;
        }

        let Some(path) = first_existing_audio(&catalog, cue_candidates(request.cue)) else {
            // Missing assets stay silent; the throttle window is still
            // consumed so same-frame repeats don't pile up.
            state.mark_fired(request.cue, now);
            continue;
        };

        commands.spawn((
            GameCleanup,
            AudioPlayer::<AudioSource>::new(asset_server.load(path.to_string())),
            PlaybackSettings {
                mode: PlaybackMode::Despawn,
                volume: Volume::Linear((cue_base_volume(request.cue) * bus).clamp(0.0, 1.0)),
                ..default()
            },
        ));

        state.mark_fired(request.cue, now);
    }
}

/// Automatic progression/player stingers without touching large systems.
pub fn observe_player_audio_state(
    mut state: ResMut<ReactiveAudioState>,
    player_q: Query<(&Player, &Health), With<Player>>,
    mut writer: MessageWriter<ReactiveAudioRequest>,
) {
    let Ok((player, health)) = player_q.single() else {
        state.last_player_level = None;
        state.last_player_hp = None;
        state.low_hp_armed = true;
        return;
    };

    if let Some(prev_level) = state.last_player_level
        && player.level > prev_level
    {
        writer.write(ReactiveAudioRequest::new(ReactiveCue::LevelUp));
    }
    state.last_player_level = Some(player.level);

    let crit = (health.max as f32 * 0.10).ceil() as i32;
    if let Some(prev_hp) = state.last_player_hp
        && health.hp < prev_hp
        && health.hp > 0
        && health.hp <= crit
    {
        writer.write(ReactiveAudioRequest::new(ReactiveCue::PlayerCritical));
    }

    // Low-health warning fires once on crossing below 25%, rearms above it.
    let low = (health.max as f32 * 0.25).ceil() as i32;
    if health.hp > low {
        state.low_hp_armed = true;
    } else if health.hp > 0 && state.low_hp_armed {
        writer.write(ReactiveAudioRequest::new(ReactiveCue::PlayerCritical));
        state.low_hp_armed = false;
    }

    state.last_player_hp = Some(health.hp);
}

/// Boss appear / defeated stingers keyed on BossBrain membership deltas.
pub fn observe_boss_audio_state(
    mut state: ResMut<ReactiveAudioState>,
    bosses: Query<Entity, With<BossBrain>>,
    mut writer: MessageWriter<ReactiveAudioRequest>,
) {
    let current: HashSet<Entity> = bosses.iter().collect();

    if current.difference(&state.known_bosses).next().is_some() {
        writer.write(ReactiveAudioRequest::new(ReactiveCue::BossAppear));
    }

    if state.known_bosses.difference(&current).next().is_some() {
        writer.write(ReactiveAudioRequest::new(ReactiveCue::BossDefeated));
    }

    state.known_bosses = current;
}

/// Generic kill / kill-streak feedback.
///
/// `RemovedComponents<Enemy>` also sees non-death marker removals (the IDPD
/// Van sheds its marker when empty); acceptable for a broad feedback cue.
pub fn observe_kill_audio_state(
    time: Res<Time>,
    mut state: ResMut<ReactiveAudioState>,
    mut removed: RemovedComponents<Enemy>,
    mut writer: MessageWriter<ReactiveAudioRequest>,
) {
    let now = time.elapsed_secs();

    for _ in removed.read() {
        writer.write(ReactiveAudioRequest::new(ReactiveCue::Kill));

        if state.note_kill(now) {
            writer.write(ReactiveAudioRequest::new(ReactiveCue::KillStreak));
        }
    }
}

/// Maps the tree's real UiAction variants onto reactive UI cues.
pub fn ui_action_to_cue(action: &UiAction) -> Option<ReactiveCue> {
    match action {
        UiAction::StartGame | UiAction::MainMenuPlay | UiAction::Resume => {
            Some(ReactiveCue::UiConfirm)
        }

        UiAction::QuitToTitle | UiAction::QuitApp => Some(ReactiveCue::UiBack),

        UiAction::ToggleLoadout => Some(ReactiveCue::UiBack),

        UiAction::OpenSettings
        | UiAction::OpenCredits
        | UiAction::CloseOverlay
        | UiAction::SaveSettings => Some(ReactiveCue::UiClick),

        UiAction::SelectCharacter(_)
        | UiAction::PickMutation(_)
        | UiAction::SelectMutation(_)
        | UiAction::SelectCrown(_) => Some(ReactiveCue::UiConfirm),

        UiAction::SelectSkin(_)
        | UiAction::NextLanguage
        | UiAction::CycleStartWeapon(_)
        | UiAction::CycleStoredWeapon(_)
        | UiAction::CycleCrown(_) => Some(ReactiveCue::UiCycle),

        // Slider spam stays silent.
        UiAction::SetMasterVol(_)
        | UiAction::SetSfxVol(_)
        | UiAction::SetMusicVol(_)
        | UiAction::SetAmbienceVol(_)
        | UiAction::SetLanguage(_) => None,
        UiAction::SettingsCategory(_)
        | UiAction::SettingsBack
        | UiAction::ShowPauseConfirm(_)
        | UiAction::CancelPauseConfirm
        | UiAction::ConfirmPause(_) => Some(ReactiveCue::UiClick),
        UiAction::SettingToggle(_)
        | UiAction::SettingCycle { .. }
        | UiAction::SettingInput { .. }
        | UiAction::SettingResetOptions
        | UiAction::SettingEraseProgress
        | UiAction::SettingViewCredits
        | UiAction::SettingOpenSubcategory(_) => Some(ReactiveCue::UiClick),
        UiAction::SettingSlider { .. } => None,
    }
}

/// Reads the same UiAction drain stream as `process_ui_actions`.
pub fn play_ui_action_audio(
    mut reader: MessageReader<UiBridgeAction>,
    mut writer: MessageWriter<ReactiveAudioRequest>,
) {
    for bridged in reader.read() {
        if let Some(cue) = ui_action_to_cue(&bridged.0) {
            writer.write(ReactiveAudioRequest::new(cue));
        }
    }
}

/// Mirror of every UI action, emitted by `process_ui_actions` so multiple
/// readers can observe the drain without touching the Arc/Mutex channel.
#[derive(bevy::ecs::message::Message, Clone, Debug)]
pub struct UiBridgeAction(pub UiAction);

#[derive(Component)]
pub struct CombatIntensityLayer {
    #[allow(dead_code)] // retained for future per-area mixing rules
    pub area: AreaId,
    pub current: f32,
}

#[derive(Resource, Default)]
pub struct CombatIntensityState {
    last_area: Option<AreaId>,
}

pub fn intensity_candidates(area: AreaId) -> &'static [&'static str] {
    match area {
        AreaId::Desert => &[
            "audio/music/mus_desert_intensity.ogg",
            "audio/music/desert_intensity.ogg",
        ],
        AreaId::Sewers | AreaId::PizzaSewers => &[
            "audio/music/mus_sewers_intensity.ogg",
            "audio/music/sewers_intensity.ogg",
        ],
        AreaId::Scrapyards => &[
            "audio/music/mus_scrapyard_intensity.ogg",
            "audio/music/scrapyard_intensity.ogg",
        ],
        AreaId::CrystalCaves | AreaId::CursedCaves => &[
            "audio/music/mus_caves_intensity.ogg",
            "audio/music/caves_intensity.ogg",
        ],
        AreaId::FrozenCity => &[
            "audio/music/mus_frozen_intensity.ogg",
            "audio/music/frozen_intensity.ogg",
        ],
        AreaId::Labs => &[
            "audio/music/mus_labs_intensity.ogg",
            "audio/music/labs_intensity.ogg",
        ],
        AreaId::Palace => &[
            "audio/music/mus_palace_intensity.ogg",
            "audio/music/palace_intensity.ogg",
        ],
        AreaId::HQ => &[
            "audio/music/mus_hq_intensity.ogg",
            "audio/music/hq_intensity.ogg",
        ],
        _ => &[],
    }
}

/// Combat pressure: ordinary enemy = 1, IDPD = 2, boss = 4.
pub fn combat_intensity_score(enemies_total: usize, idpd_count: usize, boss_count: usize) -> u32 {
    let ordinary = enemies_total.saturating_sub(idpd_count + boss_count);

    ordinary as u32 + idpd_count as u32 * 2 + boss_count as u32 * 4
}

/// Target intensity in `[0, 1]`.
pub fn combat_intensity_target(score: u32) -> f32 {
    match score {
        0..=2 => 0.0,
        3..=9 => (score - 2) as f32 / 7.0,
        _ => 1.0,
    }
}

/// Exponential half-life smoothing toward a target.
pub fn smooth_value(current: f32, target: f32, dt: f32, half_life: f32) -> f32 {
    if dt <= 0.0 || half_life <= 0.0 {
        return target;
    }

    let keep = 2.0_f32.powf(-dt / half_life);
    target + (current - target) * keep
}

pub fn reset_combat_intensity(
    mut commands: Commands,
    mut state: ResMut<CombatIntensityState>,
    layers: Query<Entity, With<CombatIntensityLayer>>,
) {
    state.last_area = None;

    for entity in layers.iter() {
        commands.entity(entity).despawn();
    }
}

pub fn update_combat_intensity_audio(
    mut commands: Commands,
    time: Res<Time>,
    asset_server: Res<AssetServer>,
    catalog: Res<AssetCatalog>,
    channels: Res<AudioChannels>,
    run: Option<Res<Run>>,
    transition: Option<Res<LoopTransition>>,
    mut state: ResMut<CombatIntensityState>,
    enemies: Query<&Enemy>,
    bosses: Query<(), With<BossBrain>>,
    mut layers: Query<(Entity, &mut CombatIntensityLayer, &mut AudioSink)>,
) {
    let Some(run) = run else {
        return;
    };

    let suppressed = transition
        .as_ref()
        .is_some_and(|t| t.campfire_active || t.throne_ii_alive);

    // Area change: drop the old layer, spawn the new one if candidates exist.
    if state.last_area != Some(run.area) {
        for (entity, _, _) in layers.iter() {
            commands.entity(entity).despawn();
        }

        state.last_area = Some(run.area);

        if let Some(path) = first_existing_audio(&catalog, intensity_candidates(run.area)) {
            commands.spawn((
                GameCleanup,
                CombatIntensityLayer {
                    area: run.area,
                    current: 0.0,
                },
                AudioPlayer::<AudioSource>::new(asset_server.load(path.to_string())),
                PlaybackSettings {
                    mode: PlaybackMode::Loop,
                    volume: Volume::SILENT,
                    ..default()
                },
            ));
        }

        return;
    }

    let mut enemy_total = 0usize;
    let mut idpd_total = 0usize;

    for enemy in enemies.iter() {
        enemy_total += 1;
        if is_idpd_kind(enemy.kind) {
            idpd_total += 1;
        }
    }

    let boss_total = bosses.iter().count();
    let score = combat_intensity_score(enemy_total, idpd_total, boss_total);
    let mut target = combat_intensity_target(score);

    if suppressed {
        target = 0.0;
    }

    let bus = channels.master.clamp(0.0, 1.0) * channels.music.clamp(0.0, 1.0) * 0.42;

    for (_, mut layer, mut sink) in &mut layers {
        layer.current = smooth_value(layer.current, target, time.delta_secs(), 0.55);
        sink.set_volume(Volume::Linear((layer.current * bus).clamp(0.0, 1.0)));
    }
}
