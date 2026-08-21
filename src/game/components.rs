//! Game components, resources, and cleanup markers.

use bevy::prelude::*;

use crate::game::content::*;

pub const ARENA_W: f32 = 2000.0;
pub const ARENA_H: f32 = 1300.0;
pub const WALL_THICK: f32 = 60.0;
pub const PLAYER_RADIUS: f32 = 12.0;
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
    pub fn world_to_cell(&self, p: Vec2) -> (i32, i32) {
        let fx = (p.x / TILE + self.cols as f32 * 0.5).floor() as i32;
        let fy = (p.y / TILE + self.rows as f32 * 0.5).floor() as i32;
        (fx, fy)
    }

    pub fn cell_center(&self, c: (i32, i32)) -> Vec2 {
        Vec2::new(
            (c.0 as f32 - self.cols as f32 * 0.5 + 0.5) * TILE,
            (c.1 as f32 - self.rows as f32 * 0.5 + 0.5) * TILE,
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
            gen_seed: 0,
            portal_open: false,
            game_over: false,
            total_kills: 0,
        }
    }
}

#[derive(Resource)]
pub struct SelectedCharacter(pub CharacterId);

impl Default for SelectedCharacter {
    fn default() -> Self {
        Self(CharacterId::Fish)
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

#[derive(Component, Clone, Copy, PartialEq, Eq)]
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

#[derive(Component)]
pub struct Inventory {
    pub weapons: [WeaponKind; 2],
    pub current: usize,
    pub ammo: [i32; 4],
}

impl Inventory {
    pub fn ammo_mut(&mut self, kind: AmmoKind) -> &mut i32 {
        match kind {
            AmmoKind::Bullets => &mut self.ammo[0],
            AmmoKind::Shells => &mut self.ammo[1],
            AmmoKind::Bolts => &mut self.ammo[2],
            AmmoKind::Explosives => &mut self.ammo[3],
        }
    }
}

#[derive(Component)]
pub struct Projectile {
    pub damage: i32,
    pub life: Timer,
    pub radius: f32,
    pub knockback: f32,
    pub explosive: bool,
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
    Weapon(WeaponKind),
    Chest,
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
