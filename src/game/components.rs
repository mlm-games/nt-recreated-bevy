use bevy::prelude::*;

use crate::game::content::*;
use serde::{Deserialize, Serialize};

pub const ARENA_W: f32 = 2560.0;
pub const ARENA_H: f32 = 1664.0;
pub const WALL_THICK: f32 = 60.0;
pub const PLAYER_RADIUS: f32 = 8.0;

// Base speed 4 px/frame = 120 px/s.
pub const PLAYER_BASE_SPEED: f32 = 120.0;

pub const PLAYER_ACCEL: f32 = 2700.0;

pub const PLAYER_FRICTION: f32 = 0.45;
pub const NT_CAM_SCALE: f32 = 0.45;

// Friction is subtractive per tick.
#[inline]
pub fn apply_gml_friction(vel: &mut Vec2, friction_f: f32, dt: f32) {

    let frames = dt * crate::app::NT_SIM_HZ as f32;
    let sp = vel.length();
    if sp > 0.0 {
        let nsp = (sp - friction_f * 30.0 * frames).max(0.0);
        *vel = if nsp == 0.0 {
            Vec2::ZERO
        } else {
            *vel * (nsp / sp)
        };
    }
}

// Scale impulse by 30, not dt.
#[inline]
pub fn gml_motion_add_clamp(vel: &mut Vec2, dir: Vec2, impulse_f: f32, cap_f: f32, dt: f32) {
    let frames = dt * crate::app::NT_SIM_HZ as f32;

    *vel += dir.normalize_or_zero() * (impulse_f * 30.0) * frames;
    let cap = cap_f * 30.0;
    if vel.length() > cap {
        *vel = vel.normalize() * cap;
    }
}

// Floor grid is 32 px tiles.
pub const TILE: f32 = 32.0;

#[derive(Resource, Default, Clone)]
pub struct FloorMask {
    pub cells: std::collections::HashSet<(i32, i32)>,
    pub cols: i32,
    pub rows: i32,
}

impl FloorMask {

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

