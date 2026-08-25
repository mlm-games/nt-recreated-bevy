//! World generation — faithful port of the upstream FloorMaker/scrMakeFloor
//! structure: 32px floor tiles stamped by walkers, 16px wall rings (Bot body
//! under a Top face drawn 8px up), small interior walls, bone/detail decals,
//! per-area props, chests on turns/dead-ends, and the scrPopulate/scrPopEnemies
//! spawn tables. Missing art crashes loudly via AssetCatalog.

use bevy::prelude::*;
use rand::RngExt;
use rand::SeedableRng;
use rand::rngs::StdRng;

use crate::game::areas::AreaId;
use crate::game::components::*;
use crate::game::content::*;
use crate::game::environment::{
    EnvironmentHazard, EnvironmentHazardSpec, PropDeathEffect, ProximityMine, SurfaceKind,
    SurfacePulse, SurfaceZone, sprite_from_candidates,
};
use crate::game::secret_areas::SecretTarget;
use bevy::ecs::query::QueryFilter;

pub const WALL_PX: f32 = 16.0;

/// One generated level, ready to render/spawn.
pub struct LevelPlan {
    pub floor_cells: Vec<(i32, i32)>,
    pub wall_cells: std::collections::HashSet<(i32, i32)>,
    /// Upstream "small walls": extra 16px Wall instances stamped inside floor
    /// tiles (scrPopProps head). Solid, render like normal walls.
    pub small_walls: Vec<(i16, i16)>,
    pub bones: Vec<(Vec2, bool)>,
    pub details: Vec<Vec2>,
    pub props: Vec<(PropKind, Vec2)>,
    pub chests: Vec<ChestSpawn>,
    pub enemies: Vec<(EnemyKind, Vec2)>,
    pub boss: Option<EnemyKind>,
    /// How many of `boss` to spawn (multi-Bandit). Other bosses ignore >1.
    pub boss_count: u32,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PropKind {
    // Existing
    Cactus,
    BigSkull,
    GroundDecal,
    Barrel,
    Pipe,
    Tires,

    // Solid environmental props
    ToxicBarrel,
    Car,
    Cocoon,
    Snowman,
    Torch,

    // Functional floor / hazard entities
    Cobweb,
    IcePatch,
    FireTrap,
    Mine,

    // Palace throne-room set pieces
    BigGenerator,
    ThroneStatue,
}

#[derive(Clone, Copy, PartialEq)]
pub enum ChestSpawn {
    Weapon(Vec2),
    Ammo(Vec2),
    Rad(Vec2),
}

/// Upstream `_area` ids for our 15-floor NT route:
/// 1 Desert, 2 Sewers, 3 Scrapyards, 4 Crystal Caves, 5 Frozen City, 6 Labs, 7 Palace
fn gml_area(floor: u32) -> i32 {
    let rf = ((floor.max(1) - 1) % 15) + 1;
    match rf {
        1..=3 => 1,   // Desert
        4 => 2,       // Sewers
        5..=7 => 3,   // Scrapyards
        8 => 4,       // Crystal Caves
        9..=11 => 5,  // Frozen City
        12 => 6,      // Labs
        13..=15 => 7, // Palace
        _ => 7,
    }
}

/// Secret areas keep their own visual/spawn families by mapping to the GML
/// area id of the region they borrow tiles from.
fn gml_area_from_run(run: &Run) -> i32 {
    use crate::game::areas::AreaId;
    match run.area {
        AreaId::Desert | AreaId::Oasis => 1,
        AreaId::Sewers | AreaId::PizzaSewers => 2,
        AreaId::Scrapyards | AreaId::City => 3,
        AreaId::CrystalCaves | AreaId::CursedCaves | AreaId::Vault | AreaId::CrownVault => 4,
        AreaId::FrozenCity | AreaId::Jungle => 5,
        AreaId::Labs | AreaId::HQ => 6,
        AreaId::Palace | AreaId::Campfire => 7,
        _ => gml_area(run.floor),
    }
}

fn is_boss_subarea(floor: u32) -> bool {
    let rf = ((floor.max(1) - 1) % 15) + 1;
    // End of each multi-floor world: Desert 3, Scrapyards 7, Frozen 11, Palace 15
    matches!(rf, 3 | 7 | 11 | 15)
}

/// Boss-subarea check that never treats a secret area as the route floor's
/// boss stage (secrets get their own boss assignment below).
fn is_boss_subarea_run(run: &Run) -> bool {
    if crate::game::secret_areas::is_secret_area(run.area) {
        return false;
    }
    is_boss_subarea(run.floor)
}

pub fn generation_goal(floor: u32) -> usize {
    if is_boss_subarea(floor) {
        let rf = ((floor.max(1) - 1) % 15) + 1;
        return if rf == 15 { 48 } else { 60 };
    }
    110
}

/// Secret areas get tighter or roomier layouts per their upstream feel.
fn generation_goal_for_run(run: &Run) -> usize {
    use crate::game::areas::AreaId;
    if crate::game::secret_areas::is_secret_area(run.area) {
        return match run.area {
            AreaId::CrownVault | AreaId::Vault => 40,
            AreaId::HQ => 70,
            AreaId::CursedCaves => 100,
            _ => 90,
        };
    }
    generation_goal(run.floor)
}

// ---------------------------------------------------------------------------
// scrMakeFloor port... (walls and screen ends are not yet one)
// ---------------------------------------------------------------------------

struct Maker {
    // Position in floor-cell units relative to origin cell.
    x: i32,
    y: i32,
    // GML direction quantized: 0=+x, 90=-y(up), 180=-x, 270=+y(down).
    dir: i32,
}

impl Maker {
    fn step_delta(&self) -> (i32, i32) {
        match self.dir {
            0 => (1, 0),
            90 => (0, -1),
            180 => (-1, 0),
            _ => (0, 1),
        }
    }
}

fn rng_choose<'a, T>(rng: &mut StdRng, items: &'a [T]) -> T
where
    T: Copy,
{
    items[rng.random_range(0..items.len())]
}

fn turn_table(rng: &mut StdRng, area: i32) -> i32 {
    const Z: i32 = 0;
    match area {
        0 => rng_choose(rng, &[Z, Z, 90, -90, 90, -90, 180]),
        2 => rng_choose(rng, &[Z, Z, Z, Z, Z, Z, Z, Z, Z, 90, -90, 90, -90, 180]),
        3 => rng_choose(rng, &[Z, Z, Z, Z, Z, 90, -90]),
        5 => rng_choose(rng, &[Z, Z, Z, Z, Z, Z, Z, Z, Z, Z, Z, Z, Z, 180, 180]),
        6 => rng_choose(rng, &[Z, Z, Z, Z, Z, Z, Z, Z, Z, Z, Z, Z, 90, -90, 180]),
        7 => rng_choose(rng, &[Z, Z, Z, Z, Z, Z, Z, Z, Z, Z, Z, Z, Z, 90, -90, 180]),
        _ => rng_choose(rng, &[Z, Z, Z, Z, Z, Z, Z, Z, Z, Z, 90, -90, 90, -90, 180]),
    }
}

pub fn generate_level(run: &Run) -> LevelPlan {
    let area = gml_area_from_run(run);
    let goal = generation_goal_for_run(run);
    let mut rng = StdRng::seed_from_u64(run.gen_seed);

    let mut plan = LevelPlan {
        floor_cells: Vec::new(),
        wall_cells: std::collections::HashSet::new(),
        small_walls: Vec::new(),
        bones: Vec::new(),
        details: Vec::new(),
        props: Vec::new(),
        chests: Vec::new(),
        enemies: Vec::new(),
        boss: None,
        boss_count: 1,
    };

    let mut seen = std::collections::HashSet::new();
    let stamp_cell = |p: (i32, i32),
                      seen: &mut std::collections::HashSet<(i32, i32)>,
                      out: &mut Vec<(i32, i32)>| {
        if seen.insert(p) {
            out.push(p);
        }
    };

    // GenCont.Create_0: one maker at (10000,10000) heading choose(0,0,90,180,270).
    let mut makers = vec![Maker {
        x: 0,
        y: 0,
        dir: rng_choose(&mut rng, &[0, 0, 90, 180, 270]),
    }];
    stamp_cell((0, 0), &mut seen, &mut plan.floor_cells);

    let mut guard = 0;
    while !makers.is_empty() && plan.floor_cells.len() <= goal {
        guard += 1;
        if guard > 200_000 {
            break;
        }

        let n_makers = makers.len();
        for mi in 0..n_makers {
            // Move 32px along direction (scrMakeFloor start).
            let (dx, dy) = makers[mi].step_delta();
            makers[mi].x += dx;
            makers[mi].y += dy;

            // Stamp floors per-area.
            let (mx, my) = (makers[mi].x, makers[mi].y);
            match area {
                1 => {
                    if rng.random::<f32>() * 2.0 < 1.0 {
                        for p in [(mx, my), (mx + 1, my), (mx + 1, my + 1), (mx, my + 1)] {
                            stamp_cell(p, &mut seen, &mut plan.floor_cells);
                        }
                    } else {
                        stamp_cell((mx, my), &mut seen, &mut plan.floor_cells);
                    }
                }
                3 => {
                    if rng.random::<f32>() * 8.0 < 1.0 {
                        for dy2 in -1..=1 {
                            for dx2 in -1..=1 {
                                stamp_cell((mx + dx2, my + dy2), &mut seen, &mut plan.floor_cells);
                            }
                        }
                    } else {
                        stamp_cell((mx, my), &mut seen, &mut plan.floor_cells);
                    }
                }
                _ => {
                    stamp_cell((mx, my), &mut seen, &mut plan.floor_cells);
                }
            }

            // Turn.
            let trn = turn_table(&mut rng, area);
            makers[mi].dir = (makers[mi].dir + trn).rem_euclid(360);

            // WeaponChest on hard turns (upstream: trn==180 always; +-90 only
            // in scrapyards/palace), away from spawn.
            let dist_from_spawn = ((mx * 32).pow(2) + (my * 32).pow(2)) as f32;
            if dist_from_spawn > 48.0 * 48.0 && (trn == 180 || (trn.abs() == 90 && area == 3)) {
                plan.chests.push(ChestSpawn::Weapon(cell_center_px(mx, my)));
            }

            // Death / branching per-area.
            let n = makers.len() as f32;
            let die_roll = rng.random::<f32>() * (19.0 + n);
            let dies = match area {
                1 => die_roll > 20.0,
                2 => rng.random::<f32>() * (14.0 + n) > 15.0,
                3 => rng.random::<f32>() * (39.0 + n) > 40.0,
                _ => die_roll > 20.0,
            };
            if dies && dist_from_spawn > 48.0 * 48.0 {
                plan.chests.push(ChestSpawn::Ammo(cell_center_px(mx, my)));
                stamp_cell((mx, my), &mut seen, &mut plan.floor_cells);
            }
            if dies {
                makers.remove(mi);
                break;
            }

            // Branching: new maker at this position.
            let branches = match area {
                1 => rng.random::<f32>() * 8.0 < 1.0,
                2 => rng.random::<f32>() * 15.0 < 1.0,
                3 => rng.random::<f32>() * 25.0 < 1.0,
                7 => rng.random::<f32>() * 16.0 < 1.0,
                _ => false,
            };
            if branches && makers.len() < 10 {
                makers.push(Maker {
                    x: mx,
                    y: my,
                    dir: makers[mi].dir,
                });
            }
        }
    }

    // Final floor + RadChest where the furthest floor ended up (stop perk).
    if let Some(&(fx, fy)) = plan
        .floor_cells
        .iter()
        .max_by_key(|c| c.0.abs() + c.1.abs())
    {
        plan.chests.push(ChestSpawn::Rad(cell_center_px(fx, fy)));
    }

    let floors = plan.floor_cells.clone();
    build_walls(run, &floors, &mut plan);
    let walls = plan.wall_cells.clone();
    populate(run, &floors, &walls, &mut plan, &mut rng);
    plan
}

