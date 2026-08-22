//! NT HUD/menu art rendered as camera-anchored world sprites
//! (repose's layout API is text/shape-only). Screen anchoring works by
//! parenting to the Camera2d: children inherit its transform.

use bevy::prelude::*;

use crate::app::AppState;
use crate::game::anim::sprite_anim;
use crate::game::components::*;
use crate::game::content::{AssetCatalog, sprite_exact};

/// Marker for menu art (title backdrop); despawned on state exit.
#[derive(Component)]
pub struct TitleArt;

/// The logo sprite — must not spin with the spiral field.
#[derive(Component)]
pub struct TitleLogo;

/// Marker for in-game HUD art; despawned with the level.
#[derive(Component)]
pub struct HudArt;

/// Handles for the pieces that update every tick.
#[derive(Resource)]
pub struct HudArtRefs {
    pub hp_fill: Entity,
    pub exp_fill: Entity,
    /// Full width of the health fill at 100%.
    pub hp_fill_w: f32,
}

// NT GUI space is 320x240; our ortho view shows window*scale world units.
// Anchors are expressed as NT GUI coords and mapped 1:1 (NT pixels == world
// pixels), offset from the view's top-left corner.

pub struct UiArtPlugin;

impl Plugin for UiArtPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::Title), (spawn_title_art, spawn_char_slots))
            .add_systems(
                Update,
                char_slot_click.run_if(in_state(AppState::Title)),
            )
            .add_systems(
                OnExit(AppState::Title),
                (despawn_title_art, despawn_hud_art),
            )
            .add_systems(OnEnter(AppState::InGame), spawn_hud_art)
            .add_systems(OnExit(AppState::InGame), despawn_hud_art)
            .add_systems(FixedUpdate, (spin_spiral, sync_hp_fill, sync_exp_fill));
    }
}

fn camera_entity(
    q: &Query<(Entity, &Transform, &Projection), With<Camera2d>>,
) -> Option<(Entity, Vec2)> {
    q.iter().next().map(|(e, tf, proj)| {
        let scale = match proj {
            Projection::Orthographic(o) => o.scale,
            _ => 1.0,
        };
        (e, Vec2::splat(scale))
    })
}

// ---------------------------------------------------------------------------
// Title: rotating spiral field + logo
// ---------------------------------------------------------------------------

fn spawn_title_art(
    mut commands: Commands,
    catalog: Res<AssetCatalog>,
    asset_server: Res<AssetServer>,
    cam_q: Query<(Entity, &Transform, &Projection), With<Camera2d>>,
) {
    let Some((cam, _)) = camera_entity(&cam_q) else {
        return;
    };

    // Rotating root; spiral tiles are its children so the WHOLE field turns
    // like the original menu instead of each tile spinning in place.
    let root = commands
        .spawn((
            TitleArt,
            SpiralRoot,
            ChildOf(cam),
            Transform::from_xyz(0.0, 0.0, -900.0),
        ))
        .id();

    let half_w: f32 = 640.0 * 0.45 + 96.0;
    let half_h: f32 = 360.0 * 0.45 + 96.0;
    let cols = ((half_w * 2.0) / 64.0).ceil() as i32;
    let rows = ((half_h * 2.0) / 64.0).ceil() as i32;
    let mut spiral = sprite_exact(&catalog, &asset_server, "images/sprSpiral.png");
    // The extracted night-area spiral is bright purple; darken to NT-menu levels.
    spiral.color = Color::srgb(0.30, 0.30, 0.42);
    for iy in -rows..=rows {
        for ix in -cols..=cols {
            commands.spawn((
                TitleArt,
                SpiralTile,
                ChildOf(root),
                spiral.clone(),
                Transform::from_xyz(ix as f32 * 64.0, iy as f32 * 64.0, 0.0),
            ));
        }
    }

    // Logo centred well above the repose card.
    commands.spawn((
        TitleArt,
        TitleLogo,
        ChildOf(cam),
        sprite_exact(&catalog, &asset_server, "images/sprLogoGlow.png"),
        Transform::from_xyz(0.0, 300.0, -880.0),
    ));
}

/// Spiral tiles only — rotated indirectly via their root.
#[derive(Component)]
pub struct SpiralTile;

/// The rotating parent of all spiral tiles.
#[derive(Component)]
pub struct SpiralRoot;

fn despawn_title_art(mut commands: Commands, q: Query<Entity, With<TitleArt>>) {
    for e in &q {
        commands.entity(e).despawn();
    }
}

