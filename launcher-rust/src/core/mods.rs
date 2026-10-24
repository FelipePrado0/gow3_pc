//! Mods (Python `scripts/mods.py`): finding the mods in the mods folder, what they show, the files
//! they replace, the conflicts between them and the saved presets; and the overlay that lays the
//! enabled mods over the game through links, the game and mod files left untouched.
//!
//! Names are compared with `to_lowercase()` where the Python reference uses `casefold()`; the two
//! agree for every name a game folder or a mod archive realistically holds.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde_json::Value;

use crate::core::platform;

/// Why a mod could not be read.
#[derive(Debug, thiserror::Error)]
pub enum ModsError {
    /// The mod, its archive or its settings are not acceptable.
    #[error("{0}")]
    Invalid(String),
    #[error(transparent)]
    Io(#[from] io::Error),
}

// ---- Constants --------------------------------------------------------------------------------

/// Top-level folders a mod may shadow (lower case). The game keeps its data in EXEC and one
/// folder per language at the game root; dvdroot_ps4 is the shadPS4 layout many mods still use.
pub const GAME_ROOTS: &[&str] = &[
    "dvdroot_ps4",
    "exec",
    "dutch",
    "english",
    "french",
    "german",
    "italian",
    "polish",
    "portuguese",
    "russian",
    "spanish",
];
/// Never replaced through the overlay: executable patches go through the patch compiler.
pub const PROTECTED: &[&str] = &["eboot.bin", "sce_module", "sce_sys"];
/// Files beside a mod's game folders that only describe it (a wrapper folder may hold them too).
pub const METADATA: &[&str] = &[".txt", ".md", ".jpg", ".jpeg", ".png", ".ini", ".json"];
pub const IMAGES: &[&str] = &[".png", ".jpg", ".jpeg"];
pub const FIRST_IMAGES: &[&str] = &["preview", "cover", "screenshot"];
/// Marker file inside a folder unpacked from a zip; holds the zip's size and time stamp.
pub const ZIP_STAMP: &str = ".from-zip";
/// Bytes a mod zip may unpack to (a zip bomb is refused).
pub const ZIP_LIMIT: u64 = 16 << 30;

type Roots = HashSet<String>;

// ---- Small path helpers -----------------------------------------------------------------------

fn name_of(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// The suffix of a file name with its dot, lower case (empty when it has none).
fn suffix_of(path: &Path) -> String {
    match path.extension() {
        Some(e) if !e.is_empty() => format!(".{}", e.to_string_lossy().to_lowercase()),
        _ => String::new(),
    }
}

fn stem_of(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

/// The entries of a folder (empty when it cannot be read).
fn entries(folder: &Path) -> Vec<PathBuf> {
    fs::read_dir(folder)
        .map(|it| it.flatten().map(|e| e.path()).collect())
        .unwrap_or_default()
}

/// A regular file that is not a link.
fn is_plain_file(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|m| m.is_file())
}

/// The parts of a relative path, as text.
fn parts_of(path: &Path) -> Vec<String> {
    path.components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect()
}

fn as_posix(path: &Path) -> String {
    parts_of(path).join("/")
}

/// A symbolic link, or on Windows any reparse point (a junction is one). The hard links and copies
/// an overlay made are not links on disk: only that overlay knows them (`Linker::is_link`).
pub fn is_link(path: &Path) -> bool {
    let Ok(meta) = fs::symlink_metadata(path) else {
        return false;
    };
    if meta.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        if meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return true;
        }
    }
    false
}

/// Like Python's `Path.resolve()`: follows links in the part that exists and keeps the rest as
/// written. A link that cannot be followed is an error.
pub(crate) fn resolve(path: &Path) -> io::Result<PathBuf> {
    let mut current = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    let mut tail = Vec::new();
    loop {
        match fs::canonicalize(&current) {
            Ok(found) => {
                let mut out = strip_verbatim(found);
                out.extend(tail.iter().rev());
                return Ok(out);
            }
            Err(error) => {
                if fs::symlink_metadata(&current).is_ok_and(|m| m.file_type().is_symlink()) {
                    return Err(error);
                }
                match (current.file_name().map(|n| n.to_owned()), current.parent()) {
                    (Some(name), Some(parent)) => {
                        tail.push(name);
                        current = parent.to_path_buf();
                    }
                    _ => return Err(error),
                }
            }
        }
    }
}

/// `\\?\C:\x` becomes `C:\x` and `\\?\UNC\server\share\x` becomes `\\server\share\x`, as Python's
/// `realpath` strips the long-path forms `canonicalize` gives on Windows (the probe opens
/// `"<app0>/..."` paths, and Windows does not normalize a `/` under `\\?\`).
fn strip_verbatim(path: PathBuf) -> PathBuf {
    if cfg!(windows) {
        let text = path.to_string_lossy();
        if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
            return PathBuf::from(format!(r"\\{rest}"));
        }
        if let Some(rest) = text.strip_prefix(r"\\?\") {
            let bytes = rest.as_bytes();
            if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
                return PathBuf::from(rest);
            }
        }
    }
    path
}

// ---- Layout -----------------------------------------------------------------------------------

/// `name` inside `folder`, matching an existing entry case-insensitively (mods made on Windows
/// often differ from the game's lower-case names).
fn child(folder: &Path, name: &str) -> PathBuf {
    if folder.is_dir() {
        let wanted = name.to_lowercase();
        for entry in entries(folder) {
            if name_of(&entry).to_lowercase() == wanted {
                return entry;
            }
        }
    }
    folder.join(name)
}

/// Allowed data folders for this game only; never learn executable/system folders (#234).
fn data_roots(game: Option<&Path>) -> Roots {
    let mut roots: Roots = GAME_ROOTS.iter().map(|r| (*r).to_string()).collect();
    if let Some(game) = game.filter(|g| g.is_dir()) {
        for entry in entries(game) {
            let name = name_of(&entry).to_lowercase();
            if entry.is_dir() && !PROTECTED.contains(&name.as_str()) {
                roots.insert(name);
            }
        }
    }
    roots
}

fn has_game_root(folder: &Path, roots: &Roots) -> bool {
    folder.is_dir()
        && entries(folder)
            .iter()
            .any(|e| e.is_dir() && roots.contains(&name_of(e).to_lowercase()))
}

/// Where a mod file lands: in a game without dvdroot_ps4 (`flat`), a mod's dvdroot_ps4/ prefix is
/// dropped so dvdroot_ps4/EXEC/... replaces EXEC/... (#473).
fn game_path(relative: &Path, flat: bool) -> Result<PathBuf, ModsError> {
    let mut parts = parts_of(relative);
    if flat && parts.len() > 1 && parts[0].to_lowercase() == "dvdroot_ps4" {
        parts.remove(0);
    }
    let relative: PathBuf = parts.iter().collect();
    if parts
        .first()
        .is_some_and(|p| PROTECTED.contains(&p.to_lowercase().as_str()))
    {
        return Err(ModsError::Invalid(format!(
            "Executable/system replacement is unsupported: {}",
            relative.display()
        )));
    }
    Ok(relative)
}

/// The one wrapper folder a mod may sit in: a single sub-folder and nothing but metadata files.
fn single_wrapper(folder: &Path) -> Option<PathBuf> {
    let list: Vec<PathBuf> = entries(folder)
        .into_iter()
        .filter(|e| !name_of(e).starts_with('.'))
        .collect();
    let mut folders = list.iter().filter(|e| e.is_dir());
    let first = folders.next()?;
    if folders.next().is_some() {
        return None;
    }
    let other_file = list
        .iter()
        .any(|e| e.is_file() && !METADATA.contains(&suffix_of(e).as_str()));
    if other_file {
        None
    } else {
        Some(first.clone())
    }
}

/// (root, prefix): where a mod's files are and the game path they go to, or None.
///
/// Accepted layouts: `<mod>/<game folder>`, `<mod>/app0/<game folder>` and one wrapper folder
/// around either (an archive extracted into a folder of its name), where `<game folder>` is EXEC,
/// a language folder or dvdroot_ps4 (`GAME_ROOTS`).
fn content_root(folder: &Path, roots: &Roots) -> Option<(PathBuf, String)> {
    if !folder.is_dir() {
        return None;
    }
    for wrapper in ["", "app0"] {
        let base = if wrapper.is_empty() {
            folder.to_path_buf()
        } else {
            child(folder, wrapper)
        };
        if has_game_root(&base, roots) {
            return Some((base, String::new()));
        }
    }
    single_wrapper(folder).and_then(|inner| content_root(&inner, roots))
}

/// The game patch files (XML) of a mod: `<mod>/patches/*.xml`, with the same one wrapper folder
/// around it that `content_root` accepts. A mod with only these has no files to replace.
pub fn patch_files(folder: &Path) -> Vec<PathBuf> {
    if !folder.is_dir() {
        return Vec::new();
    }
    let found = child(folder, "patches");
    if found.is_dir() {
        let mut files: Vec<PathBuf> = entries(&found)
            .into_iter()
            .filter(|f| f.is_file() && suffix_of(f) == ".xml")
            .collect();
        files.sort_by_key(|f| name_of(f).to_lowercase());
        if !files.is_empty() {
            return files;
        }
    }
    match single_wrapper(folder) {
        Some(inner) => patch_files(&inner),
        None => Vec::new(),
    }
}

// ---- Information shown for a mod --------------------------------------------------------------

/// What the launcher shows for a mod.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModInfo {
    pub name: String,
    pub author: String,
    pub version: String,
    pub description: String,
    pub url: String,
    pub image: Option<PathBuf>,
}

/// Name, author, version, description and url from an optional mod.json, else the folder name and
/// the start of a readme; `image` is the mod's preview (preview, cover or screenshot first, else
/// any picture) or None. Only reads inside the mod.
pub fn mod_info(folder: &Path, game: Option<&Path>) -> ModInfo {
    let layout = content_root(folder, &data_roots(game));
    let mut places = vec![folder.to_path_buf()];
    if let Some((root, _)) = &layout {
        if root != folder {
            places.push(root.clone());
        }
    }
    let mut info = ModInfo {
        name: name_of(folder),
        author: String::new(),
        version: String::new(),
        description: String::new(),
        url: String::new(),
        image: None,
    };
    let mut files = Vec::new();
    for place in &places {
        let mut here: Vec<PathBuf> = entries(place)
            .into_iter()
            .filter(|e| is_plain_file(e))
            .collect();
        here.sort_by_key(|e| name_of(e).to_lowercase());
        files.extend(here);
    }
    let fields = files
        .iter()
        .find(|e| name_of(e).to_lowercase() == "mod.json")
        .and_then(|meta| fs::read_to_string(meta).ok())
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .unwrap_or(Value::Null);
    let field = |key: &str| -> Option<String> {
        let value = fields.get(key)?.as_str()?.trim();
        (!value.is_empty()).then(|| value.to_string())
    };
    if let Some(v) = field("name") {
        info.name = v;
    }
    if let Some(v) = field("author") {
        info.author = v;
    }
    if let Some(v) = field("version") {
        info.version = v;
    }
    if let Some(v) = field("description") {
        info.description = v;
    }
    if let Some(v) = field("url") {
        info.url = v;
    }
    let url = info.url.to_lowercase();
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        info.url.clear();
    }
    let readme = files
        .iter()
        .find(|e| stem_of(e) == "readme" && matches!(suffix_of(e).as_str(), ".txt" | ".md"));
    if info.description.is_empty() {
        if let Some(readme) = readme {
            if let Ok(bytes) = fs::read(readme) {
                let text = String::from_utf8_lossy(&bytes);
                info.description = text.trim().chars().take(400).collect();
            }
        }
    }
    let mut images: Vec<&PathBuf> = files
        .iter()
        .filter(|e| IMAGES.contains(&suffix_of(e).as_str()))
        .collect();
    images.sort_by_key(|e| {
        FIRST_IMAGES
            .iter()
            .position(|f| *f == stem_of(e))
            .unwrap_or(FIRST_IMAGES.len())
    });
    info.image = images.first().map(|p| (*p).clone());
    info
}

