use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use std::rc::Rc;

use repose_core::PaddingValues;
use repose_core::View;
use repose_core::prelude::{
    AlignItems, AlignSelf, AnimationSpec, Color as RColor, Easing, JustifyContent, Modifier,
    remember,
};
use repose_material::material3::{
    ButtonConfig, DropdownMenu, DropdownMenuConfig, DropdownMenuEntry, DropdownMenuItem,
    FilledTonalButton, MenuState,
};
use repose_ui::anim_ext::{
    AnimatedVisibility, AnimatedVisibilityConfig, EnterTransition, ExitTransition,
};
use repose_ui::overlay::OverlayHandle;
use repose_ui::{Column, Row, Text as RText, TextStyle, ViewExt, ZStack};

pub mod loadout_menu;
pub mod mutation_menu;
pub mod pause_menu;
pub mod settings_menu;
pub mod title_screen;
pub mod unlock_popup;

use crate::app::{AppState, OverlayMenu, SharedUi};
use crate::game::content::{PLAYABLE_RACES, character_def};

fn t(translations: &HashMap<String, String>, key: &str, fallback: &str) -> String {
    translations
        .get(key)
        .cloned()
        .unwrap_or_else(|| fallback.to_string())
}

#[derive(Clone, Debug)]
pub enum UiAction {
    StartGame,
    /// Main-menu PLAY: into the char-select campfire.
    MainMenuPlay,
    OpenSettings,
    OpenCredits,
    CloseOverlay,
    Resume,
    QuitToTitle,
    QuitApp,
    SetMasterVol(f32),
    SetSfxVol(f32),
    SetMusicVol(f32),
    SaveSettings,
    NextLanguage,
    SetLanguage(String),
    SelectCharacter(usize),
    SelectSkin(u8),
    CycleStartWeapon(i8),
    CycleStoredWeapon(i8),
    CycleCrown(i8),
    PickMutation(usize),
}

#[derive(bevy::prelude::Resource, Clone)]
pub struct UiBridge {
    pub shared: Arc<Mutex<SharedUi>>,
    pub actions: Arc<Mutex<Vec<UiAction>>>,
}

fn spacer(h: f32) -> View {
    Column(Modifier::new().height(h).width(1.0))
}

fn popup_anim_config(key: &str) -> AnimatedVisibilityConfig {
    AnimatedVisibilityConfig {
        key: key.into(),
        spec: AnimationSpec::tween(Duration::from_millis(200), Easing::EaseOut),
        enter: EnterTransition::ScaleIn { initial: 0.95 },
        exit: ExitTransition::ScaleOut { target: 0.95 },
    }
}

pub fn compose_root(
    overlay: OverlayHandle,
    st: SharedUi,
    actions: Arc<Mutex<Vec<UiAction>>>,
) -> View {
    let root = ZStack(Modifier::new().fill_max_size());
    let settings_view = settings_ui(overlay, &st, actions.clone());

    let content = match st.phase {
        AppState::Splash => splash_ui(&st),
        AppState::Loading => loading_ui(&st),
        AppState::MainMenu => ZStack(Modifier::new().fill_max_size()).child((
            main_menu_ui(&st, actions.clone()),
            AnimatedVisibility(
                st.overlay == OverlayMenu::Settings,
                settings_view.clone(),
                popup_anim_config("menu_settings"),
            ),
        )),
        AppState::Title => ZStack(Modifier::new().fill_max_size()).child((
            title_screen::title_screen(&st, actions.clone()),
            AnimatedVisibility(
                st.overlay == OverlayMenu::Settings,
                settings_view.clone(),
                popup_anim_config("title_settings"),
            ),
            AnimatedVisibility(
                st.overlay == OverlayMenu::Credits,
                credits_ui(&st, actions.clone()),
                popup_anim_config("title_credits"),
            ),
        )),
        AppState::InGame => {
            // Original HUD: sprite art lives in game/ui_art.rs; this overlay
            // only carries the numeric texts drawn by scrDrawPlayerHUD.
            let mut children: Vec<View> = vec![nt_hud_overlay(&st)];
            if st.game_over {
                let panel = game_over_panel(&st, actions.clone());
                children.push(AnimatedVisibility(
                    true,
                    panel,
                    popup_anim_config("game_over_in"),
                ));
            } else if !st.mutation_choices.is_empty() {
                let panel = mutation_panel(&st, actions.clone());
                children.push(AnimatedVisibility(
                    true,
                    panel,
                    popup_anim_config("mutation_in"),
                ));
            }
            children.push(AnimatedVisibility(
                st.overlay == OverlayMenu::Pause,
                pause_overlay(&st, actions.clone()),
                popup_anim_config("pause"),
            ));
            children.push(AnimatedVisibility(
                st.overlay == OverlayMenu::Settings,
                settings_view.clone(),
                popup_anim_config("ingame_settings"),
            ));
            children.push(AnimatedVisibility(
                st.overlay == OverlayMenu::Credits,
                credits_ui(&st, actions.clone()),
                popup_anim_config("ingame_credits"),
            ));
            ZStack(Modifier::new().fill_max_size()).child(children)
        }
    };

    if st.transition_alpha > 0.001 || st.flash_alpha > 0.001 {
        let fade_a = (st.transition_alpha.clamp(0.0, 1.0) * 255.0) as u8;
        let flash_a = (st.flash_alpha.clamp(0.0, 1.0) * 255.0) as u8;
        root.child((
            content,
            Column(
                Modifier::new()
                    .fill_max_size()
                    .background(RColor::from_rgba(0, 0, 0, fade_a)),
            ),
            Column(
                Modifier::new()
                    .fill_max_size()
                    .background(RColor::from_rgba(flash_a, flash_a, flash_a, flash_a)),
            ),
        ))
    } else {
        root.child(content)
    }
}

