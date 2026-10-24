#![allow(clippy::unwrap_used, clippy::expect_used)]
//! The settings pages of the drawer (Tasks 24-31).

mod common;

use common::{
    browsed, click, dump_with_patches, gpu_rows, load_mods, load_trophies, make_mod, messages,
    open, state, state_with_trophies, two_displays, write_mod_patch, ARC_LAPTOP,
};
use spartanport_launcher::core::gpu::{display_label, parse_display_list};
use spartanport_launcher::core::options::DISPLAY_MODES;
use spartanport_launcher::ui::app::{self, Message};
use spartanport_launcher::ui::pages::{performance, Page};
use spartanport_launcher::ui::widgets::{self, select_message};

#[test]
fn graphics_rows_change_the_right_keys() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state(dir.path());
    open(&mut state, Page::Graphics);
    assert_eq!(state.ini.values["vsync"], "1");
    click(&mut state, "VSync");
    assert_eq!(state.ini.values["vsync"], "0");
    assert!(!state.settings.get_bool("hdr"));
    click(&mut state, "Allow HDR output");
    assert!(state.settings.get_bool("hdr"));
    // iced_test cannot open a pick_list: the pure mapping stands in for the click.
    let pick = select_message("display_mode", DISPLAY_MODES, "en", "Borderless").unwrap();
    assert!(
        matches!(&pick, Message::Set(key, value) if key == "display_mode" && value == "borderless")
    );
    let _ = app::update(&mut state, pick);
    assert_eq!(state.ini.values["display_mode"], "borderless");
    assert!(state.settings.get("display_mode").is_none());
}

/// The window of `state` at `width` × `height` with `page` open, once the drawer has stopped.
fn settled(
    state: &mut app::State,
    page: Page,
    width: f32,
    height: f32,
) -> iced_test::simulator::Simulator<'_, Message> {
    open(state, page);
    let _ = app::update(
        state,
        Message::Tick(iced::time::Instant::now() + std::time::Duration::from_secs(1)),
    );
    iced_test::simulator::Simulator::with_size(
        iced::Settings::default(),
        iced::Size::new(width, height),
        app::view(state),
    )
}

/// The bounds of the control of the row named `label`.
fn control(ui: &mut iced_test::simulator::Simulator<'_, Message>, label: &str) -> iced::Rectangle {
    ui.find(iced_test::selector::id(widgets::row::control_id(label)))
        .unwrap_or_else(|error| panic!("control of {label:?}: {error:?}"))
        .bounds()
}

#[test]
fn every_row_control_ends_on_the_same_right_edge() {
    // Mockup `.row { grid-template-columns: 1fr auto }`: drop-downs, check boxes and buttons end
    // where the row ends, whatever the name before them; right to left they start where it starts.
    for lang in ["en", "pt", "ar"] {
        let dir = tempfile::tempdir().unwrap();
        let mut state = state(dir.path());
        state.lang = lang.into();
        state.rtl = lang == "ar";
        let tr = |english: &str| spartanport_launcher::core::i18n::tr(lang, english);
        let labels = [
            tr("Display mode"),
            tr("Monitor"),
            tr("VSync"),
            tr("Allow HDR output"),
            format!("{}  ·  {}", tr("Anisotropic filtering"), tr("restart")),
            tr("Show performance overlay"),
            tr("F12 in the game"),
        ];
        let mut ui = settled(&mut state, Page::Graphics, 1280.0, 800.0);
        let edges: Vec<(String, f32)> = labels
            .iter()
            .map(|label| {
                let bounds = control(&mut ui, label);
                let edge = if lang == "ar" {
                    bounds.x
                } else {
                    bounds.x + bounds.width
                };
                (label.clone(), edge)
            })
            .collect();
        let first = edges[0].1;
        assert!(
            edges.iter().all(|(_, edge)| (edge - first).abs() < 0.5),
            "{lang}: {edges:?}"
        );
    }
}

#[test]
fn a_drop_down_keeps_the_width_of_its_longest_option() {
    // The user's screenshot: "RAM do sistema (com cache, padrão)" wrapped inside its drop-down,
    // which got what a fixed-width name left of the row. The drop-down takes its own width and
    // the name what is left; the note spans the row under both, from the name's edge.
    let dir = tempfile::tempdir().unwrap();
    let mut state = state(dir.path());
    state.lang = "pt".into();
    state.gpus = gpu_rows(&ARC_LAPTOP);
    let tr = |english: &str| spartanport_launcher::core::i18n::tr("pt", english);
    let restart = |english: &str| format!("{}  ·  {}", tr(english), tr("restart"));
    let rows = performance::rows(&state, "pt");
    let mut ui = settled(&mut state, Page::Performance, 1280.0, 800.0);
    // Mockup `.sel { font-size: 12.5px }` (R98).
    let size = 12.5;
    for (key, name, hint) in [
        (
            "stream_memory",
            restart("Stream buffer memory"),
            tr("System RAM can improve FPS on NVIDIA laptops. Choose GPU memory to restore the previous behavior. Applies on the next game launch."),
        ),
        // The longest option of the page.
        (
            "no_msaa",
            restart("Game multisampling"),
            tr(performance::NO_MSAA_HINT),
        ),
    ] {
        let row = rows.iter().find(|row| row.key == key).unwrap();
        let longest = row
            .pairs
            .iter()
            .map(|(_, label)| common::natural_size(label, None, size).width)
            .fold(0.0, f32::max);
        let control = control(&mut ui, &name);
        // iced sizes a pick list to its longest label, the arrow (the text size), the padding's
        // left side and then the padding (10 px a side) around it all, on one line.
        let natural = longest + size + 30.0;
        assert!(
            (control.width - natural).abs() < 0.5,
            "{key}: {} wide, its longest option at {size} px needs {natural}",
            control.width,
        );
        let label = ui.find(name.as_str()).unwrap().bounds();
        let note = ui.find(hint.as_str()).unwrap().bounds();
        assert!((note.x - label.x).abs() < 0.5, "{key}: {note:?}, {label:?}");
        assert!(
            (note.x + note.width - (control.x + control.width)).abs() < 0.5,
            "{key}: the note {note:?} does not span the row to {control:?}"
        );
    }
}

#[test]
fn the_multisampling_name_keeps_clear_of_its_drop_down() {
    // N1 (R98): at 1280 × 800 the long "Automatic (NVIDIA GeForce RTX 4060 Laptop GPU: on)" option
    // left the row's name a sliver, and the name's words ran over the drop-down.
    let dir = tempfile::tempdir().unwrap();
    let mut state = state(dir.path());
    state.lang = "pt".into();
    state.gpus = gpu_rows(&ARC_LAPTOP);
    let tr = |english: &str| spartanport_launcher::core::i18n::tr("pt", english);
    let name = format!("{}  ·  {}", tr("Game multisampling"), tr("restart"));
    let mut ui = settled(&mut state, Page::Performance, 1280.0, 800.0);
    let control = control(&mut ui, &name);
    let label = ui.find(name.as_str()).unwrap().bounds();
    assert!(
        label.x + label.width <= control.x + 0.5,
        "the name {label:?} runs over the drop-down {control:?}"
    );
    assert!(label.width >= 120.0 - 0.5, "the name is {label:?}");
}

#[test]
fn monitor_row_lists_the_probe_indices() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state(dir.path());
    let displays = two_displays();
    let _ = app::update(&mut state, Message::MonitorsListed(displays.clone()));
    assert_eq!(
        state.monitor_pairs,
        vec![
            (String::new(), "Primary (default)".to_string()),
            ("0".to_string(), display_label(&displays[0])),
            ("1".to_string(), display_label(&displays[1])),
        ]
    );
    let second = state.monitor_pairs[2].1.clone();
    let pick = state
        .monitor_pairs
        .iter()
        .find(|(_, label)| *label == second)
        .map(|(value, _)| Message::Set("monitor".into(), value.clone().into()))
        .unwrap();
    let _ = app::update(&mut state, pick);
    assert_eq!(state.settings.get_str("monitor"), "1");
}

#[test]
fn graphics_page_offers_the_choice() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state(dir.path());
    open(&mut state, Page::Graphics);
    // The page lists the probe's monitors, a gap in the indices stays a gap.
    let gap = parse_display_list("0\tA\t1\t0\t0\t1920\t1080\n2\tB\t0\t0\t0\t1280\t720\n");
    let _ = app::update(&mut state, Message::MonitorsListed(gap));
    let values: Vec<&str> = state
        .monitor_pairs
        .iter()
        .map(|(v, _)| v.as_str())
        .collect();
    assert_eq!(values, ["", "0", "2"]);
    assert_eq!(state.monitor_pairs[1].1, "A · 1920 × 1080 · (primary)");
    assert_eq!(state.monitor_pairs[2].1, "B · 1280 × 720");

    // A saved choice comes back; one whose monitor is gone falls back to the primary default.
    state.settings.set("monitor", "2".into());
    let _ = app::update(&mut state, Message::MonitorsListed(two_displays()));
    assert_eq!(state.settings.get_str("monitor"), "");
    state.settings.set("monitor", "1".into());
    let _ = app::update(&mut state, Message::MonitorsListed(two_displays()));
    assert_eq!(state.settings.get_str("monitor"), "1");
    let _ = app::update(&mut state, Message::MonitorsListed(Vec::new()));
    assert_eq!(state.monitor_pairs.len(), 1);
    assert_eq!(state.settings.get_str("monitor"), "");

    // The screenshots button opens <saves>/screenshots, made first when missing.
    let sent = messages(&state, "Open the screenshots folder");
    let expected = state.paths.user_dir("").join("screenshots");
    assert!(matches!(sent.as_slice(), [Message::OpenFolder(path)] if *path == expected));
}

#[test]
fn open_makes_a_missing_folder_first() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state(dir.path());
    // Python `open_path`: `mkdir` before the file manager opens it.
    let missing = dir.path().join("user").join("screenshots");
    let task = app::update(&mut state, Message::OpenFolder(missing.clone()));
    assert!(missing.is_dir());
    assert_eq!(task.units(), 1, "then it opens");
}

#[test]
fn an_empty_folder_setting_opens_the_data_folder_one() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state(dir.path());
    state.lang = "en".into();
    let _ = app::update(&mut state, Message::ShowPage(Page::Mods));
    // The Mods folder row comes first; its setting is empty.
    assert_eq!(state.settings.get_str("mods_dir"), "");
    let sent = tall_messages(&state, "Open");
    let expected = state.paths.mods_dir("");
    assert!(
        matches!(sent.as_slice(), [Message::OpenFolder(path)] if *path == expected),
        "{sent:?}"
    );
}

fn set(state: &mut app::State, key: &str, value: &str) {
    let _ = app::update(state, Message::Set(key.into(), value.into()));
}

fn values(pairs: &[(String, String)]) -> Vec<&str> {
    pairs.iter().map(|(value, _)| value.as_str()).collect()
}

#[test]
fn aspect_follows_and_filters_the_render_resolution() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state(dir.path());
    assert_eq!(state.ini.values["render_resolution"], "native");
    assert_eq!(state.aspect, "16:9");
    let _ = app::update(&mut state, Message::Aspect("16:10".into()));
    assert_eq!(state.ini.values["render_resolution"], "1920x1200");
    assert_eq!(
        values(&performance::aspect_resolution_options(&state)),
        ["1280x800", "1440x900", "1920x1200", "2560x1600"]
    );
    set(&mut state, "render_resolution", "3440x1440");
    assert_eq!(state.aspect, "21:9");
    assert!(performance::aspect_options(&state).contains(&"4:3".to_string()));
    // Confirmed in game: not beta, so no suffix.
    let options = performance::aspect_resolution_options(&state);
    let label = options.iter().find(|(v, _)| v == "3440x1440").unwrap();
    assert_eq!(label.1, "3440 × 1440");
}

#[test]
fn native_is_translated_in_the_resolution_lists() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state(dir.path());
    let native = |pairs: Vec<(String, String)>| {
        pairs
            .into_iter()
            .find(|(value, _)| value == "native")
            .unwrap()
            .1
    };
    state.lang = "en".into();
    assert_eq!(
        native(performance::resolution_options(&state)),
        "Native · 1920 × 1080 · 16:9"
    );
    assert_eq!(
        native(performance::aspect_resolution_options(&state)),
        "Native · 1920 × 1080"
    );
    // The Python passes the labels through `_()`; the word alone has its Portuguese too.
    state.lang = "pt".into();
    assert_eq!(
        native(performance::resolution_options(&state)),
        "Nativa · 1920 × 1080 · 16:9"
    );
    assert_eq!(
        native(performance::aspect_resolution_options(&state)),
        "Nativa · 1920 × 1080"
    );
    // A whole label of the Python's tables still wins.
    state.lang = "zh".into();
    assert_eq!(
        native(performance::resolution_options(&state)),
        "原生 · 1920 × 1080 · 16:9"
    );
}

