//! Portal vortex as a single WGSL quad (replaces the per-wisp entity swarm).
//!
//! Fidelity source: `nt-recreated-public-rewrite`:
//!   - `objects/SpiralCont/Create_0.gml`  (warmup `repeat 150`, orbit drift)
//!   - `objects/SpiralCont/Step_0.gml`    (angle += 8 + sin(a/300), spawn 1/tick)
//!   - `objects/Spiral/Step_0.gml`        (grow law, destroy xscale>2.5)
//!   - `scripts/scrDrawSpiral/scrDrawSpiral.gml` (white+black passes, lightning)
//!
//! Orbit is `80/50` per the public rewrite (this port tracks `~/Downloads`,
//! not the commercial YYC `130/90`). Tick rate is exactly 30 Hz like GML;
//! after SpiralCont dies wisps grow 1.5x (GML `grow *= 1.5`, destroy at 3).
//!
//! The CPU only advances the controller clock and maintains a ring of
//! `[x, y, birth_tick, rot]` vec4s; every wisp's growth, fade, lightning and
//! compositing happen in the fragment shader (`assets/shaders/vortex.wgsl`).
//! One draw call regardless of wisp count - no entity churn, no startup stall.

use bevy::prelude::*;
use bevy::render::render_resource::AsBindGroup;
use bevy::shader::ShaderRef;
use bevy::sprite_render::{AlphaMode2d, Material2d, Material2dPlugin};

use crate::app::AppState;
use crate::game::ui_art::{GUI_H, GUI_W, TitleArt, gui_map};

pub const MAX_WISPS: usize = 128;
/// `SpiralDebris` ring size (avg ~1.6 alive, lifetime 66-88 ticks - generous).
pub const MAX_DEBRIS: usize = 32;
/// Create_0 `repeat 150` - the exact number of simulated ticks before the
/// first drawn frame (faithful to SpiralCont/Create_0.gml:36).
const WARMUP_TICKS: u32 = 150;

/// `SpiralCont.type` from `SpiralCont/Create_0.gml`, derived from the area:
/// vault = Proto, HQ = IDPD, mansion/crib = Venuz, everything else = Normal.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum SpiralKind {
    #[default]
    Normal,
    Proto,
    Idpd,
    Venuz,
}

impl SpiralKind {
    fn for_gml_area(area: u8) -> Self {
        match area {
            // area_vault
            100 => Self::Proto,
            // area_hq
            106 => Self::Idpd,
            // area_mansion / area_crib
            103 | 107 => Self::Venuz,
            _ => Self::Normal,
        }
    }
}

/// GML `area_*` ints (`macros_general.gml`) for the Bevy route areas.
/// Selects the SpiralKind and the debris sprite (`"sprDebris" + area`).
pub fn gml_area_for_bevy_area(area: crate::game::areas::AreaId) -> u8 {
    use crate::game::areas::AreaId;
    match area {
        AreaId::Campfire => 0,
        AreaId::Desert => 1,
        AreaId::Sewers => 2,
        AreaId::Scrapyards => 3,
        AreaId::CrystalCaves => 4,
        AreaId::FrozenCity => 5,
        AreaId::Labs => 6,
        AreaId::Palace => 7,
        AreaId::Vault => 100,
        AreaId::Oasis => 101,
        AreaId::PizzaSewers => 102,
        // Y.V. mansion family
        AreaId::City => 103,
        AreaId::CursedCaves => 104,
        AreaId::Jungle => 105,
        AreaId::HQ => 106,
        // Vault-themed
        AreaId::CrownVault => 100,
        AreaId::Loop => 1,
    }
}

/// Rare 1/50 area-variant debris (`SpiralDebris/Create_0.gml`): static path +
/// frame (`image_index = 1`, wrapping to 0 on single-frame strips).
fn variant_debris_for_gml_area(area: u8) -> Option<(&'static str, usize)> {
    match area {
        1 => Some(("images/sprBanditHurt.png", 1)),
        2 => Some(("images/sprRatHurt.png", 1)),
        3 => Some(("images/sprCarIdle.png", 1)),
        4 => Some(("images/sprSpiderHurt.png", 1)),
        5 => Some(("images/sprFrozenCar.png", 1)),
        6 => Some(("images/sprFreak1Hurt.png", 1)),
        102 => Some(("images/sprSlice.png", 1)),
        _ => None,
    }
}

/// Crown art index for the swirl-center figure (`"sprCrown" + crown +
/// "Idle"` with GML `Crown` ints: None = 1, Death = 2 .. Protection = 13).
/// Matched by NAME against `~/Downloads` `scrCrowns.gml`, not by the Bevy
/// discriminant order (which swaps Luck/Risk).
pub fn crown_fig_path(crown: crate::game::content::CrownKind) -> Option<&'static str> {
    use crate::game::content::CrownKind;
    match crown {
        CrownKind::None => None,
        CrownKind::Death => Some("images/sprCrown2Idle.png"),
        CrownKind::Life => Some("images/sprCrown3Idle.png"),
        CrownKind::Haste => Some("images/sprCrown4Idle.png"),
        CrownKind::Guns => Some("images/sprCrown5Idle.png"),
        CrownKind::Hatred => Some("images/sprCrown6Idle.png"),
        CrownKind::Blood => Some("images/sprCrown7Idle.png"),
        CrownKind::Destiny => Some("images/sprCrown8Idle.png"),
        CrownKind::Love => Some("images/sprCrown9Idle.png"),
        CrownKind::Luck => Some("images/sprCrown10Idle.png"),
        CrownKind::Curses => Some("images/sprCrown11Idle.png"),
        CrownKind::Risk => Some("images/sprCrown12Idle.png"),
        CrownKind::Protection => Some("images/sprCrown13Idle.png"),
    }
}

/// One Venuz `SpiralStar` (`SpiralStar/Create_0` + `Step_0`): fixed angle,
/// same grow law as debris, killed past xscale 30. CPU-rendered (see below).
struct Star {
    alive: bool,
    xstart: f32,
    ystart: f32,
    dist: f32,
    angle: f32,
    grow: f32,
    xscale: f32,
    frame: f32,
    entity: Option<Entity>,
}

/// One rare variant debris: same motion as `Debris` but its own texture, so
/// it rides as a CPU sprite instead of the shared debris channel.
struct Vard {
    alive: bool,
    xstart: f32,
    ystart: f32,
    dist: f32,
    angle: f32,
    turnspeed: f32,
    rotspeed: f32,
    grow: f32,
    xscale: f32,
    image_angle: f32,
    path: &'static str,
    frame: usize,
    entity: Option<Entity>,
}

