use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use bevy::audio::{AudioPlayer, AudioSource, PlaybackMode, PlaybackSettings, Volume};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use repose_bevy::{ReposePlugin, ReposePluginSettings};
use repose_core::{prelude::Modifier, remember};
use repose_ui::overlay::OverlayHandle;

use crate::asset_tracking::AssetsLoading;
use crate::dev_tools::DevToolsPlugin;
use crate::game::content::AssetCatalog;
use crate::game::{GamePlugin, MutationChoice, SelectedCharacter};
use crate::menus::{self, UiAction, UiBridge};
use crate::save::{SAVE_VERSION, SaveData};
use crate::screens::ScreensPlugin;
use crate::theme::ThemePlugin;
use game_utils_bevy::{
    EcosystemPlugin,
    audio::AudioChannels,
    i18n::{self, I18nPlugin, LocaleResources},
    post_process::{ScreenEffectSettings, sync_post_process_settings},
    save::{SaveManager, SavePlugin},
    screen_effects::CameraBase,
    time_scale::TimeScaleControl,
    transitions::Transition,
};

const TRANSLATION_KEYS: &[&str] = &[
    "app-title",
    "start-game",
    "settings",
    "credits",
    "quit",
    "paused",
    "resume",
    "quit-to-title",
    "save",
    "back",
    "master-volume",
    "sfx-volume",
    "music-volume",
    "language",
    "score",
    "best",
    "controls-hint",
    "loading",
];

const LOCALES: &[(&str, &str)] = &[
    ("en", include_str!("../assets/locales/en/main.ftl")),
    ("es", include_str!("../assets/locales/es/main.ftl")),
    ("fr", include_str!("../assets/locales/fr/main.ftl")),
    ("de", include_str!("../assets/locales/de/main.ftl")),
    ("ja", include_str!("../assets/locales/ja/main.ftl")),
    ("zh", include_str!("../assets/locales/zh/main.ftl")),
    ("pt", include_str!("../assets/locales/pt/main.ftl")),
];

#[derive(States, Default, Clone, Eq, PartialEq, Debug, Hash)]
pub enum AppState {
    #[default]
    Splash,
    /// Logo intro done: the five big main-menu buttons (nt-rewrite
    /// `MainMenuButton`).
    MainMenu,
    /// Char-select campfire row (`Menu`/`CharSelect`).
    Loading,
    Title,
    InGame,
}

#[derive(Resource, Default, Clone, Copy, PartialEq, Eq)]
pub struct Paused(pub bool);

#[derive(Resource, Default, Clone, Copy, PartialEq, Eq, Debug)]
pub enum OverlayMenu {
    #[default]
    None,
    Settings,
    Credits,
    Pause,
}

#[derive(Resource, Default)]
pub struct PendingUnpause(pub Option<Timer>);

#[derive(Resource, Clone)]
pub struct SharedUi {
    pub phase: AppState,
    pub paused: bool,
    pub loading_progress: f32,
    pub overlay: OverlayMenu,
    pub master_vol: f32,
    pub sfx_vol: f32,
    pub music_vol: f32,
    pub high_score: u32,
    pub score: u32,
    pub transition_alpha: f32,
    pub flash_alpha: f32,
    pub language: String,
    pub saved_language: String,
    pub available_languages: Vec<String>,
    pub translations: HashMap<String, String>,
    pub hp: i32,
    pub max_hp: i32,
    pub floor: u32,
    pub world: u32,
    pub floor_in_world: u32,
    pub loop_count: u32,
    pub level: u32,
    pub rads: u32,
    pub max_rads: u32,
    pub weapons: Vec<String>,
    pub current_weapon: usize,
    pub ammo: [i32; 6],
    pub ability: String,
    pub ability_ready: bool,
    pub boss_hp: u32,
    pub boss_max: u32,
    pub boss_name: String,
    pub toast: String,
    pub toast_timer: f32,
    pub mutation_choices: Vec<String>,
    /// GML skill index for each pending mutation (1-based subimage of
    /// `sprSkillIcon` / `sprSkillIconHUD`).
    pub mutation_choice_ids: Vec<u8>,
    /// GML SkillIcon.selected: None = nothing hovered, Some(idx) = hovered/selected, requiring second click to confirm.
    pub mutation_selected: Option<usize>,
    pub game_over: bool,
    /// Acquired mutations (skill indices) for death screen – mirrors GML results.
    pub death_mutation_ids: Vec<u8>,
    pub character: String,
    /// nt-rewrite `enum Race` id of the chosen mutant (Random=0..Cuz=16).
    pub selected_character: usize,
    /// GoButton revealed after the first successful char-select click
    /// (nt-rewrite CharSelect/Mouse_4).
    pub title_go_visible: bool,
    /// Loadout panel open (Menu.loadout_open; toggled from the splat/arrow).
    pub loadout_open: bool,
    /// Race id currently hovered in the char-select row (-1 = none).
    pub title_hover_race: i32,
    /// Main-menu button index currently hovered (-1 = none).
    pub main_menu_hover: i32,
    /// Vlambeer boot card index (0..=4; 4 = NT logo stage).
    pub boot_mode: u8,
    /// Ammo count of each equipped weapon's type (HUD text).
    pub weapon_ammo: [i32; 2],
    pub best_floor: u32,
    pub total_kills: u32,
    pub loadout_summary: String,
    pub start_weapon_name: String,
    pub stored_weapon_name: String,
    pub crown: String,
    /// Numeric ids for loadout ICON frames (sprLoadoutCrown / weapon art).
    pub start_weapon_id: u8,
    pub stored_weapon_id: u8,
    pub crown_id: u8,
    pub selected_skin: u8,
    /// Menu.portrait_offsets[0]: starts at 180 on race change, approaches 0.
    pub portrait_offset: f32,
    /// Menu.textappear[0]: 2 on race change; skills text uses this.
    pub text_appear: f32,
    /// Bumped when race changes so ui_art can reset splat / char anims.
    pub selection_epoch: u32,

