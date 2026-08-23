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

/// Level-10 ultra choice. Uses the same `MutationChoice` click/number input.
#[derive(Resource)]
pub struct PendingUltra {
    pub choices: Vec<UltraMutationId>,
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
    pub headless_ready: bool,
    pub free_ammo: bool,
    pub crown: CrownKind,
    pub bolt_marrow: bool,
    pub hammerhead: bool,
    pub laser_brain: bool,
    pub recycle_gland: bool,
    pub shotgun_shoulders: bool,
    pub throne_butt: bool,
    /// Eyes' Projectile Style ultra: enemy projectiles slow further.
    pub euphoria: bool,
    /// Patience is a one-time skip; next mutation roll gets four choices.
    pub patience_bonus: bool,
    pub patience_used: bool,
    /// Chosen level-10 ultra, if any.
    pub ultra: Option<UltraMutationId>,
    /// Generic damage scaling granted by some ultras.
    pub ultra_damage_mult: f32,
    /// Generic ability scaling used by Throne Butt / ultras.
    pub ultra_ability_mult: f32,
    pub mutations: Vec<MutationId>,
}

impl Default for Player {
    fn default() -> Self {
        Self {
            speed: 240.0,
            accel: PLAYER_ACCEL,
            friction: PLAYER_FRICTION,
            speed_mult: 1.0,
            rads: 0,
            level: 1,
            next_level_rads: 60,
            pickup_range: 95.0,
            fire_rate_mult: 1.0,
            spread_mult: 1.0,
            knockback_mult: 1.0,
            melee_range_mult: 1.0,
            drop_mult: 0.0,
            medkit_mult: 1.0,
            boiling_veins: false,
            veins_threshold: 4,
            bloodlust: false,
            lucky_shot: false,
            gamma_guts: false,
            back_muscle: 0,
            stress: false,
            sharp_teeth: false,
            strong_spirit_ready: false,
            last_wish_used: false,
            chain_explosions: false,
            shield_on_hit: false,
            ability: AbilityKind::Flip,
            ability_cooldown: Timer::from_seconds(0.0, TimerMode::Once),
            headless_ready: false,
            free_ammo: false,
            crown: CrownKind::None,
            bolt_marrow: false,
            hammerhead: false,
            laser_brain: false,
            recycle_gland: false,
            shotgun_shoulders: false,
            throne_butt: false,
            euphoria: false,
            patience_bonus: false,
            patience_used: false,
            ultra: None,
            ultra_damage_mult: 1.0,
            ultra_ability_mult: 1.0,
            mutations: Vec::new(),
        }
    }
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

/// Runtime state for the equipped crown's per-floor behaviors.
#[derive(Component)]
pub struct CrownState {
    pub crown: CrownKind,
    pub life_timer: Timer,
    pub love_timer: Timer,
    pub protection_ready: bool,
    pub destiny_ready: bool,
    pub curses_timer: Timer,
}

impl CrownState {
    pub fn new(crown: CrownKind) -> Self {
        let mut life_timer = Timer::from_seconds(2.0, TimerMode::Repeating);
        life_timer.reset();

        let mut love_timer = Timer::from_seconds(35.0, TimerMode::Repeating);
        love_timer.reset();

        let mut curses_timer = Timer::from_seconds(14.0, TimerMode::Repeating);
        curses_timer.reset();

        Self {
            crown,
            life_timer,
            love_timer,
            protection_ready: true,
            destiny_ready: true,
            curses_timer,
        }
    }
}

/// Emitted once per floor (initial spawn and every portal transition) so
/// floor-start effects (crown bonuses, etc.) can react.
#[derive(bevy::ecs::message::Message, Clone, Copy, Debug)]
pub struct FloorStarted {
    pub floor: u32,
    pub area: crate::game::areas::AreaId,
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

#[derive(Component, Clone, Copy, Debug)]
pub struct BouncesLeft(pub u8);

#[derive(Component, Clone, Copy, Debug)]
pub struct PiercesLeft(pub u8);

/// Entities already damaged by this piercing projectile this lifetime.
#[derive(Component, Default, Debug, Clone)]
pub struct ProjectileHitSet(pub Vec<Entity>);

/// Marks a hazard cloud as coming from a race ability (no Team component).
/// Weapon clouds always carry `Team` instead.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct AbilityHazard;

#[derive(Component, Clone, Copy, Debug)]
pub struct SpawnHazardOnDeath(pub HazardDef);

#[derive(Component, Clone, Copy, Debug)]
pub struct SplitOnDeath(pub SplitDef);

// --- Projectile archetypes (see projectile_archetypes.rs) -------------------

/// Smart / seeker projectiles steer toward the nearest enemy.
#[derive(Component, Clone, Copy, Debug)]
pub struct Homing {
    pub turn_rate: f32,
    pub acquire_range: f32,
}

/// Sticky grenade: freezes on first solid contact, explodes when life ends.
#[derive(Component, Clone, Copy, Debug)]
pub struct Sticky {
    pub armed: bool,
    pub stuck_to: Option<Entity>,
    pub offset: Vec2,
}

impl Default for Sticky {
    fn default() -> Self {
        Self {
            armed: false,
            stuck_to: None,
            offset: Vec2::ZERO,
        }
    }
}

/// Lightning jumps between distinct targets instead of piercing linearly.
#[derive(Component, Clone, Copy, Debug)]
pub struct ChainLightning {
    pub jumps_left: u8,
    pub range: f32,
    pub falloff: f32,
}

/// Projectile payload that deploys an autonomous turret on death.
#[derive(Component, Clone, Copy, Debug)]
pub struct DeploysSentry {
    pub life: f32,
    pub fire_interval: f32,
    pub range: f32,
    pub projectile_speed: f32,
    pub projectile_damage: i32,
}

/// Overrides the default explosion radius for this projectile's death.
#[derive(Component, Clone, Copy, Debug)]
pub struct CustomExplosion {
    pub radius: f32,
}

/// When the weapon's ammo pool is empty, firing spends HP instead.
#[derive(Component, Clone, Copy, Debug)]
pub struct BloodAmmo {
    pub hp_cost: i32,
}

/// On projectile death, spawn a weapon pickup.
/// `weapon = None` rolls a random weapon.
#[derive(Component, Clone, Copy, Debug)]
pub struct SpawnsWeaponPickup {
    pub weapon: Option<WeaponId>,
}

/// Plasma secondary shrapnel ring emitted on death.
#[derive(Component, Clone, Copy, Debug)]
pub struct PlasmaBurst {
    pub pellets: u8,
    pub speed: f32,
    pub damage: i32,
    pub lifetime: f32,
    pub radius: f32,
    pub knockback: f32,
    pub color: Color,
    pub size: Vec2,
}

/// Persistent line-damage segment (Ion / Laser Cannon).
#[derive(Component, Clone, Debug)]
pub struct Beam {
    pub team: Team,
    pub dir: Vec2,
    pub length: f32,
    pub width: f32,
    pub damage: i32,
    pub knockback: f32,
    pub timer: Timer,
    pub tick: Timer,
}

// --- IDPD raids / vans (see idpd.rs) ----------------------------------------

#[derive(Component)]
pub struct IdpdVanBrain {
    pub deploy_timer: Timer,
    pub charges_left: u8,
}

impl Default for IdpdVanBrain {
    fn default() -> Self {
        Self {
            deploy_timer: Timer::from_seconds(2.2, TimerMode::Repeating),
            charges_left: 4,
        }
    }
}

/// Marker for shield units (frontal advance, reduced strafe).
#[derive(Component)]
pub struct IdpdShieldUnit;

/// Loop-only raid director state.
#[derive(Resource)]
pub struct IdpdRaidState {
    pub cooldown: Timer,
    pub warning: Timer,
    pub pending_wave: Option<RaidWave>,
    pub wave_index: u32,
    pub kills_checkpoint: u32,
}

impl Default for IdpdRaidState {
    fn default() -> Self {
        Self {
            cooldown: Timer::from_seconds(20.0, TimerMode::Repeating),
            warning: Timer::from_seconds(1.25, TimerMode::Once),
            pending_wave: None,
            wave_index: 0,
            kills_checkpoint: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RaidWave {
    Light,
    Medium,
    Heavy,
    VanDrop,
}

// --- Loop transition / Throne II interlude (see loop_transition.rs) ---------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CampfirePhase {
    Sitting,
    Rising,
    SpawnThroneII,
    Done,
}

#[derive(Component)]
pub struct CampfireState {
    pub phase: CampfirePhase,
    pub timer: Timer,
    pub spawned_throne_ii: bool,
}

impl CampfireState {
    pub fn new() -> Self {
        Self {
            phase: CampfirePhase::Sitting,
            timer: Timer::from_seconds(3.5, TimerMode::Once),
            spawned_throne_ii: false,
        }
    }

    pub fn set_phase(&mut self, phase: CampfirePhase, seconds: f32) {
        self.phase = phase;
        self.timer = Timer::from_seconds(seconds.max(0.01), TimerMode::Once);
        self.timer.reset();
    }
}

#[derive(Component)]
pub struct CampfireProp;

/// Tracks the Throne -> campfire -> Throne II -> loop-portal sequence.
#[derive(Resource, Clone, Debug, Default)]
pub struct LoopTransition {
    pub campfire_active: bool,
    pub throne_ii_alive: bool,
    pub loop_ready: bool,
    pub last_completed_loop: u32,
}

impl LoopTransition {
    pub fn blocks_portal(&self) -> bool {
        self.campfire_active || self.throne_ii_alive
    }

    pub fn begin_campfire(&mut self) {
        self.campfire_active = true;
        self.throne_ii_alive = false;
        self.loop_ready = false;
    }

    pub fn throne_ii_spawned(&mut self) {
        self.campfire_active = false;
        self.throne_ii_alive = true;
        self.loop_ready = false;
    }

    pub fn throne_ii_defeated(&mut self) {
        self.campfire_active = false;
        self.throne_ii_alive = false;
        self.loop_ready = true;
    }

    pub fn consume_loop_ready(&mut self) -> bool {
        let ready = self.loop_ready;
        self.loop_ready = false;
        ready
    }
}

/// Deferred enemy spawn for systems that do not hold asset handles.
#[derive(Component, Clone, Copy, Debug)]
pub struct PendingEnemySpawn {
    pub kind: EnemyKind,
    pub pos: Vec2,
    pub difficulty: f32,
}

/// Laser crystal orbiting a Hyper Crystal core.
#[derive(Component)]
pub struct HyperOrbitCrystal {
    pub owner: Entity,
    pub angle: f32,
    pub radius: f32,
    pub angular_speed: f32,
    pub fire_timer: Timer,
}

/// Autonomous friendly turret deployed by the Sentry Gun pod.
#[derive(Component, Clone, Debug)]
pub struct SentryTurret {
    pub life: Timer,
    pub fire: Timer,
    pub range: f32,
    pub projectile_speed: f32,
    pub projectile_damage: i32,
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
    pub dash: f32,
    pub strafe_dir: f32,
    pub strafe_timer: Timer,
    pub melee: Timer,
}

// --- Boss AI (see boss_ai.rs) -----------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BossPhase {
    Idle,
    Telegraph,
    Charging,
    Cooldown,
    Jumping,
    Landing,
    Radial,
    Beam,
    Enraged,
}

/// Phase state machine driving the bespoke boss behaviors in `boss_ai`.
#[derive(Component)]
pub struct BossBrain {
    pub phase: BossPhase,
    pub phase_timer: Timer,
    pub attack_timer: Timer,
    pub special_timer: Timer,
    pub pattern_index: usize,
    pub home: Vec2,
    pub target: Vec2,
    pub enraged: bool,
}

impl BossBrain {
    pub fn new(kind: EnemyKind, spawn: Vec2) -> Self {
        let (attack, special) = match kind {
            EnemyKind::BigBandit => (1.15, 2.8),
            EnemyKind::BigDog => (0.8, 2.2),
            EnemyKind::LilHunter => (0.55, 1.7),
            EnemyKind::Throne => (0.7, 2.5),
            EnemyKind::ThroneII => (0.85, 3.4),
            EnemyKind::Hyper => (1.1, 4.0),
            _ => (1.2, 3.0),
        };

        Self {
            phase: BossPhase::Idle,
            phase_timer: Timer::from_seconds(0.1, TimerMode::Once),
            attack_timer: Timer::from_seconds(attack, TimerMode::Repeating),
            special_timer: Timer::from_seconds(special, TimerMode::Repeating),
            pattern_index: 0,
            home: spawn,
            target: spawn,
            enraged: false,
        }
    }