/// Wrap a panel so it sits centred inside the letterboxed NT GUI surface,
/// matching sprite art placement across window sizes.
fn nt_surface_wrap(st: &SharedUi, panel: View) -> View {
    let v = nt_view(st);
    Column(
        Modifier::new()
            .fill_max_size()
            .padding_values(PaddingValues {
                left: v.ox,
                right: 0.0,
                top: v.oy,
                bottom: 0.0,
            })
            .align_items(AlignItems::FLEX_START),
    )
    .child(
        Column(
            Modifier::new()
                .width(320.0 * v.s)
                .height(240.0 * v.s)
                .justify_content(JustifyContent::CENTER)
                .align_items(AlignItems::CENTER),
        )
        .child(panel),
    )
}

/// Boot screen: the original `Logo` object draws sprLogo dead-centre on a
/// black GUI; the sprite itself is spawned by ui_art.rs.
/// The Vlambeer boot cards (Vlambeer/Draw_0), rendered per boot mode. The
/// sprites (saving icon, Vlambeer card, NT logo) live in ui_art.rs.
fn splash_ui(st: &SharedUi) -> View {
    let v = nt_view(st);
    let mut layers: Vec<View> = Vec::new();
    let cy = GUI_H_F32 / 2.0;

    match st.boot_mode {
        0 => {
            layers.push(nt_text_at(
                "DO NOT TURN OFF NUCLEAR THRONE".to_string(),
                160.0,
                cy + 20.0,
                &v,
                col(255, 255, 255),
                true,
            ));
            layers.push(nt_text_at(
                "WHILE THIS SAVING ICON IS DISPLAYED.".to_string(),
                160.0,
                cy + 30.0,
                &v,
                col(255, 255, 255),
                true,
            ));
        }
        1 => {
            layers.push(nt_text_at(
                "MADE IN GAMEMAKER".to_string(),
                160.0,
                cy,
                &v,
                col(255, 255, 255),
                true,
            ));
        }
        3 => {
            // Team credits: VLAMBEER in yellow, the rest white.
            layers.push(nt_text_at(
                "VLAMBEER".to_string(),
                160.0,
                cy - 24.0,
                &v,
                col(255, 221, 0),
                true,
            ));
            layers.push(nt_text_at(
                "PAUL VEER".to_string(),
                160.0,
                cy - 8.0,
                &v,
                col(255, 255, 255),
                true,
            ));
            layers.push(nt_text_at(
                "JUKIO KALLIO".to_string(),
                160.0,
                cy + 2.0,
                &v,
                col(255, 255, 255),
                true,
            ));
            layers.push(nt_text_at(
                "JOONAS TURNER".to_string(),
                160.0,
                cy + 12.0,
                &v,
                col(255, 255, 255),
                true,
            ));
            layers.push(nt_text_at(
                "JUSTIN CHAN".to_string(),
                160.0,
                cy + 22.0,
                &v,
                col(255, 255, 255),
                true,
            ));
            layers.push(nt_text_at(
                "YELLOWAFTERLIFE".to_string(),
                160.0,
                cy + 32.0,
                &v,
                col(255, 255, 255),
                true,
            ));
            layers.push(nt_text_at(
                "PRESENT".to_string(),
                160.0,
                cy + 48.0,
                &v,
                col(255, 255, 255),
                true,
            ));
        }
        _ => {}
    }

    ZStack(Modifier::new().fill_max_size()).child(layers)
}

const GUI_H_F32: f32 = 240.0;

