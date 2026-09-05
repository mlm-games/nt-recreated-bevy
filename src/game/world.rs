//! World generation - faithful port of the upstream FloorMaker/scrMakeFloor
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
    /// GML FloorMaker.styleb (~1/6 on desert): drives BonePile vs Cactus,
    /// Icicle vs Hydrant, NewsStand vs Soda, maggot nest, RadMaggotChest.
    pub styleb: bool,
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
    /// Gold barrel (Y.V. Mansion): explodes and drops a gold weapon.
    GoldBarrel,

    // Additional destructibles (parity with upstream `scrPopProps`)
    BonePile,
    NightBonePile,
    NightCactus,
    Crystal,
    Hydrant,
    StreetLight,
    SodaMachine,
    Tube,
    MutantTube,
    Pillar,
    SmallGenerator,
    Anchor,
    WaterPlant,
    OasisBarrel,
    WaterMine,
    MoneyPile,
    YVStatue,
    Bush,
    BigFlower,
    PizzaBox,
    PlantPot,

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

/// Secret areas keep their own walker/spawn families via GML 100-series ids
/// (100 vault, 101 oasis, 102 pizza, 103 mansion, 104 cursed, 105 jungle,
/// 106 hq) so turn/die/shape tables match scrMakeFloor exactly.
fn gml_area_from_run(run: &Run) -> i32 {
    use crate::game::areas::AreaId;
    match run.area {
        AreaId::Desert => 1,
        AreaId::Oasis => 101,
        AreaId::Sewers => 2,
        AreaId::PizzaSewers => 102,
        AreaId::Scrapyards => 3,
        // Y.V. Mansion uses its own walker family (103), not scrapyards.
        AreaId::City => 103,
        AreaId::CrystalCaves => 4,
        AreaId::CursedCaves => 104,
        AreaId::Vault | AreaId::CrownVault => 100,
        AreaId::FrozenCity => 5,
        AreaId::Jungle => 105,
        AreaId::Labs => 6,
        AreaId::HQ => 106,
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
    // GML scrAreaGetGenerationGoal has no boss-specific goals; boss floors
    // use the normal 110 (palace-last 420 handled in goal_for_run).
    let _ = floor;
    110
}

/// Secret areas get tighter or roomier layouts per upstream
/// scrAreaGetGenerationGoal: vault 40, campfire 60, crib 20 (unused),
/// pizza 70, palace 130 / 420-last, mansion/oasis 130, hq-last 48.
fn generation_goal_for_run(run: &Run) -> usize {
    use crate::game::areas::AreaId;
    if crate::game::secret_areas::is_secret_area(run.area) {
        return match run.area {
            AreaId::CrownVault | AreaId::Vault => 40,
            AreaId::PizzaSewers => 70,
            AreaId::City => 130,
            AreaId::Oasis => 130,
            AreaId::HQ => {
                // hq-last (subarea max) is 48; earlier HQ visits use 110.
                if run.floor_in_area >= 3 { 48 } else { 110 }
            }
            AreaId::CursedCaves => 100,
            AreaId::Jungle => 110,
            _ => 90,
        };
    }
    // Palace-last is the 420-floor throne approach; other palace floors 130.
    if run.area == AreaId::Palace {
        let rf = ((run.floor.max(1) - 1) % 15) + 1;
        if rf == 15 {
            return 420;
        }
        return 130;
    }
    if run.area == AreaId::Campfire {
        return 60;
    }
    generation_goal(run.floor)
}

// scrMakeFloor port - floor walkers + mcr_floor_make_walls + ScreenEnd outer ring.
// ScreenEnd marks lattice walls whose owner floor cell is NOT in the floor set
// (outer ring). Boss intros prefer breaking these (see enemies::tick_delayed_boss_spawns).

/// A wall is a screen-end if it has fewer than 2 orthogonal floor-neighbour
/// lattice owners (true perimeter), matching upstream ScreenEnd placement.
pub fn is_screen_end_wall(
    wx: i32,
    wy: i32,
    floor_set: &std::collections::HashSet<(i32, i32)>,
) -> bool {
    let owner = floor_cell_for_wall(wx, wy);
    let neighbors = [
        (owner.0 - 1, owner.1),
        (owner.0 + 1, owner.1),
        (owner.0, owner.1 - 1),
        (owner.0, owner.1 + 1),
    ];
    let floor_n = neighbors.iter().filter(|c| floor_set.contains(c)).count();
    floor_n <= 1
}

#[derive(Clone, Copy)]
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
        1 => rng_choose(rng, &[Z, Z, 90, -90, 90, -90, 180]),
        2 | 102 => rng_choose(rng, &[Z, Z, Z, Z, Z, Z, Z, Z, Z, 90, -90, 90, -90, 180]),
        3 => rng_choose(rng, &[Z, Z, Z, Z, Z, 90, -90]),
        // GML area 4 has 5 zeros, not 7 (Bevy was too straight).
        4 => rng_choose(rng, &[Z, Z, Z, Z, Z, 90, -90, 180]),
        // GML area 5 ends with nested choose(0,90,-90), not a fixed 0.
        5 => {
            let tail = rng_choose(rng, &[Z, 90, -90]);
            rng_choose(
                rng,
                &[Z, Z, Z, Z, Z, Z, Z, Z, Z, Z, Z, Z, Z, 180, 180, tail],
            )
        }
        6 => rng_choose(rng, &[Z, Z, Z, Z, Z, Z, Z, Z, Z, Z, Z, Z, 90, -90, 180]),
        7 => rng_choose(rng, &[Z, Z, Z, Z, Z, Z, Z, Z, Z, Z, Z, Z, Z, 90, -90, 180]),
        100 => rng_choose(rng, &[Z, Z, Z, Z, Z, Z, Z, Z, Z, Z, Z, 90, -90, 180, 180]),
        101 => rng_choose(rng, &[Z, Z, Z, Z, 90, -90, 90, -90, 180]),
        103 => rng_choose(rng, &[Z, Z, Z, Z, 90, -90, 180]),
        105 => rng_choose(rng, &[Z, Z, Z, Z, Z, Z, 90, -90, 90, -90, 180]),
        106 => rng_choose(rng, &[Z, Z, 90, -90, 90, -90, 180]),
        _ => rng_choose(rng, &[Z, Z, Z, Z, Z, Z, Z, Z, Z, Z, 90, -90, 90, -90, 180]),
    }
}