fn spin_spiral(time: Res<Time>, mut q: Query<&mut Transform, With<SpiralRoot>>) {
    for mut tf in &mut q {
        tf.rotate_z(time.delta_secs() * 0.25);
    }
}

// ---------------------------------------------------------------------------
// In-game HUD: sprHealthBar frame + fill, sprExpBar rad meter
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn spawn_hud_art(
    mut commands: Commands,
    catalog: Res<AssetCatalog>,
    asset_server: Res<AssetServer>,
    cam_q: Query<(Entity, &Transform, &Projection), With<Camera2d>>,
    existing: Query<(), (With<HudArt>, Without<Camera2d>)>,
) {
    if !existing.is_empty() {
        return;
    }
    let Some((cam, _)) = camera_entity(&cam_q) else {
        return;
    };

    // NT layout: HP frame 88x14 at gui(20,4); fill inset (2,3) w=84 h=8.
    // Exp bar 14x24 at gui(4,4).
    // Map: gui(x,y) -> local = (-view_half_x + x, view_half_y - y).
    let s = 0.45f32; // current base zoom (matches progression.rs CameraFollow)
    let half_x = 640.0 * s;
    let half_y = 360.0 * s;
    let gx = |x: f32| -half_x + x;
    let gy = |y: f32| half_y - y;

    commands.spawn((
        HudArt,
        ChildOf(cam),
        sprite_exact(&catalog, &asset_server, "images/sprHealthBar.png"),
        Transform::from_xyz(gx(20.0 + 44.0), gy(4.0 + 7.0), -870.0),
    ));

    let mut fill = sprite_exact(&catalog, &asset_server, "images/sprHealthFill.png");
    // Upstream tints the white fill via draw colour (default NT red).
    fill.color = Color::srgb_u8(252, 56, 0);
    fill.custom_size = Some(Vec2::new(84.0, 8.0));
    fill.rect = Some(Rect::new(0.0, 0.0, 1.0, 8.0));
    let hp_fill = commands
        .spawn((
            HudArt,
            ChildOf(cam),
            fill,
            Transform::from_xyz(gx(22.0 + 42.0), gy(7.0 + 4.0), -869.0),
        ))
        .id();

    let mut exp_sprite = sprite_exact(&catalog, &asset_server, "images/sprExpBar.png");
    // Slice horizontally by rad progress; native cell is 14x24.
    exp_sprite.rect = Some(Rect::new(0.0, 0.0, 14.0, 24.0));
    let exp_fill = commands
        .spawn((
            HudArt,
            ChildOf(cam),
            exp_sprite,
            Transform::from_xyz(gx(4.0 + 7.0), gy(4.0 + 12.0), -870.0),
        ))
        .id();

    commands.insert_resource(HudArtRefs {
        hp_fill,
        exp_fill,
        hp_fill_w: 84.0,
    });
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

#[allow(clippy::needless_option_as_deref)]
fn sync_hp_fill(
    refs: Option<Res<HudArtRefs>>,
    player_q: Query<&Health, With<Player>>,
    mut fill_q: Query<(&mut Sprite, &mut Transform), Without<Camera2d>>,
) {
    let Some(refs) = refs else {
        return;
    };
    let Ok(hp) = player_q.single() else {
        return;
    };
    let Ok((mut spr, mut tf)) = fill_q.get_mut(refs.hp_fill) else {
        return;
    };
    let frac = (hp.hp.max(0) as f32 / hp.max.max(1) as f32).clamp(0.0, 1.0);
    let w = (refs.hp_fill_w * frac).max(0.001);
    spr.custom_size = Some(Vec2::new(w, 8.0));
    // Keep the fill's LEFT edge fixed at gui x=22.
    let s = 0.45f32;
    tf.translation.x = -(640.0 * s) + 22.0 + w * 0.5;
}

fn sync_exp_fill(
    refs: Option<Res<HudArtRefs>>,
    player_q: Query<&Player, With<Player>>,
    mut q: Query<(&mut Sprite, &mut Transform), (With<HudArt>, Without<Camera2d>)>,
) {
    let Some(refs) = refs else {
        return;
    };
    let Ok(player) = player_q.single() else {
        return;
    };
    let Ok((mut spr, mut tf)) = q.get_mut(refs.exp_fill) else {
        return;
    };
    // sprExpBar is a single-frame texture; emulate NT's progress subimage by
    // slicing it horizontally and keeping the LEFT edge at gui x=4.
    let frac = (player.rads as f32 / player.next_level_rads.max(1) as f32).clamp(0.0, 1.0);
    spr.rect = Some(Rect::new(0.0, 0.0, 14.0 * frac, 24.0));
    spr.custom_size = Some(Vec2::new(14.0 * frac, 24.0));
    let s = 0.45f32;
    tf.translation.x = -(640.0 * s) + 4.0 + 7.0 * frac;
}

/// One selectable mutant pod.
#[derive(Component)]
pub struct CharSlot {
    pub index: usize,
    pub half: Vec2,
}

fn menu_sprite(
    catalog: &AssetCatalog,
    race: crate::game::content::RaceId,
    selected: bool,
) -> &'static str {
    use crate::game::content::RaceId::*;
    let name = match race {
        Fish => "sprFishMenu",
        Crystal => "sprCrystalMenu",
        Eyes => "sprEyesMenu",
        Melting => "sprMeltingMenu",
        Plant => "sprPlantMenu",
        Venuz => "sprVenuzMenu",
        Steroids => "sprSteroidsMenu",
        Robot => "sprRobotMenu",
        Chicken => "sprChickenMenu",
        Rebel => "sprRebelMenu",
        Horror => "sprHorrorMenu",
        Rogue => "sprRogueMenu",
        BigDog | Skeleton | Frog | Random | Cuz => {
            return "images/sprCharSelectLocked.png";
        }
    };
    // Prefer Selected/Deselect variants, fall back to base, then to locked pod.
    for cand in [
        if selected {
            format!("images/{name}Selected.png")
        } else {
            format!("images/{name}Deselect.png")
        },
        format!("images/{name}.png"),
    ] {
        if catalog.has(&cand) {
            return Box::leak(cand.into_boxed_str());
        }
    }
    "images/sprCharSelectLocked.png"
}

