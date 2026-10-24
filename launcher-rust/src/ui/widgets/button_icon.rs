//! Controller button icons drawn on a canvas (port of `launcher/button_icons.py`).
//!
//! Buttons are named as SDL gamepad buttons (`a`, `leftshoulder`, `dpup`, ...); the style
//! chooses PlayStation, Xbox or Nintendo symbols, colours and labels.

use iced::alignment::Vertical;
use iced::border::Radius;
use iced::mouse::Cursor;
use iced::widget::canvas::{self, Canvas, Frame, Geometry, Path, Stroke, Text};
use iced::widget::text::Alignment;
use iced::{Color, Element, Pixels, Point, Rectangle, Renderer, Size, Theme};

use crate::ui::app::Message;
use crate::ui::theme::{palette, UI_FONT};

const fn hex(r: u8, g: u8, b: u8) -> Color {
    Color::from_rgb8(r, g, b)
}

/// Symbols (PlayStation) or letters (Xbox, Nintendo) of the face buttons with their colours,
/// as `(style, button, text, colour)`.
const FACE: &[(&str, &str, &str, Color)] = &[
    ("playstation", "a", "cross", hex(0x6b, 0x9a, 0xe8)),
    ("playstation", "b", "circle", hex(0xe5, 0x55, 0x5e)),
    ("playstation", "x", "square", hex(0xd6, 0x83, 0xd6)),
    ("playstation", "y", "triangle", hex(0x45, 0xc4, 0xa2)),
    ("xbox", "a", "A", hex(0x3f, 0xae, 0x49)),
    ("xbox", "b", "B", hex(0xd9, 0x37, 0x2f)),
    ("xbox", "x", "X", hex(0x2f, 0x6f, 0xd9)),
    ("xbox", "y", "Y", hex(0xe5, 0xb5, 0x21)),
    ("nintendo", "a", "B", hex(0xcf, 0xcf, 0xcf)),
    ("nintendo", "b", "A", hex(0xcf, 0xcf, 0xcf)),
    ("nintendo", "x", "Y", hex(0xcf, 0xcf, 0xcf)),
    ("nintendo", "y", "X", hex(0xcf, 0xcf, 0xcf)),
];

/// Labels of the other buttons, by style.
const LABELS: &[(&str, &[(&str, &str)])] = &[
    (
        "playstation",
        &[
            ("leftshoulder", "L1"),
            ("rightshoulder", "R1"),
            ("lefttrigger", "L2"),
            ("righttrigger", "R2"),
            ("leftstick", "L3"),
            ("rightstick", "R3"),
            ("start", "OPTIONS"),
            ("back", "SHARE"),
            ("touchpad", "TOUCH"),
            ("guide", "PS"),
            ("misc1", "MUTE"),
        ],
    ),
    (
        "xbox",
        &[
            ("leftshoulder", "LB"),
            ("rightshoulder", "RB"),
            ("lefttrigger", "LT"),
            ("righttrigger", "RT"),
            ("leftstick", "LS"),
            ("rightstick", "RS"),
            ("start", "MENU"),
            ("back", "VIEW"),
            ("touchpad", "TOUCH"),
            ("guide", "XBOX"),
            ("misc1", "SHARE"),
        ],
    ),
    (
        "nintendo",
        &[
            ("leftshoulder", "L"),
            ("rightshoulder", "R"),
            ("lefttrigger", "ZL"),
            ("righttrigger", "ZR"),
            ("leftstick", "LS"),
            ("rightstick", "RS"),
            ("start", "+"),
            ("back", "−"),
            ("touchpad", "TOUCH"),
            ("guide", "HOME"),
            ("misc1", "CAPT"),
        ],
    ),
];

const DPAD: &[(&str, &str)] = &[
    ("dpup", "up"),
    ("dpdown", "down"),
    ("dpleft", "left"),
    ("dpright", "right"),
];

/// The game's PS4 buttons (launcher `CONTROLS`) as SDL buttons, for their PlayStation icon.
const GAME_BUTTONS: &[(&str, &str)] = &[
    ("cross", "a"),
    ("circle", "b"),
    ("square", "x"),
    ("triangle", "y"),
    ("l1", "leftshoulder"),
    ("r1", "rightshoulder"),
    ("l2", "lefttrigger"),
    ("r2", "righttrigger"),
    ("l3", "leftstick"),
    ("r3", "rightstick"),
    ("options", "start"),
    ("touchpad_left", "touchpad"),
    ("touchpad_right", "touchpad"),
    ("up", "dpup"),
    ("down", "dpdown"),
    ("left", "dpleft"),
    ("right", "dpright"),
];

/// Ink of labels and the lit d-pad arm: the launcher's text colour.
const INK: Color = palette::TEXT;
const DIM: Color = hex(0x5a, 0x52, 0x50);
const BODY: Color = hex(0x26, 0x20, 0x20);
const LETTER_INK: Color = hex(0x10, 0x10, 0x10);

