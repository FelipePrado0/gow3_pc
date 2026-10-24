#![allow(clippy::unwrap_used, clippy::expect_used)]
//! The Save button (Task 32): inactive until a setting differs from what the files hold; writes them
//! on click. Ports of `tests/test_launcher_save.py`.

use std::fs;
use std::path::Path;
use std::time::Duration;

use iced::time::Instant;
use iced_test::simulator::{simulator, Simulator};
use serde_json::{json, Value};
use spartanport_launcher::core::gpu::parse_display_list;
use spartanport_launcher::core::i18n::tr;
use spartanport_launcher::core::ini::{load_ini, INI_FLAGS};
use spartanport_launcher::core::patches::PatchMeta;
use spartanport_launcher::core::paths::Paths;
use spartanport_launcher::core::release::{Channel, Release, ReleaseId};
use spartanport_launcher::core::settings::app_defaults;
use spartanport_launcher::ui::app::{self, Message, State};
use spartanport_launcher::ui::drawer::DrawerState;
use spartanport_launcher::ui::pages::controls::ControlsMessage;
use spartanport_launcher::ui::pages::mods::{self, ModsMessage};
use spartanport_launcher::ui::pages::Page;
use spartanport_launcher::ui::persist::{dirty, SHOWN_INI_KEYS};
use spartanport_launcher::ui::play::GameState;
use spartanport_launcher::ui::update::{newer, UpdateState};
use spartanport_launcher::ui::widgets::modal::ModalKind;

const SAVE_FAILED: &str =
    "Could not save all settings. Check write permissions and free disk space, then try again.";

/// A launcher over `dir` only (its data folder), with the splash already gone.
fn state_in(dir: &Path) -> State {
    let root = dir.display().to_string();
    let env = move |key: &str| match key {
        "APPDATA" | "USERPROFILE" | "HOME" | "XDG_CONFIG_HOME" => Some(root.clone()),
        _ => None,
    };
    let mut state = State::new(Paths::from_env(dir, &env));
    state.splash.done = true;
    state
}

fn send(state: &mut State, message: Message) {
    let _ = app::update(state, message);
}

fn set(state: &mut State, key: &str, value: Value) {
    send(state, Message::Set(key.to_string(), value));
}

/// Opens a page and lets the drawer finish sliding in.
fn open(state: &mut State, page: Page) {
    send(state, Message::ShowPage(page));
    send(
        state,
        Message::Tick(Instant::now() + Duration::from_secs(1)),
    );
}

/// What a click on the Save button of the drawer header emits: nothing while it is disabled.
fn save_button(state: &State) -> Vec<Message> {
    let label = tr(&state.lang, "Save");
    let mut ui = simulator(app::view(state));
    ui.click(label.as_str())
        .unwrap_or_else(|error| panic!("click {label:?}: {error:?}"));
    ui.into_messages().collect()
}

fn save_enabled(state: &State) -> bool {
    let sent = save_button(state);
    assert!(sent.iter().all(|message| matches!(message, Message::Save)));
    !sent.is_empty()
}

/// Clicks Save and feeds what it emitted to `update`.
fn click_save(state: &mut State) {
    for message in save_button(state) {
        send(state, message);
    }
}

fn save(state: &mut State) {
    send(state, Message::Save);
}

fn shows(state: &State, label: &str) -> bool {
    simulator(app::view(state)).find(label).is_ok()
}

fn temp_files(dir: &Path) -> Vec<String> {
    fs::read_dir(dir)
        .unwrap()
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with(".tmp") || name.ends_with(".tmp"))
        .collect()
}

#[test]
fn nothing_to_save_after_opening() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state_in(dir.path());
    open(&mut state, Page::Graphics);
    assert!(!dirty(&state));
    assert!(!save_enabled(&state));
}