#[test]
fn no_msaa_auto_label_names_the_card_and_the_decision() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state(dir.path());
    state.gpus = gpu_rows(&ARC_LAPTOP);
    let options = performance::no_msaa_label(&state, "en");
    assert_eq!(values(&options), ["", "1", "0"]);
    assert_eq!(
        options[0].1,
        "Automatic (NVIDIA GeForce RTX 4060 Laptop GPU: on)"
    );
    assert_eq!(options[1].1, "Off (fixes black lines)");
    assert_eq!(options[2].1, "On");
    state.settings.set("gpu_id", "0".into());
    assert_eq!(
        performance::no_msaa_label(&state, "en")[0].1,
        "Automatic (Intel(R) Arc(TM) Graphics: off)"
    );
    // An old tool, none at all, or a spelling this launcher does not know: the plain label.
    state.gpus = gpu_rows(&[("0", "AMD Radeon RX 6600", "discrete", "", "0")]);
    assert_eq!(performance::no_msaa_label(&state, "en")[0].1, "Automatic");
    state.gpus = Vec::new();
    state.settings.set("gpu_id", "".into());
    assert_eq!(performance::no_msaa_label(&state, "en")[0].1, "Automatic");
    state.gpus = gpu_rows(&[("0", "x", "discrete", "OFF", "1")]);
    assert_eq!(performance::no_msaa_label(&state, "en")[0].1, "Automatic");
    state.gpus = gpu_rows(&ARC_LAPTOP);
    state.settings.set("gpu_id", "0".into());
    assert_eq!(
        performance::no_msaa_label(&state, "pt")[0].1,
        "Automático (Intel(R) Arc(TM) Graphics: desligado)"
    );
    state.settings.set("gpu_id", "1".into());
    assert_eq!(
        performance::no_msaa_label(&state, "pt")[0].1,
        "Automático (NVIDIA GeForce RTX 4060 Laptop GPU: ligado)"
    );
}

/// The labels the simulator finds on the rendered page, in a window tall enough for all rows.
fn page_has(state: &app::State, labels: &[&str]) {
    let mut ui = iced_test::simulator::Simulator::with_size(
        iced::Settings::default(),
        iced::Size::new(1280.0, 4000.0),
        app::view(state),
    );
    for label in labels {
        ui.find(*label)
            .unwrap_or_else(|error| panic!("find {label:?}: {error:?}"));
    }
}

#[test]
fn performance_page_offers_the_choice() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state(dir.path());
    state.lang = "en".into();
    state.gpus = gpu_rows(&ARC_LAPTOP);
    open(&mut state, Page::Performance);
    assert_eq!(state.settings.get_str("no_msaa"), "");
    // The page shows the rows with their hints. The text inside a closed pick_list is not a
    // widget the simulator can find, so the options are checked through `rows` below.
    page_has(
        &state,
        &[
            "Game multisampling  ·  restart",
            performance::NO_MSAA_HINT,
            "Graphics card  ·  restart",
        ],
    );
    // iced_test cannot open a pick_list: the page builds its drop-downs from `rows`, so those
    // are what the page offers.
    let rows = performance::rows(&state, "en");
    let row = |key: &str| rows.iter().find(|row| row.key == key).unwrap().clone();
    let boxes = row("no_msaa");
    assert_eq!(boxes.pairs, performance::no_msaa_label(&state, "en"));
    assert_eq!(boxes.current, "");
    let pick = |row: &performance::SelectRow, label: &str| {
        let (value, _) = row.pairs.iter().find(|(_, l)| l == label).unwrap();
        Message::Set(row.key.into(), value.clone().into())
    };
    let _ = app::update(&mut state, pick(&boxes, "Off (fixes black lines)"));
    assert_eq!(state.settings.get_str("no_msaa"), "1");
    // The graphics card list offers Automatic and both cards, in the order of the probe.
    let cards = row("gpu_id");
    let labels: Vec<&str> = cards.pairs.iter().map(|(_, l)| l.as_str()).collect();
    assert_eq!(
        labels,
        [
            "Automatic",
            "Intel(R) Arc(TM) Graphics",
            "NVIDIA GeForce RTX 4060 Laptop GPU"
        ]
    );
    // Choosing a card relabels Automatic at once and keeps the multisampling choice.
    let _ = app::update(&mut state, pick(&cards, "Intel(R) Arc(TM) Graphics"));
    assert_eq!(state.settings.get_str("gpu_id"), "0");
    let boxes = performance::rows(&state, "en")
        .into_iter()
        .find(|row| row.key == "no_msaa")
        .unwrap();
    assert_eq!(
        boxes.pairs[0].1,
        "Automatic (Intel(R) Arc(TM) Graphics: off)"
    );
    assert_eq!(boxes.current, "1");
    assert_eq!(state.settings.get_str("no_msaa"), "1");
    // The Portuguese page shows the translated title, hint and label.
    state.lang = "pt".into();
    let hint = spartanport_launcher::core::i18n::tr("pt", performance::NO_MSAA_HINT);
    assert_ne!(hint, performance::NO_MSAA_HINT);
    page_has(&state, &["Multisampling do jogo  ·  reiniciar", &hint]);
    let boxes = performance::rows(&state, "pt")
        .into_iter()
        .find(|row| row.key == "no_msaa")
        .unwrap();
    assert_eq!(
        boxes.pairs[0].1,
        "Automático (Intel(R) Arc(TM) Graphics: desligado)"
    );
}

#[test]
fn a_saved_mod_resolution_stays_chosen_while_its_mod_is_on() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state(dir.path());
    let builtin = performance::resolution_options(&state).len();
    let patches = state.paths.mods_dir("").join("Ultrawide").join("patches");
    std::fs::create_dir_all(&patches).unwrap();
    // 3440x1440 is built in: the mod adds nothing new and the saved size stays chosen.
    write_mod_patch(&patches, "3440x1440");
    set(&mut state, "render_resolution", "3440x1440");
    performance::refresh_mod_resolutions(&mut state);
    assert_eq!(performance::resolution_options(&state).len(), builtin);
    assert_eq!(state.ini.values["render_resolution"], "3440x1440");

    // A size that is not built in carries the (mod) label while its mod is on.
    write_mod_patch(&patches, "3840x1080");
    set(&mut state, "render_resolution", "3840x1080");
    performance::refresh_mod_resolutions(&mut state);
    let options = performance::resolution_options(&state);
    assert_eq!(options.len(), builtin + 1);
    let label = options.iter().find(|(v, _)| v == "3840x1080").unwrap();
    assert_eq!(label.1, "3840 × 1080 · 32:9 (mod)");
    assert_eq!(state.ini.values["render_resolution"], "3840x1080");
    assert_eq!(state.aspect, "32:9");
    let listed = performance::aspect_resolution_options(&state);
    let label = listed.iter().find(|(v, _)| v == "3840x1080").unwrap();
    assert_eq!(label.1, "3840 × 1080 (mod)");
    // Unticked on the Mods page: the size is gone from the list.
    std::fs::write(
        state.paths.mods_json(),
        r#"{"disabled": ["Ultrawide"], "order": []}"#,
    )
    .unwrap();
    performance::refresh_mod_resolutions(&mut state);
    assert_eq!(performance::resolution_options(&state).len(), builtin);
    // Ticked again, then "Load mods" off.
    std::fs::remove_file(state.paths.mods_json()).unwrap();
    performance::refresh_mod_resolutions(&mut state);
    assert_eq!(performance::resolution_options(&state).len(), builtin + 1);
    state.settings.set("mods_enabled", false.into());
    performance::refresh_mod_resolutions(&mut state);
    assert_eq!(performance::resolution_options(&state).len(), builtin);
}

// --- Game page (Task 26) ---

use spartanport_launcher::core::i18n::tr;
use spartanport_launcher::core::options::LANGUAGES;
use spartanport_launcher::ui::home;
use spartanport_launcher::ui::pages::game::{self, BackupMessage};
use spartanport_launcher::ui::widgets::{ModalKind, ModalMessage};

#[test]
fn choosing_the_game_folder_refreshes_title_and_patches() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state(dir.path());
    assert_eq!(state.patches.title_id, None);
    let dump = dump_with_patches(dir.path());
    browsed(&mut state, "game_dir", &dump);
    assert_eq!(
        state.settings.get_str("game_dir"),
        dump.display().to_string()
    );
    assert_eq!(state.patches.title_id.as_deref(), Some("CUSA01623"));
    let names: Vec<&str> = state
        .patches
        .selectable
        .iter()
        .map(|m| m.name.as_str())
        .collect();
    assert_eq!(names, ["Fix Alpha"]);
    assert!(state.patches.chosen.contains("Fix Alpha"));
    assert_eq!(state.patches.version, "01.02");
    assert_eq!(state.patches.xml_name, "CUSA01623.xml");
    assert_eq!(home::chips(&state)[1], "✓ CUSA01623 · v01.02");
    // Cancelling the dialog changes nothing.
    let _ = app::update(&mut state, Message::Browsed("game_dir".into(), None));
    assert_eq!(
        state.settings.get_str("game_dir"),
        dump.display().to_string()
    );
}

#[test]
fn a_long_folder_stays_on_one_line_in_its_field() {
    // The user's screenshot: the path broke into five lines. Ruling R89: an editable one-line
    // field holds it (the Python's Entry) and scrolls a long one, between the name and the buttons.
    let dir = tempfile::tempdir().unwrap();
    let mut state = state(dir.path());
    state.lang = "en".into();
    let folder = r"C:\Users\Someone\Documents\My Games\GOW III\God.of.War.III.Remastered.PS4-CUSA01623\CUSA01623";
    state.settings.set("game_dir", folder.into());
    let mut ui = settled(&mut state, Page::Game, 1280.0, 800.0);
    let field = ui.find(folder).unwrap();
    assert!(
        matches!(field, iced_test::selector::Text::Input { .. }),
        "{field:?}"
    );
    let field = field.bounds();
    let line = common::natural_size(folder, None, 16.0);
    assert!(
        line.width > field.width,
        "the field is {} wide",
        field.width
    );
    assert!(field.height < 2.0 * line.height, "{} high", field.height);
    // Mockup `.folder { grid-template-columns: 150px 1fr auto auto; gap: 8px }`.
    let name = ui.find("Game folder").unwrap().bounds();
    let browse = ui.find("Browse…").unwrap().bounds();
    assert!(
        (field.x - (name.x + 150.0 + 8.0)).abs() < 0.5,
        "name {name:?}, field {field:?}"
    );
    // Browse pads its label by 10.
    assert!(
        (browse.x - 10.0 - (field.x + field.width + 8.0)).abs() < 0.5,
        "field {field:?}, Browse {browse:?}"
    );
}

#[test]
fn typing_a_folder_sets_it_like_browse() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state(dir.path());
    state.lang = "en".into();
    let dump = dump_with_patches(dir.path()).display().to_string();
    let mut ui = settled(&mut state, Page::Game, 1280.0, 800.0);
    ui.click(iced_test::selector::id(widgets::folder::input_id(
        "game_dir",
    )))
    .unwrap();
    ui.typewrite(&dump);
    let sent: Vec<Message> = ui.into_messages().collect();
    assert!(!sent.is_empty());
    for message in sent {
        let _ = app::update(&mut state, message);
    }
    assert_eq!(state.settings.get_str("game_dir"), dump);
    // The refresh of a browsed folder: the Home reads the dump.
    assert_eq!(
        spartanport_launcher::ui::home::chips(&state)[1],
        "✓ CUSA01623 · v01.02"
    );
}

#[test]
fn a_read_for_a_folder_typed_over_is_dropped() {
    // N2: each keystroke in a folder field starts a read of the Trophies and Mods lists, and the
    // reads finish in any order: one made for a path since typed over must not replace the list.
    use spartanport_launcher::ui::pages::mods::{self, ModsMessage};
    use spartanport_launcher::ui::pages::trophies::{self, TrophiesMessage};
    let dir = tempfile::tempdir().unwrap();
    let mut state = state_with_trophies(dir.path());
    make_mod(&state, "Ultrawide", "a.bin");
    let game = state.settings.get_str("game_dir");
    let mods_dir = state.paths.mods_dir("").display().to_string();
    // The reads started while the fields held one character less.
    state
        .settings
        .set("game_dir", game[..game.len() - 1].to_string().into());
    state.settings.set(
        "mods_dir",
        mods_dir[..mods_dir.len() - 1].to_string().into(),
    );
    let stale_trophies = trophies::load(&state.paths, &state.settings);
    let stale_mods = mods::load(&state.paths, &state.settings);
    assert!(stale_trophies.icons.is_empty() && stale_mods.order.is_empty());
    state.settings.set("game_dir", game.into());
    state.settings.set("mods_dir", mods_dir.into());
    // The reads for the whole paths finish first, the older ones after them.
    load_trophies(&mut state);
    load_mods(&mut state);
    let _ = app::update(
        &mut state,
        Message::Trophies(TrophiesMessage::Loaded(Box::new(stale_trophies))),
    );
    let _ = app::update(
        &mut state,
        Message::Mods(ModsMessage::Loaded(Box::new(stale_mods))),
    );
    assert_eq!(state.trophies.icons.len(), 35);
    assert_eq!(state.mods.order, ["Ultrawide"]);
}

