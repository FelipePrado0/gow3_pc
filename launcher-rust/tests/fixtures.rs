#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Parity tests: the Rust core must reproduce what the Python launcher wrote into `fixtures/`.

use std::fs;
use std::path::PathBuf;

use indexmap::IndexMap;
use spartanport_launcher::core::controls::migrate_controls;
use spartanport_launcher::core::env::{environment_of, game_environment};
use spartanport_launcher::core::ini::{load_ini, save_ini};
use spartanport_launcher::core::paths::Paths;
use spartanport_launcher::core::settings::{app_defaults, save_settings, Settings};

fn fixture_dir(case: &str) -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures")).join(case)
}

fn load_input(case: &str) -> serde_json::Value {
    let text = fs::read_to_string(fixture_dir(case).join("input.json")).unwrap();
    serde_json::from_str(&text).unwrap()
}

/// A default the user changed on purpose after the Python 2.6.0 fixtures were generated.
struct DecidedDefault {
    /// The `settings.json` key.
    key: &'static str,
    /// Its default as the Python writes it (JSON text) and as the Rust launcher does.
    python: &'static str,
    rust: &'static str,
    /// The variable it gives the game, with the Python's value and the Rust one.
    env: Option<(&'static str, &'static str, &'static str)>,
}

/// The only differences from the Python's defaults the fixtures allow, each a user's ruling. They
/// apply to a case whose input neither stores nor edits the key, so the file holds the default.
const DECIDED_DEFAULTS: &[DecidedDefault] = &[
    // R90: GUI Scale is 1x or 1.5x, 1x by default (the Python's "auto" followed the desktop).
    DecidedDefault {
        key: "ui_scale",
        python: r#""auto""#,
        rust: r#""1""#,
        env: None,
    },
    // R91: Discord Rich Presence is on by default, so the game gets SPARTAN_DISCORD=1.
    DecidedDefault {
        key: "discord_rpc",
        python: "false",
        rust: "true",
        env: Some(("SPARTAN_DISCORD", "0", "1")),
    },
];

/// The decided defaults the case `input` leaves at their default.
fn decided_defaults(
    input: &serde_json::Value,
) -> impl Iterator<Item = &'static DecidedDefault> + '_ {
    DECIDED_DEFAULTS.iter().filter(|decided| {
        input["settings"].get(decided.key).is_none()
            && input["edits"]["app"].get(decided.key).is_none()
    })
}

/// The Python's `settings.json` of `case` with the decided defaults in place of its own.
fn expected_settings(case: &str, input: &serde_json::Value) -> String {
    let path = fixture_dir(case).join("settings.json");
    let mut text = String::from_utf8(fs::read(path).unwrap()).unwrap();
    for decided in decided_defaults(input) {
        let python = format!(r#""{}": {}"#, decided.key, decided.python);
        assert_eq!(text.matches(&python).count(), 1, "{case}: {python}");
        text = text.replacen(
            &python,
            &format!(r#""{}": {}"#, decided.key, decided.rust),
            1,
        );
    }
    text
}

#[test]
fn ini_matches_python_for_case_edited() {
    let input = load_input("edited");
    let mut text = String::new();
    for line in input["ini_lines"].as_array().unwrap() {
        text.push_str(line.as_str().unwrap());
        text.push('\n');
    }
    let edits: IndexMap<String, String> =
        serde_json::from_value(input["edits"]["ini"].clone()).unwrap();

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("spartanport.ini");
    fs::write(&path, text).unwrap();
    let ini = load_ini(&path);
    save_ini(&path, &edits, &ini.lines).unwrap();

    let expected = fs::read(fixture_dir("edited").join("spartanport.ini")).unwrap();
    assert_eq!(fs::read(&path).unwrap(), expected);
}

fn settings_bytes_for_case(case: &str) -> Vec<u8> {
    let input = load_input(case);
    let env = |key: &str| input["env"][key].as_str().map(str::to_string);
    let mut merged = app_defaults(&env);
    let saved = input["settings"].as_object().unwrap().clone();
    for (key, value) in migrate_controls(saved) {
        merged.insert(key, value);
    }
    let mut settings = Settings(merged);
    for (key, value) in input["edits"]["app"].as_object().unwrap() {
        settings.set(key, value.clone());
    }
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    save_settings(&path, &settings).unwrap();
    fs::read(&path).unwrap()
}

/// `settings.json` of `case` as the Rust launcher writes it, against the Python's.
fn assert_settings_match(case: &str) {
    let got = String::from_utf8(settings_bytes_for_case(case)).unwrap();
    assert_eq!(got, expected_settings(case, &load_input(case)));
}

#[test]
fn settings_match_python_for_case_defaults() {
    assert_settings_match("defaults");
}

#[test]
fn settings_match_python_for_case_edited() {
    assert_settings_match("edited");
}

#[test]
fn legacy_controls_settings_match_python() {
    assert_settings_match("legacy_controls");
}

/// The Save flow against `fixtures/collected/`: what the Python launcher's own `collect()` wrote for
/// the `edited` input (`tools/launcher_fixtures.py`), not the `edited` case, which saves the ini edits
/// and the app dict without the window. The launcher opens over the input's `settings.json` and
/// `spartanport.ini`, lists no monitors (the generator's probe lists none), takes the edits through
/// `Set` and collects.
#[test]
fn collect_matches_python_for_edited_case() {
    use spartanport_launcher::ui::app::{self, Message, State};
    use spartanport_launcher::ui::persist::collect;

    let input = load_input("collected");
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().to_string_lossy().into_owned();
    fs::write(
        dir.path().join("settings.json"),
        serde_json::to_string_pretty(&input["settings"]).unwrap(),
    )
    .unwrap();
    let mut ini = String::new();
    for line in input["ini_lines"].as_array().unwrap() {
        ini.push_str(line.as_str().unwrap());
        ini.push('\n');
    }
    fs::write(dir.path().join("spartanport.ini"), ini).unwrap();
    // The input's settings name the game folder, so the SPARTAN_GAME_DIR default never shows.
    let env = move |key: &str| match key {
        "SPARTAN_DATA_DIR" | "APPDATA" | "USERPROFILE" | "HOME" | "XDG_CONFIG_HOME" => {
            Some(data.clone())
        }
        _ => None,
    };
    let mut state = State::new(Paths::from_env(dir.path(), &env));
    let _ = app::update(&mut state, Message::MonitorsListed(Vec::new()));
    for (key, value) in input["edits"]["app"].as_object().unwrap() {
        let _ = app::update(&mut state, Message::Set(key.clone(), value.clone()));
    }
    for (key, value) in input["edits"]["ini"].as_object().unwrap() {
        let _ = app::update(&mut state, Message::Set(key.clone(), value.clone()));
    }
    collect(&mut state).unwrap();

    let expected = fixture_dir("collected");
    for name in ["settings.json", "spartanport.ini"] {
        // Byte for byte; as text so a failure shows the differing lines.
        let got = String::from_utf8(fs::read(dir.path().join(name)).unwrap()).unwrap();
        let want = if name == "settings.json" {
            expected_settings("collected", &input)
        } else {
            String::from_utf8(fs::read(expected.join(name)).unwrap()).unwrap()
        };
        assert_eq!(got, want, "{name}");
    }
    for name in ["mods.json", "patches.json"] {
        assert_eq!(
            dir.path().join(name).is_file(),
            expected.join(name).is_file(),
            "{name}"
        );
    }
}

fn environment_for_case(case: &str) -> (IndexMap<String, String>, String) {
    let input = load_input(case);
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().to_string_lossy().into_owned();
    let data_for_paths = data.clone();
    let path_env = move |key: &str| (key == "SPARTAN_DATA_DIR").then(|| data_for_paths.clone());
    let paths = Paths::from_env(dir.path(), &path_env);
    let env = |key: &str| input["env"][key].as_str().map(str::to_string);
    let mut merged = app_defaults(&env);
    let saved = input["settings"].as_object().unwrap().clone();
    for (key, value) in migrate_controls(saved) {
        merged.insert(key, value);
    }
    let mut settings = Settings(merged);
    for (key, value) in input["edits"]["app"].as_object().unwrap() {
        settings.set(key, value.clone());
    }
    // The base production gives the game: the whole environment, here the Python's.
    let base = environment_of(
        input["env"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str().unwrap())),
    );
    let lang = settings.get_str("ui_language");
    let lang = if lang.is_empty() {
        "en".to_string()
    } else {
        lang
    };
    let result = game_environment(&settings, &paths, &lang, &base, true);
    let kept = result
        .into_iter()
        .filter(|(k, _)| {
            ["SPARTAN_", "SDL_", "PYTHON"]
                .iter()
                .any(|p| k.starts_with(p))
        })
        .collect();
    (kept, data)
}

fn assert_environment_matches(case: &str) {
    let (actual, data) = environment_for_case(case);
    let mut expected: IndexMap<String, String> = serde_json::from_str(
        &fs::read_to_string(fixture_dir(case).join("environment.json")).unwrap(),
    )
    .unwrap();
    for decided in decided_defaults(&load_input(case)) {
        if let Some((name, python, rust)) = decided.env {
            assert_eq!(
                expected.get(name).map(String::as_str),
                Some(python),
                "{case}"
            );
            expected.insert(name.to_string(), rust.to_string());
        }
    }
    let norm = |s: &str| s.replace(['\\', '/'], std::path::MAIN_SEPARATOR_STR);
    for (key, want) in &expected {
        let got = actual.get(key);
        let matches = if want.contains("<DATA_DIR>") {
            let want = want.replace("<DATA_DIR>", &data);
            got.is_some_and(|g| norm(g) == norm(&want))
        } else {
            got == Some(want)
        };
        assert!(matches, "{case}: {key}: expected {want:?}, got {got:?}");
    }
    for key in actual.keys() {
        assert!(expected.contains_key(key), "{case}: unexpected key {key}");
    }
}

#[test]
fn environment_matches_python_for_case_defaults() {
    assert_environment_matches("defaults");
}

#[test]
fn environment_matches_python_for_case_edited() {
    assert_environment_matches("edited");
}

#[test]
fn environment_matches_python_for_case_legacy_controls() {
    assert_environment_matches("legacy_controls");
}