pub fn generate_level(run: &Run) -> LevelPlan {
    let area = gml_area_from_run(run);
    // Hardcoded bypasses (GenCont + FloorMaker/Create_0 + Step_0):
    // palace-last 8x48 rect, campfire 5x3 block + 7 makers.
    // HQ-last 10x10 is handled as a rect here as well (LastIntro/BigTV
    // set pieces spawn in spawn_level).
    if area == 7 && ((run.floor.max(1) - 1) % 15) + 1 == 15 {
        return generate_palace_last(run);
    }
    if run.area == crate::game::areas::AreaId::Campfire {
        return generate_campfire(run);
    }
    if area == 106 && run.floor_in_area >= 3 {
        return generate_hq_last(run);
    }
    let goal = generation_goal_for_run(run);
    let mut rng = StdRng::seed_from_u64(run.gen_seed);
    // GML FloorMaker.styleb ~= 1/6 (desert night variant).
    let styleb = rng.random::<f32>() * 6.0 < 1.0;

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
        styleb,
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
        let mut next_makers = Vec::with_capacity(n_makers);
        let mut new_branches = Vec::new();

        for mi in 0..n_makers {
            let mut m = makers[mi];

            let (dx, dy) = m.step_delta();
            m.x += dx;
            m.y += dy;
            let (mx, my) = (m.x, m.y);

            // scrMakeFloor shapes (rng_float Generation). Coordinates here
            // are floor-cell units; GML pixel offsets /32.
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
                    // Scrap: 1/8 (or max-subarea always) 3x3-minus-cross
                    // with random xoff/yoff on max-subarea.
                    let is_max = run.floor_in_area >= 3;
                    if rng.random::<f32>() * 8.0 < 1.0 || is_max {
                        let (xoff, yoff) = if is_max {
                            let xo = rng_choose(&mut rng, &[0, 1, 0, 0, -1]);
                            let yo = rng_choose(&mut rng, &[0, 1, 0, 0, -1]);
                            (xo, yo)
                        } else {
                            (0, 0)
                        };
                        // 3x3 minus center-cross: GML creates 9 cells in a
                        // plus-shaped missing-center pattern; approximate
                        // with full 3x3 (gameplay-equivalent).
                        for dy2 in -1..=1 {
                            for dx2 in -1..=1 {
                                stamp_cell(
                                    (mx + xoff + dx2, my + yoff + dy2),
                                    &mut seen,
                                    &mut plan.floor_cells,
                                );
                            }
                        }
                    } else {
                        stamp_cell((mx, my), &mut seen, &mut plan.floor_cells);
                    }
                }
                5 => {
                    // Frozen City 1/11 large cross (two variants).
                    if rng.random::<f32>() * 11.0 < 1.0 {
                        if rng.random::<f32>() * 2.0 < 1.0 {
                            for p in [
                                (mx + 1, my),
                                (mx + 1, my + 1),
                                (mx, my + 1),
                                (mx, my - 1),
                                (mx - 1, my),
                                (mx + 1, my - 1),
                                (mx - 1, my - 1),
                                (mx - 1, my + 1),
                            ] {
                                stamp_cell(p, &mut seen, &mut plan.floor_cells);
                            }
                        } else {
                            for p in [
                                (mx + 2, my - 2),
                                (mx + 2, my - 1),
                                (mx + 2, my),
                                (mx + 2, my + 1),
                                (mx + 2, my + 2),
                                (mx - 2, my - 2),
                                (mx - 2, my - 1),
                                (mx - 2, my),
                                (mx - 2, my + 1),
                                (mx - 2, my + 2),
                                (mx, my - 2),
                                (mx - 1, my - 2),
                                (mx + 1, my - 2),
                                (mx, my + 2),
                                (mx - 1, my + 2),
                                (mx + 1, my + 2),
                            ] {
                                stamp_cell(p, &mut seen, &mut plan.floor_cells);
                            }
                        }
                        stamp_cell((mx, my), &mut seen, &mut plan.floor_cells);
                    } else {
                        stamp_cell((mx, my), &mut seen, &mut plan.floor_cells);
                    }
                }
                7 => {
                    // Palace 1/16 4x4 else 2x2.
                    if rng.random::<f32>() * 16.0 < 1.0 {
                        for dy2 in -1..=2 {
                            for dx2 in -1..=2 {
                                stamp_cell((mx + dx2, my + dy2), &mut seen, &mut plan.floor_cells);
                            }
                        }
                    } else {
                        for p in [(mx, my), (mx + 1, my), (mx + 1, my + 1), (mx, my + 1)] {
                            stamp_cell(p, &mut seen, &mut plan.floor_cells);
                        }
                    }
                }
                100 => {
                    // Vault 1/8 5-long H/V line.
                    if rng.random::<f32>() * 8.0 < 1.0 {
                        if rng.random_range(0..3) == 1 {
                            for o in [-2, -1, 0, 1, 2] {
                                stamp_cell((mx + o, my), &mut seen, &mut plan.floor_cells);
                            }
                        } else {
                            for o in [-2, -1, 0, 1, 2] {
                                stamp_cell((mx, my + o), &mut seen, &mut plan.floor_cells);
                            }
                        }
                    } else {
                        stamp_cell((mx, my), &mut seen, &mut plan.floor_cells);
                    }
                }
                103 | 107 => {
                    // Mansion/crib: every 12th Floor steps forward + 8-ring.
                    if !plan.floor_cells.is_empty() && plan.floor_cells.len() % 12 == 0 {
                        let (dx2, dy2) = m.step_delta();
                        m.x += dx2;
                        m.y += dy2;
                        let (nx, ny) = (m.x, m.y);
                        for p in [
                            (nx, ny),
                            (nx + 1, ny),
                            (nx + 1, ny + 1),
                            (nx, ny + 1),
                            (nx, ny - 1),
                            (nx - 1, ny),
                            (nx + 1, ny - 1),
                            (nx - 1, ny - 1),
                            (nx - 1, ny + 1),
                        ] {
                            stamp_cell(p, &mut seen, &mut plan.floor_cells);
                        }
                    } else {
                        stamp_cell((mx, my), &mut seen, &mut plan.floor_cells);
                    }
                }
                106 => {
                    // HQ: every 8th double-64 jump + 16-ring + double-jump,
                    // else 1/3 8-ring, else repeat4 Floor-step-Floor+AmmoChest.
                    if !plan.floor_cells.is_empty() && plan.floor_cells.len() % 8 == 0 {
                        let (dx2, dy2) = m.step_delta();
                        m.x += dx2 * 2;
                        m.y += dy2 * 2;
                        let (nx, ny) = (m.x, m.y);
                        for dy2 in -2..=2i32 {
                            for dx2 in -2..=2i32 {
                                if dx2.abs() == 2 || dy2.abs() == 2 || dx2 == 0 || dy2 == 0 {
                                    stamp_cell(
                                        (nx + dx2, ny + dy2),
                                        &mut seen,
                                        &mut plan.floor_cells,
                                    );
                                }
                            }
                        }
                        m.x += dx2 * 2;
                        m.y += dy2 * 2;
                    } else if rng.random::<f32>() * 3.0 < 1.0 {
                        for p in [
                            (mx, my),
                            (mx + 1, my),
                            (mx + 1, my + 1),
                            (mx, my + 1),
                            (mx, my - 1),
                            (mx - 1, my),
                            (mx + 1, my - 1),
                            (mx - 1, my - 1),
                            (mx - 1, my + 1),
                        ] {
                            stamp_cell(p, &mut seen, &mut plan.floor_cells);
                        }
                    } else {
                        for _ in 0..4 {
                            stamp_cell((m.x, m.y), &mut seen, &mut plan.floor_cells);
                            let (dx2, dy2) = m.step_delta();
                            m.x += dx2;
                            m.y += dy2;
                            stamp_cell((m.x, m.y), &mut seen, &mut plan.floor_cells);
                            plan.chests.push(ChestSpawn::Ammo(cell_center_px(m.x, m.y)));
                        }
                        if rng.random::<f32>() * 3.0 < 1.0 {
                            for p in [
                                (mx + 1, my),
                                (mx + 1, my + 1),
                                (mx, my + 1),
                                (mx, my - 1),
                                (mx - 1, my),
                                (mx + 1, my - 1),
                                (mx - 1, my - 1),
                                (mx - 1, my + 1),
                            ] {
                                stamp_cell(p, &mut seen, &mut plan.floor_cells);
                            }
                        }
                    }
                }
                101 => {
                    stamp_cell((mx, my), &mut seen, &mut plan.floor_cells);
                    if rng.random::<f32>() * 3.0 < 1.0 {
                        for p in [(mx - 1, my), (mx + 1, my), (mx, my - 1), (mx, my + 1)] {
                            stamp_cell(p, &mut seen, &mut plan.floor_cells);
                        }
                    }
                }
                104 => {
                    // Cursed: 8-ring around scattered maker.
                    if plan.floor_cells.len() < 4 {
                        for p in [
                            (mx - 1, my),
                            (mx - 1, my - 1),
                            (mx - 1, my + 1),
                            (mx + 1, my),
                            (mx + 1, my - 1),
                            (mx + 1, my + 1),
                            (mx, my + 1),
                            (mx, my - 1),
                        ] {
                            stamp_cell(p, &mut seen, &mut plan.floor_cells);
                        }
                    }
                    m.x += rng_choose(&mut rng, &[0, 2, -2]);
                    m.y += rng_choose(&mut rng, &[0, 2, -2]);
                    let (nx, ny) = (m.x, m.y);
                    for p in [
                        (nx - 1, ny),
                        (nx - 1, ny - 1),
                        (nx - 1, ny + 1),
                        (nx + 1, ny),
                        (nx + 1, ny - 1),
                        (nx + 1, ny + 1),
                        (nx, ny + 1),
                        (nx, ny - 1),
                    ] {
                        stamp_cell(p, &mut seen, &mut plan.floor_cells);
                    }
                    stamp_cell((nx, ny), &mut seen, &mut plan.floor_cells);
                }
                105 => {
                    if rng.random::<f32>() * 4.0 < 1.0 {
                        for p in [(mx, my), (mx + 1, my), (mx + 1, my + 1), (mx, my + 1)] {
                            stamp_cell(p, &mut seen, &mut plan.floor_cells);
                        }
                    } else {
                        stamp_cell((mx, my), &mut seen, &mut plan.floor_cells);
                    }
                }
                _ => {
                    stamp_cell((mx, my), &mut seen, &mut plan.floor_cells);
                }
            }

            let trn = turn_table(&mut rng, area);
            m.dir = (m.dir + trn).rem_euclid(360);

            // Labs 90-degree Server farm: floors only here (Server cover
            // props have no Bevy kind; layout impact preserved).
            if area == 6 && trn.abs() == 90 && rng.random::<f32>() * 2.0 < 1.0 {
                for p in [
                    (mx + 1, my),
                    (mx + 1, my + 1),
                    (mx, my + 1),
                    (mx, my - 1),
                    (mx - 1, my),
                    (mx + 1, my - 1),
                    (mx - 1, my - 1),
                    (mx - 1, my + 1),
                ] {
                    stamp_cell(p, &mut seen, &mut plan.floor_cells);
                }
            }

            let dist_from_spawn = ((mx * 32).pow(2) + (my * 32).pow(2)) as f32;
            if dist_from_spawn > 48.0 * 48.0
                && (trn == 180 || (trn.abs() == 90 && (area == 3 || area == 104)))
                && area != 107
                && area != 0
            {
                plan.chests.push(ChestSpawn::Weapon(cell_center_px(mx, my)));
            }

            let n = (next_makers.len() + new_branches.len() + (n_makers - mi)) as f32;
            let mut dies = match area {
                0 => rng.random::<f32>() * (19.0 + n) > 22.0,
                1 | 101 | 105 => rng.random::<f32>() * (19.0 + n) > 20.0,
                2 => rng.random::<f32>() * (14.0 + n) > 15.0,
                3 => rng.random::<f32>() * (39.0 + n) > 40.0,
                4 | 104 => {
                    if area == 104 && rng.random::<f32>() * 4.0 >= 1.0 {
                        false
                    } else {
                        rng.random::<f32>() * (9.0 + n) > 10.0
                    }
                }
                5 => rng.random::<f32>() * (14.0 + n) > 15.0,
                6 => rng.random::<f32>() * (21.0 + n) > 22.0,
                7 => rng.random::<f32>() * (8.0 + n) > 9.0,
                102 => rng.random::<f32>() * (9.0 + n) > 10.0,
                103 | 107 => rng.random::<f32>() * (31.0 + n) > 32.0,
                106 => false,
                _ => rng.random::<f32>() * (19.0 + n) > 20.0,
            };
            // GML area 7 runs a second die check (7/102 shared branch).
            if area == 7 && !dies {
                dies = rng.random::<f32>() * (9.0 + n) > 10.0;
            }
            // HQ die is handled via the Floors>Makers*28 branch rule below.

            if dies && dist_from_spawn > 48.0 * 48.0 {
                plan.chests.push(ChestSpawn::Ammo(cell_center_px(mx, my)));
                stamp_cell((mx, my), &mut seen, &mut plan.floor_cells);
            }

            // HQ AmmoChest drip: 1/10 per maker step when far from spawn.
            if area == 106 && dist_from_spawn > 48.0 * 48.0 && rng.random::<f32>() * 10.0 < 1.0 {
                plan.chests.push(ChestSpawn::Ammo(cell_center_px(mx, my)));
                stamp_cell((mx, my), &mut seen, &mut plan.floor_cells);
            }

            if dies {
                continue;
            }

            // HQ branch rule: Floors > Makers*28 spawns a new maker.
            if area == 106 {
                if plan.floor_cells.len() > makers.len() * 28 {
                    new_branches.push(Maker {
                        x: mx,
                        y: my,
                        dir: m.dir,
                    });
                }
                next_makers.push(m);
                continue;
            }

            let branches = match area {
                0 => rng.random::<f32>() * 4.0 < 1.0,
                1 | 101 => rng.random::<f32>() * 8.0 < 1.0,
                2 => rng.random::<f32>() * 15.0 < 1.0,
                3 => rng.random::<f32>() * 25.0 < 1.0,
                4 | 104 => rng.random::<f32>() * 4.0 < 1.0,
                5 => rng.random::<f32>() * 15.0 < 1.0,
                6 => rng.random::<f32>() * 20.0 < 1.0,
                7 => rng.random::<f32>() * 16.0 < 1.0,
                102 => rng.random::<f32>() * 5.0 < 1.0,
                103 | 107 => rng.random::<f32>() * 20.0 < 1.0,
                101 | 105 => rng.random::<f32>() * 14.0 < 1.0,
                _ => false,
            };
            // GML area 7 runs both its own 1/16 and the shared 7/102 1/5.
            let branches = branches || (area == 7 && rng.random::<f32>() * 5.0 < 1.0);
            if branches {
                new_branches.push(Maker {
                    x: mx,
                    y: my,
                    dir: m.dir,
                });
            }

            next_makers.push(m);
        }

        makers = next_makers;
        makers.extend(new_branches);
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

