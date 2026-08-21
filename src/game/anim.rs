//! Strip-sheet animation: `anims.json` (written by tools/gen_assets.py)
//! maps sprite names to {frames, w, h, fps}; sprites store a horizontal
//! strip and we slice one frame per tick via `Sprite::rect`.

use bevy::prelude::*;
use std::collections::HashMap;

use crate::game::components::Velocity;
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
    pub path: &'static str,
    pub def: AnimDef,
    pub frame: u32,
    pub timer: Timer,
}

impl SpriteAnim {
    pub fn new(path: &'static str, def: AnimDef) -> Self {
        Self {
            path,
            def,
            frame: 0,
            timer: Timer::from_seconds(1.0 / def.fps.max(0.1), TimerMode::Repeating),
        }
    }

    /// Rect for the current frame inside the strip.
    pub fn rect(&self) -> Rect {
        let w = self.def.frame_px as f32;
        let h = self.def.height as f32;
        Rect::new(
            self.frame as f32 * w,
            0.0,
            self.frame as f32 * w + w,
            h,
        )
    }
}

/// Build an animated sprite when `path` has strip data; static otherwise.
pub fn sprite_anim(
    catalog: &AssetCatalog,
    asset_server: &AssetServer,
    path: &'static str,
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
        anim.timer.tick(time.delta());
        if anim.timer.just_finished() {
            anim.frame = (anim.frame + 1) % anim.def.frames.max(1);
            sprite.rect = Some(anim.rect());
        }
    }
}

/// Player idle/walk strip pair; `moving` selects which is displayed.
#[derive(Component)]
pub struct PlayerAnim {
    pub idle: &'static str,
    pub walk: &'static str,
    pub moving: bool,
}

/// Swap the player's strip when movement state changes.
pub fn player_anim_switch(
    asset_server: Res<AssetServer>,
    catalog: Res<AssetCatalog>,
    mut q: Query<(&Velocity, &mut PlayerAnim, &mut SpriteAnim, &mut Sprite)>,
) {
    for (vel, mut pa, mut anim, mut sprite) in &mut q {
        let moving = vel.0.length_squared() > 100.0;
        if moving == pa.moving {
            continue;
        }
        pa.moving = moving;
        let path = if moving { pa.walk } else { pa.idle };
        let Some(def) = catalog.anim_def(path) else {
            continue;
        };
        anim.path = path;
        anim.def = def;
        anim.frame = 0;
        anim.timer = Timer::from_seconds(1.0 / def.fps.max(0.1), TimerMode::Repeating);
        sprite.image = asset_server.load(path.to_string());
        sprite.rect = Some(anim.rect());
    }
}
