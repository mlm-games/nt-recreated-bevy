use bevy::prelude::*;
use rand::RngExt;

use crate::app::{OverlayMenu, Paused, PendingUnpause};
use crate::game::audio::GameAudio;
use crate::game::components::*;
use crate::game::content::*;
use crate::game::environment::PropDeathEffect;
use crate::game::secret_areas::{self, SecretTriggers};
use crate::game::world;
use crate::save::SaveData;
use game_utils_bevy::camera_follow::CameraFollow;
use game_utils_bevy::game_feel::{GameFeel, SlowMotion};
use game_utils_bevy::juice::Juice;
use game_utils_bevy::save::SaveManager;
use game_utils_bevy::screen_effects::{ChromaticAberration, ScreenEffects, Trauma};
use game_utils_bevy::vfx::{DamageNumber, TrailGhost, VfxSpawner};

#[derive(bevy::ecs::system::SystemParam)]
pub struct MutationFlagSet<'w> {
    scarier: ResMut<'w, ScarierFace>,
    euphoria: ResMut<'w, Euphoria>,
    open_mind: ResMut<'w, OpenMind>,
    heavy_heart: ResMut<'w, HeavyHeart>,
}

#[derive(bevy::ecs::system::SystemParam)]
pub struct FxSet<'w> {
    trauma: ResMut<'w, Trauma>,
    chroma: ResMut<'w, ChromaticAberration>,
    slow_mo: ResMut<'w, SlowMotion>,
}

#[derive(bevy::ecs::system::SystemParam)]
pub struct PortalSuckCtx<'w> {
    trauma: ResMut<'w, Trauma>,
    chroma: ResMut<'w, ChromaticAberration>,
    toast: ResMut<'w, Toast>,
    triggers: ResMut<'w, SecretTriggers>,
    loop_transition: ResMut<'w, LoopTransition>,
    paused: ResMut<'w, Paused>,
    deferred: ResMut<'w, DeferredFloorGen>,
}

#[derive(Resource, Default)]
pub struct DeferredFloorGen(pub bool);

pub fn setup_run(
    mut commands: Commands,
    catalog: Res<AssetCatalog>,
    asset_server: Res<AssetServer>,
    mut score: ResMut<Score>,
    mut run: ResMut<Run>,
    mut mask: ResMut<FloorMask>,
    mut dirty: ResMut<SaveDirty>,
    mut paused: ResMut<Paused>,
    mut overlay: ResMut<OverlayMenu>,
    mut pending_unpause: ResMut<PendingUnpause>,
    mut toast: ResMut<Toast>,
    character: Res<SelectedCharacter>,
    save: Res<SaveData>,
    camera_q: Query<Entity, With<Camera2d>>,
    mut floor_started: MessageWriter<FloorStarted>,
    bridge: Res<crate::menus::UiBridge>,
) {
    score.0 = 0;
    dirty.0 = false;
    run.floor = 1;
    run.world = 1;
    run.area = crate::game::areas::area_for_floor(1, 0);
    run.loop_count = 0;
    run.floor_in_area = 1;
    run.gen_seed = rand::rng().random_range(0..u64::MAX);
    run.portal_open = false;
    run.game_over = false;
    run.total_kills = 0;
    paused.0 = false;
    *overlay = OverlayMenu::None;
    pending_unpause.0 = None;
    *toast = Toast::default();
    if let Ok(mut ui) = bridge.shared.lock() {
        ui.run_id = ui.run_id.wrapping_add(1);
        ui.overlay = OverlayMenu::None;
        ui.paused = false;
    }

    commands.remove_resource::<PendingMutation>();
    commands.remove_resource::<PendingUltra>();
    commands.insert_resource(DeferredFloorGen(false));
    commands.insert_resource(LoopTransition::default());
    commands.insert_resource(MutationChoice(None));
    commands.insert_resource(ScarierFace(false));
    commands.insert_resource(Euphoria(false));
    commands.insert_resource(OpenMind(false));
    commands.insert_resource(HeavyHeart(false));

    let def = character_def(character.0);

    let loadout = save.race_loadout(character.0);
    let crown = CrownKind::from_u8(loadout.start_crown);

    let skin = if save.skin_unlocked(character.0, loadout.preferred_skin) {
        loadout.preferred_skin
    } else {
        0
    };

    let primary =
        crate::game::content::resolve_start_weapon(sanitize_weapon_id(loadout.start_weapon));

    let explicit_start = sanitize_weapon_id(loadout.start_weapon) != WeaponId::NONE;

    let mut secondary = {
        let saved = sanitize_weapon_id(loadout.stored_weapon);
        if !explicit_start || saved == primary {
            WeaponId::NONE
        } else {
            saved
        }
    };

    // Steroids alone dual-wields.
    if character.0 == crate::game::content::RaceId::Steroids && secondary == WeaponId::NONE {
        secondary = crate::game::content::WEAPON_REVOLVER;
    }

    let equipped = [primary, secondary, WeaponId::NONE];
    let mut starting_ammo = starting_ammo_for(&equipped, character.0, crown);

    if character.0 == RaceId::BigDog {
        starting_ammo[1] = 255;
        starting_ammo[4] = 44;
    }

    let (player_sprite, player_strip) =
        crate::game::anim::sprite_anim(&catalog, &asset_server, def.sprite);
    let anchor = crate::game::content::sprite_anchor(&catalog, def.sprite);
    let fire_rate_mult = if def.passive == PassiveKind::FastReload {
        0.8
    } else {
        1.0
    };

    let mut player_comp = Player {
        speed: crate::game::components::PLAYER_BASE_SPEED,
        speed_mult: def.speed_mult,
        pickup_range: def.pickup_range,
        fire_rate_mult,

        spread_mult: if character.0 == RaceId::Steroids {
            1.8
        } else {
            1.0
        },
        chain_explosions: def.passive == PassiveKind::ChainExplosions,
        shield_on_hit: def.passive == PassiveKind::ShieldOnHit,
        ability: def.ability,
        headless_ready: def.passive == PassiveKind::Headless,
        free_ammo: def.passive == PassiveKind::FreeAmmo,
        crown,
        ..Default::default()
    };

    let mut inv_comp = Inventory {
        weapons: equipped,
        weapon_slots: if character.0 == RaceId::Cuz { 3 } else { 2 },
        current: 0,
        ammo: starting_ammo,
    };

    let mut health_comp = Health {
        hp: def.max_hp,
        max: def.max_hp,
        invuln: ready_timer(),
    };

    crate::game::crown::apply_crown_to_spawn(
        crown,
        &mut player_comp,
        &mut health_comp,
        &mut inv_comp,
    );

    let mut player = commands.spawn((
        GameCleanup,
        player_comp,
        RaceState {
            race: character.0,
            skin: crate::game::content::SkinLetter::from_u8(skin)
                .unwrap_or(crate::game::content::SkinLetter::A),
        },
        inv_comp,
        FireCooldown {
            timer: ready_timer(),
            burst_left: 0,
            burst_timer: ready_timer(),
            timer_b: ready_timer(),
            burst_left_b: 0,
            burst_timer_b: ready_timer(),
        },
        health_comp,
        CrownState::new(crown),
        Team::Player,
        Hitbox {
            radius: PLAYER_RADIUS,
        },
        AimDir(Vec2::Y),
        Velocity(Vec2::ZERO),
        crate::game::anim::PlayerAnim {
            idle: def.sprite,
            walk: def.walk_sprite,
            hurt: crate::game::anim::derive_hurt_path(def.sprite),
            moving: false,
        },
        player_sprite,
        anchor,
        Transform::from_xyz(TILE * 0.5, TILE * 0.5, 20.0),
    ));
    if let Some(player_strip) = player_strip {
        player.insert(player_strip);
    }
    let player = player.id();

    Juice::pop_in(&mut commands, player, 0.25);

    if let Ok(camera) = camera_q.single() {
        commands.entity(camera).insert(CameraFollow {
            target: Some(player),
            follow_weight: 0.20,
            aim_weight: 0.10,
            aim_pull: 0.16,
            base_scale: crate::game::components::NT_CAM_SCALE,
            zoom_speed: 0.08,
            ..default()
        });
    }

    let plan = world::generate_level(&run);
    world::spawn_level(
        &mut commands,
        &catalog,
        &asset_server,
        &run,
        &plan,
        &mut mask,
    );
    floor_started.write(FloorStarted {
        floor: run.floor,
        area: run.area,
    });

    if crown.is_active() {
        toast.show(&format!(
            "{} equipped",
            crate::game::crown::crown_name_for_toast(crown)
        ));
    }
}

