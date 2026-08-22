//! Game components, resources, and cleanup markers.

use bevy::prelude::*;

use crate::game::content::*;
use serde::{Deserialize, Serialize};

pub const ARENA_W: f32 = 2560.0;
pub const ARENA_H: f32 = 1664.0;
pub const WALL_THICK: f32 = 60.0;
pub const PLAYER_RADIUS: f32 = 8.0; // upstream mskPlayer is a 16x16 mask
pub const PLAYER_ACCEL: f32 = 1500.0;
pub const PLAYER_FRICTION: f32 = 0.82;

/// 32px NT floor grid — walkable cells only (like Floor / Wall solids).
pub const TILE: f32 = 32.0;

#[derive(Resource, Default, Clone)]
pub struct FloorMask {
    pub cells: std::collections::HashSet<(i32, i32)>,
    pub cols: i32,
    pub rows: i32,
}

impl FloorMask {
    /// Cells are origin-centered tile coords matching LevelPlan exactly.
    pub fn world_to_cell(&self, p: Vec2) -> (i32, i32) {
        ((p.x / TILE).floor() as i32, (p.y / TILE).floor() as i32)
    }

    pub fn cell_center(&self, c: (i32, i32)) -> Vec2 {
        Vec2::new(
            c.0 as f32 * TILE + TILE * 0.5,
            c.1 as f32 * TILE + TILE * 0.5,
        )
    }

    pub fn is_walkable(&self, p: Vec2) -> bool {
        self.cells.contains(&self.world_to_cell(p))
    }

    /// Push a circle back onto floor tiles (NT-style floor solids).
    pub fn resolve_circle(&self, pos: &mut Vec3, radius: f32) {
        let p = pos.truncate();
        if self.is_walkable(p) {
            return;
        }
        // Snap toward nearest walkable cell center.
        let mut best = None::<(f32, Vec2)>;
        let (cx, cy) = self.world_to_cell(p);
        for dy in -3..=3 {
            for dx in -3..=3 {
                let c = (cx + dx, cy + dy);
                if !self.cells.contains(&c) {
                    continue;
                }
                let center = self.cell_center(c);
                let d = center.distance_squared(p);
                if best.map(|(bd, _)| d < bd).unwrap_or(true) {
                    best = Some((d, center));
                }
            }
        }
        if let Some((_, center)) = best {
            let dir = (center - p).normalize_or_zero();
            let dist = center.distance(p);
            let push = (dist - (TILE * 0.35 - radius)).max(0.0);
            pos.x += dir.x * push;
            pos.y += dir.y * push;
        }
    }

    pub fn random_floor_pos(&self, rng: &mut impl rand::RngExt, min_from_origin: f32) -> Vec2 {
        if self.cells.is_empty() {
            return Vec2::ZERO;
        }
        for _ in 0..80 {
            let idx = rng.random_range(0..self.cells.len());
            let c = *self.cells.iter().nth(idx).unwrap();
            let p = self.cell_center(c);
            if p.length() >= min_from_origin {
                return p;
            }
        }
        self.cell_center(*self.cells.iter().next().unwrap())
    }
}

/// Solid wall tile (collides like Prop, not destructible).
#[derive(Component)]
pub struct WallTile;

#[derive(Resource, Default)]
pub struct Score(pub u32);

/// Set when in-memory save data diverges from disk; a throttled system flushes it
/// so the high score isn't written on every kill.
#[derive(Resource, Default)]
pub struct SaveDirty(pub bool);

#[derive(Resource)]
pub struct Run {
    pub floor: u32,
    pub world: u32,
    pub area: crate::game::areas::AreaId,
    pub loop_count: u32,
    pub floor_in_area: u32,
    pub gen_seed: u64,
    pub portal_open: bool,
    pub game_over: bool,
    pub total_kills: u32,
}