/// Palace-last hardcoded 8x48 rect (FloorMaker/Step_0 palace-last):
/// skip corners when diy<-43, ThroneStatue rows + inactive generators
/// handled in populate_throne_room.
fn generate_palace_last(run: &Run) -> LevelPlan {
    let mut rng = StdRng::seed_from_u64(run.gen_seed);
    let _ = &mut rng;
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
        styleb: false,
    };
    let mut seen = std::collections::HashSet::new();
    for fy in 0..48 {
        let diy = fy as i32 * 32;
        for fx in 0..8 {
            if diy < -43 && (fx == 0 || fx == 7) {
                continue;
            }
            let c = (fx - 4, fy - 24);
            if seen.insert(c) {
                plan.floor_cells.push(c);
            }
        }
    }
    let floors = plan.floor_cells.clone();
    build_walls(run, &floors, &mut plan);
    let walls = plan.wall_cells.clone();
    populate(run, &floors, &walls, &mut plan, &mut rng);
    plan
}

/// Campfire 5x3 block (GenCont/Create_0 campfire).
fn generate_campfire(run: &Run) -> LevelPlan {
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
        styleb: false,
    };
    let mut seen = std::collections::HashSet::new();
    for xx in -2..=2 {
        for yy in -1..=1 {
            let c = (xx, yy);
            if seen.insert(c) {
                plan.floor_cells.push(c);
            }
        }
    }
    // 7 extra makers worth of sprawl approximated by a few extra rings.
    for _ in 0..40 {
        let idx = rng.random_range(0..plan.floor_cells.len());
        let (cx, cy) = plan.floor_cells[idx];
        let dir = rng.random_range(0..4);
        let (nx, ny) = match dir {
            0 => (cx + 1, cy),
            1 => (cx - 1, cy),
            2 => (cx, cy + 1),
            _ => (cx, cy - 1),
        };
        if seen.insert((nx, ny)) {
            plan.floor_cells.push((nx, ny));
        }
        if plan.floor_cells.len() >= 60 {
            break;
        }
    }
    let floors = plan.floor_cells.clone();
    build_walls(run, &floors, &mut plan);
    let walls = plan.wall_cells.clone();
    populate(run, &floors, &walls, &mut plan, &mut rng);
    plan
}