/// Like `click`, in a window tall enough to show the whole page (the drawer scrolls).
fn click_tall(state: &mut app::State, label: &str) {
    let mut ui = iced_test::simulator::Simulator::with_size(
        iced::Settings::default(),
        iced::Size::new(1280.0, 4000.0),
        app::view(state),
    );
    ui.click(label)
        .unwrap_or_else(|error| panic!("click {label:?}: {error:?}"));
    let sent: Vec<Message> = ui.into_messages().collect();
    for message in sent {
        let _ = app::update(state, message);
    }
}

#[test]
fn unchecking_a_patch_lands_in_game_patches() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state(dir.path());
    state.lang = "en".into();
    browsed(&mut state, "game_dir", &dump_with_patches(dir.path()));
    open(&mut state, Page::Game);
    click_tall(&mut state, "Fix Alpha  (Ann)");
    assert!(!state.patches.chosen.contains("Fix Alpha"));
    let saved = state.settings.get("game_patches").unwrap();
    assert_eq!(saved["CUSA01623"], serde_json::json!([]));
    click_tall(&mut state, "Fix Alpha  (Ann)");
    let saved = state.settings.get("game_patches").unwrap();
    assert_eq!(saved["CUSA01623"], serde_json::json!(["Fix Alpha"]));
}

#[test]
fn restore_asks_first_and_is_blocked_while_running() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state(dir.path());
    state.lang = "en".into();
    let user = state.paths.user_dir("");
    let backup = user.join("savedata_backups").join("20261009-180000");
    std::fs::create_dir_all(&backup).unwrap();
    std::fs::write(backup.join("data.bin"), b"old").unwrap();
    std::fs::create_dir_all(user.join("savedata")).unwrap();
    std::fs::write(user.join("savedata").join("data.bin"), b"new").unwrap();

    // Nothing chosen: nothing happens.
    let _ = app::update(&mut state, Message::Backup(BackupMessage::Restore));
    assert!(state.modal.is_none());

    let _ = app::update(&mut state, Message::Backup(BackupMessage::Refresh));
    assert_eq!(state.backups, ["20261009-180000"]);
    assert_eq!(state.backup_choice, "20261009-180000");
    // An idle game does not block it; `ui_play.rs` covers a running one.
    assert!(!game::restore_blocked(&state));

    let _ = app::update(&mut state, Message::Backup(BackupMessage::Restore));
    let asked = state.modal.clone().unwrap();
    assert!(matches!(
        &asked.kind,
        ModalKind::YesNo {
            yes: Message::Backup(BackupMessage::RestoreConfirmed),
            no: Message::Modal(ModalMessage::Dismiss)
        }
    ));
    assert_eq!(
        asked.text,
        "Replace the current saves with the backup 20261009-180000? The current saves are backed up first."
    );
    // Nothing was restored yet.
    assert_eq!(
        std::fs::read(user.join("savedata/data.bin")).unwrap(),
        b"new"
    );

    // Yes closes the dialog and starts the restore in the background.
    let _ = app::update(&mut state, Message::Backup(BackupMessage::RestoreConfirmed));
    assert!(state.modal.is_none());
    // The background work runs on its own thread: wait for the saves to come back.
    let saves = user.join("savedata/data.bin");
    for _ in 0..200 {
        if std::fs::read(&saves).is_ok_and(|bytes| bytes == b"old") {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    assert_eq!(std::fs::read(&saves).unwrap(), b"old");
    let _ = app::update(
        &mut state,
        Message::Backup(BackupMessage::Restored(Ok(Some("20261009-181000".into())))),
    );
    let done = state.modal.clone().unwrap();
    assert!(matches!(done.kind, ModalKind::Info));
    assert_eq!(
        done.text,
        "Saves restored from 20261009-180000. Previous saves: 20261009-181000."
    );
    let _ = app::update(
        &mut state,
        Message::Backup(BackupMessage::Restored(Err("disk full".into()))),
    );
    let failed = state.modal.clone().unwrap();
    assert!(matches!(failed.kind, ModalKind::Error));
    assert_eq!(failed.text, "Could not restore: disk full");
    assert!(matches!(
        game::restore_saves(&user, "nope"),
        Err(text) if text.contains("nope")
    ));
}

#[test]
fn brazilian_portuguese_is_offered_with_its_system_id() {
    let offered = LANGUAGES
        .iter()
        .find(|choice| choice.value == "17")
        .unwrap();
    assert_eq!(offered.en, "Portuguese (Brazil)");
}

#[test]
fn choosing_brazilian_portuguese_starts_the_game_with_id_17() {
    // The Language row stores the system id the game is started with (Task 33 passes it on).
    let dir = tempfile::tempdir().unwrap();
    let mut state = state(dir.path());
    let pick = select_message("language", LANGUAGES, "en", "Portuguese (Brazil)").unwrap();
    let _ = app::update(&mut state, pick);
    assert_eq!(state.settings.get_str("language"), "17");
}

#[test]
fn brazilian_portuguese_label_is_translated_in_portuguese_and_spanish() {
    assert_eq!(tr("pt", "Portuguese (Brazil)"), "Português (Brasil)");
    assert_eq!(tr("es", "Portuguese (Brazil)"), "Portugués (Brasil)");
    assert_eq!(tr("id", "Portuguese (Brazil)"), "Portugis (Brasil)");
}

#[test]
fn the_note_names_the_game_menus() {
    let note = game::LANGUAGE_NOTE;
    assert!(note.contains("Options → Game Settings → Game Text"));
    assert!(note.contains("Options → Audio → Speech"));
    assert!(note.starts_with("Used on the game's first start."));
}

#[test]
fn the_note_uses_the_game_menu_names_of_each_language() {
    let names = [
        ("pt", "Texto de jogo", "Falas"),
        ("es", "Texto del juego", "Diálogos"),
        ("pl", "Tekst gry", "Dialogi"),
        ("fr", "Texte du jeu", "Voix"),
        ("de", "Spieltext", "Sprachausgabe"),
        ("it", "Testo di gioco", "Dialoghi"),
    ];
    for (language, text, speech) in names {
        let translated = tr(language, game::LANGUAGE_NOTE);
        assert!(translated.contains(text), "{language}: {translated}");
        assert!(translated.contains(speech), "{language}: {translated}");
    }
}

// --- Trophies page (Task 27) ---

use spartanport_launcher::core::i18n::trophy_text;
use spartanport_launcher::core::trophies::TROPHY_COUNT;
use spartanport_launcher::ui::pages::trophies::{Filter, TrophiesMessage};

fn tall_has(state: &app::State, label: &str) -> bool {
    let mut ui = iced_test::simulator::Simulator::with_size(
        iced::Settings::default(),
        iced::Size::new(1280.0, 6000.0),
        app::view(state),
    );
    ui.find(label).is_ok()
}

#[test]
fn trophies_page_lists_35_cards_from_a_test_package() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state_with_trophies(dir.path());
    open(&mut state, Page::Trophies);
    load_trophies(&mut state);
    assert_eq!(state.trophies.icons.len(), TROPHY_COUNT);
    assert_eq!(state.trophies.locked_icons.len(), TROPHY_COUNT);
    assert_eq!(state.trophies.tiers.len(), TROPHY_COUNT);
    assert_eq!(
        state.trophies.unlocked.iter().copied().collect::<Vec<_>>(),
        [0, 3]
    );
    assert!(!state.trophies.loading);
    assert!(tall_has(&state, "2 of 35 unlocked"));
    assert!(tall_has(&state, &trophy_text("en", 0).0));
    assert!(tall_has(&state, &trophy_text("en", TROPHY_COUNT - 1).0));
    assert!(tall_has(
        &state,
        "Only trophies unlocked with this port are listed; the ones earned on a PlayStation are not known to it."
    ));
}

#[test]
fn trophies_page_without_a_package_says_so() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state(dir.path());
    state.lang = "en".into();
    open(&mut state, Page::Trophies);
    load_trophies(&mut state);
    assert!(state.trophies.icons.is_empty());
    assert!(tall_has(
        &state,
        "No trophy list: choose the game folder on the Game page (its sce_sys/trophy/trophy00.trp is read)."
    ));
}

#[test]
fn filter_unlocked_hides_locked() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state_with_trophies(dir.path());
    open(&mut state, Page::Trophies);
    load_trophies(&mut state);
    let (first, second, other) = (
        trophy_text("en", 0).0,
        trophy_text("en", 3).0,
        trophy_text("en", 1).0,
    );
    let _ = app::update(
        &mut state,
        Message::Trophies(TrophiesMessage::Filter(Filter::Unlocked)),
    );
    assert!(tall_has(&state, &first) && tall_has(&state, &second));
    assert!(!tall_has(&state, &other));
    let _ = app::update(
        &mut state,
        Message::Trophies(TrophiesMessage::Filter(Filter::Locked)),
    );
    assert!(!tall_has(&state, &first) && !tall_has(&state, &second));
    assert!(tall_has(&state, &other));
    // A reload keeps the chosen filter.
    load_trophies(&mut state);
    assert_eq!(state.trophies.filter, Filter::Locked);
}

#[test]
fn reset_asks_and_relocks() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state_with_trophies(dir.path());
    open(&mut state, Page::Trophies);
    // Nothing unlocked is known before the first load.
    let _ = app::update(&mut state, Message::Trophies(TrophiesMessage::Reset));
    let none = state.modal.clone().unwrap();
    assert!(matches!(none.kind, ModalKind::Info));
    assert_eq!(none.text, "No trophy is unlocked.");

    load_trophies(&mut state);
    let _ = app::update(&mut state, Message::Trophies(TrophiesMessage::Reset));
    let asked = state.modal.clone().unwrap();
    assert!(matches!(
        &asked.kind,
        ModalKind::YesNo {
            yes: Message::Trophies(TrophiesMessage::ResetConfirmed),
            no: Message::Modal(ModalMessage::Dismiss)
        }
    ));
    assert_eq!(
        asked.text,
        "Lock all 2 unlocked trophies again? A copy is kept as trophies.dat.bak."
    );
    let dat = state.paths.user_dir("").join("trophies.dat");
    assert!(dat.is_file());

    let _ = app::update(
        &mut state,
        Message::Trophies(TrophiesMessage::ResetConfirmed),
    );
    assert!(state.modal.is_none());
    // The reset runs on its own thread: wait for the file to move.
    for _ in 0..200 {
        if !dat.exists() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    assert!(!dat.exists());
    assert!(dat.with_file_name("trophies.dat.bak").is_file());
    let _ = app::update(
        &mut state,
        Message::Trophies(TrophiesMessage::ResetDone(Ok(2))),
    );
    load_trophies(&mut state);
    assert!(state.trophies.unlocked.is_empty());
    assert!(tall_has(&state, "0 of 35 unlocked"));

    let _ = app::update(
        &mut state,
        Message::Trophies(TrophiesMessage::ResetDone(Err("disk full".into()))),
    );
    let failed = state.modal.clone().unwrap();
    assert!(matches!(failed.kind, ModalKind::Error));
    assert_eq!(failed.text, "Could not reset the trophies: disk full");
}

#[test]
fn opening_the_page_and_the_buttons_start_a_load() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state(dir.path());
    state.lang = "en".into();
    open(&mut state, Page::Trophies);
    assert!(state.trophies.loading);
    load_trophies(&mut state);
    assert!(!state.trophies.loading);
    click(&mut state, "Refresh");
    assert!(state.trophies.loading);
    assert!(messages(&state, "Reset trophies")
        .iter()
        .any(|m| matches!(m, Message::Trophies(TrophiesMessage::Reset))));
}

// --- Controls page (Task 28) ---

use iced::keyboard::key::Named;
use iced::keyboard::{Key, Location, Modifiers};
use iced::mouse;
use spartanport_launcher::core::controls::{Kind, PAD_STYLES};
use spartanport_launcher::ui::pages::controls::{
    self, ControlsMessage, ControlsView, Device, MouseInput, PadChoice, CAPTURE_SECONDS,
};