pub fn cleanup_run(
    mut commands: Commands,
    q: Query<Entity, With<GameCleanup>>,
    numbers: Query<Entity, With<DamageNumber>>,
    particles: Query<Entity, With<game_utils_bevy::juice::Particle>>,
    trails: Query<Entity, With<TrailGhost>>,
    camera_q: Query<Entity, With<Camera2d>>,
    mut mask: Option<ResMut<FloorMask>>,
) {
    for e in q
        .iter()
        .chain(numbers.iter())
        .chain(particles.iter())
        .chain(trails.iter())
    {
        commands.entity(e).despawn();
    }

    for cam in &camera_q {
        commands.entity(cam).remove::<CameraFollow>();
    }
    if let Some(m) = mask.as_mut() {
        **m = FloorMask::default();
    }
}

fn ready_timer() -> Timer {
    let mut t = Timer::from_seconds(0.01, TimerMode::Once);
    t.finish();
    t
}

pub fn check_level_up(
    commands: &mut Commands,
    trauma: &mut Trauma,
    flash: &mut game_utils_bevy::screen_effects::FlashWhite,
    player: &mut Player,
    health: &mut Health,
    inv: &mut Inventory,
    toast: &mut Toast,
    audio: &GameAudio,
    pos: Vec2,
    race: RaceId,
) {
    let _ = inv;
    let mut leveled = false;

    while player.rads >= player.next_level_rads && player.level < 10 {
        player.rads -= player.next_level_rads;
        player.level += 1;
        player.next_level_rads = player.level.max(1) * 60;
        leveled = true;

        if player.level >= 10 && player.ultra.is_none() {

            player.ultra_pick_owed = true;
        } else if player.level < 10 {
            player.mutation_picks_owed = player.mutation_picks_owed.saturating_add(1);
        }
    }

    if leveled {

        toast.show(if player.ultra_pick_owed && player.level >= 10 {
            "LEVEL ULTRA!"
        } else {
            "LEVEL UP!"
        });
        level_up_feedback(
            commands,
            trauma,
            flash,
            audio,
            pos,
            if player.level >= 10 {
                Color::srgb(1.0, 0.35, 1.0)
            } else {
                Color::srgb(0.25, 1.0, 0.25)
            },
        );
    }

    let _ = health;
    let _ = race;
}

pub fn try_recharge_strong_spirit(player: &mut Player, health: &Health) {
    player.try_recharge_strong_spirit(health);
}

fn begin_between_floor_skill_picks(
    commands: &mut Commands,
    player: &mut Player,
    race: RaceId,
    paused: &mut Paused,
) {
    if player.ultra_pick_owed && player.ultra.is_none() && player.level >= 10 {
        paused.0 = true;
        let choices = ultra_choices_for(race).to_vec();
        commands.insert_resource(PendingUltra { choices });

        return;
    }

    if player.mutation_picks_owed > 0 {
        let choices = roll_mutations(player);
        if choices.is_empty() {

            while player.mutation_picks_owed > 0 {
                let c = roll_mutations(player);
                if c.is_empty() {
                    player.mutation_picks_owed = player.mutation_picks_owed.saturating_sub(1);
                } else {
                    paused.0 = true;
                    commands.insert_resource(PendingMutation { choices: c });
                    return;
                }
            }
            paused.0 = false;
            return;
        }
        paused.0 = true;
        commands.insert_resource(PendingMutation { choices });
    }
}

fn try_start_pending_floor_gen(commands: &mut Commands, run: &Run) {
    let tip = pick_loading_tip(run);
    commands.insert_resource(FloorTransition {
        active: true,
        stage: 1,
        timer: Timer::from_seconds(0.05, TimerMode::Repeating),
        progress: 0.0,
        tip,
    });
    commands.insert_resource(crate::game::vortex::SpiralCtl::warmed_up_for_gml_area(
        crate::game::vortex::gml_area_for_bevy_area(run.area),
    ));
}

fn level_up_feedback(
    commands: &mut Commands,
    trauma: &mut Trauma,
    flash: &mut game_utils_bevy::screen_effects::FlashWhite,
    audio: &GameAudio,
    pos: Vec2,
    color: Color,
) {
    ScreenEffects::add_trauma(trauma, 0.35);
    ScreenEffects::flash_white(flash, 0.15);
    VfxSpawner::spawn_burst(commands, pos, 32, color, (120.0, 360.0));
    audio.play_levelup(commands);
}

fn roll_mutations(player: &mut Player) -> Vec<MutationId> {
    let mut pool: Vec<MutationId> = ALL_MUTATIONS
        .iter()
        .copied()
        .filter(|m| {
            if *m == MutationId::Patience && player.patience_used {
                return false;
            }

            !player.mutations.contains(m)
        })
        .collect();

    let mut rng = rand::rng();
    let mut out = Vec::new();

    let destiny = player.crown == CrownKind::Destiny;
    let want_base = if destiny { 1 } else { 4 };

    let want_base = if player.patience_bonus && !destiny {
        4
    } else {
        want_base
    };
    let want = pool.len().min(want_base);

    player.patience_bonus = false;

    for _ in 0..want {
        let idx = rng.random_range(0..pool.len());
        out.push(pool.remove(idx));
    }

    out
}

pub fn handle_mutation_choice(
    keys: Res<ButtonInput<KeyCode>>,
    mut commands: Commands,
    ultra: Option<ResMut<PendingUltra>>,
    pending: Option<ResMut<PendingMutation>>,
    mut choice: ResMut<MutationChoice>,
    mut paused: ResMut<Paused>,
    mut deferred: ResMut<DeferredFloorGen>,
    mut flags: MutationFlagSet,
    mut player_q: Query<(&mut Player, &mut Health, &mut Inventory, &RaceState), With<Player>>,
    mut fx: FxSet,
    mut toast: ResMut<Toast>,
    run: Res<Run>,
    audio: Res<GameAudio>,
) {
    if ultra.is_none() && pending.is_none() {

        if choice.0.is_some() {
            choice.0 = None;
        }
        return;
    }

    if !paused.0 {
        paused.0 = true;
    }

    let mut picked: Option<usize> = choice.0.take();
    if picked.is_none() {
        if keys.just_pressed(KeyCode::Digit1) {
            picked = Some(0);
        }
        if keys.just_pressed(KeyCode::Digit2) {
            picked = Some(1);
        }
        if keys.just_pressed(KeyCode::Digit3) {
            picked = Some(2);
        }
        if keys.just_pressed(KeyCode::Digit4) {
            picked = Some(3);
        }
    }

    let Some(idx) = picked else {
        return;
    };

    if let Some(mut ultra) = ultra {
        let Some(id) = ultra.choices.get(idx).copied() else {
            return;
        };

        apply_ultra_mutation(
            &mut commands,
            &mut player_q,
            &mut fx.trauma,
            &mut fx.chroma,
            &mut fx.slow_mo,
            &mut toast,
            &audio,
            id,
        );

        commands.spawn((
            GameCleanup,
            crate::game::reactive_audio::QueuedReactiveCue(
                crate::game::reactive_audio::ReactiveCue::UltraChosen,
            ),
        ));

        ultra.choices.clear();
        commands.remove_resource::<PendingUltra>();

        if let Ok((mut player, mut health, _, race_state)) = player_q.single_mut() {
            player.ultra_pick_owed = false;

            if player.mutation_picks_owed > 0 {
                let choices = roll_mutations(&mut player);
                if choices.is_empty() {
                    health.hp = health.max;
                    try_recharge_strong_spirit(&mut player, &health);
                    player.mutation_picks_owed = 0;
                    paused.0 = false;
                    if deferred.0 {
                        try_start_pending_floor_gen(&mut commands, &run);
                        deferred.0 = false;
                    }
                } else {
                    commands.insert_resource(PendingMutation { choices });
                    paused.0 = true;
                }
            } else {
                paused.0 = false;
                if deferred.0 {
                    try_start_pending_floor_gen(&mut commands, &run);
                    deferred.0 = false;
                }
            }
        } else {
            paused.0 = false;
            if deferred.0 {
                try_start_pending_floor_gen(&mut commands, &run);
                deferred.0 = false;
            }
        }
        return;
    }

    let Some(mut pending) = pending else {
        return;
    };

    let Some(id) = pending.choices.get(idx).copied() else {
        return;
    };

    apply_mutation(
        &mut commands,
        &mut player_q,
        &mut flags.scarier,
        &mut flags.euphoria,
        &mut flags.open_mind,
        &mut flags.heavy_heart,
        &mut fx.trauma,
        &mut fx.chroma,
        &mut fx.slow_mo,
        &mut toast,
        &audio,
        id,
    );

    commands.spawn((
        GameCleanup,
        crate::game::reactive_audio::QueuedReactiveCue(
            crate::game::reactive_audio::ReactiveCue::MutationChosen,
        ),
    ));

    pending.choices.clear();
    commands.remove_resource::<PendingMutation>();

    if let Ok((mut player, mut health, _, race_state)) = player_q.single_mut() {
        player.mutation_picks_owed = player.mutation_picks_owed.saturating_sub(1);

        if player.ultra_pick_owed && player.ultra.is_none() && player.level >= 10 {
            let choices = ultra_choices_for(race_state.race).to_vec();
            commands.insert_resource(PendingUltra { choices });
            paused.0 = true;
            return;
        }

        if player.mutation_picks_owed > 0 {
            let choices = roll_mutations(&mut player);
            if choices.is_empty() {
                health.hp = health.max;
                try_recharge_strong_spirit(&mut player, &health);
                player.mutation_picks_owed = 0;
                paused.0 = false;
                if deferred.0 {
                    try_start_pending_floor_gen(&mut commands, &run);
                    deferred.0 = false;
                }
            } else {
                commands.insert_resource(PendingMutation { choices });
                paused.0 = true;
            }
            return;
        }
    }

    paused.0 = false;
    if deferred.0 {
        try_start_pending_floor_gen(&mut commands, &run);
        deferred.0 = false;
    }
}

