//! The Controls page: the in-game button icons, the device, and the table of the chosen device
//! (keyboard & mouse, or gamepad).
//!
//! The edited tables live in `ControlsView`, not in the settings: Task 32's `gather` writes their
//! `control_changes` into `settings.controls`. A key box waits for an input after a click: the next
//! key (`key_pressed`, fed by `Message::Key`), mouse button or wheel turn (`subscription`) is bound
//! with `bind_key`, or the box gives up after `CAPTURE_SECONDS`.

use std::time::Duration;

use iced::keyboard::key::Named;
use iced::keyboard::{Key, Location};
use iced::time::Instant;
use iced::widget::{button, column, container, mouse_area, pick_list, row as layout, space, text};
use iced::{event, mouse, window, Center, Element, Event, Fill, Padding, Subscription, Task};
use indexmap::IndexMap;
use serde_json::Value;

use crate::core::controls::{
    at_defaults, bind_key, control_values, default_input, sdl_key_name, table_rows, Kind, CONTROLS,
    PAD_STYLES, RAGE_BUTTONS,
};
use crate::core::gpu::connected_gamepad;
use crate::core::i18n::{tr, tr_fmt};
use crate::core::ini::INI_DEFAULTS;
use crate::core::options::{AIM_STYLES, GLYPH_STYLES, STICK_PROMPTS};
use crate::core::settings::Settings;
use crate::ui::app::{apply_set, Message, State};
use crate::ui::tasks::blocking;
use crate::ui::theme::{self, palette};
use crate::ui::widgets::button_icon::game_button;
use crate::ui::widgets::{
    button_icon, checkbox, checkbox_ini, min_width, row, section, select, slider, SELECT_MIN,
    SELECT_TEXT,
};

/// A waiting key box gives up after this long.
pub const CAPTURE_SECONDS: u64 = 5;

/// The game's buttons as the page names them.
pub const PS_BUTTON_NAMES: &[(&str, &str)] = &[
    ("cross", "Cross"),
    ("circle", "Circle"),
    ("square", "Square"),
    ("triangle", "Triangle"),
    ("l1", "L1"),
    ("r1", "R1"),
    ("l2", "L2"),
    ("r2", "R2"),
    ("l3", "L3"),
    ("r3", "R3"),
    ("options", "Options"),
    ("touchpad_left", "Touchpad left"),
    ("touchpad_right", "Touchpad right"),
    ("up", "D-pad up"),
    ("down", "D-pad down"),
    ("left", "D-pad left"),
    ("right", "D-pad right"),
];

/// The actions of the keyboard & mouse table only.
pub const ACTION_NAMES: &[(&str, &str)] = &[
    ("rage", "Rage of Sparta"),
    ("grab", "Grab"),
    ("dodge", "Dodge"),
];

/// The controller types, in `PAD_STYLES` order; the names are not translated.
pub const PAD_STYLE_NAMES: &[(&str, &str)] = &[
    ("playstation", "PlayStation"),
    ("xbox", "Xbox"),
    ("nintendo", "Nintendo"),
];

const GLYPH_HINT: &str = "Replaces the PlayStation button icons inside the game with the ones of your controller. Works on a copy of up to three game files in the port's data folder (about 410 MB); your game folder is never changed. Applied when the game starts.";
const RAGE_CHECK: &str = "Rage of Sparta on one input (L3 + R3 together)";
const KEY_NOTE: &str = "Click a box, then press a key, click a mouse button or turn the wheel (it gives up after {} seconds). An input already used by another button swaps with it; the arrow puts one button back on its default. Rage of Sparta presses L3 and R3 together: tick the box above it to use one input instead of two. Grab presses L1 and Circle together. Dodge evades the way you are moving (WASD), or back when standing still. Touchpad right opens the game's pause menu. Insert always opens the port's menu and cannot be bound. Applied when the game starts.";
const STICK_HINT: &str = "How the stick-rotation prompts are answered. Keyboard: type the turn instead, half turns from the top D S A (clockwise) or A S D, from the bottom A W D or D W A, full turns D S A W or A S D W; a key out of turn starts again. Adaptive mouse: draw the turn with the mouse.";
const DISTANCE_HINT: &str =
    "Adaptive mouse: how far the mouse moves for one step of a rotation. Also in the in-game menu.";