#[test]
fn listing_the_monitors_after_opening_is_not_a_change() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("settings.json"), r#"{"monitor": "3"}"#).unwrap();
    let mut state = state_in(dir.path());
    let displays = parse_display_list(
        "0\tMonitor A\t1\t0\t0\t1920\t1080\n1\tMonitor B\t0\t1920\t0\t2560\t1440\n",
    );
    send(&mut state, Message::MonitorsListed(displays));
    // The saved screen is gone: the primary default is shown, as it was when the files were read.
    assert_eq!(state.settings.get_str("monitor"), "");
    assert!(!dirty(&state));
    set(&mut state, "monitor", json!("1"));
    assert!(dirty(&state));
}

#[test]
fn a_changed_setting_enables_saving_until_it_is_reverted() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state_in(dir.path());
    open(&mut state, Page::Graphics);
    set(&mut state, "vsync", json!(false));
    assert!(dirty(&state));
    assert!(save_enabled(&state));
    set(&mut state, "vsync", json!(true));
    assert!(!dirty(&state));
    assert!(!save_enabled(&state));
}

#[test]
fn saving_writes_the_files_and_disables_the_button() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state_in(dir.path());
    open(&mut state, Page::Graphics);
    set(&mut state, "vsync", json!(false));
    click_save(&mut state);
    let ini = fs::read_to_string(state.paths.ini_path()).unwrap();
    assert!(ini.lines().any(|line| line == "vsync=0"), "{ini}");
    assert!(state.paths.config_file().is_file());
    assert!(!save_enabled(&state));
}

#[test]
fn nothing_is_written_before_saving() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state_in(dir.path());
    set(&mut state, "vsync", json!(false));
    assert!(dirty(&state));
    assert!(!state.paths.ini_path().exists());
    assert!(!state.paths.config_file().exists());
}

#[test]
fn saving_a_new_launcher_language_rebuilds_the_window_in_it() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state_in(dir.path());
    set(&mut state, "vsync", json!(false));
    state.log.push_text("line one\n");
    open(&mut state, Page::Graphics);
    set(&mut state, "ui_language", json!("ru"));
    save(&mut state);
    assert_eq!(state.lang, "ru");
    assert!(shows(&state, "ИГРАТЬ"));
    assert_eq!(state.ini.values["vsync"], "0"); // the settings stay
    assert_eq!(state.page, Page::Graphics);
    assert_eq!(state.drawer, DrawerState::Open(Page::Graphics));
    assert!(state.log.text().contains("line one"));
    assert!(!dirty(&state));
    assert!(!save_enabled(&state));
}

#[test]
fn rebuilding_keeps_the_status_of_a_running_game() {
    // The game state, the log and the release list stay.
    let dir = tempfile::tempdir().unwrap();
    let mut state = state_in(dir.path());
    state.log.push_text("The game is running.\n");
    state.update = UpdateState::Ready(vec![Release {
        id: ReleaseId {
            channel: Channel::Release,
            version: "2.9.0".to_string(),
            build: None,
        },
        published: String::new(),
        url: String::new(),
        page: String::new(),
    }]);
    set(&mut state, "ui_language", json!("ru"));
    save(&mut state);
    assert_eq!(state.lang, "ru");
    assert!(matches!(state.game, GameState::Idle));
    assert!(state.log.text().contains("The game is running."));
    assert!(matches!(state.update, UpdateState::Ready(_)));
    assert_eq!(newer(&state).map(|r| r.id.version.as_str()), Some("2.9.0"));
}

#[test]
fn saving_without_language_or_scale_change_does_not_rebuild() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state_in(dir.path());
    assert!(!state.home.saves);
    // A rebuild reads the disk again: saves that appear now only show after one.
    let savedata = state.paths.user_dir("").join("savedata");
    fs::create_dir_all(&savedata).unwrap();
    fs::write(savedata.join("slot0.dat"), b"save").unwrap();
    set(&mut state, "vsync", json!(false));
    save(&mut state);
    assert!(!dirty(&state));
    assert!(!state.home.saves);
    set(&mut state, "ui_language", json!("pt"));
    save(&mut state);
    assert_eq!(state.lang, "pt");
    assert!(state.home.saves);
}

