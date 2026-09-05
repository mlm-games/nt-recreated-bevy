//! Strip-sheet animation: `anims.json` (written by tools/gen_assets.py)
//! maps sprite names to {frames, w, h, fps}; sprites store a horizontal
//! strip and we slice one frame per tick via `Sprite::rect`.

use bevy::prelude::*;

use crate::game::components::{
    EnemySprites, Health, HurtAnim, Prop, PropHpTracker, PropSprites, Velocity,
};
use crate::game::content::AssetCatalog;

#[derive(Clone, Copy, Debug)]
pub struct AnimDef {
    pub frames: u32,
    pub frame_px: u32,
    pub height: u32,
    pub fps: f32,
}

/// Per-entity animation state. `path` is the strip texture; `moving` lets
/// systems swap between idle/walk variants by rewriting `path`.
#[derive(Component)]
pub struct SpriteAnim {
    pub path: String,
    pub def: AnimDef,
    pub frame: u32,
    pub timer: Timer,
    /// When true, play once then stop (hurt / chest open / one-shots).
    pub oneshot: bool,
    pub finished: bool,
}

impl SpriteAnim {
    pub fn new(path: impl Into<String>, def: AnimDef) -> Self {
        Self {
            path: path.into(),
            def,
            frame: 0,
            timer: Timer::from_seconds(1.0 / def.fps.max(0.1), TimerMode::Repeating),
            oneshot: false,
            finished: false,
        }
    }

    pub fn oneshot(path: impl Into<String>, def: AnimDef) -> Self {
        let mut a = Self::new(path, def);
        a.oneshot = true;
        a
    }

    pub fn rect(&self) -> Rect {
        let w = self.def.frame_px as f32;
        let h = self.def.height as f32;
        Rect::new(self.frame as f32 * w, 0.0, self.frame as f32 * w + w, h)
    }

    pub fn set_path(&mut self, path: impl Into<String>, def: AnimDef, oneshot: bool) {
        self.path = path.into();
        self.def = def;
        self.frame = 0;
        self.oneshot = oneshot;
        self.finished = false;
        self.timer = Timer::from_seconds(1.0 / def.fps.max(0.1), TimerMode::Repeating);
    }
}

/// Build an animated sprite when `path` has strip data; static otherwise.
pub fn sprite_anim(
    catalog: &AssetCatalog,
    asset_server: &AssetServer,
    path: &str,
) -> (Sprite, Option<SpriteAnim>) {
    let mut sprite = crate::game::content::sprite_exact(catalog, asset_server, path);
    match catalog.anim_def(path) {
        Some(def) => {
            let anim = SpriteAnim::new(path, def);
            sprite.rect = Some(anim.rect());
            (sprite, Some(anim))
        }
        None => (sprite, None),
    }
}

/// Advance every animation and slice its current frame.
pub fn animate_sprites(time: Res<Time<Fixed>>, mut q: Query<(&mut SpriteAnim, &mut Sprite)>) {
    for (mut anim, mut sprite) in &mut q {
        if anim.finished {
            continue;
        }
        anim.timer.tick(time.delta());
        if anim.timer.just_finished() {
            if anim.oneshot {
                if anim.frame + 1 >= anim.def.frames.max(1) {
                    anim.frame = anim.def.frames.saturating_sub(1);
                    anim.finished = true;
                } else {
                    anim.frame += 1;
                }
            } else {
                anim.frame = (anim.frame + 1) % anim.def.frames.max(1);
            }
            sprite.rect = Some(anim.rect());
        }
    }
}

/// Player idle/walk strip pair; `moving` selects which is displayed.
#[derive(Component)]
pub struct PlayerAnim {
    pub idle: &'static str,
    pub walk: &'static str,
    pub hurt: &'static str,
    pub moving: bool,
}

/// Swap the player's strip when movement state changes (skipped during hurt).
pub fn player_anim_switch(
    asset_server: Res<AssetServer>,
    catalog: Res<AssetCatalog>,
    mut q: Query<
        (
            &Velocity,
            &mut PlayerAnim,
            &mut SpriteAnim,
            &mut Sprite,
            &mut bevy::sprite::Anchor,
        ),
        Without<HurtAnim>,
    >,
) {
    for (vel, mut pa, mut anim, mut sprite, mut anchor) in &mut q {
        // Don't interrupt a oneshot (portal suck uses oneshot too).
        if anim.oneshot && !anim.finished {
            continue;
        }
        let moving = vel.0.length_squared() > 100.0;
        if moving == pa.moving && !anim.oneshot {
            continue;
        }
        pa.moving = moving;
        let path = if moving { pa.walk } else { pa.idle };
        let Some(def) = catalog.anim_def(path) else {
            continue;
        };
        anim.set_path(path, def, false);
        sprite.image = asset_server.load(path.to_string());
        sprite.rect = Some(anim.rect());
        *anchor = crate::game::content::sprite_anchor(&catalog, path);
    }
}