/// One `SpiralDebris` instance (objects/SpiralDebris/Create_0.gml + Step_0.gml).
struct Debris {
    alive: bool,
    xstart: f32,
    ystart: f32,
    dist: f32,
    angle: f32,
    turnspeed: f32,
    rotspeed: f32,
    xscale: f32,
    grow: f32,
    image_angle: f32,
    frame: f32,
}

/// The `SpiralCont` driver state.
#[derive(Resource)]
pub struct SpiralCtl {
    /// `image_angle` - accumulates UNBOUNDED like GML (never wraps %360);
    /// the orbit trig divides by 921/583/500 so it must reach thousands of
    /// degrees for the centre to wander like the original.
    pub angle: f32,
    /// Total elapsed 30 Hz ticks.
    pub ticks: f32,
    /// Fractional tick carry (FixedUpdate runs at 60 Hz).
    acc: f32,
    /// Ring mirrored into the material uniform:
    /// [x, y, birth_tick, rot_rad]; birth < 0 = empty slot. IDPD2-variant
    /// wisps store a NEGATED rot (variant flag; shader takes abs).
    pub ring: Vec<[f32; 4]>,
    head: usize,
    /// `SpiralDebris` instances (spawned by SpiralCont/Step_0 at 1/48 per tick,
    /// 1/16 for Proto, never for Venuz).
    debris: Vec<Debris>,
    /// Render-ready debris mirror: [x, y, rot_rad, frame + xscale/32];
    /// x < -100 = empty slot.
    pub debris_ring: Vec<[f32; 4]>,
    dhead: usize,
    /// Venuz `SpiralStar` instances: 1 spawn per tick, killed past xscale 30.
    stars: Vec<Star>,
    /// Rare 1/50 area-variant debris (own texture, CPU-rendered).
    vards: Vec<Vard>,
    /// Entities whose sim died and await despawn by the render system.
    retired: Vec<Entity>,
    pub alive: bool,
    pub death_tick: Option<f32>,
    /// SpiralCont.type + GML area int (debris sprite, center behavior).
    pub kind: SpiralKind,
    pub gml_area: u8,
}

impl SpiralCtl {
    /// Mirrors `SpiralCont/Create_0.gml:36` `repeat 150 { Step; with Spiral Step }`.
    /// 150 ticks are simulated verbatim; the ring ends up with the 128 most
    /// recent spawns (the oldest 22 have wrapped). Survivors are exactly those
    /// the GML would have kept - ~119 with age < 120. Keeping dead slots in
    /// the ring is faithful: the shader culls s>2.5 the same way GML destroys
    /// instances when `image_xscale > 2.5`.
    pub fn warmed_up() -> Self {
        Self::warmed_up_for_gml_area(0)
    }

    /// Warmup for a concrete GML area (debris sprite + SpiralKind).
    /// Menus pass campfire (0): no `GameCont` there, matching Create_0's
    /// `area = area_campfire` default.
    pub fn warmed_up_for_gml_area(gml_area: u8) -> Self {
        let mut ctl = Self {
            angle: rand::random::<f32>() * 360.0,
            ticks: 0.0,
            acc: 0.0,
            ring: vec![[-1.0; 4]; MAX_WISPS],
            head: 0,
            debris: (0..MAX_DEBRIS)
                .map(|_| Debris {
                    alive: false,
                    xstart: 0.0,
                    ystart: 0.0,
                    dist: 0.0,
                    angle: 0.0,
                    turnspeed: 0.0,
                    rotspeed: 0.0,
                    xscale: 0.0,
                    grow: 0.0,
                    image_angle: 0.0,
                    frame: 0.0,
                })
                .collect(),
            debris_ring: vec![[-1000.0; 4]; MAX_DEBRIS],
            dhead: 0,
            stars: Vec::new(),
            vards: Vec::new(),
            retired: Vec::new(),
            alive: true,
            death_tick: None,
            kind: SpiralKind::for_gml_area(gml_area),
            gml_area,
        };
        for _ in 0..WARMUP_TICKS {
            ctl.tick_once();
        }
        ctl
    }