// ---- Zip archives -----------------------------------------------------------------------------

/// A file or folder of a mod zip that passed `zip_members`.
#[derive(Debug)]
struct Member {
    index: usize,
    relative: PathBuf,
    is_dir: bool,
    size: u64,
}

fn zip_error(error: zip::result::ZipError) -> ModsError {
    ModsError::Invalid(error.to_string())
}

/// The path parts of a zip entry name, the way the Python `Path(...).parts` gives them: both
/// separators, no empty or "." parts.
fn zip_parts(filename: &str) -> Vec<String> {
    filename
        .replace('\\', "/")
        .split('/')
        .filter(|p| !p.is_empty() && *p != ".")
        .map(str::to_string)
        .collect()
}

/// The files of a mod zip and their relative paths. Anything that could write outside the mod's
/// folder (absolute paths, "..", drive letters, alternate streams) or is a link is refused; the
/// whole archive is checked before anything is extracted.
fn zip_members<R: Read + io::Seek>(
    archive: &mut zip::ZipArchive<R>,
) -> Result<Vec<Member>, ModsError> {
    let mut total: u64 = 0;
    let mut members = Vec::new();
    for index in 0..archive.len() {
        let entry = archive.by_index_raw(index).map_err(zip_error)?;
        let filename = entry.name().map_err(zip_error)?.into_owned();
        let is_dir = filename.ends_with('/');
        let parts = zip_parts(&filename);
        if is_dir && parts.is_empty() {
            continue;
        }
        if parts.is_empty()
            || filename.starts_with('/')
            || filename.starts_with('\\')
            || parts.iter().any(|p| p == "..")
            || parts
                .iter()
                .any(|p| p.contains(':') || p.contains('\0') || p.trim_end_matches([' ', '.']) != p)
        {
            return Err(ModsError::Invalid(format!(
                "unsafe path in the zip: {filename}"
            )));
        }
        if (entry.external_attributes() >> 16) & 0o170000 == 0o120000 {
            return Err(ModsError::Invalid(format!(
                "links are not allowed in a mod zip: {filename}"
            )));
        }
        total = total.saturating_add(entry.size());
        if total > ZIP_LIMIT {
            return Err(ModsError::Invalid(
                "the zip unpacks to more than 16 GiB".to_string(),
            ));
        }
        members.push(Member {
            index,
            relative: parts.iter().collect(),
            is_dir,
            size: entry.size(),
        });
    }
    Ok(members)
}

/// Copies at most `declared` bytes (the size the zip announced for the member) and no more than
/// `budget`, which shrinks by what was written. A member that inflates past either is refused, so
/// a zip that lies about its sizes cannot fill the disk.
fn copy_capped<R: Read, W: io::Write>(
    reader: &mut R,
    writer: &mut W,
    declared: u64,
    budget: &mut u64,
) -> Result<(), ModsError> {
    let limit = declared.min(*budget);
    let written = io::copy(&mut reader.take(limit.saturating_add(1)), writer)?;
    if written > declared {
        return Err(ModsError::Invalid(
            "a file in the zip is larger than its declared size".to_string(),
        ));
    }
    if written > *budget {
        return Err(ModsError::Invalid(
            "the zip unpacks to more than 16 GiB".to_string(),
        ));
    }
    *budget -= written;
    Ok(())
}

/// Extracts one zip into `target` (replacing what the last unpack left) and writes the marker.
fn extract_zip(
    archive_path: &Path,
    target: &Path,
    resolved_target: &Path,
    stamp: &str,
) -> Result<(), ModsError> {
    let mut archive = zip::ZipArchive::new(fs::File::open(archive_path)?).map_err(zip_error)?;
    let members = zip_members(&mut archive)?;
    let mut destinations = Vec::new();
    for member in &members {
        let destination = target.join(&member.relative);
        if !resolve(&destination)?.starts_with(resolved_target) {
            return Err(ModsError::Invalid(
                "unsafe path in the zip: destination outside the mod folder".to_string(),
            ));
        }
        destinations.push(destination);
    }
    if target.exists() {
        fs::remove_dir_all(target)?;
    }
    let mut budget = ZIP_LIMIT;
    for (member, destination) in members.iter().zip(&destinations) {
        if member.is_dir {
            fs::create_dir_all(destination)?;
            continue;
        }
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut source = archive.by_index(member.index).map_err(zip_error)?;
        let mut out = fs::File::create(destination)?;
        copy_capped(&mut source, &mut out, member.size, &mut budget)?;
    }
    fs::create_dir_all(target)?;
    fs::write(target.join(ZIP_STAMP), stamp)?;
    Ok(())
}

/// `<size> <modified time in ns>` of a zip, the stamp that tells when it changed.
fn zip_stamp(archive_path: &Path) -> io::Result<String> {
    let meta = fs::metadata(archive_path)?;
    let nanos = match meta.modified()?.duration_since(UNIX_EPOCH) {
        Ok(after) => i128::try_from(after.as_nanos()).unwrap_or(i128::MAX),
        Err(before) => -i128::try_from(before.duration().as_nanos()).unwrap_or(i128::MAX),
    };
    Ok(format!("{} {}", meta.len(), nanos))
}

/// Unpacks every `<root>/<name>.zip` into `<root>/<name>/` (a mod dropped into the mods folder as
/// a zip). Done again when the zip changes; a folder with that name that did not come from the zip
/// is left alone. Returns the names unpacked now; a zip that cannot be unpacked is reported on
/// stderr and skipped.
pub fn unpack_zips(root: &Path, game: Option<&Path>) -> Vec<String> {
    let mut done = Vec::new();
    if !root.is_dir() {
        return done;
    }
    let mut zips: Vec<PathBuf> = entries(root)
        .into_iter()
        .filter(|p| {
            let name = name_of(p);
            let name = if cfg!(windows) {
                name.to_lowercase()
            } else {
                name
            };
            name.len() > 4 && name.ends_with(".zip")
        })
        .collect();
    zips.sort_by_key(|p| (name_of(p).to_lowercase(), name_of(p)));
    for archive_path in zips {
        let archive_name = name_of(&archive_path);
        let stem = archive_name[..archive_name.len() - 4].to_string();
        let target = root.join(&stem);
        let resolved_target = match resolve(&target).and_then(|t| Ok((t, resolve(root)?))) {
            Ok((t, r)) if t.parent() == Some(r.as_path()) => t,
            Ok(_) => {
                eprintln!("Mods: {archive_name}: unsafe extraction folder");
                continue;
            }
            Err(error) => {
                eprintln!("Mods: {archive_name}: {error}");
                continue;
            }
        };
        let stamp = match zip_stamp(&archive_path) {
            Ok(stamp) => stamp,
            Err(error) => {
                eprintln!("Mods: {archive_name}: {error}");
                continue;
            }
        };
        let marker = target.join(ZIP_STAMP);
        if target.exists() {
            let marker_file = marker.is_file();
            let changed = marker_file && fs::read_to_string(&marker).map_or(true, |s| s != stamp);
            if !changed {
                if !marker_file {
                    eprintln!(
                        "Mods: {archive_name}: a folder called {stem} is already there; the zip is not unpacked"
                    );
                }
                continue;
            }
        }
        match extract_zip(&archive_path, &target, &resolved_target, &stamp) {
            Ok(()) => {
                done.push(stem);
                let is_mod = content_root(&target, &data_roots(game)).is_some()
                    || !patch_files(&target).is_empty();
                let kind = if is_mod {
                    "a mod"
                } else {
                    "not a mod (no EXEC, language folder, dvdroot_ps4 or patches)"
                };
                eprintln!("Mods: {archive_name} unpacked: {kind}");
            }
            Err(error) => {
                let _ = fs::remove_dir_all(&target);
                eprintln!("Mods: {archive_name}: {error}");
            }
        }
    }
    done
}

// ---- Discovery and selection ------------------------------------------------------------------

/// Named mods: folders of `root` with an accepted layout (`content_root`) or with patch files. A
/// mod dropped in as `<name>.zip` is unpacked first (`unpack_zips`).
pub fn discover(root: &Path, game: Option<&Path>) -> Vec<String> {
    if !root.is_dir() {
        return Vec::new();
    }
    unpack_zips(root, game);
    let roots = data_roots(game);
    let mut names: Vec<String> = entries(root)
        .into_iter()
        .filter(|p| p.is_dir() && (content_root(p, &roots).is_some() || !patch_files(p).is_empty()))
        .map(|p| name_of(&p))
        .collect();
    names.sort_by(|a, b| (a.to_lowercase(), a).cmp(&(b.to_lowercase(), b)));
    names
}

/// A list of strings from `settings[key]` (empty when absent), or `message` when it is not one.
fn name_list(settings: &Value, key: &str, message: &str) -> Result<Vec<String>, ModsError> {
    let Some(value) = settings.get(key) else {
        return Ok(Vec::new());
    };
    let invalid = || ModsError::Invalid(message.to_string());
    value
        .as_array()
        .ok_or_else(invalid)?
        .iter()
        .map(|n| n.as_str().map(str::to_string).ok_or_else(invalid))
        .collect()
}

/// Keeps the first of each name.
fn unique(names: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut seen = HashSet::new();
    names
        .into_iter()
        .filter(|n| seen.insert(n.clone()))
        .collect()
}

/// The enabled mods in load order: the saved order first, new folders last, alphabetical, and
/// enabled; `disabled` ones dropped. Without a settings file, every mod.
pub fn selected(
    root: &Path,
    config: Option<&Path>,
    game: Option<&Path>,
) -> Result<Vec<String>, ModsError> {
    let available = discover(root, game);
    let Some(config) = config.filter(|c| c.is_file()) else {
        return Ok(available);
    };
    let settings: Value = serde_json::from_str(&fs::read_to_string(config)?)
        .map_err(|e| ModsError::Invalid(e.to_string()))?;
    if !settings.is_object() {
        return Err(ModsError::Invalid(
            "The mods settings must be an object".to_string(),
        ));
    }
    let disabled: HashSet<String> = name_list(
        &settings,
        "disabled",
        "Disabled mods must be a list of folder names",
    )?
    .into_iter()
    .collect();
    let order = name_list(
        &settings,
        "order",
        "Mod order must be a list of folder names",
    )?;
    let present: HashSet<&String> = available.iter().collect();
    // New folders are enabled automatically and appended in alphabetical order.
    Ok(unique(
        order
            .iter()
            .chain(&available)
            .filter(|n| present.contains(n) && !disabled.contains(*n))
            .cloned(),
    ))
}

/// The patch files of the enabled mods, in load order (a later file wins where patches share a
/// name); none when mods are off. A selection that cannot be read gives none: building the overlay
/// from the same selection reports why.
pub fn enabled_patch_files(
    root: &Path,
    config: &Path,
    enabled: bool,
    game: Option<&Path>,
) -> Vec<PathBuf> {
    if !enabled {
        return Vec::new();
    }
    selected(root, Some(config), game)
        .unwrap_or_default()
        .iter()
        .flat_map(|name| patch_files(&root.join(name)))
        .collect()
}

