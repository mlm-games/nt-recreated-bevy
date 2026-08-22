use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum AreaId {
    Desert = 0,
    Sewers = 1,
    PizzaSewers = 2,
    Scrapyards = 3,
    CursedCaves = 4,
    CrystalCaves = 5,
    FrozenCity = 6,
    City = 7,
    Jungle = 8,
    Labs = 9,
    Palace = 10,
    HQ = 11,
    Oasis = 12,
    Vault = 13,
    CrownVault = 14,
    Campfire = 15,
    Loop = 16,
}

impl Default for AreaId {
    fn default() -> Self {
        Self::Desert
    }
}

#[derive(Clone, Debug)]
pub struct AreaTransition {
    pub from: AreaId,
    pub to: AreaId,
    pub condition: TransitionCondition,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransitionCondition {
    Always,
    Loop(u32),
    Crown,
    Secret,
}

/// Normal route for a one-based global floor over the supported 7-floor
/// cycle: 1-1..1-3 Desert, 2-1 Sewers, 3-1..3-3 Scrapyards, 4-1 Caves,
/// 5-1..5-3 Frozen City, 6-1 Labs, 7-1 Palace (Throne).
///
/// Secret areas (Vault/CrownVault/Oasis) are NEVER produced here — they are
/// reached only through explicit transition conditions upstream.
pub fn area_for_floor(floor: u32, _loop_count: u32) -> AreaId {
    let route_floor = ((floor.max(1) - 1) % 7) + 1;

    match route_floor {
        1..=3 => AreaId::Desert,
        4 | 5 => AreaId::Sewers,
        6 => AreaId::Scrapyards,
        7 => AreaId::Palace,
        _ => unreachable!(),
    }
}

/// (world, floor-in-world) display coordinates for a global floor.
pub fn route_coordinates(floor: u32) -> (u32, u32) {
    let route_floor = ((floor.max(1) - 1) % 7) + 1;
    let world = (floor.max(1) - 1) / 7 + 1;
    (world, route_floor)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normal_route_matches_world_order() {
        assert_eq!(area_for_floor(1, 0), AreaId::Desert);
        assert_eq!(area_for_floor(3, 0), AreaId::Desert);
        assert_eq!(area_for_floor(4, 0), AreaId::Sewers);
        assert_eq!(area_for_floor(6, 0), AreaId::Scrapyards);
        assert_eq!(area_for_floor(7, 0), AreaId::Palace);
        assert_eq!(area_for_floor(8, 1), AreaId::Desert);
    }

    #[test]
    fn route_repeats_after_throne() {
        assert_eq!(route_coordinates(8), (2, 1));
        assert_eq!(route_coordinates(15), (3, 1));
        assert_eq!(route_coordinates(21), (3, 7));
    }

    #[test]
    fn secret_areas_are_not_inserted_automatically() {
        for floor in 1..=30 {
            assert_ne!(area_for_floor(floor, floor / 7), AreaId::Vault);
            assert_ne!(area_for_floor(floor, floor / 7), AreaId::CrownVault);
            assert_ne!(area_for_floor(floor, floor / 7), AreaId::Oasis);
        }
    }
}