fn controls_update(state: &mut app::State, message: ControlsMessage) {
    let _ = app::update(state, Message::Controls(message));
}

/// Clicks the key box of `button`: the page now waits for an input for it.
fn capture(state: &mut app::State, button: &str) {
    controls_update(state, ControlsMessage::Capture(button.into()));
}

/// A key press as the keyboard subscription delivers it.
fn press_key(state: &mut app::State, key: Key, location: Location) {
    let _ = app::update(state, Message::Key(key, Modifiers::default(), location));
}

fn char_key(text: &str) -> Key {
    Key::Character(text.into())
}

/// Like `messages`, in a window tall enough to show the whole page (the drawer scrolls).
fn tall_messages(state: &app::State, label: &str) -> Vec<Message> {
    let mut ui = iced_test::simulator::Simulator::with_size(
        iced::Settings::default(),
        iced::Size::new(1280.0, 4000.0),
        app::view(state),
    );
    ui.click(label)
        .unwrap_or_else(|error| panic!("click {label:?}: {error:?}"));
    ui.into_messages().collect()
}

fn controls_state(dir: &std::path::Path) -> app::State {
    let mut state = state(dir);
    state.lang = "en".into();
    open(&mut state, Page::Controls);
    state
}

#[test]
fn pressed_keys_become_sdl_names() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = controls_state(dir.path());
    // The Tk keysyms of the Python test (q, Q, 7, F12, space, Shift_L, Control_L, KP_5,
    // BackSpace) as iced reports those keys, then a few more of the mapping.
    let presses = [
        (char_key("q"), Location::Standard, "Q"),
        (char_key("Q"), Location::Standard, "Q"),
        (char_key("7"), Location::Standard, "7"),
        (Key::Named(Named::F12), Location::Standard, "F12"),
        (Key::Named(Named::Space), Location::Standard, "Space"),
        (Key::Named(Named::Shift), Location::Left, "Left Shift"),
        (Key::Named(Named::Control), Location::Left, "Left Ctrl"),
        (char_key("5"), Location::Numpad, "Keypad 5"),
        (
            Key::Named(Named::Backspace),
            Location::Standard,
            "Backspace",
        ),
        (Key::Named(Named::Enter), Location::Standard, "Return"),
        (Key::Named(Named::Enter), Location::Numpad, "Keypad Enter"),
        (Key::Named(Named::Alt), Location::Right, "Right Alt"),
        (Key::Named(Named::Escape), Location::Standard, "Escape"),
        (Key::Named(Named::ArrowLeft), Location::Standard, "Left"),
        (
            Key::Named(Named::ContextMenu),
            Location::Standard,
            "Application",
        ),
        (Key::Named(Named::NumLock), Location::Numpad, "Numlock"),
        // The numpad with Num Lock off reports its other names; SDL sees the same keys.
        (Key::Named(Named::Home), Location::Numpad, "Keypad 7"),
        (Key::Named(Named::Delete), Location::Numpad, "Keypad ."),
        (char_key("="), Location::Standard, "Equals"),
        (char_key(","), Location::Standard, "Comma"),
        (char_key("-"), Location::Standard, "-"),
        (Key::Named(Named::F24), Location::Standard, "F24"),
    ];
    for (key, location, name) in presses {
        capture(&mut state, "cross");
        assert_eq!(state.controls.capturing.as_deref(), Some("cross"));
        press_key(&mut state, key.clone(), location);
        assert_eq!(state.controls.key["cross"], name, "{key:?} {location:?}");
        assert!(state.controls.capturing.is_none(), "{key:?}");
    }
    // While no box waits, the keys navigate as before.
    press_key(&mut state, Key::Named(Named::ArrowLeft), Location::Standard);
    assert_eq!(state.page, Page::Trophies);
}

#[test]
fn keys_that_cannot_be_bound() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = controls_state(dir.path());
    let before = state.controls.key.clone();
    // Insert, Win_L, Win_R, Print, F25 and eacute of the Python test.
    let presses = [
        (Key::Named(Named::Insert), Location::Standard),
        (Key::Named(Named::Super), Location::Left),
        (Key::Named(Named::Super), Location::Right),
        (Key::Named(Named::PrintScreen), Location::Standard),
        (Key::Named(Named::F25), Location::Standard),
        (char_key("é"), Location::Standard),
    ];
    capture(&mut state, "cross");
    for (key, location) in presses {
        press_key(&mut state, key.clone(), location);
        // The box keeps waiting and nothing changed.
        assert_eq!(
            state.controls.capturing.as_deref(),
            Some("cross"),
            "{key:?}"
        );
        assert_eq!(state.controls.key, before, "{key:?}");
    }
    assert_eq!(state.page, Page::Controls);
}

#[test]
fn binding_a_used_key_swaps_the_buttons_in_the_view() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = controls_state(dir.path());
    // Nobody has F by default: Square takes it.
    capture(&mut state, "square");
    press_key(&mut state, char_key("f"), Location::Standard);
    assert_eq!(state.controls.key["square"], "F");
    // Cross's box shows Space; clicking it waits for an input for Cross.
    click_tall(&mut state, "Space");
    assert_eq!(state.controls.capturing.as_deref(), Some("cross"));
    assert!(tall_has(&state, "Press a key or mouse (5)"));
    press_key(&mut state, char_key("f"), Location::Standard);
    assert_eq!(state.controls.key["cross"], "F");
    assert_eq!(state.controls.key["square"], "Space");
    // The rendered boxes swapped too: F is Cross's box, Space is Square's.
    assert!(matches!(
        tall_messages(&state, "F").as_slice(),
        [Message::Controls(ControlsMessage::Capture(button))] if button == "cross"
    ));
    assert!(matches!(
        tall_messages(&state, "Space").as_slice(),
        [Message::Controls(ControlsMessage::Capture(button))] if button == "square"
    ));
    let rows = controls::key_rows(&state, "en");
    let input = |button: &str| {
        rows.iter()
            .find(|row| row.button == button)
            .unwrap()
            .input
            .clone()
    };
    assert_eq!(
        (input("cross"), input("square")),
        ("F".into(), "Space".into())
    );
}

#[test]
fn device_is_its_own_choice() {
    assert_eq!(Device::ALL.map(Device::value), ["keyboard", "gamepad"]);
    let styles: Vec<&str> = PAD_STYLES.iter().map(|(style, _)| *style).collect();
    assert_eq!(styles, ["playstation", "xbox", "nintendo"]);

    let dir = tempfile::tempdir().unwrap();
    let mut state = controls_state(dir.path());
    // The first open looks for a gamepad once.
    assert!(state.controls.gamepad_probe_started);
    assert_eq!(state.controls.device, Device::Keyboard);
    assert!(!state.controls.device_chosen);
    // A gamepad found shows its table, but an untouched first choice stays automatic.
    controls_update(&mut state, ControlsMessage::GamepadDetected(true));
    assert_eq!(state.controls.device, Device::Gamepad);
    assert!(!state.controls.device_chosen);
    assert!(state.settings.get("controls_device").is_none());
    assert!(tall_has(&state, "GAMEPAD MAPPING"));
    assert!(!tall_has(&state, "KEYBOARD & MOUSE MAPPING"));

    // The device radio only chooses the table shown; the tables keep their inputs.
    capture(&mut state, "cross");
    press_key(&mut state, char_key("g"), Location::Standard);
    let (key, pad) = (state.controls.key.clone(), state.controls.pad.clone());
    click(&mut state, "Keyboard & mouse");
    assert_eq!(state.controls.device, Device::Keyboard);
    assert!(state.controls.device_chosen);
    assert_eq!(state.settings.get_str("controls_device"), "keyboard");
    assert!(tall_has(&state, "KEYBOARD & MOUSE MAPPING"));
    assert!(!tall_has(&state, "GAMEPAD MAPPING"));
    assert_eq!((&state.controls.key, &state.controls.pad), (&key, &pad));
    // Once chosen, a gamepad found later does not switch the page.
    controls_update(&mut state, ControlsMessage::GamepadDetected(true));
    assert_eq!(state.controls.device, Device::Keyboard);
    click(&mut state, "Gamepad");
    assert_eq!(state.controls.device, Device::Gamepad);
    assert_eq!(state.settings.get_str("controls_device"), "gamepad");
    assert_eq!((&state.controls.key, &state.controls.pad), (&key, &pad));
    // The saved choice comes back and the probe does not run.
    let again = ControlsView::new(&state.settings);
    assert_eq!(again.device, Device::Gamepad);
    assert!(again.device_chosen);
    let mut reopened = state;
    reopened.controls = again;
    open(&mut reopened, Page::Controls);
    assert!(!reopened.controls.gamepad_probe_started);
}

#[test]
fn nintendo_names_follow_the_button_positions() {
    let names = controls::pad_labels("nintendo", "en");
    let label = |sdl: &str| {
        names
            .iter()
            .find(|(name, _)| name == sdl)
            .unwrap()
            .1
            .clone()
    };
    assert_eq!(
        (label("a"), label("b"), label("x"), label("y")),
        ("B".into(), "A".into(), "Y".into(), "X".into())
    );
    // A label maps back to one button.
    let unique: std::collections::BTreeSet<&String> = names.iter().map(|(_, l)| l).collect();
    assert_eq!(unique.len(), names.len());

    let dir = tempfile::tempdir().unwrap();
    let mut state = controls_state(dir.path());
    click(&mut state, "Gamepad");
    assert_eq!(state.controls.pad_style, "playstation");
    click_tall(&mut state, "Nintendo");
    assert_eq!(state.controls.pad_style, "nintendo");
    assert_eq!(state.settings.get_str("pad_style"), "nintendo");
    // The table relabels its boxes; L2, R2 and Touchpad left keep their fixed text.
    let rows = controls::pad_rows(&state, "en");
    let row = |button: &str| {
        rows.iter()
            .find(|row| row.button == button)
            .unwrap()
            .clone()
    };
    let PadChoice::Pick { pairs, current } = row("cross").choice else {
        panic!("cross has a box");
    };
    assert_eq!(current, "B");
    assert_eq!(pairs, controls::pad_labels("nintendo", "en"));
    assert!(matches!(row("circle").choice, PadChoice::Pick { current, .. } if current == "A"));
    assert!(matches!(row("l2").choice, PadChoice::Fixed(text) if text == "L2 trigger"));
    assert!(
        matches!(row("touchpad_left").choice, PadChoice::Fixed(text) if text == "L1 + Touchpad right")
    );
    page_has(&state, &["L2 trigger", "R2 trigger", "L1 + Touchpad right"]);
    // Choosing a label stores its SDL name: Nintendo's A is the east button.
    let pick = controls::pad_pick("cross", &pairs, "A");
    assert!(matches!(
        &pick,
        Message::Controls(ControlsMessage::PadPick(button, name)) if button == "cross" && name == "b"
    ));
    let _ = app::update(&mut state, pick);
    assert_eq!(state.controls.pad["cross"], "b");
    assert_eq!(state.controls.pad["circle"], "b");
}

