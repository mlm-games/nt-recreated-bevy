//! Nuclear Throne GUI art (nt-rewrite draw events) rendered as
//! camera-anchored world sprites. All placement uses NT's 320x240 logical
//! GUI coordinate system mapped 1:1 into camera space; sprites keep their
//! native dimensions and GameMaker origins (from anims.json).

use bevy::audio::AudioSource;
use bevy::audio::{AudioPlayer, PlaybackMode, PlaybackSettings, Volume};
use bevy::ecs::query::QueryFilter;
use bevy::prelude::*;
use rand::RngExt;

use crate::app::AppState;
use crate::game::components::{Health, Inventory, Player};
use crate::game::content::AmmoKind;
use crate::game::content::{AssetCatalog, CHAR_SELECT_RACES, WeaponId, sprite_exact};
use crate::menus::UiBridge;
use game_utils_bevy::screen_effects::CameraBase;
use game_utils_bevy::transitions::Transition;

/// Marker for menu art (title backdrop); despawned on state exit.
#[derive(Component)]
pub struct TitleArt;

/// Marker for in-game HUD art; despawned with the level.
#[derive(Component)]
pub struct HudArt;

/// Handles for the HUD pieces that update every tick.
#[derive(Resource)]
pub struct HudArtRefs {
    /// Dark background strip (frame 2) and health fill strip (frame 1).
    pub hp_bg: Entity,
    pub hp_fg: Entity,
    /// Rad thermometer (frame = fraction * 16) and LEVEL UP overlay.
    pub exp_bar: Entity,
    pub exp_level: Entity,
    /// Per NT ammo type (Bullets..Energy): background + fill icon.
    pub ammo_bg: [Entity; 5],
    pub ammo_icon: [Entity; 5],
    /// Primary/secondary weapon icon: four outline copies + black body.
    pub wep: [([Entity; 4], Entity); 2],
    /// Weapon gml ids currently rendered (texture-swap dedup).
    pub wep_ids: [u8; 2],
}

/// nt-rewrite GUI constants (macros_general.gml, scrDrawSpiral.gml).
const GUI_W: f32 = 320.0;
const GUI_H: f32 = 240.0;
const LETTERBOX_SIZE: f32 = 36.0;
const POD_W: f32 = 16.0;
const POD_H: f32 = 24.0;
const SLOT_XSTART: f32 = 8.0;

pub const CAM_SCALE: f32 = 0.45;

/// The 320x240 NT GUI surface, uniformly scaled and letterboxed inside the
/// camera view (exactly how GameMaker's GUI layer behaves).
///
/// `s` is world units per NT pixel; `ox`/`oy` are the centered margins in
/// world units. Derived from the *live* ortho scale so gameplay zoom keeps
/// the surface glued to the same screen rect.
struct GuiMap {
    s: f32,
    ox: f32,
    oy: f32,
    hw: f32,
    hh: f32,
}

fn gui_map(win_w: f32, win_h: f32, cam_scale: f32) -> GuiMap {
    let hw = win_w * cam_scale * 0.5;
    let hh = win_h * cam_scale * 0.5;
    let s = ((hw * 2.0) / GUI_W).min((hh * 2.0) / GUI_H);
    GuiMap {
        s,
        ox: ((hw * 2.0) - GUI_W * s) * 0.5,
        oy: ((hh * 2.0) - GUI_H * s) * 0.5,
        hw,
        hh,
    }
}

impl GuiMap {
    fn to_world(&self, x: f32, y: f32) -> Vec2 {
        Vec2::new(
            -self.hw + self.ox + x * self.s,
            self.hh - self.oy - y * self.s,
        )
    }

    fn to_gui(&self, p: Vec2) -> Vec2 {
        Vec2::new(
            (p.x + self.hw - self.ox) / self.s,
            (self.hh - p.y - self.oy) / self.s,
        )
    }
}

/// GameMaker builtin `c_gray` — unselected char-select pods.
const C_GRAY: Color = Color::srgb_u8(128, 128, 128);
/// `#999999` (`c_uigray`, macros_gameplay.gml) — unhovered GoButton.
const C_UIGRAY: Color = Color::srgb_u8(153, 153, 153);

/// Slot geometry reproduced from nt-rewrite `Menu/Create_0`.
pub fn slot_ystart() -> f32 {
    GUI_H - POD_H - ((LETTERBOX_SIZE - POD_H) / 2.0).floor()
}

fn slot_step(count: usize) -> f32 {
    20.0f32.min(((GUI_W - 40.0) / (count as f32).max(1.0)).floor())
}

fn slot_x(i: usize, step: f32) -> f32 {
    SLOT_XSTART + step * i as f32
}

/// GoButton placement from `Menu/Create_0`: right of the last slot, sunk
/// into the letterbox by half its bbox height minus 2.
fn go_button_pos(step: f32, count: usize) -> (f32, f32) {
    let last_x = slot_x(count - 1, step);
    let bbox_half_h = (19.0_f32 / 2.0).floor();
    (
        last_x + step + 2.0,
        GUI_H - LETTERBOX_SIZE + bbox_half_h - 2.0,
    )
}

/// Metadata row from anims.json: [frames, w, h, fps, xorigin, yorigin].
type SpriteMeta = [f32; 6];

fn meta_of(catalog: &AssetCatalog, path: &str) -> SpriteMeta {
    catalog
        .anims
        .get(path)
        .copied()
        .unwrap_or([1.0, 16.0, 16.0, 0.0, 8.0, 8.0])
}

fn race_skin_subimage(race: usize, skin: u8) -> i32 {
    if race == 0 {
        return -1;
    }
    let r = race as i32;
    let s = skin as i32;
    if s < 2 {
        s + (r - 1) * 2
    } else {
        s * 16 + (r - 1)
    }
}

fn loadout_available(race: usize) -> bool {
    // Mirrors scr_loadout_is_available_for_race: false for BigDog(13), Skeleton(14), Frog(15)
    !matches!(race, 13 | 14 | 15)
}

fn max_skin_count(race: usize) -> usize {
    match race {
        13 | 15 => 1, // BigDog, Frog
        14 => 2,      // Skeleton (without secret, 2; with secret also 2)
        8 => 4,       // Robot
        _ => 3,
    }
}

/// One `draw_sprite_ext(sprite, subimage, x, y, xscale, yscale, angle,
/// blend, alpha)` translation. `gui_x/gui_y` are the GM drawing point
/// (origin-relative): left = x - xorigin*xscale, top = y - yorigin*yscale.
#[allow(clippy::too_many_arguments)]
fn gm_sprite(
    catalog: &AssetCatalog,
    assets: &AssetServer,
    map: &GuiMap,
    path: &'static str,
    frame: usize,
    gui_x: f32,
    gui_y: f32,
    xscale: f32,
    yscale: f32,
    tint: Color,
    z: f32,
) -> (Sprite, Transform) {
    let m = meta_of(catalog, path);
    let (_frames, w, h, _fps, xorigin, yorigin) = (m[0], m[1], m[2], m[3], m[4], m[5]);
    let fw = w.max(1.0);
    let fh = h.max(1.0);
    let frame_count = m[0].max(1.0) as usize;
    let frame = frame % frame_count.max(1);
    let mut sprite = sprite_exact(catalog, assets, path);
    // Source rectangle = frame rectangle (strips are horizontal).
    sprite.rect = Some(Rect::new(
        frame as f32 * fw,
        0.0,
        (frame as f32 + 1.0) * fw,
        fh,
    ));
    sprite.color = tint;
    // Native dimensions in NT pixels; GuiMap.s scales the whole surface.
    sprite.custom_size = Some(Vec2::new(fw * xscale * map.s, fh * yscale * map.s));

    let left = gui_x - xorigin * xscale;
    let top = gui_y - yorigin * yscale;
    let center = map.to_world(left + fw * xscale / 2.0, top + fh * yscale / 2.0);

    (sprite, Transform::from_xyz(center.x, center.y, z))
}

// ---------------------------------------------------------------------------
// Boot logo (nt-rewrite object `Logo`: sprLogo centred on the GUI)
// ---------------------------------------------------------------------------

/// The full Vlambeer boot sequence (objects `Vlambeer` + `Logo`):
///
/// mode 0: sprSaving icon + "do not turn off" note      (120 ticks)
/// mode 1: "MADE IN GAMEMAKER"                          (60 ticks)
/// mode 2: sprVlambeer card + additive glow            (120 ticks)
/// mode 3: team credits                                 (60 ticks)
/// mode 4: NT logo — frame-stepped machinegun intro,    (input)
///         then any key/click -> main menu buttons.
#[derive(Resource)]
struct BootState {
    mode: u8,
    t: f32,
    da: f32,
    shake: f32,
    guns: u8,
    booms: bool,
    rendered_mode: i8,
    spawned: Vec<Entity>,
}

impl Default for BootState {
    fn default() -> Self {
        Self {
            mode: 0,
            t: 0.0,
            da: 0.0,
            shake: 0.0,
            guns: 0,
            booms: false,
            rendered_mode: -1,
            spawned: Vec::new(),
        }
    }
}

fn reset_boot(mut boot: ResMut<BootState>) {
    *boot = BootState::default();
}

fn despawn_boot_art(mut commands: Commands, q: Query<Entity, With<BootArt>>) {
    for e in &q {
        commands.entity(e).despawn();
    }
}

/// Quit-to-menu path: SpiralCont was destroyed with the run, rebuild it.
#[allow(clippy::type_complexity)]
fn spawn_spiral_field(
    mut commands: Commands,
    state: Res<State<AppState>>,
    ctl: Option<Res<SpiralCtl>>,
    portal: Query<(), With<PortalLoop>>,
    catalog: Option<Res<AssetCatalog>>,
    asset_server: Res<AssetServer>,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    cam_q: Query<(Entity, &Transform, &Projection), With<Camera2d>>,
) {
    if *state.get() != AppState::MainMenu || ctl.is_some() {
        return;
    }
    let Some(catalog) = catalog else {
        return;
    };
    let Some((cam, map)) = view_setup(&windows, &cam_q) else {
        return;
    };
    commands.insert_resource(SpiralCtl {
        angle: rand::random::<f32>() * 360.0,
    });
    for _ in 0..150 {
        spawn_spiral_wisp(
            &mut commands,
            &catalog,
            &asset_server,
            cam,
            &map,
            SpiralCtl {
                angle: rand::random::<f32>() * 360.0,
            },
            Some(rand::random::<f32>() * 1.2),
        );
    }
    if catalog.has_audio("audio/sndPortalLoop.wav") && portal.is_empty() {
        commands.spawn((
            PortalLoop,
            AudioPlayer::<AudioSource>::new(asset_server.load("audio/sndPortalLoop.wav")),
            PlaybackSettings {
                mode: PlaybackMode::Loop,
                volume: Volume::Linear(0.5),
                ..default()
            },
        ));
    }
}