/// Loading pass rendered on the NT surface: centred label plus a thin
/// white progress bar (original NT shows nothing; kept minimal).
fn loading_ui(st: &SharedUi) -> View {
    let v = nt_view(st);
    let pct = st.loading_progress.clamp(0.0, 1.0);

    let bar_x = 70.0;
    let bar_y = 132.0;
    let bar_w = 180.0;
    let bar_h = 6.0;

    ZStack(Modifier::new().fill_max_size()).child((
        nt_text_at(
            "LOADING...".to_string(),
            160.0,
            108.0,
            &v,
            col(255, 255, 255),
            true,
        ),
        Column(
            Modifier::new()
                .fill_max_size()
                .padding_values(PaddingValues {
                    left: v.ox + bar_x * v.s,
                    right: 0.0,
                    top: v.oy + bar_y * v.s,
                    bottom: 0.0,
                })
                .align_items(AlignItems::FLEX_START),
        )
        .child(
            Column(
                Modifier::new()
                    .width(bar_w * v.s)
                    .height(bar_h * v.s)
                    .background(col(20, 20, 24))
                    .border((1.0 * v.s).max(1.0), col(238, 239, 225), 0.0)
                    .padding((1.0 * v.s).max(1.0)),
            )
            .child(Column(
                Modifier::new()
                    .width(((bar_w - 2.0) * pct * v.s).max(1.0))
                    .height((bar_h - 2.0) * v.s)
                    .background(col(238, 239, 225)),
            )),
        ),
    ))
}

fn pause_overlay(st: &SharedUi, actions: Arc<Mutex<Vec<UiAction>>>) -> View {
    let a1 = actions.clone();
    let a2 = actions.clone();
    let a3 = actions.clone();
    let tr = &st.translations;

    Column(
        Modifier::new()
            .fill_max_size()
            .justify_content(JustifyContent::CENTER)
            .align_items(AlignItems::CENTER)
            .background(RColor::from_rgba(0, 0, 0, 180)),
    )
    .child(nt_surface_wrap(st, pause_panel(tr, a1, a2, a3)))
}

fn pause_panel(
    tr: &HashMap<String, String>,
    a1: Arc<Mutex<Vec<UiAction>>>,
    a2: Arc<Mutex<Vec<UiAction>>>,
    a3: Arc<Mutex<Vec<UiAction>>>,
) -> View {
    Column(
        Modifier::new()
            .width(320.0)
            .padding(24.0)
            .background(col(20, 20, 28))
            .clip_rounded(12.0)
            .align_items(AlignItems::CENTER),
    )
    .child((
        RText(t(tr, "paused", "Paused"))
            .size(36.0)
            .color(RColor::WHITE),
        spacer(16.0),
        mk_button(&t(tr, "resume", "Resume"), col(60, 140, 90), move || {
            push(&a1, UiAction::Resume)
        }),
        mk_button(&t(tr, "settings", "Settings"), col(70, 70, 90), move || {
            push(&a2, UiAction::OpenSettings)
        }),
        mk_button(
            &t(tr, "quit-to-title", "Quit to Title"),
            col(180, 60, 60),
            move || push(&a3, UiAction::QuitToTitle),
        ),
    ))
}

fn settings_ui(overlay: OverlayHandle, st: &SharedUi, actions: Arc<Mutex<Vec<UiAction>>>) -> View {
    let a_m_down = actions.clone();
    let a_m_up = actions.clone();
    let a_s_down = actions.clone();
    let a_s_up = actions.clone();
    let a_mu_down = actions.clone();
    let a_mu_up = actions.clone();
    let a_save = actions.clone();
    let a_back = actions.clone();
    let master = st.master_vol;
    let sfx = st.sfx_vol;
    let music = st.music_vol;
    let tr = &st.translations;
    let lang = &st.language;
    let langs = &st.available_languages;
    let overlay_clone = overlay.clone();
    let actions_clone = actions.clone();

    let menu_state: Rc<MenuState> = remember(MenuState::new);
    let lang_items: Vec<DropdownMenuEntry> = langs
        .iter()
        .map(|l| {
            let a = actions_clone.clone();
            let code = l.clone();
            let mut item = DropdownMenuItem::new(l.clone(), move || {
                push(&a, UiAction::SetLanguage(code.clone()))
            });
            if l == lang {
                item = item.disabled();
            }
            DropdownMenuEntry::Item(item)
        })
        .collect();
    let menu_trigger = menu_state.clone();
    let lang_label = st.language.clone();
    let trigger = FilledTonalButton(
        Modifier::new().width(100.0).height(40.0),
        move || menu_trigger.open(),
        ButtonConfig::default(),
        move || RText(lang_label.clone()).size(20.0),
    );

    let lang_dropdown = DropdownMenu(
        menu_state,
        overlay_clone,
        Modifier::new(),
        trigger,
        lang_items,
        DropdownMenuConfig {
            min_width: 100.0,
            ..Default::default()
        },
    );

    let inner = Column(
        Modifier::new()
            .width(360.0)
            .padding(24.0)
            .background(col(20, 20, 28))
            .clip_rounded(12.0)
            .align_items(AlignItems::CENTER),
    )
    .child(
        RText(t(tr, "settings", "Settings"))
            .size(36.0)
            .color(RColor::WHITE),
    )
    .child(spacer(12.0))
    .child(
        RText(format!(
            "{}: {:.0}%",
            t(tr, "master-volume", "Master"),
            master * 100.0
        ))
        .size(18.0)
        .color(RColor::WHITE),
    )
    .child(Row(Modifier::new().gap(8.0)).child((
        mk_button_sm("-", move || {
            push(&a_m_down, UiAction::SetMasterVol(master - 0.1))
        }),
        mk_button_sm("+", move || {
            push(&a_m_up, UiAction::SetMasterVol(master + 0.1))
        }),
    )))
    .child(spacer(8.0))
    .child(
        RText(format!(
            "{}: {:.0}%",
            t(tr, "sfx-volume", "SFX"),
            sfx * 100.0
        ))
        .size(18.0)
        .color(RColor::WHITE),
    )
    .child(Row(Modifier::new().gap(8.0)).child((
        mk_button_sm("-", move || push(&a_s_down, UiAction::SetSfxVol(sfx - 0.1))),
        mk_button_sm("+", move || push(&a_s_up, UiAction::SetSfxVol(sfx + 0.1))),
    )))
    .child(spacer(8.0))
    .child(
        RText(format!(
            "{}: {:.0}%",
            t(tr, "music-volume", "Music"),
            music * 100.0
        ))
        .size(18.0)
        .color(RColor::WHITE),
    )
    .child(Row(Modifier::new().gap(8.0)).child((
        mk_button_sm("-", move || {
            push(&a_mu_down, UiAction::SetMusicVol(music - 0.1))
        }),
        mk_button_sm("+", move || {
            push(&a_mu_up, UiAction::SetMusicVol(music + 0.1))
        }),
    )))
    .child(spacer(8.0))
    .child(
        RText(format!("{}:", t(tr, "language", "Language")))
            .size(18.0)
            .color(RColor::WHITE),
    )
    .child(Row(Modifier::new().gap(6.0)).child(lang_dropdown))
    .child(spacer(16.0))
    .child(mk_button(
        &t(tr, "save", "Save"),
        col(60, 120, 200),
        move || push(&a_save, UiAction::SaveSettings),
    ))
    .child(mk_button(
        &t(tr, "back", "Back"),
        col(70, 70, 90),
        move || push(&a_back, UiAction::CloseOverlay),
    ));

    Column(
        Modifier::new()
            .fill_max_size()
            .justify_content(JustifyContent::CENTER)
            .align_items(AlignItems::CENTER)
            .background(RColor::from_rgba(0, 0, 0, 180)),
    )
    .child(nt_surface_wrap(st, inner))
}

