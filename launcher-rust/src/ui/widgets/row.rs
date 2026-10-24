//! A setting: its name on the left, the control at the row's right end, an optional note below
//! across the row (mockup `.row { grid-template-columns: 1fr auto }`); mirrored right to left
//! (spec 4.5): the control at the left end, the name against the right edge.

use iced::widget::text::Wrapping;
use iced::widget::{column, container, row as layout, space, text, Id, Text};
use iced::{Center, Element, Fill, Right};

use crate::ui::app::Message;
use crate::ui::theme;
use crate::ui::widgets::leave_room;

/// Space between the name and the control (mockup `.row` gap).
pub const GAP: f32 = 14.0;
/// The narrowest a row's name gets beside a wide control (R98).
pub const NAME_MIN: f32 = 120.0;

/// The id of the box that holds the control of the row named `label`.
pub fn control_id(label: &str) -> Id {
    Id::from(format!("row-control: {label}"))
}

/// A row's name: a word longer than the name's room breaks inside it (iced does not clip text, and
/// the default `Word` wrapping lets a long word run under the control).
pub fn name<'a>(label: &str) -> Text<'a> {
    text(label.to_string()).wrapping(Wrapping::WordOrGlyph)
}

/// A row named by the text `label`.
pub fn row<'a>(
    rtl: bool,
    label: &str,
    control: Element<'a, Message>,
    hint: Option<&str>,
) -> Element<'a, Message> {
    line(rtl, name(label).into(), label, control, hint)
}

/// A row named by `name` (a check box's name, which toggles it too); `label` is its text, which
/// names the control's box.
///
/// The control keeps its own width and the name takes what is left, so a narrow drawer wraps the
/// name, never the control; a control wider than the row less `NAME_MIN` is held to that, so the
/// name keeps at least `NAME_MIN`.
pub fn line<'a>(
    rtl: bool,
    name: Element<'a, Message>,
    label: &str,
    control: Element<'a, Message>,
    hint: Option<&str>,
) -> Element<'a, Message> {
    lay_out(rtl, name, label, control, hint, NAME_MIN)
}

/// A row without a name (R93): `control` (a group of buttons) at the row's end, with the whole
/// row for itself, and the note under it; `label` names the control's box.
pub fn actions<'a>(
    rtl: bool,
    label: &str,
    control: Element<'a, Message>,
    hint: Option<&str>,
) -> Element<'a, Message> {
    lay_out(rtl, space().into(), label, control, hint, 0.0)
}

/// `name`, `control` and the note as a row; the control keeps `room` free for the name.
fn lay_out<'a>(
    rtl: bool,
    name: Element<'a, Message>,
    label: &str,
    control: Element<'a, Message>,
    hint: Option<&str>,
    room: f32,
) -> Element<'a, Message> {
    // The row lays the control out first, against its width less the gap; the name gets the rest.
    let control = leave_room(room, container(control).id(control_id(label)));
    let line = if rtl {
        // The text keeps its own alignment: an explicit one hides Arabic text in iced 0.14.
        let name = container(name).width(Fill).align_x(Right);
        layout![control, name]
    } else {
        layout![container(name).width(Fill), control]
    };
    with_note(rtl, line.spacing(GAP).align_y(Center).into(), hint)
}

/// A row whose control sits beside its name, with `action` at the row's end (R94): a check box
/// that also has a button, as "Load mods" and "Open folder" on the Mods page. Right to left the
/// row is mirrored. `label` names the control's box.
pub fn beside<'a>(
    rtl: bool,
    name: Element<'a, Message>,
    label: &str,
    control: Element<'a, Message>,
    action: Element<'a, Message>,
    hint: Option<&str>,
) -> Element<'a, Message> {
    let control = container(control).id(control_id(label));
    let line = if rtl {
        let named = layout![control, name].spacing(GAP).align_y(Center);
        layout![action, space::horizontal(), named]
    } else {
        let named = layout![name, control].spacing(GAP).align_y(Center);
        layout![named, space::horizontal(), action]
    };
    with_note(rtl, line.spacing(GAP).align_y(Center).into(), hint)
}

/// `line` with the note under it across the row, and the rows' padding.
fn with_note<'a>(
    rtl: bool,
    line: Element<'a, Message>,
    hint: Option<&str>,
) -> Element<'a, Message> {
    let mut body = column![line].spacing(2);
    if rtl {
        body = body.align_x(Right);
    }
    if let Some(hint) = hint.filter(|hint| !hint.is_empty()) {
        let note = text(hint.to_string()).size(13).style(theme::text_muted);
        // Left to right the note spans the row from the name's edge.
        body = body.push(if rtl { note } else { note.width(Fill) });
    }
    body.padding([4, 0]).into()
}