#[allow(clippy::too_many_arguments)]
fn apply_mutation(
    commands: &mut Commands,
    player_q: &mut Query<(&mut Player, &mut Health, &mut Inventory, &RaceState), With<Player>>,
    scarier: &mut ResMut<ScarierFace>,
    euphoria: &mut ResMut<Euphoria>,
    open_mind: &mut ResMut<OpenMind>,
    heavy_heart: &mut ResMut<HeavyHeart>,
    trauma: &mut Trauma,
    chroma: &mut ChromaticAberration,
    slow_mo: &mut SlowMotion,
    toast: &mut Toast,
    audio: &GameAudio,
    id: MutationId,
) {
    let Ok((mut player, mut health, mut inv, race_state)) = player_q.single_mut() else {
        return;
    };

    player.mutations.push(id);
    let def = mutation_def(id);

    match id {
        MutationId::RhinoSkin => {
            health.max += 4;
            health.hp += 4;
        }
        MutationId::PlutoniumHunger => {
            player.pickup_range += 60.0;
        }
        MutationId::TriggerFingers => {}
        MutationId::RabbitPaw => {
            player.drop_mult += 0.4;
        }
        MutationId::SecondStomach => {
            player.medkit_mult = 2.0;
        }
        MutationId::ScarierFace => {
            scarier.0 = true;
        }
        MutationId::BoilingVeins => {
            player.boiling_veins = true;
            player.veins_threshold = 4;
        }
        MutationId::ImpactWrists => {
            player.knockback_mult *= 1.6;
        }
        MutationId::ExtraFeet => {
            player.speed_mult *= 1.5;
        }
        MutationId::Bloodlust => {
            player.bloodlust = true;
        }
        MutationId::LuckyShot => {
            player.lucky_shot = true;
        }
        MutationId::GammaGuts => {
            player.gamma_guts = true;
        }
        MutationId::BackMuscle => {
            player.back_muscle += 1;
            for kind in [
                AmmoKind::Bullets,
                AmmoKind::Shells,
                AmmoKind::Bolts,
                AmmoKind::Explosives,
                AmmoKind::Energy,
            ] {
                let cap = player.ammo_cap(kind);
                let a = inv.ammo_mut(kind);
                *a = (*a).min(cap);
            }
        }
        MutationId::Euphoria => {
            euphoria.0 = true;
        }
        MutationId::LongArms => {
            player.melee_range_mult *= 1.5;
        }
        MutationId::Stress => {
            player.stress = true;
        }
        MutationId::EagleEyes => {
            player.spread_mult *= 0.4;
        }
        MutationId::OpenMind => {
            open_mind.0 = true;
        }
        MutationId::HeavyHeart => {
            heavy_heart.0 = true;
        }
        MutationId::StrongSpirit => {
            player.strong_spirit_ready = true;
            player.strong_spirit_spent = false;
            player.strong_spirit_area_cleared = false;
        }
        MutationId::SharpTeeth => {
            player.sharp_teeth = true;
        }
        MutationId::LastWish => {

            health.hp = health.max;
            try_recharge_strong_spirit(&mut player, &health);
            let add = |inv: &mut Inventory, player: &Player, kind: AmmoKind, amount: i32| {
                let cap = player.ammo_cap(kind);
                let slot = inv.ammo_mut(kind);
                *slot = (*slot + amount).min(cap);
            };
            add(&mut inv, &player, AmmoKind::Bullets, 200);
            add(&mut inv, &player, AmmoKind::Shells, 20);
            add(&mut inv, &player, AmmoKind::Bolts, 20);
            add(&mut inv, &player, AmmoKind::Explosives, 20);
            add(&mut inv, &player, AmmoKind::Energy, 20);
            player.last_wish_used = true;
        }

        MutationId::BoltMarrow => {
            player.bolt_marrow = true;
        }
        MutationId::Hammerhead => {
            player.hammerhead = true;
        }
        MutationId::LaserBrain => {
            player.laser_brain = true;
        }
        MutationId::RecycleGland => {
            player.recycle_gland = true;
        }
        MutationId::ShotgunShoulders => {
            player.shotgun_shoulders = true;
        }
        MutationId::ThroneButt => {
            player.throne_butt = true;
            apply_throne_butt_immediate_bonus(&mut player, &mut health, &mut inv, race_state.race);
        }
        MutationId::Patience => {
            player.patience_used = true;
            player.patience_bonus = true;
        }
    }

    ScreenEffects::add_trauma(trauma, 0.3);
    ScreenEffects::chromatic_pulse(chroma, 0.25);
    GameFeel::slow_motion(slow_mo, 0.5, 0.35);
    audio.play_levelup(commands);
    toast.show(&format!("{}: {}", def.name, def.description));
}