fn credits_ui(st: &SharedUi, actions: Arc<Mutex<Vec<UiAction>>>) -> View {
    let a = actions.clone();
    let tr = &st.translations;
    let inner = Column(
        Modifier::new()
            .width(400.0)
            .padding(24.0)
            .background(col(20, 20, 28))
            .clip_rounded(12.0)
            .align_items(AlignItems::CENTER),
    )
    .child((
        RText(t(tr, "credits", "Credits"))
            .size(36.0)
            .color(RColor::WHITE),
        spacer(12.0),
        RText("A source-side fan recreation of Nuclear Throne (Vlambeer)")
            .size(16.0)
            .color(RColor::WHITE),
        RText("Built with Bevy + Repose on the my-ecosystem-template-bevy")
            .size(16.0)
            .color(RColor::WHITE),
        RText("Template by mlm-games | No original game assets included")
            .size(16.0)
            .color(RColor::WHITE),
        spacer(16.0),
        mk_button(&t(tr, "back", "Back"), col(70, 70, 90), move || {
            push(&a, UiAction::CloseOverlay)
        }),
    ));

    Column(
        Modifier::new()
            .fill_max_size()
            .justify_content(JustifyContent::CENTER)
            .align_items(AlignItems::CENTER)
            .background(RColor::from_rgba(0, 0, 0, 180)),
    )
    .child(nt_surface_wrap(st, inner))
}

static NT_PANEL: RColor = RColor(7, 8, 11, 218);
static NT_PANEL_INNER: RColor = RColor(14, 15, 19, 236);
static NT_TRACK: RColor = RColor(0, 0, 0, 210);
static NT_BORDER: RColor = RColor(255, 255, 255, 34);
static NT_TEXT: RColor = RColor(238, 239, 225, 255);
static NT_MUTED: RColor = RColor(148, 151, 155, 255);
static NT_GOLD: RColor = RColor(245, 210, 92, 255);
static NT_RED: RColor = RColor(221, 56, 45, 255);
static NT_GREEN: RColor = RColor(72, 202, 96, 255);
static NT_PURPLE: RColor = RColor(181, 86, 229, 255);
#[allow(dead_code)] // palette completeness
static NT_BLUE: RColor = RColor(77, 151, 230, 255);

#[derive(Clone, Copy, Debug)]
#[allow(dead_code)] // some fields only exercised by tests
struct HudMetrics {
    margin: f32,
    player_width: f32,
    run_width: f32,
    boss_width: f32,
    panel_padding: f32,
    normal_text: f32,
    small_text: f32,
    hp_bar_width: f32,
    mutation_panel_width: f32,
    mutation_card_width: f32,
    mutation_card_height: f32,
    mutation_gap: f32,
}

