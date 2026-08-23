//! The Nuclear Throne-style game module. Built entirely on the template's
//! ecosystem (game-utils / game-utils-bevy) with placeholder sprites and

pub mod anim;
pub mod areas;
pub mod audio;
pub mod boss_ai;
pub mod boss_patterns;
pub mod combat;
pub mod components;
pub mod content;
pub mod crown;
pub mod enemies;
pub mod generated;
pub mod hud;
pub mod idpd;
pub mod input;
pub mod loop_transition;
pub mod pickups;
pub mod player;
pub mod progression;
pub mod projectile_archetypes;
pub mod projectile_math;
pub mod secret_areas;
pub mod ui_art;
pub mod weapon_runtime;
pub mod weapons_data;
pub mod world;

use bevy::prelude::*;

use crate::app::{AppState, NtSimSet, Paused};
use crate::game::audio::GameAudio;
use crate::game::components::*;
use crate::game::player as player_sys;
use crate::game::progression as progress_sys;
use game_utils_bevy::transitions::Transition;

pub use crate::game::components::{MutationChoice, Score, SelectedCharacter};
use crate::game::hud::sync_hud;

pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Score>()
            .init_resource::<input::NtInput>()
            .init_resource::<SaveDirty>()
            .init_resource::<Run>()
            .init_resource::<FloorMask>()
            .init_resource::<SelectedCharacter>()
            .init_resource::<MutationChoice>()
            .init_resource::<Toast>()
            .init_resource::<ScarierFace>()
            .init_resource::<Euphoria>()
            .init_resource::<OpenMind>()
            .init_resource::<HeavyHeart>()
            .init_resource::<secret_areas::SecretTriggers>()
            .init_resource::<idpd::IdpdRaidState>()
            .init_resource::<LoopTransition>()
            .add_message::<FloorStarted>()
            .add_systems(Startup, load_game_audio)
            .add_systems(Startup, scan_assets)
            .add_systems(PreUpdate, input::sample_input.run_if(gameplay_active))
            .add_plugins(ui_art::UiArtPlugin)
            .add_systems(OnEnter(AppState::InGame), setup_game)
            .add_systems(OnExit(AppState::InGame), teardown_game)
            .add_systems(
                FixedUpdate,
                (
                    (
                        sync_hud,
                        anim::animate_sprites,
                        progress_sys::handle_mutation_choice,
                        player_sys::face_aim,
                        pickups::tick_toast,
                        secret_areas::detect_oasis_eligibility,
                        secret_areas::detect_cursed_caves,
                        secret_areas::detect_hq,
                        secret_areas::secret_debug_toast,
                        crown::tick_crown_life,
                        crown::tick_crown_protection,
                        crown::tick_crown_love,
                        crown::tick_crown_curses,
                        crown::crown_floor_start_bonus,
                        loop_transition::tick_campfire,
                        enemies::flush_pending_enemy_spawns,
                    )
                        .in_set(NtSimSet::Always),
                    anim::player_anim_switch.in_set(NtSimSet::Input),
                    (
                        player_sys::tick_player_timers,
                        player_sys::player_aim,
                        player_sys::player_move,
                        player_sys::hammerhead_chew,
                        player_sys::blink_player,
                        player_sys::weapon_switch,
                        player_sys::player_ability,
                    )
                        .in_set(NtSimSet::Input)
                        .run_if(gameplay_active),
                    (
                        player_sys::player_fire,
                        player_sys::move_swing_fx,
                        player_sys::tick_snare_zones,
                        player_sys::tick_slowed,
                        player_sys::tick_portal_strikes,
                        player_sys::tick_hazard_clouds,
                        player_sys::ally_ai,
                        enemies::enemy_ai,
                        boss_ai::boss_ai,
                        boss_ai::tick_hyper_orbit_crystals,
                    )
                        .in_set(NtSimSet::Combat)
                        .run_if(gameplay_active),
                    (
                        idpd::tick_idpd_raids,
                        idpd::tick_idpd_vans,
                        idpd::hq_pressure,
                        combat::tick_homing_projectiles,
                        combat::tick_sticky_projectiles,
                        combat::tick_beams,
                        combat::tick_sentry_turrets,
                    )
                        .in_set(NtSimSet::Combat)
                        .run_if(gameplay_active),
                    (
                        combat::move_projectiles,
                        combat::tick_hazard_clouds,
                        combat::apply_explosions,
                        combat::projectile_hits,
                        combat::contact_damage,
                        combat::gamma_guts_aura,
                    )
                        .in_set(NtSimSet::Combat)
                        .run_if(gameplay_active),
                    (
                        combat::resolve_deaths,
                        pickups::collect_pickups,
                        progress_sys::portal_check,
                        progress_sys::portal_enter,
                        progress_sys::animate_portal,
                    )
                        .in_set(NtSimSet::Progression)
                        .run_if(gameplay_active),
                    progress_sys::flush_dirty_save
                        .in_set(NtSimSet::Cleanup)
                        .run_if(in_state(AppState::InGame)),
                ),
            )
            .add_systems(Update, clear_input_when_inactive)
            .add_systems(OnExit(AppState::InGame), clear_input_pulses)
            .add_systems(
                OnExit(AppState::InGame),
                (progress_sys::flush_dirty_save_once, teardown_game_flags).chain(),
            );
    }
}

