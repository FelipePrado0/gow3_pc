//! The launcher's own settings (`settings.json`): defaults, load, save and the portable migration.

use std::fs;
use std::io;
use std::path::Path;

use serde_json::{json, Map, Value};

use crate::core::controls::migrate_controls;
use crate::core::fs::{atomic_write, read_json};
use crate::core::paths::Paths;

/// The saved settings, in file order; keys this version does not know are kept.
#[derive(Debug, Clone, PartialEq)]
pub struct Settings(pub Map<String, Value>);

impl Settings {
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.0.get(key)
    }

    /// Strings as-is, numbers as their text, anything else empty.
    pub fn get_str(&self, key: &str) -> String {
        match self.0.get(key) {
            Some(Value::String(s)) => s.clone(),
            Some(Value::Number(n)) => n.to_string(),
            _ => String::new(),
        }
    }

    /// `true`, the number 1 or the string "1".
    pub fn get_bool(&self, key: &str) -> bool {
        match self.0.get(key) {
            Some(Value::Bool(b)) => *b,
            Some(Value::Number(n)) => n.as_i64() == Some(1) || n.as_f64() == Some(1.0),
            Some(Value::String(s)) => s == "1",
            _ => false,
        }
    }

    /// Replacing a key keeps its position; a new key goes last.
    pub fn set(&mut self, key: &str, value: Value) {
        self.0.insert(key.to_string(), value);
    }
}

/// Every setting with its default, in the order the Python launcher writes them. The game folder
/// starts empty on every system unless `SPARTAN_GAME_DIR` names one (ruling R83: the Python
/// guesses `<port parent>/CUSA01623` on Windows).
pub fn app_defaults(env: &dyn Fn(&str) -> Option<String>) -> Map<String, Value> {
    let game_dir = env("SPARTAN_GAME_DIR").unwrap_or_default();
    let defaults = json!({
        "ui_language": "en",
        "ui_scale": "1",
        "game_dir": game_dir,
        "update_dir": "",
        "experimental_upscaler": false,
        "video_backend": "auto",
        "gamemode": false,
        "mangohud": false,
        "desktop_integration": true,
        "user_dir": "",
        "mods_dir": "",
        "mods_enabled": true,
        "patches_dir": "",
        "language": "1",
        "hdr": false,
        "draw_pipe": "",
        "readbacks": "",
        "no_msaa": "",
        "stream_memory": "cached",
        "gpu_id": "",
        "monitor": "",
        "partial_height_views": true,
        "disk_shaders": true,
        "shader_prefetch": true,
        "shader_notes": false,
        "background_warmup": true,
        "parallel_warmup": true,
        "pipeline_binary_cache": true,
        "perf_diag": false,
        "frame_capture_key": false,
        "frame_stats": false,
        "gpu_profile": false,
        "vk_validation": false,
        "close_on_play": false,
        "discord_rpc": true,
        "check_updates": false,
        "game_patches": {},
        "controls": {},
        "pad_style": "playstation",
        "rage_one_input": true,
        "stick_prompts": "keyboard",
        "controls_layout": 2,
        "glyph_style": "off",
    });
    match defaults {
        Value::Object(map) => map,
        _ => Map::new(),
    }
}

/// Copies the previous launcher's `settings.json` into the data folder, once. Never overwrites;
/// every error is ignored. (`game_icon.ico` is not copied: the runner writes the window icon.)
pub fn migrate_portable_settings(target: &Path, legacy: &Path) {
    if target == legacy {
        return;
    }
    copy_if_missing(legacy, target);
}

fn copy_if_missing(from: &Path, to: &Path) {
    if to.is_file() || !from.is_file() {
        return;
    }
    if let Some(parent) = to.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::copy(from, to);
}

/// Defaults overlaid with the saved file; see `read_app_settings`.
pub fn load_app_settings(paths: &Paths) -> Settings {
    read_app_settings(paths).0
}

/// What `read_app_settings` found at `paths.config_file()` after the portable migration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsFile {
    Missing,
    Read,
    Unreadable,
}

