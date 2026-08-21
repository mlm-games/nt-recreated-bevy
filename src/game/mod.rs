//! The Nuclear Throne-style game module. Built entirely on the template's
//! ecosystem (game-utils / game-utils-bevy) with placeholder sprites and

pub mod areas;
pub mod audio;
pub mod combat;
pub mod components;
pub mod content;
pub mod enemies;
pub mod generated;
pub mod hud;
pub mod pickups;
pub mod player;
pub mod progression;
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
            .add_systems(Startup, (load_game_audio, scan_assets))
            .add_systems(OnEnter(AppState::InGame), setup_game)
            .add_systems(OnExit(AppState::InGame), teardown_game)
            .add_systems(
                FixedUpdate,
                (
                    sync_hud.in_set(NtSimSet::Always),
                    progress_sys::handle_mutation_choice.in_set(NtSimSet::Always),
                    player_sys::tick_dash.in_set(NtSimSet::Always),
                    player_sys::face_aim.in_set(NtSimSet::Always),
                    pickups::tick_toast.in_set(NtSimSet::Always),
                    (
                        player_sys::tick_player_timers,
                        player_sys::player_aim,
                        player_sys::player_move,
                        player_sys::blink_player,
                        player_sys::weapon_switch,
                        player_sys::player_ability,
                    )
                        .in_set(NtSimSet::Input)
                        .run_if(gameplay_active),
                    (
                        player_sys::player_fire,
                        player_sys::move_swing_fx,
                        enemies::enemy_ai,
                        combat::move_projectiles,
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
    camera_q: Query<Entity, With<Camera2d>>,
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
        camera_q,
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