/// Marker for all boot-sequence sprites (rebuilt per mode).
#[derive(Component)]
struct BootArt;

/// Looping logo ambience; stops when the logo is dismissed (Logo/Destroy_0).
#[derive(Component)]
struct SplashLoop;

/// Looping portal drone started with SpiralCont; lives until the run starts.
#[derive(Component)]
struct PortalLoop;

fn despawn_splash_loop(mut commands: Commands, q: Query<Entity, With<SplashLoop>>) {
    for e in &q {
        commands.entity(e).despawn();
    }
}

fn despawn_portal_loop(mut commands: Commands, q: Query<Entity, With<PortalLoop>>) {
    for e in &q {
        commands.entity(e).despawn();
    }
}

fn play_cue(
    commands: &mut Commands,
    catalog: &AssetCatalog,
    asset_server: &AssetServer,
    name: &str,
    volume: f32,
) {
    let path = format!("audio/{name}.wav");
    if !catalog.has_audio(&path) {
        return;
    }
    commands.spawn((
        AudioPlayer::<AudioSource>::new(asset_server.load(path)),
        PlaybackSettings {
            mode: PlaybackMode::Despawn,
            volume: Volume::Linear(volume),
            ..default()
        },
    ));
}

/// Card stage lengths. Upstream Vlambeer/Create_0 sets alarm[0]=120 and
/// Alarm_0 re-arms 60 (+60 for the Vlambeer card) at a game speed of
/// 30 fps (UberCont Step_0: game_set_speed(30, gamespeed_fps)).
const MODE_SECS: [f32; 4] = [4.0, 2.0, 4.0, 2.0];

/// The boot driver: mode timers, input skipping, per-mode sprites, the logo
/// sound script, and the transition to the main-menu buttons.
#[allow(clippy::type_complexity)]
fn boot_intro(
    mut commands: Commands,
    time: Res<Time>,
    state: Res<State<AppState>>,
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    mut transition: ResMut<Transition<AppState>>,
    catalog: Option<Res<AssetCatalog>>,
    asset_server: Res<AssetServer>,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    cam_q: Query<(Entity, &Transform, &Projection), (With<Camera2d>, Without<BootArt>)>,
    bridge: Res<UiBridge>,
    mut boot: ResMut<BootState>,
    mut sprites: Query<&mut Sprite, With<BootArt>>,
    mut transforms: Query<&mut Transform, With<BootArt>>,
) {
    if *state.get() != AppState::Splash {
        return;
    }
    let Some(catalog) = catalog else {
        return;
    };
    let dt = time.delta_secs();
    if let Ok(mut ui) = bridge.shared.lock() {
        ui.boot_mode = boot.mode;
    }
    let pressed =
        mouse.get_just_pressed().next().is_some() || keys.get_just_pressed().next().is_some();

    // Publish the boot stage every frame, before any per-stage logic: overlay
    // text must switch in lockstep with the sprites (never show a neighbouring
    // card's text early or leave mode-3 credits up over the logo).
    if let Ok(mut ui) = bridge.shared.lock() {
        ui.boot_mode = boot.mode.min(4);
    }

    // ----- Logo stage (mode 4) -----
    if boot.mode == 4 {
        boot.t += dt;

        // Spawn the NT logo once. Frame 0 is blank; it builds up per shot.
        if boot.spawned.is_empty()
            && let Some((cam, map)) = view_setup(&windows, &cam_q)
        {
            let (spr, tf) = gm_sprite(
                &catalog,
                &asset_server,
                &map,
                "images/sprLogo.png",
                0,
                GUI_W / 2.0,
                GUI_H / 2.0,
                1.0,
                1.0,
                Color::WHITE,
                -890.0,
            );
            boot.spawned
                .push(commands.spawn((BootArt, ChildOf(cam), spr, tf)).id());
        }

        // Logo/Alarm_0 (game runs at 30 fps; Logo/Create_0 arms alarm[0]=30):
        //   index 1 after 1.0s, then every 2 ticks; after frame 6 wait 20
        //   ticks, then frame 7 + the boom set + logo-loop ambience.
        // Times when image_index becomes 1,2,...,7:
        const STEP_T: [f32; 7] = [
            1.0,                             // -> 1
            1.0 + 2.0 / 30.0,                // -> 2
            1.0 + 4.0 / 30.0,                // -> 3
            1.0 + 6.0 / 30.0,                // -> 4
            1.0 + 8.0 / 30.0,                // -> 5
            1.0 + 10.0 / 30.0,               // -> 6
            1.0 + 10.0 / 30.0 + 20.0 / 30.0, // -> 7 boom
        ];
        while (boot.guns as usize) < STEP_T.len() && boot.t >= STEP_T[boot.guns as usize] {
            boot.guns += 1; // guns == image_index after step
            if boot.guns >= 7 {
                if catalog.has_audio("audio/sndLogoLoop.wav") {
                    commands.spawn((
                        SplashLoop,
                        AudioPlayer::<AudioSource>::new(asset_server.load("audio/sndLogoLoop.wav")),
                        PlaybackSettings {
                            mode: PlaybackMode::Loop,
                            volume: Volume::Linear(0.6),
                            ..default()
                        },
                    ));
                }
                play_cue(&mut commands, &catalog, &asset_server, "sndShovel", 0.8);
                play_cue(&mut commands, &catalog, &asset_server, "sndMeatExplo", 0.8);
                play_cue(&mut commands, &catalog, &asset_server, "sndExplosion", 0.8);
                boot.shake += 2.5;
                boot.booms = true;
            } else {
                play_cue(&mut commands, &catalog, &asset_server, "sndMachinegun", 0.5);
                boot.shake += 0.5;
            }
        }

        // Draw_0: the logo steps to the current frame and jitters by shake,
        // which decays one unit per tick.
        boot.shake = (boot.shake - dt * 30.0).max(0.0); // decays 1/tick @ 30 fps
        if let Some(logo) = boot.spawned.first().copied() {
            if let Ok(mut spr) = sprites.get_mut(logo) {
                let m = meta_of(&catalog, "images/sprLogo.png");
                let (fw, fh) = (m[1].max(1.0), m[2].max(1.0));
                // Final assembled frame is 7 (NUCLEAR + THRONE).
                let f = (boot.guns as f32).min(7.0);
                spr.rect = Some(Rect::new(f * fw, 0.0, (f + 1.0) * fw, fh));
            }
            if let (Ok(mut tf), Some((_, map))) =
                (transforms.get_mut(logo), view_setup(&windows, &cam_q))
            {
                let jx = (rand::random::<f32>() - 0.5) * 2.0 * boot.shake;
                let jy = (rand::random::<f32>() - 0.5) * 2.0 * boot.shake;
                // True GM origin from meta (centre of the logo).
                let c = map.to_world(GUI_W / 2.0 + jx, GUI_H / 2.0 + jy);
                tf.translation = c.extend(-890.0);
            }
        }

        // Logo/Mouse_53.
        if pressed {
            if boot.guns == 0 {
                // Before frame 1: speed the alarm up (min 10 ticks).
                boot.t = boot.t.max(1.0 - 10.0 / 30.0);
            } else {
                transition.begin_to_state(AppState::MainMenu);
            }
        }
        return;
    }

    // ----- Card modes 0..3 -----
    // Skip input needs a short grace per card: the click that focuses the
    // window at launch would otherwise insta-advance mode 0 ("MADE IN
    // GAMEMAKER" appearing early / both cards seemingly at once).
    let can_skip = boot.t >= 0.25;
    if (pressed && can_skip) || boot.t >= MODE_SECS[boot.mode as usize] {
        boot.mode += 1;
        boot.t = 0.0;
        boot.rendered_mode = -1;
        if boot.mode == 4 {
            // Drop any card sprites before the logo stage.
            for e in boot.spawned.drain(..) {
                commands.entity(e).despawn();
            }
            // Vlambeer/Alarm_0 mode >= 3: SpiralCont + Logo (no jingle).
            if let Some((cam, map)) = view_setup(&windows, &cam_q) {
                commands.insert_resource(SpiralCtl {
                    angle: rand::random::<f32>() * 360.0,
                });
                for _ in 0..150 {
                    spawn_spiral_wisp(
                        &mut commands,
                        &catalog,
                        &asset_server,
                        cam,
                        &map,
                        SpiralCtl {
                            angle: rand::random::<f32>() * 360.0,
                        },
                        Some(rand::random::<f32>() * 1.2),
                    );
                }
                if catalog.has_audio("audio/sndPortalLoop.wav") {
                    commands.spawn((
                        PortalLoop,
                        AudioPlayer::<AudioSource>::new(
                            asset_server.load("audio/sndPortalLoop.wav"),
                        ),
                        PlaybackSettings {
                            mode: PlaybackMode::Loop,
                            volume: Volume::Linear(0.5),
                            ..default()
                        },
                    ));
                }
            }
        } else {
            play_cue(&mut commands, &catalog, &asset_server, "sndRestart", 0.7);
        }
    } else {
        boot.t += dt;
    }

    if boot.mode >= 4 {
        return;
    }
    // da advances 0.5/tick at 30 fps (Draw_0: da += 0.5).
    boot.da += dt * 15.0;

    // Rebuild sprites when the mode changes.
    if boot.rendered_mode != boot.mode as i8 {
        boot.rendered_mode = boot.mode as i8;
        for e in boot.spawned.drain(..) {
            commands.entity(e).despawn();
        }
        let Some((cam, map)) = view_setup(&windows, &cam_q) else {
            return;
        };
        match boot.mode {
            0 => {
                // Vlambeer/Create_0: the jingle plays as the first card shows.
                play_cue(&mut commands, &catalog, &asset_server, "sndVlambeer", 0.7);
                let (spr, tf) = gm_sprite(
                    &catalog,
                    &asset_server,
                    &map,
                    "images/sprSaving.png",
                    0,
                    GUI_W / 2.0,
                    GUI_H / 2.0 - 16.0,
                    1.0,
                    1.0,
                    Color::WHITE,
                    -890.0,
                );
                boot.spawned
                    .push(commands.spawn((BootArt, ChildOf(cam), spr, tf)).id());
            }
            2 => {
                let (spr, tf) = gm_sprite(
                    &catalog,
                    &asset_server,
                    &map,
                    "images/sprVlambeer.png",
                    0,
                    0.0,
                    0.0,
                    1.0,
                    1.0,
                    Color::WHITE,
                    -890.0,
                );
                boot.spawned
                    .push(commands.spawn((BootArt, ChildOf(cam), spr, tf)).id());
                // Draw_0 adds ten additive jittered copies; four static ones
                // approximate the glow.
                for k in 0..4usize {
                    let (g, gtf) = gm_sprite(
                        &catalog,
                        &asset_server,
                        &map,
                        "images/sprVlambeer.png",
                        0,
                        if k & 1 != 0 { 2.0 } else { -2.0 },
                        if k & 2 != 0 { 2.0 } else { -2.0 },
                        1.0,
                        1.0,
                        Color::srgba(1.0, 1.0, 1.0, 0.1),
                        -889.0,
                    );
                    boot.spawned
                        .push(commands.spawn((BootArt, ChildOf(cam), g, gtf)).id());
                }
            }
            _ => {}
        }
    } else if boot.mode == 0
        && let Some(e) = boot.spawned.first().copied()
        && let Ok(mut spr) = sprites.get_mut(e)
    {
        // Saving icon animates at 30 fps (da += 0.5 per tick).
        let m = meta_of(&catalog, "images/sprSaving.png");
        let (fw, fh) = (m[1].max(1.0), m[2].max(1.0));
        let frame = (boot.da.floor() as usize) % 31;
        spr.rect = Some(Rect::new(
            frame as f32 * fw,
            0.0,
            (frame + 1) as f32 * fw,
            fh,
        ));
    }
}

