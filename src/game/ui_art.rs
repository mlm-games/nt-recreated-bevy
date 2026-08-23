//! Nuclear Throne GUI art (nt-rewrite draw events) rendered as
//! camera-anchored world sprites. All placement uses NT's 320x240 logical
//! GUI coordinate system mapped 1:1 into camera space; sprites keep their
//! native dimensions and GameMaker origins (from anims.json).

use bevy::audio::AudioSource;
use bevy::audio::{AudioPlayer, PlaybackMode, PlaybackSettings, Volume};
use bevy::prelude::*;

use crate::app::AppState;
use crate::game::components::{Health, Inventory, Player};
use crate::game::content::AmmoKind;
use crate::game::content::{AssetCatalog, CHAR_SELECT_RACES, WeaponId, sprite_exact};
use crate::menus::UiBridge;

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

/// Boot audio sequence (Logo/Alarm_0 + Draw_0): machinegun rattle per intro
/// frame, the meat-explosion set at frame 7, then the logo-loop ambience.
#[derive(Resource)]
struct BootSfx {
    t: f32,
    guns_fired: u8,
    booms_fired: bool,
    loop_started: bool,
}

fn splash_boot_audio(
    mut commands: Commands,
    time: Res<Time>,
    mut boot: Option<ResMut<BootSfx>>,
    catalog: Option<Res<AssetCatalog>>,
    asset_server: Res<AssetServer>,
) {
    let (Some(catalog), Some(boot)) = (catalog, boot.as_mut()) else {
        return;
    };

    boot.t += time.delta_secs();

    let play = |commands: &mut Commands, asset_server: &AssetServer, name: &str, volume: f32| {
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
    };

    // alarm[0] = 30 ticks (0.5s), then one frame every 2 ticks (33ms).
    let gun_times = [0.5, 0.533, 0.566, 0.6, 0.633];
    while (boot.guns_fired as usize) < gun_times.len()
        && boot.t >= gun_times[boot.guns_fired as usize]
    {
        play(&mut commands, &asset_server, "sndMachinegun", 0.5);
        boot.guns_fired += 1;
    }

    if !boot.booms_fired && boot.t >= 0.7 {
        play(&mut commands, &asset_server, "sndShovel", 0.8);
        play(&mut commands, &asset_server, "sndMeatExplo", 0.8);
        play(&mut commands, &asset_server, "sndExplosion", 0.8);
        boot.booms_fired = true;
    }

    if !boot.loop_started && boot.t >= 0.8 {
        let path = "audio/sndLogoLoop.wav";
        if catalog.has_audio(path) {
            commands.spawn((
                SplashLoop,
                AudioPlayer::<AudioSource>::new(asset_server.load(path)),
                PlaybackSettings {
                    mode: PlaybackMode::Loop,
                    volume: Volume::Linear(0.6),
                    ..default()
                },
            ));
        }
        boot.loop_started = true;
    }
}

/// Looping logo ambience; despawned when the splash ends.
#[derive(Component)]
struct SplashLoop;

fn despawn_splash_loop(mut commands: Commands, q: Query<Entity, With<SplashLoop>>) {
    for e in &q {
        commands.entity(e).despawn();
    }
}

#[allow(clippy::type_complexity)]
fn spawn_splash_art(
    mut commands: Commands,
    catalog: Option<Res<AssetCatalog>>,
    asset_server: Res<AssetServer>,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    cam_q: Query<(Entity, &Transform, &Projection), With<Camera2d>>,
    existing: Query<(), With<TitleArt>>,
) {
    let Some(catalog) = catalog else {
        return; // catalog scan may land a frame after state entry
    };
    if !existing.is_empty() {
        return;
    }
    let Some((cam, map)) = view_setup(&windows, &cam_q) else {
        return;
    };

    // draw_sprite(sprLogo, image_index, view_width/2, view_height/2).
    // The assembled logo is the LAST frame of the strip.
    let m = meta_of(&catalog, "images/sprLogo.png");
    let frame = (m[0] as usize).saturating_sub(1);
    let (logo_spr, logo_tf) = gm_sprite(
        &catalog,
        &asset_server,
        &map,
        "images/sprLogo.png",
        frame,
        GUI_W / 2.0,
        GUI_H / 2.0,
        1.0,
        1.0,
        Color::WHITE,
        -890.0,
    );
    commands.spawn((TitleArt, ChildOf(cam), logo_spr, logo_tf));
    commands.insert_resource(BootSfx {
        t: 0.0,
        guns_fired: 0,
        booms_fired: false,
        loop_started: false,
    });
}

