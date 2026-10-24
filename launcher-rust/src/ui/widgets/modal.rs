//! Info, error and yes/no dialogs over a dim layer.

use iced::widget::{button, column, container, mouse_area, row, text};
use iced::{Background, Center, Color, Element, Fill, Right};

use crate::core::i18n::tr;
use crate::ui::app::Message;
use crate::ui::theme::{self, palette, CINZEL_BOLD};

/// What a dialog asks.
// The Yes/No answers carry a `Message`, which now holds a `Release`: boxing every answer costs
// more than the bytes.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone)]
pub enum ModalKind {
    Info,
    Error,
    YesNo { yes: Message, no: Message },
}

/// An open dialog.
#[derive(Debug, Clone)]
pub struct Modal {
    pub kind: ModalKind,
    pub title: String,
    pub text: String,
}

/// Dialog events.
#[derive(Debug, Clone)]
pub enum ModalMessage {
    Dismiss,
}

/// Width of the card.
const CARD_WIDTH: f32 = 440.0;

pub fn modal<'a>(kind: ModalKind, title: &str, body: &str, lang: &str) -> Element<'a, Message> {
    let title_color = match kind {
        ModalKind::Error => palette::RED_HI,
        _ => palette::TEXT,
    };
    let ok = || {
        button(text(tr(lang, "OK")))
            .style(theme::button_flat)
            .on_press(Message::Modal(ModalMessage::Dismiss))
    };
    let buttons = match kind {
        ModalKind::Info | ModalKind::Error => row![ok()],
        ModalKind::YesNo { yes, no } => row![
            button(text(tr(lang, "Yes")))
                .style(theme::button_play)
                .on_press(yes),
            button(text(tr(lang, "No")))
                .style(theme::button_flat)
                .on_press(no),
        ],
    }
    .spacing(8);
    let card = container(
        column![
            text(title.to_string())
                .font(CINZEL_BOLD)
                .size(18)
                .color(title_color),
            text(body.to_string()),
            container(buttons).width(Fill).align_x(Right),
        ]
        .spacing(14),
    )
    .padding(20)
    .width(CARD_WIDTH)
    .style(theme::container_card);
    let layer = container(card)
        .width(Fill)
        .height(Fill)
        .align_x(Center)
        .align_y(Center)
        .style(|_| container::Style {
            background: Some(Background::Color(Color {
                a: 0.6,
                ..palette::BG
            })),
            ..container::Style::default()
        });
    // Swallows clicks so nothing under the dialog reacts.
    mouse_area(layer).on_press(Message::Noop).into()
}
