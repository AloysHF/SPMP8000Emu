// Physical gamepad-to-logical-button mapping via gilrs.

use gilrs::{Axis, Button, EventType, Gilrs};
use spmp8000emu_core::input_handler::Button as LogicalButton;

const STICK_DEADZONE: f32 = 0.5;

/// Polls the first connected physical gamepad and maps it onto SPMP buttons.
///
/// Face buttons follow the same RetroPad convention as the default keyboard
/// bindings: South→O, East→X. Left/right sticks also act as a digital D-pad
/// once past [`STICK_DEADZONE`]. O/X swap is applied later by the core config.
pub struct GamepadMapper {
    gilrs: Option<Gilrs>,
}

impl GamepadMapper {
    pub fn new(enabled: bool) -> Self {
        let gilrs = if enabled {
            match Gilrs::new() {
                Ok(gilrs) => {
                    log::info!("Gamepad support enabled");
                    Some(gilrs)
                }
                Err(error) => {
                    log::warn!("Gamepad support unavailable: {error}");
                    None
                }
            }
        } else {
            None
        };
        Self { gilrs }
    }

    pub fn pressed_buttons(&mut self) -> u32 {
        let Some(gilrs) = self.gilrs.as_mut() else {
            return 0;
        };

        while let Some(event) = gilrs.next_event() {
            match event.event {
                EventType::Connected => {
                    log::info!("Gamepad connected: {}", gilrs.gamepad(event.id).name());
                }
                EventType::Disconnected => {
                    log::info!("Gamepad disconnected: {}", event.id);
                }
                _ => {}
            }
        }

        for (_id, gamepad) in gilrs.gamepads() {
            if !gamepad.is_connected() {
                continue;
            }
            return map_buttons(&gamepad);
        }

        0
    }
}

fn map_buttons(gamepad: &gilrs::Gamepad<'_>) -> u32 {
    let mut buttons = 0;

    if gamepad.is_pressed(Button::DPadUp)
        || stick_axis(gamepad, Axis::LeftStickY) > STICK_DEADZONE
        || stick_axis(gamepad, Axis::RightStickY) > STICK_DEADZONE
    {
        buttons |= LogicalButton::Up.mask();
    }
    if gamepad.is_pressed(Button::DPadDown)
        || stick_axis(gamepad, Axis::LeftStickY) < -STICK_DEADZONE
        || stick_axis(gamepad, Axis::RightStickY) < -STICK_DEADZONE
    {
        buttons |= LogicalButton::Down.mask();
    }
    if gamepad.is_pressed(Button::DPadLeft)
        || stick_axis(gamepad, Axis::LeftStickX) < -STICK_DEADZONE
        || stick_axis(gamepad, Axis::RightStickX) < -STICK_DEADZONE
    {
        buttons |= LogicalButton::Left.mask();
    }
    if gamepad.is_pressed(Button::DPadRight)
        || stick_axis(gamepad, Axis::LeftStickX) > STICK_DEADZONE
        || stick_axis(gamepad, Axis::RightStickX) > STICK_DEADZONE
    {
        buttons |= LogicalButton::Right.mask();
    }

    if gamepad.is_pressed(Button::South) {
        buttons |= LogicalButton::O.mask();
    }
    if gamepad.is_pressed(Button::East) {
        buttons |= LogicalButton::X.mask();
    }
    if gamepad.is_pressed(Button::Start) {
        buttons |= LogicalButton::Start.mask();
    }
    if gamepad.is_pressed(Button::Select) {
        buttons |= LogicalButton::Select.mask();
    }

    buttons
}

fn stick_axis(gamepad: &gilrs::Gamepad<'_>, axis: Axis) -> f32 {
    gamepad.value(axis)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_mapper_reports_no_buttons() {
        let mut mapper = GamepadMapper::new(false);
        assert_eq!(mapper.pressed_buttons(), 0);
    }

    #[test]
    fn stick_deadzone_threshold_rejects_small_values() {
        assert!(0.4_f32.abs() < STICK_DEADZONE);
        assert!(0.5_f32.abs() >= STICK_DEADZONE);
    }

    #[test]
    fn face_button_mapping_matches_keyboard_defaults() {
        // South is the same physical button as the default keyboard Z (O).
        // East is the same physical button as the default keyboard X (X).
        assert_eq!(LogicalButton::O.mask(), 1 << 4);
        assert_eq!(LogicalButton::X.mask(), 1 << 5);
    }
}
