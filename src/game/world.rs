//! Arena/floor generation: floor visuals, border walls, seeded props (pillars,
//! crates, barrels), and world-space collision helpers.

use bevy::prelude::*;
use rand::RngExt;
use rand::SeedableRng;
use rand::rngs::StdRng;

use crate::game::components::*;
use crate::game::content::*;
use rand::seq::IndexedRandom;

#[derive(Clone, Copy)]
pub struct PropDef {
    pub size: Vec2,
    pub color: Color,
    pub hp: i32,
    pub destructible: bool,
    pub explosive: bool,
}

pub fn spawn_arena(commands: &mut Commands, asset_server: &AssetServer, run: &Run) {
    let floor = run.floor;
    let area_floor = floor_in_world(floor);

    // Tile-based level like public-rewrite: generate floor tiles via random walk,
    // then walls around them. Uses original sprFloor/sprWall when extracted,
    // fallback is still colored rects (no hard dep).
    let tile = 32.0;
    let cols = (ARENA_W / tile) as i32;
    let rows = (ARENA_H / tile) as i32;
    // Per-area generation goal like scrAreaGetGenerationGoal
    let goal = match floor_in_world(floor) {
        3 => 60,  // vault/oasis
        7 if floor_in_world(floor) == 7 && world_of(floor) > 1 => 130, // palace second loop
        _ => 110,
    };
    let mut rng = StdRng::seed_from_u64(run.gen_seed ^ 0x9E37_79B9);
    let mut floors: std::collections::HashSet<(i32, i32)> = std::collections::HashSet::new();
    let mut x = cols / 2;
    let mut y = rows / 2;
    floors.insert((x, y));
    let mut dir: i32 = rng.random_range(0..4);
    // Per-area walk with branching like scrMakeFloor
    for _ in 0..goal * 6 {
        if floors.len() >= goal as usize {
            break;
        }
        if rng.random_bool(0.25) {
            dir = match floor_in_world(floor) {
                1 => *(&[0, 0, 1, 1, 2, 2, 3][..]).choose(&mut rng).unwrap_or(&dir),
                3 => *(&[0, 1, 2, 3][..]).choose(&mut rng).unwrap_or(&dir),
                5 => *(&[0, 1, 2, 3, 0, 1][..]).choose(&mut rng).unwrap_or(&dir),
                7 => *(&[0, 0, 0, 0, 1, 2, 3][..]).choose(&mut rng).unwrap_or(&dir),
                _ => rng.random_range(0..4),
            };
        }
        let (mut nx, mut ny) = match dir {
            0 => (x + 1, y),
            1 => (x - 1, y),
            2 => (x, y + 1),
            _ => (x, y - 1),
        };
        // Area-specific cluster expansion like scrMakeFloor
        let mut to_add = vec![(nx, ny)];
        match floor_in_world(floor) {
            1 if rng.random_range(0.0..2.0) < 1.0 => {
                to_add.extend([(nx + 1, ny), (nx + 1, ny + 1), (nx, ny + 1)]);
            }
            3 if rng.random_range(0.0..8.0) < 1.0 => {
                to_add.extend([
                    (nx + 1, ny),
                    (nx + 1, ny + 1),
                    (nx, ny + 1),
                    (nx, ny - 1),
                    (nx - 1, ny),
                    (nx + 1, ny - 1),
                    (nx - 1, ny - 1),
                    (nx - 1, ny + 1),
                ]);
            }
            5 if rng.random_range(0.0..11.0) < 1.0 => {
                if rng.random_bool(0.5) {
                    to_add.extend([
                        (nx + 1, ny),
                        (nx + 1, ny + 1),
                        (nx, ny + 1),
                        (nx, ny - 1),
                        (nx - 1, ny),
                        (nx + 1, ny - 1),
                        (nx - 1, ny - 1),
                        (nx - 1, ny + 1),
                    ]);
                } else {
                    to_add.extend([
                        (nx + 2, ny - 2), (nx + 2, ny - 1), (nx + 2, ny),
                        (nx + 2, ny + 1), (nx + 2, ny + 2),
                        (nx - 2, ny - 2), (nx - 2, ny - 1), (nx - 2, ny),
                        (nx - 2, ny + 1), (nx - 2, ny + 2),
                        (nx, ny - 2), (nx - 1, ny - 2), (nx + 1, ny - 2),
                        (nx, ny + 2), (nx - 1, ny + 2), (nx + 1, ny + 2),
                    ]);
                }
            }
            7 if rng.random_range(0.0..16.0) < 1.0 => {
                for dy in -1..=2 {
                    for dx in -1..=2 {
                        to_add.push((nx + dx, ny + dy));
                    }
                }
            }
            _ => {}
        }
        let mut moved = false;
        for (ax, ay) in to_add {
            if ax >= 1 && ax < cols - 1 && ay >= 1 && ay < rows - 1 {
                if floors.insert((ax, ay)) {
                    x = ax;
                    y = ay;
                    moved = true;
                }
            }
        }
        if !moved {
            // Fallback single step
            if nx >= 1 && nx < cols - 1 && ny >= 1 && ny < rows - 1 {
                x = nx;
                y = ny;
                floors.insert((x, y));
            } else {
                dir = rng.random_range(0..4);
            }
        }
    }
    // Spawn floor tiles — try original sprFloor* first, fallback to colored rect
    for (fx, fy) in &floors {
        let wx = (*fx as f32 - cols as f32 / 2.0 + 0.5) * tile;
        let wy = (*fy as f32 - rows as f32 / 2.0 + 0.5) * tile;
        let is_alt = (fx + fy) % 2 == 0;
        let col = if area_floor == 3 {
            Color::srgb(0.62, 0.60, 0.52)
        } else if area_floor >= 4 {
            Color::srgb(0.55, 0.62, 0.68)
        } else if is_alt {
            Color::srgb(0.85, 0.72, 0.48)
        } else {
            floor_color(floor)
        };
        let floor_path = if area_floor == 3 {
            "images/sprFloor100.png"
        } else if area_floor >= 4 {
            "images/sprFloor102.png"
        } else {
            "images/sprFloor0.png"
        };
        commands.spawn((
            GameCleanup,
            LevelCleanup,
            crate::game::content::sprite_or_fallback(
                asset_server,
                floor_path,
                col,
                Vec2::splat(tile - 1.0),
            ),
            Transform::from_xyz(wx, wy, -50.0),
        ));
    }
    // Walls around floors — exact mcr_floor_make_walls (8 neighbours)
    let mut walls_set: std::collections::HashSet<(i32, i32)> = std::collections::HashSet::new();
    for (fx, fy) in &floors {
        for (dx, dy) in [(-1, 0), (1, 0), (0, -1), (0, 1), (-1, -1), (1, -1), (-1, 1), (1, 1)] {
            let n = (fx + dx, fy + dy);
            if !floors.contains(&n) {
                walls_set.insert(n);
            }
        }
    }
    for (wx, wy) in walls_set {
        let wxf = (wx as f32 - cols as f32 / 2.0 + 0.5) * tile;
        let wyf = (wy as f32 - rows as f32 / 2.0 + 0.5) * tile;
        // Pick wall sprite per area
        let wall_path = if area_floor == 3 {
            "images/sprWall100Out.png"
        } else if area_floor >= 4 {
            "images/sprWall102Out.png"
        } else {
            "images/sprWall0Out.png"
        };
        commands.spawn((
            GameCleanup,
            LevelCleanup,
            crate::game::content::sprite_or_fallback(
                asset_server,
                wall_path,
                Color::srgb(0.22, 0.16, 0.12),
                Vec2::splat(tile),
            ),
            Transform::from_xyz(wxf, wyf, -40.0),
        ));
    }

    let boss = is_boss_floor(floor);
    spawn_props(commands, run, boss);
}

