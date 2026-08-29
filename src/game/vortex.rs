//! Portal vortex as a single WGSL quad (replaces the per-wisp entity swarm).
//!
//! Fidelity source: `nt-recreated-public-rewrite` `SpiralCont/Step_0.gml`
//! orbit `80/50` (public rewrite `* 80 / * 50`). Commercial binary uses
//! `130/90` (recovered from `nuclearthrone` YYC constant pool `921,500,583,130,90`)
//! but this port tracks the public rewrite per spec.
//!   - `objects/SpiralCont/Create_0.gml`  (warmup `repeat 150`, orbit drift)
//!   - `objects/SpiralCont/Step_0.gml`    (angle += 8 + sin_deg(a/300), spawn 1/tick)
//!   - `objects/Spiral/Step_0.gml`        (grow law, destroy xscale>2.5)
//!   - `scripts/scrDrawSpiral/scrDrawSpiral.gml` (white+black passes, lightning)
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
    /// [x, y, birth_tick, rot_rad]; birth < 0 = empty slot.
    pub ring: Vec<[f32; 4]>,
    head: usize,
    /// `SpiralDebris` instances (spawned by SpiralCont/Step_0 at 1/48 per tick).
    debris: Vec<Debris>,
    /// Render-ready debris mirror: [x, y, rot_rad, frame + xscale/32];
    /// x < -100 = empty slot.
    pub debris_ring: Vec<[f32; 4]>,
    dhead: usize,
}

impl SpiralCtl {
    /// Mirrors `SpiralCont/Create_0.gml:36` `repeat 150 { Step; with Spiral Step }`.
    /// 150 ticks are simulated verbatim; the ring ends up with the 128 most
    /// recent spawns (the oldest 22 have wrapped). Survivors are exactly those
    /// the GML would have kept - ~119 with age < 120. Keeping dead slots in
    /// the ring is faithful: the shader culls s>2.5 the same way GML destroys
    /// instances when `image_xscale > 2.5`.
    pub fn warmed_up() -> Self {
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
        };
        for _ in 0..WARMUP_TICKS {
            ctl.tick_once();
        }
        ctl
    }

    /// One 30 Hz tick: SpiralCont/Step_0 + the debris spawn check and every
    /// SpiralDebris/Step_0 - shared by the warmup and the live clock so the
    /// field state is identical however the controller was armed.
    fn tick_once(&mut self) {
        self.ticks += 1.0;
        // SpiralCont/Step_0: increment angle, then emit one wisp there.
        self.angle += spiral_angle_inc(self.angle);
        let (x, y) = orbit(self.angle);
        self.ring[self.head] = [x, y, self.ticks, (self.angle + 45.0).to_radians()];
        self.head = (self.head + 1) % MAX_WISPS;

        // Debris spawn: `random(16) < 1 && random(3) < 1` (Normal type, menus).
        if rand::random::<f32>() * 16.0 < 1.0 && rand::random::<f32>() * 3.0 < 1.0 {
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
                image_angle: rand::random::<f32>() * 360.0,
                frame: (rand::random::<f32>() * 4.0).floor().min(3.0),
            };
            self.dhead = (self.dhead + 1) % MAX_DEBRIS;
        }

        // SpiralDebris/Step_0 (exact order): position from current state,
        // then advance angle/dist/grow/xscale, then self-rotation.
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
    }

    fn step(&mut self, dt_ticks: f32) {
        self.acc += dt_ticks;
        while self.acc >= 1.0 {
            self.acc -= 1.0;
            self.tick_once();
        }
    }
}

/// SpiralCont/Step_0.gml:5 - Normal-type increment (degrees).
fn spiral_angle_inc(angle: f32) -> f32 {
    8.0 + deg_sin(angle / 300.0)
}

/// SpiralCont/Step_0.gml:18-19 orbit around the GUI centre (GML sin/cos take
/// DEGREES; bevy takes radians, hence the conversions). Public rewrite
/// `Step_0` uses `* 80 / * 50`; commercial binary `130/90` is intentionally
/// not used (see module header).
fn orbit(angle: f32) -> (f32, f32) {
    (
        // public rewrite Step_0: * 80 / * 50 (not commercial 130/90)
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
    /// (bg_b, 0, 0, 0)
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

    // The shader composites everything onto an opaque background colour, so
    // the quad itself is opaque (matches scrDrawSpiral's draw_clear).
    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Opaque
    }
}

/// Marker for the fullscreen vortex quad (despawned with other title art).
#[derive(Component)]
pub struct VortexQuad;

/// States where `SpiralCont` exists upstream: created with the Logo
/// (Vlambeer/Alarm_0 mode >= 3), kept through the main-menu buttons, and
/// destroyed right before the campfire char-select (PlayButton/Other_10).
fn spiral_states(state: &AppState) -> bool {
    matches!(*state, AppState::Splash | AppState::MainMenu)
}

/// Background colour behind the swirl: scrDrawSpiral does `draw_clear(c_black)`
/// for every non-`Menu` caller, and both of our swirl states (Splash logo,
/// MainMenu buttons) are non-`Menu` - so always black. The campfire blue only
/// applies to the char-select, where the swirl is destroyed.
fn background_color(_state: &AppState) -> Color {
    Color::BLACK
}