    /// Viewport data used to pick HUD layout without adaptive APIs.
    pub viewport_width: f32,
    pub viewport_height: f32,
    pub hud_compact: bool,
    /// GenCont loading (between floors) – mirrors Floors' GENERATING overlay.
    pub gen_active: bool,
    pub gen_progress: f32,
    pub gen_tip: String,
    pub run_id: u32,
}

impl Default for SharedUi {
    fn default() -> Self {
        Self {
            phase: AppState::Splash,
            paused: false,
            loading_progress: 0.0,
            overlay: OverlayMenu::None,
            master_vol: 1.0,
            sfx_vol: 1.0,
            music_vol: 0.8,
            high_score: 0,
            score: 0,
            transition_alpha: 0.0,
            flash_alpha: 0.0,
            language: "en".to_string(),
            saved_language: "en".to_string(),
            available_languages: vec!["en".to_string()],
            translations: HashMap::new(),
            hp: 10,
            max_hp: 10,
            floor: 1,
            world: 1,
            floor_in_world: 1,
            loop_count: 0,
            level: 1,
            rads: 0,
            max_rads: 60,
            weapons: vec!["Revolver".to_string(), "Shotgun".to_string()],
            current_weapon: 0,
            ammo: [0, 0, 0, 0, 0, 0],
            ability: "Flip".to_string(),
            ability_ready: true,
            boss_hp: 0,
            boss_max: 0,
            boss_name: String::new(),
            toast: String::new(),
            toast_timer: 0.0,
            mutation_choices: Vec::new(),
            mutation_choice_ids: Vec::new(),
            mutation_selected: None,
            game_over: false,
            death_mutation_ids: Vec::new(),
            character: "Fish".to_string(),
            selected_character: 1,
            title_go_visible: false,
            loadout_open: false,
            title_hover_race: -1,
            main_menu_hover: -1,
            boot_mode: 0,
            weapon_ammo: [0, 0],
            best_floor: 0,
            total_kills: 0,
            loadout_summary: String::new(),
            start_weapon_name: "None".to_string(),
            stored_weapon_name: "None".to_string(),
            crown: "NONE".to_string(),
            start_weapon_id: 0,
            stored_weapon_id: 0,
            crown_id: 0,
            selected_skin: 0,
            portrait_offset: 0.0,
            text_appear: 0.0,
            selection_epoch: 0,
            viewport_width: 1280.0,
            viewport_height: 720.0,
            hud_compact: false,
            gen_active: false,
            gen_progress: 0.0,
            gen_tip: String::new(),
            run_id: 0,
        }
    }
}

pub const NT_SIM_HZ: f64 = 30.0;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NtSimSet {
    Always,
    Input,
    Movement,
    Combat,
    Progression,
    Cleanup,
}

/// Pixel font standing in for NT's bitmap `fntM1` (OFL, see
/// assets/fonts/Silkscreen-OFL.txt).
pub const NT_UI_FONT: &[u8] = include_bytes!("../assets/fonts/Silkscreen-Regular.ttf");

/// The loaded UI font, so sprite-side systems (splash Text2d) share the exact
/// face the Repose panels use.
#[derive(Resource, Clone)]
pub struct UiFont(pub Handle<Font>);

pub struct AppPlugin;