/// Enemy walk/idle strip swap – mirrors GML `if speed<=0 sprite=spr_idle else spr_walk`
/// (`enemy/Step_0.gml:7-24`, `Bandit/Other_10.gml` etc). Bandits were sliding
/// on idle strip because only `player_anim_switch` existed.
pub fn enemy_anim_switch(
    asset_server: Res<AssetServer>,
    catalog: Res<AssetCatalog>,
    mut q: Query<
        (
            &Velocity,
            &EnemySprites,
            &mut SpriteAnim,
            &mut Sprite,
            &mut bevy::sprite::Anchor,
        ),
        (With<crate::game::components::Enemy>, Without<HurtAnim>),
    >,
) {
    for (vel, sprites, mut anim, mut sprite, mut anchor) in &mut q {
        if anim.oneshot && !anim.finished {
            continue;
        }
        // Don't interrupt hurt one-shot handled by `tick_hurt_anims`.
        let moving = vel.0.length_squared() > 80.0;
        let idle = sprites.idle;
        let walk = sprites.walk.unwrap_or(idle);
        let desired = if moving { walk } else { idle };
        if anim.path == desired {
            continue;
        }
        // Walk missing (Maggot/Scorpion) → keep idle regardless of speed.
        if moving && sprites.walk.is_none() {
            if anim.path != idle {
                if let Some(def) = catalog.anim_def(idle) {
                    anim.set_path(idle, def, false);
                    sprite.image = asset_server.load(idle.to_string());
                    sprite.rect = Some(anim.rect());
                    *anchor = crate::game::content::sprite_anchor(&catalog, idle);
                }
            }
            continue;
        }
        let Some(def) = catalog.anim_def(desired) else {
            continue;
        };
        anim.set_path(desired, def, false);
        sprite.image = asset_server.load(desired.to_string());
        sprite.rect = Some(anim.rect());
        *anchor = crate::game::content::sprite_anchor(&catalog, desired);
    }
}

/// Begin hurt strip on an entity that has SpriteAnim + optional HurtAnim paths.
pub fn play_hurt(
    commands: &mut Commands,
    entity: Entity,
    catalog: &AssetCatalog,
    asset_server: &AssetServer,
    anim: &mut SpriteAnim,
    sprite: &mut Sprite,
    hurt_path: &'static str,
    idle: &'static str,
    walk: Option<&'static str>,
) {
    let Some(def) = catalog
        .anim_def(hurt_path)
        .or_else(|| catalog.anim_def(idle))
    else {
        return;
    };
    // Prefer real hurt strip; fall back to a short freeze on idle frame 0.
    let path = if catalog.anim_def(hurt_path).is_some() {
        hurt_path
    } else {
        idle
    };
    let def = catalog.anim_def(path).unwrap_or(def);
    anim.set_path(path, def, true);
    sprite.image = asset_server.load(path.to_string());
    sprite.rect = Some(anim.rect());

    // GM: hurt lasts while image_index <=2 (~3 frames at 0.4*30=12fps => 0.25s)
    // Keep timer as backstop; frame check above is authoritative.
    let secs = (3.0 / def.fps.max(1.0)).max(0.12).min(0.35);
    commands.entity(entity).insert(HurtAnim {
        idle,
        walk,
        hurt: path,
        timer: Timer::from_seconds(secs, TimerMode::Once),
        was_moving: false,
    });
}

/// Restore idle/walk after hurt oneshot finishes. Mirrors GM:
/// `if sprite_index==spr_hurt && image_index>2) sprite_index=spr_idle`
/// which is frame-count, not timer. Keep timer as backstop for missing strips.
pub fn tick_hurt_anims(
    time: Res<Time<Fixed>>,
    asset_server: Res<AssetServer>,
    catalog: Res<AssetCatalog>,
    mut commands: Commands,
    mut q: Query<(
        Entity,
        &mut HurtAnim,
        &mut SpriteAnim,
        &mut Sprite,
        &mut bevy::sprite::Anchor,
        Option<&Velocity>,
        Option<&mut PlayerAnim>,
        Option<&PropSprites>,
    )>,
) {
    for (e, mut hurt, mut anim, mut sprite, mut anchor, vel, mut pa, prop_sprites) in &mut q {
        hurt.timer.tick(time.delta());
        let frame_done = anim.frame >= 2;
        let hard_timeout = hurt.timer.elapsed_secs() > 0.35;
        if !(hurt.timer.just_finished()
            || frame_done
            || hard_timeout
            || (anim.oneshot && anim.finished))
        {
            continue;
        }
        let moving = vel.map(|v| v.0.length_squared() > 100.0).unwrap_or(false);
        let path = if moving {
            hurt.walk.unwrap_or(hurt.idle)
        } else {
            hurt.idle
        };
        if let Some(def) = catalog.anim_def(path) {
            anim.set_path(path, def, false);
            sprite.image = asset_server.load(path.to_string());
            sprite.rect = Some(anim.rect());
            *anchor = crate::game::content::sprite_anchor(&catalog, path);
        }
        if let Some(ref mut pa) = pa {
            pa.moving = moving;
        }
        if let Some(ps) = prop_sprites {
            sprite.flip_x = ps.flip_x;
        }
        commands.entity(e).remove::<HurtAnim>();
    }
}