/// Advance the controller and mirror its ring into the material. One system,
/// zero per-wisp entities. Runs in Update with an internal 30 Hz accumulator
/// (`Time<Virtual>` respects pause/slow-mo), so the tick rate is exact and
/// independent of both render fps and the gameplay FixedUpdate cadence.
fn vortex_tick(
    state: Res<State<AppState>>,
    time: Res<Time<Virtual>>,
    mut ctl: Option<ResMut<SpiralCtl>>,
    mut materials: ResMut<Assets<VortexMaterial>>,
    q_mat: Query<&MeshMaterial2d<VortexMaterial>, With<VortexQuad>>,
) {
    let Some(ctl) = ctl.as_mut() else {
        return;
    };
    if !spiral_states(state.get()) {
        return;
    }

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
    // Lightning is on whenever the swirl draws: scrDrawSpiral only disables it
    // (`_is_menu`) for the char-select `Menu`, where SpiralCont no longer
    // exists. Both of our swirl states (Splash logo, MainMenu buttons) are
    // non-Menu callers.
    mat.glob_a = Vec4::new(ctl.ticks, 1.0, r, g);
    mat.glob_b = Vec4::new(b, 0.0, 0.0, 0.0);
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
) {
    // SpiralCont only exists from the logo stage onward: boot_intro arms the
    // controller at mode 4 and spawn_spiral_field re-arms it on quit-to-menu.
    // Gating on the resource keeps the swirl off the splash cards (modes 0-3).
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
        "images/sprPortalLightning.png",
        "images/sprDebris0.png",
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

    let mesh = meshes.add(Rectangle::new(GUI_W, GUI_H));
    let [r, g, b, _] = background_color(state.get()).to_srgba().to_f32_array();
    let mat = VortexMaterial {
        wisps: ring_to_uniform(&ctl.ring),
        debris: debris_to_uniform(&ctl.debris_ring),
        glob_a: Vec4::new(ctl.ticks, 1.0, r, g),
        glob_b: Vec4::new(b, 0.0, 0.0, 0.0),
        spiral_tex: asset_server.load("images/sprSpiral.png"),
        bolt_tex: asset_server.load("images/sprPortalLightning.png"),
        debris_tex: asset_server.load("images/sprDebris0.png"),
    };
    let mat_handle = materials.add(mat);

    info!(
        "vortex: quad spawned (state {:?}, gui scale {:.3}, wisps seeded)",
        state.get(),
        map.s
    );

    commands.spawn((
        VortexQuad,
        TitleArt,
        ChildOf(cam),
        Mesh2d(mesh),
        MeshMaterial2d(mat_handle),
        // Layering per __global_object_depths.gml: SpiralCont=-101 renders
        // ABOVE Floor(10)/Wall/Campfire(0) but BELOW Menu(-1001). On our z
        // scale the boot/menu cards sit at -802..-800.5, so the quad slots
        // below them while staying above the scene clear.
        Transform::from_xyz(c.x, c.y, -862.5).with_scale(Vec3::new(map.s, map.s, 1.0)),
    ));
}

/// Keep the quad glued to the live GUI surface across resizes / zoom.
fn track_vortex_view(
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    cam_q: Query<&Projection, With<Camera2d>>,
    mut q: Query<&mut Transform, With<VortexQuad>>,
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
    tf.scale = Vec3::new(map.s, map.s, 1.0);
}

/// PlayButton/Other_10: entering the campfire char-select destroys
/// SpiralCont (and its CleanUp stops sndPortalLoop). Runs on
/// `OnEnter(AppState::Title)`; despawn_title_art stays as a safety net.
pub fn teardown_vortex(
    mut commands: Commands,
    q_quad: Query<Entity, With<VortexQuad>>,
    ctl: Option<Res<SpiralCtl>>,
    portal: Query<Entity, With<crate::game::ui_art::PortalLoop>>,
) {
    for e in &q_quad {
        commands.entity(e).try_despawn();
    }
    for e in &portal {
        commands.entity(e).try_despawn();
    }
    if ctl.is_some() {
        commands.remove_resource::<SpiralCtl>();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::ui_art::{GUI_H, GUI_W};

    #[test]
    fn orbit_matches_public_rewrite() {
        // public rewrite 80/50, not commercial 130/90
        let (x, y) = orbit(0.0);
        assert!((x - GUI_W / 2.0).abs() < 1e-3);
        assert!((y - GUI_H / 2.0).abs() < 1e-3);
        let (x2, y2) = orbit(500.0);
        // sin(500/500)=sin1 ~0.84, so x offset ~80*0.84*sin(500/921) etc, verify within 80
        assert!((x2 - GUI_W / 2.0).abs() <= 80.0 + 1e-3);
        assert!((y2 - GUI_H / 2.0).abs() <= 50.0 + 1e-3);
    }
}

pub struct VortexPlugin;

impl Plugin for VortexPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(Material2dPlugin::<VortexMaterial>::default())
            .add_systems(Update, (ensure_vortex_quad, track_vortex_view, vortex_tick));
    }
}