fn hud_metrics(compact: bool) -> HudMetrics {
    if compact {
        HudMetrics {
            margin: 8.0,
            player_width: 238.0,
            run_width: 142.0,
            boss_width: 300.0,
            panel_padding: 8.0,
            normal_text: 13.0,
            small_text: 9.0,
            hp_bar_width: 142.0,
            mutation_panel_width: 344.0,
            mutation_card_width: 150.0,
            mutation_card_height: 98.0,
            mutation_gap: 8.0,
        }
    } else {
        HudMetrics {
            margin: 18.0,
            player_width: 306.0,
            run_width: 194.0,
            boss_width: 438.0,
            panel_padding: 11.0,
            normal_text: 15.0,
            small_text: 11.0,
            hp_bar_width: 198.0,
            mutation_panel_width: 594.0,
            mutation_card_width: 262.0,
            mutation_card_height: 96.0,
            mutation_gap: 12.0,
        }
    }
}

pub(crate) fn is_compact_viewport(width: f32, height: f32) -> bool {
    width < 760.0 || height < 560.0
}

fn empty_view() -> View {
    Column(Modifier::new().width(0.001).height(0.001))
}

fn nt_chip(label: impl Into<String>, bg: RColor, fg: RColor, size: f32) -> View {
    Column(
        Modifier::new()
            .padding_values(PaddingValues {
                left: 6.0,
                right: 6.0,
                top: 3.0,
                bottom: 3.0,
            })
            .background(bg)
            .border(1.0, RColor::from_rgba(255, 255, 255, 22), 2.0)
            .clip_rounded(2.0)
            .justify_content(JustifyContent::CENTER)
            .align_items(AlignItems::CENTER),
    )
    .child(
        RText(label.into())
            .size(size)
            .color(fg)
            .single_line()
            .overflow_ellipsize(),
    )
}

fn hp_fill_color(hp: i32, max_hp: i32) -> RColor {
    if max_hp <= 0 {
        return NT_RED;
    }

    let fraction = hp.max(0) as f32 / max_hp as f32;

    if fraction <= 0.25 {
        col(255, 50, 42)
    } else if fraction <= 0.50 {
        col(239, 124, 42)
    } else {
        NT_RED
    }
}

#[allow(dead_code)] // tested
fn boss_display_name(name: &str) -> String {
    if name.trim().is_empty() {
        "BOSS".to_string()
    } else {
        name.to_ascii_uppercase()
    }
}

fn mutation_choice_parts(choice: &str) -> (bool, String, String) {
    let trimmed = choice.trim();
    let (is_ultra, trimmed) = if let Some(rest) = trimmed.strip_prefix("ULTRA:") {
        (true, rest.trim())
    } else {
        (false, trimmed)
    };

    if let Some((name, description)) = trimmed.split_once(" \u{2014} ") {
        (
            is_ultra,
            name.trim().to_string(),
            description.trim().to_string(),
        )
    } else if let Some((name, description)) = trimmed.split_once(" - ") {
        (
            is_ultra,
            name.trim().to_string(),
            description.trim().to_string(),
        )
    } else {
        (is_ultra, trimmed.to_string(), String::new())
    }
}

fn mutation_choice_card(
    index: usize,
    choice: &str,
    actions: Arc<Mutex<Vec<UiAction>>>,
    metrics: HudMetrics,
) -> View {
    let (is_ultra, name, description) = mutation_choice_parts(choice);

    let accent = if is_ultra { NT_GOLD } else { NT_GREEN };
    let background = if is_ultra {
        RColor(245, 210, 92, 18)
    } else {
        RColor(72, 202, 96, 14)
    };

    Column(
        Modifier::new()
            .width(metrics.mutation_card_width)
            .height(metrics.mutation_card_height)
            .padding(9.0)
            .gap(5.0)
            .background(background)
            .border(2.0, accent, 3.0)
            .clip_rounded(3.0)
            .clickable()
            .on_click(move || {
                push(&actions, UiAction::PickMutation(index));
            }),
    )
    .child((
        Row(Modifier::new()
            .align_items(AlignItems::CENTER)
            .justify_content(JustifyContent::SPACE_BETWEEN))
        .child((
            nt_chip(
                (index + 1).to_string(),
                RColor(0, 0, 0, 150),
                accent,
                metrics.small_text,
            ),
            if is_ultra {
                nt_chip(
                    "ULTRA",
                    RColor(245, 210, 92, 28),
                    NT_GOLD,
                    metrics.small_text,
                )
            } else {
                empty_view()
            },
        )),
        RText(name.to_ascii_uppercase())
            .size(metrics.normal_text)
            .color(NT_TEXT)
            .single_line()
            .overflow_ellipsize(),
        RText(description).size(metrics.small_text).color(NT_MUTED),
    ))
}