#[test]
fn repeated_language_switching_preserves_the_page() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state_in(dir.path());
    open(&mut state, Page::Advanced);
    let expected_play_texts = [
        ("pt", "JOGAR"),
        ("ru", "ИГРАТЬ"),
        ("zh", "开始游戏"),
        ("en", "PLAY"),
    ];
    for (target, play) in expected_play_texts {
        set(&mut state, "ui_language", json!(target));
        assert!(save_enabled(&state), "{target}");
        click_save(&mut state);
        assert_eq!(state.lang, target);
        assert_eq!(state.page, Page::Advanced);
        assert!(shows(&state, play), "{target}: {play}");
        assert!(!save_enabled(&state), "{target}");
        let config: Value =
            serde_json::from_slice(&fs::read(state.paths.config_file()).unwrap()).unwrap();
        assert_eq!(config["ui_language"], json!(target));
    }
}

#[test]
fn picking_a_language_in_the_menu_saves_it() {
    use spartanport_launcher::ui::lang_menu::LangMessage;
    let dir = tempfile::tempdir().unwrap();
    let mut state = state_in(dir.path());
    send(
        &mut state,
        Message::Lang(LangMessage::Pick("pt".to_string())),
    );
    assert_eq!(state.lang, "pt");
    assert!(!dirty(&state));
    let config: Value =
        serde_json::from_slice(&fs::read(state.paths.config_file()).unwrap()).unwrap();
    assert_eq!(config["ui_language"], json!("pt"));
}

fn make_mod(dir: &Path, name: &str) {
    let folder = dir.join("mods").join(name).join("EXEC");
    fs::create_dir_all(&folder).unwrap();
    fs::write(folder.join("file.bin"), b"data").unwrap();
}

#[test]
fn visiting_the_mods_page_is_not_a_change() {
    let dir = tempfile::tempdir().unwrap();
    make_mod(dir.path(), "a");
    let mut state = state_in(dir.path());
    assert_eq!(state.mods.order, ["a"]);
    // What the page's refresh brings back on a visit.
    let view = mods::load(&state.paths, &state.settings);
    send(
        &mut state,
        Message::Mods(ModsMessage::Loaded(Box::new(view))),
    );
    assert!(!dirty(&state));
}

#[test]
fn saving_writes_the_mod_order_as_python_json_does() {
    let dir = tempfile::tempdir().unwrap();
    make_mod(dir.path(), "b");
    make_mod(dir.path(), "Мод");
    let mut state = state_in(dir.path());
    assert_eq!(state.mods.order, ["b", "Мод"]);
    send(&mut state, Message::Mods(ModsMessage::Toggle("b".into())));
    assert!(dirty(&state));
    save(&mut state);
    // json.dumps({'order': ['b', 'Мод'], 'disabled': ['b']}, indent=2)
    assert_eq!(
        fs::read_to_string(state.paths.mods_json()).unwrap(),
        "{\n  \"order\": [\n    \"b\",\n    \"\\u041c\\u043e\\u0434\"\n  ],\n  \"disabled\": [\n    \"b\"\n  ]\n}"
    );
    assert!(!state.paths.patches_json().exists());
    assert!(!dirty(&state));
}

#[test]
fn saving_patches_keeps_the_choice_of_other_game_versions() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state_in(dir.path());
    fs::write(
        state.paths.patches_json(),
        r#"{"enabled": ["v1.00 patch"], "disabled": ["shown on", "zz other"]}"#,
    )
    .unwrap();
    let meta = |name: &str| PatchMeta {
        name: name.to_string(),
        author: None,
        note: None,
        enabled_by_default: false,
    };
    // The third-party patches the Mods page lists for this game version.
    for key in ["shown on", "shown off"] {
        state
            .mods
            .external
            .push((key.to_string(), dir.path().join(key), meta(key)));
    }
    state.mods.external_enabled.insert("shown on".to_string());
    assert!(dirty(&state));
    save(&mut state);
    assert_eq!(
        fs::read_to_string(state.paths.patches_json()).unwrap(),
        "{\n  \"enabled\": [\n    \"shown on\",\n    \"v1.00 patch\"\n  ],\n  \"disabled\": [\n    \
         \"shown off\",\n    \"zz other\"\n  ]\n}"
    );
    assert!(!dirty(&state));
}