pub fn is_boss_floor(floor: u32) -> bool {
    let f = floor_in_world(floor);
    f == 3 || f == 7
}

pub fn boss_for_floor(floor: u32) -> EnemyKind {
    if floor_in_world(floor) == 7 {
        EnemyKind::Throne
    } else {
        EnemyKind::BigBandit
    }
}

pub fn floor_in_world(floor: u32) -> u32 {
    ((floor - 1) % 7) + 1
}

pub fn world_of(floor: u32) -> u32 {
    ((floor - 1) / 7) + 1
}

pub fn difficulty_multiplier(floor: u32) -> f32 {
    let world = world_of(floor) as f32;
    let f = floor_in_world(floor) as f32;
    1.0 + (world - 1.0) * 0.18 + (f - 1.0) * 0.035
}

fn floor_color(floor: u32) -> Color {
    match floor_in_world(floor) {
        1 | 2 => Color::srgb(0.80, 0.66, 0.42),
        3 => Color::srgb(0.62, 0.60, 0.52),
        4 | 5 => Color::srgb(0.55, 0.62, 0.68),
        6 => Color::srgb(0.48, 0.52, 0.60),
        _ => Color::srgb(0.88, 0.30, 0.22),
    }
}

fn spawn_props(commands: &mut Commands, run: &Run, boss: bool) {
    let mut rng =
        StdRng::seed_from_u64(run.gen_seed ^ (run.floor as u64).wrapping_mul(0x9E37_79B9));

    let pillar = PropDef {
        size: Vec2::splat(52.0),
        color: Color::srgb(0.35, 0.30, 0.26),
        hp: 1,
        destructible: false,
        explosive: false,
    };
    let crate_def = PropDef {
        size: Vec2::splat(40.0),
        color: Color::srgb(0.62, 0.48, 0.28),
        hp: 2,
        destructible: true,
        explosive: false,
    };
    let barrel = PropDef {
        size: Vec2::splat(34.0),
        color: Color::srgb(0.72, 0.28, 0.18),
        hp: 1,
        destructible: true,
        explosive: true,
    };
    let wall_seg = PropDef {
        size: Vec2::new(96.0, 32.0),
        color: Color::srgb(0.22, 0.16, 0.12),
        hp: 99,
        destructible: false,
        explosive: false,
    };

    let pillar_count = if boss {
        4
    } else {
        5 + (run.floor % 4) as usize
    };
    let crate_count = if boss {
        0
    } else {
        8 + (run.floor as usize % 5) * 2
    };
    let barrel_count = if boss {
        0
    } else {
        2 + (run.floor as usize % 3)
    };
    let wall_count = if boss { 0 } else { 5 + (run.floor as usize % 3) };

    let mut placed: Vec<Vec2> = Vec::new();

    for _ in 0..pillar_count {
        let p = random_prop_pos(&mut rng, &placed);
        placed.push(p);
        spawn_prop_entity(commands, p, &pillar);
    }
    for _ in 0..crate_count {
        let p = random_prop_pos(&mut rng, &placed);
        placed.push(p);
        spawn_prop_entity(commands, p, &crate_def);
    }
    for _ in 0..barrel_count {
        let p = random_prop_pos(&mut rng, &placed);
        placed.push(p);
        spawn_prop_entity(commands, p, &barrel);
    }
    // Internal walls — visible level layout (original has maze-like walls)
    for _ in 0..wall_count {
        let p = random_prop_pos(&mut rng, &placed);
        // Random orientation: horizontal or vertical
        let is_horiz = rng.random_bool(0.5);
        let size = if is_horiz {
            wall_seg.size
        } else {
            Vec2::new(wall_seg.size.y, wall_seg.size.x)
        };
        let mut def = wall_seg.clone();
        def.size = size;
        placed.push(p);
        spawn_prop_entity(commands, p, &def);
    }
}

