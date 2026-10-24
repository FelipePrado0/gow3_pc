//! Steam's folders and its binary VDF (the non-Steam shortcuts file).
//!
//! Binary VDF: 0 = map (key, entries, 8), 1 = string, 2 = uint32, 8 = end of the map.

use crate::core::platform;
use indexmap::IndexMap;
use std::collections::HashSet;
use std::fmt;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

/// A value of the binary VDF.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Vdf {
    Map(IndexMap<String, Vdf>),
    Str(String),
    U32(u32),
}

/// Why a shortcut could not be added.
#[derive(Debug)]
pub enum SteamError {
    /// Steam is open (it rewrites `shortcuts.vdf` on exit).
    Running,
    /// No Steam account folder was found.
    NoSteam,
    /// The game dump has no background picture in this folder.
    NoArt(PathBuf),
    Io(io::Error),
}

impl From<io::Error> for SteamError {
    fn from(e: io::Error) -> Self {
        SteamError::Io(e)
    }
}

impl fmt::Display for SteamError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SteamError::Running => f.write_str("running"),
            SteamError::NoSteam => f.write_str("no_steam"),
            SteamError::NoArt(path) => write!(f, "no_art: {}", path.display()),
            SteamError::Io(e) => e.fmt(f),
        }
    }
}

impl std::error::Error for SteamError {}

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl Reader<'_> {
    fn truncated(&self) -> String {
        format!("truncated VDF at {}", self.pos)
    }

    fn byte(&mut self) -> Result<u8, String> {
        let b = *self.data.get(self.pos).ok_or_else(|| self.truncated())?;
        self.pos += 1;
        Ok(b)
    }

    fn text(&mut self) -> Result<String, String> {
        let rest = &self.data[self.pos.min(self.data.len())..];
        let len = rest
            .iter()
            .position(|&b| b == 0)
            .ok_or_else(|| self.truncated())?;
        let value = String::from_utf8_lossy(&rest[..len]).into_owned();
        self.pos += len + 1;
        Ok(value)
    }

    fn entries(&mut self) -> Result<IndexMap<String, Vdf>, String> {
        let mut result = IndexMap::new();
        loop {
            let kind = self.byte()?;
            if kind == 8 {
                return Ok(result);
            }
            let key = self.text()?;
            if kind > 2 {
                return Err(format!("unknown VDF type {kind} at {}", self.pos));
            }
            let value = match kind {
                0 => Vdf::Map(self.entries()?),
                1 => Vdf::Str(self.text()?),
                _ => {
                    let bytes = self
                        .data
                        .get(self.pos..self.pos + 4)
                        .ok_or_else(|| self.truncated())?;
                    self.pos += 4;
                    Vdf::U32(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
                }
            };
            result.insert(key, value);
        }
    }
}

/// Reads a binary VDF file.
pub fn parse_vdf(data: &[u8]) -> Result<IndexMap<String, Vdf>, String> {
    Reader { data, pos: 0 }.entries()
}

fn write_entries(out: &mut Vec<u8>, map: &IndexMap<String, Vdf>) {
    for (key, value) in map {
        match value {
            Vdf::Map(inner) => {
                out.push(0);
                out.extend_from_slice(key.as_bytes());
                out.push(0);
                write_entries(out, inner);
            }
            Vdf::Str(text) => {
                out.push(1);
                out.extend_from_slice(key.as_bytes());
                out.push(0);
                out.extend_from_slice(text.as_bytes());
                out.push(0);
            }
            Vdf::U32(n) => {
                out.push(2);
                out.extend_from_slice(key.as_bytes());
                out.push(0);
                out.extend_from_slice(&n.to_le_bytes());
            }
        }
    }
    out.push(8);
}

/// Writes a binary VDF file.
pub fn dump_vdf(tree: &IndexMap<String, Vdf>) -> Vec<u8> {
    let mut out = Vec::new();
    write_entries(&mut out, tree);
    out
}