#[test]
fn capture_takes_the_mouse_and_gives_up_in_time() {
    let pressed = |button| iced::Event::Mouse(mouse::Event::ButtonPressed(button));
    let wheel = |delta| iced::Event::Mouse(mouse::Event::WheelScrolled { delta });
    let bind = MouseInput::Bind;
    let inputs = [
        (pressed(mouse::Button::Left), Some(bind("Mouse Left"))),
        (pressed(mouse::Button::Middle), Some(bind("Mouse Middle"))),
        (pressed(mouse::Button::Right), Some(bind("Mouse Right"))),
        (pressed(mouse::Button::Back), Some(bind("Mouse Back"))),
        (pressed(mouse::Button::Forward), Some(bind("Mouse Forward"))),
        // A button without an input name: the box gives up (Python `clicked` -> `finish`).
        (pressed(mouse::Button::Other(8)), Some(MouseInput::GiveUp)),
        (
            wheel(mouse::ScrollDelta::Lines { x: 0.0, y: 1.0 }),
            Some(bind("Wheel Up")),
        ),
        (
            wheel(mouse::ScrollDelta::Pixels { x: 0.0, y: -40.0 }),
            Some(bind("Wheel Down")),
        ),
        (
            iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
            None,
        ),
    ];
    for (event, name) in inputs {
        assert_eq!(controls::mouse_input(&event), name, "{event:?}");
    }

    let dir = tempfile::tempdir().unwrap();
    let mut state = controls_state(dir.path());
    capture(&mut state, "cross");
    controls_update(
        &mut state,
        ControlsMessage::Captured("cross".into(), "Wheel Up".into()),
    );
    assert_eq!(state.controls.key["cross"], "Wheel Up");
    assert!(state.controls.capturing.is_none());
    // An input that arrives when no box waits binds nothing.
    controls_update(
        &mut state,
        ControlsMessage::Captured("cross".into(), "Mouse Left".into()),
    );
    assert_eq!(state.controls.key["cross"], "Wheel Up");

    // The box counts down and gives up after CAPTURE_SECONDS.
    assert_eq!(CAPTURE_SECONDS, 5);
    capture(&mut state, "circle");
    let started = state.controls.capture_started.unwrap();
    let _ = app::update(
        &mut state,
        Message::Tick(started + std::time::Duration::from_millis(4900)),
    );
    assert_eq!(state.controls.capturing.as_deref(), Some("circle"));
    assert!(tall_has(&state, "Press a key or mouse (1)"));
    // While it waits, a click anywhere is only its input (the mouse subscription delivers it):
    // the button, the box or the tab under the cursor does not react.
    for label in ["Reset keyboard & mouse to defaults", "Wheel Up", "Graphics"] {
        let sent = tall_messages(&state, label);
        assert!(
            sent.iter().all(|message| matches!(message, Message::Noop)),
            "{label}: {sent:?}"
        );
    }
    let _ = app::update(
        &mut state,
        Message::Tick(started + std::time::Duration::from_secs(CAPTURE_SECONDS)),
    );
    assert!(state.controls.capturing.is_none());
    assert_eq!(state.controls.key["circle"], "Left Shift");
    assert!(tall_has(&state, "Left Shift"));
}

#[test]
fn reset_buttons_and_the_rage_rows() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = controls_state(dir.path());
    let defaults = state.controls.key.clone();
    assert_eq!(defaults["square"], "Mouse Left");
    assert_eq!(defaults["rage"], "Mouse Middle");
    // The arrow puts one button back on its default, swapping with the button that had it.
    capture(&mut state, "cross");
    press_key(&mut state, char_key("h"), Location::Standard);
    assert_eq!(state.controls.key["cross"], "H");
    controls_update(&mut state, ControlsMessage::ResetKey("cross".into()));
    assert_eq!(state.controls.key["cross"], "Space");
    // L1 leaves Q, R1 takes it: L1's arrow gives L1 its Q back and R1 the old H.
    capture(&mut state, "l1");
    press_key(&mut state, char_key("h"), Location::Standard);
    capture(&mut state, "r1");
    press_key(&mut state, char_key("q"), Location::Standard);
    controls_update(&mut state, ControlsMessage::ResetKey("l1".into()));
    assert_eq!(state.controls.key["l1"], "Q");
    assert_eq!(state.controls.key["r1"], "H");
    click_tall(&mut state, "Reset keyboard & mouse to defaults");
    assert_eq!(state.controls.key, defaults);

    // Rage of Sparta on one input greys L3 and R3; off, it greys the Rage row.
    let enabled = |state: &app::State, button: &str| {
        controls::key_rows(state, "en")
            .into_iter()
            .find(|row| row.button == button)
            .unwrap()
            .enabled
    };
    assert!(state.settings.get_bool("rage_one_input"));
    assert!(!enabled(&state, "l3") && !enabled(&state, "r3") && enabled(&state, "rage"));
    click_tall(&mut state, "Rage of Sparta on one input (L3 + R3 together)");
    assert!(!state.settings.get_bool("rage_one_input"));
    assert!(enabled(&state, "l3") && enabled(&state, "r3") && !enabled(&state, "rage"));
    assert!(enabled(&state, "cross"));

    // The gamepad table goes back to the controller's own buttons.
    controls_update(
        &mut state,
        ControlsMessage::PadPick("cross".into(), "y".into()),
    );
    controls_update(&mut state, ControlsMessage::ResetTable(Kind::Pad));
    assert_eq!(state.controls.pad["cross"], "a");
    assert_eq!(state.controls.pad["l2"], "");
}

/// `state` on the Controls page once the drawer has stopped (a key box's click restarts the clock).
fn settle(state: &mut app::State) {
    let _ = app::update(
        state,
        Message::Tick(iced::time::Instant::now() + std::time::Duration::from_secs(1)),
    );
}

#[test]
fn the_keyboard_reset_works_in_the_window_as_it_opens() {
    // Item 8 (adjustments 4): "Reset keyboard & mouse to defaults does nothing". At 1280 × 800,
    // with the page scrolled down to the button, a click puts a saved change back on its default
    // and Save turns on (the user's own files held no changed key: nothing to reset there).
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("settings.json"),
        r#"{"controls": {"key": {"cross": "H"}}}"#,
    )
    .unwrap();
    let mut state = controls_state(dir.path());
    settle(&mut state);
    assert_eq!(state.controls.key["cross"], "H");
    assert!(!spartanport_launcher::ui::persist::dirty(&state));
    let mut ui = iced_test::simulator::Simulator::with_size(
        iced::Settings::default(),
        iced::Size::new(1280.0, 800.0),
        app::view(&state),
    );
    ui.point_at(iced::Point::new(300.0, 500.0));
    let _ = ui.simulate([iced::Event::Mouse(mouse::Event::WheelScrolled {
        delta: mouse::ScrollDelta::Pixels { x: 0.0, y: -700.0 },
    })]);
    ui.click("Reset keyboard & mouse to defaults").unwrap();
    let sent: Vec<Message> = ui.into_messages().collect();
    for message in sent {
        let _ = app::update(&mut state, message);
    }
    assert_eq!(state.controls.key["cross"], "Space");
    assert!(spartanport_launcher::ui::persist::dirty(&state));
}

#[test]
fn the_reset_buttons_are_active_only_off_the_defaults() {
    // On the defaults a reset has nothing to change: its button is off, as Save is with nothing
    // to save, so a click that changes rows out of sight still shows that it acted.
    let dir = tempfile::tempdir().unwrap();
    let mut state = controls_state(dir.path());
    let resets = |state: &app::State, label: &str| {
        tall_messages(state, label)
            .into_iter()
            .filter(|message| matches!(message, Message::Controls(ControlsMessage::ResetTable(_))))
            .count()
    };
    assert_eq!(resets(&state, "Reset keyboard & mouse to defaults"), 0);
    capture(&mut state, "cross");
    press_key(&mut state, char_key("h"), Location::Standard);
    assert_eq!(resets(&state, "Reset keyboard & mouse to defaults"), 1);
    click_tall(&mut state, "Reset keyboard & mouse to defaults");
    assert_eq!(resets(&state, "Reset keyboard & mouse to defaults"), 0);

    controls_update(&mut state, ControlsMessage::Device(Device::Gamepad));
    assert_eq!(resets(&state, "Reset gamepad to defaults"), 0);
    controls_update(
        &mut state,
        ControlsMessage::PadPick("cross".into(), "y".into()),
    );
    assert_eq!(resets(&state, "Reset gamepad to defaults"), 1);
}

/// The vertical offsets of the scrollables of `ui`.
fn scroll_offsets(
    ui: &mut iced_test::runtime::UserInterface<'_, Message, iced::Theme, iced::Renderer>,
    renderer: &iced::Renderer,
) -> Vec<f32> {
    use iced::advanced::widget::operation::{Operation, Scrollable};
    struct Offsets(Vec<f32>);
    impl Operation for Offsets {
        fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
            operate(self);
        }
        fn scrollable(
            &mut self,
            _id: Option<&iced::widget::Id>,
            _bounds: iced::Rectangle,
            _content_bounds: iced::Rectangle,
            translation: iced::Vector,
            _state: &mut dyn Scrollable,
        ) {
            self.0.push(translation.y);
        }
    }
    let mut offsets = Offsets(Vec::new());
    ui.operate(renderer, &mut offsets);
    offsets.0
}

#[test]
fn a_waiting_key_box_keeps_the_page_where_it_was_scrolled() {
    // Found while looking at item 8: clicking a key box low on the page threw the page back to
    // its top (the waiting layer changed the window's root from a column to a stack, which drops
    // the state of everything under it), so the waiting box and the reset button went out of
    // sight; the end of the wait threw it to the top again.
    use iced::advanced::renderer::Headless;
    use iced_test::runtime::user_interface::{Cache, UserInterface};
    let dir = tempfile::tempdir().unwrap();
    let mut state = controls_state(dir.path());
    settle(&mut state);
    let mut renderer = iced::futures::executor::block_on(<iced::Renderer as Headless>::new(
        iced::Font::with_name("Fira Sans"),
        iced::Pixels(16.0),
        Some("tiny-skia"),
    ))
    .unwrap();
    let size = iced::Size::new(1280.0, 800.0);
    let cursor = mouse::Cursor::Available(iced::Point::new(300.0, 500.0));
    let wheel = iced::Event::Mouse(mouse::Event::WheelScrolled {
        delta: mouse::ScrollDelta::Pixels { x: 0.0, y: -600.0 },
    });
    let mut ui = UserInterface::build(app::view(&state), size, Cache::default(), &mut renderer);
    let mut sent = Vec::new();
    let _ = ui.update(
        &[wheel],
        cursor,
        &mut renderer,
        &mut iced::advanced::clipboard::Null,
        &mut sent,
    );
    let scrolled = scroll_offsets(&mut ui, &renderer);
    assert!(scrolled.iter().any(|y| *y > 100.0), "{scrolled:?}");
    let mut cache = ui.into_cache();
    // The box waits, then gets its key.
    for step in 0..2 {
        if step == 0 {
            capture(&mut state, "dodge");
        } else {
            press_key(&mut state, char_key("h"), Location::Standard);
        }
        settle(&mut state);
        let mut ui = UserInterface::build(app::view(&state), size, cache, &mut renderer);
        assert_eq!(scroll_offsets(&mut ui, &renderer), scrolled, "step {step}");
        cache = ui.into_cache();
    }
}

#[test]
fn a_gamepad_found_while_a_box_waits_ends_the_capture() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = controls_state(dir.path());
    let before = state.controls.key.clone();
    // The player clicks a key box before the first-open probe answers.
    capture(&mut state, "cross");
    controls_update(&mut state, ControlsMessage::GamepadDetected(true));
    assert_eq!(state.controls.device, Device::Gamepad);
    assert!(state.controls.capturing.is_none());
    assert!(state.controls.capture_started.is_none());
    assert_eq!(state.controls.key, before);
    // No hidden box takes the next click: it reaches the gamepad section.
    assert!(matches!(
        tall_messages(&state, "Nintendo").as_slice(),
        [Message::Controls(ControlsMessage::PadStyle(style))] if style == "nintendo"
    ));
}

#[test]
fn losing_the_focus_ends_the_capture() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = controls_state(dir.path());
    let before = state.controls.key.clone();
    capture(&mut state, "cross");
    let _ = app::update(&mut state, Message::WindowFocused(false));
    assert!(!state.focused);
    assert!(state.controls.capturing.is_none());
    assert!(state.controls.capture_started.is_none());
    assert_eq!(state.controls.key, before);
    // The next key navigates again instead of binding.
    let _ = app::update(&mut state, Message::WindowFocused(true));
    press_key(&mut state, Key::Named(Named::ArrowLeft), Location::Standard);
    assert_eq!(state.page, Page::Trophies);
    assert_eq!(state.controls.key, before);
}

// --- Mods page (Task 29) ---

use spartanport_launcher::ui::pages::mods::ModsMessage;

fn mods_state(dir: &std::path::Path) -> app::State {
    let mut state = state(dir);
    state.lang = "en".into();
    state
}

fn mods_update(state: &mut app::State, message: ModsMessage) {
    let _ = app::update(state, Message::Mods(message));
}

#[test]
fn mods_list_follows_the_folder_and_saved_order() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = mods_state(dir.path());
    for name in ["a", "b", "c"] {
        make_mod(&state, name, &format!("{name}.bin"));
    }
    std::fs::write(
        state.paths.mods_json(),
        r#"{"order": ["b","a"], "disabled": ["a"]}"#,
    )
    .unwrap();
    load_mods(&mut state);
    assert_eq!(state.mods.order, ["b", "a", "c"]);
    assert!(state.mods.enabled["b"]);
    assert!(!state.mods.enabled["a"]);
    assert!(state.mods.enabled["c"]);
    open(&mut state, Page::Mods);
    assert!(tall_has(&state, "2 of 3 active"));
}