fn mutation_panel(st: &SharedUi, actions: Arc<Mutex<Vec<UiAction>>>) -> View {
    let metrics = hud_metrics(st.hud_compact);

    let is_ultra = st
        .mutation_choices
        .iter()
        .any(|choice| choice.trim().starts_with("ULTRA:"));

    let cards = st
        .mutation_choices
        .iter()
        .enumerate()
        .map(|(index, choice)| mutation_choice_card(index, choice, actions.clone(), metrics))
        .collect::<Vec<_>>();

    let rows = cards
        .chunks(2)
        .map(|chunk| {
            Row(Modifier::new()
                .gap(metrics.mutation_gap)
                .justify_content(JustifyContent::CENTER)
                .align_items(AlignItems::CENTER))
            .children(chunk.to_vec())
        })
        .collect::<Vec<_>>();

    let accent = if is_ultra { NT_GOLD } else { NT_GREEN };

    let panel = Column(
        Modifier::new()
            .width(metrics.mutation_panel_width)
            .padding(if st.hud_compact { 14.0 } else { 20.0 })
            .gap(10.0)
            .background(RColor(6, 7, 10, 244))
            .border(2.0, accent, 4.0)
            .clip_rounded(4.0)
            .align_items(AlignItems::CENTER),
    )
    .child((
        RText(if is_ultra {
            "CHOOSE ULTRA MUTATION"
        } else {
            "CHOOSE MUTATION"
        })
        .size(if st.hud_compact { 20.0 } else { 27.0 })
        .color(accent)
        .single_line(),
        RText("PRESS 1 / 2 / 3 / 4 OR SELECT A CARD")
            .size(metrics.small_text)
            .color(NT_MUTED)
            .single_line(),
        Column(Modifier::new().gap(metrics.mutation_gap)).children(rows),
    ));

    Column(
        Modifier::new()
            .fill_max_size()
            .justify_content(JustifyContent::CENTER)
            .align_items(AlignItems::CENTER)
            .background(RColor(0, 0, 0, 188)),
    )
    .child(nt_surface_wrap(st, panel))
}

fn game_over_panel(st: &SharedUi, actions: Arc<Mutex<Vec<UiAction>>>) -> View {
    let a1 = actions.clone();
    let a2 = actions.clone();
    let tr = &st.translations;
    let panel = Column(
        Modifier::new()
            .width(420.0)
            .padding(28.0)
            .background(col(22, 16, 16))
            .clip_rounded(14.0)
            .align_items(AlignItems::CENTER),
    )
    .child(vec![
        RText("GAME OVER").size(44.0).color(col(230, 70, 70)),
        spacer(8.0),
        RText(format!("{}: {}", t(tr, "score", "Score"), st.score))
            .size(22.0)
            .color(RColor::WHITE),
        RText(format!("{}: {}", t(tr, "best", "Best"), st.high_score))
            .size(16.0)
            .color(col(200, 200, 200)),
        RText(format!("FLOOR {}", st.best_floor))
            .size(16.0)
            .color(col(200, 200, 200)),
        RText(format!("KILLS {}", st.total_kills))
            .size(16.0)
            .color(col(200, 200, 200)),
        spacer(18.0),
        mk_button(&t(tr, "retry", "Retry"), col(60, 140, 90), move || {
            push(&a1, UiAction::StartGame)
        }),
        mk_button(
            &t(tr, "quit-to-title", "Quit to Title"),
            col(180, 60, 60),
            move || push(&a2, UiAction::QuitToTitle),
        ),
    ]);

    Column(
        Modifier::new()
            .fill_max_size()
            .justify_content(JustifyContent::CENTER)
            .align_items(AlignItems::CENTER)
            .background(RColor::from_rgba(0, 0, 0, 200)),
    )
    .child(nt_surface_wrap(st, panel))
}

fn mk_button(label: &str, _bg: RColor, on_click: impl Fn() + 'static) -> View {
    FilledTonalButton(
        Modifier::new().width(260.0).height(52.0).margin(8.0),
        on_click,
        ButtonConfig::default(),
        move || RText(label).size(20.0),
    )
}

#[allow(dead_code)] // retained for menu submodules / future panels
fn mk_button_colored(label: &str, bg: RColor, on_click: impl Fn() + 'static) -> View {
    FilledTonalButton(
        Modifier::new()
            .width(170.0)
            .height(46.0)
            .margin(4.0)
            .background(bg),
        on_click,
        ButtonConfig::default(),
        move || RText(label).size(16.0),
    )
}

fn mk_button_sm(label: &str, on_click: impl Fn() + 'static) -> View {
    FilledTonalButton(
        Modifier::new().width(48.0).height(40.0),
        on_click,
        ButtonConfig::default(),
        move || RText(label).size(20.0),
    )
}

fn col(r: u8, g: u8, b: u8) -> RColor {
    RColor::from_rgba(r, g, b, 255)
}

/// Pill chip label (Floppy-Warriors reward_chip style).
#[allow(dead_code)] // retained for title/settings/game-over panels
pub(crate) fn reward_chip(label: impl Into<String>, bg: RColor, fg: RColor) -> View {
    Column(
        Modifier::new()
            .padding_values(PaddingValues {
                left: 9.0,
                right: 9.0,
                top: 5.0,
                bottom: 5.0,
            })
            .background(bg)
            .clip_rounded(999.0)
            .justify_content(JustifyContent::CENTER)
            .align_items(AlignItems::CENTER),
    )
    .child(
        RText(label.into())
            .size(11.0)
            .color(fg)
            .single_line()
            .overflow_ellipsize(),
    )
}