/// The 32-bit id Steam gives a non-Steam game: the CRC of its quoted exe and name, top bit set.
pub fn shortcut_appid(exe: &str, name: &str) -> u32 {
    crc32fast::hash(format!("{exe}{name}").as_bytes()) | 0x8000_0000
}

/// The command that lists Steam's process.
pub fn steam_query(windows: bool) -> Vec<&'static str> {
    if windows {
        vec!["tasklist", "/FI", "IMAGENAME eq steam.exe", "/NH"]
    } else {
        vec!["pgrep", "-x", "steam"]
    }
}

/// Reads the answer of [`steam_query`].
pub fn running_from(windows: bool, code: Option<i32>, stdout: &str) -> bool {
    if windows {
        stdout.to_lowercase().contains("steam.exe")
    } else {
        code == Some(0)
    }
}

/// Whether Steam is open. A command that cannot start counts as not running.
pub fn steam_running() -> bool {
    let windows = cfg!(windows);
    let query = steam_query(windows);
    let mut cmd = Command::new(query[0]);
    cmd.args(&query[1..]);
    platform::no_window(&mut cmd);
    match cmd.output() {
        Ok(out) => running_from(
            windows,
            out.status.code(),
            &String::from_utf8_lossy(&out.stdout),
        ),
        Err(_) => false,
    }
}

/// Steam installation folders to look in, the registry's first on Windows.
pub fn steam_roots_from(windows: bool, registry: Option<PathBuf>, home: &Path) -> Vec<PathBuf> {
    if windows {
        let mut roots: Vec<PathBuf> = registry.into_iter().collect();
        roots.push(PathBuf::from("C:/Program Files (x86)/Steam"));
        roots.push(PathBuf::from("C:/Program Files/Steam"));
        roots
    } else {
        [
            ".local/share/Steam",
            ".steam/steam",
            ".var/app/com.valvesoftware.Steam/.local/share/Steam",
        ]
        .iter()
        .map(|p| home.join(p))
        .collect()
    }
}

/// [`steam_roots_from`] for this machine and user.
pub fn steam_roots() -> Vec<PathBuf> {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_default();
    steam_roots_from(cfg!(windows), platform::steam_path_from_registry(), &home)
}

/// `userdata/<account>` folders of the Steam installation(s) of this user, each once.
pub fn account_folders(roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    let mut found = Vec::new();
    for base in roots {
        let Ok(read) = std::fs::read_dir(base.join("userdata")) else {
            continue;
        };
        let mut entries: Vec<PathBuf> = read.filter_map(|e| e.ok().map(|e| e.path())).collect();
        entries.sort();
        for entry in entries {
            let name = entry.file_name().and_then(|n| n.to_str()).unwrap_or("");
            let numbered =
                !name.is_empty() && name.bytes().all(|b| b.is_ascii_digit()) && name != "0";
            if !numbered || !entry.is_dir() {
                continue;
            }
            let real = std::fs::canonicalize(&entry).unwrap_or_else(|_| entry.clone());
            if seen.insert(real) {
                found.push(entry);
            }
        }
    }
    found
}

/// What Steam starts: the program, its launch options and the folder it starts in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shortcut {
    pub exe: PathBuf,
    pub options: String,
    pub start_dir: PathBuf,
}

/// What `add_to_steam` did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Added {
    pub accounts: usize,
    pub updated: bool,
    pub app_id: u32,
}