#[test]
fn the_load_mods_box_sits_beside_its_name() {
    // The user's screenshot (R94): the box of "Load mods" sat in the middle of the row. A row with a
    // check box and an action keeps the box beside its name and the button at the row's end.
    for lang in ["en", "pt", "ar"] {
        let dir = tempfile::tempdir().unwrap();
        let mut state = mods_state(dir.path());
        state.lang = lang.into();
        state.rtl = lang == "ar";
        make_mod(&state, "a", "a.bin");
        load_mods(&mut state);
        let name = tr(lang, "Load mods");
        let mut ui = settled(&mut state, Page::Mods, 1280.0, 800.0);
        let boxed = control(&mut ui, &name);
        let label = ui.find(name.as_str()).unwrap().bounds();
        let open = ui.find(tr(lang, "Open folder").as_str()).unwrap().bounds();
        let gap = if lang == "ar" {
            label.x - (boxed.x + boxed.width)
        } else {
            boxed.x - (label.x + label.width)
        };
        assert!(
            (0.0..=16.0).contains(&gap),
            "{lang}: the box {boxed:?} is {gap} from its name {label:?}"
        );
        if lang == "ar" {
            assert!(open.x < boxed.x, "{lang}: {open:?} vs {boxed:?}");
        } else {
            // Refresh ends the Load order heading at the page's edge; both pad their label by 10.
            let refresh = ui.find(tr(lang, "Refresh").as_str()).unwrap().bounds();
            assert!(
                (open.x + open.width - (refresh.x + refresh.width)).abs() < 0.5,
                "{lang}: Open folder {open:?}, Refresh {refresh:?}"
            );
        }
    }
}

#[test]
fn moving_a_mod_changes_the_order() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = mods_state(dir.path());
    for name in ["a", "b", "c"] {
        make_mod(&state, name, &format!("{name}.bin"));
    }
    load_mods(&mut state);
    assert_eq!(state.mods.order, ["a", "b", "c"]);
    mods_update(&mut state, ModsMessage::Move(0, 1));
    assert_eq!(state.mods.order, ["b", "a", "c"]);
    // Past either end nothing moves.
    mods_update(&mut state, ModsMessage::Move(0, -1));
    mods_update(&mut state, ModsMessage::Move(2, 1));
    assert_eq!(state.mods.order, ["b", "a", "c"]);
}

#[test]
fn conflict_summary_names_the_winner() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = mods_state(dir.path());
    make_mod(&state, "alpha", "same.bin");
    make_mod(&state, "beta", "same.bin");
    load_mods(&mut state);
    let found = spartanport_launcher::core::mods::conflicts(
        &state.paths.mods_dir(""),
        &state.mods.order,
        None,
    );
    let names = state.mods.order.clone();
    mods_update(&mut state, ModsMessage::Conflicts(names, found));
    open(&mut state, Page::Mods);
    assert!(tall_has(
        &state,
        "⚠ 1 file is changed by two mods. beta wins."
    ));
    // The lower mod moves up: the other one wins.
    mods_update(&mut state, ModsMessage::Move(1, -1));
    assert_eq!(state.mods.order, ["beta", "alpha"]);
    let found = spartanport_launcher::core::mods::conflicts(
        &state.paths.mods_dir(""),
        &state.mods.order,
        None,
    );
    let names = state.mods.order.clone();
    mods_update(&mut state, ModsMessage::Conflicts(names, found));
    assert!(tall_has(
        &state,
        "⚠ 1 file is changed by two mods. alpha wins."
    ));
}

#[test]
fn a_corrupt_presets_file_is_listed_empty_with_a_warning() {
    const WARNING: &str = "mods_presets.json could not be read, so no preset is listed and the file is left as it is. Fix or delete it to save presets again.";
    let dir = tempfile::tempdir().unwrap();
    let mut state = mods_state(dir.path());
    make_mod(&state, "a", "a.bin");
    std::fs::write(state.paths.mod_presets(), "{oops").unwrap();
    load_mods(&mut state);
    open(&mut state, Page::Mods);
    assert!(state.mods.presets.is_empty());
    assert!(tall_has(&state, WARNING));
    // Saving or deleting a preset would replace it: the file is kept and the warning shows.
    mods_update(&mut state, ModsMessage::PresetName("x".into()));
    for message in [ModsMessage::SavePreset, ModsMessage::DeletePreset] {
        mods_update(&mut state, message);
        let shown = state.modal.take().expect("a warning");
        assert!(matches!(shown.kind, ModalKind::Error), "{shown:?}");
        assert_eq!(shown.text, WARNING);
        assert_eq!(std::fs::read(state.paths.mod_presets()).unwrap(), b"{oops");
    }
}

#[test]
fn preset_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = mods_state(dir.path());
    for name in ["a", "b"] {
        make_mod(&state, name, &format!("{name}.bin"));
    }
    load_mods(&mut state);
    // No name: an Info dialog and no file.
    mods_update(&mut state, ModsMessage::SavePreset);
    let modal = state.modal.take().unwrap();
    assert_eq!(modal.text, "Type a name for the preset first.");
    assert!(!state.paths.mod_presets().exists());

    mods_update(&mut state, ModsMessage::Toggle("a".into()));
    mods_update(&mut state, ModsMessage::Move(0, 1));
    mods_update(&mut state, ModsMessage::PresetName("x".into()));
    mods_update(&mut state, ModsMessage::SavePreset);
    let saved: serde_json::Value =
        serde_json::from_slice(&std::fs::read(state.paths.mod_presets()).unwrap()).unwrap();
    assert_eq!(
        saved,
        serde_json::json!({"x": {"order": ["b", "a"], "disabled": ["a"]}})
    );
    assert_eq!(state.mods.presets, ["x"]);

    mods_update(&mut state, ModsMessage::Toggle("a".into()));
    mods_update(&mut state, ModsMessage::Move(0, 1));
    assert_eq!(state.mods.order, ["a", "b"]);
    assert!(state.mods.enabled["a"]);
    mods_update(&mut state, ModsMessage::LoadPreset);
    assert_eq!(state.mods.order, ["b", "a"]);
    assert!(!state.mods.enabled["a"]);

    mods_update(&mut state, ModsMessage::DeletePreset);
    let saved: serde_json::Value =
        serde_json::from_slice(&std::fs::read(state.paths.mod_presets()).unwrap()).unwrap();
    assert_eq!(saved, serde_json::json!({}));
    assert!(state.mods.presets.is_empty());
    assert!(state.mods.preset_name.is_empty());
}

#[test]
fn the_list_follows_the_mods() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = mods_state(dir.path());
    performance::refresh_mod_resolutions(&mut state);
    let builtin = performance::resolution_options(&state).len();
    // No mod yet.
    load_mods(&mut state);
    assert_eq!(performance::resolution_options(&state).len(), builtin);
    // The Mods page lists the new folder.
    let patches = state.paths.mods_dir("").join("Ultrawide").join("patches");
    std::fs::create_dir_all(&patches).unwrap();
    write_mod_patch(&patches, "3840x1080");
    load_mods(&mut state);
    let options = performance::resolution_options(&state);
    assert_eq!(options.len(), builtin + 1);
    let label = options.iter().find(|(v, _)| v == "3840x1080").unwrap();
    assert_eq!(label.1, "3840 × 1080 · 32:9 (mod)");
    // Unticked on the Mods page.
    mods_update(&mut state, ModsMessage::Toggle("Ultrawide".into()));
    assert_eq!(performance::resolution_options(&state).len(), builtin);
    mods_update(&mut state, ModsMessage::Toggle("Ultrawide".into()));
    assert_eq!(performance::resolution_options(&state).len(), builtin + 1);
    // "Load mods" off.
    let _ = app::update(
        &mut state,
        Message::Set("mods_enabled".into(), false.into()),
    );
    assert_eq!(performance::resolution_options(&state).len(), builtin);
}

/// Copies the folder `from` to `to`, files and subfolders.
fn copy_tree(from: &std::path::Path, to: &std::path::Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

#[test]
fn the_bundled_ultrawide_mod_lists_with_its_patches() {
    // Item 7 (adjustments 4): the repo's `extras/mods/Ultrawide`, put in the mods folder as its
    // README says, lists on the Mods page with its game patches, and its patch file is the one the
    // game merges (Python `enabled_patch_files`). Its sizes are built in since 2.4.0, so the
    // Render resolution list gets no "(mod)" entry from it.
    use spartanport_launcher::core::{mods as core_mods, patches::resolution_names};
    let bundled = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../extras/mods/Ultrawide");
    let dir = tempfile::tempdir().unwrap();
    let mut state = mods_state(dir.path());
    let root = state.paths.mods_dir("");
    copy_tree(&bundled, &root.join("Ultrawide"));
    performance::refresh_mod_resolutions(&mut state);
    let builtin = performance::resolution_options(&state).len();
    load_mods(&mut state);
    assert_eq!(state.mods.order, ["Ultrawide"]);
    assert!(state.mods.enabled["Ultrawide"]);
    assert_eq!(state.mods.facts["Ultrawide"], (0, true));
    let names = core_mods::selected(&root, Some(&state.paths.mods_json()), None).unwrap();
    assert_eq!(names, ["Ultrawide"]);
    let files = core_mods::patch_files(&root.join("Ultrawide"));
    assert_eq!(
        files,
        [root.join("Ultrawide").join("patches").join("Ultrawide.xml")]
    );
    let sizes = resolution_names(&files);
    assert!(sizes.contains(&"3440x1440".to_string()), "{sizes:?}");
    assert_eq!(performance::resolution_options(&state).len(), builtin);
}

// --- Advanced page (Task 30) ---

use spartanport_launcher::core::options::{UI_LANGUAGES, UI_SCALES};
use spartanport_launcher::ui::pages::advanced;
use spartanport_launcher::ui::play::GameState;

fn pick_row(state: &app::State, key: &str, value: &str) -> Message {
    let rows = advanced::rows(state, "en");
    let row = rows.iter().find(|row| row.key == key).unwrap();
    let label = &row.pairs.iter().find(|(v, _)| v == value).unwrap().1;
    let choices = match key {
        "ui_language" => UI_LANGUAGES,
        _ => UI_SCALES,
    };
    select_message(key, choices, "en", label).unwrap()
}

#[test]
fn language_and_scale_rows_change_settings() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state(dir.path());
    state.lang = "en".into();
    let pick = pick_row(&state, "ui_language", "pt");
    let _ = app::update(&mut state, pick);
    assert_eq!(state.settings.get_str("ui_language"), "pt");
    // The interface changes language when the launcher opens again (Save is Task 32).
    assert_eq!(state.lang, "en");
    let pick = pick_row(&state, "ui_scale", "1.5");
    let _ = app::update(&mut state, pick);
    assert_eq!(state.settings.get_str("ui_scale"), "1.5");
    // R90: a file from before shows its closest choice, "auto" as 1x and "2" as 1.5x.
    for (stored, shown) in [("auto", "1"), ("2", "1.5")] {
        state.settings.set("ui_scale", stored.into());
        let rows = advanced::rows(&state, "en");
        let row = rows.iter().find(|row| row.key == "ui_scale").unwrap();
        assert_eq!(row.current, shown, "{stored}");
    }
}

#[test]
fn clear_cache_is_blocked_while_running() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state(dir.path());
    // An idle game does not block it; `ui_play.rs` covers a running one.
    assert!(!advanced::clear_cache_blocked(&state));
    state.game = GameState::Idle;
    let cache = dir.path().join("user").join("cache");
    std::fs::create_dir_all(&cache).unwrap();
    std::fs::write(cache.join("a.bin"), b"x").unwrap();
    assert_eq!(advanced::clear_cache(&dir.path().join("user")), Ok(()));
    assert!(!cache.exists());
    // Nothing to remove is not an error.
    assert_eq!(advanced::clear_cache(&dir.path().join("user")), Ok(()));
    let _ = app::update(&mut state, Message::ShaderCacheCleared(Ok(())));
    let shown = state.modal.take().unwrap();
    assert!(matches!(shown.kind, ModalKind::Info));
    assert!(shown.text.starts_with("Shader cache cleared."));
    let _ = app::update(&mut state, Message::ShaderCacheCleared(Err("busy".into())));
    let shown = state.modal.take().unwrap();
    assert!(matches!(shown.kind, ModalKind::Error));
    assert_eq!(
        shown.text,
        "Could not delete all of the shader cache; another program may be using it: busy"
    );
}

