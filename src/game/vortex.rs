//! Portal vortex as a single WGSL quad (replaces the per-wisp entity swarm).
//!
//! Fidelity source: `~/Downloads/nt-recreated-public-rewrite`
//!   - `objects/Vlambeer/Alarm_0.gml`     (SpiralCont created with the Logo)
//!   - `objects/PlayButton/Other_10.gml`  (destroyed before campfire Menu)
//!   - `objects/SpiralCont/Create_0.gml`  (warmup `repeat 150`, orbit drift)
//!   - `objects/SpiralCont/Step_0.gml`    (angle += 8 + sin_deg(a/300), spawn 1/tick)
//!   - `objects/Spiral/Step_0.gml`        (grow law, destroy xscale>2.5)
//!   - `scripts/scrDrawSpiral/scrDrawSpiral.gml` (white+black passes, lightning)
//!
//! The CPU only advances the controller clock and maintains a ring of
//! `[x, y, birth_tick, rot]` vec4s; every wisp's growth, fade, lightning and
//! compositing happen in the fragment shader (`assets/shaders/vortex.wgsl`).
//! One draw call regardless of wisp count — no entity churn, no startup stall.

use bevy::prelude::*;
use bevy::render::render_resource::AsBindGroup;
use bevy::shader::ShaderRef;
use bevy::sprite_render::{AlphaMode2d, Material2d, Material2dPlugin};

use crate::app::AppState;
use crate::game::ui_art::{gui_map, TitleArt, GUI_H, GUI_W};

pub const MAX_WISPS: usize = 128;
/// Spiral/Step_0: wisps die at xscale > 2.5 ≙ tick ~118 at 30 Hz.
const WISP_LIFETIME_TICKS: f32 = 120.0;
/// Create_0 fast-forward so the field is fully established on frame one
/// instead of fading in over minutes of idling.
const WARMUP_TICKS: u32 = 240;

/// The `SpiralCont` driver state.
#[derive(Resource)]
pub struct SpiralCtl {
    /// `image_angle` — accumulates UNBOUNDED like GML (never wraps %360);
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
}

impl SpiralCtl {
    /// Spawned with the `repeat 150` warmup already simulated
    /// (SpiralCont/Create_0.gml:36) plus extra aged wisps so growth has
    /// visible arcs immediately.
    pub fn warmed_up() -> Self {
        let mut ctl = Self {
            angle: rand::random::<f32>() * 360.0,
            ticks: 0.0,
            acc: 0.0,
            ring: vec![[-1.0; 4]; MAX_WISPS],
            head: 0,
        };
        let n = 110.min(MAX_WISPS);
        for i in 0..n {
            ctl.angle += spiral_angle_inc(ctl.angle);
            let (x, y) = orbit(ctl.angle);
            // Staggered births spread over the warmup window: oldest first.
            let age = WARMUP_TICKS as f32 * (1.0 - i as f32 / n as f32);
            let birth = WARMUP_TICKS as f32 - age;
            if age < WISP_LIFETIME_TICKS {
                ctl.ring[ctl.head] = [x, y, birth, (ctl.angle + 45.0).to_radians()];
                ctl.head = (ctl.head + 1) % MAX_WISPS;
            }
        }
        ctl.ticks = WARMUP_TICKS as f32;
        ctl
    }

    fn step(&mut self, dt_ticks: f32) {
        self.acc += dt_ticks;
        while self.acc >= 1.0 {
            self.acc -= 1.0;
            self.ticks += 1.0;
            // SpiralCont/Step_0: increment angle, then emit one wisp there.
            self.angle += spiral_angle_inc(self.angle);
            let (x, y) = orbit(self.angle);
            self.ring[self.head] = [x, y, self.ticks, (self.angle + 45.0).to_radians()];
            self.head = (self.head + 1) % MAX_WISPS;
        }
    }
}

/// SpiralCont/Step_0.gml:5 — Normal-type increment (degrees).
fn spiral_angle_inc(angle: f32) -> f32 {
    8.0 + deg_sin(angle / 300.0)
}

/// SpiralCont/Step_0.gml:18-19 orbit around the GUI centre (GML sin/cos take
/// DEGREES; bevy takes radians, hence the conversions).
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

// ---------------------------------------------------------------------------
// GPU material
// ---------------------------------------------------------------------------

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
}

fn ring_to_uniform(ring: &[[f32; 4]]) -> [Vec4; MAX_WISPS] {
    let mut out = [Vec4::NEG_ONE; MAX_WISPS];
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

/// Background colour behind the swirl: campfire-menu blue on menus, black
/// during the splash logo (scrDrawSpiral draw_clear behaviour).
fn background_color(state: &AppState) -> Color {
    if *state == AppState::Splash {
        Color::BLACK
    } else {
        Color::srgb_u8(0x6a, 0x7a, 0xaf) // scrAreaGetBackround(area_campfire)
    }
}

/// Advance the controller and mirror its ring into the material. One system,
/// zero per-wisp entities.
fn vortex_tick(
    state: Res<State<AppState>>,
    time: Res<Time<Fixed>>,
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
    let [r, g, b, _] = background_color(state.get()).to_srgba().to_f32_array();
    // Lightning runs everywhere EXCEPT the Menu screen (`_is_menu` gate in
    // scrDrawSpiral.gml:8); our Menu-equivalent state is MainMenu.
    mat.glob_a = Vec4::new(
        ctl.ticks,
        f32::from(*state.get() != AppState::MainMenu),
        r,
        g,
    );
    mat.glob_b = Vec4::new(b, 0.0, 0.0, 0.0);
}

/// Spawn the vortex quad once; keeps `SpiralCtl` alive alongside it.
#[allow(clippy::type_complexity)]
fn ensure_vortex_quad(
    mut commands: Commands,
    state: Res<State<AppState>>,
    asset_server: Res<AssetServer>,
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
        glob_a: Vec4::new(
            ctl.ticks,
            f32::from(*state.get() != AppState::MainMenu),
            r,
            g,
        ),
        glob_b: Vec4::new(b, 0.0, 0.0, 0.0),
        spiral_tex: asset_server.load("images/sprSpiral.png"),
        bolt_tex: asset_server.load("images/sprPortalLightning.png"),
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

pub struct VortexPlugin;

impl Plugin for VortexPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(Material2dPlugin::<VortexMaterial>::default()).add_systems(
            Update,
            (ensure_vortex_quad, track_vortex_view, vortex_tick),
        );
    }
}