impl Plugin for AppPlugin {
    fn build(&self, app: &mut App) {
        let shared = Arc::new(Mutex::new(SharedUi::default()));
        let actions = Arc::new(Mutex::new(Vec::<UiAction>::new()));
        let shared_ui = shared.clone();
        let actions_ui = actions.clone();

        {
            let mut fonts = app.world_mut().resource_mut::<Assets<Font>>();
            let ui_font = fonts.add(Font::from_bytes(NT_UI_FONT.to_vec()));
            app.insert_resource(UiFont(ui_font));
        }

        app.init_state::<AppState>()
            .insert_resource(ClearColor(Color::BLACK))
            .insert_resource(Paused(false))
            .insert_resource(OverlayMenu::None)
            .insert_resource(PendingUnpause(None))
            .insert_resource(Time::<Fixed>::from_hz(NT_SIM_HZ))
            .insert_resource(UiBridge {
                shared: shared.clone(),
                actions: actions.clone(),
            })
            .add_plugins(
                ReposePlugin::with_settings(
                    ReposePluginSettings {
                        clear_alpha: 0.0,
                        compose_every_frame: true,
                        msaa_samples: 1,
                        overlay: true,
                        sampler: bevy::image::ImageSampler::nearest(),
                    },
                    move |_s, _c| {
                        let st = shared_ui.lock().unwrap().clone();
                        let acts = actions_ui.clone();
                        let overlay_rc = remember(OverlayHandle::new);
                        let overlay = (*overlay_rc).clone();
                        let root = menus::compose_root(overlay.clone(), st, acts);
                        overlay.host(Modifier::new().fill_max_size(), root)
                    },
                )
                .with_font_bytes(NT_UI_FONT),
            )
            .add_plugins((
                ThemePlugin,
                EcosystemPlugin::<AppState>::new(I18nPlugin::new(TRANSLATION_KEYS, LOCALES)),
                SavePlugin::<SaveData>::new(SaveManager::new(
                    "com",
                    "nt-recreated",
                    "nt-recreated-bevy",
                    "save.ron",
                    SAVE_VERSION,
                )),
                ScreensPlugin,
                GamePlugin,
                DevToolsPlugin,
            ))
            .add_systems(Startup, setup_camera)
            .configure_sets(
                FixedUpdate,
                (
                    NtSimSet::Always,
                    NtSimSet::Input,
                    NtSimSet::Movement,
                    NtSimSet::Combat,
                    NtSimSet::Progression,
                    NtSimSet::Cleanup,
                )
                    .chain(),
            )
            .add_systems(
                Update,
                (
                    sanitize_save,
                    apply_saved_settings,
                    sync_ui_viewport,
                    sync_post_process_settings::<AppState>,
                    force_death_overlay_state,
                    process_ui_actions,
                    handle_pause_input,
                    handle_mutation_keys,
                    handle_death_restart,
                    tick_pending_unpause,
                    // Sync after all overlay/paused changes so PostUpdate compose
                    // sees fresh SharedUi in same frame – prevents 1-frame stale
                    // MainMenu Settings flash after QuitToTitle (was before process).
                    sync_shared_ui,
                    sync_virtual_time_with_pause,
                )
                    .chain(),
            );
    }
}

fn sync_ui_viewport(windows: Query<&Window, With<PrimaryWindow>>, bridge: Res<UiBridge>) {
    let Ok(window) = windows.single() else {
        return;
    };

    let Ok(mut ui) = bridge.shared.lock() else {
        return;
    };

    ui.viewport_width = window.width();
    ui.viewport_height = window.height();
    ui.hud_compact = crate::menus::is_compact_viewport(ui.viewport_width, ui.viewport_height);
}

fn sanitize_save(mut save: ResMut<SaveData>) {
    if save.version < crate::save::SAVE_VERSION {
        save.sanitize_loadouts();
        save.version = crate::save::SAVE_VERSION;
    } else if save.is_added() {
        save.sanitize_loadouts();
    }
}

fn apply_saved_settings(save: Res<SaveData>, mut locale: ResMut<LocaleResources>) {
    if !save.is_added() && !save.is_changed() {
        return;
    }
    if locale
        .available
        .iter()
        .any(|l| l == &save.settings.language)
    {
        locale.set_locale(&save.settings.language);
    }
}

fn setup_camera(mut commands: Commands) {
    commands.spawn((
        Camera2d,
        // Arena art uses negative z for back-to-front ordering, so
        // widen the frustum to include it (like floppy-warriors).
        Projection::Orthographic(OrthographicProjection {
            near: -10000.0,
            far: 10000.0,
            ..OrthographicProjection::default_2d()
        }),
        Transform::from_xyz(0.0, 0.0, 1000.0),
        CameraBase {
            translation: Vec3::new(0.0, 0.0, 1000.0),
            rotation: 0.0,
        },
        ScreenEffectSettings::default(),
    ));
}