const HIDDEN_UPSCALING_INI: &str = "upscaler=fsr4\npreset=3\nfsr1=1\nsharpen=1\nsharpness=0.73\n\
     rcas=1\nrcas_strength=42\nobject_motion=0\noutput_res=2560x1440\n";

#[test]
fn hidden_settings_are_preserved_without_migration() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("spartanport.ini"), HIDDEN_UPSCALING_INI).unwrap();
    let mut state = state_in(dir.path());
    let before = load_ini(&state.paths.ini_path()).values;
    for key in [
        "upscaler",
        "preset",
        "fsr1",
        "sharpen",
        "sharpness",
        "rcas",
        "rcas_strength",
        "object_motion",
        "output_res",
        "live_resolution",
    ] {
        assert!(!SHOWN_INI_KEYS.contains(&key), "{key}");
    }
    assert!(!dirty(&state));
    set(&mut state, "vsync", json!(false));
    save(&mut state);
    let after = load_ini(&state.paths.ini_path()).values;
    for (key, value) in &before {
        if key != "vsync" {
            assert_eq!(&after[key], value, "{key}");
        }
    }
    // A parked key the file lacked is not written either.
    let text = fs::read_to_string(state.paths.ini_path()).unwrap();
    assert!(!text.contains("live_resolution"), "{text}");
    set(&mut state, "ui_language", json!("ru"));
    save(&mut state);
    assert_eq!(state.lang, "ru");
    assert_eq!(load_ini(&state.paths.ini_path()).values["upscaler"], "fsr4");
}

#[test]
fn save_preserves_external_changes_and_refreshes_the_controls() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state_in(dir.path());
    save(&mut state);
    let path = state.paths.ini_path();
    let text = fs::read_to_string(&path).unwrap();
    assert!(text.contains("fps_limit=0"), "{text}");
    fs::write(
        &path,
        text.replace("fps_limit=0", "fps_limit=60") + "external_runtime_key=keep\n",
    )
    .unwrap();
    set(&mut state, "vsync", json!(false));
    save(&mut state);
    let values = load_ini(&path).values;
    assert_eq!(values["fps_limit"], "60");
    assert_eq!(values["external_runtime_key"], "keep");
    assert_eq!(values["vsync"], "0");
    assert_eq!(state.ini.values["fps_limit"], "60");
    assert!(!dirty(&state));
}

#[test]
fn partial_save_failure_keeps_pending_state() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state_in(dir.path());
    save(&mut state);
    set(&mut state, "vsync", json!(false));
    // settings.json cannot be replaced: a folder holds its name.
    let config = state.paths.config_file();
    fs::remove_file(&config).unwrap();
    fs::create_dir(&config).unwrap();
    save(&mut state);
    let modal = state.modal.take().expect("an error dialog");
    assert!(matches!(modal.kind, ModalKind::Error));
    assert_eq!(modal.text, tr("en", SAVE_FAILED));
    assert!(dirty(&state));
    assert!(temp_files(dir.path()).is_empty());
    fs::remove_dir(&config).unwrap();
    save(&mut state);
    assert!(state.modal.is_none());
    assert!(config.is_file());
    assert!(!dirty(&state));
}

