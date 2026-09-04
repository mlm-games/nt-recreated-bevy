//! Nuclear Throne GUI art (nt-rewrite draw events) rendered as
//! camera-anchored world sprites. All placement uses NT's 320x240 logical
//! GUI coordinate system mapped 1:1 into camera space; sprites keep their
//! native dimensions and GameMaker origins (from anims.json).

use std::collections::HashSet;

use bevy::audio::AudioSource;
use bevy::audio::{AudioPlayer, PlaybackMode, PlaybackSettings, Volume};
use bevy::ecs::query::QueryFilter;
use bevy::prelude::*;
use rand::RngExt;

use crate::app::AppState;
use crate::game::components::{FloorTransition, Health, Inventory, Player};
use crate::game::content::AmmoKind;
use crate::game::content::{AssetCatalog, CHAR_SELECT_RACES, WeaponId, sprite_exact};
use crate::menus::UiBridge;
use crate::save::SaveData;
use game_utils_bevy::screen_effects::CameraBase;
use game_utils_bevy::transitions::Transition;

/// Marker for menu art (title backdrop); despawned on state exit.
#[derive(Component)]
pub struct TitleArt;

/// World-space campfire scene (floors/walls/fire/chars). NOT parented to camera.
#[derive(Component)]
pub struct TitleWorldArt;

/// Marker for screen-space title UI art (camera-anchored, GUI-mapped).
/// Separates rebuildable title UI layer from the stable world/campfire scene.
#[derive(Component)]
struct TitleScreenUiArt;

#[derive(Component)]
struct CampCharArt {
    race: usize,
    /// NT-pixel offset from campfire (GM x-64, y-64).
    offset: Vec2,
    path_slct: &'static str,
    path_to: &'static str,
    path_menu: &'static str,
    path_from: &'static str,
    current: CampCharPhase,
    anim: f32,
    frames: usize,
    fw: f32,
    fh: f32,
    /// pixel scale at spawn
    s: f32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CampCharPhase {
    Slct,
    To,
    Menu,
    From,
}

/// Marker for in-game HUD art; despawned with the level.
#[derive(Component)]
pub struct HudArt;

/// Marker for mutation-choice icons (sprSkillIcon 24×32) – camera-anchored,
/// despawned when the choice is resolved.
#[derive(Component)]
struct MutationIconArt;

#[derive(Resource, Default)]
struct MutationArtRefs {
    entities: Vec<Entity>,
}

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
pub(crate) const GUI_W: f32 = 320.0;
pub(crate) const GUI_H: f32 = 240.0;
const LETTERBOX_SIZE: f32 = 36.0;
const POD_W: f32 = 16.0;
const POD_H: f32 = 24.0;
const SLOT_XSTART: f32 = 8.0;

pub const CAM_SCALE: f32 = 0.45;

/// scrDrawLetterbox `_margin`: solid-black side fill width, in GUI pixels.
/// Must use the actual letterbox sprite width, like the original GML does.
fn letterbox_margin(catalog: &AssetCatalog, effective_w: f32) -> f32 {
    let lb_w = meta_of(catalog, "images/sprLetterbox.png")[1].max(1.0);
    (effective_w - lb_w).max(0.0)
}

/// scrMenuDrawLoadout crown grid slots, GM-exact. `_crown_x` starts at
/// `_crownright - _crownsize*3` (=248), wraps when it passes `_crownright`
/// OR right after crwn_none - so RANDOM+NONE sit alone on row one and the
/// remaining twelve flow 4-per-row from `_crownleft` (=220).
/// Returns `(crown_id, gui_x, gui_y)`; crown size is 28 px.
pub fn crown_slot_positions() -> Vec<(u8, f32, f32)> {
    let step = 28.0_f32;
    let right = GUI_W + 12.0;
    let left = right - ((14 / 3) as f32) * step;
    let mut out = Vec::with_capacity(14);
    let mut x = right - step * 3.0;
    let mut y = LETTERBOX_SIZE * 2.0 - 24.0;
    for id in 0u8..14 {
        out.push((id, x, y));
        x += step;
        if x >= right || id == 1 {
            x = left;
            y += step;
        }
    }
    out
}

/// The 320x240 NT GUI surface, uniformly scaled and letterboxed inside the
/// camera view (exactly how GameMaker's GUI layer behaves).
///
/// `s` is world units per NT pixel; `ox`/`oy` are the centered margins in
/// world units. Derived from the *live* ortho scale so gameplay zoom keeps
/// the surface glued to the same screen rect.
pub(crate) struct GuiMap {
    pub(crate) s: f32,
    pub(crate) ox: f32,
    pub(crate) oy: f32,
    pub(crate) hw: f32,
    pub(crate) hh: f32,
}

pub(crate) fn gui_map(win_w: f32, win_h: f32, cam_scale: f32) -> GuiMap {
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
    pub(crate) fn to_world(&self, x: f32, y: f32) -> Vec2 {
        Vec2::new(
            -self.hw + self.ox + x * self.s,
            self.hh - self.oy - y * self.s,
        )
    }

    pub(crate) fn to_gui(&self, p: Vec2) -> Vec2 {
        Vec2::new(
            (p.x + self.hw - self.ox) / self.s,
            (self.hh - p.y - self.oy) / self.s,
        )
    }
}

/// GameMaker builtin `c_gray` - unselected char-select pods.
const C_GRAY: Color = Color::srgb_u8(128, 128, 128);
/// `#999999` (`c_uigray`, macros_gameplay.gml) - unhovered GoButton.
const C_UIGRAY: Color = Color::srgb_u8(153, 153, 153);

/// Slot geometry reproduced from nt-rewrite `Menu/Create_0`.
pub fn slot_ystart() -> f32 {
    GUI_H - POD_H - ((LETTERBOX_SIZE - POD_H) / 2.0).floor()
}

fn slot_step(count: usize) -> f32 {
    // Menu/Create_0: min(20, floor((game_screen_width - 40) / count))
    // where game_screen_width is the FIXED #macro 320 (macros_general.gml:6).
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

/// scrRaceGetMaxSkinCount: BigDog/Frog 1, Skeleton 2, Robot 4, else 3.
pub fn max_skin_count(race: usize) -> usize {
    match race {
        13 | 15 => 1,
        14 => 2,
        8 => 4,
        _ => 3,
    }
}

fn race_default_weapon_id(race: usize) -> u8 {
    match race {
        6 => 255,  //TODO: Venuz golden_revolver - not in our subset
        9 => 254,  // Chicken sword
        12 => 253, // Rogue rifle
        13 => 252, // BigDog spin
        14 => 251, // Skeleton rusty
        15 => 250, // Frog golden pistol
        16 => 255, // Cuz golden
        _ => WeaponId::REVOLVER.0,
    }
}

/// scrMenuDrawLoadout skin column: x = _crownleft - _crownsize/2 - 22 = 184;
/// y starts at gui_h/2 - (skinsize/2)*count - 2 and steps 28 per entry.
/// Returns `(idx, gui_x, gui_y)` for `count` entries.
pub fn skin_slot_positions(count: usize) -> Vec<(usize, f32, f32)> {
    let size = 28.0_f32; // sprLoadoutSkin width (32) - 4
    let x = 220.0 - 28.0_f32 * 0.5 - 22.0;
    let mut y = GUI_H * 0.5 - (size * 0.5) * count as f32 - 2.0;
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        out.push((i, x, y));
        y += size;
    }
    out
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

// Boot logo (nt-rewrite object `Logo`: sprLogo centred on the GUI)

/// The full Vlambeer boot sequence (objects `Vlambeer` + `Logo`):
///
/// mode 0: sprSaving icon + "do not turn off" note      (120 ticks)
/// mode 1: "MADE IN GAMEMAKER"                          (60 ticks)
/// mode 2: sprVlambeer card + additive glow            (120 ticks)
/// mode 3: team credits                                 (60 ticks)
/// mode 4: NT logo - frame-stepped machinegun intro,    (input)
///         then any key/click -> main menu buttons.
///
/// Every card sprite is spawned ONCE up front and kept hidden; card switches
/// flip Visibility for the outgoing and incoming sets in the SAME frame, so a
/// new card can never composite over (or lag behind) the previous one,
/// regardless of command-flush timing.
#[derive(Resource)]
struct BootState {
    mode: u8,
    t: f32,
    da: f32,
    shake: f32,
    guns: u8,
    booms: bool,
    wave: f32,
    /// Last mode whose sprites were made visible (one-shot gating).
    rendered_mode: i8,
    /// All card art built and parked hidden.
    built: bool,
    /// sprSaving icon (mode 0).
    icon: Option<Entity>,
    /// sprVlambeer main + ten additive glow copies (mode 2).
    vlambeer: Vec<Entity>,
    /// NT logo (mode 4).
    logo: Option<Entity>,
    /// NT logo glow (mode 4, image_index 7): 8 additive copies.
    logo_glow: Vec<Entity>,
    /// Per-mode Repose-replacement text lines, pre-spawned hidden:
    /// (mode, entities). Rendered as Text2d so text and sprite cards share
    /// ONE visibility timeline - no cross-renderer timing at all.
    texts: Vec<(u8, Vec<Entity>)>,
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
            wave: 0.0,
            rendered_mode: -1,
            built: false,
            icon: None,
            vlambeer: Vec::new(),
            logo: None,
            logo_glow: Vec::new(),
            texts: Vec::new(),
        }
    }
}

fn reset_boot(mut boot: ResMut<BootState>) {
    *boot = BootState::default();
}

fn despawn_boot_art(mut commands: Commands, q: Query<Entity, With<BootArt>>) {
    for e in &q {
        commands.entity(e).try_despawn();
    }
}

