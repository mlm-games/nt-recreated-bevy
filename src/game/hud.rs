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
    character: Option<Res<SelectedCharacter>>,
    player_q: Query<(&Player, &Health, &Inventory), With<Player>>,
    boss_q: Query<(&Enemy, &Health), With<Enemy>>,
) {
    let Ok(mut ui) = bridge.shared.lock() else {
        return;
    };

    let Some(run) = run else {
        ui.game_over = false;
        ui.mutation_choices.clear();
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
    if let Some((hp, max)) = progression::boss_info(&boss_q) {
        ui.boss_hp = hp;
        ui.boss_max = max;
    }

    if let Some(character) = character {
        ui.character = character_def(character.0).name.to_string();
        ui.selected_character = PLAYABLE_RACES
            .iter()
            .position(|c| *c == character.0)
            .unwrap_or(0);
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
        ui.ability = ability_name(player.ability).to_string();
        ui.ability_ready = player.ability_cooldown.is_finished();
    }

    ui.floor = run.floor;
    ui.world = run.world;
    ui.floor_in_world = world::floor_in_world(run.floor);

    if let Some(score) = score {
        ui.score = score.0;
    }
    if let Some(save) = save {
        ui.high_score = save.high_score;
        ui.best_floor = save.best_floor;
        ui.total_kills = save.total_kills;
    }

    ui.mutation_choices = pending
        .map(|p| {
            p.choices
                .iter()
                .map(|m| {
                    let def = mutation_def(*m);
                    format!("{} — {}", def.name, def.description)
                })
                .collect()
        })
        .unwrap_or_default();
}

/// Reset the HUD when a new run begins (called from OnEnter(InGame) after the
/// first sync of the frame).
pub fn reset_hud_flags(bridge: Res<UiBridge>) {
    if let Ok(mut ui) = bridge.shared.lock() {
        ui.game_over = false;
        ui.mutation_choices.clear();
        ui.toast.clear();
        ui.toast_timer = 0.0;
        ui.boss_hp = 0;
        ui.boss_max = 0;
    }
}
