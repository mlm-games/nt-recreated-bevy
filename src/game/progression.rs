//! Run progression: level-ups + mutation selection, portal spawn/entry, floor
//! transitions, and the run setup/cleanup hooks.

use bevy::prelude::*;
use rand::RngExt;

use crate::app::{OverlayMenu, Paused, PendingUnpause};
use crate::game::audio::GameAudio;
use crate::game::components::*;
use crate::game::content::*;
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
    camera_q: Query<Entity, With<Camera2d>>,
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
    commands.insert_resource(MutationChoice(None));
    commands.insert_resource(ScarierFace(false));
    commands.insert_resource(Euphoria(false));
    commands.insert_resource(OpenMind(false));
    commands.insert_resource(HeavyHeart(false));

    let def = character_def(character.0);

    let (player_sprite, player_strip) =
        crate::game::anim::sprite_anim(&catalog, &asset_server, def.sprite);
    let mut player = commands
        .spawn((
            GameCleanup,
            Player {
                speed: 240.0 * def.speed_mult,
                accel: PLAYER_ACCEL,
                friction: PLAYER_FRICTION,
                speed_mult: 1.0,
                rads: 0,
                level: 1,
                next_level_rads: 60,
                pickup_range: def.pickup_range,
                fire_rate_mult: 1.0,
                spread_mult: 1.0,
                knockback_mult: 1.0,
                melee_range_mult: 1.0,
                drop_mult: 0.0,
                medkit_mult: 1.0,
                boiling_veins: false,
                veins_threshold: 4,
                bloodlust: false,
                lucky_shot: false,
                gamma_guts: false,
                back_muscle: 0,
                stress: false,
                sharp_teeth: false,
                strong_spirit_ready: false,
                last_wish_used: false,
                chain_explosions: def.passive == PassiveKind::ChainExplosions,
                shield_on_hit: def.passive == PassiveKind::ShieldOnHit,
                ability: def.ability,
                ability_cooldown: Timer::from_seconds(0.0, TimerMode::Once),
                mutations: Vec::new(),
            },
            Inventory {
                weapons: [WeaponId::REVOLVER, WeaponId::NONE, WeaponId::NONE],
                weapon_slots: if character.0 == RaceId::Cuz { 3 } else { 2 },
                current: 0,
                ammo: [0, 96, 0, 0, 0, 0],
            },
            FireCooldown {
                timer: ready_timer(),
                burst_left: 0,
                burst_timer: ready_timer(),
            },
            Health {
                hp: def.max_hp,
                max: def.max_hp,
                invuln: ready_timer(),
            },
            Team::Player,
            Hitbox {
                radius: PLAYER_RADIUS,
            },
            AimDir(Vec2::Y),
            Velocity(Vec2::ZERO),
            crate::game::anim::PlayerAnim {
                idle: def.sprite,
                walk: def.walk_sprite,
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
            follow_weight: 0.18,
            aim_weight: 0.12,
            aim_pull: 0.28,
            base_scale: 0.45,
            zoom_speed: 0.08,
            ..default()
        });
    }

    let plan = world::generate_level(&run);
    world::spawn_level(&mut commands, &catalog, &asset_server, &run, &plan, &mut mask);
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
) {
    while player.rads > player.next_level_rads && player.level < 10 {
        player.rads -= player.next_level_rads;
        player.level += 1;
        player.next_level_rads = player.level.max(1) * 60;

        let choices = roll_mutations(player);
        if choices.is_empty() {
            // No mutations left: full heal instead.
            health.hp = health.max;
            continue;
        }

        commands.insert_resource(PendingMutation { choices });
        toast.show("LEVEL UP! Choose a mutation (1/2/3)");
        ScreenEffects::add_trauma(trauma, 0.35);
        ScreenEffects::flash_white(flash, 0.05);
        VfxSpawner::spawn_burst(
            commands,
            pos,
            32,
            Color::srgb(0.25, 1.0, 0.25),
            (120.0, 360.0),
        );
        audio.play_levelup(commands);
        let _ = inv;
        return;
    }
}

