//! A folder setting (mockup `.folder`): its name, the path in an editable one-line field (the
//! Python's Entry, ruling R89), `Browse…` and `Open`. A long path scrolls inside the field. Typing
//! sends the same message as a folder the dialog chose. `Open` opens the folder it is given (the
//! setting, or the data folder's default for an empty one), made first when missing.

use std::path::PathBuf;

use iced::widget::{button, container, row as layout, text, text_input, Id};
use iced::{Center, Element, Fill, Right};

use crate::core::i18n::tr;
use crate::ui::app::Message;
use crate::ui::theme;

/// Width of the name (mockup `.folder { grid-template-columns: 150px 1fr auto auto }`).
pub const NAME_WIDTH: f32 = 150.0;
/// Space between the parts (mockup `.folder` gap).
const GAP: f32 = 8.0;

/// The id of the field of the folder setting `key`.
pub fn input_id(key: &str) -> Id {
    Id::from(format!("folder-input: {key}"))
}

pub fn folder<'a>(
    rtl: bool,
    key: &str,
    label: &str,
    value: &str,
    empty_hint: &str,
    open: Option<PathBuf>,
    lang: &str,
) -> Element<'a, Message> {
    let typed = key.to_string();
    // An empty field shows what an empty setting means.
    let field = text_input(empty_hint, value)
        .id(input_id(key))
        .on_input(move |path| Message::Browsed(typed.clone(), Some(PathBuf::from(path))))
        .padding([6, 10])
        .width(Fill)
        .style(theme::text_input_dark);
    let browse = button(text(tr(lang, "Browse…")))
        .style(theme::button_flat)
        .on_press(Message::Browse(key.to_string()));
    let open = button(text(tr(lang, "Open")))
        .style(theme::button_flat)
        .on_press_maybe(open.map(Message::OpenFolder));
    // Right to left the line is mirrored: the buttons first, the name against the right edge.
    let line = if rtl {
        // The text keeps its own alignment: an explicit one hides Arabic text in iced 0.14.
        let name = container(text(label.to_string()))
            .width(NAME_WIDTH)
            .align_x(Right);
        layout![open, browse, field, name]
    } else {
        layout![
            text(label.to_string()).width(NAME_WIDTH),
            field,
            browse,
            open
        ]
    };
    container(line.spacing(GAP).align_y(Center))
        .padding([4, 0])
        .into()
}
