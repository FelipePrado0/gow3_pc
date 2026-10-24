#![allow(clippy::unwrap_used, clippy::expect_used)]
//! The first-run setup: when it opens, its steps, its edits and how it ends.

mod common;

use std::path::Path;
use std::time::Duration;

use iced::keyboard::key::Named;
use iced::keyboard::{Key, Location, Modifiers};
use iced::window;
use iced::Size;

use common::{browsed, dump_with_patches, gpu_rows, messages, ARC_LAPTOP};
use iced_test::simulator::simulator;
use spartanport_launcher::core::i18n::tr;
use spartanport_launcher::core::options::DISPLAY_MODES;
use spartanport_launcher::core::paths::Paths;
use spartanport_launcher::ui::app::{self, scene_size, Message, State, BAR_HEIGHT};
use spartanport_launcher::ui::lang_menu::LangMessage;
use spartanport_launcher::ui::pages::{performance, Page};
use spartanport_launcher::ui::setup::{can_advance, folder_problems, SetupMessage, Step};
use spartanport_launcher::ui::splash::{Check, SplashMessage};
use spartanport_launcher::ui::widgets::choice_pairs;

/// A state over `dir` only, as the program opens it, with the splash already gone.
fn opened(dir: &Path) -> State {
    let root = dir.display().to_string();
    let env = move |key: &str| match key {
        "APPDATA" | "USERPROFILE" | "HOME" | "XDG_CONFIG_HOME" => Some(root.clone()),
        _ => None,
    };
    let mut state = State::open(Paths::from_env(dir, &env));
    state.splash.done = true;
    state
}

/// A state over `dir` with the setup already on `step`.
fn at_step(dir: &Path, step: Step) -> State {
    let mut state = opened(dir);
    state.setup.as_mut().unwrap().step = step;
    state
}

/// `dump_with_patches` plus the `sce_module` files the port needs, so every required file passes.
fn complete_dump(port: &Path) -> std::path::PathBuf {
    let dump = dump_with_patches(port);
    std::fs::create_dir_all(dump.join("sce_module")).unwrap();
    for name in ["libc.prx", "libSceFios2.prx"] {
        std::fs::write(dump.join("sce_module").join(name), b"prx").unwrap();
    }
    dump
}

fn setup(state: &mut State, message: SetupMessage) {
    let _ = app::update(state, Message::Setup(message));
}

#[test]
fn the_setup_opens_only_without_a_settings_file() {
    let dir = tempfile::tempdir().unwrap();
    let state = opened(dir.path());
    assert_eq!(state.setup.as_ref().map(|s| s.step), Some(Step::Folders));

    for text in ["{}", "{not json"] {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("settings.json"), text).unwrap();
        assert!(opened(dir.path()).setup.is_none(), "{text:?}");
    }
    // `new` is what every other test builds on: never the setup.
    let dir = tempfile::tempdir().unwrap();
    assert!(common::state(dir.path()).setup.is_none());
}

#[test]
fn next_and_back_walk_the_three_steps() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = opened(dir.path());
    browsed(&mut state, "game_dir", &complete_dump(dir.path()));
    setup(&mut state, SetupMessage::Back);
    assert_eq!(state.setup.as_ref().unwrap().step, Step::Folders);
    setup(&mut state, SetupMessage::Next);
    assert_eq!(state.setup.as_ref().unwrap().step, Step::Gpu);
    setup(&mut state, SetupMessage::Next);
    assert_eq!(state.setup.as_ref().unwrap().step, Step::Video);
    setup(&mut state, SetupMessage::Next);
    assert_eq!(state.setup.as_ref().unwrap().step, Step::Video);
    setup(&mut state, SetupMessage::Back);
    assert_eq!(state.setup.as_ref().unwrap().step, Step::Gpu);
}

#[test]
fn the_setup_replaces_the_home_and_the_bar() {
    let dir = tempfile::tempdir().unwrap();
    let state = opened(dir.path());
    let mut ui = simulator(app::view(&state));
    assert!(ui.find("Initial setup").is_ok());
    assert!(ui.find("Step 1 of 3 · Folders").is_ok());
    assert!(ui.find("Skip").is_ok());
    assert!(ui.find("Next").is_ok());
    assert!(ui.find("Back").is_err());
    assert!(ui.find("PLAY").is_err());
    assert!(ui.find("Graphics").is_err(), "the tab bar is not drawn");
    assert!(ui.find("English ▾").is_ok(), "the language button stays");
}

