//! What Save and PLAY write: the window's settings as data (`gather`), the four files (`collect`),
//! whether the window differs from them (`dirty`), the Save button (`save`) and the window in
//! another language (`rebuild`). Ports of the Python launcher's methods of the same names.

use std::collections::BTreeSet;
use std::io::{self, Write};
use std::path::PathBuf;

use iced::Task;
use indexmap::IndexMap;
use serde::Serialize;
use serde_json::ser::{Formatter, PrettyFormatter};
use serde_json::{Map, Value};

use crate::core::controls::{control_changes, Kind};
use crate::core::fs::{atomic_write, load_json};
use crate::core::gpu::{list_displays, monitor_options, monitor_selection, Display};
use crate::core::i18n::{is_rtl, launcher_language, system_language, tr, tr_fmt};
use crate::core::ini::{load_ini, save_ini, INI_FLAGS};
use crate::core::settings::{save_settings, Settings};
use crate::ui::app::{self, Message, State};
use crate::ui::home;
use crate::ui::pages::mods::{self, ModsMessage};
use crate::ui::pages::{game, performance};
use crate::ui::theme;
use crate::ui::widgets::{Modal, ModalKind};
use crate::APP_NAME;

/// The `spartanport.ini` keys the window has a control for, in the order the Python launcher
/// creates their variables (Home, Graphics, Performance, Game, Controls): `collect` writes only
/// these, in this order, so a first save appends the same lines. The parked Upscaling keys
/// (`upscaler`, `preset`, `object_motion`, `output_res`, `live_resolution`) have no control and are
/// never written: a file keeps whatever it holds for them.
pub const SHOWN_INI_KEYS: &[&str] = &[
    "display_mode",
    "fps_limit",
    "engine_fps",
    "render_resolution",
    "vsync",
    "anisotropy",
    "show_fps",
    "frames_queued",
    "async_shaders",
    "stale_readback",
    "deferred_readback",
    "cheat_double_jumps",
    "cheat_very_hard",
    "mouse_prompt_distance",
    "mouse_aim_style",
    "mouse_aim_sensitivity",
    "mouse_lock",
    "vibration",
];

/// The error dialog of a failed save.
const SAVE_FAILED: &str =
    "Could not save all settings. Check write permissions and free disk space, then try again.";

/// The start-up warning about settings files that are there but could not be read (spec §6).
const UNREADABLE: &str = "These files could not be read:\n\n{}\n\nThe launcher shows default values for what it could not read. The files stay as they are until you save (Save or PLAY), which writes what it shows.";

/// The one dialog naming every settings file that could not be read at start; none when all
/// were read (or are missing). The files stay as they are until a save.
pub fn unreadable_warning(lang: &str, files: &[PathBuf]) -> Option<Modal> {
    if files.is_empty() {
        return None;
    }
    let names: Vec<String> = files
        .iter()
        .map(|file| file.display().to_string())
        .collect();
    Some(Modal {
        kind: ModalKind::Error,
        title: APP_NAME.to_string(),
        text: tr_fmt(lang, UNREADABLE, &[&names.join("\n")]),
    })
}

/// What `collect` writes, as data: `settings.json`, the ini values, and `mods.json` and
/// `patches.json` when the window lists mods and third-party patches.
#[derive(Debug, Clone, PartialEq)]
pub struct Persisted {
    pub app: Map<String, Value>,
    pub ini: IndexMap<String, String>,
    pub mods: Option<Value>,
    pub patches: Option<Value>,
}

/// A file of the save could not be written.
#[derive(Debug, thiserror::Error)]
#[error("could not save all settings: {0}")]
pub struct PersistError(#[from] io::Error);

fn strings<'a>(names: impl IntoIterator<Item = &'a String>) -> Value {
    Value::Array(names.into_iter().cloned().map(Value::String).collect())
}

fn string_map(values: IndexMap<String, String>) -> Value {
    Value::Object(
        values
            .into_iter()
            .map(|(key, value)| (key, Value::String(value)))
            .collect(),
    )
}

/// A setting as text, as `Settings::get_str` reads it.
fn text_of(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Number(number)) => number.to_string(),
        _ => String::new(),
    }
}

