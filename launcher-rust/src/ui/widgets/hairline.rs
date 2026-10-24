//! The 1 px red line of the borders (ruling R88): the tab bar's top edge, the drawer's inner edge
//! and the line under the Log page's buttons.

use iced::widget::{container, space, Container};
use iced::{Background, Fill, Theme};

use crate::ui::app::Message;
use crate::ui::theme::palette;

/// The line's thickness in logical pixels.
pub const HAIRLINE: f32 = 1.0;

/// The line's colour: `palette::RED`.
pub fn hairline_style(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(palette::RED)),
        ..container::Style::default()
    }
}

/// A horizontal line across its parent, found by `id`; `.width(HAIRLINE).height(Fill)` stands it
/// up.
pub fn hairline<'a>(id: &'static str) -> Container<'a, Message> {
    container(space())
        .id(id)
        .width(Fill)
        .height(HAIRLINE)
        .style(hairline_style)
}
