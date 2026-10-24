//! A drop-down over a list of choices; selecting one stores its value, not its label. It is as wide
//! as its longest option, and at least the mockup's `.sel` minimum.

use iced::widget::pick_list;
use iced::Element;
use serde_json::Value;

use crate::core::i18n::tr;
use crate::core::options::Choice;
use crate::ui::app::Message;
use crate::ui::theme;
use crate::ui::widgets::min_width;

/// The narrowest a drop-down gets (mockup `.sel { min-width: 170px }`).
pub const SELECT_MIN: f32 = 170.0;
/// The narrowest a short drop-down gets (mockup `.sel.sm { min-width: 90px }`).
pub const SELECT_MIN_SMALL: f32 = 90.0;
/// The text size of every drop-down (mockup `.sel { font-size: 12.5px }`).
pub const SELECT_TEXT: f32 = 12.5;

/// What a choice is called in `lang`: the Russian text when there is one, else `tr` of English.
fn label(choice: &Choice, lang: &str) -> String {
    match choice.ru {
        Some(ru) if lang == "ru" => ru.to_string(),
        _ => tr(lang, choice.en),
    }
}

/// The `(value, label)` pairs of a choice table in `lang`.
pub fn choice_pairs(choices: &[Choice], lang: &str) -> Vec<(String, String)> {
    choices
        .iter()
        .map(|choice| (choice.value.to_string(), label(choice, lang)))
        .collect()
}

/// The `Set` message of the choice shown as `chosen`, if any choice has that label.
pub fn select_message(key: &str, choices: &[Choice], lang: &str, chosen: &str) -> Option<Message> {
    choices
        .iter()
        .find(|choice| label(choice, lang) == chosen)
        .map(|choice| Message::Set(key.to_string(), Value::String(choice.value.to_string())))
}

pub fn select<'a>(
    key: &str,
    choices: &'a [Choice],
    current: &str,
    lang: &str,
) -> Element<'a, Message> {
    let labels: Vec<String> = choices.iter().map(|choice| label(choice, lang)).collect();
    let selected = choices
        .iter()
        .find(|choice| choice.value == current)
        .map(|choice| label(choice, lang));
    let key = key.to_string();
    let lang = lang.to_string();
    let list = pick_list(labels, selected, move |chosen: String| {
        select_message(&key, choices, &lang, &chosen).unwrap_or(Message::Noop)
    })
    .text_size(SELECT_TEXT)
    .style(theme::pick_list_dark);
    min_width(SELECT_MIN, list).into()
}

/// Same, over `(value, label)` pairs whose labels are already translated.
pub fn select_pairs<'a>(
    key: &str,
    pairs: &[(String, String)],
    current: &str,
) -> Element<'a, Message> {
    pairs_list(key, pairs, current, SELECT_MIN)
}

/// Same, for a list of short labels (mockup `.sel.sm`).
pub fn select_pairs_small<'a>(
    key: &str,
    pairs: &[(String, String)],
    current: &str,
) -> Element<'a, Message> {
    pairs_list(key, pairs, current, SELECT_MIN_SMALL)
}

fn pairs_list<'a>(
    key: &str,
    pairs: &[(String, String)],
    current: &str,
    min: f32,
) -> Element<'a, Message> {
    let labels: Vec<String> = pairs.iter().map(|(_, label)| label.clone()).collect();
    let selected = pairs
        .iter()
        .find(|(value, _)| value == current)
        .map(|(_, label)| label.clone());
    let key = key.to_string();
    let pairs = pairs.to_vec();
    let list = pick_list(labels, selected, move |chosen: String| {
        pairs
            .iter()
            .find(|(_, label)| *label == chosen)
            .map(|(value, _)| Message::Set(key.clone(), Value::String(value.clone())))
            .unwrap_or(Message::Noop)
    })
    .text_size(SELECT_TEXT)
    .style(theme::pick_list_dark);
    min_width(min, list).into()
}