/// Pill stat bar (Floppy-Warriors hud_stat_bar style).
#[allow(dead_code)] // retained for title/settings/game-over panels
pub(crate) fn hud_stat_bar(width: f32, height: f32, frac: f32, fill: RColor) -> View {
    let f = frac.clamp(0.0, 1.0);
    let inner_w = if f <= 0.0 {
        0.001
    } else {
        (width * f).max(2.0)
    };
    let radius = (height * 0.5).max(2.0);

    Column(
        Modifier::new()
            .width(width)
            .height(height)
            .background(RColor::from_rgba(0, 0, 0, 170))
            .border(1.0, RColor::from_rgba(255, 255, 255, 24), radius)
            .clip_rounded(radius),
    )
    .child(Column(
        Modifier::new()
            .width(inner_w)
            .height(height)
            .background(fill)
            .clip_rounded(radius)
            .align_self(AlignSelf::FLEX_START),
    ))
}

fn push(actions: &Arc<Mutex<Vec<UiAction>>>, a: UiAction) {
    if let Ok(mut q) = actions.lock() {
        q.push(a);
    }
}

#[cfg(test)]
mod nt_ui_tests {
    use super::*;

    #[test]
    fn desktop_viewport_is_not_compact() {
        assert!(!is_compact_viewport(1280.0, 720.0));
        assert!(!is_compact_viewport(1920.0, 1080.0));
    }

    #[test]
    fn mobile_and_small_windows_are_compact() {
        assert!(is_compact_viewport(360.0, 800.0));
        assert!(is_compact_viewport(720.0, 540.0));
        assert!(is_compact_viewport(640.0, 720.0));
    }

    #[test]
    fn mutation_choice_splits_normal() {
        let (ultra, name, desc) = mutation_choice_parts("RHINO SKIN \u{2014} +4 max HP");

        assert!(!ultra);
        assert_eq!(name, "RHINO SKIN");
        assert_eq!(desc, "+4 max HP");
    }

    #[test]
    fn mutation_choice_splits_ultra() {
        let (ultra, name, desc) =
            mutation_choice_parts("ULTRA: CONFISCATE \u{2014} Better weapon drops");

        assert!(ultra);
        assert_eq!(name, "CONFISCATE");
        assert_eq!(desc, "Better weapon drops");
    }

    #[test]
    fn mutation_choice_accepts_ascii_dash() {
        let (_, name, desc) = mutation_choice_parts("LASER BRAIN - Stronger energy weapons");

        assert_eq!(name, "LASER BRAIN");
        assert_eq!(desc, "Stronger energy weapons");
    }

    #[test]
    fn mutation_choice_without_description_is_safe() {
        let (ultra, name, desc) = mutation_choice_parts("STRONG SPIRIT");

        assert!(!ultra);
        assert_eq!(name, "STRONG SPIRIT");
        assert!(desc.is_empty());
    }

    #[test]
    fn empty_boss_name_has_fallback() {
        assert_eq!(boss_display_name(""), "BOSS");
        assert_eq!(boss_display_name("   "), "BOSS");
    }

    #[test]
    fn boss_name_is_uppercase() {
        assert_eq!(boss_display_name("Lil Hunter"), "LIL HUNTER");
    }

    #[test]
    fn low_health_uses_danger_color_path() {
        let danger = hp_fill_color(1, 10);
        let healthy = hp_fill_color(10, 10);

        assert_ne!(danger, healthy);
    }

    #[test]
    fn compact_metrics_fit_mutation_panel_on_phone() {
        let metrics = hud_metrics(true);

        assert!(metrics.mutation_panel_width <= 360.0);
        assert!(
            metrics.mutation_card_width * 2.0 + metrics.mutation_gap < metrics.mutation_panel_width
        );
    }

    #[test]
    fn desktop_metrics_leave_screen_margin() {
        let metrics = hud_metrics(false);

        assert!(metrics.player_width + metrics.run_width + metrics.margin * 2.0 < 1280.0);
        assert!(metrics.boss_width < 1280.0);
    }
}