fn sync_shared_ui(
    state: Res<State<AppState>>,
    paused: Res<Paused>,
    overlay: Res<OverlayMenu>,
    bridge: Res<UiBridge>,
    save: Res<SaveData>,
    score: Option<Res<crate::game::Score>>,
    transition: Res<Transition<AppState>>,
    flash: Res<game_utils_bevy::screen_effects::FlashWhite>,
    locale: Res<LocaleResources>,
    mut channels: ResMut<AudioChannels>,
    loading: Option<Res<AssetsLoading>>,
    asset_server: Res<AssetServer>,
    selected: Res<SelectedCharacter>,
    floor_trans: Option<Res<crate::game::components::FloorTransition>>,
) {
    let Ok(mut ui) = bridge.shared.lock() else {
        return;
    };
    ui.phase = state.get().clone();
    if state.is_changed() && *state.get() == AppState::Title {
        // Menu/Create_0 spawns GoButton with visible = false, but the current
        // player race remains the actual selected race.
        ui.title_go_visible = false;
        ui.loadout_open = false;
        ui.title_hover_race = -1;
        ui.selected_character = selected.0 as usize;
        ui.character = match selected.0 {
            crate::game::content::RaceId::Random => "Random".to_string(),
            r => crate::game::content::character_def(r).name.to_string(),
        };
        ui.portrait_offset = 0.0;
        ui.text_appear = 0.0;
    }
    if state.is_changed() && *state.get() == AppState::MainMenu {
        ui.main_menu_hover = -1;
    }
    ui.paused = paused.0;
    ui.overlay = *overlay;
    ui.high_score = save.high_score;
    ui.score = score.map(|s| s.0).unwrap_or(0);
    if *overlay != OverlayMenu::Settings {
        ui.master_vol = save.settings.master_volume;
        ui.sfx_vol = save.settings.sfx_volume;
        ui.music_vol = save.settings.music_volume;
    }
    ui.transition_alpha = transition.overlay_alpha;
    ui.flash_alpha = flash.amount;
    ui.language = locale.current.clone();
    ui.available_languages = locale.available.clone();
    ui.translations = i18n::get_current_translations(&locale);
    ui.loading_progress = match loading {
        Some(l) if !l.0.is_empty() => {
            l.0.iter()
                .filter(|h| asset_server.is_loaded_with_dependencies(h.id()))
                .count() as f32
                / l.0.len() as f32
        }
        _ => 1.0,
    };
    if let Some(ft) = floor_trans {
        ui.gen_active = ft.active;
        ui.gen_progress = ft.progress;
        ui.gen_tip = ft.tip.clone();
    } else {
        ui.gen_active = false;
        ui.gen_progress = 0.0;
        ui.gen_tip.clear();
    }
    channels.master = save.settings.master_volume;
    channels.sfx = save.settings.sfx_volume;
    channels.music = save.settings.music_volume;
    // Loadout summary for title screen
    {
        let sel = selected.0;
        let lo = save.race_loadout(sel);
        let def = crate::game::content::character_def(sel);
        ui.character = def.name.to_string();
        ui.selected_character = sel as usize;
        // Persisted cskin pick drives the menu portrait + loadout highlight.
        ui.selected_skin = lo.preferred_skin;
        let equipped_start = crate::game::content::resolve_start_weapon(lo.start_weapon);

        ui.start_weapon_name = crate::game::content::weapon_id_name(equipped_start).to_string();

        ui.stored_weapon_name = if lo.stored_weapon == crate::game::content::WEAPON_NONE {
            "NONE".to_string()
        } else {
            crate::game::content::weapon_id_name(lo.stored_weapon).to_string()
        };

        ui.crown = crate::game::content::crown_short_name(lo.start_crown).to_string();
        ui.start_weapon_id = equipped_start.0;
        ui.stored_weapon_id = lo.stored_weapon.0;
        // Grid frames + highlight compare against nt-rewrite crwn_* ids.
        ui.crown_id = crate::game::content::crown_port_to_gml(lo.start_crown);
        ui.loadout_summary = format!(
            "{} | start {} | stored {} | crown {} | {}",
            def.name,
            crate::game::content::weapon_id_name(equipped_start),
            crate::game::content::weapon_id_name(lo.stored_weapon),
            crate::game::content::crown_short_name(lo.start_crown),
            crate::game::content::ability_name(def.ability)
        );
    }
}

fn tick_pending_unpause(
    real: Res<Time<Real>>,
    mut pending: ResMut<PendingUnpause>,
    mut paused: ResMut<Paused>,
) {
    let Some(timer) = pending.0.as_mut() else {
        return;
    };
    if timer.tick(real.delta()).just_finished() {
        pending.0 = None;
        paused.0 = false;
    }
}