#[test]
fn the_last_step_offers_finish_and_back() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = opened(dir.path());
    state.setup.as_mut().unwrap().step = Step::Video;
    let mut ui = simulator(app::view(&state));
    assert!(ui.find("Step 3 of 3 · Video").is_ok());
    assert!(ui.find("Finish").is_ok());
    assert!(ui.find("Back").is_ok());
    assert!(ui.find("Next").is_err());
    assert!(matches!(
        common::messages(&state, "Back").as_slice(),
        [Message::Setup(SetupMessage::Back)]
    ));
}

#[test]
fn the_setup_reads_in_portuguese() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = opened(dir.path());
    state.lang = "pt".into();
    let mut ui = simulator(app::view(&state));
    assert!(ui.find("Configuração inicial").is_ok());
    assert!(ui.find("Passo 1 de 3 · Pastas").is_ok());
    assert!(ui.find("Pular").is_ok());
    assert!(ui.find("Avançar").is_ok());
}

#[test]
fn the_footer_is_mirrored_right_to_left() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = opened(dir.path());
    let skip_left_of_next = |state: &State| {
        let mut ui = simulator(app::view(state));
        let skip = ui.find(tr(&state.lang, "Skip")).unwrap().bounds();
        let next = ui.find(tr(&state.lang, "Next")).unwrap().bounds();
        skip.x < next.x
    };
    assert!(skip_left_of_next(&state));
    state.lang = "ar".into();
    state.rtl = true;
    assert!(!skip_left_of_next(&state), "Skip is at the right");
}

#[test]
fn next_waits_for_a_complete_game_folder() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = opened(dir.path());
    assert!(!can_advance(&state));
    assert!(messages(&state, "Next").is_empty(), "Next has no on_press");
    let lines = folder_problems(&state, "game_dir");
    assert_eq!(
        lines.first().map(String::as_str),
        Some("Missing or empty files:")
    );
    assert!(
        lines.iter().any(|line| line.contains("eboot.bin")),
        "{lines:?}"
    );
    assert!(folder_problems(&state, "update_dir").is_empty());
    {
        let mut ui = simulator(app::view(&state));
        assert!(ui.find("Missing or empty files:").is_ok());
    }

    let dump = complete_dump(dir.path());
    browsed(&mut state, "game_dir", &dump);
    assert!(folder_problems(&state, "game_dir").is_empty());
    assert!(can_advance(&state));
    assert!(
        state.home.info.is_some(),
        "the Home facts follow the folder"
    );
    assert!(matches!(
        messages(&state, "Next").as_slice(),
        [Message::Setup(SetupMessage::Next)]
    ));
    setup(&mut state, SetupMessage::Next);
    assert_eq!(state.setup.as_ref().unwrap().step, Step::Gpu);
}

#[test]
fn next_is_ignored_while_the_folder_is_incomplete() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = opened(dir.path());
    setup(&mut state, SetupMessage::Next);
    assert_eq!(state.setup.as_ref().unwrap().step, Step::Folders);
}

#[test]
fn the_update_folder_must_differ_from_the_game_folder() {
    // The update folder is chosen first and the game folder is then pointed at the same folder:
    // the update folder's check follows the game folder.
    let dir = tempfile::tempdir().unwrap();
    let mut state = opened(dir.path());
    let dump = complete_dump(dir.path());
    browsed(&mut state, "update_dir", &dump);
    assert!(
        folder_problems(&state, "update_dir").is_empty(),
        "no game folder yet"
    );
    browsed(&mut state, "game_dir", &dump);
    assert_eq!(
        folder_problems(&state, "update_dir"),
        ["Choose two separate folders for the game and its update, or leave the update folder empty."]
    );
    assert!(!can_advance(&state));

    let other = dir.path().join("update");
    std::fs::create_dir_all(&other).unwrap();
    browsed(&mut state, "update_dir", &other);
    let lines = folder_problems(&state, "update_dir");
    assert_eq!(
        lines.first().map(String::as_str),
        Some("Missing or empty files:")
    );
    assert!(!can_advance(&state));
}