fn cell_center_px(cx: i32, cy: i32) -> Vec2 {
    Vec2::new(cx as f32 * TILE + TILE * 0.5, cy as f32 * TILE + TILE * 0.5)
}

fn cell_center_i(cx: i32, cy: i32) -> (f32, f32) {
    (cx as f32 * TILE + TILE * 0.5, cy as f32 * TILE + TILE * 0.5)
}

/// Lattice (wx,wy) -> world center of one 16px wall cell.
fn wall_center(wx: i32, wy: i32) -> Vec2 {
    Vec2::new(
        wx as f32 * WALL_PX + WALL_PX * 0.5,
        wy as f32 * WALL_PX + WALL_PX * 0.5,
    )
}

// ---------------------------------------------------------------------------
// mcr_floor_make_walls — 12-probe ring on the 16px lattice
// ---------------------------------------------------------------------------

fn build_walls(run: &Run, floors: &[(i32, i32)], plan: &mut LevelPlan) {
    let _ = run;
    let floor_set: std::collections::HashSet<(i32, i32)> = floors.iter().copied().collect();

    for &(cx, cy) in floors {
        // Tile spans lattice cells [2cx..2cx+2) x [2cy..2cy+2).
        // Probe the 12 surrounding 16px positions (mcr_floor_make_walls).
        let probes = [
            (-1, -1),
            (0, -1),
            (1, -1),
            (2, -1), //
            (2, 0),
            (2, 1), //
            (-1, 0),
            (-1, 1), //
            (-1, 2),
            (0, 2),
            (1, 2),
            (2, 2),
        ];
        for (ox, oy) in probes {
            let wx = cx * 2 + ox;
            let wy = cy * 2 + oy;
            // Skip if this lattice cell lies inside any floor tile.
            let owner = (wx.div_euclid(2), wy.div_euclid(2));
            if floor_set.contains(&owner) {
                continue;
            }
            plan.wall_cells.insert((wx, wy));
        }
    }
}

// ---------------------------------------------------------------------------
// scrPopulate / scrPopProps / scrPopEnemies
// ---------------------------------------------------------------------------

fn side_solid(walls: &std::collections::HashSet<(i32, i32)>, cx: i32, cy: i32, dx: i32) -> bool {
    // Full 32px side covered by walls: both vertical halves walled.
    let wx = cx * 2 + if dx < 0 { -1 } else { 2 };
    walls.contains(&(wx, cy * 2)) && walls.contains(&(wx, cy * 2 + 1))
}