    /// One 30 Hz tick: SpiralCont/Step_0 + the debris spawn check and every
    /// SpiralDebris/Step_0 - shared by the warmup and the live clock so the
    /// field state is identical however the controller was armed.
    ///
    /// Drain: once SpiralCont is gone, GML compounds `grow *= 1.5` EVERY tick
    /// (Spiral/Step_0, SpiralDebris/Step_0), so every survivor exceeds the
    /// xscale-3 kill plane within ~21 ticks (~0.7s: oldest pop in ~4, mid in
    /// ~8, newborns in ~21). Wisp ages are rewound 5.5 extra ticks per dead
    /// tick (6.5x aging) for the identical staggered clear through the
    /// shader growth table; debris compounds for real on the CPU.
    fn tick_once(&mut self) {
        self.ticks += 1.0;
        if self.alive {
            let kind = self.kind;
            // SpiralCont/Step_0: increment angle, then emit at the center.
            // IDPD/Venuz pin the center; Normal/Proto wander (orbit).
            self.angle += spiral_angle_inc(self.angle, kind);
            let (x, y) = if matches!(kind, SpiralKind::Idpd | SpiralKind::Venuz) {
                (GUI_W / 2.0, GUI_H / 2.0)
            } else {
                orbit(self.angle)
            };
            if kind == SpiralKind::Venuz {
                // Venuz emits only SpiralStars (no normal wisps, no debris).
                self.push_star(x, y);
            } else {
                let mut rot = (self.angle + 45.0).to_radians();
                if kind == SpiralKind::Idpd && (self.ticks as i64 % 11) <= 1 {
                    // `if other.time % 11 <= 1 sprite_index = sprSpiralIDPD2`;
                    // variant rides in the rot sign (shader takes abs).
                    rot = -rot;
                }
                self.ring[self.head] = [x, y, self.ticks, rot];
                self.head = (self.head + 1) % MAX_WISPS;

                // Debris: `random(16) < 1`, plus `random(3) < 1` except Proto
                // (which always passes). Venuz handled above.
                let proto = kind == SpiralKind::Proto;
                if rand::random::<f32>() * 16.0 < 1.0
                    && (proto || rand::random::<f32>() * 3.0 < 1.0)
                {
                    // Rare 1/50 area variant goes the CPU route (own texture).
                    if rand::random::<f32>() * 50.0 < 1.0
                        && let Some((path, frame)) =
                            variant_debris_for_gml_area(self.gml_area)
                    {
                        self.push_vard(x, y, path, frame);
                    } else {
                        let d = &mut self.debris[self.dhead];
                        *d = Debris {
                            alive: true,
                            xstart: x,
                            ystart: y,
                            dist: rand::random::<f32>() * 135.0 + 10.0,
                            angle: rand::random::<f32>() * 360.0,
                            turnspeed: rand::random::<f32>() * 8.0 - 4.0,
                            rotspeed: rand::random::<f32>() * 16.0 - 8.0,
                            xscale: 0.0,
                            grow: 0.0,
                            // GML default image_angle is 0; only rotspeed varies.
                            image_angle: 0.0,
                            frame: (rand::random::<f32>() * 4.0).floor().min(3.0),
                        };
                        self.dhead = (self.dhead + 1) % MAX_DEBRIS;
                    }
                }
            }
        } else {
            // No new spawns; survivors fast-forward (see method docs).
            for slot in self.ring.iter_mut() {
                if slot[2] >= 0.0 {
                    slot[2] -= 5.5;
                }
            }
        }

        // SpiralDebris/Step_0 (exact order): position from current state,
        // then advance angle/dist/grow/xscale, then self-rotation.
        let drain = !self.alive;
        for (i, d) in self.debris.iter_mut().enumerate() {
            if !d.alive {
                continue;
            }
            let (rad, dir) = (d.dist * d.xscale, d.angle.to_radians());
            let dx = rad * dir.cos();
            let dy = -rad * dir.sin(); // lengthdir_y: negative sin (y-down flip)
            d.angle += d.turnspeed;
            d.dist += d.grow;
            d.grow += 0.0005;
            d.xscale += d.grow / 1.5;
            d.grow = (d.grow + 1.0) * (1.0 + 0.001 * d.xscale) - 1.0;
            if drain {
                // `if !instance_exists(SpiralCont) grow *= 1.5`, same position
                // in the sequence as the GML (before the xscale term below).
                d.grow *= 1.5;
            }
            d.grow *= d.xscale * 0.05 + 1.0;
            d.image_angle += d.rotspeed;
            if dx + d.xstart < -16.0
                || dx + d.xstart > GUI_W + 16.0
                || dy + d.ystart < -16.0
                || dy + d.ystart > GUI_H + 16.0
            {
                d.alive = false;
                self.debris_ring[i] = [-1000.0; 4];
                continue;
            }
            // pack: frame + xscale/32 (xscale stays below ~21 before cull)
            self.debris_ring[i] = [
                d.xstart + dx,
                d.ystart + dy,
                d.image_angle.to_radians(),
                d.frame + d.xscale / 32.0,
            ];
        }

        // SpiralStar/Step_0 (exact order): fixed angle (the `angle +=
        // turnspeed` line is commented out upstream), same grow law as
        // debris, killed past xscale 30 (no view cull).
        for s in self.stars.iter_mut() {
            if !s.alive {
                continue;
            }
            s.dist += s.grow;
            s.grow += 0.0005;
            s.xscale += s.grow / 1.5;
            s.grow = (s.grow + 1.0) * (1.0 + 0.001 * s.xscale) - 1.0;
            if drain {
                s.grow *= 1.5;
            }
            s.grow *= s.xscale / 20.0 + 1.0;
            if s.xscale > 30.0 {
                s.alive = false;
                if let Some(e) = s.entity.take() {
                    self.retired.push(e);
                }
            }
        }

        // Variant debris ride the debris recurrence (they ARE SpiralDebris
        // instances with a swapped sprite), including the view cull.
        for v in self.vards.iter_mut() {
            if !v.alive {
                continue;
            }
            let (rad, dir) = (v.dist * v.xscale, v.angle.to_radians());
            let dx = rad * dir.cos();
            let dy = -rad * dir.sin();
            v.angle += v.turnspeed;
            v.dist += v.grow;
            v.grow += 0.0005;
            v.xscale += v.grow / 1.5;
            v.grow = (v.grow + 1.0) * (1.0 + 0.001 * v.xscale) - 1.0;
            if drain {
                v.grow *= 1.5;
            }
            v.grow *= v.xscale * 0.05 + 1.0;
            v.image_angle += v.rotspeed;
            if dx + v.xstart < -16.0
                || dx + v.xstart > GUI_W + 16.0
                || dy + v.ystart < -16.0
                || dy + v.ystart > GUI_H + 16.0
            {
                v.alive = false;
                if let Some(e) = v.entity.take() {
                    self.retired.push(e);
                }
            }
        }
    }

    /// Spawn a Venuz star (SpiralStar/Create_0): `image_index =
    /// choose(0,0,0,1)`, scale/grow zeroed. Reuses dead slots.
    fn push_star(&mut self, x: f32, y: f32) {
        let star = Star {
            alive: true,
            xstart: x,
            ystart: y,
            dist: rand::random::<f32>() * 135.0 + 10.0,
            angle: rand::random::<f32>() * 360.0,
            grow: 0.0,
            xscale: 0.0,
            frame: if rand::random::<f32>() * 4.0 < 1.0 {
                1.0
            } else {
                0.0
            },
            entity: None,
        };
        if let Some(slot) = self.stars.iter_mut().find(|s| !s.alive) {
            *slot = star;
        } else {
            self.stars.push(star);
        }
    }

    /// Spawn a rare variant debris (shares the debris roll, own texture).
    fn push_vard(&mut self, x: f32, y: f32, path: &'static str, frame: usize) {
        let vard = Vard {
            alive: true,
            xstart: x,
            ystart: y,
            dist: rand::random::<f32>() * 135.0 + 10.0,
            angle: rand::random::<f32>() * 360.0,
            turnspeed: rand::random::<f32>() * 8.0 - 4.0,
            // `rotspeed = random_range(20, 30) * choose(1, -1)`
            rotspeed: rand::random_range(20.0..30.0)
                * if rand::random_bool(0.5) { 1.0 } else { -1.0 },
            grow: 0.0,
            xscale: 0.0,
            // GML default image_angle is 0; variants only change rotspeed.
            image_angle: 0.0,
            path,
            frame,
            entity: None,
        };
        if let Some(slot) = self.vards.iter_mut().find(|v| !v.alive) {
            *slot = vard;
        } else {
            self.vards.push(vard);
        }
    }

    fn step(&mut self, dt_ticks: f32) {
        // Exact GML 30 Hz cadence alive and dead; post-death acceleration is
        // modelled per-wisp (birth rewind + debris grow compounding in
        // tick_once), not by overclocking the whole clock.
        self.acc += dt_ticks;
        while self.acc >= 1.0 {
            self.acc -= 1.0;
            self.tick_once();
        }
    }
}

/// SpiralCont/Step_0.gml angle increments (degrees): Normal/IDPD/Venuz add
/// `8 + sin(angle/300)`; Proto adds `10 + sin(angle/300) * 2 + orandom(1)`.
fn spiral_angle_inc(angle: f32, kind: SpiralKind) -> f32 {
    if kind == SpiralKind::Proto {
        10.0 + deg_sin(angle / 300.0) * 2.0 + (rand::random::<f32>() * 2.0 - 1.0)
    } else {
        8.0 + deg_sin(angle / 300.0)
    }
}