fn roll_mutations(player: &Player) -> Vec<MutationId> {
    let mut pool: Vec<MutationId> = ALL_MUTATIONS
        .iter()
        .copied()
        .filter(|m| !player.mutations.contains(m))
        .collect();
    let mut rng = rand::rng();
    let mut out = Vec::new();
    let want = pool.len().min(3);
    for _ in 0..want {
        let idx = rng.random_range(0..pool.len());
        out.push(pool.remove(idx));
    }
    out
}

pub fn handle_mutation_choice(
    keys: Res<ButtonInput<KeyCode>>,
    mut commands: Commands,
    pending: Option<ResMut<PendingMutation>>,
    mut choice: ResMut<MutationChoice>,
    mut paused: ResMut<Paused>,
    mut scarier: ResMut<ScarierFace>,
    mut euphoria: ResMut<Euphoria>,
    mut open_mind: ResMut<OpenMind>,
    mut heavy_heart: ResMut<HeavyHeart>,
    mut player_q: Query<(&mut Player, &mut Health), With<Player>>,
    mut trauma: ResMut<Trauma>,
    mut chroma: ResMut<ChromaticAberration>,
    mut slow_mo: ResMut<SlowMotion>,
    mut toast: ResMut<Toast>,
    audio: Res<GameAudio>,
) {
    let Some(mut pending) = pending else {
        // Consume a stale UI choice if no mutation is pending.
        if choice.0.is_some() {
            choice.0 = None;
        }
        return;
    };

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
    }

    let Some(idx) = picked else {
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

    pending.choices.clear();
    commands.remove_resource::<PendingMutation>();
    paused.0 = false;
}

#[allow(clippy::too_many_arguments)]
fn apply_mutation(
    commands: &mut Commands,
    player_q: &mut Query<(&mut Player, &mut Health), With<Player>>,
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
    let Ok((mut player, mut health)) = player_q.single_mut() else {
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
    }

    ScreenEffects::add_trauma(trauma, 0.3);
    ScreenEffects::chromatic_pulse(chroma, 0.25);
    GameFeel::slow_motion(slow_mo, 0.5, 0.35);
    audio.play_levelup(commands);
    toast.show(&format!("{}: {}", def.name, def.description));
}

pub fn portal_check(
    mut commands: Commands,
    catalog: Res<AssetCatalog>,
    asset_server: Res<AssetServer>,
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
    if !enemy_q.is_empty() {
        return;
    }

    run.portal_open = true;

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
    catalog: Res<AssetCatalog>,
    asset_server: Res<AssetServer>,
    mut run: ResMut<Run>,
    mut mask: ResMut<FloorMask>,
    mut trauma: ResMut<Trauma>,
    mut chroma: ResMut<ChromaticAberration>,
    mut toast: ResMut<Toast>,
    audio: Res<GameAudio>,
    portal_q: Query<(Entity, &Transform), With<Portal>>,
    level_q: Query<Entity, With<LevelCleanup>>,
    mut player_q: Query<(&mut Transform, &mut Health), (With<Player>, Without<Portal>)>,
) {
    if run.game_over {
        return;
    }

    let Ok((portal_e, portal_tf)) = portal_q.single() else {
        return;
    };

    let Ok((mut player_tf, mut health)) = player_q.single_mut() else {
        return;
    };

    let dist = player_tf
        .translation
        .truncate()
        .distance(portal_tf.translation.truncate());
    if dist > 40.0 {
        return;
    }

    // Clean current floor.
    for e in &level_q {
        commands.entity(e).despawn();
    }
    commands.entity(portal_e).despawn();

    run.floor += 1;
    run.world = world::world_of(run.floor);
    run.loop_count = (run.floor - 1) / 7;
    run.floor_in_area = ((run.floor - 1) % 7) + 1;
    run.area = crate::game::areas::area_for_floor(run.floor, run.loop_count);
    run.portal_open = false;
    run.gen_seed = rand::rng().random_range(0..u64::MAX);

    health.hp = (health.hp + 1).min(health.max);

    let plan = world::generate_level(&run);
    world::spawn_level(&mut commands, &catalog, &asset_server, &run, &plan, &mut mask);
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
    toast.show(&format!(
        "FLOOR {}-{}",
        run.world,
        world::floor_in_world(run.floor)
    ));
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