fn populate(
    run: &Run,
    floors: &[(i32, i32)],
    walls: &std::collections::HashSet<(i32, i32)>,
    plan: &mut LevelPlan,
    mut rng: &mut StdRng,
) {
    let area = gml_area_from_run(run);
    let boss_sub = is_boss_subarea_run(run);

    // GameCont.hard: +1 per area cleared, +loops. NTT: min enemies = 3 + hard/1.5; per-tile chance = hard / (10 + hard).
    let hard = game_hard(run);
    let enemy_min = (3.0 + hard / 1.5).floor().max(3.0) as usize;
    // Soft ceiling so huge floors don't spawn hundreds on loop 10.
    let enemy_soft_max = (enemy_min * 4).max(24).min(80);

    let mut prop_tiles: std::collections::HashSet<(i32, i32)> = std::collections::HashSet::new();

    for &(cx, cy) in floors {
        let (px, py) = cell_center_i(cx, cy);
        let dist_sq = px * px + py * py;

        // --- Small interior walls (scrPopProps head) ---
        if !boss_sub && rng.random::<f32>() * 5.0 < 1.0 && dist_sq > 100.0 * 100.0 {
            let sx = px + rng.random_range(-8.0..8.0);
            let sy = py + rng.random_range(-8.0..8.0);
            let wx = (sx / WALL_PX).floor() as i32;
            let wy = (sy / WALL_PX).floor() as i32;
            plan.small_walls.push((wx as i16, wy as i16));
            prop_tiles.insert((cx, cy));
        }
    }

    // --- Detail decals + bone strips (scrPopulate decal region) ---
    for &(cx, cy) in floors {
        let (px, py) = cell_center_i(cx, cy);

        if rng.random::<f32>() * 6.0 < 1.0 {
            plan.details.push(Vec2::new(
                px + rng.random_range(-14.0..14.0),
                py + rng.random_range(-14.0..14.0),
            ));
        }

        if area == 1
            && side_solid(walls, cx, cy, -1)
            && side_solid(walls, cx, cy, 1)
            && !walls_cover_tile_with_smalls(plan, cx, cy)
        {
            plan.bones.push((Vec2::new(px - 16.0, py - 16.0), false));
            plan.bones.push((Vec2::new(px - 16.0, py), false));
            plan.bones.push((Vec2::new(px + 16.0, py - 16.0), true));
            plan.bones.push((Vec2::new(px + 16.0, py), true));
        }
    }

    // --- Props pass (scrPopProps, RNGStates.Props) ---
    for &(cx, cy) in floors {
        if prop_tiles.contains(&(cx, cy)) {
            continue;
        }
        let (px, py) = cell_center_i(cx, cy);

        let kind = match area {
            // Desert
            1 => {
                if rng.random::<f32>() * 60.0 < 1.0 {
                    PropKind::BigSkull
                } else if rng.random::<f32>() * 4.0 < 3.0 {
                    PropKind::Cactus
                } else {
                    PropKind::GroundDecal
                }
            }

            // Sewers
            2 => {
                let roll = rng.random_range(0..12);
                match roll {
                    0 => PropKind::ToxicBarrel,
                    1..=3 => PropKind::Barrel,
                    4 => PropKind::GroundDecal,
                    _ => PropKind::Pipe,
                }
            }

            // Scrapyards
            3 => {
                let roll = rng.random_range(0..20);
                match roll {
                    0 => PropKind::Car,
                    1 => PropKind::Mine,
                    2..=10 => PropKind::Tires,
                    11 => PropKind::GroundDecal,
                    _ => PropKind::Pipe,
                }
            }

            // Crystal Caves
            4 => {
                let roll = rng.random_range(0..10);
                match roll {
                    0..=4 => PropKind::Cobweb,
                    5..=7 => PropKind::Cocoon,
                    _ => PropKind::GroundDecal,
                }
            }

            // Frozen City
            5 => {
                let roll = rng.random_range(0..14);
                match roll {
                    0..=7 => PropKind::IcePatch,
                    8..=10 => PropKind::Snowman,
                    11 => PropKind::Car,
                    _ => PropKind::GroundDecal,
                }
            }

            // Labs
            6 => {
                let roll = rng.random_range(0..12);
                match roll {
                    0..=3 => PropKind::ToxicBarrel,
                    4 => PropKind::FireTrap,
                    5 => PropKind::Mine,
                    6..=9 => PropKind::Pipe,
                    _ => PropKind::GroundDecal,
                }
            }

            // Palace
            7 => {
                let roll = rng.random_range(0..12);
                match roll {
                    0 => PropKind::Mine,
                    1..=2 => PropKind::FireTrap,
                    3..=5 => PropKind::Torch,
                    6 => PropKind::BigSkull,
                    _ => PropKind::GroundDecal,
                }
            }

            _ => PropKind::GroundDecal,
        };

        // Functional floor patches occur more frequently than solid props;
        // they do not block enemy/chest placement because they never claim
        // `prop_tiles`.
        let threshold = match kind {
            PropKind::Cobweb | PropKind::IcePatch => 2.6,
            PropKind::FireTrap => 1.35,
            PropKind::Mine => 0.85,
            _ => 1.0,
        };

        // Upstream gate: random(unlikeliness) > threshold exits.
        if rng.random::<f32>() * 10.0 > threshold {
            continue;
        }

        let claims_tile = !matches!(
            kind,
            PropKind::GroundDecal | PropKind::Cobweb | PropKind::IcePatch | PropKind::FireTrap
        );

        if claims_tile {
            prop_tiles.insert((cx, cy));
        }
        plan.props.push((kind, Vec2::new(px, py)));
    }

    // --- Enemy pass (scrPopEnemies) ---
    // Boss floors still get trash mobs upstream; only the bare Throne room
    // (route floor 15) stays sparse so the boss has room.
    let rf_route = ((run.floor.max(1) - 1) % 15) + 1;
    let skip_enemies = boss_sub && rf_route == 15;
    let mut enemy_tiles: Vec<(EnemyKind, Vec2)> = Vec::new();
    for &(cx, cy) in floors {
        if skip_enemies {
            break;
        }
        let (px, py) = cell_center_i(cx, cy);
        let dist_sq = px * px + py * py;
        if dist_sq < 120.0 * 120.0 || prop_tiles.contains(&(cx, cy)) {
            continue;
        }
        if walls_cover_tile(walls, cx, cy)
            || plan
                .small_walls
                .iter()
                .any(|&(wx, wy)| (wx as i32).div_euclid(2) == cx && (wy as i32).div_euclid(2) == cy)
        {
            continue;
        }
        // Upstream: if (random(10 + hard) < hard) area_pop_enemies();
        let chance = hard / (10.0 + hard);
        if rng.random::<f32>() >= chance && enemy_tiles.len() >= enemy_min {
            continue;
        }
        if enemy_tiles.len() >= enemy_soft_max {
            break;
        }

        let center = Vec2::new(px, py);
        let pick_kind = |rng: &mut StdRng, w: &[EnemyKind]| w[rng.random_range(0..w.len())];
        let loop_extras = loop_elite_candidates(area, run.loop_count);

        // --- Secret areas first: each keeps its upstream spawn table ---
        {
            use crate::game::areas::AreaId;
            let secret_kind = match run.area {
                AreaId::Oasis => Some(pick_kind(
                    &mut rng,
                    &[
                        EnemyKind::Bandit,
                        EnemyKind::Bandit,
                        EnemyKind::Scorpion,
                        EnemyKind::Maggot,
                        EnemyKind::Crab,
                    ],
                )),
                AreaId::PizzaSewers => Some(pick_kind(
                    &mut rng,
                    &[
                        EnemyKind::Rat,
                        EnemyKind::Rat,
                        EnemyKind::BigRat,
                        EnemyKind::Freak,
                        EnemyKind::Ballguy,
                    ],
                )),
                AreaId::Jungle => Some(pick_kind(
                    &mut rng,
                    &[
                        EnemyKind::Assassin,
                        EnemyKind::Assassin,
                        EnemyKind::Freak,
                        EnemyKind::Bandit,
                        EnemyKind::Spider,
                    ],
                )),
                AreaId::CursedCaves => Some(pick_kind(
                    &mut rng,
                    &[
                        EnemyKind::Spider,
                        EnemyKind::LaserCrystal,
                        EnemyKind::Crystal,
                        EnemyKind::Freak,
                        EnemyKind::Assassin,
                    ],
                )),
                AreaId::City => {
                    // Y.V. Mansion — light popo / bandits.
                    Some(pick_kind(
                        &mut rng,
                        &[EnemyKind::Bandit, EnemyKind::IdpdGrunt, EnemyKind::Assassin],
                    ))
                }
                AreaId::Vault | AreaId::CrownVault => {
                    // Guardians are the boss; keep trash sparse and elite.
                    Some(pick_kind(
                        &mut rng,
                        &[
                            EnemyKind::RobotGuard,
                            EnemyKind::Turret,
                            EnemyKind::IdpdElite,
                        ],
                    ))
                }
                AreaId::HQ => Some(pick_kind(
                    &mut rng,
                    &[
                        EnemyKind::IdpdGrunt,
                        EnemyKind::IdpdGrunt,
                        EnemyKind::IdpdShield,
                        EnemyKind::IdpdElite,
                    ],
                )),
                _ => None,
            };
            if let Some(k) = secret_kind {
                enemy_tiles.push((k, center));
                continue;
            }
        }

        match area {
            1 => {
                if rng.random::<f32>() * 7.0 < 1.0 {
                    let k = pick_kind(&mut rng, &[EnemyKind::Maggot, EnemyKind::Scorpion]);
                    enemy_tiles.push((k, center));
                } else if rng.random::<f32>() * 30.0 < 1.0 {
                    plan.props.push((PropKind::Barrel, center));
                    for _ in 0..3 {
                        enemy_tiles.push((
                            EnemyKind::Bandit,
                            center
                                + Vec2::new(
                                    rng.random_range(-2.0..2.0),
                                    rng.random_range(-2.0..2.0),
                                ),
                        ));
                    }
                } else {
                    let mut cands = vec![
                        EnemyKind::Bandit,
                        EnemyKind::Bandit,
                        EnemyKind::Bandit,
                        EnemyKind::Bandit,
                        EnemyKind::Bandit,
                        EnemyKind::Bandit,
                        EnemyKind::Maggot,
                        EnemyKind::Scorpion,
                    ];
                    cands.extend(loop_extras.iter().copied());
                    let k = pick_kind(&mut rng, &cands);
                    enemy_tiles.push((k, center));
                }
            }
            2 => {
                // Sewers: rats dominate, with maggots/freaks/bandits mixed in.
                // Loop Sewers swaps most fodder for Ballguys (upstream).
                if run.loop_count > 0 && rng.random::<f32>() * 3.0 >= 1.0 {
                    let mut cands = vec![
                        EnemyKind::Ballguy,
                        EnemyKind::Ballguy,
                        EnemyKind::Ballguy,
                        EnemyKind::Rat,
                        EnemyKind::Freak,
                    ];
                    cands.extend(loop_extras.iter().copied());
                    let k = pick_kind(&mut rng, &cands);
                    enemy_tiles.push((k, center));
                } else if rng.random::<f32>() * 9.0 < 1.0 {
                    let k = pick_kind(
                        &mut rng,
                        &[
                            EnemyKind::BigRat,
                            EnemyKind::Freak,
                            EnemyKind::Bandit,
                            EnemyKind::Scorpion,
                        ],
                    );
                    enemy_tiles.push((k, center));
                } else {
                    let mut cands = vec![
                        EnemyKind::Rat,
                        EnemyKind::Rat,
                        EnemyKind::Rat,
                        EnemyKind::Maggot,
                        EnemyKind::Freak,
                        EnemyKind::Bandit,
                    ];
                    cands.extend(loop_extras.iter().copied());
                    let k = pick_kind(&mut rng, &cands);
                    enemy_tiles.push((k, center));
                }
            }
            3 => {
                // Scrapyards: robot guards, turrets, assassins.
                let mut cands = vec![
                    EnemyKind::RobotGuard,
                    EnemyKind::RobotGuard,
                    EnemyKind::Assassin,
                    EnemyKind::Turret,
                    EnemyKind::Bandit,
                ];
                cands.extend(loop_extras.iter().copied());
                let k = pick_kind(&mut rng, &cands);
                enemy_tiles.push((k, center));
            }
            4 => {
                // Crystal Caves: spiders, crystals, and laser crystals.
                let mut cands = vec![
                    EnemyKind::Spider,
                    EnemyKind::Spider,
                    EnemyKind::Crystal,
                    EnemyKind::LaserCrystal,
                    EnemyKind::Freak,
                ];
                cands.extend(loop_extras.iter().copied());
                let k = pick_kind(&mut rng, &cands);
                enemy_tiles.push((k, center));
            }
            5 => {
                // Frozen City: snow bandits, wolves, snipers; IDPD on loops.
                let mut frozen = vec![
                    EnemyKind::SnowBandit,
                    EnemyKind::SnowBandit,
                    EnemyKind::Wolf,
                    EnemyKind::Sniper,
                    EnemyKind::Assassin,
                ];
                if run.loop_count > 0 {
                    frozen.extend(std::iter::repeat_n(
                        EnemyKind::IdpdGrunt,
                        (run.loop_count.min(3) * 2) as usize,
                    ));
                }
                frozen.extend(loop_extras.iter().copied());
                let k = pick_kind(&mut rng, &frozen);
                enemy_tiles.push((k, center));
            }
            6 | 7 => {
                // Labs / Palace: mixed late-game garrisons with necromancers;
                // IDPD squads scale with loop count.
                let mut late = vec![
                    EnemyKind::RobotGuard,
                    EnemyKind::Necromancer,
                    EnemyKind::Assassin,
                    EnemyKind::Freak,
                    EnemyKind::Turret,
                ];
                late.extend(std::iter::repeat_n(
                    EnemyKind::IdpdGrunt,
                    (run.loop_count.min(3) * 2) as usize,
                ));
                late.extend(std::iter::repeat_n(
                    EnemyKind::IdpdShield,
                    run.loop_count.min(3) as usize,
                ));
                if run.loop_count >= 2 {
                    late.push(EnemyKind::IdpdElite);
                }
                late.extend(loop_extras.iter().copied());
                let k = pick_kind(&mut rng, &late);
                enemy_tiles.push((k, center));
            }
            _ => {}
        }
    }

    // Guarantee minimum enemy count by filling from distant floor cells.
    if !skip_enemies {
        let mut extras = floors
            .iter()
            .copied()
            .filter(|(cx, cy)| {
                let (px, py) = cell_center_i(*cx, *cy);
                let d = px * px + py * py;
                d >= 120.0 * 120.0 && !prop_tiles.contains(&(*cx, *cy))
            })
            .collect::<Vec<_>>();
        extras.sort_by_key(|&(cx, cy)| {
            let (px, py) = cell_center_i(cx, cy);
            -((px * px + py * py) as i32)
        });
        let mut ei = 0;
        while enemy_tiles.len() < enemy_min && ei < extras.len() {
            let (cx, cy) = extras[ei];
            ei += 1;
            let center = cell_center_px(cx, cy);
            if enemy_tiles.iter().any(|(_, p)| p.distance(center) < 8.0) {
                continue;
            }
            let cands = default_area_enemies(area, run.loop_count);
            if cands.is_empty() {
                break;
            }
            let k = cands[rng.random_range(0..cands.len())];
            enemy_tiles.push((k, center));
        }
    }
    plan.enemies = enemy_tiles;

    // Bosses. Looped Crystal Caves visits get the Hyper Crystal instead of a
    // quiet single-floor stop. Loop Sewers gets Mom, loop Labs the
    // Technomancer; HQ runs and the Crown Vault host their own bosses.
    if boss_sub {
        let kind = boss_for_floor_and_loop(run.floor, run.loop_count);
        plan.boss = Some(kind);
        if matches!(kind, EnemyKind::BigBandit | EnemyKind::BigBanditLoop) {
            plan.boss_count = big_bandit_count(run.loop_count);
        }
    } else {
        match run.area {
            AreaId::Sewers if run.loop_count >= 1 => plan.boss = Some(EnemyKind::Mom),
            AreaId::Labs if run.loop_count >= 1 => plan.boss = Some(EnemyKind::Technomancer),
            AreaId::CrystalCaves if run.loop_count >= 1 => plan.boss = Some(EnemyKind::Hyper),
            AreaId::CrownVault | AreaId::Vault => plan.boss = Some(EnemyKind::OldGuardian),
            AreaId::HQ => plan.boss = Some(EnemyKind::Captain),
            _ => {}
        }
    }

    // Palace throne room set piece (route floor 15)
    let rf = ((run.floor.max(1) - 1) % 15) + 1;
    if rf == 15 && !crate::game::secret_areas::is_secret_area(run.area) {
        populate_throne_room(run, plan);
    }

    // Chest trimming (scrPopChests): keep the furthest of each kind.
    trim_chests(&mut plan.chests);
}