    pub fn set_phase(&mut self, phase: BossPhase, seconds: f32) {
        self.phase = phase;
        self.phase_timer = Timer::from_seconds(seconds.max(0.01), TimerMode::Once);
        self.phase_timer.reset();
    }
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

/// A destructible prop that leads to a secret area when destroyed.
#[derive(Component, Clone, Copy, Debug)]
pub struct SecretEntrance {
    pub target: crate::game::secret_areas::SecretTarget,
}

#[derive(Component, Clone, Copy, Debug, Default)]
pub struct ManholeCover;

#[derive(Component, Clone, Copy, Debug, Default)]
pub struct ProtoStatue;

#[derive(Component, Clone, Copy, Debug, Default)]
pub struct GoldCar;

#[derive(Component, Clone, Copy, Debug, Default)]
pub struct BloodFlower;

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

/// Y.V. Pop Pop — next successful shot fires a second volley.
#[derive(Component)]
pub struct PopPopCharges(pub u8);

/// Plant snare zone — slows enemies while alive.
#[derive(Component)]
pub struct SnareZone {
    pub timer: Timer,
    pub radius: f32,
    pub slow: f32,
}

/// Temporary enemy slow applied by Snare / toxic.
#[derive(Component)]
pub struct Slowed {
    pub timer: Timer,
    pub factor: f32,
}

/// Rebel ally that shoots toward nearest enemy.
#[derive(Component)]
pub struct Ally {
    pub life: Timer,
    pub shoot: Timer,
}

/// Rogue portal strike telegraphed blast.
#[derive(Component)]
pub struct PortalStrike {
    pub timer: Timer,
    pub radius: f32,
    pub damage: i32,
}

/// Frog / Horror residual hazard cloud
#[derive(Component)]
pub struct HazardCloud {
    pub kind: HazardKind,
    pub radius: f32,
    pub damage: i32,
    pub timer: Timer,
    pub tick: Timer,
}

/// Chicken headless grace (one lethal soak per floor).
#[derive(Component, Default)]
pub struct HeadlessReady(pub bool);