/// Gameplay zoom AND chase offset (CameraFollow) must not leak into the menu
/// screens: the Repose hitbox layer is zoom-independent and all menu art is
/// placed in world coords around the origin, so restore base scale and centre.
fn reset_camera_view(
    mut q: Query<(&mut Transform, &mut Projection, Option<&mut CameraBase>), With<Camera2d>>,
) {
    for (mut tf, mut p, base) in &mut q {
        tf.translation.x = 0.0;
        tf.translation.y = 0.0;
        if let Some(mut b) = base {
            b.translation = tf.translation;
            b.rotation = 0.0;
        }
        if let Projection::Orthographic(o) = p.as_mut() {
            o.scale = CAM_SCALE;
        }
    }
}

pub struct UiArtPlugin;

impl Plugin for UiArtPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CharSelectArt>()
            .init_resource::<BootState>()
            .add_systems(OnEnter(AppState::Splash), (reset_camera_view, reset_boot))
            .add_systems(
                OnEnter(AppState::MainMenu),
                (reset_camera_view, spawn_spiral_field),
            )
            .add_systems(
                OnExit(AppState::Splash),
                (despawn_boot_art, despawn_splash_loop),
            )
            .add_systems(
                OnEnter(AppState::Title),
                (reset_camera_view, spawn_char_select),
            )
            .add_systems(
                OnExit(AppState::Title),
                (despawn_title_art, despawn_hud_art),
            )
            .add_systems(
                Update,
                (char_select_tick.run_if(in_state(AppState::Title))).chain(),
            )
            .add_systems(Update, main_menu_hover)
            .add_systems(Update, boot_intro)
            .add_systems(OnEnter(AppState::InGame), spawn_hud_art)
            .add_systems(OnEnter(AppState::Loading), despawn_portal_loop)
            .add_systems(OnExit(AppState::InGame), despawn_hud_art)
            .add_systems(FixedUpdate, spiral_field)
            .add_systems(FixedUpdate, sync_hud_art);
    }
}

/// (camera entity, GUI map for the current window + live ortho zoom).
fn view_setup<F: QueryFilter>(
    windows: &Query<&Window, With<bevy::window::PrimaryWindow>>,
    cam_q: &Query<(Entity, &Transform, &Projection), F>,
) -> Option<(Entity, GuiMap)> {
    let win = windows.iter().next()?;
    let (_, _tf, proj) = cam_q.iter().next()?;
    let scale = match proj {
        Projection::Orthographic(o) => o.scale,
        _ => 1.0,
    };
    Some((
        cam_q.iter().next()?.0,
        gui_map(win.width(), win.height(), scale),
    ))
}

// ---------------------------------------------------------------------------
// Title: rotating spiral field + logo
// ---------------------------------------------------------------------------

/// One growing spiral wisp (nt-rewrite object `Spiral`).
#[derive(Component)]
struct SpiralWisp {
    /// Current image_xscale/yscale (NT sprite-scale units).
    s: f32,
    /// Per-tick growth, compounding like upstream `grow`.
    grow: f32,
    /// Animation clock (image_speed = 2).
    anim: f32,
}

/// The `SpiralCont` driver state.
#[derive(Resource)]
struct SpiralCtl {
    angle: f32,
}

#[allow(clippy::too_many_arguments)]
fn spawn_spiral_wisp(
    commands: &mut Commands,
    catalog: &AssetCatalog,
    asset_server: &AssetServer,
    cam: Entity,
    map: &GuiMap,
    ctl: SpiralCtl,
    initial_scale: Option<f32>,
) {
    let a = ctl.angle.to_radians();
    // SpiralCont/Step_0 orbit around the GUI centre.
    let cx = GUI_W / 2.0 + (a / 921.0).sin() * (a / 500.0).sin() * 80.0;
    let cy = GUI_H / 2.0 + (a / 583.0).cos() * (a / 500.0).sin() * 50.0;

    // draw_sprite_ext(..., image_xscale * 10, ..., image_angle + 45, ...);
    // sprSpiral origin is its centre, so rotation about the entity is right.
    let wisp_scale = initial_scale.unwrap_or(0.0);
    let draw_scale = wisp_scale * 10.0;
    let (mut spr, mut tf) = gm_sprite(
        catalog,
        asset_server,
        map,
        "images/sprSpiral.png",
        0,
        cx,
        cy,
        draw_scale,
        draw_scale,
        Color::WHITE,
        -895.0,
    );
    spr.color = Color::srgba(0.55, 0.52, 0.66, 1.0);
    tf.rotation = Quat::from_rotation_z(a + 45.0f32.to_radians());
    commands.spawn((
        TitleArt,
        SpiralWisp {
            s: wisp_scale,
            grow: 0.0002,
            anim: rand::random::<f32>() * 2.0,
        },
        ChildOf(cam),
        spr,
        tf,
    ));
}

/// SpiralCont/Step_0 + Spiral/Step_0: orbit, emit, grow, dissolve.
#[allow(clippy::type_complexity)]
fn spiral_field(
    state: Res<State<AppState>>,
    mut commands: Commands,
    catalog: Res<AssetCatalog>,
    asset_server: Res<AssetServer>,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    cam_q: Query<(Entity, &Transform, &Projection), With<Camera2d>>,
    mut ctl: Option<ResMut<SpiralCtl>>,
    mut wisps: Query<(Entity, &mut SpiralWisp, &mut Sprite)>,
) {
    if !matches!(
        state.get(),
        AppState::Splash | AppState::MainMenu | AppState::Title
    ) {
        return;
    }
    let Some((cam, map)) = view_setup(&windows, &cam_q) else {
        return;
    };
    let Some(ctl) = ctl.as_mut() else {
        return;
    };

    spawn_spiral_wisp(
        &mut commands,
        &catalog,
        &asset_server,
        cam,
        &map,
        SpiralCtl { angle: ctl.angle },
        None,
    );
    ctl.angle = (ctl.angle + 8.0) % 360.0;

    let m = meta_of(&catalog, "images/sprSpiral.png");
    let (fw, fh) = (m[1].max(1.0), m[2].max(1.0));
    for (entity, mut wisp, mut spr) in &mut wisps {
        // Upstream growth law, rate lifted 0.0005 -> 0.004 so wisps live
        // seconds at our fixed tick rate instead of minutes.
        wisp.grow = (wisp.grow + 1.0) * (1.0 + 0.004 * wisp.s) - 1.0;
        wisp.s += wisp.grow;
        wisp.anim += 2.0;

        if wisp.s > 2.5 {
            commands.entity(entity).despawn();
            continue;
        }

        let draw_scale = wisp.s * 10.0;
        spr.custom_size = Some(Vec2::new(fw * draw_scale * map.s, fh * draw_scale * map.s));
        // Upstream draws white + black(0.8 - xscale) overlays: young wisps
        // are nearly black, mature ones full art. Emulate by scaling the tint.
        let f = (wisp.s * 1.25 + 0.2).clamp(0.2, 1.0);
        spr.color = Color::srgb(0.55 * f, 0.52 * f, 0.66 * f);
        let frame = (wisp.anim.floor() as usize) % 2;
        spr.rect = Some(Rect::new(
            frame as f32 * fw,
            0.0,
            (frame + 1) as f32 * fw,
            fh,
        ));
    }
}

fn despawn_title_art(
    mut commands: Commands,
    q: Query<Entity, With<TitleArt>>,
    art: Option<Res<CharSelectArt>>,
    ctl: Option<Res<SpiralCtl>>,
) {
    for e in &q {
        commands.entity(e).despawn();
    }
    if art.is_some() {
        commands.remove_resource::<CharSelectArt>();
    }
    if ctl.is_some() {
        commands.remove_resource::<SpiralCtl>();
    }
}

// ---------------------------------------------------------------------------
// Char select (nt-rewrite objects: Menu/Create_0, CharSelect, GoButton)
// ---------------------------------------------------------------------------

/// Live handles for the title char-select art.
#[derive(Resource, Default)]
struct CharSelectArt {
    /// (pod entity, race id, gui x) — one per `CharSelect` instance.
    pods: Vec<(Entity, usize, f32)>,
    /// GoButton entity + base gui position.
    go_button: Option<(Entity, f32, f32)>,
    /// Pop-in offset (`addy`), approaches 0.
    addy: f32,
    /// Accumulated animation clock for the hovered button.
    go_anim: f32,
    /// Bottom letterbox + top letterbox.
    letterbox: Vec<Entity>,
    /// sprCharSplat under the name area.
    splat: Option<Entity>,
    /// sprBigPortrait (frame = race id), bottom-left.
    big_portrait: Option<Entity>,
    /// sprBigName (frame = race id).
    big_name: Option<Entity>,
    splat_anim: f32,
    /// sprCampfire burning centre-screen (camera is centred on it).
    campfire: Option<Entity>,
    campfire_anim: f32,
    /// sprLogMenu bench above the fire.
    log: Option<Entity>,
    /// CampChar mutants around the fire: (entity, frames, fw, fh).
    chars: Vec<(Entity, usize, f32, f32)>,
    char_anim: f32,
    /// Right-side loadout art (scrMenuDrawLoadout).
    arrow: Option<Entity>,
    loadout_splat: Option<Entity>,
    crown_icon: Option<Entity>,
    /// (entity, weapon gml id) per slot; swapped on equipment change.
    wep_icons: [Option<(Entity, u8)>; 2],
    /// Open-panel state (Menu.loadout_frame via approach()).
    loadout_anim: f32,
    /// sprLoadoutOpen panel + crown grid entries (entity, gui x, gui y).
    open_panel: Option<Entity>,
    crown_grid: Vec<(Entity, f32, f32)>,
    skin_grid: Vec<(Entity, f32, f32)>,
}