fn shortcut_entry(app_id: u32, exe: &str, start_dir: &str, options: &str, icon: &Path) -> Vdf {
    let text = |s: &str| Vdf::Str(s.to_string());
    let mut e = IndexMap::new();
    e.insert("appid".to_string(), Vdf::U32(app_id));
    e.insert("AppName".to_string(), text(crate::APP_NAME));
    e.insert("Exe".to_string(), text(exe));
    e.insert("StartDir".to_string(), text(start_dir));
    e.insert("icon".to_string(), text(&icon.display().to_string()));
    e.insert("ShortcutPath".to_string(), text(""));
    e.insert("LaunchOptions".to_string(), text(options));
    e.insert("IsHidden".to_string(), Vdf::U32(0));
    e.insert("AllowDesktopConfig".to_string(), Vdf::U32(1));
    e.insert("AllowOverlay".to_string(), Vdf::U32(1));
    e.insert("OpenVR".to_string(), Vdf::U32(0));
    e.insert("Devkit".to_string(), Vdf::U32(0));
    e.insert("DevkitGameID".to_string(), text(""));
    e.insert("DevkitOverrideAppID".to_string(), Vdf::U32(0));
    e.insert("LastPlayTime".to_string(), Vdf::U32(0));
    e.insert("FlatpakAppID".to_string(), text(""));
    e.insert("tags".to_string(), Vdf::Map(IndexMap::new()));
    Vdf::Map(e)
}

fn invalid(message: String) -> SteamError {
    SteamError::Io(io::Error::new(io::ErrorKind::InvalidData, message))
}

