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
/// Large-orb split line angles for Throne II (full-circle lines).
#[allow(dead_code)] // wired into Throne II orb splits next pattern pass
pub fn split_line_dirs(base_angle: f32, lines: usize) -> Vec<f32> {
    if lines == 0 {
        return Vec::new();
    }
    let step = std::f32::consts::TAU / lines as f32;
    (0..lines).map(|i| base_angle + step * i as f32).collect()
}

/// Star/static attack angles.
#[allow(dead_code)]
pub fn star_angles(points: usize, phase: f32) -> Vec<f32> {
    ring_angles(points.max(1), phase)
}

/// Orbiting laser crystal count for Hyper Crystal.
#[allow(dead_code)]
pub fn hyper_orbit_count(loop_count: u32) -> usize {
    5 + loop_count.saturating_sub(1) as usize * 2
}

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

#[cfg(test)]
mod loop_pattern_tests {
    use super::*;

    #[test]
    fn split_lines_cover_full_circle() {
        let dirs = split_line_dirs(0.0, 8);
        assert_eq!(dirs.len(), 8);
        assert!((dirs[4] - std::f32::consts::PI).abs() < 0.001);
    }

    #[test]
    fn hyper_orbit_scales_with_loop() {
        assert_eq!(hyper_orbit_count(0), 5);
        assert_eq!(hyper_orbit_count(1), 5);
        assert_eq!(hyper_orbit_count(2), 7);
        assert_eq!(hyper_orbit_count(3), 9);
    }

    #[test]
    fn star_has_requested_points() {
        assert_eq!(star_angles(12, 0.1).len(), 12);
    }

    #[test]
    fn loop_floor_math_starts_loop_one_at_16() {
        let floor = 1 * 15 + 1;
        assert_eq!(floor, 16);
        assert_eq!(crate::game::areas::route_coordinates(floor), (1, 1));
        assert_eq!(
            crate::game::areas::area_for_floor(floor, 1),
            crate::game::areas::AreaId::Desert
        );
    }

    #[test]
    fn loop_floor_math_starts_loop_two_at_31() {
        let floor = 2 * 15 + 1;
        assert_eq!(floor, 31);
        assert_eq!(crate::game::areas::route_coordinates(floor), (1, 1));
    }
}
