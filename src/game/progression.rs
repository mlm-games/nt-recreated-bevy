//! Run progression: level-ups + mutation selection, portal spawn/entry, floor
//! transitions, and the run setup/cleanup hooks.

use bevy::prelude::*;
use rand::RngExt;

use crate::app::{OverlayMenu, Paused, PendingUnpause};
use crate::game::audio::GameAudio;
use crate::game::components::*;
use crate::game::content::*;
use crate::game::secret_areas::{self, SecretTriggers};
use crate::game::world;
use crate::save::SaveData;
use game_utils_bevy::camera_follow::CameraFollow;
use game_utils_bevy::game_feel::{GameFeel, SlowMotion};
use game_utils_bevy::juice::Juice;
use game_utils_bevy::save::SaveManager;
use game_utils_bevy::screen_effects::{ChromaticAberration, ScreenEffects, Trauma};
use game_utils_bevy::vfx::{DamageNumber, TrailGhost, VfxSpawner};

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

    commands.remove_resource::<PendingMutation>();
    commands.remove_resource::<PendingUltra>();
    commands.insert_resource(LoopTransition::default());
    commands.insert_resource(crate::game::ambience::AreaAudioState::default());
    commands.insert_resource(MutationChoice(None));
    commands.insert_resource(ScarierFace(false));
    commands.insert_resource(Euphoria(false));
    commands.insert_resource(OpenMind(false));
    commands.insert_resource(HeavyHeart(false));

    let def = character_def(character.0);

    // Saved race loadout drives the starting kit (upstream Campfire menu).
    let loadout = save.race_loadout(character.0);
    let crown = CrownKind::from_u8(loadout.start_crown);

    let primary = {
        let saved = sanitize_weapon_id(loadout.start_weapon);
        if saved == WeaponId::NONE {
            WeaponId::REVOLVER
        } else {
            saved
        }
    };

    let secondary = {
        let saved = sanitize_weapon_id(loadout.stored_weapon);
        if saved == primary {
            WeaponId::NONE
        } else {
            saved
        }
    };

    let equipped = [primary, secondary, WeaponId::NONE];
    let starting_ammo = starting_ammo_for(&equipped);

    let (player_sprite, player_strip) =
        crate::game::anim::sprite_anim(&catalog, &asset_server, def.sprite);
    let fire_rate_mult = if def.passive == PassiveKind::FastReload {
        0.8 // 25% faster (lower cooldown)
    } else {
        1.0
    };

    // Build player components as locals so the crown can mutate them before
    // insertion (upstream crowns reshape HP/weapons at run start).
    let mut player_comp = Player {
        speed: 240.0 * def.speed_mult,
        pickup_range: def.pickup_range,
        fire_rate_mult,
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
            skin: crate::game::content::SkinLetter::A,
        },
        inv_comp,
        FireCooldown {
            timer: ready_timer(),
            burst_left: 0,
            burst_timer: ready_timer(),
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
            aim_pull: 0.16, // high pull + raw mouse world aim = jitter
            base_scale: 0.45,
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
    while player.rads >= player.next_level_rads && player.level < 10 {
        player.rads -= player.next_level_rads;
        player.level += 1;
        player.next_level_rads = player.level.max(1) * 60;

        if player.level >= 10 && player.ultra.is_none() {
            let choices = ultra_choices_for(race).to_vec();
            let _ = race;
            commands.insert_resource(PendingUltra { choices });
            toast.show("LEVEL ULTRA! Choose an ultra mutation (1/2)");
            level_up_feedback(
                commands,
                trauma,
                flash,
                audio,
                pos,
                Color::srgb(1.0, 0.35, 1.0),
            );
            let _ = inv;
            return;
        }

        let choices = roll_mutations(player);
        if choices.is_empty() {
            // No mutations left: full heal instead.
            health.hp = health.max;
            continue;
        }

        commands.insert_resource(PendingMutation { choices });
        toast.show("LEVEL UP! Choose a mutation (1/2/3)");
        level_up_feedback(
            commands,
            trauma,
            flash,
            audio,
            pos,
            Color::srgb(0.25, 1.0, 0.25),
        );
        let _ = inv;
        return;
    }
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

    let want = if player.patience_bonus { 4 } else { 3 };
    let want = pool.len().min(want);

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
    mut scarier: ResMut<ScarierFace>,
    mut euphoria: ResMut<Euphoria>,
    mut open_mind: ResMut<OpenMind>,
    mut heavy_heart: ResMut<HeavyHeart>,
    mut player_q: Query<(&mut Player, &mut Health, &mut Inventory, &RaceState), With<Player>>,
    mut trauma: ResMut<Trauma>,
    mut chroma: ResMut<ChromaticAberration>,
    mut slow_mo: ResMut<SlowMotion>,
    mut toast: ResMut<Toast>,
    audio: Res<GameAudio>,
) {
    if ultra.is_none() && pending.is_none() {
        // Consume a stale UI choice if nothing is pending.
        if choice.0.is_some() {
            choice.0 = None;
        }
        return;
    }

    // Freeze gameplay while choosing.
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
            &mut trauma,
            &mut chroma,
            &mut slow_mo,
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
        paused.0 = false;
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
        &mut scarier,
        &mut euphoria,
        &mut open_mind,
        &mut heavy_heart,
        &mut trauma,
        &mut chroma,
        &mut slow_mo,
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
    paused.0 = false;
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
        }
        MutationId::SharpTeeth => {
            player.sharp_teeth = true;
        }
        MutationId::LastWish => {
            player.last_wish_used = false;
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
                *inv.ammo_mut(kind) = ammo_max(kind);
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
    toast.show(&format!("ULTRA — {}: {}", def.name, def.description));

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
    open_mind: Res<OpenMind>,
    mask: Res<FloorMask>,
    enemy_q: Query<(), With<Enemy>>,
    audio: Res<GameAudio>,
) {
    if run.game_over || run.portal_open {
        return;
    }
    // Throne I campfire / Throne II fight suppress the normal exit.
    if loop_transition.blocks_portal() {
        return;
    }
    if !enemy_q.is_empty() {
        return;
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
    let e = pc.id();

    Juice::pop_in(&mut commands, e, 0.3);
    ScreenEffects::add_trauma(&mut trauma, 0.25);
    ScreenEffects::chromatic_pulse(&mut chroma, 0.25);
    audio.play_portal(&mut commands);

    // Level-clear reward chest (Open Mind spawns extras).
    crate::game::pickups::spawn_chest(
        &mut commands,
        &catalog,
        &asset_server,
        ChestKind::Ammo,
        pos + Vec2::new(0.0, -48.0),
    );
    if open_mind.0 {
        crate::game::pickups::spawn_chest(
            &mut commands,
            &catalog,
            &asset_server,
            ChestKind::Ammo,
            pos + Vec2::new(64.0, -32.0),
        );
        crate::game::pickups::spawn_chest(
            &mut commands,
            &catalog,
            &asset_server,
            ChestKind::Ammo,
            pos + Vec2::new(-64.0, -32.0),
        );
    }
}

pub fn portal_enter(
    mut commands: Commands,
    run: Res<Run>,
    portal_q: Query<(Entity, &Transform), With<Portal>>,
    player_q: Query<(Entity, &Transform), (With<Player>, Without<Portal>, Without<PortalSucking>)>,
) {
    if run.game_over {
        return;
    }

    let Ok((portal_e, portal_tf)) = portal_q.single() else {
        return;
    };

    let Ok((player_e, player_tf)) = player_q.single() else {
        return;
    };

    let ppos = player_tf.translation.truncate();
    let tpos = portal_tf.translation.truncate();
    if ppos.distance(tpos) > 40.0 {
        return;
    }

    // Begin suck-in (NT Portal/Collision pulls the player over ~16 frames
    // @30fps); tick_portal_suck finishes the floor transition.
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
    mut trauma: ResMut<Trauma>,
    mut chroma: ResMut<ChromaticAberration>,
    mut toast: ResMut<Toast>,
    audio: Res<GameAudio>,
    mut triggers: ResMut<SecretTriggers>,
    mut loop_transition: ResMut<LoopTransition>,
    mut floor_started: MessageWriter<FloorStarted>,
    level_q: Query<Entity, With<LevelCleanup>>,
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
    // Ease-in toward the portal + spin + shrink (vortex look).
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

    ScreenEffects::add_trauma(&mut trauma, 0.02);
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

    // Clean current floor.
    for e in &level_q {
        commands.entity(e).despawn();
    }
    commands.entity(portal_e).despawn();

    // Priority: completed-loop portal -> queued secret -> ordinary advance.
    let looped = crate::game::loop_transition::try_apply_loop_portal_transition(
        &mut run,
        &mut loop_transition,
        &mut trauma,
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
        secret_areas::apply_secret_transition(&mut run, &mut triggers)
    };

    if let Some(secret) = entered_secret {
        commands.spawn((
            GameCleanup,
            crate::game::reactive_audio::QueuedReactiveCue(
                crate::game::reactive_audio::ReactiveCue::SecretFound,
            ),
        ));
        toast.show(&format!("ENTERING {}", secret.name()));
    } else if !looped {
        commands.spawn((
            GameCleanup,
            crate::game::reactive_audio::QueuedReactiveCue(
                crate::game::reactive_audio::ReactiveCue::PortalEnter,
            ),
        ));
        toast.show(&format!(
            "FLOOR {}-{}",
            run.world,
            world::floor_in_world(run.floor)
        ));
    } else {
        toast.show(&format!("LOOP {}", run.loop_count));
    }

    health.hp = (health.hp + 1).min(health.max);
    // Chicken passive: refresh headless each floor.
    if character_def(race_state.race).passive == PassiveKind::Headless {
        player.headless_ready = true;
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
    // Spawn player on a floor cell near origin
    if let Some(c) = mask.cells.iter().min_by_key(|c| {
        let p = mask.cell_center(**c);
        (p.length() * 1000.0) as i32
    }) {
        let p = mask.cell_center(*c);
        player_tf.translation = Vec3::new(p.x, p.y, 20.0);
    } else {
        player_tf.translation = Vec3::new(0.0, 0.0, 20.0);
    }

    ScreenEffects::add_trauma(&mut trauma, 0.55);
    ScreenEffects::chromatic_pulse(&mut chroma, 0.65);
    audio.play_portal(&mut commands);
}

pub fn animate_portal(time: Res<Time<Fixed>>, mut q: Query<&mut Transform, With<Portal>>) {
    let s = 1.0 + (time.elapsed_secs() * 8.0).sin() * 0.12;
    for mut tf in &mut q {
        tf.scale = Vec3::splat(s);
        tf.rotate_z(time.delta_secs() * 2.2);
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

/// NT starting ammo: bullets come pre-stacked; every other family starts at
/// three pickup units of the weapon's type.
fn starting_ammo_for(weapons: &[WeaponId; MAX_WEAPON_SLOTS]) -> [i32; MAX_AMMO_TYPES] {
    let mut ammo = [0; MAX_AMMO_TYPES];

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

        let amount = match kind {
            AmmoKind::Bullets => 96,
            _ => ammo_pickup_amount(kind) * 3,
        };

        ammo[index] = ammo[index].max(amount);
    }

    ammo
}

#[cfg(test)]
mod loadout_tests {
    use super::*;

    #[test]
    fn revolver_receives_starting_bullets() {
        let ammo = starting_ammo_for(&[WeaponId::REVOLVER, WeaponId::NONE, WeaponId::NONE]);
        assert_eq!(ammo[1], 96);
    }

    #[test]
    fn shotgun_loadout_receives_shells() {
        let ammo = starting_ammo_for(&[WeaponId(5), WeaponId::NONE, WeaponId::NONE]);
        assert!(ammo[2] > 0);
    }

    #[test]
    fn corrupt_weapon_does_not_grant_ammo() {
        let ammo = starting_ammo_for(&[WeaponId(255), WeaponId::NONE, WeaponId::NONE]);
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

        assert_eq!(choices.len(), 3);
    }

    #[test]
    fn completed_pool_heals_instead_of_rolling() {
        let mut player = dummy_player();
        player.mutations = ALL_MUTATIONS.to_vec();
        // Patience was used, so it is excluded from the pool as well.
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