/// The five big main-menu buttons (nt-rewrite `MainMenuButton`): PLAY,
/// CO-OP, SETTINGS, STATS, QUIT — big pixel text centred at gui x=160,
/// stacked 24 px apart from y=72. Hover tints c_uigray -> white.
fn main_menu_ui(st: &SharedUi, actions: Arc<Mutex<Vec<UiAction>>>) -> View {
    let v = nt_view(st);
    const LABELS: [(&str, i32); 5] = [
        ("PLAY", 0),
        ("CO-OP", 1),
        ("SETTINGS", 2),
        ("STATS", 3),
        ("QUIT", 4),
    ];

    let mut layers: Vec<View> = Vec::new();
    for (label, index) in LABELS {
        let gy = 72.0 + index as f32 * 24.0;
        // CO-OP and STATS have no backend in this port yet: c_uidark, inert.
        let available = matches!(index, 0 | 2 | 4);
        let color = if !available {
            col(64, 64, 64)
        } else if st.main_menu_hover == index {
            col(255, 255, 255)
        } else {
            col(153, 153, 153)
        };

        let a = actions.clone();
        layers.push(
            Row(Modifier::new()
                .fill_max_size()
                .padding_values(PaddingValues {
                    left: v.ox,
                    right: 0.0,
                    top: v.oy + (gy - 10.0) * v.s,
                    bottom: 0.0,
                })
                .align_items(AlignItems::FLEX_START))
            .child(
                Column(
                    Modifier::new()
                        .width(320.0 * v.s)
                        .height(20.0 * v.s)
                        .align_items(AlignItems::CENTER)
                        .clickable_ext(available, None, None, move || match index {
                            0 => push(&a, UiAction::MainMenuPlay),
                            2 => push(&a, UiAction::OpenSettings),
                            4 => push(&a, UiAction::QuitApp),
                            _ => {}
                        }),
                )
                .child(
                    RText(label)
                        .size((14.0 * v.s).clamp(10.0, 160.0))
                        .font_family("Silkscreen")
                        .color(color)
                        .single_line(),
                ),
            ),
        );
    }

    ZStack(Modifier::new().fill_max_size()).child(layers)
}

/// Original HUD text pass — everything scrDrawPlayerHUD draws as text,
/// placed in NT GUI coordinates scaled into window space. Sprite art
/// (health bar, fills, rad meter, ammo/weapon icons) lives in ui_art.rs.
fn nt_hud_overlay(st: &SharedUi) -> View {
    let v = nt_view(st);
    let mut layers: Vec<View> = Vec::new();

    // Health string, centred at gui (67, 7).
    layers.push(nt_text_at(
        format!("{}/{}", st.hp.max(0), st.max_hp.max(0)),
        67.0,
        7.0,
        &v,
        col(255, 255, 255),
        true,
    ));

    // Level number centred at gui (11, 16) until ultra.
    if st.level < 99 {
        layers.push(nt_text_at(
            st.level.to_string(),
            11.0,
            16.0,
            &v,
            col(255, 255, 255),
            true,
        ));
    }

    // Ammo counts left-aligned at (dx + 18, dy + 5) per weapon slot; the
    // stored weapon renders in silver (c_silver) like upstream.
    for slot in 0..2usize {
        let amount = st.weapon_ammo[slot];
        let color = if slot == st.current_weapon {
            col(255, 255, 255)
        } else {
            col(192, 192, 192)
        };
        layers.push(nt_text_at(
            amount.to_string(),
            42.0 + slot as f32 * 44.0,
            21.0,
            &v,
            color,
            false,
        ));
    }

    // LOW HP warning at gui (110, 7), red.
    if st.hp <= 4 && st.hp != st.max_hp {
        layers.push(nt_text_at(
            "LOW HP".to_string(),
            110.0,
            7.0,
            &v,
            col(255, 60, 40),
            false,
        ));
    }

    ZStack(Modifier::new().fill_max_size()).child(layers)
}

/// Window-space mapping of the 320x240 NT GUI surface: uniform pixel scale
/// plus centered letterbox offsets. Matches ui_art::GuiMap exactly and, like
/// GameMaker's GUI layer, is independent of gameplay camera zoom.
pub(crate) struct NtView {
    pub s: f32,
    pub ox: f32,
    pub oy: f32,
}

pub(crate) fn nt_view(st: &SharedUi) -> NtView {
    let w = if st.viewport_width > 1.0 {
        st.viewport_width
    } else {
        1280.0
    };
    let h = if st.viewport_height > 1.0 {
        st.viewport_height
    } else {
        720.0
    };
    let s = (w / 320.0).min(h / 240.0);
    NtView {
        s,
        ox: (w - 320.0 * s) * 0.5,
        oy: (h - 240.0 * s) * 0.5,
    }
}

/// One text layer anchored at NT GUI coords. Centred texts sit in a box
/// whose centre is the anchor; left texts run out to the right.
fn nt_text_at(text: String, gx: f32, gy: f32, v: &NtView, color: RColor, centered: bool) -> View {
    let box_w = if centered {
        (2.0 * gx * v.s).max(1.0)
    } else {
        200.0 * v.s
    };
    Column(
        Modifier::new()
            .fill_max_size()
            .padding_values(PaddingValues {
                left: v.ox,
                right: 0.0,
                top: v.oy + gy * v.s,
                bottom: 0.0,
            })
            .align_items(AlignItems::FLEX_START),
    )
    .child(
        Column(Modifier::new().width(box_w).align_items(if centered {
            AlignItems::CENTER
        } else {
            AlignItems::FLEX_START
        }))
        .child(
            RText(text)
                .size((7.0 * v.s).clamp(8.0, 96.0))
                .font_family("Silkscreen")
                .color(color)
                .single_line(),
        ),
    )
}