#[test]
fn performance_page_has_the_shader_cache_rows() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state(dir.path());
    state.lang = "en".into();
    open(&mut state, Page::Performance);
    page_has(
        &state,
        &[
            "Reuse compiled shaders from disk  ·  restart",
            "Read the shader cache into memory at start  ·  restart",
            "Show shader notes in the corner  ·  restart",
            "Shader cache",
            "Delete shader cache",
        ],
    );
    assert!(state.settings.get_bool("disk_shaders"));
    assert!(!state.settings.get_bool("shader_notes"));
    // The rows sit below the first screen: a tall window reaches them.
    let mut ui = settled(&mut state, Page::Performance, 1280.0, 4000.0);
    for label in [
        "Reuse compiled shaders from disk  ·  restart",
        "Show shader notes in the corner  ·  restart",
        "Delete shader cache",
    ] {
        ui.click(label)
            .unwrap_or_else(|error| panic!("click {label:?}: {error:?}"));
    }
    let emitted: Vec<Message> = ui.into_messages().collect();
    // The button asks the same question as the Advanced page's.
    assert!(matches!(emitted.last(), Some(Message::ClearShaderCache)));
    for message in emitted.into_iter().take(2) {
        let _ = app::update(&mut state, message);
    }
    assert!(!state.settings.get_bool("disk_shaders"));
    assert!(state.settings.get_bool("shader_notes"));
    state.lang = "pt".into();
    page_has(
        &state,
        &[
            "Reutilizar shaders compilados do disco  ·  reiniciar",
            "Apagar cache de shaders",
        ],
    );
}

#[test]
fn deleting_the_shader_cache_asks_then_deletes() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state(dir.path());
    state.lang = "en".into();
    let user = state.paths.user_dir("");
    let cache = user.join("cache");
    // An empty cache needs no question.
    assert_eq!(advanced::cache_size(&user), 0);
    let _ = app::update(&mut state, Message::ShaderCacheSized(0));
    let shown = state.modal.take().unwrap();
    assert!(matches!(shown.kind, ModalKind::Info));
    assert_eq!(shown.text, "The shader cache is already empty.");
    // 4 KiB deep inside: the question names the size in MB and the folder.
    let deep = cache
        .join("CUSA01623")
        .join("pipeline-binaries")
        .join("shared");
    std::fs::create_dir_all(&deep).unwrap();
    std::fs::write(deep.join("a.bin"), vec![b'x'; 4096]).unwrap();
    assert_eq!(advanced::cache_size(&user), 4096);
    let _ = app::update(&mut state, Message::ShaderCacheSized(4096));
    let asked = state.modal.take().unwrap();
    assert!(matches!(
        &asked.kind,
        ModalKind::YesNo {
            yes: Message::ClearShaderCacheConfirmed,
            no: Message::Modal(ModalMessage::Dismiss)
        }
    ));
    assert_eq!(
        asked.text,
        format!(
            "Delete the shader cache (0 MB in {})? The game compiles the shaders again, so the next start stutters for a few minutes.",
            cache.display()
        )
    );
    assert!(cache.exists());
    // Yes removes it and says so; a folder that stays names itself.
    assert_eq!(advanced::clear_cache(&user), Ok(()));
    assert!(!cache.exists());
    let _ = app::update(&mut state, Message::ShaderCacheCleared(Ok(())));
    let shown = state.modal.take().unwrap();
    assert!(shown.text.starts_with("Shader cache cleared."));
    let _ = app::update(
        &mut state,
        Message::ShaderCacheCleared(Err(cache.display().to_string())),
    );
    let shown = state.modal.take().unwrap();
    assert!(matches!(shown.kind, ModalKind::Error));
    assert_eq!(
        shown.text,
        format!(
            "Could not delete all of the shader cache; another program may be using it: {}",
            cache.display()
        )
    );
    // Whole megabytes, halves to even, as Python's round().
    assert_eq!(advanced::megabytes(2_621_440), 2);
    assert_eq!(advanced::megabytes(1_572_864), 2);
    assert_eq!(advanced::megabytes(3_145_729), 3);
}

#[test]
fn effects_row_toggles_the_fire() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state(dir.path());
    state.lang = "en".into();
    assert!(state.effects);
    open(&mut state, Page::Advanced);
    click(&mut state, "Effects");
    assert_eq!(state.settings.get("launcher_effects"), Some(&false.into()));
    assert!(!state.effects);
    click(&mut state, "Effects");
    assert!(state.effects);
    // Saved off: the next start begins with the fire off.
    std::fs::write(
        dir.path().join("settings.json"),
        r#"{"launcher_effects": false}"#,
    )
    .unwrap();
    let again = common::state(dir.path());
    assert!(!again.effects);
}

#[test]
fn check_updates_row_hidden_without_self_update() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state(dir.path());
    state.self_update = false;
    assert!(!advanced::check_keys(&state).contains(&"check_updates"));
    assert!(advanced::check_keys(&state).contains(&"close_on_play"));
    state.self_update = true;
    assert!(advanced::check_keys(&state).contains(&"check_updates"));
}

#[test]
fn the_launcher_buttons_wrap_instead_of_squeezing() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state(dir.path());
    // Portuguese has the longest labels of the row; 1280 × 800 gives the narrowest drawer. The row
    // of Windows (with Check for updates) on every system.
    state.lang = "pt".into();
    state.self_update = true;
    open(&mut state, Page::Advanced);
    let _ = app::update(
        &mut state,
        Message::Tick(iced::time::Instant::now() + std::time::Duration::from_secs(1)),
    );
    let mut ui = iced_test::simulator::Simulator::with_size(
        iced::Settings::default(),
        iced::Size::new(1280.0, 800.0),
        app::view(&state),
    );
    for label in [tr("pt", "Desktop shortcut"), tr("pt", "Add to Steam")] {
        let shown = ui.find(label.as_str()).unwrap().bounds().width;
        // The buttons' labels set no font: the default one.
        let alone = common::natural_size(&label, None, 16.0).width;
        assert!(shown >= alone - 0.5, "{label}: {shown} wide, needs {alone}");
    }
}

#[test]
fn the_files_buttons_share_one_row_at_its_end() {
    // R93: "Port folder", "spartanport.ini" and "Clear shader cache" on one line, ending where
    // every row's control ends (starting there right to left), each label on one line.
    for lang in [
        "en", "pt", "es", "fr", "de", "it", "pl", "ru", "tr", "ar", "zh", "ja", "ko", "id",
    ] {
        let dir = tempfile::tempdir().unwrap();
        let mut state = state(dir.path());
        state.lang = lang.into();
        state.rtl = lang == "ar";
        let labels = [
            tr(lang, "Port folder"),
            "spartanport.ini".to_string(),
            tr(lang, "Clear shader cache"),
        ];
        let mut ui = settled(&mut state, Page::Advanced, 1280.0, 4000.0);
        let scale = control(&mut ui, &tr(lang, "GUI Scale"));
        let files = control(&mut ui, advanced::FILES_ROW);
        let shown: Vec<iced::Rectangle> = labels
            .iter()
            .map(|label| ui.find(label.as_str()).unwrap().bounds())
            .collect();
        for (label, bounds) in labels.iter().zip(&shown) {
            let alone = common::natural_size(label, None, 16.0);
            assert!(
                (bounds.y - shown[0].y).abs() < 0.5 && bounds.height < alone.height * 1.5,
                "{lang}: {label} at {bounds:?}, the first at {:?}",
                shown[0]
            );
            assert!(
                bounds.x >= files.x && bounds.x + bounds.width <= files.x + files.width + 0.5,
                "{lang}: {label} {bounds:?} outside {files:?}"
            );
        }
        let (edge, row_edge) = if lang == "ar" {
            (files.x, scale.x)
        } else {
            (files.x + files.width, scale.x + scale.width)
        };
        assert!(
            (edge - row_edge).abs() < 0.5,
            "{lang}: {files:?} vs {scale:?}"
        );
        // The last button pads its label by 10 to the box's end.
        let last = shown[2];
        if lang != "ar" {
            assert!(
                (last.x + last.width + 10.0 - edge).abs() < 0.5,
                "{lang}: {last:?} vs {edge}"
            );
        }
    }
}

#[test]
fn diagnostics_is_a_plain_section_after_the_others() {
    // R93: no fold; the Diagnostics heading and its rows are always shown, last on the page.
    let dir = tempfile::tempdir().unwrap();
    let mut state = state(dir.path());
    state.lang = "en".into();
    for key in [
        "frame_stats",
        "gpu_profile",
        "perf_diag",
        "frame_capture_key",
        "vk_validation",
    ] {
        assert!(advanced::check_keys(&state).contains(&key), "{key}");
    }
    open(&mut state, Page::Advanced);
    let _ = app::update(
        &mut state,
        Message::Tick(iced::time::Instant::now() + std::time::Duration::from_secs(1)),
    );
    let mut ui = iced_test::simulator::Simulator::with_size(
        iced::Settings::default(),
        iced::Size::new(1280.0, 4000.0),
        app::view(&state),
    );
    let heading = ui.find("DIAGNOSTICS (FOR DEVELOPMENT)").unwrap().bounds();
    let files = ui.find("FILES").unwrap().bounds();
    assert!(heading.y > files.y, "{heading:?} before {files:?}");
    for label in [
        "Frame statistics in the log (every 5 s)",
        "Vulkan validation layers (needs the Vulkan SDK; much slower)",
    ] {
        let shown = ui.find(label).unwrap().bounds();
        assert!(shown.y > heading.y, "{label}: {shown:?}");
    }
    drop(ui);
    click_tall(&mut state, "Frame statistics in the log (every 5 s)");
    assert!(state.settings.get_bool("frame_stats"));
}

#[test]
fn steam_shortcut_starts_in_the_programs_folder() {
    let shortcut =
        advanced::steam_shortcut(std::path::Path::new("/home/u/Apps/spartanport.AppImage"));
    assert_eq!(
        shortcut.exe,
        std::path::PathBuf::from("/home/u/Apps/spartanport.AppImage")
    );
    assert_eq!(shortcut.start_dir, std::path::PathBuf::from("/home/u/Apps"));
    assert_eq!(shortcut.options, "--play");
}

#[test]
fn steam_results_become_the_texts() {
    use spartanport_launcher::core::steam::{Added, SteamError};
    let added = |updated| {
        Ok(Added {
            accounts: 2,
            updated,
            app_id: 1,
        })
    };
    assert_eq!(
        advanced::steam_message("en", &added(false)).unwrap(),
        "The shortcut \"SpartanPort\" was added to Steam (accounts: 2), with its icon, capsules, background and logo. Open Steam to see it in the library."
    );
    assert_eq!(
        advanced::steam_message("en", &added(true)).unwrap(),
        "The shortcut \"SpartanPort\" was updated in Steam (accounts: 2), with its icon, capsules, background and logo. Open Steam to see it in the library."
    );
    assert_eq!(
        advanced::steam_message("en", &Err(SteamError::Running)).unwrap_err(),
        "Steam is open. Close it completely (Steam > Exit) and click again: Steam rewrites its shortcuts when it closes."
    );
    assert_eq!(
        advanced::steam_message("en", &Err(SteamError::NoSteam)).unwrap_err(),
        "Steam was not found for this user."
    );
    assert_eq!(
        advanced::steam_message("en", &Err(SteamError::NoArt("x".into()))).unwrap_err(),
        "The game folder has no sce_sys/pic1.png for the pictures."
    );
    let long = std::io::Error::other("x".repeat(500));
    let other = advanced::steam_message("en", &Err(SteamError::Io(long))).unwrap_err();
    assert_eq!(other.chars().count(), 400);
}

#[test]
fn stale_conflicts_are_dropped() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = mods_state(dir.path());
    make_mod(&state, "alpha", "same.bin");
    make_mod(&state, "beta", "same.bin");
    load_mods(&mut state);
    let root = state.paths.mods_dir("");
    let old_names = state.mods.order.clone();
    let old = spartanport_launcher::core::mods::conflicts(&root, &old_names, None);
    assert_eq!(old[0].1.last().unwrap(), "beta");
    // An edit lands before the older result does.
    mods_update(&mut state, ModsMessage::Move(1, -1));
    let new_names = state.mods.order.clone();
    assert_eq!(new_names, ["beta", "alpha"]);
    mods_update(&mut state, ModsMessage::Conflicts(old_names, Vec::new()));
    assert_eq!(state.mods.conflicts, old);
    let current = spartanport_launcher::core::mods::conflicts(&root, &new_names, None);
    assert_eq!(current[0].1.last().unwrap(), "alpha");
    // The stale result changed nothing; the matching one applies.
    mods_update(
        &mut state,
        ModsMessage::Conflicts(new_names.clone(), current.clone()),
    );
    assert_eq!(state.mods.conflicts, current);
    // And a stale one after it is still dropped.
    mods_update(
        &mut state,
        ModsMessage::Conflicts(vec!["alpha".into(), "beta".into()], old),
    );
    assert_eq!(state.mods.conflicts, current);
}

// --- Log page (Task 31) ---

use spartanport_launcher::ui::pages::log::{self, LogMessage};

