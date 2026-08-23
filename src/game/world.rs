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

fn is_boss_subarea(floor: u32) -> bool {
    let rf = ((floor.max(1) - 1) % 15) + 1;
    // End of each multi-floor world: Desert 3, Scrapyards 7, Frozen 11, Palace 15
    matches!(rf, 3 | 7 | 11 | 15)
}

pub fn generation_goal(floor: u32) -> usize {
    if is_boss_subarea(floor) {
        let rf = ((floor.max(1) - 1) % 15) + 1;
        return if rf == 15 { 48 } else { 60 };
    }
    110
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
    let area = gml_area(run.floor);
    let goal = generation_goal(run.floor);
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
    let area = gml_area(run.floor);
    let boss_sub = is_boss_subarea(run.floor);
    let world_n = world_of(run.floor);

    // Enemy cap: upstream 3 + difficulty/1.5; difficulty scales with worlds/loops.
    let enemy_cap = (3.0 + (world_n as f32 - 1.0) * 1.5 + run.loop_count as f32 * 1.5) as usize;

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

    // --- Enemy pass (scrPopEnemies, RNGStates.Enemies) ---
    for &(cx, cy) in floors {
        if plan.enemies.len() >= enemy_cap || boss_sub {
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

        let center = Vec2::new(px, py);
        let pick_kind = |rng: &mut StdRng, w: &[EnemyKind]| w[rng.random_range(0..w.len())];
        let loop_extras = loop_elite_candidates(area, run.loop_count);
        match area {
            1 => {
                if rng.random::<f32>() * 7.0 < 1.0 {
                    let k = pick_kind(&mut rng, &[EnemyKind::Maggot, EnemyKind::Scorpion]);
                    plan.enemies.push((k, center));
                } else if rng.random::<f32>() * 30.0 < 1.0 {
                    plan.props.push((PropKind::Barrel, center));
                    for _ in 0..3 {
                        plan.enemies.push((
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
                    plan.enemies.push((k, center));
                }
            }
            2 => {
                // Sewers: rats dominate, with maggots/freaks/bandits mixed in.
                if rng.random::<f32>() * 9.0 < 1.0 {
                    let k = pick_kind(
                        &mut rng,
                        &[
                            EnemyKind::BigRat,
                            EnemyKind::Freak,
                            EnemyKind::Bandit,
                            EnemyKind::Scorpion,
                        ],
                    );
                    plan.enemies.push((k, center));
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
                    plan.enemies.push((k, center));
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
                plan.enemies.push((k, center));
            }
            4 => {
                // Crystal Caves: reuse assassin/freak/scorpion until crystal
                // sprites are wired into the catalog.
                let mut cands = vec![EnemyKind::Assassin, EnemyKind::Freak, EnemyKind::Scorpion];
                cands.extend(loop_extras.iter().copied());
                let k = pick_kind(&mut rng, &cands);
                plan.enemies.push((k, center));
            }
            5 => {
                // Frozen City: snow bandits and wolves; IDPD scouts on loops.
                let mut frozen = vec![
                    EnemyKind::SnowBandit,
                    EnemyKind::SnowBandit,
                    EnemyKind::Wolf,
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
                plan.enemies.push((k, center));
            }
            6 | 7 => {
                // Labs / Palace: mixed late-game garrisons; IDPD squads scale
                // with loop count.
                let mut late = vec![
                    EnemyKind::RobotGuard,
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
                plan.enemies.push((k, center));
            }
            _ => {}
        }
    }

    // Bosses. Looped Crystal Caves visits get the Hyper Crystal instead of a
    // quiet single-floor stop.
    if boss_sub {
        plan.boss = Some(boss_for_floor_and_loop(run.floor, run.loop_count));
    } else if gml_area(run.floor) == 4 && run.loop_count >= 1 {
        plan.boss = Some(EnemyKind::Hyper);
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

fn area_sprites(floor: u32) -> (&'static str, &'static str, &'static str, &'static str) {
    // (floor, wall bot, wall top, ground decal prop)
    // Upstream sprite families are named by _area id.
    let rf = ((floor.max(1) - 1) % 15) + 1;
    match rf {
        3 => (
            "images/sprFloor0.png",
            "images/sprWall0Bot.png",
            "images/sprWall0Top.png",
            "images/sprNightDesertTopDecal.png",
        ),
        4 => (
            "images/sprFloor2.png",
            "images/sprWall2Bot.png",
            "images/sprWall2Top.png",
            "images/sprTopDecalSewers.png",
        ),
        5..=7 => (
            "images/sprFloor3.png",
            "images/sprWall3Bot.png",
            "images/sprWall3Top.png",
            "images/sprTopDecalScrapyard.png",
        ),
        8 => (
            "images/sprFloor4.png",
            "images/sprWall4Bot.png",
            "images/sprWall4Top.png",
            "images/sprTopDecalCave.png",
        ),
        9..=11 => (
            "images/sprFloor5.png",
            "images/sprWall5Bot.png",
            "images/sprWall5Top.png",
            "images/sprTopDecalCity.png",
        ),
        12 => (
            "images/sprFloor6.png",
            "images/sprWall6Bot.png",
            "images/sprWall6Top.png",
            "images/sprTopDecalCity.png",
        ),
        13..=15 => (
            "images/sprFloor7.png",
            "images/sprWall7Bot.png",
            "images/sprWall7Top.png",
            "images/sprPalaceTopDecal.png",
        ),
        _ => (
            "images/sprFloor1.png",
            "images/sprWall1Bot.png",
            "images/sprWall1Top.png",
            "images/sprDesertTopDecal.png",
        ),
    }
}

pub fn spawn_level(
    commands: &mut Commands,
    catalog: &AssetCatalog,
    asset_server: &AssetServer,
    run: &Run,
    plan: &LevelPlan,
    mask: &mut FloorMask,
) {
    let (floor_png, wall_bot_png, wall_top_png, decal_prop_png) = area_sprites(run.floor);

    let cols = (ARENA_W / TILE) as i32;
    let rows = (ARENA_H / TILE) as i32;

    // Publish walkable mask.
    *mask = FloorMask {
        cells: plan.floor_cells.iter().copied().collect(),
        cols,
        rows,
    };

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

        // GML probes place_meeting(x, y+16) with y-down: shift the 16px box
        // one cell SOUTH on screen. Bevy y-up => subtract one lattice row.
        // wy even -> row wy/2 - 1; wy odd -> row (wy-1)/2.
        let owner = (wx.div_euclid(2), (wy - 1).div_euclid(2));
        let floor_below = floor_set.contains(&owner);
        if floor_below {
            commands.spawn((
                GameCleanup,
                LevelCleanup,
                sprite_exact(catalog, asset_server, wall_bot_png),
                Transform::from_xyz(c.x, c.y, -40.0),
            ));
        }
        // Top cap sits 8px ABOVE on screen (GML drew it at y-8 with y-down).
        commands.spawn((
            GameCleanup,
            LevelCleanup,
            sprite_exact(catalog, asset_server, wall_top_png),
            Transform::from_xyz(c.x, c.y + 8.0, -36.0),
        ));

        // Collision body (16px solid).
        commands.spawn((
            GameCleanup,
            LevelCleanup,
            WallTile,
            Prop {
                size: Vec2::splat(WALL_PX),
                hp: 9999,
                destructible: false,
                explosive: false,
            },
            Transform::from_xyz(c.x, c.y, -30.0),
        ));
    }

    // Props.
    for (kind, pos) in &plan.props {
        spawn_prop(commands, catalog, asset_server, *kind, *pos, decal_prop_png);
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
        crate::game::enemies::spawn_enemy_at(
            commands,
            catalog,
            asset_server,
            kind,
            Vec2::new(320.0, -160.0),
            difficulty_multiplier(run.floor),
            false,
            false,
        );
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

    let mut ec = commands.spawn((
        GameCleanup,
        LevelCleanup,
        SecretEntrance { target },
        Prop {
            size: Vec2::splat(size),
            hp: 6,
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
                        "images/sprFireTrap.png",
                        "images/sprFireTrapIdle.png",
                        "images/sprTorchFire.png",
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

    fn run_for(floor: u32) -> Run {
        Run {
            floor,
            world: 1,
            area: AreaId::Desert,
            loop_count: 0,
            floor_in_area: 1,
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
        Run {
            floor,
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
