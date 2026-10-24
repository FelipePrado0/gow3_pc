#![allow(clippy::unwrap_used, clippy::expect_used)]
//! The settings widgets: what they emit when the user acts on them.

mod common;

use iced::{Element, Size};
use iced_test::core::Settings;
use iced_test::simulator::{simulator, Simulator};
use serde_json::Value;
use spartanport_launcher::core::options::UPSCALERS;
use spartanport_launcher::ui::app::Message;
use spartanport_launcher::ui::widgets::modal::ModalMessage;
use spartanport_launcher::ui::widgets::{self, ModalKind};

/// Renders `view`, clicks the widget showing `label` and returns what it emitted.
fn click(view: Element<'_, Message>, label: &str) -> Vec<Message> {
    let mut ui = simulator(view);
    ui.click(label)
        .unwrap_or_else(|error| panic!("click {label:?}: {error:?}"));
    ui.into_messages().collect()
}

#[test]
fn select_emits_set_with_the_value_not_the_label() {
    // The simulator cannot open a pick_list overlay reliably, so the label -> value mapping the
    // widget's `on_select` runs is tested directly; the widget itself is built to prove it renders.
    let _view = widgets::select("upscaler", UPSCALERS, "fsr3", "en");
    let message = widgets::select_message(
        "upscaler",
        UPSCALERS,
        "en",
        "TAA (native resolution anti-aliasing)",
    );
    assert!(
        matches!(&message, Some(Message::Set(key, Value::String(value))) if key == "upscaler" && value == "taa"),
        "{message:?}"
    );
    let russian = widgets::select_message("upscaler", UPSCALERS, "ru", "Выключен");
    assert!(
        matches!(&russian, Some(Message::Set(_, Value::String(value))) if value == "off"),
        "{russian:?}"
    );
    assert!(widgets::select_message("upscaler", UPSCALERS, "en", "nope").is_none());
}

#[test]
fn checkbox_ini_emits_one_and_zero() {
    for (checked, expected) in [(true, "0"), (false, "1")] {
        let messages = click(
            widgets::checkbox_ini(false, "vsync", "Vsync", checked, None),
            "Vsync",
        );
        assert_eq!(messages.len(), 1);
        assert!(
            matches!(&messages[0], Message::Set(key, Value::String(v)) if key == "vsync" && v == expected),
            "{checked}: {messages:?}"
        );
    }
    let messages = click(
        widgets::checkbox(false, "fx", "Effects", false, Some("hint")),
        "Effects",
    );
    assert!(matches!(&messages[0], Message::Set(key, Value::Bool(true)) if key == "fx"));
}

#[test]
fn a_check_box_sits_at_the_end_of_its_row() {
    // Mockup `.row` with `.cb`: the name first, the box at the row's end; mirrored right to left.
    for rtl in [false, true] {
        let row = widgets::checkbox_ini(rtl, "vsync", "Vsync", true, Some("A note."));
        let mut ui = Simulator::with_size(Settings::default(), Size::new(600.0, 200.0), row);
        let boxed = ui
            .find(iced_test::selector::id(widgets::row::control_id("Vsync")))
            .unwrap()
            .bounds();
        let name = ui.find("Vsync").unwrap().bounds();
        if rtl {
            assert!(
                boxed.x.abs() < 0.5 && name.x > boxed.x,
                "{boxed:?}, {name:?}"
            );
        } else {
            assert!(
                (boxed.x + boxed.width - 600.0).abs() < 0.5 && name.x < boxed.x,
                "{boxed:?}, {name:?}"
            );
        }
    }
}

