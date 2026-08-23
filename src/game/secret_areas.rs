//! Secret-area transition state machine: trigger detection (destroyed
//! entrances, Oasis eligibility, cursed weapons, HQ loops), a queued-target
//! resource, and the shared floor-advance path used by portal_enter.

use bevy::prelude::*;

use crate::game::areas::{AreaId, area_for_floor, route_coordinates};
use crate::game::components::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SecretTarget {
    Oasis,
    PizzaSewers,
    YvMansion,
    CursedCaves,
    Jungle,
    Vault,
    CrownVault,
    Hq,
}

impl SecretTarget {
    pub fn area(self) -> AreaId {
        match self {
            SecretTarget::Oasis => AreaId::Oasis,
            SecretTarget::PizzaSewers => AreaId::PizzaSewers,
            SecretTarget::YvMansion => AreaId::City,
            SecretTarget::CursedCaves => AreaId::CursedCaves,
            SecretTarget::Jungle => AreaId::Jungle,
            SecretTarget::Vault => AreaId::Vault,
            SecretTarget::CrownVault => AreaId::CrownVault,
            SecretTarget::Hq => AreaId::HQ,
        }
    }

    pub fn display(self) -> (u32, u32) {
        match self {
            SecretTarget::Oasis => (1, 5),
            SecretTarget::PizzaSewers => (2, 5),
            SecretTarget::YvMansion => (3, 5),
            SecretTarget::CursedCaves => (4, 5),
            SecretTarget::Jungle => (5, 5),
            SecretTarget::Vault => (0, 1),
            SecretTarget::CrownVault => (0, 2),
            SecretTarget::Hq => (0, 3),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            SecretTarget::Oasis => "OASIS",
            SecretTarget::PizzaSewers => "PIZZA SEWERS",
            SecretTarget::YvMansion => "Y.V. MANSION",
            SecretTarget::CursedCaves => "CURSED CAVES",
            SecretTarget::Jungle => "JUNGLE",
            SecretTarget::Vault => "VAULT",
            SecretTarget::CrownVault => "CROWN VAULT",
            SecretTarget::Hq => "I.D.P.D. HQ",
        }
    }

    pub fn return_floor(self, current_floor: u32) -> u32 {
        match self {
            // Oasis/Pizza both return to Scrapyards 3-1 in this route model.
            SecretTarget::Oasis | SecretTarget::PizzaSewers => 5,

            // Mansion exits to 3-3 if entered from Scrapyards.
            SecretTarget::YvMansion => 7,

            // Cursed Caves returns to Frozen City.
            SecretTarget::CursedCaves => 9,

            // Jungle returns to Labs.
            SecretTarget::Jungle => 12,

            // Vault/CrownVault/HQ continue to the next ordinary floor.
            SecretTarget::Vault | SecretTarget::CrownVault | SecretTarget::Hq => {
                current_floor.saturating_add(1)
            }
        }
    }
}

#[derive(Resource, Debug, Clone)]
pub struct SecretTriggers {
    queued: Option<SecretTarget>,
    pub last_secret: Option<SecretTarget>,
    pub oasis_eligible: bool,
    pub damage_taken_this_floor: bool,
}

impl Default for SecretTriggers {
    fn default() -> Self {
        Self {
            queued: None,
            last_secret: None,
            oasis_eligible: true,
            damage_taken_this_floor: false,
        }
    }
}

impl SecretTriggers {
    pub fn queue(&mut self, target: SecretTarget) {
        // Priority: vault-style hard transitions should override soft transitions.
        let replace = match (self.queued, target) {
            (None, _) => true,
            (Some(SecretTarget::Oasis), _) => true,
            (Some(SecretTarget::PizzaSewers), SecretTarget::Vault | SecretTarget::CrownVault) => {
                true
            }
            (Some(SecretTarget::YvMansion), SecretTarget::Vault | SecretTarget::CrownVault) => true,
            (Some(SecretTarget::CursedCaves), SecretTarget::Vault | SecretTarget::CrownVault) => {
                true
            }
            (Some(SecretTarget::Jungle), SecretTarget::Vault | SecretTarget::CrownVault) => true,
            _ => false,
        };

        if replace {
            self.queued = Some(target);
        }
    }