#[cfg(unix)]
#[test]
fn failed_save_keeps_pending_state() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let mut state = state_in(dir.path());
    save(&mut state);
    let ini = state.paths.ini_path();
    let config = state.paths.config_file();
    let before = (fs::read(&ini).unwrap(), fs::read(&config).unwrap());
    set(&mut state, "vsync", json!(false));
    fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o555)).unwrap();
    // A superuser writes anyway: nothing to test then.
    let probe = dir.path().join("probe");
    if fs::write(&probe, b"").is_ok() {
        let _ = fs::remove_file(&probe);
        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o755)).unwrap();
        return;
    }
    save(&mut state);
    fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o755)).unwrap();
    let modal = state.modal.take().expect("an error dialog");
    assert!(matches!(modal.kind, ModalKind::Error));
    assert_eq!(
        (fs::read(&ini).unwrap(), fs::read(&config).unwrap()),
        before
    );
    assert!(dirty(&state));
    assert!(temp_files(dir.path()).is_empty());
    save(&mut state);
    assert!(!dirty(&state));
}

/// A valid value of a setting that differs from `current`.
fn other_value(dir: &Path, key: &str, current: &Value) -> Value {
    match current {
        Value::Bool(on) => json!(!on),
        _ => {
            let text = current.as_str().unwrap_or_default();
            if key == "ui_language" {
                json!("pt")
            } else if key == "ui_scale" {
                json!("1.5")
            } else if key.ends_with("_dir") {
                json!(dir.join(key).display().to_string())
            } else if !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit()) {
                json!((text.parse::<u64>().unwrap() + 5).to_string())
            } else {
                json!(format!("{key}-changed"))
            }
        }
    }
}

/// The button's state is `dirty` (the tests above click it); here it is read directly, as drawing
/// the window for each of the settings would take most of a minute in a debug build.
fn change_and_save(state: &mut State, name: &str, message: Message) {
    send(state, message);
    assert!(dirty(state), "{name}: Save stays inactive after a change");
    save(state);
    assert!(state.modal.is_none(), "{name}: the save failed");
    assert!(!dirty(state), "{name}: Save stays active after saving");
}

#[test]
fn every_setting_lights_save_persists_and_comes_back() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state_in(dir.path());
    let mut expected_ini = Vec::new();
    for key in SHOWN_INI_KEYS {
        let current = state.ini.values[*key].clone();
        let value = if INI_FLAGS.contains(key) {
            if current == "1" { "0" } else { "1" }.to_string()
        } else {
            other_value(dir.path(), key, &json!(current))
                .as_str()
                .unwrap()
                .to_string()
        };
        change_and_save(&mut state, key, Message::Set(key.to_string(), json!(value)));
        expected_ini.push((*key, value));
    }
    let defaults = app_defaults(&|_| None);
    let mut expected_app = Vec::new();
    for (key, default) in &defaults {
        // The controller type is the Controls page's own (below); objects and numbers have no box.
        if key == "pad_style" || !(default.is_boolean() || default.is_string()) {
            continue;
        }
        let current = state.settings.get(key).cloned().unwrap_or(Value::Null);
        let value = other_value(dir.path(), key, &current);
        change_and_save(&mut state, key, Message::Set(key.clone(), value.clone()));
        expected_app.push((key.clone(), value));
    }
    change_and_save(
        &mut state,
        "pad_style",
        Message::Controls(ControlsMessage::PadStyle("xbox".into())),
    );
    send(
        &mut state,
        Message::Controls(ControlsMessage::Capture("cross".into())),
    );
    change_and_save(
        &mut state,
        "key cross",
        Message::Controls(ControlsMessage::Captured("cross".into(), "Q".into())),
    );
    change_and_save(
        &mut state,
        "pad cross",
        Message::Controls(ControlsMessage::PadPick("cross".into(), "b".into())),
    );

    let reopened = state_in(dir.path());
    for (key, value) in &expected_ini {
        assert_eq!(&reopened.ini.values[*key], value, "{key}");
    }
    for (key, value) in &expected_app {
        assert_eq!(reopened.settings.get(key), Some(value), "{key}");
    }
    assert_eq!(reopened.controls.pad_style, "xbox");
    assert_eq!(reopened.controls.key["cross"], "Q");
    assert_eq!(reopened.controls.pad["cross"], "b");
    assert!(!dirty(&reopened));
}

/// A folder in the place of `settings.json`, so no save can replace it.
fn block_settings(state: &State) {
    fs::create_dir(state.paths.config_file()).unwrap();
}