const AIM_STYLE_HINT: &str = "Aiming with the mouse while L2 is held (Head of Helios, Bow of Apollo). Relative: the stick follows the movement and goes back to neutral when the mouse stops. Absolute: where the mouse was when L2 was pressed is the centre, and the stick points from it to the mouse. Also in the in-game menu.";
const SENSITIVITY_HINT: &str = "Aiming with the mouse while L2 is held: lower needs more mouse movement for the same stick, higher less. Also in the in-game menu.";
const MOUSE_LOCK_HINT: &str = "While you play, the mouse stays in the game window, so a click cannot land on another monitor or app. The in-game menu and switching windows get it back. Also in the in-game menu.";
const VIBRATION_HINT: &str = "Strength of the gamepad's vibration; 0% turns it off.";
const PAD_NOTE: &str = "L2/R2 stay on the triggers. Touchpad right opens the game's pause menu; Touchpad left (photo mode) is L1 + Touchpad right. R3+L2 always opens the port's menu. Applied when the game starts.";

/// Side of a button icon.
const ICON_SIZE: f32 = 28.0;
/// Width of a key box, enough for the countdown text.
const BOX_WIDTH: f32 = 220.0;

/// The device whose table the page shows; both keep working in the game.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Device {
    #[default]
    Keyboard,
    Gamepad,
}

impl Device {
    /// The Python launcher's `CONTROL_DEVICES` order.
    pub const ALL: [Device; 2] = [Device::Keyboard, Device::Gamepad];

    /// The value saved as `controls_device`.
    pub fn value(self) -> &'static str {
        match self {
            Device::Keyboard => "keyboard",
            Device::Gamepad => "gamepad",
        }
    }

    /// The English radio label, which is also the translation key.
    pub fn label_en(self) -> &'static str {
        match self {
            Device::Keyboard => "Keyboard & mouse",
            Device::Gamepad => "Gamepad",
        }
    }

    pub fn from_value(value: &str) -> Option<Device> {
        Device::ALL
            .into_iter()
            .find(|device| device.value() == value)
    }
}

/// The page's edits: the device, both tables and the key box that waits for an input.
#[derive(Debug, Clone)]
pub struct ControlsView {
    pub device: Device,
    /// Whether the player chose the device; an untouched first choice follows the gamepad probe.
    pub device_chosen: bool,
    /// Keyboard & mouse input by game button (`table_rows(Kind::Key)`).
    // Task 32: `gather` writes `control_changes` of `key` and `pad` into `settings.controls`.
    pub key: IndexMap<String, String>,
    /// SDL gamepad button by game button (`CONTROLS`); empty for L2, R2 and Touchpad left.
    pub pad: IndexMap<String, String>,
    pub pad_style: String,
    /// The game button whose key box waits for an input.
    pub capturing: Option<String>,
    pub capture_started: Option<Instant>,
    /// Whether the first open already looked for a gamepad.
    pub gamepad_probe_started: bool,
}

impl ControlsView {
    /// The saved tables, device and controller type (Python `build_controls`).
    pub fn new(settings: &Settings) -> ControlsView {
        let null = Value::Null;
        let saved = settings.get("controls").unwrap_or(&null);
        let style = settings.get_str("pad_style");
        let pad_style = if PAD_STYLES.iter().any(|(known, _)| *known == style) {
            style
        } else {
            "playstation".to_string()
        };
        let device = Device::from_value(&settings.get_str("controls_device"));
        ControlsView {
            device: device.unwrap_or_default(),
            device_chosen: device.is_some(),
            key: table(saved, Kind::Key),
            pad: table(saved, Kind::Pad),
            pad_style,
            capturing: None,
            capture_started: None,
            gamepad_probe_started: false,
        }
    }
}

/// Every row of a table on its saved input, or on its default (`""` when it has none).
fn table(saved: &Value, kind: Kind) -> IndexMap<String, String> {
    let chosen = control_values(saved, kind);
    table_rows(kind)
        .into_iter()
        .map(|row| {
            let value = chosen
                .get(row.button)
                .cloned()
                .unwrap_or_else(|| default_input(row, kind).unwrap_or_default().to_string());
            (row.button.to_string(), value)
        })
        .collect()
}