impl Default for Run {
    fn default() -> Self {
        Self {
            floor: 1,
            world: 1,
            area: crate::game::areas::AreaId::Desert,
            loop_count: 0,
            floor_in_area: 1,
            gen_seed: 0,
            portal_open: false,
            game_over: false,
            total_kills: 0,
        }
    }
}

#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct SelectedCharacter(pub RaceId);

impl Default for SelectedCharacter {
    fn default() -> Self {
        Self(RaceId::Fish)
    }
}

/// Active level-up: the game is paused and the player must pick one.
#[derive(Resource)]
pub struct PendingMutation {
    pub choices: Vec<MutationId>,
}

/// Set by the UI (Repose buttons) when the player clicks a mutation choice.
#[derive(Resource, Default)]
pub struct MutationChoice(pub Option<usize>);

#[derive(Resource)]
pub struct Toast {
    pub text: String,
    pub timer: Timer,
}

impl Default for Toast {
    fn default() -> Self {
        Self {
            text: String::new(),
            timer: Timer::from_seconds(0.0, TimerMode::Once),
        }
    }
}

/// Scarier Face: new enemies spawn with 80% HP.
#[derive(Resource, Default)]
pub struct ScarierFace(pub bool);

/// Euphoria: enemy projectiles spawn slower.
#[derive(Resource, Default)]
pub struct Euphoria(pub bool);

/// Open Mind: extra chests spawn with each level clear.
#[derive(Resource, Default)]
pub struct OpenMind(pub bool);

/// Heavy Heart: enemies can drop weapons.
#[derive(Resource, Default)]
pub struct HeavyHeart(pub bool);

/// Marker for everything that belongs to the whole run (despawned when leaving
/// the InGame state).
#[derive(Component)]
pub struct GameCleanup;

/// Marker for everything that belongs to the current floor (despawned when
/// taking a portal to the next floor).
#[derive(Component)]
pub struct LevelCleanup;

#[derive(Component)]
pub struct Player {
    pub speed: f32,
    pub accel: f32,
    pub friction: f32,
    pub speed_mult: f32,
    pub rads: u32,
    pub level: u32,
    pub next_level_rads: u32,
    pub pickup_range: f32,
    pub fire_rate_mult: f32,
    pub spread_mult: f32,
    pub knockback_mult: f32,
    pub melee_range_mult: f32,
    pub drop_mult: f32,
    pub medkit_mult: f32,
    pub boiling_veins: bool,
    pub veins_threshold: i32,
    pub bloodlust: bool,
    pub lucky_shot: bool,
    pub gamma_guts: bool,
    pub back_muscle: u32,
    pub stress: bool,
    pub sharp_teeth: bool,
    pub strong_spirit_ready: bool,
    pub last_wish_used: bool,
    pub chain_explosions: bool,
    pub shield_on_hit: bool,
    pub ability: AbilityKind,
    pub ability_cooldown: Timer,
    pub mutations: Vec<MutationId>,
}

#[derive(Component)]
pub struct AimDir(pub Vec2);

#[derive(Component)]
pub struct Velocity(pub Vec2);

#[derive(Component)]
pub struct Health {
    pub hp: i32,
    pub max: i32,
    pub invuln: Timer,
}

#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Team {
    Player,
    Enemy,
}

#[derive(Component)]
pub struct Hitbox {
    pub radius: f32,
}

#[derive(Component)]
pub struct FireCooldown {
    pub timer: Timer,
    pub burst_left: usize,
    pub burst_timer: Timer,
}

pub const MAX_WEAPON_SLOTS: usize = 3;
pub const MAX_AMMO_TYPES: usize = 6;

#[derive(Component, Clone, Debug)]
pub struct Inventory {
    pub weapons: [WeaponId; MAX_WEAPON_SLOTS],
    pub weapon_slots: usize, // 2 normally, 3 for Cuz
    pub current: usize,
    pub ammo: [i32; MAX_AMMO_TYPES],
}