/// Defaults overlaid with the saved file, and what the file was: `Unreadable` is there but an I/O
/// error, invalid JSON or not a JSON object (spec §6 warns once). Such a file gives the defaults
/// and is left untouched until the next save; a `Missing` one is no problem.
pub fn read_app_settings(paths: &Paths) -> (Settings, SettingsFile) {
    let config = paths.config_file();
    migrate_portable_settings(&config, &paths.legacy_config_file());
    let (saved, file) = match read_json::<Value>(&config) {
        Ok(Some(Value::Object(map))) => (map, SettingsFile::Read),
        Ok(None) => (Map::new(), SettingsFile::Missing),
        Ok(Some(_)) | Err(_) => (Map::new(), SettingsFile::Unreadable),
    };
    let mut merged =
        app_defaults(&|k| (k == "SPARTAN_GAME_DIR").then(|| paths.game_dir_default().to_string()));
    for (key, value) in migrate_controls(saved) {
        merged.insert(key, value);
    }
    (Settings(merged), file)
}

/// Writes the settings as 2-space JSON without a trailing newline.
pub fn save_settings(path: &Path, settings: &Settings) -> io::Result<()> {
    let text = serde_json::to_string_pretty(&settings.0).map_err(io::Error::other)?;
    atomic_write(path, text.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::paths::Paths;
    use serde_json::json;
    use std::fs;

    fn no_env(_: &str) -> Option<String> {
        None
    }

    #[test]
    fn defaults_are_in_python_order() {
        let defaults = app_defaults(&no_env);
        assert_eq!(defaults.len(), 43);
        assert_eq!(defaults.keys().next().unwrap(), "ui_language");
        assert_eq!(defaults.keys().next_back().unwrap(), "glyph_style");
        // The 2.7.0 shader cache switches sit between the screen effects and the warm-up.
        let keys: Vec<&str> = defaults.keys().map(String::as_str).collect();
        let at = keys
            .iter()
            .position(|key| *key == "partial_height_views")
            .unwrap();
        assert_eq!(
            &keys[at..at + 5],
            [
                "partial_height_views",
                "disk_shaders",
                "shader_prefetch",
                "shader_notes",
                "background_warmup"
            ]
        );
        assert_eq!(defaults["disk_shaders"], json!(true));
        assert_eq!(defaults["shader_prefetch"], json!(true));
        assert_eq!(defaults["shader_notes"], json!(false));
        // Ruling R83: no folder is guessed, on any system.
        assert_eq!(defaults["game_dir"], json!(""));
        // Ruling R90: GUI Scale starts at 1x (the Python's "auto" followed the desktop).
        assert_eq!(defaults["ui_scale"], json!("1"));
        // Ruling R91: Discord Rich Presence starts on.
        assert_eq!(defaults["discord_rpc"], json!(true));
        let env = |k: &str| (k == "SPARTAN_GAME_DIR").then(|| "/games".to_string());
        assert_eq!(app_defaults(&env)["game_dir"], json!("/games"));
    }

    #[test]
    fn unknown_keys_survive_a_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let saved = json!({"zzz_future": {"a": [1, 2]}, "ui_language": "pt"});
        fs::write(&path, saved.to_string()).unwrap();
        let paths = Paths::from_env(dir.path(), &|k| {
            (k == "SPARTAN_DATA_DIR").then(|| dir.path().to_string_lossy().into_owned())
        });
        let mut settings = load_app_settings(&paths);
        assert_eq!(settings.get_str("ui_language"), "pt");
        settings.set("hdr", json!(true));
        save_settings(&path, &settings).unwrap();
        let back: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(back["zzz_future"], json!({"a": [1, 2]}));
        assert_eq!(back["hdr"], json!(true));
        assert_eq!(
            back.as_object().unwrap().keys().next_back().unwrap(),
            "zzz_future"
        );
    }

    #[test]
    fn loading_a_json_array_or_string_falls_back_to_defaults() {
        for text in ["[]", "\"x\""] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("settings.json");
            fs::write(&path, text).unwrap();
            let data = dir.path().to_string_lossy().into_owned();
            let paths = Paths::from_env(dir.path(), &|k| match k {
                "SPARTAN_DATA_DIR" => Some(data.clone()),
                "APPDATA" | "XDG_CONFIG_HOME" => Some(format!("{data}/nowhere")),
                _ => None,
            });
            let settings = load_app_settings(&paths);
            let defaults = app_defaults(&|_| None);
            assert_eq!(settings.0, defaults);
            assert_eq!(fs::read(&path).unwrap(), text.as_bytes());
        }
    }

    #[test]
    fn an_unreadable_settings_file_is_reported_and_left_as_it_is() {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().to_string_lossy().into_owned();
        let paths = Paths::from_env(dir.path(), &|k| {
            (k == "SPARTAN_DATA_DIR").then(|| data.clone())
        });
        // Missing: the defaults, nothing to report.
        let (settings, file) = read_app_settings(&paths);
        assert_eq!(file, SettingsFile::Missing);
        assert_eq!(settings.get_str("ui_language"), "en");
        for broken in ["{not json", "[1, 2]", ""] {
            fs::write(paths.config_file(), broken).unwrap();
            let (settings, file) = read_app_settings(&paths);
            assert_eq!(file, SettingsFile::Unreadable, "{broken:?}");
            assert_eq!(settings.get_str("ui_language"), "en", "{broken:?}");
            assert_eq!(fs::read_to_string(paths.config_file()).unwrap(), broken);
        }
        fs::write(paths.config_file(), r#"{"ui_language": "pt"}"#).unwrap();
        let (settings, file) = read_app_settings(&paths);
        assert_eq!(file, SettingsFile::Read);
        assert_eq!(settings.get_str("ui_language"), "pt");
    }

    #[test]
    fn a_missing_file_is_told_apart_from_an_unreadable_one() {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().to_string_lossy().into_owned();
        let paths = Paths::from_env(dir.path(), &|k| match k {
            "SPARTAN_DATA_DIR" => Some(data.clone()),
            "APPDATA" | "XDG_CONFIG_HOME" => Some(format!("{data}/nowhere")),
            _ => None,
        });
        assert_eq!(read_app_settings(&paths).1, SettingsFile::Missing);
        for broken in ["{not json", "[1, 2]", ""] {
            fs::write(paths.config_file(), broken).unwrap();
            assert_eq!(
                read_app_settings(&paths).1,
                SettingsFile::Unreadable,
                "{broken:?}"
            );
        }
        fs::write(paths.config_file(), r#"{"ui_language": "pt"}"#).unwrap();
        let (settings, file) = read_app_settings(&paths);
        assert_eq!(file, SettingsFile::Read);
        assert_eq!(settings.get_str("ui_language"), "pt");
    }

    #[test]
    fn a_legacy_settings_file_counts_as_read() {
        // A settings file only at the old location: the portable migration copies it before the
        // file is looked for, so it counts as read.
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        // `Paths::legacy_config_file` is `<APPDATA>/gow3-launcher/settings.json` on Windows and
        // `<XDG_CONFIG_HOME>/gow3-launcher/settings.json` elsewhere.
        let legacy = dir.path().join("legacy").join("gow3-launcher");
        fs::create_dir_all(&legacy).unwrap();
        fs::write(legacy.join("settings.json"), r#"{"ui_language": "de"}"#).unwrap();
        let config_home = dir.path().join("legacy").to_string_lossy().into_owned();
        let paths = Paths::from_env(dir.path(), &|k| match k {
            "SPARTAN_DATA_DIR" => Some(data.to_string_lossy().into_owned()),
            "APPDATA" | "XDG_CONFIG_HOME" => Some(config_home.clone()),
            _ => None,
        });
        let (settings, file) = read_app_settings(&paths);
        assert_eq!(file, SettingsFile::Read);
        assert_eq!(settings.get_str("ui_language"), "de");
        assert!(paths.config_file().is_file(), "the migration copied it");
    }

    #[test]
    fn portable_migration_copies_only_settings() {
        let dir = tempfile::tempdir().unwrap();
        let legacy_dir = dir.path().join("legacy");
        let target_dir = dir.path().join("data");
        fs::create_dir_all(&legacy_dir).unwrap();
        let legacy = legacy_dir.join("settings.json");
        let target = target_dir.join("settings.json");
        fs::write(&legacy, "old").unwrap();
        fs::write(legacy_dir.join("game_icon.ico"), "ico").unwrap();

        migrate_portable_settings(&target, &legacy);
        assert_eq!(fs::read_to_string(&target).unwrap(), "old");
        assert!(
            !target_dir.join("game_icon.ico").exists(),
            "the runner writes the window icon itself"
        );

        fs::write(&legacy, "newer").unwrap();
        migrate_portable_settings(&target, &legacy);
        assert_eq!(fs::read_to_string(&target).unwrap(), "old");

        migrate_portable_settings(&legacy, &legacy);
        assert_eq!(fs::read_to_string(&legacy).unwrap(), "newer");
    }
}