fn play_ui_sfx(
    commands: &mut Commands,
    asset_server: &AssetServer,
    catalog: &AssetCatalog,
    stem: &str,
    volume: f32,
) {
    let Some(path) = catalog.resolve_audio_path(stem) else {
        bevy::log::warn!("missing ui sfx: {stem}");
        return;
    };

    commands.spawn((
        AudioPlayer::<AudioSource>::new(asset_server.load(path)),
        PlaybackSettings {
            mode: PlaybackMode::Despawn,
            volume: Volume::Linear(volume),
            ..default()
        },
    ));
}

fn set_vol(bridge: &UiBridge, field: impl Fn(&mut SharedUi) -> &mut f32, v: f32) -> f32 {
    let v = v.clamp(0.0, 1.0);
    if let Ok(mut ui) = bridge.shared.lock() {
        *field(&mut ui) = v;
    }
    v
}

fn process_ui_actions(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    catalog: Res<AssetCatalog>,
    bridge: Res<UiBridge>,
    mut paused: ResMut<Paused>,
    mut overlay: ResMut<OverlayMenu>,
    mut save: ResMut<SaveData>,
    mut exit: MessageWriter<AppExit>,
    mut transition: ResMut<Transition<AppState>>,
    mut next_state: ResMut<NextState<AppState>>,
    manager: Res<SaveManager>,
    mut pending_unpause: ResMut<PendingUnpause>,
    mut locale: ResMut<LocaleResources>,
    mut channels: ResMut<AudioChannels>,
    mut selected: ResMut<SelectedCharacter>,
    mut mutation_choice: ResMut<MutationChoice>,
) {
    let Ok(mut q) = bridge.actions.lock() else {
        return;
    };
    for action in q.drain(..) {
        match action {
            UiAction::StartGame => {
                if let Ok(mut ui) = bridge.shared.lock() {
                    ui.title_go_visible = false;
                    ui.title_hover_race = -1;
                }
                // Loading uses Loading state spiral (GenCont) - simple fade, vortex stays via Loading state
                transition.begin_to_state(AppState::Loading);
            }
            UiAction::MainMenuPlay => {
                play_ui_sfx(&mut commands, &asset_server, &catalog, "sndClick", 0.7);
                play_ui_sfx(
                    &mut commands,
                    &asset_server,
                    &catalog,
                    "sndMenuCharSelect",
                    0.7,
                );
                if let Ok(mut ui) = bridge.shared.lock() {
                    ui.title_go_visible = false;
                    ui.title_hover_race = -1;
                    ui.loadout_open = false;
                }
                // OG PlayButton/Other_10 is instant - SpiralCont wisps linger via grow*=1.5 drain
                transition.active = false;
                transition.phase = game_utils_bevy::transitions::TransitionPhase::Idle;
                transition.progress = 0.0;
                transition.overlay_alpha = 0.0;
                transition.circle_progress = 0.0;
                transition.block_input = false;
                transition.pending_state = None;
                next_state.set(AppState::Title);
            }
            UiAction::OpenSettings => {
                if let Ok(mut ui) = bridge.shared.lock() {
                    ui.saved_language = locale.current.clone();
                }
                *overlay = OverlayMenu::Settings;
            }
            UiAction::OpenCredits => *overlay = OverlayMenu::Credits,
            UiAction::CloseOverlay => {
                if *overlay == OverlayMenu::Settings
                    && let Ok(ui) = bridge.shared.lock()
                {
                    locale.set_locale(&ui.saved_language);
                }
                match *overlay {
                    OverlayMenu::Settings | OverlayMenu::Credits if paused.0 => {
                        *overlay = OverlayMenu::Pause;
                    }
                    OverlayMenu::Pause if paused.0 => {
                        *overlay = OverlayMenu::None;
                        pending_unpause.0 = Some(Timer::from_seconds(0.2, TimerMode::Once));
                    }
                    _ => {
                        *overlay = OverlayMenu::None;
                    }
                }
            }
            UiAction::Resume => {
                *overlay = OverlayMenu::None;
                pending_unpause.0 = Some(Timer::from_seconds(0.2, TimerMode::Once));
            }
            UiAction::QuitToTitle => {
                paused.0 = false;
                *overlay = OverlayMenu::None;
                pending_unpause.0 = None;
                // Keep quit-to-menu instant for now; vortex handled in MainMenuPlay.
                // If a wipe is desired here, switch to begin_vortex_to(MainMenu).
                transition.active = false;
                transition.phase = game_utils_bevy::transitions::TransitionPhase::Idle;
                transition.progress = 0.0;
                transition.overlay_alpha = 0.0;
                transition.circle_progress = 0.0;
                transition.block_input = false;
                transition.pending_state = None;
                next_state.set(AppState::MainMenu);
            }
            UiAction::QuitApp => {
                exit.write(AppExit::Success);
            }
            UiAction::SetMasterVol(v) => {
                let v = set_vol(&bridge, |ui| &mut ui.master_vol, v);
                channels.master = v;
            }
            UiAction::SetSfxVol(v) => {
                let v = set_vol(&bridge, |ui| &mut ui.sfx_vol, v);
                channels.sfx = v;
            }
            UiAction::SetMusicVol(v) => {
                let v = set_vol(&bridge, |ui| &mut ui.music_vol, v);
                channels.music = v;
            }
            UiAction::SaveSettings => {
                if let Ok(ui) = bridge.shared.lock() {
                    save.settings.master_volume = ui.master_vol;
                    save.settings.sfx_volume = ui.sfx_vol;
                    save.settings.music_volume = ui.music_vol;
                    save.settings.language = locale.current.clone();
                }
                let _ = manager.save(&*save);
                if let Ok(mut ui) = bridge.shared.lock() {
                    ui.saved_language = locale.current.clone();
                }
                if paused.0 {
                    *overlay = OverlayMenu::Pause;
                } else {
                    *overlay = OverlayMenu::None;
                }
            }
            UiAction::NextLanguage => {
                let available = locale.available.clone();
                if available.is_empty() {
                    continue;
                }
                let current = locale.current.clone();
                let idx = available.iter().position(|l| *l == current).unwrap_or(0);
                let next = (idx + 1) % available.len();
                if let Some(next_locale) = available.get(next) {
                    locale.set_locale(next_locale);
                }
            }
            UiAction::SetLanguage(ref lang) => {
                if locale.available.contains(lang) {
                    locale.set_locale(lang);
                }
            }
            UiAction::SelectCharacter(i) => {
                let Some(race) = crate::game::content::race_from_gml_id(i) else {
                    continue;
                };

                if !save.race_unlocked(race) {
                    play_ui_sfx(&mut commands, &asset_server, &catalog, "sndNoSelect", 0.5);
                    continue;
                }

                let already_selected = bridge
                    .shared
                    .lock()
                    .map(|ui| ui.selected_character == i)
                    .unwrap_or(false);

                if already_selected {
                    if let Ok(mut ui) = bridge.shared.lock() {
                        ui.title_go_visible = false;
                        ui.title_hover_race = -1;
                    }
                    transition.begin_to_state(AppState::Loading);
                    continue;
                }

                // scrCampfireMenuSelectionChange
                selected.0 = race;
                let lo = save.race_loadout(race);

                if let Ok(mut ui) = bridge.shared.lock() {
                    ui.selected_character = i;
                    ui.character = match race {
                        crate::game::content::RaceId::Random => "Random".to_string(),
                        r => crate::game::content::character_def(r).name.to_string(),
                    };
                    ui.title_go_visible = true;
                    ui.selected_skin = lo.preferred_skin;
                    ui.crown_id = crate::game::content::crown_port_to_gml(lo.start_crown);
                    let start = crate::game::content::resolve_start_weapon(lo.start_weapon);
                    ui.start_weapon_id = start.0;
                    ui.stored_weapon_id = lo.stored_weapon.0;
                    ui.start_weapon_name = crate::game::content::weapon_id_name(start).to_string();
                    ui.stored_weapon_name = if lo.stored_weapon.0 == 0 {
                        "NONE".to_string()
                    } else {
                        crate::game::content::weapon_id_name(lo.stored_weapon).to_string()
                    };
                    if matches!(
                        race,
                        crate::game::content::RaceId::BigDog
                            | crate::game::content::RaceId::Skeleton
                            | crate::game::content::RaceId::Frog
                    ) {
                        ui.loadout_open = false;
                    }
                    ui.portrait_offset = 180.0;
                    ui.text_appear = 2.0;
                    ui.selection_epoch = ui.selection_epoch.wrapping_add(1);
                }

                let race_sfx = format!(
                    "snd{}Slct",
                    match race {
                        crate::game::content::RaceId::Fish => "Fish",
                        crate::game::content::RaceId::Crystal => "Crystal",
                        crate::game::content::RaceId::Eyes => "Eyes",
                        crate::game::content::RaceId::Melting => "Melting",
                        crate::game::content::RaceId::Plant => "Plant",
                        crate::game::content::RaceId::Venuz => "Venuz",
                        crate::game::content::RaceId::Steroids => "Steroids",
                        crate::game::content::RaceId::Robot => "Robot",
                        crate::game::content::RaceId::Chicken => "Chicken",
                        crate::game::content::RaceId::Rebel => "Rebel",
                        crate::game::content::RaceId::Horror => "Horror",
                        crate::game::content::RaceId::Rogue => "Rogue",
                        crate::game::content::RaceId::BigDog => "Dog",
                        crate::game::content::RaceId::Skeleton => "Skeleton",
                        crate::game::content::RaceId::Frog => "Frog",
                        crate::game::content::RaceId::Cuz => "Cuz",
                        crate::game::content::RaceId::Random => "MenuSelect",
                    }
                );
                if catalog.resolve_audio_path(&race_sfx).is_some() {
                    play_ui_sfx(&mut commands, &asset_server, &catalog, &race_sfx, 1.0);
                } else {
                    play_ui_sfx(&mut commands, &asset_server, &catalog, "sndMenuSelect", 1.0);
                }
            }
            UiAction::SelectSkin(s) => {
                let race = selected.0;
                // Panel is force-closed for Random upstream; zones linger.
                if race == crate::game::content::RaceId::Random {
                    continue;
                }
                // scrMenuDrawLoadout #region Skins: locked -> sndNoSelect;
                // re-clicking the current skin is silent; otherwise persist
                // cskin + play the per-letter cue.
                let already = save.race_loadout(race).preferred_skin == s;
                if !already && save.skin_unlocked(race, s) {
                    save.race_loadout_mut(race).preferred_skin = s;
                    let _ = manager.save(&*save);
                    if let Ok(mut ui) = bridge.shared.lock() {
                        ui.selected_skin = s;
                    }
                    let cue = match s {
                        2 => "sndMenuCSkin",
                        1 => "sndMenuBSkin",
                        _ => "sndMenuASkin",
                    };
                    play_ui_sfx(&mut commands, &asset_server, &catalog, cue, 1.0);
                } else if !already {
                    play_ui_sfx(&mut commands, &asset_server, &catalog, "sndNoSelect", 0.5);
                }
            }
            UiAction::ToggleLoadout => {
                if let Ok(mut ui) = bridge.shared.lock() {
                    ui.loadout_open = !ui.loadout_open;
                }
            }
            UiAction::CycleStartWeapon(_) => {
                let race = selected.0;
                let lo = save.race_loadout_mut(race);

                // The start selector toggles between the normal Revolver and this race's
                // stored weapon. It is not a global weapon browser.
                if lo.stored_weapon.0 != 0 {
                    lo.start_weapon = if lo.start_weapon.0 == 0 {
                        lo.stored_weapon
                    } else {
                        crate::game::content::WEAPON_NONE
                    };

                    let _ = manager.save(&*save);
                }
            }
            UiAction::CycleStoredWeapon(_) => {
                // Stored weapons are earned and persisted by gameplay. The character
                // menu must not manufacture/cycle arbitrary stored weapons.
            }
            UiAction::CycleCrown(dir) => {
                let race = selected.0;
                // Only cycle through unlocked crowns, matching GML's crowngot check
                let mut next = save.race_loadout(race).start_crown;
                for _ in 0..crate::game::content::CrownKind::ALL.len() {
                    next = crate::game::content::cycle_crown_id(next, dir);
                    let gml = crate::game::content::crown_port_to_gml(next);
                    if save.crown_unlocked(race, gml) || next == 0 {
                        break;
                    }
                }
                let lo = save.race_loadout_mut(race);
                lo.start_crown = next;
                let _ = manager.save(&*save);
            }
            UiAction::SelectCrown(crown_id) => {
                let race = selected.0;
                // Upstream force-closes the panel for Random; zones can
                // linger, so treat it as a locked press.
                if race == crate::game::content::RaceId::Random {
                    continue;
                }
                // scrMenuDrawLoadout: unlocked -> equip + sndMenuCrown,
                // locked -> sndNoSelect. Zone ids are GML crwn_* ids; the
                // loadout stores port CrownKind ids (NONE=0, Death=1, ...).
                let already = save.race_loadout(race).start_crown
                    == crate::game::content::crown_gml_to_port(crown_id);
                if !already && save.crown_unlocked(race, crown_id) {
                    save.race_loadout_mut(race).start_crown =
                        crate::game::content::crown_gml_to_port(crown_id);
                    let _ = manager.save(&*save);
                    play_ui_sfx(&mut commands, &asset_server, &catalog, "sndMenuCrown", 1.0);
                } else if !already {
                    play_ui_sfx(&mut commands, &asset_server, &catalog, "sndNoSelect", 0.5);
                }
            }
            UiAction::SelectMutation(idx) => {
                // GML SkillIcon: first click = select (sndHover), second = confirm
                let already = bridge
                    .shared
                    .lock()
                    .map(|ui| ui.mutation_selected == Some(idx))
                    .unwrap_or(false);
                if already {
                    mutation_choice.0 = Some(idx);
                    if let Ok(mut ui) = bridge.shared.lock() {
                        ui.mutation_selected = None;
                    }
                } else {
                    if let Ok(mut ui) = bridge.shared.lock() {
                        ui.mutation_selected = Some(idx);
                    }
                    play_ui_sfx(&mut commands, &asset_server, &catalog, "sndHover", 0.45);
                }
            }
            UiAction::PickMutation(idx) => {
                // Direct confirm (used when already selected, or via keyboard 1-4 second press)
                let selected = bridge
                    .shared
                    .lock()
                    .map(|ui| ui.mutation_selected)
                    .unwrap_or(None);
                if selected == Some(idx) {
                    mutation_choice.0 = Some(idx);
                    if let Ok(mut ui) = bridge.shared.lock() {
                        ui.mutation_selected = None;
                    }
                } else {
                    // First press selects, second will pick - mirror SelectMutation
                    if let Ok(mut ui) = bridge.shared.lock() {
                        ui.mutation_selected = Some(idx);
                    }
                    play_ui_sfx(&mut commands, &asset_server, &catalog, "sndHover", 0.45);
                }
            }
        }
    }
}