/// The settings the window shows (Python `gather`); reads the state and changes nothing.
pub fn gather(state: &State) -> Persisted {
    // `Set` already stored every edit in the settings and the ini values.
    let mut app = state.settings.0.clone();
    if let Some(title_id) = &state.patches.title_id {
        let chosen = state
            .patches
            .selectable
            .iter()
            .filter(|meta| state.patches.chosen.contains(&meta.name))
            .map(|meta| &meta.name);
        let mut all = match app.get("game_patches") {
            Some(Value::Object(map)) => map.clone(),
            _ => Map::new(),
        };
        all.insert(title_id.clone(), strings(chosen));
        app.insert("game_patches".to_string(), Value::Object(all));
    }
    let mut ini = state.ini.values.clone();
    // A switch is a check box in the window: anything but "1" reads as off.
    for key in SHOWN_INI_KEYS.iter().filter(|key| INI_FLAGS.contains(key)) {
        let on = ini.get(*key).is_some_and(|value| value == "1");
        ini.insert((*key).to_string(), if on { "1" } else { "0" }.to_string());
    }
    let mut controls = Map::new();
    controls.insert(
        "key".to_string(),
        string_map(control_changes(&state.controls.key, Kind::Key)),
    );
    controls.insert(
        "pad".to_string(),
        string_map(control_changes(&state.controls.pad, Kind::Pad)),
    );
    app.insert("controls".to_string(), Value::Object(controls));
    // The Controls page's own choice; `settings.pad_style` may still hold an unknown saved value.
    app.insert(
        "pad_style".to_string(),
        Value::String(state.controls.pad_style.clone()),
    );
    // An untouched first choice stays automatic.
    if state.controls.device_chosen {
        app.insert(
            "controls_device".to_string(),
            Value::from(state.controls.device.value()),
        );
    }
    let view = &state.mods;
    let mods = (!view.order.is_empty()).then(|| {
        let disabled = view
            .order
            .iter()
            .filter(|name| !view.enabled.get(*name).copied().unwrap_or(false));
        let mut map = Map::new();
        map.insert("order".to_string(), strings(&view.order));
        map.insert("disabled".to_string(), strings(disabled));
        Value::Object(map)
    });
    let patches = (!view.external.is_empty()).then(|| {
        let keys: BTreeSet<&String> = view.external.iter().map(|(key, _, _)| key).collect();
        let (enabled, disabled): (Vec<&String>, Vec<&String>) = keys
            .into_iter()
            .partition(|key| view.external_enabled.contains(*key));
        let mut map = Map::new();
        map.insert("enabled".to_string(), strings(enabled));
        map.insert("disabled".to_string(), strings(disabled));
        Value::Object(map)
    });
    Persisted {
        app,
        ini,
        mods,
        patches,
    }
}

/// Whether the window differs from the files of the last save (Save is active).
pub fn dirty(state: &State) -> bool {
    state.saved.as_ref() != Some(&gather(state))
}

/// Lists the monitors again when the pending window system differs from the one the list was made
/// under (Python `refresh_monitors`), so the stored choice belongs to the backend the file carries.
/// Never on Windows, where no window system is selectable. The choice on screen is kept when the
/// new list has it, else the collected one, else the primary default. True when it listed again.
pub fn refresh_monitors(state: &mut State) -> bool {
    let backend = state.settings.get_str("video_backend");
    if cfg!(windows) || backend == state.displays_backend {
        return false;
    }
    let displays = list_displays(&state.paths, &backend);
    state.monitor_pairs = monitor_options(&displays);
    let visible = state.settings.get_str("monitor");
    let selected = if state
        .monitor_pairs
        .iter()
        .any(|(value, _)| *value == visible)
    {
        visible
    } else {
        let collected = text_of(state.saved.as_ref().and_then(|s| s.app.get("monitor")));
        monitor_selection(&displays, &collected)
    };
    state.settings.set("monitor", Value::String(selected));
    state.displays = displays;
    state.displays_backend = backend;
    true
}

/// The start-up list of monitors arrived: the saved choice follows it as the one on screen does.
/// The Python launcher lists them while it builds the window, before it takes what the files hold.
pub fn follow_monitor_list(state: &mut State, displays: &[Display]) {
    if let Some(saved) = state.saved.as_mut() {
        let collected = text_of(saved.app.get("monitor"));
        saved.app.insert(
            "monitor".to_string(),
            Value::String(monitor_selection(displays, &collected)),
        );
    }
}