/// Controls page events.
#[derive(Debug, Clone)]
pub enum ControlsMessage {
    Device(Device),
    /// A key box was clicked: it waits for an input for this game button.
    Capture(String),
    /// The input (SDL name) the waiting box of this game button got.
    Captured(String, String),
    /// The waiting box gives up (Python `finish`): its time ran out, the window lost the focus,
    /// or a mouse button without an input name was pressed.
    CaptureTimeout,
    /// The arrow of a row: that button back on its default.
    ResetKey(String),
    ResetTable(Kind),
    PadStyle(String),
    /// A gamepad box: the game button and the SDL gamepad button chosen for it.
    PadPick(String, String),
    /// What the probe of the first open found.
    GamepadDetected(bool),
}

pub fn update(state: &mut State, message: ControlsMessage) -> Task<Message> {
    match message {
        ControlsMessage::Device(device) => {
            show_device(state, device);
            state.controls.device_chosen = true;
            apply_set(state, "controls_device", Value::from(device.value()));
        }
        ControlsMessage::Capture(button) => {
            let now = Instant::now();
            state.now = now;
            state.controls.capturing = Some(button);
            state.controls.capture_started = Some(now);
        }
        // An input that arrives for a box no longer waiting binds nothing.
        ControlsMessage::Captured(button, name) => {
            if state.controls.capturing.as_deref() == Some(button.as_str()) {
                bind_key(&mut state.controls.key, &button, &name);
                stop_capture(state);
            }
        }
        ControlsMessage::CaptureTimeout => stop_capture(state),
        ControlsMessage::ResetKey(button) => {
            if let Some(row) = table_rows(Kind::Key)
                .into_iter()
                .find(|row| row.button == button)
            {
                let default = default_input(row, Kind::Key).unwrap_or_default();
                bind_key(&mut state.controls.key, &button, default);
            }
        }
        ControlsMessage::ResetTable(kind) => {
            let values = match kind {
                Kind::Key => &mut state.controls.key,
                Kind::Pad => &mut state.controls.pad,
            };
            for row in table_rows(kind) {
                if let Some(value) = values.get_mut(row.button) {
                    *value = default_input(row, kind).unwrap_or_default().to_string();
                }
            }
        }
        ControlsMessage::PadStyle(style) => {
            state.controls.pad_style = style.clone();
            apply_set(state, "pad_style", Value::String(style));
        }
        ControlsMessage::PadPick(button, name) => {
            if let Some(value) = state.controls.pad.get_mut(&button) {
                *value = name;
            }
        }
        ControlsMessage::GamepadDetected(found) => {
            if found && !state.controls.device_chosen {
                show_device(state, Device::Gamepad);
            }
        }
    }
    Task::none()
}

/// Shows a device's section. The keyboard table's waiting box gives up when it hides, so no
/// click is bound to a box out of sight.
fn show_device(state: &mut State, device: Device) {
    if device != Device::Keyboard {
        stop_capture(state);
    }
    state.controls.device = device;
}

fn stop_capture(state: &mut State) {
    state.controls.capturing = None;
    state.controls.capture_started = None;
}

/// The first open without a chosen device looks for a gamepad, once, in the background.
pub fn opened(state: &mut State) -> Task<Message> {
    if state.controls.device_chosen || state.controls.gamepad_probe_started {
        return Task::none();
    }
    state.controls.gamepad_probe_started = true;
    let paths = state.paths.clone();
    blocking(move || connected_gamepad(&paths))
        .map(|found| Message::Controls(ControlsMessage::GamepadDetected(found.unwrap_or(false))))
}

/// A key pressed while a box waits: its SDL name binds it; a key that cannot be bound leaves the
/// box waiting.
pub fn key_pressed(state: &mut State, key: &Key, location: Location) -> Task<Message> {
    let Some(button) = state.controls.capturing.clone() else {
        return Task::none();
    };
    match neutral_key_name(key, location).and_then(|name| sdl_key_name(&name)) {
        Some(name) => update(state, ControlsMessage::Captured(button, name)),
        None => Task::none(),
    }
}

