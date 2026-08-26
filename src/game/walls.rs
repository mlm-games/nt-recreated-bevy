//! Destructible walls: the single break pipeline shared by hammerhead, boss
//! charges, explosions, and delayed boss entrances. Breaking a wall despawns
//! its linked visuals, expands the floor mask under it, and stamps the
//! owner-cell floor sprite so holes look walkable.

use bevy::prelude::*;

use crate::game::components::*;
use crate::game::content::{sprite_exact, AssetCatalog};
use crate::game::world::{
    WALL_PX, area_sprites_for_run, expand_floor_for_wall, floor_cell_for_wall,
};
use game_utils_bevy::screen_effects::{ScreenEffects, Trauma};
use game_utils_bevy::vfx::VfxSpawner;

/// Apply all `PendingWallBreak` markers this tick.
pub fn apply_pending_wall_breaks(
    mut commands: Commands,
    catalog: Res<AssetCatalog>,
    asset_server: Res<AssetServer>,
    run: Res<Run>,
    mut mask: ResMut<FloorMask>,
    mut trauma: ResMut<Trauma>,
    pending: Query<(Entity, &PendingWallBreak)>,
    walls: Query<(Entity, &WallCell, &WallVisuals, &Transform), With<WallTile>>,
) {
    for (marker_e, brk) in &pending {
        commands.entity(marker_e).despawn();

        for (wall_e, cell, visuals, tf) in &walls {
            let wpos = tf.translation.truncate();
            if (cell.0, cell.1) != brk.cell && wpos.distance(brk.pos) > WALL_PX * 0.75 {
                continue;
            }

            for part in &visuals.parts {
                commands.entity(*part).despawn();
            }
            commands.entity(wall_e).despawn();

            if brk.spawn_floor {
                expand_floor_for_wall(&mut mask, cell.0, cell.1);
                let owner = floor_cell_for_wall(cell.0, cell.1);
                let pos = Vec2::new(
                    owner.0 as f32 * TILE + TILE * 0.5,
                    owner.1 as f32 * TILE + TILE * 0.5,
                );
                let (floor_png, _, _, _, _, _) = area_sprites_for_run(&run, &catalog);
                commands.spawn((
                    GameCleanup,
                    LevelCleanup,
                    sprite_exact(&catalog, &asset_server, floor_png),
                    Transform::from_translation(pos.extend(-50.0)),
                ));
            }

            VfxSpawner::spawn_burst(
                &mut commands,
                wpos,
                8,
                Color::srgb(0.7, 0.65, 0.55),
                (40.0, 140.0),
            );
            ScreenEffects::add_trauma(&mut trauma, 0.06);
        }
    }
}

/// Queue a break for every wall solid within `radius` of `pos`.
pub fn queue_wall_breaks_in_radius(
    commands: &mut Commands,
    walls: &Query<(Entity, &WallCell, &Transform), With<WallTile>>,
    pos: Vec2,
    radius: f32,
) {
    for (_, cell, tf) in walls {
        let wpos = tf.translation.truncate();
        if wpos.distance(pos) <= radius {
            commands.spawn((
                GameCleanup,
                LevelCleanup,
                PendingWallBreak {
                    cell: (cell.0, cell.1),
                    pos: wpos,
                    spawn_floor: true,
                },
            ));
        }
    }
}

/// Carve from `from` toward `to` at half-tile steps (boss charges).
pub fn queue_wall_breaks_along_segment(
    commands: &mut Commands,
    walls: &Query<(Entity, &WallCell, &Transform), With<WallTile>>,
    from: Vec2,
    to: Vec2,
    half_width: f32,
) {
    let delta = to - from;
    let len = delta.length().max(1.0);
    let dir = delta / len;
    let steps = (len / (WALL_PX * 0.5)).ceil() as i32;
    for i in 0..=steps {
        let p = from + dir * (i as f32 * WALL_PX * 0.5);
        queue_wall_breaks_in_radius(commands, walls, p, half_width);
    }
}

