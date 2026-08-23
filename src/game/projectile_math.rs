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

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::prelude::Entity;

    #[test]
    fn split_follows_parent_direction_up() {
        let dirs = split_directions(Vec2::Y, 3, 0.0, &[0.0, 0.0, 0.0]);
        assert_eq!(dirs.len(), 3);
        for d in dirs {
            assert!(d.y > 0.9, "expected upward split, got {d:?}");
            assert!(d.x.abs() < 0.1);
        }
    }

    #[test]
    fn split_follows_parent_direction_left() {
        let dirs = split_directions(Vec2::NEG_X, 1, 0.0, &[0.0]);
        assert!(dirs[0].x < -0.9);
        assert!(dirs[0].y.abs() < 0.1);
    }

    #[test]
    fn split_spread_is_relative_not_world_x() {
        // Parent flying +Y; sample at +spread should tilt right of up, not world +X.
        let dirs = split_directions(Vec2::Y, 1, std::f32::consts::FRAC_PI_2, &[1.0]);
        // base π/2 + π/2 = π → left (-X)
        assert!(dirs[0].x < -0.9, "got {:?}", dirs[0]);
    }

    #[test]
    fn bounce_flips_x_on_vertical_wall() {
        let v = bounce_velocity(Vec2::new(100.0, 50.0), Vec2::X);
        assert!((v.x + 100.0).abs() < 1e-3);
        assert!((v.y - 50.0).abs() < 1e-3);
    }

    #[test]
    fn bounce_flips_y_on_horizontal_wall() {
        let v = bounce_velocity(Vec2::new(40.0, -80.0), Vec2::Y);
        assert!((v.x - 40.0).abs() < 1e-3);
        assert!((v.y - 80.0).abs() < 1e-3);
    }

    #[test]
    fn pierce_consumes_charge_only_on_damage() {
        let (despawn, left) = should_despawn_after_hit(true, Some(2));
        assert!(!despawn);
        assert_eq!(left, Some(1));

        let (despawn, left) = should_despawn_after_hit(false, Some(2));
        assert!(!despawn);
        assert_eq!(left, Some(2));

        let (despawn, _) = should_despawn_after_hit(true, Some(0));
        assert!(despawn);

        let (despawn, _) = should_despawn_after_hit(true, None);
        assert!(despawn);
    }

    #[test]
    fn hit_set_rejects_duplicates() {
        let a = Entity::from_bits(1);
        let b = Entity::from_bits(2);
        let mut set = Vec::new();
        assert!(record_hit(&mut set, a));
        assert!(!record_hit(&mut set, a));
        assert!(record_hit(&mut set, b));
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn free_ammo_heal_is_one_per_ammo_pickup() {
        // Document Robot passive contract used by pickups.rs
        fn free_ammo_heal(gained: i32, free_ammo: bool) -> i32 {
            if free_ammo && gained > 0 { 1 } else { 0 }
        }
        assert_eq!(free_ammo_heal(8, true), 1);
        assert_eq!(free_ammo_heal(0, true), 0);
        assert_eq!(free_ammo_heal(8, false), 0);
    }
}