/// Quit-to-menu path: SpiralCont was destroyed with the run, rebuild it.
/// The swirl itself is the WGSL vortex quad (`game::vortex`); this only
/// re-arms the controller resource and the portal ambience.
#[allow(clippy::type_complexity)]
fn spawn_spiral_field(
    mut commands: Commands,
    state: Res<State<AppState>>,
    ctl: Option<Res<crate::game::vortex::SpiralCtl>>,
    portal: Query<(), With<PortalLoop>>,
    catalog: Option<Res<AssetCatalog>>,
    asset_server: Res<AssetServer>,
) {
    if *state.get() != AppState::MainMenu || ctl.is_some() {
        return;
    }
    let Some(catalog) = catalog else {
        return;
    };
    commands.insert_resource(crate::game::vortex::SpiralCtl::warmed_up());
    if portal.is_empty() {
        play_loop(
            &mut commands,
            &catalog,
            &asset_server,
            "sndPortalLoop",
            0.5,
            PortalLoop,
        );
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
pub(crate) struct PortalLoop;

/// Campfire ambience loop started on Title enter (Menu/Create_0 MusCont amb0).
#[derive(Component)]
struct CampfireAmb;

fn despawn_campfire_amb(mut commands: Commands, q: Query<Entity, With<CampfireAmb>>) {
    for e in &q {
        commands.entity(e).try_despawn();
    }
}

fn despawn_splash_loop(mut commands: Commands, q: Query<Entity, With<SplashLoop>>) {
    for e in &q {
        commands.entity(e).try_despawn();
    }
}

/// The GameMaker splash draw event calls draw_clear(c_black). In this port the
/// splash cards are Bevy world sprites, so the clear must live in the same
/// camera layer as BootArt, not in Repose.
#[derive(Component)]
struct BootClear;

fn set_splash_camera_clear(mut q: Query<&mut Camera, With<Camera2d>>) {
    for mut camera in &mut q {
        camera.clear_color = bevy::camera::ClearColorConfig::Custom(Color::BLACK);
    }
}

fn restore_camera_clear(mut q: Query<&mut Camera, With<Camera2d>>) {
    for mut camera in &mut q {
        camera.clear_color = bevy::camera::ClearColorConfig::Default;
    }
}

fn spawn_boot_clear(
    mut commands: Commands,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    cam_q: Query<(Entity, &Transform, &Projection), With<Camera2d>>,
) {
    let Some((cam, map)) = view_setup(&windows, &cam_q) else {
        return;
    };

    let c = map.to_world(GUI_W * 0.5, GUI_H * 0.5);
    commands.spawn((
        BootArt,
        BootClear,
        ChildOf(cam),
        Sprite {
            color: Color::BLACK,
            custom_size: Some(Vec2::new(GUI_W * map.s, GUI_H * map.s)),
            ..default()
        },
        Transform::from_xyz(c.x, c.y, -999.0),
    ));
}

fn resolve_audio_path(catalog: &AssetCatalog, stem: &str) -> Option<String> {
    catalog.resolve_audio_path(stem)
}

fn play_cue(
    commands: &mut Commands,
    catalog: &AssetCatalog,
    asset_server: &AssetServer,
    name: &str,
    volume: f32,
) {
    let path = match resolve_audio_path(catalog, name) {
        Some(p) => p,
        None => {
            bevy::log::warn!("missing audio cue: {name}");
            return;
        }
    };
    commands.spawn((
        AudioPlayer::<AudioSource>::new(asset_server.load(path)),
        PlaybackSettings {
            mode: PlaybackMode::Despawn,
            volume: Volume::Linear(volume),
            ..default()
        },
    ));
}

fn play_loop(
    commands: &mut Commands,
    catalog: &AssetCatalog,
    asset_server: &AssetServer,
    name: &str,
    volume: f32,
    marker: impl Bundle,
) {
    let path = match resolve_audio_path(catalog, name) {
        Some(p) => p,
        None => {
            bevy::log::warn!("missing audio loop: {name}");
            return;
        }
    };
    commands.spawn((
        marker,
        AudioPlayer::<AudioSource>::new(asset_server.load(path)),
        PlaybackSettings {
            mode: PlaybackMode::Loop,
            volume: Volume::Linear(volume),
            ..default()
        },
    ));
}

fn sync_boot_mode_ui(bridge: &UiBridge, mode: u8) {
    if let Ok(mut ui) = bridge.shared.lock() {
        ui.boot_mode = mode.min(4);
    }
}

fn sprite_frame_count(catalog: &AssetCatalog, path: &str) -> usize {
    meta_of(catalog, path)[0].max(1.0) as usize
}

fn wrap_sprite_frame(catalog: &AssetCatalog, path: &str, raw: usize) -> usize {
    raw % sprite_frame_count(catalog, path).max(1)
}

fn pick_usize(items: &[usize]) -> usize {
    items[rand::rng().random_range(0..items.len())]
}

fn pick_i32(items: &[i32]) -> i32 {
    items[rand::rng().random_range(0..items.len())]
}

fn campfire_floor_frame(catalog: &AssetCatalog) -> usize {
    // Floor/Create_0:
    // if random(500)<1 image_index=3
    // else image_index = choose(0,0,0,0,0,0,0,1,2) + choose(0,4)
    let raw = if rand::random::<f32>() * 500.0 < 1.0 {
        3
    } else {
        pick_usize(&[0, 0, 0, 0, 0, 0, 0, 1, 2]) + pick_usize(&[0, 4])
    };
    wrap_sprite_frame(catalog, "images/sprFloor0.png", raw)
}

fn floor0_frame(catalog: &AssetCatalog) -> usize {
    campfire_floor_frame(catalog)
}

fn title_world_to_gui(wx: f32, wy: f32) -> (f32, f32) {
    // scrCampfireMenuCreate places the campfire at world (64,64).
    // The Bevy title scene places the campfire at the GUI center, so every
    // MenuGen/FloorMaker world coordinate is rendered relative to that anchor.
    (GUI_W * 0.5 + wx - 64.0, GUI_H * 0.5 + wy - 64.0)
}

fn title_floor_owner_below(wx: i32, wy: i32) -> (i32, i32) {
    (wx.div_euclid(2), (wy + 1).div_euclid(2))
}

fn title_wall_xy(wx: i32, wy: i32) -> (f32, f32) {
    title_world_to_gui(wx as f32 * 16.0, wy as f32 * 16.0)
}

fn add_title_floor_cell(floors: &mut HashSet<(i32, i32)>, wx: i32, wy: i32) {
    // Floor object positions are 32px-grid world coordinates.
    // Duplicate Floor creation is ignored by Floor/Create_0 via place_meeting.
    floors.insert((wx.div_euclid(32), wy.div_euclid(32)));
}

fn floor_maker_step_delta(direction: i32) -> (i32, i32) {
    match direction.rem_euclid(360) {
        0 => (32, 0),
        90 => (0, -32),
        180 => (-32, 0),
        270 => (0, 32),
        _ => (0, 0),
    }
}

fn menu_gen_floor_cells() -> HashSet<(i32, i32)> {
    // Exact title-area floor source:
    // MenuGen/Create_0 creates the initial 3x4 cluster field, then creates
    // four FloorMaker instances at choose(0,32,64,96,128). FloorMaker/Create_0
    // sets goal=50 while MenuGen exists, and scrMakeFloor uses the area 0
    // branch for turns/splitting.
    let mut floors: HashSet<(i32, i32)> = HashSet::new();

    let mut dix = 32_i32;
    let mut diy = 32_i32;

    for _row in 0..3 {
        for _col in 0..4 {
            // GameMaker choose(), not weighted: choose(32,0,-32).
            let mody = pick_i32(&[32, 0, -32]);
            let cx = dix + mody;
            let cy = diy + mody;

            for oy in [-32, 0, 32] {
                for ox in [-32, 0, 32] {
                    add_title_floor_cell(&mut floors, cx + ox, cy + oy);
                }
            }

            dix += 32;
        }

        // This is intentionally 0, not 32. It matches MenuGen/Create_0.
        dix = 0;
        diy += 32;
    }

    #[derive(Clone, Copy)]
    struct Maker {
        x: i32,
        y: i32,
        direction: i32,
    }

    let mut makers: Vec<Maker> = Vec::with_capacity(8);

    for _ in 0..4 {
        let x = pick_i32(&[0, 32, 64, 96, 128]);
        let y = pick_i32(&[0, 32, 64, 96, 128]);
        let direction = pick_i32(&[0, 0, 90, 180, 270]);

        // FloorMaker/Create_0 ends by creating a Floor at its position.
        add_title_floor_cell(&mut floors, x, y);
        makers.push(Maker { x, y, direction });
    }

    let mut guard = 0usize;
    while !makers.is_empty() && guard < 512 {
        guard += 1;

        let active_count = makers.len() as i32;
        let mut next: Vec<Maker> = Vec::with_capacity(makers.len() + 2);

        for mut maker in makers.drain(..) {
            // FloorMaker/Step_0: if instance_number(Floor) > goal, create one
            // last floor and destroy the maker.
            if floors.len() > 50 {
                add_title_floor_cell(&mut floors, maker.x, maker.y);
                continue;
            }

            // scrMakeFloor area_campfire branch.
            let (dx, dy) = floor_maker_step_delta(maker.direction);
            maker.x += dx;
            maker.y += dy;
            add_title_floor_cell(&mut floors, maker.x, maker.y);

            let trn = pick_i32(&[0, 0, 90, -90, 90, -90, 180]);
            maker.direction = (maker.direction + trn).rem_euclid(360);

            // scrMakeFloor creates another Floor on 180-degree turns.
            if trn == 180 {
                add_title_floor_cell(&mut floors, maker.x, maker.y);
            }

            // Area 0 early-destroy/split rules.
            let span = 19 + active_count;
            if rand::random::<f32>() * span as f32 > 22.0 {
                add_title_floor_cell(&mut floors, maker.x, maker.y);
                continue;
            }

            next.push(maker);

            if rand::random::<f32>() * 4.0 < 1.0 {
                let child = Maker {
                    x: maker.x,
                    y: maker.y,
                    direction: pick_i32(&[0, 0, 90, 180, 270]),
                };
                add_title_floor_cell(&mut floors, child.x, child.y);
                next.push(child);
            }
        }

        makers = next;
    }

    // MenuGen/Alarm_1:
    // with(Floor) create missing cardinal neighbours.
    let base: Vec<(i32, i32)> = floors.iter().copied().collect();
    for (cx, cy) in base {
        floors.insert((cx - 1, cy));
        floors.insert((cx + 1, cy));
        floors.insert((cx, cy - 1));
        floors.insert((cx, cy + 1));
    }

    floors
}

// Wall/Create_0 exact frame families for area_campfire / sprWall0*
fn campfire_wall_body_frame(catalog: &AssetCatalog) -> usize {
    let raw = if rand::random::<f32>() * 150.0 < 1.0 {
        3
    } else {
        pick_usize(&[0, 0, 0, 0, 0, 0, 0, 1, 2]) + pick_usize(&[0, 4])
    };
    wrap_sprite_frame(catalog, "images/sprWall0Bot.png", raw)
}

fn campfire_wall_top_frame(catalog: &AssetCatalog) -> usize {
    let raw = if rand::random::<f32>() * 200.0 < 1.0 {
        3
    } else {
        pick_usize(&[0, 0, 0, 0, 0, 0, 0, 1, 2]) + pick_usize(&[0, 4, 8])
    };
    wrap_sprite_frame(catalog, "images/sprWall0Top.png", raw)
}

fn campfire_wall_out_frame(catalog: &AssetCatalog) -> usize {
    let raw = pick_usize(&[0, 0, 0, 0, 1, 2, 3, 4]) + pick_usize(&[0, 4]);
    wrap_sprite_frame(catalog, "images/sprWall0Out.png", raw)
}

fn wall0_body_frame(catalog: &AssetCatalog) -> usize {
    campfire_wall_body_frame(catalog)
}

fn wall0_top_frame(catalog: &AssetCatalog) -> usize {
    campfire_wall_top_frame(catalog)
}

fn wall0_out_frame(catalog: &AssetCatalog) -> usize {
    campfire_wall_out_frame(catalog)
}

fn camp_char_sprite_set(
    catalog: &AssetCatalog,
    race: crate::game::content::RaceId,
) -> Option<(&'static str, &'static str, &'static str, &'static str)> {
    use crate::game::content::RaceId;
    if race == RaceId::BigDog {
        let slct = "images/sprScrapBossSleep.png";
        let menu = "images/sprScrapBossIdle.png";
        let to = "images/sprScrapBossIntro.png";
        let from = "images/sprScrapBossSleepHurt.png";
        if catalog.has(slct) {
            return Some((slct, to, menu, from));
        }
    }
    let base = match race {
        RaceId::Fish => "Fish",
        RaceId::Crystal => "Crystal",
        RaceId::Eyes => "Eyes",
        RaceId::Melting => "Melting",
        RaceId::Plant => "Plant",
        RaceId::Venuz => "Venuz",
        RaceId::Steroids => "Steroids",
        RaceId::Robot => "Robot",
        RaceId::Chicken => "Chicken",
        RaceId::Rebel => "Rebel",
        RaceId::Horror => "Horror",
        RaceId::Rogue => "Rogue",
        RaceId::BigDog => "Dog",
        RaceId::Skeleton => "Skeleton",
        RaceId::Frog => "Fish",
        RaceId::Cuz => "Cuz",
        RaceId::Random => return None,
    };
    let menu = format!("images/spr{base}Menu.png");
    let sel = format!("images/spr{base}MenuSelect.png");
    let selected = format!("images/spr{base}MenuSelected.png");
    let desel = format!("images/spr{base}MenuDeselect.png");
    let idle = format!("images/sprMutant{}Idle.png", race as u8);
    let pick = |p: String, fb: &str| -> Option<&'static str> {
        if catalog.has(&p) {
            Some(Box::leak(p.into_boxed_str()))
        } else if !fb.is_empty() && catalog.has(fb) {
            Some(Box::leak(fb.to_string().into_boxed_str()))
        } else {
            None
        }
    };
    let slct = pick(menu.clone(), &idle)?;
    let to = pick(sel, slct).unwrap_or(slct);
    let menu_s = pick(selected, slct).unwrap_or(slct);
    let from = pick(desel, slct).unwrap_or(slct);
    Some((slct, to, menu_s, from))
}

fn spawn_world_sprite(
    commands: &mut Commands,
    catalog: &AssetCatalog,
    assets: &AssetServer,
    path: &'static str,
    frame: usize,
    nt_x: f32,
    nt_y: f32,
    s: f32,
    z: f32,
    flip_x: bool,
) -> Entity {
    let m = meta_of(catalog, path);
    let (frames, fw, fh, _fps, xorigin, yorigin) = (
        m[0].max(1.0),
        m[1].max(1.0),
        m[2].max(1.0),
        m[3],
        m[4],
        m[5],
    );
    let frame = frame % frames as usize;
    let mut sprite = sprite_exact(catalog, assets, path);
    sprite.rect = Some(Rect::new(
        frame as f32 * fw,
        0.0,
        (frame as f32 + 1.0) * fw,
        fh,
    ));
    sprite.custom_size = Some(Vec2::new(fw * s, fh * s));
    sprite.flip_x = flip_x;
    let left = nt_x - xorigin;
    let top = nt_y - yorigin;
    let cx = (left + fw * 0.5) * s;
    let cy = -(top + fh * 0.5) * s;
    commands
        .spawn((
            TitleArt,
            TitleWorldArt,
            sprite,
            Transform::from_xyz(cx, cy, z),
        ))
        .id()
}

fn apply_camp_char_path(
    catalog: &AssetCatalog,
    assets: &AssetServer,
    spr: &mut Sprite,
    path: &'static str,
    cc: &mut CampCharArt,
) {
    let m = meta_of(catalog, path);
    cc.frames = m[0].max(1.0) as usize;
    cc.fw = m[1].max(1.0);
    cc.fh = m[2].max(1.0);
    spr.image = assets.load(path);
    spr.rect = Some(Rect::new(0.0, 0.0, cc.fw, cc.fh));
    spr.custom_size = Some(Vec2::new(cc.fw * cc.s, cc.fh * cc.s));
}

fn set_title_camera_clear(mut q: Query<&mut Camera, With<Camera2d>>) {
    for mut camera in &mut q {
        camera.clear_color = bevy::camera::ClearColorConfig::Custom(Color::srgb_u8(106, 122, 175));
    }
}

/// Card stage lengths. Upstream Vlambeer/Create_0 sets alarm[0]=120 and
/// Alarm_0 re-arms 60 (+60 for the Vlambeer card) at a game speed of
/// 30 fps (UberCont Step_0: game_set_speed(30, gamespeed_fps)).
const MODE_SECS: [f32; 4] = [4.0, 2.0, 4.0, 2.0];

/// Build every boot-card sprite once, parked hidden. Card switches only flip
/// Visibility (old set hidden + new set visible queued in the same frame), so
/// swaps are atomic - no compositing, no blank gaps.
#[allow(clippy::too_many_arguments)]
/// One centred Silkscreen line, the Bevy-sprite twin of the old Repose
/// `nt_text_at(..., centered)` splash labels.
fn splash_text_line(
    commands: &mut Commands,
    cam: Entity,
    map: &GuiMap,
    font: &Handle<Font>,
    text: &str,
    gui_x: f32,
    gui_y: f32,
    color: Color,
) -> Entity {
    let font_size = (7.0 * map.s).clamp(8.0, 96.0);
    let c = map.to_world(gui_x, gui_y);
    commands
        .spawn((
            BootArt,
            ChildOf(cam),
            Visibility::Hidden,
            Text2d::new(text.to_string()),
            TextFont {
                font: font.clone().into(),
                font_size: FontSize::Px(font_size),
                ..default()
            },
            TextColor(color),
            TextLayout::justify(Justify::Center),
            Transform::from_xyz(c.x, c.y, -802.0),
        ))
        .id()
}

fn build_boot_cards(
    commands: &mut Commands,
    catalog: &AssetCatalog,
    asset_server: &AssetServer,
    map: &GuiMap,
    cam: Entity,
    font: &Handle<Font>,
    boot: &mut BootState,
) {
    // Mode 0: saving icon. Vlambeer/Create_0 plays the jingle with it.
    play_cue(commands, catalog, asset_server, "sndVlambeer", 0.7);
    let (spr, tf) = gm_sprite(
        catalog,
        asset_server,
        map,
        "images/sprSaving.png",
        0,
        GUI_W / 2.0,
        GUI_H / 2.0 - 16.0,
        1.0,
        1.0,
        Color::WHITE,
        -801.0,
    );
    boot.icon = Some(
        commands
            .spawn((BootArt, ChildOf(cam), Visibility::Hidden, spr, tf))
            .id(),
    );

    // Mode 2: Vlambeer/Draw_0 draws the card at
    //   _px = (view_width - sprite_width) div 2
    //   _py = view_height - sprite_height
    // (origin 0,0), plus ten additive orandom(4) glow copies.
    let m = meta_of(catalog, "images/sprVlambeer.png");
    let fw = m[1].max(1.0);
    let fh = m[2].max(1.0);
    let ox = m[4];
    let oy = m[5];
    let px = ((GUI_W - fw) * 0.5).floor() + ox;
    let py = (GUI_H - fh) + oy;

    let (spr, tf) = gm_sprite(
        catalog,
        asset_server,
        map,
        "images/sprVlambeer.png",
        0,
        px,
        py,
        1.0,
        1.0,
        Color::WHITE,
        -801.0,
    );
    boot.vlambeer.push(
        commands
            .spawn((BootArt, ChildOf(cam), Visibility::Hidden, spr, tf))
            .id(),
    );
    for _ in 0..10 {
        let (g, gtf) = gm_sprite(
            catalog,
            asset_server,
            map,
            "images/sprVlambeer.png",
            0,
            px,
            py,
            1.0,
            1.0,
            Color::srgba(1.0, 1.0, 1.0, 0.1),
            -800.5,
        );
        boot.vlambeer.push(
            commands
                .spawn((BootArt, ChildOf(cam), Visibility::Hidden, g, gtf))
                .id(),
        );
    }

    // Mode 4: NT logo. Frame 0 is blank; it builds up per machinegun shot.
    let (spr, tf) = gm_sprite(
        catalog,
        asset_server,
        map,
        "images/sprLogo.png",
        0,
        GUI_W / 2.0,
        GUI_H / 2.0,
        1.0,
        1.0,
        Color::WHITE,
        -801.0,
    );
    boot.logo = Some(
        commands
            .spawn((BootArt, ChildOf(cam), Visibility::Hidden, spr, tf))
            .id(),
    );
    // Mode 4 glow: Logo/Draw_0 draws 8 additive sprLogoGlow copies around
    // the logo when image_index == 7, radius 4 + sin(wave)*(2+random(1)).
    for _ in 0..8 {
        let (g, gtf) = gm_sprite(
            catalog,
            asset_server,
            map,
            "images/sprLogoGlow.png",
            0,
            GUI_W / 2.0,
            GUI_H / 2.0,
            1.0,
            1.0,
            Color::srgba(1.0, 1.0, 1.0, 0.05),
            -800.8,
        );
        boot.logo_glow.push(
            commands
                .spawn((BootArt, ChildOf(cam), Visibility::Hidden, g, gtf))
                .id(),
        );
    }

    // Text cards (modes 0/1/3) - same hidden-until-switched lifecycle as the
    // sprite cards above.
    let cy = GUI_H / 2.0;
    let mut group = |lines: Vec<(&str, Color)>, base_y: f32, step: f32| -> Vec<Entity> {
        lines
            .iter()
            .enumerate()
            .map(|(i, (s, col))| {
                splash_text_line(
                    commands,
                    cam,
                    map,
                    font,
                    s,
                    GUI_W / 2.0,
                    base_y + step * i as f32,
                    *col,
                )
            })
            .collect()
    };

    let mode0 = group(
        vec![
            ("DO NOT TURN OFF NUCLEAR THRONE", Color::WHITE),
            ("WHILE THIS SAVING ICON IS DISPLAYED.", Color::WHITE),
        ],
        cy + 20.0,
        10.0,
    );
    boot.texts.push((0, mode0));

    let mode1 = group(vec![("MADE IN GAMEMAKER", Color::WHITE)], cy, 10.0);
    boot.texts.push((1, mode1));

    const CREDITS: [(&str, bool); 9] = [
        ("VLAMBEER", true),
        ("", false),
        ("PAUL VEER", false),
        ("JUKIO KALLIO", false),
        ("JOONAS TURNER", false),
        ("JUSTIN CHAN", false),
        ("YELLOWAFTERLIFE", false),
        ("", false),
        ("PRESENT", false),
    ];
    let gold = Color::srgb_u8(255, 221, 0);
    let white = Color::WHITE;
    let mode3 = CREDITS
        .iter()
        .enumerate()
        .filter(|(_, (s, _))| !s.is_empty())
        .map(|(i, (s, is_gold))| {
            splash_text_line(
                commands,
                cam,
                map,
                font,
                s,
                GUI_W / 2.0,
                cy + (i as f32 - 4.0) * 10.0,
                if *is_gold { gold } else { white },
            )
        })
        .collect();
    boot.texts.push((3, mode3));

    boot.built = true;
}

/// Vlambeer + Logo boot driver.
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
    ui_font: Res<crate::app::UiFont>,
    mut boot: ResMut<BootState>,
    mut sprites: Query<&mut Sprite, With<BootArt>>,
    mut transforms: Query<&mut Transform, With<BootArt>>,
    mut visibilities: Query<&mut Visibility, With<BootArt>>,
) {
    if *state.get() != AppState::Splash {
        return;
    }
    let Some(catalog) = catalog else {
        return;
    };

    // Build all card art once (hidden); retry until the camera view is ready.
    if !boot.built {
        let Some((cam, map)) = view_setup(&windows, &cam_q) else {
            return;
        };
        build_boot_cards(
            &mut commands,
            &catalog,
            &asset_server,
            &map,
            cam,
            &ui_font.0,
            &mut boot,
        );
    }

    let dt = time.delta_secs();
    let pressed =
        mouse.get_just_pressed().next().is_some() || keys.get_just_pressed().next().is_some();

    // Keep Repose text locked to the current card.
    sync_boot_mode_ui(&bridge, boot.mode);

    // ----- Logo stage (mode 4) -----
    if boot.mode == 4 {
        boot.t += dt;

        if let Some(logo) = boot.logo {
            if let Ok(mut vis) = visibilities.get_mut(logo) {
                *vis = Visibility::Visible;
            }
        }

        // Logo/Alarm_0 (30 fps; Create_0 arms alarm[0]=30): index 1 after
        // 1.0s, then every 2 ticks; after frame 6 wait 20 ticks, then frame 7
        // + boom set + logo-loop ambience. Times when image_index hits 1..7:
        const STEP_T: [f32; 7] = [
            1.0,
            1.0 + 2.0 / 30.0,
            1.0 + 4.0 / 30.0,
            1.0 + 6.0 / 30.0,
            1.0 + 8.0 / 30.0,
            1.0 + 10.0 / 30.0,
            1.0 + 10.0 / 30.0 + 20.0 / 30.0,
        ];
        while (boot.guns as usize) < STEP_T.len() && boot.t >= STEP_T[boot.guns as usize] {
            boot.guns += 1;
            if boot.guns >= 7 {
                if !boot.booms {
                    play_loop(
                        &mut commands,
                        &catalog,
                        &asset_server,
                        "sndLogoLoop",
                        0.6,
                        SplashLoop,
                    );
                    play_cue(&mut commands, &catalog, &asset_server, "sndShovel", 0.8);
                    play_cue(&mut commands, &catalog, &asset_server, "sndMeatExplo", 0.8);
                    play_cue(&mut commands, &catalog, &asset_server, "sndExplosion", 0.8);
                    boot.shake += 2.5;
                    boot.booms = true;
                }
            } else {
                play_cue(&mut commands, &catalog, &asset_server, "sndMachinegun", 0.5);
                boot.shake += 0.5;
            }
        }

        // Draw_0: the logo steps to the current frame and jitters by shake,
        // which decays one unit per tick.
        boot.shake = (boot.shake - dt * 30.0).max(0.0);
        if let Some(logo) = boot.logo {
            if let Ok(mut spr) = sprites.get_mut(logo) {
                let m = meta_of(&catalog, "images/sprLogo.png");
                let (fw, fh) = (m[1].max(1.0), m[2].max(1.0));
                let f = (boot.guns as f32).min(7.0);
                spr.rect = Some(Rect::new(f * fw, 0.0, (f + 1.0) * fw, fh));
            }
            if let (Ok(mut tf), Some((_, map))) =
                (transforms.get_mut(logo), view_setup(&windows, &cam_q))
            {
                let jx = (rand::random::<f32>() - 0.5) * 2.0 * boot.shake;
                let jy = (rand::random::<f32>() - 0.5) * 2.0 * boot.shake;
                let c = map.to_world(GUI_W / 2.0 + jx, GUI_H / 2.0 + jy);
                tf.translation = c.extend(-801.0);
            }
        }

        // Logo/Draw_0 glow: sprLogoGlow additive at image_index == 7.
        let show_glow = boot.guns >= 7;
        for &e in &boot.logo_glow.clone() {
            if let Ok(mut v) = visibilities.get_mut(e) {
                *v = if show_glow {
                    Visibility::Visible
                } else {
                    Visibility::Hidden
                };
            }
        }
        if show_glow {
            boot.wave += dt * 3.9;
            // Reuse the shaken centre; fallback to GUI centre if logo missing.
            let (base_x, base_y) = if let Some(logo) = boot.logo
                && let Ok(tf) = transforms.get(logo)
            {
                // tf already at shaken world pos; convert back to GUI for radial offset.
                if let Some((_, map)) = view_setup(&windows, &cam_q) {
                    let g = map.to_gui(tf.translation.truncate());
                    (g.x, g.y)
                } else {
                    (GUI_W / 2.0, GUI_H / 2.0)
                }
            } else {
                (GUI_W / 2.0, GUI_H / 2.0)
            };
            if let Some((_, map)) = view_setup(&windows, &cam_q) {
                for (i, &e) in boot.logo_glow.iter().enumerate() {
                    if let Ok(mut tf) = transforms.get_mut(e) {
                        let ang = i as f32 * 45.0;
                        let r_extra: f32 = rand::random::<f32>();
                        let radius = 4.0 + (boot.wave + i as f32 * 0.02).sin() * (2.0 + r_extra);
                        let jx = radius * ang.to_radians().cos();
                        let jy = radius * ang.to_radians().sin();
                        // GML lengthdir_y is +sin in GUI coords (y-down positive), so jy as is.
                        let c = map.to_world(base_x + jx, base_y + jy);
                        tf.translation = c.extend(-800.8);
                    }
                }
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
    // Advance only after the current card has been displayed at least once.
    let can_advance = boot.rendered_mode == boot.mode as i8;
    if can_advance && (pressed || boot.t >= MODE_SECS[boot.mode as usize]) {
        boot.mode += 1;
        boot.t = 0.0;
        boot.rendered_mode = -1;

        if boot.mode == 4 {
            // Vlambeer/Alarm_0 mode >= 3: SpiralCont + portal drone.
            // The swirl is the WGSL vortex quad; just arm the controller
            // (pre-warmed so the field is established immediately).
            commands.insert_resource(crate::game::vortex::SpiralCtl::warmed_up());
            play_loop(
                &mut commands,
                &catalog,
                &asset_server,
                "sndPortalLoop",
                0.5,
                PortalLoop,
            );
        } else {
            play_cue(&mut commands, &catalog, &asset_server, "sndRestart", 0.7);
        }
    } else {
        boot.t += dt;
    }

    // Atomic card swap: outgoing and incoming Visibility flips are queued in
    // the same frame, so they apply together. This MUST run for every mode
    // change INCLUDING the 3->4 hand-off to the logo stage, which otherwise
    // never hides the credits text group.
    if let Some(icon) = boot.icon
        && let Ok(mut vis) = visibilities.get_mut(icon)
    {
        *vis = if boot.mode == 0 {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
    for e in &boot.vlambeer {
        if let Ok(mut vis) = visibilities.get_mut(*e) {
            *vis = if boot.mode == 2 {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };
        }
    }
    if let Some(logo) = boot.logo
        && let Ok(mut vis) = visibilities.get_mut(logo)
    {
        *vis = if boot.mode == 4 {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
    for (group_mode, ents) in &boot.texts {
        for e in ents {
            if let Ok(mut vis) = visibilities.get_mut(*e) {
                *vis = if *group_mode == boot.mode {
                    Visibility::Visible
                } else {
                    Visibility::Hidden
                };
            }
        }
    }

    if boot.mode >= 4 {
        return;
    }

    // Draw_0: da += 0.5 once per 30-FPS game tick.
    boot.da += dt * 15.0;

    if boot.rendered_mode != boot.mode as i8 {
        boot.rendered_mode = boot.mode as i8;
    }

    if boot.mode == 0
        && let Some(icon) = boot.icon
        && let Ok(mut spr) = sprites.get_mut(icon)
    {
        // sprSaving animates: da += 0.5 per tick.
        let m = meta_of(&catalog, "images/sprSaving.png");
        let (fw, fh) = (m[1].max(1.0), m[2].max(1.0));
        let n = sprite_frame_count(&catalog, "images/sprSaving.png").max(1);
        let frame = (boot.da.floor() as usize) % n;
        spr.rect = Some(Rect::new(
            frame as f32 * fw,
            0.0,
            (frame + 1) as f32 * fw,
            fh,
        ));
    } else if boot.mode == 2
        && boot.vlambeer.len() > 1
        && let Some((_, map)) = view_setup(&windows, &cam_q)
    {
        // Re-jitter the ten glow copies every frame (orandom(4)); the main
        // card itself never advances frames.
        let m = meta_of(&catalog, "images/sprVlambeer.png");
        let fw = m[1].max(1.0);
        let fh = m[2].max(1.0);
        let ox = m[4];
        let oy = m[5];
        let px = ((GUI_W - fw) * 0.5).floor() + ox;
        let py = (GUI_H - fh) + oy;
        for e in boot.vlambeer.iter().copied().skip(1) {
            if let Ok(mut tf) = transforms.get_mut(e) {
                let jx = rand::rng().random_range(-4..=4) as f32;
                let jy = rand::rng().random_range(-4..=4) as f32;
                let left = (px + jx) - ox;
                let top = (py + jy) - oy;
                let c = map.to_world(left + fw * 0.5, top + fh * 0.5);
                tf.translation = c.extend(-889.0);
            }
        }
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
            .init_resource::<GenContUi>()
            .add_systems(
                OnEnter(AppState::Splash),
                (
                    reset_camera_view,
                    set_splash_camera_clear,
                    reset_boot,
                    spawn_boot_clear,
                )
                    .chain(),
            )
            .add_systems(
                OnEnter(AppState::MainMenu),
                (reset_camera_view, spawn_spiral_field).chain(),
            )
            .add_systems(
                OnExit(AppState::Splash),
                (despawn_boot_art, despawn_splash_loop, restore_camera_clear),
            )
            .add_systems(
                OnEnter(AppState::Title),
                (
                    reset_camera_view,
                    set_title_camera_clear,
                    // PlayButton/Other_10 order: SpiralCont dies, THEN MenuGen/Menu exist.
                    crate::game::vortex::teardown_vortex,
                    spawn_char_select,
                )
                    .chain(),
            )
            .add_systems(
                OnExit(AppState::Title),
                (
                    despawn_title_art,
                    despawn_hud_art,
                    despawn_campfire_amb,
                    restore_camera_clear,
                )
                    .chain(),
            )
            .add_systems(Update, main_menu_hover)
            .add_systems(Update, boot_intro)
            .add_systems(
                Update,
                (respawn_title_screen_ui_on_layout_change, char_select_tick)
                    .chain()
                    .run_if(in_state(AppState::Title)),
            )
            .add_systems(Update, hide_title_during_transition)
            .add_systems(OnEnter(AppState::InGame), spawn_hud_art)
            .add_systems(OnExit(AppState::InGame), despawn_hud_art)
            .add_systems(OnExit(AppState::InGame), despawn_mutation_art)
            .add_systems(FixedUpdate, sync_hud_art)
            .add_systems(Update, sync_mutation_icons)
            .add_systems(Update, sync_gencont_art);
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

// Title: rotating spiral field + logo - now rendered by game::vortex (WGSL).
// SpiralCtl lives in crate::game::vortex; ensure_vortex_quad/vortex_tick run
// from VortexPlugin. despawn_title_art below still tears the resource down.

fn despawn_title_art(
    mut commands: Commands,
    q: Query<Entity, With<TitleArt>>,
    art: Option<Res<CharSelectArt>>,
    ctl: Option<Res<crate::game::vortex::SpiralCtl>>,
) {
    for e in &q {
        commands.entity(e).try_despawn();
    }
    if art.is_some() {
        commands.remove_resource::<CharSelectArt>();
    }
    if ctl.is_some() {
        commands.remove_resource::<crate::game::vortex::SpiralCtl>();
    }
}

// Char select (nt-rewrite objects: Menu/Create_0, CharSelect, GoButton)

/// Live handles for the title char-select art.
#[derive(Resource, Default)]
struct CharSelectArt {
    /// (pod entity, race id, gui x) - one per `CharSelect` instance.
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
    /// CampChar mutants around the fire.
    chars: Vec<Entity>,
    /// Last selection_epoch we reacted to.
    last_selection_epoch: u32,
    /// view lerp state in NT pixels relative to campfire (0,0)=fire centre.
    view_x: f32,
    view_y: f32,
    /// pixel scale used when spawning world art (map.s at spawn).
    world_s: f32,
    campfire_entity: Option<Entity>,
    /// legacy anim kept for compat (unused after phase machine)
    char_anim: f32,
    /// Right-side loadout art (scrMenuDrawLoadout).
    arrow: Option<Entity>,
    loadout_splat: Option<Entity>,
    crown_icon: Option<Entity>,
    /// (entity, weapon gml id) per slot; swapped on equipment change.
    wep_icons: [Option<(Entity, u8)>; 2],
    /// Open-panel state (Menu.loadout_frame via approach()).
    loadout_anim: f32,
    /// sprLoadoutOpen panel (bottom-right origin).
    open_panel: Option<Entity>,
    /// Open-panel crown grid: (entity, crown id, gui x, gui y, last locked).
    crown_grid: Vec<(Entity, u8, f32, f32, bool)>,
    /// Skin column: (entity, skin idx, last locked). Positions are live -
    /// the column start depends on the selected race's skin count.
    skin_grid: Vec<(Entity, usize, bool)>,
    prev_go_visible: bool,
    /// Layout basis used for the currently spawned screen-space title UI.
    layout_w: f32,
    layout_h: f32,
    layout_scale: f32,
}

fn title_layout_changed(art: &CharSelectArt, window: &Window, scale: f32) -> bool {
    (art.layout_w - window.width()).abs() > f32::EPSILON
        || (art.layout_h - window.height()).abs() > f32::EPSILON
        || (art.layout_scale - scale).abs() > f32::EPSILON
}

fn remember_title_layout(art: &mut CharSelectArt, window: &Window, scale: f32) {
    art.layout_w = window.width();
    art.layout_h = window.height();
    art.layout_scale = scale;
}

const GO_W: f32 = 31.0;
const GO_H: f32 = 19.0;
/// sprGoButtonSymbolic yorigin (from anims.json / GoButton.yy).
const GO_YORIGIN: f32 = -2.0;

#[allow(clippy::type_complexity)]
fn spawn_char_select_world(
    commands: &mut Commands,
    catalog: &AssetCatalog,
    asset_server: &AssetServer,
    save: &SaveData,
    map: &GuiMap,
    art: &mut CharSelectArt,
    selected: &crate::game::SelectedCharacter,
    cam_tf_q: &mut Query<(&mut Transform, Option<&mut CameraBase>), With<Camera2d>>,
) {
    // World-space campfire level (MenuGen): floors/walls/decals are WORLD objects
    // (not ChildOf(cam)) so camera focus pans. Background is camera clear #6a7aaf.
    {
        let s = map.s;
        art.world_s = s;
        art.view_x = 0.0;
        art.view_y = 0.0;
        let title_floor: HashSet<(i32, i32)> = menu_gen_floor_cells();

        for &(cx, cy) in &title_floor {
            let nt_x = cx as f32 * 32.0 - 64.0;
            let nt_y = cy as f32 * 32.0 - 64.0;
            spawn_world_sprite(
                commands,
                catalog,
                asset_server,
                "images/sprFloor0.png",
                campfire_floor_frame(catalog),
                nt_x,
                nt_y,
                s,
                -890.0,
                false,
            );
        }

        for &(cx, cy) in &title_floor {
            if rand::random::<f32>() * 6.0 >= 1.0 {
                continue;
            }
            let nt_x = cx as f32 * 32.0 + 16.0 - 64.0;
            let nt_y = cy as f32 * 32.0 + 16.0 - 64.0;
            if rand::rng().random_range(0..=21) != 0 {
                let cactus_path: &'static str = match pick_i32(&[1, 2, 3]) {
                    1 => "images/sprNightCactus.png",
                    2 => "images/sprNightCactus2.png",
                    _ => "images/sprNightCactus3.png",
                };
                if catalog.has(cactus_path) {
                    spawn_world_sprite(
                        commands,
                        catalog,
                        asset_server,
                        cactus_path,
                        0,
                        nt_x,
                        nt_y,
                        s,
                        -865.0,
                        false,
                    );
                }
            } else if catalog.has("images/sprNightDesertTopDecal.png") {
                let frame = rand::rng().random_range(
                    0..sprite_frame_count(catalog, "images/sprNightDesertTopDecal.png"),
                );
                spawn_world_sprite(
                    commands,
                    catalog,
                    asset_server,
                    "images/sprNightDesertTopDecal.png",
                    frame,
                    nt_x,
                    nt_y,
                    s,
                    -864.0,
                    false,
                );
            }
        }

        let mut wall_seen: HashSet<(i32, i32)> = HashSet::new();
        let probes = [
            (-1, -1),
            (0, -1),
            (1, -1),
            (2, -1),
            (2, 0),
            (2, 1),
            (-1, 0),
            (-1, 1),
            (-1, 2),
            (0, 2),
            (1, 2),
            (2, 2),
        ];
        for &(cx, cy) in &title_floor {
            for (ox, oy) in probes {
                let wx = cx * 2 + ox;
                let wy = cy * 2 + oy;
                let owner = (wx.div_euclid(2), wy.div_euclid(2));
                if title_floor.contains(&owner) || !wall_seen.insert((wx, wy)) {
                    continue;
                }
                let nt_x = wx as f32 * 16.0 - 64.0;
                let nt_y = wy as f32 * 16.0 - 64.0;
                let floor_below = title_floor.contains(&title_floor_owner_below(wx, wy));
                if catalog.has("images/sprWall0Out.png") {
                    spawn_world_sprite(
                        commands,
                        catalog,
                        asset_server,
                        "images/sprWall0Out.png",
                        campfire_wall_out_frame(catalog),
                        nt_x,
                        nt_y,
                        s,
                        -889.5,
                        false,
                    );
                }
                if floor_below && catalog.has("images/sprWall0Bot.png") {
                    spawn_world_sprite(
                        commands,
                        catalog,
                        asset_server,
                        "images/sprWall0Bot.png",
                        campfire_wall_body_frame(catalog),
                        nt_x,
                        nt_y,
                        s,
                        -889.0,
                        false,
                    );
                }
                if catalog.has("images/sprWall0Top.png") {
                    // Top piece is 8px up (gy - 8).
                    spawn_world_sprite(
                        commands,
                        catalog,
                        asset_server,
                        "images/sprWall0Top.png",
                        campfire_wall_top_frame(catalog),
                        nt_x,
                        nt_y - 8.0,
                        s,
                        -888.0,
                        false,
                    );
                }
            }
        }
    }

    // Campfire scene (scrCampfireMenuCreate): WORLD objects at NT offsets from fire (0,0).
    {
        let s = art.world_s.max(0.001);
        let camp = spawn_world_sprite(
            commands,
            catalog,
            asset_server,
            "images/sprCampfire.png",
            0,
            0.0,
            0.0,
            s,
            -872.0,
            rand::random::<bool>(),
        );
        art.campfire = Some(camp);
        art.campfire_entity = Some(camp);

        let log = spawn_world_sprite(
            commands,
            catalog,
            asset_server,
            "images/sprLogMenu.png",
            0,
            0.0,
            -32.0,
            s,
            -884.0,
            false,
        );
        art.log = Some(log);

        use crate::game::content::RaceId;
        let fixed: [(RaceId, f32, f32); 4] = [
            (RaceId::Fish, 0.0, -32.0),
            (RaceId::Crystal, 0.0, 32.0),
            (RaceId::Eyes, 40.0, 0.0),
            (RaceId::Melting, -40.0, 0.0),
        ];
        let others = [
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
        ];

        // collect existing offsets for simple distance rejection
        let mut placed_offsets: Vec<Vec2> = Vec::new();
        let mut char_anchors: Vec<(usize, Vec2)> = Vec::new();
        for (race, dx, dy) in fixed.iter().copied() {
            if !save.race_unlocked(race) {
                continue;
            }
            let Some((slct, to, menu, from)) = camp_char_sprite_set(catalog, race) else {
                continue;
            };
            let e = spawn_world_sprite(
                commands,
                catalog,
                asset_server,
                slct,
                0,
                dx,
                dy,
                s,
                -866.0,
                rand::random::<bool>(),
            );
            let m = meta_of(catalog, slct);
            commands.entity(e).insert(CampCharArt {
                race: race as usize,
                offset: Vec2::new(dx, dy),
                path_slct: slct,
                path_to: to,
                path_menu: menu,
                path_from: from,
                current: CampCharPhase::Slct,
                anim: 0.0,
                frames: m[0].max(1.0) as usize,
                fw: m[1].max(1.0),
                fh: m[2].max(1.0),
                s,
            });
            art.chars.push(e);
            placed_offsets.push(Vec2::new(dx, dy));
            char_anchors.push((race as usize, Vec2::new(dx, dy)));
        }
        for race in others {
            if !save.race_unlocked(race) {
                continue;
            }
            let Some((slct, to, menu, from)) = camp_char_sprite_set(catalog, race) else {
                continue;
            };
            // random distance like upstream: 32+rand*32 + rand*64*rand
            let r1: f32 = rand::random();
            let r2: f32 = rand::random();
            let r3: f32 = rand::random();
            let dist = 32.0 + r1 * 32.0 + r2 * 64.0 * r3;
            let ang = rand::random::<f32>() * std::f32::consts::TAU;
            let dx = ang.cos() * dist;
            let dy = ang.sin() * dist;
            // reject if too close to another char (<32)
            let mut too_close = false;
            for p in &placed_offsets {
                if (*p - Vec2::new(dx, dy)).length() < 32.0 {
                    too_close = true;
                    break;
                }
            }
            if too_close {
                continue;
            }
            let e = spawn_world_sprite(
                commands,
                catalog,
                asset_server,
                slct,
                0,
                dx,
                dy,
                s,
                -866.0,
                rand::random::<bool>(),
            );
            let m = meta_of(catalog, slct);
            commands.entity(e).insert(CampCharArt {
                race: race as usize,
                offset: Vec2::new(dx, dy),
                path_slct: slct,
                path_to: to,
                path_menu: menu,
                path_from: from,
                current: CampCharPhase::Slct,
                anim: 0.0,
                frames: m[0].max(1.0) as usize,
                fw: m[1].max(1.0),
                fh: m[2].max(1.0),
                s,
            });
            art.chars.push(e);
            placed_offsets.push(Vec2::new(dx, dy));
            char_anchors.push((race as usize, Vec2::new(dx, dy)));

            // Chicken TV
            if race == RaceId::Chicken {
                let tv_path = if catalog.has("images/sprTV.png") {
                    "images/sprTV.png"
                } else if catalog.has("images/sprTV.gif") {
                    "images/sprTV.gif"
                } else {
                    ""
                };
                if !tv_path.is_empty() {
                    let jx = (rand::random::<f32>() - 0.5) * 4.0;
                    let jy = (rand::random::<f32>() - 0.5) * 8.0 - 32.0;
                    spawn_world_sprite(
                        commands,
                        catalog,
                        asset_server,
                        tv_path,
                        0,
                        dx + jx,
                        dy + jy,
                        s,
                        -865.0,
                        false,
                    );
                }
            }
        }
        // Menu/Create_0: if char[race] exists, snap view onto it immediately.
        {
            let race_id = selected.0 as usize;
            if race_id != 0 {
                if let Some((_, off)) = char_anchors.iter().find(|(r, _)| *r == race_id) {
                    art.view_x = off.x;
                    art.view_y = off.y;
                } else {
                    art.view_x = 0.0;
                    art.view_y = 0.0;
                }
            } else {
                art.view_x = 0.0;
                art.view_y = 0.0;
            }
            // Apply immediate snap to live camera (OnEnter runs before first tick).
            let s = art.world_s.max(0.001);
            for (mut tf, base) in cam_tf_q.iter_mut() {
                tf.translation.x = art.view_x * s;
                tf.translation.y = -art.view_y * s;
                if let Some(mut b) = base {
                    b.translation.x = art.view_x * s;
                    b.translation.y = -art.view_y * s;
                }
            }
        }
        // Campfire ambience (Menu/Create_0 MusCont amb = amb0).
        // Try common stems; resolve_audio_path handles stem substring fallback.
        for stem in ["amb0", "sndCampfire", "ambCampfire", "sndCampfireLoop"] {
            if catalog.resolve_audio_path(stem).is_some() {
                play_loop(commands, catalog, asset_server, stem, 0.55, CampfireAmb);
                break;
            }
        }
    }
}

fn spawn_char_select_screen_ui(
    commands: &mut Commands,
    catalog: &AssetCatalog,
    asset_server: &AssetServer,
    save: &SaveData,
    cam: Entity,
    map: &GuiMap,
    art: &mut CharSelectArt,
) {
    let count = CHAR_SELECT_RACES.len();
    let step = slot_step(count);
    let ystart = slot_ystart();

    for (i, race) in CHAR_SELECT_RACES.iter().enumerate() {
        let race_id = *race as usize;
        let x = slot_x(i, step);

        let unlocked = save.race_unlocked(*race);
        let sprite_path = if unlocked {
            "images/sprCharSelect.png"
        } else {
            "images/sprCharSelectLocked.png"
        };

        // CharSelect/Draw_0:
        // draw_sprite_ext(can ? sprite_index : sprCharSelectLocked,
        //                 race, x, y, 1, 1, 0, color, 1)
        let (pod_spr, pod_tf) = gm_sprite(
            catalog,
            asset_server,
            map,
            sprite_path,
            race_id,
            x,
            ystart,
            1.0,
            1.0,
            C_GRAY,
            -860.0,
        );
        let pod = commands
            .spawn((TitleArt, TitleScreenUiArt, ChildOf(cam), pod_spr, pod_tf))
            .id();
        art.pods.push((pod, race_id, x));
    }

    {
        const LB_STRIP_Z: f32 = -864.0;
        const LB_FRAME: usize = 3;
        let lb_h = meta_of(catalog, "images/sprLetterbox.png")[2].max(1.0);
        let yscale = LETTERBOX_SIZE / (lb_h - 9.0);
        let bh = lb_h * yscale;
        let (spr, tf) = gm_sprite(
            catalog,
            asset_server,
            map,
            "images/sprLetterbox.png",
            LB_FRAME,
            0.0,
            -1.0,
            1.0,
            yscale,
            Color::WHITE,
            LB_STRIP_Z,
        );
        art.letterbox.push(
            commands
                .spawn((TitleArt, TitleScreenUiArt, ChildOf(cam), spr, tf))
                .id(),
        );

        let (mut spr, tf) = gm_sprite(
            catalog,
            asset_server,
            map,
            "images/sprLetterbox.png",
            LB_FRAME,
            0.0,
            GUI_H + 2.0 - bh,
            1.0,
            yscale,
            Color::WHITE,
            LB_STRIP_Z,
        );
        spr.flip_x = true;
        spr.flip_y = true;
        art.letterbox.push(
            commands
                .spawn((TitleArt, TitleScreenUiArt, ChildOf(cam), spr, tf))
                .id(),
        );
    }

    // Char splat sits on the bottom letterbox (scrCampfireMenuDrawRacePortrait,
    // fa_left/fa_bottom): draw point (0, 205), origin (0, 64). Native size -
    // GameMaker never scales it.
    {
        let (spr, tf) = gm_sprite(
            catalog,
            asset_server,
            map,
            "images/sprCharSplat.png",
            0,
            0.0,
            GUI_H - LETTERBOX_SIZE + 1.0,
            1.0,
            1.0,
            Color::WHITE,
            -855.0,
        );
        art.splat = Some(
            commands
                .spawn((TitleArt, TitleScreenUiArt, ChildOf(cam), spr, tf))
                .id(),
        );
    }

    // Big portrait (sprCampfireMenuDrawRacePortrait, fa_left): draw point
    // (16, 240). Subimages are the per-race skin portraits; frame = race id.
    // Hidden until a non-random pick.
    {
        let (spr, tf) = gm_sprite(
            catalog,
            asset_server,
            map,
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
                .spawn((
                    TitleArt,
                    TitleScreenUiArt,
                    ChildOf(cam),
                    Visibility::Hidden,
                    spr,
                    tf,
                ))
                .id(),
        );
    }

    // Big name plate (frame = race id), draw point (0, 137). Hidden until a
    // non-random pick.
    {
        let (spr, tf) = gm_sprite(
            catalog,
            asset_server,
            map,
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
                .spawn((
                    TitleArt,
                    TitleScreenUiArt,
                    ChildOf(cam),
                    Visibility::Hidden,
                    spr,
                    tf,
                ))
                .id(),
        );
    }

    // Right-side loadout art (scrMenuDrawLoadout, closed state): splat pinned
    // to the right edge, arrow above it, current crown and both weapons.
    {
        let (spr, tf) = gm_sprite(
            catalog,
            asset_server,
            map,
            "images/sprLoadoutSplat.png",
            0,
            GUI_W + 2.0,
            GUI_H - LETTERBOX_SIZE + 2.0,
            1.0,
            1.0,
            Color::WHITE,
            -853.0,
        );
        art.loadout_splat = Some(
            commands
                .spawn((TitleArt, TitleScreenUiArt, ChildOf(cam), spr, tf))
                .id(),
        );

        let (spr, tf) = gm_sprite(
            catalog,
            asset_server,
            map,
            "images/sprLoadoutArrow.png",
            0,
            GUI_W + 2.0 - 16.0,
            GUI_H - LETTERBOX_SIZE + 2.0 - 16.0,
            1.0,
            1.0,
            C_UIGRAY,
            -847.0,
        );
        art.arrow = Some(
            commands
                .spawn((TitleArt, TitleScreenUiArt, ChildOf(cam), spr, tf))
                .id(),
        );

        let (spr, tf) = gm_sprite(
            catalog,
            asset_server,
            map,
            "images/sprLoadoutCrown.png",
            0,
            GUI_W + 2.0 - 60.0,
            GUI_H - LETTERBOX_SIZE + 2.0 - 40.0,
            1.0,
            1.0,
            Color::WHITE,
            -852.0,
        );
        art.crown_icon = Some(
            commands
                .spawn((TitleArt, TitleScreenUiArt, ChildOf(cam), spr, tf))
                .id(),
        );

        for slot in 0..2usize {
            let wx = GUI_W + 2.0 - 60.0 + if slot == 0 { -8.0 } else { 16.0 };
            let wy = GUI_H - LETTERBOX_SIZE + 2.0 - 15.0;
            let (spr, tf) = gm_loadout_weapon(
                catalog,
                asset_server,
                map,
                WeaponId::REVOLVER,
                wx,
                wy,
                if slot == 0 {
                    Color::WHITE
                } else {
                    Color::srgb_u8(192, 192, 192)
                },
                -846.0,
            );
            let e = if slot == 1 {
                commands
                    .spawn((
                        TitleArt,
                        TitleScreenUiArt,
                        ChildOf(cam),
                        spr,
                        tf,
                        Visibility::Hidden,
                    ))
                    .id()
            } else {
                commands
                    .spawn((TitleArt, TitleScreenUiArt, ChildOf(cam), spr, tf))
                    .id()
            };
            art.wep_icons[slot] = Some((e, if slot == 0 { WeaponId::REVOLVER.0 } else { 0 }));
        }

        // Open panel (sprLoadoutOpen, bottom-right origin) + the crown grid
        // layout from scrMenuDrawLoadout: start (248,48), step 28, wrap at
        // the right edge back to x=220.
        // Scale per upstream: _xscale = max(1, (_w - _skins_x)/200) - with
        // _skins_x = _w-136 this is always 1; _yscale = (_splat_y-36)/168+0.05.
        let (spr, tf) = gm_sprite(
            catalog,
            asset_server,
            map,
            "images/sprLoadoutOpen.png",
            0,
            GUI_W,
            GUI_H - LETTERBOX_SIZE + 2.0,
            ((GUI_W - 184.0) / (256.0 - 56.0)).max(1.0),
            (GUI_H - LETTERBOX_SIZE + 2.0 - LETTERBOX_SIZE) / 168.0 + 0.05,
            Color::WHITE,
            -849.0,
        );
        art.open_panel = Some(
            commands
                .spawn((
                    TitleArt,
                    TitleScreenUiArt,
                    ChildOf(cam),
                    Visibility::Hidden,
                    spr,
                    tf,
                ))
                .id(),
        );

        // Crown grid at the exact scrMenuDrawLoadout slots; lock state is
        // corrected on the first char_select_tick pass.
        for (crown_id, gx, gy) in crown_slot_positions() {
            let (spr, tf) = gm_sprite(
                catalog,
                asset_server,
                map,
                "images/sprLoadoutCrown.png",
                crown_id as usize,
                gx,
                gy,
                1.0,
                1.0,
                C_UIGRAY,
                -848.0,
            );
            art.crown_grid.push((
                commands
                    .spawn((TitleArt, TitleScreenUiArt, ChildOf(cam), spr, tf))
                    .id(),
                crown_id,
                gx,
                gy,
                false,
            ));
        }
        // Skins (left side of loadout panel) - up to 4 slots; exact y for
        // the live race count is applied every tick (scrMenuDrawLoadout).
        for (idx, _gx, _gy) in skin_slot_positions(4) {
            let (spr, tf) = gm_sprite(
                catalog,
                asset_server,
                map,
                "images/sprLoadoutSkin.png",
                0,
                _gx,
                _gy,
                1.0,
                1.0,
                C_UIGRAY,
                -848.0,
            );
            art.skin_grid.push((
                commands
                    .spawn((TitleArt, TitleScreenUiArt, ChildOf(cam), spr, tf))
                    .id(),
                idx,
                false,
            ));
        }
    }

    // Menu/Create_0 spawns GoButton right of the last slot, hidden.
    let (gx, gy) = go_button_pos(step, count);
    let (go_spr, go_tf) = gm_sprite(
        catalog,
        asset_server,
        map,
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
        .spawn((
            TitleArt,
            TitleScreenUiArt,
            ChildOf(cam),
            Visibility::Hidden,
            go_spr,
            go_tf,
        ))
        .id();
    art.go_button = Some((go, gx, gy));
    art.addy = 1.0;
}

#[allow(clippy::type_complexity)]
fn spawn_char_select(
    mut commands: Commands,
    catalog: Res<AssetCatalog>,
    asset_server: Res<AssetServer>,
    save: Res<SaveData>,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    cam_proj_q: Query<(Entity, &Projection), With<Camera2d>>,
    selected: Res<crate::game::SelectedCharacter>,
    mut cam_tf_q: Query<(&mut Transform, Option<&mut CameraBase>), With<Camera2d>>,
) {
    let Some((cam, map, window, scale)) = ({
        let Ok(win) = windows.single() else {
            return;
        };
        let Ok((cam_ent, proj)) = cam_proj_q.single() else {
            return;
        };
        let scale = match proj {
            Projection::Orthographic(o) => o.scale,
            _ => 1.0,
        };
        Some((
            cam_ent,
            gui_map(win.width(), win.height(), scale),
            win,
            scale,
        ))
    }) else {
        return;
    };

    let mut art = CharSelectArt::default();
    spawn_char_select_world(
        &mut commands,
        &catalog,
        &asset_server,
        &save,
        &map,
        &mut art,
        &selected,
        &mut cam_tf_q,
    );
    spawn_char_select_screen_ui(
        &mut commands,
        &catalog,
        &asset_server,
        &save,
        cam,
        &map,
        &mut art,
    );
    remember_title_layout(&mut art, window, scale);
    commands.insert_resource(art);
}

#[allow(clippy::type_complexity)]
fn respawn_title_screen_ui_on_layout_change(
    mut commands: Commands,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    cam_q: Query<(Entity, &Transform, &Projection), With<Camera2d>>,
    q_ui: Query<Entity, With<TitleScreenUiArt>>,
    mut art: ResMut<CharSelectArt>,
    save: Res<SaveData>,
    catalog: Res<AssetCatalog>,
    asset_server: Res<AssetServer>,
) {
    let Ok(window) = windows.single() else {
        return;
    };

    let Some((cam, map)) = view_setup(&windows, &cam_q) else {
        return;
    };

    let scale = match cam_q.single().ok().map(|(_, _, p)| p) {
        Some(Projection::Orthographic(o)) => o.scale,
        _ => return,
    };

    if !title_layout_changed(&art, window, scale) {
        return;
    }

    for e in &q_ui {
        commands.entity(e).try_despawn();
    }

    // Preserve runtime animation/state fields.
    let addy = art.addy;
    let go_anim = art.go_anim;
    let splat_anim = art.splat_anim;
    let loadout_anim = art.loadout_anim;
    let last_selection_epoch = art.last_selection_epoch;

    // Clear only screen-space handles; keep world/campfire state untouched.
    art.pods.clear();
    art.go_button = None;
    art.letterbox.clear();
    art.splat = None;
    art.big_portrait = None;
    art.big_name = None;
    art.arrow = None;
    art.loadout_splat = None;
    art.crown_icon = None;
    art.wep_icons = [None, None];
    art.open_panel = None;
    art.crown_grid.clear();
    art.skin_grid.clear();

    spawn_char_select_screen_ui(
        &mut commands,
        &catalog,
        &asset_server,
        &save,
        cam,
        &map,
        &mut art,
    );

    art.addy = addy;
    art.go_anim = go_anim;
    art.splat_anim = splat_anim;
    art.loadout_anim = loadout_anim;
    art.last_selection_epoch = last_selection_epoch;

    remember_title_layout(&mut art, window, scale);
}

fn hide_title_during_transition(
    transition: Res<Transition<AppState>>,
    art: Option<Res<CharSelectArt>>,
    mut vis_q: Query<&mut Visibility>,
) {
    let Some(art) = art else {
        return;
    };
    // Hide fish/camp chars and the select-bar pods while any transition is
    // covering/uncovering. Repose fade and vortex both drive
    // `Transition.overlay_alpha` / `phase`; when active the title screen
    // should be behind the transition, not over it.
    let hide = transition.active
        || transition.phase != game_utils_bevy::transitions::TransitionPhase::Idle;
    for (entity, _, _) in &art.pods {
        if let Ok(mut vis) = vis_q.get_mut(*entity) {
            *vis = if hide {
                Visibility::Hidden
            } else {
                Visibility::Visible
            };
        }
    }
    for entity in &art.chars {
        if let Ok(mut vis) = vis_q.get_mut(*entity) {
            *vis = if hide {
                Visibility::Hidden
            } else {
                Visibility::Visible
            };
        }
    }
    // Also hide campfire/log which are part of the world scene
    for opt in [art.campfire, art.campfire_entity, art.log] {
        if let Some(e) = opt {
            if let Ok(mut vis) = vis_q.get_mut(e) {
                *vis = if hide {
                    Visibility::Hidden
                } else {
                    Visibility::Visible
                };
            }
        }
    }
    for e in &art.letterbox {
        if let Ok(mut vis) = vis_q.get_mut(*e) {
            *vis = if hide {
                Visibility::Hidden
            } else {
                Visibility::Visible
            };
        }
    }
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
    // Settings overlay is modal - don't leak hover lift/sndHover to the buttons behind it (GML MenuOptions blocks)
    if ui.overlay != crate::app::OverlayMenu::None {
        if ui.main_menu_hover != -1 {
            ui.main_menu_hover = -1;
        }
        return;
    }

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
            && let Some(path) = resolve_audio_path(&catalog, "sndHover")
        {
            commands.spawn((
                AudioPlayer::<AudioSource>::new(asset_server.load(path)),
                PlaybackSettings {
                    mode: PlaybackMode::Despawn,
                    volume: Volume::Linear(0.45),
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
    mut cam_tf: Query<
        (&mut Transform, Option<&mut CameraBase>),
        (With<Camera2d>, Without<CampCharArt>),
    >,
    mut camp_chars: Query<
        (&mut CampCharArt, &mut Sprite, &mut Transform),
        (With<CampCharArt>, Without<Camera2d>),
    >,
    mut art: ResMut<CharSelectArt>,
    bridge: Res<UiBridge>,
    save: Res<SaveData>,
    catalog: Res<AssetCatalog>,
    asset_server: Res<AssetServer>,
    mut sprites: Query<&mut Sprite, Without<CampCharArt>>,
    mut transforms: Query<&mut Transform, (Without<CampCharArt>, Without<Camera2d>)>,
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
    // Modal overlay blocks char-select hover (GML MenuOptions is modal)
    if ui.overlay != crate::app::OverlayMenu::None {
        if ui.title_hover_race != -1 {
            ui.title_hover_race = -1;
        }
        return;
    }
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
        let this_race = selected_race == *race_id;
        let unlocked = crate::game::content::race_from_gml_id(*race_id)
            .map(|r| save.race_unlocked(r))
            .unwrap_or(true);

        // CharSelect/Draw_0:
        // _color = (can && selected) ? c_white : c_gray
        // selected is driven by (_pointed || _this_race)
        sprite.color = if unlocked && (pointed || this_race) {
            Color::WHITE
        } else {
            C_GRAY
        };
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
        if let Ok(mut tf) = transforms.get_mut(e) {
            let m = meta_of(&catalog, "images/sprBigPortrait.png");
            let (fw, fh, xorigin, yorigin) = (m[1].max(1.0), m[2].max(1.0), m[4], m[5]);
            let draw_x = 16.0 - ui.portrait_offset;
            let draw_y = GUI_H;
            let left = draw_x - xorigin;
            let top = draw_y - yorigin;
            let center = map.to_world(left + fw * 0.5, top + fh * 0.5);
            tf.translation.x = center.x;
            tf.translation.y = center.y;
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

    // --- Menu/Step_1 camera focus (view lerp) ---
    {
        let s = art.world_s.max(0.001);
        let target = if selected_race == 0 {
            Vec2::ZERO
        } else {
            let mut t = Vec2::ZERO;
            let mut found = false;
            for (cc, _, _) in camp_chars.iter() {
                if cc.race == selected_race {
                    t = cc.offset;
                    found = true;
                    break;
                }
            }
            if found { t } else { Vec2::ZERO }
        };
        let k = 1.0 - (0.9_f32).powf(time.delta_secs() * 30.0);
        art.view_x += (target.x - art.view_x) * k;
        art.view_y += (target.y - art.view_y) * k;
        if let Ok((mut tf, base)) = cam_tf.single_mut() {
            tf.translation.x = art.view_x * s;
            tf.translation.y = -art.view_y * s;
            if let Some(mut b) = base {
                b.translation.x = tf.translation.x;
                b.translation.y = tf.translation.y;
            }
        }
    }

    // portrait slide + text appear approach
    if ui.portrait_offset > 0.0 {
        ui.portrait_offset = (ui.portrait_offset - 12.0 * 30.0 * time.delta_secs()).max(0.0);
    }
    if ui.text_appear > 0.0 {
        ui.text_appear = (ui.text_appear - 30.0 * time.delta_secs()).max(0.0);
    }

    // CampChar selection epoch -> phase kicks
    if ui.selection_epoch != art.last_selection_epoch {
        art.last_selection_epoch = ui.selection_epoch;
        art.splat_anim = 0.0;
        for (mut cc, mut spr, _) in camp_chars.iter_mut() {
            let want_sel = cc.race == selected_race && selected_race != 0;
            if want_sel {
                cc.current = CampCharPhase::To;
                apply_camp_char_path(&catalog, &asset_server, &mut spr, cc.path_to, &mut cc);
            } else if cc.current == CampCharPhase::Menu || cc.current == CampCharPhase::To {
                cc.current = CampCharPhase::From;
                apply_camp_char_path(&catalog, &asset_server, &mut spr, cc.path_from, &mut cc);
            }
            cc.anim = 0.0;
        }
    }

    // Campfire scene animation: fire at 12 fps (image_speed 0.4 @ 30 tps)
    art.campfire_anim = (art.campfire_anim + 12.0 * time.delta_secs()) % 4.0;
    if let Some(e) = art.campfire
        && let Ok(mut spr) = sprites.get_mut(e)
    {
        let (fw, fh) = (52.0, 52.0);
        let f = art.campfire_anim.floor().min(3.0);
        spr.rect = Some(Rect::new(f * fw, 0.0, (f + 1.0) * fw, fh));
    }
    // CampChar phase machine @ image_speed 0.4 *30 =12 fps
    let dt_frames = 12.0 * time.delta_secs();
    for (mut cc, mut spr, _) in camp_chars.iter_mut() {
        let focused = cc.race == selected_race && selected_race != 0;
        cc.anim += dt_frames;
        let frames = cc.frames.max(1);
        if cc.anim >= frames as f32 {
            cc.anim = 0.0;
            match cc.current {
                CampCharPhase::To if focused => {
                    cc.current = CampCharPhase::Menu;
                    apply_camp_char_path(&catalog, &asset_server, &mut spr, cc.path_menu, &mut cc);
                }
                CampCharPhase::From if !focused => {
                    cc.current = CampCharPhase::Slct;
                    apply_camp_char_path(&catalog, &asset_server, &mut spr, cc.path_slct, &mut cc);
                }
                CampCharPhase::Menu if !focused => {
                    cc.current = CampCharPhase::From;
                    apply_camp_char_path(&catalog, &asset_server, &mut spr, cc.path_from, &mut cc);
                }
                CampCharPhase::Slct if focused => {
                    cc.current = CampCharPhase::To;
                    apply_camp_char_path(&catalog, &asset_server, &mut spr, cc.path_to, &mut cc);
                }
                CampCharPhase::To if !focused => {
                    cc.current = CampCharPhase::From;
                    apply_camp_char_path(&catalog, &asset_server, &mut spr, cc.path_from, &mut cc);
                }
                CampCharPhase::From if focused => {
                    cc.current = CampCharPhase::To;
                    apply_camp_char_path(&catalog, &asset_server, &mut spr, cc.path_to, &mut cc);
                }
                _ => {}
            }
        }
        let f = (cc.anim.floor() as usize).min(frames - 1);
        spr.rect = Some(Rect::new(
            f as f32 * cc.fw,
            0.0,
            (f as f32 + 1.0) * cc.fw,
            cc.fh,
        ));
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
    // Crowns grid (scrMenuDrawLoadout #region Crowns): locked crowns use
    // sprLockedLoadoutCrown; tint white only when unlocked AND (pointed or
    // currently selected); pointed entries lift 1 px. RANDOM hides until the
    // race has any crown above NONE unlocked.
    let crown_race = crate::game::content::race_from_gml_id(selected_race)
        .unwrap_or(crate::game::content::RaceId::Random);
    let any_crowns = save.any_crown_unlocked(crown_race);
    for (e, crown_id, gx, gy, last_locked) in art.crown_grid.iter_mut() {
        let unlocked = save.crown_unlocked(crown_race, *crown_id);
        let visible = fullview && avail && (*crown_id != 0 || any_crowns);
        if let Ok(mut vis) = visibility.get_mut(*e) {
            *vis = if visible {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };
        }
        if !visible {
            continue;
        }

        // point_in_circle(_mx,_my, _crown_x, _crown_y, _crownsize*0.5)
        let pointed = cursor_gui.is_some_and(|m| (m.x - *gx).hypot(m.y - *gy) <= 14.0);
        let is_selected = *crown_id == ui.crown_id;
        let suspect = unlocked && (pointed || is_selected);

        if *last_locked != !unlocked
            && let Ok(mut spr) = sprites.get_mut(*e)
        {
            let path: &str = if unlocked {
                "images/sprLoadoutCrown.png"
            } else {
                "images/sprLockedLoadoutCrown.png"
            };
            spr.image = asset_server.load(path);
            *last_locked = !unlocked;
        }
        if let Ok(mut spr) = sprites.get_mut(*e) {
            let (fw, fh) = (32.0, 32.0);
            spr.rect = Some(Rect::new(
                (*crown_id as f32) * fw,
                0.0,
                ((*crown_id as f32) + 1.0) * fw,
                fh,
            ));
            spr.color = if suspect { Color::WHITE } else { C_UIGRAY };
        }
        if let Ok(mut tf) = transforms.get_mut(*e) {
            let c = map.to_world(*gx, *gy - i32::from(suspect) as f32);
            tf.translation.x = c.x;
            tf.translation.y = c.y;
        }
    }
    // Skins grid (scrMenuDrawLoadout #region Skins): column start depends on
    // the live skin count; locked skins use sprLoadoutSkinLocked; white only
    // when unlocked AND (selected or pointed); pointed entries lift 1 px.
    let skin_count = if avail {
        max_skin_count(selected_race)
    } else {
        0
    };
    let skin_slots = skin_slot_positions(skin_count);
    for (e, idx, last_locked) in art.skin_grid.iter_mut() {
        let visible = fullview && avail && *idx < skin_count;
        if let Ok(mut vis) = visibility.get_mut(*e) {
            *vis = if visible {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };
        }
        if !visible {
            continue;
        }

        let Some((_, sx, sy)) = skin_slots.iter().find(|(i, _, _)| i == idx) else {
            continue;
        };
        let (sx, sy) = (*sx, *sy);
        let unlocked = save.skin_unlocked(crown_race, *idx as u8);
        // point_in_circle(_mx,_my, _skins_x, _skins_y, 10)
        let pointed = cursor_gui.is_some_and(|m| (m.x - sx).hypot(m.y - sy) <= 10.0);
        let is_selected = ui.selected_skin == *idx as u8;
        let selection = unlocked && (is_selected || pointed);

        if *last_locked != !unlocked
            && let Ok(mut spr) = sprites.get_mut(*e)
        {
            let path: &str = if unlocked {
                "images/sprLoadoutSkin.png"
            } else {
                "images/sprLoadoutSkinLocked.png"
            };
            spr.image = asset_server.load(path);
            *last_locked = !unlocked;
        }
        if let Ok(mut spr) = sprites.get_mut(*e) {
            let sub = race_skin_subimage(selected_race, *idx as u8).max(0) as f32;
            spr.rect = Some(Rect::new(sub * 32.0, 0.0, (sub + 1.0) * 32.0, 32.0));
            spr.color = if selection { Color::WHITE } else { C_UIGRAY };
        }
        if let Ok(mut tf) = transforms.get_mut(*e) {
            let c = map.to_world(sx, sy - i32::from(selection) as f32);
            tf.translation.x = c.x;
            tf.translation.y = c.y;
        }
    }

    if let Some(e) = art.arrow {
        let pointed = cursor_gui.is_some_and(|m| {
            m.x >= GUI_W - 28.0 && m.x <= GUI_W - 4.0 && m.y >= GUI_H - 54.0 && m.y <= GUI_H - 30.0
        });
        if let Ok(mut vis) = visibility.get_mut(e) {
            *vis = if avail {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };
        }
        if let Ok(mut spr) = sprites.get_mut(e) {
            spr.color = if pointed { Color::WHITE } else { C_UIGRAY };
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
        // Weapon id 0 is "no weapon". Never draw an icon for it. Previously
        // slot 0 was always visible, causing id 0 to render using WEAPONS[0]
        // metadata and appear as a bogus gun for some characters.
        // Original scrMenuDrawLoadout skips slot 1 when _weapon == _default_weapon
        // (and we also skip when both slots hold the same gun).
        let is_duplicate_default = slot == 1 && id == race_default_weapon_id(selected_race);
        let is_duplicate_start = slot == 1 && id != 0 && id == ui.start_weapon_id;
        let should_show =
            avail && show_name && id != 0 && !is_duplicate_default && !is_duplicate_start;
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
        if id == 0 {
            if let Ok(mut vis) = visibility.get_mut(e) {
                *vis = Visibility::Hidden;
            }
            continue;
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

        if ui.title_go_visible && !art.prev_go_visible {
            art.addy = 1.0;
        }
        art.prev_go_visible = ui.title_go_visible;
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

// In-game HUD (nt-rewrite scripts/scrDrawPlayerHUD.gml)

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
/// (xoffset, yoffset - 8) sized (weapon_width, 14) - scrDrawPlayerHUD.
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
        0, // was 2; strip is 1 frame post-extract
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
/// weapon icons and outline tints - all from live components.
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
    floor_trans: Option<Res<FloorTransition>>,
) {
    let Some(refs) = refs.as_mut() else {
        return;
    };
    // Hide HUD entirely during GenCont loading (draw_clear black + spiral)
    if floor_trans.is_some_and(|f| f.active) {
        for mut vis in &mut visibilities {
            *vis = Visibility::Hidden;
        }
        return;
    } else {
        for mut vis in &mut visibilities {
            *vis = Visibility::Visible;
        }
    }
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

    // Health fills: 1×8 sprHealthFill stretched to width = 84 * frac.
    // GM draws at (22, 7) with xscale=width, origin (0,0).
    let lst = health.hp.max(0) as f32; // TODO: track lsthealth for lag bar
    let cur = health.hp.max(0) as f32;
    let max = health.max.max(1) as f32;
    let bg_w = (84.0 * (lst / max)).clamp(0.0, 84.0);
    let fg_w = (84.0 * (cur / max)).clamp(0.0, 84.0);

    for (entity, w) in [(refs.hp_bg, bg_w), (refs.hp_fg, fg_w)] {
        if let Ok(mut spr) = sprites.get_mut(entity) {
            // gm_sprite baked xscale into custom_size; rebuild width only.
            spr.custom_size = Some(Vec2::new(w.max(0.001) * map.s, 8.0 * map.s));
        }
        if let Ok(mut tf) = transforms.get_mut(entity) {
            // origin (0,0): center = (22 + w/2, 7 + 4) in GUI
            let center = map.to_world(22.0 + w * 0.5, 7.0 + 4.0);
            tf.translation.x = center.x;
            tf.translation.y = center.y;
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

fn despawn_hud_art(
    mut commands: Commands,
    q: Query<Entity, With<HudArt>>,
    refs: Option<Res<HudArtRefs>>,
) {
    for e in &q {
        commands.entity(e).try_despawn();
    }
    if refs.is_some() {
        commands.remove_resource::<HudArtRefs>();
    }
}

fn despawn_mutation_art(
    mut commands: Commands,
    q: Query<Entity, With<MutationIconArt>>,
    refs: Option<Res<MutationArtRefs>>,
) {
    for e in &q {
        commands.entity(e).try_despawn();
    }
    if refs.is_some() {
        commands.remove_resource::<MutationArtRefs>();
    }
}

/// Sync mutation choice icons (SkillIcon)
fn sync_mutation_icons(
    mut commands: Commands,
    catalog: Res<AssetCatalog>,
    asset_server: Res<AssetServer>,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    cam_q: Query<(Entity, &Transform, &Projection), With<Camera2d>>,
    bridge: Res<crate::menus::UiBridge>,
    existing: Query<Entity, With<MutationIconArt>>,
    mut refs: Option<ResMut<MutationArtRefs>>,
) {
    let (ids, selected) = if let Ok(ui) = bridge.shared.lock() {
        (ui.mutation_choice_ids.clone(), ui.mutation_selected)
    } else {
        (Vec::new(), None)
    };
    let has_pending = !ids.is_empty();
    // Despawn when choice cleared / picked
    if !has_pending {
        if refs.is_some() || !existing.is_empty() {
            for e in &existing {
                commands.entity(e).try_despawn();
            }
            if let Some(mut r) = refs {
                r.entities.clear();
            }
        }
        return;
    }
    // Clear old icons before respawning for new layout/count - respawn each frame so selected tint updates instantly
    for e in &existing {
        commands.entity(e).try_despawn();
    }
    let Some((cam, map)) = view_setup(&windows, &cam_q) else {
        return;
    };
    // Native sprite is sprSkillIcon 24×32. Verify catalog has it; fall back to HUD 16×16.
    let icon_path = if catalog.has("images/sprSkillIcon.png") {
        "images/sprSkillIcon.png"
    } else {
        "images/sprSkillIconHUD.png"
    };
    // Layout: mirrors LevCont/Other_10 – view_width 320, num icons,
    // step = min(32, floor(320/(num+1))) == 32 for 2-4 choices, half 16.
    // x = view_xview_center - (num-1)*half + index*step, y = view_height -21.
    let n = ids.len();
    let step = (320.0 / (n as f32 + 1.0)).floor().min(32.0);
    let half = step * 0.5;
    let start_x = 160.0 - (n as f32 - 1.0) * half;
    let icon_y = GUI_H - 21.0;
    let mut new_entities = Vec::with_capacity(n);
    for (i, &skill_id) in ids.iter().enumerate() {
        let gui_x = start_x + i as f32 * step;
        // skill_id is 1-based (scrSkills), frame 0-based
        let frame = (skill_id as usize).saturating_sub(1) % 30;
        let is_selected = selected == Some(i);
        // GML SkillIcon Draw: selected ? c_white : c_gray (128,128,128)
        let tint = if is_selected {
            Color::WHITE
        } else {
            Color::srgb_u8(128, 128, 128)
        };
        let (spr, tf) = gm_sprite(
            &catalog,
            &asset_server,
            &map,
            icon_path,
            frame,
            gui_x,
            icon_y,
            1.0,
            1.0,
            tint,
            -850.0,
        );
        let e = commands
            .spawn((MutationIconArt, ChildOf(cam), spr, tf))
            .id();
        new_entities.push(e);
    }
    if let Some(mut r) = refs {
        r.entities = new_entities;
    } else {
        commands.insert_resource(MutationArtRefs {
            entities: new_entities,
        });
    }
}

#[derive(Component)]
struct GenContArt;

#[derive(Resource, Default)]
struct GenContUi {
    text_gen: Option<Entity>,
    text_tip: Option<Entity>,
    bar_border: Option<Entity>,
    bar_bg: Option<Entity>,
    bar_fill: Option<Entity>,
}

fn sync_gencont_art(
    mut commands: Commands,
    ft: Res<FloorTransition>,
    mut ui: ResMut<GenContUi>,
    q: Query<Entity, With<GenContArt>>,
) {
    // World-space GenCont now handled by Repose (menus::gen_cont_overlay) for
    // resolution-independent letterboxing. Despawn any leftover world entities
    // to avoid double bar / drift on resize (ChildOf camera offset).
    for e in &q {
        commands.entity(e).try_despawn();
    }
    if ui.text_gen.is_some() || ui.bar_bg.is_some() {
        *ui = GenContUi::default();
    }
    let _ = ft;
}

#[cfg(test)]
mod campfire_ui_tests {
    use super::*;

    /// 1280x720 at base cam scale: s = min(576/320, 324/240) = 1.35.
    #[test]
    fn gui_map_scales_letterboxed_16x9() {
        let m = gui_map(1280.0, 720.0, CAM_SCALE);
        assert!((m.s - 1.35).abs() < 1e-4);
        assert!((m.ox - 72.0).abs() < 1e-3);
        assert!(m.oy.abs() < 1e-3);
    }

    /// scrDrawLetterbox at a 16:9 surface: margin = gui_w - 320 = 106.67.
    #[test]
    fn margin_matches_gml_on_wide_surface() {
        let m = gui_map(1280.0, 720.0, CAM_SCALE);
        let effective_w = (m.hw * 2.0) / m.s;
        assert!((effective_w - 426.6667).abs() < 1e-3);
        let mut catalog = AssetCatalog {
            anims: Default::default(),
            ..Default::default()
        };
        catalog.anims.insert(
            "images/sprLetterbox.png".to_string(),
            [1.0, 320.0, 44.0, 0.0, 0.0, 0.0],
        );
        assert!((letterbox_margin(&catalog, effective_w) - 106.6667).abs() < 1e-3);
        assert_eq!(letterbox_margin(&catalog, 320.0), 0.0);
    }

    /// Crown grid: RANDOM+NONE alone on row one at x 248/276, then the
    /// wrap-after-crwn_none rule forces 4-per-row from _crownleft=220.
    #[test]
    fn crown_slots_match_scrMenuDrawLoadout() {
        let slots = crown_slot_positions();
        assert_eq!(slots.len(), 14);
        let expected: [(u8, f32, f32); 14] = [
            (0, 248.0, 48.0),
            (1, 276.0, 48.0),
            (2, 220.0, 76.0),
            (3, 248.0, 76.0),
            (4, 276.0, 76.0),
            (5, 304.0, 76.0),
            (6, 220.0, 104.0),
            (7, 248.0, 104.0),
            (8, 276.0, 104.0),
            (9, 304.0, 104.0),
            (10, 220.0, 132.0),
            (11, 248.0, 132.0),
            (12, 276.0, 132.0),
            (13, 304.0, 132.0),
        ];
        for (got, want) in slots.iter().zip(expected.iter()) {
            assert_eq!(got.0, want.0);
            assert!((got.1 - want.1).abs() < 1e-3, "x mismatch id {}", got.0);
            assert!((got.2 - want.2).abs() < 1e-3, "y mismatch id {}", got.0);
        }
    }

    /// Port <-> GML crown id boundary round-trips; NONE collapses to 0.
    #[test]
    fn crown_id_mapping_roundtrips() {
        // crwn_random(0) is grid-only and has no port form; it maps to
        // port 0 (NONE) and stays there.
        assert_eq!(crate::game::content::crown_gml_to_port(0), 0);
        for gml in 1u8..14 {
            let port = crate::game::content::crown_gml_to_port(gml);
            assert_eq!(crate::game::content::crown_port_to_gml(port), gml);
        }
        assert_eq!(crate::game::content::crown_gml_to_port(1), 0);
        assert_eq!(crate::game::content::crown_port_to_gml(1), 2);
    }

    /// Skin column geometry: x fixed at 184; y start = 120 - 14*count - 2,
    /// step 28 - verified against scrMenuDrawLoadout's _skins_y formula.
    #[test]
    fn skin_slots_match_scrMenuDrawLoadout() {
        // Most races: 3 skins -> starts at 76.
        let three = skin_slot_positions(3);
        assert!(three.iter().all(|(_, x, _)| (*x - 184.0).abs() < 1e-3));
        for (i, want_y) in [76.0_f32, 104.0, 132.0].iter().enumerate() {
            assert!((three[i].2 - want_y).abs() < 1e-3);
        }
        // Robot: 4 skins -> starts at 62.
        let four = skin_slot_positions(4);
        assert!((four[0].2 - 62.0).abs() < 1e-3);
        assert!((four[3].2 - 146.0).abs() < 1e-3);
        // BigDog/Frog: single skin centred at 104.
        let one = skin_slot_positions(1);
        assert!((one[0].2 - 104.0).abs() < 1e-3);
    }
}