/// True when the segment a→b passes close to any wall solid (LoS probe).
pub fn segment_hits_wall(
    a: Vec2,
    b: Vec2,
    walls: &Query<(Entity, &WallCell, &Transform), With<WallTile>>,
) -> bool {
    let delta = b - a;
    let len = delta.length();
    if len < 1.0 {
        return false;
    }
    let dir = delta / len;
    let steps = (len / 12.0).ceil() as i32;
    for i in 1..steps.max(1) {
        let p = a + dir * (i as f32 * 12.0);
        for (_, _, tf) in walls {
            if tf.translation.truncate().distance(p) < WALL_PX * 0.55 {
                return true;
            }
        }
    }
    false
}

/// Per-floor budget + throne-room gate reset driven by `FloorStarted`.
pub fn reset_hammerhead_budget(
    mut events: MessageReader<FloorStarted>,
    mut budget: ResMut<HammerheadBudget>,
    mut throne: ResMut<ThroneRoomState>,
) {
    if events.read().next().is_some() {
        budget.remaining = HammerheadBudget::default().remaining;
        throne.reset();
    }
}

/// Palace throne-room: generators and statues are props whose death drives
/// the loop gate and spawns guardians. This runs after prop damage has set
/// `Prop.hp <= 0` but before the deferred despawn flush.
pub fn handle_throne_room_props(
    mut commands: Commands,
    mut throne_room: ResMut<ThroneRoomState>,
    mut toast: ResMut<Toast>,
    run: Res<Run>,
    mut bosses: Query<(&Enemy, &mut Health), With<BossBrain>>,
    q: Query<(Entity, &Prop, Option<&BigGenerator>, Option<&ThroneStatueProp>, &Transform)>,
) {
    for (e, prop, big_gen, statue, tf) in &q {
        if prop.hp > 0 {
            continue;
        }

        if big_gen.is_some() {
            // Only count once per generator entity (hp just crossed).
            // Use a marker to avoid double-counting if this system sees the
            // same prop over multiple ticks before despawn.
            // Since we run once per tick and the prop will be despawned
            // next flush, we can safely count now.
            let before = throne_room.generators_destroyed;
            throne_room.note_generator_destroyed();
            if throne_room.generators_destroyed != before {
                toast.show(&format!(
                    "GENERATOR {}/{}",
                    throne_room.generators_destroyed, throne_room.generators_total
                ));
            }
            if throne_room.all_generators_down && !throne_room.halved_throne {
                throne_room.halved_throne = true;
                toast.show("THE THRONE WEAKENS");
                if run.loop_count == 0 {
                    for (enemy, mut hp) in &mut bosses {
                        if enemy.kind == crate::game::content::EnemyKind::Throne {
                            hp.hp = (hp.hp / 2).max(1);
                            hp.max = (hp.max / 2).max(1);
                        }
                    }
                }
            }
            // Let the original prop-damage system handle the despawn;
            // we just drive the gate.
            let _ = e;
        }

        if let Some(statue) = statue {
            let pos = tf.translation.truncate();
            for i in 0..statue.guardian_count {
                let ang = i as f32 * std::f32::consts::TAU / statue.guardian_count as f32;
                let p = pos + Vec2::new(ang.cos(), ang.sin()) * 36.0;
                commands.spawn(crate::game::components::PendingEnemySpawn {
                    kind: crate::game::content::EnemyKind::PalaceGuardian,
                    pos: p,
                    difficulty: 1.0,
                });
            }
        }
    }
}

/// Throne carpet occupancy: sets `ThroneRoomState.player_on_carpet`.
pub fn update_carpet_occupancy(
    mut throne_room: ResMut<ThroneRoomState>,
    player_q: Query<&Transform, With<Player>>,
    carpets: Query<(&Transform, &ThroneCarpet)>,
) {
    throne_room.player_on_carpet = false;
    let Ok(ptf) = player_q.single() else {
        return;
    };
    let p = ptf.translation.truncate();
    for (tf, carpet) in &carpets {
        let c = tf.translation.truncate();
        let d = (p - c).abs();
        if d.x <= carpet.half_extents.x && d.y <= carpet.half_extents.y {
            throne_room.player_on_carpet = true;
            break;
        }
    }
}

