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

fn ingame_hud(st: &SharedUi, _actions: Arc<Mutex<Vec<UiAction>>>) -> View {
    let tr = &st.translations;

    // Weapon slots row — active slot highlighted, ammo count beside each.
    let mut weapon_chips: Vec<View> = Vec::new();
    for (i, name) in st.weapons.iter().enumerate() {
        let active = i == st.current_weapon;
        weapon_chips.push(reward_chip(
            format!("{} {}", if active { "▶" } else { " " }, name),
            if active {
                RColor::from_rgba(255, 210, 120, 60)
            } else {
                RColor::from_rgba(255, 255, 255, 18)
            },
            if active {
                col(255, 220, 150)
            } else {
                col(170, 175, 190)
            },
        ));
    }
    let weapons_row =
        Row(Modifier::new().gap(6.0).align_items(AlignItems::CENTER)).child(weapon_chips);

    // Ammo type counters (upstream icon row: bullets/shells/bolts/explo/energy).
    const AMMO_LABELS: [&str; 5] = ["B", "S", "B", "E", "N"];
    let mut ammo_chips: Vec<View> = Vec::new();
    for (i, label) in AMMO_LABELS.iter().enumerate() {
        let count = st.ammo[i + 1];
        let tint = match i {
            0 => col(230, 200, 90),
            1 => col(220, 120, 60),
            2 => col(150, 200, 230),
            3 => col(210, 210, 100),
            _ => col(140, 220, 250),
        };
        ammo_chips.push(reward_chip(
            format!("{label} {}", count.max(0)),
            RColor::from_rgba(255, 255, 255, 14),
            tint,
        ));
    }
    let ammo_row = Row(Modifier::new().gap(5.0)).child(ammo_chips);

    // Ability status chip.
    let ability_chip = reward_chip(
        format!(
            "{} {}",
            st.ability,
            if st.ability_ready { "READY" } else { "..." }
        ),
        if st.ability_ready {
            RColor::from_rgba(120, 220, 130, 40)
        } else {
            RColor::from_rgba(255, 255, 255, 14)
        },
        if st.ability_ready {
            col(120, 220, 130)
        } else {
            col(150, 155, 168)
        },
    );

    // Player panel (top-left): HP, weapons, ammo, ability.
    let player_panel = Column(
        Modifier::new()
            .width(300.0)
            .margin(36.0)
            .padding(12.0)
            .gap(8.0)
            .background(RColor::from_rgba(10, 12, 18, 215))
            .border(1.5, RColor::from_rgba(255, 210, 120, 50), 14.0)
            .clip_rounded(14.0)
            .align_items(AlignItems::STRETCH),
    )
    .child((
        Row(Modifier::new()
            .justify_content(JustifyContent::SPACE_BETWEEN)
            .align_items(AlignItems::CENTER))
        .child((
            reward_chip(
                &st.character,
                RColor::from_rgba(120, 170, 255, 40),
                col(150, 190, 255),
            ),
            ability_chip,
        )),
        weapons_row,
        ammo_row,
    ));

    // Run info panel (top-right): floor/world/loop, rads, score.
    let run_panel = Column(
        Modifier::new()
            .width(190.0)
            .padding(12.0)
            .gap(6.0)
            .background(RColor::from_rgba(10, 12, 18, 215))
            .border(1.5, RColor::from_rgba(120, 170, 255, 45), 14.0)
            .clip_rounded(14.0)
            .align_items(AlignItems::FLEX_END),
    )
    .child((
        RText(format!("FLOOR {}-{}", st.world, st.floor_in_world))
            .size(20.0)
            .color(RColor::WHITE),
        reward_chip(
            format!("{} {}", t(tr, "score", "Score"), st.score),
            RColor::from_rgba(255, 255, 255, 14),
            col(190, 195, 210),
        ),
        reward_chip(
            format!("{} {}", t(tr, "best", "Best"), st.high_score),
            RColor::from_rgba(255, 255, 255, 10),
            col(150, 155, 168),
        ),
    ));

    // Boss bar (top-center) when a boss is alive.
    let boss_view = if st.boss_max > 0 {
        let pct = (st.boss_hp as f32 / st.boss_max as f32).clamp(0.0, 1.0);
        Column(
            Modifier::new()
                .width(420.0)
                .padding(8.0)
                .gap(4.0)
                .background(RColor::from_rgba(14, 10, 16, 210))
                .border(1.5, RColor::from_rgba(200, 120, 255, 55), 12.0)
                .clip_rounded(12.0),
        )
        .child((
            RText("BOSS").size(10.0).color(col(200, 160, 230)),
            hud_stat_bar(404.0, 10.0, pct, col(200, 110, 255)),
        ))
    } else {
        Column(Modifier::new().width(0.0).height(0.0))
    };

    // Bottom-center hint + toast.
    let hint = Column(
        Modifier::new()
            .padding_values(PaddingValues {
                left: 14.0,
                right: 14.0,
                top: 6.0,
                bottom: 6.0,
            })
            .background(RColor::from_rgba(8, 8, 12, 150))
            .clip_rounded(999.0),
    )
    .child(
        RText(t(
            tr,
            "controls-hint",
            "WASD move | Mouse aim | LMB shoot | 1/2 swap | E ability | Esc pause",
        ))
        .size(12.0)
        .color(col(150, 155, 168)),
    );

    let toast_view = if st.toast_timer > 0.0 && !st.toast.is_empty() {
        let a = ((st.toast_timer.clamp(0.0, 1.0)) * 255.0) as u8;
        Column(
            Modifier::new()
                .padding_values(PaddingValues {
                    left: 16.0,
                    right: 16.0,
                    top: 8.0,
                    bottom: 8.0,
                })
                .background(RColor::from_rgba(18, 18, 28, a))
                .clip_rounded(10.0),
        )
        .child(
            RText(&st.toast)
                .size(18.0)
                .color(RColor::from_rgba(255, 230, 150, a)),
        )
    } else {
        Column(Modifier::new().width(0.0).height(0.0))
    };

    ZStack(Modifier::new().fill_max_size()).child((
        Column(
            Modifier::new()
                .fill_max_size()
                .padding(14.0)
                .align_items(AlignItems::FLEX_START)
                .justify_content(JustifyContent::FLEX_START),
        )
        .child(player_panel),
        Column(
            Modifier::new()
                .fill_max_size()
                .padding(14.0)
                .align_items(AlignItems::FLEX_END)
                .justify_content(JustifyContent::FLEX_START),
        )
        .child(run_panel),
        Column(
            Modifier::new()
                .fill_max_size()
                .padding_values(PaddingValues {
                    left: 14.0,
                    right: 14.0,
                    top: 14.0,
                    bottom: 0.0,
                })
                .align_items(AlignItems::CENTER),
        )
        .child(boss_view),
        Column(
            Modifier::new()
                .fill_max_size()
                .padding(12.0)
                .align_items(AlignItems::CENTER)
                .justify_content(JustifyContent::FLEX_END)
                .gap(8.0),
        )
        .child((toast_view, hint)),
    ))
}

fn mutation_panel(st: &SharedUi, actions: Arc<Mutex<Vec<UiAction>>>) -> View {
    let mut rows: Vec<View> = vec![
        RText("CHOOSE A MUTATION").size(30.0).color(RColor::WHITE),
        spacer(6.0),
        RText("(or press 1 / 2 / 3)")
            .size(14.0)
            .color(col(170, 170, 170)),
        spacer(12.0),
    ];
    for (i, choice) in st.mutation_choices.iter().enumerate() {
        let a = actions.clone();
        let text = choice.clone();
        rows.push(mk_button_colored(
            &format!("{}  —  {}", i + 1, text),
            col(70, 120, 90),
            move || push(&a, UiAction::PickMutation(i)),
        ));
    }

    let panel = Column(
        Modifier::new()
            .width(520.0)
            .padding(24.0)
            .background(col(18, 18, 26))
            .clip_rounded(14.0)
            .align_items(AlignItems::CENTER),
    )
    .child(rows);

    Column(
        Modifier::new()
            .fill_max_size()
            .justify_content(JustifyContent::CENTER)
            .align_items(AlignItems::CENTER)
            .background(RColor::from_rgba(0, 0, 0, 160)),
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
