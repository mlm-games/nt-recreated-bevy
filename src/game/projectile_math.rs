//! Pure helpers for projectile terminal effects (unit-tested without Bevy app).

use bevy::prelude::Vec2;

/// Spread angles relative to a parent flight direction.
pub fn split_directions(base_dir: Vec2, pellets: u8, spread: f32, samples: &[f32]) -> Vec<Vec2> {
    let base_angle = if base_dir.length_squared() > 1e-6 {
        base_dir.y.atan2(base_dir.x)
    } else {
        0.0
    };
    let mut out = Vec::with_capacity(pellets as usize);
    for i in 0..pellets as usize {
        let t = samples.get(i).copied().unwrap_or(0.0).clamp(-1.0, 1.0);
        let angle = base_angle + t * spread;
        out.push(Vec2::new(angle.cos(), angle.sin()));
    }
    out
}

/// Reflect velocity off an axis-aligned wall normal (unit X or Y).
pub fn bounce_velocity(vel: Vec2, normal: Vec2) -> Vec2 {
    vel - 2.0 * vel.dot(normal) * normal
}

/// Decide whether a piercing projectile should despawn after a contact.
/// `damaged` = real HP change; `pierce_left_before` = charges before this hit.
pub fn should_despawn_after_hit(
    damaged: bool,
    pierce_left_before: Option<u8>,
) -> (bool, Option<u8>) {
    match pierce_left_before {
        Some(n) if n > 0 && damaged => (false, Some(n - 1)),
        Some(n) if n > 0 && !damaged => (false, Some(n)), // shield/invuln: pass through
        Some(0) | None if damaged => (true, pierce_left_before.map(|_| 0)),
        _ => (true, pierce_left_before),
    }
}

/// Track whether `target` is new to the pierce set.
pub fn record_hit(set: &mut Vec<bevy::prelude::Entity>, target: bevy::prelude::Entity) -> bool {
    if set.contains(&target) {
        false
    } else {
        set.push(target);
        true
    }
}