/// SpiralCont/Step_0.gml:18-19 orbit around the GUI centre (GML sin/cos take
/// DEGREES; bevy takes radians, hence the conversions). Public rewrite uses
/// `* 80 / * 50`.
fn orbit(angle: f32) -> (f32, f32) {
    (
        GUI_W / 2.0 + deg_sin(angle / 921.0) * deg_sin(angle / 500.0) * 80.0,
        GUI_H / 2.0 + deg_cos(angle / 583.0) * deg_sin(angle / 500.0) * 50.0,
    )
}

fn deg_sin(deg: f32) -> f32 {
    deg.to_radians().sin()
}

fn deg_cos(deg: f32) -> f32 {
    deg.to_radians().cos()
}

// GPU material

/// Uniform layout must mirror `assets/shaders/vortex.wgsl`.
/// NOTE: a nested `[[f32;4]; N]` makes encase compute a stride-4 array and
/// abort ("array stride must be a multiple of 16"); a flat array of `Vec4`
/// (alignment 16) is byte-identical on the wire and always valid.
#[derive(AsBindGroup, Asset, TypePath, Debug, Clone)]
struct VortexMaterial {
    /// Per-wisp [x, y, birth_tick, rot]; birth < 0 = inactive slot.
    #[uniform(0)]
    wisps: [Vec4; MAX_WISPS],
    /// (tick_now, lightning_enabled, bg_r, bg_g)
    #[uniform(1)]
    glob_a: Vec4,
    /// (bg_b, bg_alpha, kill_scale, kind + debris16 * 4): kind is the
    /// SpiralKind discriminant (Normal 0 / Proto 1 / Idpd 2 / Venuz 3);
    /// debris16 flags jungle `sprDebris105` (16px frames, not 8px).
    #[uniform(2)]
    glob_b: Vec4,
    #[texture(3)]
    #[sampler(4)]
    spiral_tex: Handle<Image>,
    #[texture(5)]
    #[sampler(6)]
    bolt_tex: Handle<Image>,
    /// Render-ready debris: [x, y, rot_rad, frame + xscale/32]; x < -100 = empty.
    #[uniform(7)]
    debris: [Vec4; MAX_DEBRIS],
    #[texture(8)]
    #[sampler(9)]
    debris_tex: Handle<Image>,
    /// Vault `sprSpiralProto` (green 64px) for the Proto kind.
    #[texture(10)]
    #[sampler(11)]
    spiral_proto_tex: Handle<Image>,
    /// HQ `sprSpiralIDPD` / `sprSpiralIDPD2` (128px) for the Idpd kind.
    #[texture(12)]
    #[sampler(13)]
    spiral_idpd_tex: Handle<Image>,
    #[texture(14)]
    #[sampler(15)]
    spiral_idpd2_tex: Handle<Image>,
}

fn ring_to_uniform(ring: &[[f32; 4]]) -> [Vec4; MAX_WISPS] {
    let mut out = [Vec4::NEG_ONE; MAX_WISPS];
    for (dst, src) in out.iter_mut().zip(ring.iter()) {
        *dst = Vec4::new(src[0], src[1], src[2], src[3]);
    }
    out
}

fn debris_to_uniform(ring: &[[f32; 4]]) -> [Vec4; MAX_DEBRIS] {
    let mut out = [Vec4::new(-1000.0, 0.0, 0.0, 0.0); MAX_DEBRIS];
    for (dst, src) in out.iter_mut().zip(ring.iter()) {
        *dst = Vec4::new(src[0], src[1], src[2], src[3]);
    }
    out
}

impl Material2d for VortexMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/vortex.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }
}

/// Marker for a CPU-rendered Venuz star (index into `SpiralCtl.stars`).
#[derive(Component)]
struct VortexStarDot(usize);

/// Marker for a CPU-rendered variant debris (index into `SpiralCtl.vards`).
#[derive(Component)]
struct VortexVardDot(usize);

/// Swirl-center crown figure (`scrDrawSpiral` `with SpiralCont` block).
#[derive(Component)]
struct SpiralCrownFig;

/// Swirl-center player figure (same block, `spr_hurt` frame 1).
#[derive(Component)]
struct SpiralPlayerFig;

/// Marker for the fullscreen vortex quad (despawned with other title art).
#[derive(Component)]
pub struct VortexQuad;