fn save_failed_shown(state: &State) -> bool {
    state
        .modal
        .as_ref()
        .is_some_and(|modal| matches!(modal.kind, ModalKind::Error) && modal.text == SAVE_FAILED)
}

#[test]
fn closing_the_window_saves_first() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state_in(dir.path());
    set(&mut state, "vsync", json!(false));
    // Python `close`: collect, then destroy the window.
    let task = app::update(
        &mut state,
        Message::CloseRequested(iced::window::Id::unique()),
    );
    let ini = fs::read_to_string(state.paths.ini_path()).unwrap();
    assert!(ini.lines().any(|line| line == "vsync=0"), "{ini}");
    assert!(state.modal.is_none());
    assert!(!dirty(&state));
    assert_eq!(task.units(), 1, "the window closes");
}

#[test]
fn a_failed_save_keeps_the_window_open() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state_in(dir.path());
    set(&mut state, "vsync", json!(false));
    block_settings(&state);
    let task = app::update(
        &mut state,
        Message::CloseRequested(iced::window::Id::unique()),
    );
    assert_eq!(task.units(), 0, "the window stays");
    assert!(save_failed_shown(&state), "{:?}", state.modal);
    assert!(dirty(&state));
}

#[test]
fn opening_the_ini_saves_first() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state_in(dir.path());
    open(&mut state, Page::Advanced);
    // A window tall enough for the whole page: the button sits lower with Linux's fonts.
    let sent: Vec<Message> = {
        let mut ui = Simulator::with_size(
            iced::Settings::default(),
            iced::Size::new(1280.0, 4000.0),
            app::view(&state),
        );
        ui.click("spartanport.ini").unwrap();
        ui.into_messages().collect()
    };
    assert!(matches!(sent.as_slice(), [Message::OpenIni]), "{sent:?}");
    set(&mut state, "vsync", json!(false));
    // Python `open_path`: an `.ini` is collected first, then opened.
    let task = app::update(&mut state, Message::OpenIni);
    let ini = fs::read_to_string(state.paths.ini_path()).unwrap();
    assert!(ini.lines().any(|line| line == "vsync=0"), "{ini}");
    assert_eq!(task.units(), 1, "the ini opens");

    set(&mut state, "vsync", json!(true));
    fs::remove_file(state.paths.config_file()).unwrap();
    block_settings(&state);
    let task = app::update(&mut state, Message::OpenIni);
    assert_eq!(task.units(), 0, "nothing opens after a failed save");
    assert!(save_failed_shown(&state), "{:?}", state.modal);
}

#[test]
fn add_to_steam_saves_first() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state_in(dir.path());
    // No eboot.bin there: adding to Steam answers at once.
    set(
        &mut state,
        "game_dir",
        json!(dir.path().join("game").display().to_string()),
    );
    let task = app::update(&mut state, Message::AddToSteam);
    let saved: Value =
        serde_json::from_slice(&fs::read(state.paths.config_file()).unwrap()).unwrap();
    assert_eq!(
        saved["game_dir"],
        json!(dir.path().join("game").display().to_string())
    );
    assert_eq!(task.units(), 1, "adding to Steam starts");

    set(&mut state, "game_dir", json!(""));
    fs::remove_file(state.paths.config_file()).unwrap();
    block_settings(&state);
    let task = app::update(&mut state, Message::AddToSteam);
    assert_eq!(task.units(), 0, "no Add to Steam after a failed save");
    assert!(save_failed_shown(&state), "{:?}", state.modal);
}

/// An ini that is not UTF-8: a Latin-1 comment, a parked key and a key of the player's own.
const LATIN1_INI: &[u8] = b"# caf\xe9 au lait\nupscaler=fsr4\nmy_key=1\nvsync=1\n";