/// Writes `settings.json`, `spartanport.ini`, `mods.json` and `patches.json` (Python `collect`).
/// The ini gets the shown keys whose value changed since the last save or that the file lacks;
/// every other line, comment and key in it stays, so changes made outside the launcher survive.
/// On failure the settings, the ini values and the baseline stay as they were, so the edits stay
/// pending.
pub fn collect(state: &mut State) -> Result<(), PersistError> {
    // Before the gather, so the monitor choice lands in settings.json too.
    refresh_monitors(state);
    let pending = gather(state);
    let ini_path = state.paths.ini_path();
    let disk = load_ini(&ini_path);
    let persisted: BTreeSet<&str> = disk
        .lines
        .iter()
        .filter(|line| line.contains('=') && !line.trim_start().starts_with('#'))
        .filter_map(|line| line.split_once('=').map(|(key, _)| key.trim()))
        .collect();
    // `saved` is set when the state is built; without it, what the file holds is the baseline.
    let baseline = state
        .saved
        .as_ref()
        .map_or(&disk.values, |saved| &saved.ini);
    let mut edited = IndexMap::new();
    for key in SHOWN_INI_KEYS {
        let Some(value) = pending.ini.get(*key) else {
            continue;
        };
        if !persisted.contains(key) || baseline.get(*key) != Some(value) {
            edited.insert((*key).to_string(), value.clone());
        }
    }
    write_files(state, &pending, &edited, &disk.lines)?;
    state.settings = Settings(pending.app);
    let resolution = state.ini.values.get("render_resolution").cloned();
    state.ini = load_ini(&ini_path);
    // The ini may hold a size changed outside the launcher: the Aspect ratio box follows it.
    if state.ini.values.get("render_resolution") != resolution.as_ref() {
        performance::resolution_chosen(state);
    }
    state.saved = Some(gather(state));
    // The files now hold what the window shows.
    state.settings_unreadable = false;
    Ok(())
}

fn write_files(
    state: &State,
    pending: &Persisted,
    edited: &IndexMap<String, String>,
    lines: &[String],
) -> io::Result<()> {
    let paths = &state.paths;
    save_ini(&paths.ini_path(), edited, lines)?;
    save_settings(&paths.config_file(), &Settings(pending.app.clone()))?;
    if let Some(mods) = &pending.mods {
        atomic_write(&paths.mods_json(), &python_json(mods)?)?;
    }
    if let Some(patches) = &pending.patches {
        // Patches of other game versions keep their choice.
        let chosen: Value = load_json(&paths.patches_json(), Value::Null);
        let shown: BTreeSet<&String> = state.mods.external.iter().map(|(key, _, _)| key).collect();
        let merged = |field: &str| -> BTreeSet<String> {
            let names = |value: Option<&Value>| -> Vec<String> {
                value
                    .and_then(Value::as_array)
                    .map(|list| {
                        list.iter()
                            .filter_map(|name| name.as_str().map(str::to_string))
                            .collect()
                    })
                    .unwrap_or_default()
            };
            names(chosen.get(field))
                .into_iter()
                .filter(|name| !shown.contains(name))
                .chain(names(patches.get(field)))
                .collect()
        };
        let mut map = Map::new();
        map.insert("enabled".to_string(), strings(&merged("enabled")));
        map.insert("disabled".to_string(), strings(&merged("disabled")));
        atomic_write(&paths.patches_json(), &python_json(&Value::Object(map))?)?;
    }
    Ok(())
}

/// The error dialog of a failed `collect` (Save and PLAY).
pub fn show_save_failed(state: &mut State) {
    state.modal = Some(Modal {
        kind: ModalKind::Error,
        title: APP_NAME.to_string(),
        text: tr(&state.lang, SAVE_FAILED),
    });
}

/// `collect`, with the error dialog when it fails (Python's `if not self.collect(): return`):
/// whether the files were written.
pub fn collect_or_report(state: &mut State) -> bool {
    if collect(state).is_err() {
        show_save_failed(state);
        return false;
    }
    true
}

/// `collect_or_report` for the saves the player did not ask for (closing the window, opening the
/// ini, Add to Steam): while a settings file that could not be read at start is unsaved, nothing is
/// written (spec §6: it stays as it is until Save or PLAY) and the action goes on.
pub fn collect_unless_unreadable(state: &mut State) -> bool {
    state.settings_unreadable || collect_or_report(state)
}

/// The Save button (Python `save`): the files, without starting the game. A new launcher language
/// or zoom rebuilds the window in it, and a new zoom sizes the window again; a failed save shows
/// the error dialog and keeps the edits.
pub fn save(state: &mut State) -> Task<Message> {
    if !collect_or_report(state) {
        return Task::none();
    }
    let language = launcher_language(&state.settings.get_str("ui_language"), system_language);
    let scale = theme::ui_scale(&state.settings.get_str("ui_scale"));
    let rezoomed = scale != state.scale;
    if language != state.lang || rezoomed {
        let rebuilt = rebuild(state, language, scale);
        if rezoomed {
            return Task::batch([rebuilt, app::fit_window(state)]);
        }
        return rebuilt;
    }
    Task::none()
}

