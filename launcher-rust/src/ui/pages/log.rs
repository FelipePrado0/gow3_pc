//! The Log page: the game's output over the drawer background, with Copy, Clear and a shortcut
//! to `last_run.log`.

use iced::widget::operation::{snap_to, RelativeOffset};
use iced::widget::scrollable::{Direction, Scrollbar};
use iced::widget::{button, column, row, scrollable, text, Id};
use iced::{Center, Element, Task};

use crate::core::i18n::{tr, tr_fmt};
use crate::core::log::MAX_LOG_LINES;
use crate::ui::app::{Message, State};
use crate::ui::theme::{self, MONO_FONT};
use crate::ui::widgets::hairline;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogMessage {
    Copy,
    Clear,
    OpenLastRun,
}

/// The id of the red line between the buttons and the log (mockup `#logbox` border-top).
pub const SEPARATOR_ID: &str = "log-separator";

/// The id the log body is scrolled by.
pub fn scroll_id() -> Id {
    Id::new("log-body")
}

/// Scrolls the log to its last line.
pub fn scroll_to_end() -> Task<Message> {
    // Down only: a long line keeps its horizontal position.
    snap_to(
        scroll_id(),
        RelativeOffset {
            x: None,
            y: Some(1.0),
        },
    )
}

/// Opens `last_run.log` of the current user folder.
pub fn open_last_run_message(state: &State) -> Message {
    Message::OpenPath(
        state
            .paths
            .run_log_path(&state.settings.get_str("user_dir")),
    )
}

/// `MAX_LOG_LINES` with a space between the thousands, as `6 000`.
fn kept_lines() -> String {
    let n = MAX_LOG_LINES.to_string();
    let mut out = String::new();
    for (i, c) in n.chars().enumerate() {
        if i > 0 && (n.len() - i).is_multiple_of(3) {
            out.push(' ');
        }
        out.push(c);
    }
    out
}

fn flat<'a>(label: String, message: Option<LogMessage>) -> Element<'a, Message> {
    let mut b = button(text(label)).style(theme::button_flat);
    if let Some(message) = message {
        b = b.on_press(Message::Log(message));
    }
    b.into()
}

pub fn view(state: &State) -> Element<'_, Message> {
    let lang = state.lang.as_str();
    let has_text = !state.log.is_empty();
    let on = |message| has_text.then_some(message);
    let header = row![
        flat(tr(lang, "Copy"), on(LogMessage::Copy)),
        flat(tr(lang, "Clear"), on(LogMessage::Clear)),
        flat(tr(lang, "Open last_run.log"), Some(LogMessage::OpenLastRun)),
        text(tr_fmt(lang, "{} lines kept", &[&kept_lines()])).style(theme::text_muted),
    ]
    .spacing(6)
    .align_y(Center);
    // No dark box (spec L2): the drawer's panel shows through.
    let body = scrollable(text(state.log.text()).font(MONO_FONT).size(12))
        .id(scroll_id())
        .direction(Direction::Both {
            vertical: Scrollbar::default(),
            horizontal: Scrollbar::default(),
        })
        .width(iced::Fill)
        .height(iced::Fill);
    // Mockup: 12 px from the buttons to the line and from the line to the text.
    column![header, hairline(SEPARATOR_ID), body]
        .spacing(12)
        .into()
}

/// Applies the page's events.
pub fn update(state: &mut State, message: LogMessage) -> Task<Message> {
    match message {
        LogMessage::Copy => iced::clipboard::write(state.log.text()),
        LogMessage::Clear => {
            state.log.clear();
            scroll_to_end()
        }
        LogMessage::OpenLastRun => Task::done(open_last_run_message(state)),
    }
}
