//! Palette, fonts, widget styles, the zoom rule and the right-to-left mirror of the launcher.

use iced::alignment::Horizontal;
use iced::font::{Family, Weight};
use iced::widget::{button, checkbox, container, pick_list, text, text_input};
use iced::{Background, Border, Color, Font, Padding, Theme};

use crate::core::options::ui_scale_choice;

/// Colours of the launcher (spec section 3.3).
pub mod palette {
    use iced::Color;

    pub const BG: Color = Color::from_rgb8(0x0d, 0x0b, 0x0b);
    pub const PANEL: Color = Color::from_rgb8(0x16, 0x12, 0x12);
    pub const CARD: Color = Color::from_rgb8(0x1d, 0x17, 0x17);
    pub const LINE: Color = Color::from_rgb8(0x2a, 0x22, 0x22);
    pub const TEXT: Color = Color::from_rgb8(0xe6, 0xe0, 0xd8);
    pub const MUTED: Color = Color::from_rgb8(0x8a, 0x81, 0x7a);
    pub const RED: Color = Color::from_rgb8(0xa4, 0x16, 0x1a);
    pub const RED_HI: Color = Color::from_rgb8(0xc8, 0x1e, 0x25);
    pub const SILVER: Color = Color::from_rgb8(0xc9, 0xc4, 0xbd);
    pub const GOLD: Color = Color::from_rgb8(0xb8, 0x86, 0x2b);
}

use palette::{BG, CARD, GOLD, LINE, MUTED, PANEL, RED, RED_HI, TEXT};

/// Mockup `.input`: its background, its edge, and the text of an empty field (`#5a5250`).
const INPUT_BG: Color = Color::from_rgb8(0x12, 0x0f, 0x0f);
const INPUT_LINE: Color = Color::from_rgb8(0x2e, 0x26, 0x26);
const INPUT_PLACEHOLDER: Color = Color::from_rgb8(0x5a, 0x52, 0x50);
/// A hovered field's edge (mockup `#langbtn:hover`).
const LINE_HOVER: Color = Color::from_rgb8(0x4a, 0x3a, 0x3a);

/// Page titles and the PLAY button.
pub const CINZEL_BLACK: Font = Font {
    family: Family::Name("Cinzel"),
    weight: Weight::Black,
    ..Font::DEFAULT
};
/// Tabs and section headings.
pub const CINZEL_BOLD: Font = Font {
    family: Family::Name("Cinzel"),
    weight: Weight::Bold,
    ..Font::DEFAULT
};
/// Where the 2.6.0 launcher already uses Bodoni Moda.
pub const BODONI: Font = Font {
    family: Family::Name("Bodoni Moda"),
    ..Font::DEFAULT
};
/// Everything else: the system sans.
pub const UI_FONT: Font = Font::DEFAULT;
pub const MONO_FONT: Font = Font::MONOSPACE;

/// The embedded faces, to hand to `iced::application(...).font(...)`.
pub fn font_bytes() -> [&'static [u8]; 3] {
    [
        include_bytes!("../../assets/Cinzel-Black.ttf"),
        include_bytes!("../../assets/Cinzel-Bold.ttf"),
        include_bytes!("../../assets/BodoniModa.ttf"),
    ]
}

pub fn theme() -> Theme {
    Theme::custom(
        "SpartanPort".to_string(),
        iced::theme::Palette {
            background: BG,
            text: TEXT,
            primary: RED,
            success: GOLD,
            warning: GOLD,
            danger: RED_HI,
        },
    )
}

fn border(color: Color) -> Border {
    Border {
        color,
        width: 1.0,
        radius: 4.0.into(),
    }
}

pub fn button_play(_theme: &Theme, status: button::Status) -> button::Style {
    let fill = match status {
        button::Status::Hovered | button::Status::Pressed => RED_HI,
        _ => RED,
    };
    let text_color = match status {
        button::Status::Disabled => MUTED,
        _ => TEXT,
    };
    button::Style {
        background: Some(Background::Color(fill)),
        text_color,
        border: Border {
            radius: 4.0.into(),
            ..Border::default()
        },
        ..button::Style::default()
    }
}

/// `color` at `opacity` of its own alpha (the Home block behind a wide drawer).
pub fn faded(color: Color, opacity: f32) -> Color {
    color.scale_alpha(opacity)
}

/// `style` with its fill, text and border at `opacity` of their own alpha.
pub fn faded_button(style: button::Style, opacity: f32) -> button::Style {
    button::Style {
        background: style
            .background
            .map(|background| background.scale_alpha(opacity)),
        text_color: faded(style.text_color, opacity),
        border: Border {
            color: faded(style.border.color, opacity),
            ..style.border
        },
        ..style
    }
}

pub fn button_flat(_theme: &Theme, status: button::Status) -> button::Style {
    let edge = match status {
        button::Status::Hovered | button::Status::Pressed => RED_HI,
        _ => LINE,
    };
    button::Style {
        background: Some(Background::Color(CARD)),
        text_color: if matches!(status, button::Status::Disabled) {
            MUTED
        } else {
            TEXT
        },
        border: border(edge),
        ..button::Style::default()
    }
}

/// A page tab; the underline of the active one is drawn by the tab bar.
pub fn tab(active: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_theme, status| {
        let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
        button::Style {
            background: active.then_some(Background::Color(CARD)),
            text_color: if active || hovered { RED_HI } else { MUTED },
            border: Border::default(),
            ..button::Style::default()
        }
    }
}

