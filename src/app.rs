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

    MainMenu,

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

    pub mutation_choice_ids: Vec<u8>,

    pub mutation_selected: Option<usize>,
    pub game_over: bool,

    pub death_mutation_ids: Vec<u8>,
    pub character: String,

    pub selected_character: usize,

    pub title_go_visible: bool,

    pub loadout_open: bool,

    pub title_hover_race: i32,

    pub main_menu_hover: i32,

    pub boot_mode: u8,

    pub weapon_ammo: [i32; 2],
    pub best_floor: u32,
    pub total_kills: u32,
    pub loadout_summary: String,
    pub start_weapon_name: String,
    pub stored_weapon_name: String,
    pub crown: String,

    pub start_weapon_id: u8,
    pub stored_weapon_id: u8,
    pub crown_id: u8,
    pub selected_skin: u8,

    pub portrait_offset: f32,

    pub text_appear: f32,

    pub selection_epoch: u32,

    pub viewport_width: f32,
    pub viewport_height: f32,
    pub hud_compact: bool,

    pub gen_active: bool,
    pub gen_progress: f32,
    pub gen_tip: String,
    pub run_id: u32,

    pub settings_page: u8,
    pub settings_page_stack: Vec<u8>,

    pub ambience_vol: f32,
    pub volume_3dsound: bool,
    pub screenshake: f32,
    pub freezeframes: f32,
    pub bloom: bool,
    pub particles: bool,
    pub show_hud: bool,
    pub show_timer: bool,
    pub show_area: bool,
    pub boss_intros: bool,
    pub auto_pause: bool,
    pub pause_button: bool,
    pub achievements_popup: bool,
    pub vsync: bool,
    pub fullscreen: bool,
    pub widescreen: bool,
    pub crosshair: u8,
    pub sideart: u8,
    pub pixel_mode: u8,
    pub gamepad_enabled: bool,
    pub gamepad_type: u8,
    pub aim_assist: bool,
    pub auto_aim: bool,
    pub volume_controls: bool,
    pub split_fire: bool,
    pub fixed_sight: bool,
    pub controls_scale: f32,
    pub show_tutorial: bool,
    pub player_color_hex: String,
    pub profile_name: String,
    pub cprefs: [bool; 8],

    pub pause_confirm: Option<u8>,
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
            settings_page: 0,
            settings_page_stack: Vec::new(),
            ambience_vol: 1.0,
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
            cprefs: [true, true, false, true, true, true, true, false],
            pause_confirm: None,
        }
    }
}

// Sim runs at 30 Hz fixed.
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

pub const NT_UI_FONT: &[u8] = include_bytes!("../assets/fonts/Silkscreen-Regular.ttf");

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
                        let st = match shared_ui.lock() {
                            Ok(g) => g.clone(),
                            Err(poisoned) => {
                                bevy::log::warn!("SharedUi mutex poisoned, recovering");
                                poisoned.into_inner().clone()
                            }
                        };
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

                    sync_shared_ui,
                    sync_virtual_time_with_pause,
                )
                    .chain(),
            )
            .add_systems(OnExit(AppState::InGame), reset_pause_on_exit)
            .add_systems(OnEnter(AppState::MainMenu), reset_pause_on_exit)
            .add_systems(OnEnter(AppState::Title), reset_pause_on_exit)
            .add_systems(OnEnter(AppState::Splash), reset_pause_on_exit);
    }
}

fn sync_ui_viewport(windows: Query<&Window, With<PrimaryWindow>>, bridge: Res<UiBridge>) {
    let Ok(window) = windows.single() else {
        return;
    };

    let mut ui = match bridge.shared.lock() {
        Ok(g) => g,
        Err(p) => {
            bevy::log::warn!("SharedUi mutex poisoned in sync_ui_viewport");
            p.into_inner()
        }
    };

    ui.viewport_width = window.width();
    ui.viewport_height = window.height();
    ui.hud_compact = crate::menus::is_compact_viewport(ui.viewport_width, ui.viewport_height);
}