/// Gives up on the waiting box `CAPTURE_SECONDS` after it started.
pub fn tick(state: &mut State, at: Instant) {
    let expired = state.controls.capture_started.is_some_and(|started| {
        at.saturating_duration_since(started) >= Duration::from_secs(CAPTURE_SECONDS)
    });
    if expired && state.controls.capturing.is_some() {
        let _ = update(state, ControlsMessage::CaptureTimeout);
    }
}

/// While a box waits: mouse buttons and wheel turns, as its input.
pub fn subscription(state: &State) -> Subscription<Message> {
    let Some(button) = state.controls.capturing.clone() else {
        return Subscription::none();
    };
    event::listen_with(
        |event: Event, _status: event::Status, _window: window::Id| mouse_input(&event),
    )
    .with(button)
    .map(|(button, input)| {
        Message::Controls(match input {
            MouseInput::Bind(name) => ControlsMessage::Captured(button, name.to_string()),
            MouseInput::GiveUp => ControlsMessage::CaptureTimeout,
        })
    })
}

/// While a box waits, a layer over the whole window takes the clicks and the wheel (the Python's
/// `grab_set`): they reach the box through `subscription`, and nothing under the layer reacts, so
/// a click on the box itself does not start it waiting again. The idle cursor keeps the hover
/// look of the widgets under it off.
pub fn input_catcher<'a>() -> Element<'a, Message> {
    mouse_area(container(space()).width(Fill).height(Fill))
        .on_press(Message::Noop)
        .on_scroll(|_| Message::Noop)
        .interaction(mouse::Interaction::Idle)
        .into()
}

/// What a mouse event does to the waiting box.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseInput {
    /// The input it binds.
    Bind(&'static str),
    /// A button without an input name: the box gives up (Python `clicked` -> `finish`).
    GiveUp,
}

/// A mouse button press or a wheel turn as the waiting box's input (Python `TK_TO_MOUSE`,
/// `mouse_name`); `None` for any other event.
pub fn mouse_input(event: &Event) -> Option<MouseInput> {
    let name = match event {
        Event::Mouse(mouse::Event::ButtonPressed(button)) => match button {
            mouse::Button::Left => "Mouse Left",
            mouse::Button::Middle => "Mouse Middle",
            mouse::Button::Right => "Mouse Right",
            mouse::Button::Back => "Mouse Back",
            mouse::Button::Forward => "Mouse Forward",
            mouse::Button::Other(_) => return Some(MouseInput::GiveUp),
        },
        Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
            let (mouse::ScrollDelta::Lines { y, .. } | mouse::ScrollDelta::Pixels { y, .. }) =
                delta;
            if *y > 0.0 {
                "Wheel Up"
            } else if *y < 0.0 {
                "Wheel Down"
            } else {
                return None;
            }
        }
        _ => return None,
    };
    Some(MouseInput::Bind(name))
}

const FUNCTION_KEYS: [Named; 24] = [
    Named::F1,
    Named::F2,
    Named::F3,
    Named::F4,
    Named::F5,
    Named::F6,
    Named::F7,
    Named::F8,
    Named::F9,
    Named::F10,
    Named::F11,
    Named::F12,
    Named::F13,
    Named::F14,
    Named::F15,
    Named::F16,
    Named::F17,
    Named::F18,
    Named::F19,
    Named::F20,
    Named::F21,
    Named::F22,
    Named::F23,
    Named::F24,
];

/// The neutral name `sdl_key_name` reads for an iced key (the Python's `TK_TO_SDL_KEYS` inputs),
/// or `None` for a key without one. Characters come as typed, ASCII letters upper-cased.
pub fn neutral_key_name(key: &Key, location: Location) -> Option<String> {
    match key {
        Key::Character(typed) if location == Location::Numpad => keypad_character(typed.as_str()),
        Key::Character(typed) if typed.as_str() == " " => Some("Space".to_string()),
        Key::Character(typed) => Some(typed.as_str().to_ascii_uppercase()),
        Key::Named(named) if location == Location::Numpad => {
            keypad_named(*named).or_else(|| named_key(*named, location))
        }
        Key::Named(named) => named_key(*named, location),
        Key::Unidentified => None,
    }
}