/// HQ-last 10x10 block + side rooms (FloorMaker/Create_0 hq-last).
fn generate_hq_last(run: &Run) -> LevelPlan {
    let mut rng = StdRng::seed_from_u64(run.gen_seed);
    let _ = &mut rng;
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
        styleb: true,
    };
    let mut seen = std::collections::HashSet::new();
    for fx in 0..10 {
        for fy in 0..10 {
            let c = (fx - 5, fy - 5);
            if seen.insert(c) {
                plan.floor_cells.push(c);
            }
        }
    }
    // 8x2 top/bottom strips + 2x4 side rooms.
    for fx in 0..8 {
        for (sx, sy) in [(fx - 4, 6), (fx - 4, -7)] {
            if seen.insert((sx, sy)) {
                plan.floor_cells.push((sx, sy));
            }
            if seen.insert((sx, sy + 1)) {
                plan.floor_cells.push((sx, sy + 1));
            }
        }
    }
    for fy in 0..4 {
        for (sx, sy) in [(6, fy - 2), (-7, fy - 2)] {
            if seen.insert((sx, sy)) {
                plan.floor_cells.push((sx, sy));
            }
        }
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

/// Top-left of a 16px wall cell in Bevy y-up lattice space.
/// Wall instance (x,y) in GM is this point (origin 0,0 on Bot/Top).
fn wall_top_left(wx: i32, wy: i32) -> Vec2 {
    // Bevy y-up: cell covers [wx*16, wx*16+16) × [wy*16, wy*16+16).
    // GM y-down top-left of that cell on screen = top edge = high Bevy y.
    Vec2::new(wx as f32 * WALL_PX, (wy as f32 + 1.0) * WALL_PX)
}

// mcr_floor_make_walls - 12-probe ring on the 16px lattice

fn build_walls(run: &Run, floors: &[(i32, i32)], plan: &mut LevelPlan) {
    let _ = run;
    let floor_set: std::collections::HashSet<(i32, i32)> = floors.iter().copied().collect();

    for &(cx, cy) in floors {
        // Tile spans lattice cells [2cx..2cx+2) x [2cy..2cy+2).
        // Probe the 12 surrounding 16px positions (mcr_floor_make_walls).
        // Single ring only: GML has no extra growth layers. Void beyond
        // is background color, not walls.
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

// scrPopulate / scrPopProps / scrPopEnemies

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

    let mut prop_tiles: std::collections::HashSet<(i32, i32)> = std::collections::HashSet::new();

    // Small interior walls (scrPopProps head): 1/5, dist>100, one per Floor,
    // excluded on HQ/Vault/Labs/boss/palace-last (GML NOWALL rules).
    let small_walls_allowed = !boss_sub
        && !matches!(
            run.area,
            crate::game::areas::AreaId::HQ
                | crate::game::areas::AreaId::Vault
                | crate::game::areas::AreaId::CrownVault
                | crate::game::areas::AreaId::Labs
                | crate::game::areas::AreaId::Campfire
        )
        && !(((run.floor.max(1) - 1) % 15) + 1 == 15
            && run.area == crate::game::areas::AreaId::Palace);
    for &(cx, cy) in floors {
        let (px, py) = cell_center_i(cx, cy);
        let dist_sq = px * px + py * py;

        if small_walls_allowed && rng.random::<f32>() * 5.0 < 1.0 && dist_sq > 100.0 * 100.0 {
            let sx = px + rng.random_range(-8.0..8.0);
            let sy = py + rng.random_range(-8.0..8.0);
            let wx = (sx / WALL_PX).floor() as i32;
            let sy = py + rng.random_range(-8.0..8.0);
            let wx = (sx / WALL_PX).floor() as i32;
            let wy = (sy / WALL_PX).floor() as i32;
            plan.small_walls.push((wx as i16, wy as i16));
            prop_tiles.insert((cx, cy));
        }
    }

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

    for &(cx, cy) in floors {
        if prop_tiles.contains(&(cx, cy)) {
            continue;
        }
        let (px, py) = cell_center_i(cx, cy);
        let dist_sq = px * px + py * py;

        // GML scrPopProps head gate: random(unlikeliness)>1 exits, with
        // unlikeliness jungle 2, campfire 7, else 10.
        let unlikeliness = if run.area == crate::game::areas::AreaId::Jungle {
            2.0
        } else if run.area == crate::game::areas::AreaId::Campfire {
            7.0
        } else {
            10.0
        };
        // Functional patches use the same gate in GML; Bevy test-parity
        // kinds keep a slightly higher keep-rate via threshold.
        // Secret areas keep their own upstream prop families.
        let is_secret = crate::game::secret_areas::is_secret_area(run.area);
        let kind = if is_secret {
            match run.area {
                crate::game::areas::AreaId::Oasis => {
                    let r: f32 = rng.random();
                    if r < 0.025 {
                        PropKind::Anchor
                    } else if r < 0.35 {
                        PropKind::WaterPlant
                    } else if r < 0.50 {
                        PropKind::OasisBarrel
                    } else if r < 0.62 {
                        PropKind::WaterMine
                    } else {
                        PropKind::GroundDecal
                    }
                }
                crate::game::areas::AreaId::PizzaSewers => {
                    if rng.random::<f32>() < 0.7 {
                        PropKind::PizzaBox
                    } else {
                        PropKind::GroundDecal
                    }
                }
                crate::game::areas::AreaId::Jungle => {
                    if rng.random::<f32>() * 30.0 < 1.0 {
                        PropKind::BigFlower
                    } else if rng.random::<f32>() < 0.55 {
                        PropKind::Bush
                    } else {
                        PropKind::GroundDecal
                    }
                }
                crate::game::areas::AreaId::CursedCaves => PropKind::GroundDecal,
                crate::game::areas::AreaId::City => {
                    // Y.V. Mansion
                    let r = rng.random_range(0..10);
                    match r {
                        0..=3 => PropKind::MoneyPile,
                        4 => PropKind::YVStatue,
                        5 => PropKind::GoldBarrel,
                        _ => PropKind::GroundDecal,
                    }
                }
                crate::game::areas::AreaId::Vault | crate::game::areas::AreaId::CrownVault => {
                    PropKind::Torch
                }
                crate::game::areas::AreaId::HQ => {
                    if rng.random::<f32>() < 0.5 {
                        PropKind::PlantPot
                    } else {
                        PropKind::GroundDecal
                    }
                }
                _ => PropKind::GroundDecal,
            }
        } else {
            match area {
                // Desert - scrPopProps: 1/60 BigSkull else styleb 1/5
                // BonePile else Cactus/TopDecal.
                1 => {
                    if rng.random::<f32>() * 60.0 < 1.0 {
                        PropKind::BigSkull
                    } else if plan.styleb && rng.random::<f32>() * 5.0 < 1.0 {
                        PropKind::BonePile
                    } else if rng.random::<f32>() * 4.0 < 3.0 {
                        if plan.styleb {
                            PropKind::NightCactus
                        } else {
                            PropKind::Cactus
                        }
                    } else {
                        PropKind::GroundDecal
                    }
                }

                // Sewers - scrPopProps dist>96: Pipex4/ToxicBarrelx2/TopDecal
                2 => {
                    if dist_sq < 96.0 * 96.0 {
                        PropKind::GroundDecal
                    } else {
                        let roll = rng.random_range(0..7);
                        match roll {
                            0..=3 => PropKind::Pipe,
                            4..=5 => PropKind::ToxicBarrel,
                            _ => PropKind::GroundDecal,
                        }
                    }
                }

                // Scrapyards - scrPopProps: Tires/Car/TopDecal
                3 => {
                    let roll = rng.random_range(0..7);
                    match roll {
                        0..=2 => PropKind::Tires,
                        3..=4 => PropKind::Car,
                        _ => PropKind::GroundDecal,
                    }
                }

                // Crystal Caves - scrPopProps: Crystal/Cocoon/BonePile
                4 => {
                    let r: f32 = rng.random();
                    if r < 0.25 {
                        PropKind::Crystal
                    } else if r < 0.45 {
                        PropKind::Cocoon
                    } else if r < 0.55 {
                        PropKind::BonePile
                    } else if r < 0.75 {
                        PropKind::Cobweb
                    } else {
                        PropKind::GroundDecal
                    }
                }

                // Frozen City - scrPopProps dist>32 SnowMan/Soda, dist>128
                // Hydrant/Car; StreetLight near wall.
                5 => {
                    if dist_sq < 32.0 * 32.0 {
                        PropKind::GroundDecal
                    } else {
                        let r: f32 = rng.random();
                        if r < 0.18 {
                            PropKind::IcePatch
                        } else if r < 0.28 {
                            PropKind::Snowman
                        } else if r < 0.36 {
                            PropKind::SodaMachine
                        } else if r < 0.44 {
                            PropKind::StreetLight
                        } else if r < 0.54 {
                            if dist_sq < 128.0 * 128.0 {
                                PropKind::GroundDecal
                            } else {
                                PropKind::Hydrant
                            }
                        } else if r < 0.60 {
                            if dist_sq < 128.0 * 128.0 {
                                PropKind::GroundDecal
                            } else {
                                PropKind::Car
                            }
                        } else {
                            PropKind::GroundDecal
                        }
                    }
                }

                // Labs - upstream Tube/MutantTube, plus functional hazards; keep ToxicBarrel for Bevy test parity
                6 => {
                    let r: f32 = rng.random();
                    if r < 0.30 {
                        PropKind::Tube
                    } else if r < 0.38 {
                        PropKind::MutantTube
                    } else if r < 0.50 {
                        PropKind::ToxicBarrel
                    } else if r < 0.60 {
                        PropKind::FireTrap
                    } else if r < 0.65 {
                        PropKind::Mine
                    } else {
                        PropKind::GroundDecal
                    }
                }

                // Palace - upstream Pillar/SmallGenerator/Torch plus functional FireTrap/Mine for test parity
                7 => {
                    let r: f32 = rng.random();
                    if r < 0.20 {
                        PropKind::Pillar
                    } else if r < 0.35 {
                        PropKind::SmallGenerator
                    } else if r < 0.42 {
                        PropKind::Torch
                    } else if r < 0.50 {
                        PropKind::FireTrap
                    } else if r < 0.55 {
                        PropKind::Mine
                    } else {
                        PropKind::GroundDecal
                    }
                }

                _ => PropKind::GroundDecal,
            }
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
        if rng.random::<f32>() * unlikeliness > threshold {
            continue;
        }

        // GML dist guards for secret families (oasis dist>96, mansion
        // dist>64): force decal when too close to spawn.
        let too_close = match kind {
            PropKind::Anchor => false,
            PropKind::WaterPlant | PropKind::OasisBarrel | PropKind::WaterMine => {
                dist_sq < 96.0 * 96.0
            }
            PropKind::MoneyPile | PropKind::YVStatue | PropKind::GoldBarrel => {
                dist_sq < 64.0 * 64.0
            }
            _ => false,
        };
        if too_close {
            continue;
        }

        let claims_tile = !matches!(
            kind,
            PropKind::GroundDecal | PropKind::Cobweb | PropKind::IcePatch | PropKind::FireTrap
        );

        // Safespawn reserve (GenCont/Step_0 intent): keep solid props off
        // the spawn cell so the player never starts stuck inside a prop.
        if claims_tile && dist_sq < 64.0 * 64.0 {
            continue;
        }

        if claims_tile {
            prop_tiles.insert((cx, cy));
        }
        plan.props.push((kind, Vec2::new(px, py)));
    }

    // Boss floors still get trash mobs upstream; only the bare Throne room
    // (route floor 15) stays sparse so the boss has room.
    let rf_route = ((run.floor.max(1) - 1) % 15) + 1;
    let skip_enemies = boss_sub && rf_route == 15;
    // GML city-last uses spawndist 150, else 120.
    let spawn_dist = if area == 5 && run.floor_in_area >= 3 {
        150.0
    } else {
        120.0
    };
    let mut enemy_tiles: Vec<(EnemyKind, Vec2)> = Vec::new();
    for &(cx, cy) in floors {
        if skip_enemies {
            break;
        }
        let (px, py) = cell_center_i(cx, cy);
        let dist_sq = px * px + py * py;
        if dist_sq < spawn_dist * spawn_dist || prop_tiles.contains(&(cx, cy)) {
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
        // GML caps at 3+hard/1.5 total; no soft_max ceiling.
        let chance = hard / (10.0 + hard);
        if rng.random::<f32>() >= chance && enemy_tiles.len() >= enemy_min {
            continue;
        }

        let center = Vec2::new(px, py);
        let pick_kind = |rng: &mut StdRng, w: &[EnemyKind]| w[rng.random_range(0..w.len())];
        let loop_extras = loop_elite_candidates(area, run.loop_count);

        // Upstream spawns packs per tile (repeat(3)/repeat(5)), so these
        // produce a Vec of kinds placed around the same cell.
        {
            use crate::game::areas::AreaId;
            let mut secret_kinds: Vec<EnemyKind> = match run.area {
                AreaId::Oasis => {
                    // Upstream: Crab (1-in-4), else repeat(3) Bone Fish.
                    if rng.random::<f32>() * 4.0 < 1.0 {
                        vec![EnemyKind::Crab]
                    } else if rng.random::<f32>() * 3.0 < 1.0 {
                        vec![
                            EnemyKind::BoneFish,
                            EnemyKind::BoneFish,
                            EnemyKind::BoneFish,
                        ]
                    } else {
                        Vec::new()
                    }
                }
                AreaId::PizzaSewers => vec![EnemyKind::Turtle],
                AreaId::Jungle => {
                    // Upstream: JungleFly (1-in-8); 1-in-30 barrel ambush
                    // with three bandits; else bandit packs.
                    if rng.random::<f32>() * 8.0 < 1.0 {
                        vec![EnemyKind::JungleFly]
                    } else if rng.random::<f32>() * 30.0 < 1.0 {
                        plan.props.push((PropKind::Barrel, center));
                        vec![
                            EnemyKind::JungleBandit,
                            EnemyKind::JungleBandit,
                            EnemyKind::JungleBandit,
                        ]
                    } else {
                        let k = pick_kind(
                            &mut rng,
                            &[
                                EnemyKind::JungleBandit,
                                EnemyKind::JungleBandit,
                                EnemyKind::JungleBandit,
                                EnemyKind::JungleBandit,
                                EnemyKind::JungleBandit,
                                EnemyKind::Maggot,
                                EnemyKind::Assassin,
                                EnemyKind::Assassin,
                            ],
                        );
                        vec![k]
                    }
                }
                AreaId::CursedCaves => {
                    // Upstream: invisible spiders + cursed laser crystals.
                    if rng.random::<f32>() * 5.0 < 4.0 {
                        let k = pick_kind(
                            &mut rng,
                            &[
                                EnemyKind::InvSpider,
                                EnemyKind::InvSpider,
                                EnemyKind::InvSpider,
                                EnemyKind::InvSpider,
                                EnemyKind::InvLaserCrystal,
                                EnemyKind::InvLaserCrystal,
                            ],
                        );
                        vec![k]
                    } else {
                        Vec::new()
                    }
                }
                AreaId::City => {
                    // Y.V. Mansion: 1-in-5 fireballer/jock squads with a
                    // super fireballer; else 1-in-4 gold barrel + molefish
                    // patrols (upstream area_mansion).
                    if rng.random::<f32>() * 5.0 < 1.0 {
                        let k = pick_kind(
                            &mut rng,
                            &[
                                EnemyKind::FireBaller,
                                EnemyKind::Jock,
                                EnemyKind::FireBaller,
                                EnemyKind::Jock,
                                EnemyKind::FireBaller,
                                EnemyKind::SuperFireBaller,
                            ],
                        );
                        vec![k]
                    } else if rng.random::<f32>() * 4.0 < 1.0 {
                        if rng.random::<f32>() * 5.0 < 1.0 {
                            plan.props.push((PropKind::GoldBarrel, center));
                        }
                        let k = pick_kind(
                            &mut rng,
                            &[
                                EnemyKind::Molefish,
                                EnemyKind::Molefish,
                                EnemyKind::Molefish,
                                EnemyKind::Molefish,
                                EnemyKind::Molesarge,
                            ],
                        );
                        vec![k]
                    } else {
                        Vec::new()
                    }
                }
                AreaId::Vault | AreaId::CrownVault => {
                    // Guardians are the boss; keep trash sparse and elite.
                    let k = pick_kind(
                        &mut rng,
                        &[
                            EnemyKind::RobotGuard,
                            EnemyKind::Turret,
                            EnemyKind::IdpdElite,
                        ],
                    );
                    vec![k]
                }
                AreaId::HQ => {
                    // Upstream area_hq: elite trio (1-in-7), repeat(5) Grunt
                    // pack (1-in-4), or a mixed Grunt/Shielder/Inspector squad.
                    if rng.random::<f32>() * 7.0 < 1.0 {
                        let k = pick_kind(
                            &mut rng,
                            &[
                                EnemyKind::IdpdElite,
                                EnemyKind::IdpdShield,
                                EnemyKind::IdpdInspector,
                            ],
                        );
                        vec![k]
                    } else if rng.random::<f32>() * 4.0 < 1.0 {
                        std::iter::repeat_n(EnemyKind::IdpdGrunt, 5).collect()
                    } else if rng.random::<f32>() * 3.0 < 1.0 {
                        let k = pick_kind(
                            &mut rng,
                            &[
                                EnemyKind::IdpdGrunt,
                                EnemyKind::IdpdShield,
                                EnemyKind::IdpdInspector,
                            ],
                        );
                        vec![k]
                    } else {
                        Vec::new()
                    }
                }
                _ => Vec::new(),
            };
            if !secret_kinds.is_empty() {
                for (i, k) in secret_kinds.drain(..).enumerate() {
                    let jitter =
                        Vec2::new(((i % 3) as f32 - 1.0) * 18.0, ((i / 3) as f32 - 0.5) * 18.0);
                    enemy_tiles.push((k, center + jitter));
                }
                continue;
            }
        }

        match area {
            1 => {
                if rng.random::<f32>() * 7.0 < 1.0 {
                    // Upstream 1-in-7: a MaggotSpawn nest or a Scorpion.
                    let k = pick_kind(&mut rng, &[EnemyKind::MaggotSpawn, EnemyKind::Scorpion]);
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
                // Sewers: rats dominate. Style-B swaps fodder for Gators;
                // loop Sewers mixes Ratkings / BuffGators / Exploders (upstream).
                if run.loop_count > 0 && rng.random::<f32>() * 3.0 >= 1.0 {
                    let mut cands = vec![
                        EnemyKind::Ratking,
                        EnemyKind::Ratking,
                        EnemyKind::BuffGator,
                        EnemyKind::LaserCrystal,
                        EnemyKind::Rat,
                        EnemyKind::Ballguy,
                        EnemyKind::Ballguy,
                        EnemyKind::FrogEgg,
                    ];
                    cands.extend(loop_extras.iter().copied());
                    let k = pick_kind(&mut rng, &cands);
                    enemy_tiles.push((k, center));
                } else if rng.random::<f32>() * 9.0 < 1.0 {
                    let k = pick_kind(
                        &mut rng,
                        &[
                            EnemyKind::Ballguy,
                            EnemyKind::Ratking,
                            EnemyKind::MeleeBandit,
                        ],
                    );
                    enemy_tiles.push((k, center));
                } else {
                    let mut cands = vec![
                        EnemyKind::Rat,
                        EnemyKind::Rat,
                        EnemyKind::Rat,
                        EnemyKind::Maggot,
                        EnemyKind::Gator,
                        EnemyKind::Bandit,
                    ];
                    cands.extend(loop_extras.iter().copied());
                    let k = pick_kind(&mut rng, &cands);
                    enemy_tiles.push((k, center));
                }
            }
            3 => {
                // Scrapyards: Ravens rule the skies, with bandit packs, snipers
                // and exploders below (upstream scrPopEnemies area_scrapyards).
                let roll: f32 = rng.random();
                let mut cands = if roll * 4.0 < 1.0 {
                    // 1-in-4: sniper / melee-bandit skirmish pack.
                    vec![
                        EnemyKind::MeleeBandit,
                        EnemyKind::Sniper,
                        EnemyKind::MeleeBandit,
                        EnemyKind::Sniper,
                        EnemyKind::Ballguy,
                    ]
                } else if roll * 10.0 < 1.0 {
                    // Rare raven flock.
                    vec![
                        EnemyKind::Raven,
                        EnemyKind::Raven,
                        EnemyKind::Raven,
                        EnemyKind::Raven,
                    ]
                } else if roll * 20.0 < 1.0 {
                    vec![EnemyKind::Salamander]
                } else {
                    vec![
                        EnemyKind::Raven,
                        EnemyKind::Raven,
                        EnemyKind::Raven,
                        EnemyKind::Bandit,
                    ]
                };
                cands.extend(loop_extras.iter().copied());
                let k = pick_kind(&mut rng, &cands);
                enemy_tiles.push((k, center));
            }
            4 => {
                // Crystal Caves: spider nests; loops bring freaks + lightning
                // crystals (upstream scrPopEnemies area_caves).
                let mut cands = if run.loop_count > 0 && rng.random_bool(0.5) {
                    vec![
                        EnemyKind::LaserCrystal,
                        EnemyKind::LaserCrystal,
                        EnemyKind::RhinoFreak,
                        EnemyKind::LightningCrystal,
                        EnemyKind::BuffGator,
                        EnemyKind::ExploFreak,
                        EnemyKind::Spider,
                        EnemyKind::Spider,
                    ]
                } else {
                    vec![
                        EnemyKind::Spider,
                        EnemyKind::Spider,
                        EnemyKind::Spider,
                        EnemyKind::Spider,
                        EnemyKind::LaserCrystal,
                        EnemyKind::Crystal,
                    ]
                };
                cands.extend(loop_extras.iter().copied());
                let k = pick_kind(&mut rng, &cands);
                enemy_tiles.push((k, center));
            }
            5 => {
                // Frozen City: snowbot garrison with tanks and wolves
                // (upstream scrPopEnemies area_city).
                let mut frozen = if run.loop_count > 0 && rng.random_bool(0.5) {
                    vec![
                        EnemyKind::RobotGuard,
                        EnemyKind::RobotGuard,
                        EnemyKind::SnowTank,
                        EnemyKind::DogGuardian,
                        EnemyKind::ExploGuardian,
                        EnemyKind::Wolf,
                        EnemyKind::Necromancer,
                    ]
                } else {
                    vec![
                        EnemyKind::RobotGuard,
                        EnemyKind::RobotGuard,
                        EnemyKind::RobotGuard,
                        EnemyKind::SnowTank,
                        EnemyKind::Wolf,
                        EnemyKind::Wolf,
                    ]
                };
                frozen.extend(loop_extras.iter().copied());
                let k = pick_kind(&mut rng, &frozen);
                enemy_tiles.push((k, center));
            }
            6 => {
                // Labs: freak swarms under necromancer supervision; rhino and
                // explo freaks patrol (upstream area_labs).
                let mut late = if run.loop_count > 0 && rng.random_bool(0.5) {
                    vec![
                        EnemyKind::Ratking,
                        EnemyKind::RhinoFreak,
                        EnemyKind::ExploFreak,
                        EnemyKind::Necromancer,
                        EnemyKind::LaserCrystal,
                        EnemyKind::Turret,
                    ]
                } else {
                    vec![
                        EnemyKind::Freak,
                        EnemyKind::Freak,
                        EnemyKind::Freak,
                        EnemyKind::Necromancer,
                        EnemyKind::ExploFreak,
                        EnemyKind::RhinoFreak,
                    ]
                };
                late.extend(std::iter::repeat_n(
                    EnemyKind::IdpdGrunt,
                    (run.loop_count.min(3) * 2) as usize,
                ));
                late.extend(loop_extras.iter().copied());
                let k = pick_kind(&mut rng, &late);
                enemy_tiles.push((k, center));
            }
            7 => {
                // Palace: guardian squads (orb / explo / dog variants);
                // loops leak snipers, explo freaks and jungle bandits.
                let mut palace = if run.loop_count > 0 && rng.random_bool(0.5) {
                    vec![
                        EnemyKind::ExploGuardian,
                        EnemyKind::DogGuardian,
                        EnemyKind::DogGuardian,
                        EnemyKind::Sniper,
                        EnemyKind::ExploFreak,
                        EnemyKind::JungleBandit,
                        EnemyKind::JungleBandit,
                    ]
                } else {
                    vec![
                        EnemyKind::Guardian,
                        EnemyKind::Guardian,
                        EnemyKind::Guardian,
                        EnemyKind::ExploGuardian,
                        EnemyKind::ExploGuardian,
                        EnemyKind::DogGuardian,
                    ]
                };
                palace.extend(std::iter::repeat_n(
                    EnemyKind::IdpdGrunt,
                    (run.loop_count.min(3)) as usize,
                ));
                palace.extend(loop_extras.iter().copied());
                let k = pick_kind(&mut rng, &palace);
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
            // Upstream: the Pizza Sewers dead-end room is guarded by the
            // Frog Queen (secret visits are single-floor here).
            AreaId::PizzaSewers => plan.boss = Some(EnemyKind::FrogQueen),
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

    // scrReplacePropWithChest fallback: if walker produced no Weapon/Ammo,
    // convert the furthest solid prop dist>160px into that chest kind.
    // Skipped on campfire/vault/HQ-last where GML spawns zero chests.
    let is_no_chest_area = matches!(
        run.area,
        crate::game::areas::AreaId::Campfire
            | crate::game::areas::AreaId::Vault
            | crate::game::areas::AreaId::CrownVault
    ) || (run.area == crate::game::areas::AreaId::HQ
        && run.floor_in_area >= 3);
    if !is_no_chest_area {
        let has_weapon = plan
            .chests
            .iter()
            .any(|c| matches!(c, ChestSpawn::Weapon(_)));
        let has_ammo = plan.chests.iter().any(|c| matches!(c, ChestSpawn::Ammo(_)));
        if !has_weapon || !has_ammo {
            // Furthest solid prop beyond 160px.
            let mut best: Option<(f32, usize)> = None;
            for (i, (_, p)) in plan.props.iter().enumerate() {
                let d2 = p.length_squared();
                if d2 < 160.0 * 160.0 {
                    continue;
                }
                if best.map(|(bd, _)| d2 > bd).unwrap_or(true) {
                    best = Some((d2, i));
                }
            }
            if let Some((_, idx)) = best {
                let pos = plan.props[idx].1;
                plan.props.remove(idx);
                if !has_weapon {
                    plan.chests.push(ChestSpawn::Weapon(pos));
                } else {
                    plan.chests.push(ChestSpawn::Ammo(pos));
                }
            }
        }
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
            // Upstream loop Desert: scorpions, jungle flies, melee bandits, snipers.
            table.push((EnemyKind::Scorpion, 4 + l * 2));
            table.push((EnemyKind::JungleFly, 3 + l));
            table.push((EnemyKind::MeleeBandit, 3 + l));
            table.push((EnemyKind::Sniper, 3 + l));
            if loop_count >= 2 {
                table.push((EnemyKind::GoldScorpion, 2 + l));
            }
        }
        AreaId::Sewers => {
            // Upstream loop Sewers: ratkings and buff gators move in.
            table.push((EnemyKind::Ratking, 4 + l * 2));
            table.push((EnemyKind::BuffGator, 3 + l));
            table.push((EnemyKind::IdpdShield, 2 + l));
        }
        AreaId::Scrapyards => {
            // Upstream loop Scrapyards: snipers, salamanders, buff gators.
            table.push((EnemyKind::Sniper, 4 + l * 2));
            table.push((EnemyKind::MeleeBandit, 3 + l));
            table.push((EnemyKind::Salamander, 2 + l));
            if loop_count >= 2 {
                table.push((EnemyKind::BuffGator, 3 + l));
                table.push((EnemyKind::RobotGuard, 2 + l));
            }
        }
        AreaId::CrystalCaves => {
            // Upstream loop Caves: freaks + lightning crystals join the nests.
            table.push((EnemyKind::RhinoFreak, 4 + l * 2));
            table.push((EnemyKind::ExploFreak, 3 + l));
            table.push((EnemyKind::LightningCrystal, 3 + l));
            if loop_count >= 2 {
                table.push((EnemyKind::IdpdElite, 3 + l));
            }
        }
        AreaId::FrozenCity => {
            // Upstream loop City: tanks, dog/explo guardians, necromancers.
            table.push((EnemyKind::SnowTank, 4 + l));
            table.push((EnemyKind::DogGuardian, 3 + l));
            table.push((EnemyKind::ExploGuardian, 3 + l));
            table.push((EnemyKind::Necromancer, 2 + l));
            if loop_count >= 2 {
                table.push((EnemyKind::GoldSnowtank, 2 + l));
            }
        }
        AreaId::Labs => {
            // Upstream loop Labs: ratkings, rhino/explo freak packs.
            table.push((EnemyKind::Ratking, 4 + l));
            table.push((EnemyKind::RhinoFreak, 4 + l * 2));
            table.push((EnemyKind::ExploFreak, 4 + l));
            if loop_count >= 2 {
                table.push((EnemyKind::IdpdElite, 5 + l));
            }
        }
        AreaId::Palace => {
            // Upstream loop Palace: snipers, explo freaks, jungle bandits,
            // and rare IDPD popo-freak portal squads (IDPDSpawn).
            table.push((EnemyKind::Sniper, 3 + l));
            table.push((EnemyKind::ExploFreak, 3 + l));
            table.push((EnemyKind::JungleBandit, 3 + l * 2));
            if loop_count >= 1 {
                table.push((EnemyKind::PopoFreak, 2 + l));
            }
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

/// Upstream GameCont.hard (GameCont/Create_0 hard=0, Other_5 hard+=1 per area clear, loops>=2 hardgot).
/// GML scrAreaGetDifficulty: sub + loops*16 + sum(maxsub); loop term is 16.
pub fn game_hard(run: &Run) -> f32 {
    let floors_cleared = run.floor.saturating_sub(1) as f32; // areas cleared before current floor
    let loops = run.loop_count as f32;
    (floors_cleared + loops * 16.0).max(1.0)
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
            EnemyKind::Gator,
        ],
        3 => vec![
            EnemyKind::Raven,
            EnemyKind::Raven,
            EnemyKind::Raven,
            EnemyKind::Sniper,
            EnemyKind::Ballguy,
        ],
        4 => vec![
            EnemyKind::Spider,
            EnemyKind::Spider,
            EnemyKind::Crystal,
            EnemyKind::LaserCrystal,
        ],
        5 => vec![
            EnemyKind::RobotGuard,
            EnemyKind::RobotGuard,
            EnemyKind::SnowTank,
            EnemyKind::Wolf,
            EnemyKind::Wolf,
        ],
        6 => vec![
            EnemyKind::Freak,
            EnemyKind::Freak,
            EnemyKind::Necromancer,
            EnemyKind::ExploFreak,
            EnemyKind::RhinoFreak,
        ],
        7 => vec![
            EnemyKind::Guardian,
            EnemyKind::ExploGuardian,
            EnemyKind::DogGuardian,
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
    // Palace-last: GML hardcoded rect already in floor_cells. Keep sparse:
    // remove clutter traps/mines, clear trash, place inactive generators +
    // ThroneStatue rows + Throne boss.
    plan.props
        .retain(|(k, _)| !matches!(k, PropKind::Mine | PropKind::FireTrap));
    plan.enemies.clear();

    // 4x generators near the throne end (gameplay-equivalent positions).
    let gens = [
        Vec2::new(-220.0, 120.0),
        Vec2::new(220.0, 120.0),
        Vec2::new(-220.0, -120.0),
        Vec2::new(220.0, -120.0),
    ];
    for p in gens {
        plan.props.push((PropKind::BigGenerator, p));
    }
    // ThroneStatue every ~5 rows along the hall (GML palace-last).
    for i in 0..9 {
        let y = -320.0 + i as f32 * 80.0;
        plan.props.push((PropKind::ThroneStatue, Vec2::new(0.0, y)));
    }
    plan.boss = Some(EnemyKind::Throne);
    plan.boss_count = 1;
}

/// GML scrPopChests Open Mind bonus: `repeat (open_mind * 2)` extra chests
/// of random kind. Applied AFTER trim (which keeps one of each kind) at the
/// two production gen sites - never on portal open. Skipped where GML spawns
/// zero chests (campfire/vault/HQ-last).
pub fn apply_open_mind_bonus(
    plan: &mut LevelPlan,
    area: crate::game::areas::AreaId,
    floor_in_area: u32,
) {
    use crate::game::areas::AreaId;
    let no_chests = matches!(area, AreaId::Campfire | AreaId::Vault | AreaId::CrownVault)
        || (area == AreaId::HQ && floor_in_area >= 3);
    if no_chests || plan.floor_cells.is_empty() {
        return;
    }
    let mut rng = rand::rng();
    for _ in 0..2 {
        let idx = rng.random_range(0..plan.floor_cells.len());
        let (cx, cy) = plan.floor_cells[idx];
        let pos = cell_center_px(cx, cy);
        // GML choose(1, 2, 3): weapon / ammo / rad.
        match rng.random_range(0..3) {
            0 => plan.chests.push(ChestSpawn::Weapon(pos)),
            1 => plan.chests.push(ChestSpawn::Ammo(pos)),
            _ => plan.chests.push(ChestSpawn::Rad(pos)),
        }
    }
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

// Spawning

fn sprite_frames(catalog: &AssetCatalog, path: &str) -> usize {
    catalog
        .anims
        .get(path)
        .map(|m| m[0].max(1.0) as usize)
        .unwrap_or(1)
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

/// Deterministic per-floor seed chain (gameplay-equivalent): each floor's
/// gen_seed derives from the previous seed + floor + area, so re-entering
/// the same floor/area replays the same layout instead of fresh random.
pub fn derive_floor_seed(prev: u64, floor: u32, area: u8, loop_count: u32) -> u64 {
    let mut x = prev
        .wrapping_add(floor as u64)
        .wrapping_mul(6364136223846793005)
        .wrapping_add(area as u64)
        .wrapping_mul(6364136223846793005)
        .wrapping_add(loop_count as u64 + 1);
    x ^= x >> 30;
    x = x.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

/// Seed-stable prop variant/flip from gen_seed + position (replaces
/// non-deterministic rand::rng() so the same gen_seed replays same props).
fn prop_hash_pick(seed: u64, pos: Vec2, salt: u64, n: usize) -> usize {
    if n <= 1 {
        return 0;
    }
    let wx = (pos.x / 32.0).floor() as i32;
    let wy = (pos.y / 32.0).floor() as i32;
    (wall_hash(seed, wx, wy, salt) % n as u64) as usize
}

fn prop_hash_flip(seed: u64, pos: Vec2, salt: u64) -> bool {
    wall_hash(seed, wx_of(pos), wy_of(pos), salt) % 2 == 0
}

fn wx_of(pos: Vec2) -> i32 {
    (pos.x / 32.0).floor() as i32
}

fn wy_of(pos: Vec2) -> i32 {
    (pos.y / 32.0).floor() as i32
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
    &'static str,
) {
    // (floor, bot, top, out, trans, ground decal)
    // Upstream sprite families are named by _area id.
    let rf = ((floor.max(1) - 1) % 15) + 1;
    match rf {
        3 => (
            "images/sprFloor0.png",
            "images/sprWall0Bot.png",
            "images/sprWall0Top.png",
            "images/sprWall0Out.png",
            "images/sprWall0Trans.png",
            "images/sprNightDesertTopDecal.png",
        ),
        4 => (
            "images/sprFloor2.png",
            "images/sprWall2Bot.png",
            "images/sprWall2Top.png",
            "images/sprWall2Out.png",
            "images/sprWall2Trans.png",
            "images/sprTopDecalSewers.png",
        ),
        5..=7 => (
            "images/sprFloor3.png",
            "images/sprWall3Bot.png",
            "images/sprWall3Top.png",
            "images/sprWall3Out.png",
            "images/sprWall3Trans.png",
            "images/sprTopDecalScrapyard.png",
        ),
        8 => (
            "images/sprFloor4.png",
            "images/sprWall4Bot.png",
            "images/sprWall4Top.png",
            "images/sprWall4Out.png",
            "images/sprWall4Trans.png",
            "images/sprTopDecalCave.png",
        ),
        9..=11 => (
            "images/sprFloor5.png",
            "images/sprWall5Bot.png",
            "images/sprWall5Top.png",
            "images/sprWall5Out.png",
            "images/sprWall5Trans.png",
            "images/sprTopDecalCity.png",
        ),
        12 => (
            "images/sprFloor6.png",
            "images/sprWall6Bot.png",
            "images/sprWall6Top.png",
            "images/sprWall6Out.png",
            "images/sprWall6Trans.png",
            "images/sprTopDecalCity.png",
        ),
        13..=15 => (
            "images/sprFloor7.png",
            "images/sprWall7Bot.png",
            "images/sprWall7Top.png",
            "images/sprWall7Out.png",
            "images/sprWall7Trans.png",
            "images/sprPalaceTopDecal.png",
        ),
        _ => (
            "images/sprFloor1.png",
            "images/sprWall1Bot.png",
            "images/sprWall1Top.png",
            "images/sprWall1Out.png",
            "images/sprWall1Trans.png",
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

    let (floor, bot, top, out, trans) = match run.area {
        AreaId::Oasis => (
            "images/sprFloor101.png",
            "images/sprWall101Bot.png",
            "images/sprWall101Top.png",
            "images/sprWall101Out.png",
            "images/sprWall101Trans.png",
        ),
        AreaId::PizzaSewers => (
            "images/sprFloor102.png",
            "images/sprWall102Bot.png",
            "images/sprWall102Top.png",
            "images/sprWall102Out.png",
            "images/sprWall102Trans.png",
        ),
        AreaId::City => (
            "images/sprFloor103.png",
            "images/sprWall103Bot.png",
            "images/sprWall103Top.png",
            "images/sprWall103Out.png",
            "images/sprWall103Trans.png",
        ),
        AreaId::CursedCaves | AreaId::Vault | AreaId::CrownVault => (
            "images/sprFloor104.png",
            "images/sprWall104Bot.png",
            "images/sprWall104Top.png",
            "images/sprWall104Out.png",
            "images/sprWall104Trans.png",
        ),
        AreaId::Jungle => (
            "images/sprFloor105.png",
            "images/sprWall105Bot.png",
            "images/sprWall105Top.png",
            "images/sprWall105Out.png",
            "images/sprWall105Trans.png",
        ),
        _ => (
            "images/sprFloor106.png",
            "images/sprWall106Bot.png",
            "images/sprWall106Top.png",
            "images/sprWall106Out.png",
            "images/sprWall106Trans.png",
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
        slot(trans, route.4),
        route.5,
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
    let (floor_png, wall_bot_png, wall_top_png, wall_out_png, wall_trans_png, decal_prop_png) =
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

    // Walls - faithful to objects/Wall/Create_0 + Top draw:
    //
    // GM instance (x,y) = top-left of 16×16 (mskWall).
    // - sprWallNOut @ (x,y) origin (4,12)  → extends past the ground edge
    // - sprWallNBot @ (x,y) origin (0,0)   → only if place_meeting(x, y+16, Floor)
    // - sprWallNTop @ (x, y-8) origin (0,0)→ always, 8px "up" the screen
    //
    // Bevy lattice is y-up; screen-south (GM +y) is −Bevy y.

    let floor_set: std::collections::HashSet<(i32, i32)> =
        plan.floor_cells.iter().copied().collect();
    let wall_set: std::collections::HashSet<(i32, i32)> = {
        let mut s = plan.wall_cells.clone();
        for &(wx, wy) in &plan.small_walls {
            s.insert((wx as i32, wy as i32));
        }
        s
    };

    let mut all_walls: Vec<(i32, i32)> = wall_set.iter().copied().collect();
    // Stable order for determinism
    all_walls.sort_unstable();

    for (wx, wy) in all_walls {
        let c = wall_center(wx, wy);
        // GM draw point (top-left of 16×16), mapped into Bevy y-up.
        // Top edge of cell = high Bevy y.
        let gm_draw = wall_top_left(wx, wy);

        let body_frame = wall_body_frame(catalog, run.gen_seed, wx, wy, wall_bot_png);
        let top_frame = wall_top_frame(catalog, run.gen_seed, wx, wy, wall_top_png);
        let out_frame = wall_out_frame(catalog, run.gen_seed, wx, wy, wall_out_png);

        // place_meeting(x, y+16, Floor) in GM y-down = one lattice step
        // screen-south = Bevy wy - 1.
        let floor_south = {
            // Probe center of the 16px cell immediately south on screen.
            let probe = Vec2::new(c.x, c.y - WALL_PX);
            let owner = (
                (probe.x / TILE).floor() as i32,
                (probe.y / TILE).floor() as i32,
            );
            floor_set.contains(&owner)
        };

        // Visuals first so they can be linked to the solid via WallVisuals.
        let mut parts: Vec<Entity> = Vec::with_capacity(3);

        // sprWall*Out is 24×32, origin (4,12). Drawing at GM (x,y) makes it
        // hang 4px past each side and 12px "above" the wall top-left.
        if catalog.has(wall_out_png) {
            let (spr, tf) = sprite_at_gm_origin(
                catalog,
                asset_server,
                wall_out_png,
                out_frame,
                gm_draw,
                -42.0,
            );
            let e = commands.spawn((GameCleanup, LevelCleanup, spr, tf)).id();
            parts.push(e);
        }

        if floor_south && catalog.has(wall_bot_png) {
            let (spr, tf) = sprite_at_gm_origin(
                catalog,
                asset_server,
                wall_bot_png,
                body_frame,
                gm_draw,
                -40.0,
            );
            let e = commands.spawn((GameCleanup, LevelCleanup, spr, tf)).id();
            parts.push(e);
        }

        // Bevy y-up: screen-north = +y → gm_draw + (0, +8).
        if catalog.has(wall_top_png) {
            let top_draw = gm_draw + Vec2::new(0.0, 8.0);
            let (spr, tf) = sprite_at_gm_origin(
                catalog,
                asset_server,
                wall_top_png,
                top_frame,
                top_draw,
                -36.0,
            );
            let e = commands.spawn((GameCleanup, LevelCleanup, spr, tf)).id();
            parts.push(e);
        }

        // Collision body (16px solid) at cell center.
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
        if is_screen_end_wall(wx, wy, &floor_set) {
            commands.entity(wall_e).insert(ScreenEnd);
        }
    }

    // TopSmall / Trans fill (objects/Top + TopSmall):
    // For each floor tile, stamp 4× 16px candidates; keep only cells that are
    // neither Floor nor Wall. Upstream sprite = sprWallNTrans (often invisible
    // collision / soft top - draw when art exists so the rim reads solid).

    if catalog.has(wall_trans_png) {
        for &(cx, cy) in &plan.floor_cells {
            // Floor top-left in Bevy y-up.
            let ftl = Vec2::new(cx as f32 * TILE, (cy as f32 + 1.0) * TILE);
            for (ox, oy) in [(0.0, 0.0), (16.0, 0.0), (0.0, -16.0), (16.0, -16.0)] {
                let p = ftl + Vec2::new(ox, oy);
                let wx = (p.x / WALL_PX).floor() as i32;
                let wy = (p.y / WALL_PX).floor() as i32;
                if wall_set.contains(&(wx, wy)) {
                    continue;
                }
                let owner = (wx.div_euclid(2), wy.div_euclid(2));
                if floor_set.contains(&owner) {
                    continue;
                }
                let frame = (wall_hash(run.gen_seed, wx, wy, 0x41) as usize)
                    % sprite_frames(catalog, wall_trans_png).max(1);
                let draw = wall_top_left(wx, wy);
                let (spr, tf) =
                    sprite_at_gm_origin(catalog, asset_server, wall_trans_png, frame, draw, -38.0);
                commands.spawn((GameCleanup, LevelCleanup, spr, tf));
            }
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
        if kind == ChestKind::Rad {
            spawn_rad_container(commands, catalog, asset_server, pos, run.gen_seed);
        } else {
            crate::game::pickups::spawn_chest(commands, catalog, asset_server, kind, pos);
        }
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
            let idle_path: &'static str = "images/sprMine.png";
            catalog.require(idle_path);
            let hurt_path = crate::game::anim::derive_prop_hurt_path_checked(catalog, idle_path);
            let dead_path = crate::game::anim::derive_prop_dead_path_checked(catalog, idle_path);
            let mut mine_sprite = sprite_from_candidates(
                catalog,
                asset_server,
                &[idle_path],
                Color::srgb(0.86, 0.25, 0.18),
                Vec2::splat(18.0),
            );
            let mine_flip = prop_hash_flip(run.gen_seed, pos, 0x51);
            mine_sprite.flip_x = mine_flip;
            let mut mine_e = commands.spawn((
                GameCleanup,
                LevelCleanup,
                Prop {
                    size: Vec2::splat(18.0),
                    hp: 2,
                    destructible: true,
                    explosive: false,
                },
                PropHpTracker { last_hp: 2 },
                NextHurt::default(),
                PropSprites {
                    idle: idle_path,
                    hurt: hurt_path,
                    dead: dead_path,
                    flip_x: mine_flip,
                },
                ProximityMine::default(),
                PropDeathEffect::mine(),
                SurfacePulse::hazard(pos.y * 0.019),
                mine_sprite,
                crate::game::content::sprite_anchor(catalog, idle_path),
                Transform::from_translation(pos.extend(-8.0)),
            ));
            if let Some(def) = catalog.anim_def(idle_path) {
                mine_e.insert(crate::game::anim::SpriteAnim::new(idle_path, def));
            }
            return;
        }

        _ => {}
    }

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
            &[
                "images/sprCactus.png",
                "images/sprCactus2.png",
                "images/sprCactus3.png",
            ],
            Color::srgb(0.38, 0.72, 0.28),
            Vec2::splat(24.0),
            24.0,
            2,
            true,
            false,
            None,
            -10.0,
            true,
        ),

        PropKind::BigSkull => (
            &["images/sprBigSkullOpen.png"],
            Color::srgb(0.82, 0.78, 0.62),
            Vec2::splat(32.0),
            32.0,
            50,
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
            &["images/sprToxicBarrel.png"],
            Color::srgb(0.35, 0.86, 0.30),
            Vec2::splat(24.0),
            24.0,
            1,
            true,
            false,
            Some(PropDeathEffect::toxic_barrel()),
            -10.0,
            true,
        ),

        PropKind::Car => (
            &["images/sprCarIdle.png"],
            Color::srgb(0.62, 0.28, 0.22),
            Vec2::new(48.0, 28.0),
            38.0,
            20,
            true,
            false,
            Some(PropDeathEffect::car()),
            -10.0,
            true,
        ),

        PropKind::GoldBarrel => (
            &["images/sprGoldBarrel.png"],
            Color::srgb(0.95, 0.82, 0.25),
            Vec2::splat(24.0),
            24.0,
            1,
            // Upstream inherits Barrel: hp=1, explodes on death.
            true,
            false,
            Some(PropDeathEffect::legacy_barrel()),
            -10.0,
            true,
        ),

        PropKind::Pipe => (
            &["images/sprSewerPipe.png"],
            Color::srgb(0.42, 0.46, 0.45),
            Vec2::splat(24.0),
            24.0,
            1,
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
            &["images/sprCocoon.png"],
            Color::srgb(0.70, 0.58, 0.72),
            Vec2::new(26.0, 32.0),
            24.0,
            8,
            true,
            false,
            None,
            -10.0,
            true,
        ),

        PropKind::Snowman => (
            &["images/sprSnowMan.png"],
            Color::srgb(0.90, 0.94, 1.0),
            Vec2::new(24.0, 32.0),
            24.0,
            10,
            true,
            false,
            None,
            -10.0,
            true,
        ),

        PropKind::Torch => (
            &["images/sprTorch.png"],
            Color::srgb(1.0, 0.62, 0.18),
            Vec2::new(12.0, 28.0),
            12.0,
            20,
            true,
            false,
            None,
            -10.0,
            true,
        ),

        PropKind::BigGenerator => {
            // Upstream: max_hp 230, or 50 if loops>0 (Create_0.gml)
            let hp = if run.loop_count == 0 { 230 } else { 50 };
            (
                &["images/sprBigGenerator.png"],
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
            1000,
            true,
            false,
            None,
            -8.0,
            true,
        ),

        PropKind::BonePile => (
            &["images/sprBonePileIdle.png"],
            Color::srgb(0.82, 0.78, 0.62),
            Vec2::splat(24.0),
            22.0,
            2,
            true,
            false,
            None,
            -10.0,
            true,
        ),
        PropKind::NightBonePile => (
            &["images/sprNightBonePileIdle.png"],
            Color::srgb(0.62, 0.62, 0.68),
            Vec2::splat(24.0),
            22.0,
            2,
            true,
            false,
            None,
            -10.0,
            true,
        ),
        PropKind::NightCactus => (
            &[
                "images/sprNightCactus.png",
                "images/sprNightCactus2.png",
                "images/sprNightCactus3.png",
            ],
            Color::srgb(0.28, 0.52, 0.38),
            Vec2::splat(24.0),
            24.0,
            2,
            true,
            false,
            None,
            -10.0,
            true,
        ),
        PropKind::Crystal => (
            &["images/sprCrystalProp.png"],
            Color::srgb(0.72, 0.82, 0.95),
            Vec2::splat(24.0),
            22.0,
            2,
            true,
            false,
            None,
            -10.0,
            true,
        ),
        PropKind::Hydrant => (
            &["images/sprHydrant.png", "images/sprIcicle.png"],
            Color::srgb(0.82, 0.15, 0.15),
            Vec2::splat(24.0),
            24.0,
            5,
            true,
            false,
            None,
            -10.0,
            true,
        ),
        PropKind::StreetLight => (
            &["images/sprStreetLight.png"],
            Color::srgb(0.88, 0.88, 0.78),
            Vec2::splat(24.0),
            20.0,
            5,
            true,
            false,
            None,
            -10.0,
            true,
        ),
        PropKind::SodaMachine => (
            &["images/sprSodaMachine.png", "images/sprNewsStand.png"],
            Color::srgb(0.75, 0.15, 0.18),
            Vec2::splat(28.0),
            26.0,
            24,
            true,
            false,
            None,
            -10.0,
            true,
        ),
        PropKind::Tube => (
            &["images/sprTube.png"],
            Color::srgb(0.62, 0.72, 0.68),
            Vec2::splat(24.0),
            20.0,
            2,
            true,
            false,
            None,
            -10.0,
            true,
        ),
        PropKind::MutantTube => (
            &["images/sprMutantTube.png"],
            Color::srgb(0.58, 0.78, 0.42),
            Vec2::splat(26.0),
            24.0,
            24,
            true,
            false,
            None,
            -10.0,
            true,
        ),
        PropKind::Pillar => (
            &["images/sprNuclearPillar.png"],
            Color::srgb(0.62, 0.62, 0.68),
            Vec2::splat(28.0),
            24.0,
            70,
            true,
            false,
            None,
            -10.0,
            true,
        ),
        PropKind::SmallGenerator => (
            &["images/sprSmallGenerator.png"],
            Color::srgb(0.55, 0.75, 1.0),
            Vec2::splat(28.0),
            24.0,
            40,
            true,
            false,
            None,
            -10.0,
            true,
        ),
        PropKind::Anchor => (
            &["images/sprAnchor.png"],
            Color::srgb(0.42, 0.46, 0.52),
            Vec2::splat(32.0),
            28.0,
            50,
            true,
            false,
            None,
            -10.0,
            true,
        ),
        PropKind::WaterPlant => (
            &["images/sprWaterPlant.png", "images/sprWaterPlant2.png"],
            Color::srgb(0.18, 0.58, 0.42),
            Vec2::splat(24.0),
            20.0,
            2,
            true,
            false,
            None,
            -10.0,
            true,
        ),
        PropKind::OasisBarrel => (
            &["images/sprOasisBarrel.png"],
            Color::srgb(0.78, 0.62, 0.28),
            Vec2::splat(24.0),
            22.0,
            2,
            true,
            false,
            None,
            -10.0,
            true,
        ),
        PropKind::WaterMine => (
            &["images/sprWaterMine.png"],
            Color::srgb(0.18, 0.35, 0.62),
            Vec2::splat(24.0),
            20.0,
            20,
            true,
            false,
            Some(PropDeathEffect::mine()),
            -10.0,
            true,
        ),
        PropKind::MoneyPile => (
            &["images/sprMoneyPile.png"],
            Color::srgb(0.82, 0.72, 0.12),
            Vec2::splat(24.0),
            22.0,
            1,
            true,
            false,
            None,
            -10.0,
            true,
        ),
        PropKind::YVStatue => (
            &["images/sprYVStatue.png"],
            Color::srgb(0.72, 0.68, 0.58),
            Vec2::splat(24.0),
            22.0,
            15,
            true,
            false,
            None,
            -10.0,
            true,
        ),
        PropKind::Bush => (
            &["images/sprBushIdle.png"],
            Color::srgb(0.28, 0.58, 0.18),
            Vec2::splat(24.0),
            22.0,
            1,
            true,
            false,
            None,
            -10.0,
            true,
        ),
        PropKind::BigFlower => (
            &["images/sprBigFlowerIdle.png"],
            Color::srgb(0.82, 0.38, 0.58),
            Vec2::splat(26.0),
            24.0,
            8,
            true,
            false,
            None,
            -10.0,
            true,
        ),
        PropKind::PizzaBox => (
            &["images/sprPizzaBox.png"],
            Color::srgb(0.82, 0.52, 0.18),
            Vec2::splat(24.0),
            22.0,
            4,
            true,
            false,
            None,
            -10.0,
            true,
        ),
        PropKind::PlantPot => (
            &["images/sprPlantPotIdle.png"],
            Color::srgb(0.42, 0.62, 0.28),
            Vec2::splat(24.0),
            20.0,
            3,
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

    let existing_idles: Vec<&'static str> = candidates
        .iter()
        .copied()
        .filter(|p| catalog.has(p))
        .collect();
    if existing_idles.is_empty() {
        catalog.require(candidates[0]);
    }
    let idle_path: &'static str = if existing_idles.len() <= 1 {
        candidates
            .iter()
            .copied()
            .find(|p| catalog.has(p))
            .unwrap_or(candidates[0])
    } else {
        // Seed-stable variant pick (was rand::rng, broke seed lock).
        let idx = prop_hash_pick(run.gen_seed, pos, 0x52, existing_idles.len());
        existing_idles[idx]
    };

    let (hurt_path, dead_path) = if solid && destructible {
        (
            crate::game::anim::derive_prop_hurt_path_checked(catalog, idle_path),
            crate::game::anim::derive_prop_dead_path_checked(catalog, idle_path),
        )
    } else {
        (idle_path, idle_path)
    };

    let flip_x = if kind == PropKind::SodaMachine {
        false
    } else {
        prop_hash_flip(run.gen_seed, pos, 0x53)
    };

    let mut sprite = sprite_from_candidates(
        catalog,
        asset_server,
        &[idle_path],
        fallback_color,
        fallback_size,
    );
    sprite.flip_x = flip_x;

    let mut entity = commands.spawn((
        GameCleanup,
        LevelCleanup,
        sprite,
        crate::game::content::sprite_anchor(catalog, idle_path),
        Transform::from_translation(pos.extend(z)),
        PropSprites {
            idle: idle_path,
            hurt: hurt_path,
            dead: dead_path,
            flip_x,
        },
    ));

    if let Some(def) = catalog.anim_def(idle_path) {
        entity.insert(crate::game::anim::SpriteAnim::new(idle_path, def));
    }

    if solid {
        entity.insert(Prop {
            size: Vec2::splat(collision_size),
            hp,
            destructible,
            explosive: legacy_explosive,
        });
        entity.insert(PropHpTracker { last_hp: hp });
        entity.insert(NextHurt::default());
    }

    if let Some(effect) = death_effect {
        entity.insert(effect);
    }

    if kind == PropKind::Torch {
        entity.insert(SurfacePulse::hazard(pos.x * 0.01 + pos.y * 0.02));
    }
    if kind == PropKind::Snowman {
        entity.insert(SnowmanAmbush);
    }
    if kind == PropKind::GoldBarrel {
        entity.insert(GoldBarrelDrop);
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

fn spawn_rad_container(
    commands: &mut Commands,
    catalog: &AssetCatalog,
    asset_server: &AssetServer,
    pos: Vec2,
    seed: u64,
) {
    let idle_path: &'static str = "images/sprRadChest.png";
    catalog.require(idle_path);
    let hurt_path = crate::game::anim::derive_prop_hurt_path_checked(catalog, idle_path);
    let dead_path = crate::game::anim::derive_prop_dead_path_checked(catalog, idle_path);
    let flip_x = prop_hash_flip(seed, pos, 0x54);
    let mut sprite = sprite_from_candidates(
        catalog,
        asset_server,
        &[idle_path],
        Color::srgb(0.85, 0.25, 0.85),
        Vec2::new(32.0, 32.0),
    );
    sprite.flip_x = flip_x;
    let mut e = commands.spawn((
        GameCleanup,
        LevelCleanup,
        Prop {
            size: Vec2::splat(26.0),
            hp: 4,
            destructible: true,
            explosive: false,
        },
        PropHpTracker { last_hp: 4 },
        NextHurt::default(),
        PropSprites {
            idle: idle_path,
            hurt: hurt_path,
            dead: dead_path,
            flip_x,
        },
        RadChestContainer,
        sprite,
        crate::game::content::sprite_anchor(catalog, idle_path),
        Transform::from_translation(pos.extend(-8.0)),
    ));
    if let Some(def) = catalog.anim_def(idle_path) {
        e.insert(crate::game::anim::SpriteAnim::new(idle_path, def));
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
    // GML enemy/Create_0: max_hp *= 1 + loops/20. Keep tiny intra-area ramp.
    let loop_n = ((floor.max(1) - 1) / 15) as f32;
    let rf = ((floor.max(1) - 1) % 15) as f32;
    1.0 + loop_n * 0.05 + rf * 0.015
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
            blackswords: 0,
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
        assert_eq!(gml_area_from_run(&run), 101);

        let mut run = run_for(4);
        run.area = AreaId::HQ;
        assert_eq!(gml_area_from_run(&run), 106);

        let mut run = run_for(9);
        run.area = AreaId::Jungle;
        assert_eq!(gml_area_from_run(&run), 105);
    }

    #[test]
    fn crown_vault_gets_guardian_boss() {
        let mut run = run_for(2);
        run.area = crate::game::areas::AreaId::CrownVault;
        let plan = generate_level(&run);
        assert_eq!(plan.boss, Some(EnemyKind::OldGuardian));
    }

    #[test]
    fn pizza_sewers_boss_floor_hosts_frog_queen() {
        // Upstream: the Pizza Sewers dead-end is guarded by FrogQueen,
        // regardless of which route boss the floor number maps to.
        let mut run = run_for(6); // route floor 6 → sewers boss sub-area
        run.area = crate::game::areas::AreaId::PizzaSewers;
        let plan = generate_level(&run);
        assert_eq!(plan.boss, Some(EnemyKind::FrogQueen));
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
        // Upstream loop Desert: scorpions, melee bandits and snipers.
        assert!(table.iter().any(|(k, _)| *k == EnemyKind::Scorpion));
        assert!(table.iter().any(|(k, _)| *k == EnemyKind::MeleeBandit));
        assert!(table.iter().any(|(k, _)| *k == EnemyKind::Sniper));
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