const GO_W: f32 = 31.0;
const GO_H: f32 = 19.0;
/// sprGoButtonSymbolic yorigin (from anims.json / GoButton.yy).
const GO_YORIGIN: f32 = -2.0;

#[allow(clippy::type_complexity)]
fn spawn_char_select(
    mut commands: Commands,
    catalog: Res<AssetCatalog>,
    asset_server: Res<AssetServer>,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    cam_q: Query<(Entity, &Transform, &Projection), With<Camera2d>>,
) {
    let Some((cam, map)) = view_setup(&windows, &cam_q) else {
        return;
    };

    let count = CHAR_SELECT_RACES.len();
    let step = slot_step(count);
    let ystart = slot_ystart();
    let mut art = CharSelectArt::default();

    for (i, race) in CHAR_SELECT_RACES.iter().enumerate() {
        let race_id = *race as usize;
        let x = slot_x(i, step);

        // CharSelect/Draw_0:
        //   draw_sprite_ext(can ? sprite_index : sprCharSelectLocked,
        //                   race, x, y, 1, 1, 0, color, 1)
        // The port has no unlock gating yet, so every pod uses the real
        // sprite; frame index == race id.
        let (pod_spr, pod_tf) = gm_sprite(
            &catalog,
            &asset_server,
            &map,
            "images/sprCharSelect.png",
            race_id,
            x,
            ystart,
            1.0,
            1.0,
            C_GRAY,
            -860.0,
        );
        let pod = commands
            .spawn((TitleArt, ChildOf(cam), pod_spr, pod_tf))
            .id();
        art.pods.push((pod, race_id, x));
    }

    // Campfire-area level (MenuGen): backdrop colour scrAreaGetBackround
    // Color(area_campfire) = #6a7aaf, then a jittered sprFloor0 tile field.
    // The floor renders in front of the portal spiral, like upstream depths.
    {
        let c = map.to_world(GUI_W / 2.0, GUI_H / 2.0);
        commands.spawn((
            TitleArt,
            ChildOf(cam),
            Sprite {
                color: Color::srgb_u8(106, 122, 175),
                custom_size: Some(Vec2::new(GUI_W * map.s, GUI_H * map.s)),
                ..default()
            },
            Transform::from_xyz(c.x, c.y, -899.0),
        ));
    }
    {
        // 12x10 field (+1 ring) of 32 px tiles with the MenuGen jitter
        // (mody = choose(32, 0, -32)) and the Floor frame weighting
        // (mostly 0, sometimes 1/2, rarely 3).
        let mut spawn_tile = |gx: f32, gy: f32| {
            let mody = [(0.0_f32, 2i32), (-32.0, 1), (32.0, 1)];
            let pick = |w: i32| rand::rng().random_range(0..w);
            let total: i32 = mody.iter().map(|(_, w)| w).sum();
            let mut r = pick(total);
            let mut dy = 0.0;
            for (v, w) in mody {
                r -= w;
                if r < 0 {
                    dy = v;
                    break;
                }
            }
            let frame = if rand::random::<f32>() < 1.0 / 500.0 {
                3usize
            } else {
                match pick(9) {
                    0..=6 => 0,
                    7 => 1,
                    _ => 2,
                }
            };
            let (spr, mut tf) = gm_sprite(
                &catalog,
                &asset_server,
                &map,
                "images/sprFloor0.png",
                frame,
                gx + dy,
                gy + dy,
                1.0,
                1.0,
                Color::WHITE,
                -890.0,
            );
            tf.translation.z = -890.0;
            commands.spawn((TitleArt, ChildOf(cam), spr, tf));
        };
        for j in -1..10i32 {
            for i in -1..12i32 {
                spawn_tile(i as f32 * 32.0 + 16.0, j as f32 * 32.0 + 16.0);
            }
        }
        // Walls around the floor field (mcr_floor_make_walls): one-tile thick border.
        {
            for j in -2..12i32 {
                for i in -2..14i32 {
                    let is_floor = i >= -1 && i <= 12 && j >= -1 && j <= 10;
                    let is_edge = i == -2 || i == 13 || j == -2 || j == 11;
                    if is_edge && !is_floor {
                        let (wall_path, frame) = if j == -2 {
                            ("images/sprWall0Top.png", 0)
                        } else if j == 11 {
                            ("images/sprWall0Bot.png", 0)
                        } else {
                            ("images/sprWall0Out.png", 0)
                        };
                        if catalog.has(wall_path) {
                            let (spr, tf) = gm_sprite(
                                &catalog,
                                &asset_server,
                                &map,
                                wall_path,
                                frame,
                                i as f32 * 32.0 + 16.0,
                                j as f32 * 32.0 + 16.0,
                                1.0,
                                1.0,
                                Color::WHITE,
                                -889.0,
                            );
                            commands.spawn((TitleArt, ChildOf(cam), spr, tf));
                        }
                    }
                }
            }
        }
    }

    // Letterbox (scrDrawLetterbox): one 320x44 strip per edge, solid 36 px
    // plus a ragged drip edge; yscale = 36/(44-9), top at y=-1, bottom
    // mirrored at (320, 242) with both flips. Frame 0 (Menu/Create_0).
    {
        let yscale = LETTERBOX_SIZE / (44.0 - 9.0);
        for (gui_x, gui_y, flip) in [(0.0_f32, -1.0_f32, false), (GUI_W, GUI_H + 2.0, true)] {
            let (mut spr, tf) = gm_sprite(
                &catalog,
                &asset_server,
                &map,
                "images/sprLetterbox.png",
                0,
                gui_x,
                gui_y,
                1.0,
                yscale,
                Color::WHITE,
                -850.0,
            );
            spr.flip_x = flip;
            spr.flip_y = flip;
            if flip {
                // Negative scales would mirror about the origin; emulate with
                // flips and keep custom_size positive.
                if let Some(sz) = spr.custom_size.as_mut() {
                    sz.y = sz.y.abs();
                }
            }
            art.letterbox
                .push(commands.spawn((TitleArt, ChildOf(cam), spr, tf)).id());
        }
    }

    // Campfire scene (scrCampfireMenuCreate): the camera centres on the fire,
    // so in GUI coords the fire sits at the screen centre with the log bench
    // above it and the fixed four mutants around it.
    {
        // Campfire: image_speed 0.4 @ 30 fps, random horizontal flip.
        let (mut spr, tf) = gm_sprite(
            &catalog,
            &asset_server,
            &map,
            "images/sprCampfire.png",
            0,
            GUI_W / 2.0,
            GUI_H / 2.0,
            1.0,
            1.0,
            Color::WHITE,
            -872.0,
        );
        spr.flip_x = rand::random::<bool>();
        art.campfire = Some(commands.spawn((TitleArt, ChildOf(cam), spr, tf)).id());

        // LogMenu bench at (0, -32) relative to the fire.
        let (spr, tf) = gm_sprite(
            &catalog,
            &asset_server,
            &map,
            "images/sprLogMenu.png",
            0,
            GUI_W / 2.0,
            GUI_H / 2.0 - 32.0,
            1.0,
            1.0,
            Color::WHITE,
            -884.0,
        );
        art.log = Some(commands.spawn((TitleArt, ChildOf(cam), spr, tf)).id());

        // The fixed four: Fish (0,-32), Crystal (+32 below), Eyes (+40,0),
        // Melting (-40,0). Everyone else scatters 40..110 px out.
        use crate::game::content::RaceId;
        let fixed: [(RaceId, f32, f32); 4] = [
            (RaceId::Fish, 0.0, -32.0),
            (RaceId::Crystal, 0.0, 32.0),
            (RaceId::Eyes, 40.0, 0.0),
            (RaceId::Melting, -40.0, 0.0),
        ];
        // Resolves the idle strip for a mutant (plain Menu sprite where the
        // dump has it; frozen Deselect frame stands in otherwise) and builds
        // its sprite centred on the GUI origin, ready to place.
        let mut menu_sprite = |race: RaceId| -> Option<(Sprite, Transform, usize, f32, f32)> {
            let (path, fallback): (&'static str, &'static str) = match race {
                RaceId::Fish => ("images/sprFishMenu.png", "images/sprFishMenuDeselect.png"),
                RaceId::Crystal => ("images/sprCrystalMenu.png", ""),
                RaceId::Eyes => ("images/sprEyesMenu.png", ""),
                RaceId::Melting => ("images/sprMeltingMenu.png", ""),
                RaceId::Plant => ("images/sprPlantMenu.png", ""),
                RaceId::Venuz => ("images/sprVenuzMenu.png", "images/sprVenuzMenuDeselect.png"),
                RaceId::Steroids => (
                    "images/sprSteroidsMenu.png",
                    "images/sprSteroidsMenuDeselect.png",
                ),
                RaceId::Robot => ("images/sprRobotMenu.png", ""),
                RaceId::Chicken => ("images/sprChickenMenu.png", ""),
                RaceId::Rebel => ("images/sprRebelMenu.png", ""),
                RaceId::Horror => ("images/sprHorrorMenu.png", ""),
                RaceId::Rogue => ("images/sprRogueMenu.png", "images/sprRogueMenuDeselect.png"),
                RaceId::BigDog => ("images/sprBigDogMenu.png", "images/sprDogMenu.png"),
                RaceId::Skeleton => (
                    "images/sprSkeletonMenu.png",
                    "images/sprSkeletonMenuDeselect.png",
                ),
                RaceId::Frog => ("images/sprFrogMenu.png", "images/sprFrogMenuDeselect.png"),
                RaceId::Cuz => ("images/sprCuzMenu.png", ""),
                RaceId::Random => return None,
            };
            let chosen: &'static str = if catalog.anims.contains_key(path) {
                path
            } else if !fallback.is_empty() && catalog.anims.contains_key(fallback) {
                fallback
            } else {
                // Gamemaker fallback: scr_race_get_sprite(_name, "Menu", _default) where _default is mutant idle.
                let mutant_path = format!("images/sprMutant{}Idle.png", race as u8);
                if catalog.has(&mutant_path) {
                    Box::leak(mutant_path.into_boxed_str())
                } else {
                    return None;
                }
            };
            let m = meta_of(&catalog, chosen);
            let (frames, fw, fh) = (m[0].max(1.0), m[1].max(1.0), m[2].max(1.0));
            let (mut spr, mut tf) = gm_sprite(
                &catalog,
                &asset_server,
                &map,
                chosen,
                0,
                GUI_W / 2.0,
                GUI_H / 2.0,
                1.0,
                1.0,
                Color::WHITE,
                -866.0,
            );
            spr.flip_x = rand::random::<bool>();
            Some((spr, tf, frames as usize, fw, fh))
        };
        let mut place =
            |spr: Sprite, mut tf: Transform, dx: f32, dy: f32, frames: usize, fw: f32, fh: f32| {
                let c = map.to_world(GUI_W / 2.0 + dx, GUI_H / 2.0 + dy);
                tf.translation = c.extend(-866.0);
                let e = commands.spawn((TitleArt, ChildOf(cam), spr, tf)).id();
                art.chars.push((e, frames, fw, fh));
            };

        for (race, dx, dy) in fixed {
            if let Some((spr, tf, frames, fw, fh)) = menu_sprite(race) {
                place(spr, tf, dx, dy, frames, fw, fh);
            }
        }
        for race in [
            RaceId::Plant,
            RaceId::Venuz,
            RaceId::Steroids,
            RaceId::Robot,
            RaceId::Chicken,
            RaceId::Rebel,
            RaceId::Horror,
            RaceId::Rogue,
            RaceId::BigDog,
            RaceId::Skeleton,
            RaceId::Frog,
            RaceId::Cuz,
        ] {
            let ang = rand::random::<f32>() * std::f32::consts::TAU;
            let dist = rand::random::<f32>() * 70.0 + 40.0;
            let dx = ang.cos() * dist;
            let dy = ang.sin() * dist * 0.75;
            if let Some((spr, tf, frames, fw, fh)) = menu_sprite(race) {
                place(spr, tf, dx, dy, frames, fw, fh);
            }
        }
    }

    // Char splat sits on the bottom letterbox (scrCampfireMenuDrawRacePortrait,
    // fa_left/fa_bottom): draw point (0, 205), origin (0, 64).
    {
        let (spr, tf) = gm_sprite(
            &catalog,
            &asset_server,
            &map,
            "images/sprCharSplat.png",
            0,
            0.0,
            GUI_H - LETTERBOX_SIZE + 1.0,
            1.0,
            1.0,
            Color::WHITE,
            -855.0,
        );
        art.splat = Some(commands.spawn((TitleArt, ChildOf(cam), spr, tf)).id());
    }

    // Big portrait (sprCampfireMenuDrawRacePortrait, fa_left): draw point
    // (16, 240). Subimages are the per-race skin portraits; frame = race id.
    // Hidden until a non-random pick.
    {
        let (spr, tf) = gm_sprite(
            &catalog,
            &asset_server,
            &map,
            "images/sprBigPortrait.png",
            1, // Fish default
            16.0,
            GUI_H,
            1.0,
            1.0,
            Color::WHITE,
            -856.0,
        );
        art.big_portrait = Some(
            commands
                .spawn((TitleArt, ChildOf(cam), Visibility::Hidden, spr, tf))
                .id(),
        );
    }

    // Big name plate (frame = race id), draw point (0, 137). Hidden until a
    // non-random pick.
    {
        let (spr, tf) = gm_sprite(
            &catalog,
            &asset_server,
            &map,
            "images/sprBigName.png",
            1, // Fish default
            0.0,
            GUI_H - LETTERBOX_SIZE - 32.0 - 35.0,
            1.0,
            1.0,
            Color::WHITE,
            -854.0,
        );
        art.big_name = Some(
            commands
                .spawn((TitleArt, ChildOf(cam), Visibility::Hidden, spr, tf))
                .id(),
        );
    }

    // Right-side loadout art (scrMenuDrawLoadout, closed state): splat pinned
    // to the right edge, arrow above it, current crown and both weapons.
    {
        let (spr, tf) = gm_sprite(
            &catalog,
            &asset_server,
            &map,
            "images/sprLoadoutSplat.png",
            0,
            GUI_W + 2.0,
            GUI_H - LETTERBOX_SIZE + 2.0,
            1.0,
            1.0,
            Color::WHITE,
            -853.0,
        );
        art.loadout_splat = Some(commands.spawn((TitleArt, ChildOf(cam), spr, tf)).id());

        let (spr, tf) = gm_sprite(
            &catalog,
            &asset_server,
            &map,
            "images/sprLoadoutArrow.png",
            0,
            GUI_W + 2.0 - 16.0,
            GUI_H - LETTERBOX_SIZE + 2.0 - 16.0,
            1.0,
            1.0,
            C_UIGRAY,
            -852.0,
        );
        art.arrow = Some(commands.spawn((TitleArt, ChildOf(cam), spr, tf)).id());

        let (spr, tf) = gm_sprite(
            &catalog,
            &asset_server,
            &map,
            "images/sprLoadoutCrown.png",
            0,
            GUI_W + 2.0 - 60.0,
            GUI_H - LETTERBOX_SIZE + 2.0 - 40.0,
            1.0,
            1.0,
            Color::WHITE,
            -852.0,
        );
        art.crown_icon = Some(commands.spawn((TitleArt, ChildOf(cam), spr, tf)).id());

        for slot in 0..2usize {
            let wx = GUI_W + 2.0 - 60.0 + if slot == 0 { -8.0 } else { 16.0 };
            let wy = GUI_H - LETTERBOX_SIZE + 2.0 - 15.0;
            let (spr, tf) = gm_loadout_weapon(
                &catalog,
                &asset_server,
                &map,
                WeaponId::REVOLVER,
                wx,
                wy,
                if slot == 0 {
                    Color::WHITE
                } else {
                    Color::srgb_u8(192, 192, 192)
                },
                -851.0,
            );
            let e = commands.spawn((TitleArt, ChildOf(cam), spr, tf)).id();
            art.wep_icons[slot] = Some((e, WeaponId::REVOLVER.0));
        }

        // Open panel (sprLoadoutOpen, bottom-right origin) + the crown grid
        // layout from scrMenuDrawLoadout: start (248,48), step 28, wrap at
        // the right edge back to x=220.
        let (spr, tf) = gm_sprite(
            &catalog,
            &asset_server,
            &map,
            "images/sprLoadoutOpen.png",
            0,
            GUI_W,
            GUI_H - LETTERBOX_SIZE + 4.0,
            (GUI_W - 184.0) / (256.0 - 56.0),
            (GUI_H - LETTERBOX_SIZE + 4.0 - LETTERBOX_SIZE) / 168.0 + 0.05,
            Color::WHITE,
            -849.0,
        );
        art.open_panel = Some(
            commands
                .spawn((TitleArt, ChildOf(cam), Visibility::Hidden, spr, tf))
                .id(),
        );

        let mut crowns: Vec<(f32, f32)> = Vec::with_capacity(14);
        let (mut cx, mut cy) = (248.0_f32, 48.0_f32);
        for _ in 0..14 {
            crowns.push((cx, cy));
            cx += 28.0;
            if cx >= 332.0 {
                cx = 220.0;
                cy += 28.0;
            }
        }
        for (gx, gy) in crowns {
            let (spr, tf) = gm_sprite(
                &catalog,
                &asset_server,
                &map,
                "images/sprLoadoutCrown.png",
                0,
                gx,
                gy,
                1.0,
                1.0,
                C_UIGRAY,
                -848.0,
            );
            art.crown_grid.push((
                commands.spawn((TitleArt, ChildOf(cam), spr, tf)).id(),
                gx,
                gy,
            ));
        }
        // Skins (left side of loadout panel) – 4 entries max (Robot has 4)
        {
            let crown_left = 220.0;
            let crown_size = 28.0;
            let skins_x = crown_left - crown_size * 0.5 - 22.0;
            let skins_y_start = GUI_H * 0.5 - (28.0 * 0.5) * 4.0 - 2.0;
            let skin_size = 28.0;
            for idx in 0..4 {
                let gy = skins_y_start + idx as f32 * skin_size;
                let gx = skins_x;
                let (spr, tf) = gm_sprite(
                    &catalog,
                    &asset_server,
                    &map,
                    "images/sprLoadoutSkin.png",
                    0,
                    gx,
                    gy,
                    1.0,
                    1.0,
                    C_UIGRAY,
                    -848.0,
                );
                art.skin_grid.push((
                    commands.spawn((TitleArt, ChildOf(cam), spr, tf)).id(),
                    gx,
                    gy,
                ));
            }
        }
    }

    // Menu/Create_0 spawns GoButton right of the last slot, hidden.
    let (gx, gy) = go_button_pos(step, count);
    let (go_spr, go_tf) = gm_sprite(
        &catalog,
        &asset_server,
        &map,
        "images/sprGoButtonSymbolic.png",
        0,
        gx,
        gy + 1.0, // Create_0 sets addy = 1
        1.0,
        1.0,
        C_UIGRAY,
        -856.0,
    );
    let go = commands
        .spawn((TitleArt, ChildOf(cam), Visibility::Hidden, go_spr, go_tf))
        .id();
    art.go_button = Some((go, gx, gy));
    art.addy = 1.0;

    commands.insert_resource(art);
}

