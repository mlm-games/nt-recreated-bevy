//! IDPD raids: loop-only escalation, edge spawns, van deployments, and the
//! HQ garrison pressure loop.

use bevy::prelude::*;

use crate::game::areas::AreaId;
use crate::game::audio::GameAudio;
use crate::game::components::*;
use crate::game::content::{AssetCatalog, EnemyKind, enemy_def};
use crate::game::enemies::spawn_enemy_at;
use crate::game::reactive_audio::{QueuedReactiveCue, ReactiveCue};

pub use crate::game::components::IdpdRaidState;
use game_utils_bevy::screen_effects::{ScreenEffects, Trauma};
use game_utils_bevy::vfx::VfxSpawner;

fn is_raid_suppressed_area(area: AreaId) -> bool {
    matches!(
        area,
        AreaId::Vault
            | AreaId::CrownVault
            | AreaId::Oasis
            | AreaId::PizzaSewers
            | AreaId::Campfire
            | AreaId::HQ
    )
}

/// True for every enemy managed by the IDPD raid director.
///
/// Keep this centralized so the campfire gate, raid director, death effects,
/// and future HQ logic cannot disagree about which units must be cleared.
pub fn is_idpd_kind(kind: EnemyKind) -> bool {
    matches!(
        kind,
        EnemyKind::IdpdGrunt | EnemyKind::IdpdShield | EnemyKind::IdpdElite | EnemyKind::IdpdVan
    )
}

/// Whether the raid director may enqueue a brand-new warning/wave.
///
/// Existing pending waves are handled separately; this only answers whether
/// the trigger condition may create another one.
pub fn may_queue_new_raid(transition: &LoopTransition) -> bool {
    !transition.blocks_new_idpd_raids()
}

pub fn should_trigger_idpd(
    run: &Run,
    enemies_alive: usize,
    kills_since_checkpoint: u32,
    pending: bool,
) -> bool {
    if pending || run.loop_count == 0 || run.game_over {
        return false;
    }

    if is_raid_suppressed_area(run.area) {
        return false;
    }

    if enemies_alive > 4 {
        return false;
    }

    kills_since_checkpoint >= 10
}

pub fn choose_wave(loop_count: u32, floor: u32, roll: u8) -> RaidWave {
    let pressure = loop_count * 10 + floor.min(30);
    if pressure >= 28 {
        if roll % 4 == 0 {
            RaidWave::VanDrop
        } else if roll % 2 == 0 {
            RaidWave::Heavy
        } else {
            RaidWave::Medium
        }
    } else if pressure >= 18 {
        if roll % 5 == 0 {
            RaidWave::VanDrop
        } else {
            RaidWave::Medium
        }
    } else {
        RaidWave::Light
    }
}