#[test]
fn the_folders_are_checked_again_on_a_change_or_the_focus() {
    // Files copied into the game folder from a file manager: the red list stays until the window
    // gets the focus back, and a redraw reads nothing from the disk.
    let dir = tempfile::tempdir().unwrap();
    let mut state = opened(dir.path());
    let dump = dir.path().join("dump");
    std::fs::create_dir_all(&dump).unwrap();
    browsed(&mut state, "game_dir", &dump);
    assert!(!can_advance(&state));
    complete_dump(dir.path());
    assert!(
        !can_advance(&state),
        "nothing told the setup the files arrived"
    );
    let _ = app::update(&mut state, Message::WindowFocused(true));
    assert!(can_advance(&state));
    assert!(folder_problems(&state, "game_dir").is_empty());

    // A path typed into the update field is checked at once.
    let _ = app::update(
        &mut state,
        Message::Set("update_dir".into(), dump.display().to_string().into()),
    );
    assert_eq!(
        folder_problems(&state, "update_dir"),
        ["Choose two separate folders for the game and its update, or leave the update folder empty."]
    );
    assert!(!can_advance(&state));
}

/// The labels of the graphics card drop-down, in order.
fn gpu_labels(state: &State) -> Vec<String> {
    let rows = performance::rows(state, "en");
    let cards = rows.iter().find(|row| row.key == "gpu_id").unwrap();
    cards.pairs.iter().map(|(_, label)| label.clone()).collect()
}

#[test]
fn the_card_step_lists_automatic_and_the_cards() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = at_step(dir.path(), Step::Gpu);
    let mut ui = simulator(app::view(&state));
    assert!(ui.find("Step 2 of 3 · Graphics card").is_ok());
    // A closed drop-down draws its choice itself, so the simulator cannot find it: the choices
    // are read from the rows the page builds.
    assert_eq!(gpu_labels(&state), ["Automatic"]);
    assert!(ui
        .find("Graphics card not checked: the game picks one.")
        .is_ok());
    drop(ui);

    // The cards are listed only after the player reached this step and chose one: the choice
    // stays and the list fills in.
    let _ = app::update(&mut state, Message::Set("gpu_id".into(), "1".into()));
    let _ = app::update(
        &mut state,
        Message::Splash(SplashMessage::CheckDone(Check::Gpus(gpu_rows(&ARC_LAPTOP)))),
    );
    assert_eq!(state.settings.get_str("gpu_id"), "1");
    let mut ui = simulator(app::view(&state));
    assert_eq!(
        gpu_labels(&state),
        [
            "Automatic",
            "Intel(R) Arc(TM) Graphics",
            "NVIDIA GeForce RTX 4060 Laptop GPU"
        ]
    );
    assert!(ui
        .find("Graphics card not checked: the game picks one.")
        .is_err());
}

/// The label the Display mode drop-down shows for the current ini value.
fn display_mode_label(state: &State) -> Option<String> {
    let current = state
        .ini
        .values
        .get("display_mode")
        .cloned()
        .unwrap_or_default();
    choice_pairs(DISPLAY_MODES, "en")
        .into_iter()
        .find(|(value, _)| *value == current)
        .map(|(_, label)| label)
}

/// The `(current, its label)` of the `performance::rows` drop-down `key`.
fn row_shown(state: &State, key: &str) -> (String, Option<String>) {
    let row = performance::rows(state, "en")
        .into_iter()
        .find(|row| row.key == key)
        .unwrap();
    let label = row
        .pairs
        .iter()
        .find(|(value, _)| *value == row.current)
        .map(|(_, label)| label.clone());
    (row.current, label)
}

#[test]
fn the_video_step_edits_the_ini_values() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = at_step(dir.path(), Step::Video);
    {
        let mut ui = simulator(app::view(&state));
        for label in [
            "Display mode",
            "Aspect ratio",
            "Frame rate limit",
            "Engine frame rate",
        ] {
            assert!(ui.find(label).is_ok(), "{label}");
        }
    }
    assert_eq!(
        state.ini.values.get("display_mode").map(String::as_str),
        Some("windowed")
    );
    assert_eq!(display_mode_label(&state).as_deref(), Some("Windowed"));
    assert_eq!(
        row_shown(&state, "fps_limit"),
        ("0".into(), Some("Unlimited".into()))
    );
    assert_eq!(row_shown(&state, "engine_fps").0, "120");
    for (key, value) in [
        ("fps_limit", "60"),
        ("display_mode", "fullscreen"),
        ("engine_fps", "144"),
    ] {
        let _ = app::update(&mut state, Message::Set(key.into(), value.into()));
        assert_eq!(state.ini.values.get(key).map(String::as_str), Some(value));
    }
    assert_eq!(display_mode_label(&state).as_deref(), Some("Fullscreen"));
    assert_eq!(row_shown(&state, "fps_limit").0, "60");
    assert_eq!(row_shown(&state, "engine_fps").0, "144");
}

