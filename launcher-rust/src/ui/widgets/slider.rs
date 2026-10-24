//! A whole-number slider with its value and unit beside it.

use iced::widget::{row as layout, slider as slide, text};
use iced::{Center, Element};
use serde_json::Value;

use crate::ui::app::Message;

/// Width of the track.
const TRACK: f32 = 220.0;

pub fn slider<'a>(key: &str, low: f32, high: f32, unit: &str, value: f32) -> Element<'a, Message> {
    let key = key.to_string();
    let track = slide(low..=high, value, move |v| {
        Message::Set(key.clone(), Value::from(v as i64))
    })
    .step(1.0_f32)
    .width(TRACK);
    layout![track, text(format!("{} {unit}", value as i64))]
        .spacing(10)
        .align_y(Center)
        .into()
}