/// SDL button of a game (PS4) button.
pub fn game_button(button: &str) -> Option<&'static str> {
    GAME_BUTTONS
        .iter()
        .find(|(game, _)| *game == button)
        .map(|(_, sdl)| *sdl)
}

/// How an icon is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconKind {
    /// PlayStation face symbol (`text` names the shape).
    Symbol,
    /// Xbox / Nintendo face letter.
    Letter,
    DPad,
    Bumper,
    Trigger,
    Stick,
    Pill,
}

/// What to draw for a button in a style.
#[derive(Debug, Clone, PartialEq)]
pub struct IconSpec {
    pub kind: IconKind,
    pub text: String,
    pub color: Color,
}

fn found(kind: IconKind, text: &str, color: Color) -> Option<IconSpec> {
    Some(IconSpec {
        kind,
        text: text.to_string(),
        color,
    })
}

/// The icon of an SDL button in a style, or `None` for an unknown button.
/// `label:<text>` is a small pill with that text (actions such as L3+R3).
pub fn spec(button: &str, style: &str) -> Option<IconSpec> {
    if let Some(text) = button.strip_prefix("label:") {
        return found(IconKind::Pill, text, INK);
    }
    if let Some((_, _, text, color)) = FACE.iter().find(|(s, b, _, _)| *s == style && *b == button)
    {
        let kind = if style == "playstation" {
            IconKind::Symbol
        } else {
            IconKind::Letter
        };
        return found(kind, text, *color);
    }
    if let Some((_, arm)) = DPAD.iter().find(|(b, _)| *b == button) {
        return found(IconKind::DPad, arm, INK);
    }
    let label = LABELS
        .iter()
        .find(|(s, _)| *s == style)
        .and_then(|(_, labels)| labels.iter().find(|(b, _)| *b == button))
        .map(|(_, label)| *label)
        .filter(|label| !label.is_empty())?;
    let kind = if button.ends_with("shoulder") {
        IconKind::Bumper
    } else if button.ends_with("trigger") {
        IconKind::Trigger
    } else if button.ends_with("stick") {
        IconKind::Stick
    } else {
        IconKind::Pill
    };
    found(kind, label, INK)
}

/// The icon of `button` in a `size` × `size` square.
pub fn button_icon<'a>(button: &str, style: &str, size: f32) -> Element<'a, Message> {
    Canvas::new(IconProgram {
        spec: spec(button, style),
    })
    .width(size)
    .height(size)
    .into()
}

struct IconProgram {
    spec: Option<IconSpec>,
}

impl canvas::Program<Message> for IconProgram {
    type State = ();

    fn draw(
        &self,
        _state: &(),
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        if let Some(spec) = &self.spec {
            paint(&mut frame, spec, bounds.width.min(bounds.height));
        }
        vec![frame.into_geometry()]
    }
}

fn label(frame: &mut Frame, content: &str, at: Point, size: f32, color: Color) {
    frame.fill_text(Text {
        content: content.to_string(),
        position: at,
        color,
        size: Pixels(size),
        font: UI_FONT,
        align_x: Alignment::Center,
        align_y: Vertical::Center,
        ..Text::default()
    });
}

/// Corner radii of a bumper, trigger or pill bar: half the height all round, except triggers,
/// which are rounded (`0.2 s`) at the top only and square at the bottom.
fn bar_radius(kind: IconKind, s: f32, top: f32, bottom: f32) -> Radius {
    if kind == IconKind::Trigger {
        let r = s * 0.2;
        Radius {
            top_left: r,
            top_right: r,
            bottom_right: 0.0,
            bottom_left: 0.0,
        }
    } else {
        ((bottom - top) / 2.0).into()
    }
}

