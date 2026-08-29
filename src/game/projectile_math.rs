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

/// Outward normal from a circle vs axis-aligned box (closest-point).
/// Returns None if not overlapping.
pub fn circle_aabb_normal(pos: Vec2, radius: f32, center: Vec2, half: Vec2) -> Option<Vec2> {
    let closest = Vec2::new(
        pos.x.clamp(center.x - half.x, center.x + half.x),
        pos.y.clamp(center.y - half.y, center.y + half.y),
    );
    let delta = pos - closest;
    let d2 = delta.length_squared();
    if d2 > radius * radius {
        return None;
    }
    if d2 > 1e-8 {
        return Some(delta.normalize());
    }
    // Center inside box: push out along shallowest axis.
    let dx = half.x - (pos.x - center.x).abs();
    let dy = half.y - (pos.y - center.y).abs();
    if dx < dy {
        Some(Vec2::new((pos.x - center.x).signum(), 0.0))
    } else {
        Some(Vec2::new(0.0, (pos.y - center.y).signum()))
    }
}

pub fn arena_wall_normal(pos: Vec2, radius: f32, arena_w: f32, arena_h: f32) -> Option<Vec2> {
    let hx = arena_w * 0.5 - radius;
    let hy = arena_h * 0.5 - radius;
    let mut n = Vec2::ZERO;
    if pos.x > hx {
        n.x = 1.0;
    } else if pos.x < -hx {
        n.x = -1.0;
    }
    if pos.y > hy {
        n.y = 1.0;
    } else if pos.y < -hy {
        n.y = -1.0;
    }
    if n == Vec2::ZERO {
        None
    } else {
        Some(n.normalize_or_zero())
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disc_bounces_off_prop_aabb() {
        let n = circle_aabb_normal(Vec2::new(10.0, 0.0), 4.0, Vec2::ZERO, Vec2::splat(8.0));
        assert!(n.is_some());
        let v = bounce_velocity(Vec2::new(100.0, 0.0), n.unwrap());
        assert!(v.x < 0.0);
    }

    #[test]
    fn arena_wall_normal_detects_edge() {
        let n = arena_wall_normal(Vec2::new(1290.0, 0.0), 4.0, 2560.0, 1664.0);
        assert!(n.is_some());
        assert!(n.unwrap().x > 0.0);
    }
}
