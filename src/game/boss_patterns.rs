//! Pure boss pattern math: fans, rings, aim leading, orbit points.

use bevy::prelude::*;

pub fn fan_angles(base_angle: f32, count: usize, spread: f32) -> Vec<f32> {
    if count == 0 {
        return Vec::new();
    }

    if count == 1 {
        return vec![base_angle];
    }

    let center = (count as f32 - 1.0) * 0.5;
    (0..count)
        .map(|i| base_angle + (i as f32 - center) * spread)
        .collect()
}

pub fn ring_angles(count: usize, phase: f32) -> Vec<f32> {
    if count == 0 {
        return Vec::new();
    }

    let step = std::f32::consts::TAU / count as f32;
    (0..count).map(|i| phase + step * i as f32).collect()
}

pub fn dir_from_angle(angle: f32) -> Vec2 {
    Vec2::new(angle.cos(), angle.sin()).normalize_or_zero()
}

pub fn lead_target(shooter: Vec2, target: Vec2, target_vel: Vec2, projectile_speed: f32) -> Vec2 {
    let to = target - shooter;
    let dist = to.length();
    if projectile_speed <= 1.0 || dist <= 1.0 {
        return to.normalize_or_zero();
    }

    let time = (dist / projectile_speed).clamp(0.0, 1.2);
    (target + target_vel * time - shooter).normalize_or_zero()
}

#[allow(dead_code)] // pure helper kept for upcoming orbit-phase patterns
pub fn orbit_point(center: Vec2, radius: f32, angle: f32) -> Vec2 {
    center + dir_from_angle(angle) * radius
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fan_has_center_angle_for_odd_counts() {
        let angles = fan_angles(1.0, 5, 0.25);
        assert_eq!(angles.len(), 5);
        assert!((angles[2] - 1.0).abs() < 0.0001);
        assert!((angles[0] - 0.5).abs() < 0.0001);
        assert!((angles[4] - 1.5).abs() < 0.0001);
    }

    #[test]
    fn fan_single_is_base() {
        let angles = fan_angles(2.0, 1, 0.5);
        assert_eq!(angles, vec![2.0]);
    }

    #[test]
    fn ring_is_evenly_spaced() {
        let angles = ring_angles(4, 0.0);
        assert_eq!(angles.len(), 4);
        assert!((angles[1] - std::f32::consts::FRAC_PI_2).abs() < 0.0001);
        assert!((angles[2] - std::f32::consts::PI).abs() < 0.0001);
    }

    #[test]
    fn dir_from_zero_is_x_axis() {
        let d = dir_from_angle(0.0);
        assert!(d.x > 0.99);
        assert!(d.y.abs() < 0.001);
    }

    #[test]
    fn lead_target_points_forward() {
        let d = lead_target(Vec2::ZERO, Vec2::new(10.0, 0.0), Vec2::new(0.0, 10.0), 20.0);
        assert!(d.x > 0.6);
        assert!(d.y > 0.2);
    }

    #[test]
    fn orbit_point_uses_radius() {
        let p = orbit_point(Vec2::new(2.0, 3.0), 10.0, 0.0);
        assert!((p.x - 12.0).abs() < 0.0001);
        assert!((p.y - 3.0).abs() < 0.0001);
    }

    #[test]
    fn big_dog_ring_counts_are_even() {
        let normal = ring_angles(14, 0.0);
        let enraged = ring_angles(18, 0.0);
        assert_eq!(normal.len(), 14);
        assert_eq!(enraged.len(), 18);
    }

    #[test]
    fn throne_ring_has_no_duplicate_zero_for_full_circle() {
        let angles = ring_angles(24, 0.0);
        assert_eq!(angles.len(), 24);
        assert!(angles[0].abs() < 0.0001);
        assert!(angles[23] < std::f32::consts::TAU);
    }

    #[test]
    fn lil_hunter_fan_is_symmetric() {
        let angles = fan_angles(0.0, 3, 0.13);
        assert_eq!(angles.len(), 3);
        assert!((angles[0] + angles[2]).abs() < 0.0001);
        assert!(angles[1].abs() < 0.0001);
    }

    #[test]
    fn big_bandit_fan_is_wider_than_lil_hunter() {
        let bandit = fan_angles(0.0, 5, 0.16);
        let hunter = fan_angles(0.0, 3, 0.13);
        let bandit_width = bandit.last().unwrap() - bandit.first().unwrap();
        let hunter_width = hunter.last().unwrap() - hunter.first().unwrap();
        assert!(bandit_width > hunter_width);
    }

    #[test]
    fn lead_target_does_not_nan_on_zero_speed() {
        let d = lead_target(Vec2::ZERO, Vec2::ZERO, Vec2::ZERO, 0.0);
        assert!(d.is_finite());
    }
}