fn paint(frame: &mut Frame, spec: &IconSpec, s: f32) {
    let m = s / 2.0;
    let center = Point::new(m, m);
    let color = spec.color;
    match spec.kind {
        IconKind::Symbol | IconKind::Letter => {
            let disc = Path::circle(center, m - 1.0);
            let letter = spec.kind == IconKind::Letter;
            frame.fill(&disc, if letter { color } else { BODY });
            if letter {
                label(
                    frame,
                    &spec.text,
                    center,
                    (s * 0.42).floor().max(7.0),
                    LETTER_INK,
                );
                return;
            }
            let stroke = Stroke::default()
                .with_color(color)
                .with_width((s / 11.0).floor().max(2.0));
            let r = s * 0.24;
            let shape = match spec.text.as_str() {
                "cross" => Path::new(|b| {
                    b.move_to(Point::new(m - r, m - r));
                    b.line_to(Point::new(m + r, m + r));
                    b.move_to(Point::new(m - r, m + r));
                    b.line_to(Point::new(m + r, m - r));
                }),
                "circle" => Path::circle(center, r),
                "square" => Path::rectangle(Point::new(m - r, m - r), Size::new(2.0 * r, 2.0 * r)),
                _ => Path::new(|b| {
                    b.move_to(Point::new(m, m - r * 1.15));
                    b.line_to(Point::new(m + r * 1.1, m + r * 0.8));
                    b.line_to(Point::new(m - r * 1.1, m + r * 0.8));
                    b.close();
                }),
            };
            frame.stroke(&shape, stroke);
        }
        IconKind::DPad => {
            let (t, a) = (s * 0.3, s * 0.08);
            let arms = [
                ("up", Point::new(m - t / 2.0, a), Size::new(t, m - a)),
                ("down", Point::new(m - t / 2.0, m), Size::new(t, s - a - m)),
                ("left", Point::new(a, m - t / 2.0), Size::new(m - a, t)),
                ("right", Point::new(m, m - t / 2.0), Size::new(s - a - m, t)),
            ];
            for (name, top_left, size) in arms {
                let lit = name == spec.text;
                frame.fill(
                    &Path::rectangle(top_left, size),
                    if lit { INK } else { DIM },
                );
            }
        }
        IconKind::Stick => {
            let disc = Path::circle(center, m - 1.0);
            frame.fill(&disc, BODY);
            frame.stroke(
                &disc,
                Stroke::default()
                    .with_color(DIM)
                    .with_width((s / 14.0).floor().max(1.0)),
            );
            label(
                frame,
                &spec.text,
                center,
                (s * 0.28).floor().max(6.0),
                color,
            );
        }
        IconKind::Bumper | IconKind::Trigger | IconKind::Pill => {
            // Bumpers: a flat bar; triggers: taller with a rounded top; the rest: a small pill.
            let (top, bottom) = match spec.kind {
                IconKind::Bumper => (s * 0.28, s * 0.72),
                IconKind::Trigger => (s * 0.12, s * 0.88),
                _ => (s * 0.3, s * 0.7),
            };
            let radius = bar_radius(spec.kind, s, top, bottom);
            let bar = Path::rounded_rectangle(
                Point::new(1.0, top),
                Size::new(s - 2.0, bottom - top),
                radius,
            );
            frame.fill(&bar, BODY);
            let ratio = if spec.text.chars().count() > 3 {
                0.2
            } else {
                0.3
            };
            label(
                frame,
                &spec.text,
                Point::new(m, (top + bottom) / 2.0),
                (s * ratio).floor().max(5.0),
                color,
            );
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use crate::core::controls::{CONTROLS, PAD_STYLES};

    fn kind_text(button: &str, style: &str) -> Option<(IconKind, String)> {
        spec(button, style).map(|found| (found.kind, found.text))
    }

    #[test]
    fn every_game_button_has_a_playstation_icon() {
        for row in &CONTROLS {
            let sdl = game_button(row.button).expect(row.button);
            assert!(spec(sdl, "playstation").is_some(), "{}", row.button);
        }
    }

    #[test]
    fn every_gamepad_button_has_an_icon_in_both_styles() {
        for (style, labels) in PAD_STYLES {
            for (sdl, _) in *labels {
                assert!(spec(sdl, style).is_some(), "{style} {sdl}");
            }
        }
        for row in &CONTROLS {
            if let Some(pad) = row.pad {
                for (style, _) in PAD_STYLES {
                    assert!(spec(pad, style).is_some(), "{style} {pad}");
                }
            }
        }
    }

    #[test]
    fn triggers_are_rounded_on_top_and_square_at_the_bottom() {
        let trigger = bar_radius(IconKind::Trigger, 28.0, 3.36, 24.64);
        assert_eq!(trigger.top_left, 5.6);
        assert_eq!(trigger.top_right, 5.6);
        assert_eq!(trigger.bottom_left, 0.0);
        assert_eq!(trigger.bottom_right, 0.0);
        let bumper = bar_radius(IconKind::Bumper, 28.0, 7.84, 20.16);
        assert_eq!(bumper.bottom_left, bumper.top_left);
        assert!(bumper.bottom_left > 0.0);
    }

    #[test]
    fn styles_differ_where_the_controllers_do() {
        let is = |button, style, kind, text: &str| {
            assert_eq!(kind_text(button, style), Some((kind, text.to_string())));
        };
        is("a", "playstation", IconKind::Symbol, "cross");
        is("a", "xbox", IconKind::Letter, "A");
        is("leftshoulder", "xbox", IconKind::Bumper, "LB");
        is("righttrigger", "playstation", IconKind::Trigger, "R2");
        is("dpleft", "xbox", IconKind::DPad, "left");
        assert!(spec("nonsense", "xbox").is_none());
    }
}