impl Inventory {
    pub fn ammo_mut(&mut self, kind: AmmoKind) -> &mut i32 {
        let idx = match kind {
            AmmoKind::None => 0,
            AmmoKind::Bullets => 1,
            AmmoKind::Shells => 2,
            AmmoKind::Bolts => 3,
            AmmoKind::Explosives => 4,
            AmmoKind::Energy => 5,
        };
        &mut self.ammo[idx]
    }

    pub fn ammo_of(&self, kind: AmmoKind) -> i32 {
        match kind {
            AmmoKind::None => self.ammo[0],
            AmmoKind::Bullets => self.ammo[1],
            AmmoKind::Shells => self.ammo[2],
            AmmoKind::Bolts => self.ammo[3],
            AmmoKind::Explosives => self.ammo[4],
            AmmoKind::Energy => self.ammo[5],
        }
    }
}

#[derive(Component, Clone, Debug)]
pub struct RaceState {
    pub race: RaceId,
    pub skin: SkinLetter,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct RaceLoadout {
    pub unlocked: bool,
    pub unlocked_skins: [bool; 4],
    pub stored_weapon: WeaponId,
    pub start_weapon: WeaponId,
    pub start_crown: u8,
}

impl Default for WeaponId {
    fn default() -> Self {
        Self(0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HitId {
    Weapon(WeaponId),
    Explosion(WeaponId),
    Enemy(u16),
    Contact,
    Fire,
    Toxic,
    Trap,
    Crown,
    Other(u16),
}

#[derive(Clone, Copy, Debug)]
pub struct DamageSource {
    pub owner: Entity,
    pub team: Team,
    pub hit_id: HitId,
}

#[derive(Component)]
pub struct Projectile {
    pub damage: i32,
    pub life: Timer,
    pub radius: f32,
    pub knockback: f32,
    pub explosive: bool,
    pub source: Option<DamageSource>,
}

#[derive(Component, Clone, Copy)]
pub struct Enemy {
    pub kind: EnemyKind,
    pub score: u32,
    pub touch_damage: i32,
    pub rad_drop: usize,
    pub drop_chance: usize,
    pub weapon_chance: usize,
}

#[derive(Component)]
pub struct EnemyBrain {
    pub speed: f32,
    pub accel: f32,
    pub preferred_range: f32,
    pub shoot_range: f32,
    pub attack: Timer,
    pub burst_left: usize,
    pub burst_timer: Timer,
    pub telegraph: f32,
    pub dash: f32,
    pub dash_cooldown: Timer,
    pub strafe_dir: f32,
    pub strafe_timer: Timer,
    pub melee: Timer,
}

#[derive(Component)]
pub struct Pickup {
    pub kind: PickupKind,
}

#[derive(Clone, Copy)]
pub enum PickupKind {
    Rad(u32),
    Medkit(i32),
    Ammo(AmmoKind, i32),
    Weapon(WeaponId),
    Chest(ChestKind),
}

/// Upstream chest flavours (scrPopChests).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ChestKind {
    Weapon,
    Ammo,
    Rad,
}

// Back-compat for legacy WeaponKind pickups
impl From<WeaponKind> for PickupKind {
    fn from(k: WeaponKind) -> Self {
        PickupKind::Weapon(k.into())
    }
}

#[derive(Component)]
pub struct Portal;

#[derive(Component)]
pub struct Prop {
    pub size: Vec2,
    pub hp: i32,
    pub destructible: bool,
    pub explosive: bool,
}

/// Visual for a melee swing (fades out quickly).
#[derive(Component)]
pub struct SwingFx {
    pub timer: Timer,
}

/// Fish's Flip: short dash with i-frames.
#[derive(Component)]
pub struct Dash {
    pub timer: Timer,
    pub dir: Vec2,
}

/// Crystal's Shield: absorbs enemy projectiles while active.
#[derive(Component)]
pub struct Shield {
    pub timer: Timer,
}

/// Eyes' Telekinesis: pulls pickups toward the player while active.
#[derive(Component)]
pub struct Telekinesis {
    pub timer: Timer,
}