/// MainMenuButton/Step_0 hover: point-in-rect over the five labels; plays
/// sndHover on change.
#[allow(clippy::type_complexity)]
fn main_menu_hover(
    mut commands: Commands,
    state: Res<State<AppState>>,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    cam_q: Query<(&Camera, &GlobalTransform, &Projection), With<Camera2d>>,
    bridge: Res<UiBridge>,
    catalog: Option<Res<AssetCatalog>>,
    asset_server: Res<AssetServer>,
) {
    if *state.get() != AppState::MainMenu {
        return;
    }
    let Ok(window) = windows.single() else {
        return;
    };
    let (Some(scale), Ok(gt)) = (
        cam_q.iter().next().map(|(_, _, p)| match p {
            Projection::Orthographic(o) => o.scale,
            _ => CAM_SCALE,
        }),
        cam_q.single().map(|(_, gt, _)| gt.clone()),
    ) else {
        return;
    };
    let map = gui_map(window.width(), window.height(), scale);
    let Ok(mut ui) = bridge.shared.lock() else {
        return;
    };

    let mut hovered = -1_i32;
    if let Some(cursor) = window.cursor_position() {
        if let Ok(world) = cam_q
            .iter()
            .next()
            .unwrap()
            .0
            .viewport_to_world_2d(&gt, cursor)
        {
            let g = map.to_gui(world);
            // Label strip: x centred on 160, each row 20 px tall.
            if g.x >= 60.0 && g.x <= 260.0 {
                let row = ((g.y - 62.0) / 24.0).floor();
                if (0.0..5.0).contains(&row) {
                    hovered = row as i32;
                }
            }
        }
    }

    if ui.main_menu_hover != hovered {
        ui.main_menu_hover = hovered;
        // sndHover fires only for available rows (0, 2, 4).
        if matches!(hovered, 0 | 2 | 4)
            && let Some(catalog) = catalog
            && catalog.has_audio("audio/sndHover.wav")
        {
            commands.spawn((
                AudioPlayer::<AudioSource>::new(asset_server.load("audio/sndHover.wav")),
                PlaybackSettings {
                    mode: PlaybackMode::Despawn,
                    volume: Volume::Linear(0.5),
                    ..default()
                },
            ));
        }
    }
}