/// CPU-rendered swirl layer: Venuz stars, rare variant debris (both need
/// their own textures, so they ride as sprites), and the swirl-center crown
/// + player figures from `scrDrawSpiral`'s `with SpiralCont` block.
///
/// GML draws the figures whenever SpiralCont exists (never during the
/// linger: PlayButton destroys it instantly), at the drifting center for
/// Normal/Proto and the screen center for Idpd/Venuz. Rotations are GML
/// degrees CCW-visual, used directly as Bevy `rotation.z` (same convention).
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn sync_spiral_cpu_layer(
    mut commands: Commands,
    catalog: Res<crate::game::content::AssetCatalog>,
    asset_server: Res<AssetServer>,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    cam_q: Query<
        (Entity, &Transform, &Projection),
        (
            With<Camera2d>,
            Without<VortexStarDot>,
            Without<VortexVardDot>,
        ),
    >,
    state: Res<State<AppState>>,
    ctl: Option<ResMut<SpiralCtl>>,
    mut star_q: Query<
        (Entity, &VortexStarDot, &mut Transform, &mut Sprite),
        (With<VortexStarDot>, Without<VortexVardDot>),
    >,
    mut vard_q: Query<
        (Entity, &VortexVardDot, &mut Transform, &mut Sprite),
        (With<VortexVardDot>, Without<VortexStarDot>),
    >,
    fig_q: Query<Entity, Or<(With<SpiralCrownFig>, With<SpiralPlayerFig>)>>,
    player_q: Query<
        (
            &crate::game::anim::PlayerAnim,
            &crate::game::components::Player,
        ),
        With<crate::game::components::Player>,
    >,
) {
    let Some(mut ctl) = ctl else {
        for (e, _, _, _) in &star_q {
            commands.entity(e).try_despawn();
        }
        for (e, _, _, _) in &vard_q {
            commands.entity(e).try_despawn();
        }
        for e in &fig_q {
            commands.entity(e).try_despawn();
        }
        return;
    };
    for e in ctl.retired.drain(..) {
        commands.entity(e).try_despawn();
    }
    let Ok(win) = windows.single() else {
        return;
    };
    let Some((cam, _, proj)) = cam_q.iter().next() else {
        return;
    };
    let Projection::Orthographic(o) = proj else {
        return;
    };
    let map = gui_map(win.width(), win.height(), o.scale);
    let quad_z = if *state.get() == AppState::Title {
        -845.0
    } else {
        -885.0
    };

    // --- Stars (Venuz): white sprite, alpha fades in with xscale. This is
    // exact on the black background: GML's black pass (alpha 1 - xscale)
    // over black equals white at alpha xscale, and past xscale 1 both are
    // pure white.
    for (i, s) in ctl.stars.iter_mut().enumerate() {
        if !s.alive {
            continue;
        }
        let (dx, dir) = (s.dist * s.xscale, s.angle.to_radians());
        // lengthdir in y-down GUI space: x += len*cos, y += -len*sin.
        let gx = s.xstart + dx * dir.cos();
        let gy = s.ystart - dx * dir.sin();
        let alpha = s.xscale.clamp(0.0, 1.0);
        match s.entity {
            None => {
                let (mut spr, tf) = crate::game::ui_art::gm_sprite(
                    &catalog,
                    &asset_server,
                    &map,
                    "images/sprSpiralStar.png",
                    s.frame as usize,
                    gx,
                    gy,
                    s.xscale.max(0.001),
                    s.xscale.max(0.001),
                    Color::WHITE,
                    quad_z + 2.0,
                );
                spr.color.set_alpha(alpha);
                s.entity = Some(
                    commands
                        .spawn((VortexStarDot(i), spr, tf, ChildOf(cam)))
                        .id(),
                );
            }
            Some(e) => {
                if let Ok((_, dot, mut tf, mut spr)) = star_q.get_mut(e) {
                    debug_assert_eq!(dot.0, i);
                    place_dot(&map, &mut tf, &mut spr, gx, gy, 3.0, 3.0, 1.0, 1.0, s.xscale, 0.0, alpha);
                }
            }
        }
    }

    // --- Variant debris: same treatment with per-dot texture + rotation.
    for (i, v) in ctl.vards.iter_mut().enumerate() {
        if !v.alive {
            continue;
        }
        let (rad, dir) = (v.dist * v.xscale, v.angle.to_radians());
        let gx = v.xstart + rad * dir.cos();
        let gy = v.ystart - rad * dir.sin();
        let alpha = v.xscale.clamp(0.0, 1.0);
        match v.entity {
            None => {
                catalog.require(v.path);
                let (mut spr, tf) = crate::game::ui_art::gm_sprite(
                    &catalog,
                    &asset_server,
                    &map,
                    v.path,
                    v.frame,
                    gx,
                    gy,
                    v.xscale.max(0.001),
                    v.xscale.max(0.001),
                    Color::WHITE,
                    quad_z + 2.0,
                );
                spr.color.set_alpha(alpha);
                v.entity = Some(
                    commands
                        .spawn((VortexVardDot(i), spr, tf, ChildOf(cam)))
                        .id(),
                );
            }
            Some(e) => {
                if let Ok((_, dot, mut tf, mut spr)) = vard_q.get_mut(e) {
                    debug_assert_eq!(dot.0, i);
                    // Frame geometry varies per path; resolve once per frame
                    // is overkill — paths are fixed per dot, read dims cheap.
                    let m = sprite_dims(&catalog, v.path);
                    place_dot(
                        &map,
                        &mut tf,
                        &mut spr,
                        gx,
                        gy,
                        m.0,
                        m.1,
                        m.2,
                        m.3,
                        v.xscale,
                        v.image_angle.to_radians(),
                        alpha,
                    );
                }
            }
        }
    }

    // --- Center figures: alive spiral + a player, like `with SpiralCont`.
    for e in &fig_q {
        commands.entity(e).try_despawn();
    }
    if !ctl.alive {
        return;
    }
    let Ok((pa, player)) = player_q.single() else {
        return;
    };
    let (fx, fy) = if matches!(ctl.kind, SpiralKind::Idpd | SpiralKind::Venuz) {
        (GUI_W / 2.0, GUI_H / 2.0)
    } else {
        orbit(ctl.angle)
    };
    let ang = ctl.angle;
    if let Some(crown_path) = crown_fig_path(player.crown) {
        catalog.require(crown_path);
        let len = 15.0 + deg_sin(ang / 60.0) * 4.0;
        let dir = (-ang / 5.3).to_radians();
        // lengthdir in y-down GUI space: x = len*cos, y = -len*sin.
        let gx = fx + len * dir.cos();
        let gy = fy - len * dir.sin();
        let sc = 0.6 + deg_sin(ang / 200.0) / 4.0;
        let (spr, tf) = crate::game::ui_art::gm_sprite(
            &catalog,
            &asset_server,
            &map,
            crown_path,
            1,
            gx,
            gy,
            sc,
            sc,
            Color::WHITE,
            quad_z + 4.0,
        );
        let mut tf = tf;
        tf.rotation = Quat::from_rotation_z((-ang * 2.2).to_radians());
        commands.spawn((SpiralCrownFig, spr, tf, ChildOf(cam)));
    }
    {
        let sc = 0.8 + deg_sin(ang / 200.0) / 5.0;
        let (spr, tf) = crate::game::ui_art::gm_sprite(
            &catalog,
            &asset_server,
            &map,
            pa.hurt,
            1,
            fx,
            fy,
            sc,
            sc,
            Color::WHITE,
            quad_z + 4.0,
        );
        let mut tf = tf;
        tf.rotation = Quat::from_rotation_z((-ang * 2.0).to_radians());
        commands.spawn((SpiralPlayerFig, spr, tf, ChildOf(cam)));
    }
}

/// Reposition/rescale a CPU dot sprite in place (same origin math as
/// `gm_sprite`, plus a direct rotation which shares GML's CCW convention).
#[allow(clippy::too_many_arguments)]
fn place_dot(
    map: &crate::game::ui_art::GuiMap,
    tf: &mut Transform,
    spr: &mut Sprite,
    gui_x: f32,
    gui_y: f32,
    fw: f32,
    fh: f32,
    ox: f32,
    oy: f32,
    xscale: f32,
    rot_rad: f32,
    alpha: f32,
) {
    let xs = xscale.max(0.001);
    spr.custom_size = Some(Vec2::new(fw * xs * map.s, fh * xs * map.s));
    let left = gui_x - ox * xs;
    let top = gui_y - oy * xs;
    let center = map.to_world(left + fw * xs * 0.5, top + fh * xs * 0.5);
    tf.translation.x = center.x;
    tf.translation.y = center.y;
    tf.rotation = Quat::from_rotation_z(rot_rad);
    spr.color.set_alpha(alpha);
}

/// Frame geometry (w, h, xorigin, yorigin) for a catalog strip.
fn sprite_dims(catalog: &crate::game::content::AssetCatalog, path: &str) -> (f32, f32, f32, f32) {
    crate::game::ui_art::sprite_meta(catalog, path)
}