#[test]
fn an_ini_without_settings_is_shown_and_kept() {
    // The ini survived, settings.json did not.
    let dir = tempfile::tempdir().unwrap();
    let probe = opened(dir.path());
    let ini = probe.paths.ini_path();
    std::fs::write(&ini, "# mine\nfps_limit=60\nextra=kept\n").unwrap();
    let mut state = at_step(dir.path(), Step::Video);
    assert!(state.setup.is_some());
    assert_eq!(
        state.ini.values.get("fps_limit").map(String::as_str),
        Some("60")
    );
    assert_eq!(row_shown(&state, "fps_limit").0, "60");
    let mut ui = simulator(app::view(&state));
    assert!(ui.find("Frame rate limit").is_ok());
    drop(ui);
    setup(&mut state, SetupMessage::Finish);
    let written = std::fs::read_to_string(&ini).unwrap();
    assert!(
        written.starts_with(
            "# mine
fps_limit=60
extra=kept
"
        ),
        "{written}"
    );
}

fn settings_file(dir: &Path) -> serde_json::Value {
    serde_json::from_slice(&std::fs::read(dir.join("settings.json")).unwrap()).unwrap()
}

#[test]
fn finish_writes_the_choices_and_shows_the_home() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = opened(dir.path());
    let dump = complete_dump(dir.path());
    browsed(&mut state, "game_dir", &dump);
    let _ = app::update(&mut state, Message::Set("gpu_id".into(), "1".into()));
    let _ = app::update(&mut state, Message::Set("fps_limit".into(), "60".into()));
    state.setup.as_mut().unwrap().step = Step::Video;
    setup(&mut state, SetupMessage::Finish);
    assert!(state.setup.is_none());
    let saved = settings_file(dir.path());
    assert_eq!(saved["game_dir"], dump.display().to_string());
    assert_eq!(saved["gpu_id"], "1");
    let ini = std::fs::read_to_string(state.paths.ini_path()).unwrap();
    assert!(ini.contains("fps_limit=60"), "{ini}");
    let mut ui = simulator(app::view(&state));
    assert!(ui.find("PLAY").is_ok());
    assert!(ui.find("Initial setup").is_err());
    assert!(!spartanport_launcher::ui::persist::dirty(&state));
}

#[test]
fn skip_writes_the_defaults_but_keeps_the_language() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = opened(dir.path());
    let dump = complete_dump(dir.path());
    browsed(&mut state, "game_dir", &dump);
    let _ = app::update(&mut state, Message::Set("fps_limit".into(), "60".into()));
    state.lang = "pt".into();
    setup(&mut state, SetupMessage::Skip);
    assert!(state.setup.is_none());
    let saved = settings_file(dir.path());
    assert_eq!(saved["game_dir"], "");
    assert_eq!(saved["ui_language"], "pt");
    assert_eq!(
        state.ini.values.get("fps_limit").map(String::as_str),
        Some("0")
    );
    assert!(
        state.home.info.is_none(),
        "the Home facts follow the restored folder"
    );
}

#[test]
fn skip_puts_the_aspect_back_with_the_resolution() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = at_step(dir.path(), Step::Video);
    let resolution = |state: &State| state.ini.values.get("render_resolution").cloned();
    assert_eq!(state.aspect, "16:9");
    assert!(performance::aspect_options(&state).contains(&"21:9".to_string()));
    let _ = app::update(&mut state, Message::Aspect("21:9".into()));
    assert_ne!(resolution(&state).as_deref(), Some("native"));
    setup(&mut state, SetupMessage::Skip);
    assert_eq!(resolution(&state).as_deref(), Some("native"));
    // The aspect of the restored resolution.
    assert_eq!(state.aspect, "16:9");
}

#[test]
fn closing_the_window_writes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = opened(dir.path());
    let _ = app::update(&mut state, Message::Set("fps_limit".into(), "60".into()));
    let _ = app::update(&mut state, Message::CloseRequested(window::Id::unique()));
    assert!(!dir.path().join("settings.json").exists());
    assert!(state.setup.is_some());
}

