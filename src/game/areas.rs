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

pub fn area_for_floor(floor: u32, loop_count: u32) -> AreaId {
    // Simplified mapping; upstream scrArea is far richer.
    // Generates placeholder that keeps floor/world progression compatible.
    match (floor % 7, loop_count) {
        (3, _) => AreaId::Vault,
        (0, _) if floor >= 7 => AreaId::Palace,
        (1, _) => AreaId::Desert,
        (2, _) => AreaId::Sewers,
        (4, _) => AreaId::CrystalCaves,
        (5, _) => AreaId::FrozenCity,
        _ => AreaId::Desert,
    }
}