/// States where `SpiralCont` exists upstream: created with the Logo
/// (Vlambeer/Alarm_0 mode >= 3), kept through the main-menu buttons, and
/// destroyed right before the campfire char-select (PlayButton/Other_10).
/// Also active during InGame FloorTransition (GenCont loading) spiral.
fn spiral_states(state: &AppState) -> bool {
    matches!(
        *state,
        AppState::Splash | AppState::MainMenu | AppState::InGame | AppState::Loading
    )
}

/// Background colour behind the swirl: scrDrawSpiral does `draw_clear(c_black)`
/// for every non-`Menu` caller, and both of our swirl states (Splash logo,
/// MainMenu buttons) are non-`Menu` - so always black. The campfire blue only
/// applies to the char-select, where the swirl is destroyed.
fn background_color(_state: &AppState) -> Color {
    Color::BLACK
}

fn vortex_needs_black(
    state: &AppState,
    ft: Option<&crate::game::components::FloorTransition>,
    pending: bool,
) -> bool {
    match *state {
        AppState::Title => false, // transparent over campfire
        AppState::Loading => true,
        AppState::InGame => ft.is_some_and(|f| f.active) || pending,
        AppState::Splash | AppState::MainMenu => true,
    }
}

/// Advance the controller and mirror its ring into the material. One system,
/// zero per-wisp entities. Runs in Update with an internal 30 Hz accumulator
/// (`Time<Virtual>` respects pause/slow-mo), so the tick rate is exact and
/// independent of both render fps and the gameplay FixedUpdate cadence.
fn vortex_tick(
    state: Res<State<AppState>>,
    time: Res<Time<Real>>,
    mut ctl: Option<ResMut<SpiralCtl>>,
    mut materials: ResMut<Assets<VortexMaterial>>,
    q_mat: Query<&MeshMaterial2d<VortexMaterial>, With<VortexQuad>>,
    ft: Option<Res<crate::game::components::FloorTransition>>,
    pending: Option<Res<crate::game::components::PendingMutation>>,
    pending_ultra: Option<Res<crate::game::components::PendingUltra>>,
) {
    let Some(ctl) = ctl.as_mut() else {
        return;
    };
    ctl.step(time.delta_secs() * 30.0);

    let Ok(mat_handle) = q_mat.single().map(|m| m.0.clone()) else {
        warn_once!("vortex: tick could not find quad material");
        return;
    };
    let Some(mut mat) = materials.get_mut(&mat_handle) else {
        warn_once!("vortex: material asset missing for handle");
        return;
    };
    mat.wisps = ring_to_uniform(&ctl.ring);
    mat.debris = debris_to_uniform(&ctl.debris_ring);
    let [r, g, b, _] = background_color(state.get()).to_srgba().to_f32_array();
    let pending_any = pending.is_some() || pending_ultra.is_some();
    let bg_alpha = if vortex_needs_black(state.get(), ft.as_deref(), pending_any) {
        1.0
    } else {
        0.0
    };
    // Kill plane stays 2.5 alive and dead: post-death ages fast-forward
    // (birth rewind) so survivors stagger-pop through the table max just
    // like GML's compounding grow crossing xscale 3. kindpacked selects the
    // growth table/art and the debris frame size (see glob_b docs).
    let kindpacked =
        ctl.kind as u8 as f32 + if ctl.gml_area == 105 { 4.0 } else { 0.0 };
    mat.glob_a = Vec4::new(ctl.ticks, 1.0, r, g);
    mat.glob_b = Vec4::new(b, bg_alpha, 2.5, kindpacked);
}

/// Spawn the vortex quad once; keeps `SpiralCtl` alive alongside it.
#[allow(clippy::type_complexity)]
fn ensure_vortex_quad(
    mut commands: Commands,
    state: Res<State<AppState>>,
    asset_server: Res<AssetServer>,
    catalog: Res<crate::game::content::AssetCatalog>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<VortexMaterial>>,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    cam_q: Query<(Entity, &Transform, &Projection), With<Camera2d>>,
    existing: Query<(), (With<VortexQuad>, Without<Camera2d>)>,
    ctl: Option<Res<SpiralCtl>>,
    ft: Option<Res<crate::game::components::FloorTransition>>,
    pending: Option<Res<crate::game::components::PendingMutation>>,
    pending_ultra: Option<Res<crate::game::components::PendingUltra>>,
) {
    let Some(ctl) = ctl else {
        return;
    };
    if !spiral_states(state.get()) {
        return;
    }
    if !existing.is_empty() {
        return;
    }
    for path in [
        "images/sprSpiral.png",
        "images/sprSpiralProto.png",
        "images/sprSpiralIDPD.png",
        "images/sprSpiralIDPD2.png",
        "images/sprPortalLightning.png",
        "images/sprSpiralStar.png",
        &format!("images/sprDebris{}.png", ctl.gml_area),
    ] {
        catalog.require(path);
    }
    let Some((cam, _tf, proj)) = cam_q.iter().next() else {
        return;
    };
    let Projection::Orthographic(o) = proj else {
        return;
    };
    let Ok(win) = windows.single() else {
        return;
    };

    let map = gui_map(win.width(), win.height(), o.scale);
    let c = map.to_world(GUI_W / 2.0, GUI_H / 2.0);
    // The shader maps uv 0..1 to GUI 0..320 x 0..240, so the mesh must be
    // exactly the 320x240 GUI surface: anything wider stretches the swirl
    // (on 16:9 a full-view quad pulls every circle 33% wide). Letterbox
    // margins stay clear-colour black, exactly like GML's draw_clear + bars.
    let mesh = meshes.add(Rectangle::new(GUI_W, GUI_H));
    let [r, g, b, _] = background_color(state.get()).to_srgba().to_f32_array();
    let pending_any = pending.is_some() || pending_ultra.is_some();
    let bg_alpha = if vortex_needs_black(state.get(), ft.as_deref(), pending_any) {
        1.0
    } else {
        0.0
    };
    let kindpacked =
        ctl.kind as u8 as f32 + if ctl.gml_area == 105 { 4.0 } else { 0.0 };
    let mat = VortexMaterial {
        wisps: ring_to_uniform(&ctl.ring),
        debris: debris_to_uniform(&ctl.debris_ring),
        glob_a: Vec4::new(ctl.ticks, 1.0, r, g),
        glob_b: Vec4::new(b, bg_alpha, 2.5, kindpacked),
        spiral_tex: asset_server.load("images/sprSpiral.png"),
        bolt_tex: asset_server.load("images/sprPortalLightning.png"),
        debris_tex: asset_server
            .load(format!("images/sprDebris{}.png", ctl.gml_area)),
        spiral_proto_tex: asset_server.load("images/sprSpiralProto.png"),
        spiral_idpd_tex: asset_server.load("images/sprSpiralIDPD.png"),
        spiral_idpd2_tex: asset_server.load("images/sprSpiralIDPD2.png"),
    };
    let mat_handle = materials.add(mat);

    info!(
        "vortex: quad spawned (state {:?}, gui scale {:.3}, wisps seeded)",
        state.get(),
        map.s
    );

    let vortex_z = if *state.get() == AppState::Title {
        -845.0 // Title: in front of pods/chars (-860..-846) so vortex covers characters during drain – hide_title_during_transition already hides, lingering wisps should be over characters
    } else {
        -885.0
    };
    commands.spawn((
        VortexQuad,
        TitleArt,
        ChildOf(cam),
        Mesh2d(mesh),
        MeshMaterial2d(mat_handle),
        Transform::from_xyz(c.x, c.y, vortex_z).with_scale(Vec3::new(map.s, map.s, 1.0)),
    ));
}