/// The folders `build_overlay` lays over the game, in load order (Python `mods.py`'s `main`):
/// with mods on, shadPS4's loose `<game>-mods` folder beside the game, then the mods folder itself
/// when it is a loose overlay (it holds a game folder), else the mods `selected` in it.
pub fn overlay_layers(
    game: &Path,
    mods_dir: &Path,
    config: Option<&Path>,
    enabled: bool,
) -> Result<Vec<PathBuf>, ModsError> {
    let mut layers = Vec::new();
    if !enabled {
        return Ok(layers);
    }
    let mut legacy = resolve(game)?.into_os_string();
    legacy.push("-mods");
    let legacy = PathBuf::from(legacy);
    if legacy.is_dir() {
        layers.push(legacy);
    }
    if has_game_root(mods_dir, &data_roots(Some(game))) {
        layers.push(mods_dir.to_path_buf());
    } else {
        for name in selected(mods_dir, config, Some(game))? {
            layers.push(mods_dir.join(name));
        }
    }
    Ok(layers)
}

// ---- Files and conflicts ----------------------------------------------------------------------

/// Visits `directory`: refuses links, then collects its files and goes into its folders.
fn walk(
    directory: &Path,
    root: &Path,
    prefix: &str,
    roots: &Roots,
    found: &mut Vec<(PathBuf, PathBuf)>,
) -> Result<(), ModsError> {
    let mut folders = Vec::new();
    let mut files = Vec::new();
    for entry in entries(directory) {
        // A Windows junction is not a symlink but would let a mod reach outside its folder;
        // `is_link` rejects both.
        if is_link(&entry) {
            return Err(ModsError::Invalid(format!(
                "Mod links are unsupported: {}",
                entry.display()
            )));
        }
        if entry.is_dir() {
            folders.push(entry);
        } else {
            files.push(entry);
        }
    }
    files.sort_by_key(|f| name_of(f));
    folders.sort_by_key(|f| name_of(f));
    for source in files {
        let inner = source.strip_prefix(root).map_err(|_| {
            ModsError::Invalid(format!("Not a regular mod file: {}", source.display()))
        })?;
        let relative = if prefix.is_empty() {
            inner.to_path_buf()
        } else {
            Path::new(prefix).join(inner)
        };
        let first = parts_of(&relative)
            .into_iter()
            .next()
            .unwrap_or_default()
            .to_lowercase();
        // Readme/metadata stay outside the mounted game. This loader handles assets; executable
        // patches use the existing patch compiler, with address validation.
        if !roots.contains(&first) {
            if PROTECTED.contains(&first.as_str()) {
                return Err(ModsError::Invalid(format!(
                    "Executable/system replacement is unsupported: {}",
                    source.display()
                )));
            }
            continue;
        }
        if !source.is_file() {
            return Err(ModsError::Invalid(format!(
                "Not a regular mod file: {}",
                source.display()
            )));
        }
        found.push((relative, source));
    }
    for folder in folders {
        walk(&folder, root, prefix, roots, found)?;
    }
    Ok(())
}

/// (mod path, source) of every file a mod replaces or adds under a `GAME_ROOTS` folder; the game
/// path is `game_path(mod path)`. A mod with only patches has none.
pub fn mod_files(folder: &Path, game: Option<&Path>) -> Result<Vec<(PathBuf, PathBuf)>, ModsError> {
    let roots = data_roots(game);
    let Some((root, prefix)) = content_root(folder, &roots) else {
        if !patch_files(folder).is_empty() {
            return Ok(Vec::new()); // patches only
        }
        return Err(ModsError::Invalid(format!(
            "{}: expected EXEC, a language folder or dvdroot_ps4 inside the mod folder",
            folder.display()
        )));
    };
    let root = resolve(&root)?;
    let mut found = Vec::new();
    walk(&root, &root, &prefix, &roots, &mut found)?;
    Ok(found)
}

/// Files more than one of `names` (in load order) replaces: `[(game path, [mods])]`, the last mod
/// of each list wins. Paths match case-insensitively, as in the overlay; unreadable mods are
/// skipped.
pub fn conflicts(root: &Path, names: &[String], game: Option<&Path>) -> Vec<(String, Vec<String>)> {
    let mut owners: HashMap<String, (String, Vec<String>)> = HashMap::new();
    for name in names {
        let Ok(files) = mod_files(&root.join(name), game) else {
            continue;
        };
        for (relative, _source) in files {
            // The game has no dvdroot_ps4.
            let Ok(relative) = game_path(&relative, true) else {
                break;
            };
            let path = as_posix(&relative);
            owners
                .entry(path.to_lowercase())
                .or_insert_with(|| (path, Vec::new()))
                .1
                .push(name.clone());
        }
    }
    let mut found: Vec<(String, Vec<String>)> = owners
        .into_values()
        .filter(|(_, mods)| mods.len() > 1)
        .collect();
    found.sort();
    found
}

// ---- Presets ----------------------------------------------------------------------------------

/// (order, disabled) of a saved preset for the mods present now: missing ones are dropped, new
/// ones go last and enabled.
pub fn apply_preset(preset: &Value, available: &[String]) -> (Vec<String>, Vec<String>) {
    let list = |key: &str| -> Vec<String> {
        preset
            .get(key)
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|n| n.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    };
    let present: HashSet<&String> = available.iter().collect();
    let order = unique(
        list("order")
            .into_iter()
            .chain(available.iter().cloned())
            .filter(|n| present.contains(n)),
    );
    let disabled = list("disabled")
        .into_iter()
        .filter(|n| present.contains(n))
        .collect();
    (order, disabled)
}

// ---- Links ------------------------------------------------------------------------------------

/// How an entry of an overlay reaches its game or mod file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LinkKind {
    /// A symbolic link (on Windows it needs the privilege Developer Mode gives).
    Symlink,
    /// A Windows junction to a folder: no privilege needed, local volumes only.
    Junction,
    /// A hard link to a file: same volume only.
    HardLink,
    /// No link: a folder mirrored as real folders whose files are linked one by one, or a file
    /// copied.
    Mirror,
}

/// Every way of linking allowed: what runs outside the tests.
pub(crate) fn every_way(_: LinkKind) -> bool {
    true
}

/// What one overlay remembers while it links: the Python's module-level `LINKED`, kept per
/// overlay. Junctions, hard links and copies are recorded with their targets (a hard link or a copy
/// cannot be told from a real file afterwards), and a way of linking the volume refused (symlinks
/// without the privilege, junctions on a network share) is not tried again. Hard links are always
/// tried: they fail only for the files of another volume.
pub(crate) struct Linker<'a> {
    allowed: &'a dyn Fn(LinkKind) -> bool,
    linked: HashMap<PathBuf, PathBuf>,
    refused: HashSet<LinkKind>,
    /// The folders being mirrored, outermost first (resolved): one holding a link back to another
    /// would be mirrored forever.
    mirroring: Vec<PathBuf>,
}

impl<'a> Linker<'a> {
    /// A linker that tries only the ways `allowed` gives (the tests refuse some to run the others
    /// where the system would not need them).
    pub(crate) fn new(allowed: &'a dyn Fn(LinkKind) -> bool) -> Linker<'a> {
        Linker {
            allowed,
            linked: HashMap::new(),
            refused: HashSet::new(),
            mirroring: Vec::new(),
        }
    }

    fn may(&self, kind: LinkKind) -> bool {
        (self.allowed)(kind) && !self.refused.contains(&kind)
    }

    fn refuse(&mut self, kind: LinkKind, error: &io::Error) {
        // A name already taken or a folder missing says nothing about the volume.
        if !matches!(
            error.kind(),
            io::ErrorKind::AlreadyExists | io::ErrorKind::NotFound
        ) {
            self.refused.insert(kind);
        }
    }

    /// Links `link` to `target` (the Python's `make_link`): a symlink; on Windows, when that is
    /// refused, a junction for a folder and a hard link for a file; failing those, a mirrored folder
    /// or a copied file. Elsewhere than Windows a refused symlink is an error, as in the Python (the
    /// tests reach the other ways there by not allowing symlinks).
    pub(crate) fn make(&mut self, link: &Path, target: &Path) -> io::Result<LinkKind> {
        let is_dir = target.is_dir();
        if self.may(LinkKind::Symlink) {
            match symlink_to(link, target, is_dir) {
                Ok(()) => return Ok(LinkKind::Symlink),
                Err(error) if !cfg!(windows) => return Err(error),
                Err(error) => self.refuse(LinkKind::Symlink, &error),
            }
        }
        if is_dir {
            if self.may(LinkKind::Junction) {
                match platform::create_junction(link, target) {
                    Ok(()) => {
                        self.linked.insert(link.to_path_buf(), target.to_path_buf());
                        return Ok(LinkKind::Junction);
                    }
                    Err(error) => self.refuse(LinkKind::Junction, &error),
                }
            }
            self.mirror(link, target)?;
            return Ok(LinkKind::Mirror);
        }
        let kind = if self.may(LinkKind::HardLink) && fs::hard_link(target, link).is_ok() {
            LinkKind::HardLink
        } else if (self.allowed)(LinkKind::Mirror) {
            copy_new(target, link)?;
            LinkKind::Mirror
        } else {
            return Err(no_way(link));
        };
        self.linked.insert(link.to_path_buf(), target.to_path_buf());
        Ok(kind)
    }

    /// A folder made of real folders whose files are linked one by one: what a network share, which
    /// takes neither symlinks nor junctions, can hold. Not recorded: it is the plain folder it looks
    /// like.
    fn mirror(&mut self, link: &Path, target: &Path) -> io::Result<()> {
        if !(self.allowed)(LinkKind::Mirror) {
            return Err(no_way(link));
        }
        let real = fs::canonicalize(target)?;
        if self.mirroring.contains(&real) {
            return Err(io::Error::other(format!(
                "{}: the folder leads back into itself",
                target.display()
            )));
        }
        match fs::create_dir(link) {
            Ok(()) => {}
            // An empty folder a refused junction could not take back.
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists && is_empty_folder(link) => {
            }
            Err(error) => return Err(error),
        }
        self.mirroring.push(real);
        let result = self.link_entries(link, target);
        self.mirroring.pop();
        result
    }

    /// Links each entry of the folder `target` into the folder `folder`, under the same name.
    pub(crate) fn link_entries(&mut self, folder: &Path, target: &Path) -> io::Result<()> {
        for entry in fs::read_dir(target)? {
            let entry = entry?;
            self.make(&folder.join(entry.file_name()), &entry.path())?;
        }
        Ok(())
    }

    /// The Python's `is_link`: a link on disk, or a junction, hard link or copy this overlay made.
    pub(crate) fn is_link(&self, path: &Path) -> bool {
        self.linked.contains_key(path) || is_link(path)
    }

    /// Where `path` leads: what this overlay recorded, else the resolved link.
    pub(crate) fn target(&self, path: &Path) -> Option<PathBuf> {
        self.linked.get(path).cloned().or_else(|| link_target(path))
    }

    /// Removes `path` without following links, and forgets what was recorded there.
    pub(crate) fn remove(&mut self, path: &Path) -> io::Result<()> {
        let folder = !self.is_link(path) && fs::symlink_metadata(path).is_ok_and(|m| m.is_dir());
        remove_tree(path)?;
        if folder {
            self.linked.retain(|link, _| !link.starts_with(path));
        } else {
            self.linked.remove(path);
        }
        Ok(())
    }

    /// Makes the linked folder `directory` a real folder of links to what it held, one level deep,
    /// so a file inside can be replaced without writing through a link (the Python's `expand`). A
    /// missing folder is made; a file where a folder is wanted is a conflict.
    pub(crate) fn expand(&mut self, directory: &Path) -> Result<(), ModsError> {
        let conflict =
            || ModsError::Invalid(format!("File/directory conflict at {}", name_of(directory)));
        if self.is_link(directory) {
            let target = self.target(directory).ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("{}: the link leads nowhere", directory.display()),
                )
            })?;
            if !target.is_dir() {
                return Err(conflict());
            }
            self.remove(directory)?;
            fs::create_dir(directory)?;
            self.link_entries(directory, &target)?;
        } else if directory.exists() && !directory.is_dir() {
            return Err(conflict());
        } else if !directory.is_dir() {
            fs::create_dir(directory)?;
        }
        Ok(())
    }
}