/// Loop-only elite substitutions appended to an area's spawn table.
///
/// Weighted entries (`kind, weight`) are expanded into repeated pick
/// candidates by the enemy pass.
fn apply_loop_elite_substitutions(
    table: &mut Vec<(EnemyKind, usize)>,
    area: AreaId,
    loop_count: u32,
) {
    if loop_count == 0 {
        return;
    }

    let l = loop_count.min(4) as usize;

    match area {
        AreaId::Desert => {
            table.push((EnemyKind::SnowBandit, 4 + l * 2));
            table.push((EnemyKind::IdpdGrunt, 3 + l));
        }
        AreaId::Sewers => {
            table.push((EnemyKind::BigRat, 5 + l * 2));
            table.push((EnemyKind::IdpdShield, 2 + l));
        }
        AreaId::Scrapyards => {
            table.push((EnemyKind::RobotGuard, 5 + l * 2));
            table.push((EnemyKind::Turret, 3 + l));
            table.push((EnemyKind::IdpdGrunt, 4 + l));
        }
        AreaId::CrystalCaves => {
            table.push((EnemyKind::Freak, 5 + l * 2));
            table.push((EnemyKind::Assassin, 4 + l));
            if loop_count >= 2 {
                table.push((EnemyKind::IdpdElite, 3 + l));
            }
        }
        AreaId::FrozenCity => {
            table.push((EnemyKind::Wolf, 4 + l * 2));
            table.push((EnemyKind::IdpdShield, 4 + l));
        }
        AreaId::Labs | AreaId::Palace => {
            if loop_count >= 2 {
                table.push((EnemyKind::IdpdElite, 5 + l));
            }
        }
        _ => {}
    }
}

/// Expanded pick candidates for the current floor's route area.
fn loop_elite_candidates(area_num: i32, loop_count: u32) -> Vec<EnemyKind> {
    let area = match area_num {
        1 => AreaId::Desert,
        2 => AreaId::Sewers,
        3 => AreaId::Scrapyards,
        4 => AreaId::CrystalCaves,
        5 => AreaId::FrozenCity,
        6 => AreaId::Labs,
        7 => AreaId::Palace,
        _ => return Vec::new(),
    };

    let mut table = Vec::new();
    apply_loop_elite_substitutions(&mut table, area, loop_count);

    let mut out = Vec::new();
    for (kind, weight) in table {
        for _ in 0..weight.min(8) {
            out.push(kind);
        }
    }
    out
}

/// Upstream GameCont.hard approximation.
pub fn game_hard(run: &Run) -> f32 {
    // +1 per area finished (each multi-floor world step), +1 per loop.
    let areas_done = (run.floor.max(1) - 1) as f32;
    // Desert1 starts hard≈1 after first clear; seed at least 1 so floor 1 has enemies.
    (1.0 + areas_done * 0.55 + run.loop_count as f32 * 2.0).max(1.0)
}

fn default_area_enemies(area: i32, loop_count: u32) -> Vec<EnemyKind> {
    let mut c = match area {
        1 => vec![
            EnemyKind::Bandit,
            EnemyKind::Bandit,
            EnemyKind::Bandit,
            EnemyKind::Maggot,
            EnemyKind::Scorpion,
        ],
        2 => vec![
            EnemyKind::Rat,
            EnemyKind::Rat,
            EnemyKind::Maggot,
            EnemyKind::Freak,
        ],
        3 => vec![
            EnemyKind::RobotGuard,
            EnemyKind::Assassin,
            EnemyKind::Turret,
            EnemyKind::Bandit,
        ],
        4 => vec![
            EnemyKind::Spider,
            EnemyKind::Spider,
            EnemyKind::Crystal,
            EnemyKind::LaserCrystal,
            EnemyKind::Freak,
        ],
        5 => vec![
            EnemyKind::SnowBandit,
            EnemyKind::SnowBandit,
            EnemyKind::Wolf,
            EnemyKind::Sniper,
            EnemyKind::Assassin,
        ],
        _ => vec![
            EnemyKind::RobotGuard,
            EnemyKind::Assassin,
            EnemyKind::Freak,
            EnemyKind::Turret,
        ],
    };
    c.extend(loop_elite_candidates(area, loop_count));
    c
}

fn walls_cover_tile_with_smalls(plan: &LevelPlan, cx: i32, cy: i32) -> bool {
    plan.small_walls
        .iter()
        .any(|&(wx, wy)| (wx as i32).div_euclid(2) == cx && (wy as i32).div_euclid(2) == cy)
}

fn walls_cover_tile(walls: &std::collections::HashSet<(i32, i32)>, cx: i32, cy: i32) -> bool {
    for ox in 0..2 {
        for oy in 0..2 {
            if walls.contains(&(cx * 2 + ox, cy * 2 + oy)) {
                return true;
            }
        }
    }
    false
}

fn populate_throne_room(run: &Run, plan: &mut LevelPlan) {
    // Keep palace throne room sparse: remove clutter traps/mines.
    plan.props
        .retain(|(k, _)| !matches!(k, PropKind::Mine | PropKind::FireTrap));
    plan.enemies.clear();

    let gens = [
        Vec2::new(-220.0, 120.0),
        Vec2::new(220.0, 120.0),
        Vec2::new(-220.0, -120.0),
        Vec2::new(220.0, -120.0),
    ];
    for p in gens {
        plan.props.push((PropKind::BigGenerator, p));
    }
    let statues = [
        Vec2::new(-70.0, 160.0),
        Vec2::new(70.0, 160.0),
        Vec2::new(-70.0, 40.0),
        Vec2::new(70.0, 40.0),
        Vec2::new(-70.0, -80.0),
        Vec2::new(70.0, -80.0),
    ];
    for p in statues {
        plan.props.push((PropKind::ThroneStatue, p));
    }
    plan.boss = Some(EnemyKind::Throne);
    plan.boss_count = 1;
}

fn trim_chests(chests: &mut Vec<ChestSpawn>) {
    use std::collections::HashMap;
    let mut furthest: HashMap<u8, ChestSpawn> = HashMap::new();
    for c in chests.iter().copied() {
        let key = match c {
            ChestSpawn::Weapon(_) => 0u8,
            ChestSpawn::Ammo(_) => 1,
            ChestSpawn::Rad(_) => 2,
        };
        let d = match c {
            ChestSpawn::Weapon(p) | ChestSpawn::Ammo(p) | ChestSpawn::Rad(p) => p.length_squared(),
        };
        let keep = match furthest.get(&key) {
            Some(existing) => {
                let ed = match *existing {
                    ChestSpawn::Weapon(p) | ChestSpawn::Ammo(p) | ChestSpawn::Rad(p) => {
                        p.length_squared()
                    }
                };
                d > ed
            }
            None => true,
        };
        if keep {
            furthest.insert(key, c);
        }
    }
    chests.clear();
    for (_, c) in furthest {
        chests.push(c);
    }
}

// ---------------------------------------------------------------------------
// Spawning
// ---------------------------------------------------------------------------

fn sprite_frames(catalog: &AssetCatalog, path: &str) -> usize {
    catalog
        .anims
        .get(path)
        .map(|m| m[0].max(1.0) as usize)
        .unwrap_or(1)
}

fn sprite_exact_frame(
    catalog: &AssetCatalog,
    asset_server: &AssetServer,
    path: &str,
    frame: usize,
) -> Sprite {
    let mut sprite = sprite_exact(catalog, asset_server, path);
    if let Some(m) = catalog.anims.get(path)
        && m[0] > 1.0
    {
        let f = frame % sprite_frames(catalog, path).max(1);
        let w = m[1].max(1.0);
        let h = m[2].max(1.0);
        sprite.rect = Some(Rect::new(f as f32 * w, 0.0, (f + 1) as f32 * w, h));
    }
    sprite
}