pub fn container_panel(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(PANEL)),
        text_color: Some(TEXT),
        ..container::Style::default()
    }
}

pub fn container_card(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(CARD)),
        text_color: Some(TEXT),
        border: border(LINE),
        ..container::Style::default()
    }
}

pub fn checkbox_red(_theme: &Theme, status: checkbox::Status) -> checkbox::Style {
    let (checked, hovered) = match status {
        checkbox::Status::Active { is_checked } | checkbox::Status::Disabled { is_checked } => {
            (is_checked, false)
        }
        checkbox::Status::Hovered { is_checked } => (is_checked, true),
    };
    let edge = if checked || hovered { RED_HI } else { LINE };
    checkbox::Style {
        background: Background::Color(if checked { RED } else { CARD }),
        icon_color: TEXT,
        border: border(edge),
        text_color: Some(TEXT),
    }
}

pub fn pick_list_dark(_theme: &Theme, status: pick_list::Status) -> pick_list::Style {
    let edge = match status {
        pick_list::Status::Active => LINE,
        _ => RED_HI,
    };
    pick_list::Style {
        text_color: TEXT,
        placeholder_color: MUTED,
        handle_color: MUTED,
        background: Background::Color(CARD),
        border: border(edge),
    }
}

/// Mockup `.input`: the folder fields.
pub fn text_input_dark(_theme: &Theme, status: text_input::Status) -> text_input::Style {
    let edge = match status {
        text_input::Status::Focused { .. } => RED_HI,
        text_input::Status::Hovered => LINE_HOVER,
        _ => INPUT_LINE,
    };
    text_input::Style {
        background: Background::Color(INPUT_BG),
        border: Border {
            color: edge,
            width: 1.0,
            radius: 3.0.into(),
        },
        icon: MUTED,
        placeholder: INPUT_PLACEHOLDER,
        value: palette::SILVER,
        selection: RED,
    }
}

pub fn text_muted(_theme: &Theme) -> text::Style {
    text::Style { color: Some(MUTED) }
}

/// Zoom factor of the stored GUI Scale, in screen pixels per logical pixel whatever the desktop's
/// scale (R90): 1x or 1.5x (`options::ui_scale_choice` maps an older file's values).
pub fn ui_scale(mode: &str) -> f32 {
    match ui_scale_choice(mode) {
        "1.5" => 1.5,
        _ => 1.0,
    }
}

/// The scale factor iced applies on top of the desktop's own (which it already draws at), so the
/// interface shows at `scale` screen pixels per logical pixel.
pub fn window_zoom(scale: f32, desktop_scale: f32) -> f32 {
    if desktop_scale > 0.0 {
        scale / desktop_scale
    } else {
        scale
    }
}

/// `side` as a right-to-left language shows it: left and right swap.
pub fn mirror_x(side: Horizontal, rtl: bool) -> Horizontal {
    match side {
        Horizontal::Left if rtl => Horizontal::Right,
        Horizontal::Right if rtl => Horizontal::Left,
        side => side,
    }
}

/// `padding` as a right-to-left language shows it: the left and right sides swap.
pub fn mirror_padding(padding: Padding, rtl: bool) -> Padding {
    if rtl {
        Padding {
            left: padding.right,
            right: padding.left,
            ..padding
        }
    } else {
        padding
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn hex(c: iced::Color) -> String {
        let [r, g, b, _] = c.into_rgba8();
        format!("#{r:02x}{g:02x}{b:02x}")
    }

    #[test]
    fn palette_matches_the_spec() {
        let expected = [
            (palette::BG, "#0d0b0b"),
            (palette::PANEL, "#161212"),
            (palette::CARD, "#1d1717"),
            (palette::LINE, "#2a2222"),
            (palette::TEXT, "#e6e0d8"),
            (palette::MUTED, "#8a817a"),
            (palette::RED, "#a4161a"),
            (palette::RED_HI, "#c81e25"),
            (palette::SILVER, "#c9c4bd"),
            (palette::GOLD, "#b8862b"),
        ];
        for (color, spec) in expected {
            assert_eq!(hex(color), spec);
        }
    }

    #[test]
    fn a_faded_button_keeps_its_colours_at_the_opacity() {
        let style = faded_button(button_flat(&theme(), button::Status::Active), 0.35);
        assert_eq!(
            style.background,
            Some(Background::Color(CARD.scale_alpha(0.35)))
        );
        assert_eq!(style.text_color, TEXT.scale_alpha(0.35));
        assert_eq!(style.border.color, LINE.scale_alpha(0.35));
        assert_eq!(faded(MUTED, 0.35), MUTED.scale_alpha(0.35));
    }

    #[test]
    fn ui_scale_rules() {
        // R90: GUI Scale is 1x or 1.5x, absolute on any desktop (`window_zoom` leaves the
        // desktop's own scale out). An older file's "auto" loads as 1x and its "2" as 1.5x.
        assert_eq!(ui_scale("1"), 1.0);
        assert_eq!(ui_scale("1.5"), 1.5);
        assert_eq!(ui_scale("auto"), 1.0);
        assert_eq!(ui_scale("2"), 1.5);
        assert_eq!(ui_scale("bogus"), 1.0);
        assert_eq!(ui_scale(""), 1.0);
    }

    #[test]
    fn fonts_are_embedded() {
        for bytes in font_bytes() {
            assert!(!bytes.is_empty());
            assert!(bytes.starts_with(&[0, 1, 0, 0]) || bytes.starts_with(b"true"));
        }
    }
}