/// Four arena-edge points ordered farthest-first from the player so raids
/// enter from off-pressure edges.
pub fn edge_spawn_points_away_from(player_pos: Vec2) -> [Vec2; 4] {
    let margin = 56.0;

    let left = Vec2::new(
        -ARENA_W * 0.5 + margin,
        player_pos.y.clamp(-ARENA_H * 0.4, ARENA_H * 0.4),
    );
    let right = Vec2::new(
        ARENA_W * 0.5 - margin,
        player_pos.y.clamp(-ARENA_H * 0.4, ARENA_H * 0.4),
    );
    let top = Vec2::new(
        player_pos.x.clamp(-ARENA_W * 0.4, ARENA_W * 0.4),
        ARENA_H * 0.5 - margin,
    );
    let bottom = Vec2::new(
        player_pos.x.clamp(-ARENA_W * 0.4, ARENA_W * 0.4),
        -ARENA_H * 0.5 + margin,
    );

    let mut pts = [left, right, top, bottom];
    pts.sort_by(|a, b| {
        b.distance_squared(player_pos)
            .partial_cmp(&a.distance_squared(player_pos))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    pts
}

#[allow(clippy::too_many_arguments)]
pub fn tick_idpd_raids(
    time: Res<Time<Fixed>>,
    mut commands: Commands,
    catalog: Res<AssetCatalog>,
    asset_server: Res<AssetServer>,
    audio: Res<GameAudio>,
    mut trauma: ResMut<Trauma>,
    mut raid: ResMut<IdpdRaidState>,
    run: Res<Run>,
    transition: Res<LoopTransition>,
    player_q: Query<&Transform, With<Player>>,
    enemies_q: Query<(), With<Enemy>>,
    mut toast: ResMut<Toast>,
) {
    raid.cooldown.tick(time.delta());

    let Ok(player_tf) = player_q.single() else {
        return;
    };
    let player_pos = player_tf.translation.truncate();
    let enemies_alive = enemies_q.iter().count();
    let kills_since_checkpoint = run.total_kills.saturating_sub(raid.kills_checkpoint);

    // Once Throne II has spawned — or after it dies while the loop portal is
    // waiting — there must be no late warning left in the director. A pending
    // warning during the campfire itself is intentionally retained: it was
    // queued before the Throne died and is the alternate IDPD-clear path.
    if transition.throne_ii_alive || transition.loop_ready {
        raid.pending_wave = None;
        return;
    }

    let may_queue = may_queue_new_raid(&transition);

    if may_queue
        && should_trigger_idpd(
            &run,
            enemies_alive,
            kills_since_checkpoint,
            raid.pending_wave.is_some(),
        )
        && raid.cooldown.just_finished()
    {
        let roll = ((run.gen_seed ^ run.total_kills as u64 ^ run.floor as u64) & 0xFF) as u8;
        let wave = choose_wave(run.loop_count, run.floor, roll);
        raid.pending_wave = Some(wave);
        raid.warning = Timer::from_seconds(1.25, TimerMode::Once);
        toast.show("IDPD INCOMING");
        commands.spawn((GameCleanup, QueuedReactiveCue(ReactiveCue::IdpdIncoming)));
        ScreenEffects::add_trauma(&mut trauma, 0.12);
        return;
    }

    // No new warning may begin during the campfire, but an already-pending
    // warning is allowed to finish below (alternate IDPD-clear path).
    if transition.campfire_active && raid.pending_wave.is_none() {
        return;
    }

    let Some(wave) = raid.pending_wave else {
        return;
    };

    raid.warning.tick(time.delta());
    if !raid.warning.just_finished() {
        return;
    }

    spawn_raid_wave(
        &mut commands,
        &catalog,
        &asset_server,
        player_pos,
        run.loop_count,
        wave,
    );

    raid.pending_wave = None;
    raid.wave_index += 1;
    raid.kills_checkpoint = run.total_kills;
    raid.cooldown = Timer::from_seconds(
        (18.0 - (run.loop_count as f32 * 1.5)).max(8.0),
        TimerMode::Once,
    );

    audio.play_portal(&mut commands);
    VfxSpawner::spawn_burst(
        &mut commands,
        player_pos,
        16,
        Color::srgb(0.45, 0.7, 1.0),
        (120.0, 260.0),
    );
}

pub fn spawn_raid_wave(
    commands: &mut Commands,
    catalog: &AssetCatalog,
    asset_server: &AssetServer,
    player_pos: Vec2,
    loop_count: u32,
    wave: RaidWave,
) {
    let points = edge_spawn_points_away_from(player_pos);
    let difficulty = 1.0 + loop_count as f32 * 0.18;

    match wave {
        RaidWave::Light => {
            spawn_grunt(commands, catalog, asset_server, points[0], difficulty);
            spawn_grunt(commands, catalog, asset_server, points[1], difficulty);
            spawn_shield(commands, catalog, asset_server, points[2], difficulty);
        }

        RaidWave::Medium => {
            spawn_grunt(commands, catalog, asset_server, points[0], difficulty);
            spawn_grunt(commands, catalog, asset_server, points[1], difficulty);
            spawn_shield(commands, catalog, asset_server, points[2], difficulty);
            spawn_elite(commands, catalog, asset_server, points[3], difficulty);
        }

        RaidWave::Heavy => {
            for &p in &points {
                spawn_grunt(commands, catalog, asset_server, p, difficulty);
            }
            spawn_elite(
                commands,
                catalog,
                asset_server,
                (points[0] + points[1]) * 0.5,
                difficulty + 0.15,
            );
            spawn_shield(
                commands,
                catalog,
                asset_server,
                (points[2] + points[3]) * 0.5,
                difficulty + 0.15,
            );
        }

        RaidWave::VanDrop => {
            spawn_van(
                commands,
                catalog,
                asset_server,
                points[0],
                difficulty + 0.25,
            );
            spawn_shield(commands, catalog, asset_server, points[1], difficulty);
            spawn_elite(commands, catalog, asset_server, points[2], difficulty);
        }
    }
}

fn spawn_at(
    commands: &mut Commands,
    catalog: &AssetCatalog,
    asset_server: &AssetServer,
    kind: EnemyKind,
    pos: Vec2,
    difficulty: f32,
) {
    spawn_enemy_at(
        commands,
        catalog,
        asset_server,
        kind,
        pos,
        difficulty,
        false,
        false,
    );
}

fn spawn_grunt(
    commands: &mut Commands,
    catalog: &AssetCatalog,
    asset_server: &AssetServer,
    pos: Vec2,
    difficulty: f32,
) {
    spawn_at(
        commands,
        catalog,
        asset_server,
        EnemyKind::IdpdGrunt,
        pos,
        difficulty,
    );
}

fn spawn_shield(
    commands: &mut Commands,
    catalog: &AssetCatalog,
    asset_server: &AssetServer,
    pos: Vec2,
    difficulty: f32,
) {
    spawn_at(
        commands,
        catalog,
        asset_server,
        EnemyKind::IdpdShield,
        pos,
        difficulty,
    );
}

fn spawn_elite(
    commands: &mut Commands,
    catalog: &AssetCatalog,
    asset_server: &AssetServer,
    pos: Vec2,
    difficulty: f32,
) {
    spawn_at(
        commands,
        catalog,
        asset_server,
        EnemyKind::IdpdElite,
        pos,
        difficulty,
    );
}

fn spawn_van(
    commands: &mut Commands,
    catalog: &AssetCatalog,
    asset_server: &AssetServer,
    pos: Vec2,
    difficulty: f32,
) {
    spawn_at(
        commands,
        catalog,
        asset_server,
        EnemyKind::IdpdVan,
        pos,
        difficulty,
    );
}

/// Vans hold position and periodically deploy reinforcements.
pub fn tick_idpd_vans(
    time: Res<Time<Fixed>>,
    mut commands: Commands,
    catalog: Res<AssetCatalog>,
    asset_server: Res<AssetServer>,
    mut vans: Query<(Entity, &Transform, &mut IdpdVanBrain), With<Enemy>>,
) {
    for (entity, tf, mut van) in vans.iter_mut() {
        if van.charges_left == 0 {
            continue;
        }

        van.deploy_timer.tick(time.delta());
        if !van.deploy_timer.just_finished() {
            continue;
        }

        van.charges_left -= 1;
        let pos = tf.translation.truncate();

        spawn_grunt(
            &mut commands,
            &catalog,
            &asset_server,
            pos + Vec2::new(-18.0, -22.0),
            1.15,
        );
        spawn_grunt(
            &mut commands,
            &catalog,
            &asset_server,
            pos + Vec2::new(18.0, -22.0),
            1.15,
        );

        if van.charges_left % 2 == 0 {
            spawn_shield(
                &mut commands,
                &catalog,
                &asset_server,
                pos + Vec2::new(0.0, 26.0),
                1.2,
            );
        }

        // Once empty, the van is just a stationary target.
        commands.entity(entity).remove::<IdpdShieldUnit>();
        let _ = enemy_def(EnemyKind::IdpdVan);
    }
}

/// HQ garrison: while visiting the I.D.P.D. HQ secret, keep pressure on.
pub fn hq_pressure(
    time: Res<Time<Fixed>>,
    mut commands: Commands,
    catalog: Res<AssetCatalog>,
    asset_server: Res<AssetServer>,
    run: Res<Run>,
    player_q: Query<&Transform, With<Player>>,
    enemies_q: Query<(), With<Enemy>>,
    mut raid: ResMut<IdpdRaidState>,
) {
    if run.area != AreaId::HQ {
        return;
    }

    let Ok(player_tf) = player_q.single() else {
        return;
    };

    raid.cooldown.tick(time.delta());

    let enemies_alive = enemies_q.iter().count();
    if enemies_alive >= 8 {
        return;
    }

    if !raid.cooldown.just_finished() {
        return;
    }

    let player_pos = player_tf.translation.truncate();
    let points = edge_spawn_points_away_from(player_pos);

    spawn_grunt(&mut commands, &catalog, &asset_server, points[0], 1.3);
    spawn_shield(&mut commands, &catalog, &asset_server, points[1], 1.35);
    spawn_elite(&mut commands, &catalog, &asset_server, points[2], 1.4);

    if raid.wave_index % 2 == 0 {
        spawn_van(&mut commands, &catalog, &asset_server, points[3], 1.45);
    }

    raid.wave_index += 1;
    raid.cooldown = Timer::from_seconds(9.5, TimerMode::Once);
}