/// Per-frame hover/tint/animation, mirroring CharSelect/Draw_0 and
/// GoButton/Draw_0.
#[allow(clippy::type_complexity)]
fn char_select_tick(
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    cam_q: Query<(&Camera, &GlobalTransform, &Projection), With<Camera2d>>,
    mut art: ResMut<CharSelectArt>,
    bridge: Res<UiBridge>,
    catalog: Res<AssetCatalog>,
    asset_server: Res<AssetServer>,
    mut sprites: Query<&mut Sprite>,
    mut transforms: Query<&mut Transform>,
    mut visibility: Query<&mut Visibility>,
    time: Res<Time>,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    let scale = match cam_q.iter().next().map(|(_, _, p)| p) {
        Some(Projection::Orthographic(o)) => o.scale,
        _ => return,
    };
    let map = gui_map(window.width(), window.height(), scale);
    let cursor_gui = window
        .cursor_position()
        .and_then(|cursor| {
            cam_q
                .single()
                .ok()
                .and_then(|(cam, gt, _)| cam.viewport_to_world_2d(gt, cursor).ok())
        })
        .map(|w| map.to_gui(w));

    let Ok(mut ui) = bridge.shared.lock() else {
        return;
    };
    let selected_race = ui.selected_character;

    // CharSelect/Draw_0: _pointed via bbox rectangle.
    let mut hovered_race = -1_i32;
    if let Some(mouse) = cursor_gui {
        for (_, race_id, x) in &art.pods {
            if mouse.x >= *x
                && mouse.x <= *x + POD_W
                && mouse.y >= slot_ystart()
                && mouse.y <= slot_ystart() + POD_H
            {
                hovered_race = *race_id as i32;
                break;
            }
        }
    }

    for (entity, race_id, _) in &art.pods {
        let Ok(mut sprite) = sprites.get_mut(*entity) else {
            continue;
        };
        // CharSelect/Draw_0: _color = (can && selected) ? c_white : c_gray.
        // Hover only raises the tooltip; it does NOT whiten the pod.
        let is_mine = selected_race == *race_id;
        sprite.color = if is_mine { Color::WHITE } else { C_GRAY };
    }

    // Big name + splat follow the selected mutant (not Random).
    let show_name = selected_race > 0 && selected_race <= 16;

    // Animate splat while a mutant is selected.
    if show_name {
        art.splat_anim = (art.splat_anim + 12.0 * time.delta_secs()).min(3.0);
    } else {
        art.splat_anim = 0.0;
    }
    if let Some(e) = art.splat
        && let Ok(mut spr) = sprites.get_mut(e)
    {
        let fw = 154.0;
        let fh = 64.0;
        let f = art.splat_anim.floor().min(3.0);
        spr.rect = Some(Rect::new(f * fw, 0.0, (f + 1.0) * fw, fh));
    }
    if let Some(e) = art.big_portrait {
        if let Ok(mut vis) = visibility.get_mut(e) {
            *vis = if show_name {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };
        }
        if show_name && let Ok(mut spr) = sprites.get_mut(e) {
            let m = meta_of(&catalog, "images/sprBigPortrait.png");
            let (fw, fh) = (m[1].max(1.0), m[2].max(1.0));
            let skin = ui.selected_skin;
            let sub = race_skin_subimage(selected_race, skin);
            let f = (sub as f32).clamp(0.0, m[0] - 1.0);
            spr.rect = Some(Rect::new(f * fw, 0.0, (f + 1.0) * fw, fh));
        }
    }
    if let Some(e) = art.big_name {
        if let Ok(mut vis) = visibility.get_mut(e) {
            *vis = if show_name {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };
        }
        if show_name && let Ok(mut spr) = sprites.get_mut(e) {
            let fw = 180.0;
            let fh = 35.0;
            let f = selected_race as f32;
            spr.rect = Some(Rect::new(f * fw, 0.0, (f + 1.0) * fw, fh));
        }
    }

    // Campfire scene animation: fire at 12 fps (image_speed 0.4 @ 30 tps),
    // mutants idle-looping at 6 fps.
    art.campfire_anim = (art.campfire_anim + 12.0 * time.delta_secs()) % 4.0;
    if let Some(e) = art.campfire
        && let Ok(mut spr) = sprites.get_mut(e)
    {
        let (fw, fh) = (52.0, 52.0);
        let f = art.campfire_anim.floor().min(3.0);
        spr.rect = Some(Rect::new(f * fw, 0.0, (f + 1.0) * fw, fh));
    }
    art.char_anim += 6.0 * time.delta_secs();
    for (e, frames, fw, fh) in &art.chars {
        if let Ok(mut spr) = sprites.get_mut(*e) {
            let f = art.char_anim.floor() as usize % (*frames).max(1);
            let (fw, fh) = (*fw, *fh);
            spr.rect = Some(Rect::new(f as f32 * fw, 0.0, (f + 1) as f32 * fw, fh));
        }
    }

    // Right-side loadout (scrMenuDrawLoadout): the splat shows while closed,
    // the panel opens through loadout_frame (approach()d in Other_11), and
    // the closed crown/weapon row gives way to the grid + weapon slots.
    let open = ui.loadout_open;
    let target = if open { 4.0 } else { 0.0 };
    let step = 15.0 * time.delta_secs();
    art.loadout_anim = if art.loadout_anim < target {
        (art.loadout_anim + step).min(target)
    } else {
        (art.loadout_anim - step).max(target)
    };
    let fullview = art.loadout_anim >= 2.0;
    let avail = show_name && loadout_available(selected_race);

    if let Some(e) = art.open_panel {
        if let Ok(mut vis) = visibility.get_mut(e) {
            *vis = if art.loadout_anim > 0.05 {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };
        }
        if let Ok(mut spr) = sprites.get_mut(e) {
            let fw = 256.0;
            let fh = 168.0;
            let f = art.loadout_anim.floor().min(4.0);
            spr.rect = Some(Rect::new(f * fw, 0.0, (f + 1.0) * fw, fh));
        }
    }
    // Crowns grid: unlocked = sprLoadoutCrown white/gray, locked = sprLockedLoadoutCrown gray
    for (idx, (e, _, _)) in art.crown_grid.iter().enumerate() {
        if let Ok(mut vis) = visibility.get_mut(*e) {
            *vis = if fullview && avail {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };
        }
        if fullview
            && avail
            && let Ok(mut spr) = sprites.get_mut(*e)
        {
            let crown_id = idx as u8;
            let is_selected = crown_id == ui.crown_id;
            let (fw, fh) = (32.0, 32.0);
            let f = (crown_id as f32).min(13.0);
            spr.rect = Some(Rect::new(f * fw, 0.0, (f + 1.0) * fw, fh));
            spr.color = if is_selected { Color::WHITE } else { C_UIGRAY };
        }
    }
    // Skins grid: left side of panel, 4 entries max
    for (idx, (e, _, _)) in art.skin_grid.iter().enumerate() {
        let skin_count = if avail {
            max_skin_count(selected_race)
        } else {
            0
        };
        if let Ok(mut vis) = visibility.get_mut(*e) {
            *vis = if fullview && avail && idx < skin_count {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };
        }
        if fullview
            && avail
            && idx < skin_count
            && let Ok(mut spr) = sprites.get_mut(*e)
        {
            let sub = race_skin_subimage(selected_race, idx as u8);
            let f = (sub as f32).clamp(0.0, 63.0);
            let (fw, fh) = (32.0, 32.0);
            spr.rect = Some(Rect::new(f * fw, 0.0, (f + 1.0) * fw, fh));
            let is_selected = idx as u8 == ui.selected_skin;
            spr.color = if is_selected { Color::WHITE } else { C_UIGRAY };
        }
    }

    if let Some(e) = art.arrow {
        let pointed = cursor_gui.is_some_and(|m| {
            m.x >= GUI_W - 28.0 && m.x <= GUI_W - 4.0 && m.y >= GUI_H - 54.0 && m.y <= GUI_H - 30.0
        });
        if let Ok(mut spr) = sprites.get_mut(e) {
            spr.color = if avail {
                if pointed { Color::WHITE } else { C_UIGRAY }
            } else {
                Color::NONE
            };
            let (x0, x1) = if open { (24.0, 48.0) } else { (0.0, 24.0) };
            spr.rect = Some(Rect::new(x0, 0.0, x1, 24.0));
        }
    }
    if let Some(e) = art.loadout_splat
        && let Ok(mut spr) = sprites.get_mut(e)
    {
        spr.color = if art.loadout_anim < 0.5 && avail {
            Color::WHITE
        } else {
            Color::NONE
        };
    }
    if let Some(e) = art.crown_icon {
        if let Ok(mut vis) = visibility.get_mut(e) {
            *vis = if avail && !fullview {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };
        }
        if avail
            && !fullview
            && let Ok(mut spr) = sprites.get_mut(e)
        {
            let (fw, fh) = (32.0, 32.0);
            let f = (ui.crown_id as f32).min(13.0);
            spr.rect = Some(Rect::new(f * fw, 0.0, (f + 1.0) * fw, fh));
        }
    }
    // Weapon icons: closed row at (254,190)/(278,190), open slots at
    // (252,163)/(296,163); art swaps on equipment change.
    let wep_pos: [(f32, f32); 2] = if fullview {
        [(252.0, 163.0), (296.0, 163.0)]
    } else {
        [(254.0, 190.0), (278.0, 190.0)]
    };
    for (slot, id) in [(0usize, ui.start_weapon_id), (1, ui.stored_weapon_id)] {
        let Some((e, cur)) = art.wep_icons[slot] else {
            continue;
        };
        let should_show = if fullview {
            avail && show_name
        } else {
            avail && show_name && (slot == 0 || id != 0)
        };
        if let Ok(mut vis) = visibility.get_mut(e) {
            *vis = if should_show {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };
        }
        if !should_show {
            continue;
        }
        if let Ok(mut tf) = transforms.get_mut(e) {
            let c = map.to_world(wep_pos[slot].0, wep_pos[slot].1);
            tf.translation.x = c.x;
            tf.translation.y = c.y;
        }
        if cur != id {
            art.wep_icons[slot] = Some((e, id));
            let data = crate::game::content::weapon_meta(WeaponId(id));
            let mut chosen_path: Option<(&'static str, bool)> = None;
            if let Some(lout) = data.wep_lout {
                let p = format!("images/{lout}.png");
                if catalog.has(&p) {
                    chosen_path = Some((Box::leak(p.into_boxed_str()), true));
                }
            }
            if chosen_path.is_none() {
                let p = format!("images/{}.png", data.wep_sprt);
                if catalog.has(&p) {
                    chosen_path = Some((Box::leak(p.into_boxed_str()), false));
                } else if let Some(hud) = crate::game::content::weapon_hud_sprite(id) {
                    chosen_path = Some((hud, false));
                }
            }
            if let Some((path, is_lout)) = chosen_path {
                let m = meta_of(&catalog, path);
                let (fw, fh) = (m[1].max(1.0), m[2].max(1.0));
                if let Ok(mut spr) = sprites.get_mut(e) {
                    spr.image = asset_server.load(path.to_string());
                    spr.rect = Some(Rect::new(0.0, 0.0, fw, fh));
                    if is_lout {
                        spr.custom_size = Some(Vec2::new(fw * map.s, fh * map.s));
                    } else {
                        spr.custom_size = Some(Vec2::new(fw * 2.0 * map.s, fh * 2.0 * map.s));
                    }
                }
            }
        }
        if let Ok(mut spr) = sprites.get_mut(e) {
            spr.color = if slot == 0 {
                Color::WHITE
            } else {
                Color::srgb_u8(192, 192, 192)
            };
        }
    }

    // GoButton/Draw_0: animate while pointed; pop in via `addy`; lift 1 px
    // while pointed; white when pointed, c_uigray otherwise. Hidden until a
    // mutant has been clicked (visible flag).
    if let Some((entity, gx, gy)) = art.go_button {
        if let Ok(mut vis) = visibility.get_mut(entity) {
            *vis = if ui.title_go_visible {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };
        }
        let top_base = gy - GO_YORIGIN;
        let pointed = ui.title_go_visible
            && cursor_gui.is_some_and(|m| {
                m.x >= gx && m.x <= gx + GO_W && m.y >= top_base && m.y <= top_base + GO_H
            });

        art.go_anim = if pointed {
            art.go_anim + 0.4 * 60.0 * time.delta_secs()
        } else {
            0.0
        };
        if ui.title_go_visible && art.addy > 0.0 {
            art.addy = (art.addy - time.delta_secs() * 60.0).max(0.0);
        }

        let frame = (art.go_anim.floor() as usize) % 6;
        if let Ok(mut sprite) = sprites.get_mut(entity) {
            sprite.rect = Some(Rect::new(
                frame as f32 * GO_W,
                0.0,
                (frame + 1) as f32 * GO_W,
                GO_H,
            ));
            sprite.color = if pointed { Color::WHITE } else { C_UIGRAY };
        }
        if let Ok(mut tf) = transforms.get_mut(entity) {
            let draw_y = gy + art.addy - i32::from(pointed) as f32;
            let center = map.to_world(gx + GO_W / 2.0, draw_y - GO_YORIGIN + GO_H / 2.0);
            tf.translation = center.extend(-856.0);
        }
    }

    // Reference: tooltip = (!_this_race && keyboard_pointed).
    let tooltip_race = if hovered_race >= 0 && hovered_race as usize != selected_race {
        hovered_race
    } else {
        -1
    };
    if ui.title_hover_race != tooltip_race {
        ui.title_hover_race = tooltip_race;
    }
}

// ---------------------------------------------------------------------------
// In-game HUD (nt-rewrite scripts/scrDrawPlayerHUD.gml)
// ---------------------------------------------------------------------------

/// Ammo icon sprite pairs per NT ammo type (Bullets..Energy).
const AMMO_SPRITES: [(&str, &str); 5] = [
    ("images/sprBulletIconBG.png", "images/sprBulletIcon.png"),
    ("images/sprShotIconBG.png", "images/sprShotIcon.png"),
    ("images/sprBoltIconBG.png", "images/sprBoltIcon.png"),
    ("images/sprExploIconBG.png", "images/sprExploIcon.png"),
    ("images/sprEnergyIconBG.png", "images/sprEnergyIcon.png"),
];

/// Icon strips are 8 frames; drawn subimage = frames - ceil(fill * frames).
const AMMO_FILL_FRAMES: f32 = 7.0;

/// Source rectangle for a weapon HUD icon: subimage 1, region starting at
/// (xoffset, yoffset - 8) sized (weapon_width, 14) — scrDrawPlayerHUD.
fn weapon_icon_rect(m: SpriteMeta, wide: bool) -> Rect {
    let (_frames, w, _h, _fps, ox, oy) = (m[0], m[1], m[2], m[3], m[4], m[5]);
    let ww = if wide { 32.0 } else { 16.0 };
    let x0 = w + ox;
    let y0 = oy - 8.0;
    Rect::new(x0, y0, x0 + ww, y0 + 14.0)
}

/// Top-left gui position for weapon slot icons (24,16) and (68,16).
fn wep_slot_pos(slot: usize) -> f32 {
    24.0 + slot as f32 * 44.0
}

#[allow(clippy::type_complexity)]
fn spawn_hud_art(
    mut commands: Commands,
    catalog: Res<AssetCatalog>,
    asset_server: Res<AssetServer>,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    cam_q: Query<(Entity, &Transform, &Projection), With<Camera2d>>,
    existing: Query<(), (With<HudArt>, Without<Camera2d>)>,
) {
    if !existing.is_empty() {
        return;
    }
    let Some((cam, map)) = view_setup(&windows, &cam_q) else {
        return;
    };

    let (bar_spr, bar_tf) = gm_sprite(
        &catalog,
        &asset_server,
        &map,
        "images/sprHealthBar.png",
        2,
        20.0,
        4.0,
        1.0,
        1.0,
        Color::WHITE,
        -870.0,
    );
    commands.spawn((HudArt, ChildOf(cam), bar_spr, bar_tf));

    // Fill strips: sprHealthFill is 1 px wide; upstream stretches it over the
    // 84 px track (bg = lsthealth frame 2, fg = hp frame 1) at gui (22, 7).
    let mk_fill = |frame: usize, z: f32| {
        gm_sprite(
            &catalog,
            &asset_server,
            &map,
            "images/sprHealthFill.png",
            frame,
            22.0,
            7.0,
            84.0,
            1.0,
            Color::WHITE,
            z,
        )
    };
    let (bg_spr, bg_tf) = mk_fill(2, -869.0);
    let hp_bg = commands.spawn((HudArt, ChildOf(cam), bg_spr, bg_tf)).id();
    let (fg_spr, fg_tf) = mk_fill(1, -868.0);
    let hp_fg = commands.spawn((HudArt, ChildOf(cam), fg_spr, fg_tf)).id();

    // Experience bar: sprExpBar subimage = min(1, rads/max_rads) * 16 at (4,4).
    let (exp_spr, exp_tf) = gm_sprite(
        &catalog,
        &asset_server,
        &map,
        "images/sprExpBar.png",
        0,
        4.0,
        4.0,
        1.0,
        1.0,
        Color::WHITE,
        -869.0,
    );
    let exp_bar = commands.spawn((HudArt, ChildOf(cam), exp_spr, exp_tf)).id();

    // Level-up overlay sprExpBarLevel at (4,4), origin (1,1): shown while a
    // mutation choice is pending (GameCont.skillpoints > 0 upstream).
    let (lvl_spr, lvl_tf) = gm_sprite(
        &catalog,
        &asset_server,
        &map,
        "images/sprExpBarLevel.png",
        0,
        4.0,
        4.0,
        1.0,
        1.0,
        Color::WHITE,
        -867.0,
    );
    let exp_level = commands
        .spawn((HudArt, ChildOf(cam), Visibility::Hidden, lvl_spr, lvl_tf))
        .id();

    // Ammo icon stacks along the bottom-left, one BG + fill icon per type:
    // dx = 2 + (type-1)*10, Bolts and beyond shift left 2; dy = 32.
    let mut ammo_bg: [Option<Entity>; 5] = [None; 5];
    let mut ammo_icon: [Option<Entity>; 5] = [None; 5];
    for t in 0..5usize {
        let dx = 2.0 + t as f32 * 10.0 - if t >= 2 { 2.0 } else { 0.0 };
        let (bg_path, icon_path) = AMMO_SPRITES[t];
        let bg_static: &'static str = bg_path;
        let icon_static: &'static str = icon_path;
        let (bs, bt) = gm_sprite(
            &catalog,
            &asset_server,
            &map,
            bg_static,
            0,
            dx,
            32.0,
            1.0,
            1.0,
            Color::WHITE,
            -869.0,
        );
        ammo_bg[t] = Some(commands.spawn((HudArt, ChildOf(cam), bs, bt)).id());
        let (is, it) = gm_sprite(
            &catalog,
            &asset_server,
            &map,
            icon_static,
            7,
            dx,
            32.0,
            1.0,
            1.0,
            Color::WHITE,
            -868.0,
        );
        ammo_icon[t] = Some(commands.spawn((HudArt, ChildOf(cam), is, it)).id());
    }
    let ammo_bg = ammo_bg.map(|e| e.expect("ammo background"));
    let ammo_icon = ammo_icon.map(|e| e.expect("ammo icon"));

    // Weapon slots: four outline copies (white active, #404040 inactive)
    // around a black body, drawn from the weapon's own sprite art.
    let mut wep: [([Option<Entity>; 4], Option<Entity>); 2] = Default::default();
    for slot in 0..2usize {
        let dx = wep_slot_pos(slot);
        let outline_tint = if slot == 0 {
            Color::WHITE
        } else {
            Color::srgb_u8(64, 64, 64)
        };
        for (i, (ox, oy)) in [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)]
            .into_iter()
            .enumerate()
        {
            let (spr, tf) = gm_weapon_icon(
                &catalog,
                &asset_server,
                &map,
                WeaponId::REVOLVER,
                false,
                dx + ox,
                16.0 + oy,
                outline_tint,
                -866.0,
            );
            wep[slot].0[i] = Some(commands.spawn((HudArt, ChildOf(cam), spr, tf)).id());
        }
        let (body, btf) = gm_weapon_icon(
            &catalog,
            &asset_server,
            &map,
            WeaponId::REVOLVER,
            false,
            dx,
            16.0,
            Color::srgb_u8(0, 0, 0),
            -865.0,
        );
        wep[slot].1 = Some(commands.spawn((HudArt, ChildOf(cam), body, btf)).id());
    }
    let wep = [
        (
            wep[0].0.map(|e| e.expect("outline")),
            wep[0].1.expect("body"),
        ),
        (
            wep[1].0.map(|e| e.expect("outline")),
            wep[1].1.expect("body"),
        ),
    ];

    commands.insert_resource(HudArtRefs {
        hp_bg,
        hp_fg,
        exp_bar,
        exp_level,
        ammo_bg,
        ammo_icon,
        wep,
        wep_ids: [WeaponId::REVOLVER.0, 0],
    });
}