#[test]
fn a_long_control_leaves_the_row_name_its_floor() {
    // N1 (R98): the control takes its natural width but never the name's last 120 px, and a
    // word longer than the name's room breaks inside it instead of running under the control.
    let long = "Automatic (NVIDIA GeForce RTX 4060 Laptop GPU: on)".to_string();
    let name = "Supercalifragilisticexpialidocious";
    let one_line = common::natural_size(name, None, 16.0);
    for rtl in [false, true] {
        let control = widgets::select_pairs("no_msaa", &[(String::new(), long.clone())], "");
        let row = widgets::row(rtl, name, control, None);
        let mut ui = Simulator::with_size(Settings::default(), Size::new(400.0, 200.0), row);
        let control = ui
            .find(iced_test::selector::id(widgets::row::control_id(name)))
            .unwrap()
            .bounds();
        let label = ui.find(name).unwrap().bounds();
        // The name's room: the row less the control and the gap between them.
        let room = if rtl {
            400.0 - (control.x + control.width)
        } else {
            control.x
        } - widgets::row::GAP;
        assert!(
            room >= 120.0 - 0.5,
            "rtl {rtl}: {room} left beside {control:?}"
        );
        let clear = if rtl {
            control.x + control.width <= label.x + 0.5
        } else {
            label.x + label.width <= control.x + 0.5
        };
        assert!(clear, "rtl {rtl}: {label:?} overlaps {control:?}");
        assert!(
            label.height > one_line.height * 1.5,
            "rtl {rtl}: the word stays on one line: {label:?}"
        );
    }
}

#[test]
fn folder_browse_emits_browse_with_the_key() {
    let view = widgets::folder(false, "game_dir", "Game folder", "", "Not set", None, "en");
    let messages = click(view, "Browse…");
    assert!(
        matches!(&messages[0], Message::Browse(key) if key == "game_dir"),
        "{messages:?}"
    );
}

#[test]
fn a_folder_field_holds_the_path_and_sends_what_is_typed() {
    let view = widgets::folder(
        false,
        "mods_dir",
        "Mods folder",
        r"D:\Mods",
        "Empty: mods/",
        None,
        "en",
    );
    let mut ui = simulator(view);
    let field = ui.find(r"D:\Mods").unwrap();
    assert!(
        matches!(field, iced_test::selector::Text::Input { .. }),
        "{field:?}"
    );
    // The click lands after the short path, so the typing goes at its end.
    ui.click(r"D:\Mods").unwrap();
    ui.typewrite("2");
    let messages: Vec<Message> = ui.into_messages().collect();
    assert!(
        matches!(messages.last(), Some(Message::Browsed(key, Some(path)))
            if key == "mods_dir" && path.as_path() == std::path::Path::new(r"D:\Mods2")),
        "{messages:?}"
    );
}

#[test]
fn folder_open_targets_the_folder_it_is_given() {
    let target = std::path::PathBuf::from("data").join("user");
    let view = widgets::folder(
        false,
        "user_dir",
        "Saves folder",
        "",
        "",
        Some(target.clone()),
        "en",
    );
    let messages = click(view, "Open");
    assert!(
        matches!(messages.as_slice(), [Message::OpenFolder(path)] if *path == target),
        "{messages:?}"
    );
    // Without one (an unset game folder) Open does nothing.
    let view = widgets::folder(false, "game_dir", "Game folder", "", "", None, "en");
    assert!(click(view, "Open").is_empty());
}

#[test]
fn modal_yes_no_emit_their_messages() {
    let yes_no = || ModalKind::YesNo {
        yes: Message::Save,
        no: Message::Stop,
    };
    let view = widgets::modal(yes_no(), "Title", "Body", "en");
    assert!(matches!(click(view, "Yes")[0], Message::Save));
    let view = widgets::modal(yes_no(), "Title", "Body", "en");
    assert!(matches!(click(view, "No")[0], Message::Stop));
    for kind in [ModalKind::Info, ModalKind::Error] {
        let view = widgets::modal(kind, "Title", "Body", "en");
        assert!(matches!(
            click(view, "OK")[0],
            Message::Modal(ModalMessage::Dismiss)
        ));
    }
}