    pub fn resolve_circle(&self, pos: &mut Vec3, radius: f32) {
        let p = pos.truncate();
        if self.is_walkable(p) {
            return;
        }

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

#[derive(Component)]
pub struct WallTile;

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct WallCell(pub i32, pub i32);

#[derive(Component, Clone, Debug, Default)]
pub struct WallVisuals {
    pub parts: Vec<Entity>,
}

#[derive(Component, Clone, Copy, Debug)]
pub struct PendingWallBreak {
    pub cell: (i32, i32),
    pub pos: Vec2,

    pub spawn_floor: bool,
}

#[derive(Resource, Debug)]
pub struct HammerheadBudget {
    pub remaining: u32,
}

impl Default for HammerheadBudget {
    fn default() -> Self {
        Self { remaining: 20 }
    }
}

#[derive(Resource, Default, Debug, Clone)]
pub struct LastDamageTaken {
    pub hit_id: Option<HitId>,
    pub enemy_kind: Option<EnemyKind>,
    pub source_name: String,
}

impl LastDamageTaken {
    pub fn note(&mut self, hit_id: Option<HitId>, enemy_kind: Option<EnemyKind>) {
        self.hit_id = hit_id;
        self.enemy_kind = enemy_kind;
        self.source_name = match (hit_id, enemy_kind) {
            (_, Some(kind)) => enemy_def(kind).name.to_ascii_uppercase(),
            (Some(HitId::Enemy(id)), None) => EnemyKind::from_u16(id)
                .map(|k| enemy_def(k).name.to_ascii_uppercase())
                .unwrap_or_else(|| "ENEMY".into()),
            (Some(HitId::Contact), _) => "CONTACT".into(),
            (Some(HitId::Toxic), _) => "TOXIC".into(),
            (Some(HitId::Fire), _) => "FIRE".into(),
            (Some(HitId::Trap), _) => "TRAP".into(),
            (Some(HitId::Explosion(_)), _) => "EXPLOSION".into(),
            (Some(HitId::Weapon(_)), _) => "BULLET".into(),
            (Some(HitId::Crown), _) => "CROWN".into(),
            (Some(HitId::Other(_)), _) => "???".into(),
            _ => "???".into(),
        };
    }

    pub fn note_from_source(&mut self, source: Option<&DamageSource>) {
        match source {
            Some(s) => self.note(Some(s.hit_id), s.enemy_kind),
            None => self.note(None, None),
        }
    }
}

#[derive(Component)]
pub struct BossIntro {
    pub timer: Timer,
}

#[derive(Resource, Default)]
pub struct Score(pub u32);

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

    pub blackswords: u32,
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
            blackswords: 0,
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

#[derive(Resource)]
pub struct PendingMutation {
    pub choices: Vec<MutationId>,
}

#[derive(Resource)]
pub struct PendingUltra {
    pub choices: Vec<UltraMutationId>,
}

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

#[derive(Resource, Default)]
pub struct ScarierFace(pub bool);

#[derive(Resource, Default)]
pub struct Euphoria(pub bool);

#[derive(Resource, Default)]
pub struct OpenMind(pub bool);

#[derive(Resource, Default)]
pub struct HeavyHeart(pub bool);

#[derive(Component)]
pub struct GameCleanup;

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
    pub accuracy: f32,
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
    pub strong_spirit_spent: bool,
    pub strong_spirit_area_cleared: bool,
    pub last_wish_used: bool,
    pub mutation_picks_owed: u32,
    pub ultra_pick_owed: bool,
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

    pub euphoria: bool,

    pub patience_bonus: bool,
    pub patience_used: bool,

    pub ultra: Option<UltraMutationId>,

    pub ultra_damage_mult: f32,

    pub ultra_ability_mult: f32,
    pub mutations: Vec<MutationId>,
}

impl Default for Player {
    fn default() -> Self {
        Self {
            speed: PLAYER_BASE_SPEED,
            accel: PLAYER_ACCEL,
            friction: PLAYER_FRICTION,
            speed_mult: 1.0,
            rads: 0,
            level: 1,
            next_level_rads: 60,
            pickup_range: 95.0,
            fire_rate_mult: 1.0,
            spread_mult: 1.0,
            accuracy: 1.0,
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
            strong_spirit_spent: false,
            strong_spirit_area_cleared: false,
            last_wish_used: false,
            mutation_picks_owed: 0,
            ultra_pick_owed: false,
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

impl Player {

    pub fn ammo_cap(&self, kind: AmmoKind) -> i32 {
        ammo_cap_with(self.back_muscle, kind)
    }

    pub fn try_recharge_strong_spirit(&mut self, health: &Health) {
        if self.strong_spirit_ready {
            return;
        }
        let has_ss = self.mutations.contains(&MutationId::StrongSpirit)
            || matches!(
                self.ultra,
                Some(UltraMutationId::MeltingDetachment | UltraMutationId::BigDogGuardian)
            );
        if !has_ss {
            return;
        }
        if self.strong_spirit_spent
            && self.strong_spirit_area_cleared
            && health.hp >= health.max
            && health.max > 1
        {
            self.strong_spirit_ready = true;
            self.strong_spirit_spent = false;
            self.strong_spirit_area_cleared = false;
        }
    }
}

pub fn ammo_cap_with(back_muscle: u32, kind: AmmoKind) -> i32 {
    let base = crate::game::content::ammo_max(kind);
    if back_muscle == 0 || kind == AmmoKind::None {
        return base;
    }
    base + match kind {
        AmmoKind::Bullets => 300 * back_muscle as i32,
        _ => 44 * back_muscle as i32,
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

#[derive(Component, Clone, Copy, Debug, Default)]
pub struct NextHurt(pub u64);

#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct CurrentFrame(pub u64);

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

    pub timer_b: Timer,
    pub burst_left_b: usize,
    pub burst_timer_b: Timer,
}

pub const MAX_WEAPON_SLOTS: usize = 3;
pub const MAX_AMMO_TYPES: usize = 6;

#[derive(Component, Clone, Debug)]
pub struct Inventory {
    pub weapons: [WeaponId; MAX_WEAPON_SLOTS],
    pub weapon_slots: usize,
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

#[derive(bevy::ecs::message::Message, Clone, Copy, Debug)]
pub struct FloorStarted {
    pub floor: u32,
    pub area: crate::game::areas::AreaId,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct RaceLoadout {
    pub unlocked: bool,
    pub unlocked_skins: [bool; 4],

    pub preferred_skin: u8,
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

impl HitId {
    #[inline]
    pub fn from_enemy_kind(kind: EnemyKind) -> Self {
        HitId::Enemy(kind as u16)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct DamageSource {
    pub owner: Entity,
    pub team: Team,
    pub hit_id: HitId,

    pub enemy_kind: Option<EnemyKind>,
}

impl DamageSource {
    pub fn enemy(owner: Entity, kind: EnemyKind) -> Self {
        Self {
            owner,
            team: Team::Enemy,
            hit_id: HitId::from_enemy_kind(kind),
            enemy_kind: Some(kind),
        }
    }

    pub fn player_weapon(owner: Entity, wep: WeaponId) -> Self {
        Self {
            owner,
            team: Team::Player,
            hit_id: HitId::Weapon(wep),
            enemy_kind: None,
        }
    }
}

#[derive(Component, Clone, Copy, Debug)]
pub struct ProjectileFriction(pub f32);

#[derive(Component, Debug)]
pub struct GrenadeFuse {
    pub smoke_armed: bool,
    pub friction_switched: bool,
    pub alarm1: Timer,
}

#[derive(Component, Debug)]
pub struct ShellBonus {
    pub timer: Timer,
    pub bonus: i32,
}

#[derive(Component, Clone, Copy, Debug)]
pub struct ShellWallBounce(pub f32);

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

#[derive(Component, Clone, Copy, Debug, Default)]
pub struct HitsAllTeams;

#[derive(Component, Debug)]
pub struct SpawnGrace(pub Timer);

#[derive(Component, Default, Debug, Clone)]
pub struct ProjectileHitSet(pub Vec<Entity>);

#[derive(Component, Clone, Copy, Debug, Default)]
pub struct AbilityHazard;

#[derive(Component, Clone, Copy, Debug)]
pub struct SpawnHazardOnDeath(pub HazardDef);

#[derive(Component, Clone, Copy, Debug)]
pub struct SplitOnDeath(pub SplitDef);

#[derive(Component, Clone, Copy, Debug)]
pub struct Homing {
    pub turn_rate: f32,
    pub acquire_range: f32,
}

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

#[derive(Component, Debug)]
pub struct FlameTrail {
    pub timer: Timer,
    pub spec: HazardDef,
}

#[derive(Component, Clone, Copy, Debug)]
pub struct ChainLightning {
    pub jumps_left: u8,
    pub range: f32,
    pub falloff: f32,
}

#[derive(Component, Debug)]
pub struct LightningArc {
    pub timer: Timer,
}

#[derive(Component, Clone, Copy, Debug)]
pub struct DeploysSentry {
    pub life: f32,
    pub fire_interval: f32,
    pub range: f32,
    pub projectile_speed: f32,
    pub projectile_damage: i32,
}

#[derive(Component, Clone, Copy, Debug)]
pub struct CustomExplosion {
    pub radius: f32,
}

#[derive(Component, Clone, Copy, Debug)]
pub struct BloodAmmo {
    pub hp_cost: i32,
}

#[derive(Component, Clone, Copy, Debug)]
pub struct SpawnsWeaponPickup {
    pub weapon: Option<WeaponId>,
}

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
    pub source: Option<DamageSource>,
}

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

#[derive(Component)]
pub struct IdpdShieldUnit;

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

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CampfirePhase {

    Sitting,

    WaitingForIdpd,

    Rising,

    SpawnThroneII,
}

#[derive(Component)]
pub struct CampfireState {
    pub phase: CampfirePhase,

    pub timer: Timer,

    pub idpd_clear_confirm: Timer,

    pub idpd_gate_armed: bool,

    pub spawned_throne_ii: bool,
}

impl CampfireState {
    pub fn new() -> Self {
        Self {
            phase: CampfirePhase::Sitting,
            timer: Timer::from_seconds(3.5, TimerMode::Once),
            idpd_clear_confirm: Timer::from_seconds(0.35, TimerMode::Once),
            idpd_gate_armed: false,
            spawned_throne_ii: false,
        }
    }

    pub fn set_phase(&mut self, phase: CampfirePhase, seconds: f32) {
        self.phase = phase;
        self.timer = Timer::from_seconds(seconds.max(0.01), TimerMode::Once);
        self.timer.reset();
    }

    pub fn arm_idpd_gate(&mut self) {
        self.phase = CampfirePhase::WaitingForIdpd;
        self.idpd_gate_armed = true;
        self.idpd_clear_confirm.reset();
    }

    pub fn reset_idpd_clear_confirmation(&mut self) {
        self.idpd_clear_confirm.reset();
    }
}

#[derive(Component)]
pub struct CampfireProp;

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

    pub fn blocks_new_idpd_raids(&self) -> bool {
        self.campfire_active || self.throne_ii_alive || self.loop_ready
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

#[derive(Component, Clone, Copy, Debug)]
pub struct PendingEnemySpawn {
    pub kind: EnemyKind,
    pub pos: Vec2,
    pub difficulty: f32,
}

#[derive(Component, Clone, Copy, Debug)]
pub struct PendingDelayedBoss {
    pub kind: EnemyKind,
    pub initial_trash: u32,
    pub kill_fraction: f32,

    pub from_wall: bool,
}

impl PendingDelayedBoss {
    pub fn kills_needed(&self) -> u32 {
        (((self.initial_trash as f32) * self.kill_fraction).ceil() as u32).max(1)
    }
}

#[derive(Component)]
pub struct HyperOrbitCrystal {
    pub owner: Entity,
    pub angle: f32,
    pub radius: f32,
    pub angular_speed: f32,
    pub fire_timer: Timer,
}

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

    pub walk: f32,

    pub ammo: u8,

    pub gunangle: f32,
}

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

    Spawning,
    Teleport,
    CarpetBeam,
}

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
            EnemyKind::BigBandit | EnemyKind::BigBanditLoop => {
                if kind == EnemyKind::BigBanditLoop {
                    (0.95, 2.25)
                } else {
                    (1.15, 2.8)
                }
            }
            EnemyKind::BigDog | EnemyKind::BigDogLoop => {
                if kind == EnemyKind::BigDogLoop {
                    (0.62, 1.8)
                } else {
                    (0.8, 2.2)
                }
            }
            EnemyKind::LilHunter | EnemyKind::LilHunterLoop => {
                if kind == EnemyKind::LilHunterLoop {
                    (0.42, 1.35)
                } else {
                    (0.55, 1.7)
                }
            }
            EnemyKind::Throne => (0.7, 2.5),
            EnemyKind::ThroneII => (0.85, 3.4),
            EnemyKind::Hyper => (1.1, 4.0),
            EnemyKind::Mom => (1.0, 2.4),
            EnemyKind::Technomancer => (2.0, 3.5),
            EnemyKind::Captain => (0.7, 2.0),
            EnemyKind::OldGuardian => (0.9, 2.2),
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

#[derive(Component, Clone, Copy, Debug, Default)]
pub struct WepPickupAmmo(pub bool);

#[derive(Clone, Copy)]
pub enum PickupKind {
    Rad(u32),
    Medkit(i32),
    Ammo(AmmoKind, i32),
    Weapon(WeaponId),
    Chest(ChestKind),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ChestKind {
    Weapon,
    Ammo,
    Rad,
}

impl From<WeaponKind> for PickupKind {
    fn from(k: WeaponKind) -> Self {
        PickupKind::Weapon(k.into())
    }
}

#[derive(Component)]
pub struct Portal;

#[derive(Component)]
pub struct PortalShock {
    pub timer: Timer,
    pub radius: f32,
}

#[derive(Component)]
pub struct PortalClear {
    pub timer: Timer,
}

#[derive(Component)]
pub struct PortalClosing {
    pub timer: Timer,
}

#[derive(Resource, Default)]
pub struct PortalCarriedWeapons(pub Vec<WeaponId>);

#[derive(Component)]
pub struct HurtAnim {
    pub idle: &'static str,
    pub walk: Option<&'static str>,
    pub hurt: &'static str,
    pub timer: Timer,
    pub was_moving: bool,
}

#[derive(Component)]
pub struct FireAnim {
    pub idle: &'static str,
    pub walk: Option<&'static str>,
    pub timer: Timer,
}

#[derive(Component)]
pub struct OpenedChest;

#[derive(Component)]
pub struct PickupLifetime {
    pub timer: Timer,
}

#[derive(Component)]
pub struct GroundPhysics {
    pub vel: Vec2,
    pub rotspeed: f32,
}

#[derive(Component)]
pub struct PortalSucking {
    pub portal: Entity,
    pub timer: Timer,
    pub start_pos: Vec2,
    pub target_pos: Vec2,
}

#[derive(Component)]
pub struct WeaponVisual {
    pub owner: Entity,
    pub wkick: f32,
    pub wep_id: WeaponId,

    pub slot: u8,
}

#[derive(Component, Clone, Copy)]
pub struct WeaponVisualOwner;

#[derive(Component, Clone, Copy)]
pub struct EnemySprites {
    pub idle: &'static str,
    pub walk: Option<&'static str>,
    pub hurt: &'static str,
}

#[derive(Component)]
pub struct Prop {
    pub size: Vec2,
    pub hp: i32,
    pub destructible: bool,
    pub explosive: bool,
}

#[derive(Component, Clone, Copy)]
pub struct PropSprites {
    pub idle: &'static str,
    pub hurt: &'static str,
    pub dead: &'static str,
    pub flip_x: bool,
}

#[derive(Component, Clone, Copy)]
pub struct PropHpTracker {
    pub last_hp: i32,
}

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

#[derive(Component)]
pub struct SwingFx {
    pub timer: Timer,
}

#[derive(Component)]
pub struct Dash {
    pub timer: Timer,
    pub dir: Vec2,
}

#[derive(Component)]
pub struct Shield {
    pub timer: Timer,
}

#[derive(Component)]
pub struct Telekinesis {
    pub timer: Timer,
}

#[derive(Component)]
pub struct PopPopCharges(pub u8);

#[derive(Component)]
pub struct SnareZone {
    pub timer: Timer,
    pub radius: f32,
    pub slow: f32,
}

#[derive(Component)]
pub struct Slowed {
    pub timer: Timer,
    pub factor: f32,
}

#[derive(Component)]
pub struct Ally {
    pub life: Timer,
    pub shoot: Timer,
}

#[derive(Component)]
pub struct PortalStrike {
    pub timer: Timer,
    pub radius: f32,
    pub damage: i32,
}

#[derive(Component)]
pub struct HazardCloud {
    pub kind: HazardKind,
    pub radius: f32,
    pub damage: i32,
    pub timer: Timer,
    pub tick: Timer,
}

#[derive(Component, Default)]
pub struct HeadlessReady(pub bool);

#[derive(Component, Clone, Copy, Debug)]
pub struct CrownPedestal {
    pub kind: CrownKind,
}

#[derive(Resource, Debug, Clone)]
pub struct ThroneRoomState {
    pub generators_total: u8,
    pub generators_destroyed: u8,
    pub all_generators_down: bool,

    pub loop_eligible: bool,

    pub player_on_carpet: bool,
    pub halved_throne: bool,
}

impl Default for ThroneRoomState {
    fn default() -> Self {
        Self {
            generators_total: 4,
            generators_destroyed: 0,
            all_generators_down: false,
            loop_eligible: false,
            player_on_carpet: false,
            halved_throne: false,
        }
    }
}

impl ThroneRoomState {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn note_generator_destroyed(&mut self) {
        self.generators_destroyed = self.generators_destroyed.saturating_add(1);
        if self.generators_destroyed >= self.generators_total {
            self.all_generators_down = true;
            self.loop_eligible = true;
        }
    }
}

#[derive(Component, Clone, Copy, Debug)]
pub struct BigGenerator {
    pub index: u8,
}

#[derive(Component, Clone, Copy, Debug)]
pub struct ThroneStatueProp {

    pub guardian_count: u8,
}

#[derive(Component, Clone, Copy, Debug)]
pub struct SnowmanAmbush;

#[derive(Component, Clone, Copy, Debug)]
pub struct GoldBarrelDrop;

#[derive(Component, Clone, Copy, Debug)]
pub struct RadChestContainer;

#[derive(Component, Clone, Copy, Debug)]
pub struct ThroneCarpet {
    pub half_extents: Vec2,
}

#[derive(Component, Debug)]
pub struct Corpse {
    pub kind: EnemyKind,
    pub life: Timer,
    pub pos: Vec2,
}

// Flag Dying before deferred despawn.
#[derive(Component, Debug, Default)]
pub struct Dying;

#[derive(Component, Debug)]
pub struct PlayerDying {
    pub timer: Timer,
}

#[derive(Component, Clone, Copy, Debug, Default)]
pub struct ScreenEnd;

#[derive(Resource, Debug, Clone)]
pub struct FloorTransition {
    pub active: bool,

    pub stage: u8,
    pub timer: Timer,
    pub progress: f32,
    pub tip: String,
}

impl Default for FloorTransition {
    fn default() -> Self {
        Self {
            active: false,
            stage: 0,
            timer: Timer::from_seconds(0.05, TimerMode::Repeating),
            progress: 0.0,
            tip: String::new(),
        }
    }
}

#[cfg(test)]
mod loop_boss_brain_tests {
    use super::*;

    #[test]
    fn loop_boss_brains_have_faster_cadence() {
        let base = BossBrain::new(EnemyKind::BigDog, Vec2::ZERO);
        let looped = BossBrain::new(EnemyKind::BigDogLoop, Vec2::ZERO);

        assert!(looped.attack_timer.duration() < base.attack_timer.duration());
        assert!(looped.special_timer.duration() < base.special_timer.duration());
    }
}

#[cfg(test)]
mod source_tests {
    use super::*;

    #[test]
    fn damage_source_enemy_encodes_kind() {
        let e = Entity::from_bits(0x0100_0001);
        let s = DamageSource::enemy(e, EnemyKind::Bandit);
        assert_eq!(s.hit_id, HitId::from_enemy_kind(EnemyKind::Bandit));
        assert_eq!(s.enemy_kind, Some(EnemyKind::Bandit));
        assert_ne!(s.owner, Entity::PLACEHOLDER);
    }

    #[test]
    fn last_damage_names_enemy_from_hit_id() {
        let mut last = LastDamageTaken::default();
        last.note(Some(HitId::from_enemy_kind(EnemyKind::Scorpion)), None);
        assert_eq!(last.source_name, "SCORPION");

        last.note_from_source(Some(&DamageSource::enemy(
            Entity::from_bits(0x0200_0001),
            EnemyKind::Turtle,
        )));
        assert_eq!(last.source_name, "TURTLE");
    }
}
