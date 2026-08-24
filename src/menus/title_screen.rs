//! Authentic Repose title interface translated from the nt-rewrite objects
//! (`Menu`, `CharSelect`, `GoButton`). Sprite pods live in `game/ui_art.rs`;
//! this module owns input hitboxes and text overlays, placed on the same
//! centered 320x240 NT GUI surface (`menus::nt_view`) that the sprites use.

use super::*;
use crate::game::content::{CHAR_SELECT_RACES, race_from_gml_id};

/// Slot geometry mirrors Menu/Create_0; must agree with ui_art.rs.
const POD_W: f32 = 16.0;
const POD_H: f32 = 24.0;
const SLOT_XSTART: f32 = 8.0;

fn slot_step(count: usize) -> f32 {
    20.0f32.min(((320.0 - 40.0) / (count as f32).max(1.0)).floor())
}

pub fn title_screen(st: &SharedUi, actions: Arc<Mutex<Vec<UiAction>>>) -> View {
    let v = nt_view(st);

    ZStack(Modifier::new().fill_max_size()).child((
        char_select_layer(st, actions.clone(), &v),
        go_button_layer(st, actions.clone(), &v),
        char_text_layer(st, &v),
        loadout_layer(st, actions, &v),
        tooltip_layer(st, &v),
    ))
}

/// Bottom-right loadout chips (start wep / stored / crown) — mirrors the
/// Menu loadout row.
fn loadout_layer(st: &SharedUi, actions: Arc<Mutex<Vec<UiAction>>>, v: &NtView) -> View {
    if st.selected_character == 0 {
        return Column(Modifier::new().width(0.001).height(0.001));
    }

    let a1 = actions.clone();
    let a2 = actions.clone();
    let a3 = actions;

    let chip = |label: String, on: Box<dyn Fn() + 'static>| {
        Column(
            Modifier::new()
                .padding_values(PaddingValues {
                    left: 6.0 * v.s,
                    right: 6.0 * v.s,
                    top: 3.0 * v.s,
                    bottom: 3.0 * v.s,
                })
                .background(RColor::from_rgba(12, 12, 16, 220))
                .clip_rounded(3.0)
                .clickable_ext(true, None, None, move || on()),
        )
        .child(
            RText(label)
                .size((7.0 * v.s).clamp(8.0, 96.0))
                .font_family("Silkscreen")
                .color(RColor::WHITE)
                .single_line(),
        )
    };

    Row(Modifier::new()
        .fill_max_size()
        .padding_values(PaddingValues {
            left: v.ox + 160.0 * v.s,
            right: 0.0,
            top: v.oy + (240.0 - 36.0 - 28.0) * v.s,
            bottom: 0.0,
        })
        .gap(6.0 * v.s)
        .align_items(AlignItems::FLEX_START))
    .child((
        chip(
            format!("WEP {}", st.start_weapon_name.to_ascii_uppercase()),
            Box::new(move || push(&a1, UiAction::CycleStartWeapon(1))),
        ),
        chip(
            format!("BWP {}", st.stored_weapon_name.to_ascii_uppercase()),
            Box::new(move || push(&a2, UiAction::CycleStoredWeapon(1))),
        ),
        chip(
            format!("CRN {}", st.crown.to_ascii_uppercase()),
            Box::new(move || push(&a3, UiAction::CycleCrown(1))),
        ),
    ))
}

/// scrCampfireMenuDrawCharText: the chosen mutant's big name plus their
/// passive/active skill lines, anchored to the bottom-left letterbox.
fn char_text_layer(st: &SharedUi, v: &NtView) -> View {
    let race = crate::game::content::race_from_gml_id(st.selected_character);
    let Some(race) = race.filter(|r| *r != crate::game::content::RaceId::Random) else {
        return Column(Modifier::new().width(0.001).height(0.001));
    };

    let name = character_def(race).name.to_ascii_uppercase();
    let passive = crate::game::content::race_passive_text(race);
    let active =
        crate::game::content::ability_name(crate::game::content::character_def(race).ability);

    Row(Modifier::new()
        .fill_max_size()
        .padding_values(PaddingValues {
            left: v.ox + 2.0 * v.s,
            right: 0.0,
            top: v.oy + (240.0 - 36.0 - 32.0) * v.s,
            bottom: 0.0,
        })
        .align_items(AlignItems::FLEX_START))
    .child(
        Column(Modifier::new().gap(2.0 * v.s)).child((
            RText(name)
                .size((14.0 * v.s).clamp(10.0, 160.0))
                .font_family("Silkscreen")
                .color(RColor::WHITE)
                .single_line(),
            RText(format!("PASSIVE: {passive}"))
                .size((7.0 * v.s).clamp(8.0, 96.0))
                .font_family("Silkscreen")
                .color(RColor::WHITE)
                .single_line(),
            RText(format!("ACTIVE: {active}"))
                .size((7.0 * v.s).clamp(8.0, 96.0))
                .font_family("Silkscreen")
                .color(RColor::WHITE)
                .single_line(),
        )),
    )
}

