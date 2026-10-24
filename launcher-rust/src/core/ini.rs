//! The game's `spartanport.ini`: defaults, tolerant reading and in-place rewriting.

use std::io;
use std::path::Path;

use indexmap::IndexMap;

use super::fs::atomic_write;

/// Keys that hold a `0`/`1` switch.
pub static INI_FLAGS: &[&str] = &[
    "cheat_double_jumps",
    "cheat_very_hard",
    "object_motion",
    "show_fps",
    "vsync",
    "async_shaders",
    "deferred_readback",
    "stale_readback",
];

pub static INI_DEFAULTS: &[(&str, &str)] = &[
    ("upscaler", "fsr3"),
    ("preset", "1"),
    ("object_motion", "1"),
    ("show_fps", "1"),
    ("output_res", "1920x1080"),
    ("live_resolution", "auto"),
    ("display_mode", "windowed"),
    ("vsync", "1"),
    ("fps_limit", "0"),
    ("async_shaders", "1"),
    ("deferred_readback", "1"),
    ("stale_readback", "1"),
    ("render_resolution", "native"),
    ("engine_fps", "120"),
    ("frames_queued", "1"),
    ("cheat_very_hard", "0"),
    ("cheat_double_jumps", "0"),
    ("anisotropy", "0"),
    ("vibration", "100"),
    ("mouse_prompt_distance", "48"),
    ("mouse_lock", "1"),
    ("mouse_aim_style", "0"),
    ("mouse_aim_sensitivity", "100"),
];

const HEADER: &str = "# spartanport settings (in-game menu: Insert / R3+L2)";

pub struct Ini {
    pub values: IndexMap<String, String>,
    pub lines: Vec<String>,
}

fn is_assignment(line: &str) -> bool {
    line.contains('=') && !line.trim_start().starts_with('#')
}

/// The defaults overlaid with the file's keys; see `read_ini`.
pub fn load_ini(path: &Path) -> Ini {
    read_ini(path).0
}

/// The defaults overlaid with the file's keys, and whether the file is there but could not be read
/// as UTF-8 text (spec §6 warns once). Text that is not UTF-8 is read lossily, so its comments and
/// other keys are still the lines a save rewrites around the edited ones.
pub fn read_ini(path: &Path) -> (Ini, bool) {
    let mut values: IndexMap<String, String> = INI_DEFAULTS
        .iter()
        .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
        .collect();
    let (text, unreadable) = match std::fs::read(path) {
        Ok(bytes) => match String::from_utf8(bytes) {
            Ok(text) => (text, false),
            Err(error) => (String::from_utf8_lossy(error.as_bytes()).into_owned(), true),
        },
        Err(error) if error.kind() == io::ErrorKind::NotFound => (String::new(), false),
        Err(_) => (String::new(), true),
    };
    let lines: Vec<String> = text.lines().map(str::to_string).collect();
    for line in lines.iter().filter(|line| is_assignment(line)) {
        if let Some((key, value)) = line.split_once('=') {
            values.insert(key.trim().to_string(), value.trim().to_string());
        }
    }
    (Ini { values, lines }, unreadable)
}

pub fn save_ini(
    path: &Path,
    edited: &IndexMap<String, String>,
    lines: &[String],
) -> io::Result<()> {
    let mut written: Vec<&str> = Vec::new();
    let mut out: Vec<String> = Vec::new();
    for line in lines {
        if is_assignment(line) {
            if let Some((raw_key, _)) = line.split_once('=') {
                let key = raw_key.trim();
                if let Some(value) = edited.get(key) {
                    out.push(format!("{key}={value}"));
                    written.push(key);
                    continue;
                }
            }
        }
        out.push(line.clone());
    }
    if lines.is_empty() {
        out.push(HEADER.to_string());
    }
    for (key, value) in edited {
        if !written.contains(&key.as_str()) {
            out.push(format!("{key}={value}"));
        }
    }
    let mut text = out.join("\n");
    text.push('\n');
    atomic_write(path, text.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn ini_file(text: &str) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("spartanport.ini");
        fs::write(&path, text).unwrap();
        (dir, path)
    }

    #[test]
    fn load_keeps_the_first_equals_only_and_trims() {
        let (_dir, path) = ini_file("extra=a=b\n fps_limit = 60 \n");
        let ini = load_ini(&path);
        assert_eq!(ini.values["extra"], "a=b");
        assert_eq!(ini.values["fps_limit"], "60");
        assert_eq!(ini.values["upscaler"], "fsr3");
    }

    #[test]
    fn comments_are_not_keys() {
        let (_dir, path) = ini_file("  # vsync=0\nshow_fps=0\n");
        let ini = load_ini(&path);
        assert_eq!(ini.values["vsync"], "1");
        assert_eq!(ini.values["show_fps"], "0");
        assert_eq!(ini.lines.len(), 2);
    }

    #[test]
    fn save_rewrites_in_place_and_appends_new_keys() {
        let (_dir, path) = ini_file("# note\nfps_limit = 60\nkeep=1\n");
        let lines = load_ini(&path).lines;
        let mut edited = IndexMap::new();
        edited.insert("vsync".to_string(), "0".to_string());
        edited.insert("fps_limit".to_string(), "120".to_string());
        save_ini(&path, &edited, &lines).unwrap();
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "# note\nfps_limit=120\nkeep=1\nvsync=0\n"
        );
    }

    #[test]
    fn a_file_that_is_not_utf8_keeps_its_lines() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("spartanport.ini");
        fs::write(
            &path,
            b"# caf\xe9 au lait\nupscaler=fsr4\nvsync=0\nmy_key=1\n".as_slice(),
        )
        .unwrap();
        let (ini, unreadable) = read_ini(&path);
        assert!(unreadable);
        assert_eq!(ini.values["vsync"], "0");
        assert_eq!(ini.values["upscaler"], "fsr4");
        assert_eq!(ini.lines.len(), 4);
        // A save rewrites only the edited key: the comment, the parked key and the player's own
        // key stay.
        let mut edited = IndexMap::new();
        edited.insert("vsync".to_string(), "1".to_string());
        save_ini(&path, &edited, &ini.lines).unwrap();
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "# caf\u{fffd} au lait\nupscaler=fsr4\nvsync=1\nmy_key=1\n"
        );
    }

    #[test]
    fn only_a_file_that_is_there_can_be_unreadable() {
        let dir = tempfile::tempdir().unwrap();
        let (ini, unreadable) = read_ini(&dir.path().join("spartanport.ini"));
        assert!(!unreadable);
        assert!(ini.lines.is_empty());
        // A folder in its place cannot be read.
        let (ini, unreadable) = read_ini(dir.path());
        assert!(unreadable);
        assert_eq!(ini.values["vsync"], "1");
        let (_dir, path) = ini_file("vsync=0\n");
        assert!(!read_ini(&path).1);
    }

    #[test]
    fn save_writes_the_header_into_an_empty_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("spartanport.ini");
        let mut edited = IndexMap::new();
        edited.insert("vsync".to_string(), "0".to_string());
        save_ini(&path, &edited, &[]).unwrap();
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "# spartanport settings (in-game menu: Insert / R3+L2)\nvsync=0\n"
        );
    }
}