fn gameplay_active(
    state: Res<State<AppState>>,
    paused: Res<Paused>,
    transition: Res<Transition<AppState>>,
    run: Res<Run>,
) -> bool {
    *state.get() == AppState::InGame && !paused.0 && !transition.block_input && !run.game_over
}

fn load_game_audio(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands.insert_resource(GameAudio::load(&asset_server));
}

fn scan_assets(mut commands: Commands) {
    let catalog = content::scan_asset_catalog();
    commands.insert_resource(catalog);
}

fn setup_game(
    commands: Commands,
    catalog: Res<content::AssetCatalog>,
    asset_server: Res<AssetServer>,
    score: ResMut<Score>,
    run: ResMut<Run>,
    mask: ResMut<FloorMask>,
    dirty: ResMut<SaveDirty>,
    paused: ResMut<Paused>,
    overlay: ResMut<crate::app::OverlayMenu>,
    pending_unpause: ResMut<crate::app::PendingUnpause>,
    toast: ResMut<Toast>,
    character: Res<SelectedCharacter>,
    save: Res<crate::save::SaveData>,
    camera_q: Query<Entity, With<Camera2d>>,
    floor_started: MessageWriter<FloorStarted>,
) {
    progress_sys::setup_run(
        commands,
        catalog,
        asset_server,
        score,
        run,
        mask,
        dirty,
        paused,
        overlay,
        pending_unpause,
        toast,
        character,
        save,
        camera_q,
        floor_started,
    );
}

fn teardown_game(
    commands: Commands,
    q: Query<Entity, With<GameCleanup>>,
    numbers: Query<Entity, With<game_utils_bevy::vfx::DamageNumber>>,
    particles: Query<Entity, With<game_utils_bevy::juice::Particle>>,
    trails: Query<Entity, With<game_utils_bevy::vfx::TrailGhost>>,
    camera_q: Query<Entity, With<Camera2d>>,
    mask: Option<ResMut<FloorMask>>,
) {
    progress_sys::cleanup_run(commands, q, numbers, particles, trails, camera_q, mask);
}

fn teardown_game_flags(bridge: Res<crate::menus::UiBridge>) {
    hud::reset_hud_flags(bridge);
}

fn clear_input_pulses(mut input: ResMut<input::NtInput>) {
    input.clear_transient();
}

fn clear_input_when_inactive(
    paused: Res<Paused>,
    state: Res<State<AppState>>,
    mut input: ResMut<input::NtInput>,
) {
    if paused.0 || *state.get() != AppState::InGame {
        input.clear_transient();
    }
}
