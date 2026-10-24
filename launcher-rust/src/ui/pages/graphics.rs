//! The Graphics page: window, textures, overlay and screenshots.
//!
//! Ruling R35: like Python 2.6.0, the Experimental and Upscaling sections (`experimental_upscaler`,
//! `upscaler`, `preset`, `output_res`, `live_resolution`, `object_motion`) stay parked until the
//! image quality of temporal upscaling is validated, so this page does not show them.

use iced::widget::{button, column, text};
use iced::Element;

use crate::core::i18n::tr;
use crate::core::options::{ANISOTROPY, DISPLAY_MODES};
use crate::ui::app::{Message, State};
use crate::ui::theme;
use crate::ui::widgets::{checkbox, checkbox_ini, row, section, select, select_pairs};

/// An ini flag is on when its value is `"1"`.
fn ini_on(state: &State, key: &str) -> bool {
    state.ini.values.get(key).is_some_and(|value| value == "1")
}

pub fn view(state: &State) -> Element<'_, Message> {
    let lang = state.lang.as_str();
    let hint = |english: &str| tr(lang, english);
    let ini = |key: &str| state.ini.values.get(key).cloned().unwrap_or_default();

    // The list the probe made, or just the primary default while it has not answered.
    let pairs: Vec<(String, String)> = if state.monitor_pairs.is_empty() {
        crate::core::gpu::monitor_options(&[])
    } else {
        state.monitor_pairs.clone()
    }
    .into_iter()
    .map(|(value, label)| {
        let label = if value.is_empty() {
            tr(lang, &label)
        } else {
            label
        };
        (value, label)
    })
    .collect();
    let monitor = state.settings.get_str("monitor");

    // The Python marks a row that needs a new start of the game with a small `restart` note.
    let anisotropy_label = format!(
        "{}  ·  {}",
        tr(lang, "Anisotropic filtering"),
        tr(lang, "restart")
    );

    let shots = button(text(tr(lang, "Open the screenshots folder")))
        .style(theme::button_flat)
        .on_press(Message::OpenFolder(
            state
                .paths
                .user_dir(&state.settings.get_str("user_dir"))
                .join("screenshots"),
        ));

    column![
        section(state.rtl, &tr(lang, "Window")),
        row(
            state.rtl,
            &tr(lang, "Display mode"),
            select("display_mode", DISPLAY_MODES, &ini("display_mode"), lang),
            Some(&hint(
                "Borderless covers the screen without changing the display mode."
            )),
        ),
        row(
            state.rtl,
            &tr(lang, "Monitor"),
            select_pairs("monitor", &pairs, &monitor),
            Some(&hint(
                "The screen the game opens its window on. Primary is the monitor the system considers the main one."
            )),
        ),
        checkbox_ini(
            state.rtl,
            "vsync",
            &tr(lang, "VSync"),
            ini_on(state, "vsync"),
            Some(&hint("Off allows tearing for the lowest latency.")),
        ),
        checkbox(
            state.rtl,
            "hdr",
            &tr(lang, "Allow HDR output"),
            state.settings.get_bool("hdr"),
            Some(&hint("When HDR is on in Windows and the display supports it.")),
        ),
        section(state.rtl, &tr(lang, "Textures")),
        row(
            state.rtl,
            &anisotropy_label,
            select("anisotropy", ANISOTROPY, &ini("anisotropy"), lang),
            Some(&hint(
                "Sharper floors and walls seen at an angle, at little cost on most graphics cards."
            )),
        ),
        section(state.rtl, &tr(lang, "Overlay")),
        checkbox_ini(
            state.rtl,
            "show_fps",
            &tr(lang, "Show performance overlay"),
            ini_on(state, "show_fps"),
            Some(&hint(
                "Items, size and corner: in-game menu (Insert / R3+L2) > Overlay."
            )),
        ),
        section(state.rtl, &tr(lang, "Screenshots")),
        row(
            state.rtl,
            &tr(lang, "F12 in the game"),
            shots.into(),
            Some(&hint(
                "Saves the game's image, without the overlay, as a PNG in the screenshots folder of the saves."
            )),
        ),
    ]
    .spacing(2)
    .into()
}
