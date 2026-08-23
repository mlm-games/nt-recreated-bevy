//! Throne -> campfire -> Throne II -> loop-portal sequence.
//!
//! Loop floor model: global 1-based floors, route = `(floor - 1) % 15 + 1`,
//! `loop_count = (floor - 1) / 15`. Loop 1 therefore starts at **floor 16**,
//! not floor 1.

use bevy::prelude::*;

use crate::game::areas::{area_for_floor, route_coordinates};
use crate::game::components::*;
use crate::game::content::EnemyKind;
use game_utils_bevy::screen_effects::{ScreenEffects, Trauma};
use game_utils_bevy::vfx::VfxSpawner;

/// Called from `resolve_deaths` when Throne I dies. Starts the interlude and
/// suppresses normal portal spawning until Throne II is dealt with.
pub fn begin_throne_campfire(
    commands: &mut Commands,
    transition: &mut LoopTransition,
    toast: &mut Toast,
    trauma: &mut Trauma,
    player_pos: Vec2,
) {
    if transition.campfire_active || transition.throne_ii_alive {
        return;
    }

    transition.begin_campfire();

    let pos = player_pos + Vec2::new(0.0, -42.0);

    commands.spawn((
        GameCleanup,
        LevelCleanup,
        CampfireProp,
        CampfireState::new(),
        Sprite {
            color: Color::srgb(1.0, 0.58, 0.22),
            custom_size: Some(Vec2::splat(30.0)),
            ..default()
        },
        Transform::from_translation(pos.extend(18.0)),
    ));

    toast.show("REST");
    ScreenEffects::add_trauma(trauma, 0.10);
}

/// Called from `resolve_deaths` when Throne II dies.
pub fn mark_throne_ii_defeated(toast: &mut Toast, trauma: &mut Trauma) {
    // Loop-ready state is set by the caller via LoopTransition; this helper
    // only handles presentation so the caller owns the resource mutation.
    toast.show("THE LOOP OPENS");
    ScreenEffects::add_trauma(trauma, 0.35);
}

/// Campfire interlude: rest -> something stirs -> Throne II rises.
pub fn tick_campfire(
    time: Res<Time<Fixed>>,
    mut commands: Commands,
    mut transition: ResMut<LoopTransition>,
    mut trauma: ResMut<Trauma>,
    mut toast: ResMut<Toast>,
    mut campfires: Query<(Entity, &Transform, &mut CampfireState), With<CampfireProp>>,
) {
    for (entity, tf, mut campfire) in campfires.iter_mut() {
        campfire.timer.tick(time.delta());

        match campfire.phase {
            CampfirePhase::Sitting => {
                if campfire.timer.elapsed_secs() % 0.35 < time.delta_secs() {
                    VfxSpawner::spawn_burst(
                        &mut commands,
                        tf.translation.truncate(),
                        2,
                        Color::srgb(1.0, 0.7, 0.35),
                        (20.0, 58.0),
                    );
                }

                if campfire.timer.just_finished() {
                    campfire.set_phase(CampfirePhase::Rising, 1.15);
                    toast.show("SOMETHING STIRS...");
                    ScreenEffects::add_trauma(&mut trauma, 0.18);
                }
            }

            CampfirePhase::Rising => {
                ScreenEffects::add_trauma(&mut trauma, 0.02);

                if campfire.timer.just_finished() {
                    campfire.set_phase(CampfirePhase::SpawnThroneII, 0.35);
                }
            }

            CampfirePhase::SpawnThroneII => {
                if !campfire.timer.just_finished() || campfire.spawned_throne_ii {
                    continue;
                }

                campfire.spawned_throne_ii = true;
                transition.throne_ii_spawned();

                let spawn = tf.translation.truncate() + Vec2::new(0.0, 84.0);

                commands.spawn((
                    GameCleanup,
                    LevelCleanup,
                    PendingEnemySpawn {
                        kind: EnemyKind::ThroneII,
                        pos: spawn,
                        difficulty: 1.0 + transition.last_completed_loop as f32 * 0.45,
                    },
                ));

                VfxSpawner::spawn_burst(
                    &mut commands,
                    spawn,
                    42,
                    Color::srgb(0.82, 0.45, 1.0),
                    (150.0, 480.0),
                );
                ScreenEffects::add_trauma(&mut trauma, 0.45);
                toast.show("THE THRONE RISES");

                commands.entity(entity).despawn();
            }

            CampfirePhase::Done => {
                commands.entity(entity).despawn();
            }
        }
    }
}

