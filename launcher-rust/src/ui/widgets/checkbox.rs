//! Check boxes as setting rows (mockup `.row` with `.cb`): the name, then the box at the row's end;
//! the name toggles it too. One stores a JSON bool, the other the ini's `"1"` / `"0"`.

use iced::widget::{checkbox as check_box, mouse_area};
use iced::Element;
use serde_json::Value;

use crate::ui::app::Message;
use crate::ui::theme;
use crate::ui::widgets::row::{beside, line, name};

/// The name and the box of a check box row; the name toggles the box too.
fn parts<'a, F>(
    label: &str,
    checked: bool,
    on_toggle: Option<F>,
) -> (Element<'a, Message>, Element<'a, Message>)
where
    F: Fn(bool) -> Message + 'a,
{
    let boxed = check_box(checked).style(theme::checkbox_red);
    match on_toggle {
        Some(toggle) => (
            mouse_area(name(label)).on_press(toggle(!checked)).into(),
            boxed.on_toggle(toggle).into(),
        ),
        None => (name(label).into(), boxed.into()),
    }
}

/// A check box row: `on_toggle` makes the message of a new state; without it the box is drawn but
/// nothing toggles it.
pub fn toggle_row<'a, F>(
    rtl: bool,
    label: &str,
    checked: bool,
    on_toggle: Option<F>,
    hint: Option<&str>,
) -> Element<'a, Message>
where
    F: Fn(bool) -> Message + 'a,
{
    let (name, boxed) = parts(label, checked, on_toggle);
    line(rtl, name, label, boxed, hint)
}

/// A check box of the setting `key` beside its name, with `action` at the row's end (R94).
pub fn checkbox_beside<'a>(
    rtl: bool,
    key: &str,
    label: &str,
    checked: bool,
    action: Element<'a, Message>,
) -> Element<'a, Message> {
    let key = key.to_string();
    let toggle = move |on| Message::Set(key.clone(), Value::Bool(on));
    let (name, boxed) = parts(label, checked, Some(toggle));
    beside(rtl, name, label, boxed, action, None)
}

fn setting<'a>(
    rtl: bool,
    key: &str,
    label: &str,
    checked: bool,
    hint: Option<&str>,
    value: impl Fn(bool) -> Value + 'a,
) -> Element<'a, Message> {
    let key = key.to_string();
    let toggle = move |on| Message::Set(key.clone(), value(on));
    toggle_row(rtl, label, checked, Some(toggle), hint)
}

pub fn checkbox<'a>(
    rtl: bool,
    key: &str,
    label: &str,
    checked: bool,
    hint: Option<&str>,
) -> Element<'a, Message> {
    setting(rtl, key, label, checked, hint, Value::Bool)
}

pub fn checkbox_ini<'a>(
    rtl: bool,
    key: &str,
    label: &str,
    checked: bool,
    hint: Option<&str>,
) -> Element<'a, Message> {
    setting(rtl, key, label, checked, hint, |on| {
        Value::String(if on { "1" } else { "0" }.to_string())
    })
}