/// A numpad key that types a character (Num Lock on).
fn keypad_character(typed: &str) -> Option<String> {
    match typed {
        "+" | "-" | "*" | "/" => Some(format!("Keypad {typed}")),
        // The decimal key types a comma on some layouts; it is the same key (Tk's KP_Decimal).
        "." | "," => Some("Keypad .".to_string()),
        digit if digit.len() == 1 && digit.bytes().all(|byte| byte.is_ascii_digit()) => {
            Some(format!("Keypad {digit}"))
        }
        _ => None,
    }
}

/// Enter and, with Num Lock off, the other names the numpad reports; SDL sees the keypad keys.
fn keypad_named(named: Named) -> Option<String> {
    let name = match named {
        Named::Enter => "Keypad Enter",
        Named::Insert => "Keypad 0",
        Named::End => "Keypad 1",
        Named::ArrowDown => "Keypad 2",
        Named::PageDown => "Keypad 3",
        Named::ArrowLeft => "Keypad 4",
        Named::Clear => "Keypad 5",
        Named::ArrowRight => "Keypad 6",
        Named::Home => "Keypad 7",
        Named::ArrowUp => "Keypad 8",
        Named::PageUp => "Keypad 9",
        Named::Delete => "Keypad .",
        _ => return None,
    };
    Some(name.to_string())
}

fn named_key(named: Named, location: Location) -> Option<String> {
    let side = |left: &'static str, right: &'static str| {
        if location == Location::Right {
            right
        } else {
            left
        }
    };
    let name = match named {
        Named::Space => "Space",
        Named::Enter => "Return",
        Named::Tab => "Tab",
        Named::Backspace => "Backspace",
        Named::Delete => "Delete",
        Named::Escape => "Escape",
        Named::Home => "Home",
        Named::End => "End",
        Named::PageUp => "PageUp",
        Named::PageDown => "PageDown",
        Named::CapsLock => "CapsLock",
        Named::NumLock => "Numlock",
        Named::ScrollLock => "ScrollLock",
        Named::Pause => "Pause",
        Named::ContextMenu => "Application",
        Named::ArrowUp => "Up",
        Named::ArrowDown => "Down",
        Named::ArrowLeft => "Left",
        Named::ArrowRight => "Right",
        Named::Shift => side("Left Shift", "Right Shift"),
        Named::Control => side("Left Ctrl", "Right Ctrl"),
        Named::Alt => side("Left Alt", "Right Alt"),
        // AltGr is the right Alt key (Tk's Alt_R, SDL's RALT).
        Named::AltGraph => "Right Alt",
        _ => {
            return FUNCTION_KEYS
                .iter()
                .position(|key| *key == named)
                .map(|index| format!("F{}", index + 1))
        }
    };
    Some(name.to_string())
}

/// The English name of a game button or action, which is also the translation key.
fn button_name(button: &'static str) -> &'static str {
    PS_BUTTON_NAMES
        .iter()
        .chain(ACTION_NAMES)
        .find(|(name, _)| *name == button)
        .map_or(button, |(_, english)| *english)
}

/// The game button's icon as `(icon, style)`: its PlayStation button, a pill for the two-button
/// actions (Rage of Sparta, Grab), the right stick labelled RS for Dodge.
fn game_icon(button: &str) -> (&'static str, &'static str) {
    match (game_button(button), button) {
        (Some(sdl), _) => (sdl, "playstation"),
        (None, "rage") => ("label:L3+R3", "playstation"),
        (None, "grab") => ("label:L1+O", "playstation"),
        _ => ("rightstick", "xbox"),
    }
}

/// One row of the keyboard & mouse table as the page draws it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyRow {
    pub button: &'static str,
    pub title: String,
    /// What the key box shows: the input, `(none)`, or the countdown while it waits.
    pub input: String,
    /// Off for L3 and R3 while Rage of Sparta is on one input, and for the Rage row otherwise.
    pub enabled: bool,
}

