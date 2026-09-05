//! Pushes live game state into the Repose `SharedUi` so the in-game HUD,
//! boss bar, mutation picker, and game-over overlay can be composed by the UI.

use bevy::prelude::*;

use crate::game::components::*;
use crate::game::content::*;
use crate::game::progression;
use crate::game::world;
use crate::menus::UiBridge;

pub fn sync_hud(
    bridge: Res<UiBridge>,
    run: Option<Res<Run>>,
    score: Option<Res<Score>>,
    save: Option<Res<crate::save::SaveData>>,
    toast: Option<Res<Toast>>,
    pending: Option<Res<PendingMutation>>,
    pending_ultra: Option<Res<PendingUltra>>,
    character: Option<Res<SelectedCharacter>>,
    player_q: Query<(&Player, &Health, &Inventory), With<Player>>,
    boss_q: Query<(&Enemy, &Health), With<BossBrain>>,
) {
    let mut ui = match bridge.shared.lock() {
        Ok(g) => g,
        Err(p) => {
            bevy::log::warn!("SharedUi poisoned in sync_hud");
            p.into_inner()
        }
    };

    let Some(run) = run else {
        ui.game_over = false;
        ui.mutation_choices.clear();
        ui.mutation_choice_ids.clear();
        ui.death_mutation_ids.clear();
        ui.boss_hp = 0;
        ui.boss_max = 0;
        ui.boss_name.clear();
        ui.loop_count = 0;
        ui.toast.clear();
        ui.toast_timer = 0.0;
        return;
    };

    ui.game_over = run.game_over;

    if let Some(toast) = toast {
        ui.toast = toast.text.clone();
        ui.toast_timer = if toast.timer.duration().is_zero() {
            0.0
        } else {
            1.0 - toast.timer.fraction()
        };
    }

    ui.boss_hp = 0;
    ui.boss_max = 0;
    ui.boss_name.clear();

    if let Some((enemy, health)) = boss_q.iter().max_by_key(|(_, health)| health.max) {
        ui.boss_hp = health.hp.max(0) as u32;
        ui.boss_max = health.max.max(1) as u32;
        ui.boss_name = enemy_def(enemy.kind).name.to_string();
    }

    if let Some(character) = character {
        ui.character = character_def(character.0).name.to_string();
        // nt-rewrite `enum Race` id (Random=0..Cuz=16).
        ui.selected_character = character.0 as usize;
    }

    if let Ok((player, health, inv)) = player_q.single() {
        ui.hp = health.hp.max(0);
        ui.max_hp = health.max;
        ui.level = player.level;
        ui.rads = player.rads;
        ui.max_rads = player.next_level_rads;
        ui.weapons = (0..inv.weapon_slots)
            .map(|i| weapon_id_name(inv.weapons[i]).to_string())
            .collect();
        ui.current_weapon = inv.current;
        ui.ammo = inv.ammo;
        // Ammo count of each weapon's own type, for the HUD text pass.
        let t1 = crate::game::content::weapon_meta(inv.weapons[0]).wep_type as usize;
        let t2 = if inv.weapon_slots > 1 {
            crate::game::content::weapon_meta(inv.weapons[1]).wep_type as usize
        } else {
            0
        };
        ui.weapon_ammo = [inv.ammo[t1.min(5)], inv.ammo[t2.min(5)]];
        // GML scrDrawPlayerHUD draws ammo text only `if _type != Ammo.None`.
        // Melee shows no count: use -1 sentinel so the HUD skips it.
        if t1 == 0 {
            ui.weapon_ammo[0] = -1;
        }
        if t2 == 0 || inv.weapon_slots <= 1 {
            ui.weapon_ammo[1] = -1;
        }
        ui.ability = ability_name(player.ability).to_string();
        ui.ability_ready = player.ability_cooldown.is_finished();
        ui.crown = crown_short_name(player.crown.to_u8()).to_string();
    }

    ui.floor = run.floor;
    ui.world = run.world;
    ui.floor_in_world = world::floor_in_world(run.floor);
    ui.loop_count = run.loop_count;

    if let Some(score) = score {
        ui.score = score.0;
    }
    if let Some(save) = save {
        ui.high_score = save.high_score;
        ui.best_floor = save.best_floor;
        ui.total_kills = save.total_kills;
    }

    let (choices, ids) = if let Some(ultra) = pending_ultra {
        (
            ultra
                .choices
                .iter()
                .map(|u| {
                    let def = ultra_mutation_def(*u);
                    format!("ULTRA: {} - {}", def.name, def.description)
                })
                .collect::<Vec<_>>(),
            ultra
                .choices
                .iter()
                .map(|u| ultra_skill_index(*u))
                .collect(),
        )
    } else if let Some(p) = pending {
        (
            p.choices
                .iter()
                .map(|m| {
                    let def = mutation_def(*m);
                    format!("{} - {}", def.name, def.description)
                })
                .collect::<Vec<_>>(),
            p.choices.iter().map(|m| mutation_skill_index(*m)).collect(),
        )
    } else {
        (Vec::new(), Vec::new())
    };
    let prev_len = ui.mutation_choices.len();
    ui.mutation_choices = choices;
    ui.mutation_choice_ids = ids;
    // GML LevCont recreates SkillIcons anew each level-up; clear stale selection when offers change
    if ui.mutation_choices.len() != prev_len {
        ui.mutation_selected = None;
    }
    if ui.mutation_choices.is_empty() {
        ui.mutation_selected = None;
    } else if let Some(sel) = ui.mutation_selected {
        if sel >= ui.mutation_choices.len() {
            ui.mutation_selected = None;
        }
    }
    // Death screen mutations – capture once while Player still alive for 0.85s (PlayerDying)
    if run.game_over {
        if let Ok((player, _, _)) = player_q.single() {
            ui.death_mutation_ids = player
                .mutations
                .iter()
                .map(|m| mutation_skill_index(*m))
                .collect();
        }
    } else {
        ui.death_mutation_ids.clear();
    }
}

/// Reset the HUD when a new run begins (called from OnEnter(InGame) after the
/// first sync of the frame).
pub fn reset_hud_flags(bridge: Res<UiBridge>) {
    let mut ui = match bridge.shared.lock() {
        Ok(g) => g,
        Err(p) => {
            bevy::log::warn!("SharedUi poisoned in reset_hud_flags");
            p.into_inner()
        }
    };
    ui.game_over = false;
    ui.mutation_choices.clear();
    ui.mutation_choice_ids.clear();
    ui.mutation_selected = None;
    ui.death_mutation_ids.clear();
    ui.toast.clear();
    ui.toast_timer = 0.0;
    ui.boss_hp = 0;
    ui.boss_max = 0;
    ui.boss_name.clear();
    ui.loop_count = 0;
}