/// The window in another language or zoom (Python `rebuild`). The views are functions of the
/// state, so rebuilding is only this update: the language, its direction and the zoom change, and
/// what the views show from the disk (the Home facts, the mod and patch lists, the game patches) is
/// read again. Every setting, the page, the drawer, the log, the game, the update state (the
/// release list, a check or a download under way) and an open dialog stay as they were.
pub fn rebuild(state: &mut State, language: String, scale: f32) -> Task<Message> {
    state.rtl = is_rtl(&language);
    state.lang = language;
    state.scale = scale;
    home::refresh_facts(state);
    // Read now, as the Python does, so the lists are in place before the new baseline.
    let lists = mods::load(&state.paths, &state.settings);
    let conflicts = mods::update(state, ModsMessage::Loaded(Box::new(lists)));
    game::refresh_game_patches(state);
    state.saved = Some(gather(state));
    conflicts
}

/// Python's `json.dumps(value, indent=2)`: two-space indent, `": "` after keys, no final newline,
/// and every character past `~` written as a `\uXXXX` escape (UTF-16 units past the BMP).
fn python_json(value: &Value) -> io::Result<Vec<u8>> {
    let mut out = Vec::new();
    let mut serializer = serde_json::Serializer::with_formatter(
        &mut out,
        AsciiFormatter(PrettyFormatter::with_indent(b"  ")),
    );
    value.serialize(&mut serializer).map_err(io::Error::other)?;
    Ok(out)
}

/// `PrettyFormatter` with Python's `ensure_ascii`.
struct AsciiFormatter<'a>(PrettyFormatter<'a>);

impl Formatter for AsciiFormatter<'_> {
    fn begin_array<W: ?Sized + Write>(&mut self, writer: &mut W) -> io::Result<()> {
        self.0.begin_array(writer)
    }

    fn end_array<W: ?Sized + Write>(&mut self, writer: &mut W) -> io::Result<()> {
        self.0.end_array(writer)
    }

    fn begin_array_value<W: ?Sized + Write>(
        &mut self,
        writer: &mut W,
        first: bool,
    ) -> io::Result<()> {
        self.0.begin_array_value(writer, first)
    }

    fn end_array_value<W: ?Sized + Write>(&mut self, writer: &mut W) -> io::Result<()> {
        self.0.end_array_value(writer)
    }

    fn begin_object<W: ?Sized + Write>(&mut self, writer: &mut W) -> io::Result<()> {
        self.0.begin_object(writer)
    }

    fn end_object<W: ?Sized + Write>(&mut self, writer: &mut W) -> io::Result<()> {
        self.0.end_object(writer)
    }

    fn begin_object_key<W: ?Sized + Write>(
        &mut self,
        writer: &mut W,
        first: bool,
    ) -> io::Result<()> {
        self.0.begin_object_key(writer, first)
    }

    fn begin_object_value<W: ?Sized + Write>(&mut self, writer: &mut W) -> io::Result<()> {
        self.0.begin_object_value(writer)
    }

    fn end_object_value<W: ?Sized + Write>(&mut self, writer: &mut W) -> io::Result<()> {
        self.0.end_object_value(writer)
    }

    fn write_string_fragment<W: ?Sized + Write>(
        &mut self,
        writer: &mut W,
        fragment: &str,
    ) -> io::Result<()> {
        // serde_json escapes the quote, the backslash and the controls below 0x20 itself.
        for c in fragment.chars() {
            if c < '\u{7f}' {
                writer.write_all(&[c as u8])?;
            } else {
                for unit in c.encode_utf16(&mut [0; 2]) {
                    write!(writer, "\\u{unit:04x}")?;
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::ini::INI_DEFAULTS;
    use serde_json::json;

    #[test]
    fn shown_ini_keys_are_the_18_controls_of_the_window() {
        assert_eq!(SHOWN_INI_KEYS.len(), 18);
        for key in SHOWN_INI_KEYS {
            assert!(
                INI_DEFAULTS.iter().any(|(name, _)| name == key),
                "{key} is not an ini key"
            );
        }
        for parked in [
            "upscaler",
            "preset",
            "object_motion",
            "output_res",
            "live_resolution",
        ] {
            assert!(!SHOWN_INI_KEYS.contains(&parked), "{parked}");
        }
    }

    #[test]
    fn python_json_escapes_like_ensure_ascii() {
        let value =
            json!({"order": ["Мод", "a\"b\\c", "tab\there", "x\u{7f}", "🎮"], "disabled": []});
        let text = String::from_utf8(python_json(&value).unwrap()).unwrap();
        // json.dumps({...}, indent=2) of the same dict.
        assert_eq!(
            text,
            "{\n  \"order\": [\n    \"\\u041c\\u043e\\u0434\",\n    \"a\\\"b\\\\c\",\n    \
             \"tab\\there\",\n    \"x\\u007f\",\n    \"\\ud83c\\udfae\"\n  ],\n  \"disabled\": []\n}"
        );
    }
}