pub struct UiArtPlugin;

impl Plugin for UiArtPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CharSelectArt>()
            .add_systems(OnEnter(AppState::Splash), spawn_splash_art)
            .add_systems(OnExit(AppState::Splash), despawn_title_art)
            .add_systems(
                OnEnter(AppState::Title),
                (spawn_title_art, spawn_char_select),
            )
            .add_systems(
                OnExit(AppState::Title),
                (despawn_title_art, despawn_hud_art),
            )
            .add_systems(Update, char_select_tick.run_if(in_state(AppState::Title)))
            .add_systems(OnEnter(AppState::InGame), spawn_hud_art)
            .add_systems(OnExit(AppState::InGame), despawn_hud_art)
            .add_systems(FixedUpdate, (spiral_field, sync_hud_art));
    }
}

/// (camera entity, GUI map for the current window + live ortho zoom).
fn view_setup(
    windows: &Query<&Window, With<bevy::window::PrimaryWindow>>,
    cam_q: &Query<(Entity, &Transform, &Projection), With<Camera2d>>,
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

#[allow(clippy::type_complexity)]
fn spawn_title_art(
    mut commands: Commands,
    catalog: Res<AssetCatalog>,
    asset_server: Res<AssetServer>,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    cam_q: Query<(Entity, &Transform, &Projection), With<Camera2d>>,
) {
    let Some((cam, map)) = view_setup(&windows, &cam_q) else {
        return;
    };

    commands.insert_resource(SpiralCtl {
        angle: rand::random::<f32>() * 360.0,
    });

    // SpiralCont/Create_0 warm start: 150 pre-ticked emissions.
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
}

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
    mut commands: Commands,
    catalog: Res<AssetCatalog>,
    asset_server: Res<AssetServer>,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    cam_q: Query<(Entity, &Transform, &Projection), With<Camera2d>>,
    mut ctl: Option<ResMut<SpiralCtl>>,
    mut wisps: Query<(Entity, &mut SpiralWisp, &mut Sprite)>,
) {
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
    let go = commands.spawn((TitleArt, ChildOf(cam), go_spr, go_tf)).id();
    art.go_button = Some((go, gx, gy));
    art.addy = 1.0;

    commands.insert_resource(art);
}

/// Per-frame hover/tint/animation, mirroring CharSelect/Draw_0 and
/// GoButton/Draw_0.
#[allow(clippy::type_complexity)]
fn char_select_tick(
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    cam_q: Query<(&Camera, &GlobalTransform, &Projection), With<Camera2d>>,
    mut art: ResMut<CharSelectArt>,
    bridge: Res<UiBridge>,
    mut sprites: Query<&mut Sprite>,
    mut transforms: Query<&mut Transform>,
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
        let pointed = hovered_race == *race_id as i32;
        let is_mine = selected_race == *race_id;
        // _color = (can && selected) ? c_white : c_gray
        sprite.color = if pointed || is_mine {
            Color::WHITE
        } else {
            C_GRAY
        };
    }

    // GoButton/Draw_0: animate while pointed; pop in via `addy`; lift 1 px
    // while pointed; white when pointed, c_uigray otherwise. Hidden until a
    // mutant has been clicked (visible flag).
    if let Some((entity, gx, gy)) = art.go_button {
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

    // Publish hover for the Repose tooltip layer.
    if ui.title_hover_race != hovered_race {
        ui.title_hover_race = hovered_race;
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

    // Healthbar shell: draw_sprite(sprHealthBar, 2, 20, 4). Single frame,
    // so the out-of-range subimage wraps to 0.
    let (bar_spr, bar_tf) = gm_sprite(
        &catalog,
        &asset_server,
        &map,
        "images/sprHealthBar.png",
        0,
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
