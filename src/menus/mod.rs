use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use std::rc::Rc;

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

use crate::app::{AppState, OverlayMenu, SharedUi};
use crate::game::content::{CHARACTERS, character_def};

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
            let mut children: Vec<View> = vec![hud];
            if st.game_over {
                children.push(game_over_panel(&st, actions.clone()));
            } else if !st.mutation_choices.is_empty() {
                children.push(mutation_panel(&st, actions.clone()));
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

    // Bottom character slots like public-rewrite CharSelect at view_height - LETTERBOX_SIZE
    let mut char_row: Vec<View> = Vec::new();
    for (i, cid) in CHARACTERS.iter().enumerate() {
        let a = actions.clone();
        let def = character_def(*cid);
        let selected = st.selected_character == i;
        // Slot 20px step, 8px start — matches Menu/Create_0.gml _slot_step_size
        let bg = if selected {
            col(240, 210, 110)
        } else {
            col(40, 40, 55)
        };
        let border = if selected {
            col(255, 255, 200)
        } else {
            col(80, 80, 100)
        };
        char_row.push(
            Column(
                Modifier::new()
                    .width(56.0)
                    .height(56.0)
                    .background(bg)
                    .border(2.0, border, 4.0)
                    .clip_rounded(4.0)
                    .justify_content(JustifyContent::CENTER)
                    .align_items(AlignItems::CENTER)
                    .margin(2.0),
            )
            .child(RText(&def.name[..1.min(def.name.len())]).size(28.0).color(col(20, 20, 30))),
        );
        // Clickable overlay
        let _ = a;
        char_row.push(
            Column(Modifier::new().width(1.0).height(1.0)).child(RText("").size(1.0).color(RColor::WHITE)),
        );
    }
    // Make row clickable via invisible buttons overlay — use colored buttons as fallback when sprCharSelect not extracted
    let mut char_buttons: Vec<View> = Vec::new();
    for (i, cid) in CHARACTERS.iter().enumerate() {
        let a = actions.clone();
        let def = character_def(*cid);
        let selected = st.selected_character == i;
        char_buttons.push(mk_button_colored(
            def.name,
            if selected {
                col(90, 170, 110)
            } else {
                col(60, 60, 80)
            },
            move || push(&a, UiAction::SelectCharacter(i)),
        ));
    }

    Column(
        Modifier::new()
            .fill_max_size()
            .justify_content(JustifyContent::SPACE_BETWEEN)
            .align_items(AlignItems::CENTER)
            .background(col(10, 8, 16))
            .padding(16.0),
    )
    .child(vec![
        Column(
            Modifier::new()
                .fill_max_size()
                .justify_content(JustifyContent::CENTER)
                .align_items(AlignItems::CENTER),
        )
        .child(vec![
            RText(t(tr, "app-title", "NUCLEAR THRONE"))
                .size(48.0)
                .color(col(240, 210, 110)),
            RText("MOBILE REBUILD  •  SELECT MUTANT")
                .size(11.0)
                .color(col(140, 140, 160)),
            spacer(8.0),
            // GoButton at _slot_x + step +2 like Menu/Create_0.gml:49
            mk_button(
                &t(tr, "start-game", "▶  PLAY"),
                col(80, 160, 100),
                move || push(&a1, UiAction::StartGame),
            ),
            spacer(8.0),
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
                    let a = a4.clone();
                    move || push(&a, UiAction::QuitApp)
                }),
            ]),
        ]),
        Column(
            Modifier::new()
                .width(320.0)
                .height(72.0)
                .justify_content(JustifyContent::CENTER)
                .align_items(AlignItems::CENTER),
        )
        .child(vec![
            RText("MUTANTS")
                .size(10.0)
                .color(col(120, 120, 140)),
            Row(Modifier::new().gap(4.0).justify_content(JustifyContent::CENTER)).child(char_buttons),
        ]),
    ])
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

    // Exact 88x14 health bar like scrDrawPlayerHUD (sprHealthBar 88x14 at 20,4)
    let hp_pct = (st.hp as f32 / st.max_hp.max(1) as f32).clamp(0.0, 1.0);
    let health_bar = Column(
        Modifier::new()
            .width(88.0)
            .height(14.0)
            .background(col(30, 30, 38))
            .clip_rounded(2.0)
            .padding(1.0),
    )
    .child(Column(
        Modifier::new()
            .width(84.0 * hp_pct)
            .height(8.0)
            .background(col(210, 60, 60))
            .clip_rounded(1.0),
    ));

    let current = if st.current_weapon == 0 {
        &st.weapon1
    } else {
        &st.weapon2
    };

    let top_left = Column(
        Modifier::new()
            .padding(8.0)
            .align_items(AlignItems::FLEX_START)
            .justify_content(JustifyContent::FLEX_START),
    )
    .child((
        health_bar,
        spacer(4.0),
        RText(format!("{} | {}", st.character, current))
            .size(14.0)
            .color(RColor::WHITE),
        RText(format!(
            "A:{} S:{} B:{} E:{}",
            st.ammo[0], st.ammo[1], st.ammo[2], st.ammo[3]
        ))
        .size(12.0)
        .color(col(200, 200, 210)),
        RText(format!(
            "{}: {}",
            st.ability,
            if st.ability_ready { "READY" } else { "..." }
        ))
        .size(11.0)
        .color(if st.ability_ready {
            col(120, 220, 130)
        } else {
            col(150, 150, 150)
        }),
    ));

    let top_right = Column(
        Modifier::new()
            .padding(8.0)
            .align_items(AlignItems::FLEX_END)
            .justify_content(JustifyContent::FLEX_START),
    )
    .child((
        RText(format!(
            "FLOOR {}-{} LV{}",
            st.world, st.floor_in_world, st.level
        ))
        .size(13.0)
        .color(RColor::WHITE),
        RText(format!("RADS {}", st.rads))
            .size(12.0)
            .color(col(120, 240, 120)),
        RText(format!("{}: {}", t(tr, "score", "Score"), st.score))
            .size(11.0)
            .color(col(220, 220, 220)),
        RText(format!("{}: {}", t(tr, "best", "Best"), st.high_score))
            .size(10.0)
            .color(col(170, 170, 170)),
        RText(format!("BEST FLOOR {}", st.best_floor))
            .size(10.0)
            .color(col(150, 150, 150)),
    ));

    let boss_bar = if st.boss_max > 0 {
        let pct = (st.boss_hp as f32 / st.boss_max as f32).clamp(0.0, 1.0);
        Column(
            Modifier::new()
                .width(420.0)
                .height(16.0)
                .background(col(40, 30, 30))
                .clip_rounded(4.0),
        )
        .child(Column(
            Modifier::new()
                .width((420.0 * pct).max(1.0))
                .height(16.0)
                .background(col(200, 50, 50))
                .clip_rounded(4.0)
                .align_self(AlignSelf::FLEX_START),
        ))
    } else {
        Column(Modifier::new().width(1.0).height(1.0))
    };

    let toast_view = if st.toast_timer > 0.0 && !st.toast.is_empty() {
        let a = ((st.toast_timer.clamp(0.0, 1.0)) * 255.0) as u8;
        RText(&st.toast)
            .size(22.0)
            .color(RColor::from_rgba(255, 255, 255, a))
    } else {
        RText("").size(1.0).color(RColor::WHITE)
    };

    ZStack(Modifier::new().fill_max_size()).child((Column(
        Modifier::new()
            .fill_max_size()
            .padding(16.0)
            .align_items(AlignItems::FLEX_START)
            .justify_content(JustifyContent::SPACE_BETWEEN),
    )
    .child((
        Column(
            Modifier::new()
                .fill_max_size()
                .justify_content(JustifyContent::SPACE_BETWEEN),
        )
        .child((top_left, top_right)),
        Column(
            Modifier::new()
                .fill_max_size()
                .justify_content(JustifyContent::CENTER)
                .align_items(AlignItems::CENTER),
        )
        .child(boss_bar),
        Column(
            Modifier::new()
                .fill_max_size()
                .justify_content(JustifyContent::FLEX_END)
                .align_items(AlignItems::CENTER),
        )
        .child((
            toast_view,
            RText(t(
                tr,
                "controls-hint",
                "WASD move | Mouse aim | LMB/Space shoot | 1/2 switch | E ability | Esc pause",
            ))
            .size(13.0)
            .color(col(180, 180, 180)),
        )),
    )),))
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

fn push(actions: &Arc<Mutex<Vec<UiAction>>>, a: UiAction) {
    if let Ok(mut q) = actions.lock() {
        q.push(a);
    }
}