/// Loadout weapon icon (scrLoadoutDrawWeapon fallback path): the weapon's
/// regular sprite, centred on the draw point, scaled 2x and tilted 30°.
#[allow(clippy::too_many_arguments)]
fn gm_loadout_weapon(
    catalog: &AssetCatalog,
    assets: &AssetServer,
    map: &GuiMap,
    id: WeaponId,
    gui_x: f32,
    gui_y: f32,
    tint: Color,
    z: f32,
) -> (Sprite, Transform) {
    // Gamemaker: scr_weapon_get_loadout_sprite(_weapon) ? loadout art 1x : regular sprite 2x @30°
    let data = crate::game::content::weapon_meta(id);
    if let Some(lout) = data.wep_lout {
        let lout_path = format!("images/{lout}.png");
        if catalog.has(&lout_path) {
            let path: &'static str = Box::leak(lout_path.into_boxed_str());
            let m = meta_of(catalog, path);
            let (fw, fh) = (m[1].max(1.0), m[2].max(1.0));
            let mut sprite = sprite_exact(catalog, assets, path);
            sprite.rect = Some(Rect::new(0.0, 0.0, fw, fh));
            sprite.color = tint;
            sprite.custom_size = Some(Vec2::new(fw * map.s, fh * map.s));
            let center = map.to_world(gui_x, gui_y);
            return (sprite, Transform::from_xyz(center.x, center.y, z));
        }
    }
    let sprt = data.wep_sprt;
    let sprt_path = format!("images/{sprt}.png");
    let path: &'static str = if catalog.has(&sprt_path) {
        Box::leak(sprt_path.into_boxed_str())
    } else {
        // Fallback: HUD sprite or revolver
        crate::game::content::weapon_hud_sprite(id.0).unwrap_or("images/sprRevolver.png")
    };
    let m = meta_of(catalog, path);
    let (fw, fh) = (m[1].max(1.0), m[2].max(1.0));

    let mut sprite = sprite_exact(catalog, assets, path);
    sprite.rect = Some(Rect::new(0.0, 0.0, fw, fh));
    sprite.color = tint;
    sprite.custom_size = Some(Vec2::new(fw * 2.0 * map.s, fh * 2.0 * map.s));

    let center = map.to_world(gui_x, gui_y);
    (
        sprite,
        Transform::from_xyz(center.x, center.y, z)
            .with_rotation(Quat::from_rotation_z(30.0f32.to_radians())),
    )
}