/// Map idle sprite path → conventional hurt/walk names used by NT art.
pub fn derive_hurt_path(idle: &'static str) -> &'static str {
    // Static table - keep in sync with imported spr*Hurt.png names.
    // Handles B/C skin variants like sprMutant1BIdle.png -> sprMutant1BHurt.png
    // by stripping the skin suffix before matching.
    let base = if idle.contains("sprMutant") {
        // Check B/C skin variants first
        if idle.contains("sprMutant1BIdle") {
            return "images/sprMutant1BHurt.png";
        }
        if idle.contains("sprMutant1CIdle") {
            return "images/sprMutant1CHurt.png";
        }
        if idle.contains("sprMutant2BIdle") {
            return "images/sprMutant2BHurt.png";
        }
        if idle.contains("sprMutant2CIdle") {
            return "images/sprMutant2CHurt.png";
        }
        if idle.contains("sprMutant3BIdle") {
            return "images/sprMutant3BHurt.png";
        }
        if idle.contains("sprMutant3CIdle") {
            return "images/sprMutant3CHurt.png";
        }
        if idle.contains("sprMutant4BIdle") {
            return "images/sprMutant4BHurt.png";
        }
        if idle.contains("sprMutant4CIdle") {
            return "images/sprMutant4CHurt.png";
        }
        if idle.contains("sprMutant5BIdle") {
            return "images/sprMutant5BHurt.png";
        }
        if idle.contains("sprMutant5CIdle") {
            return "images/sprMutant5CHurt.png";
        }
        if idle.contains("sprMutant6BIdle") {
            return "images/sprMutant6BHurt.png";
        }
        if idle.contains("sprMutant6CIdle") {
            return "images/sprMutant6CHurt.png";
        }
        if idle.contains("sprMutant7BIdle") {
            return "images/sprMutant7BHurt.png";
        }
        if idle.contains("sprMutant7CIdle") {
            return "images/sprMutant7CHurt.png";
        }
        if idle.contains("sprMutant8BIdle") {
            return "images/sprMutant8BHurt.png";
        }
        if idle.contains("sprMutant8CIdle") {
            return "images/sprMutant8CHurt.png";
        }
        if idle.contains("sprMutant9BIdle") {
            return "images/sprMutant9BHurt.png";
        }
        if idle.contains("sprMutant9CIdle") {
            return "images/sprMutant9CHurt.png";
        }
        if idle.contains("sprMutant10BIdle") {
            return "images/sprMutant10BHurt.png";
        }
        if idle.contains("sprMutant10CIdle") {
            return "images/sprMutant10CHurt.png";
        }
        if idle.contains("sprMutant11BIdle") {
            return "images/sprMutant11BHurt.png";
        }
        if idle.contains("sprMutant11CIdle") {
            return "images/sprMutant11CHurt.png";
        }
        if idle.contains("sprMutant12BIdle") {
            return "images/sprMutant12BHurt.png";
        }
        if idle.contains("sprMutant12CIdle") {
            return "images/sprMutant12CHurt.png";
        }
        if idle.contains("sprMutant13BIdle") {
            return "images/sprMutant13BHurt.png";
        }
        if idle.contains("sprMutant13CIdle") {
            return "images/sprMutant13CHurt.png";
        }
        if idle.contains("sprMutant14BIdle") {
            return "images/sprMutant14BHurt.png";
        }
        if idle.contains("sprMutant14CIdle") {
            return "images/sprMutant14CHurt.png";
        }
        if idle.contains("sprMutant15BIdle") {
            return "images/sprMutant15BHurt.png";
        }
        if idle.contains("sprMutant15CIdle") {
            return "images/sprMutant15CHurt.png";
        }
        if idle.contains("sprMutant16BIdle") {
            return "images/sprMutant16BHurt.png";
        }
        if idle.contains("sprMutant16CIdle") {
            return "images/sprMutant16CHurt.png";
        }
        // fall through to A variants below
        idle
    } else {
        idle
    };
    match base {
        "images/sprBanditIdle.png" => "images/sprBanditHurt.png",
        "images/sprMaggotIdle.png" => "images/sprMaggotHurt.png",
        "images/sprScorpionIdle.png" => "images/sprScorpionHurt.png",
        "images/sprRatIdle.png" => "images/sprRatHurt.png",
        "images/sprRatkingIdle.png" => "images/sprRatkingHurt.png",
        "images/sprFreak1Idle.png" => "images/sprFreak1Hurt.png",
        "images/sprJungleAssassinIdle.png" => "images/sprJungleAssassinHurt.png",
        "images/sprSnowBotIdle.png" => "images/sprSnowBotHurt.png",
        "images/sprTurretIdle.png" => "images/sprTurretHurt.png",
        "images/sprSnowBanditIdle.png" => "images/sprSnowBanditHurt.png",
        "images/sprWolfIdle.png" => "images/sprWolfHurt.png",
        "images/sprBanditBossIdle.png" => "images/sprBanditBossHurt.png",
        // Expanded roster (upstream spr*Hurt strips)
        "images/sprGatorIdle.png" => "images/sprGatorHurt.png",
        "images/sprBuffGatorIdle.png" => "images/sprBuffGatorHurt.png",
        "images/sprRavenIdle.png" => "images/sprRavenHurt.png",
        "images/sprSalamanderIdle.png" => "images/sprSalamanderHurt.png",
        "images/sprMeleeIdle.png" => "images/sprMeleeHurt.png",
        "images/sprJungleBanditIdle.png" => "images/sprJungleBanditHurt.png",
        "images/sprBigMaggotIdle.png" => "images/sprBigMaggotHurt.png",
        "images/sprFastRatIdle.png" => "images/sprFastRatHurt.png",
        "images/sprGoldScorpionIdle.png" => "images/sprGoldScorpionHurt.png",
        "images/sprLightningCrystalIdle.png" => "images/sprLightningCrystalHurt.png",
        "images/sprExploFreakIdle.png" => "images/sprExploFreakHurt.png",
        "images/sprRhinoFreakIdle.png" => "images/sprRhinoFreakHurt.png",
        "images/sprSnowTankIdle.png" => "images/sprSnowTankHurt.png",
        "images/sprGoldTankIdle.png" => "images/sprGoldTankHurt.png",
        "images/sprGuardianIdle.png" => "images/sprGuardianHurt.png",
        "images/sprExploGuardianIdle.png" => "images/sprExploGuardianHurt.png",
        "images/sprDogGuardianWalk.png" => "images/sprDogGuardianHurt.png",
        // Secret areas & mansion garrison
        "images/sprBoneFish1Idle.png" => "images/sprBoneFish1Hurt.png",
        "images/sprTurtleIdle.png" => "images/sprTurtleHurt.png",
        "images/sprMolefishIdle.png" => "images/sprMolefishHurt.png",
        "images/sprMolesargeIdle.png" => "images/sprMolesargeHurt.png",
        "images/sprFireBallerIdle.png" => "images/sprFireBallerHurt.png",
        "images/sprSuperFireBallerIdle.png" => "images/sprSuperFireBallerHurt.png",
        "images/sprJockIdle.png" => "images/sprJockHurt.png",
        "images/sprJungleFlyIdle.png" => "images/sprJungleFlyHurt.png",
        "images/sprInvSpiderIdle.png" => "images/sprInvSpiderHurt.png",
        "images/sprInvLaserCrystalIdle.png" => "images/sprInvLaserCrystalHurt.png",
        "images/sprPopoFreakIdle.png" => "images/sprPopoFreakHurt.png",
        "images/sprMSpawnIdle.png" => "images/sprMSpawnHurt.png",
        // Secret boss
        "images/sprFrogQueenIdle.png" => "images/sprFrogQueenHurt.png",
        // Mutants A
        "images/sprMutant1Idle.png" => "images/sprMutant1Hurt.png",
        "images/sprMutant2Idle.png" => "images/sprMutant2Hurt.png",
        "images/sprMutant3Idle.png" => "images/sprMutant3Hurt.png",
        "images/sprMutant4Idle.png" => "images/sprMutant4Hurt.png",
        "images/sprMutant5Idle.png" => "images/sprMutant5Hurt.png",
        "images/sprMutant6Idle.png" => "images/sprMutant6Hurt.png",
        "images/sprMutant7Idle.png" => "images/sprMutant7Hurt.png",
        "images/sprMutant8Idle.png" => "images/sprMutant8Hurt.png",
        "images/sprMutant9Idle.png" => "images/sprMutant9Hurt.png",
        "images/sprMutant10Idle.png" => "images/sprMutant10Hurt.png",
        "images/sprMutant11Idle.png" => "images/sprMutant11Hurt.png",
        "images/sprMutant12Idle.png" => "images/sprMutant12Hurt.png",
        "images/sprMutant13Idle.png" => "images/sprMutant13Hurt.png",
        "images/sprMutant14Idle.png" => "images/sprMutant14Hurt.png",
        "images/sprMutant15Idle.png" => "images/sprMutant15Hurt.png",
        "images/sprMutant16Idle.png" => "images/sprMutant16Hurt.png",
        "images/sprBarrel.png" => "images/sprBarrelHurt.png",
        "images/sprToxicBarrel.png" => "images/sprToxicBarrelHurt.png",
        "images/sprGoldBarrel.png" => "images/sprGoldBarrelHurt.png",
        "images/sprOasisBarrel.png" => "images/sprOasisBarrelHurt.png",
        "images/sprCactus.png" => "images/sprCactusHurt.png",
        "images/sprCactus2.png" => "images/sprCactus2Hurt.png",
        "images/sprCactus3.png" => "images/sprCactus3Hurt.png",
        "images/sprCactusB.png" => "images/sprCactusBHurt.png",
        "images/sprCactusB2.png" => "images/sprCactusB2Hurt.png",
        "images/sprCactusB3.png" => "images/sprCactusB3Hurt.png",
        "images/sprNightCactus.png" => "images/sprNightCactusHurt.png",
        "images/sprNightCactus2.png" => "images/sprNightCactus2Hurt.png",
        "images/sprNightCactus3.png" => "images/sprNightCactus3Hurt.png",
        "images/sprSewerPipe.png" => "images/sprSewerPipeHurt.png",
        "images/sprPipe.png" => "images/sprSewerPipeHurt.png",
        "images/sprTires.png" => "images/sprTiresHurt.png",
        "images/sprCocoon.png" => "images/sprCocoonHurt.png",
        "images/sprSnowMan.png" => "images/sprSnowManHurt.png",
        "images/sprSnowManIdle.png" => "images/sprSnowManHurt.png",
        "images/sprTorch.png" => "images/sprTorchHurt.png",
        "images/sprBonePileIdle.png" => "images/sprBonePileHurt.png",
        "images/sprBonePile.png" => "images/sprBonePileHurt.png",
        "images/sprNightBonePileIdle.png" => "images/sprNightBonePileHurt.png",
        "images/sprBushIdle.png" => "images/sprBushHurt.png",
        "images/sprBush.png" => "images/sprBushHurt.png",
        "images/sprBigFlowerIdle.png" => "images/sprBigFlowerHurt.png",
        "images/sprPlantPotIdle.png" => "images/sprPlantPotHurt.png",
        "images/sprCrystalProp.png" => "images/sprCrystalPropHurt.png",
        "images/sprHydrant.png" => "images/sprHydrantHurt.png",
        "images/sprIcicle.png" => "images/sprIcicleHurt.png",
        "images/sprStreetLight.png" => "images/sprStreetLightHurt.png",
        "images/sprSodaMachine.png" => "images/sprSodaMachineHurt.png",
        "images/sprNewsStand.png" => "images/sprNewsStandHurt.png",
        "images/sprTube.png" => "images/sprTubeHurt.png",
        "images/sprMutantTube.png" => "images/sprMutantTubeHurt.png",
        "images/sprNuclearPillar.png" => "images/sprNuclearPillarHurt.png",
        "images/sprPillar.png" => "images/sprNuclearPillarHurt.png",
        "images/sprSmallGenerator.png" => "images/sprSmallGeneratorHurt.png",
        "images/sprBigGenerator.png" => "images/sprBigGeneratorHurt.png",
        "images/sprGenerator.png" => "images/sprBigGeneratorHurt.png",
        "images/sprAnchor.png" => "images/sprAnchorHurt.png",
        "images/sprWaterPlant.png" => "images/sprWaterPlantHurt.png",
        "images/sprWaterPlant2.png" => "images/sprWaterPlantHurt.png",
        "images/sprWaterMine.png" => "images/sprWaterMineHurt.png",
        "images/sprMine.png" => "images/sprMineHurt.png",
        "images/sprMineIdle.png" => "images/sprMineHurt.png",
        "images/sprMoneyPile.png" => "images/sprMoneyPileHurt.png",
        "images/sprYVStatue.png" => "images/sprYVStatueHurt.png",
        "images/sprPizzaBox.png" => "images/sprPizzaBoxHurt.png",
        "images/sprCarIdle.png" => "images/sprCarHurt.png",
        "images/sprFrozenCar.png" => "images/sprFrozenCarHurt.png",
        "images/sprBigSkullOpen.png" => "images/sprBigSkullOpenHurt.png",
        "images/sprBigSkull.png" => "images/sprBigSkullOpenHurt.png",
        "images/sprRadChest.png" => "images/sprRadChestHurt.png",
        "images/sprRadChestIdle.png" => "images/sprRadChestHurt.png",
        "images/sprThroneStatue.png" => "images/sprThroneStatue.png",
        _ => idle, // fallback: replay idle as freeze
    }
}

pub fn derive_dead_path(idle: &'static str) -> &'static str {
    let base = if idle.contains("sprMutant") {
        if idle.contains("sprMutant1BIdle") {
            return "images/sprMutant1BDead.png";
        }
        if idle.contains("sprMutant1CIdle") {
            return "images/sprMutant1CDead.png";
        }
        if idle.contains("sprMutant2BIdle") {
            return "images/sprMutant2BDead.png";
        }
        if idle.contains("sprMutant2CIdle") {
            return "images/sprMutant2CDead.png";
        }
        if idle.contains("sprMutant3BIdle") {
            return "images/sprMutant3BDead.png";
        }
        if idle.contains("sprMutant3CIdle") {
            return "images/sprMutant3CDead.png";
        }
        if idle.contains("sprMutant4BIdle") {
            return "images/sprMutant4BDead.png";
        }
        if idle.contains("sprMutant4CIdle") {
            return "images/sprMutant4CDead.png";
        }
        if idle.contains("sprMutant5BIdle") {
            return "images/sprMutant5BDead.png";
        }
        if idle.contains("sprMutant5CIdle") {
            return "images/sprMutant5CDead.png";
        }
        if idle.contains("sprMutant6BIdle") {
            return "images/sprMutant6BDead.png";
        }
        if idle.contains("sprMutant6CIdle") {
            return "images/sprMutant6CDead.png";
        }
        if idle.contains("sprMutant7BIdle") {
            return "images/sprMutant7BDead.png";
        }
        if idle.contains("sprMutant7CIdle") {
            return "images/sprMutant7CDead.png";
        }
        if idle.contains("sprMutant8BIdle") {
            return "images/sprMutant8BDead.png";
        }
        if idle.contains("sprMutant8CIdle") {
            return "images/sprMutant8CDead.png";
        }
        if idle.contains("sprMutant9BIdle") {
            return "images/sprMutant9BDead.png";
        }
        if idle.contains("sprMutant9CIdle") {
            return "images/sprMutant9CDead.png";
        }
        if idle.contains("sprMutant10BIdle") {
            return "images/sprMutant10BDead.png";
        }
        if idle.contains("sprMutant10CIdle") {
            return "images/sprMutant10CDead.png";
        }
        if idle.contains("sprMutant11BIdle") {
            return "images/sprMutant11BDead.png";
        }
        if idle.contains("sprMutant11CIdle") {
            return "images/sprMutant11CDead.png";
        }
        if idle.contains("sprMutant12BIdle") {
            return "images/sprMutant12BDead.png";
        }
        if idle.contains("sprMutant12CIdle") {
            return "images/sprMutant12CDead.png";
        }
        if idle.contains("sprMutant13BIdle") {
            return "images/sprMutant13BDead.png";
        }
        if idle.contains("sprMutant13CIdle") {
            return "images/sprMutant13CDead.png";
        }
        if idle.contains("sprMutant14BIdle") {
            return "images/sprMutant14BDead.png";
        }
        if idle.contains("sprMutant14CIdle") {
            return "images/sprMutant14CDead.png";
        }
        if idle.contains("sprMutant15BIdle") {
            return "images/sprMutant15BDead.png";
        }
        if idle.contains("sprMutant15CIdle") {
            return "images/sprMutant15CDead.png";
        }
        if idle.contains("sprMutant16BIdle") {
            return "images/sprMutant16BDead.png";
        }
        if idle.contains("sprMutant16CIdle") {
            return "images/sprMutant16CDead.png";
        }
        idle
    } else {
        idle
    };
    match base {
        "images/sprMutant1Idle.png" => "images/sprMutant1Dead.png",
        "images/sprMutant2Idle.png" => "images/sprMutant2Dead.png",
        "images/sprMutant3Idle.png" => "images/sprMutant3Dead.png",
        "images/sprMutant4Idle.png" => "images/sprMutant4Dead.png",
        "images/sprMutant5Idle.png" => "images/sprMutant5Dead.png",
        "images/sprMutant6Idle.png" => "images/sprMutant6Dead.png",
        "images/sprMutant7Idle.png" => "images/sprMutant7Dead.png",
        "images/sprMutant8Idle.png" => "images/sprMutant8Dead.png",
        "images/sprMutant9Idle.png" => "images/sprMutant9Dead.png",
        "images/sprMutant10Idle.png" => "images/sprMutant10Dead.png",
        "images/sprMutant11Idle.png" => "images/sprMutant11Dead.png",
        "images/sprMutant12Idle.png" => "images/sprMutant12Dead.png",
        "images/sprMutant13Idle.png" => "images/sprMutant13Dead.png",
        "images/sprMutant14Idle.png" => "images/sprMutant14Dead.png",
        "images/sprMutant15Idle.png" => "images/sprMutant15Dead.png",
        "images/sprMutant16Idle.png" => "images/sprMutant16Dead.png",
        "images/sprBanditIdle.png" => "images/sprBanditDead.png",
        "images/sprMaggotIdle.png" => "images/sprMaggotDead.png",
        "images/sprScorpionIdle.png" => "images/sprScorpionDead.png",
        "images/sprRatIdle.png" => "images/sprRatDead.png",
        "images/sprRatkingIdle.png" => "images/sprRatkingDead.png",
        "images/sprFreak1Idle.png" => "images/sprFreak1Dead.png",
        "images/sprJungleAssassinIdle.png" => "images/sprJungleAssassinDead.png",
        "images/sprSnowBotIdle.png" => "images/sprSnowBotDead.png",
        "images/sprTurretIdle.png" => "images/sprTurretDead.png",
        "images/sprSnowBanditIdle.png" => "images/sprSnowBanditDead.png",
        "images/sprWolfIdle.png" => "images/sprWolfDead.png",
        "images/sprBanditBossIdle.png" => "images/sprBanditBossDead.png",
        "images/sprGatorIdle.png" => "images/sprGatorDead.png",
        "images/sprBuffGatorIdle.png" => "images/sprBuffGatorDead.png",
        "images/sprRavenIdle.png" => "images/sprRavenDead.png",
        "images/sprSalamanderIdle.png" => "images/sprSalamanderDead.png",
        "images/sprMeleeIdle.png" => "images/sprMeleeDead.png",
        "images/sprJungleBanditIdle.png" => "images/sprJungleBanditDead.png",
        "images/sprBigMaggotIdle.png" => "images/sprBigMaggotDead.png",
        "images/sprFastRatIdle.png" => "images/sprFastRatDead.png",
        "images/sprGoldScorpionIdle.png" => "images/sprGoldScorpionDead.png",
        "images/sprLightningCrystalIdle.png" => "images/sprLightningCrystalDead.png",
        "images/sprExploFreakIdle.png" => "images/sprExploFreakDead.png",
        "images/sprRhinoFreakIdle.png" => "images/sprRhinoFreakDead.png",
        "images/sprSnowTankIdle.png" => "images/sprSnowTankDead.png",
        "images/sprGoldTankIdle.png" => "images/sprGoldTankDead.png",
        "images/sprGuardianIdle.png" => "images/sprGuardianDead.png",
        "images/sprExploGuardianIdle.png" => "images/sprExploGuardianDead.png",
        "images/sprDogGuardianWalk.png" => "images/sprDogGuardianDead.png",
        "images/sprBoneFish1Idle.png" => "images/sprBoneFish1Dead.png",
        "images/sprTurtleIdle.png" => "images/sprTurtleDead.png",
        "images/sprMolefishIdle.png" => "images/sprMolefishDead.png",
        "images/sprMolesargeIdle.png" => "images/sprMolesargeDead.png",
        "images/sprFireBallerIdle.png" => "images/sprFireBallerDead.png",
        "images/sprSuperFireBallerIdle.png" => "images/sprSuperFireBallerDead.png",
        "images/sprJockIdle.png" => "images/sprJockDead.png",
        "images/sprJungleFlyIdle.png" => "images/sprJungleFlyDead.png",
        "images/sprInvSpiderIdle.png" => "images/sprInvSpiderDead.png",
        "images/sprInvLaserCrystalIdle.png" => "images/sprInvLaserCrystalDead.png",
        "images/sprPopoFreakIdle.png" => "images/sprPopoFreakDead.png",
        "images/sprMSpawnIdle.png" => "images/sprMSpawnDead.png",
        "images/sprFrogQueenIdle.png" => "images/sprFrogQueenDead.png",
        _ => idle,
    }
}

/// GML spr_dead naming for props (verified against objects/*/Create_0.gml).
/// Static table only — no dynamic string building, no silent fallback.
pub fn derive_prop_dead_path(idle: &'static str) -> &'static str {
    match idle {
        "images/sprBarrel.png" => "images/sprBarrelDead.png",
        "images/sprToxicBarrel.png" => "images/sprToxicBarrelDead.png",
        "images/sprGoldBarrel.png" => "images/sprGoldBarrelDead.png",
        "images/sprOasisBarrel.png" => "images/sprOasisBarrelDead.png",
        "images/sprCactus.png" => "images/sprCactusDead.png",
        "images/sprCactus2.png" => "images/sprCactus2Dead.png",
        "images/sprCactus3.png" => "images/sprCactus3Dead.png",
        "images/sprCactusB.png" => "images/sprCactusBDead.png",
        "images/sprCactusB2.png" => "images/sprCactusB2Dead.png",
        "images/sprCactusB3.png" => "images/sprCactusB3Dead.png",
        "images/sprNightCactus.png" => "images/sprNightCactusDead.png",
        "images/sprNightCactus2.png" => "images/sprNightCactus2Dead.png",
        "images/sprNightCactus3.png" => "images/sprNightCactus3Dead.png",
        "images/sprSewerPipe.png" => "images/sprSewerPipeDead.png",
        "images/sprPipe.png" => "images/sprSewerPipeDead.png",
        "images/sprTires.png" => "images/sprTiresDead.png",
        "images/sprCocoon.png" => "images/sprCocoonDead.png",
        "images/sprSnowMan.png" => "images/sprSnowManDead.png",
        "images/sprSnowManIdle.png" => "images/sprSnowManDead.png",
        "images/sprTorch.png" => "images/sprTorchDead.png",
        "images/sprBonePileIdle.png" => "images/sprBonePileDead.png",
        "images/sprBonePile.png" => "images/sprBonePileDead.png",
        "images/sprNightBonePileIdle.png" => "images/sprNightBonePileDead.png",
        "images/sprBushIdle.png" => "images/sprBushDead.png",
        "images/sprBush.png" => "images/sprBushDead.png",
        "images/sprBigFlowerIdle.png" => "images/sprBigFlowerDead.png",
        "images/sprPlantPotIdle.png" => "images/sprPlantPotDead.png",
        "images/sprCrystalProp.png" => "images/sprCrystalPropDead.png",
        "images/sprHydrant.png" => "images/sprHydrantDead.png",
        "images/sprIcicle.png" => "images/sprIcicleDead.png",
        "images/sprStreetLight.png" => "images/sprStreetLightDead.png",
        "images/sprSodaMachine.png" => "images/sprSodaMachineDead.png",
        "images/sprNewsStand.png" => "images/sprNewsStandDead.png",
        "images/sprTube.png" => "images/sprTubeDead.png",
        "images/sprMutantTube.png" => "images/sprMutantTubeDead.png",
        "images/sprNuclearPillar.png" => "images/sprNuclearPillarDead.png",
        "images/sprPillar.png" => "images/sprNuclearPillarDead.png",
        "images/sprSmallGenerator.png" => "images/sprSmallGeneratorDead.png",
        "images/sprBigGenerator.png" => "images/sprBigGeneratorDead.png",
        "images/sprGenerator.png" => "images/sprBigGeneratorDead.png",
        "images/sprAnchor.png" => "images/sprAnchorDead.png",
        "images/sprWaterPlant.png" => "images/sprWaterPlantDead.png",
        "images/sprWaterPlant2.png" => "images/sprWaterPlantDead.png",
        "images/sprWaterMine.png" => "images/sprWaterMineDead.png",
        "images/sprMine.png" => "images/sprMineDead.png",
        "images/sprMineIdle.png" => "images/sprMineDead.png",
        "images/sprMoneyPile.png" => "images/sprMoneyPileDead.png",
        "images/sprYVStatue.png" => "images/sprYVStatueDead.png",
        "images/sprPizzaBox.png" => "images/sprPizzaBoxDead.png",
        "images/sprCarIdle.png" => "images/sprScorchmark.png",
        "images/sprFrozenCar.png" => "images/sprScorchmark.png",
        "images/sprBigSkullOpen.png" => "images/sprBigSkullDead.png",
        "images/sprBigSkull.png" => "images/sprBigSkullDead.png",
        "images/sprRadChest.png" => "images/sprRadChestCorpse.png",
        "images/sprRadChestIdle.png" => "images/sprRadChestCorpse.png",
        "images/sprThroneStatue.png" => "images/sprThroneStatueDead.png",
        _ => idle,
    }
}

/// Strict prop hurt resolver: bails with gen_assets hint if art is missing.
pub fn derive_prop_hurt_path_checked(catalog: &AssetCatalog, idle: &'static str) -> &'static str {
    let hurt = derive_hurt_path(idle);
    catalog.require(hurt);
    hurt
}

/// Strict prop dead resolver: bails with gen_assets hint if art is missing.
pub fn derive_prop_dead_path_checked(catalog: &AssetCatalog, idle: &'static str) -> &'static str {
    let dead = derive_prop_dead_path(idle);
    catalog.require(dead);
    dead
}

pub fn derive_walk_path(idle: &'static str) -> Option<&'static str> {
    match idle {
        "images/sprBanditIdle.png" => Some("images/sprBanditWalk.png"),
        // Maggot and Scorpion have no walk strips in this WAD (idle-only);
        // they simply fall through to None.
        "images/sprRatIdle.png" => Some("images/sprRatWalk.png"),
        "images/sprFreak1Idle.png" => Some("images/sprFreak1Walk.png"),
        "images/sprJungleAssassinIdle.png" => Some("images/sprJungleAssassinWalk.png"),
        "images/sprSnowBotIdle.png" => Some("images/sprSnowBotWalk.png"),
        "images/sprSnowBanditIdle.png" => Some("images/sprSnowBanditWalk.png"),
        "images/sprWolfIdle.png" => Some("images/sprWolfWalk.png"),
        // Expanded roster (upstream spr*Walk strips)
        "images/sprGatorIdle.png" => Some("images/sprGatorWalk.png"),
        "images/sprBuffGatorIdle.png" => Some("images/sprBuffGatorWalk.png"),
        "images/sprRavenIdle.png" => Some("images/sprRavenWalk.png"),
        "images/sprSalamanderIdle.png" => Some("images/sprSalamanderWalk.png"),
        "images/sprMeleeIdle.png" => Some("images/sprMeleeWalk.png"),
        "images/sprJungleBanditIdle.png" => Some("images/sprJungleBanditWalk.png"),
        "images/sprFastRatIdle.png" => Some("images/sprFastRatWalk.png"),
        "images/sprRatkingIdle.png" => Some("images/sprRatkingWalk.png"),
        "images/sprGoldScorpionIdle.png" => Some("images/sprGoldScorpionWalk.png"),
        "images/sprExploFreakIdle.png" => Some("images/sprExploFreakWalk.png"),
        "images/sprRhinoFreakIdle.png" => Some("images/sprRhinoFreakWalk.png"),
        "images/sprSnowTankIdle.png" => Some("images/sprSnowTankWalk.png"),
        "images/sprGoldTankIdle.png" => Some("images/sprGoldTankWalk.png"),
        "images/sprExploGuardianIdle.png" => Some("images/sprExploGuardianWalk.png"),
        // ADD - present in full WAD / gen_assets NT_ALL_SPRITES=1
        "images/sprGuardianIdle.png" => Some("images/sprGuardianWalk.png"),
        "images/sprTurtleIdle.png" => Some("images/sprTurtleWalk.png"),
        "images/sprBigMaggotIdle.png" => Some("images/sprBigMaggotWalk.png"),
        "images/sprBanditBossIdle.png" => Some("images/sprBanditBossWalk.png"),
        "images/sprFireBallerIdle.png" => Some("images/sprFireBallerWalk.png"),
        "images/sprSuperFireBallerIdle.png" => Some("images/sprSuperFireBallerWalk.png"),
        "images/sprBoneFish1Idle.png" => Some("images/sprBoneFish1Walk.png"),
        "images/sprMolefishIdle.png" => Some("images/sprMolefishWalk.png"),
        "images/sprMolesargeIdle.png" => Some("images/sprMolesargeWalk.png"),
        "images/sprJockIdle.png" => Some("images/sprJockWalk.png"),
        "images/sprJungleFlyIdle.png" => Some("images/sprJungleFlyWalk.png"),
        "images/sprInvSpiderIdle.png" => Some("images/sprInvSpiderWalk.png"),
        "images/sprPopoFreakIdle.png" => Some("images/sprPopoFreakWalk.png"),
        "images/sprFrogQueenIdle.png" => Some("images/sprFrogQueenWalk.png"),
        // DogGuardian uses Walk as "idle" already - hurt maps from Walk path
        _ => None,
    }
}

pub fn derive_walk_path_checked(
    catalog: &AssetCatalog,
    idle: &'static str,
) -> Option<&'static str> {
    derive_walk_path(idle).filter(|p| catalog.has(p))
}

pub fn derive_hurt_path_checked(catalog: &AssetCatalog, idle: &'static str) -> &'static str {
    let hurt = derive_hurt_path(idle);
    if catalog.has(hurt) { hurt } else { idle }
}

/// Play the hurt strip on any enemy whose Health just dropped.
pub fn hurt_on_damage(
    mut commands: Commands,
    catalog: Res<AssetCatalog>,
    asset_server: Res<AssetServer>,
    mut last_enemy_hp: Local<std::collections::HashMap<Entity, i32>>,
    mut last_player_hp: Local<std::collections::HashMap<Entity, i32>>,
    mut damaged: Query<
        (
            Entity,
            &Health,
            &EnemySprites,
            &mut SpriteAnim,
            &mut Sprite,
            &mut bevy::sprite::Anchor,
        ),
        (
            With<crate::game::components::Enemy>,
            Without<HurtAnim>,
            Without<crate::game::components::Player>,
        ),
    >,
    mut player_damaged: Query<
        (
            Entity,
            &Health,
            &PlayerAnim,
            &mut SpriteAnim,
            &mut Sprite,
            &mut bevy::sprite::Anchor,
        ),
        (
            With<crate::game::components::Player>,
            Without<HurtAnim>,
            Without<crate::game::components::Enemy>,
        ),
    >,
) {
    for (e, health, sprites, mut anim, mut sprite, mut anchor) in &mut damaged {
        let last = last_enemy_hp.get(&e).copied().unwrap_or(health.max);
        // Only react to actual hp drop, and let resolve_deaths own lethal.
        if health.hp >= health.max || health.hp <= 0 || health.hp == last {
            last_enemy_hp.insert(e, health.hp);
            continue;
        }
        last_enemy_hp.insert(e, health.hp);
        play_hurt(
            &mut commands,
            e,
            &catalog,
            &asset_server,
            &mut anim,
            &mut sprite,
            sprites.hurt,
            sprites.idle,
            sprites.walk,
        );
        let hurt_path = if catalog.anim_def(sprites.hurt).is_some() {
            sprites.hurt
        } else {
            sprites.idle
        };
        *anchor = crate::game::content::sprite_anchor(&catalog, hurt_path);
    }
    for (e, health, pa, mut anim, mut sprite, mut anchor) in &mut player_damaged {
        let last = last_player_hp.get(&e).copied().unwrap_or(health.max);
        if health.hp >= health.max || health.hp <= 0 || health.hp == last {
            last_player_hp.insert(e, health.hp);
            continue;
        }
        last_player_hp.insert(e, health.hp);
        play_hurt(
            &mut commands,
            e,
            &catalog,
            &asset_server,
            &mut anim,
            &mut sprite,
            pa.hurt,
            pa.idle,
            Some(pa.walk),
        );
        *anchor = crate::game::content::sprite_anchor(&catalog, pa.hurt);
    }
}

/// Tick PlayerDying – keeps dead strip visible for 0.85s then despawns
/// player entity, leaving the GroundPhysics corpse from resolve_deaths.
pub fn tick_player_dying(
    time: Res<Time<Fixed>>,
    mut commands: Commands,
    mut q: Query<(Entity, &mut crate::game::components::PlayerDying)>,
) {
    for (e, mut dying) in &mut q {
        dying.timer.tick(time.delta());
        if dying.timer.just_finished() {
            commands.entity(e).try_despawn();
        }
    }
}

/// GML prop/Step_1 + hitme damage: when hp drops but still alive, play
/// spr_hurt until image_index > 2, then spr_idle. Lethal is owned by
/// combat despawn + corpse.
pub fn prop_hurt_on_damage(
    mut commands: Commands,
    catalog: Res<AssetCatalog>,
    asset_server: Res<AssetServer>,
    mut q: Query<
        (
            Entity,
            &Prop,
            &mut PropHpTracker,
            &PropSprites,
            Option<&mut SpriteAnim>,
            &mut Sprite,
            &mut bevy::sprite::Anchor,
        ),
        (With<Prop>, Without<HurtAnim>),
    >,
) {
    for (e, prop, mut tracker, sprites, anim_opt, mut sprite, mut anchor) in &mut q {
        if !prop.destructible {
            tracker.last_hp = prop.hp;
            continue;
        }
        if prop.hp <= 0 {
            tracker.last_hp = prop.hp;
            continue;
        }
        if prop.hp >= tracker.last_hp {
            tracker.last_hp = prop.hp;
            continue;
        }
        tracker.last_hp = prop.hp;

        let Some(mut anim) = anim_opt else {
            continue;
        };

        play_hurt(
            &mut commands,
            e,
            &catalog,
            &asset_server,
            &mut anim,
            &mut sprite,
            sprites.hurt,
            sprites.idle,
            None,
        );
        *anchor = crate::game::content::sprite_anchor(&catalog, sprites.hurt);

        if sprites.flip_x {
            sprite.flip_x = true;
        }
    }
}