/// The rows of the keyboard & mouse table; `view` draws them.
pub fn key_rows(state: &State, lang: &str) -> Vec<KeyRow> {
    let combined = state.settings.get_bool("rage_one_input");
    table_rows(Kind::Key)
        .into_iter()
        .map(|row| {
            let enabled = if row.button == "rage" {
                combined
            } else {
                !(combined && RAGE_BUTTONS.contains(&row.button))
            };
            KeyRow {
                button: row.button,
                title: tr(lang, button_name(row.button)),
                input: key_box_text(state, lang, row.button),
                enabled,
            }
        })
        .collect()
}

fn key_box_text(state: &State, lang: &str, button: &str) -> String {
    let view = &state.controls;
    if view.capturing.as_deref() == Some(button) {
        let waited = view.capture_started.map_or(0, |started| {
            state.now.saturating_duration_since(started).as_secs()
        });
        let left = CAPTURE_SECONDS.saturating_sub(waited).max(1);
        return format!("{} ({left})", tr(lang, "Press a key or mouse"));
    }
    match view.key.get(button).map(String::as_str) {
        None | Some("" | "None") => tr(lang, "(none)"),
        Some(input) => input.to_string(),
    }
}

/// What a row of the gamepad table offers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PadChoice {
    /// A drop-down over `(SDL name, label)` pairs; `current` is the label shown.
    Pick {
        pairs: Vec<(String, String)>,
        current: String,
    },
    /// L2, R2 and Touchpad left are not remapped on the gamepad: a fixed text.
    Fixed(String),
}

/// One row of the gamepad table as the page draws it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PadRow {
    pub button: &'static str,
    pub title: String,
    /// The SDL button of the gamepad column's icon, in the chosen controller type.
    pub icon: String,
    pub choice: PadChoice,
}

/// SDL gamepad button and its label in a controller type, in `lang` (Python `pad_labels`).
pub fn pad_labels(style: &str, lang: &str) -> Vec<(String, String)> {
    PAD_STYLES
        .iter()
        .find(|(known, _)| *known == style)
        .map(|(_, labels)| {
            labels
                .iter()
                .map(|(name, label)| (name.to_string(), tr(lang, label)))
                .collect()
        })
        .unwrap_or_default()
}

/// The rows of the gamepad table; `view` draws them.
pub fn pad_rows(state: &State, lang: &str) -> Vec<PadRow> {
    let view = &state.controls;
    let pairs = pad_labels(&view.pad_style, lang);
    CONTROLS
        .iter()
        .map(|row| {
            let chosen = view.pad.get(row.button).cloned().unwrap_or_default();
            // L2/R2 show their trigger, Touchpad left the touchpad.
            let icon = if chosen.is_empty() {
                game_button(row.button).unwrap_or_default().to_string()
            } else {
                chosen.clone()
            };
            let choice = match row.pad {
                Some(_) => PadChoice::Pick {
                    current: pairs
                        .iter()
                        .find(|(name, _)| *name == chosen)
                        .map_or(chosen, |(_, label)| label.clone()),
                    pairs: pairs.clone(),
                },
                None if row.button == "touchpad_left" => {
                    PadChoice::Fixed(tr(lang, "L1 + Touchpad right"))
                }
                None => PadChoice::Fixed(tr_fmt(
                    lang,
                    "{} trigger",
                    &[&tr(lang, button_name(row.button))],
                )),
            };
            PadRow {
                button: row.button,
                title: tr(lang, button_name(row.button)),
                icon,
                choice,
            }
        })
        .collect()
}

/// Choosing `label` in the gamepad box of `button`: its SDL name, or the label itself when no
/// pair has it (Python `pad_name`).
pub fn pad_pick(button: &str, pairs: &[(String, String)], label: &str) -> Message {
    let name = pairs
        .iter()
        .find(|(_, known)| known == label)
        .map_or(label, |(name, _)| name.as_str());
    Message::Controls(ControlsMessage::PadPick(
        button.to_string(),
        name.to_string(),
    ))
}

/// An ini number, or its default when it is not one.
fn ini_number(state: &State, key: &str) -> f32 {
    let parse = |text: &str| text.trim().parse::<f32>().ok();
    state
        .ini
        .values
        .get(key)
        .and_then(|text| parse(text))
        .or_else(|| {
            INI_DEFAULTS
                .iter()
                .find(|(name, _)| *name == key)
                .and_then(|(_, text)| parse(text))
        })
        .unwrap_or_default()
}