/// A weapon HUD icon via draw_sprite_part_ext semantics (subimage 1 crop).
#[allow(clippy::too_many_arguments)]
fn gm_weapon_icon(
    catalog: &AssetCatalog,
    assets: &AssetServer,
    map: &GuiMap,
    id: WeaponId,
    wide: bool,
    gui_x: f32,
    gui_y: f32,
    tint: Color,
    z: f32,
) -> (Sprite, Transform) {
    let path = crate::game::content::weapon_hud_sprite(id.0).unwrap_or("images/sprRevolver.png");
    let path: &'static str = Box::leak(path.to_string().into_boxed_str());
    let m = meta_of(catalog, path);
    let fw = m[1].max(1.0);
    let fh = m[2].max(1.0);
    let rect = weapon_icon_rect(m, wide);

    let mut sprite = sprite_exact(catalog, assets, path);
    sprite.rect = Some(rect);
    sprite.color = tint;
    sprite.custom_size = Some(Vec2::new(rect.width() * map.s, rect.height() * map.s));

    // draw_sprite_part_ext draws without origin offset relative to the given
    // position; the crop already encodes it.
    let center = map.to_world(gui_x + rect.width() / 2.0, gui_y + rect.height() / 2.0);
    let _ = (fw, fh);
    (sprite, Transform::from_xyz(center.x, center.y, z))
}

/// Per-tick HUD sync: health fill widths, rad-bar frame, ammo icon fills,
/// weapon icons and outline tints — all from live components.
#[allow(clippy::type_complexity)]
fn sync_hud_art(
    mut refs: Option<ResMut<HudArtRefs>>,
    catalog: Res<AssetCatalog>,
    asset_server: Res<AssetServer>,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    cam_q: Query<&Projection, With<Camera2d>>,
    player_q: Query<(&Health, &Player, &Inventory), With<Player>>,
    mut sprites: Query<&mut Sprite, With<HudArt>>,
    mut transforms: Query<&mut Transform, With<HudArt>>,
    mut visibilities: Query<&mut Visibility, With<HudArt>>,
) {
    let Some(refs) = refs.as_mut() else {
        return;
    };
    let Ok((health, player, inv)) = player_q.single() else {
        return;
    };
    let (Some(window), Some(proj)) = (windows.single().ok(), cam_q.single().ok()) else {
        return;
    };
    let scale = match proj {
        Projection::Orthographic(o) => o.scale,
        _ => return,
    };
    let map = gui_map(window.width(), window.height(), scale);

    // Health fills anchored at gui x=22; width = 84 * hp fraction (NT px).
    let frac = (health.hp.max(0) as f32 / health.max.max(1) as f32).clamp(0.0, 1.0);
    for entity in [refs.hp_bg, refs.hp_fg] {
        let w = 84.0 * frac;
        if let Ok(mut spr) = sprites.get_mut(entity) {
            spr.custom_size = Some(Vec2::new((w.max(0.001)) * map.s, 8.0 * map.s));
        }
        if let Ok(mut tf) = transforms.get_mut(entity) {
            let center = map.to_world(22.0 + w / 2.0, 7.0 + 4.0);
            tf.translation.x = center.x;
        }
    }

    // Rad bar subimage = floor(min(1, rads/max) * 16).
    let rad_frac = (player.rads as f32 / player.next_level_rads.max(1) as f32).clamp(0.0, 1.0);
    let rad_frame = (rad_frac * 16.0).floor().min(16.0);
    if let Ok(mut spr) = sprites.get_mut(refs.exp_bar) {
        let fw = meta_of(&catalog, "images/sprExpBar.png")[1].max(1.0);
        spr.rect = Some(Rect::new(rad_frame * fw, 0.0, (rad_frame + 1.0) * fw, 24.0));
    }
    // Level-up overlay while a mutation pick is pending.
    if let Ok(mut vis) = visibilities.get_mut(refs.exp_level) {
        *vis = if player.rads >= player.next_level_rads && player.next_level_rads > 0 {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }

    // Ammo stacks: bg frame from equipped weapon types, icon fill from counts.
    let t1 = crate::game::content::weapon_meta(inv.weapons[0]).wep_type as usize;
    let t2 = if inv.weapon_slots > 1 {
        crate::game::content::weapon_meta(inv.weapons[1]).wep_type as usize
    } else {
        0
    };
    for t in 0..5usize {
        let kind = ammo_kind(t + 1);
        let fill = (inv.ammo[t + 1] as f32 / crate::game::content::ammo_max(kind).max(1) as f32)
            .clamp(0.0, 1.0);
        let bg_frame = if t + 1 == t1 {
            2
        } else if t + 1 == t2 {
            1
        } else {
            0
        };
        if let Ok(mut spr) = sprites.get_mut(refs.ammo_bg[t]) {
            let fw = meta_of(&catalog, AMMO_SPRITES[t].0)[1].max(1.0);
            spr.rect = Some(Rect::new(
                bg_frame as f32 * fw,
                0.0,
                (bg_frame + 1) as f32 * fw,
                12.0,
            ));
        }
        let icon_frame =
            (AMMO_FILL_FRAMES - (fill * AMMO_FILL_FRAMES).ceil()).clamp(0.0, AMMO_FILL_FRAMES);
        if let Ok(mut spr) = sprites.get_mut(refs.ammo_icon[t]) {
            let fw = meta_of(&catalog, AMMO_SPRITES[t].1)[1].max(1.0);
            let fi = icon_frame.round();
            spr.rect = Some(Rect::new(fi * fw, 0.0, (fi + 1.0) * fw, 12.0));
        }
    }

    // Weapon icons: swap texture when equipment changes; outline copies are
    // white for the active slot, #404040 for the stored one.
    for slot in 0..2usize {
        let slot_idx = slot.min(inv.weapon_slots.saturating_sub(1));
        let id = inv.weapons[slot_idx];
        let wide = slot_idx == 0 && crate::game::content::weapon_meta(id).wep_type as usize == 0;
        if refs.wep_ids[slot] != id.0 {
            refs.wep_ids[slot] = id.0;
            if let Some(path) = crate::game::content::weapon_hud_sprite(id.0) {
                for entity in refs.wep[slot]
                    .0
                    .iter()
                    .chain(std::iter::once(&refs.wep[slot].1))
                {
                    if let Ok(mut spr) = sprites.get_mut(*entity) {
                        spr.image = asset_server.load(path.to_string());
                        spr.rect = Some(weapon_icon_rect(meta_of(&catalog, path), wide));
                    }
                }
            }
        }
        let outline_tint = if slot == inv.current {
            Color::WHITE
        } else {
            Color::srgb_u8(64, 64, 64)
        };
        for entity in refs.wep[slot].0.iter() {
            if let Ok(mut spr) = sprites.get_mut(*entity) {
                spr.color = outline_tint;
            }
        }
    }
}

fn ammo_kind(idx: usize) -> AmmoKind {
    match idx {
        1 => AmmoKind::Bullets,
        2 => AmmoKind::Shells,
        3 => AmmoKind::Bolts,
        4 => AmmoKind::Explosives,
        _ => AmmoKind::Energy,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn char_select_geometry_matches_menu_create() {
        // view_height(240) - height(24) - ((36 - 24) div 2)
        assert_eq!(slot_ystart(), 210.0);
        let count = CHAR_SELECT_RACES.len();
        assert_eq!(count, 17);
        // min(20, floor((320 - 40) / 17))
        assert_eq!(slot_step(count), 16.0);
        // Last slot at 8 + 16*16; GoButton right of it plus step and 2.
        let (gx, gy) = go_button_pos(slot_step(count), count);
        assert_eq!((gx, gy), (282.0, 211.0));
    }

    #[test]
    fn gui_map_is_letterboxed_and_centered() {
        // 1280x720 @ zoom 0.45 -> visible world 576x324; NT surface scales
        // by height (1.35 world units/px) and centres horizontally.
        let m = gui_map(1280.0, 720.0, CAM_SCALE);
        assert_eq!(m.s, 1.35);
        assert_eq!(m.ox, 72.0); // (576 - 320*1.35) / 2
        assert_eq!(m.oy, 0.0);
        // Top-left of the GUI surface.
        let tl = m.to_world(0.0, 0.0);
        assert!((tl.x - (-288.0 + 72.0)).abs() < 1e-4);
        assert!((tl.y - 162.0).abs() < 1e-4);
        // Roundtrip.
        assert!((m.to_gui(m.to_world(123.0, 77.0)) - Vec2::new(123.0, 77.0)).length() < 1e-4);
    }
}

fn despawn_hud_art(
    mut commands: Commands,
    q: Query<Entity, With<HudArt>>,
    refs: Option<Res<HudArtRefs>>,
) {
    for e in &q {
        commands.entity(e).despawn();
    }
    if refs.is_some() {
        commands.remove_resource::<HudArtRefs>();
    }
}