/// Keep the quad glued to the live GUI surface across resizes / zoom.
fn track_vortex_view(
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    cam_q: Query<&Projection, With<Camera2d>>,
    mut q: Query<&mut Transform, With<VortexQuad>>,
    state: Res<State<AppState>>,
) {
    let Ok(mut tf) = q.single_mut() else {
        return;
    };
    let Some(Projection::Orthographic(o)) = cam_q.iter().next() else {
        return;
    };
    let Ok(win) = windows.single() else {
        return;
    };
    let map = gui_map(win.width(), win.height(), o.scale);
    let c = map.to_world(GUI_W / 2.0, GUI_H / 2.0);
    tf.translation.x = c.x;
    tf.translation.y = c.y;
    tf.translation.z = if *state.get() == AppState::Title {
        -845.0
    } else {
        -885.0
    };
    tf.scale = Vec3::new(map.s, map.s, 1.0);
}

/// PlayButton/Other_10: entering the campfire char-select destroys
/// SpiralCont (and its CleanUp stops sndPortalLoop). Runs on
/// `OnEnter(AppState::Title)`; despawn_title_art stays as a safety net.
/// GML Spiral instances linger and grow 1.5x until xscale>3, so we keep the
/// vortex quad for a brief drain instead of popping.
pub fn teardown_vortex(
    mut commands: Commands,
    q_quad: Query<Entity, With<VortexQuad>>,
    ctl: Option<ResMut<SpiralCtl>>,
    portal: Query<Entity, With<crate::game::ui_art::PortalLoop>>,
) {
    if let Some(mut c) = ctl {
        if c.alive {
            c.alive = false;
            c.death_tick = Some(c.ticks);
        }
    }
    for e in &portal {
        commands.entity(e).try_despawn();
    }
    let _ = q_quad;
}

fn ensure_spiral_for_levelup(
    mut commands: Commands,
    state: Res<State<AppState>>,
    run: Res<crate::game::components::Run>,
    ft: Option<Res<crate::game::components::FloorTransition>>,
    pending: Option<Res<crate::game::components::PendingMutation>>,
    pending_ultra: Option<Res<crate::game::components::PendingUltra>>,
    ctl: Option<Res<SpiralCtl>>,
) {
    if *state.get() != AppState::InGame {
        return;
    }
    let needs_spiral =
        ft.as_deref().is_some_and(|f| f.active) || pending.is_some() || pending_ultra.is_some();
    if !needs_spiral {
        return;
    }
    if ctl.as_deref().is_none_or(|c| !c.alive) {
        commands.insert_resource(SpiralCtl::warmed_up_for_gml_area(gml_area_for_bevy_area(
            run.area,
        )));
    }
}

fn despawn_vortex_when_done(
    mut commands: Commands,
    q_quad: Query<Entity, With<VortexQuad>>,
    ctl: Option<Res<SpiralCtl>>,
    portal: Query<Entity, With<crate::game::ui_art::PortalLoop>>,
    state: Res<State<AppState>>,
) {
    let Some(ctl) = ctl else {
        return;
    };
    if ctl.alive {
        return;
    }
    // GML clears every survivor within ~21 ticks of SpiralCont's death
    // (compounding grow vs the xscale-3 plane), so the quad only needs a
    // small margin beyond that before the reveal completes.
    let drain = 26.0;
    if let Some(death) = ctl.death_tick {
        if ctl.ticks - death < drain {
            return;
        }
    } else {
        return;
    }
    if matches!(*state.get(), AppState::Loading) {
        return;
    }
    if !matches!(*state.get(), AppState::Title | AppState::InGame) {
        return;
    }
    for e in &q_quad {
        commands.entity(e).try_despawn();
    }
    for e in &portal {
        commands.entity(e).try_despawn();
    }
    commands.remove_resource::<SpiralCtl>();
}