fn lock_shared(
    bridge: &crate::menus::UiBridge,
) -> Option<std::sync::MutexGuard<'_, crate::app::SharedUi>> {
    match bridge.shared.lock() {
        Ok(g) => Some(g),
        Err(p) => {
            bevy::log::warn!("SharedUi mutex poisoned");
            Some(p.into_inner())
        }
    }
}
fn lock_actions(
    bridge: &crate::menus::UiBridge,
) -> Option<std::sync::MutexGuard<'_, Vec<crate::menus::UiAction>>> {
    match bridge.actions.lock() {
        Ok(g) => Some(g),
        Err(p) => {
            bevy::log::warn!("UiAction mutex poisoned");
            Some(p.into_inner())
        }
    }
}
fn lock_shared_mut(
    bridge: &crate::menus::UiBridge,
) -> Option<std::sync::MutexGuard<'_, crate::app::SharedUi>> {
    lock_shared(bridge)
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
    let mut ui = match bridge.shared.lock() {
        Ok(g) => g,
        Err(p) => {
            bevy::log::warn!("SharedUi mutex poisoned in sync_shared_ui");
            p.into_inner()
        }
    };
    ui.phase = state.get().clone();
    if state.is_changed() && *state.get() == AppState::Title {

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
        ui.ambience_vol = save.settings.ambience_volume;
        ui.volume_3dsound = save.settings.volume_3dsound;
        ui.screenshake = save.settings.screenshake;
        ui.freezeframes = save.settings.freezeframes;
        ui.bloom = save.settings.bloom;
        ui.particles = save.settings.particles;
        ui.show_hud = save.settings.show_hud;
        ui.show_timer = save.settings.show_timer;
        ui.show_area = save.settings.show_area;
        ui.boss_intros = save.settings.boss_intros;
        ui.auto_pause = save.settings.auto_pause;
        ui.pause_button = save.settings.pause_button;
        ui.achievements_popup = save.settings.achievements_popup;
        ui.vsync = save.settings.vsync;
        ui.fullscreen = save.settings.fullscreen;
        ui.widescreen = save.settings.widescreen;
        ui.crosshair = save.settings.crosshair;
        ui.sideart = save.settings.sideart;
        ui.pixel_mode = save.settings.pixel_mode;
        ui.gamepad_enabled = save.settings.gamepad_enabled;
        ui.gamepad_type = save.settings.gamepad_type;
        ui.aim_assist = save.settings.aim_assist;
        ui.auto_aim = save.settings.auto_aim;
        ui.volume_controls = save.settings.volume_controls;
        ui.split_fire = save.settings.split_fire;
        ui.fixed_sight = save.settings.fixed_sight;
        ui.controls_scale = save.settings.controls_scale;
        ui.show_tutorial = save.settings.show_tutorial;
        ui.player_color_hex = save.settings.player_color_hex.clone();
        ui.profile_name = save.settings.profile_name.clone();
        ui.cprefs = [
            save.settings.cprefs_eyes,
            save.settings.cprefs_melting,
            save.settings.cprefs_plant,
            save.settings.cprefs_yv,
            save.settings.cprefs_steroids,
            save.settings.cprefs_horror,
            save.settings.cprefs_rogue,
            save.settings.cprefs_skeleton,
        ];
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

    {
        let sel = selected.0;
        let lo = save.race_loadout(sel);
        let def = crate::game::content::character_def(sel);
        ui.character = def.name.to_string();
        ui.selected_character = sel as usize;

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
    let mut guard = match bridge.shared.lock() {
        Ok(g) => g,
        Err(p) => {
            bevy::log::warn!("SharedUi poisoned in set_vol");
            p.into_inner()
        }
    };
    *field(&mut *guard) = v;
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
    let actions: Vec<UiAction> = match bridge.actions.lock() {
        Ok(mut q) => std::mem::take(&mut *q),
        Err(p) => {
            bevy::log::warn!("UiAction mutex poisoned, recovering");
            let mut inner = p.into_inner();
            std::mem::take(&mut *inner)
        }
    };
    for action in actions {
        match action {
            UiAction::StartGame => {
                if let Some(mut ui) = lock_shared(&bridge) {
                    ui.title_go_visible = false;
                    ui.title_hover_race = -1;
                }

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
                if let Some(mut ui) = lock_shared(&bridge) {
                    ui.title_go_visible = false;
                    ui.title_hover_race = -1;
                    ui.loadout_open = false;
                }

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
                if let Some(mut ui) = lock_shared(&bridge) {
                    ui.saved_language = locale.current.clone();
                    ui.settings_page = 0;
                    ui.settings_page_stack.clear();
                    ui.pause_confirm = None;
                }
                *overlay = OverlayMenu::Settings;
            }
            UiAction::SettingsCategory(cat) => {
                if let Some(mut ui) = lock_shared(&bridge) {
                    let cur = ui.settings_page;
                    ui.settings_page_stack.push(cur);
                    ui.settings_page = cat;
                }
                play_ui_sfx(&mut commands, &asset_server, &catalog, "sndClick", 0.7);
            }
            UiAction::SettingsBack => {
                let should_close = {
                    match lock_shared(&bridge) {
                        Some(mut ui) => {
                            if let Some(prev) = ui.settings_page_stack.pop() {
                                ui.settings_page = prev;
                                false
                            } else if ui.settings_page != 0 {
                                ui.settings_page = 0;
                                false
                            } else {
                                true
                            }
                        }
                        None => true,
                    }
                };
                if should_close {
                    if *overlay == OverlayMenu::Settings
                        && let Some(ui) = lock_shared(&bridge)
                    {
                        locale.set_locale(&ui.saved_language);
                    }
                    match *overlay {
                        OverlayMenu::Settings | OverlayMenu::Credits if paused.0 => {
                            *overlay = OverlayMenu::Pause;
                            if let Some(mut ui) = lock_shared(&bridge) {
                                ui.settings_page = 0;
                                ui.settings_page_stack.clear();
                            }
                        }
                        _ => {
                            *overlay = OverlayMenu::None;
                            if let Some(mut ui) = lock_shared(&bridge) {
                                ui.settings_page = 0;
                                ui.settings_page_stack.clear();
                            }
                        }
                    }
                } else {
                    play_ui_sfx(&mut commands, &asset_server, &catalog, "sndClickBack", 0.6);
                }
            }
            UiAction::ShowPauseConfirm(kind) => {
                if let Some(mut ui) = lock_shared(&bridge) {
                    ui.pause_confirm = Some(kind);
                }
                play_ui_sfx(&mut commands, &asset_server, &catalog, "sndClick", 0.7);
            }
            UiAction::CancelPauseConfirm => {
                if let Some(mut ui) = lock_shared(&bridge) {
                    ui.pause_confirm = None;
                }
                play_ui_sfx(&mut commands, &asset_server, &catalog, "sndClickBack", 0.6);
            }
            UiAction::ConfirmPause(kind) => {
                if let Some(mut ui) = lock_shared(&bridge) {
                    ui.pause_confirm = None;
                }
                if kind == 0 {

                    paused.0 = false;
                    *overlay = OverlayMenu::None;
                    pending_unpause.0 = None;
                    if let Some(mut ui) = lock_shared(&bridge) {
                        ui.paused = false;
                        ui.overlay = OverlayMenu::None;
                        ui.phase = AppState::MainMenu;
                        ui.game_over = false;
                        ui.settings_page = 0;
                        ui.settings_page_stack.clear();
                    }
                    transition.active = false;
                    transition.phase = game_utils_bevy::transitions::TransitionPhase::Idle;
                    transition.progress = 0.0;
                    transition.overlay_alpha = 0.0;
                    transition.circle_progress = 0.0;
                    transition.block_input = false;
                    transition.pending_state = None;
                    next_state.set(AppState::MainMenu);
                    play_ui_sfx(&mut commands, &asset_server, &catalog, "sndClick", 0.7);
                } else {

                    paused.0 = false;
                    *overlay = OverlayMenu::None;
                    pending_unpause.0 = None;
                    if let Some(mut ui) = lock_shared(&bridge) {
                        ui.paused = false;
                        ui.overlay = OverlayMenu::None;
                        ui.game_over = false;
                    }
                    transition.begin_to_state(AppState::Loading);
                    play_ui_sfx(&mut commands, &asset_server, &catalog, "sndClick", 0.7);
                }
            }
            UiAction::OpenCredits => *overlay = OverlayMenu::Credits,
            UiAction::CloseOverlay => {
                if *overlay == OverlayMenu::Settings
                    && let Some(ui) = lock_shared(&bridge)
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
                if let Some(mut ui) = lock_shared(&bridge) {
                    ui.pause_confirm = None;
                }
            }
            UiAction::QuitToTitle => {
                paused.0 = false;
                *overlay = OverlayMenu::None;
                pending_unpause.0 = None;

                if let Some(mut ui) = lock_shared(&bridge) {
                    ui.paused = false;
                    ui.overlay = OverlayMenu::None;
                    ui.phase = AppState::MainMenu;
                    ui.game_over = false;
                }

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
                save.settings.master_volume = v;
                if let Err(e) = manager.save(&*save) {
                    bevy::log::error!("save failed: {e}");
                }
                play_ui_sfx(
                    &mut commands,
                    &asset_server,
                    &catalog,
                    "sndSliderLetGo",
                    0.5,
                );
            }
            UiAction::SetSfxVol(v) => {
                let v = set_vol(&bridge, |ui| &mut ui.sfx_vol, v);
                channels.sfx = v;
                save.settings.sfx_volume = v;
                if let Err(e) = manager.save(&*save) {
                    bevy::log::error!("save failed: {e}");
                }
                play_ui_sfx(
                    &mut commands,
                    &asset_server,
                    &catalog,
                    "sndSliderLetGo",
                    0.5,
                );
            }
            UiAction::SetMusicVol(v) => {
                let v = set_vol(&bridge, |ui| &mut ui.music_vol, v);
                channels.music = v;
                save.settings.music_volume = v;
                if let Err(e) = manager.save(&*save) {
                    bevy::log::error!("save failed: {e}");
                }
                play_ui_sfx(
                    &mut commands,
                    &asset_server,
                    &catalog,
                    "sndSliderLetGo",
                    0.5,
                );
            }
            UiAction::SetAmbienceVol(v) => {
                let v = set_vol(&bridge, |ui| &mut ui.ambience_vol, v);
                save.settings.ambience_volume = v;
                if let Err(e) = manager.save(&*save) {
                    bevy::log::error!("save failed: {e}");
                }
                play_ui_sfx(
                    &mut commands,
                    &asset_server,
                    &catalog,
                    "sndSliderLetGo",
                    0.5,
                );
            }
            UiAction::SaveSettings => {
                if let Some(ui) = lock_shared(&bridge) {
                    save.settings.master_volume = ui.master_vol;
                    save.settings.sfx_volume = ui.sfx_vol;
                    save.settings.music_volume = ui.music_vol;
                    save.settings.ambience_volume = ui.ambience_vol;
                    save.settings.volume_3dsound = ui.volume_3dsound;
                    save.settings.screenshake = ui.screenshake;
                    save.settings.freezeframes = ui.freezeframes;
                    save.settings.bloom = ui.bloom;
                    save.settings.particles = ui.particles;
                    save.settings.show_hud = ui.show_hud;
                    save.settings.show_timer = ui.show_timer;
                    save.settings.show_area = ui.show_area;
                    save.settings.boss_intros = ui.boss_intros;
                    save.settings.auto_pause = ui.auto_pause;
                    save.settings.pause_button = ui.pause_button;
                    save.settings.achievements_popup = ui.achievements_popup;
                    save.settings.vsync = ui.vsync;
                    save.settings.fullscreen = ui.fullscreen;
                    save.settings.widescreen = ui.widescreen;
                    save.settings.crosshair = ui.crosshair;
                    save.settings.sideart = ui.sideart;
                    save.settings.pixel_mode = ui.pixel_mode;
                    save.settings.gamepad_enabled = ui.gamepad_enabled;
                    save.settings.gamepad_type = ui.gamepad_type;
                    save.settings.aim_assist = ui.aim_assist;
                    save.settings.auto_aim = ui.auto_aim;
                    save.settings.volume_controls = ui.volume_controls;
                    save.settings.split_fire = ui.split_fire;
                    save.settings.fixed_sight = ui.fixed_sight;
                    save.settings.controls_scale = ui.controls_scale;
                    save.settings.show_tutorial = ui.show_tutorial;
                    save.settings.player_color_hex = ui.player_color_hex.clone();
                    save.settings.profile_name = ui.profile_name.clone();
                    save.settings.cprefs_eyes = ui.cprefs[0];
                    save.settings.cprefs_melting = ui.cprefs[1];
                    save.settings.cprefs_plant = ui.cprefs[2];
                    save.settings.cprefs_yv = ui.cprefs[3];
                    save.settings.cprefs_steroids = ui.cprefs[4];
                    save.settings.cprefs_horror = ui.cprefs[5];
                    save.settings.cprefs_rogue = ui.cprefs[6];
                    save.settings.cprefs_skeleton = ui.cprefs[7];
                    save.settings.language = locale.current.clone();
                }
                if let Err(e) = manager.save(&*save) {
                    bevy::log::error!("save failed: {e}");
                }
                if let Some(mut ui) = lock_shared(&bridge) {
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

                save.settings.language = lang.clone();
                if let Err(e) = manager.save(&*save) {
                    bevy::log::error!("save failed: {e}");
                }
                if let Some(mut ui) = lock_shared(&bridge) {
                    ui.saved_language = lang.clone();
                }
                play_ui_sfx(&mut commands, &asset_server, &catalog, "sndClick", 0.7);
            }
            UiAction::SettingToggle(ref key) => {
                let toggled = match key.as_str() {
                    "volume_3dsound" => {
                        let nv = !save.settings.volume_3dsound;
                        save.settings.volume_3dsound = nv;
                        if let Some(mut ui) = lock_shared(&bridge) {
                            ui.volume_3dsound = nv;
                        }
                        nv
                    }
                    "bloom" => {
                        let nv = !save.settings.bloom;
                        save.settings.bloom = nv;
                        if let Some(mut ui) = lock_shared(&bridge) {
                            ui.bloom = nv;
                        }
                        nv
                    }
                    "particles" => {
                        let nv = !save.settings.particles;
                        save.settings.particles = nv;
                        if let Some(mut ui) = lock_shared(&bridge) {
                            ui.particles = nv;
                        }
                        nv
                    }
                    "show_hud" => {
                        let nv = !save.settings.show_hud;
                        save.settings.show_hud = nv;
                        if let Some(mut ui) = lock_shared(&bridge) {
                            ui.show_hud = nv;
                        }
                        nv
                    }
                    "show_timer" => {
                        let nv = !save.settings.show_timer;
                        save.settings.show_timer = nv;
                        if let Some(mut ui) = lock_shared(&bridge) {
                            ui.show_timer = nv;
                        }
                        nv
                    }
                    "show_area" => {
                        let nv = !save.settings.show_area;
                        save.settings.show_area = nv;
                        if let Some(mut ui) = lock_shared(&bridge) {
                            ui.show_area = nv;
                        }
                        nv
                    }
                    "boss_intros" => {
                        let nv = !save.settings.boss_intros;
                        save.settings.boss_intros = nv;
                        if let Some(mut ui) = lock_shared(&bridge) {
                            ui.boss_intros = nv;
                        }
                        nv
                    }
                    "auto_pause" => {
                        let nv = !save.settings.auto_pause;
                        save.settings.auto_pause = nv;
                        if let Some(mut ui) = lock_shared(&bridge) {
                            ui.auto_pause = nv;
                        }
                        nv
                    }
                    "pause_button" => {
                        let nv = !save.settings.pause_button;
                        save.settings.pause_button = nv;
                        if let Some(mut ui) = lock_shared(&bridge) {
                            ui.pause_button = nv;
                        }
                        nv
                    }
                    "achievements_popup" => {
                        let nv = !save.settings.achievements_popup;
                        save.settings.achievements_popup = nv;
                        if let Some(mut ui) = lock_shared(&bridge) {
                            ui.achievements_popup = nv;
                        }
                        nv
                    }
                    "vsync" => {
                        let nv = !save.settings.vsync;
                        save.settings.vsync = nv;
                        if let Some(mut ui) = lock_shared(&bridge) {
                            ui.vsync = nv;
                        }
                        nv
                    }
                    "fullscreen" => {
                        let nv = !save.settings.fullscreen;
                        save.settings.fullscreen = nv;
                        if let Some(mut ui) = lock_shared(&bridge) {
                            ui.fullscreen = nv;
                        }
                        nv
                    }
                    "widescreen" => {
                        let nv = !save.settings.widescreen;
                        save.settings.widescreen = nv;
                        if let Some(mut ui) = lock_shared(&bridge) {
                            ui.widescreen = nv;
                        }
                        nv
                    }
                    "gamepad_enabled" => {
                        let nv = !save.settings.gamepad_enabled;
                        save.settings.gamepad_enabled = nv;
                        if let Some(mut ui) = lock_shared(&bridge) {
                            ui.gamepad_enabled = nv;
                        }
                        nv
                    }
                    "aim_assist" => {
                        let nv = !save.settings.aim_assist;
                        save.settings.aim_assist = nv;
                        if let Some(mut ui) = lock_shared(&bridge) {
                            ui.aim_assist = nv;
                        }
                        nv
                    }
                    "auto_aim" => {
                        let nv = !save.settings.auto_aim;
                        save.settings.auto_aim = nv;
                        if let Some(mut ui) = lock_shared(&bridge) {
                            ui.auto_aim = nv;
                        }
                        nv
                    }
                    "volume_controls" => {
                        let nv = !save.settings.volume_controls;
                        save.settings.volume_controls = nv;
                        if let Some(mut ui) = lock_shared(&bridge) {
                            ui.volume_controls = nv;
                        }
                        nv
                    }
                    "split_fire" => {
                        let nv = !save.settings.split_fire;
                        save.settings.split_fire = nv;
                        if let Some(mut ui) = lock_shared(&bridge) {
                            ui.split_fire = nv;
                        }
                        nv
                    }
                    "fixed_sight" => {
                        let nv = !save.settings.fixed_sight;
                        save.settings.fixed_sight = nv;
                        if let Some(mut ui) = lock_shared(&bridge) {
                            ui.fixed_sight = nv;
                        }
                        nv
                    }
                    "show_tutorial" => {
                        let nv = !save.settings.show_tutorial;
                        save.settings.show_tutorial = nv;
                        if let Some(mut ui) = lock_shared(&bridge) {
                            ui.show_tutorial = nv;
                        }
                        nv
                    }
                    s if s.starts_with("cprefs_") => {
                        let idx: usize = s
                            .strip_prefix("cprefs_")
                            .and_then(|x| x.parse().ok())
                            .unwrap_or(99);
                        if idx < 8 {
                            let arr = [
                                save.settings.cprefs_eyes,
                                save.settings.cprefs_melting,
                                save.settings.cprefs_plant,
                                save.settings.cprefs_yv,
                                save.settings.cprefs_steroids,
                                save.settings.cprefs_horror,
                                save.settings.cprefs_rogue,
                                save.settings.cprefs_skeleton,
                            ];
                            let cur = arr[idx];
                            let nv = !cur;
                            match idx {
                                0 => save.settings.cprefs_eyes = nv,
                                1 => save.settings.cprefs_melting = nv,
                                2 => save.settings.cprefs_plant = nv,
                                3 => save.settings.cprefs_yv = nv,
                                4 => save.settings.cprefs_steroids = nv,
                                5 => save.settings.cprefs_horror = nv,
                                6 => save.settings.cprefs_rogue = nv,
                                7 => save.settings.cprefs_skeleton = nv,
                                _ => {}
                            }
                            if let Some(mut ui) = lock_shared(&bridge) {
                                ui.cprefs[idx] = nv;
                            }
                            nv
                        } else {
                            false
                        }
                    }
                    _ => {
                        bevy::log::warn!("unknown toggle key {key}");
                        false
                    }
                };
                let _ = toggled;
                if let Err(e) = manager.save(&*save) {
                    bevy::log::error!("save failed: {e}");
                }
                play_ui_sfx(&mut commands, &asset_server, &catalog, "sndClick", 0.6);
            }
            UiAction::SettingSlider { ref key, value } => {
                let v = value.clamp(0.0, 2.0);
                match key.as_str() {
                    "screenshake" => {
                        save.settings.screenshake = v;
                        if let Some(mut ui) = lock_shared(&bridge) {
                            ui.screenshake = v;
                        }
                    }
                    "freezeframes" => {
                        save.settings.freezeframes = v;
                        if let Some(mut ui) = lock_shared(&bridge) {
                            ui.freezeframes = v;
                        }
                    }
                    "controls_scale" => {
                        let c = v.clamp(0.0, 1.0);
                        save.settings.controls_scale = c;
                        if let Some(mut ui) = lock_shared(&bridge) {
                            ui.controls_scale = c;
                        }
                    }
                    _ => {
                        bevy::log::warn!("unknown slider {key}");
                    }
                }
                if let Err(e) = manager.save(&*save) {
                    bevy::log::error!("save failed: {e}");
                }
                play_ui_sfx(
                    &mut commands,
                    &asset_server,
                    &catalog,
                    "sndSliderLetGo",
                    0.5,
                );
            }
            UiAction::SettingCycle { ref key, dir } => {
                match key.as_str() {
                    "crosshair" => {
                        let nv = (save.settings.crosshair as i16 + dir as i16).rem_euclid(4) as u8;
                        save.settings.crosshair = nv;
                        if let Some(mut ui) = lock_shared(&bridge) {
                            ui.crosshair = nv;
                        }
                    }
                    "sideart" => {
                        let nv = (save.settings.sideart as i16 + dir as i16).rem_euclid(4) as u8;
                        save.settings.sideart = nv;
                        if let Some(mut ui) = lock_shared(&bridge) {
                            ui.sideart = nv;
                        }
                    }
                    "pixel_mode" => {
                        let nv = ((save.settings.pixel_mode as i16 - 1 + dir as i16).rem_euclid(4)
                            + 1) as u8;
                        save.settings.pixel_mode = nv;
                        if let Some(mut ui) = lock_shared(&bridge) {
                            ui.pixel_mode = nv;
                        }
                    }
                    "gamepad_type" => {
                        let nv =
                            (save.settings.gamepad_type as i16 + dir as i16).rem_euclid(4) as u8;
                        save.settings.gamepad_type = nv;
                        if let Some(mut ui) = lock_shared(&bridge) {
                            ui.gamepad_type = nv;
                        }
                    }
                    _ => {
                        bevy::log::warn!("unknown cycle {key}");
                    }
                }
                if let Err(e) = manager.save(&*save) {
                    bevy::log::error!("save failed: {e}");
                }
                play_ui_sfx(&mut commands, &asset_server, &catalog, "sndClick", 0.6);
            }
            UiAction::SettingInput { ref key, ref value } => {
                match key.as_str() {
                    "player_color_hex" => {
                        save.settings.player_color_hex = value.clone();
                        if let Some(mut ui) = lock_shared(&bridge) {
                            ui.player_color_hex = value.clone();
                        }
                    }
                    "profile_name" => {
                        save.settings.profile_name = value.clone();
                        if let Some(mut ui) = lock_shared(&bridge) {
                            ui.profile_name = value.clone();
                        }
                    }
                    _ => {
                        bevy::log::warn!("unknown input {key}");
                    }
                }
                if let Err(e) = manager.save(&*save) {
                    bevy::log::error!("save failed: {e}");
                }
                play_ui_sfx(&mut commands, &asset_server, &catalog, "sndClick", 0.6);
            }
            UiAction::SettingResetOptions => {
                *save = SaveData::default();

                if let Err(e) = manager.save(&*save) {
                    bevy::log::error!("save failed: {e}");
                }
                if let Some(mut ui) = lock_shared(&bridge) {
                    ui.master_vol = save.settings.master_volume;
                    ui.sfx_vol = save.settings.sfx_volume;
                    ui.music_vol = save.settings.music_volume;
                    ui.ambience_vol = save.settings.ambience_volume;
                    ui.volume_3dsound = save.settings.volume_3dsound;
                    ui.screenshake = save.settings.screenshake;
                    ui.bloom = save.settings.bloom;
                }
                play_ui_sfx(&mut commands, &asset_server, &catalog, "sndClick", 0.7);
            }
            UiAction::SettingEraseProgress => {
                save.high_score = 0;
                save.best_floor = 0;
                save.total_runs = 0;
                save.total_kills = 0;
                save.unlocked_characters = vec!["Fish".to_string()];
                save.races.clear();
                save.crown_got.clear();
                if let Err(e) = manager.save(&*save) {
                    bevy::log::error!("save failed: {e}");
                }
                play_ui_sfx(&mut commands, &asset_server, &catalog, "sndClick", 0.7);
            }
            UiAction::SettingViewCredits => {
                *overlay = OverlayMenu::Credits;
                play_ui_sfx(
                    &mut commands,
                    &asset_server,
                    &catalog,
                    "sndMenuCredits",
                    0.7,
                );
            }
            UiAction::SettingOpenSubcategory(cat) => {
                if let Some(mut ui) = lock_shared(&bridge) {
                    let cur = ui.settings_page;
                    ui.settings_page_stack.push(cur);
                    ui.settings_page = cat;
                }
                play_ui_sfx(&mut commands, &asset_server, &catalog, "sndClick", 0.7);
            }
            UiAction::SelectCharacter(i) => {
                let Some(race) = crate::game::content::race_from_gml_id(i) else {
                    continue;
                };

                if !save.race_unlocked(race) {
                    play_ui_sfx(&mut commands, &asset_server, &catalog, "sndNoSelect", 0.5);
                    continue;
                }

                let already_selected = lock_shared(&bridge)
                    .map(|ui| ui.selected_character == i)
                    .unwrap_or(false);

                if already_selected {
                    if let Some(mut ui) = lock_shared(&bridge) {
                        ui.title_go_visible = false;
                        ui.title_hover_race = -1;
                    }
                    transition.begin_to_state(AppState::Loading);
                    continue;
                }

                selected.0 = race;
                let lo = save.race_loadout(race);

                if let Some(mut ui) = lock_shared(&bridge) {
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

                if race == crate::game::content::RaceId::Random {
                    continue;
                }

                let already = save.race_loadout(race).preferred_skin == s;
                if !already && save.skin_unlocked(race, s) {
                    save.race_loadout_mut(race).preferred_skin = s;
                    if let Err(e) = manager.save(&*save) {
                        bevy::log::error!("save failed: {e}");
                    }
                    if let Some(mut ui) = lock_shared(&bridge) {
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
                if let Some(mut ui) = lock_shared(&bridge) {
                    ui.loadout_open = !ui.loadout_open;
                }
            }
            UiAction::CycleStartWeapon(_) => {
                let race = selected.0;
                let lo = save.race_loadout_mut(race);

                if lo.stored_weapon.0 != 0 {
                    lo.start_weapon = if lo.start_weapon.0 == 0 {
                        lo.stored_weapon
                    } else {
                        crate::game::content::WEAPON_NONE
                    };

                    if let Err(e) = manager.save(&*save) {
                        bevy::log::error!("save failed: {e}");
                    }
                }
            }
            UiAction::CycleStoredWeapon(_) => {

            }
            UiAction::CycleCrown(dir) => {
                let race = selected.0;

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
                if let Err(e) = manager.save(&*save) {
                    bevy::log::error!("save failed: {e}");
                }
            }
            UiAction::SelectCrown(crown_id) => {
                let race = selected.0;

                if race == crate::game::content::RaceId::Random {
                    continue;
                }

                let already = save.race_loadout(race).start_crown
                    == crate::game::content::crown_gml_to_port(crown_id);
                if !already && save.crown_unlocked(race, crown_id) {
                    save.race_loadout_mut(race).start_crown =
                        crate::game::content::crown_gml_to_port(crown_id);
                    if let Err(e) = manager.save(&*save) {
                        bevy::log::error!("save failed: {e}");
                    }
                    play_ui_sfx(&mut commands, &asset_server, &catalog, "sndMenuCrown", 1.0);
                } else if !already {
                    play_ui_sfx(&mut commands, &asset_server, &catalog, "sndNoSelect", 0.5);
                }
            }
            UiAction::SelectMutation(idx) => {

                let already = lock_shared(&bridge)
                    .map(|ui| ui.mutation_selected == Some(idx))
                    .unwrap_or(false);
                if already {
                    mutation_choice.0 = Some(idx);
                    if let Some(mut ui) = lock_shared(&bridge) {
                        ui.mutation_selected = None;
                    }
                } else {
                    if let Some(mut ui) = lock_shared(&bridge) {
                        ui.mutation_selected = Some(idx);
                    }
                    play_ui_sfx(&mut commands, &asset_server, &catalog, "sndHover", 0.45);
                }
            }
            UiAction::PickMutation(idx) => {

                let selected = lock_shared(&bridge)
                    .map(|ui| ui.mutation_selected)
                    .unwrap_or(None);
                if selected == Some(idx) {
                    mutation_choice.0 = Some(idx);
                    if let Some(mut ui) = lock_shared(&bridge) {
                        ui.mutation_selected = None;
                    }
                } else {

                    if let Some(mut ui) = lock_shared(&bridge) {
                        ui.mutation_selected = Some(idx);
                    }
                    play_ui_sfx(&mut commands, &asset_server, &catalog, "sndHover", 0.45);
                }
            }
        }
    }
}

fn force_death_overlay_state(
    state: Res<State<AppState>>,
    run: Option<Res<crate::game::components::Run>>,
    mut paused: ResMut<Paused>,
    mut overlay: ResMut<OverlayMenu>,
    mut pending_unpause: ResMut<PendingUnpause>,
) {
    if *state.get() != AppState::InGame {
        return;
    }
    let Some(run) = run else {
        return;
    };

    if run.game_over {
        paused.0 = false;
        *overlay = OverlayMenu::None;
        pending_unpause.0 = None;
    }
}

fn reset_pause_on_exit(
    mut paused: ResMut<Paused>,
    mut overlay: ResMut<OverlayMenu>,
    mut pending_unpause: ResMut<PendingUnpause>,
    bridge: Res<crate::menus::UiBridge>,
    mut run: Option<ResMut<crate::game::components::Run>>,
) {
    paused.0 = false;
    *overlay = OverlayMenu::None;
    pending_unpause.0 = None;
    if let Some(r) = run.as_mut() {
        r.game_over = false;
    }
    if let Some(mut ui) = lock_shared(&bridge) {
        ui.paused = false;
        ui.overlay = OverlayMenu::None;
        ui.game_over = false;
        ui.pause_confirm = None;
        ui.settings_page = 0;
        ui.settings_page_stack.clear();
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
    bridge: Res<crate::menus::UiBridge>,
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
            if let Some(mut ui) = lock_shared(&bridge) {
                ui.pause_confirm = None;
                ui.settings_page = 0;
                ui.settings_page_stack.clear();
            }
        }
        OverlayMenu::Pause => {
            if let Some(mut ui) = lock_shared(&bridge) {
                if ui.pause_confirm.is_some() {
                    ui.pause_confirm = None;
                    return;
                }
            }
            *overlay = OverlayMenu::None;
            pending_unpause.0 = Some(Timer::from_seconds(0.2, TimerMode::Once));
        }
        OverlayMenu::Settings | OverlayMenu::Credits => {
            let should_pop = {
                if let Some(ui) = lock_shared(&bridge) {
                    !ui.settings_page_stack.is_empty() || ui.settings_page != 0
                } else {
                    false
                }
            };
            if should_pop {
                if let Some(mut ui) = lock_shared(&bridge) {
                    if let Some(prev) = ui.settings_page_stack.pop() {
                        ui.settings_page = prev;
                    } else {
                        ui.settings_page = 0;
                    }
                }
            } else if paused.0 {
                *overlay = OverlayMenu::Pause;
                if let Some(mut ui) = lock_shared(&bridge) {
                    ui.settings_page = 0;
                    ui.settings_page_stack.clear();
                }
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
    let Some(ui) = lock_shared(&bridge) else {
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

        if let Some(mut q) = lock_actions(&bridge) {

            let already = lock_shared(&bridge)
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
    if let Some(mut ui) = lock_shared(&bridge) {
        ui.title_go_visible = false;
        ui.title_hover_race = -1;
    }
    transition.begin_to_state(AppState::Loading);
}

fn sync_virtual_time_with_pause(paused: Res<Paused>, mut ctrl: ResMut<TimeScaleControl>) {
    ctrl.paused = paused.0;
}