fn apply_throne_butt_immediate_bonus(
    player: &mut Player,
    health: &mut Health,
    inv: &mut Inventory,
    race: RaceId,
) {
    player.ultra_ability_mult *= 1.15;

    match race {
        RaceId::Fish => {
            player.speed_mult *= 1.05;
        }
        RaceId::Crystal => {
            health.max += 2;
            health.hp += 2;
        }
        RaceId::Eyes => {
            player.pickup_range += 45.0;
        }
        RaceId::Melting => {
            player.chain_explosions = true;
        }
        RaceId::Plant => {
            player.speed_mult *= 1.08;
        }
        RaceId::Venuz => {
            player.fire_rate_mult *= 0.92;
        }
        RaceId::Steroids => {
            for kind in [
                AmmoKind::Bullets,
                AmmoKind::Shells,
                AmmoKind::Bolts,
                AmmoKind::Explosives,
                AmmoKind::Energy,
            ] {
                *inv.ammo_mut(kind) += ammo_pickup_amount(kind);
            }
        }
        RaceId::Robot => {
            player.free_ammo = true;
        }
        RaceId::Chicken => {
            player.headless_ready = true;
        }
        RaceId::Rebel => {
            health.hp = (health.hp + 1).min(health.max);
        }
        RaceId::Horror => {
            player.lucky_shot = true;
        }
        RaceId::Rogue => {
            player.boiling_veins = true;
        }
        RaceId::BigDog => {
            player.ultra_damage_mult *= 1.1;
        }
        RaceId::Skeleton => {
            player.bloodlust = true;
        }
        RaceId::Frog => {
            player.gamma_guts = true;
        }
        RaceId::Cuz | RaceId::Random => {
            player.fire_rate_mult *= 0.95;
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn apply_ultra_mutation(
    commands: &mut Commands,
    player_q: &mut Query<(&mut Player, &mut Health, &mut Inventory, &RaceState), With<Player>>,
    trauma: &mut Trauma,
    chroma: &mut ChromaticAberration,
    slow_mo: &mut SlowMotion,
    toast: &mut Toast,
    audio: &GameAudio,
    id: UltraMutationId,
) {
    let Ok((mut player, mut health, mut inv, race_state)) = player_q.single_mut() else {
        return;
    };

    player.ultra = Some(id);

    match id {
        UltraMutationId::FishGunWarrant => {
            player.fire_rate_mult *= 0.75;
            player.spread_mult *= 0.85;
        }
        UltraMutationId::FishConfiscate => {
            player.drop_mult += 0.35;
            for kind in [
                AmmoKind::Bullets,
                AmmoKind::Shells,
                AmmoKind::Bolts,
                AmmoKind::Explosives,
                AmmoKind::Energy,
            ] {
                let slot = inv.ammo_mut(kind);
                *slot = (*slot + ammo_pickup_amount(kind) * 2).min(ammo_max(kind));
            }
        }

        UltraMutationId::CrystalFortress => {
            health.max += 6;
            health.hp += 6;
            player.ultra_ability_mult *= 1.4;
        }
        UltraMutationId::CrystalJuggernaut => {
            health.max += 3;
            health.hp += 3;
            player.speed_mult *= 1.18;
        }

        UltraMutationId::EyesMonsterStyle => {
            player.pickup_range += 160.0;
            player.ultra_ability_mult *= 1.45;
        }
        UltraMutationId::EyesProjectileStyle => {
            player.ultra_ability_mult *= 1.2;
            player.euphoria = true;
        }

        UltraMutationId::MeltingBrainCapacity => {
            player.chain_explosions = true;
            player.ultra_ability_mult *= 1.6;
        }
        UltraMutationId::MeltingDetachment => {
            health.max += 2;
            health.hp += 2;
            player.strong_spirit_ready = true;
        }

        UltraMutationId::PlantTrapper => {
            player.ultra_ability_mult *= 1.6;
            player.speed_mult *= 1.06;
        }
        UltraMutationId::PlantKiller => {
            player.speed_mult *= 1.18;
            player.fire_rate_mult *= 0.85;
        }

        UltraMutationId::VenuzBack2Bizniz => {
            player.ultra_ability_mult *= 1.5;
            player.fire_rate_mult *= 0.9;
        }
        UltraMutationId::VenuzGunGod => {
            player.fire_rate_mult *= 0.72;
            player.spread_mult *= 0.7;
        }

        UltraMutationId::SteroidsAmbidextrous => {
            player.fire_rate_mult *= 0.7;
            player.knockback_mult *= 0.85;
        }
        UltraMutationId::SteroidsGetArmed => {
            player.ultra_ability_mult *= 1.6;
            for kind in [
                AmmoKind::Bullets,
                AmmoKind::Shells,
                AmmoKind::Bolts,
                AmmoKind::Explosives,
                AmmoKind::Energy,
            ] {
                *inv.ammo_mut(kind) = player.ammo_cap(kind);
            }
        }

        UltraMutationId::RobotRefinedTaste => {
            player.free_ammo = true;
            player.medkit_mult *= 1.5;
        }
        UltraMutationId::RobotRegurgitate => {
            player.free_ammo = true;
            player.drop_mult += 0.5;
            player.ultra_ability_mult *= 1.35;
        }

        UltraMutationId::ChickenHarderToKill => {
            player.headless_ready = true;
            health.max += 2;
            health.hp += 2;
        }
        UltraMutationId::ChickenDetermination => {
            player.ultra_damage_mult *= 1.25;
            player.speed_mult *= 1.1;
        }

        UltraMutationId::RebelPersonalGuard => {
            player.ultra_ability_mult *= 1.45;
            health.max += 2;
            health.hp += 2;
        }
        UltraMutationId::RebelRiot => {
            player.ultra_ability_mult *= 1.8;
            player.fire_rate_mult *= 0.9;
        }

        UltraMutationId::HorrorStalker => {
            player.ultra_ability_mult *= 1.6;
            player.laser_brain = true;
        }
        UltraMutationId::HorrorAnomaly => {
            player.pickup_range += 80.0;
            player.lucky_shot = true;
            player.laser_brain = true;
        }

        UltraMutationId::RogueSuperBlastArmor => {
            player.boiling_veins = true;
            player.veins_threshold = 6;
            health.max += 2;
            health.hp += 2;
        }
        UltraMutationId::RoguePortalStrike => {
            player.ultra_ability_mult *= 1.7;
            player.fire_rate_mult *= 0.9;
        }

        UltraMutationId::BigDogHeavyArtillery => {
            player.ultra_damage_mult *= 1.35;
            player.knockback_mult *= 1.25;
        }
        UltraMutationId::BigDogGuardian => {
            health.max += 5;
            health.hp += 5;
            player.strong_spirit_ready = true;
        }

        UltraMutationId::SkeletonBloodArmor => {
            health.max += 4;
            health.hp += 4;
            player.bloodlust = true;
        }
        UltraMutationId::SkeletonNecromancy => {
            player.bloodlust = true;
            player.recycle_gland = true;
            player.lucky_shot = true;
        }

        UltraMutationId::FrogToxicLord => {
            player.ultra_ability_mult *= 1.7;
            player.gamma_guts = true;
        }
        UltraMutationId::FrogSwampBody => {
            health.max += 3;
            health.hp += 3;
            player.boiling_veins = true;
        }

        UltraMutationId::CuzHoarder => {
            inv.weapon_slots = MAX_WEAPON_SLOTS;
            player.drop_mult += 0.25;
        }
        UltraMutationId::CuzQuickSwap => {
            inv.weapon_slots = MAX_WEAPON_SLOTS;
            player.fire_rate_mult *= 0.82;
            player.ultra_ability_mult *= 1.4;
        }
    }

    let def = ultra_mutation_def(id);

    ScreenEffects::add_trauma(trauma, 0.55);
    ScreenEffects::chromatic_pulse(chroma, 0.4);
    GameFeel::slow_motion(slow_mo, 0.35, 0.5);
    audio.play_levelup(commands);
    toast.show(&format!("ULTRA - {}: {}", def.name, def.description));

    debug_assert!(
        ultra_choices_for(race_state.race).contains(&id) || race_state.race == RaceId::Random,
        "picked ultra {id:?} outside race {:?}",
        race_state.race,
    );
}

pub fn portal_check(
    mut commands: Commands,
    catalog: Res<AssetCatalog>,
    asset_server: Res<AssetServer>,
    loop_transition: Res<LoopTransition>,
    mut run: ResMut<Run>,
    mut trauma: ResMut<Trauma>,
    mut chroma: ResMut<ChromaticAberration>,
    mask: Res<FloorMask>,
    enemy_q: Query<(), With<Enemy>>,
    enemy_shots: Query<(Entity, &Team), With<crate::game::components::Projectile>>,
    audio: Res<GameAudio>,
) {
    if run.game_over || run.portal_open {
        return;
    }

    if loop_transition.blocks_portal() {
        return;
    }
    if !enemy_q.is_empty() {
        return;
    }

    for (e, team) in &enemy_shots {
        if *team != Team::Player {
            commands.entity(e).despawn();
        }
    }

    run.portal_open = true;
    commands.spawn((
        GameCleanup,
        crate::game::reactive_audio::QueuedReactiveCue(
            crate::game::reactive_audio::ReactiveCue::PortalOpen,
        ),
    ));

    let mut rng = rand::rng();
    let pos = mask.random_floor_pos(&mut rng, 80.0);

    let (portal_sprite, portal_strip) =
        crate::game::anim::sprite_anim(&catalog, &asset_server, "images/sprPortal.png");
    let mut pc = commands.spawn((
        GameCleanup,
        LevelCleanup,
        Portal,
        portal_sprite,
        Transform::from_xyz(pos.x, pos.y, 5.0),
    ));
    if let Some(portal_strip) = portal_strip {
        pc.insert(portal_strip);
    }

    commands.spawn((
        GameCleanup,
        LevelCleanup,
        PortalShock {
            timer: Timer::from_seconds(2.0 / 30.0, TimerMode::Once),
            radius: 72.0,
        },
        Transform::from_xyz(pos.x, pos.y, 6.0),
    ));
    commands.spawn((
        GameCleanup,
        LevelCleanup,
        PortalClear {
            timer: Timer::from_seconds(5.0 / 30.0, TimerMode::Once),
        },
        Transform::from_xyz(pos.x, pos.y, 6.0),
    ));

    VfxSpawner::spawn_burst(
        &mut commands,
        pos,
        4,
        Color::srgb(0.5, 0.8, 1.0),
        (60.0, 160.0),
    );

    ScreenEffects::add_trauma(&mut trauma, 0.25);
    ScreenEffects::chromatic_pulse(&mut chroma, 0.25);
    audio.play_portal(&mut commands);
}

pub fn portal_attract(
    time: Res<Time<Fixed>>,
    mut commands: Commands,
    catalog: Res<AssetCatalog>,
    asset_server: Res<AssetServer>,
    mut player_q: Query<
        (
            Entity,
            &mut Transform,
            Option<&mut crate::game::anim::SpriteAnim>,
            Option<&mut Sprite>,
            Option<&crate::game::anim::PlayerAnim>,
            Option<&AimDir>,
        ),
        (With<Player>, Without<Portal>, Without<PortalSucking>),
    >,
    mut weapon_q: Query<
        (Entity, &mut Transform, &Pickup, Option<&mut GroundPhysics>),
        (Without<Player>, Without<Portal>),
    >,
    portal_q: Query<&Transform, With<Portal>>,
    mask: Res<FloorMask>,
    run: Res<Run>,
) {
    if run.game_over || !run.portal_open {
        return;
    }
    let Ok(portal_tf) = portal_q.single() else {
        return;
    };
    let tpos = portal_tf.translation.truncate();
    let dt = time.delta_secs();
    let frames = dt * crate::app::NT_SIM_HZ as f32;

    let mut attract_step = |ppos: Vec2| -> Option<(Vec2, f32, f32)> {
        let dist = ppos.distance(tpos);
        if dist > 96.0 || dist < 0.5 {
            return None;
        }
        if crate::game::walls::segment_hits_wall(ppos, tpos, &mask) {
            return None;
        }
        let spd = if dist > 48.0 { 2.0 } else { 5.0 };
        let dir = (tpos - ppos).normalize_or_zero();
        Some((dir, spd, dist))
    };

    if let Ok((player_e, mut ptf, mut anim, mut sprite, pa, aim)) = player_q.single_mut() {
        let ppos = ptf.translation.truncate();
        if let Some((dir, spd, dist)) = attract_step(ppos) {
            let delta = dir * spd * 30.0 * dt;
            let nx = Vec2::new(ppos.x + delta.x, ppos.y);
            let ny = Vec2::new(ppos.x, ppos.y + delta.y);
            if mask.is_walkable(nx) {
                ptf.translation.x = nx.x;
            }
            if mask.is_walkable(ny) {
                ptf.translation.y = ny.y;
            }
            if dist <= 48.0 {

                let right = aim
                    .map(|a| if a.0.x < 0.0 { -1.0 } else { 1.0 })
                    .unwrap_or(1.0);
                ptf.rotation *= Quat::from_rotation_z((-30.0_f32.to_radians()) * right * frames);
                if let (Some(anim), Some(sprite), Some(pa)) = (anim.as_mut(), sprite.as_mut(), pa) {

                    if !(anim.oneshot && !anim.finished) {
                        crate::game::anim::play_hurt(
                            &mut commands,
                            player_e,
                            &catalog,
                            &asset_server,
                            anim,
                            sprite,
                            pa.hurt,
                            pa.idle,
                            Some(pa.walk),
                        );
                    }
                }
            } else {

                ptf.rotation = Quat::IDENTITY;
            }
        }
    }

    for (e, mut wtf, pickup, gp) in &mut weapon_q {
        let PickupKind::Weapon(_) = pickup.kind else {
            continue;
        };

        if wtf.scale.x < 0.01 {
            continue;
        }
        let ppos = wtf.translation.truncate();
        let Some((dir, spd, _dist)) = attract_step(ppos) else {
            continue;
        };

        let delta = dir * spd * 30.0 * dt;
        let nx = Vec2::new(ppos.x + delta.x, ppos.y);
        let ny = Vec2::new(ppos.x, ppos.y + delta.y);
        if mask.is_walkable(nx) {
            wtf.translation.x = nx.x;
        }
        if mask.is_walkable(ny) {
            wtf.translation.y = ny.y;
        }

        let rotspeed = gp.as_ref().map(|g| g.rotspeed).unwrap_or(0.8);
        wtf.rotation *= Quat::from_rotation_z((-15.0_f32.to_radians()) * rotspeed * frames);

        if let Some(mut gp) = gp {
            gp.vel *= 0.8;
        }
        let _ = e;
    }
}

pub fn tick_portal_shock(
    time: Res<Time<Fixed>>,
    mut commands: Commands,
    catalog: Res<AssetCatalog>,
    asset_server: Res<AssetServer>,
    mut shocks: Query<(Entity, &Transform, &mut PortalShock)>,
    mut props: Query<
        (
            Entity,
            &mut Prop,
            &Transform,
            Option<&PropDeathEffect>,
            Option<&PropSprites>,
        ),
        With<Prop>,
    >,
    mut chests: Query<(Entity, &Transform, &Pickup), (Without<OpenedChest>, Without<Player>)>,
    mut anims: Query<&mut crate::game::anim::SpriteAnim>,
    mut sprites: Query<&mut Sprite>,
    mut enemy_shots: Query<(Entity, &Transform, &Team), With<crate::game::components::Projectile>>,
    entrances: Query<&SecretEntrance>,
    mut secrets: ResMut<SecretTriggers>,
    run: Res<Run>,
    player_q: Query<&Player>,
) {

    let hasted = player_q
        .single()
        .is_ok_and(|p| p.crown == crate::game::content::CrownKind::Haste);
    for (shock_e, shock_tf, mut shock) in &mut shocks {
        shock.timer.tick(time.delta());
        let center = shock_tf.translation.truncate();

        let mut killed: Vec<(
            Entity,
            Vec2,
            bool,
            Option<PropDeathEffect>,
            Option<PropSprites>,
        )> = Vec::new();
        for (prop_e, mut prop, prop_tf, death, ps) in &mut props {
            if !prop.destructible || prop.hp <= 0 {
                continue;
            }
            let ppos = prop_tf.translation.truncate();
            let half = prop.size * 0.5;
            let closest = Vec2::new(
                center.x.clamp(ppos.x - half.x, ppos.x + half.x),
                center.y.clamp(ppos.y - half.y, ppos.y + half.y),
            );
            if center.distance(closest) > shock.radius {
                continue;
            }
            prop.hp = 0;
            killed.push((prop_e, ppos, prop.explosive, death.copied(), ps.copied()));
        }
        for (prop_e, ppos, explosive, death, ps) in killed {
            if let Some(sprites) = ps {
                crate::game::environment::spawn_prop_corpse(
                    &mut commands,
                    &catalog,
                    &asset_server,
                    ppos,
                    &sprites,
                );
            }
            crate::game::environment::spawn_prop_death_effect(
                &mut commands,
                ppos,
                death,
                explosive,
                None,
            );
            if let Ok(entrance) = entrances.get(prop_e) {
                secrets.queue(entrance.target);
            }
            commands.entity(prop_e).despawn();
        }

        for (chest_e, chest_tf, pickup) in &mut chests {
            let PickupKind::Chest(kind) = pickup.kind else {
                continue;
            };
            let cpos = chest_tf.translation.truncate();
            if center.distance(cpos) > shock.radius {
                continue;
            }

            crate::game::pickups::open_chest_shock(
                &mut commands,
                &catalog,
                &asset_server,
                &mut anims,
                &mut sprites,
                chest_e,
                kind,
            );
            match kind {
                ChestKind::Weapon => {
                    let weapon = crate::game::combat::random_weapon(&mut rand::rng());
                    crate::game::pickups::spawn_pickup(
                        &mut commands,
                        &catalog,
                        &asset_server,
                        PickupKind::Weapon(weapon),
                        cpos,
                        0,
                        false,
                    );
                }
                ChestKind::Ammo => {

                    for _ in 0..2 {
                        crate::game::pickups::spawn_pickup(
                            &mut commands,
                            &catalog,
                            &asset_server,
                            PickupKind::Ammo(AmmoKind::None, 0),
                            cpos,
                            run.loop_count,
                            hasted,
                        );
                    }
                }
                ChestKind::Rad => {
                    for _ in 0..25 {
                        let ang = rand::rng().random_range(0.0..std::f32::consts::TAU);
                        let d = rand::rng().random_range(6.0..26.0);
                        crate::game::pickups::spawn_pickup(
                            &mut commands,
                            &catalog,
                            &asset_server,
                            PickupKind::Rad(1),
                            cpos + Vec2::new(ang.cos() * d, ang.sin() * d),
                            0,
                            false,
                        );
                    }
                }
            }
        }

        for (proj_e, proj_tf, team) in &mut enemy_shots {
            if *team == Team::Player {
                continue;
            }
            if proj_tf.translation.truncate().distance(center) <= shock.radius {
                commands.entity(proj_e).despawn();
            }
        }
        if shock.timer.just_finished() {
            commands.entity(shock_e).despawn();
        }
    }
}

pub fn tick_portal_clear(
    time: Res<Time<Fixed>>,
    mut commands: Commands,
    mut clears: Query<(Entity, &Transform, &mut PortalClear)>,
    walls: Query<(Entity, &WallCell, &Transform), With<WallTile>>,
) {
    for (clear_e, clear_tf, mut clear) in &mut clears {
        clear.timer.tick(time.delta());
        let center = clear_tf.translation.truncate();
        for (_, cell, wtf) in &walls {
            if wtf.translation.truncate().distance(center) < 48.0 {
                commands.spawn((
                    GameCleanup,
                    LevelCleanup,
                    PendingWallBreak {
                        cell: (cell.0, cell.1),
                        pos: wtf.translation.truncate(),
                        spawn_floor: true,
                    },
                ));
            }
        }
        if clear.timer.just_finished() {
            commands.entity(clear_e).despawn();
        }
    }
}

// Portal latch blocks re-trigger.
pub fn portal_enter(
    mut commands: Commands,
    run: Res<Run>,
    portal_q: Query<(Entity, &Transform, Option<&PortalClosing>), With<Portal>>,
    mut player_q: Query<
        (
            Entity,
            &Transform,
            &mut Velocity,
            &RaceState,
            &mut Inventory,
            &Player,
        ),
        (With<Player>, Without<Portal>, Without<PortalSucking>),
    >,
    mut weapon_q: Query<(Entity, &Transform, &Pickup), (Without<Player>, Without<Portal>)>,
) {
    if run.game_over {
        return;
    }

    let Ok((portal_e, portal_tf, closing)) = portal_q.single() else {
        return;
    };

    if closing.is_some() {
        return;
    }

    let Ok((player_e, player_tf, mut vel, race_state, mut inv, player)) = player_q.single_mut()
    else {
        return;
    };

    let ppos = player_tf.translation.truncate();
    let tpos = portal_tf.translation.truncate();
    if ppos.distance(tpos) > 48.0 {
        return;
    }
    let _ = &mut vel;

    commands.entity(portal_e).insert(PortalClosing {
        timer: Timer::from_seconds(90.0 / 30.0, TimerMode::Once),
    });

    if race_state.race == RaceId::Robot {
        for (wep_e, wep_tf, pickup) in &mut weapon_q {
            let PickupKind::Weapon(w) = pickup.kind else {
                continue;
            };
            if wep_tf.translation.truncate().distance(tpos) > 96.0 {
                continue;
            }

            let kind = weapon_ammo(w);
            if kind != AmmoKind::None {
                let add = ammo_pickup_amount(kind);
                let slot = inv.ammo_mut(kind);
                *slot = (*slot + add).min(player.ammo_cap(kind));
            }
            VfxSpawner::spawn_burst(
                &mut commands,
                wep_tf.translation.truncate(),
                6,
                Color::srgb(0.6, 0.6, 0.6),
                (30.0, 90.0),
            );
            commands.entity(wep_e).despawn();
        }
    }

    commands.entity(player_e).insert(PortalSucking {
        portal: portal_e,
        timer: Timer::from_seconds(0.55, TimerMode::Once),
        start_pos: ppos,
        target_pos: tpos,
    });
    commands.spawn((
        GameCleanup,
        crate::game::reactive_audio::QueuedReactiveCue(
            crate::game::reactive_audio::ReactiveCue::PortalEnter,
        ),
    ));
}

pub fn tick_portal_suck(
    time: Res<Time<Fixed>>,
    mut commands: Commands,
    catalog: Res<AssetCatalog>,
    asset_server: Res<AssetServer>,
    mut run: ResMut<Run>,
    mut mask: ResMut<FloorMask>,
    audio: Res<GameAudio>,
    mut floor_started: MessageWriter<FloorStarted>,
    mut ctx: PortalSuckCtx,
    level_q: Query<Entity, With<LevelCleanup>>,
    weapon_q: Query<&Pickup, Without<Player>>,
    carried: Res<PortalCarriedWeapons>,
    mut player_q: Query<
        (
            Entity,
            &mut Transform,
            &mut Health,
            &mut Player,
            &RaceState,
            &mut PortalSucking,
            Option<&mut crate::game::anim::SpriteAnim>,
            Option<&mut Sprite>,
        ),
        With<Player>,
    >,
) {
    let Ok((
        player_e,
        mut player_tf,
        mut health,
        mut player,
        race_state,
        mut suck,
        mut anim,
        mut sprite,
    )) = player_q.single_mut()
    else {
        return;
    };

    suck.timer.tick(time.delta());
    let t = suck.timer.fraction().clamp(0.0, 1.0);

    let ease = t * t;
    let pos = suck.start_pos.lerp(suck.target_pos, ease);
    player_tf.translation.x = pos.x;
    player_tf.translation.y = pos.y;
    player_tf.rotation = Quat::from_rotation_z(t * std::f32::consts::TAU * 2.0);
    let scale = 1.0 - ease * 0.85;
    player_tf.scale = Vec3::splat(scale.max(0.08));

    if let (Some(anim), Some(sprite)) = (anim.as_mut(), sprite.as_mut()) {
        anim.oneshot = true;
        anim.finished = true;
        sprite.color.set_alpha(1.0 - ease * 0.5);
    }

    ScreenEffects::add_trauma(&mut ctx.trauma, 0.02);
    if !suck.timer.just_finished() {
        return;
    }

    let portal_e = suck.portal;
    commands.entity(player_e).remove::<PortalSucking>();
    player_tf.rotation = Quat::IDENTITY;
    player_tf.scale = Vec3::ONE;
    if let Some(sprite) = sprite.as_mut() {
        sprite.color.set_alpha(1.0);
    }

    if run.floor == 1 && run.area == crate::game::areas::AreaId::Desert {
        let mut swords = carried.0.iter().filter(|w| w.0 == 46).count() as u32;
        for pickup in &weapon_q {
            if matches!(pickup.kind, PickupKind::Weapon(w) if w.0 == 46) {
                swords += 1;
            }
        }
        run.blackswords += swords;
    }

    for e in &level_q {
        commands.entity(e).despawn();
    }
    commands.entity(portal_e).despawn();

    let looped = crate::game::loop_transition::try_apply_loop_portal_transition(
        &mut run,
        &mut ctx.loop_transition,
        &mut ctx.trauma,
    );

    if looped {
        commands.spawn((
            GameCleanup,
            crate::game::reactive_audio::QueuedReactiveCue(
                crate::game::reactive_audio::ReactiveCue::LoopComplete,
            ),
        ));
    }

    let entered_secret = if looped {
        None
    } else {
        secret_areas::apply_secret_transition(&mut run, &mut ctx.triggers)
    };

    if let Some(secret) = entered_secret {
        commands.spawn((
            GameCleanup,
            crate::game::reactive_audio::QueuedReactiveCue(
                crate::game::reactive_audio::ReactiveCue::SecretFound,
            ),
        ));
        ctx.toast.show(&format!("ENTERING {}", secret.name()));
    } else if !looped {
        commands.spawn((
            GameCleanup,
            crate::game::reactive_audio::QueuedReactiveCue(
                crate::game::reactive_audio::ReactiveCue::PortalEnter,
            ),
        ));
        ctx.toast.show(&format!(
            "FLOOR {}-{}",
            run.world,
            world::floor_in_world(run.floor)
        ));
    } else {
        ctx.toast.show(&format!("LOOP {}", run.loop_count));
    }

    if player.strong_spirit_spent {
        player.strong_spirit_area_cleared = true;
    }

    if player.ultra_pick_owed || player.mutation_picks_owed > 0 {

        ctx.deferred.0 = true;
        begin_between_floor_skill_picks(
            &mut commands,
            &mut player,
            race_state.race,
            &mut ctx.paused,
        );
        if player.mutation_picks_owed == 0 && !player.ultra_pick_owed {
            if !ctx.paused.0 {
                ctx.deferred.0 = false;

            } else {
                player_tf.translation = Vec3::new(10000.0, 10000.0, 20.0);
                return;
            }
        } else {
            player_tf.translation = Vec3::new(10000.0, 10000.0, 20.0);
            return;
        }
    }

    ctx.deferred.0 = false;
    let tip = pick_loading_tip(&run);
    commands.insert_resource(FloorTransition {
        active: true,
        stage: 1,
        timer: Timer::from_seconds(0.05, TimerMode::Repeating),
        progress: 0.0,
        tip,
    });
    commands.insert_resource(crate::game::vortex::SpiralCtl::warmed_up_for_gml_area(
        crate::game::vortex::gml_area_for_bevy_area(run.area),
    ));

    player_tf.translation = Vec3::new(10000.0, 10000.0, 20.0);

    let _ = (
        &catalog,
        &asset_server,
        &mut mask,
        &mut floor_started,
        &mut health,
        &race_state,
        &player,
    );
    let _ = (&mut ctx.chroma, &audio);
}

pub fn tick_floor_transition(
    time: Res<Time<Fixed>>,
    mut commands: Commands,
    catalog: Res<AssetCatalog>,
    asset_server: Res<AssetServer>,
    mut run: ResMut<Run>,
    mut mask: ResMut<FloorMask>,
    mut ft: ResMut<FloorTransition>,
    mut trauma: ResMut<Trauma>,
    mut chroma: ResMut<ChromaticAberration>,
    audio: Res<GameAudio>,
    mut floor_started: MessageWriter<FloorStarted>,
    mut player_q: Query<(&mut Transform, &mut Health, &mut Player, &RaceState), With<Player>>,
    mut carried: ResMut<PortalCarriedWeapons>,
    open_mind: Res<OpenMind>,
    mut spiral: Option<ResMut<crate::game::vortex::SpiralCtl>>,
) {
    if !ft.active {
        return;
    }
    match ft.stage {
        1 => {
            ft.progress = (ft.progress + time.delta_secs() * 0.85).min(1.0);
            if ft.progress >= 1.0 {
                ft.stage = 2;
                ft.timer = Timer::from_seconds(4.0 / 30.0, TimerMode::Once);
            }
        }
        2 => {
            ft.timer.tick(time.delta());
            if !ft.timer.just_finished() {
                return;
            }
            let Ok((mut tf, mut health, mut player, race)) = player_q.single_mut() else {
                return;
            };
            let plan = world::generate_level(&run);

            let mut plan = plan;
            if open_mind.0 {
                world::apply_open_mind_bonus(&mut plan, run.area, run.floor_in_area);
            }
            world::spawn_level(
                &mut commands,
                &catalog,
                &asset_server,
                &run,
                &plan,
                &mut mask,
            );
            floor_started.write(FloorStarted {
                floor: run.floor,
                area: run.area,
            });
            health.hp = (health.hp + 1).min(health.max);
            try_recharge_strong_spirit(&mut player, &health);
            if character_def(race.race).passive == PassiveKind::Headless {
                player.headless_ready = true;
            }
            if let Some(c) = mask
                .cells
                .iter()
                .min_by_key(|c| (mask.cell_center(**c).length() * 1000.0) as i32)
            {
                let p = mask.cell_center(*c);
                tf.translation = Vec3::new(p.x, p.y, 20.0);
            } else {
                tf.translation = Vec3::new(0.0, 0.0, 20.0);
            }
            tf.rotation = Quat::IDENTITY;
            tf.scale = Vec3::ONE;
            run.portal_open = false;
            ft.active = false;

            if !carried.0.is_empty() {
                let base = tf.translation.truncate();
                for (i, w) in carried.0.drain(..).enumerate() {
                    let ang = (i as f32) * std::f32::consts::TAU / 4.0;
                    crate::game::pickups::spawn_pickup(
                        &mut commands,
                        &catalog,
                        &asset_server,
                        PickupKind::Weapon(w),
                        base + Vec2::new(ang.cos(), ang.sin()) * 24.0,
                        0,
                        false,
                    );
                }
            }

            if let Some(mut s) = spiral {
                if s.alive {
                    s.alive = false;
                    s.death_tick = Some(s.ticks);
                }
            }
            ScreenEffects::add_trauma(&mut trauma, 0.55);
            ScreenEffects::chromatic_pulse(&mut chroma, 0.65);
            audio.play_portal(&mut commands);
        }
        _ => {}
    }
}

fn pick_loading_tip(_run: &Run) -> String {
    const TIPS: &[&str] = &[
        "KILL ENEMIES TO LEVEL UP",
        "MUTATIONS STACK AFTER EACH LEVEL",
        "PORTALS OPEN WHEN THE AREA IS CLEAR",
        "WATCH YOUR AMMO - REVOLVER IS 3 DMG",
        "HOLD SHIFT TO AIM SLOWLY",
        "BOILING VEINS SAVES YOU AT LOW HP",
        "RHINO SKIN GIVES +4 MAX HP",
        "YOU CAN CARRY TWO WEAPONS",
        "RAD CANISTERS DROP FROM STRONG ENEMIES",
        "LOOP TO FIND NEW MUTATIONS",
    ];
    let mut rng = rand::rng();
    TIPS[rng.random_range(0..TIPS.len())].to_string()
}

pub fn animate_portal(time: Res<Time<Fixed>>, mut q: Query<&mut Transform, With<Portal>>) {
    let s = 1.0 + (time.elapsed_secs() * 8.0).sin() * 0.12;
    for mut tf in &mut q {
        tf.scale = Vec3::splat(s);
        tf.rotate_z(time.delta_secs() * 2.2);
    }
}

pub fn apply_floor_reach_unlocks(
    mut applied: Local<u32>,
    run: Res<Run>,
    mut dirty: ResMut<SaveDirty>,
    mut toast: ResMut<Toast>,
    mut save: ResMut<SaveData>,
) {
    if run.floor <= *applied {
        return;
    }
    *applied = run.floor;
    let unlocked = crate::game::generated::unlocks::check_progress_unlocks(
        &mut save,
        run.floor,
        run.loop_count,
        false,
        false,
        false,
    );
    if !unlocked.is_empty() {
        dirty.0 = true;
        for race in unlocked {
            toast.show(&format!(
                "UNLOCKED {}",
                character_def(race).name.to_ascii_uppercase()
            ));
        }
    }
}

pub fn flush_dirty_save(
    mut accumulator: Local<f32>,
    time: Res<Time<Fixed>>,
    mut dirty: ResMut<SaveDirty>,
    save: Res<SaveData>,
    manager: Res<SaveManager>,
) {
    if !dirty.0 {
        return;
    }
    *accumulator += time.delta_secs();
    if *accumulator >= 5.0 {
        *accumulator = 0.0;
        let _ = manager.save(&*save);
        dirty.0 = false;
    }
}

pub fn flush_dirty_save_once(
    dirty: Res<SaveDirty>,
    save: Res<SaveData>,
    manager: Res<SaveManager>,
) {
    if dirty.0 {
        let _ = manager.save(&*save);
    }
}

pub fn boss_info(q: &Query<(&Enemy, &Health), With<Enemy>>) -> Option<(u32, u32)> {
    for (enemy, health) in q {
        if is_boss(enemy.kind) {
            return Some((health.hp.max(0) as u32, health.max as u32));
        }
    }
    None
}

// Start ammo is pickup amount x3.
fn starting_ammo_for(
    weapons: &[WeaponId; MAX_WEAPON_SLOTS],
    race: RaceId,
    crown: CrownKind,
) -> [i32; MAX_AMMO_TYPES] {
    let mut ammo = [0; MAX_AMMO_TYPES];

    let fish = if race == RaceId::Fish { 1 } else { 0 };
    let haste = if crown == CrownKind::Haste { 1 } else { 0 };

    let typ_ammo = |kind: AmmoKind| -> i32 {
        let base = match kind {
            AmmoKind::Bullets => 32,
            AmmoKind::Shells => 8,
            AmmoKind::Bolts => 7,
            AmmoKind::Explosives => 6,
            AmmoKind::Energy => 10,
            AmmoKind::None => 0,
        };
        let fish_bonus = match kind {
            AmmoKind::Bullets => 8 * fish,
            AmmoKind::Shells => 2 * fish,
            AmmoKind::Bolts => 2 * fish,
            AmmoKind::Explosives => 2 * fish,
            AmmoKind::Energy => 3 * fish,
            AmmoKind::None => 0,
        };
        base + fish_bonus + haste
    };

    for &weapon in weapons {
        if weapon == WeaponId::NONE {
            continue;
        }

        let kind = weapon_ammo(weapon);
        let index = match kind {
            AmmoKind::None => continue,
            AmmoKind::Bullets => 1,
            AmmoKind::Shells => 2,
            AmmoKind::Bolts => 3,
            AmmoKind::Explosives => 4,
            AmmoKind::Energy => 5,
        };

        let amount = typ_ammo(kind) * 3;

        ammo[index] = ammo[index].max(amount);
    }

    ammo
}

#[cfg(test)]
mod loadout_tests {
    use super::*;

    #[test]
    fn revolver_receives_starting_bullets() {
        let ammo = starting_ammo_for(
            &[WeaponId::REVOLVER, WeaponId::NONE, WeaponId::NONE],
            RaceId::Robot,
            CrownKind::None,
        );
        assert_eq!(ammo[1], 96);
    }

    #[test]
    fn fish_gets_extra_starting_ammo() {
        let ammo = starting_ammo_for(
            &[WeaponId::REVOLVER, WeaponId::NONE, WeaponId::NONE],
            RaceId::Fish,
            CrownKind::None,
        );
        assert_eq!(ammo[1], 120);
    }

    #[test]
    fn shotgun_loadout_receives_shells() {
        let ammo = starting_ammo_for(
            &[WeaponId(5), WeaponId::NONE, WeaponId::NONE],
            RaceId::Robot,
            CrownKind::None,
        );
        assert!(ammo[2] > 0);
    }

    #[test]
    fn corrupt_weapon_does_not_grant_ammo() {
        let ammo = starting_ammo_for(
            &[WeaponId(255), WeaponId::NONE, WeaponId::NONE],
            RaceId::Robot,
            CrownKind::None,
        );
        assert_eq!(ammo, [0; MAX_AMMO_TYPES]);
    }
}

#[cfg(test)]
mod mutation_progression_tests {
    use super::*;
    use crate::game::content::CrownKind;

    fn dummy_player() -> Player {
        Player {
            crown: CrownKind::None,
            ..Default::default()
        }
    }

    #[test]
    fn patience_grants_four_next_roll_choices() {
        let mut player = dummy_player();
        player.patience_bonus = true;

        let choices = roll_mutations(&mut player);

        assert_eq!(choices.len(), 4);
        assert!(!player.patience_bonus);
    }

    #[test]
    fn patience_does_not_repeat_after_used() {
        let mut player = dummy_player();
        player.patience_used = true;

        for _ in 0..100 {
            let choices = roll_mutations(&mut player);
            assert!(!choices.contains(&MutationId::Patience));
        }
    }

    #[test]
    fn normal_roll_has_three_choices() {
        let mut player = dummy_player();

        let choices = roll_mutations(&mut player);

        assert_eq!(choices.len(), 4);
    }

    #[test]
    fn completed_pool_heals_instead_of_rolling() {
        let mut player = dummy_player();
        player.mutations = ALL_MUTATIONS.to_vec();

        player.patience_used = true;

        let choices = roll_mutations(&mut player);

        assert!(choices.is_empty());
    }

    #[test]
    fn ultra_choices_are_two_for_each_race() {
        for race in PLAYABLE_RACES {
            let [a, b] = ultra_choices_for(race);
            assert_ne!(a, b);
        }
    }
}

#[cfg(test)]
mod portal_vortex_parity_tests {
    use super::*;
    use crate::game::pickups::tick_pickup_drag;
    use bevy::asset::AssetPlugin;
    use bevy::time::TimeUpdateStrategy;

    fn harness() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()));

        app.init_asset::<Image>();
        app.insert_resource(TimeUpdateStrategy::FixedTimesteps(1));
        app.insert_resource(Time::<Fixed>::from_hz(crate::app::NT_SIM_HZ));
        let mut run = Run::default();
        run.portal_open = true;
        app.insert_resource(run);

        let mut mask = FloorMask::default();
        for cx in -1..=3 {
            mask.cells.insert((cx, 0));
        }
        app.insert_resource(mask);
        app.init_resource::<Toast>();
        app.init_resource::<PortalCarriedWeapons>();
        app.init_resource::<SecretTriggers>();
        let mut catalog = AssetCatalog::default();
        catalog
            .images
            .insert("images/sprBarrelDead.png".to_string());

        catalog.images.insert("images/sprRevolver.png".to_string());
        app.insert_resource(catalog);
        app.add_systems(
            FixedUpdate,
            (
                portal_attract,
                tick_pickup_drag,
                tick_portal_shock,
                tick_portal_clear,
                portal_enter,
            ),
        );
        app
    }

    fn portal_pos() -> Vec2 {
        Vec2::new(64.0, 0.0)
    }

    #[test]
    fn weapons_drag_toward_vortex_then_persist() {
        let mut app = harness();
        let portal = portal_pos();
        app.world_mut()
            .spawn((Portal, Transform::from_translation(portal.extend(5.0))));

        let gun = app
            .world_mut()
            .spawn((
                Pickup {
                    kind: PickupKind::Weapon(WeaponId::REVOLVER),
                },
                GroundPhysics {
                    vel: Vec2::ZERO,
                    rotspeed: 0.8,
                },
                Transform::from_translation(Vec2::ZERO.extend(8.0)),
            ))
            .id();

        app.world_mut().spawn((
            Player::default(),
            Velocity(Vec2::ZERO),
            Transform::from_translation(Vec2::ZERO.extend(20.0)),
        ));

        let dist = |app: &App| {
            app.world()
                .get::<Transform>(gun)
                .map(|tf| tf.translation.truncate().distance(portal))
                .unwrap_or(0.0)
        };
        let start = dist(&app);
        assert!((start - 64.0).abs() < 0.01);
        for _ in 0..5 {
            app.update();
        }

        let mid = dist(&app);
        assert!(mid < start, "gun not dragged: {mid} vs {start}");
        assert!(mid > 1.0, "gun teleported instead of dragged");
        for _ in 0..40 {
            app.update();
        }

        let carried = app.world().resource::<PortalCarriedWeapons>();
        assert!(
            !carried.0.is_empty(),
            "gun reaching the vortex was not carried over"
        );
        assert!(
            app.world().get_entity(gun).is_err(),
            "carried gun entity should despawn"
        );
    }

    #[test]
    fn far_ring_drag_never_overrides_player_velocity() {
        let mut app = harness();
        let portal = portal_pos();
        app.world_mut()
            .spawn((Portal, Transform::from_translation(portal.extend(5.0))));

        let player = app
            .world_mut()
            .spawn((
                Player::default(),
                Velocity(Vec2::new(120.0, 0.0)),
                Transform::from_translation((portal + Vec2::new(-80.0, 0.0)).extend(20.0)),
            ))
            .id();
        // First update only inits fixed time; the 5 measured ticks are real steps.
        app.update();
        for _ in 0..5 {
            app.update();
        }

        let vel = app.world().get::<Velocity>(player).unwrap().0;
        assert!(
            (vel - Vec2::new(120.0, 0.0)).length() < 0.01,
            "drag stole player velocity: {vel}"
        );

        let p = app
            .world()
            .get::<Transform>(player)
            .unwrap()
            .translation
            .truncate();
        let d = p.distance(portal);
        assert!((d - 70.0).abs() < 1.0, "far-ring nudge wrong: {d}");
    }

    #[test]
    fn rads_magnet_globally_while_portal_open() {
        let mut app = harness();
        app.world_mut().spawn((
            Portal,
            Transform::from_translation(portal_pos().extend(5.0)),
        ));
        app.world_mut().spawn((
            Player::default(),
            Velocity(Vec2::ZERO),
            Transform::from_translation(Vec2::ZERO.extend(20.0)),
        ));

        let rad = app
            .world_mut()
            .spawn((
                Pickup {
                    kind: PickupKind::Rad(1),
                },
                Transform::from_translation(Vec2::new(200.0, 0.0).extend(8.0)),
            ))
            .id();
        let player_dist = |app: &App| {
            app.world()
                .get::<Transform>(rad)
                .map(|tf| tf.translation.truncate().distance(Vec2::ZERO))
                .unwrap_or(f32::MAX)
        };
        let start = player_dist(&app);
        assert!((start - 200.0).abs() < 0.01);
        for _ in 0..10 {
            app.update();
        }
        let end = player_dist(&app);

        assert!(end < start - 50.0, "rad not magnetized: {end} vs {start}");
    }

    #[test]
    fn shock_kills_props_and_opens_chests() {
        let mut app = harness();
        app.world_mut().spawn((
            Portal,
            Transform::from_translation(portal_pos().extend(5.0)),
        ));
        app.world_mut().spawn((
            PortalShock {
                timer: Timer::from_seconds(2.0 / 30.0, TimerMode::Once),
                radius: 72.0,
            },
            Transform::from_translation(portal_pos().extend(6.0)),
        ));

        let barrel = app
            .world_mut()
            .spawn((
                Prop {
                    size: Vec2::splat(18.0),
                    hp: 10,
                    destructible: true,
                    explosive: false,
                },
                PropSprites {
                    idle: "images/sprBarrel.png",
                    hurt: "images/sprBarrelHurt.png",
                    dead: "images/sprBarrelDead.png",
                    flip_x: false,
                },
                Transform::from_translation(Vec2::new(74.0, 0.0).extend(-8.0)),
            ))
            .id();

        let chest = app
            .world_mut()
            .spawn((
                Pickup {
                    kind: PickupKind::Chest(ChestKind::Weapon),
                },
                Transform::from_translation(Vec2::ZERO.extend(8.0)),
            ))
            .id();
        for _ in 0..10 {
            app.update();
        }
        assert!(
            app.world().get_entity(barrel).is_err(),
            "shock did not kill the barrel prop"
        );
        assert!(
            app.world().get::<OpenedChest>(chest).is_some(),
            "shock did not auto-open the chest"
        );

        let corpses = app
            .world_mut()
            .query_filtered::<Entity, (With<PickupLifetime>, Without<Pickup>)>()
            .iter(app.world())
            .count();
        assert!(corpses > 0, "barrel left no spr_dead corpse");
    }
}