fn random_prop_pos(rng: &mut StdRng, placed: &[Vec2]) -> Vec2 {
    for _ in 0..200 {
        let x = rng.random_range(-ARENA_W / 2.0 + 220.0..ARENA_W / 2.0 - 220.0);
        let y = rng.random_range(-ARENA_H / 2.0 + 220.0..ARENA_H / 2.0 - 220.0);
        let p = Vec2::new(x, y);
        if p.length() < 320.0 {
            continue;
        }
        if placed.iter().any(|q| q.distance(p) < 120.0) {
            continue;
        }
        return p;
    }
    Vec2::new(
        rng.random_range(-500.0..500.0),
        rng.random_range(-400.0..400.0),
    )
}

fn spawn_prop_entity(commands: &mut Commands, pos: Vec2, def: &PropDef) {
    let e = commands
        .spawn((
            GameCleanup,
            LevelCleanup,
            Prop {
                size: def.size,
                hp: def.hp,
                destructible: def.destructible,
                explosive: def.explosive,
            },
            Sprite {
                color: def.color,
                custom_size: Some(def.size),
                ..default()
            },
            Transform::from_translation(pos.extend(-10.0)),
        ))
        .id();
    game_utils_bevy::juice::Juice::pop_in(commands, e, 0.12);
}

/// Clamp a position inside the arena borders.
pub fn clamp_to_arena(pos: &mut Vec3, radius: f32) {
    pos.x = pos.x.clamp(-ARENA_W / 2.0 + radius, ARENA_W / 2.0 - radius);
    pos.y = pos.y.clamp(-ARENA_H / 2.0 + radius, ARENA_H / 2.0 - radius);
}

/// Push a circle (pos, radius) out of every solid prop AABB.
pub fn resolve_prop_collision(
    pos: &mut Vec3,
    radius: f32,
    props: &Query<(Entity, &Prop, &Transform), With<Prop>>,
) {
    for (_, prop, tf) in props.iter() {
        let center = tf.translation.truncate();
        let half = prop.size / 2.0;
        let p = pos.truncate();
        let closest = Vec2::new(
            p.x.clamp(center.x - half.x, center.x + half.x),
            p.y.clamp(center.y - half.y, center.y + half.y),
        );
        let d = p - closest;
        let dist = d.length();
        if dist >= radius || dist <= 0.0001 {
            continue;
        }
        pos.x += d.x / dist * (radius - dist);
        pos.y += d.y / dist * (radius - dist);
    }
}

/// Does a circle (pos, radius) overlap a solid prop?
pub fn circle_hits_prop(
    pos: Vec2,
    radius: f32,
    props: &Query<(Entity, &mut Prop, &Transform), With<Prop>>,
) -> bool {
    for (_, prop, tf) in props.iter() {
        let center = tf.translation.truncate();
        let half = prop.size / 2.0;
        let closest = Vec2::new(
            pos.x.clamp(center.x - half.x, center.x + half.x),
            pos.y.clamp(center.y - half.y, center.y + half.y),
        );
        if pos.distance(closest) < radius {
            return true;
        }
    }
    false
}