#[test]
fn unreadable_settings_files_warn_once_at_start() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("settings.json"), "{not json").unwrap();
    fs::write(dir.path().join("spartanport.ini"), LATIN1_INI).unwrap();
    let state = state_in(dir.path());
    // One dialog names both files.
    let shown = state.modal.clone().expect("a warning");
    assert!(matches!(shown.kind, ModalKind::Error), "{shown:?}");
    for file in [state.paths.config_file(), state.paths.ini_path()] {
        assert!(
            shown.text.contains(&file.display().to_string()),
            "{}",
            shown.text
        );
    }
    assert!(
        shown
            .text
            .contains("The files stay as they are until you save"),
        "{}",
        shown.text
    );
    // The defaults are shown and nothing is written before a save.
    assert_eq!(state.settings.get_str("ui_language"), "en");
    assert_eq!(fs::read(state.paths.config_file()).unwrap(), b"{not json");
    assert_eq!(fs::read(state.paths.ini_path()).unwrap(), LATIN1_INI);
    assert!(!dirty(&state));

    let dir = tempfile::tempdir().unwrap();
    assert!(
        state_in(dir.path()).modal.is_none(),
        "missing files are fine"
    );
}

/// A state over a corrupt `settings.json` and an ini that is not UTF-8, its warning dismissed.
fn state_over_unreadable_files(dir: &Path) -> State {
    fs::write(dir.join("settings.json"), "{not json").unwrap();
    fs::write(dir.join("spartanport.ini"), LATIN1_INI).unwrap();
    let mut state = state_in(dir);
    assert!(state.modal.take().is_some(), "the start-up warning");
    state
}

fn files_untouched(state: &State) {
    let read = |file| String::from_utf8_lossy(&fs::read(file).unwrap()).into_owned();
    assert_eq!(read(state.paths.config_file()), "{not json");
    assert_eq!(
        read(state.paths.ini_path()),
        String::from_utf8_lossy(LATIN1_INI)
    );
}

#[test]
fn closing_keeps_an_unreadable_file_until_a_save() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state_over_unreadable_files(dir.path());
    set(&mut state, "vsync", json!(false));
    // Spec §6: the file stays as it is until the user saves.
    let task = app::update(
        &mut state,
        Message::CloseRequested(iced::window::Id::unique()),
    );
    assert_eq!(task.units(), 1, "the window closes");
    assert!(state.modal.is_none());
    files_untouched(&state);

    // Once saved, the files hold what the window shows, and closing saves again (Python `close`).
    save(&mut state);
    set(&mut state, "vsync", json!(true));
    let _ = app::update(
        &mut state,
        Message::CloseRequested(iced::window::Id::unique()),
    );
    let ini = fs::read_to_string(state.paths.ini_path()).unwrap();
    assert!(ini.lines().any(|line| line == "vsync=1"), "{ini}");
}

#[test]
fn opening_the_ini_or_adding_to_steam_keeps_an_unreadable_file() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state_over_unreadable_files(dir.path());
    set(&mut state, "vsync", json!(false));
    let task = app::update(&mut state, Message::OpenIni);
    assert_eq!(task.units(), 1, "the ini opens");
    let task = app::update(&mut state, Message::AddToSteam);
    assert_eq!(task.units(), 1, "adding to Steam starts");
    assert!(state.modal.is_none());
    files_untouched(&state);
}

#[test]
fn a_save_keeps_the_lines_of_an_ini_that_is_not_utf8() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("spartanport.ini"), LATIN1_INI).unwrap();
    let mut state = state_in(dir.path());
    state.modal = None;
    set(&mut state, "vsync", json!(false));
    save(&mut state);
    assert!(state.modal.is_none());
    // Its lines stay in place (the byte that is not UTF-8 becomes U+FFFD); the shown keys it
    // lacks are appended, as for any file.
    let text = fs::read_to_string(state.paths.ini_path()).unwrap();
    assert!(
        text.starts_with("# caf\u{fffd} au lait\nupscaler=fsr4\nmy_key=1\nvsync=0\n"),
        "{text}"
    );
    assert!(text.lines().any(|line| line == "fps_limit=0"), "{text}");
}
