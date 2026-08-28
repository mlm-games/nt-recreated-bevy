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

impl AreaId {
    /// Route area for a one-based global floor (loop derived from the floor).
    pub fn from_route_floor(floor: u32) -> AreaId {
        let loop_count = (floor.max(1) - 1) / 15;
        area_for_floor(floor, loop_count)
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

/// Normal route for a one-based global floor over the supported 15-floor
/// NT cycle: 1-1..1-3 Desert, 2-1 Sewers, 3-1..3-3 Scrapyards, 4-1 Caves,
/// 5-1..5-3 Frozen City, 6-1 Labs, 7-1..7-3 Palace (Throne).
///
/// Secret areas (Vault/CrownVault/Oasis) are NEVER produced here - they are
/// reached only through explicit transition conditions upstream.
pub fn area_for_floor(floor: u32, _loop_count: u32) -> AreaId {
    let route_floor = (floor.max(1) - 1) % 15 + 1;
    match route_floor {
        1..=3 => AreaId::Desert,
        4 => AreaId::Sewers,
        5..=7 => AreaId::Scrapyards,
        8 => AreaId::CrystalCaves,
        9..=11 => AreaId::FrozenCity,
        12 => AreaId::Labs,
        13..=15 => AreaId::Palace,
        _ => unreachable!(),
    }
}

/// (world, floor-in-world) display coordinates for a global floor.
/// Worlds 1..7 correspond to Desert/Sewers/Scrapyards/Caves/Frozen/Labs/Palace
/// with variable floors per world; looping re-enters at 1-1.
pub fn route_coordinates(floor: u32) -> (u32, u32) {
    let route_floor = (floor.max(1) - 1) % 15 + 1;
    match route_floor {
        1..=3 => (1, route_floor),
        4 => (2, 1),
        5..=7 => (3, route_floor - 4),
        8 => (4, 1),
        9..=11 => (5, route_floor - 8),
        12 => (6, 1),
        13..=15 => (7, route_floor - 12),
        _ => unreachable!(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normal_route_matches_world_order() {
        assert_eq!(area_for_floor(1, 0), AreaId::Desert);
        assert_eq!(area_for_floor(3, 0), AreaId::Desert);
        assert_eq!(area_for_floor(4, 0), AreaId::Sewers);
        assert_eq!(area_for_floor(5, 0), AreaId::Scrapyards);
        assert_eq!(area_for_floor(8, 0), AreaId::CrystalCaves);
        assert_eq!(area_for_floor(9, 0), AreaId::FrozenCity);
        assert_eq!(area_for_floor(12, 0), AreaId::Labs);
        assert_eq!(area_for_floor(13, 0), AreaId::Palace);
        assert_eq!(area_for_floor(15, 0), AreaId::Palace);
        assert_eq!(area_for_floor(16, 1), AreaId::Desert);
    }

    #[test]
    fn route_repeats_after_throne() {
        assert_eq!(route_coordinates(4), (2, 1));
        assert_eq!(route_coordinates(8), (4, 1));
        assert_eq!(route_coordinates(12), (6, 1));
        assert_eq!(route_coordinates(13), (7, 1));
        assert_eq!(route_coordinates(16), (1, 1));
        assert_eq!(route_coordinates(31), (1, 1));
    }

    #[test]
    fn secret_areas_are_not_inserted_automatically() {
        for floor in 1..=30 {
            assert_ne!(area_for_floor(floor, floor / 15), AreaId::Vault);
            assert_ne!(area_for_floor(floor, floor / 15), AreaId::CrownVault);
            assert_ne!(area_for_floor(floor, floor / 15), AreaId::Oasis);
        }
    }
}