fn mark_vortex_dead(ctl: Option<ResMut<SpiralCtl>>) {
    if let Some(mut c) = ctl {
        if c.alive {
            c.alive = false;
            c.death_tick = Some(c.ticks);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::ui_art::{GUI_H, GUI_W};

    #[test]
    fn spiral_cpu_layer_system_params_are_disjoint() {
        // B0001 (conflicting queries) fires at system init, so initializing
        // the system on an empty world reproduces the startup panic headless.
        let mut world = World::new();
        let mut sys =
            bevy::ecs::system::IntoSystem::into_system(sync_spiral_cpu_layer);
        bevy::ecs::system::System::initialize(&mut sys, &mut world);
    }

    #[test]
    fn orbit_matches_public_rewrite() {
        let (x, y) = orbit(0.0);
        assert!((x - GUI_W / 2.0).abs() < 1e-3);
        assert!((y - GUI_H / 2.0).abs() < 1e-3);
        let mut max_dx: f32 = 0.0;
        let mut max_dy: f32 = 0.0;
        let mut a: f32 = 0.0;
        while a < 200000.0 {
            let (x2, y2) = orbit(a);
            max_dx = max_dx.max((x2 - GUI_W / 2.0).abs());
            max_dy = max_dy.max((y2 - GUI_H / 2.0).abs());
            assert!((x2 - GUI_W / 2.0).abs() <= 80.0 + 1e-3);
            assert!((y2 - GUI_H / 2.0).abs() <= 50.0 + 1e-3);
            a += 137.0;
        }
        assert!(max_dx > 40.0, "orbit never wanders in x: {max_dx}");
        assert!(max_dy > 25.0, "orbit never wanders in y: {max_dy}");
    }

    #[test]
    fn spiral_kind_follows_gml_areas() {
        use SpiralKind::*;
        assert_eq!(SpiralKind::for_gml_area(0), Normal);
        assert_eq!(SpiralKind::for_gml_area(1), Normal);
        assert_eq!(SpiralKind::for_gml_area(7), Normal);
        assert_eq!(SpiralKind::for_gml_area(100), Proto);
        assert_eq!(SpiralKind::for_gml_area(106), Idpd);
        assert_eq!(SpiralKind::for_gml_area(103), Venuz);
        assert_eq!(SpiralKind::for_gml_area(107), Venuz);
    }

    #[test]
    fn bevy_areas_map_to_gml_area_ints() {
        use crate::game::areas::AreaId;
        let cases = [
            (AreaId::Campfire, 0),
            (AreaId::Desert, 1),
            (AreaId::Sewers, 2),
            (AreaId::Scrapyards, 3),
            (AreaId::CrystalCaves, 4),
            (AreaId::FrozenCity, 5),
            (AreaId::Labs, 6),
            (AreaId::Palace, 7),
            (AreaId::Vault, 100),
            (AreaId::Oasis, 101),
            (AreaId::PizzaSewers, 102),
            (AreaId::City, 103),
            (AreaId::CursedCaves, 104),
            (AreaId::Jungle, 105),
            (AreaId::HQ, 106),
            (AreaId::CrownVault, 100),
            (AreaId::Loop, 1),
        ];
        for (area, want) in cases {
            assert_eq!(gml_area_for_bevy_area(area), want, "{area:?}");
        }
    }

    #[test]
    fn crown_fig_paths_use_gml_crown_ints() {
        use crate::game::content::CrownKind;
        let cases = [
            (CrownKind::None, None),
            (CrownKind::Death, Some("images/sprCrown2Idle.png")),
            (CrownKind::Life, Some("images/sprCrown3Idle.png")),
            (CrownKind::Haste, Some("images/sprCrown4Idle.png")),
            (CrownKind::Guns, Some("images/sprCrown5Idle.png")),
            (CrownKind::Hatred, Some("images/sprCrown6Idle.png")),
            (CrownKind::Blood, Some("images/sprCrown7Idle.png")),
            (CrownKind::Destiny, Some("images/sprCrown8Idle.png")),
            (CrownKind::Love, Some("images/sprCrown9Idle.png")),
            (CrownKind::Luck, Some("images/sprCrown10Idle.png")),
            (CrownKind::Curses, Some("images/sprCrown11Idle.png")),
            (CrownKind::Risk, Some("images/sprCrown12Idle.png")),
            (CrownKind::Protection, Some("images/sprCrown13Idle.png")),
        ];
        for (crown, want) in cases {
            assert_eq!(crown_fig_path(crown), want, "{crown:?}");
        }
    }

    #[test]
    fn venuz_emits_stars_not_wisps() {
        let ctl = SpiralCtl::warmed_up_for_gml_area(103);
        assert_eq!(ctl.kind, SpiralKind::Venuz);
        let wisps = ctl.ring.iter().filter(|s| s[2] >= 0.0).count();
        assert_eq!(wisps, 0, "venuz must not emit normal wisps");
        let stars = ctl.stars.iter().filter(|s| s.alive).count();
        assert!(stars > 40, "venuz warmup should hold a starfield, got {stars}");
    }

    #[test]
    fn idpd_spawns_centered_with_both_variants() {
        let ctl = SpiralCtl::warmed_up_for_gml_area(106);
        assert_eq!(ctl.kind, SpiralKind::Idpd);
        for s in ctl.ring.iter().filter(|s| s[2] >= 0.0) {
            assert!((s[0] - 160.0).abs() < 1e-3, "idpd wisp off-center x");
            assert!((s[1] - 120.0).abs() < 1e-3, "idpd wisp off-center y");
        }
        let neg = ctl.ring.iter().filter(|s| s[2] >= 0.0 && s[3] < 0.0).count();
        let pos = ctl.ring.iter().filter(|s| s[2] >= 0.0 && s[3] >= 0.0).count();
        assert!(neg > 0 && pos > 0, "idpd2 variant never/always taken ({neg}/{pos})");
    }

    #[test]
    fn proto_angles_advance_faster_than_normal() {
        let mut proto = SpiralCtl::warmed_up_for_gml_area(100);
        let a0 = proto.angle;
        for _ in 0..30 {
            proto.tick_once();
        }
        let da = proto.angle - a0;
        // 10 +/- 3 per tick (sin*2 + orandom(1)).
        assert!(da > 30.0 * 6.0 && da < 30.0 * 14.0, "proto rate off: {da}");
    }

    #[test]
    fn vortex_tick_rate_matches_gml_30hz() {
        // Exact 30 Hz cadence alive AND dead (post-death acceleration is
        // per-wisp birth rewind, never a clock overdrive).
        let mut ctl = SpiralCtl::warmed_up();
        let t0 = ctl.ticks;
        ctl.step(30.0);
        assert!((ctl.ticks - t0 - 30.0).abs() < 1e-3);
        ctl.alive = false;
        ctl.death_tick = Some(ctl.ticks);
        let t1 = ctl.ticks;
        ctl.step(30.0);
        assert!((ctl.ticks - t1 - 30.0).abs() < 1e-3);
    }

    #[test]
    fn vortex_drain_clears_like_gml_compounding() {
        // GML kills every survivor within ~21 ticks of SpiralCont's death.
        // Ages must fast-forward (~6.5x) so the youngest wisp (age 0) passes
        // the shader kill plane (scale 2.5 at table age ~117) by tick ~20.
        let mut ctl = SpiralCtl::warmed_up();
        let youngest = ctl
            .ring
            .iter()
            .filter(|s| s[2] >= 0.0)
            .map(|s| ctl.ticks - s[2])
            .fold(f32::INFINITY, f32::min);
        assert!(youngest < 130.0, "warmup left no live wisps");
        ctl.alive = false;
        ctl.death_tick = Some(ctl.ticks);
        for _ in 0..5 {
            ctl.tick_once();
        }
        let youngest_after = ctl
            .ring
            .iter()
            .filter(|s| s[2] >= 0.0)
            .map(|s| ctl.ticks - s[2])
            .fold(f32::INFINITY, f32::min);
        assert!(
            youngest_after - youngest >= 30.0,
            "drain aging too slow: {youngest} -> {youngest_after}"
        );
        for _ in 0..20 {
            ctl.tick_once();
        }
        let stale = ctl
            .ring
            .iter()
            .filter(|s| s[2] >= 0.0)
            .filter(|s| ctl.ticks - s[2] <= 130.0)
            .count();
        assert_eq!(stale, 0, "{stale} wisps still under the kill plane");
    }
}

pub struct VortexPlugin;

impl Plugin for VortexPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(Material2dPlugin::<VortexMaterial>::default())
            .add_systems(OnEnter(AppState::InGame), mark_vortex_dead)
            .add_systems(OnExit(AppState::Loading), mark_vortex_dead)
            .add_systems(
                Update,
                (
                    ensure_spiral_for_levelup,
                    ensure_vortex_quad,
                    track_vortex_view,
                    vortex_tick,
                    sync_spiral_cpu_layer,
                    despawn_vortex_when_done,
                ),
            );
    }
}