#[test]
fn log_page_shows_buffer_lines_and_clears() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state(dir.path());
    state.lang = "en".into();
    state.log.push_text("line one\nline two\n");
    open(&mut state, Page::Log);
    let mut ui = iced_test::simulator::simulator(app::view(&state));
    // The body is one text widget, so it is found by its whole text.
    assert!(ui
        .find(
            "line one
line two"
        )
        .is_ok());
    drop(ui);

    // Copy hands the text to the clipboard; the buffer stays as it was.
    click(&mut state, "Copy");
    assert_eq!(state.log.len(), 2);

    // Open last_run.log targets <user_dir>/last_run.log.
    let expected = state
        .paths
        .run_log_path(&state.settings.get_str("user_dir"));
    assert!(matches!(
        log::open_last_run_message(&state),
        Message::OpenPath(path) if path == expected
    ));
    let sent = messages(&state, "Open last_run.log");
    assert!(matches!(
        sent.as_slice(),
        [Message::Log(LogMessage::OpenLastRun)]
    ));
    let _ = app::update(&mut state, Message::Log(LogMessage::OpenLastRun));

    click(&mut state, "Clear");
    assert_eq!(state.log.len(), 0);
    let mut ui = iced_test::simulator::simulator(app::view(&state));
    assert!(ui
        .find(
            "line one
line two"
        )
        .is_err());
}

#[test]
fn a_red_hairline_separates_the_log_header_from_the_log() {
    // Mockup `#logbox` border-top (ruling R88): between the button row and the text, across it.
    let dir = tempfile::tempdir().unwrap();
    let mut state = state(dir.path());
    state.lang = "en".into();
    state.log.push_text(
        "line one
",
    );
    open(&mut state, Page::Log);
    let _ = app::update(
        &mut state,
        Message::Tick(iced::time::Instant::now() + std::time::Duration::from_secs(1)),
    );
    let mut ui = iced_test::simulator::simulator(app::view(&state));
    let line = ui
        .find(iced_test::selector::id(log::SEPARATOR_ID))
        .unwrap()
        .bounds();
    let copy = ui.find("Copy").unwrap().bounds();
    let kept = ui.find("6 000 lines kept").unwrap().bounds();
    let text = ui.find("line one").unwrap().bounds();
    assert_eq!(line.height, 1.0);
    assert!(
        line.y >= copy.y + copy.height && line.y + line.height <= text.y,
        "header {copy:?}, line {line:?}, log {text:?}"
    );
    // From the first button (which pads its label by 10) to past the line count.
    assert!((line.x - (copy.x - 10.0)).abs() < 0.5, "{line:?}, {copy:?}");
    assert!(
        line.x + line.width >= kept.x + kept.width,
        "{line:?}, {kept:?}"
    );
}

#[test]
fn log_header_translates() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state(dir.path());
    state.lang = "pt".into();
    open(&mut state, Page::Log);
    let mut ui = iced_test::simulator::simulator(app::view(&state));
    assert!(ui.find("6 000 linhas mantidas").is_ok());
}

/// The release list and the install of any listed release (Task 34, version menu Task 5):
/// nothing here reaches the network or spawns anything.
mod update_flow {
    use std::path::{Path, PathBuf};
    use std::sync::mpsc;

    use spartanport_launcher::core::release::{Channel, Release, ReleaseId};
    use spartanport_launcher::ui::app::{self, Message};
    use spartanport_launcher::ui::splash;
    use spartanport_launcher::ui::update::{
        confirm_text, install_decision, is_older, newer, offer_decision, releases, Decision,
        InstallPlan, UpdateMessage, UpdateState,
    };
    use spartanport_launcher::ui::widgets::ModalKind;
    use spartanport_launcher::VERSION;

    use super::common::state;

    const OLDER: &str = "This version is older than the installed one.";

    fn release(version: &str, build: Option<&str>, prerelease: bool) -> Release {
        Release {
            id: ReleaseId {
                channel: if prerelease {
                    Channel::PreRelease
                } else {
                    Channel::Release
                },
                version: version.to_string(),
                build: build.map(str::to_string),
            },
            published: "2026-10-22T00:00:00Z".to_string(),
            url: format!("https://example.test/{version}.zip"),
            page: "https://example.test/page".to_string(),
        }
    }

    fn checked(manual: bool, result: Result<Vec<Release>, String>) -> Message {
        Message::Update(UpdateMessage::Checked { manual, result })
    }

    fn modal_text(state: &app::State) -> String {
        state
            .modal
            .as_ref()
            .map(|m| m.text.clone())
            .unwrap_or_default()
    }

    fn downloading(release: Release) -> UpdateState {
        UpdateState::Downloading {
            releases: vec![release.clone()],
            release,
            percent: 0,
        }
    }

    #[test]
    fn a_list_with_a_newer_stable_is_ready_and_newer() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = state(dir.path());
        let status = state.status.clone();
        let list = vec![
            release("999.0.0", None, false),
            release(VERSION, None, false),
        ];
        let _ = app::update(&mut state, checked(false, Ok(list.clone())));
        assert!(matches!(&state.update, UpdateState::Ready(shown) if *shown == list));
        assert_eq!(releases(&state), list.as_slice());
        assert_eq!(newer(&state), Some(&list[0]));
        assert!(state.modal.is_none());
        assert_eq!(state.status, status);
    }

    #[test]
    fn manual_check_with_latest_shows_the_info() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = state(dir.path());
        let list = vec![release(VERSION, None, false)];
        let _ = app::update(&mut state, checked(true, Ok(list.clone())));
        assert!(matches!(&state.update, UpdateState::Ready(shown) if *shown == list));
        assert_eq!(
            modal_text(&state),
            format!("You have the latest version ({VERSION}).")
        );
        assert!(matches!(
            state.modal.as_ref().map(|m| &m.kind),
            Some(ModalKind::Info)
        ));
        // The same list found silently shows nothing.
        state.modal = None;
        let _ = app::update(&mut state, checked(false, Ok(list)));
        assert!(state.modal.is_none());
    }

    #[test]
    fn a_failed_manual_check_reports_and_a_silent_one_does_not() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = state(dir.path());
        let _ = app::update(&mut state, checked(true, Err("offline".into())));
        assert_eq!(modal_text(&state), "Could not check for updates: offline");
        assert!(matches!(
            state.modal.as_ref().map(|m| &m.kind),
            Some(ModalKind::Error)
        ));
        state.modal = None;
        let _ = app::update(&mut state, checked(false, Err("offline".into())));
        assert!(state.modal.is_none());
        assert!(matches!(&state.update, UpdateState::Failed(error) if error == "offline"));
    }

    #[test]
    fn a_failed_check_keeps_a_ready_list() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = state(dir.path());
        let list = vec![release("2.7.0", None, false)];
        state.update = UpdateState::Ready(list.clone());
        let _ = app::update(&mut state, checked(true, Err("offline".into())));
        assert!(matches!(&state.update, UpdateState::Ready(shown) if *shown == list));
        assert_eq!(modal_text(&state), "Could not check for updates: offline");
        assert!(matches!(
            state.modal.as_ref().map(|m| &m.kind),
            Some(ModalKind::Error)
        ));
        state.modal = None;
        let _ = app::update(&mut state, checked(false, Err("offline".into())));
        assert!(matches!(&state.update, UpdateState::Ready(shown) if *shown == list));
        assert!(state.modal.is_none());
    }

    #[test]
    fn offer_decision_follows_newer_and_manual() {
        assert_eq!(offer_decision(true, true), Decision::Offer);
        assert_eq!(offer_decision(true, false), Decision::Offer);
        assert_eq!(offer_decision(false, true), Decision::Latest);
        assert_eq!(offer_decision(false, false), Decision::Nothing);
    }

    #[test]
    fn the_start_up_list_is_ready() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = state(dir.path());
        let list = vec![release("2.7.0", None, false), release("2.6.0", None, false)];
        splash::start_up_offer(&mut state, list.clone());
        assert!(matches!(&state.update, UpdateState::Ready(shown) if *shown == list));
        // A download keeps its state.
        let fetching = release("2.7.0", None, false);
        state.update = downloading(fetching.clone());
        splash::start_up_offer(&mut state, vec![release("2.8.0", None, false)]);
        assert!(matches!(
            &state.update,
            UpdateState::Downloading { release, .. } if *release == fetching
        ));
    }

    #[test]
    fn installing_the_installed_release_does_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = state(dir.path());
        let installed = release(VERSION, None, false);
        assert_eq!(installed.id, state.installed);
        let task = app::update(
            &mut state,
            Message::Update(UpdateMessage::Install(installed)),
        );
        assert!(state.modal.is_none());
        assert_eq!(task.units(), 0);

        // The stamp's channel is frozen at build time; the list's follows the API.
        state.installed = release("2.7.0", None, true).id;
        let task = app::update(
            &mut state,
            Message::Update(UpdateMessage::Install(release("2.7.0", None, false))),
        );
        assert!(state.modal.is_none());
        assert_eq!(task.units(), 0);
    }

    #[test]
    fn install_is_blocked_while_running() {
        let always = |_: &Path| true;
        assert_eq!(
            install_decision(true, true, true, None, &always),
            InstallPlan::Blocked
        );
        assert_eq!(
            install_decision(false, true, true, None, &always),
            InstallPlan::Confirm
        );
    }

    #[test]
    fn windows_source_tree_opens_the_page() {
        let always = |_: &Path| true;
        assert_eq!(
            install_decision(false, true, false, None, &always),
            InstallPlan::OpenPage
        );
    }

    #[test]
    fn linux_needs_a_writable_appimage_folder() {
        let image = PathBuf::from("/opt/app/SpartanPort.AppImage");
        let yes = |_: &Path| true;
        let no = |_: &Path| false;
        assert_eq!(
            install_decision(false, false, false, None, &yes),
            InstallPlan::OpenPage
        );
        assert_eq!(
            install_decision(false, false, false, Some(&image), &yes),
            InstallPlan::Confirm
        );
        assert!(matches!(
            install_decision(false, false, false, Some(&image), &no),
            InstallPlan::ReadOnly(_)
        ));
    }

    #[test]
    fn install_asks_and_marks_an_older_release() {
        let installed = release("2.6.0", None, false).id;
        let question = "Install version {} now? The launcher closes, installs it and opens again. \
                        Saves and settings are kept.";
        let older = release("2.5.0", None, false);
        assert!(is_older(&older, &installed));
        assert_eq!(
            confirm_text("en", &older, &installed),
            format!("{}\n{OLDER}", question.replace("{}", "2.5.0"))
        );
        let newer = release("2.7.0", None, false);
        assert!(!is_older(&newer, &installed));
        assert_eq!(
            confirm_text("en", &newer, &installed),
            question.replace("{}", "2.7.0")
        );
        // A test build of the installed version is not older.
        let test = release("2.6.0", Some("x"), true);
        assert!(!is_older(&test, &installed));
        let text = confirm_text("en", &test, &installed);
        assert!(text.starts_with("Install version 2.6.0 · x now?"), "{text}");
        assert!(!text.ends_with(OLDER), "{text}");
    }

    #[test]
    fn install_is_ignored_while_downloading() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = state(dir.path());
        let fetching = release("2.7.0", None, false);
        state.update = downloading(fetching.clone());
        let task = app::update(
            &mut state,
            Message::Update(UpdateMessage::Install(release("2.5.0", None, false))),
        );
        assert!(matches!(
            &state.update,
            UpdateState::Downloading { release, .. } if *release == fetching
        ));
        assert!(state.modal.is_none());
        assert_eq!(task.units(), 0);
    }

    #[test]
    fn a_failed_download_goes_back_to_the_list() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = state(dir.path());
        let fetched = release("2.7.0", None, false);
        state.update = downloading(fetched.clone());
        let (_sender, receiver) = mpsc::channel();
        state.update_progress = Some(receiver);
        let _ = app::update(
            &mut state,
            Message::Update(UpdateMessage::Done(Err("boom".into()))),
        );
        assert!(matches!(&state.update, UpdateState::Ready(list) if *list == vec![fetched]));
        assert_eq!(modal_text(&state), "Update failed: boom");
        assert!(matches!(
            state.modal.as_ref().map(|m| &m.kind),
            Some(ModalKind::Error)
        ));
        assert!(state.update_progress.is_none());
    }

    #[test]
    fn progress_updates_the_percent() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = state(dir.path());
        state.update = downloading(release("2.7.0", None, false));
        let _ = app::update(&mut state, Message::Update(UpdateMessage::Progress(42)));
        assert!(matches!(
            state.update,
            UpdateState::Downloading { percent: 42, .. }
        ));
    }
}
