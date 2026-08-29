//! The Nuclear Throne-style game module. Built entirely on the template's
//! ecosystem (game-utils / game-utils-bevy) with placeholder sprites and

pub mod ambience;
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
pub mod environment;
pub mod generated;
pub mod hud;
pub mod idpd;
pub mod input;
pub mod loop_transition;
pub mod pickups;
pub mod player;
pub mod progression;
pub mod projectile_archetypes;
pub mod projectile_art;
pub mod projectile_math;
pub mod reactive_audio;
pub mod secret_areas;
pub mod skin_unlocks;
pub mod ui_art;
pub mod vortex;
pub mod walls;
pub mod weapon_runtime;
pub mod weapons_data;
pub mod world;

use bevy::prelude::*;

use crate::app::{AppState, NtSimSet, Paused};
use crate::game::audio::GameAudio;
use crate::game::components::*;
use crate::game::player as player_sys;
use crate::game::progression as progress_sys;
use game_utils_bevy::screen_effects::ScreenEffectsConfig;
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
            .init_resource::<HammerheadBudget>()
            .init_resource::<LastDamageTaken>()
            .init_resource::<skin_unlocks::CrystalDamageTaken>()
            .init_resource::<ThroneRoomState>()
            .init_resource::<ambience::AreaAudioState>()
            .add_message::<reactive_audio::ReactiveAudioRequest>()
            .add_message::<reactive_audio::UiBridgeAction>()
            .init_resource::<reactive_audio::ReactiveAudioState>()
            .init_resource::<reactive_audio::CombatIntensityState>()
            .add_message::<FloorStarted>()
            .add_systems(Startup, scan_assets_and_audio)
            // Nuclear Throne's shake is positional-only: rotation jitter pivots on the
            // camera center, so it reads as near-zero when the player is centered and
            // increasingly violent toward screen edges. Disable it.
            .insert_resource(ScreenEffectsConfig {
                rotation_jitter_2d: 0.0,
                ..Default::default()
            })
            .add_systems(PreUpdate, input::sample_input.run_if(gameplay_active))
            .add_plugins(ui_art::UiArtPlugin)
            .add_plugins(vortex::VortexPlugin)
            .add_systems(OnEnter(AppState::InGame), setup_game)
            .add_systems(
                OnEnter(AppState::InGame),
                reactive_audio::reset_reactive_audio_state,
            )
            .add_systems(OnExit(AppState::InGame), teardown_game)
            .add_systems(
                OnExit(AppState::InGame),
                reactive_audio::reset_combat_intensity,
            )
            .add_systems(
                FixedUpdate,
                (
                    (
                        (
                            sync_hud,
                            anim::animate_sprites,
                            progress_sys::handle_mutation_choice,
                            player_sys::face_aim,
                            pickups::tick_toast,
                            secret_areas::observe_oasis_floor_start,
                            secret_areas::detect_oasis_eligibility,
                            secret_areas::detect_cursed_caves,
                            secret_areas::detect_hq,
                            secret_areas::secret_debug_toast,
                            crown::tick_crown_life,
                            crown::tick_crown_protection,
                            crown::tick_crown_love,
                            crown::tick_crown_curses,
                            crown::crown_floor_start_bonus,
                            crown::tick_crown_pedestal,
                            loop_transition::tick_campfire,
                            enemies::flush_pending_enemy_spawns,
                        )
                            .in_set(NtSimSet::Always),
                        (
                            skin_unlocks::tick_area_skins,
                            skin_unlocks::tick_global_skins,
                            skin_unlocks::tick_crystal_damage,
                            skin_unlocks::tick_robot_skins,
                        )
                            .in_set(NtSimSet::Always),
                        (
                            anim::tick_hurt_anims,
                            anim::hurt_on_damage,
                            player_sys::ensure_weapon_visual,
                            player_sys::tick_weapon_visuals,
                            progress_sys::tick_portal_suck,
                        )
                            .in_set(NtSimSet::Always),
                    ),
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
                        enemies::tick_frog_eggs,
                        enemies::tick_delayed_boss_spawns,
                        enemies::tick_boss_intro,
                        enemies::tick_corpses,
                        boss_ai::boss_ai,
                        boss_ai::tick_hyper_orbit_crystals,
                        walls::apply_pending_wall_breaks,
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
                        combat::tick_spawn_grace,
                        combat::tick_flame_trails,
                        combat::tick_lightning_arcs,
                        combat::tick_hit_effects,
                        secret_areas::tick_oasis_bandit_window,
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
            .add_systems(
                FixedUpdate,
                progress_sys::apply_floor_reach_unlocks
                    .in_set(NtSimSet::Always)
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                FixedUpdate,
                walls::reset_hammerhead_budget
                    .in_set(NtSimSet::Always)
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                FixedUpdate,
                walls::update_carpet_occupancy
                    .in_set(NtSimSet::Combat)
                    .before(boss_ai::boss_ai)
                    .run_if(gameplay_active),
            )
            .add_systems(
                FixedUpdate,
                walls::handle_throne_room_props
                    .in_set(NtSimSet::Combat)
                    .after(combat::move_projectiles)
                    .after(combat::apply_explosions)
                    .run_if(gameplay_active),
            )
            .add_systems(
                FixedUpdate,
                (
                    environment::apply_surface_effects
                        .after(player_sys::player_move)
                        .after(enemies::enemy_ai),
                    environment::tick_proximity_mines.before(combat::apply_explosions),
                    environment::tick_environment_hazards.after(combat::apply_explosions),
                    environment::animate_environment,
                )
                    .in_set(NtSimSet::Combat)
                    .run_if(gameplay_active),
            )
            .add_systems(
                Update,
                (
                    ambience::sync_area_audio,
                    ambience::tick_area_audio_fades,
                    ambience::sync_area_audio_volumes.after(ambience::tick_area_audio_fades),
                ),
            )
            .add_systems(
                Update,
                (
                    (
                        reactive_audio::observe_player_audio_state,
                        reactive_audio::observe_boss_audio_state,
                        reactive_audio::observe_kill_audio_state,
                        reactive_audio::play_ui_action_audio,
                        reactive_audio::flush_queued_cues,
                        reactive_audio::play_reactive_audio_requests,
                    )
                        .chain(),
                    reactive_audio::update_combat_intensity_audio,
                )
                    .in_set(NtSimSet::Always),
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

fn scan_assets_and_audio(mut commands: Commands, asset_server: Res<AssetServer>) {
    let catalog = content::scan_asset_catalog();
    let audio = GameAudio::load(&asset_server, &catalog);
    commands.insert_resource(catalog);
    commands.insert_resource(audio);
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