fn force_death_overlay_state(
    run: Option<Res<crate::game::components::Run>>,
    mut paused: ResMut<Paused>,
    mut overlay: ResMut<OverlayMenu>,
    mut pending_unpause: ResMut<PendingUnpause>,
) {
    let Some(run) = run else {
        return;
    };

    if run.game_over {
        paused.0 = false;
        *overlay = OverlayMenu::None;
        pending_unpause.0 = None;
    }
}

fn handle_pause_input(
    keys: Res<ButtonInput<KeyCode>>,
    state: Res<State<AppState>>,
    run: Option<Res<crate::game::components::Run>>,
    mut paused: ResMut<Paused>,
    mut overlay: ResMut<OverlayMenu>,
    mut pending_unpause: ResMut<PendingUnpause>,
    transition: Res<Transition<AppState>>,
) {
    if *state.get() != AppState::InGame {
        return;
    }
    if transition.block_input {
        return;
    }
    if run.as_deref().is_some_and(|run| run.game_over) {
        return;
    }
    if !keys.just_pressed(KeyCode::Escape) {
        return;
    }
    match *overlay {
        OverlayMenu::None if !paused.0 => {
            paused.0 = true;
            *overlay = OverlayMenu::Pause;
            pending_unpause.0 = None;
        }
        OverlayMenu::Pause => {
            *overlay = OverlayMenu::None;
            pending_unpause.0 = Some(Timer::from_seconds(0.2, TimerMode::Once));
        }
        OverlayMenu::Settings | OverlayMenu::Credits => {
            if paused.0 {
                *overlay = OverlayMenu::Pause;
            } else {
                *overlay = OverlayMenu::None;
            }
        }
        _ => {}
    }
}