fn no_way(link: &Path) -> io::Error {
    io::Error::new(
        io::ErrorKind::Unsupported,
        format!("{}: no allowed way to link it", link.display()),
    )
}

fn is_empty_folder(path: &Path) -> bool {
    !is_link(path) && fs::read_dir(path).is_ok_and(|mut it| it.next().is_none())
}

/// `link` made a symbolic link to `target` (Python's `link.symlink_to(target, target_is_directory)`).
fn symlink_to(link: &Path, target: &Path, is_dir: bool) -> io::Result<()> {
    #[cfg(windows)]
    {
        if is_dir {
            std::os::windows::fs::symlink_dir(target, link)
        } else {
            std::os::windows::fs::symlink_file(target, link)
        }
    }
    #[cfg(not(windows))]
    {
        let _ = is_dir;
        std::os::unix::fs::symlink(target, link)
    }
}

/// Copies the file `target` to the new file `link`, never through an entry already there; the
/// modification time follows, as `shutil.copy2` keeps it.
fn copy_new(target: &Path, link: &Path) -> io::Result<()> {
    let mut source = fs::File::open(target)?;
    let mut copy = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(link)?;
    io::copy(&mut source, &mut copy)?;
    if let Ok(modified) = source.metadata().and_then(|m| m.modified()) {
        // The contents are what the game reads; a time stamp that cannot be set changes nothing.
        let _ = copy.set_modified(modified);
    }
    Ok(())
}

/// Links `link` to `target`, trying each way in the Python's order (see `Linker::make`), and
/// returns the way used.
pub fn make_link(link: &Path, target: &Path) -> io::Result<LinkKind> {
    Linker::new(&every_way).make(link, target)
}

/// Whether a symlink can be made in `directory` (on Windows it needs Developer Mode or an
/// administrator).
pub fn symlinks_work(directory: &Path) -> bool {
    let Ok(probe) = tempfile::Builder::new()
        .prefix("link-test-")
        .tempdir_in(directory)
    else {
        return false;
    };
    symlink_to(&probe.path().join("link"), probe.path(), true).is_ok()
}

/// Where the link `path` leads, resolved; `None` when it leads nowhere.
pub fn link_target(path: &Path) -> Option<PathBuf> {
    fs::canonicalize(path).ok().map(strip_verbatim)
}

/// Removes the link `path` itself: a junction or a Windows folder symlink with `remove_dir`, which
/// never empties what it leads to; anything else with `remove_file`.
pub fn remove_link(path: &Path) -> io::Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_DIRECTORY: u32 = 0x10;
        if fs::symlink_metadata(path)?.file_attributes() & FILE_ATTRIBUTE_DIRECTORY != 0 {
            return fs::remove_dir(path);
        }
    }
    fs::remove_file(path)
}

/// Deletes `path` and what it holds without ever following a link: a link is removed, not what it
/// leads to, so the game, the mods and the dumps stay whole.
pub fn remove_tree(path: &Path) -> io::Result<()> {
    if is_link(path) {
        return remove_link(path);
    }
    if fs::symlink_metadata(path)?.is_dir() {
        for entry in fs::read_dir(path)? {
            remove_tree(&entry?.path())?;
        }
        fs::remove_dir(path)
    } else {
        fs::remove_file(path)
    }
}

/// The Python's `expand` on its own (see `Linker::expand`): hard links and copies an overlay made
/// are known to that overlay only.
pub fn expand(directory: &Path) -> Result<(), ModsError> {
    Linker::new(&every_way).expand(directory)
}

// ---- Overlay ----------------------------------------------------------------------------------

/// The overlay folders `build_overlay` makes in `out`: `mod-game-XXXXXX`.
const OVERLAY_PREFIX: &str = "mod-game-";

/// The game folder to mount: an overlay of links with the enabled mods laid over the game, or the
/// game folder itself when no mod replaces a file.
#[derive(Debug)]
pub struct Overlay {
    pub root: PathBuf,
    /// Whether `root` was made by `build_overlay` (and so goes with `remove`).
    owned: bool,
}

impl Overlay {
    /// Deletes the overlay without following its links (the game and the mods stay whole); the
    /// game folder itself is left alone.
    pub fn remove(self) -> io::Result<()> {
        if self.owned {
            remove_tree(&self.root)
        } else {
            Ok(())
        }
    }
}

/// The game with `mods` laid over it in load order, the last one winning a file: a folder
/// `mod-game-XXXXXX` in `out` whose entries link to the game's, with the folders on the way to a
/// replaced file made real and the mod's file linked in its place. Names take the game's spelling
/// whatever the case a mod uses. Without a file to replace, the game folder itself. The overlays a
/// crashed run left in `out` are removed first, never through their links. On Windows without the
/// symlink privilege, an `out` on another volume than the game is traded for the game's parent
/// folder (hard links reach the same volume only).
pub fn build_overlay(game: &Path, out: &Path, mods: &[PathBuf]) -> Result<Overlay, ModsError> {
    build_overlay_with(game, out, mods, &every_way)
}

/// `build_overlay` trying only the ways of linking `allowed` gives.
pub(crate) fn build_overlay_with(
    game: &Path,
    out: &Path,
    mods: &[PathBuf],
    allowed: &dyn Fn(LinkKind) -> bool,
) -> Result<Overlay, ModsError> {
    let game = strip_verbatim(fs::canonicalize(game)?);
    let flat = !child(&game, "dvdroot_ps4").is_dir();
    let mut replacements = Vec::new();
    let mut owners: HashMap<String, String> = HashMap::new();
    for root in mods {
        let name = name_of(root);
        let mut count = 0;
        for (relative, source) in mod_files(root, Some(&game))? {
            let relative = game_path(&relative, flat)?;
            let key = as_posix(&relative).to_lowercase();
            if let Some(before) = owners.insert(key, name.clone()) {
                eprintln!("Mods: {}: {before} -> {name}", relative.display());
            }
            replacements.push((relative, source));
            count += 1;
        }
        eprintln!("Mods: {name}: {count} files");
    }
    if replacements.is_empty() {
        return Ok(Overlay {
            root: game,
            owned: false,
        });
    }
    let mut out = resolve(out)?;
    fs::create_dir_all(&out)?;
    remove_stale_overlays(&out);
    if cfg!(windows)
        && !(allowed(LinkKind::Symlink) && symlinks_work(&out))
        && drive(&out) != drive(&game)
    {
        // Hard links only reach files on the same volume: the overlay goes beside the game.
        if let Some(parent) = game.parent() {
            out = parent.to_path_buf();
        }
    }
    let root = tempfile::Builder::new()
        .prefix(OVERLAY_PREFIX)
        .tempdir_in(&out)?
        .keep();
    let mut linker = Linker::new(allowed);
    match lay_over(&mut linker, &game, &root, &replacements) {
        Ok((replaced, added)) => {
            eprintln!("Mods: {replaced} game files replaced, {added} added");
            Ok(Overlay { root, owned: true })
        }
        Err(error) => {
            // The links go, never what they lead to; the error reported is the one that stopped
            // the overlay.
            let _ = remove_tree(&root);
            Err(error)
        }
    }
}

/// Fills the new overlay `root`: links to the game's entries, then each replacement in order.
/// Returns how many game files were replaced and how many files were added.
fn lay_over(
    linker: &mut Linker<'_>,
    game: &Path,
    root: &Path,
    replacements: &[(PathBuf, PathBuf)],
) -> Result<(usize, usize), ModsError> {
    linker.link_entries(root, game)?;
    let (mut replaced, mut added) = (0, 0);
    for (relative, source) in replacements {
        let parts = parts_of(relative);
        let Some((name, folders)) = parts.split_last() else {
            continue;
        };
        // Each part takes the game's spelling when it exists in another case.
        let mut parent = root.to_path_buf();
        for folder in folders {
            parent = child(&parent, folder);
            linker.expand(&parent)?;
        }
        let destination = child(&parent, name);
        if linker.is_link(&destination) {
            if linker
                .target(&destination)
                .is_some_and(|t| t.starts_with(game))
            {
                replaced += 1;
            }
            linker.remove(&destination)?;
        } else if fs::symlink_metadata(&destination).is_ok() {
            return Err(ModsError::Invalid(format!(
                "File/directory conflict: {}",
                relative.display()
            )));
        } else {
            added += 1;
        }
        linker.make(&destination, source)?;
    }
    Ok((replaced, added))
}

/// Removes the overlays a crashed run left in `out`, never through their links. One that cannot
/// go is reported and left: the new overlay gets another name.
fn remove_stale_overlays(out: &Path) {
    for entry in entries(out) {
        if name_of(&entry).starts_with(OVERLAY_PREFIX) {
            if let Err(error) = remove_tree(&entry) {
                eprintln!("Mods: {}: {error}", entry.display());
            }
        }
    }
}

/// The drive or share a path starts with, lower case (`c:`); empty without one.
fn drive(path: &Path) -> String {
    match path.components().next() {
        Some(Component::Prefix(prefix)) => prefix.as_os_str().to_string_lossy().to_lowercase(),
        _ => String::new(),
    }
}

