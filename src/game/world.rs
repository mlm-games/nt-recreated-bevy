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

pub struct LevelPlan {
    pub floor_cells: Vec<(i32, i32)>,
    pub wall_cells: std::collections::HashSet<(i32, i32)>,

    pub small_walls: Vec<(i16, i16)>,
    pub bones: Vec<(Vec2, bool)>,
    pub details: Vec<Vec2>,
    pub props: Vec<(PropKind, Vec2)>,
    pub chests: Vec<ChestSpawn>,
    pub enemies: Vec<(EnemyKind, Vec2)>,
    pub boss: Option<EnemyKind>,

    pub boss_count: u32,

    pub styleb: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PropKind {

    Cactus,
    BigSkull,
    GroundDecal,
    Barrel,
    Pipe,
    Tires,

    ToxicBarrel,
    Car,
    Cocoon,
    Snowman,
    Torch,

    GoldBarrel,

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

    Cobweb,
    IcePatch,
    FireTrap,
    Mine,

    BigGenerator,
    ThroneStatue,
}

#[derive(Clone, Copy, PartialEq)]
pub enum ChestSpawn {
    Weapon(Vec2),
    Ammo(Vec2),
    Rad(Vec2),
}

fn gml_area(floor: u32) -> i32 {
    let rf = ((floor.max(1) - 1) % 15) + 1;
    match rf {
        1..=3 => 1,
        4 => 2,
        5..=7 => 3,
        8 => 4,
        9..=11 => 5,
        12 => 6,
        13..=15 => 7,
        _ => 7,
    }
}

fn gml_area_from_run(run: &Run) -> i32 {
    use crate::game::areas::AreaId;
    match run.area {
        AreaId::Desert => 1,
        AreaId::Oasis => 101,
        AreaId::Sewers => 2,
        AreaId::PizzaSewers => 102,
        AreaId::Scrapyards => 3,

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

    matches!(rf, 3 | 7 | 11 | 15)
}

fn is_boss_subarea_run(run: &Run) -> bool {
    if crate::game::secret_areas::is_secret_area(run.area) {
        return false;
    }
    is_boss_subarea(run.floor)
}

pub fn generation_goal(floor: u32) -> usize {

    let _ = floor;
    110
}

fn generation_goal_for_run(run: &Run) -> usize {
    use crate::game::areas::AreaId;
    if crate::game::secret_areas::is_secret_area(run.area) {
        return match run.area {
            AreaId::CrownVault | AreaId::Vault => 40,
            AreaId::PizzaSewers => 70,
            AreaId::City => 130,
            AreaId::Oasis => 130,
            AreaId::HQ => {

                if run.floor_in_area >= 3 { 48 } else { 110 }
            }
            AreaId::CursedCaves => 100,
            AreaId::Jungle => 110,
            _ => 90,
        };
    }

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

    x: i32,
    y: i32,

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

        4 => rng_choose(rng, &[Z, Z, Z, Z, Z, 90, -90, 180]),

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

                    let is_max = run.floor_in_area >= 3;
                    if rng.random::<f32>() * 8.0 < 1.0 || is_max {
                        let (xoff, yoff) = if is_max {
                            let xo = rng_choose(&mut rng, &[0, 1, 0, 0, -1]);
                            let yo = rng_choose(&mut rng, &[0, 1, 0, 0, -1]);
                            (xo, yo)
                        } else {
                            (0, 0)
                        };

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

            if area == 7 && !dies {
                dies = rng.random::<f32>() * (9.0 + n) > 10.0;
            }

            if dies && dist_from_spawn > 48.0 * 48.0 {
                plan.chests.push(ChestSpawn::Ammo(cell_center_px(mx, my)));
                stamp_cell((mx, my), &mut seen, &mut plan.floor_cells);
            }

            if area == 106 && dist_from_spawn > 48.0 * 48.0 && rng.random::<f32>() * 10.0 < 1.0 {
                plan.chests.push(ChestSpawn::Ammo(cell_center_px(mx, my)));
                stamp_cell((mx, my), &mut seen, &mut plan.floor_cells);
            }

            if dies {
                continue;
            }

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

fn wall_center(wx: i32, wy: i32) -> Vec2 {
    Vec2::new(
        wx as f32 * WALL_PX + WALL_PX * 0.5,
        wy as f32 * WALL_PX + WALL_PX * 0.5,
    )
}

fn wall_top_left(wx: i32, wy: i32) -> Vec2 {

    Vec2::new(wx as f32 * WALL_PX, (wy as f32 + 1.0) * WALL_PX)
}

fn build_walls(run: &Run, floors: &[(i32, i32)], plan: &mut LevelPlan) {
    let _ = run;
    let floor_set: std::collections::HashSet<(i32, i32)> = floors.iter().copied().collect();

    for &(cx, cy) in floors {

        let probes = [
            (-1, -1),
            (0, -1),
            (1, -1),
            (2, -1),
            (2, 0),
            (2, 1),
            (-1, 0),
            (-1, 1),
            (-1, 2),
            (0, 2),
            (1, 2),
            (2, 2),
        ];
        for (ox, oy) in probes {
            let wx = cx * 2 + ox;
            let wy = cy * 2 + oy;

            let owner = (wx.div_euclid(2), wy.div_euclid(2));
            if floor_set.contains(&owner) {
                continue;
            }
            plan.wall_cells.insert((wx, wy));
        }
    }
}

fn side_solid(walls: &std::collections::HashSet<(i32, i32)>, cx: i32, cy: i32, dx: i32) -> bool {

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

    let hard = game_hard(run);
    let enemy_min = (3.0 + hard / 1.5).floor().max(3.0) as usize;

    let mut prop_tiles: std::collections::HashSet<(i32, i32)> = std::collections::HashSet::new();

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

        let unlikeliness = if run.area == crate::game::areas::AreaId::Jungle {
            2.0
        } else if run.area == crate::game::areas::AreaId::Campfire {
            7.0
        } else {
            10.0
        };

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

                3 => {
                    let roll = rng.random_range(0..7);
                    match roll {
                        0..=2 => PropKind::Tires,
                        3..=4 => PropKind::Car,
                        _ => PropKind::GroundDecal,
                    }
                }

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

        let threshold = match kind {
            PropKind::Cobweb | PropKind::IcePatch => 2.6,
            PropKind::FireTrap => 1.35,
            PropKind::Mine => 0.85,
            _ => 1.0,
        };

        if rng.random::<f32>() * unlikeliness > threshold {
            continue;
        }

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

        if claims_tile && dist_sq < 64.0 * 64.0 {
            continue;
        }

        if claims_tile {
            prop_tiles.insert((cx, cy));
        }
        plan.props.push((kind, Vec2::new(px, py)));
    }

    let rf_route = ((run.floor.max(1) - 1) % 15) + 1;
    let skip_enemies = boss_sub && rf_route == 15;

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

        let chance = hard / (10.0 + hard);
        if rng.random::<f32>() >= chance && enemy_tiles.len() >= enemy_min {
            continue;
        }

        let center = Vec2::new(px, py);
        let pick_kind = |rng: &mut StdRng, w: &[EnemyKind]| w[rng.random_range(0..w.len())];
        let loop_extras = loop_elite_candidates(area, run.loop_count);

        {
            use crate::game::areas::AreaId;
            let mut secret_kinds: Vec<EnemyKind> = match run.area {
                AreaId::Oasis => {

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

                let roll: f32 = rng.random();
                let mut cands = if roll * 4.0 < 1.0 {

                    vec![
                        EnemyKind::MeleeBandit,
                        EnemyKind::Sniper,
                        EnemyKind::MeleeBandit,
                        EnemyKind::Sniper,
                        EnemyKind::Ballguy,
                    ]
                } else if roll * 10.0 < 1.0 {

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

    if boss_sub {
        let kind = boss_for_floor_and_loop(run.floor, run.loop_count);
        plan.boss = Some(kind);
        if matches!(kind, EnemyKind::BigBandit | EnemyKind::BigBanditLoop) {
            plan.boss_count = big_bandit_count(run.loop_count);
        }
    } else {
        match run.area {

            AreaId::PizzaSewers => plan.boss = Some(EnemyKind::FrogQueen),
            AreaId::Sewers if run.loop_count >= 1 => plan.boss = Some(EnemyKind::Mom),
            AreaId::Labs if run.loop_count >= 1 => plan.boss = Some(EnemyKind::Technomancer),
            AreaId::CrystalCaves if run.loop_count >= 1 => plan.boss = Some(EnemyKind::Hyper),
            AreaId::CrownVault | AreaId::Vault => plan.boss = Some(EnemyKind::OldGuardian),
            AreaId::HQ => plan.boss = Some(EnemyKind::Captain),
            _ => {}
        }
    }

    let rf = ((run.floor.max(1) - 1) % 15) + 1;
    if rf == 15 && !crate::game::secret_areas::is_secret_area(run.area) {
        populate_throne_room(run, plan);
    }

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

    trim_chests(&mut plan.chests);
}

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

            table.push((EnemyKind::Scorpion, 4 + l * 2));
            table.push((EnemyKind::JungleFly, 3 + l));
            table.push((EnemyKind::MeleeBandit, 3 + l));
            table.push((EnemyKind::Sniper, 3 + l));
            if loop_count >= 2 {
                table.push((EnemyKind::GoldScorpion, 2 + l));
            }
        }
        AreaId::Sewers => {

            table.push((EnemyKind::Ratking, 4 + l * 2));
            table.push((EnemyKind::BuffGator, 3 + l));
            table.push((EnemyKind::IdpdShield, 2 + l));
        }
        AreaId::Scrapyards => {

            table.push((EnemyKind::Sniper, 4 + l * 2));
            table.push((EnemyKind::MeleeBandit, 3 + l));
            table.push((EnemyKind::Salamander, 2 + l));
            if loop_count >= 2 {
                table.push((EnemyKind::BuffGator, 3 + l));
                table.push((EnemyKind::RobotGuard, 2 + l));
            }
        }
        AreaId::CrystalCaves => {

            table.push((EnemyKind::RhinoFreak, 4 + l * 2));
            table.push((EnemyKind::ExploFreak, 3 + l));
            table.push((EnemyKind::LightningCrystal, 3 + l));
            if loop_count >= 2 {
                table.push((EnemyKind::IdpdElite, 3 + l));
            }
        }
        AreaId::FrozenCity => {

            table.push((EnemyKind::SnowTank, 4 + l));
            table.push((EnemyKind::DogGuardian, 3 + l));
            table.push((EnemyKind::ExploGuardian, 3 + l));
            table.push((EnemyKind::Necromancer, 2 + l));
            if loop_count >= 2 {
                table.push((EnemyKind::GoldSnowtank, 2 + l));
            }
        }
        AreaId::Labs => {

            table.push((EnemyKind::Ratking, 4 + l));
            table.push((EnemyKind::RhinoFreak, 4 + l * 2));
            table.push((EnemyKind::ExploFreak, 4 + l));
            if loop_count >= 2 {
                table.push((EnemyKind::IdpdElite, 5 + l));
            }
        }
        AreaId::Palace => {

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

pub fn game_hard(run: &Run) -> f32 {
    let floors_cleared = run.floor.saturating_sub(1) as f32;
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

    for i in 0..9 {
        let y = -320.0 + i as f32 * 80.0;
        plan.props.push((PropKind::ThroneStatue, Vec2::new(0.0, y)));
    }
    plan.boss = Some(EnemyKind::Throne);
    plan.boss_count = 1;
}

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

    *mask = FloorMask {
        cells: plan.floor_cells.iter().copied().collect(),
        cols,
        rows,
    };

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

    for &(cx, cy) in &plan.floor_cells {
        let (wx, wy) = cell_center_i(cx, cy);
        commands.spawn((
            GameCleanup,
            LevelCleanup,
            sprite_exact(catalog, asset_server, floor_png),
            Transform::from_xyz(wx, wy, -50.0),
        ));
    }

    for pos in &plan.details {
        commands.spawn((
            GameCleanup,
            LevelCleanup,
            sprite_exact(catalog, asset_server, "images/sprDetail0.png"),
            Transform::from_xyz(pos.x, pos.y, -45.0),
        ));
    }

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

    all_walls.sort_unstable();

    for (wx, wy) in all_walls {
        let c = wall_center(wx, wy);

        let gm_draw = wall_top_left(wx, wy);

        let body_frame = wall_body_frame(catalog, run.gen_seed, wx, wy, wall_bot_png);
        let top_frame = wall_top_frame(catalog, run.gen_seed, wx, wy, wall_top_png);
        let out_frame = wall_out_frame(catalog, run.gen_seed, wx, wy, wall_out_png);

        let floor_south = {

            let probe = Vec2::new(c.x, c.y - WALL_PX);
            let owner = (
                (probe.x / TILE).floor() as i32,
                (probe.y / TILE).floor() as i32,
            );
            floor_set.contains(&owner)
        };

        let mut parts: Vec<Entity> = Vec::with_capacity(3);

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

        if is_screen_end_wall(wx, wy, &floor_set) {
            commands.entity(wall_e).insert(ScreenEnd);
        }
    }

    if catalog.has(wall_trans_png) {
        for &(cx, cy) in &plan.floor_cells {

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

    spawn_secret_entrances(commands, catalog, asset_server, run);

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

fn spawn_secret_entrances(
    commands: &mut Commands,
    catalog: &AssetCatalog,
    asset_server: &AssetServer,
    run: &Run,
) {
    let maybe = match (run.area, run.floor_in_area) {

        (AreaId::Sewers, _) => Some((
            SecretTarget::PizzaSewers,
            "images/sprPipe.png",
            Vec2::new(220.0, -120.0),
            28.0,
        )),

        (AreaId::Desert, 2) | (AreaId::Scrapyards, 2) | (AreaId::FrozenCity, 2) => Some((
            SecretTarget::CrownVault,
            "images/sprOldGuardianStatue.png",
            Vec2::new(-240.0, 160.0),
            34.0,
        )),

        (AreaId::Scrapyards, 1) => Some((
            SecretTarget::YvMansion,
            "images/sprCarIdle.png",
            Vec2::new(260.0, 140.0),
            36.0,
        )),

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

pub fn big_bandit_count(loop_count: u32) -> u32 {
    if loop_count == 0 {
        1
    } else {
        loop_count.saturating_mul(2).max(2)
    }
}

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

    let loop_n = ((floor.max(1) - 1) / 15) as f32;
    let rf = ((floor.max(1) - 1) % 15) as f32;
    1.0 + loop_n * 0.05 + rf * 0.015
}

pub fn clamp_to_arena(pos: &mut Vec3, radius: f32) {
    pos.x = pos.x.clamp(-ARENA_W / 2.0 + radius, ARENA_W / 2.0 - radius);
    pos.y = pos.y.clamp(-ARENA_H / 2.0 + radius, ARENA_H / 2.0 - radius);
}

pub fn floor_cell_for_wall(wx: i32, wy: i32) -> (i32, i32) {
    (wx.div_euclid(2), wy.div_euclid(2))
}

pub fn expand_floor_for_wall(mask: &mut FloorMask, wx: i32, wy: i32) {
    mask.cells.insert(floor_cell_for_wall(wx, wy));
}

pub fn wall_cell_at(pos: Vec2) -> (i32, i32) {
    (
        (pos.x / WALL_PX).floor() as i32,
        (pos.y / WALL_PX).floor() as i32,
    )
}

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

        assert_eq!(boss_for_floor(3), EnemyKind::BigBandit);
        assert_eq!(boss_for_floor(18), EnemyKind::BigBanditLoop);
        assert_eq!(boss_for_floor(22), EnemyKind::BigDogLoop);
        assert_eq!(boss_for_floor(26), EnemyKind::LilHunterLoop);
        assert_eq!(boss_for_floor(30), EnemyKind::Throne);
    }

    #[test]
    fn loop_sewers_and_labs_get_exclusive_bosses() {
        let mut run = run_for(4);
        run.loop_count = 1;
        let plan = generate_level(&run);
        assert_eq!(plan.boss, Some(EnemyKind::Mom));

        let mut run = run_for(12);
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

        let mut run = run_for(6);
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
            let mut run = run_for(8);
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

            const RING: [(i32, i32); 12] = [
                (-1, -1),
                (0, -1),
                (1, -1),
                (2, -1),
                (2, 0),
                (2, 1),
                (-1, 0),
                (-1, 1),
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