    pub fn take_queued(&mut self) -> Option<SecretTarget> {
        self.queued.take()
    }

    pub fn queued(&self) -> Option<SecretTarget> {
        self.queued
    }

    pub fn reset_floor_flags(&mut self) {
        self.oasis_eligible = true;
        self.damage_taken_this_floor = false;
    }

    pub fn mark_damage_taken(&mut self) {
        self.damage_taken_this_floor = true;
        self.oasis_eligible = false;
    }
}

pub fn is_secret_area(area: AreaId) -> bool {
    matches!(
        area,
        AreaId::Oasis
            | AreaId::PizzaSewers
            | AreaId::CursedCaves
            | AreaId::Jungle
            | AreaId::Vault
            | AreaId::CrownVault
            | AreaId::HQ
            | AreaId::City
    )
}

pub fn target_for_secret_area(area: AreaId) -> Option<SecretTarget> {
    match area {
        AreaId::Oasis => Some(SecretTarget::Oasis),
        AreaId::PizzaSewers => Some(SecretTarget::PizzaSewers),
        AreaId::City => Some(SecretTarget::YvMansion),
        AreaId::CursedCaves => Some(SecretTarget::CursedCaves),
        AreaId::Jungle => Some(SecretTarget::Jungle),
        AreaId::Vault => Some(SecretTarget::Vault),
        AreaId::CrownVault => Some(SecretTarget::CrownVault),
        AreaId::HQ => Some(SecretTarget::Hq),
        _ => None,
    }
}

/// Consume any queued secret and move `run` onto that secret area; otherwise,
/// if we are currently inside a secret, return to its route exit; otherwise,
/// advance one ordinary floor. Returns the secret entered, if any.
pub fn apply_secret_transition(
    run: &mut Run,
    triggers: &mut SecretTriggers,
) -> Option<SecretTarget> {
    if let Some(target) = triggers.take_queued() {
        let (world, floor_in_area) = target.display();
        run.area = target.area();
        run.world = world;
        run.floor_in_area = floor_in_area;
        run.portal_open = false;
        run.gen_seed = rand::random::<u64>();
        triggers.last_secret = Some(target);
        triggers.reset_floor_flags();
        return Some(target);
    }

    let previous_secret = target_for_secret_area(run.area);

    if let Some(previous_secret) = previous_secret {
        let floor = previous_secret.return_floor(run.floor);
        run.floor = floor.max(1);
        run.loop_count = (run.floor - 1) / 15;
        let (world, floor_in_area) = route_coordinates(run.floor);
        run.world = world;
        run.floor_in_area = floor_in_area;
        run.area = area_for_floor(run.floor, run.loop_count);
        run.portal_open = false;
        run.gen_seed = rand::random::<u64>();
        triggers.reset_floor_flags();
        return None;
    }

    run.floor += 1;
    run.loop_count = (run.floor - 1) / 15;
    let (world, floor_in_area) = route_coordinates(run.floor);
    run.world = world;
    run.floor_in_area = floor_in_area;
    run.area = area_for_floor(run.floor, run.loop_count);
    run.portal_open = false;
    run.gen_seed = rand::random::<u64>();
    triggers.reset_floor_flags();
    None
}

/// Oasis: reach Desert 1-3 at full HP without taking damage this floor.
pub fn detect_oasis_eligibility(
    run: Res<Run>,
    mut triggers: ResMut<SecretTriggers>,
    player_q: Query<&Health, With<Player>>,
) {
    if run.area != AreaId::Desert || run.floor != 3 {
        return;
    }

    let Ok(health) = player_q.single() else {
        return;
    };

    if health.hp >= health.max && triggers.oasis_eligible && !triggers.damage_taken_this_floor {
        triggers.queue(SecretTarget::Oasis);
    }
}