#[test]
fn a_failed_finish_keeps_the_setup_open() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = opened(dir.path());
    // A folder where the file should be: the atomic write cannot rename over it.
    std::fs::create_dir_all(state.paths.config_file()).unwrap();
    let _ = app::update(&mut state, Message::Set("fps_limit".into(), "60".into()));
    setup(&mut state, SetupMessage::Finish);
    assert!(state.setup.is_some());
    assert!(state.modal.is_some(), "the SAVE_FAILED dialog");
    let _ = app::update(
        &mut state,
        Message::Key(
            Key::Named(Named::Escape),
            Modifiers::default(),
            Location::Standard,
        ),
    );
    assert!(state.modal.is_none());
    assert!(state.setup.is_some());
    assert_eq!(
        state.ini.values.get("fps_limit").map(String::as_str),
        Some("60")
    );
}

fn press(state: &mut State, key: Key) {
    let _ = app::update(
        state,
        Message::Key(key, Modifiers::default(), Location::Standard),
    );
}

#[test]
fn keys_stay_inside_the_setup() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = opened(dir.path());
    press(&mut state, Key::Named(Named::ArrowRight));
    assert_eq!(state.page, Page::Home);
    press(&mut state, Key::Named(Named::Escape));
    assert!(state.setup.is_some());
    press(&mut state, Key::Named(Named::Enter));
    assert_eq!(
        state.setup.as_ref().unwrap().step,
        Step::Folders,
        "Next not allowed yet"
    );
    assert!(matches!(
        state.game,
        spartanport_launcher::ui::play::GameState::Idle
    ));

    let dump = complete_dump(dir.path());
    browsed(&mut state, "game_dir", &dump);
    press(&mut state, Key::Named(Named::Enter));
    assert_eq!(state.setup.as_ref().unwrap().step, Step::Gpu);
    press(&mut state, Key::Named(Named::Enter));
    assert_eq!(state.setup.as_ref().unwrap().step, Step::Video);
    press(&mut state, Key::Named(Named::Enter));
    assert!(state.setup.is_none(), "Enter on the last step is Finish");
    assert!(dir.path().join("settings.json").exists());
}

#[test]
fn a_language_pick_rebuilds_without_writing() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = at_step(dir.path(), Step::Gpu);
    let _ = app::update(&mut state, Message::Lang(LangMessage::Pick("pt".into())));
    assert_eq!(state.lang, "pt");
    assert_eq!(state.setup.as_ref().unwrap().step, Step::Gpu);
    assert!(!dir.path().join("settings.json").exists());
    let mut ui = simulator(app::view(&state));
    assert!(ui.find("Passo 2 de 3 · Placa de vídeo").is_ok());
}

#[test]
fn a_language_pick_keeps_the_ini_edits_for_finish() {
    // An ini that outlived settings.json: an edit made before a language pick still reaches it.
    let dir = tempfile::tempdir().unwrap();
    let ini = opened(dir.path()).paths.ini_path();
    std::fs::write(&ini, "fps_limit=0\n").unwrap();
    let mut state = at_step(dir.path(), Step::Video);
    let _ = app::update(&mut state, Message::Set("fps_limit".into(), "60".into()));
    let _ = app::update(&mut state, Message::Lang(LangMessage::Pick("pt".into())));
    setup(&mut state, SetupMessage::Finish);
    let written = std::fs::read_to_string(&ini).unwrap();
    assert!(written.contains("fps_limit=60"), "{written}");
}

#[test]
fn the_setup_scene_is_the_whole_window() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = opened(dir.path());
    let _ = app::update(&mut state, Message::Resized(Size::new(1280.0, 800.0)));
    assert_eq!(scene_size(&state), Size::new(1280.0, 800.0));
    state.setup = None;
    assert_eq!(scene_size(&state), Size::new(1280.0, 800.0 - BAR_HEIGHT));
}

#[test]
fn the_fire_burns_at_splash_strength_under_the_setup() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = opened(dir.path());
    let start = state.now;
    let _ = app::update(
        &mut state,
        Message::Tick(start + Duration::from_millis(200)),
    );
    assert_eq!(state.fire.intensity(), 1.0);
    state.setup = None;
    let _ = app::update(
        &mut state,
        Message::Tick(start + Duration::from_millis(400)),
    );
    assert_eq!(state.fire.intensity(), 0.8);
}
