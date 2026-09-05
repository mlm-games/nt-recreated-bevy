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
    SetAmbienceVol(f32),
    SaveSettings,
    NextLanguage,
    SetLanguage(String),
    /// MenuOptions navigation: 0 Main, 1 Audio, 2 Video, 3 Game, 4 Controls, 5 Language
    SettingsCategory(u8),
    SettingsBack,
    /// Pause confirmation: 0 MENU -> QuitToTitle, 1 RETRY -> Restart
    ShowPauseConfirm(u8),
    CancelPauseConfirm,
    ConfirmPause(u8),
    SelectCharacter(usize),
    SelectSkin(u8),
    /// Toggle the char-select loadout panel (Menu.loadout_open).
    ToggleLoadout,
    CycleStartWeapon(i8),
    CycleStoredWeapon(i8),
    CycleCrown(i8),
    /// Pick a specific crown slot in the open loadout grid.
    SelectCrown(u8),
    SelectMutation(usize),
    PickMutation(usize),
    /// Toggle boolean setting by GML save key (e.g. "volume_3dsound", "visual_bloom")
    SettingToggle(String),
    /// Set slider 0.0..1.0 (or 0..2 for screenshake) for key
    SettingSlider {
        key: String,
        value: f32,
    },
    /// Cycle list setting (-1/1) for key
    SettingCycle {
        key: String,
        dir: i8,
    },
    /// Direct input string commit (e.g. profile_name, player_color)
    SettingInput {
        key: String,
        value: String,
    },
    /// Reset all options / erase progress disclaimers (from Game_Data)
    SettingResetOptions,
    SettingEraseProgress,
    SettingViewCredits,
    SettingOpenSubcategory(u8),
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
        AppState::MainMenu => {
            let mut layers: Vec<View> = vec![main_menu_ui(&st, actions.clone())];
            if st.overlay == OverlayMenu::Settings {
                layers.push(scrim());
            }
            layers.push(AnimatedVisibility(
                st.overlay == OverlayMenu::Settings,
                settings_view.clone(),
                popup_anim_config("menu_settings"),
            ));
            ZStack(Modifier::new().fill_max_size()).child(layers)
        }
        AppState::Title => {
            let mut layers: Vec<View> = vec![title_screen::title_screen(&st, actions.clone())];
            if st.overlay == OverlayMenu::Settings || st.overlay == OverlayMenu::Credits {
                layers.push(scrim());
            }
            layers.push(AnimatedVisibility(
                st.overlay == OverlayMenu::Settings,
                settings_view.clone(),
                popup_anim_config("title_settings"),
            ));
            layers.push(AnimatedVisibility(
                st.overlay == OverlayMenu::Credits,
                credits_ui(&st, actions.clone()),
                popup_anim_config("title_credits"),
            ));
            ZStack(Modifier::new().fill_max_size()).child(layers)
        }
        AppState::InGame => {
            // GenCont / between-floor loading owns the screen completely.
            // Do not draw HUD, pause, mutation, or death UI over it.
            if st.gen_active {
                gen_cont_overlay(&st)
            } else {
                let mut children: Vec<View> = Vec::new();

                if st.show_hud {
                    children.push(nt_hud_overlay(&st));
                }

                if st.game_over {
                    children.push(game_over_panel(&st, actions.clone()));
                } else if !st.mutation_choices.is_empty() {
                    children.push(mutation_panel(&st, actions.clone()));
                }

                if matches!(
                    st.overlay,
                    OverlayMenu::Pause | OverlayMenu::Settings | OverlayMenu::Credits
                ) {
                    children.push(scrim());
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

/// Boot screen: ALL splash content (saving icon, Vlambeer card, logo AND the
/// per-card text lines) is rendered Bevy-side in ui_art.rs so sprites and text
/// share one visibility timeline. Repose renders nothing during Splash.
fn splash_ui(_st: &SharedUi) -> View {
    ZStack(Modifier::new().fill_max_size())
}

const GUI_H_F32: f32 = 240.0;

fn loading_ui(st: &SharedUi) -> View {
    let mut loading = st.clone();
    loading.gen_active = true;
    loading.gen_progress = st.loading_progress.clamp(0.0, 1.0);
    loading.gen_tip.clear();
    gen_cont_overlay(&loading)
}

fn gen_cont_overlay(st: &SharedUi) -> View {
    // GML GenCont/Draw_0 exact:
    //   scrDrawSpiral() is WGSL vortex (already behind)
    //   _progress = instance_number(Floor)/goal
    //   _percentage = string_pad_zeroes(round(progress*100),2)+"%"
    //   _text = loc_fmt("GenCont:Generating","GENERATING... %",pct) or Venuz "VERIFYING... %"
    let v = nt_view(st);
    let pct = st.gen_progress.clamp(0.0, 1.0);
    let pct_text = format!("GENERATING... {}%", (pct * 100.0).round() as u32);
    ZStack(Modifier::new().fill_max_size()).child(
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
            ZStack(Modifier::new().width(320.0 * v.s).height(240.0 * v.s)).child((
                Column(
                    Modifier::new()
                        .fill_max_size()
                        .background(RColor::from_rgba(0, 0, 0, 0)),
                ),
                Column(
                    Modifier::new()
                        .fill_max_size()
                        .padding_values(PaddingValues {
                            left: 0.0,
                            right: 0.0,
                            top: 66.0 * v.s,
                            bottom: 0.0,
                        })
                        .align_items(AlignItems::CENTER),
                )
                .child(
                    RText(pct_text)
                        .size((7.0 * v.s).clamp(8.0, 96.0))
                        .font_family("Silkscreen")
                        .color(col(125, 131, 141))
                        .single_line(),
                ),
                // GML: draw_text_nt(_cx, _cy+24, "@s"+tip) => 144
                Column(
                    Modifier::new()
                        .fill_max_size()
                        .padding_values(PaddingValues {
                            left: 0.0,
                            right: 0.0,
                            top: 144.0 * v.s,
                            bottom: 0.0,
                        })
                        .align_items(AlignItems::CENTER),
                )
                .child(
                    RText(format!("@s{}", st.gen_tip))
                        .size((7.0 * v.s).clamp(8.0, 96.0))
                        .font_family("Silkscreen")
                        .color(col(125, 131, 141))
                        .single_line(),
                ),
                Column(
                    Modifier::new()
                        .fill_max_size()
                        .padding_values(PaddingValues {
                            left: 0.0,
                            right: 0.0,
                            top: 168.0 * v.s,
                            bottom: 0.0,
                        })
                        .align_items(AlignItems::CENTER),
                )
                .child(
                    RText(roadmap_text(st))
                        .size((5.0 * v.s).clamp(7.0, 64.0))
                        .font_family("Silkscreen")
                        .color(col(125, 131, 141))
                        .single_line(),
                ),
            )),
        ),
    )
}

fn roadmap_text(st: &SharedUi) -> String {
    if st.world == 0 && st.floor == 0 {
        return String::new();
    }
    format!("{}-{}  LOOP {}", st.world, st.floor_in_world, st.loop_count)
}

/// Full-screen scrim (~/temp Dialog scrim pattern): rendered as a direct
/// child of the root `ZStack` (definite 800x600), OUTSIDE `AnimatedVisibility`.
/// The inflow wrapper (`fill_max_width`, auto height) collapses `fill_max_size`
/// percent children to h=0, so a dim inside the animated content never paints.
fn scrim() -> View {
    Column(
        Modifier::new()
            .fill_max_size()
            .background(RColor::from_rgba(0, 0, 0, 230))
            .clickable()
            .on_click(|| {}),
    )
}

fn pause_overlay(st: &SharedUi, actions: Arc<Mutex<Vec<UiAction>>>) -> View {
    let v = nt_view(st);
    let mut layers: Vec<View> = Vec::new();
    if st.pause_confirm.is_none() {
        layers.push(nt_text_at(
            "PAUSED".to_string(),
            160.0,
            60.0,
            &v,
            col(238, 239, 225),
            true,
        ));
    }
    if let Some(confirm) = st.pause_confirm {
        // GML confirmation: left=52 right=268 y=192 (bottom-48)
        let left_label = "BACK";
        let right_label = if confirm == 0 { "QUIT" } else { "RETRY" };
        let a_back = actions.clone();
        layers.push(bigname_button_at(
            left_label.to_string(),
            52.0,
            192.0,
            &v,
            col(153, 153, 153),
            move || push(&a_back, UiAction::CancelPauseConfirm),
        ));
        let a_conf = actions.clone();
        let c = confirm;
        layers.push(bigname_button_at(
            right_label.to_string(),
            268.0,
            192.0,
            &v,
            if confirm == 0 {
                col(221, 56, 45)
            } else {
                col(98, 220, 88)
            },
            move || push(&a_conf, UiAction::ConfirmPause(c)),
        ));
        layers.push(nt_text_at(
            "ARE YOU SURE?".to_string(),
            160.0,
            120.0,
            &v,
            col(238, 239, 225),
            true,
        ));
    } else {
        // scrMakePauseButtons: left+45 topRow, left+60 bottomRow, right-68 topRow, right-78 bottomRow
        let a_menu = actions.clone();
        layers.push(bigname_button_at(
            "MENU".to_string(),
            45.0,
            176.0,
            &v,
            col(153, 153, 153),
            move || push(&a_menu, UiAction::ShowPauseConfirm(0)),
        ));
        let a_retry = actions.clone();
        layers.push(bigname_button_at(
            "RETRY".to_string(),
            60.0,
            208.0,
            &v,
            col(153, 153, 153),
            move || push(&a_retry, UiAction::ShowPauseConfirm(1)),
        ));
        let a_settings = actions.clone();
        layers.push(bigname_button_at(
            "SETTINGS".to_string(),
            252.0,
            176.0,
            &v,
            col(153, 153, 153),
            move || push(&a_settings, UiAction::OpenSettings),
        ));
        let a_cont = actions.clone();
        layers.push(bigname_button_at(
            "CONTINUE".to_string(),
            242.0,
            208.0,
            &v,
            col(153, 153, 153),
            move || push(&a_cont, UiAction::Resume),
        ));
    }
    ZStack(Modifier::new().fill_max_size()).child(layers)
}

#[allow(dead_code)]
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

fn settings_ui(_overlay: OverlayHandle, st: &SharedUi, actions: Arc<Mutex<Vec<UiAction>>>) -> View {
    // GML MenuOptions full fidelity: mirrors Other_20 category definitions.
    // Categories: 0 Main, 1 Audio, 2 Video, 3 Video_Display, 4 Game, 5 Game_Profile, 6 Game_Color, 7 Game_Data, 8 Controls, 9 Controls_Remapping, 10 Controls_Prefs, 11 Controls_Experimental, 12 Language
    // Draw mirrors Other_10 generic item loop (slider/switch/list/category/button).
    let v = nt_view(st);
    let mut layers: Vec<View> = Vec::new();
    match st.settings_page {
        0 => {
            layers.push(nt_text_at(
                "OPTIONS".to_string(),
                160.0,
                24.0,
                &v,
                col(153, 153, 153),
                true,
            ));
            let cats: [(&str, u8); 5] = [
                ("AUDIO", 1),
                ("VIDEO", 2),
                ("GAME", 4),
                ("CONTROLS", 8),
                ("LANGUAGE", 12),
            ];
            let start_y = 72.0;
            for (i, (label, idx)) in cats.iter().enumerate() {
                let gy = start_y + i as f32 * 24.0;
                let a = actions.clone();
                let id = *idx;
                layers.push(bigname_button_at(
                    label.to_string(),
                    160.0,
                    gy,
                    &v,
                    col(153, 153, 153),
                    move || push(&a, UiAction::SettingsCategory(id)),
                ));
            }
            let a_back = actions.clone();
            layers.push(bigname_button_at(
                "BACK".to_string(),
                160.0,
                220.0,
                &v,
                col(125, 131, 141),
                move || push(&a_back, UiAction::CloseOverlay),
            ));
        }
        1 => {
            // Audio: GML Audio category exact: Master/Music/Ambience/Sfx + 3dSound
            layers.push(nt_text_at(
                "AUDIO".to_string(),
                160.0,
                24.0,
                &v,
                col(153, 153, 153),
                true,
            ));
            let rows: [(f32, &str, f32, u8); 4] = [
                (56.0, "MASTER VOLUME", st.master_vol, 0),
                (76.0, "MUSIC VOLUME", st.music_vol, 1),
                (96.0, "AMBIENCE VOLUME", st.ambience_vol, 2),
                (116.0, "EFFECTS VOLUME", st.sfx_vol, 3),
            ];
            for (gy, label, val, kind) in rows {
                layers.push(nt_text_at(
                    label.to_string(),
                    80.0,
                    gy,
                    &v,
                    col(238, 239, 225),
                    false,
                ));
                layers.push(nt_text_at(
                    format!("{:.0}%", val * 100.0),
                    200.0,
                    gy,
                    &v,
                    col(125, 131, 141),
                    false,
                ));
                let v_down = (val - 0.1).clamp(0.0, 1.0);
                let v_up = (val + 0.1).clamp(0.0, 1.0);
                let a_down = actions.clone();
                let a_up = actions.clone();
                layers.push(arrow_button_at("<", 44.0, gy, &v, move || match kind {
                    0 => push(&a_down, UiAction::SetMasterVol(v_down)),
                    1 => push(&a_down, UiAction::SetMusicVol(v_down)),
                    2 => push(&a_down, UiAction::SetAmbienceVol(v_down)),
                    _ => push(&a_down, UiAction::SetSfxVol(v_down)),
                }));
                layers.push(arrow_button_at(">", 252.0, gy, &v, move || match kind {
                    0 => push(&a_up, UiAction::SetMasterVol(v_up)),
                    1 => push(&a_up, UiAction::SetMusicVol(v_up)),
                    2 => push(&a_up, UiAction::SetAmbienceVol(v_up)),
                    _ => push(&a_up, UiAction::SetSfxVol(v_up)),
                }));
            }
            let a_sw = actions.clone();
            let cur3d = st.volume_3dsound;
            layers.push(nt_text_at(
                "3D SOUND".to_string(),
                80.0,
                140.0,
                &v,
                col(238, 239, 225),
                false,
            ));
            layers.push(nt_text_at(
                if cur3d {
                    "ON".to_string()
                } else {
                    "OFF".to_string()
                },
                200.0,
                140.0,
                &v,
                col(125, 131, 141),
                false,
            ));
            layers.push(hitbox_at(60.0, 134.0, 200.0, 16.0, &v, move || {
                push(&a_sw, UiAction::SettingToggle("volume_3dsound".to_string()));
            }));
            let a_back = actions.clone();
            layers.push(bigname_button_at(
                "BACK".to_string(),
                160.0,
                200.0,
                &v,
                col(125, 131, 141),
                move || push(&a_back, UiAction::SettingsBack),
            ));
        }
        2 => {
            // VIDEO: Crosshair(list), SideArt(list), Screenshake(slider), FreezeFrames(slider), Bloom(switch), Particles(switch), HideHUD(switch), PixelMode(list), DISPLAY category
            layers.push(nt_text_at(
                "VIDEO".to_string(),
                160.0,
                24.0,
                &v,
                col(153, 153, 153),
                true,
            ));
            let mut y = 48.0;
            // Crosshair list – values 0..3 placeholder (real sprCrosshair count)
            let cross = st.crosshair;
            layers.push(nt_text_at(
                "CROSSHAIR".to_string(),
                80.0,
                y,
                &v,
                col(238, 239, 225),
                false,
            ));
            layers.push(nt_text_at(
                format!("< {} >", cross + 1),
                200.0,
                y,
                &v,
                col(125, 131, 141),
                false,
            ));
            let a_l = actions.clone();
            let a_r = actions.clone();
            layers.push(hitbox_at(40.0, y - 6.0, 24.0, 16.0, &v, move || {
                push(
                    &a_l,
                    UiAction::SettingCycle {
                        key: "crosshair".to_string(),
                        dir: -1,
                    },
                )
            }));
            layers.push(hitbox_at(240.0, y - 6.0, 24.0, 16.0, &v, move || {
                push(
                    &a_r,
                    UiAction::SettingCycle {
                        key: "crosshair".to_string(),
                        dir: 1,
                    },
                )
            }));
            y += 18.0;
            let side = st.sideart;
            layers.push(nt_text_at(
                "SIDE ART".to_string(),
                80.0,
                y,
                &v,
                col(238, 239, 225),
                false,
            ));
            layers.push(nt_text_at(
                format!("< {} >", side),
                200.0,
                y,
                &v,
                col(125, 131, 141),
                false,
            ));
            let a_l = actions.clone();
            let a_r = actions.clone();
            layers.push(hitbox_at(40.0, y - 6.0, 24.0, 16.0, &v, move || {
                push(
                    &a_l,
                    UiAction::SettingCycle {
                        key: "sideart".to_string(),
                        dir: -1,
                    },
                )
            }));
            layers.push(hitbox_at(240.0, y - 6.0, 24.0, 16.0, &v, move || {
                push(
                    &a_r,
                    UiAction::SettingCycle {
                        key: "sideart".to_string(),
                        dir: 1,
                    },
                )
            }));
            y += 18.0;
            for (label, val, key) in [
                ("SCREENSHAKE", st.screenshake, "screenshake"),
                ("FREEZE FRAMES", st.freezeframes, "freezeframes"),
            ] {
                layers.push(nt_text_at(
                    label.to_string(),
                    80.0,
                    y,
                    &v,
                    col(238, 239, 225),
                    false,
                ));
                layers.push(nt_text_at(
                    format!("{:.0}%", (val * 100.0).clamp(0.0, 200.0)),
                    200.0,
                    y,
                    &v,
                    col(125, 131, 141),
                    false,
                ));
                let d = (val - 0.1).clamp(0.0, 2.0);
                let u = (val + 0.1).clamp(0.0, 2.0);
                let a_d = actions.clone();
                let a_u = actions.clone();
                let k1 = key.to_string();
                let k2 = key.to_string();
                layers.push(hitbox_at(40.0, y - 6.0, 24.0, 16.0, &v, move || {
                    push(
                        &a_d,
                        UiAction::SettingSlider {
                            key: k1.clone(),
                            value: d,
                        },
                    )
                }));
                layers.push(hitbox_at(240.0, y - 6.0, 24.0, 16.0, &v, move || {
                    push(
                        &a_u,
                        UiAction::SettingSlider {
                            key: k2.clone(),
                            value: u,
                        },
                    )
                }));
                layers.push(nt_text_at(
                    "<".to_string(),
                    44.0,
                    y,
                    &v,
                    col(238, 239, 225),
                    true,
                ));
                layers.push(nt_text_at(
                    ">".to_string(),
                    252.0,
                    y,
                    &v,
                    col(238, 239, 225),
                    true,
                ));
                y += 18.0;
            }
            for (label, cur, key) in [
                ("BLOOM", st.bloom, "bloom"),
                ("PARTICLES", st.particles, "particles"),
                ("HIDE HUD", !st.show_hud, "show_hud"),
            ] {
                layers.push(nt_text_at(
                    label.to_string(),
                    80.0,
                    y,
                    &v,
                    col(238, 239, 225),
                    false,
                ));
                layers.push(nt_text_at(
                    if cur {
                        "ON".to_string()
                    } else {
                        "OFF".to_string()
                    },
                    200.0,
                    y,
                    &v,
                    col(125, 131, 141),
                    false,
                ));
                let a_sw = actions.clone();
                let k = key.to_string();
                layers.push(hitbox_at(60.0, y - 6.0, 200.0, 16.0, &v, move || {
                    push(&a_sw, UiAction::SettingToggle(k.clone()))
                }));
                y += 18.0;
            }
            let pm = st.pixel_mode;
            layers.push(nt_text_at(
                "PIXEL MODE".to_string(),
                80.0,
                y,
                &v,
                col(238, 239, 225),
                false,
            ));
            layers.push(nt_text_at(
                format!("< {} >", pm),
                200.0,
                y,
                &v,
                col(125, 131, 141),
                false,
            ));
            let a_l = actions.clone();
            let a_r = actions.clone();
            layers.push(hitbox_at(40.0, y - 6.0, 24.0, 16.0, &v, move || {
                push(
                    &a_l,
                    UiAction::SettingCycle {
                        key: "pixel_mode".to_string(),
                        dir: -1,
                    },
                )
            }));
            layers.push(hitbox_at(240.0, y - 6.0, 24.0, 16.0, &v, move || {
                push(
                    &a_r,
                    UiAction::SettingCycle {
                        key: "pixel_mode".to_string(),
                        dir: 1,
                    },
                )
            }));
            y += 18.0;
            // DISPLAY SETTINGS category button
            let a_cat = actions.clone();
            layers.push(bigname_button_at(
                "DISPLAY SETTINGS".to_string(),
                160.0,
                y,
                &v,
                col(153, 153, 153),
                move || push(&a_cat, UiAction::SettingsCategory(3)),
            ));
            y += 18.0;
            let a_back = actions.clone();
            layers.push(bigname_button_at(
                "BACK".to_string(),
                160.0,
                200.0,
                &v,
                col(125, 131, 141),
                move || push(&a_back, UiAction::SettingsBack),
            ));
        }
        3 => {
            // Video_Display
            layers.push(nt_text_at(
                "DISPLAY".to_string(),
                160.0,
                24.0,
                &v,
                col(153, 153, 153),
                true,
            ));
            let mut y = 56.0;
            for (label, cur, key) in [
                ("WIDESCREEN", st.widescreen, "widescreen"),
                ("FULLSCREEN", st.fullscreen, "fullscreen"),
                ("VSYNC", st.vsync, "vsync"),
            ] {
                layers.push(nt_text_at(
                    label.to_string(),
                    80.0,
                    y,
                    &v,
                    col(238, 239, 225),
                    false,
                ));
                layers.push(nt_text_at(
                    if cur {
                        "ON".to_string()
                    } else {
                        "OFF".to_string()
                    },
                    200.0,
                    y,
                    &v,
                    col(125, 131, 141),
                    false,
                ));
                let a_sw = actions.clone();
                let k = key.to_string();
                layers.push(hitbox_at(60.0, y - 6.0, 200.0, 16.0, &v, move || {
                    push(&a_sw, UiAction::SettingToggle(k.clone()))
                }));
                y += 20.0;
            }
            let a_back = actions.clone();
            layers.push(bigname_button_at(
                "BACK".to_string(),
                160.0,
                200.0,
                &v,
                col(125, 131, 141),
                move || push(&a_back, UiAction::SettingsBack),
            ));
        }
        4 => {
            // GAME
            layers.push(nt_text_at(
                "GAME".to_string(),
                160.0,
                24.0,
                &v,
                col(153, 153, 153),
                true,
            ));
            let mut y = 48.0;
            for (label, cur, key) in [
                ("BOSS INTROS", st.boss_intros, "boss_intros"),
                ("PLAY TUTORIAL", st.show_tutorial, "show_tutorial"),
                ("SHOW TIMER", st.show_timer, "show_timer"),
                ("SHOW AREA", st.show_area, "show_area"),
                ("PAUSE BUTTON", st.pause_button, "pause_button"),
                (
                    "ACHIEVEMENT POPUPS",
                    st.achievements_popup,
                    "achievements_popup",
                ),
                ("AUTO PAUSE", st.auto_pause, "auto_pause"),
            ] {
                layers.push(nt_text_at(
                    label.to_string(),
                    80.0,
                    y,
                    &v,
                    col(238, 239, 225),
                    false,
                ));
                layers.push(nt_text_at(
                    if cur {
                        "ON".to_string()
                    } else {
                        "OFF".to_string()
                    },
                    200.0,
                    y,
                    &v,
                    col(125, 131, 141),
                    false,
                ));
                let a_sw = actions.clone();
                let k = key.to_string();
                layers.push(hitbox_at(60.0, y - 6.0, 200.0, 16.0, &v, move || {
                    push(&a_sw, UiAction::SettingToggle(k.clone()))
                }));
                y += 18.0;
            }
            // VIEW CREDITS button
            let a_cred = actions.clone();
            layers.push(bigname_button_at(
                "VIEW CREDITS".to_string(),
                160.0,
                y,
                &v,
                col(153, 153, 153),
                move || push(&a_cred, UiAction::SettingViewCredits),
            ));
            y += 20.0;
            // subcategories
            let a_prof = actions.clone();
            layers.push(bigname_button_at(
                "PROFILE".to_string(),
                160.0,
                y,
                &v,
                col(153, 153, 153),
                move || push(&a_prof, UiAction::SettingsCategory(5)),
            ));
            y += 20.0;
            let a_col = actions.clone();
            layers.push(bigname_button_at(
                "COLOR".to_string(),
                160.0,
                y,
                &v,
                col(153, 153, 153),
                move || push(&a_col, UiAction::SettingsCategory(6)),
            ));
            y += 20.0;
            let a_data = actions.clone();
            layers.push(bigname_button_at(
                "DATA".to_string(),
                160.0,
                y,
                &v,
                col(153, 153, 153),
                move || push(&a_data, UiAction::SettingsCategory(7)),
            ));
            y += 20.0;
            let a_back = actions.clone();
            layers.push(bigname_button_at(
                "BACK".to_string(),
                160.0,
                200.0,
                &v,
                col(125, 131, 141),
                move || push(&a_back, UiAction::SettingsBack),
            ));
        }
        8 => {
            // CONTROLS (main)
            layers.push(nt_text_at(
                "CONTROLS".to_string(),
                160.0,
                24.0,
                &v,
                col(153, 153, 153),
                true,
            ));
            let mut y = 48.0;
            for (label, cur, key) in [
                ("GAMEPAD", st.gamepad_enabled, "gamepad_enabled"),
                ("AIM ASSIST", st.aim_assist, "aim_assist"),
                ("AUTO AIM", st.auto_aim, "auto_aim"),
                ("VOLUME CONTROLS", st.volume_controls, "volume_controls"),
                ("SPLIT FIRE", st.split_fire, "split_fire"),
                ("FIXED SIGHT", st.fixed_sight, "fixed_sight"),
            ] {
                layers.push(nt_text_at(
                    label.to_string(),
                    80.0,
                    y,
                    &v,
                    col(238, 239, 225),
                    false,
                ));
                layers.push(nt_text_at(
                    if cur {
                        "ON".to_string()
                    } else {
                        "OFF".to_string()
                    },
                    200.0,
                    y,
                    &v,
                    col(125, 131, 141),
                    false,
                ));
                let a_sw = actions.clone();
                let k = key.to_string();
                layers.push(hitbox_at(60.0, y - 6.0, 200.0, 16.0, &v, move || {
                    push(&a_sw, UiAction::SettingToggle(k.clone()))
                }));
                y += 18.0;
            }
            // Gamepad type list
            let gt = st.gamepad_type;
            layers.push(nt_text_at(
                "GAMEPAD STYLE".to_string(),
                80.0,
                y,
                &v,
                col(238, 239, 225),
                false,
            ));
            let names = ["XBONE", "PS4", "Switch", "SteamDeck"];
            let nm = names[(gt as usize) % names.len()];
            layers.push(nt_text_at(
                format!("< {} >", nm),
                200.0,
                y,
                &v,
                col(125, 131, 141),
                false,
            ));
            let a_l = actions.clone();
            let a_r = actions.clone();
            layers.push(hitbox_at(40.0, y - 6.0, 24.0, 16.0, &v, move || {
                push(
                    &a_l,
                    UiAction::SettingCycle {
                        key: "gamepad_type".to_string(),
                        dir: -1,
                    },
                )
            }));
            layers.push(hitbox_at(240.0, y - 6.0, 24.0, 16.0, &v, move || {
                push(
                    &a_r,
                    UiAction::SettingCycle {
                        key: "gamepad_type".to_string(),
                        dir: 1,
                    },
                )
            }));
            y += 18.0;
            layers.push(nt_text_at(
                "SIZE SCALE".to_string(),
                80.0,
                y,
                &v,
                col(238, 239, 225),
                false,
            ));
            layers.push(nt_text_at(
                format!("{:.0}%", st.controls_scale * 100.0),
                200.0,
                y,
                &v,
                col(125, 131, 141),
                false,
            ));
            let d = (st.controls_scale - 0.1).clamp(0.0, 1.0);
            let u = (st.controls_scale + 0.1).clamp(0.0, 1.0);
            let a_d = actions.clone();
            let a_u = actions.clone();
            layers.push(arrow_button_at("<", 44.0, y, &v, move || {
                push(
                    &a_d,
                    UiAction::SettingSlider {
                        key: "controls_scale".to_string(),
                        value: d,
                    },
                )
            }));
            layers.push(arrow_button_at(">", 252.0, y, &v, move || {
                push(
                    &a_u,
                    UiAction::SettingSlider {
                        key: "controls_scale".to_string(),
                        value: u,
                    },
                )
            }));
            y += 18.0;
            let a_rem = actions.clone();
            layers.push(bigname_button_at(
                "REMAP".to_string(),
                160.0,
                y,
                &v,
                col(153, 153, 153),
                move || push(&a_rem, UiAction::SettingsCategory(9)),
            ));
            y += 20.0;
            let a_pref = actions.clone();
            layers.push(bigname_button_at(
                "CHAR PREFS".to_string(),
                160.0,
                y,
                &v,
                col(153, 153, 153),
                move || push(&a_pref, UiAction::SettingsCategory(10)),
            ));
            y += 20.0;
            let a_exp = actions.clone();
            layers.push(bigname_button_at(
                "EXPERIMENTAL".to_string(),
                160.0,
                y,
                &v,
                col(153, 153, 153),
                move || push(&a_exp, UiAction::SettingsCategory(11)),
            ));
            y += 20.0;
            let a_back = actions.clone();
            layers.push(bigname_button_at(
                "BACK".to_string(),
                160.0,
                200.0,
                &v,
                col(125, 131, 141),
                move || push(&a_back, UiAction::SettingsBack),
            ));
        }
        5 => {
            // Game_Profile
            layers.push(nt_text_at(
                "PROFILE".to_string(),
                160.0,
                24.0,
                &v,
                col(153, 153, 153),
                true,
            ));
            let mut y = 48.0;
            layers.push(nt_text_at(
                "PROFILE NAME".to_string(),
                80.0,
                y,
                &v,
                col(238, 239, 225),
                false,
            ));
            layers.push(nt_text_at(
                if st.profile_name.is_empty() {
                    "NONE".to_string()
                } else {
                    st.profile_name.clone()
                },
                200.0,
                y,
                &v,
                col(125, 131, 141),
                false,
            ));
            y += 20.0;
            layers.push(nt_text_at(
                "COLOR".to_string(),
                80.0,
                y,
                &v,
                col(238, 239, 225),
                false,
            ));
            let col_hex = if st.player_color_hex.is_empty() {
                "DEFAULT".to_string()
            } else {
                st.player_color_hex.clone()
            };
            layers.push(nt_text_at(col_hex, 200.0, y, &v, col(125, 131, 141), false));
            let a_col = actions.clone();
            layers.push(bigname_button_at(
                "COLOR".to_string(),
                160.0,
                y + 20.0,
                &v,
                col(153, 153, 153),
                move || push(&a_col, UiAction::SettingsCategory(6)),
            ));
            let a_back = actions.clone();
            layers.push(bigname_button_at(
                "BACK".to_string(),
                160.0,
                200.0,
                &v,
                col(125, 131, 141),
                move || push(&a_back, UiAction::SettingsBack),
            ));
        }
        6 => {
            // Game_Color – RGB sliders via hex input stub + color preview
            layers.push(nt_text_at(
                "COLOR".to_string(),
                160.0,
                24.0,
                &v,
                col(153, 153, 153),
                true,
            ));
            layers.push(nt_text_at(
                format!(
                    "HEX: {}",
                    if st.player_color_hex.is_empty() {
                        "DEFAULT".to_string()
                    } else {
                        st.player_color_hex.clone()
                    }
                ),
                160.0,
                60.0,
                &v,
                col(238, 239, 225),
                true,
            ));
            // For demo: tapping cycles a preset list via SettingInput
            let presets = ["FF0000", "00FF00", "0000FF", "", "FF00FF"];
            let cur = st.player_color_hex.clone();
            let idx = presets.iter().position(|p| *p == cur).unwrap_or(3);
            let next = presets[(idx + 1) % presets.len()].to_string();
            let a_sw = actions.clone();
            layers.push(bigname_button_at(
                "CYCLE COLOR".to_string(),
                160.0,
                90.0,
                &v,
                col(153, 153, 153),
                move || {
                    push(
                        &a_sw,
                        UiAction::SettingInput {
                            key: "player_color_hex".to_string(),
                            value: next.clone(),
                        },
                    )
                },
            ));
            let a_back = actions.clone();
            layers.push(bigname_button_at(
                "BACK".to_string(),
                160.0,
                200.0,
                &v,
                col(125, 131, 141),
                move || push(&a_back, UiAction::SettingsBack),
            ));
        }
        7 => {
            // Game_Data
            layers.push(nt_text_at(
                "DATA".to_string(),
                160.0,
                24.0,
                &v,
                col(153, 153, 153),
                true,
            ));
            let a_reset = actions.clone();
            layers.push(bigname_button_at(
                "RESET OPTIONS".to_string(),
                160.0,
                80.0,
                &v,
                col(238, 239, 225),
                move || push(&a_reset, UiAction::SettingResetOptions),
            ));
            let a_erase = actions.clone();
            layers.push(bigname_button_at(
                "ERASE PROGRESS".to_string(),
                160.0,
                110.0,
                &v,
                col(221, 56, 45),
                move || push(&a_erase, UiAction::SettingEraseProgress),
            ));
            let a_back = actions.clone();
            layers.push(bigname_button_at(
                "BACK".to_string(),
                160.0,
                200.0,
                &v,
                col(125, 131, 141),
                move || push(&a_back, UiAction::SettingsBack),
            ));
        }
        9 => {
            // Controls_Remapping_Keys – list keybinds (stub – GML has input capturing)
            layers.push(nt_text_at(
                "REMAP".to_string(),
                160.0,
                24.0,
                &v,
                col(153, 153, 153),
                true,
            ));
            let keys = [
                ("FIRE", "fire"),
                ("ACTIVE", "spec"),
                ("SWAP", "swap"),
                ("PICK", "pick"),
            ];
            let mut y = 60.0;
            for (label, _k) in keys {
                layers.push(nt_text_at(
                    label.to_string(),
                    160.0,
                    y,
                    &v,
                    col(238, 239, 225),
                    true,
                ));
                y += 18.0;
            }
            layers.push(nt_text_at(
                "PRESS ANY KEY – WIP".to_string(),
                160.0,
                y,
                &v,
                col(125, 131, 141),
                true,
            ));
            let a_back = actions.clone();
            layers.push(bigname_button_at(
                "BACK".to_string(),
                160.0,
                200.0,
                &v,
                col(125, 131, 141),
                move || push(&a_back, UiAction::SettingsBack),
            ));
        }
        10 => {
            // Controls_Preferences – 8 cprefs switches
            layers.push(nt_text_at(
                "CHAR PREFS".to_string(),
                160.0,
                24.0,
                &v,
                col(153, 153, 153),
                true,
            ));
            let labels = [
                "EYES", "MELTING", "PLANT", "VENUZ", "STER", "HORROR", "ROGUE", "SKELETON",
            ];
            let mut y = 48.0;
            for (i, label) in labels.iter().enumerate() {
                let cur = st.cprefs[i];
                layers.push(nt_text_at(
                    label.to_string(),
                    80.0,
                    y,
                    &v,
                    col(238, 239, 225),
                    false,
                ));
                layers.push(nt_text_at(
                    if cur {
                        "ON".to_string()
                    } else {
                        "OFF".to_string()
                    },
                    200.0,
                    y,
                    &v,
                    col(125, 131, 141),
                    false,
                ));
                let a_sw = actions.clone();
                layers.push(hitbox_at(60.0, y - 6.0, 200.0, 16.0, &v, move || {
                    push(&a_sw, UiAction::SettingToggle(format!("cprefs_{}", i)))
                }));
                y += 18.0;
            }
            let a_back = actions.clone();
            layers.push(bigname_button_at(
                "BACK".to_string(),
                160.0,
                200.0,
                &v,
                col(125, 131, 141),
                move || push(&a_back, UiAction::SettingsBack),
            ));
        }
        11 => {
            // Controls_Experimental
            layers.push(nt_text_at(
                "EXPERIMENTAL".to_string(),
                160.0,
                24.0,
                &v,
                col(153, 153, 153),
                true,
            ));
            layers.push(nt_text_at(
                "KEYBOARD MODE – WIP".to_string(),
                160.0,
                80.0,
                &v,
                col(125, 131, 141),
                true,
            ));
            let a_back = actions.clone();
            layers.push(bigname_button_at(
                "BACK".to_string(),
                160.0,
                200.0,
                &v,
                col(125, 131, 141),
                move || push(&a_back, UiAction::SettingsBack),
            ));
        }
        12 => {
            // LANGUAGE full from earlier
            layers.push(nt_text_at(
                "LANGUAGE".to_string(),
                160.0,
                24.0,
                &v,
                col(153, 153, 153),
                true,
            ));
            let mut y = 60.0;
            for lang in st.available_languages.clone() {
                let is_cur = lang == st.language;
                let label = lang.to_ascii_uppercase();
                let a = actions.clone();
                let lc = lang.clone();
                layers.push(bigname_button_at(
                    label,
                    160.0,
                    y,
                    &v,
                    if is_cur {
                        col(255, 255, 255)
                    } else {
                        col(153, 153, 153)
                    },
                    move || push(&a, UiAction::SetLanguage(lc.clone())),
                ));
                y += 20.0;
            }
            let a_back = actions.clone();
            layers.push(bigname_button_at(
                "BACK".to_string(),
                160.0,
                200.0,
                &v,
                col(125, 131, 141),
                move || push(&a_back, UiAction::SettingsBack),
            ));
        }
        _ => {}
    }
    // SAVE hint only on Audio where values changed? original saves on Back via scrOptionsUpdate.
    // We keep explicit SAVE at bottom for Main? Use existing SaveSettings on Audio BACK? Keep implicit.
    ZStack(Modifier::new().fill_max_size()).child(layers)
}

fn bigname_button_at(
    label: String,
    gx: f32,
    gy: f32,
    v: &NtView,
    color: RColor,
    on_click: impl Fn() + 'static,
) -> View {
    // GML draw_text_bigname with scale 0.65 – we use Silkscreen at ~10*s for bigname vs 7*s for normal
    let font_px = (10.0 * v.s).clamp(10.0, 140.0);
    let gw = 120.0;
    let gh = 22.0;
    Column(
        Modifier::new()
            .fill_max_size()
            .padding_values(PaddingValues {
                left: v.ox + (gx - gw * 0.5) * v.s,
                right: 0.0,
                top: v.oy + (gy - gh * 0.5) * v.s,
                bottom: 0.0,
            })
            .align_items(AlignItems::FLEX_START),
    )
    .child(
        Column(
            Modifier::new()
                .width(gw * v.s)
                .height(gh * v.s)
                .justify_content(JustifyContent::CENTER)
                .align_items(AlignItems::CENTER)
                .clickable()
                .on_click(on_click),
        )
        .child(
            RText(label)
                .size(font_px)
                .font_family("Silkscreen")
                .color(color)
                .single_line(),
        ),
    )
}

fn arrow_button_at(
    glyph: &str,
    gx: f32,
    gy: f32,
    v: &NtView,
    on_click: impl Fn() + 'static,
) -> View {
    let font_px = (7.0 * v.s).clamp(8.0, 96.0);
    Column(
        Modifier::new()
            .fill_max_size()
            .padding_values(PaddingValues {
                left: v.ox + (gx - 12.0) * v.s,
                right: 0.0,
                top: v.oy + (gy - 3.0) * v.s,
                bottom: 0.0,
            })
            .align_items(AlignItems::FLEX_START),
    )
    .child(
        Column(
            Modifier::new()
                .width(24.0 * v.s)
                .height(16.0 * v.s)
                .justify_content(JustifyContent::CENTER)
                .align_items(AlignItems::CENTER)
                .clickable()
                .on_click(on_click),
        )
        .child(
            RText(glyph.to_string())
                .size(font_px)
                .font_family("Silkscreen")
                .color(col(238, 239, 225))
                .single_line(),
        ),
    )
}

fn hitbox_at(x: f32, y: f32, w: f32, h: f32, v: &NtView, on_click: impl Fn() + 'static) -> View {
    Column(
        Modifier::new()
            .fill_max_size()
            .padding_values(PaddingValues {
                left: v.ox + x * v.s,
                right: 0.0,
                top: v.oy + y * v.s,
                bottom: 0.0,
            })
            .align_items(AlignItems::FLEX_START),
    )
    .child(Column(
        Modifier::new()
            .width(w * v.s)
            .height(h * v.s)
            .clickable()
            .on_click(on_click),
    ))
}

fn credits_ui(st: &SharedUi, actions: Arc<Mutex<Vec<UiAction>>>) -> View {
    // GML Credits object draws centered text at 320×240; we mirror with nt_text_at
    let v = nt_view(st);
    let a = actions.clone();
    let tr = &st.translations;
    let mut layers: Vec<View> = Vec::new();
    layers.push(nt_text_at(
        t(tr, "credits", "CREDITS").to_ascii_uppercase(),
        160.0,
        40.0,
        &v,
        col(238, 239, 225),
        true,
    ));
    layers.push(nt_text_at(
        "A fan recreation of Nuclear Throne (Vlambeer)".to_string(),
        160.0,
        80.0,
        &v,
        col(238, 239, 225),
        true,
    ));
    layers.push(nt_text_at(
        "Built with Bevy + Repose".to_string(),
        160.0,
        96.0,
        &v,
        col(125, 131, 141),
        true,
    ));
    layers.push(nt_text_at(
        "No original game assets included".to_string(),
        160.0,
        112.0,
        &v,
        col(125, 131, 141),
        true,
    ));
    layers.push(text_button_at(
        "BACK".to_string(),
        160.0,
        180.0,
        80.0,
        18.0,
        &v,
        col(125, 131, 141),
        move || push(&a, UiAction::CloseOverlay),
    ));
    ZStack(Modifier::new().fill_max_size()).child(layers)
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
    // GML LevCont/Draw_64 + Other_10 exact:
    // - scrDrawSpiral() is the WGSL vortex quad (no dim)
    // - bigname at (160,48) + appear slide, subtitle at (160,75) "@s" text
    // - icons at y = view_height-21 = 219, x = center-(n-1)*half+idx*step
    //   step = min(32, floor(320/(n+1))), half = step/2, scale = max(0.65, step/32)
    // - SkillIcon draws sprSkillIcon[skill] at x,y+appeary-sign(selected) c_gray/c_white
    // - selected description at (160,179) = view_height-61 (center middle)
    // Bevy: vortex is separate quad; this layer provides text+hitboxes only;
    // icons themselves are camera-anchored gm_sprite in ui_art::sync_mutation_icons
    // at identical gui_x/y so Repose hitbox and sprite stay pixel-locked.
    let v = nt_view(st);
    let is_ultra = st
        .mutation_choices
        .iter()
        .any(|choice| choice.trim().starts_with("ULTRA:"));

    let is_robot = st.character.to_ascii_lowercase() == "robot";
    let (title, subtitle, robot_extra) = if is_ultra {
        if is_robot {
            ("ULTRA MUTATION", "INSTALL ULTRA UPDATE", None)
        } else {
            ("ULTRA MUTATION", "PICK YOUR ULTRA MUTATION", None)
        }
    } else if is_robot {
        ("LEVEL UP", "INSTALL UPDATES", Some("DO NOT TURN OFF ROBOT"))
    } else {
        ("LEVEL UP", "SELECT MUTATIONS", None)
    };
    let accent = if is_ultra {
        col(255, 221, 0)
    } else {
        col(98, 220, 88)
    };

    let n = st.mutation_choices.len().max(1).min(8);
    // LevCont/Other_10: step_size = min(32, floor(view_width/(num+1)))
    // scale = max(0.65, step/32) – matches ui_art sync_mutation_icons
    let step = (320.0 / (n as f32 + 1.0)).floor().min(32.0);
    let scale = (step / 32.0).max(0.65);
    let half = step * 0.5;
    let start_x = 160.0 - (n as f32 - 1.0) * half;
    // GML icons at y = view_height-21 = 219, origin (12,16), 24x32 * scale
    let icon_y = 219.0; // view_height -21
    let hit_w = 24.0 * scale;
    let hit_h = 32.0 * scale;
    let hit_y = icon_y - 16.0 * scale;

    let mut layers: Vec<View> = Vec::new();

    // No dim – GML draws spiral over the paused view, not a black rect.
    layers.push(Column(
        Modifier::new()
            .fill_max_size()
            .background(RColor::from_rgba(0, 0, 0, 0)),
    ));

    // GML LevCont/Draw_0: draw_text_bigname at (160,48) + appear, draw_text_nt at (160,75)
    // after appear settles. Original uses sprLevelUpText/sprLevelUltraText bigname
    // and loc'd subtitle (SELECT % MUTATIONS / PICK YOUR ULTRA MUTATION).
    // We render at appear==0 so positions are exact final frame.
    layers.push(nt_text_at(title.to_string(), 160.0, 48.0, &v, accent, true));
    layers.push(nt_text_at(
        subtitle.to_string(),
        160.0,
        75.0,
        &v,
        col(238, 239, 225),
        true,
    ));
    if let Some(extra) = robot_extra {
        layers.push(nt_text_at(
            extra.to_string(),
            160.0,
            87.0,
            &v,
            col(238, 239, 225),
            true,
        ));
    }

    for (i, choice) in st.mutation_choices.iter().enumerate().take(n) {
        let (_ultra, _name, _desc) = mutation_choice_parts(choice);
        let icon_x = start_x + i as f32 * step;
        let x = icon_x - hit_w * 0.5;
        let a = actions.clone();
        let idx = i;
        let is_selected = st.mutation_selected == Some(i);
        // GML SkillIcon/Mouse_4: first press selects (c_gray->c_white, sndHover), second press confirms
        // No numbers, no border – just invisible hitbox matching sprite bbox scaled.
        let a2 = actions.clone();
        layers.push(
            Column(
                Modifier::new()
                    .fill_max_size()
                    .padding_values(PaddingValues {
                        left: v.ox + x * v.s,
                        right: 0.0,
                        top: v.oy + hit_y * v.s,
                        bottom: 0.0,
                    })
                    .align_items(AlignItems::FLEX_START),
            )
            .child(Column(
                Modifier::new()
                    .width(hit_w * v.s)
                    .height(hit_h * v.s)
                    .background(RColor::from_rgba(0, 0, 0, 0))
                    .clickable()
                    .on_click(move || {
                        if is_selected {
                            push(&a, UiAction::PickMutation(idx))
                        } else {
                            push(&a2, UiAction::SelectMutation(idx))
                        }
                    }),
            )),
        );
    }

    // GML SkillIcon/Draw_0 selected draws txt2 = "@wNAME#@sDESC" at
    // (view_width/2, view_height-61) = (160,179) with fa_center/fa_middle,
    // two lines (name white, desc small gray). Only when selected & appeary==0.
    // Original shows nothing when nothing selected (icons just gray).
    if let Some(sel) = st
        .mutation_selected
        .and_then(|i| st.mutation_choices.get(i))
    {
        let (_, name, desc) = mutation_choice_parts(sel);
        // Name white, desc gray – two separate centered lines as in GML # newline
        layers.push(nt_text_at(
            name.to_ascii_uppercase(),
            160.0,
            173.0,
            &v,
            RColor::WHITE,
            true,
        ));
        if !desc.is_empty() {
            layers.push(nt_text_at(desc, 160.0, 185.0, &v, col(238, 239, 225), true));
        }
    }

    ZStack(Modifier::new().fill_max_size()).child(layers)
}

fn game_over_panel(st: &SharedUi, actions: Arc<Mutex<Vec<UiAction>>>) -> View {
    let v = nt_view(st);
    let quit_actions = actions.clone();

    let mut layers: Vec<View> = Vec::new();

    layers.push(Column(
        Modifier::new()
            .fill_max_size()
            .background(RColor::from_rgba(0, 0, 0, 230)),
    ));

    layers.push(nt_text_at(
        "DEAD".to_string(),
        160.0,
        36.0,
        &v,
        col(230, 48, 42),
        true,
    ));

    if !st.toast.is_empty() {
        layers.push(nt_text_at(
            st.toast.clone(),
            160.0,
            54.0,
            &v,
            col(238, 239, 225),
            true,
        ));
    }

    let stats_y0 = 78.0;
    layers.push(nt_text_at(
        format!("AREA {}-{}", st.world, st.floor_in_world),
        160.0,
        stats_y0,
        &v,
        col(238, 239, 225),
        true,
    ));
    layers.push(nt_text_at(
        format!("LOOP {}", st.loop_count),
        160.0,
        stats_y0 + 14.0,
        &v,
        col(125, 131, 141),
        true,
    ));
    layers.push(nt_text_at(
        format!("KILLS {}", st.total_kills),
        160.0,
        stats_y0 + 28.0,
        &v,
        col(238, 239, 225),
        true,
    ));
    layers.push(nt_text_at(
        format!("SCORE {}", st.score),
        160.0,
        stats_y0 + 42.0,
        &v,
        col(238, 239, 225),
        true,
    ));
    layers.push(nt_text_at(
        format!("BEST {}", st.high_score),
        160.0,
        stats_y0 + 56.0,
        &v,
        col(125, 131, 141),
        true,
    ));

    // Mutations row – text fallback; sprite icons drawn in ui_art via death_mutation_ids if available
    if !st.death_mutation_ids.is_empty() {
        // GML draws sprSkillIconHUD row; Bevy ui_art will handle icons if desired.
        // Keep a subtle text fallback at 160,160 so Repose-only builds still show count.
        layers.push(nt_text_at(
            format!("MUTATIONS {}", st.death_mutation_ids.len()),
            160.0,
            160.0,
            &v,
            col(156, 160, 150),
            true,
        ));
    }

    layers.push(nt_text_at(
        "R - RESTART".to_string(),
        160.0,
        200.0,
        &v,
        col(238, 239, 225),
        true,
    ));
    layers.push(nt_text_at(
        "CLICK - MENU".to_string(),
        160.0,
        214.0,
        &v,
        col(125, 131, 141),
        true,
    ));

    // Invisible full-screen click → QuitToTitle; keyboard R → StartGame handled in process_ui_actions
    layers.push(Column(
        Modifier::new().fill_max_size().clickable().on_click({
            let a = quit_actions.clone();
            move || push(&a, UiAction::QuitToTitle)
        }),
    ));

    ZStack(Modifier::new().fill_max_size()).child(layers)
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

pub(crate) fn push(actions: &Arc<Mutex<Vec<UiAction>>>, a: UiAction) {
    if let Ok(mut q) = actions.lock() {
        q.push(a);
    }
}

/// The five big main-menu buttons (nt-rewrite `MainMenuButton`): PLAY,
/// CO-OP, SETTINGS, STATS, QUIT - big pixel text centred at gui x=160,
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

    layers.push(Column(
        Modifier::new()
            .fill_max_size()
            .background(RColor::from_rgba(0, 0, 0, 40)),
    ));

    for (label, index) in LABELS {
        let gy = 72.0 + index as f32 * 24.0;
        // CO-OP and STATS have no backend in this port yet: c_uidark, inert.
        let available = matches!(index, 0 | 2 | 4);
        let hovered = st.main_menu_hover == index;
        let color = if !available {
            col(64, 64, 64)
        } else if hovered {
            col(255, 255, 255)
        } else {
            col(153, 153, 153)
        };
        // Hover lifts the row by 1 NT px (MainMenuButton/Draw_0).
        let lift = if hovered && available { 1.0 } else { 0.0 };

        let a = actions.clone();
        layers.push(
            Row(Modifier::new()
                .fill_max_size()
                .padding_values(PaddingValues {
                    left: v.ox,
                    right: 0.0,
                    top: v.oy + (gy - 10.0 - lift) * v.s,
                    bottom: 0.0,
                })
                .align_items(AlignItems::FLEX_START))
            .child(
                Column(
                    Modifier::new()
                        .width(320.0 * v.s)
                        .height(22.0 * v.s)
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
                        .size((16.0 * v.s).clamp(12.0, 180.0))
                        .font_family("Silkscreen")
                        .color(color)
                        .single_line(),
                ),
            ),
        );
    }

    ZStack(Modifier::new().fill_max_size()).child(layers)
}

fn hud_weapon_ammo(st: &SharedUi, slot: usize) -> i32 {
    let Some(name) = st.weapons.get(slot) else {
        return 0;
    };

    let n = name.to_ascii_lowercase();

    let ammo_index = if n == "none" || n.is_empty() {
        0
    } else if n.contains("shotgun")
        || n.contains("super shotgun")
        || n.contains("sawed")
        || n.contains("flak")
    {
        2 // shells
    } else if n.contains("crossbow")
        || n.contains("splinter")
        || n.contains("disc")
        || n.contains("seeker")
        || n.contains("bolt")
    {
        3 // bolts
    } else if n.contains("grenade")
        || n.contains("bazooka")
        || n.contains("missile")
        || n.contains("launcher")
        || n.contains("nuke")
    {
        4 // explosives
    } else if n.contains("laser")
        || n.contains("plasma")
        || n.contains("lightning")
        || n.contains("energy")
        || n.contains("flame")
    {
        5 // energy
    } else {
        1 // bullets
    };

    if ammo_index == 0 {
        0
    } else {
        st.ammo[ammo_index].max(0)
    }
}

/// Original HUD text pass - everything scrDrawPlayerHUD draws as text,
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

    // Level number centred at gui (11, 16) with fa_middle until ultra.
    if st.level < 99 {
        layers.push(nt_text_at_ex(
            st.level.to_string(),
            11.0,
            16.0,
            &v,
            col(255, 255, 255),
            true,
            true, // middle_y
        ));
    }

    // Ammo counts left-aligned at (dx + 18, dy + 5) per weapon slot; the
    // stored weapon renders in silver (c_silver) like upstream.
    // GML scrDrawPlayerHUD draws this only `if _type != Ammo.None` -
    // melee weapons show no count at all (not "0").
    for slot in 0..2usize {
        let amount = st.weapon_ammo.get(slot).copied().unwrap_or(-1);
        if amount < 0 {
            continue;
        }
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

/// Anchor text at NT GUI coords.
/// - `centered`: fa_center / horizontal centre on `gx` (fa_top vertically
///   unless `middle_y`)
/// - `middle_y`: fa_middle vertical (used for the level number at (11,16))
fn nt_text_at(text: String, gx: f32, gy: f32, v: &NtView, color: RColor, centered: bool) -> View {
    nt_text_at_ex(text, gx, gy, v, color, centered, false)
}

fn nt_text_at_ex(
    text: String,
    gx: f32,
    gy: f32,
    v: &NtView,
    color: RColor,
    centered: bool,
    middle_y: bool,
) -> View {
    let font_px = (7.0 * v.s).clamp(8.0, 96.0);
    // Approximate fntM1 glyph box; Silkscreen ~1 em tall.
    let half_h = font_px * 0.5;
    let top = if middle_y {
        v.oy + gy * v.s - half_h
    } else {
        v.oy + gy * v.s
    };

    let (left, box_w, align) = if centered {
        // Width 2*gx so the box centre sits on gx (same trick as before),
        // but clamp so very-left anchors still work.
        let w = (2.0 * gx * v.s).max(font_px);
        (v.ox + gx * v.s - w * 0.5, w, AlignItems::CENTER)
    } else {
        // LEFT-ALIGNED: must start at gx (this was the HUD bug).
        (v.ox + gx * v.s, 200.0 * v.s, AlignItems::FLEX_START)
    };

    Column(
        Modifier::new()
            .fill_max_size()
            .padding_values(PaddingValues {
                left: left.max(0.0),
                right: 0.0,
                top: top.max(0.0),
                bottom: 0.0,
            })
            .align_items(AlignItems::FLEX_START),
    )
    .child(
        Column(Modifier::new().width(box_w).align_items(align)).child(
            RText(text)
                .size(font_px)
                .font_family("Silkscreen")
                .color(color)
                .single_line(),
        ),
    )
}

fn text_button_at(
    label: String,
    gx: f32,
    gy: f32,
    gw: f32,
    gh: f32,
    v: &NtView,
    color: RColor,
    on_click: impl Fn() + 'static,
) -> View {
    Column(
        Modifier::new()
            .fill_max_size()
            .padding_values(PaddingValues {
                left: v.ox + (gx - gw * 0.5) * v.s,
                right: 0.0,
                top: v.oy + (gy - gh * 0.5) * v.s,
                bottom: 0.0,
            })
            .align_items(AlignItems::FLEX_START),
    )
    .child(
        Column(
            Modifier::new()
                .width(gw * v.s)
                .height(gh * v.s)
                .justify_content(JustifyContent::CENTER)
                .align_items(AlignItems::CENTER)
                .clickable()
                .on_click(on_click),
        )
        .child(
            RText(label)
                .size((8.0 * v.s).max(8.0))
                .font_family("Silkscreen")
                .color(color)
                .single_line(),
        ),
    )
}
