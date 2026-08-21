//! Arena/floor generation: floor tiles + solid walls + props on floors only.

use bevy::prelude::*;
use rand::RngExt;
use rand::SeedableRng;
use rand::rngs::StdRng;
use rand::seq::IndexedRandom;

use crate::game::components::*;
use crate::game::content::*;

#[derive(Clone, Copy)]
pub struct PropDef {
    pub size: Vec2,
    pub color: Color,
    pub hp: i32,
    pub destructible: bool,
    pub explosive: bool,
    pub sprite: Option<&'static str>,
}

pub fn spawn_arena(
    commands: &mut Commands,
    asset_server: &AssetServer,
    run: &Run,
    mask: &mut FloorMask,
) {
    let floor = run.floor;
    let area_floor = floor_in_world(floor);

    let tile = TILE;
    let cols = (ARENA_W / tile) as i32;
    let rows = (ARENA_H / tile) as i32;

    let goal = match area_floor {
        3 => 60,
        7 => 130,
        _ => 110,
    };

    let mut rng = StdRng::seed_from_u64(run.gen_seed ^ 0x9E37_79B9);
    let mut floors: std::collections::HashSet<(i32, i32)> = std::collections::HashSet::new();
    let mut x = cols / 2;
    let mut y = rows / 2;
    floors.insert((x, y));
    let mut dir: i32 = rng.random_range(0..4);

    for _ in 0..goal * 6 {
        if floors.len() >= goal as usize {
            break;
        }
        if rng.random_bool(0.25) {
            dir = match area_floor {
                1 => *(&[0, 0, 1, 1, 2, 2, 3][..])
                    .choose(&mut rng)
                    .unwrap_or(&dir),
                3 => *(&[0, 1, 2, 3][..]).choose(&mut rng).unwrap_or(&dir),
                5 => *(&[0, 1, 2, 3, 0, 1][..]).choose(&mut rng).unwrap_or(&dir),
                7 => *(&[0, 0, 0, 0, 1, 2, 3][..])
                    .choose(&mut rng)
                    .unwrap_or(&dir),
                _ => rng.random_range(0..4),
            };
        }
        let (nx, ny) = match dir {
            0 => (x + 1, y),
            1 => (x - 1, y),
            2 => (x, y + 1),
            _ => (x, y - 1),
        };
        let mut to_add = vec![(nx, ny)];
        match area_floor {
            1 if rng.random_range(0.0..2.0) < 1.0 => {
                to_add.extend([(nx + 1, ny), (nx + 1, ny + 1), (nx, ny + 1)]);
            }
            3 if rng.random_range(0.0..8.0) < 1.0 => {
                for dy in -1..=1 {
                    for dx in -1..=1 {
                        to_add.push((nx + dx, ny + dy));
                    }
                }
            }
            5 if rng.random_range(0.0..11.0) < 1.0 => {
                for dy in -1..=1 {
                    for dx in -1..=1 {
                        to_add.push((nx + dx, ny + dy));
                    }
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
            if ax >= 1 && ax < cols - 1 && ay >= 1 && ay < rows - 1 && floors.insert((ax, ay)) {
                x = ax;
                y = ay;
                moved = true;
            }
        }
        if !moved {
            if nx >= 1 && nx < cols - 1 && ny >= 1 && ny < rows - 1 {
                x = nx;
                y = ny;
                floors.insert((x, y));
            } else {
                dir = rng.random_range(0..4);
            }
        }
    }

    // Publish mask for movement / spawns
    *mask = FloorMask {
        cells: floors.clone(),
        cols,
        rows,
    };

    // Floor visuals — native size when art exists (no forced stretch when possible)
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
            "images/sprFloor1.png"
        } else {
            "images/sprFloor0.png"
        };
        commands.spawn((
            GameCleanup,
            LevelCleanup,
            sprite_or_fallback(asset_server, floor_path, col, Vec2::splat(tile)),
            Transform::from_xyz(wx, wy, -50.0),
        ));
    }

    let mut walls_set: std::collections::HashSet<(i32, i32)> = std::collections::HashSet::new();
    for (fx, fy) in &floors {
        for (dx, dy) in [
            (-1, 0),
            (1, 0),
            (0, -1),
            (0, 1),
            (-1, -1),
            (1, -1),
            (-1, 1),
            (1, 1),
        ] {
            let n = (fx + dx, fy + dy);
            if !floors.contains(&n) {
                walls_set.insert(n);
            }
        }
    }
    for (wx, wy) in walls_set {
        let wxf = (wx as f32 - cols as f32 / 2.0 + 0.5) * tile;
        let wyf = (wy as f32 - rows as f32 / 2.0 + 0.5) * tile;
        let wall_path = if area_floor == 3 {
            "images/sprWall100Out.png"
        } else if area_floor >= 4 {
            "images/sprWall0Out.png"
        } else {
            "images/sprWall0Out.png"
        };
        commands.spawn((
            GameCleanup,
            LevelCleanup,
            WallTile,
            Prop {
                size: Vec2::splat(tile),
                hp: 9999,
                destructible: false,
                explosive: false,
            },
            sprite_or_fallback(
                asset_server,
                wall_path,
                Color::srgb(0.22, 0.16, 0.12),
                Vec2::splat(tile),
            ),
            Transform::from_xyz(wxf, wyf, -40.0),
        ));
    }

    let boss = is_boss_floor(floor);
    spawn_props(commands, asset_server, run, mask, boss);
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

pub(crate) fn floor_color(floor: u32) -> Color {
    match floor_in_world(floor) {
        1 | 2 => Color::srgb(0.80, 0.66, 0.42),
        3 => Color::srgb(0.62, 0.60, 0.52),
        4 | 5 => Color::srgb(0.55, 0.62, 0.68),
        6 => Color::srgb(0.48, 0.52, 0.60),
        _ => Color::srgb(0.88, 0.30, 0.22),
    }
}

fn spawn_props(
    commands: &mut Commands,
    asset_server: &AssetServer,
    run: &Run,
    mask: &FloorMask,
    boss: bool,
) {
    let mut rng =
        StdRng::seed_from_u64(run.gen_seed ^ (run.floor as u64).wrapping_mul(0x9E37_79B9));

    let pillar = PropDef {
        size: Vec2::splat(32.0),
        color: Color::srgb(0.35, 0.30, 0.26),
        hp: 1,
        destructible: false,
        explosive: false,
        sprite: Some("images/sprCactus.png"),
    };
    let crate_def = PropDef {
        size: Vec2::splat(24.0),
        color: Color::srgb(0.62, 0.48, 0.28),
        hp: 2,
        destructible: true,
        explosive: false,
        sprite: Some("images/sprCrate.png"),
    };
    let barrel = PropDef {
        size: Vec2::splat(20.0),
        color: Color::srgb(0.72, 0.28, 0.18),
        hp: 1,
        destructible: true,
        explosive: true,
        sprite: Some("images/sprBarrel.png"),
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

    let mut placed: Vec<Vec2> = Vec::new();
    let mut place = |rng: &mut StdRng, placed: &mut Vec<Vec2>, def: &PropDef| {
        for _ in 0..40 {
            let p = mask.random_floor_pos(rng, 160.0);
            if placed.iter().any(|q| q.distance(p) < 64.0) {
                continue;
            }
            placed.push(p);
            spawn_prop_entity(commands, asset_server, p, def);
            return;
        }
    };

    for _ in 0..pillar_count {
        place(&mut rng, &mut placed, &pillar);
    }
    for _ in 0..crate_count {
        place(&mut rng, &mut placed, &crate_def);
    }
    for _ in 0..barrel_count {
        place(&mut rng, &mut placed, &barrel);
    }
}

fn spawn_prop_entity(
    commands: &mut Commands,
    asset_server: &AssetServer,
    pos: Vec2,
    def: &PropDef,
) {
    let sprite = if let Some(path) = def.sprite {
        sprite_or_fallback(asset_server, path, def.color, def.size)
    } else {
        Sprite {
            color: def.color,
            custom_size: Some(def.size),
            ..default()
        }
    };
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
            sprite,
            Transform::from_translation(pos.extend(-10.0)),
        ))
        .id();
    game_utils_bevy::juice::Juice::pop_in(commands, e, 0.12);
}

/// Clamp into arena AABB (secondary). Primary constraint is FloorMask.
pub fn clamp_to_arena(pos: &mut Vec3, radius: f32) {
    pos.x = pos.x.clamp(-ARENA_W / 2.0 + radius, ARENA_W / 2.0 - radius);
    pos.y = pos.y.clamp(-ARENA_H / 2.0 + radius, ARENA_H / 2.0 - radius);
}

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