/// An ini flag is on when its value is `"1"`.
fn ini_on(state: &State, key: &str) -> bool {
    state.ini.values.get(key).is_some_and(|value| value == "1")
}

fn note<'a>(body: String) -> Element<'a, Message> {
    container(text(body).size(13).style(theme::text_muted))
        .padding(Padding::ZERO.top(4))
        .into()
}

/// A radio of the page drawn as a flat button; the chosen one reads like the active tab.
fn radio<'a>(label: String, active: bool, message: Message) -> Element<'a, Message> {
    button(text(label))
        .style(move |theme, status| {
            let mut style = theme::button_flat(theme, status);
            if active {
                style.text_color = palette::RED_HI;
                style.border.color = palette::RED_HI;
            }
            style
        })
        .on_press(message)
        .into()
}

/// The reset of the `kind` table: off while the table already holds the defaults, so a click,
/// whose rows may be out of sight, still shows that it acted.
fn reset_button<'a>(state: &State, label: String, kind: Kind) -> Element<'a, Message> {
    let values = match kind {
        Kind::Key => &state.controls.key,
        Kind::Pad => &state.controls.pad,
    };
    let reset = (!at_defaults(values, kind))
        .then_some(Message::Controls(ControlsMessage::ResetTable(kind)));
    container(
        button(text(label))
            .style(theme::button_flat)
            .on_press_maybe(reset),
    )
    .padding(Padding::ZERO.top(14))
    .into()
}

fn key_line<'a>(row: KeyRow) -> Element<'a, Message> {
    let (icon, style) = game_icon(row.button);
    let mut input = button(text(row.input))
        .width(BOX_WIDTH)
        .style(theme::button_flat);
    let mut reset = button(text("\u{21a9}")).style(theme::button_flat);
    if row.enabled {
        let button = row.button.to_string();
        input = input.on_press(Message::Controls(ControlsMessage::Capture(button.clone())));
        reset = reset.on_press(Message::Controls(ControlsMessage::ResetKey(button)));
    }
    layout![
        button_icon(icon, style, ICON_SIZE),
        text(row.title).width(Fill),
        input,
        reset,
    ]
    .spacing(10)
    .align_y(Center)
    .into()
}

fn pad_line<'a>(row: PadRow, pad_style: &str) -> Element<'a, Message> {
    let (icon, style) = game_icon(row.button);
    let choice: Element<'a, Message> = match row.choice {
        PadChoice::Pick { pairs, current } => {
            let labels: Vec<String> = pairs.iter().map(|(_, label)| label.clone()).collect();
            let button = row.button;
            min_width(
                SELECT_MIN,
                pick_list(labels, Some(current), move |label: String| {
                    pad_pick(button, &pairs, &label)
                })
                .text_size(SELECT_TEXT)
                .style(theme::pick_list_dark),
            )
            .into()
        }
        PadChoice::Fixed(fixed) => text(fixed).style(theme::text_muted).into(),
    };
    layout![
        button_icon(icon, style, ICON_SIZE),
        text(row.title).width(Fill),
        button_icon(&row.icon, pad_style, ICON_SIZE),
        choice,
    ]
    .spacing(10)
    .align_y(Center)
    .into()
}