fn wall_hash(seed: u64, wx: i32, wy: i32, salt: u64) -> u64 {
    let mut x = seed
        ^ ((wx as i64 as u64) << 32)
        ^ (wy as i64 as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ salt;
    x ^= x >> 30;
    x = x.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

fn wall_body_frame(catalog: &AssetCatalog, seed: u64, wx: i32, wy: i32, path: &str) -> usize {
    // Wall/Create_0 image_index
    let raw = if wall_hash(seed, wx, wy, 0x11) % 150 == 0 {
        3
    } else {
        [0usize, 0, 0, 0, 0, 0, 0, 1, 2][(wall_hash(seed, wx, wy, 0x12) % 9) as usize]
            + [0usize, 4][(wall_hash(seed, wx, wy, 0x13) % 2) as usize]
    };
    raw % sprite_frames(catalog, path).max(1)
}

fn wall_top_frame(catalog: &AssetCatalog, seed: u64, wx: i32, wy: i32, path: &str) -> usize {
    let raw = if wall_hash(seed, wx, wy, 0x21) % 200 == 0 {
        3
    } else {
        [0usize, 0, 0, 0, 0, 0, 0, 1, 2][(wall_hash(seed, wx, wy, 0x22) % 9) as usize]
            + [0usize, 4, 8][(wall_hash(seed, wx, wy, 0x23) % 3) as usize]
    };
    raw % sprite_frames(catalog, path).max(1)
}

fn wall_out_frame(catalog: &AssetCatalog, seed: u64, wx: i32, wy: i32, path: &str) -> usize {
    let raw = [0usize, 0, 0, 0, 1, 2, 3, 4][(wall_hash(seed, wx, wy, 0x31) % 8) as usize]
        + [0usize, 4][(wall_hash(seed, wx, wy, 0x32) % 2) as usize];
    raw % sprite_frames(catalog, path).max(1)
}

fn area_sprites(
    floor: u32,
) -> (
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
) {
    // (floor, wall bot, wall top, wall out, ground decal)
    // Upstream sprite families are named by _area id.
    let rf = ((floor.max(1) - 1) % 15) + 1;
    match rf {
        3 => (
            "images/sprFloor0.png",
            "images/sprWall0Bot.png",
            "images/sprWall0Top.png",
            "images/sprWall0Out.png",
            "images/sprNightDesertTopDecal.png",
        ),
        4 => (
            "images/sprFloor2.png",
            "images/sprWall2Bot.png",
            "images/sprWall2Top.png",
            "images/sprWall2Out.png",
            "images/sprTopDecalSewers.png",
        ),
        5..=7 => (
            "images/sprFloor3.png",
            "images/sprWall3Bot.png",
            "images/sprWall3Top.png",
            "images/sprWall3Out.png",
            "images/sprTopDecalScrapyard.png",
        ),
        8 => (
            "images/sprFloor4.png",
            "images/sprWall4Bot.png",
            "images/sprWall4Top.png",
            "images/sprWall4Out.png",
            "images/sprTopDecalCave.png",
        ),
        9..=11 => (
            "images/sprFloor5.png",
            "images/sprWall5Bot.png",
            "images/sprWall5Top.png",
            "images/sprWall5Out.png",
            "images/sprTopDecalCity.png",
        ),
        12 => (
            "images/sprFloor6.png",
            "images/sprWall6Bot.png",
            "images/sprWall6Top.png",
            "images/sprWall6Out.png",
            "images/sprTopDecalCity.png",
        ),
        13..=15 => (
            "images/sprFloor7.png",
            "images/sprWall7Bot.png",
            "images/sprWall7Top.png",
            "images/sprWall7Out.png",
            "images/sprPalaceTopDecal.png",
        ),
        _ => (
            "images/sprFloor1.png",
            "images/sprWall1Bot.png",
            "images/sprWall1Top.png",
            "images/sprWall1Out.png",
            "images/sprDesertTopDecal.png",
        ),
    }
}

/// Secret areas use their own tile families (100-series art); any slot whose
/// PNG was not imported falls back to the route family so `sprite_exact`
/// never hits its missing-asset panic.
pub(crate) fn area_sprites_for_run(
    run: &Run,
    catalog: &AssetCatalog,
) -> (
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
) {
    use crate::game::areas::AreaId;
    let route = area_sprites(run.floor);
    let is_secret_tile_family = matches!(
        run.area,
        AreaId::Oasis
            | AreaId::PizzaSewers
            | AreaId::City
            | AreaId::CursedCaves
            | AreaId::Vault
            | AreaId::CrownVault
            | AreaId::Jungle
            | AreaId::HQ
    );
    if !is_secret_tile_family {
        return route;
    }

    let (floor, bot, top, out) = match run.area {
        AreaId::Oasis => (
            "images/sprFloor101.png",
            "images/sprWall101Bot.png",
            "images/sprWall101Top.png",
            "images/sprWall101Out.png",
        ),
        AreaId::PizzaSewers => (
            "images/sprFloor102.png",
            "images/sprWall102Bot.png",
            "images/sprWall102Top.png",
            "images/sprWall102Out.png",
        ),
        AreaId::City => (
            "images/sprFloor103.png",
            "images/sprWall103Bot.png",
            "images/sprWall103Top.png",
            "images/sprWall103Out.png",
        ),
        AreaId::CursedCaves | AreaId::Vault | AreaId::CrownVault => (
            "images/sprFloor104.png",
            "images/sprWall104Bot.png",
            "images/sprWall104Top.png",
            "images/sprWall104Out.png",
        ),
        AreaId::Jungle => (
            "images/sprFloor105.png",
            "images/sprWall105Bot.png",
            "images/sprWall105Top.png",
            "images/sprWall105Out.png",
        ),
        _ => (
            "images/sprFloor106.png",
            "images/sprWall106Bot.png",
            "images/sprWall106Top.png",
            "images/sprWall106Out.png",
        ),
    };

    let slot = |secret: &'static str, fallback: &'static str| {
        if catalog.has(secret) {
            secret
        } else {
            fallback
        }
    };

    (
        slot(floor, route.0),
        slot(bot, route.1),
        slot(top, route.2),
        slot(out, route.3),
        route.4,
    )
}

pub fn spawn_level(
    commands: &mut Commands,
    catalog: &AssetCatalog,
    asset_server: &AssetServer,
    run: &Run,
    plan: &LevelPlan,
    mask: &mut FloorMask,
) {
    let (floor_png, wall_bot_png, wall_top_png, wall_out_png, decal_prop_png) =
        area_sprites_for_run(run, catalog);

    let cols = (ARENA_W / TILE) as i32;
    let rows = (ARENA_H / TILE) as i32;

    // Publish walkable mask.
    *mask = FloorMask {
        cells: plan.floor_cells.iter().copied().collect(),
        cols,
        rows,
    };

    // Outside / void ring beyond walls (upstream Outside + floorex).
    // Fill a bounding pad around the level with dark exterior tiles so the
    // area beyond walls matches the GM rewrite instead of empty camera void.
    {
        let (min_c, max_c) = {
            let mut minx = i32::MAX;
            let mut miny = i32::MAX;
            let mut maxx = i32::MIN;
            let mut maxy = i32::MIN;
            for &(cx, cy) in &plan.floor_cells {
                minx = minx.min(cx);
                miny = miny.min(cy);
                maxx = maxx.max(cx);
                maxy = maxy.max(cy);
            }
            ((minx - 6, miny - 6), (maxx + 6, maxy + 6))
        };
        let floor_set: std::collections::HashSet<(i32, i32)> =
            plan.floor_cells.iter().copied().collect();
        let outside_png = match ((run.floor.max(1) - 1) % 15) + 1 {
            4 => "images/sprFloor2.png",
            5..=7 => "images/sprFloor3.png",
            8 => "images/sprFloor4.png",
            9..=11 => "images/sprFloor5.png",
            12 => "images/sprFloor6.png",
            13..=15 => "images/sprFloor7.png",
            3 => "images/sprFloor0.png",
            _ => "images/sprFloor1.png",
        };
        // Prefer dedicated exterior if imported.
        let outside_png = if catalog.has("images/sprFloorEx1.png") {
            "images/sprFloorEx1.png"
        } else {
            outside_png
        };
        for cy in min_c.1..=max_c.1 {
            for cx in min_c.0..=max_c.0 {
                if floor_set.contains(&(cx, cy)) {
                    continue;
                }
                let (wx, wy) = cell_center_i(cx, cy);
                let mut spr = sprite_exact(catalog, asset_server, outside_png);
                // Dim exterior so walls read clearly (GM draws Outside darker).
                spr.color = Color::srgb(0.45, 0.45, 0.48);
                commands.spawn((
                    GameCleanup,
                    LevelCleanup,
                    spr,
                    Transform::from_xyz(wx, wy, -60.0),
                ));
            }
        }
    }

    // Floors.
    for &(cx, cy) in &plan.floor_cells {
        let (wx, wy) = cell_center_i(cx, cy);
        commands.spawn((
            GameCleanup,
            LevelCleanup,
            sprite_exact(catalog, asset_server, floor_png),
            Transform::from_xyz(wx, wy, -50.0),
        ));
    }

    // Detail decals.
    for pos in &plan.details {
        commands.spawn((
            GameCleanup,
            LevelCleanup,
            sprite_exact(catalog, asset_server, "images/sprDetail0.png"),
            Transform::from_xyz(pos.x, pos.y, -45.0),
        ));
    }

    // Bones strips (16px pieces, mirrored halves).
    for (pos, flip) in &plan.bones {
        commands.spawn((
            GameCleanup,
            LevelCleanup,
            Sprite {
                flip_x: *flip,
                ..sprite_exact(catalog, asset_server, "images/sprBones.png")
            },
            Transform::from_xyz(pos.x, pos.y, -44.0),
        ));
    }

    // Wall bodies (Bot): visible only when a Floor overlaps the probe point
    // (x, y + 16) — the exact upstream rule (`place_meeting(x, y + 16, Floor)`).
    // The Top face overlays every wall 8px higher.
    let floor_set: std::collections::HashSet<(i32, i32)> =
        plan.floor_cells.iter().copied().collect();
    // Ring walls + interior small walls share the same renderer.
    let mut all_walls: Vec<(i32, i32)> = plan.wall_cells.iter().copied().collect();
    all_walls.extend(
        plan.small_walls
            .iter()
            .map(|&(wx, wy)| (wx as i32, wy as i32)),
    );
    for (wx, wy) in all_walls {
        let c = wall_center(wx, wy);
        let body_frame = wall_body_frame(catalog, run.gen_seed, wx, wy, wall_bot_png);
        let top_frame = wall_top_frame(catalog, run.gen_seed, wx, wy, wall_top_png);
        let out_frame = wall_out_frame(catalog, run.gen_seed, wx, wy, wall_out_png);

        // GML place_meeting(x, y+16, Floor) with y-down.
        // Bevy y-up lattice: owner of the cell one step "south" on screen.
        let owner = (wx.div_euclid(2), (wy - 1).div_euclid(2));
        let floor_below = floor_set.contains(&owner);

        // Visuals first so they can be linked to the solid via WallVisuals.
        let mut parts: Vec<Entity> = Vec::with_capacity(3);

        if catalog.has(wall_out_png) {
            let e = commands
                .spawn((
                    GameCleanup,
                    LevelCleanup,
                    sprite_exact_frame(catalog, asset_server, wall_out_png, out_frame),
                    Transform::from_xyz(c.x, c.y, -42.0),
                ))
                .id();
            parts.push(e);
        }

        if floor_below {
            let e = commands
                .spawn((
                    GameCleanup,
                    LevelCleanup,
                    sprite_exact_frame(catalog, asset_server, wall_bot_png, body_frame),
                    Transform::from_xyz(c.x, c.y, -40.0),
                ))
                .id();
            parts.push(e);
        }

        {
            let e = commands
                .spawn((
                    GameCleanup,
                    LevelCleanup,
                    sprite_exact_frame(catalog, asset_server, wall_top_png, top_frame),
                    Transform::from_xyz(c.x, c.y + 8.0, -36.0),
                ))
                .id();
            parts.push(e);
        }

        // Collision body (16px solid). Walls only break through the explicit
        // PendingWallBreak pipeline (hammerhead / charges / explosions), never
        // by generic projectile erosion.
        let wall_e = commands
            .spawn((
                GameCleanup,
                LevelCleanup,
                WallTile,
                WallCell(wx, wy),
                WallVisuals { parts },
                Prop {
                    size: Vec2::splat(WALL_PX),
                    hp: 9999,
                    destructible: false,
                    explosive: false,
                },
                Transform::from_xyz(c.x, c.y, -30.0),
            ))
            .id();
        // Outer ring walls preferentially break for boss intros / generators.
        let is_screen_end = !floor_set.contains(&floor_cell_for_wall(wx, wy));
        if is_screen_end {
            commands.entity(wall_e).insert(ScreenEnd);
        }
    }

    // Props.
    for (kind, pos) in &plan.props {
        spawn_prop(
            commands,
            catalog,
            asset_server,
            *kind,
            *pos,
            decal_prop_png,
            run,
        );
    }

    // Secret entrances (destructible markers; destroying one queues the
    // secret via the SecretEntrance component).
    spawn_secret_entrances(commands, catalog, asset_server, run);

    // Chests.
    for chest in &plan.chests {
        let (kind, pos) = match *chest {
            ChestSpawn::Weapon(p) => (ChestKind::Weapon, p),
            ChestSpawn::Ammo(p) => (ChestKind::Ammo, p),
            ChestSpawn::Rad(p) => (ChestKind::Rad, p),
        };
        crate::game::pickups::spawn_chest(commands, catalog, asset_server, kind, pos);
    }

    // Enemies.
    for (kind, pos) in &plan.enemies {
        crate::game::enemies::spawn_enemy_at(
            commands,
            catalog,
            asset_server,
            *kind,
            *pos,
            difficulty_multiplier(run.floor),
            false,
            false,
        );
    }
    if let Some(kind) = plan.boss {
        match kind {
            EnemyKind::BigBandit | EnemyKind::BigBanditLoop => {
                // Upstream: loop_count *2 simultaneous (L1→2, L2→4…); each
                // bursts from a wall after a staggered kill fraction.
                let n = plan.boss_count.max(1);
                for i in 0..n {
                    commands.spawn((
                        GameCleanup,
                        LevelCleanup,
                        PendingDelayedBoss {
                            kind,
                            initial_trash: (plan.enemies.len() as u32).max(1),
                            kill_fraction: 0.10 + (i as f32) * 0.02,
                            from_wall: true,
                        },
                    ));
                }
            }
            EnemyKind::BigDog | EnemyKind::BigDogLoop => {
                // Sleeping opposite side of the map.
                crate::game::enemies::spawn_enemy_at(
                    commands,
                    catalog,
                    asset_server,
                    kind,
                    Vec2::new(-280.0, 180.0),
                    difficulty_multiplier(run.floor),
                    false,
                    false,
                );
            }
            other => {
                let pos = match other {
                    EnemyKind::Throne => Vec2::new(0.0, 200.0),
                    EnemyKind::Mom => Vec2::new(0.0, -40.0),
                    EnemyKind::Technomancer => Vec2::new(0.0, 0.0),
                    EnemyKind::Captain => Vec2::new(0.0, 80.0),
                    EnemyKind::OldGuardian => Vec2::new(0.0, 60.0),
                    EnemyKind::Hyper => Vec2::new(0.0, 0.0),
                    _ => Vec2::new(320.0, -160.0),
                };
                crate::game::enemies::spawn_enemy_at(
                    commands,
                    catalog,
                    asset_server,
                    other,
                    pos,
                    difficulty_multiplier(run.floor),
                    false,
                    false,
                );
            }
        }
    }

    // Throne carpet (visual + laser trigger volume) when Throne is the boss.
    if matches!(plan.boss, Some(EnemyKind::Throne)) {
        commands.spawn((
            GameCleanup,
            LevelCleanup,
            ThroneCarpet {
                half_extents: Vec2::new(36.0, 240.0),
            },
            Sprite {
                color: Color::srgba(0.75, 0.12, 0.14, 0.85),
                custom_size: Some(Vec2::new(72.0, 480.0)),
                ..default()
            },
            Transform::from_xyz(0.0, 0.0, -48.0),
        ));
    }

    // Crown Vault: a crown pedestal near the center (pick on contact).
    if run.area == AreaId::CrownVault {
        let pool = [
            CrownKind::Life,
            CrownKind::Haste,
            CrownKind::Guns,
            CrownKind::Blood,
            CrownKind::Luck,
            CrownKind::Protection,
            CrownKind::Love,
            CrownKind::Risk,
            CrownKind::Destiny,
            CrownKind::Curses,
            CrownKind::Hatred,
        ];
        let kind = pool[rand::rng().random_range(0..pool.len())];
        commands.spawn((
            GameCleanup,
            LevelCleanup,
            CrownPedestal { kind },
            Sprite {
                color: Color::srgb(1.0, 0.85, 0.25),
                custom_size: Some(Vec2::splat(28.0)),
                ..default()
            },
            Transform::from_translation(Vec2::new(0.0, 40.0).extend(14.0)),
        ));
    }
}

/// Destructible secret-entrance markers for the current area/floor.
/// Destroying one queues its secret target (see combat's prop-death hook).
fn spawn_secret_entrances(
    commands: &mut Commands,
    catalog: &AssetCatalog,
    asset_server: &AssetServer,
    run: &Run,
) {
    let maybe = match (run.area, run.floor_in_area) {
        // Sewers manhole.
        (AreaId::Sewers, _) => Some((
            SecretTarget::PizzaSewers,
            "images/sprPipe.png",
            Vec2::new(220.0, -120.0),
            28.0,
        )),
        // Proto/crown statue appears on the stage before each boss floor.
        (AreaId::Desert, 2) | (AreaId::Scrapyards, 2) | (AreaId::FrozenCity, 2) => Some((
            SecretTarget::CrownVault,
            "images/sprOldGuardianStatue.png",
            Vec2::new(-240.0, 160.0),
            34.0,
        )),
        // Y.V. Mansion hook in Scrapyards.
        (AreaId::Scrapyards, 1) => Some((
            SecretTarget::YvMansion,
            "images/sprCarIdle.png",
            Vec2::new(260.0, 140.0),
            36.0,
        )),
        // Jungle hook in Frozen City.
        (AreaId::FrozenCity, 1) => Some((
            SecretTarget::Jungle,
            "images/sprBushIdle.png",
            Vec2::new(-260.0, -140.0),
            30.0,
        )),
        _ => None,
    };

    let Some((target, sprite, pos, size)) = maybe else {
        return;
    };

    // HP scales with loop (upstream proto statue tanks).
    let hp = if matches!(target, SecretTarget::CrownVault | SecretTarget::Vault) {
        120 + run.loop_count as i32 * 12
    } else {
        6
    };
    let mut ec = commands.spawn((
        GameCleanup,
        LevelCleanup,
        SecretEntrance { target },
        Prop {
            size: Vec2::splat(size),
            hp,
            destructible: true,
            explosive: false,
        },
        sprite_exact(catalog, asset_server, sprite),
        Transform::from_translation(pos.extend(12.0)),
    ));

    match target {
        SecretTarget::PizzaSewers => {
            ec.insert(ManholeCover);
        }
        SecretTarget::CrownVault | SecretTarget::Vault => {
            ec.insert(ProtoStatue);
            // Four guards around the statue (upstream vault entrance).
            let guard = if run.area == AreaId::FrozenCity {
                EnemyKind::SnowBandit
            } else {
                EnemyKind::Bandit
            };
            for i in 0..4 {
                let ang = i as f32 * std::f32::consts::FRAC_PI_2;
                let p = pos + Vec2::from_angle(ang) * 36.0;
                crate::game::enemies::spawn_enemy_at(
                    commands,
                    catalog,
                    asset_server,
                    guard,
                    p,
                    crate::game::world::difficulty_multiplier(run.floor),
                    false,
                    false,
                );
            }
        }
        SecretTarget::YvMansion => {
            ec.insert(GoldCar);
        }
        SecretTarget::Jungle => {
            ec.insert(BloodFlower);
        }
        _ => {}
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_prop(
    commands: &mut Commands,
    catalog: &AssetCatalog,
    asset_server: &AssetServer,
    kind: PropKind,
    pos: Vec2,
    decal_png: &'static str,
    run: &Run,
) {
    // --- Functional floor / hazard entities (no solid Prop component) ---
    match kind {
        PropKind::Cobweb => {
            commands.spawn((
                GameCleanup,
                LevelCleanup,
                SurfaceZone {
                    kind: SurfaceKind::Cobweb,
                    half_size: Vec2::splat(18.0),
                },
                SurfacePulse::subtle(pos.x * 0.017),
                sprite_from_candidates(
                    catalog,
                    asset_server,
                    &[
                        "images/sprCobweb.png",
                        "images/sprSpiderWeb.png",
                        "images/sprWeb.png",
                        "images/sprCocoon.png",
                        "images/sprBones.png",
                    ],
                    Color::srgba(0.78, 0.78, 0.72, 0.62),
                    Vec2::splat(36.0),
                ),
                Transform::from_translation(pos.extend(-41.0)),
            ));
            return;
        }

        PropKind::IcePatch => {
            commands.spawn((
                GameCleanup,
                LevelCleanup,
                SurfaceZone {
                    kind: SurfaceKind::Ice,
                    half_size: Vec2::splat(20.0),
                },
                SurfacePulse::subtle(pos.y * 0.014),
                sprite_from_candidates(
                    catalog,
                    asset_server,
                    &["images/sprIceDecal.png", "images/sprIcePatch.png"],
                    Color::srgba(0.62, 0.86, 1.0, 0.58),
                    Vec2::splat(40.0),
                ),
                Transform::from_translation(pos.extend(-41.0)),
            ));
            return;
        }

        PropKind::FireTrap => {
            let spec = EnvironmentHazardSpec::fire_trap();

            commands.spawn((
                GameCleanup,
                LevelCleanup,
                EnvironmentHazard::new(spec),
                SurfacePulse::hazard(pos.x * 0.011),
                sprite_from_candidates(
                    catalog,
                    asset_server,
                    &[
                        "images/sprTrapFire.png",
                        "images/sprFireTrap.png",
                        "images/sprFireTrapIdle.png",
                        "images/sprTorchFire.png",
                        // Fix: fallback to existing Torch and Flame assets (WAD has sprTorch but not FireTrap)
                        "images/sprTorch.png",
                        "images/sprFlameBall.png",
                    ],
                    spec.kind.color(),
                    Vec2::splat(32.0),
                ),
                Transform::from_translation(pos.extend(5.0)),
            ));
            return;
        }

        PropKind::Mine => {
            commands.spawn((
                GameCleanup,
                LevelCleanup,
                Prop {
                    size: Vec2::splat(18.0),
                    hp: 2,
                    destructible: true,
                    explosive: false,
                },
                ProximityMine::default(),
                PropDeathEffect::mine(),
                SurfacePulse::hazard(pos.y * 0.019),
                sprite_from_candidates(
                    catalog,
                    asset_server,
                    &["images/sprMine.png", "images/sprMineIdle.png"],
                    Color::srgb(0.86, 0.25, 0.18),
                    Vec2::splat(18.0),
                ),
                Transform::from_translation(pos.extend(-8.0)),
            ));
            return;
        }

        _ => {}
    }

    // --- Ordinary solid props / decals ---
    let (
        candidates,
        fallback_color,
        fallback_size,
        collision_size,
        hp,
        destructible,
        legacy_explosive,
        death_effect,
        z,
        solid,
    ): (
        &[&str],
        Color,
        Vec2,
        f32,
        i32,
        bool,
        bool,
        Option<PropDeathEffect>,
        f32,
        bool,
    ) = match kind {
        PropKind::Cactus => (
            &["images/sprCactus.png"],
            Color::srgb(0.38, 0.72, 0.28),
            Vec2::splat(24.0),
            24.0,
            4,
            true,
            false,
            None,
            -10.0,
            true,
        ),

        PropKind::BigSkull => (
            &["images/sprBigSkull.png"],
            Color::srgb(0.82, 0.78, 0.62),
            Vec2::splat(32.0),
            32.0,
            8,
            true,
            false,
            None,
            -10.0,
            true,
        ),

        PropKind::Barrel => (
            &["images/sprBarrel.png"],
            Color::srgb(0.72, 0.28, 0.18),
            Vec2::splat(24.0),
            24.0,
            1,
            true,
            true,
            None,
            -10.0,
            true,
        ),

        PropKind::ToxicBarrel => (
            &[
                "images/sprToxicBarrel.png",
                "images/sprToxicBarrelHurt.png",
                "images/sprBarrel.png",
            ],
            Color::srgb(0.35, 0.86, 0.30),
            Vec2::splat(24.0),
            24.0,
            3,
            true,
            false,
            Some(PropDeathEffect::toxic_barrel()),
            -10.0,
            true,
        ),

        PropKind::Car => (
            &[
                "images/sprCarIdle.png",
                "images/sprCarHurt.png",
                "images/sprIcyCar.png",
            ],
            Color::srgb(0.62, 0.28, 0.22),
            Vec2::new(48.0, 28.0),
            38.0,
            10,
            true,
            false,
            Some(PropDeathEffect::car()),
            -10.0,
            true,
        ),

        PropKind::Pipe => (
            &["images/sprPipe.png"],
            Color::srgb(0.42, 0.46, 0.45),
            Vec2::splat(24.0),
            24.0,
            6,
            true,
            false,
            None,
            -10.0,
            true,
        ),

        PropKind::Tires => (
            &["images/sprTires.png"],
            Color::srgb(0.20, 0.20, 0.22),
            Vec2::splat(28.0),
            28.0,
            6,
            true,
            false,
            None,
            -10.0,
            true,
        ),

        PropKind::Cocoon => (
            &["images/sprCocoon.png", "images/sprCocoonHurt.png"],
            Color::srgb(0.70, 0.58, 0.72),
            Vec2::new(26.0, 32.0),
            24.0,
            7,
            true,
            false,
            None,
            -10.0,
            true,
        ),

        PropKind::Snowman => (
            &[
                "images/sprSnowManIdle.png",
                "images/sprSnowManHurt.png",
                "images/sprSnowMan.png",
            ],
            Color::srgb(0.90, 0.94, 1.0),
            Vec2::new(24.0, 32.0),
            24.0,
            5,
            true,
            false,
            None,
            -10.0,
            true,
        ),

        PropKind::Torch => (
            &["images/sprTorch.png", "images/sprTorchHurt.png"],
            Color::srgb(1.0, 0.62, 0.18),
            Vec2::new(12.0, 28.0),
            12.0,
            4,
            true,
            false,
            None,
            -10.0,
            true,
        ),

        PropKind::BigGenerator => {
            let hp = if run.loop_count == 0 {
                40
            } else {
                (18 - run.loop_count as i32 * 2).max(8)
            };
            (
                &["images/sprGenerator.png", "images/sprBigGenerator.png"],
                Color::srgb(0.55, 0.75, 1.0),
                Vec2::new(40.0, 48.0),
                40.0,
                hp,
                true,
                false,
                None,
                -8.0,
                true,
            )
        }
        PropKind::ThroneStatue => (
            &["images/sprThroneStatue.png"],
            Color::srgb(0.9, 0.82, 0.55),
            Vec2::splat(36.0),
            32.0,
            12,
            true,
            false,
            None,
            -8.0,
            true,
        ),

        PropKind::GroundDecal => (
            &[decal_png],
            Color::srgba(0.5, 0.5, 0.5, 0.5),
            Vec2::splat(32.0),
            0.0,
            9_999,
            false,
            false,
            None,
            -42.0,
            false,
        ),

        PropKind::Cobweb | PropKind::IcePatch | PropKind::FireTrap | PropKind::Mine => {
            unreachable!("functional props returned before ordinary prop tuple")
        }
    };

    let sprite = sprite_from_candidates(
        catalog,
        asset_server,
        candidates,
        fallback_color,
        fallback_size,
    );

    let mut entity = commands.spawn((
        GameCleanup,
        LevelCleanup,
        sprite,
        Transform::from_translation(pos.extend(z)),
    ));

    if solid {
        entity.insert(Prop {
            size: Vec2::splat(collision_size),
            hp,
            destructible,
            explosive: legacy_explosive,
        });
    }

    if let Some(effect) = death_effect {
        entity.insert(effect);
    }

    if kind == PropKind::Torch {
        entity.insert(SurfacePulse::hazard(pos.x * 0.01 + pos.y * 0.02));
    }
    if kind == PropKind::BigGenerator {
        entity.insert(BigGenerator { index: 0 });
    }
    if kind == PropKind::ThroneStatue {
        entity.insert(ThroneStatueProp {
            guardian_count: (1 + run.loop_count).min(6) as u8,
        });
    }
}

pub fn is_boss_floor(floor: u32) -> bool {
    is_boss_subarea(floor)
}

/// Loop-aware boss selection: floors 3/7/11 use stronger variants from
/// loop 1 onward; 15 stays Throne because the campfire/Throne II path owns
/// the loop gate.
pub fn boss_for_floor_and_loop(floor: u32, loop_count: u32) -> EnemyKind {
    let rf = ((floor.max(1) - 1) % 15) + 1;
    match rf {
        3 if loop_count > 0 => EnemyKind::BigBanditLoop,
        3 => EnemyKind::BigBandit,
        7 if loop_count > 0 => EnemyKind::BigDogLoop,
        7 => EnemyKind::BigDog,
        11 if loop_count > 0 => EnemyKind::LilHunterLoop,
        11 => EnemyKind::LilHunter,
        15 => EnemyKind::Throne,
        _ => EnemyKind::BigBandit,
    }
}

/// Upstream: Big Bandits on loop = loop_count * 2 (L1→2, L2→4, …). Pre-loop = 1.
pub fn big_bandit_count(loop_count: u32) -> u32 {
    if loop_count == 0 {
        1
    } else {
        loop_count.saturating_mul(2).max(2)
    }
}

/// Floor-only convenience wrapper (loop derived from global floor).
#[allow(dead_code)]
pub fn boss_for_floor(floor: u32) -> EnemyKind {
    boss_for_floor_and_loop(floor, (floor.max(1) - 1) / 15)
}

pub fn floor_in_world(floor: u32) -> u32 {
    let rf = ((floor.max(1) - 1) % 15) + 1;
    match rf {
        1..=3 => rf,
        4 => 1,
        5..=7 => rf - 4,
        8 => 1,
        9..=11 => rf - 8,
        12 => 1,
        13..=15 => rf - 12,
        _ => 1,
    }
}

pub fn world_of(floor: u32) -> u32 {
    let rf = ((floor.max(1) - 1) % 15) + 1;
    match rf {
        1..=3 => 1,
        4 => 2,
        5..=7 => 3,
        8 => 4,
        9..=11 => 5,
        12 => 6,
        13..=15 => 7,
        _ => 1,
    }
}

pub fn difficulty_multiplier(floor: u32) -> f32 {
    // Difficulty scales with loop (every 15 floors) plus intra-area progress.
    let loop_n = ((floor.max(1) - 1) / 15) as f32;
    let rf = ((floor.max(1) - 1) % 15) as f32;
    1.0 + loop_n * 0.45 + rf * 0.015
}

/// Clamp into the generated mask bounds (secondary safety net).
pub fn clamp_to_arena(pos: &mut Vec3, radius: f32) {
    pos.x = pos.x.clamp(-ARENA_W / 2.0 + radius, ARENA_W / 2.0 - radius);
    pos.y = pos.y.clamp(-ARENA_H / 2.0 + radius, ARENA_H / 2.0 - radius);
}

/// Convert a broken 16px wall lattice cell into walkable floor (32px owner).
pub fn floor_cell_for_wall(wx: i32, wy: i32) -> (i32, i32) {
    (wx.div_euclid(2), wy.div_euclid(2))
}

/// Make a broken wall's owner tile walkable in the floor mask.
pub fn expand_floor_for_wall(mask: &mut FloorMask, wx: i32, wy: i32) {
    mask.cells.insert(floor_cell_for_wall(wx, wy));
}

/// World position -> wall lattice cell.
pub fn wall_cell_at(pos: Vec2) -> (i32, i32) {
    (
        (pos.x / WALL_PX).floor() as i32,
        (pos.y / WALL_PX).floor() as i32,
    )
}

/// Generic over the caller's prop-query filter so systems that must be
/// statically disjoint from an `&mut Transform` enemy/player query (boss AI,
/// etc.) can pass extra `Without<T>` filters.
pub fn resolve_prop_collision<F>(
    pos: &mut Vec3,
    radius: f32,
    props: &Query<(Entity, &Prop, &Transform), F>,
) where
    F: QueryFilter,
{
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
    props: &Query<(Entity, &mut Prop, &Transform, Option<&PropDeathEffect>), With<Prop>>,
) -> bool {
    for (_, prop, tf, _) in props.iter() {
        if !prop.destructible && prop.hp >= 9999 {
            // Indestructible decor still blocks bullets.
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::areas::AreaId;
    use crate::game::components::ThroneRoomState;

    fn run_for(floor: u32) -> Run {
        let loop_count = (floor.max(1) - 1) / 15;
        Run {
            floor,
            world: world_of(floor),
            area: AreaId::from_route_floor(floor),
            loop_count,
            floor_in_area: floor_in_world(floor),
            gen_seed: 0xDEAD_BEEF,
            portal_open: false,
            game_over: false,
            total_kills: 0,
        }
    }

    #[test]
    fn hq_area_exists_for_spawns() {
        assert_eq!(AreaId::HQ as u8, 11);
    }

    #[test]
    fn boss_mapping_still_ends_normal_cycle_at_throne() {
        assert_eq!(boss_for_floor(15), EnemyKind::Throne);
        assert_eq!(boss_for_floor(30), EnemyKind::Throne);
    }

    #[test]
    fn bosses_map_to_area_ends() {
        // First cycle uses base bosses.
        assert_eq!(boss_for_floor_and_loop(3, 0), EnemyKind::BigBandit);
        assert_eq!(boss_for_floor_and_loop(7, 0), EnemyKind::BigDog);
        assert_eq!(boss_for_floor_and_loop(11, 0), EnemyKind::LilHunter);
        assert_eq!(boss_for_floor_and_loop(15, 0), EnemyKind::Throne);
    }

    #[test]
    fn loop_cycle_bosses_use_loop_variants() {
        assert_eq!(boss_for_floor_and_loop(18, 1), EnemyKind::BigBanditLoop);
        assert_eq!(boss_for_floor_and_loop(22, 1), EnemyKind::BigDogLoop);
        assert_eq!(boss_for_floor_and_loop(26, 1), EnemyKind::LilHunterLoop);
        assert_eq!(boss_for_floor_and_loop(30, 1), EnemyKind::Throne);

        assert_eq!(boss_for_floor_and_loop(33, 2), EnemyKind::BigBanditLoop);
        assert_eq!(boss_for_floor_and_loop(37, 2), EnemyKind::BigDogLoop);
        assert_eq!(boss_for_floor_and_loop(45, 2), EnemyKind::Throne);
    }

    #[test]
    fn floor_derived_boss_selection_stays_consistent() {
        // boss_for_floor derives loop from global floor.
        assert_eq!(boss_for_floor(3), EnemyKind::BigBandit);
        assert_eq!(boss_for_floor(18), EnemyKind::BigBanditLoop);
        assert_eq!(boss_for_floor(22), EnemyKind::BigDogLoop);
        assert_eq!(boss_for_floor(26), EnemyKind::LilHunterLoop);
        assert_eq!(boss_for_floor(30), EnemyKind::Throne);
    }

    #[test]
    fn loop_sewers_and_labs_get_exclusive_bosses() {
        let mut run = run_for(4); // Sewers 2-1
        run.loop_count = 1;
        let plan = generate_level(&run);
        assert_eq!(plan.boss, Some(EnemyKind::Mom));

        let mut run = run_for(12); // Labs 6-1
        run.loop_count = 1;
        let plan = generate_level(&run);
        assert_eq!(plan.boss, Some(EnemyKind::Technomancer));
    }

    #[test]
    fn boss_floors_except_palace_still_get_trash() {
        for floor in [3_u32, 7, 11] {
            let run = run_for(floor);
            let plan = generate_level(&run);
            assert!(
                !plan.enemies.is_empty(),
                "floor {floor}: boss floor should have trash mobs"
            );
        }
    }

    #[test]
    fn secret_area_uses_own_gml_area() {
        let mut run = run_for(4);
        run.area = AreaId::Oasis;
        assert_eq!(gml_area_from_run(&run), 1);

        let mut run = run_for(4);
        run.area = AreaId::HQ;
        assert_eq!(gml_area_from_run(&run), 6);

        let mut run = run_for(9);
        run.area = AreaId::Jungle;
        assert_eq!(gml_area_from_run(&run), 5);
    }

    #[test]
    fn crown_vault_gets_guardian_boss() {
        let mut run = run_for(2);
        run.area = crate::game::areas::AreaId::CrownVault;
        let plan = generate_level(&run);
        assert_eq!(plan.boss, Some(EnemyKind::OldGuardian));
    }

    #[test]
    fn loop_bandit_counts() {
        assert_eq!(big_bandit_count(0), 1);
        assert_eq!(big_bandit_count(1), 2);
        assert_eq!(big_bandit_count(2), 4);
        assert_eq!(big_bandit_count(3), 6);
    }

    #[test]
    fn throne_room_loop_requires_generators() {
        let mut s = ThroneRoomState::default();
        assert!(!s.loop_eligible);
        for _ in 0..4 {
            s.note_generator_destroyed();
        }
        assert!(s.loop_eligible);
        assert!(s.all_generators_down);
    }

    #[test]
    fn crystal_caves_spawn_the_real_roster() {
        let mut spiders = false;
        let mut crystals = false;
        for seed in 0..64u64 {
            let mut run = run_for(8); // Crystal Caves
            run.gen_seed = seed;
            let plan = generate_level(&run);
            for (kind, _) in &plan.enemies {
                if *kind == EnemyKind::Spider {
                    spiders = true;
                }
                if matches!(*kind, EnemyKind::Crystal | EnemyKind::LaserCrystal) {
                    crystals = true;
                }
            }
        }
        assert!(spiders, "no spiders across 64 caves seeds");
        assert!(crystals, "no crystals across 64 caves seeds");
    }

    #[test]
    fn vault_goal_is_tighter_than_route_floors() {
        let mut run = run_for(8);
        run.area = crate::game::areas::AreaId::Vault;
        assert_eq!(generation_goal_for_run(&run), 40);
        assert_eq!(generation_goal_for_run(&run_for(5)), 110);
    }

    #[test]
    fn level_generation_produces_consistent_levels() {
        for floor in 1..=15u32 {
            let plan = generate_level(&run_for(floor));
            assert!(!plan.floor_cells.is_empty(), "floor {floor}: no tiles");
            assert!(!plan.wall_cells.is_empty(), "floor {floor}: no walls");
            if !is_boss_floor(floor) {
                assert!(
                    !plan.enemies.is_empty(),
                    "floor {floor}: expected regular enemies"
                );
            } else {
                assert!(plan.boss.is_some(), "floor {floor}: expected boss");
            }
            // Walls never overlap floors, and every tile's 12-cell ring is
            // sealed (no dark gaps between wall masses and floors).
            const RING: [(i32, i32); 12] = [
                (-1, -1),
                (0, -1),
                (1, -1),
                (2, -1), //
                (2, 0),
                (2, 1), //
                (-1, 0),
                (-1, 1), //
                (-1, 2),
                (0, 2),
                (1, 2),
                (2, 2),
            ];
            let walls_and_smalls = |wx: i32, wy: i32| {
                plan.wall_cells.contains(&(wx, wy))
                    || plan
                        .small_walls
                        .iter()
                        .any(|&(sx, sy)| sx as i32 == wx && sy as i32 == wy)
            };
            for &(cx, cy) in &plan.floor_cells {
                for ox in 0..2 {
                    for oy in 0..2 {
                        assert!(
                            !plan.wall_cells.contains(&(cx * 2 + ox, cy * 2 + oy)),
                            "floor {floor}: wall inside floor tile ({cx},{cy})"
                        );
                    }
                }
                for (ox, oy) in RING {
                    let (wx, wy) = (cx * 2 + ox, cy * 2 + oy);
                    let owner = (wx.div_euclid(2), wy.div_euclid(2));
                    assert!(
                        plan.floor_cells.contains(&owner) || walls_and_smalls(wx, wy),
                        "floor {floor}: unsealed ring cell ({wx},{wy}) next to ({cx},{cy})"
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod loop_boss_spawn_tests {
    use super::*;

    #[test]
    fn loop_substitutions_do_nothing_on_loop_zero() {
        let mut table = vec![(EnemyKind::Bandit, 10)];
        apply_loop_elite_substitutions(&mut table, AreaId::Desert, 0);
        assert_eq!(table, vec![(EnemyKind::Bandit, 10)]);
    }

    #[test]
    fn desert_loop_gets_pressure_units() {
        let mut table = vec![(EnemyKind::Bandit, 10)];
        apply_loop_elite_substitutions(&mut table, AreaId::Desert, 1);
        assert!(table.iter().any(|(k, _)| *k == EnemyKind::SnowBandit));
        assert!(table.iter().any(|(k, _)| *k == EnemyKind::IdpdGrunt));
    }

    #[test]
    fn caves_loop_two_gets_idpd_elite() {
        let mut table = vec![(EnemyKind::Scorpion, 10)];
        apply_loop_elite_substitutions(&mut table, AreaId::CrystalCaves, 2);
        assert!(table.iter().any(|(k, _)| *k == EnemyKind::IdpdElite));
    }

    #[test]
    fn hyper_applies_to_looped_crystal_caves_only() {
        // gml_area(floor)==4 corresponds to route floor 8 (+15 per loop).
        assert_eq!(gml_area(8), 4);
        assert_eq!(gml_area(23), 4);
        assert_ne!(gml_area(7), 4);
        assert_ne!(gml_area(9), 4);
    }
}

#[cfg(test)]
mod environment_gen_tests {
    use super::*;

    fn run_for_floor(floor: u32) -> Run {
        let loop_count = (floor.max(1) - 1) / 15;
        Run {
            floor,
            world: world_of(floor),
            area: AreaId::from_route_floor(floor),
            loop_count,
            floor_in_area: floor_in_world(floor),
            ..Default::default()
        }
    }

    #[test]
    fn functional_environment_kinds_are_non_claiming() {
        // These never claim prop_tiles, so enemies/chests can share the cell.
        for kind in [
            PropKind::Cobweb,
            PropKind::IcePatch,
            PropKind::FireTrap,
            PropKind::GroundDecal,
        ] {
            let claims = !matches!(
                kind,
                PropKind::GroundDecal | PropKind::Cobweb | PropKind::IcePatch | PropKind::FireTrap
            );
            assert!(!claims, "{kind:?} should not claim tiles");
        }
    }

    #[test]
    fn crystal_caves_generate_cave_props() {
        let mut found = false;
        for seed in 0..64u64 {
            let mut run = run_for_floor(8);
            run.gen_seed = seed;
            let plan = generate_level(&run);
            found |= plan
                .props
                .iter()
                .any(|(kind, _)| matches!(kind, PropKind::Cobweb | PropKind::Cocoon));
        }
        assert!(found, "no cobwebs/cocoons generated across 64 seeds");
    }

    #[test]
    fn frozen_city_generates_ice() {
        let mut found = false;
        for seed in 0..64u64 {
            let mut run = run_for_floor(9);
            run.gen_seed = seed;
            let plan = generate_level(&run);
            found |= plan
                .props
                .iter()
                .any(|(kind, _)| *kind == PropKind::IcePatch);
        }
        assert!(found, "no ice patches generated across 64 seeds");
    }

    #[test]
    fn labs_generate_toxic_barrels() {
        let mut found = false;
        for seed in 0..96u64 {
            let mut run = run_for_floor(12);
            run.gen_seed = seed;
            let plan = generate_level(&run);
            found |= plan
                .props
                .iter()
                .any(|(kind, _)| *kind == PropKind::ToxicBarrel);
        }
        assert!(found, "no toxic barrels generated across 96 seeds");
    }

    #[test]
    fn palace_generates_fire_traps_or_mines() {
        let mut found = false;
        for seed in 0..96u64 {
            let mut run = run_for_floor(13);
            run.gen_seed = seed;
            let plan = generate_level(&run);
            found |= plan
                .props
                .iter()
                .any(|(kind, _)| matches!(kind, PropKind::FireTrap | PropKind::Mine));
        }
        assert!(found, "no traps/mines generated across 96 seeds");
    }

    #[test]
    fn generated_props_sit_on_floor_cells() {
        for floor in [4_u32, 5, 8, 9, 12, 13] {
            let run = run_for_floor(floor);
            let plan = generate_level(&run);

            let floors: std::collections::HashSet<_> = plan
                .floor_cells
                .iter()
                .map(|&(x, y)| cell_center_px(x, y))
                .map(|p| (p.x.round() as i32, p.y.round() as i32))
                .collect();

            for (_, pos) in &plan.props {
                let key = (pos.x.round() as i32, pos.y.round() as i32);
                assert!(
                    floors.contains(&key),
                    "floor {floor}: prop {pos:?} not on a floor cell"
                );
            }
        }
    }
}
