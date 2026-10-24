//! A small status pill.

use iced::widget::{container, text};
use iced::{Border, Element};

use crate::ui::app::Message;
use crate::ui::theme::{faded, palette};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChipKind {
    Ok,
    Warn,
    Plain,
}

/// A chip whose text and edge show at `opacity` (1 normally; the Home's 35 % behind a wide
/// drawer).
pub fn chip<'a>(label: &str, kind: ChipKind, opacity: f32) -> Element<'a, Message> {
    let edge = match kind {
        ChipKind::Ok => palette::GOLD,
        ChipKind::Warn => palette::RED_HI,
        ChipKind::Plain => palette::LINE,
    };
    container(
        text(label.to_string())
            .size(12)
            .color(faded(palette::SILVER, opacity)),
    )
    .padding([2, 8])
    .style(move |_| container::Style {
        border: Border {
            color: faded(edge, opacity),
            width: 1.0,
            radius: 10.0.into(),
        },
        ..container::Style::default()
    })
    .into()
}