fn spawn_char_slots(
    mut commands: Commands,
    catalog: Res<AssetCatalog>,
    asset_server: Res<AssetServer>,
    selected: Res<crate::game::SelectedCharacter>,
    cam_q: Query<Entity, With<Camera2d>>,
) {
    let Ok(cam) = cam_q.single() else {
        return;
    };
    use crate::game::content::{PLAYABLE_RACES, RaceId};
    let cols = 8usize;
    let pitch_x = 64.0f32;
    let pitch_y = 72.0;
    for (i, race) in PLAYABLE_RACES.iter().enumerate() {
        let col = i % cols;
        let row = i / cols;
        let n = PLAYABLE_RACES.len();
        let row_n = if row == 0 { cols.min(n) } else { n - cols };
        let x = (col as f32 - (cols as f32 - 1.0) * 0.5) * pitch_x;
        let y = -250.0 - row as f32 * pitch_y + (row == 1) as i32 as f32 * 0.0;
        let is_sel = selected.0 == *race;
        let path = menu_sprite(&catalog, *race, is_sel);
        // Native pod art varies from 16x24 to 96x96; normalise for a grid.
        let mut spr = sprite_exact(&catalog, &asset_server, path);
        spr.custom_size = Some(Vec2::splat(44.0));
        commands.spawn((
            TitleArt,
            CharSlot {
                index: i,
                half: Vec2::splat(28.0),
            },
            ChildOf(cam),
            spr,
            Transform::from_xyz(x, y, -860.0),
        ));
        let _ = row_n;
    }
}

/// Click hit-testing for char slots (Title state only).
fn char_slot_click(
    mut commands: Commands,
    mouse: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    cam_q: Query<(&Camera, &GlobalTransform), With<Camera2d>>,
    slots: Query<(&CharSlot, &GlobalTransform)>,
    bridge: Res<crate::menus::UiBridge>,
) {
    if !mouse.just_pressed(MouseButton::Left) {
        return;
    }
    let Ok(window) = windows.single() else {
        return;
    };
    let Some(cursor) = window.cursor_position() else {
        return;
    };
    let Ok((cam, cam_gt)) = cam_q.single() else {
        return;
    };
    let Ok(world) = cam.viewport_to_world_2d(cam_gt, cursor) else {
        return;
    };
    for (slot, tf) in &slots {
        let c = tf.translation().truncate();
        if (world.x - c.x).abs() <= slot.half.x && (world.y - c.y).abs() <= slot.half.y {
            if let Ok(mut q) = bridge.actions.lock() {
                q.push(crate::menus::UiAction::SelectCharacter(slot.index));
            }
            return;
        }
    }
}
