use bevy::input::gamepad::{Gamepad, GamepadAxis, GamepadButton};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

/// Unified gameplay input sampled once per frame from keyboard, mouse,
/// gamepads, and touch. Pulse-style fields are consumed by FixedUpdate
/// gameplay systems via `take_*`.
#[derive(Resource, Debug, Clone)]
pub struct NtInput {
    pub move_axis: Vec2,
    pub aim_axis: Vec2,
    pub fire_held: bool,

    fire_pressed: bool,
    ability_pressed: bool,
    weapon_slot: Option<usize>,
    cycle_weapon: i8,
}

impl Default for NtInput {
    fn default() -> Self {
        Self {
            move_axis: Vec2::ZERO,
            aim_axis: Vec2::ZERO,
            fire_held: false,
            fire_pressed: false,
            ability_pressed: false,
            weapon_slot: None,
            cycle_weapon: 0,
        }
    }
}

impl NtInput {
    pub fn take_fire_pressed(&mut self) -> bool {
        std::mem::take(&mut self.fire_pressed)
    }

    pub fn take_ability_pressed(&mut self) -> bool {
        std::mem::take(&mut self.ability_pressed)
    }

    pub fn take_weapon_slot(&mut self) -> Option<usize> {
        self.weapon_slot.take()
    }

    pub fn take_cycle_weapon(&mut self) -> i8 {
        std::mem::take(&mut self.cycle_weapon)
    }

    #[allow(dead_code)]
    pub fn clear_transient(&mut self) {
        self.fire_pressed = false;
        self.ability_pressed = false;
        self.weapon_slot = None;
        self.cycle_weapon = 0;
    }
}

fn dead_zone(value: Vec2) -> Vec2 {
    const DEAD_ZONE: f32 = 0.22;

    let length = value.length();
    if length <= DEAD_ZONE {
        return Vec2::ZERO;
    }

    let scaled = ((length - DEAD_ZONE) / (1.0 - DEAD_ZONE)).clamp(0.0, 1.0);
    value.normalize_or_zero() * scaled
}

fn keyboard_move(keys: &ButtonInput<KeyCode>) -> Vec2 {
    let mut value = Vec2::ZERO;

    if keys.pressed(KeyCode::KeyW) || keys.pressed(KeyCode::ArrowUp) {
        value.y += 1.0;
    }
    if keys.pressed(KeyCode::KeyS) || keys.pressed(KeyCode::ArrowDown) {
        value.y -= 1.0;
    }
    if keys.pressed(KeyCode::KeyA) || keys.pressed(KeyCode::ArrowLeft) {
        value.x -= 1.0;
    }
    if keys.pressed(KeyCode::KeyD) || keys.pressed(KeyCode::ArrowRight) {
        value.x += 1.0;
    }

    value.normalize_or_zero()
}

pub fn sample_input(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    touches: Res<Touches>,
    windows: Query<&Window, With<PrimaryWindow>>,
    gamepads: Query<&Gamepad>,
    mut output: ResMut<NtInput>,
) {
    let mut move_axis = keyboard_move(&keys);
    let mut aim_axis = Vec2::ZERO;

    let mut fire_held = mouse.pressed(MouseButton::Left) || keys.pressed(KeyCode::Space);
    let mut fire_pressed =
        mouse.just_pressed(MouseButton::Left) || keys.just_pressed(KeyCode::Space);
    let mut ability_pressed =
        keys.just_pressed(KeyCode::KeyE) || keys.just_pressed(KeyCode::ShiftLeft);

    let mut weapon_slot = None;
    let mut cycle_weapon = 0_i8;

    if keys.just_pressed(KeyCode::Digit1) {
        weapon_slot = Some(0);
    } else if keys.just_pressed(KeyCode::Digit2) {
        weapon_slot = Some(1);
    } else if keys.just_pressed(KeyCode::Digit3) {
        weapon_slot = Some(2);
    }

    for gamepad in &gamepads {
        let left = dead_zone(Vec2::new(
            gamepad.get(GamepadAxis::LeftStickX).unwrap_or(0.0),
            gamepad.get(GamepadAxis::LeftStickY).unwrap_or(0.0),
        ));
        let right = dead_zone(Vec2::new(
            gamepad.get(GamepadAxis::RightStickX).unwrap_or(0.0),
            gamepad.get(GamepadAxis::RightStickY).unwrap_or(0.0),
        ));

        if left != Vec2::ZERO {
            move_axis = left;
        }
        if right != Vec2::ZERO {
            aim_axis = right;
        }

        fire_held |= gamepad.pressed(GamepadButton::RightTrigger2);
        fire_pressed |= gamepad.just_pressed(GamepadButton::RightTrigger2);
        ability_pressed |= gamepad.just_pressed(GamepadButton::LeftTrigger2);

        if gamepad.just_pressed(GamepadButton::DPadLeft) {
            weapon_slot = Some(0);
        } else if gamepad.just_pressed(GamepadButton::DPadUp) {
            weapon_slot = Some(1);
        } else if gamepad.just_pressed(GamepadButton::DPadRight) {
            weapon_slot = Some(2);
        }

        if gamepad.just_pressed(GamepadButton::North) {
            cycle_weapon = 1;
        }
    }

    if let Ok(window) = windows.single() {
        let width = window.width();

        // Top-right touch buttons: [cycle weapon][active ability].
        for touch in touches.iter_just_pressed() {
            let start = touch.start_position();

            if start.y < 96.0 && start.x >= width - 96.0 {
                ability_pressed = true;
            } else if start.y < 96.0 && start.x >= width - 192.0 {
                cycle_weapon = 1;
            } else if start.x >= width * 0.5 {
                fire_pressed = true;
            }
        }

        // Left half: movement stick. Right half: aim + fire stick.
        for touch in touches.iter() {
            let start = touch.start_position();

            if start.y < 96.0 && start.x >= width - 192.0 {
                continue;
            }

            let screen_delta = touch.position() - start;
            let stick = Vec2::new(screen_delta.x, -screen_delta.y) / 56.0;
            let stick = dead_zone(stick.clamp_length_max(1.0));

            if start.x < width * 0.5 {
                if stick != Vec2::ZERO {
                    move_axis = stick;
                }
            } else {
                fire_held = true;
                if stick != Vec2::ZERO {
                    aim_axis = stick;
                }
            }
        }
    }

    output.move_axis = move_axis.clamp_length_max(1.0);
    output.aim_axis = aim_axis.clamp_length_max(1.0);
    output.fire_held = fire_held;

    // Keep pulses queued until a FixedUpdate gameplay system consumes them.
    output.fire_pressed |= fire_pressed;
    output.ability_pressed |= ability_pressed;

    if weapon_slot.is_some() {
        output.weapon_slot = weapon_slot;
    }
    output.cycle_weapon = output.cycle_weapon.saturating_add(cycle_weapon);
}