/// Cursed Caves: carry an endgame/cursed-tier weapon (interim hook on generated
/// IDs 100..=127 until a proper cursed flag exists in weapon data) in the
/// Crystal Caves.
pub fn detect_cursed_caves(
    run: Res<Run>,
    mut triggers: ResMut<SecretTriggers>,
    player_q: Query<&Inventory, With<Player>>,
) {
    if run.area != AreaId::CrystalCaves {
        return;
    }

    let Ok(inv) = player_q.single() else {
        return;
    };

    let current = inv.weapons[inv.current];
    if (100..=127).contains(&current.0) {
        triggers.queue(SecretTarget::CursedCaves);
    }
}

/// I.D.P.D. HQ: late loops in the Labs, deterministic per-floor pseudo roll so
/// manual repros and tests stay stable for a given seed/floor pair.
pub fn detect_hq(run: Res<Run>, mut triggers: ResMut<SecretTriggers>) {
    if run.area == AreaId::Labs && run.loop_count >= 2 {
        let roll =
            ((run.gen_seed ^ run.floor as u64).wrapping_mul(6364136223846793005) >> 56) as u8;
        if roll < 18 {
            triggers.queue(SecretTarget::Hq);
        }
    }
}

pub fn secret_debug_toast(triggers: Res<SecretTriggers>, mut toast: ResMut<Toast>) {
    if let Some(target) = triggers.queued() {
        if toast.timer.is_finished() {
            toast.show(&format!("SECRET ROUTE: {}", target.name()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_priority_replaces_oasis_with_vault() {
        let mut t = SecretTriggers::default();
        t.queue(SecretTarget::Oasis);
        t.queue(SecretTarget::CrownVault);
        assert_eq!(t.queued(), Some(SecretTarget::CrownVault));
    }

    #[test]
    fn oasis_and_pizza_return_to_scrapyards() {
        assert_eq!(SecretTarget::Oasis.return_floor(3), 5);
        assert_eq!(SecretTarget::PizzaSewers.return_floor(4), 5);
    }

    #[test]
    fn vault_continues_from_current_floor() {
        assert_eq!(SecretTarget::Vault.return_floor(8), 9);
        assert_eq!(SecretTarget::CrownVault.return_floor(12), 13);
    }

    #[test]
    fn secret_area_mapping_is_bidirectional() {
        assert_eq!(SecretTarget::Oasis.area(), AreaId::Oasis);
        assert_eq!(
            target_for_secret_area(AreaId::Oasis),
            Some(SecretTarget::Oasis)
        );
        assert_eq!(
            target_for_secret_area(AreaId::CrownVault),
            Some(SecretTarget::CrownVault)
        );
    }

    #[test]
    fn normal_transition_advances_floor() {
        let mut run = Run::default();
        let mut triggers = SecretTriggers::default();
        let secret = apply_secret_transition(&mut run, &mut triggers);
        assert_eq!(secret, None);
        assert_eq!(run.floor, 2);
        assert_eq!(run.area, AreaId::Desert);
        assert_eq!(run.world, 1);
        assert_eq!(run.floor_in_area, 2);
    }

    #[test]
    fn queued_secret_does_not_advance_floor() {
        let mut run = Run::default();
        run.floor = 3;
        run.world = 1;
        run.floor_in_area = 3;
        run.area = AreaId::Desert;

        let mut triggers = SecretTriggers::default();
        triggers.queue(SecretTarget::Oasis);

        let secret = apply_secret_transition(&mut run, &mut triggers);
        assert_eq!(secret, Some(SecretTarget::Oasis));
        assert_eq!(run.floor, 3);
        assert_eq!(run.area, AreaId::Oasis);
        assert_eq!(run.world, 1);
        assert_eq!(run.floor_in_area, 5);
    }

    #[test]
    fn leaving_oasis_returns_to_scrapyards() {
        let mut run = Run::default();
        run.floor = 3;
        run.area = AreaId::Oasis;
        run.world = 1;
        run.floor_in_area = 5;

        let mut triggers = SecretTriggers::default();
        let secret = apply_secret_transition(&mut run, &mut triggers);

        assert_eq!(secret, None);
        assert_eq!(run.floor, 5);
        assert_eq!(run.area, AreaId::Scrapyards);
        assert_eq!(run.world, 3);
        assert_eq!(run.floor_in_area, 1);
    }
}