// ---- Tests ------------------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Write;

    const PATCH: &str = "<Patch><Metadata Name=\"Resolution Patch - 3440x1440\" AppVer=\"01.02\"><PatchList/></Metadata></Patch>";

    fn write(path: &Path, bytes: &[u8]) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }

    fn names(files: &[PathBuf]) -> Vec<String> {
        files.iter().map(|f| name_of(f)).collect()
    }

    /// The names in a folder, sorted (empty when it is missing).
    fn listing(folder: &Path) -> Vec<String> {
        let mut found = names(&entries(folder));
        found.sort();
        found
    }

    /// The ways of linking the overlay tests force in turn: whatever works here (symlinks, or on
    /// Windows without the privilege junctions and hard links), no symlinks (junctions and hard
    /// links on Windows, mirrored folders elsewhere), a network share (no symlinks or junctions:
    /// mirrored folders of hard links) and copies only.
    type Way = (&'static str, fn(LinkKind) -> bool);
    const WAYS: [Way; 4] = [
        ("any", |_| true),
        ("no symlinks", |kind| kind != LinkKind::Symlink),
        ("network share", |kind| {
            matches!(kind, LinkKind::HardLink | LinkKind::Mirror)
        }),
        ("copies", |kind| kind == LinkKind::Mirror),
    ];

    /// A game (dvdroot_ps4 layout) and a mods folder, as `ModTests.setUp` makes them.
    struct Fixture {
        _temp: tempfile::TempDir,
        root: PathBuf,
        game: PathBuf,
        moddir: PathBuf,
    }

    impl Fixture {
        fn new() -> Fixture {
            let temp = tempfile::tempdir().unwrap();
            let root = resolve(temp.path()).unwrap();
            let game = root.join("CUSA01623");
            write(&game.join("eboot.bin"), b"original executable");
            write(&game.join("dvdroot_ps4/data/a.wad"), b"original");
            write(&game.join("dvdroot_ps4/data/b.wad"), b"untouched");
            Fixture {
                moddir: root.join("mods"),
                _temp: temp,
                root,
                game,
            }
        }

        fn mod_file(&self, name: &str, contents: &[u8], file: &str) -> PathBuf {
            let root = self.moddir.join(name);
            write(&root.join("dvdroot_ps4/data").join(file), contents);
            root
        }

        fn patch_mod(&self, name: &str, files: &[&str], wrapper: Option<&str>) -> PathBuf {
            let folder = self
                .moddir
                .join(name)
                .join(wrapper.unwrap_or(""))
                .join("patches");
            for file in files {
                write(&folder.join(file), PATCH.as_bytes());
            }
            self.moddir.join(name)
        }

        fn overlay(
            &self,
            mods: &[PathBuf],
            allowed: fn(LinkKind) -> bool,
        ) -> Result<Overlay, ModsError> {
            build_overlay_with(&self.game, &self.root.join("out"), mods, &allowed)
        }

        /// `<mods>/<name>.zip` with the given (path, bytes) entries.
        fn make_zip(&self, name: &str, entries: &[(&str, &[u8])]) -> PathBuf {
            fs::create_dir_all(&self.moddir).unwrap();
            let path = self.moddir.join(format!("{name}.zip"));
            let mut archive = zip::ZipWriter::new(fs::File::create(&path).unwrap());
            for (member, data) in entries {
                archive
                    .start_file(*member, zip::write::SimpleFileOptions::default())
                    .unwrap();
                archive.write_all(data).unwrap();
            }
            archive.finish().unwrap();
            path
        }
    }

    fn open(path: &Path) -> zip::ZipArchive<fs::File> {
        zip::ZipArchive::new(fs::File::open(path).unwrap()).unwrap()
    }

    #[test]
    fn selection_disable_and_new_mod() {
        let f = Fixture::new();
        for name in ["C", "B", "A"] {
            f.mod_file(name, b"mod", "a.wad");
        }
        let config = f.root.join("mods.json");
        fs::write(
            &config,
            r#"{"order": ["B", "A", "deleted"], "disabled": ["A"]}"#,
        )
        .unwrap();
        assert_eq!(
            selected(&f.moddir, Some(&config), None).unwrap(),
            ["B", "C"]
        );
        assert_eq!(selected(&f.moddir, None, None).unwrap(), ["A", "B", "C"]);
    }

    #[test]
    fn invalid_profile_fails() {
        let f = Fixture::new();
        f.mod_file("A", b"mod", "a.wad");
        let config = f.root.join("mods.json");
        fs::write(&config, r#"{"disabled":"A"}"#).unwrap();
        let error = selected(&f.moddir, Some(&config), None).unwrap_err();
        assert_eq!(
            error.to_string(),
            "Disabled mods must be a list of folder names"
        );
        fs::write(&config, r#"{"order":[1]}"#).unwrap();
        let error = selected(&f.moddir, Some(&config), None).unwrap_err();
        assert_eq!(
            error.to_string(),
            "Mod order must be a list of folder names"
        );
        // An unreadable selection gives no patch files (the overlay reports the error).
        f.patch_mod("P", &["mine.xml"], None);
        assert!(enabled_patch_files(&f.moddir, &config, true, None).is_empty());
    }

    #[test]
    fn a_mod_with_only_patches_is_a_mod_and_replaces_no_file() {
        let f = Fixture::new();
        let folder = f.patch_mod("Only patches", &["mine.xml"], None);
        f.mod_file("Files", b"mod", "a.wad");
        assert_eq!(discover(&f.moddir, None), ["Files", "Only patches"]);
        assert!(mod_files(&folder, None).unwrap().is_empty());
        let result = build_overlay(&f.game, &f.root.join("out"), &[folder]).unwrap();
        assert_eq!(result.root, f.game); // nothing to link
        assert!(!result.owned);
    }

    #[test]
    fn enabled_patch_files_follow_the_selection() {
        let f = Fixture::new();
        f.patch_mod("A", &["mine.xml"], None);
        f.patch_mod("B", &["b.xml"], None);
        f.mod_file("Files only", b"mod", "a.wad");
        let config = f.root.join("mods.json");
        fs::write(&config, r#"{"order": ["B", "A"], "disabled": []}"#).unwrap();
        let names = |enabled: bool| -> Vec<String> {
            enabled_patch_files(&f.moddir, &config, enabled, None)
                .iter()
                .map(|file| {
                    let owner = file.parent().and_then(Path::parent).unwrap();
                    format!("{}/{}", name_of(owner), name_of(file))
                })
                .collect()
        };
        assert_eq!(names(true), ["B/b.xml", "A/mine.xml"]); // load order
        fs::write(&config, r#"{"order": ["B", "A"], "disabled": ["B"]}"#).unwrap();
        assert_eq!(names(true), ["A/mine.xml"]);
        assert!(names(false).is_empty()); // "Load mods" off
    }

    /// `test_shadps4_sibling_folder_and_global_disable` (`mods.py`'s `main`): the shadPS4 folder
    /// beside the game is a layer while mods are on, and nothing is laid over the game when off.
    #[test]
    fn shadps4_sibling_folder_and_global_disable() {
        let f = Fixture::new();
        let legacy = PathBuf::from(format!("{}-mods", f.game.display()));
        write(&legacy.join("dvdroot_ps4/data/a.wad"), b"legacy");
        for (enabled, expected) in [(true, &b"legacy"[..]), (false, b"original")] {
            let layers = overlay_layers(&f.game, &f.moddir, None, enabled).unwrap();
            let overlay = build_overlay(&f.game, &f.root.join("out"), &layers).unwrap();
            let content = fs::read(overlay.root.join("dvdroot_ps4/data/a.wad")).unwrap();
            assert_eq!(content, expected, "enabled {enabled}");
            overlay.remove().unwrap();
        }
    }

    /// A mods folder that is itself a game overlay (its own `dvdroot_ps4`) is the one layer; else
    /// the selection of `mods.json`, whose errors stop the start.
    #[test]
    fn a_loose_overlay_or_the_selection_are_the_layers() {
        let f = Fixture::new();
        write(&f.moddir.join("dvdroot_ps4/data/a.wad"), b"loose");
        let layers = overlay_layers(&f.game, &f.moddir, None, true).unwrap();
        assert_eq!(layers, std::slice::from_ref(&f.moddir));
        let f = Fixture::new();
        f.mod_file("B", b"b", "a.wad");
        f.mod_file("A", b"a", "a.wad");
        let config = f.root.join("mods.json");
        fs::write(&config, r#"{"order": ["B", "A"], "disabled": []}"#).unwrap();
        let layers = overlay_layers(&f.game, &f.moddir, Some(&config), true).unwrap();
        assert_eq!(layers, [f.moddir.join("B"), f.moddir.join("A")]);
        fs::write(&config, "{").unwrap();
        assert!(overlay_layers(&f.game, &f.moddir, Some(&config), true).is_err());
        // Mods off: the selection is not read.
        assert_eq!(
            overlay_layers(&f.game, &f.moddir, Some(&config), false).unwrap(),
            Vec::<PathBuf>::new()
        );
    }

    #[test]
    fn merge_replacement_new_file_and_directory_listing() {
        for (way, allowed) in WAYS {
            let f = Fixture::new();
            let a = f.mod_file("A", b"mod", "a.wad");
            f.mod_file("A", b"new", "new.wad");
            let result = f.overlay(&[a], allowed).unwrap();
            let folder = result.root.join("dvdroot_ps4/data");
            let game_data = f.game.join("dvdroot_ps4/data");
            assert_eq!(fs::read(folder.join("a.wad")).unwrap(), b"mod", "{way}");
            assert_eq!(
                fs::read(folder.join("b.wad")).unwrap(),
                b"untouched",
                "{way}"
            );
            assert_eq!(fs::read(folder.join("new.wad")).unwrap(), b"new", "{way}");
            assert_eq!(listing(&folder), ["a.wad", "b.wad", "new.wad"], "{way}");
            assert_eq!(
                fs::read(game_data.join("a.wad")).unwrap(),
                b"original",
                "{way}"
            );
            assert!(!game_data.join("new.wad").exists(), "{way}");
            assert_eq!(
                fs::read(result.root.join("eboot.bin")).unwrap(),
                b"original executable",
                "{way}"
            );
            // Removing the overlay leaves the game and the mod whole.
            let root = result.root.clone();
            result.remove().unwrap();
            assert!(fs::symlink_metadata(&root).is_err(), "{way}");
            assert_eq!(listing(&game_data), ["a.wad", "b.wad"], "{way}");
            assert_eq!(
                fs::read(game_data.join("b.wad")).unwrap(),
                b"untouched",
                "{way}"
            );
            let mod_data = f.moddir.join("A/dvdroot_ps4/data");
            assert_eq!(fs::read(mod_data.join("a.wad")).unwrap(), b"mod", "{way}");
        }
    }

    #[test]
    fn a_network_share_without_symlinks_or_junctions_still_overlays() {
        // A network share (SMB) refuses symlinks and junctions alike; only hard links work. The
        // overlay must still merge the mod and leave the game folder untouched.
        let f = Fixture::new();
        let a = f.mod_file("A", b"mod", "a.wad");
        f.mod_file("A", b"new", "new.wad");
        let share = |kind| matches!(kind, LinkKind::HardLink | LinkKind::Mirror);
        let result = f.overlay(&[a], share).unwrap();
        let folder = result.root.join("dvdroot_ps4/data");
        assert_eq!(fs::read(folder.join("a.wad")).unwrap(), b"mod");
        assert_eq!(fs::read(folder.join("b.wad")).unwrap(), b"untouched");
        assert_eq!(fs::read(folder.join("new.wad")).unwrap(), b"new");
        assert_eq!(
            fs::read(f.game.join("dvdroot_ps4/data/a.wad")).unwrap(),
            b"original"
        );
        // Real folders: the game's files are reached through hard links.
        assert!(!is_link(&result.root.join("dvdroot_ps4")));
        assert!(!is_link(&folder));
    }

    #[test]
    fn last_mod_wins_and_reordering_changes_winner() {
        for (way, allowed) in WAYS {
            let f = Fixture::new();
            let a = f.mod_file("A", b"A", "a.wad");
            let b = f.mod_file("B", b"B", "a.wad");
            for (layers, winner) in [([&a, &b], b"B"), ([&b, &a], b"A")] {
                let layers: Vec<PathBuf> = layers.into_iter().cloned().collect();
                let result = f.overlay(&layers, allowed).unwrap();
                let file = result.root.join("dvdroot_ps4/data/a.wad");
                assert_eq!(fs::read(file).unwrap(), winner, "{way}");
            }
        }
    }

    #[test]
    fn no_mod_returns_original_game() {
        let f = Fixture::new();
        let result = build_overlay(&f.game, &f.root.join("out"), &[]).unwrap();
        assert_eq!(result.root, f.game);
        assert!(!f.root.join("out").exists());
        result.remove().unwrap(); // the game folder itself is never removed
        assert!(f.game.join("eboot.bin").is_file());
    }

    #[cfg(windows)]
    #[test]
    fn verbatim_paths_lose_their_prefix_as_python_resolve_does() {
        // canonicalize gives the \\?\ forms, Python's resolve() the plain ones. The probe opens
        // "<app0>/..." paths, and Windows does not normalize a / under \\?\.
        for (verbatim, plain) in [
            (r"\\?\C:\Games\CUSA01623", r"C:\Games\CUSA01623"),
            (
                r"\\?\UNC\server\share\Games\CUSA01623",
                r"\\server\share\Games\CUSA01623",
            ),
            (r"\\?\UNC\server\share", r"\\server\share"),
        ] {
            let stripped = strip_verbatim(PathBuf::from(verbatim));
            assert_eq!(stripped.as_os_str(), plain, "{verbatim}");
        }
    }

    /// `path`, on a local drive, through this machine's admin share (`\\localhost\C$\...`): a
    /// network share without a network. `None` where the share is not served.
    #[cfg(windows)]
    fn through_admin_share(path: &Path) -> Option<PathBuf> {
        let (drive, rest) = path.to_str()?.split_once(r":\")?;
        let shared = PathBuf::from(format!(r"\\localhost\{drive}$\{rest}"));
        shared.exists().then_some(shared)
    }

    #[cfg(windows)]
    #[test]
    fn a_game_on_a_share_is_overlaid_beside_it_without_the_verbatim_prefix() {
        let f = Fixture::new();
        let Some(game) = through_admin_share(&f.game) else {
            eprintln!(r"skipped: \\localhost\ is not reachable");
            return;
        };
        let lower = |path: &Path| path.to_string_lossy().to_lowercase();
        // No mod: the game folder itself, as Python's resolve() spells it.
        let plain = build_overlay(&game, &f.root.join("out"), &[]).unwrap();
        assert_eq!(lower(&plain.root), lower(&game));
        // Hard links only (a share refuses symlinks) and an out folder on another volume than the
        // share: the overlay goes beside the game.
        let a = f.mod_file("A", b"mod", "a.wad");
        let share = |kind| matches!(kind, LinkKind::HardLink | LinkKind::Mirror);
        let result = build_overlay_with(&game, &f.root.join("out"), &[a], &share).unwrap();
        let beside = result.root.parent().unwrap();
        assert_eq!(lower(beside), lower(&through_admin_share(&f.root).unwrap()));
        assert!(name_of(&result.root).starts_with(OVERLAY_PREFIX));
        let folder = result.root.join("dvdroot_ps4/data");
        assert_eq!(fs::read(folder.join("a.wad")).unwrap(), b"mod");
        assert_eq!(fs::read(folder.join("b.wad")).unwrap(), b"untouched");
        let root = result.root.clone();
        result.remove().unwrap();
        assert!(!root.exists());
        assert_eq!(
            fs::read(f.game.join("dvdroot_ps4/data/a.wad")).unwrap(),
            b"original"
        );
    }

    #[test]
    fn directory_conflict_does_not_modify_base() {
        for (way, allowed) in WAYS {
            let f = Fixture::new();
            let a = f.mod_file("A", b"mod", "a.wad");
            fs::remove_file(a.join("dvdroot_ps4/data/a.wad")).unwrap();
            write(&a.join("dvdroot_ps4/data/b.wad/child"), b"bad");
            let error = f.overlay(&[a], allowed).unwrap_err();
            assert_eq!(
                error.to_string(),
                "File/directory conflict at b.wad",
                "{way}"
            );
            assert_eq!(
                fs::read(f.game.join("dvdroot_ps4/data/b.wad")).unwrap(),
                b"untouched",
                "{way}"
            );
            assert!(listing(&f.root.join("out")).is_empty(), "{way}");
        }
    }

    #[test]
    fn other_case_replaces_the_game_file() {
        for (way, allowed) in WAYS {
            let f = Fixture::new();
            let root = f.moddir.join("Upper");
            write(&root.join("DVDROOT_PS4/Data/A.WAD"), b"upper");
            let result = f.overlay(&[root], allowed).unwrap();
            let folder = result.root.join("dvdroot_ps4/data");
            assert_eq!(fs::read(folder.join("a.wad")).unwrap(), b"upper", "{way}");
            assert_eq!(listing(&folder), ["a.wad", "b.wad"], "{way}");
            assert_eq!(listing(&result.root), ["dvdroot_ps4", "eboot.bin"], "{way}");
        }
    }

    #[test]
    fn a_stale_overlay_is_removed_before_a_new_one() {
        let f = Fixture::new();
        let out = f.root.join("out");
        let kept = f.root.join("kept");
        write(&kept.join("keep.txt"), b"keep");
        write(&out.join("boot.bin"), b"derived"); // not an overlay: stays
        let stale = out.join("mod-game-old");
        write(&stale.join("eboot.bin"), b"left by a crashed run");
        // The link a crashed run left: a junction on Windows (no privilege needed), a symlink
        // elsewhere. Removing the overlay must not go through it.
        #[cfg(windows)]
        crate::core::platform::create_junction(&stale.join("EXEC"), &kept).unwrap();
        #[cfg(not(windows))]
        std::os::unix::fs::symlink(&kept, stale.join("EXEC")).unwrap();
        assert!(is_link(&stale.join("EXEC")));
        let a = f.mod_file("A", b"mod", "a.wad");
        let overlay = build_overlay(&f.game, &out, &[a]).unwrap();
        assert!(fs::symlink_metadata(&stale).is_err());
        assert_eq!(fs::read(kept.join("keep.txt")).unwrap(), b"keep");
        assert_eq!(listing(&kept), ["keep.txt"]);
        let made = name_of(&overlay.root);
        assert!(made.starts_with("mod-game-"), "{made}");
        assert_eq!(listing(&out), ["boot.bin".to_string(), made]);
        assert_eq!(
            fs::read(overlay.root.join("dvdroot_ps4/data/a.wad")).unwrap(),
            b"mod"
        );
    }

    #[test]
    fn each_way_of_linking_is_tried_in_turn() {
        let f = Fixture::new();
        let folder = f.game.join("dvdroot_ps4");
        let file = f.game.join("eboot.bin");
        let out = f.root.join("links");
        fs::create_dir(&out).unwrap();
        let kinds = |way: &str, allowed: fn(LinkKind) -> bool| -> (LinkKind, LinkKind) {
            let mut linker = Linker::new(&allowed);
            let (to_folder, to_file) = (out.join(format!("{way} folder")), out.join(way));
            let made = (
                linker.make(&to_folder, &folder).unwrap(),
                linker.make(&to_file, &file).unwrap(),
            );
            // The game is reached through the links whatever they are.
            let inside = fs::read(to_folder.join("data/a.wad")).unwrap();
            assert_eq!(inside, b"original", "{way}");
            let through = fs::read(&to_file).unwrap();
            assert_eq!(through, b"original executable", "{way}");
            // A refused way is remembered: a junction exists only on Windows.
            let refused = linker.refused.contains(&LinkKind::Junction);
            assert_eq!(
                refused,
                made.0 == LinkKind::Mirror && allowed(LinkKind::Junction)
            );
            made
        };
        // Symlinks need a privilege on Windows (Developer Mode): without it, a junction and a hard
        // link.
        let any = kinds("any", |_| true);
        if symlinks_work(&out) {
            assert_eq!(any, (LinkKind::Symlink, LinkKind::Symlink));
        } else {
            assert_eq!(any, (LinkKind::Junction, LinkKind::HardLink));
        }
        let folder_kind = if cfg!(windows) {
            LinkKind::Junction
        } else {
            LinkKind::Mirror
        };
        assert_eq!(
            kinds("no symlinks", |kind| kind != LinkKind::Symlink),
            (folder_kind, LinkKind::HardLink)
        );
        assert_eq!(
            kinds("share", |kind| matches!(
                kind,
                LinkKind::HardLink | LinkKind::Mirror
            )),
            (LinkKind::Mirror, LinkKind::HardLink)
        );
        assert_eq!(
            kinds("copies", |kind| kind == LinkKind::Mirror),
            (LinkKind::Mirror, LinkKind::Mirror)
        );
        let nothing: fn(LinkKind) -> bool = |_| false;
        assert!(Linker::new(&nothing)
            .make(&out.join("none"), &file)
            .is_err());
        assert!(fs::symlink_metadata(out.join("none")).is_err());
        // Removing the links never goes into the game.
        remove_tree(&out).unwrap();
        assert_eq!(
            listing(&f.game.join("dvdroot_ps4/data")),
            ["a.wad", "b.wad"]
        );
        assert_eq!(fs::read(&file).unwrap(), b"original executable");
    }

    #[test]
    fn a_hostile_game_folder_gives_errors_not_panics() {
        for (way, allowed) in WAYS {
            let f = Fixture::new();
            let data = f.game.join("dvdroot_ps4/data");
            let gone = f.root.join("gone");
            fs::create_dir(&gone).unwrap();
            // A folder that leads back to its parent, and one whose target is missing.
            #[cfg(windows)]
            {
                use crate::core::platform::create_junction;
                create_junction(&data.join("loop"), &f.game.join("dvdroot_ps4")).unwrap();
                create_junction(&data.join("missing"), &gone).unwrap();
            }
            #[cfg(not(windows))]
            {
                std::os::unix::fs::symlink(f.game.join("dvdroot_ps4"), data.join("loop")).unwrap();
                std::os::unix::fs::symlink(&gone, data.join("missing")).unwrap();
            }
            fs::remove_dir(&gone).unwrap();
            let a = f.mod_file("A", b"mod", "a.wad");
            if let Ok(result) = f.overlay(&[a], allowed) {
                result.remove().unwrap();
            }
            assert_eq!(fs::read(data.join("a.wad")).unwrap(), b"original", "{way}");
            assert_eq!(fs::read(data.join("b.wad")).unwrap(), b"untouched", "{way}");
            assert!(
                is_link(&data.join("loop")) && is_link(&data.join("missing")),
                "{way}"
            );
        }
    }

    #[test]
    fn patch_files_of_a_mod_and_its_wrapper_folder() {
        let f = Fixture::new();
        f.patch_mod("A", &["b.xml", "a.xml", "notes.txt"], None);
        f.patch_mod("Wrapped", &["mine.xml"], Some("Wrapped v1"));
        assert_eq!(names(&patch_files(&f.moddir.join("A"))), ["a.xml", "b.xml"]);
        assert_eq!(names(&patch_files(&f.moddir.join("Wrapped"))), ["mine.xml"]);
        assert!(patch_files(&f.root.join("missing")).is_empty());
    }

    #[test]
    fn a_zip_in_the_mods_folder_is_unpacked_and_is_a_mod() {
        let f = Fixture::new();
        f.make_zip(
            "Ultra",
            &[("patches/u.xml", PATCH.as_bytes()), ("README.md", b"hello")],
        );
        f.make_zip("Assets", &[("dvdroot_ps4/data/a.wad", b"zipped")]);
        assert_eq!(discover(&f.moddir, None), ["Assets", "Ultra"]);
        assert_eq!(names(&patch_files(&f.moddir.join("Ultra"))), ["u.xml"]);
        assert_eq!(
            fs::read(f.moddir.join("Assets/dvdroot_ps4/data/a.wad")).unwrap(),
            b"zipped"
        );
        assert!(f.moddir.join("Ultra").join(ZIP_STAMP).is_file());
        assert!(f.moddir.join("Ultra.zip").is_file()); // the zip stays
    }

    #[test]
    fn a_zip_with_a_folder_around_the_mod() {
        let f = Fixture::new();
        f.make_zip("Wrapped", &[("Wrapped v1/patches/u.xml", PATCH.as_bytes())]);
        assert_eq!(discover(&f.moddir, None), ["Wrapped"]);
        assert_eq!(names(&patch_files(&f.moddir.join("Wrapped"))), ["u.xml"]);
    }

    #[test]
    fn a_changed_zip_is_unpacked_again_without_leftovers() {
        let f = Fixture::new();
        let zip_path = f.make_zip("Ultra", &[("patches/old.xml", PATCH.as_bytes())]);
        discover(&f.moddir, None);
        assert!(f.moddir.join("Ultra/patches/old.xml").is_file());
        fs::remove_file(&zip_path).unwrap();
        f.make_zip(
            "Ultra",
            &[("patches/new.xml", PATCH.as_bytes()), ("extra.txt", b"x")],
        );
        // Another time stamp, whatever the file system's resolution.
        let file = fs::File::options().write(true).open(&zip_path).unwrap();
        file.set_modified(UNIX_EPOCH + std::time::Duration::from_secs(1))
            .unwrap();
        drop(file);
        discover(&f.moddir, None);
        assert_eq!(names(&patch_files(&f.moddir.join("Ultra"))), ["new.xml"]);
        assert!(!f.moddir.join("Ultra/patches/old.xml").exists());
    }

    #[test]
    fn a_folder_the_user_made_is_never_replaced_by_a_zip() {
        let f = Fixture::new();
        f.patch_mod("Ultra", &["mine.xml"], None);
        f.make_zip("Ultra", &[("patches/zipped.xml", PATCH.as_bytes())]);
        discover(&f.moddir, None);
        assert_eq!(names(&patch_files(&f.moddir.join("Ultra"))), ["mine.xml"]);
        assert!(!f.moddir.join("Ultra").join(ZIP_STAMP).exists());
    }

    #[test]
    fn unsafe_zips_are_refused() {
        let f = Fixture::new();
        let outside = f.root.join("evil.txt");
        let patch = ("patches/u.xml", PATCH.as_bytes());
        f.make_zip("Slip", &[("../evil.txt", b"x"), patch]);
        f.make_zip("Absolute", &[("/abs.txt", b"x"), patch]);
        f.make_zip("Drive", &[("C:/drive.txt", b"x"), patch]);
        let link = f.moddir.join("Link.zip");
        let mut archive = zip::ZipWriter::new(fs::File::create(&link).unwrap());
        archive
            .add_symlink(
                "patches/u.xml",
                "target",
                zip::write::SimpleFileOptions::default(),
            )
            .unwrap();
        archive.finish().unwrap();
        assert!(discover(&f.moddir, None).is_empty());
        for name in ["Slip", "Absolute", "Drive", "Link"] {
            assert!(!f.moddir.join(name).exists(), "{name}");
        }
        assert!(!outside.exists());
        let error = zip_members(&mut open(&link)).unwrap_err();
        assert_eq!(
            error.to_string(),
            "links are not allowed in a mod zip: patches/u.xml"
        );
    }

    #[test]
    fn a_zip_that_is_not_a_mod_or_not_a_zip_is_skipped() {
        let f = Fixture::new();
        f.make_zip("Notes", &[("readme.txt", b"nothing to load")]);
        fs::write(f.moddir.join("Broken.zip"), b"not a zip").unwrap();
        assert!(discover(&f.moddir, None).is_empty());
        assert!(!f.moddir.join("Broken").exists());
    }

    #[test]
    fn nested_drive_and_alternate_stream_paths_are_refused() {
        let f = Fixture::new();
        for member in [
            "wrapper/E:/outside.txt",
            "wrapper/E:outside.txt",
            "wrapper\\E:\\outside.txt",
            "patches/u.xml:stream",
        ] {
            let path = f.make_zip(
                "Unsafe",
                &[("patches/u.xml", PATCH.as_bytes()), (member, b"unsafe")],
            );
            let error = zip_members(&mut open(&path)).unwrap_err();
            assert!(
                error.to_string().contains("unsafe path"),
                "{member}: {error}"
            );
        }
    }

    #[test]
    fn windows_normalized_traversal_paths_are_refused() {
        let f = Fixture::new();
        for member in [
            "patches/.. /outside.txt",
            "patches/.../outside.txt",
            "patches/..\\outside.txt",
        ] {
            let path = f.make_zip("Unsafe", &[(member, b"unsafe")]);
            let error = zip_members(&mut open(&path)).unwrap_err();
            assert!(
                error.to_string().contains("unsafe path"),
                "{member}: {error}"
            );
        }
    }

    #[test]
    fn unsafe_zip_is_refused_before_any_member_is_extracted() {
        let f = Fixture::new();
        f.make_zip(
            "Unsafe",
            &[
                ("patches/u.xml", PATCH.as_bytes()),
                ("wrapper/E:/outside.txt", b"unsafe"),
            ],
        );
        assert!(unpack_zips(&f.moddir, None).is_empty());
        assert!(!f.moddir.join("Unsafe").exists());
    }

    #[test]
    fn extraction_is_capped_at_the_declared_size_and_the_total() {
        let data = [7u8; 10];
        let mut out = Vec::new();
        let mut budget = 100;
        copy_capped(&mut io::Cursor::new(data), &mut out, 10, &mut budget).unwrap();
        assert_eq!((out.len(), budget), (10, 90));
        // More bytes than declared.
        let mut out = Vec::new();
        let error = copy_capped(&mut io::Cursor::new(data), &mut out, 4, &mut budget).unwrap_err();
        assert_eq!(
            error.to_string(),
            "a file in the zip is larger than its declared size"
        );
        assert!(out.len() <= 5); // stopped reading right after the cap
                                 // The running total, lowered for the test.
        let mut out = Vec::new();
        let mut budget = 6;
        let error = copy_capped(&mut io::Cursor::new(data), &mut out, 10, &mut budget).unwrap_err();
        assert_eq!(error.to_string(), "the zip unpacks to more than 16 GiB");
        assert!(out.len() <= 7);
    }

    #[test]
    fn a_zip_over_the_limit_is_refused() {
        let f = Fixture::new();
        let path = f.make_zip("Small", &[("patches/u.xml", PATCH.as_bytes())]);
        assert_eq!(zip_members(&mut open(&path)).unwrap().len(), 1);
        assert_eq!(ZIP_LIMIT, 16 * 1024 * 1024 * 1024);
    }

    #[test]
    fn mod_symlinks_rejected() {
        let f = Fixture::new();
        let a = f.mod_file("A", b"mod", "a.wad");
        let link = a.join("dvdroot_ps4/escape");
        let target = f.game.join("dvdroot_ps4/data");
        #[cfg(not(windows))]
        std::os::unix::fs::symlink(&target, &link).unwrap();
        // Without the symlink privilege a junction is the fallback, so it must be rejected just
        // like a symlink.
        #[cfg(windows)]
        if std::os::windows::fs::symlink_dir(&target, &link).is_err() {
            crate::core::platform::create_junction(&link, &target).unwrap();
        }
        let error = mod_files(&a, None).unwrap_err();
        assert!(error.to_string().starts_with("Mod links are unsupported"));
        let error = build_overlay(&f.game, &f.root.join("out"), &[a]).unwrap_err();
        assert!(error.to_string().starts_with("Mod links are unsupported"));
        // A junction is refused on Windows even where symlinks work.
        #[cfg(windows)]
        {
            let b = f.mod_file("B", b"mod", "a.wad");
            crate::core::platform::create_junction(&b.join("dvdroot_ps4/escape"), &target).unwrap();
            let error = build_overlay(&f.game, &f.root.join("out"), &[b]).unwrap_err();
            assert!(error.to_string().starts_with("Mod links are unsupported"));
        }
    }

    #[test]
    fn executable_replacement_rejected() {
        let f = Fixture::new();
        let a = f.mod_file("A", b"mod", "a.wad");
        write(&a.join("eboot.bin"), b"unsupported");
        assert!(mod_files(&a, None).is_err());
        let error = build_overlay(&f.game, &f.root.join("out"), &[a]).unwrap_err();
        assert!(error
            .to_string()
            .starts_with("Executable/system replacement is unsupported"));
        assert_eq!(
            fs::read(f.game.join("eboot.bin")).unwrap(),
            b"original executable"
        );
    }

    #[test]
    fn app0_wrapped_mod_discovered() {
        let f = Fixture::new();
        fs::create_dir_all(f.moddir.join("Wrapped/app0/dvdroot_ps4")).unwrap();
        assert_eq!(discover(&f.moddir, None), ["Wrapped"]);
    }

    #[test]
    fn wrapped_layouts() {
        let f = Fixture::new();
        for (name, path) in [
            ("Archive", "Archive v1.2/dvdroot_ps4/data"),
            ("Nested", "Nested/app0/dvdroot_ps4/data"),
        ] {
            write(
                &f.moddir.join(name).join(path).join("a.wad"),
                name.as_bytes(),
            );
        }
        write(&f.moddir.join("Archive/readme.txt"), b"notes");
        fs::create_dir_all(f.moddir.join("Junk/stuff")).unwrap();
        assert_eq!(discover(&f.moddir, None), ["Archive", "Nested"]);
        for name in ["Archive", "Nested"] {
            let files = mod_files(&f.moddir.join(name), None).unwrap();
            assert_eq!(files.len(), 1, "{name}");
            assert_eq!(as_posix(&files[0].0), "dvdroot_ps4/data/a.wad");
            assert_eq!(fs::read(&files[0].1).unwrap(), name.as_bytes());
        }
    }

    /// The game keeps its data in EXEC and one folder per language, with no dvdroot_ps4.
    struct GameLayout {
        f: Fixture,
    }

    impl GameLayout {
        fn new() -> GameLayout {
            let f = Fixture::new();
            fs::remove_dir_all(f.game.join("dvdroot_ps4")).unwrap();
            for folder in ["EXEC/WAD/PS4_LE", "PORTUGUESE/SOUNDS", "sce_sys"] {
                fs::create_dir_all(f.game.join(folder)).unwrap();
            }
            write(&f.game.join("EXEC/WAD/PS4_LE/R_UI.WAD"), b"original ui");
            write(&f.game.join("PORTUGUESE/SOUNDS/X.XVAG"), b"original voice");
            GameLayout { f }
        }

        fn mod_at(&self, name: &str, path: &str, contents: &[u8]) -> PathBuf {
            write(&self.f.moddir.join(name).join(path), contents);
            self.f.moddir.join(name)
        }

        fn overlay(
            &self,
            names: &[&str],
            allowed: fn(LinkKind) -> bool,
        ) -> Result<Overlay, ModsError> {
            let mods: Vec<PathBuf> = names.iter().map(|n| self.f.moddir.join(n)).collect();
            self.f.overlay(&mods, allowed)
        }

        fn game_paths(&self, name: &str) -> Vec<String> {
            let found = mod_files(&self.f.moddir.join(name), Some(&self.f.game)).unwrap();
            found
                .iter()
                .map(|(relative, _)| as_posix(&game_path(relative, true).unwrap()))
                .collect()
        }
    }

    #[test]
    fn exec_and_language_folders_replace_game_files() {
        let g = GameLayout::new();
        g.mod_at("UI", "exec/wad/ps4_le/r_ui.wad", b"mod ui");
        g.mod_at("UI", "Portuguese/SOUNDS/X.XVAG", b"mod voice");
        g.mod_at("UI", "readme.txt", b"ignored");
        assert_eq!(discover(&g.f.moddir, Some(&g.f.game)), ["UI"]);
        assert_eq!(
            g.game_paths("UI"),
            ["Portuguese/SOUNDS/X.XVAG", "exec/wad/ps4_le/r_ui.wad"]
        );
        assert_eq!(
            fs::read(g.f.game.join("EXEC/WAD/PS4_LE/R_UI.WAD")).unwrap(),
            b"original ui"
        );
        for (way, allowed) in WAYS {
            let view = g.overlay(&["UI"], allowed).unwrap().root;
            let ui = fs::read(view.join("EXEC/WAD/PS4_LE/R_UI.WAD")).unwrap();
            assert_eq!(ui, b"mod ui", "{way}");
            let voice = fs::read(view.join("PORTUGUESE/SOUNDS/X.XVAG")).unwrap();
            assert_eq!(voice, b"mod voice", "{way}");
            assert!(!view.join("readme.txt").exists(), "{way}");
            let original = fs::read(g.f.game.join("EXEC/WAD/PS4_LE/R_UI.WAD")).unwrap();
            assert_eq!(original, b"original ui", "{way}");
        }
    }

    #[test]
    fn region_folders_are_used_for_discovery_and_overlay() {
        for (way, allowed) in WAYS {
            let g = GameLayout::new();
            let original = g.f.game.join("JAPANESE/text.wad");
            write(&original, b"original");
            g.mod_at("JP", "Pack/app0/japanese/text.wad", b"translated");
            assert_eq!(discover(&g.f.moddir, Some(&g.f.game)), ["JP"], "{way}");
            let view = g.overlay(&["JP"], allowed).unwrap().root;
            let text = fs::read(view.join("JAPANESE/text.wad")).unwrap();
            assert_eq!(text, b"translated", "{way}");
            assert_eq!(fs::read(&original).unwrap(), b"original", "{way}");
        }
    }

    #[test]
    fn learning_game_folders_does_not_allow_system_replacements() {
        let g = GameLayout::new();
        for folder in ["sce_sys", "sce_module", "eboot.bin"] {
            let bad = g.mod_at("Bad", &format!("{folder}/payload"), b"mod");
            assert!(
                discover(&g.f.moddir, Some(&g.f.game)).is_empty(),
                "{folder}"
            );
            let out = g.f.root.join("out");
            assert!(
                build_overlay(&g.f.game, &out, std::slice::from_ref(&bad)).is_err(),
                "{folder}"
            );
            fs::remove_dir_all(&bad).unwrap();
        }
        assert_eq!(
            fs::read(g.f.game.join("eboot.bin")).unwrap(),
            b"original executable"
        );
    }

    #[test]
    fn dvdroot_prefix_is_dropped_when_the_game_has_none() {
        let g = GameLayout::new();
        g.mod_at(
            "Voice",
            "dvdroot_ps4/PORTUGUESE/SOUNDS/X.XVAG",
            b"mod voice",
        );
        assert_eq!(g.game_paths("Voice"), ["PORTUGUESE/SOUNDS/X.XVAG"]);
        let kept = game_path(Path::new("dvdroot_ps4/PORTUGUESE/X.XVAG"), false).unwrap();
        assert_eq!(as_posix(&kept), "dvdroot_ps4/PORTUGUESE/X.XVAG");
    }

    #[test]
    fn wrapper_folder_around_exec() {
        let g = GameLayout::new();
        g.mod_at("Pack", "Pack v1.0/EXEC/WAD/PS4_LE/R_UI.WAD", b"wrapped");
        assert_eq!(discover(&g.f.moddir, Some(&g.f.game)), ["Pack"]);
        assert_eq!(g.game_paths("Pack"), ["EXEC/WAD/PS4_LE/R_UI.WAD"]);
    }

    #[test]
    fn system_files_are_refused_with_or_without_the_prefix() {
        let g = GameLayout::new();
        g.mod_at("Bad", "EXEC/WAD/PS4_LE/R_UI.WAD", b"mod");
        g.mod_at("Bad", "sce_sys/param.sfo", b"mod");
        g.mod_at("Sneaky", "dvdroot_ps4/eboot.bin", b"mod");
        assert!(mod_files(&g.f.moddir.join("Bad"), Some(&g.f.game)).is_err());
        let sneaky = mod_files(&g.f.moddir.join("Sneaky"), Some(&g.f.game)).unwrap();
        let error = game_path(&sneaky[0].0, true).unwrap_err();
        assert!(error
            .to_string()
            .starts_with("Executable/system replacement is unsupported: eboot.bin"));
        assert_eq!(
            fs::read(g.f.game.join("eboot.bin")).unwrap(),
            b"original executable"
        );
    }

    #[test]
    fn conflicts_match_across_layouts() {
        let g = GameLayout::new();
        g.mod_at("A", "EXEC/WAD/PS4_LE/R_UI.WAD", b"mod");
        g.mod_at("B", "dvdroot_ps4/EXEC/WAD/PS4_LE/R_UI.WAD", b"mod");
        let names = ["A".to_string(), "B".to_string()];
        assert_eq!(
            conflicts(&g.f.moddir, &names, None),
            [(
                "EXEC/WAD/PS4_LE/R_UI.WAD".to_string(),
                vec!["A".to_string(), "B".to_string()]
            )]
        );
    }

    #[test]
    fn region_folders_do_not_leak_to_another_game() {
        let g = GameLayout::new();
        fs::create_dir_all(g.f.game.join("JAPANESE")).unwrap();
        g.mod_at("JP", "JAPANESE/text.wad", b"mod");
        assert_eq!(discover(&g.f.moddir, Some(&g.f.game)), ["JP"]);
        let other = g.f.root.join("other");
        fs::create_dir_all(&other).unwrap();
        assert!(discover(&g.f.moddir, Some(&other)).is_empty());
        assert!(discover(&g.f.moddir, None).is_empty());
    }

    /// `ModInfo.setUp`: a mod called "Gold Armor" with one EXEC file.
    fn gold_armor() -> (tempfile::TempDir, PathBuf) {
        let temp = tempfile::tempdir().unwrap();
        let folder = temp.path().join("Gold Armor");
        write(&folder.join("EXEC/R_HERO00.WAD"), b"x");
        (temp, folder)
    }

    #[test]
    fn mod_json_fields() {
        let (_temp, folder) = gold_armor();
        let fields = json!({"name": "Kratos Gold Armor", "author": "someone", "version": "1.2",
            "description": "Golden armor.", "url": "https://example.com/mod"});
        fs::write(folder.join("mod.json"), fields.to_string()).unwrap();
        let info = mod_info(&folder, None);
        assert_eq!(
            (
                info.name,
                info.author,
                info.version,
                info.description,
                info.url
            ),
            (
                "Kratos Gold Armor".to_string(),
                "someone".to_string(),
                "1.2".to_string(),
                "Golden armor.".to_string(),
                "https://example.com/mod".to_string()
            )
        );
        assert_eq!(info.image, None);
    }

    #[test]
    fn without_mod_json_the_folder_name_and_readme_are_used() {
        let (_temp, folder) = gold_armor();
        let text = format!("\n  First line.\nSecond line.\n\n{}", "x".repeat(900));
        fs::write(folder.join("Readme.txt"), text).unwrap();
        let info = mod_info(&folder, None);
        assert_eq!(info.name, "Gold Armor");
        assert!(info.description.starts_with("First line.\nSecond line."));
        assert!(info.description.chars().count() <= 400);
        assert_eq!(
            (info.author, info.version, info.url),
            (String::new(), String::new(), String::new())
        );
    }

    #[test]
    fn invalid_mod_json_falls_back() {
        let (_temp, folder) = gold_armor();
        fs::write(folder.join("mod.json"), "{not json").unwrap();
        assert_eq!(mod_info(&folder, None).name, "Gold Armor");
        fs::write(folder.join("mod.json"), r#"["a list"]"#).unwrap();
        assert_eq!(mod_info(&folder, None).name, "Gold Armor");
    }

    #[test]
    fn only_web_links_are_kept() {
        let (_temp, folder) = gold_armor();
        fs::write(
            folder.join("mod.json"),
            json!({"url": "file:///C:/Windows/system32"}).to_string(),
        )
        .unwrap();
        assert_eq!(mod_info(&folder, None).url, "");
    }

    #[test]
    fn preview_image_wins_over_other_images() {
        let (_temp, folder) = gold_armor();
        for name in ["a.jpg", "screenshot.png", "Preview.PNG"] {
            fs::write(folder.join(name), b"img").unwrap();
        }
        let image = |folder: &Path| name_of(&mod_info(folder, None).image.unwrap());
        assert_eq!(image(&folder), "Preview.PNG");
        fs::remove_file(folder.join("Preview.PNG")).unwrap();
        assert_eq!(image(&folder), "screenshot.png");
        fs::remove_file(folder.join("screenshot.png")).unwrap();
        assert_eq!(image(&folder), "a.jpg");
    }

    #[test]
    fn metadata_inside_a_wrapper_folder() {
        let (temp, _folder) = gold_armor();
        let wrapper = temp.path().join("Pack");
        write(&wrapper.join("Pack v1/EXEC/R_UI.WAD"), b"x");
        write(
            &wrapper.join("Pack v1/mod.json"),
            json!({"name": "Wrapped"}).to_string().as_bytes(),
        );
        write(&wrapper.join("Pack v1/cover.jpg"), b"img");
        assert_eq!(discover(temp.path(), None), ["Gold Armor", "Pack"]);
        let info = mod_info(&wrapper, None);
        assert_eq!(
            (info.name, name_of(&info.image.unwrap())),
            ("Wrapped".to_string(), "cover.jpg".to_string())
        );
    }

    #[test]
    fn mod_json_beside_the_wrapper_folder_keeps_the_mod() {
        let (temp, _folder) = gold_armor();
        let wrapper = temp.path().join("Outer");
        write(&wrapper.join("Outer v2/EXEC/R_UI.WAD"), b"x");
        write(
            &wrapper.join("mod.json"),
            json!({"name": "Outer Mod"}).to_string().as_bytes(),
        );
        assert!(discover(temp.path(), None).contains(&"Outer".to_string()));
        assert_eq!(mod_info(&wrapper, None).name, "Outer Mod");
    }

    // ---- tests/test_mod_conflicts.py ----

    fn conflict_mod(root: &Path, name: &str, files: &[&str]) {
        for file in files {
            write(&root.join(name).join("dvdroot_ps4").join(file), b"x");
        }
    }

    fn strings(names: &[&str]) -> Vec<String> {
        names.iter().map(|n| (*n).to_string()).collect()
    }

    #[test]
    fn last_in_order_wins() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        conflict_mod(root, "a", &["chr/kratos.wad", "ui/hud.wad"]);
        conflict_mod(root, "b", &["chr/kratos.wad"]);
        conflict_mod(root, "c", &["CHR/Kratos.wad", "music.wad"]);
        let found = conflicts(root, &strings(&["a", "b", "c"]), None);
        assert_eq!(found.len(), 1);
        let (path, owners) = &found[0];
        assert_eq!(path.to_lowercase(), "chr/kratos.wad"); // this game has no dvdroot_ps4
        assert_eq!(owners, &strings(&["a", "b", "c"])); // c, the last one, wins
    }

    #[test]
    fn only_given_mods_count() {
        let temp = tempfile::tempdir().unwrap();
        conflict_mod(temp.path(), "a", &["x.wad"]);
        conflict_mod(temp.path(), "b", &["x.wad"]);
        assert!(conflicts(temp.path(), &strings(&["a"]), None).is_empty());
    }

    #[test]
    fn no_conflict_and_broken_mod() {
        let temp = tempfile::tempdir().unwrap();
        conflict_mod(temp.path(), "a", &["x.wad"]);
        conflict_mod(temp.path(), "b", &["y.wad"]);
        fs::create_dir(temp.path().join("broken")).unwrap();
        let names = strings(&["a", "b", "broken", "missing"]);
        assert!(conflicts(temp.path(), &names, None).is_empty());
    }

    #[test]
    fn preset_skips_missing_and_appends_new() {
        let preset = json!({"order": ["b", "gone", "a"], "disabled": ["a", "gone"]});
        let (order, disabled) = apply_preset(&preset, &strings(&["a", "b", "c"]));
        assert_eq!(order, ["b", "a", "c"]);
        assert_eq!(disabled, ["a"]);
    }

    #[test]
    fn preset_with_bad_shape() {
        let preset = json!({"order": "nope", "disabled": null});
        assert_eq!(
            apply_preset(&preset, &strings(&["a"])),
            (strings(&["a"]), Vec::new())
        );
    }
}