fn handle_mutation_keys(
    keys: Res<ButtonInput<KeyCode>>,
    state: Res<State<AppState>>,
    bridge: Res<UiBridge>,
) {
    if *state.get() != AppState::InGame {
        return;
    }
    let Ok(ui) = bridge.shared.lock() else {
        return;
    };
    if ui.mutation_choices.is_empty() || ui.gen_active || ui.game_over {
        return;
    }
    let len = ui.mutation_choices.len();
    drop(ui);
    let idx = if keys.just_pressed(KeyCode::Digit1) {
        Some(0)
    } else if keys.just_pressed(KeyCode::Digit2) {
        Some(1)
    } else if keys.just_pressed(KeyCode::Digit3) {
        Some(2)
    } else if keys.just_pressed(KeyCode::Digit4) {
        Some(3)
    } else {
        None
    };
    if let Some(i) = idx {
        if i >= len {
            return;
        }
        // Push via bridge like UI click - respects two-step select/confirm
        if let Ok(mut q) = bridge.actions.lock() {
            // Check if already selected
            let already = bridge
                .shared
                .lock()
                .map(|ui| ui.mutation_selected == Some(i))
                .unwrap_or(false);
            if already {
                q.push(UiAction::PickMutation(i));
            } else {
                q.push(UiAction::SelectMutation(i));
            }
        }
    }
}

fn handle_death_restart(
    keys: Res<ButtonInput<KeyCode>>,
    state: Res<State<AppState>>,
    run: Option<Res<crate::game::components::Run>>,
    mut transition: ResMut<Transition<AppState>>,
    bridge: Res<UiBridge>,
) {
    if *state.get() != AppState::InGame {
        return;
    }
    let Some(run) = run else {
        return;
    };
    if !run.game_over {
        return;
    }
    if !keys.just_pressed(KeyCode::KeyR) {
        return;
    }
    if let Ok(mut ui) = bridge.shared.lock() {
        ui.title_go_visible = false;
        ui.title_hover_race = -1;
    }
    transition.begin_to_state(AppState::Loading);
}

fn sync_virtual_time_with_pause(paused: Res<Paused>, mut ctrl: ResMut<TimeScaleControl>) {
    ctrl.paused = paused.0;
}
