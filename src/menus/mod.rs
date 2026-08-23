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

fn slide_in_config(key: &str, dx: f32, dy: f32) -> AnimatedVisibilityConfig {
    AnimatedVisibilityConfig {
        key: key.into(),
        spec: AnimationSpec::tween(Duration::from_millis(260), Easing::EaseOut),
        enter: EnterTransition::FadeIn.and(EnterTransition::SlideIn {
            offset_x: dx,
            offset_y: dy,
        }),
        exit: ExitTransition::FadeOut,
    }
}

fn rise_in_config(key: &str) -> AnimatedVisibilityConfig {
    slide_in_config(key, 0.0, 18.0)
}

pub fn compose_root(
    overlay: OverlayHandle,
    st: SharedUi,
    actions: Arc<Mutex<Vec<UiAction>>>,
) -> View {
    let root = ZStack(Modifier::new().fill_max_size());
    let settings_view = settings_ui(overlay, &st, actions.clone());

    let content = match st.phase {
        AppState::Splash => splash_ui(),
        AppState::Loading => loading_ui(&st),
        AppState::Title => ZStack(Modifier::new().fill_max_size()).child((
            title_ui(&st, actions.clone()),
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
            let hud = ingame_hud(&st, actions.clone());
            let hud = AnimatedVisibility(true, hud, slide_in_config("hud_in", 0.0, -14.0));
            let mut children: Vec<View> = vec![hud];
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

fn splash_ui() -> View {
    Column(
        Modifier::new()
            .fill_max_size()
            .justify_content(JustifyContent::CENTER)
            .align_items(AlignItems::CENTER)
            .background(col(8, 8, 12)),
    )
    .child(
        RText("Nuclear Throne (Bevy Recreation)")
            .size(36.0)
            .color(RColor::WHITE),
    )
}

fn loading_ui(st: &SharedUi) -> View {
    let pct = st.loading_progress.clamp(0.0, 1.0);
    Column(
        Modifier::new()
            .fill_max_size()
            .justify_content(JustifyContent::CENTER)
            .align_items(AlignItems::CENTER)
            .background(col(8, 8, 12)),
    )
    .child(RText("Loading...").size(32.0).color(RColor::WHITE))
    .child(spacer(16.0))
    .child(
        RText(format!("{:.0}%", pct * 100.0))
            .size(18.0)
            .color(RColor::WHITE),
    )
    .child(spacer(12.0))
    .child(
        Column(
            Modifier::new()
                .width(320.0)
                .height(12.0)
                .background(col(30, 30, 38))
                .clip_rounded(6.0),
        )
        .child(Column(
            Modifier::new()
                .width((320.0 * pct).max(1.0))
                .height(12.0)
                .background(col(96, 165, 250))
                .clip_rounded(6.0)
                .align_self(AlignSelf::FLEX_START),
        )),
    )
}

fn title_ui(st: &SharedUi, actions: Arc<Mutex<Vec<UiAction>>>) -> View {
    let a1 = actions.clone();
    let a2 = actions.clone();
    let a3 = actions.clone();
    let a4 = actions.clone();
    let tr = &st.translations;

    // Character grid (2 rows of 8)
    let mut row1: Vec<View> = Vec::new();
    let mut row2: Vec<View> = Vec::new();
    for (i, race) in PLAYABLE_RACES.iter().enumerate() {
        let def = character_def(*race);
        let label = def.name;
        // Short label for button (first word)
        let short = label.split_whitespace().next().unwrap_or(label);
        let a = actions.clone();
        // Highlight selected via btn content but keep helper simple
        let btn = mk_button_sm(short, move || push(&a, UiAction::SelectCharacter(i)));
        if i < 8 {
            row1.push(btn);
        } else {
            row2.push(btn);
        }
    }
    let sel_idx = st.selected_character.min(PLAYABLE_RACES.len() - 1);
    let sel_def = character_def(PLAYABLE_RACES[sel_idx]);
    let sel_ability = crate::game::content::ability_name(sel_def.ability);
    let sel_line = format!(
        "▶ {}  •  {}  •  {}",
        sel_def.name, sel_ability, st.loadout_summary
    );

    // Loadout cycle buttons
    let a_sw_prev = actions.clone();
    let a_sw_next = actions.clone();
    let a_st_prev = actions.clone();
    let a_st_next = actions.clone();
    let a_cr_prev = actions.clone();
    let a_cr_next = actions.clone();

    Column(
        Modifier::new()
            .fill_max_size()
            .justify_content(JustifyContent::CENTER)
            .align_items(AlignItems::CENTER)
            .background(RColor::from_rgba(10, 8, 16, 120)),
    )
    .child(AnimatedVisibility(
        true,
        Column(
            Modifier::new()
                .width(620.0)
                .padding(24.0)
                .background(RColor::from_rgba(8, 8, 14, 150))
                .clip_rounded(18.0)
                .border(2.0, col(90, 90, 110), 18.0)
                .align_items(AlignItems::CENTER)
                .gap(6.0),
        )
        .child(vec![
            RText(t(tr, "app-title", "NUCLEAR THRONE"))
                .size(40.0)
                .color(col(240, 210, 110)),
            spacer(2.0),
            reward_chip(
                format!(
                    "{} {} • {} {}",
                    t(tr, "score", "Score"),
                    st.high_score,
                    t(tr, "best", "Best"),
                    st.best_floor
                ),
                RColor::from_rgba(255, 255, 255, 16),
                col(190, 195, 210),
            ),
            spacer(6.0),
            RText("SELECT CHARACTER")
                .size(12.0)
                .color(col(150, 155, 168)),
            Row(Modifier::new().gap(4.0)).child(row1),
            Row(Modifier::new().gap(4.0)).child(row2),
            reward_chip(
                sel_line,
                RColor::from_rgba(120, 170, 255, 30),
                col(180, 200, 255),
            ),
            spacer(4.0),
            RText(format!(
                "Start: {}  •  Stored: {}  •  Crown: {}",
                st.start_weapon_name, st.stored_weapon_name, st.crown
            ))
            .size(11.0)
            .color(col(170, 175, 190)),
            Row(Modifier::new().gap(6.0)).child(vec![
                mk_button_sm("S-◀", move || {
                    push(&a_sw_prev, UiAction::CycleStartWeapon(-1))
                }),
                mk_button_sm("S-▶", move || {
                    push(&a_sw_next, UiAction::CycleStartWeapon(1))
                }),
                mk_button_sm("T-◀", move || {
                    push(&a_st_prev, UiAction::CycleStoredWeapon(-1))
                }),
                mk_button_sm("T-▶", move || {
                    push(&a_st_next, UiAction::CycleStoredWeapon(1))
                }),
                mk_button_sm("C◀", move || push(&a_cr_prev, UiAction::CycleCrown(-1))),
                mk_button_sm("C▶", move || push(&a_cr_next, UiAction::CycleCrown(1))),
            ]),
            spacer(10.0),
            mk_button(
                &t(tr, "start-game", "▶  PLAY"),
                col(80, 160, 100),
                move || push(&a1, UiAction::StartGame),
            ),
            Row(Modifier::new().gap(6.0)).child(vec![
                mk_button_sm(&t(tr, "settings", "Settings"), {
                    let a = a2.clone();
                    move || push(&a, UiAction::OpenSettings)
                }),
                mk_button_sm(&t(tr, "credits", "Credits"), {
                    let a = a3.clone();
                    move || push(&a, UiAction::OpenCredits)
                }),
                mk_button_sm(&t(tr, "quit", "Quit"), {
                    let a = a4;
                    move || push(&a, UiAction::QuitApp)
                }),
            ]),
        ]),
        rise_in_config("title_card"),
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
    .child(pause_panel(tr, a1, a2, a3))
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
    .child(inner)
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
    .child(inner)
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

fn nt_panel(width: f32, accent: RColor, padding: f32, child: View) -> View {
    Column(
        Modifier::new()
            .width(width)
            .padding(padding)
            .background(NT_PANEL)
            .border(2.0, accent, 3.0)
            .clip_rounded(3.0)
            .align_items(AlignItems::STRETCH),
    )
    .child(child)
}

fn nt_section(child: View) -> View {
    Column(
        Modifier::new()
            .padding_values(PaddingValues {
                left: 6.0,
                right: 6.0,
                top: 5.0,
                bottom: 5.0,
            })
            .background(NT_PANEL_INNER)
            .border(1.0, NT_BORDER, 2.0)
            .clip_rounded(2.0),
    )
    .child(child)
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

fn nt_bar_inner_width(width: f32, fraction: f32) -> f32 {
    let usable = (width - 4.0).max(0.001);
    let fraction = fraction.clamp(0.0, 1.0);

    if fraction <= 0.0 {
        0.001
    } else {
        (usable * fraction).max(1.0)
    }
}

fn nt_bar(width: f32, height: f32, fraction: f32, fill: RColor) -> View {
    let inner_width = nt_bar_inner_width(width, fraction);

    Column(
        Modifier::new()
            .width(width)
            .height(height)
            .padding(2.0)
            .background(NT_TRACK)
            .border(1.0, NT_BORDER, 2.0)
            .clip_rounded(2.0)
            .align_items(AlignItems::FLEX_START),
    )
    .child(Column(
        Modifier::new()
            .width(inner_width)
            .height((height - 4.0).max(1.0))
            .background(fill),
    ))
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

fn weapon_slot_view(index: usize, name: &str, active: bool, metrics: HudMetrics) -> View {
    let marker = if active { ">" } else { " " };
    let fg = if active { NT_GOLD } else { NT_MUTED };
    let bg = if active {
        RColor::from_rgba(245, 210, 92, 34)
    } else {
        RColor::from_rgba(255, 255, 255, 8)
    };

    Row(Modifier::new()
        .height(if metrics.small_text < 10.0 {
            20.0
        } else {
            23.0
        })
        .padding_values(PaddingValues {
            left: 5.0,
            right: 5.0,
            top: 2.0,
            bottom: 2.0,
        })
        .background(bg)
        .border(
            1.0,
            if active {
                RColor::from_rgba(245, 210, 92, 88)
            } else {
                RColor::from_rgba(255, 255, 255, 12)
            },
            2.0,
        )
        .align_items(AlignItems::CENTER)
        .justify_content(JustifyContent::SPACE_BETWEEN))
    .child((
        RText(format!("{marker} {}", index + 1))
            .size(metrics.small_text)
            .color(fg)
            .single_line(),
        RText(name.to_ascii_uppercase())
            .size(metrics.small_text)
            .color(fg)
            .single_line()
            .overflow_ellipsize(),
    ))
}

fn ammo_cell(label: &str, amount: i32, tint: RColor, compact: bool) -> View {
    Column(
        Modifier::new()
            .width(if compact { 40.0 } else { 50.0 })
            .height(if compact { 31.0 } else { 35.0 })
            .padding(3.0)
            .background(RColor::from_rgba(255, 255, 255, 8))
            .border(1.0, RColor::from_rgba(255, 255, 255, 18), 2.0)
            .clip_rounded(2.0)
            .align_items(AlignItems::CENTER)
            .justify_content(JustifyContent::CENTER),
    )
    .child((
        RText(label)
            .size(if compact { 7.0 } else { 8.0 })
            .color(NT_MUTED)
            .single_line(),
        RText(amount.max(0).to_string())
            .size(if compact { 11.0 } else { 13.0 })
            .color(tint)
            .single_line(),
    ))
}

fn ingame_hud(st: &SharedUi, _actions: Arc<Mutex<Vec<UiAction>>>) -> View {
    let metrics = hud_metrics(st.hud_compact);
    let compact = st.hud_compact;

    let hp_fraction = if st.max_hp <= 0 {
        0.0
    } else {
        st.hp.max(0) as f32 / st.max_hp as f32
    };

    let rad_fraction = if st.max_rads == 0 {
        0.0
    } else {
        st.rads as f32 / st.max_rads as f32
    };

    let weapon_views = st
        .weapons
        .iter()
        .enumerate()
        .map(|(index, name)| weapon_slot_view(index, name, index == st.current_weapon, metrics))
        .collect::<Vec<_>>();

    const AMMO: [(&str, usize, RColor); 5] = [
        ("BUL", 1, RColor(238, 205, 82, 255)),
        ("SHL", 2, RColor(224, 121, 54, 255)),
        ("BLT", 3, RColor(130, 196, 225, 255)),
        ("EXP", 4, RColor(218, 204, 82, 255)),
        ("NRG", 5, RColor(102, 219, 238, 255)),
    ];

    let ammo_views = AMMO
        .iter()
        .map(|(label, slot, tint)| {
            ammo_cell(
                label,
                st.ammo.get(*slot).copied().unwrap_or_default(),
                *tint,
                compact,
            )
        })
        .collect::<Vec<_>>();

    let header = Row(Modifier::new()
        .align_items(AlignItems::CENTER)
        .justify_content(JustifyContent::SPACE_BETWEEN))
    .child((
        RText(st.character.to_ascii_uppercase())
            .size(metrics.normal_text)
            .color(NT_TEXT)
            .single_line()
            .overflow_ellipsize(),
        if st.crown != "NONE" && !st.crown.is_empty() {
            nt_chip(
                format!("CROWN {}", st.crown),
                RColor(245, 210, 92, 24),
                NT_GOLD,
                metrics.small_text,
            )
        } else {
            empty_view()
        },
    ));

    let health = nt_section(
        Column(Modifier::new().gap(4.0)).child((
            Row(Modifier::new()
                .align_items(AlignItems::CENTER)
                .justify_content(JustifyContent::SPACE_BETWEEN))
            .child((
                RText("HP")
                    .size(metrics.small_text)
                    .color(NT_MUTED)
                    .single_line(),
                RText(format!("{}/{}", st.hp.max(0), st.max_hp.max(0)))
                    .size(metrics.normal_text)
                    .color(hp_fill_color(st.hp, st.max_hp))
                    .single_line(),
            )),
            nt_bar(
                metrics.hp_bar_width,
                if compact { 10.0 } else { 12.0 },
                hp_fraction,
                hp_fill_color(st.hp, st.max_hp),
            ),
        )),
    );

    let weapons = nt_section(Column(Modifier::new().gap(3.0)).child(weapon_views));

    let ammo = Row(Modifier::new()
        .gap(if compact { 3.0 } else { 5.0 })
        .align_items(AlignItems::CENTER))
    .child(ammo_views);

    let ability = nt_section(
        Row(Modifier::new()
            .align_items(AlignItems::CENTER)
            .justify_content(JustifyContent::SPACE_BETWEEN))
        .child((
            RText(st.ability.to_ascii_uppercase())
                .size(metrics.small_text)
                .color(NT_TEXT)
                .single_line()
                .overflow_ellipsize(),
            nt_chip(
                if st.ability_ready { "READY" } else { "WAIT" },
                if st.ability_ready {
                    RColor(72, 202, 96, 28)
                } else {
                    RColor(255, 255, 255, 8)
                },
                if st.ability_ready { NT_GREEN } else { NT_MUTED },
                metrics.small_text,
            ),
        )),
    );

    let player_panel = nt_panel(
        metrics.player_width,
        RColor(245, 210, 92, 100),
        metrics.panel_padding,
        Column(Modifier::new().gap(6.0)).child((header, health, weapons, ammo, ability)),
    );

    let floor_title = if st.loop_count > 0 {
        format!("{}-{}  LOOP {}", st.world, st.floor_in_world, st.loop_count)
    } else {
        format!("{}-{}", st.world, st.floor_in_world)
    };

    let run_panel = nt_panel(
        metrics.run_width,
        RColor(77, 151, 230, 95),
        metrics.panel_padding,
        Column(Modifier::new().gap(6.0).align_items(AlignItems::STRETCH)).child((
            RText(floor_title)
                .size(metrics.normal_text + 1.0)
                .color(NT_TEXT)
                .single_line(),
            nt_section(
                Column(Modifier::new().gap(3.0)).child((
                    Row(Modifier::new()
                        .justify_content(JustifyContent::SPACE_BETWEEN)
                        .align_items(AlignItems::CENTER))
                    .child((
                        RText(format!("LEVEL {}", st.level))
                            .size(metrics.small_text)
                            .color(NT_GREEN)
                            .single_line(),
                        RText(format!("{}/{}", st.rads, st.max_rads))
                            .size(metrics.small_text)
                            .color(NT_MUTED)
                            .single_line(),
                    )),
                    nt_bar(
                        (metrics.run_width - metrics.panel_padding * 2.0 - 14.0).max(1.0),
                        8.0,
                        rad_fraction,
                        NT_GREEN,
                    ),
                )),
            ),
            Row(Modifier::new().justify_content(JustifyContent::SPACE_BETWEEN)).child((
                RText("SCORE").size(metrics.small_text).color(NT_MUTED),
                RText(st.score.to_string())
                    .size(metrics.normal_text)
                    .color(NT_GOLD),
            )),
        )),
    );

    let boss_view = if st.boss_max > 0 {
        let fraction = st.boss_hp as f32 / st.boss_max.max(1) as f32;

        nt_panel(
            metrics.boss_width,
            RColor(181, 86, 229, 120),
            if compact { 7.0 } else { 9.0 },
            Column(Modifier::new().gap(4.0)).child((
                Row(Modifier::new()
                    .align_items(AlignItems::CENTER)
                    .justify_content(JustifyContent::SPACE_BETWEEN))
                .child((
                    RText(boss_display_name(&st.boss_name))
                        .size(if compact { 11.0 } else { 13.0 })
                        .color(NT_TEXT)
                        .single_line()
                        .overflow_ellipsize(),
                    RText(format!("{}/{}", st.boss_hp, st.boss_max))
                        .size(if compact { 9.0 } else { 11.0 })
                        .color(NT_PURPLE)
                        .single_line(),
                )),
                nt_bar(
                    (metrics.boss_width - if compact { 28.0 } else { 36.0 }).max(1.0),
                    if compact { 9.0 } else { 11.0 },
                    fraction,
                    NT_PURPLE,
                ),
            )),
        )
    } else {
        empty_view()
    };

    let toast = if st.toast_timer > 0.0 && !st.toast.is_empty() {
        let alpha = (st.toast_timer.clamp(0.0, 1.0) * 255.0) as u8;

        Column(
            Modifier::new()
                .padding_values(PaddingValues {
                    left: 14.0,
                    right: 14.0,
                    top: 7.0,
                    bottom: 7.0,
                })
                .background(RColor(7, 8, 11, alpha))
                .border(1.0, RColor(245, 210, 92, alpha / 2), 3.0)
                .clip_rounded(3.0),
        )
        .child(
            RText(st.toast.to_ascii_uppercase())
                .size(if compact { 13.0 } else { 17.0 })
                .color(RColor(255, 227, 135, alpha))
                .single_line()
                .overflow_ellipsize(),
        )
    } else {
        empty_view()
    };

    let controls = if compact {
        empty_view()
    } else {
        nt_chip(
            "WASD MOVE  |  MOUSE AIM  |  LMB FIRE  |  1/2 SWAP  |  E ABILITY",
            RColor(0, 0, 0, 130),
            NT_MUTED,
            10.0,
        )
    };

    let player_anchor = Column(
        Modifier::new()
            .fill_max_size()
            .padding(metrics.margin)
            .align_items(AlignItems::FLEX_START)
            .justify_content(JustifyContent::FLEX_START),
    )
    .child(player_panel);

    let run_anchor = Column(
        Modifier::new()
            .fill_max_size()
            .padding(metrics.margin)
            .align_items(AlignItems::FLEX_END)
            .justify_content(if compact {
                JustifyContent::FLEX_END
            } else {
                JustifyContent::FLEX_START
            }),
    )
    .child(run_panel);

    let boss_anchor = if compact {
        Column(
            Modifier::new()
                .fill_max_size()
                .padding_values(PaddingValues {
                    left: metrics.margin,
                    right: metrics.margin,
                    top: 0.0,
                    bottom: 92.0,
                })
                .align_items(AlignItems::CENTER)
                .justify_content(JustifyContent::FLEX_END),
        )
        .child(boss_view)
    } else {
        Column(
            Modifier::new()
                .fill_max_size()
                .padding(metrics.margin)
                .align_items(AlignItems::CENTER)
                .justify_content(JustifyContent::FLEX_START),
        )
        .child(boss_view)
    };

    let feedback_anchor = Column(
        Modifier::new()
            .fill_max_size()
            .padding(metrics.margin)
            .gap(7.0)
            .align_items(AlignItems::CENTER)
            .justify_content(JustifyContent::FLEX_END),
    )
    .child((toast, controls));

    ZStack(Modifier::new().fill_max_size()).child((
        player_anchor,
        run_anchor,
        boss_anchor,
        feedback_anchor,
    ))
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
            .padding(if st.hud_compact { 6.0 } else { 20.0 })
            .justify_content(JustifyContent::CENTER)
            .align_items(AlignItems::CENTER)
            .background(RColor(0, 0, 0, 188)),
    )
    .child(panel)
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
    .child(panel)
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
    fn bar_width_clamps_low() {
        assert_eq!(nt_bar_inner_width(100.0, -4.0), 0.001);
        assert_eq!(nt_bar_inner_width(100.0, 0.0), 0.001);
    }

    #[test]
    fn bar_width_clamps_high() {
        let full = nt_bar_inner_width(100.0, 1.0);
        let over = nt_bar_inner_width(100.0, 8.0);

        assert!((full - 96.0).abs() < 0.001);
        assert!((over - 96.0).abs() < 0.001);
    }

    #[test]
    fn bar_width_half_uses_half_inner_track() {
        let width = nt_bar_inner_width(100.0, 0.5);
        assert!((width - 48.0).abs() < 0.001);
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