/// Adds (or updates) the shortcut for every Steam account of the user, with its pictures.
/// Refuses while Steam is open (`running` overrides the check) or when it is not installed.
pub fn add_to_steam(
    game: &Path,
    shortcut: &Shortcut,
    icon: &Path,
    fonts: &[PathBuf],
    steam_root: Option<&Path>,
    running: Option<bool>,
) -> Result<Added, SteamError> {
    if running.unwrap_or_else(steam_running) {
        return Err(SteamError::Running);
    }
    let roots = match steam_root {
        Some(root) => vec![root.to_path_buf()],
        None => steam_roots(),
    };
    let accounts = account_folders(&roots);
    if accounts.is_empty() {
        return Err(SteamError::NoSteam);
    }
    let font = crate::core::steam_art::open_font(fonts)?;
    let exe = format!("\"{}\"", shortcut.exe.display());
    let start_dir = format!("\"{}\"", shortcut.start_dir.display());
    let app_id = shortcut_appid(&exe, crate::APP_NAME);
    let mut updated = false;
    for account in &accounts {
        let config = account.join("config");
        std::fs::create_dir_all(&config)?;
        let icon_path =
            crate::core::steam_art::make_art(game, icon, &config.join("grid"), app_id, &font)?;
        let file = config.join("shortcuts.vdf");
        let has_data = std::fs::metadata(&file).is_ok_and(|m| m.is_file() && m.len() > 0);
        let mut tree = if has_data {
            parse_vdf(&std::fs::read(&file)?)
                .map_err(|e| invalid(format!("{}: {e}", file.display())))?
        } else {
            IndexMap::new()
        };
        let slot_map = tree
            .entry("shortcuts".to_string())
            .or_insert_with(|| Vdf::Map(IndexMap::new()));
        let Vdf::Map(shortcuts) = slot_map else {
            return Err(invalid(format!(
                "{}: shortcuts is not a map",
                file.display()
            )));
        };
        let mut entry = shortcut_entry(app_id, &exe, &start_dir, &shortcut.options, &icon_path);
        let found = shortcuts.iter().find_map(|(key, value)| {
            let Vdf::Map(map) = value else { return None };
            let named = matches!(map.get("AppName"), Some(Vdf::Str(n))
                if n == crate::APP_NAME || n == crate::core::legacy::LEGACY_TITLE);
            (map.get("appid") == Some(&Vdf::U32(app_id)) || named).then(|| key.clone())
        });
        let slot = match found {
            Some(slot) => {
                updated = true;
                if let (Vdf::Map(new), Some(Vdf::Map(old))) = (&mut entry, shortcuts.get(&slot)) {
                    if let Some(tags) = old.get("tags") {
                        new.insert("tags".to_string(), tags.clone());
                    }
                    if let Some(played) = old.get("LastPlayTime") {
                        new.insert("LastPlayTime".to_string(), played.clone());
                    }
                }
                slot
            }
            None => shortcuts
                .keys()
                .filter(|k| !k.is_empty() && k.bytes().all(|b| b.is_ascii_digit()))
                .filter_map(|k| k.parse::<u64>().ok())
                .max()
                .map_or(0, |n| n + 1)
                .to_string(),
        };
        shortcuts.insert(slot, entry);
        if file.is_file() {
            std::fs::copy(&file, config.join("shortcuts.vdf.spartanport-backup"))?;
        }
        crate::core::fs::atomic_write(&file, &dump_vdf(&tree))?;
    }
    Ok(Added {
        accounts: accounts.len(),
        updated,
        app_id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use indexmap::IndexMap;
    use std::path::{Path, PathBuf};

    fn sample() -> IndexMap<String, Vdf> {
        let mut tags = IndexMap::new();
        tags.insert("0".to_string(), Vdf::Str("favorite".into()));
        let mut entry = IndexMap::new();
        entry.insert("appid".to_string(), Vdf::U32(0x8000_0001));
        entry.insert("AppName".to_string(), Vdf::Str("A".into()));
        entry.insert("tags".to_string(), Vdf::Map(tags));
        entry.insert("IsHidden".to_string(), Vdf::U32(0));
        let mut shortcuts = IndexMap::new();
        shortcuts.insert("0".to_string(), Vdf::Map(entry));
        let mut tree = IndexMap::new();
        tree.insert("shortcuts".to_string(), Vdf::Map(shortcuts));
        tree
    }

    #[test]
    fn round_trip_keeps_strings_numbers_and_nested_maps() {
        let tree = sample();
        let bytes = dump_vdf(&tree);
        assert_eq!(parse_vdf(&bytes), Ok(tree));
        let again = parse_vdf(&bytes).unwrap();
        assert_eq!(dump_vdf(&again), bytes);
    }

    #[test]
    fn empty_file_of_steam() {
        let bytes = b"\x00shortcuts\x00\x08\x08";
        let tree = parse_vdf(bytes).unwrap();
        let mut want = IndexMap::new();
        want.insert("shortcuts".to_string(), Vdf::Map(IndexMap::new()));
        assert_eq!(tree, want);
        assert_eq!(dump_vdf(&tree), bytes);
    }

    #[test]
    fn broken_files_are_errors() {
        assert_eq!(
            parse_vdf(b"\x05x\x00"),
            Err("unknown VDF type 5 at 3".to_string())
        );
        let err = parse_vdf(b"\x00shortcuts\x00\x01AppName\x00Spar").unwrap_err();
        assert!(err.contains("truncated"), "{err}");
    }

    #[test]
    fn app_id_is_the_crc_of_exe_and_name_with_the_top_bit() {
        assert_eq!(
            shortcut_appid("\"/home/user/SpartanPort.AppImage\"", "SpartanPort"),
            0xa02add32
        );
    }

    #[test]
    fn windows_looks_in_the_registry_folder_then_the_usual_ones() {
        let home = Path::new("/home/u");
        let win = steam_roots_from(true, Some(PathBuf::from("D:/Games/Steam")), home);
        assert_eq!(win[0], PathBuf::from("D:/Games/Steam"));
        assert!(win.contains(&PathBuf::from("C:/Program Files (x86)/Steam")));
        assert!(win.contains(&PathBuf::from("C:/Program Files/Steam")));
        assert_eq!(
            steam_roots_from(false, None, home),
            vec![
                home.join(".local/share/Steam"),
                home.join(".steam/steam"),
                home.join(".var/app/com.valvesoftware.Steam/.local/share/Steam"),
            ]
        );
    }

    #[test]
    fn steam_is_running_by_the_process_list() {
        assert!(running_from(
            true,
            Some(0),
            "steam.exe   1234 Console   1   150,000 K"
        ));
        assert!(!running_from(
            true,
            Some(0),
            "INFO: No tasks are running which match the specified criteria."
        ));
        assert!(running_from(false, Some(0), ""));
        assert!(!running_from(false, Some(1), ""));
        assert_eq!(steam_query(false)[0], "pgrep");
        assert_eq!(steam_query(true)[0], "tasklist");
    }

    #[test]
    fn accounts_are_the_numbered_folders_but_0() {
        let dir = tempfile::tempdir().unwrap();
        let users = dir.path().join("userdata");
        for name in ["1111", "2222", "0", "abc"] {
            std::fs::create_dir_all(users.join(name)).unwrap();
        }
        std::fs::write(users.join("3333"), "").unwrap();
        let missing = dir.path().join("missing");
        let found = account_folders(&[missing, dir.path().to_path_buf()]);
        assert_eq!(found, vec![users.join("1111"), users.join("2222")]);
    }

    #[cfg(unix)]
    #[test]
    fn a_linked_root_is_counted_once() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a");
        std::fs::create_dir_all(a.join("userdata/1111")).unwrap();
        std::fs::create_dir_all(a.join("userdata/2222")).unwrap();
        let b = dir.path().join("b");
        std::os::unix::fs::symlink(&a, &b).unwrap();
        let found = account_folders(&[a, b]);
        assert_eq!(found.len(), 2);
    }

    use crate::core::draw::{Mode, Picture};
    use crate::core::steam_art::open_font;

    struct Fixture {
        _dir: tempfile::TempDir,
        game: PathBuf,
        icon: PathBuf,
        root: PathBuf,
        fonts: Vec<PathBuf>,
    }

    fn fixture() -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        let game = dir.path().join("game");
        std::fs::create_dir_all(game.join("sce_sys")).unwrap();
        Picture::new(Mode::Rgba, 480, 270, &[90, 20, 20])
            .to_rgba_image()
            .save(game.join("sce_sys").join("pic1.png"))
            .unwrap();
        let icon = dir.path().join("icon.png");
        Picture::new(Mode::Rgba, 64, 64, &[1, 2, 3, 255])
            .to_rgba_image()
            .save(&icon)
            .unwrap();
        let root = dir.path().join("steam");
        for name in ["1111", "2222", "0"] {
            std::fs::create_dir_all(root.join("userdata").join(name)).unwrap();
        }
        let mut other = IndexMap::new();
        other.insert("appid".to_string(), Vdf::U32(5));
        other.insert("AppName".to_string(), Vdf::Str("Other".into()));
        other.insert("Exe".to_string(), Vdf::Str("\"/x\"".into()));
        let mut tags = IndexMap::new();
        tags.insert("0".to_string(), Vdf::Str("keep".into()));
        other.insert("tags".to_string(), Vdf::Map(tags));
        write_shortcuts(&root, "2222", vec![("0", other)]);
        let fonts = vec![Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("launcher")
            .join("Cinzel-Black.ttf")];
        assert!(open_font(&fonts).is_ok());
        Fixture {
            _dir: dir,
            game,
            icon,
            root,
            fonts,
        }
    }

    fn shortcuts_file(root: &Path, account: &str) -> PathBuf {
        root.join("userdata")
            .join(account)
            .join("config")
            .join("shortcuts.vdf")
    }

    fn write_shortcuts(root: &Path, account: &str, entries: Vec<(&str, IndexMap<String, Vdf>)>) {
        let mut map = IndexMap::new();
        for (slot, entry) in entries {
            map.insert(slot.to_string(), Vdf::Map(entry));
        }
        let mut tree = IndexMap::new();
        tree.insert("shortcuts".to_string(), Vdf::Map(map));
        let file = shortcuts_file(root, account);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, dump_vdf(&tree)).unwrap();
    }

    fn shortcuts_of(root: &Path, account: &str) -> IndexMap<String, Vdf> {
        let bytes = std::fs::read(shortcuts_file(root, account)).unwrap();
        match parse_vdf(&bytes).unwrap().swap_remove("shortcuts") {
            Some(Vdf::Map(map)) => map,
            other => panic!("{other:?}"),
        }
    }

    fn entry<'a>(shortcuts: &'a IndexMap<String, Vdf>, slot: &str) -> &'a IndexMap<String, Vdf> {
        match &shortcuts[slot] {
            Vdf::Map(map) => map,
            other => panic!("{other:?}"),
        }
    }

    fn text(entry: &IndexMap<String, Vdf>, key: &str) -> String {
        match &entry[key] {
            Vdf::Str(s) => s.clone(),
            other => panic!("{other:?}"),
        }
    }

    fn command() -> Shortcut {
        Shortcut {
            exe: PathBuf::from("/home/user/spartanport.AppImage"),
            options: "--play".into(),
            start_dir: PathBuf::from("/home/user"),
        }
    }

    fn add(f: &Fixture) -> Result<Added, SteamError> {
        add_to_steam(
            &f.game,
            &command(),
            &f.icon,
            &f.fonts,
            Some(&f.root),
            Some(false),
        )
    }

    #[test]
    fn every_account_gets_the_shortcut_and_the_five_pictures() {
        let f = fixture();
        let added = add(&f).unwrap();
        assert_eq!(added.accounts, 2);
        assert!(!added.updated);
        let shortcuts = shortcuts_of(&f.root, "1111");
        let e = entry(&shortcuts, "0");
        assert_eq!(text(e, "AppName"), "SpartanPort");
        assert_eq!(text(e, "Exe"), "\"/home/user/spartanport.AppImage\"");
        assert_eq!(text(e, "LaunchOptions"), "--play");
        assert_eq!(text(e, "StartDir"), "\"/home/user\"");
        assert_eq!(e["appid"], Vdf::U32(added.app_id));
        let grid = f
            .root
            .join("userdata")
            .join("1111")
            .join("config")
            .join("grid");
        assert_eq!(
            text(e, "icon"),
            grid.join(format!("{}_icon.png", added.app_id))
                .display()
                .to_string()
        );
        let keys: Vec<&str> = e.keys().map(String::as_str).collect();
        assert_eq!(
            keys,
            [
                "appid",
                "AppName",
                "Exe",
                "StartDir",
                "icon",
                "ShortcutPath",
                "LaunchOptions",
                "IsHidden",
                "AllowDesktopConfig",
                "AllowOverlay",
                "OpenVR",
                "Devkit",
                "DevkitGameID",
                "DevkitOverrideAppID",
                "LastPlayTime",
                "FlatpakAppID",
                "tags"
            ]
        );
        for name in ["_hero.png", "_logo.png", ".png", "p.png", "_icon.png"] {
            assert!(grid.join(format!("{}{name}", added.app_id)).is_file());
        }
        assert!(!f.root.join("userdata/0/config/shortcuts.vdf").exists());
    }

    #[test]
    fn other_shortcuts_stay_and_a_backup_is_made() {
        let f = fixture();
        add(&f).unwrap();
        let shortcuts = shortcuts_of(&f.root, "2222");
        let other = entry(&shortcuts, "0");
        assert_eq!(text(other, "AppName"), "Other");
        let mut tags = IndexMap::new();
        tags.insert("0".to_string(), Vdf::Str("keep".into()));
        assert_eq!(other["tags"], Vdf::Map(tags));
        assert_eq!(text(entry(&shortcuts, "1"), "AppName"), "SpartanPort");
        assert!(shortcuts_file(&f.root, "2222")
            .with_file_name("shortcuts.vdf.spartanport-backup")
            .is_file());
    }

    #[test]
    fn adding_again_updates_the_same_shortcut() {
        let f = fixture();
        add(&f).unwrap();
        let second = add(&f).unwrap();
        assert!(second.updated);
        let shortcuts = shortcuts_of(&f.root, "2222");
        let ours = shortcuts
            .values()
            .filter(|v| matches!(v, Vdf::Map(m) if m["AppName"] == Vdf::Str("SpartanPort".into())))
            .count();
        assert_eq!(ours, 1);
    }

    #[test]
    fn an_update_keeps_the_tags_and_the_play_time() {
        let f = fixture();
        let mut old = IndexMap::new();
        old.insert("appid".to_string(), Vdf::U32(7));
        old.insert("AppName".to_string(), Vdf::Str("SpartanPort".into()));
        old.insert("Exe".to_string(), Vdf::Str("\"/old\"".into()));
        let mut tags = IndexMap::new();
        tags.insert("0".to_string(), Vdf::Str("fav".into()));
        old.insert("tags".to_string(), Vdf::Map(tags.clone()));
        old.insert("LastPlayTime".to_string(), Vdf::U32(1234));
        write_shortcuts(&f.root, "1111", vec![("0", old)]);
        assert!(add(&f).unwrap().updated);
        let shortcuts = shortcuts_of(&f.root, "1111");
        let ours = entry(&shortcuts, "0");
        assert_eq!(ours["tags"], Vdf::Map(tags));
        assert_eq!(ours["LastPlayTime"], Vdf::U32(1234));
        assert_eq!(text(ours, "Exe"), "\"/home/user/spartanport.AppImage\"");
    }

    #[test]
    fn a_shortcut_from_before_the_rename_is_updated_not_doubled() {
        let f = fixture();
        let mut old = IndexMap::new();
        old.insert("appid".to_string(), Vdf::U32(7));
        old.insert("AppName".to_string(), Vdf::Str("God of War III".into()));
        old.insert("Exe".to_string(), Vdf::Str("\"/old\"".into()));
        write_shortcuts(&f.root, "1111", vec![("0", old)]);
        let added = add(&f).unwrap();
        assert!(added.updated);
        let shortcuts = shortcuts_of(&f.root, "1111");
        let names: Vec<String> = shortcuts
            .values()
            .map(|v| match v {
                Vdf::Map(m) => text(m, "AppName"),
                other => panic!("{other:?}"),
            })
            .collect();
        assert_eq!(names, ["SpartanPort"]);
    }

    #[test]
    fn steam_open_and_steam_missing_are_refused() {
        let f = fixture();
        let open = add_to_steam(
            &f.game,
            &command(),
            &f.icon,
            &f.fonts,
            Some(&f.root),
            Some(true),
        );
        assert!(matches!(open, Err(SteamError::Running)));
        let none = f.root.parent().unwrap().join("none");
        let missing = add_to_steam(
            &f.game,
            &command(),
            &f.icon,
            &f.fonts,
            Some(&none),
            Some(false),
        );
        assert!(matches!(missing, Err(SteamError::NoSteam)));
        assert!(!shortcuts_file(&f.root, "1111").exists());
    }

    #[test]
    fn a_game_folder_without_the_background_is_refused() {
        let f = fixture();
        std::fs::remove_file(f.game.join("sce_sys").join("pic1.png")).unwrap();
        assert!(matches!(add(&f), Err(SteamError::NoArt(_))));
    }

    #[test]
    fn a_corrupt_shortcuts_file_is_left_as_it_is() {
        let f = fixture();
        let file = shortcuts_file(&f.root, "2222");
        let backup = file.with_file_name("shortcuts.vdf.spartanport-backup");
        let corrupt = b"\x00shortcuts\x00\x01AppName\x00Spar";
        std::fs::write(&file, corrupt).unwrap();
        std::fs::write(&backup, b"old").unwrap();
        let error = add(&f).unwrap_err();
        assert!(matches!(error, SteamError::Io(_)));
        assert!(
            error.to_string().contains(&file.display().to_string()),
            "{error}"
        );
        assert_eq!(std::fs::read(&file).unwrap(), corrupt);
        assert_eq!(std::fs::read(&backup).unwrap(), b"old");
    }
}