/// Applies the loop-portal transition when `loop_ready` is pending.
///
/// Loop 1 starts at global floor 16 (= 1*15 + 1), loop 2 at 31, and so on —
/// this keeps `(floor - 1) / 15`, `route_coordinates`, `area_for_floor`,
/// boss-floor mapping, and IDPD loop gating coherent.
#[allow(clippy::needless_pass_by_ref_mut)]
pub fn try_apply_loop_portal_transition(
    run: &mut Run,
    transition: &mut LoopTransition,
    trauma: &mut Trauma,
) -> bool {
    if !transition.consume_loop_ready() {
        return false;
    }

    let next_loop = run.loop_count + 1;
    run.loop_count = next_loop;
    run.floor = next_loop * 15 + 1;

    let (world, floor_in_area) = route_coordinates(run.floor);
    run.world = world;
    run.floor_in_area = floor_in_area;
    run.area = area_for_floor(run.floor, run.loop_count);
    run.portal_open = false;
    run.gen_seed = rand::random::<u64>();

    transition.last_completed_loop = next_loop;

    ScreenEffects::add_trauma(trauma, 0.35);

    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::areas::AreaId;

    #[test]
    fn loop_one_starts_at_global_floor_16() {
        let mut run = Run::default();
        run.floor = 15;
        run.loop_count = 0;

        let mut transition = LoopTransition::default();
        transition.throne_ii_spawned();
        transition.throne_ii_defeated();

        let mut trauma = Trauma::default();
        assert!(try_apply_loop_portal_transition(
            &mut run,
            &mut transition,
            &mut trauma
        ));

        assert_eq!(run.loop_count, 1);
        assert_eq!(run.floor, 16);
        assert_eq!(run.world, 1);
        assert_eq!(run.floor_in_area, 1);
        assert_eq!(run.area, AreaId::Desert);
    }

    #[test]
    fn loop_two_starts_at_global_floor_31() {
        let mut run = Run::default();
        run.loop_count = 1;
        run.floor = 30;

        let mut transition = LoopTransition::default();
        transition.throne_ii_defeated();

        let mut trauma = Trauma::default();
        assert!(try_apply_loop_portal_transition(
            &mut run,
            &mut transition,
            &mut trauma
        ));

        assert_eq!(run.loop_count, 2);
        assert_eq!(run.floor, 31);
        assert_eq!(run.world, 1);
        assert_eq!(run.area, AreaId::Desert);
    }

    #[test]
    fn no_loop_ready_is_noop() {
        let mut run = Run::default();
        run.floor = 7;
        run.loop_count = 0;
        let mut transition = LoopTransition::default();

        let mut trauma = Trauma::default();
        assert!(!try_apply_loop_portal_transition(
            &mut run,
            &mut transition,
            &mut trauma
        ));
        assert_eq!(run.floor, 7);
        assert_eq!(run.loop_count, 0);
    }

    #[test]
    fn transition_blocks_portal_until_throne_ii_is_dead() {
        let mut t = LoopTransition::default();
        assert!(!t.blocks_portal());

        t.begin_campfire();
        assert!(t.blocks_portal());
        assert!(!t.loop_ready);

        t.throne_ii_spawned();
        assert!(t.blocks_portal());

        t.throne_ii_defeated();
        assert!(!t.blocks_portal());
        assert!(t.loop_ready);
        assert!(t.consume_loop_ready());
        assert!(!t.consume_loop_ready());
    }

    #[test]
    fn campfire_state_starts_sitting() {
        let c = CampfireState::new();
        assert_eq!(c.phase, CampfirePhase::Sitting);
        assert!(!c.spawned_throne_ii);
        assert!(c.timer.duration().as_secs_f32() >= 3.0);
    }
}