/// One invisible click target per `CharSelect` instance (CharSelect/Mouse_4).
fn char_select_layer(st: &SharedUi, actions: Arc<Mutex<Vec<UiAction>>>, v: &NtView) -> View {
    let count = CHAR_SELECT_RACES.len();
    let ystart = crate::game::ui_art::slot_ystart();

    let mut cells: Vec<View> = Vec::with_capacity(count);
    for race in CHAR_SELECT_RACES.iter() {
        let race_id = *race as usize;
        let a = actions.clone();
        cells.push(Column(
            Modifier::new()
                .width(POD_W * v.s)
                .height(POD_H * v.s)
                .clickable_ext(true, None, None, move || {
                    push(&a, UiAction::SelectCharacter(race_id));
                }),
        ));
    }

    // The whole strip is one row; every pod rect forwards its own race id.
    // Hitboxes sit exactly on the sprite rects (step == POD_W at 17 slots).
    let _ = st;
    Row(Modifier::new()
        .fill_max_size()
        .padding_values(PaddingValues {
            left: v.ox + SLOT_XSTART * v.s,
            right: 0.0,
            top: v.oy + ystart * v.s,
            bottom: 0.0,
        })
        .gap(0.0)
        .align_items(AlignItems::FLEX_START))
    .child(cells)
}

/// GoButton/Mouse_4: clicking the visible button starts the run.
fn go_button_layer(st: &SharedUi, actions: Arc<Mutex<Vec<UiAction>>>, v: &NtView) -> View {
    let count = CHAR_SELECT_RACES.len();
    let step = slot_step(count);
    let last_x = SLOT_XSTART + step * (count - 1) as f32;
    // Menu/Create_0 placement.
    let gx = last_x + step + 2.0;
    let gy = 240.0 - 36.0 + (19.0_f32 / 2.0).floor() - 2.0;
    const GO_W: f32 = 31.0;
    const GO_H: f32 = 19.0;
    const GO_YORIGIN: f32 = -2.0;
    let visible = st.title_go_visible;

    let a = actions.clone();
    Column(
        Modifier::new()
            .fill_max_size()
            .padding_values(PaddingValues {
                left: v.ox + gx * v.s,
                right: 0.0,
                top: v.oy + (gy - GO_YORIGIN) * v.s,
                bottom: 0.0,
            })
            .align_items(AlignItems::FLEX_START),
    )
    .child(Column(
        Modifier::new()
            .width(GO_W * v.s)
            .height(GO_H * v.s)
            .clickable_ext(visible, None, None, move || {
                if visible {
                    push(&a, UiAction::StartGame);
                }
            }),
    ))
}

/// Menu/Draw_74 tooltip: race name above the pointed pod.
fn tooltip_layer(st: &SharedUi, v: &NtView) -> View {
    let hover = st.title_hover_race;
    if hover < 0 {
        return Column(Modifier::new().width(0.001).height(0.001));
    }

    let name = match race_from_gml_id(hover as usize) {
        Some(crate::game::content::RaceId::Random) => "RANDOM".to_string(),
        Some(race) => character_def(race).name.to_ascii_uppercase(),
        None => return Column(Modifier::new().width(0.001).height(0.001)),
    };

    // Center the pill over the pointed pod's bbox centre.
    let step = slot_step(CHAR_SELECT_RACES.len());
    let pod_cx_nt = SLOT_XSTART + hover as f32 * step + POD_W / 2.0;
    let pill_w = 120.0 * v.s;

    Row(Modifier::new()
        .fill_max_size()
        .padding_values(PaddingValues {
            left: (v.ox + pod_cx_nt * v.s - pill_w / 2.0).max(0.0),
            right: 0.0,
            top: v.oy + (crate::game::ui_art::slot_ystart() - 20.0) * v.s,
            bottom: 0.0,
        })
        .align_items(AlignItems::FLEX_START))
    .child(
        Column(
            Modifier::new()
                .width(pill_w)
                .padding_values(PaddingValues {
                    left: 4.0 * v.s,
                    right: 4.0 * v.s,
                    top: 2.0 * v.s,
                    bottom: 2.0 * v.s,
                })
                .background(RColor::from_rgba(8, 8, 12, 210))
                .clip_rounded(3.0)
                .align_items(AlignItems::CENTER),
        )
        .child(
            RText(name)
                .size((7.0 * v.s).clamp(8.0, 96.0))
                .font_family("Silkscreen")
                .color(RColor::WHITE)
                .single_line(),
        ),
    )
}