fn keyboard_section<'a>(state: &State, lang: &str) -> Element<'a, Message> {
    let hint = |english: &str| tr(lang, english);
    let rage_on = state.settings.get_bool("rage_one_input");
    let mut lines = column![].spacing(6);
    for key_row in key_rows(state, lang) {
        if key_row.button == "rage" {
            lines = lines.push(checkbox(
                state.rtl,
                "rage_one_input",
                &tr(lang, RAGE_CHECK),
                rage_on,
                None,
            ));
        }
        lines = lines.push(key_line(key_row));
    }
    let aim_style = state
        .ini
        .values
        .get("mouse_aim_style")
        .cloned()
        .unwrap_or_default();
    column![
        section(state.rtl, &tr(lang, "Keyboard & Mouse Mapping")),
        note(tr(
            lang,
            "Which key or mouse input presses each button of the game."
        )),
        container(lines).padding(Padding::ZERO.top(12)),
        note(tr_fmt(lang, KEY_NOTE, &[&CAPTURE_SECONDS.to_string()])),
        reset_button(
            state,
            tr(lang, "Reset keyboard & mouse to defaults"),
            Kind::Key
        ),
        row(
            state.rtl,
            &tr(lang, "Joystick prompts"),
            select(
                "stick_prompts",
                STICK_PROMPTS,
                &state.settings.get_str("stick_prompts"),
                lang
            ),
            Some(&hint(STICK_HINT)),
        ),
        row(
            state.rtl,
            &tr(lang, "Mouse prompt distance"),
            slider(
                "mouse_prompt_distance",
                16.0,
                200.0,
                "px",
                ini_number(state, "mouse_prompt_distance")
            ),
            Some(&hint(DISTANCE_HINT)),
        ),
        row(
            state.rtl,
            &tr(lang, "Right stick style"),
            select("mouse_aim_style", AIM_STYLES, &aim_style, lang),
            Some(&hint(AIM_STYLE_HINT)),
        ),
        row(
            state.rtl,
            &tr(lang, "Right stick sensitivity"),
            slider(
                "mouse_aim_sensitivity",
                10.0,
                200.0,
                "%",
                ini_number(state, "mouse_aim_sensitivity")
            ),
            Some(&hint(SENSITIVITY_HINT)),
        ),
        checkbox_ini(
            state.rtl,
            "mouse_lock",
            &tr(lang, "Keep the mouse in the game window"),
            ini_on(state, "mouse_lock"),
            Some(&hint(MOUSE_LOCK_HINT)),
        ),
    ]
    .spacing(2)
    .into()
}

fn gamepad_section<'a>(state: &State, lang: &str) -> Element<'a, Message> {
    let styles = PAD_STYLE_NAMES
        .iter()
        .fold(layout![].spacing(14), |radios, (style, name)| {
            radios.push(radio(
                name.to_string(),
                state.controls.pad_style == *style,
                Message::Controls(ControlsMessage::PadStyle(style.to_string())),
            ))
        });
    let lines = pad_rows(state, lang)
        .into_iter()
        .fold(column![].spacing(6), |lines, pad_row| {
            lines.push(pad_line(pad_row, &state.controls.pad_style))
        });
    column![
        section(state.rtl, &tr(lang, "Gamepad Mapping")),
        note(tr(
            lang,
            "Which gamepad button presses each button of the game."
        )),
        row(state.rtl, &tr(lang, "Controller type"), styles.into(), None),
        row(
            state.rtl,
            &tr(lang, "Vibration"),
            slider("vibration", 0.0, 100.0, "%", ini_number(state, "vibration")),
            Some(&tr(lang, VIBRATION_HINT)),
        ),
        container(lines).padding(Padding::ZERO.top(12)),
        note(tr(lang, PAD_NOTE)),
        reset_button(state, tr(lang, "Reset gamepad to defaults"), Kind::Pad),
    ]
    .spacing(2)
    .into()
}

/// The in-game icons, the device, then only the chosen device's section (Python `show_device`).
pub fn view(state: &State) -> Element<'_, Message> {
    let lang = state.lang.as_str();
    let devices = Device::ALL
        .into_iter()
        .fold(layout![].spacing(18), |radios, device| {
            radios.push(radio(
                tr(lang, device.label_en()),
                state.controls.device == device,
                Message::Controls(ControlsMessage::Device(device)),
            ))
        });
    let device_section = match state.controls.device {
        Device::Keyboard => keyboard_section(state, lang),
        Device::Gamepad => gamepad_section(state, lang),
    };
    column![
        row(
            state.rtl,
            &tr(lang, "In-game button icons"),
            select(
                "glyph_style",
                GLYPH_STYLES,
                &state.settings.get_str("glyph_style"),
                lang
            ),
            Some(&tr(lang, GLYPH_HINT)),
        ),
        row(state.rtl, &tr(lang, "Device"), devices.into(), None),
        device_section,
    ]
    .spacing(2)
    .into()
}
