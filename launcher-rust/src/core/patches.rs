//! Patches of the game's eboot: the built-in XML's metadata, the selection rules, third-party
//! patch files and the report of the last start; and the compiler (Python `scripts/patches.py`),
//! which finds each patch site by its signature in any build and writes `patches.bin`
//! (`G3PATCH2`), `patch_status.json` and `patches-merged.xml` as the Python does.

use std::collections::{HashMap, HashSet};
use std::io;
use std::ops::Range;
use std::path::{Path, PathBuf};

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use super::fs::atomic_write;
use super::game::{game_profile, GameProfile};
use super::paths::Paths;
use super::sfo::{read_sfo, SfoValue};

/// Resolution patches share the video arena with the texture fix: only one of them is on.
pub const EXCLUSIVE_PREFIX: &str = "Resolution Patch";
/// Fixes with nothing to choose: not offered in the launcher and always applied.
pub const ALWAYS_ON: &[&str] = &["Bug Fix - Startup Black Screen"];
/// An older build's patch file, left in `patches/` by an update.
const LEGACY_PATCH_FILE: &str = "God_of_War_III_Remastered.xml";
const FRAME_RATE_PREFIX: &str = "Frame Rate Patch";

#[derive(Debug, thiserror::Error)]
pub enum PatchError {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error("{0}")]
    Xml(String),
    /// A patch file, selection or eboot the compiler cannot use: where the Python stops, with its
    /// text where it has one.
    #[error("{0}")]
    Invalid(String),
    /// A site whose signature is not found once: the patch is left out with this reason.
    #[error("{0}")]
    Lookup(String),
}

/// One `Metadata` element of a patch file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatchMeta {
    pub name: String,
    pub author: Option<String>,
    pub note: Option<String>,
    pub enabled_by_default: bool,
}

/// The report `patches.py` writes after a start (`patch_status.json`), in its key order. The
/// Python writes only the first three keys; the last three are the launcher's, written only when
/// set, and a file without them still reads.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PatchStatus {
    /// SHA-256 of the game's `eboot.bin` (`null` when it could not be read).
    #[serde(default)]
    pub sha256: Option<String>,
    #[serde(default)]
    pub applied: Vec<String>,
    /// Patch name to the reason it was left out.
    #[serde(default)]
    pub skipped: IndexMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine_fps: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dmem_mb: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vblank_hz: Option<u32>,
}

/// The built-in patch file of a game profile.
pub fn builtin_xml(paths: &Paths, profile: &GameProfile) -> PathBuf {
    paths.port_dir.join("patches").join(profile.xml)
}

/// Parses a patch file. One nested deeper than [`MAX_DEPTH`] is refused first: roxmltree's
/// parser recurses once per level and would overflow the stack.
fn parse(text: &str) -> Result<roxmltree::Document<'_>, PatchError> {
    let depth = nesting_bound(text);
    if depth > MAX_DEPTH {
        return Err(PatchError::Xml(format!(
            "elements may nest {depth} deep (at most {MAX_DEPTH})"
        )));
    }
    let options = roxmltree::ParsingOptions {
        allow_dtd: true,
        ..Default::default()
    };
    roxmltree::Document::parse_with_options(text, options)
        .map_err(|error| PatchError::Xml(error.to_string()))
}

/// Deeper than any patch file (they nest four deep) and shallow enough for roxmltree's
/// recursion, about 8 KiB a level in a debug build.
const MAX_DEPTH: usize = 32;

/// An upper bound of the element nesting roxmltree reaches on `text`. Without entity
/// declarations, the depth of the tags outside comments, CDATA sections, processing
/// instructions and quoted attribute values; an entity may expand to markup, so with one every
/// `<` counts.
fn nesting_bound(text: &str) -> usize {
    let bytes = text.as_bytes();
    if text.contains("<!ENTITY") {
        return bytes.iter().filter(|&&b| b == b'<').count();
    }
    // The offset just past the first `end` at or after `from` (the end of the text without one).
    let past = |from: usize, end: &[u8]| {
        bytes
            .get(from..)
            .and_then(|rest| rest.windows(end.len()).position(|w| w == end))
            .map_or(bytes.len(), |at| from + at + end.len())
    };
    let (mut depth, mut deepest, mut at) = (0usize, 0usize, 0usize);
    while let Some(start) = bytes
        .get(at..)
        .and_then(|rest| rest.iter().position(|&b| b == b'<'))
        .map(|i| at + i)
    {
        let tag = bytes.get(start..).unwrap_or_default();
        at = if tag.starts_with(b"<!--") {
            past(start + 4, b"-->")
        } else if tag.starts_with(b"<![CDATA[") {
            past(start + 9, b"]]>")
        } else if tag.starts_with(b"<?") {
            past(start + 2, b"?>")
        } else if tag.starts_with(b"<!") {
            // A declaration: what follows is scanned like content, which only counts more.
            start + 2
        } else if tag.starts_with(b"</") {
            depth = depth.saturating_sub(1);
            start + 2
        } else {
            // A start tag, to its `>` outside quotes; `/>` closes it at once.
            let mut quote = None;
            let mut end = None;
            for (i, &b) in tag.iter().enumerate().skip(1) {
                match quote {
                    Some(q) if b == q => quote = None,
                    Some(_) => {}
                    None if b == b'"' || b == b'\'' => quote = Some(b),
                    None if b == b'>' => {
                        end = Some(i);
                        break;
                    }
                    None => {}
                }
            }
            let empty = end.is_some_and(|i| tag.get(i - 1) == Some(&b'/'));
            if !empty {
                depth += 1;
                deepest = deepest.max(depth);
            }
            end.map_or(bytes.len(), |i| start + i + 1)
        };
    }
    deepest
}

fn non_empty(value: Option<&str>) -> Option<String> {
    value.filter(|v| !v.is_empty()).map(str::to_string)
}

fn meta_of(node: roxmltree::Node) -> PatchMeta {
    PatchMeta {
        name: node.attribute("Name").unwrap_or_default().to_string(),
        author: non_empty(node.attribute("Author")),
        note: non_empty(node.attribute("Note")),
        enabled_by_default: node
            .attribute("isEnabled")
            .is_some_and(|v| v.to_lowercase() == "true"),
    }
}

fn metadata<'a, 'input>(
    doc: &'a roxmltree::Document<'input>,
) -> impl Iterator<Item = roxmltree::Node<'a, 'input>> {
    doc.descendants().filter(|n| n.has_tag_name("Metadata"))
}

/// The patches the XML has for this app version, in file order.
pub fn game_patch_metas(xml: &Path, app_version: &str) -> Result<Vec<PatchMeta>, PatchError> {
    let text = std::fs::read_to_string(xml)?;
    let doc = parse(&text).map_err(|e| PatchError::Xml(e.to_string()))?;
    Ok(metadata(&doc)
        .filter(|n| n.attribute("AppVer") == Some(app_version))
        .map(meta_of)
        .collect())
}

/// The patches the launcher offers: without the resolution and frame rate ones (they follow
/// Performance) and the always-on fixes.
pub fn selectable(metas: &[PatchMeta]) -> Vec<PatchMeta> {
    metas
        .iter()
        .filter(|m| {
            !(m.name.starts_with(EXCLUSIVE_PREFIX)
                || m.name.starts_with(FRAME_RATE_PREFIX)
                || ALWAYS_ON.contains(&m.name.as_str()))
        })
        .cloned()
        .collect()
}

/// Sort key of a size: its pixel count, then the text; anything that is not WxH goes last. A
/// count past `u128` saturates: it still sorts after the names that are not WxH, as Python's
/// unbounded integers sort it (two such sizes then go by their text).
fn pixels(size: &str) -> (u128, String) {
    let (width, height) = size.split_once('x').unwrap_or((size, ""));
    let digits = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit());
    if digits(width) && digits(height) {
        // Only digits: the one way to fail is a number too large.
        let number = |s: &str| s.parse::<u128>().unwrap_or(u128::MAX);
        return (
            number(width).saturating_mul(number(height)),
            size.to_string(),
        );
    }
    (1 << 60, size.to_string())
}

/// Sizes ("3440x1440") of the "Resolution Patch - <size>" entries of these files, smallest first.
pub fn resolution_names(xmls: &[PathBuf]) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    for path in xmls {
        let Ok(text) = std::fs::read_to_string(path) else {
            continue;
        };
        let Ok(doc) = parse(&text) else {
            continue;
        };
        for node in metadata(&doc) {
            let name = node.attribute("Name").unwrap_or_default();
            if let Some(size) = name
                .strip_prefix(EXCLUSIVE_PREFIX)
                .and_then(|rest| rest.strip_prefix(" - "))
            {
                let size = size.trim();
                if !size.is_empty() && !found.iter().any(|f| f == size) {
                    found.push(size.to_string());
                }
            }
        }
    }
    found.sort_by_key(|s| pixels(s));
    found
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

/// `(key, file, metadata)` of the eboot patches for this version in `dir/*.xml` (third-party
/// shadPS4 or GoldHEN files). The key is `"<file name>/<patch name>"`, the launcher's selection
/// in `patches.json`. The built-in files are left out, and so are files whose `ID`s do not
/// include any of `ids`.
pub fn external_patches(
    dir: &Path,
    app_version: &str,
    exclude: &Path,
    ids: &[&str],
) -> Vec<(String, PathBuf, PatchMeta)> {
    let mut found = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return found;
    };
    let mut paths: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "xml") && p.is_file())
        .collect();
    paths.sort();
    for path in paths {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        // The profile's file (patches/ in a checkout) and an older build's are not external.
        if same_file(&path, exclude) || name == "CUSA01623.xml" || name == LEGACY_PATCH_FILE {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let doc = match parse(&text) {
            Ok(doc) => doc,
            Err(error) => {
                eprintln!("Patches: {name}: {error}");
                continue;
            }
        };
        let file_ids: Vec<&str> = doc
            .descendants()
            .filter(|n| n.has_tag_name("ID"))
            .filter_map(|n| n.text())
            .map(str::trim)
            .collect();
        if !file_ids.is_empty() && !file_ids.iter().any(|id| ids.contains(id)) {
            continue;
        }
        for node in metadata(&doc) {
            if node.attribute("AppVer") == Some(app_version)
                && node.attribute("AppElf").unwrap_or("eboot.bin") == "eboot.bin"
            {
                let meta = meta_of(node);
                found.push((format!("{name}/{}", meta.name), path.clone(), meta));
            }
        }
    }
    found
}

/// The keys of the external patches that are on. `config` is the content of `patches.json`,
/// `{"enabled": [key, ...], "disabled": [key, ...]}`: a key in `enabled` is on, one in `disabled`
/// is off, any other follows the file's `isEnabled`. `enabled` wins when a key is in both.
pub fn external_enabled(found: &[(String, PathBuf, PatchMeta)], config: &Value) -> Vec<String> {
    found
        .iter()
        .filter(|(key, _, meta)| external_on(config, key, meta))
        .map(|(key, _, _)| key.clone())
        .collect()
}

/// Whether the external patch `key` is on under `config` (see [`external_enabled`]).
fn external_on(config: &Value, key: &str, meta: &PatchMeta) -> bool {
    let listed = |field: &str| {
        config
            .get(field)
            .and_then(Value::as_array)
            .is_some_and(|list| list.iter().any(|v| v.as_str() == Some(key)))
    };
    listed("enabled") || (!listed("disabled") && meta.enabled_by_default)
}

/// The last start's patch report in `out_dir`; `None` when missing or not valid.
pub fn patch_status(out_dir: &Path) -> Option<PatchStatus> {
    let bytes = std::fs::read(out_dir.join("patch_status.json")).ok()?;
    serde_json::from_slice(&bytes).ok()
}

// --- The compiler (Python `scripts/patches.py`)

/// The PS4 address of the eboot's virtual address 0: patch addresses are PS4 addresses, image
/// offsets are not.
pub const EBOOT_BASE: u64 = 0x40_0000;
/// SHA-256 of the `eboot.bin` the built-in `Address` values and third-party patch files are
/// written for.
pub const REFERENCE_SHA256: &str =
    "d85c8135d330c3b601bf5dc3f1dd86bd6119fb8a9fce372bed2c9785f9a79299";
/// The retail direct memory pool (`runtime_memory.c` `POOL_SIZE`): a note's "DMEM to N MB" is on
/// top of it.
pub const DMEM_RETAIL_MB: u64 = 5056;
/// The fix a resolution patch replaces: both resize the video arena.
const REPLACED_BY_EXCLUSIVE: &[&str] = &["Bug Fix - Texture Corruption Fix"];
const PATCH_MAGIC: &[u8; 8] = b"G3PATCH2";
const PT_LOAD: u32 = 1;
/// The largest eboot image the compiler builds (the loader's own limit).
const IMAGE_LIMIT: u64 = 512 << 20;
const BASE: i64 = EBOOT_BASE as i64;

/// The virtual address range of an eboot `PT_LOAD` segment, `vaddr..vaddr + memsz` (`.bss`
/// included): every patch write must fit inside one.
pub type Segment = Range<u64>;

/// Image offsets of a patch's named `Target`s. Offsets are signed because a hostile `At`, or an
/// `Address` below [`EBOOT_BASE`], gives a negative one (that no segment holds).
pub type Targets = HashMap<String, i64>;

/// One write of `patches.bin`: `bytes` at image offset `address` (the PS4 address minus
/// [`EBOOT_BASE`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Write {
    pub address: u64,
    pub bytes: Vec<u8>,
}

/// What the selected patches' notes ask the runtime for.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Requirements {
    /// Direct memory in MiB: the retail pool plus the largest "DMEM to N MB" of the notes.
    pub dmem_mb: Option<u64>,
    /// The largest "VBlank to N FPS" of the notes.
    pub vblank_hz: Option<u32>,
}

fn read_text(path: &Path) -> Result<String, PatchError> {
    std::fs::read_to_string(path).map_err(|error| with_path(path, error))
}

fn read_file(path: &Path) -> Result<Vec<u8>, PatchError> {
    std::fs::read(path).map_err(|error| with_path(path, error))
}

fn with_path(path: &Path, error: io::Error) -> PatchError {
    PatchError::Io(io::Error::new(
        error.kind(),
        format!("{}: {error}", path.display()),
    ))
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// `Line (3:5)`: an element and where it starts in its file, for messages.
fn describe(node: roxmltree::Node) -> String {
    let at = node.document().text_pos_at(node.range().start);
    format!("{} ({at})", node.tag_name().name())
}

fn is_enabled(meta: roxmltree::Node) -> bool {
    meta.attribute("isEnabled")
        .is_some_and(|v| v.to_lowercase() == "true")
}

/// `text` as Python's `repr` spells a string.
fn py_repr(text: &str) -> String {
    let quote = if text.contains('\'') && !text.contains('"') {
        '"'
    } else {
        '\''
    };
    let mut out = String::with_capacity(text.len() + 2);
    out.push(quote);
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c == quote => {
                out.push('\\');
                out.push(c);
            }
            c if c < ' ' || c == '\u{7f}' => out.push_str(&format!("\\x{:02x}", u32::from(c))),
            c => out.push(c),
        }
    }
    out.push(quote);
    out
}

/// A list of strings as Python prints it: `['a', 'b']`.
pub(crate) fn py_list<S: AsRef<str>>(items: &[S]) -> String {
    let items: Vec<String> = items.iter().map(|item| py_repr(item.as_ref())).collect();
    format!("[{}]", items.join(", "))
}

/// A number as Python's `{:#x}` spells it.
fn py_hex(value: i128) -> String {
    if value < 0 {
        format!("-{:#x}", value.unsigned_abs())
    } else {
        format!("{value:#x}")
    }
}

/// Python's `int(text, radix)` for radix 0, 10 or 16: surrounding whitespace, a sign, a
/// `0x`/`0o`/`0b` prefix (radix 0 takes the base from it and refuses a decimal with leading
/// zeros) and single underscores between digits. `None` where Python raises.
pub(crate) fn py_int(text: &str, radix: u32) -> Option<i128> {
    let text = text.trim();
    let (negative, unsigned) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text.strip_prefix('+').unwrap_or(text)),
    };
    let prefix = unsigned.get(..2).map(str::to_ascii_lowercase);
    let prefixed = match (radix, prefix.as_deref()) {
        (0 | 16, Some("0x")) => Some(16),
        (0, Some("0o")) => Some(8),
        (0, Some("0b")) => Some(2),
        _ => None,
    };
    let (base, digits) = match prefixed {
        // One underscore may follow the prefix ("0x_ff").
        Some(base) => {
            let rest = unsigned.get(2..)?;
            (base, rest.strip_prefix('_').unwrap_or(rest))
        }
        None => (if radix == 0 { 10 } else { radix }, unsigned),
    };
    if digits.is_empty()
        || digits.starts_with('_')
        || digits.ends_with('_')
        || digits.contains("__")
    {
        return None;
    }
    let digits: String = digits.chars().filter(|&c| c != '_').collect();
    if !digits.chars().all(|c| c.is_digit(base)) {
        return None;
    }
    // int('010', 0) raises: a base-0 decimal has no leading zero unless it is all zeros.
    if radix == 0 && base == 10 && digits.starts_with('0') && digits.chars().any(|c| c != '0') {
        return None;
    }
    let value = i128::from_str_radix(&digits, base).ok()?;
    Some(if negative { -value } else { value })
}

/// Python's `float(text)`: surrounding whitespace, `inf` and `nan`, single underscores between
/// digits.
fn py_float(text: &str) -> Option<f64> {
    let text = text.trim();
    let bytes = text.as_bytes();
    let underscores_between_digits = bytes.iter().enumerate().all(|(i, &b)| {
        b != b'_'
            || (i > 0
                && bytes.get(i - 1).is_some_and(u8::is_ascii_digit)
                && bytes.get(i + 1).is_some_and(u8::is_ascii_digit))
    });
    if !underscores_between_digits {
        return None;
    }
    text.replace('_', "").parse().ok()
}

fn invalid_literal(text: &str, radix: u32) -> PatchError {
    PatchError::Invalid(format!(
        "invalid literal for int() with base {radix}: {}",
        py_repr(text)
    ))
}

/// Attribute `name` of `node` read as Python's `int(value, radix)`; `default` when absent.
fn int_attribute(
    node: roxmltree::Node,
    name: &str,
    default: Option<&str>,
    radix: u32,
) -> Result<i64, PatchError> {
    let text = node
        .attribute(name)
        .or(default)
        .ok_or_else(|| PatchError::Invalid(format!("{}: no {name}", describe(node))))?;
    py_int(text, radix)
        .and_then(|value| i64::try_from(value).ok())
        .ok_or_else(|| invalid_literal(text, radix))
}

fn out_of_range(node: roxmltree::Node) -> PatchError {
    PatchError::Invalid(format!("{}: offset out of range", describe(node)))
}

/// The image offset of an element's `Address` (the reference build's site).
fn reference_site(node: roxmltree::Node) -> Result<i64, PatchError> {
    int_attribute(node, "Address", None, 0)?
        .checked_sub(BASE)
        .ok_or_else(|| out_of_range(node))
}

/// Python's `bytes.fromhex`: pairs of hex digits, ASCII whitespace allowed between pairs.
fn from_hex(text: &str) -> Result<Vec<u8>, PatchError> {
    let bytes = text.as_bytes();
    let digit = |at: usize| bytes.get(at).and_then(|&b| char::from(b).to_digit(16));
    let mut out = Vec::with_capacity(bytes.len() / 2);
    let mut at = 0;
    while let Some(&byte) = bytes.get(at) {
        if matches!(byte, b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c) {
            at += 1;
            continue;
        }
        let (Some(high), Some(low)) = (digit(at), digit(at + 1)) else {
            let position = if digit(at).is_none() { at } else { at + 1 };
            return Err(PatchError::Invalid(format!(
                "non-hexadecimal number found in fromhex() arg at position {position}"
            )));
        };
        // Two hex digits: below 256.
        out.push(((high << 4) | low) as u8);
        at += 2;
    }
    Ok(out)
}

// --- The eboot

fn elf_error(what: impl std::fmt::Display) -> PatchError {
    PatchError::Invalid(format!("eboot.elf: {what}"))
}

/// `elf[at..at + size]`, or the error naming what is missing.
fn elf_bytes(elf: &[u8], at: u64, size: u64) -> Result<&[u8], PatchError> {
    let range = usize::try_from(at).ok().zip(
        at.checked_add(size)
            .and_then(|end| usize::try_from(end).ok()),
    );
    range
        .and_then(|(start, end)| elf.get(start..end))
        .ok_or_else(|| {
            elf_error(format_args!(
                "{size:#x} bytes at {at:#x} are past its end ({:#x} bytes)",
                elf.len()
            ))
        })
}

fn elf_field<const N: usize>(elf: &[u8], at: u64) -> Result<[u8; N], PatchError> {
    elf_bytes(elf, at, N as u64)?
        .try_into()
        .map_err(|_| elf_error(format_args!("no field at {at:#x}")))
}

/// The fields of an ELF program header the compiler reads.
struct ProgramHeader {
    kind: u32,
    offset: u64,
    vaddr: u64,
    filesz: u64,
    memsz: u64,
}

/// The program headers of `eboot.elf`: `e_phnum` records of 56 bytes (`<IIQQQQQQ`), `e_phentsize`
/// apart from `e_phoff`.
fn program_headers(elf: &[u8]) -> Result<Vec<ProgramHeader>, PatchError> {
    let phoff = u64::from_le_bytes(elf_field(elf, 0x20)?);
    let entry_size = u64::from(u16::from_le_bytes(elf_field(elf, 0x36)?));
    let count = u16::from_le_bytes(elf_field(elf, 0x38)?);
    (0..u64::from(count))
        .map(|i| {
            let at = phoff
                .checked_add(i * entry_size)
                .ok_or_else(|| elf_error("program header offset out of range"))?;
            let record = elf_bytes(elf, at, 56)?;
            Ok(ProgramHeader {
                kind: u32::from_le_bytes(elf_field(record, 0)?),
                offset: u64::from_le_bytes(elf_field(record, 8)?),
                vaddr: u64::from_le_bytes(elf_field(record, 16)?),
                filesz: u64::from_le_bytes(elf_field(record, 32)?),
                memsz: u64::from_le_bytes(elf_field(record, 40)?),
            })
        })
        .collect()
}

/// The `PT_LOAD` segments of `eboot.elf`, `vaddr..vaddr + memsz`.
pub fn eboot_segments(elf: &[u8]) -> Result<Vec<Segment>, PatchError> {
    program_headers(elf)?
        .iter()
        .filter(|header| header.kind == PT_LOAD)
        .map(|header| {
            let end = header
                .vaddr
                .checked_add(header.memsz)
                .ok_or_else(|| elf_error("segment end out of range"))?;
            Ok(header.vaddr..end)
        })
        .collect()
}

/// The `PT_LOAD` file bytes of `eboot.elf` at their virtual addresses (image offsets): what the
/// signatures are searched in.
pub fn eboot_image(elf: &[u8]) -> Result<Vec<u8>, PatchError> {
    let loads: Vec<ProgramHeader> = program_headers(elf)?
        .into_iter()
        .filter(|header| header.kind == PT_LOAD)
        .collect();
    let mut size: Option<u64> = None;
    for load in &loads {
        let end = load
            .vaddr
            .checked_add(load.filesz)
            .ok_or_else(|| elf_error("segment end out of range"))?;
        size = Some(size.map_or(end, |size| size.max(end)));
    }
    let size = size.ok_or_else(|| elf_error("no PT_LOAD segment"))?;
    if size > IMAGE_LIMIT {
        return Err(elf_error(format_args!(
            "an image of {size:#x} bytes exceeds the 512 MiB limit"
        )));
    }
    let mut image = vec![0u8; usize::try_from(size).map_err(|_| elf_error("image too large"))?];
    for load in &loads {
        let bytes = elf_bytes(elf, load.offset, load.filesz)?;
        let slot = usize::try_from(load.vaddr)
            .ok()
            .and_then(|start| image.get_mut(start..start.checked_add(bytes.len())?))
            .ok_or_else(|| elf_error("segment outside the image"))?;
        slot.copy_from_slice(bytes);
    }
    Ok(image)
}

// --- Signatures: the bytes around a site in the reference build, "??" where a build may differ
// (rip-relative displacements, rel32 branch targets); tools/make_signatures.py writes them.

/// A parsed `Pattern` attribute, with at least one fixed byte.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pattern {
    bytes: Vec<Option<u8>>,
    /// The longest run of fixed bytes (the first of equal ones): what is searched for.
    anchor: Range<usize>,
}

impl Pattern {
    /// Whether the pattern matches the image at `start`.
    fn matches_at(&self, image: &[u8], start: usize) -> bool {
        start
            .checked_add(self.bytes.len())
            .and_then(|end| image.get(start..end))
            .is_some_and(|window| {
                window
                    .iter()
                    .zip(&self.bytes)
                    .all(|(byte, want)| want.is_none_or(|want| want == *byte))
            })
    }
}

/// A signature such as `"48 8d 05 ?? ?? ?? ?? c3"`.
pub fn parse_pattern(text: &str) -> Result<Pattern, PatchError> {
    let bytes = text
        .split_whitespace()
        .map(|token| {
            if token == "??" {
                return Ok(None);
            }
            let value = py_int(token, 16).ok_or_else(|| invalid_literal(token, 16))?;
            u8::try_from(value)
                .map(Some)
                .map_err(|_| PatchError::Invalid("bytes must be in range(0, 256)".to_string()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut anchor = 0..0;
    let mut start = None;
    for (i, byte) in bytes.iter().chain(std::iter::once(&None)).enumerate() {
        match (byte, start) {
            (Some(_), None) => start = Some(i),
            (None, Some(run)) => {
                if i - run > anchor.len() {
                    anchor = run..i;
                }
                start = None;
            }
            _ => {}
        }
    }
    if anchor.is_empty() {
        return Err(PatchError::Invalid(
            "pattern without fixed bytes".to_string(),
        ));
    }
    Ok(Pattern { bytes, anchor })
}

/// Horspool search for a non-empty `needle`.
struct Finder<'a> {
    needle: &'a [u8],
    shift: [usize; 256],
}

impl<'a> Finder<'a> {
    fn new(needle: &'a [u8]) -> Self {
        let mut shift = [needle.len(); 256];
        let last = needle.len().saturating_sub(1);
        for (i, &byte) in needle.iter().enumerate().take(last) {
            shift[usize::from(byte)] = last - i;
        }
        Finder { needle, shift }
    }

    /// The first offset at or after `from` where the needle starts.
    fn find(&self, hay: &[u8], from: usize) -> Option<usize> {
        let mut at = from;
        loop {
            let window = hay.get(at..at.checked_add(self.needle.len())?)?;
            if window == self.needle {
                return Some(at);
            }
            let &tail = window.last()?;
            at = at.checked_add(self.shift[usize::from(tail)])?;
        }
    }
}

/// Image offsets where `pattern` matches, in order, at most `limit` of them.
pub fn find_pattern(image: &[u8], pattern: &Pattern, limit: usize) -> Vec<u64> {
    let anchor: Vec<u8> = pattern
        .bytes
        .get(pattern.anchor.clone())
        .unwrap_or_default()
        .iter()
        .flatten()
        .copied()
        .collect();
    let finder = Finder::new(&anchor);
    let mut found = Vec::new();
    let mut from = 0;
    while found.len() < limit {
        let Some(at) = finder.find(image, from) else {
            break;
        };
        if let Some(start) = at.checked_sub(pattern.anchor.start) {
            if pattern.matches_at(image, start) {
                found.push(start as u64);
            }
        }
        from = at + 1;
    }
    found
}

/// The image offset of a `Line`'s or `Target`'s site: its `Pattern` (found once) plus `At`; the
/// reference build's `Address` without a pattern. A signature found no time or twice is a
/// [`PatchError::Lookup`].
pub fn locate(image: &[u8], element: roxmltree::Node, prefix: &str) -> Result<i64, PatchError> {
    let pattern = element
        .attribute(format!("{prefix}Pattern").as_str())
        .filter(|pattern| !pattern.is_empty());
    let Some(pattern) = pattern else {
        return reference_site(element);
    };
    let site = match find_pattern(image, &parse_pattern(pattern)?, 2)[..] {
        [site] => site,
        [] => return Err(PatchError::Lookup("signature not found".to_string())),
        _ => {
            return Err(PatchError::Lookup(
                "signature found more than once".to_string(),
            ))
        }
    };
    let at = int_attribute(element, &format!("{prefix}At"), Some("0"), 0)?;
    i64::try_from(site)
        .ok()
        .and_then(|site| site.checked_add(at))
        .ok_or_else(|| out_of_range(element))
}

/// The little-endian `i32` at image offset `at` (a rel32 operand).
fn i32_at(image: &[u8], at: i64) -> Result<i32, PatchError> {
    usize::try_from(at)
        .ok()
        .and_then(|start| image.get(start..start.checked_add(4)?))
        .and_then(|bytes| bytes.try_into().ok())
        .map(i32::from_le_bytes)
        .ok_or_else(|| {
            PatchError::Invalid(format!(
                "unpack_from requires a buffer of at least {} bytes for unpacking 4 bytes at \
                 offset {at} (actual buffer size is {})",
                i128::from(at) + 4,
                image.len()
            ))
        })
}

/// `items[index]` as Python indexes a list (negative from the end).
fn python_index(items: &[i64], index: i64) -> Option<i64> {
    let index = if index < 0 {
        i64::try_from(items.len()).ok()?.checked_add(index)?
    } else {
        index
    };
    items.get(usize::try_from(index).ok()?).copied()
}

/// Image offsets of a patch's `Target`s: a line's site (`Line`, its index), a site of its own
/// (`Pattern`/`At`), or the address a rip-relative or rel32 operand there points to (`Ref`: the
/// operand is at the site; `Tail`: the instruction's bytes after it).
pub fn resolve_targets(
    image: &[u8],
    meta: roxmltree::Node,
    sites: &[i64],
) -> Result<Targets, PatchError> {
    let mut targets = Targets::new();
    for target in meta.descendants().filter(|n| n.has_tag_name("Target")) {
        let at = if target.attribute("Line").is_some_and(|l| !l.is_empty()) {
            let index = int_attribute(target, "Line", None, 10)?;
            python_index(sites, index)
                .ok_or_else(|| PatchError::Lookup("list index out of range".to_string()))?
        } else {
            let at = locate(image, target, "")?;
            if target.attribute("Ref").is_some_and(|r| !r.is_empty()) {
                let disp = i32_at(image, at)?;
                let tail = int_attribute(target, "Tail", Some("0"), 10)?;
                at.checked_add(4)
                    .and_then(|end| end.checked_add(tail))
                    .and_then(|end| end.checked_add(i64::from(disp)))
                    .ok_or_else(|| out_of_range(target))?
            } else {
                at
            }
        };
        if let Some(name) = target.attribute("Name") {
            targets.insert(name.to_string(), at);
        }
    }
    Ok(targets)
}

/// `value`'s hex bytes with each `{rel:NAME:TAIL}` replaced by the rel32 to target `NAME`,
/// measured from the end of the instruction (`TAIL` bytes after the operand). `at` is where the
/// bytes go.
pub fn fill(value: &str, at: i64, targets: &Targets) -> Result<Vec<u8>, PatchError> {
    let value = value.replace(' ', "");
    let mut out = Vec::new();
    let mut rest = value.as_str();
    while let Some((before, after)) = rest.split_once('{') {
        let Some((field, next)) = after.split_once('}') else {
            break;
        };
        out.extend(from_hex(before)?);
        let parts: Vec<&str> = field.split(':').collect();
        let [_, name, tail] = parts[..] else {
            return Err(PatchError::Invalid(format!(
                "bad field {}",
                py_repr(&format!("{{{field}}}"))
            )));
        };
        let target = *targets
            .get(name)
            .ok_or_else(|| PatchError::Invalid(format!("no target {}", py_repr(name))))?;
        let tail = py_int(tail, 10).ok_or_else(|| invalid_literal(tail, 10))?;
        // Every term is within 64 bits (a tail outside them gives no 32-bit rel), so the sum
        // cannot overflow an i128.
        let rel = i64::try_from(tail)
            .ok()
            .map(|tail| {
                let end = i128::from(at) + out.len() as i128 + 4 + i128::from(tail);
                i128::from(target) - end
            })
            .and_then(|rel| i32::try_from(rel).ok())
            .ok_or_else(|| {
                PatchError::Invalid(format!(
                    "the rel32 to {} does not fit in 32 bits",
                    py_repr(name)
                ))
            })?;
        out.extend(rel.to_le_bytes());
        rest = next;
    }
    out.extend(from_hex(rest)?);
    Ok(out)
}

/// A `Line`'s literal bytes: `bytes`, `bytes16/32/64`, `float32/64`, `utf8` and `utf16`.
pub fn encode(line: roxmltree::Node) -> Result<Vec<u8>, PatchError> {
    let kind = line.attribute("Type");
    let value = || {
        line.attribute("Value")
            .ok_or_else(|| PatchError::Invalid(format!("{}: no Value", describe(line))))
    };
    let float = |text: &str| {
        py_float(text).ok_or_else(|| {
            PatchError::Invalid(format!(
                "could not convert string to float: {}",
                py_repr(text)
            ))
        })
    };
    match kind {
        Some("bytes") => from_hex(&value()?.replace(' ', "")),
        Some(kind @ ("bytes16" | "bytes32" | "bytes64")) => {
            let size = match kind {
                "bytes16" => 2,
                "bytes32" => 4,
                _ => 8,
            };
            let text = value()?;
            let number = py_int(text, 0).ok_or_else(|| invalid_literal(text, 0))?;
            let bytes = u64::try_from(number)
                .ok()
                .filter(|&n| size == 8 || n >> (size * 8) == 0)
                .map(u64::to_le_bytes)
                .ok_or_else(|| PatchError::Invalid("int too big to convert".to_string()))?;
            Ok(bytes.get(..size).unwrap_or_default().to_vec())
        }
        Some("float32") => {
            let number = float(value()?)?;
            let single = number as f32;
            if single.is_infinite() && !number.is_infinite() {
                return Err(PatchError::Invalid(
                    "float too large to pack with f format".to_string(),
                ));
            }
            Ok(single.to_le_bytes().to_vec())
        }
        Some("float64") => Ok(float(value()?)?.to_le_bytes().to_vec()),
        Some("utf8") => {
            let mut bytes = value()?.as_bytes().to_vec();
            bytes.push(0);
            Ok(bytes)
        }
        Some("utf16") => {
            let mut bytes: Vec<u8> = value()?.encode_utf16().flat_map(u16::to_le_bytes).collect();
            bytes.extend([0, 0]);
            Ok(bytes)
        }
        _ => Err(PatchError::Invalid(format!(
            "unsupported patch type {}",
            kind.map_or_else(|| "None".to_string(), py_repr)
        ))),
    }
}

/// Whether `len` bytes at image offset `start` fit inside one segment.
fn fits(segments: &[Segment], start: u64, len: usize) -> bool {
    let end = start.checked_add(len as u64);
    segments
        .iter()
        .any(|segment| segment.start <= start && end.is_some_and(|end| end <= segment.end))
}

/// The sites of a patch's lines and its targets: found by signature in `image`, or the reference
/// build's addresses without it. A line with a `Target` writes at that target (data the code
/// points to, such as a `.bss` variable, with no bytes of its own to find).
fn sites_of(
    meta: roxmltree::Node,
    lines: &[roxmltree::Node],
    image: Option<&[u8]>,
) -> Result<(Vec<i64>, Targets), PatchError> {
    let (sites, targets) = match image {
        Some(image) => {
            let sites = lines
                .iter()
                .map(|line| locate(image, *line, ""))
                .collect::<Result<Vec<_>, _>>()?;
            let targets = resolve_targets(image, meta, &sites)?;
            (sites, targets)
        }
        None => {
            let sites = lines
                .iter()
                .map(|line| reference_site(*line))
                .collect::<Result<Vec<_>, _>>()?;
            let mut targets = Targets::new();
            for target in meta.descendants().filter(|n| n.has_tag_name("Target")) {
                let at = reference_site(target)?;
                if let Some(name) = target.attribute("Name") {
                    targets.insert(name.to_string(), at);
                }
            }
            (sites, targets)
        }
    };
    let sites = lines
        .iter()
        .zip(sites)
        .map(
            |(line, site)| match line.attribute("Target").filter(|t| !t.is_empty()) {
                // The Python's KeyError, whose text is the key's repr.
                Some(name) => targets
                    .get(name)
                    .copied()
                    .ok_or_else(|| PatchError::Lookup(py_repr(name))),
                None => Ok(site),
            },
        )
        .collect::<Result<Vec<_>, _>>()?;
    Ok((sites, targets))
}

/// The writes of the named patches of `xml`, and the patches left out with the reason (a site
/// not found once: the whole patch is left out). With the game's `image` each site is found by
/// its signature (any build); without it, at the reference build's `Address`. `app_version`
/// `None`: any version.
pub fn compile_patches(
    xml: &Path,
    names: &[String],
    app_version: Option<&str>,
    segments: &[Segment],
    image: Option<&[u8]>,
) -> Result<(Vec<Write>, IndexMap<String, String>), PatchError> {
    let text = read_text(xml)?;
    let doc = parse(&text)?;
    let mut found: HashMap<&str, roxmltree::Node> = HashMap::new();
    for meta in metadata(&doc) {
        let Some(name) = meta.attribute("Name") else {
            continue;
        };
        if names.iter().any(|n| n == name)
            && app_version.is_none_or(|version| meta.attribute("AppVer") == Some(version))
            && meta.attribute("AppElf").unwrap_or("eboot.bin") == "eboot.bin"
        {
            found.insert(name, meta);
        }
    }
    let missing: Vec<&str> = names
        .iter()
        .map(String::as_str)
        .filter(|name| !found.contains_key(name))
        .collect();
    if !missing.is_empty() {
        return Err(PatchError::Invalid(format!(
            "patches not found for app version {}: {}",
            app_version.unwrap_or("None"),
            py_list(&missing)
        )));
    }
    let mut writes = Vec::new();
    let mut skipped = IndexMap::new();
    for name in names {
        let Some(&meta) = found.get(name.as_str()) else {
            continue;
        };
        let lines: Vec<_> = meta
            .descendants()
            .filter(|n| n.has_tag_name("Line"))
            .collect();
        let (sites, targets) = match sites_of(meta, &lines, image) {
            Ok(found) => found,
            Err(PatchError::Lookup(reason)) => {
                skipped.insert(name.clone(), reason);
                continue;
            }
            Err(error) => return Err(error),
        };
        for (line, offset) in lines.iter().zip(sites) {
            let value = line.attribute("Value").unwrap_or_default();
            let bytes = if value.contains('{') {
                fill(value, offset, &targets)?
            } else {
                encode(*line)?
            };
            let Some(address) = u64::try_from(offset)
                .ok()
                .filter(|&start| fits(segments, start, bytes.len()))
            else {
                return Err(PatchError::Invalid(format!(
                    "{name}: address {} is outside the eboot",
                    py_hex(i128::from(offset) + i128::from(BASE))
                )));
            };
            writes.push(Write { address, bytes });
        }
    }
    Ok((writes, skipped))
}

// --- The game folder

/// A text value of the game folder's `param.sfo`; `None` when it cannot be read.
fn sfo_text(game: &Path, key: &str) -> Option<String> {
    if game.as_os_str().is_empty() {
        return None;
    }
    match read_sfo(&game.join("sce_sys").join("param.sfo"))?.swap_remove(key)? {
        SfoValue::Text(text) => Some(text),
        SfoValue::Int(_) => None,
    }
}

/// `APP_VER` of the game folder's `param.sfo` ("01.02"); `None` when it cannot be read.
pub fn game_app_version(game: &Path) -> Option<String> {
    sfo_text(game, "APP_VER")
}

/// `TITLE_ID` of the game folder's `param.sfo`.
pub fn game_title_id(game: &Path) -> Option<String> {
    sfo_text(game, "TITLE_ID")
}

/// The patch profile of a game folder, by the title ID or the title of its `param.sfo`.
pub fn game_profile_for(game: &Path) -> Option<&'static GameProfile> {
    let title = sfo_text(game, "TITLE");
    game_profile(game_title_id(game).as_deref(), title.as_deref())
}

/// Whether the built-in patches are tried on this game folder (any region and version).
pub fn game_patchable(game: Option<&Path>) -> bool {
    game.is_some_and(|game| !game.as_os_str().is_empty() && game_profile_for(game).is_some())
}

/// SHA-256 of the game folder's `eboot.bin`, hex; `None` when it cannot be read.
pub fn eboot_sha256(game: &Path) -> Option<String> {
    let bytes = std::fs::read(game.join("eboot.bin")).ok()?;
    Some(
        Sha256::digest(&bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect(),
    )
}

// --- Selection

/// Names of the patches the XML has for this app version, once each, in file order.
pub fn available_patches(xml: &Path, app_version: &str) -> Result<Vec<String>, PatchError> {
    let text = read_text(xml)?;
    let doc = parse(&text)?;
    let mut names: Vec<String> = Vec::new();
    for meta in metadata(&doc).filter(|m| m.attribute("AppVer") == Some(app_version)) {
        let name = meta.attribute("Name").unwrap_or_default();
        if !names.iter().any(|n| n == name) {
            names.push(name.to_string());
        }
    }
    Ok(names)
}

/// The file's `isEnabled` patches for this version, then the names in `extra`, then the
/// [`ALWAYS_ON`] fixes. `only`: `extra` is the whole selection (the launcher's), without the
/// file's defaults. A chosen resolution patch replaces the texture fix; two are an error.
pub fn selected_patches(
    xml: &Path,
    app_version: &str,
    extra: &[String],
    only: bool,
) -> Result<Vec<String>, PatchError> {
    let text = read_text(xml)?;
    let doc = parse(&text)?;
    let metas: Vec<roxmltree::Node> = metadata(&doc)
        .filter(|m| m.attribute("AppVer") == Some(app_version))
        .collect();
    let name_of = |meta: &roxmltree::Node| meta.attribute("Name").unwrap_or_default().to_string();
    let mut names: Vec<String> = if only {
        Vec::new()
    } else {
        metas
            .iter()
            .filter(|meta| is_enabled(**meta))
            .map(name_of)
            .collect()
    };
    let defaults = names.clone();
    names.extend(
        extra
            .iter()
            .map(|name| name.trim())
            .filter(|name| !name.is_empty() && !defaults.iter().any(|d| d == name))
            .map(str::to_string),
    );
    let chosen = names.clone();
    names.extend(
        metas
            .iter()
            .map(name_of)
            .filter(|name| ALWAYS_ON.contains(&name.as_str()) && !chosen.contains(name)),
    );
    let exclusive: Vec<&String> = names
        .iter()
        .filter(|name| name.starts_with(EXCLUSIVE_PREFIX))
        .collect();
    if exclusive.len() > 1 {
        return Err(PatchError::Invalid(format!(
            "choose one resolution patch, not {}",
            py_list(&exclusive)
        )));
    }
    let replaced = !exclusive.is_empty();
    Ok(names
        .into_iter()
        .filter(|name| !(replaced && REPLACED_BY_EXCLUSIVE.contains(&name.as_str())))
        .collect())
}

/// Names of the patches of these files that are on by default (`isEnabled`) for this app version;
/// a file that cannot be read is left out.
pub fn default_enabled(xmls: &[PathBuf], app_version: &str) -> Vec<String> {
    let mut names = Vec::new();
    for path in xmls {
        let Ok(text) = std::fs::read_to_string(path) else {
            continue;
        };
        let Ok(doc) = parse(&text) else {
            continue;
        };
        names.extend(
            metadata(&doc)
                .filter(|m| m.attribute("AppVer") == Some(app_version) && is_enabled(*m))
                .map(|m| m.attribute("Name").unwrap_or_default().to_string()),
        );
    }
    names
}

/// The number of the first `before<digits>after` in `note` (the Python's `re.search`).
fn note_number(note: &str, before: &str, after: &str) -> Option<u64> {
    note.match_indices(before).find_map(|(at, _)| {
        let rest = note.get(at + before.len()..)?;
        let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
        let tail = rest.get(digits..)?;
        if digits == 0 || !tail.starts_with(after) {
            return None;
        }
        rest.get(..digits)?.parse().ok()
    })
}

/// What the selected patches' notes ask for: shadPS4 notes say "Requires setting DMEM to N MB"
/// (N on top of the retail pool) and "VBlank to N FPS".
pub fn patch_requirements(
    xml: &Path,
    names: &[String],
    app_version: &str,
) -> Result<Requirements, PatchError> {
    let text = read_text(xml)?;
    let doc = parse(&text)?;
    let mut needs = Requirements::default();
    for meta in metadata(&doc) {
        let named = meta
            .attribute("Name")
            .is_some_and(|name| names.iter().any(|n| n == name));
        if !named || meta.attribute("AppVer") != Some(app_version) {
            continue;
        }
        let note = meta.attribute("Note").unwrap_or_default();
        if let Some(mb) = note_number(note, "DMEM to ", " MB") {
            let dmem = DMEM_RETAIL_MB.saturating_add(mb);
            needs.dmem_mb = Some(needs.dmem_mb.unwrap_or(0).max(dmem));
        }
        if let Some(hz) =
            note_number(note, "VBlank to ", " FPS").and_then(|hz| u32::try_from(hz).ok())
        {
            needs.vblank_hz = Some(needs.vblank_hz.unwrap_or(0).max(hz));
        }
    }
    Ok(needs)
}

// --- patches-merged.xml, as Python's ElementTree parses, indents and writes it

/// An element as ElementTree keeps it: `text` before the first child, `tail` after the element.
/// Comments and processing instructions are dropped and the text around them joined. Its depth
/// is bounded by [`parse`], so the recursions over it are too.
#[derive(Debug, Clone)]
struct Element {
    tag: String,
    attributes: Vec<(String, String)>,
    text: Option<String>,
    tail: Option<String>,
    children: Vec<Element>,
}

impl Element {
    fn from_node(node: roxmltree::Node) -> Element {
        let mut element = Element {
            tag: node.tag_name().name().to_string(),
            attributes: node
                .attributes()
                .map(|a| (a.name().to_string(), a.value().to_string()))
                .collect(),
            text: None,
            tail: None,
            children: Vec::new(),
        };
        for child in node.children() {
            if child.is_element() {
                element.children.push(Element::from_node(child));
            } else if let Some(text) = child.text().filter(|_| child.is_text()) {
                let slot = match element.children.last_mut() {
                    Some(last) => &mut last.tail,
                    None => &mut element.text,
                };
                slot.get_or_insert_with(String::new).push_str(text);
            }
        }
        element
    }

    fn get(&self, name: &str) -> Option<&str> {
        self.attributes
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }

    /// The element and its descendants named `tag`, in document order (ElementTree's `iter`).
    fn collect<'a>(&'a self, tag: &str, found: &mut Vec<&'a Element>) {
        if self.tag == tag {
            found.push(self);
        }
        for child in &self.children {
            child.collect(tag, found);
        }
    }

    /// ElementTree's `indent` with two spaces, for an element with children at `level`: blank
    /// texts and tails become the indentation, others are kept.
    fn indent(&mut self, level: usize) {
        let blank = |text: &Option<String>| text.as_deref().is_none_or(|t| t.trim().is_empty());
        let inner = format!("\n{}", "  ".repeat(level + 1));
        if blank(&self.text) {
            self.text = Some(inner.clone());
        }
        for child in &mut self.children {
            if !child.children.is_empty() {
                child.indent(level + 1);
            }
            if blank(&child.tail) {
                child.tail = Some(inner.clone());
            }
        }
        if let Some(last) = self.children.last_mut() {
            if blank(&last.tail) {
                last.tail = Some(format!("\n{}", "  ".repeat(level)));
            }
        }
    }

    /// ElementTree's `write` of the element (`short_empty_elements`: `<a />`).
    fn serialize(&self, out: &mut String) {
        out.push('<');
        out.push_str(&self.tag);
        for (name, value) in &self.attributes {
            out.push(' ');
            out.push_str(name);
            out.push_str("=\"");
            escape_attribute(value, out);
            out.push('"');
        }
        let text = self.text.as_deref().filter(|text| !text.is_empty());
        if text.is_some() || !self.children.is_empty() {
            out.push('>');
            if let Some(text) = text {
                escape_text(text, out);
            }
            for child in &self.children {
                child.serialize(out);
            }
            out.push_str("</");
            out.push_str(&self.tag);
            out.push('>');
        } else {
            out.push_str(" />");
        }
        if let Some(tail) = &self.tail {
            escape_text(tail, out);
        }
    }
}

fn escape_text(text: &str, out: &mut String) {
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            c => out.push(c),
        }
    }
}

fn escape_attribute(text: &str, out: &mut String) {
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\r' => out.push_str("&#13;"),
            '\n' => out.push_str("&#10;"),
            '\t' => out.push_str("&#09;"),
            c => out.push(c),
        }
    }
}

fn read_tree(path: &Path) -> Result<Element, PatchError> {
    let text = read_text(path)?;
    let doc = parse(&text)?;
    Ok(Element::from_node(doc.root_element()))
}

/// The non-empty `ID` texts of a tree, trimmed.
fn ids_of(root: &Element) -> HashSet<String> {
    let mut found = Vec::new();
    root.collect("ID", &mut found);
    found
        .iter()
        .filter_map(|id| id.text.as_deref())
        .filter(|text| !text.is_empty())
        .map(|text| text.trim().to_string())
        .collect()
}

/// Writes `out`: the patches of `base` plus those of the mods' patch files (`extras`, in load
/// order). A patch with a name already there is replaced; a file for other titles is left out,
/// and one that does not parse too, with a note in `log`. Written as the Python's ElementTree
/// writes it, with LF line ends (the Python's text mode gives CRLF on Windows).
pub fn merge_patch_files(
    base: &Path,
    extras: &[PathBuf],
    out: &Path,
    log: &mut dyn io::Write,
) -> Result<(), PatchError> {
    let mut root = read_tree(base)?;
    let ids = ids_of(&root);
    for path in extras {
        let extra = match read_tree(path) {
            Ok(extra) => extra,
            Err(error) => {
                let _ = writeln!(log, "Patches: {}: {error}", file_name(path));
                continue;
            }
        };
        let extra_ids = ids_of(&extra);
        if !extra_ids.is_empty() && extra_ids.is_disjoint(&ids) {
            continue;
        }
        let folder = path
            .parent()
            .and_then(Path::parent)
            .map(file_name)
            .unwrap_or_default();
        let mut metas = Vec::new();
        extra.collect("Metadata", &mut metas);
        for meta in metas {
            let name = meta.get("Name");
            root.children
                .retain(|child| !(child.tag == "Metadata" && child.get("Name") == name));
            root.children.push(meta.clone());
            let _ = writeln!(
                log,
                "Patches: {} from the mod file {folder}/{}",
                name.unwrap_or("None"),
                file_name(path)
            );
        }
    }
    if !root.children.is_empty() {
        root.indent(0);
    }
    let mut xml = String::from("<?xml version='1.0' encoding='utf-8'?>\n");
    root.serialize(&mut xml);
    atomic_write(out, xml.as_bytes()).map_err(|error| with_path(out, error))
}

// --- Third-party patch files

/// The external patches that are on: `patches.json` (`config`, `{"enabled": [...], "disabled":
/// [...]}`) overrides each file's `isEnabled`.
pub fn external_selection(
    found: &[(String, PathBuf, PatchMeta)],
    config: Option<&Path>,
) -> Result<Vec<(String, PathBuf, PatchMeta)>, PatchError> {
    let settings = match config.filter(|path| path.is_file()) {
        Some(path) => serde_json::from_str(&read_text(path)?)
            .map_err(|error| PatchError::Invalid(format!("{}: {error}", path.display())))?,
        None => Value::Null,
    };
    Ok(found
        .iter()
        .filter(|(key, _, meta)| external_on(&settings, key, meta))
        .cloned()
        .collect())
}

/// The writes of one external patch: the `occurrence`-th `Metadata` of its file for this version
/// that reads as `meta` (a file may repeat a patch).
fn external_writes(
    path: &Path,
    meta: &PatchMeta,
    occurrence: usize,
    app_version: &str,
    segments: &[Segment],
) -> Result<Vec<Write>, PatchError> {
    let text = read_text(path)?;
    let doc = parse(&text)?;
    let element = metadata(&doc)
        .filter(|m| {
            m.attribute("AppVer") == Some(app_version)
                && m.attribute("AppElf").unwrap_or("eboot.bin") == "eboot.bin"
                && meta_of(*m) == *meta
        })
        .nth(occurrence)
        .ok_or_else(|| PatchError::Invalid(format!("no longer in {}", file_name(path))))?;
    element
        .descendants()
        .filter(|n| n.has_tag_name("Line"))
        .map(|line| {
            let text = line.attribute("Address").unwrap_or_default();
            let address = py_int(text, 0).ok_or_else(|| invalid_literal(text, 0))?;
            let bytes = encode(line)?;
            address
                .checked_sub(i128::from(EBOOT_BASE))
                .and_then(|offset| u64::try_from(offset).ok())
                .filter(|&start| fits(segments, start, bytes.len()))
                .map(|address| Write { address, bytes })
                .ok_or_else(|| PatchError::Invalid(format!("address {text} is outside the eboot")))
        })
        .collect()
}

/// The writes of the selected external patches; a patch with a line that cannot be written is
/// skipped whole, with a note in `log`.
pub fn compile_external(
    selected: &[(String, PathBuf, PatchMeta)],
    app_version: &str,
    segments: &[Segment],
    log: &mut dyn io::Write,
) -> Vec<Write> {
    let mut writes = Vec::new();
    for (index, (key, path, meta)) in selected.iter().enumerate() {
        // Each entry is one Metadata of its file. Entries alike (file, name and attributes) are
        // selected alike and keep the file's order, so the k-th of them here is the k-th there.
        let occurrence = selected
            .iter()
            .take(index)
            .filter(|(_, other_path, other)| other_path == path && other == meta)
            .count();
        match external_writes(path, meta, occurrence, app_version, segments) {
            Ok(ours) => {
                let count = ours.len();
                writes.extend(ours);
                let _ = writeln!(log, "Patches: external {key} ({count} writes)");
            }
            Err(error) => {
                let _ = writeln!(log, "Patches: skipped {key}: {error}");
            }
        }
    }
    writes
}

// --- The files the probe reads

/// Writes `out/patches.bin` (`G3PATCH2`): the patch base ([`EBOOT_BASE`], so the loader can rebase
/// the pointers patches write into relocated slots), the count, then each write's image offset,
/// length and bytes.
pub fn write_patches(out: &Path, writes: &[Write]) -> io::Result<()> {
    let mut blob = PATCH_MAGIC.to_vec();
    blob.extend(EBOOT_BASE.to_le_bytes());
    blob.extend((writes.len() as u64).to_le_bytes());
    for write in writes {
        blob.extend(write.address.to_le_bytes());
        blob.extend((write.bytes.len() as u64).to_le_bytes());
        blob.extend(&write.bytes);
    }
    atomic_write(&out.join("patches.bin"), &blob)
}

/// `json.dumps`' default layout: one line, `", "` between items and `": "` after keys.
struct PythonCompact;

impl serde_json::ser::Formatter for PythonCompact {
    fn begin_array_value<W: ?Sized + io::Write>(
        &mut self,
        writer: &mut W,
        first: bool,
    ) -> io::Result<()> {
        if first {
            Ok(())
        } else {
            writer.write_all(b", ")
        }
    }

    fn begin_object_key<W: ?Sized + io::Write>(
        &mut self,
        writer: &mut W,
        first: bool,
    ) -> io::Result<()> {
        if first {
            Ok(())
        } else {
            writer.write_all(b", ")
        }
    }

    fn begin_object_value<W: ?Sized + io::Write>(&mut self, writer: &mut W) -> io::Result<()> {
        writer.write_all(b": ")
    }
}

/// `value` as Python's `json.dumps(value)`: [`PythonCompact`] and [`ensure_ascii`].
pub(crate) fn python_json(value: &impl Serialize) -> io::Result<String> {
    let mut bytes = Vec::new();
    value
        .serialize(&mut serde_json::Serializer::with_formatter(
            &mut bytes,
            PythonCompact,
        ))
        .map_err(io::Error::other)?;
    let text = String::from_utf8(bytes).map_err(io::Error::other)?;
    Ok(ensure_ascii(&text))
}

/// `text`, JSON serde_json wrote, as Python's `json.dumps` spells it with `ensure_ascii`: every
/// character past `~` (DEL included) as `\uXXXX`, outside the BMP as a surrogate pair. Below DEL
/// serde_json already escapes what the Python does.
pub(crate) fn ensure_ascii(text: &str) -> String {
    let mut ascii = String::with_capacity(text.len());
    for c in text.chars() {
        if c < '\u{7f}' {
            ascii.push(c);
        } else {
            for unit in c.encode_utf16(&mut [0; 2]).iter() {
                ascii.push_str(&format!("\\u{unit:04x}"));
            }
        }
    }
    ascii
}

/// Writes `out/patch_status.json` as the Python's `json.dumps` of it.
pub fn write_status(out: &Path, status: &PatchStatus) -> io::Result<()> {
    atomic_write(
        &out.join("patch_status.json"),
        python_json(status)?.as_bytes(),
    )
}

/// What `patches.py`'s command line gives it (`run.py` fills it from its environment).
#[derive(Debug, Clone, Copy)]
pub struct PatchJob<'a> {
    /// `out/`: holds `eboot.elf`; receives `patches.bin`, `patch_status.json` and, with mods,
    /// `patches-merged.xml`.
    pub out: &'a Path,
    /// The game folder (`eboot.bin`, `sce_sys/param.sfo`).
    pub game: &'a Path,
    /// The folder of the built-in patch files (`<port>/patches`).
    pub builtin_dir: &'a Path,
    /// The enabled mods' patch files, in load order (`--mod-patches`).
    pub mod_patches: &'a [PathBuf],
    /// Patch names (`--extra`: `SPARTAN_PATCHES` split at `;`).
    pub extra: &'a [String],
    /// `SPARTAN_PATCHES_ONLY=1`: `extra` is the whole selection.
    pub only: bool,
    /// Third-party patch files (`--patches-dir`), applied on the reference build only.
    pub patches_dir: Option<&'a Path>,
    /// Their selection, `patches.json` (`--patches-config`).
    pub patches_config: Option<&'a Path>,
}

/// `patches.py`'s `main`: compiles the selected patches of the game's profile against
/// `out/eboot.elf` into `out/patches.bin` and `out/patch_status.json`, and prints to `log` what
/// the Python prints. A game without a profile gets a `patches.bin` without writes and no status.
pub fn write_game_patches(job: &PatchJob<'_>, log: &mut dyn io::Write) -> Result<(), PatchError> {
    let title = game_title_id(job.game);
    let title = title.as_deref().unwrap_or("None");
    let Some(profile) = game_profile_for(job.game) else {
        write_patches(job.out, &[]).map_err(|error| with_path(job.out, error))?;
        let _ = writeln!(log, "Patches: no patch profile for {title}: none applied");
        return Ok(());
    };
    let mut xml = job.builtin_dir.join(profile.xml);
    if !job.mod_patches.is_empty() {
        let merged = job.out.join("patches-merged.xml");
        merge_patch_files(&xml, job.mod_patches, &merged, log)?;
        xml = merged;
    }
    let sha = eboot_sha256(job.game);
    let _ = writeln!(
        log,
        "Patches: {title} {}, eboot.bin sha256 {}",
        game_app_version(job.game).as_deref().unwrap_or("None"),
        sha.as_deref().unwrap_or("None")
    );
    let names = selected_patches(&xml, profile.app_version, job.extra, job.only)?;
    let elf = read_file(&job.out.join("eboot.elf"))?;
    let segments = eboot_segments(&elf)?;
    let image = eboot_image(&elf)?;
    let (mut writes, skipped) = compile_patches(&xml, &names, None, &segments, Some(&image))?;
    for (name, reason) in &skipped {
        let _ = writeln!(log, "Patches: {name} skipped ({reason} in this eboot.bin)");
    }
    if let Some(dir) = job
        .patches_dir
        .filter(|_| sha.as_deref() == Some(REFERENCE_SHA256))
    {
        // Third-party files use the reference build's absolute addresses, so they are only
        // applied there. After the built-in ones: an external patch of the same bytes wins.
        let found = external_patches(dir, profile.app_version, &xml, profile.ids);
        let selected = external_selection(&found, job.patches_config)?;
        writes.extend(compile_external(
            &selected,
            profile.app_version,
            &segments,
            log,
        ));
    }
    write_patches(job.out, &writes).map_err(|error| with_path(job.out, error))?;
    let applied: Vec<String> = names
        .iter()
        .filter(|name| !skipped.contains_key(*name))
        .cloned()
        .collect();
    let summary = if applied.is_empty() {
        "none".to_string()
    } else {
        py_list(&applied)
    };
    let status = PatchStatus {
        sha256: sha,
        applied,
        skipped,
        ..PatchStatus::default()
    };
    write_status(job.out, &status).map_err(|error| with_path(job.out, error))?;
    let _ = writeln!(
        log,
        "Patches: {title}; {} writes from {summary}",
        writes.len()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    const GAME_XML: &str = r#"<?xml version="1.0"?>
<Patch>
  <TitleID><ID>CUSA01623</ID></TitleID>
  <Metadata Name="Bug Fix - Startup Black Screen" AppVer="01.02" isEnabled="true"/>
  <Metadata Name="Resolution Patch - 1920x1080" AppVer="01.02" isEnabled="false"/>
  <Metadata Name="Resolution Patch - 854x480" AppVer="01.02"/>
  <Metadata Name="Resolution Patch - 3440x1440" AppVer="01.02"/>
  <Metadata Name="Resolution Patch - wide" AppVer="01.02"/>
  <Metadata Name="Frame Rate Patch - 120 FPS" AppVer="01.02"/>
  <Metadata Name="Skip Intro" Author="me" Note="faster" AppVer="01.02" isEnabled="TRUE"/>
  <Metadata Name="Old Fix" Author="" Note="" AppVer="01.00" isEnabled="true"/>
</Patch>"#;

    const EXTRA_XML: &str = r#"<Patch>
  <TitleID><ID>CUSA99999</ID></TitleID>
  <Metadata Name="Other Game" AppVer="01.02"/>
</Patch>"#;

    const MINE_XML: &str = r#"<Patch>
  <TitleID><ID>CUSA01623</ID></TitleID>
  <Metadata Name="Cheat" Author="a" AppVer="01.02" isEnabled="true"/>
  <Metadata Name="Off" AppVer="01.02"/>
  <Metadata Name="Wrong Elf" AppVer="01.02" AppElf="other.elf"/>
  <Metadata Name="Wrong Version" AppVer="01.00"/>
</Patch>"#;

    fn write(dir: &Path, name: &str, text: &str) -> PathBuf {
        let path = dir.join(name);
        fs::write(&path, text).unwrap();
        path
    }

    fn names(metas: &[PatchMeta]) -> Vec<&str> {
        metas.iter().map(|m| m.name.as_str()).collect()
    }

    #[test]
    fn metas_filter_by_app_version() {
        let dir = tempfile::tempdir().unwrap();
        let xml = write(dir.path(), "g.xml", GAME_XML);
        let metas = game_patch_metas(&xml, "01.02").unwrap();
        assert_eq!(metas.len(), 7);
        assert_eq!(metas[0].name, "Bug Fix - Startup Black Screen");
        assert!(metas[0].enabled_by_default);
        assert!(!metas[1].enabled_by_default);
        assert!(!metas[2].enabled_by_default);
        let skip = metas.iter().find(|m| m.name == "Skip Intro").unwrap();
        assert_eq!(skip.author.as_deref(), Some("me"));
        assert_eq!(skip.note.as_deref(), Some("faster"));
        assert!(skip.enabled_by_default);
        let old = game_patch_metas(&xml, "01.00").unwrap();
        assert_eq!(names(&old), ["Old Fix"]);
        assert_eq!(old[0].author, None);
        assert_eq!(old[0].note, None);
        assert!(game_patch_metas(&xml, "09.99").unwrap().is_empty());
        assert!(matches!(
            game_patch_metas(&write(dir.path(), "bad.xml", "<a>"), "01.02"),
            Err(PatchError::Xml(_))
        ));
        assert!(matches!(
            game_patch_metas(&dir.path().join("none.xml"), "01.02"),
            Err(PatchError::Io(_))
        ));
    }

    #[test]
    fn selectable_hides_resolution_frame_rate_and_always_on() {
        let dir = tempfile::tempdir().unwrap();
        let xml = write(dir.path(), "g.xml", GAME_XML);
        let metas = game_patch_metas(&xml, "01.02").unwrap();
        assert_eq!(names(&selectable(&metas)), ["Skip Intro"]);
    }

    #[test]
    fn resolution_names_sorted_by_pixels() {
        let dir = tempfile::tempdir().unwrap();
        let a = write(dir.path(), "a.xml", GAME_XML);
        let b = write(dir.path(), "b.xml", GAME_XML);
        let bad = write(dir.path(), "bad.xml", "<a>");
        let gone = dir.path().join("gone.xml");
        assert_eq!(
            resolution_names(&[a, bad, gone, b]),
            ["854x480", "1920x1080", "3440x1440", "wide"]
        );
    }

    #[test]
    fn an_absurd_size_from_a_mod_sorts_last_as_in_python() {
        // A mod's XML can name any size. Python's integers have no limit: a pixel count above
        // 2^60 sorts after the names that are not WxH, and 0 times anything is 0.
        let dir = tempfile::tempdir().unwrap();
        let file = |name: &str, sizes: &[String]| {
            let entries: String = sizes
                .iter()
                .map(|size| {
                    format!(r#"<Metadata Name="Resolution Patch - {size}" AppVer="01.02"/>"#)
                })
                .collect();
            write(dir.path(), name, &format!("<Patch>{entries}</Patch>"))
        };
        let wide = format!("{0}x{0}", "9".repeat(20)); // overflows the product
        let tall = format!("{}x2", "1".repeat(45)); // overflows the width
        let flat = format!("0x{}", "7".repeat(45)); // 0 pixels
        let sizes = [
            wide.clone(),
            "Custom".into(),
            flat.clone(),
            "1280x720".into(),
        ];
        assert_eq!(
            resolution_names(&[file("a.xml", &sizes)]),
            [flat, "1280x720".into(), "Custom".into(), wide]
        );
        let sizes = [tall.clone(), "Custom".into()];
        assert_eq!(
            resolution_names(&[file("b.xml", &sizes)]),
            ["Custom".to_string(), tall]
        );
    }

    #[test]
    fn external_patches_skip_builtin_and_other_games() {
        let dir = tempfile::tempdir().unwrap();
        let builtin = write(dir.path(), "builtin.xml", GAME_XML);
        write(dir.path(), "CUSA01623.xml", GAME_XML);
        write(dir.path(), LEGACY_PATCH_FILE, GAME_XML);
        write(dir.path(), "other.xml", EXTRA_XML);
        write(dir.path(), "broken.xml", "<a>");
        write(dir.path(), "notes.txt", MINE_XML);
        let mine = write(dir.path(), "mine.xml", MINE_XML);
        let found = external_patches(dir.path(), "01.02", &builtin, &["CUSA01623"]);
        let keys: Vec<&str> = found.iter().map(|f| f.0.as_str()).collect();
        assert_eq!(keys, ["mine.xml/Cheat", "mine.xml/Off"]);
        assert_eq!(found[0].1, mine);
        assert!(found[0].2.enabled_by_default);
        assert!(external_patches(&dir.path().join("none"), "01.02", &builtin, &[]).is_empty());
    }

    #[test]
    fn external_enabled_follows_the_config() {
        let meta = |on| PatchMeta {
            name: String::new(),
            author: None,
            note: None,
            enabled_by_default: on,
        };
        let p = PathBuf::from("x.xml");
        let found = vec![
            ("x.xml/a".to_string(), p.clone(), meta(true)),
            ("x.xml/b".to_string(), p.clone(), meta(true)),
            ("x.xml/c".to_string(), p.clone(), meta(false)),
            ("x.xml/d".to_string(), p, meta(false)),
        ];
        let none = serde_json::json!({});
        assert_eq!(external_enabled(&found, &none), ["x.xml/a", "x.xml/b"]);
        let cfg = serde_json::json!({"enabled": ["x.xml/c", "x.xml/b"], "disabled": ["x.xml/b", "x.xml/a"]});
        assert_eq!(external_enabled(&found, &cfg), ["x.xml/b", "x.xml/c"]);
    }

    #[test]
    fn patch_status_missing_is_none() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(patch_status(dir.path()), None);
        write(dir.path(), "patch_status.json", "not json");
        assert_eq!(patch_status(dir.path()), None);
    }

    #[test]
    fn patch_status_reads_applied_and_skipped() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            "patch_status.json",
            r#"{"sha256":"ab","applied":["A","B"],"skipped":{"C":"site not found"}}"#,
        );
        let status = patch_status(dir.path()).unwrap();
        assert_eq!(status.applied, ["A", "B"]);
        assert_eq!(
            status.skipped.get("C").map(String::as_str),
            Some("site not found")
        );
    }

    #[test]
    fn builtin_xml_is_under_port_patches() {
        let paths = Paths::from_env(Path::new("app"), &|_| None);
        let profile = crate::core::game::game_profile(Some("CUSA01623"), None).unwrap();
        assert_eq!(
            builtin_xml(&paths, profile),
            paths.port_dir.join("patches").join(profile.xml)
        );
    }
}

/// The compile side: `tests/test_patches.py` by name (the two that read the player's dump are in
/// `tests/dump.rs`), then the writers' formats and hostile input.
#[cfg(test)]
mod compile_tests {
    use super::*;
    use std::collections::{HashMap, HashSet};
    use std::fs;

    const ALL: u64 = 0x600_0000;

    fn repo(path: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join(path)
    }

    fn spartan() -> PathBuf {
        repo("patches/CUSA01623.xml")
    }

    fn ultrawide() -> PathBuf {
        repo("extras/mods/Ultrawide/patches/Ultrawide.xml")
    }

    /// One segment from 0 to `end` (the Python tests' `[(0, end)]`).
    fn segments(end: u64) -> Vec<Segment> {
        std::iter::once(0..end).collect()
    }

    fn strings(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| name.to_string()).collect()
    }

    fn compile(
        xml: &Path,
        names: &[&str],
        version: Option<&str>,
        end: u64,
        image: Option<&[u8]>,
    ) -> (Vec<Write>, IndexMap<String, String>) {
        compile_patches(xml, &strings(names), version, &segments(end), image).unwrap()
    }

    fn pairs(writes: &[Write]) -> Vec<(u64, Vec<u8>)> {
        writes
            .iter()
            .map(|w| (w.address, w.bytes.clone()))
            .collect()
    }

    /// The writes by address: Python's `dict(writes)`, where a later write wins.
    fn by_address(writes: &[Write]) -> HashMap<u64, Vec<u8>> {
        writes
            .iter()
            .map(|w| (w.address, w.bytes.clone()))
            .collect()
    }

    /// Where the rel32 at `data[at..at + 4]` points, measured from `end`.
    fn rel(data: &[u8], at: usize, end: u64) -> u64 {
        let disp = i32::from_le_bytes(data[at..at + 4].try_into().unwrap());
        end.checked_add_signed(i64::from(disp)).unwrap()
    }

    fn reasons(entries: &[(&str, &str)]) -> IndexMap<String, String> {
        entries
            .iter()
            .map(|(name, reason)| (name.to_string(), reason.to_string()))
            .collect()
    }

    fn metadata_names(xml: &Path) -> Vec<String> {
        let text = fs::read_to_string(xml).unwrap();
        let doc = parse(&text).unwrap();
        metadata(&doc)
            .map(|m| m.attribute("Name").unwrap_or_default().to_string())
            .collect()
    }

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    fn hex_bytes(text: &str) -> Vec<u8> {
        (0..text.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap())
            .collect()
    }

    /// A `param.sfo` holding these text values.
    fn sfo(entries: &[(&str, &str)]) -> Vec<u8> {
        let keys_off = 20 + 16 * entries.len();
        let (mut keys, mut values, mut table) = (Vec::new(), Vec::new(), Vec::new());
        for (key, value) in entries {
            let mut raw = value.as_bytes().to_vec();
            raw.push(0);
            table.extend_from_slice(&(keys.len() as u16).to_le_bytes());
            table.extend_from_slice(&0x204u16.to_le_bytes());
            table.extend_from_slice(&(raw.len() as u32).to_le_bytes());
            table.extend_from_slice(&(raw.len() as u32).to_le_bytes());
            table.extend_from_slice(&(values.len() as u32).to_le_bytes());
            keys.extend_from_slice(key.as_bytes());
            keys.push(0);
            values.extend_from_slice(&raw);
        }
        let values_off = keys_off + keys.len();
        let mut out = b"\0PSF".to_vec();
        out.extend_from_slice(&0x101u32.to_le_bytes());
        out.extend_from_slice(&(keys_off as u32).to_le_bytes());
        out.extend_from_slice(&(values_off as u32).to_le_bytes());
        out.extend_from_slice(&(entries.len() as u32).to_le_bytes());
        out.extend(table);
        out.extend(keys);
        out.extend(values);
        out
    }

    /// A game folder whose `param.sfo` holds these values.
    fn game_folder(root: &Path, entries: &[(&str, &str)]) -> PathBuf {
        let game = root.join("game");
        fs::create_dir_all(game.join("sce_sys")).unwrap();
        fs::write(game.join("sce_sys").join("param.sfo"), sfo(entries)).unwrap();
        game
    }

    // --- ExternalPatchTests

    const EXTERNAL: &str = r#"<?xml version="1.0"?>
<Patch>
  <TitleID><ID>CUSA01623</ID></TitleID>
  <Metadata Title="SpartanPort" Name="On" Author="x" PatchVer="1.0" AppVer="01.02" AppElf="eboot.bin" isEnabled="true">
    <PatchList><Line Type="bytes" Address="0x00401000" Value="9090"/></PatchList>
  </Metadata>
  <Metadata Title="SpartanPort" Name="Off" Author="x" PatchVer="1.0" AppVer="01.02" AppElf="eboot.bin">
    <PatchList><Line Type="bytes32" Address="0x00402000" Value="0x12345678"/></PatchList>
  </Metadata>
  <Metadata Title="SpartanPort" Name="Mask" Author="x" PatchVer="1.0" AppVer="01.02" AppElf="eboot.bin" isEnabled="true">
    <PatchList><Line Type="mask" Value="90 ?? 90" Offset="0"/></PatchList>
  </Metadata>
  <Metadata Title="SpartanPort" Name="Old" Author="x" PatchVer="1.0" AppVer="01.00" AppElf="eboot.bin" isEnabled="true">
    <PatchList><Line Type="bytes" Address="0x00403000" Value="90"/></PatchList>
  </Metadata>
</Patch>"#;

    #[test]
    fn selection_and_unsupported_lines() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path();
        fs::write(folder.join("extra.xml"), EXTERNAL).unwrap();
        fs::write(
            folder.join("other.xml"),
            EXTERNAL.replace("CUSA01623", "CUSA99999"),
        )
        .unwrap();
        fs::write(folder.join("broken.xml"), "<Patch>").unwrap();
        let found = external_patches(folder, "01.02", &spartan(), &["CUSA01623"]);
        let keys: Vec<&str> = found.iter().map(|f| f.0.as_str()).collect();
        assert_eq!(keys, ["extra.xml/On", "extra.xml/Off", "extra.xml/Mask"]);
        // The file's isEnabled; the mask patch is skipped as a whole.
        let mut log = Vec::new();
        let selected = external_selection(&found, None).unwrap();
        let writes = compile_external(&selected, "01.02", &segments(ALL), &mut log);
        assert_eq!(pairs(&writes), [(0x1000, vec![0x90, 0x90])]);
        assert_eq!(
            String::from_utf8(log).unwrap(),
            "Patches: external extra.xml/On (1 writes)\n\
             Patches: skipped extra.xml/Mask: invalid literal for int() with base 0: ''\n"
        );
        let config = folder.join("patches.json");
        fs::write(
            &config,
            r#"{"enabled": ["extra.xml/Off"], "disabled": ["extra.xml/On"]}"#,
        )
        .unwrap();
        let selected = external_selection(&found, Some(&config)).unwrap();
        let writes = compile_external(&selected, "01.02", &segments(ALL), &mut Vec::new());
        assert_eq!(
            pairs(&writes),
            [(0x2000, 0x1234_5678u32.to_le_bytes().to_vec())]
        );
        // An address no 64-bit offset holds is outside the eboot, as the Python says.
        let far = folder.join("far");
        fs::create_dir_all(&far).unwrap();
        let huge = "-170141183460469231731687303715884105727";
        fs::write(
            far.join("far.xml"),
            format!(
                r#"<Patch><Metadata Name="Far" AppVer="01.02" isEnabled="true"><PatchList><Line Type="bytes" Address="{huge}" Value="90"/></PatchList></Metadata></Patch>"#
            ),
        )
        .unwrap();
        let found = external_patches(&far, "01.02", &spartan(), &["CUSA01623"]);
        let mut log = Vec::new();
        let selected = external_selection(&found, None).unwrap();
        assert!(compile_external(&selected, "01.02", &segments(ALL), &mut log).is_empty());
        assert_eq!(
            String::from_utf8(log).unwrap(),
            format!("Patches: skipped far.xml/Far: address {huge} is outside the eboot\n")
        );
    }

    #[test]
    fn a_repeated_external_name_compiles_each_element_once() {
        // The Python compiles each Metadata's own lines, in file order: both "Twin" entries, and
        // only the second "Pair" (the first is off).
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("twin.xml"),
            r#"<Patch>
  <TitleID><ID>CUSA01623</ID></TitleID>
  <Metadata Name="Twin" AppVer="01.02" isEnabled="true"><PatchList><Line Type="bytes" Address="0x00401000" Value="90"/></PatchList></Metadata>
  <Metadata Name="Twin" AppVer="01.02" isEnabled="true"><PatchList><Line Type="bytes" Address="0x00402000" Value="91"/></PatchList></Metadata>
  <Metadata Name="Pair" AppVer="01.02"><PatchList><Line Type="bytes" Address="0x00403000" Value="92"/></PatchList></Metadata>
  <Metadata Name="Pair" AppVer="01.02" isEnabled="true"><PatchList><Line Type="bytes" Address="0x00404000" Value="93"/></PatchList></Metadata>
</Patch>"#,
        )
        .unwrap();
        let found = external_patches(dir.path(), "01.02", &spartan(), &["CUSA01623"]);
        let mut log = Vec::new();
        let selected = external_selection(&found, None).unwrap();
        let writes = compile_external(&selected, "01.02", &segments(ALL), &mut log);
        assert_eq!(
            pairs(&writes),
            [
                (0x1000, vec![0x90]),
                (0x2000, vec![0x91]),
                (0x4000, vec![0x93])
            ]
        );
        assert_eq!(
            String::from_utf8(log).unwrap(),
            "Patches: external twin.xml/Twin (1 writes)\n\
             Patches: external twin.xml/Twin (1 writes)\n\
             Patches: external twin.xml/Pair (1 writes)\n"
        );
    }

    #[test]
    fn an_older_builds_patch_file_left_by_an_update_is_not_external() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(LEGACY_PATCH_FILE), EXTERNAL).unwrap();
        fs::write(dir.path().join("extra.xml"), EXTERNAL).unwrap();
        let found = external_patches(dir.path(), "01.02", &spartan(), &["CUSA01623"]);
        let files: HashSet<&str> = found.iter().filter_map(|f| f.0.split('/').next()).collect();
        assert_eq!(files, HashSet::from(["extra.xml"]));
    }

    #[test]
    fn built_in_file_is_not_external() {
        let builtin = spartan();
        let found = external_patches(builtin.parent().unwrap(), "01.02", &builtin, &["CUSA01623"]);
        assert!(found.is_empty());
    }

    // --- GameProfileTests

    #[test]
    fn profiles_by_title_id() {
        let profile = |id| game_profile(id, None).map(|p| (p.xml, p.app_version));
        assert_eq!(profile(Some("CUSA01623")), Some(("CUSA01623.xml", "01.02")));
        // The Asian release.
        assert_eq!(profile(Some("CUSA01740")), Some(("CUSA01623.xml", "01.02")));
        assert_eq!(profile(Some("CUSA99999")), None);
        assert_eq!(profile(None), None);
        assert!(spartan().ends_with("CUSA01623.xml"));
    }

    #[test]
    fn spartanport_defaults_and_extras() {
        let select = |extra: &[&str], version: &str, only: bool| {
            selected_patches(&spartan(), version, &strings(extra), only).unwrap()
        };
        assert_eq!(
            select(&[], "01.02", false),
            [
                "Bug Fix - Texture Corruption Fix",
                "Skip Any Video With X Button",
                "Bug Fix - Startup Black Screen"
            ]
        );
        assert_eq!(
            select(
                &["Resolution Patch - 720p", " Skip Any Video With X Button"],
                "01.02",
                false
            ),
            [
                "Skip Any Video With X Button",
                "Bug Fix - Startup Black Screen",
                "Resolution Patch - 720p"
            ]
        );
        assert!(select(&[], "01.00", false).is_empty());
        // The launcher's selection is exact: unchecked defaults stay off, except the fixes that
        // are always on (a list saved before they existed must still get them).
        assert_eq!(
            select(&["Skip Any Video With X Button"], "01.02", true),
            [
                "Skip Any Video With X Button",
                "Bug Fix - Startup Black Screen"
            ]
        );
        assert_eq!(
            select(&[""], "01.02", true),
            ["Bug Fix - Startup Black Screen"]
        );
        assert!(select(&[], "01.00", true).is_empty());
    }

    #[test]
    fn spartanport_resolution_patch_replaces_texture_fix() {
        assert_eq!(
            selected_patches(
                &spartan(),
                "01.02",
                &strings(&["Resolution Patch - 4K"]),
                false
            )
            .unwrap(),
            [
                "Skip Any Video With X Button",
                "Bug Fix - Startup Black Screen",
                "Resolution Patch - 4K"
            ]
        );
        let two = strings(&["Resolution Patch - 4K", "Resolution Patch - 720p"]);
        match selected_patches(&spartan(), "01.02", &two, false) {
            Err(PatchError::Invalid(text)) => assert_eq!(
                text,
                "choose one resolution patch, not ['Resolution Patch - 4K', 'Resolution Patch - 720p']"
            ),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn profile_by_title_for_any_region() {
        assert_eq!(
            game_profile(Some("CUSA01715"), None).map(|p| p.xml),
            Some("CUSA01623.xml")
        );
        // The game's own title (param.sfo TITLE), with or without the (R) and TM marks: this is
        // what found the patches of a release under an ID that is not listed.
        for title in [
            "God of War\u{ae} III Remastered",
            "God of War\u{2122} III Remastered",
            "God of War III Remastered",
        ] {
            assert!(
                game_profile(Some("CUSA99999"), Some(title)).is_some(),
                "{title}"
            );
        }
        // The project name is not a game title.
        assert!(game_profile(Some("CUSA99999"), Some("SpartanPort")).is_none());
        assert!(game_profile(Some("CUSA99999"), Some("Another Game")).is_none());
        let dir = tempfile::tempdir().unwrap();
        let game = game_folder(
            dir.path(),
            &[("TITLE_ID", "CUSA77777"), ("TITLE", "God of War III")],
        );
        assert!(game_patchable(Some(&game)));
        assert!(!game_patchable(None));
        // No param.sfo: unknown game.
        let empty = dir.path().join("empty");
        fs::create_dir_all(&empty).unwrap();
        assert!(!game_patchable(Some(&empty)));
    }

    // --- SignatureTests

    const SIGNATURES: &str = r#"<?xml version="1.0"?>
<Patch>
  <Metadata Name="Found" AppVer="01.02">
    <PatchList><Line Type="bytes" Address="0x401004" Value="9090" Pattern="11 22 ?? 44 55 66" At="1"/></PatchList>
  </Metadata>
  <Metadata Name="Call" AppVer="01.02">
    <PatchList><Line Type="bytes" Address="0x401010" Value="e8{rel:there:0}" Pattern="aa bb cc dd ee" At="0"/></PatchList>
    <Target Name="there" Address="0x401100" Pattern="e9 ?? ?? ?? ?? 77 88" At="1" Ref="1"/>
  </Metadata>
  <Metadata Name="Missing" AppVer="01.02">
    <PatchList>
      <Line Type="bytes" Address="0x401004" Value="9090" Pattern="11 22 ?? 44 55 66" At="1"/>
      <Line Type="bytes" Address="0x401020" Value="90" Pattern="de ad be ef" At="0"/>
    </PatchList>
  </Metadata>
  <Metadata Name="Data" AppVer="01.02">
    <PatchList><Line Type="bytes32" Address="0x402000" Target="variable" Value="0x2"/></PatchList>
    <Target Name="variable" Address="0x402000" Pattern="44 89 35 ?? ?? ?? ?? c6" At="3" Ref="1"/>
  </Metadata>
</Patch>"#;

    fn signature_xml(dir: &Path) -> PathBuf {
        let path = dir.join("test.xml");
        fs::write(&path, SIGNATURES).unwrap();
        path
    }

    /// The image of the Python's `SignatureTests.setUp`: each signature planted once.
    fn signature_image() -> Vec<u8> {
        let mut image = vec![0u8; 0x400];
        image[0x40..0x46].copy_from_slice(&[0x11, 0x22, 0x33, 0x44, 0x55, 0x66]);
        image[0x80..0x85].copy_from_slice(&[0xaa, 0xbb, 0xcc, 0xdd, 0xee]);
        image[0x200] = 0xe9;
        image[0x201..0x205].copy_from_slice(&(0x300i32 - 0x205).to_le_bytes());
        image[0x205..0x207].copy_from_slice(&[0x77, 0x88]);
        // mov [rip+disp], r14d storing to 0x3f0, then the c6 of the next instruction.
        image[0x280..0x283].copy_from_slice(&[0x44, 0x89, 0x35]);
        image[0x283..0x287].copy_from_slice(&(0x3f0i32 - 0x287).to_le_bytes());
        image[0x287] = 0xc6;
        image
    }

    #[test]
    fn sites_found_anywhere_and_rel32_recomputed() {
        let dir = tempfile::tempdir().unwrap();
        let xml = signature_xml(dir.path());
        let image = signature_image();
        let (writes, skipped) = compile(
            &xml,
            &["Found", "Call", "Missing"],
            None,
            0x400,
            Some(&image),
        );
        let mut call = vec![0xe8];
        call.extend((0x300i32 - (0x80 + 5)).to_le_bytes());
        assert_eq!(pairs(&writes), [(0x41, vec![0x90, 0x90]), (0x80, call)]);
        // One site of the patch is missing: the whole patch is left out, with the reason.
        assert_eq!(skipped, reasons(&[("Missing", "signature not found")]));
    }

    #[test]
    fn ambiguous_signature_is_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let xml = signature_xml(dir.path());
        let mut image = signature_image();
        image.copy_within(0x40..0x46, 0x100);
        let (writes, skipped) = compile(&xml, &["Found"], None, 0x400, Some(&image));
        assert!(writes.is_empty());
        assert_eq!(
            skipped,
            reasons(&[("Found", "signature found more than once")])
        );
    }

    #[test]
    fn without_the_image_the_reference_addresses_are_used() {
        let dir = tempfile::tempdir().unwrap();
        let xml = signature_xml(dir.path());
        let (writes, _) = compile(&xml, &["Found", "Call"], Some("01.02"), 0x10000, None);
        assert_eq!(pairs(&writes)[0], (0x1004, vec![0x90, 0x90]));
        assert_eq!(writes[1].address, 0x1010);
    }

    #[test]
    fn data_line_writes_at_its_target() {
        // The variable has no bytes of its own to find: the code that stores to it points at it.
        let dir = tempfile::tempdir().unwrap();
        let xml = signature_xml(dir.path());
        let image = signature_image();
        let two = 2u32.to_le_bytes().to_vec();
        let (writes, _) = compile(&xml, &["Data"], None, 0x400, Some(&image));
        assert_eq!(pairs(&writes), [(0x3f0, two.clone())]);
        let (writes, _) = compile(&xml, &["Data"], Some("01.02"), 0x10000, None);
        assert_eq!(pairs(&writes), [(0x2000, two)]);
        // Store not found: the whole patch is left out.
        let mut image = image;
        image[0x280..0x288].fill(0);
        let (writes, skipped) = compile(&xml, &["Data"], None, 0x400, Some(&image));
        assert!(writes.is_empty());
        assert_eq!(skipped, reasons(&[("Data", "signature not found")]));
    }

    /// Bytes `at..at + n` of a signature, `None` for `??`.
    fn fixed(pattern: &str, at: usize, n: usize) -> Vec<Option<u8>> {
        pattern
            .split_whitespace()
            .map(|b| (b != "??").then(|| u8::from_str_radix(b, 16).unwrap()))
            .skip(at)
            .take(n)
            .collect()
    }

    /// A C integer of `cheat_hook.h` (`0xc4`, `0`).
    fn c_int(text: &str) -> u64 {
        let text = text.trim();
        match text.strip_prefix("0x") {
            Some(hex) => u64::from_str_radix(hex, 16).unwrap(),
            None => text.parse().unwrap(),
        }
    }

    /// The hooks of `src/cheat_hook.h` as the Python test's regular expression reads them:
    /// offset, original bytes, signature and its At.
    fn cheat_hooks(header: &str) -> Vec<(u64, Vec<u8>, String, usize)> {
        let mut hooks = Vec::new();
        for (start, _) in header.match_indices("{0x") {
            let entry = &header[start + 1..];
            let Some((offset, rest)) = entry.split_once(',') else {
                continue;
            };
            let Some(rest) = rest.strip_prefix("SPARTAN_CHEAT_") else {
                continue;
            };
            let mut fields = rest.splitn(4, ',');
            let (Some(id), Some(length), Some(replacement), Some(rest)) =
                (fields.next(), fields.next(), fields.next(), fields.next())
            else {
                continue;
            };
            let word = id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
            if !word || length.parse::<u32>().is_err() || replacement.parse::<u32>().is_err() {
                continue;
            }
            let Some(rest) = rest.trim_start().strip_prefix('{') else {
                continue;
            };
            let Some((original, rest)) = rest.split_once("},") else {
                continue;
            };
            let Some((_, rest)) = rest.split_once("{\"") else {
                continue;
            };
            let Some((pattern, rest)) = rest.split_once("\",") else {
                continue;
            };
            let Some((at, _)) = rest.split_once("}}") else {
                continue;
            };
            let original = original.split(',').map(|b| c_int(b) as u8).collect();
            hooks.push((
                c_int(offset),
                original,
                pattern.to_string(),
                at.trim().parse().unwrap(),
            ));
        }
        hooks
    }

    #[test]
    fn signatures_match_the_full_update_eboot() {
        // Without that eboot.bin here: the original bytes others published for it must be the
        // fixed bytes of each signature at its site, with one shift per code region.
        let known: Value = serde_json::from_str(
            &fs::read_to_string(repo("tests/data/eboot_6e72_sites.json")).unwrap(),
        )
        .unwrap();
        let text = fs::read_to_string(spartan()).unwrap();
        let doc = parse(&text).unwrap();
        let metas: HashMap<&str, Vec<roxmltree::Node>> = metadata(&doc)
            .map(|m| {
                let lines = m.descendants().filter(|n| n.has_tag_name("Line"));
                (m.attribute("Name").unwrap(), lines.collect())
            })
            .collect();
        let mut shifts: HashMap<i128, HashSet<i128>> = HashMap::new();
        for (name, sites) in known["patches"].as_object().unwrap() {
            for site in sites.as_array().unwrap() {
                let index = site[0].as_u64().unwrap() as usize;
                let address = py_int(site[1].as_str().unwrap(), 0).unwrap();
                let original = hex_bytes(site[2].as_str().unwrap());
                let line = metas[name.as_str()][index];
                let at = line.attribute("At").unwrap().parse().unwrap();
                let ours = fixed(line.attribute("Pattern").unwrap(), at, original.len());
                assert!(
                    ours.iter()
                        .zip(&original)
                        .all(|(b, o)| b.is_none_or(|b| b == *o)),
                    "{name} line {index}"
                );
                let reference = py_int(line.attribute("Address").unwrap(), 0).unwrap();
                shifts
                    .entry(reference >> 16)
                    .or_default()
                    .insert(address - reference);
            }
        }
        assert!(shifts.values().all(|s| s.len() == 1), "{shifts:?}");
        let hooks = cheat_hooks(&fs::read_to_string(repo("src/cheat_hook.h")).unwrap());
        for (name, cheat) in known["cheats"].as_object().unwrap() {
            let address = c_int(cheat[0].as_str().unwrap());
            let original = hex_bytes(cheat[1].as_str().unwrap());
            // The cheats' region moved -0x10 (Rage of Sparta has two sites with the same bytes).
            let matching: Vec<_> = hooks
                .iter()
                .filter(|h| h.1 == original && h.0.checked_sub(0x10) == Some(address))
                .collect();
            assert_eq!(matching.len(), 1, "{name}");
            let (_, _, pattern, at) = matching[0];
            let all: Vec<Option<u8>> = original.iter().map(|&b| Some(b)).collect();
            assert_eq!(fixed(pattern, *at, original.len()), all, "{name}");
        }
    }

    #[test]
    fn spartanport_patches_compile() {
        let names = metadata_names(&spartan());
        let (writes, skipped) =
            compile_patches(&spartan(), &names, Some("01.02"), &segments(ALL), None).unwrap();
        assert!(!writes.is_empty());
        // The Python's call without `skipped` raises on any lookup: no Target= is dangling.
        assert!(skipped.is_empty(), "{skipped:?}");
    }

    #[test]
    fn spartanport_startup_black_screen() {
        // The frame index variable (.bss) starts at 2, the index before the first frame's 0.
        let (writes, _) = compile(
            &spartan(),
            &["Bug Fix - Startup Black Screen"],
            Some("01.02"),
            ALL,
            None,
        );
        assert_eq!(pairs(&writes), [(0x14b_a600, 2u32.to_le_bytes().to_vec())]);
    }

    #[test]
    fn spartanport_skip_intro_targets() {
        // Ported from the other 01.02 build: every jump must land where this eboot has the code.
        let (writes, _) = compile(&spartan(), &["Skip Intro"], Some("01.02"), ALL, None);
        let writes = by_address(&writes);
        // jne over the logo start
        assert_eq!(writes[&0x18_4950], [0xeb, 0x34]);
        // mov [flag], 1; mov eax, 1; ret
        let getter = &writes[&0x0c_8440];
        assert_eq!(getter.len(), 16);
        assert_eq!(rel(getter, 2, 0x4c_844a), 0x1e7_9258);
        // The boot call now goes through the cave.
        let call = &writes[&0x01_b36a];
        assert_eq!(rel(call, 1, 0x41_b36f), 0x61_2170);
        // mov [flag], 1; jmp to the original callee
        let cave = &writes[&0x21_2170];
        assert_eq!(cave.len(), 15);
        assert_eq!(rel(cave, 2, 0x61_217a), 0x1e7_9258);
        assert_eq!(rel(cave, 11, 0x61_217f), 0x4b_4c90);
    }

    #[test]
    fn the_8k_patch_dimensions_and_memory() {
        let name = "Resolution Patch - 8K";
        let (writes, _) = compile(&spartan(), &[name], Some("01.02"), ALL, None);
        let expected: HashMap<u64, Vec<u8>> = [
            (0x55a876, 0x9090, 2),
            (0x5f0b3b, 7680, 4),
            (0x5f0b43, 4320, 4),
            (0x5f0b4c, 7680, 2),
            (0x5f0b53, 4320, 2),
            (0x5643f3, 4320, 4),
            (0x564400, 7680, 4),
            (0x5ff769, 7680, 4),
            (0x5ff76f, 4320, 4),
            (0x7c46b4, 7680, 4),
            (0x7c46b8, 4320, 4),
            (0x56bb82, 4320, 4),
            (0x60953d, 4320, 4),
            (0x7f2a3c, 3840, 4),
            (0x7f2a40, 2160, 4),
            (0x7cd918, 0xff00_0000, 4),
        ]
        .into_iter()
        .map(|(address, value, size): (u64, u64, usize)| {
            (address - 0x40_0000, value.to_le_bytes()[..size].to_vec())
        })
        .collect();
        assert_eq!(by_address(&writes), expected);
        assert_eq!(
            patch_requirements(&spartan(), &strings(&[name]), "01.02").unwrap(),
            Requirements {
                dmem_mb: Some(5056 + 6144),
                vblank_hz: None
            }
        );
    }

    #[test]
    fn requirements_from_patch_notes() {
        // Notes ask for extra direct memory (shadPS4 "DMEM") and a VBlank rate.
        let needs = |names: &[&str]| {
            let found = patch_requirements(&spartan(), &strings(names), "01.02").unwrap();
            (found.dmem_mb, found.vblank_hz)
        };
        assert_eq!(
            needs(&["Resolution Patch - 4K", "Skip Any Video With X Button"]),
            (Some(5056 + 6144), None)
        );
        assert_eq!(needs(&["Frame Rate Patch - 120 FPS"]), (None, Some(120)));
        assert_eq!(needs(&["Frame Rate Patch - 144 FPS"]), (None, Some(144)));
        assert_eq!(needs(&[]), (None, None));
    }

    #[test]
    fn the_144_frame_rate_patch_has_the_reference_sites_and_value() {
        let text = fs::read_to_string(spartan()).unwrap();
        let doc = parse(&text).unwrap();
        let meta = metadata(&doc)
            .find(|m| m.attribute("Name") == Some("Frame Rate Patch - 144 FPS"))
            .unwrap();
        let list = meta
            .children()
            .find(|n| n.has_tag_name("PatchList"))
            .unwrap();
        let lines: Vec<_> = list.children().filter(|n| n.is_element()).collect();
        let addresses: Vec<_> = lines.iter().map(|l| l.attribute("Address")).collect();
        assert_eq!(addresses, [Some("0x0048ff1a"), Some("0x0075dbe4")]);
        assert_eq!(lines[1].attribute("Value"), Some("00001043"));
        assert!(meta
            .attribute("Note")
            .unwrap()
            .contains("VBlank to 144 FPS"));
    }

    // --- ModPatchTests

    const MOD: &str = r#"<?xml version="1.0"?>
<Patch>
  <TitleID><ID>CUSA01623</ID></TitleID>
  <Metadata Name="Resolution Patch - 3440x1440" Note="Requires setting DMEM to 4608 MB or more." AppVer="01.02" AppElf="eboot.bin" isEnabled="false">
    <PatchList><Line Type="bytes" Address="0x00401000" Value="9090"/></PatchList>
  </Metadata>
  <Metadata Name="Resolution Patch - 2560x1080" AppVer="01.02" AppElf="eboot.bin" isEnabled="false">
    <PatchList><Line Type="bytes" Address="0x00401000" Value="9090"/></PatchList>
  </Metadata>
  <Metadata Name="Skip Any Video With X Button" Note="replaced by the mod" AppVer="01.02" AppElf="eboot.bin" isEnabled="true">
    <PatchList><Line Type="bytes" Address="0x00401002" Value="90"/></PatchList>
  </Metadata>
</Patch>"#;

    const OTHER_GAME: &str = r#"<Patch><TitleID><ID>CUSA99999</ID></TitleID><Metadata Name="Elsewhere" AppVer="01.02"><PatchList/></Metadata></Patch>"#;

    fn mod_file(dir: &Path) -> PathBuf {
        let path = dir.join("mod.xml");
        fs::write(&path, MOD).unwrap();
        path
    }

    #[test]
    fn resolution_names_of_patch_files_smallest_first() {
        let dir = tempfile::tempdir().unwrap();
        let path = mod_file(dir.path());
        assert_eq!(resolution_names(&[path]), ["2560x1080", "3440x1440"]);
        assert!(resolution_names(&[dir.path().join("missing.xml")]).is_empty());
    }

    #[test]
    fn merge_adds_replaces_and_skips_other_titles() {
        let dir = tempfile::tempdir().unwrap();
        let other = dir.path().join("other.xml");
        fs::write(&other, OTHER_GAME).unwrap();
        let merged = dir.path().join("out").join("merged.xml");
        merge_patch_files(
            &spartan(),
            &[mod_file(dir.path()), other],
            &merged,
            &mut Vec::new(),
        )
        .unwrap();
        let names = metadata_names(&merged);
        assert!(names.iter().any(|n| n == "Resolution Patch - 3440x1440"));
        assert!(!names.iter().any(|n| n == "Elsewhere"));
        // Replaced, not repeated.
        let skip = "Skip Any Video With X Button";
        assert_eq!(names.iter().filter(|n| *n == skip).count(), 1);
        let text = fs::read_to_string(&merged).unwrap();
        let doc = parse(&text).unwrap();
        let note = metadata(&doc)
            .find(|m| m.attribute("Name") == Some(skip))
            .and_then(|m| m.attribute("Note"));
        assert_eq!(note, Some("replaced by the mod"));
        // The built-in patches are all still there, and the merged file is selectable like the
        // original.
        for name in metadata_names(&spartan()) {
            assert!(name == skip || names.contains(&name), "{name}");
        }
        let chosen = selected_patches(
            &merged,
            "01.02",
            &strings(&["Resolution Patch - 3440x1440"]),
            true,
        )
        .unwrap();
        // (the startup fix is always added)
        assert!(chosen.iter().any(|n| n == "Resolution Patch - 3440x1440"));
        assert!(!chosen.iter().any(|n| n == "Resolution Patch - 2560x1080"));
        assert_eq!(
            patch_requirements(
                &merged,
                &strings(&["Resolution Patch - 3440x1440"]),
                "01.02"
            )
            .unwrap()
            .dmem_mb,
            Some(5056 + 4608)
        );
    }

    #[test]
    fn a_broken_mod_file_is_left_out() {
        let dir = tempfile::tempdir().unwrap();
        let broken = dir.path().join("broken.xml");
        fs::write(&broken, "<Patch><Metadata").unwrap();
        let merged = dir.path().join("merged.xml");
        let mut log = Vec::new();
        merge_patch_files(
            &spartan(),
            &[broken, mod_file(dir.path())],
            &merged,
            &mut log,
        )
        .unwrap();
        assert!(metadata_names(&merged)
            .iter()
            .any(|n| n == "Resolution Patch - 2560x1080"));
        assert!(String::from_utf8(log)
            .unwrap()
            .starts_with("Patches: broken.xml: "));
    }

    #[test]
    fn default_enabled_patches_of_a_mod() {
        let dir = tempfile::tempdir().unwrap();
        let path = mod_file(dir.path());
        assert_eq!(
            default_enabled(std::slice::from_ref(&path), "01.02"),
            ["Skip Any Video With X Button"]
        );
        assert!(default_enabled(&[path], "01.00").is_empty());
    }

    /// The Ultrawide mod's sizes: (width, height) -> (extra direct memory MiB, k).
    fn sizes() -> [((u32, u32), (u64, f64)); 5] {
        [
            ((2560, 1080), (3072, (2560.0 / 1080.0) / (16.0 / 9.0))),
            ((3440, 1440), (4608, (3440.0 / 1440.0) / (16.0 / 9.0))),
            ((3840, 1600), (6144, (3840.0 / 1600.0) / (16.0 / 9.0))),
            ((5120, 1440), (6144, 2.0)),
            ((5120, 2160), (6144, (5120.0 / 2160.0) / (16.0 / 9.0))),
        ]
    }

    /// 16:10 and 4:3, taller than 16:9: only the resolution and the aspect ratio (the camera's
    /// fixed horizontal view already shows more at the top and bottom), so no field of view
    /// change.
    const TALLER: [((u32, u32), u64); 10] = [
        ((1920, 1200), 3072),
        ((2560, 1600), 4608),
        ((1280, 800), 3072),
        ((1440, 900), 3072),
        ((1360, 768), 3072),
        ((1280, 960), 3072),
        ((1440, 1080), 3072),
        ((1920, 1440), 3072),
        ((2048, 1536), 3072),
        ((2560, 1920), 4608),
    ];

    #[test]
    fn the_ultrawide_mod_lists_its_resolutions_and_memory_needs() {
        assert_eq!(
            resolution_names(&[ultrawide()]),
            [
                "1280x800",
                "1360x768",
                "1280x960",
                "1440x900",
                "1440x1080",
                "1920x1200",
                "1920x1440",
                "2560x1080",
                "2048x1536",
                "2560x1600",
                "2560x1920",
                "3440x1440",
                "3840x1600",
                "5120x1440",
                "5120x2160"
            ]
        );
        let needs = TALLER
            .into_iter()
            .chain(sizes().map(|(size, (dmem, _))| (size, dmem)));
        for ((width, height), dmem) in needs {
            let name = format!("Resolution Patch - {width}x{height}");
            let found =
                patch_requirements(&ultrawide(), std::slice::from_ref(&name), "01.02").unwrap();
            assert_eq!(
                (found.dmem_mb, found.vblank_hz),
                (Some(5056 + dmem), None),
                "{name}"
            );
        }
    }

    #[test]
    fn the_ultrawide_mod_camera_caves_form_closed_chains() {
        // Each entry (its Address values are the 6e72 build's): the four camera sites jump into
        // two chained caves. The first runs the instruction the jump replaced, the second
        // multiplies the field of view tangent by k = (width / height) / (16 / 9) and returns
        // right after the site.
        let sites = [
            (0x1cc01f, "c5fa1089d8000000"),
            (0x1cbd89, "c5fa1080d8000000"),
            (0x1ccaa9, "c5fa1080d8000000"),
            (0x1cc20f, "c5fa5e852cffffff"),
        ];
        for ((width, height), (_, k)) in sizes() {
            let name = format!("Resolution Patch - {width}x{height}");
            let (writes, _) = compile(&ultrawide(), &[name.as_str()], Some("01.02"), ALL, None);
            let writes = by_address(&writes);
            // The aspect ratio.
            let aspect = (f64::from(width) / f64::from(height)) as f32;
            assert_eq!(
                writes[&0x1f_0c77],
                [aspect.to_le_bytes(); 2].concat(),
                "{name}"
            );
            let k = (k as f32).to_le_bytes();
            let constant: Vec<u64> = writes
                .iter()
                .filter(|(_, data)| data[..] == k[..])
                .map(|(&at, _)| at)
                .collect();
            assert_eq!(constant.len(), 1, "{name}");
            for (site, original) in sites {
                let jump = &writes[&site];
                // jmp + a 3-byte nop
                assert_eq!(
                    (jump[0], &jump[5..]),
                    (0xe9, &[0x0f, 0x1f, 0x00][..]),
                    "{name}"
                );
                let first = rel(jump, 1, site + 5);
                assert_eq!(hex(&writes[&first][..8]), original, "{name}");
                assert_eq!(writes[&first][8], 0xe9, "{name}");
                let second = rel(&writes[&first], 9, first + 13);
                let cave = &writes[&second];
                // vmulss ..., [rip+k]; jmp
                assert_eq!((cave[0], cave[2], cave[8]), (0xc5, 0x59, 0xe9), "{name}");
                assert_eq!(rel(cave, 4, second + 8), constant[0], "{name}");
                assert_eq!(rel(cave, 9, second + 13), site + 8, "{name}");
            }
        }
    }

    #[test]
    fn taller_than_16_9_changes_no_camera() {
        for ((width, height), _) in TALLER {
            let name = format!("Resolution Patch - {width}x{height}");
            let (writes, _) = compile(&ultrawide(), &[name.as_str()], Some("01.02"), ALL, None);
            let writes = by_address(&writes);
            let aspect = (f64::from(width) / f64::from(height)) as f32;
            // The aspect ratio, twice; for 4:3 the second float is one unit above 4/3, which the
            // game's letterbox test (function at 0x56fc60, "== 4/3") does not match.
            let second = if aspect.to_bits() == ((4.0f64 / 3.0) as f32).to_bits() {
                f32::from_bits(aspect.to_bits() + 1)
            } else {
                aspect
            };
            let pair = [aspect.to_le_bytes(), second.to_le_bytes()].concat();
            assert_eq!(writes[&0x1f_0c77], pair, "{name}");
            // The resolution (these lines keep the built-in entries' Address values, the
            // reference build's).
            assert_eq!(writes[&0x1f_0b3b], width.to_le_bytes(), "{name}");
            assert_eq!(writes[&0x1f_0b43], height.to_le_bytes(), "{name}");
            // No camera site, no cave.
            for site in [0x1cc01f, 0x1cbd89, 0x1ccaa9, 0x1cc20f] {
                assert!(!writes.contains_key(&site), "{name}");
            }
            // The resolution lines (15) and the aspect ratio.
            assert_eq!(writes.len(), 15 + 1, "{name}");
        }
    }

    #[test]
    fn the_ultrawide_mod_is_off_until_a_resolution_is_chosen() {
        assert!(default_enabled(&[ultrawide()], "01.02").is_empty());
    }

    // --- HalfResolutionTests

    #[test]
    fn every_resolution_patch_halves_the_output() {
        // The half-resolution post-processing buffer (a 960x540 table at 0x7f2a3c) must be half
        // of the patched output, or a smaller output gets a larger buffer and overexposed lights
        // (#390).
        let size = |value: &str| {
            let bytes = hex_bytes(value);
            bytes
                .iter()
                .rev()
                .fold(0u64, |total, &b| (total << 8) | u64::from(b))
        };
        let text = fs::read_to_string(spartan()).unwrap();
        let doc = parse(&text).unwrap();
        for meta in metadata(&doc) {
            let name = meta.attribute("Name").unwrap();
            if !name.starts_with("Resolution Patch") {
                continue;
            }
            let lines: HashMap<&str, &str> = meta
                .descendants()
                .filter(|n| n.has_tag_name("Line"))
                .map(|l| {
                    (
                        l.attribute("Address").unwrap(),
                        l.attribute("Value").unwrap(),
                    )
                })
                .collect();
            let (width, height) = (size(lines["0x005f0b3b"]), size(lines["0x005f0b43"]));
            let half = (lines.get("0x007f2a3c"), lines.get("0x007f2a40"));
            let (Some(half_width), Some(half_height)) = half else {
                panic!("{name}: no half-resolution lines");
            };
            assert_eq!(
                (size(half_width), size(half_height)),
                (width / 2, height / 2),
                "{name}"
            );
        }
    }

    // --- The writers' formats and the pieces the Python tests reach only through the above

    #[test]
    fn find_pattern_returns_at_most_limit_sites_in_order() {
        let image = [1, 2, 3, 9, 1, 2, 3, 8, 1, 2, 3, 9, 1, 2];
        let pattern = parse_pattern("01 02 03 ??").unwrap();
        assert_eq!(find_pattern(&image, &pattern, 2), [0, 4]);
        // The last "01 02" has no room for the rest of the signature.
        assert_eq!(find_pattern(&image, &pattern, 5), [0, 4, 8]);
        assert_eq!(
            find_pattern(&image, &parse_pattern("?? 03 09").unwrap(), 5),
            [1, 9]
        );
        // A site would start before the image.
        assert_eq!(
            find_pattern(&image, &parse_pattern("?? ?? 01 02").unwrap(), 5),
            [2, 6, 10]
        );
        assert!(find_pattern(&image, &parse_pattern("07").unwrap(), 2).is_empty());
        assert!(matches!(
            parse_pattern("?? ??"),
            Err(PatchError::Invalid(text)) if text == "pattern without fixed bytes"
        ));
        assert!(parse_pattern("11 zz").is_err());
        assert!(parse_pattern("1ff").is_err());
    }

    #[test]
    fn encode_packs_like_struct_pack() {
        let encoded = |kind: Option<&str>, value: &str| {
            let kind = kind.map(|k| format!(" Type=\"{k}\"")).unwrap_or_default();
            let text = format!("<Line{kind} Value=\"{value}\"/>");
            let doc = parse(&text).unwrap();
            encode(doc.root_element())
        };
        let ok = |kind: &str, value: &str| hex(&encoded(Some(kind), value).unwrap());
        assert_eq!(ok("bytes", "90 90 c3"), "9090c3");
        assert_eq!(ok("bytes16", "0x1234"), "3412");
        assert_eq!(ok("bytes32", "16"), "10000000");
        assert_eq!(ok("bytes64", "0xffffffffffffffff"), "ffffffffffffffff");
        // struct.pack('<f'/'<d', float(value)) in the Python.
        assert_eq!(ok("float32", "1.5"), "0000c03f");
        assert_eq!(ok("float32", "0.1"), "cdcccc3d");
        assert_eq!(ok("float32", "-2.75"), "000030c0");
        assert_eq!(ok("float32", "inf"), "0000807f");
        assert_eq!(ok("float32", "nan"), "0000c07f");
        assert_eq!(ok("float32", "1e-50"), "00000000");
        assert_eq!(ok("float64", "0.1"), "9a9999999999b93f");
        assert_eq!(ok("float64", "1e-50"), "1fb8d44a7aee8d35");
        assert_eq!(ok("utf8", "H\u{e9}"), "48c3a900");
        assert_eq!(ok("utf16", "H\u{e9}\u{1f600}"), "4800e9003dd800de0000");
        for (kind, value) in [
            ("bytes16", "0x10000"),
            ("bytes32", "-1"),
            ("bytes32", "two"),
            ("float32", "1e40"),
            ("bytes", "9"),
        ] {
            assert!(encoded(Some(kind), value).is_err(), "{kind} {value}");
        }
        let unsupported = |kind| match encoded(kind, "90") {
            Err(PatchError::Invalid(text)) => text,
            other => panic!("{other:?}"),
        };
        assert_eq!(unsupported(Some("mask")), "unsupported patch type 'mask'");
        assert_eq!(unsupported(None), "unsupported patch type None");
    }

    #[test]
    fn fill_measures_each_rel32_from_the_end_of_its_instruction() {
        let targets = Targets::from([("flag".to_string(), 0x1000), ("callee".to_string(), 0x10)]);
        // mov [rip+flag], 1 (a 4-byte immediate after the operand); jmp callee
        let bytes = fill(
            "c7 05 {rel:flag:4} 01000000 e9{rel:callee:0}",
            0x100,
            &targets,
        )
        .unwrap();
        let mut expected = vec![0xc7, 0x05];
        expected.extend((0x1000i32 - (0x100 + 2 + 4 + 4)).to_le_bytes());
        expected.extend([1, 0, 0, 0, 0xe9]);
        expected.extend((0x10i32 - (0x100 + 11 + 4)).to_le_bytes());
        assert_eq!(bytes, expected);
        for value in [
            "e8{rel:nowhere:0}",
            "e8{rel:flag}",
            "e8{rel:flag:x}",
            "e8{rel:flag:0",
            "e",
        ] {
            assert!(fill(value, 0, &targets).is_err(), "{value}");
        }
        let far = Targets::from([("far".to_string(), 0x1_0000_0000)]);
        assert!(fill("e8{rel:far:0}", 0, &far).is_err());
    }

    #[test]
    fn write_patches_writes_the_g3patch2_layout() {
        let dir = tempfile::tempdir().unwrap();
        let writes = [
            Write {
                address: 0x41,
                bytes: vec![0x90, 0x90],
            },
            Write {
                address: 0x3f0,
                bytes: vec![2, 0, 0, 0],
            },
        ];
        write_patches(dir.path(), &writes).unwrap();
        let mut expected = b"G3PATCH2".to_vec();
        for value in [0x40_0000u64, 2, 0x41, 2] {
            expected.extend(value.to_le_bytes());
        }
        expected.extend([0x90, 0x90]);
        for value in [0x3f0u64, 4] {
            expected.extend(value.to_le_bytes());
        }
        expected.extend([2, 0, 0, 0]);
        assert_eq!(fs::read(dir.path().join("patches.bin")).unwrap(), expected);
    }

    #[test]
    fn write_status_is_pythons_json_dumps() {
        let dir = tempfile::tempdir().unwrap();
        let mut status = PatchStatus {
            applied: strings(&["A", "\u{dc}n\u{ef} \u{1f600}", "q\"\\"]),
            skipped: reasons(&[("C\u{7f}", "signature not found\n")]),
            ..PatchStatus::default()
        };
        write_status(dir.path(), &status).unwrap();
        let path = dir.path().join("patch_status.json");
        // json.dumps(...) of the Python, ensure_ascii and its ", " and ": " separators.
        let escape = |hex: &str| format!("{}u{hex}", '\\');
        let expected = format!(
            r#"{{"sha256": null, "applied": ["A", "{}n{} {}{}", "q\"\\"], "skipped": {{"C{}": "signature not found\n"}}}}"#,
            escape("00dc"),
            escape("00ef"),
            escape("d83d"),
            escape("de00"),
            escape("007f")
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), expected);
        assert_eq!(patch_status(dir.path()), Some(status.clone()));
        // The launcher's own keys come after the Python's, and only when set.
        status.sha256 = Some("ab".to_string());
        status.engine_fps = Some(120);
        status.dmem_mb = Some(11200);
        status.vblank_hz = Some(144);
        write_status(dir.path(), &status).unwrap();
        assert!(fs::read_to_string(&path)
            .unwrap()
            .ends_with(r#"}, "engine_fps": 120, "dmem_mb": 11200, "vblank_hz": 144}"#));
        assert_eq!(patch_status(dir.path()), Some(status));
    }

    #[test]
    fn merge_writes_what_elementtree_writes() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().join("base.xml");
        fs::write(
            &base,
            r#"<?xml version="1.0"?>
<!-- before the root -->
<Patch>
    <TitleID>
        <ID>CUSA01623</ID>
    </TitleID>
    <!-- between elements -->
    <Metadata Name="Keep &amp; &lt;go&gt;" Note="a &quot;quoted&quot;&#10;line&#9;tab &#13;" AppVer="01.02">
        <PatchList>
            <Line Type="bytes" Address="0x401000" Value="90"/>
        </PatchList>
        <Empty>  </Empty>
        <Text>a &lt; b &amp; c &gt; d</Text>
        <Mixed>head<!-- c -->more<B/>tail</Mixed>
    </Metadata>
    <Metadata Name="Replaced" AppVer="01.02"><PatchList/></Metadata>
</Patch>
"#,
        )
        .unwrap();
        let folder = dir.path().join("Thing").join("patches");
        fs::create_dir_all(&folder).unwrap();
        let extra = folder.join("mod.xml");
        fs::write(
            &extra,
            "<Patch><TitleID><ID>CUSA01623</ID></TitleID>\n\
             <Metadata Name=\"Replaced\" AppVer=\"01.02\" Note=\"new \u{fc}\u{2122}\"><PatchList><Line Type=\"bytes\" Address=\"0x401001\" Value=\"91\"/></PatchList></Metadata>\n\
             <Metadata Name=\"Added\" AppVer=\"01.02\"/>\n\
             </Patch>",
        )
        .unwrap();
        let out = dir.path().join("out").join("merged.xml");
        let mut log = Vec::new();
        merge_patch_files(&base, &[extra], &out, &mut log).unwrap();
        // What the Python wrote for these two files (its CRLF of Windows' text mode as LF).
        let expected = "<?xml version='1.0' encoding='utf-8'?>\n<Patch>\n  <TitleID>\n    \
            <ID>CUSA01623</ID>\n  </TitleID>\n  <Metadata Name=\"Keep &amp; &lt;go&gt;\" \
            Note=\"a &quot;quoted&quot;&#10;line&#09;tab &#13;\" AppVer=\"01.02\">\n    \
            <PatchList>\n      <Line Type=\"bytes\" Address=\"0x401000\" Value=\"90\" />\n    \
            </PatchList>\n    <Empty>  </Empty>\n    <Text>a &lt; b &amp; c &gt; d</Text>\n    \
            <Mixed>headmore<B />tail</Mixed>\n  </Metadata>\n  <Metadata Name=\"Replaced\" \
            AppVer=\"01.02\" Note=\"new \u{fc}\u{2122}\">\n    <PatchList>\n      <Line \
            Type=\"bytes\" Address=\"0x401001\" Value=\"91\" />\n    </PatchList>\n  </Metadata>\n  \
            <Metadata Name=\"Added\" AppVer=\"01.02\" />\n</Patch>";
        assert_eq!(fs::read_to_string(&out).unwrap(), expected);
        assert_eq!(
            String::from_utf8(log).unwrap(),
            "Patches: Replaced from the mod file Thing/mod.xml\n\
             Patches: Added from the mod file Thing/mod.xml\n"
        );
        // A base the Python cannot parse stops the merge.
        fs::write(&base, "<Patch>").unwrap();
        let result = merge_patch_files(&base, &[], &out, &mut Vec::new());
        assert!(matches!(result, Err(PatchError::Xml(_))));
    }

    /// A minimal ELF: the header, one program header per `(type, offset, vaddr, filesz, memsz)`,
    /// then `data`.
    fn elf(headers: &[(u32, u64, u64, u64, u64)], data: &[u8]) -> Vec<u8> {
        let mut elf = vec![0u8; 0x40];
        elf[..4].copy_from_slice(b"\x7fELF");
        elf[0x20..0x28].copy_from_slice(&0x40u64.to_le_bytes());
        elf[0x36..0x38].copy_from_slice(&56u16.to_le_bytes());
        elf[0x38..0x3a].copy_from_slice(&(headers.len() as u16).to_le_bytes());
        for &(kind, offset, vaddr, filesz, memsz) in headers {
            elf.extend(kind.to_le_bytes());
            elf.extend(5u32.to_le_bytes());
            for value in [offset, vaddr, vaddr, filesz, memsz, 0x1000] {
                elf.extend(value.to_le_bytes());
            }
        }
        elf.extend_from_slice(data);
        elf
    }

    #[test]
    fn eboot_image_places_each_load_at_its_vaddr() {
        // The data starts after the header and three program headers.
        let data_at = 0x40 + 3 * 56;
        let elf = elf(
            &[
                (1, data_at, 0x10, 4, 0x20),
                (2, data_at, 0x100, 4, 4),
                (1, data_at, 0, 2, 2),
            ],
            b"abcd",
        );
        assert_eq!(eboot_segments(&elf).unwrap(), [0x10..0x30, 0..2]);
        let mut expected = vec![0u8; 0x14];
        expected[..2].copy_from_slice(b"ab");
        expected[0x10..].copy_from_slice(b"abcd");
        assert_eq!(eboot_image(&elf).unwrap(), expected);
    }

    #[test]
    fn a_hostile_eboot_elf_gives_errors() {
        assert!(eboot_segments(&[]).is_err());
        assert!(eboot_image(&[0x7f; 0x30]).is_err());
        // No PT_LOAD: segments but no image.
        let none = elf(&[(2, 0, 0, 0, 0)], &[]);
        assert!(eboot_segments(&none).unwrap().is_empty());
        assert!(eboot_image(&none).is_err());
        // Program headers past the end of the file, or at an offset that overflows.
        let mut past = elf(&[], &[]);
        past[0x38..0x3a].copy_from_slice(&0xffffu16.to_le_bytes());
        assert!(eboot_segments(&past).is_err());
        assert!(eboot_image(&past).is_err());
        past[0x20..0x28].copy_from_slice(&u64::MAX.to_le_bytes());
        assert!(eboot_segments(&past).is_err());
        // A load whose end overflows, one past the image limit, one whose bytes are not in the file.
        for (offset, vaddr, filesz, memsz) in [
            (0, u64::MAX, 1, 1),
            (0, 0x2000_0000, 1, 1),
            (0x1000, 0, 0x10, 0x10),
            (u64::MAX, 0, 2, 2),
        ] {
            let hostile = elf(&[(1, offset, vaddr, filesz, memsz)], &[]);
            assert!(eboot_image(&hostile).is_err(), "{offset:#x} {vaddr:#x}");
        }
        assert!(eboot_segments(&elf(&[(1, 0, u64::MAX, 0, 2)], &[])).is_err());
    }

    #[test]
    fn a_hostile_patch_file_gives_errors_or_skips() {
        let dir = tempfile::tempdir().unwrap();
        let image = signature_image();
        let path = dir.path().join("hostile.xml");
        let run = |lines: &str, targets: &str| {
            fs::write(
                &path,
                format!(
                    "<Patch><Metadata Name=\"P\" AppVer=\"01.02\"><PatchList>{lines}</PatchList>{targets}</Metadata></Patch>"
                ),
            )
            .unwrap();
            compile_patches(
                &path,
                &strings(&["P"]),
                None,
                &segments(0x400),
                Some(&image),
            )
        };
        let error = |lines: &str, targets: &str| match run(lines, targets) {
            Err(PatchError::Invalid(text)) => text,
            other => panic!("{lines}{targets}: {other:?}"),
        };
        // What the Python stops on.
        for line in [
            r#"<Line Type="bytes" Value="90"/>"#,
            r#"<Line Type="bytes" Address="0x99999999999999999999" Value="90"/>"#,
            r#"<Line Type="bytes" Address="0x401041" Value="9g"/>"#,
            r#"<Line Type="bytes" Address="0x401041" Value="e8{rel:nowhere:0}"/>"#,
            r#"<Line Type="bytes" Pattern="?? ??" Value="90"/>"#,
            r#"<Line Type="bytes" Pattern="11 zz" Value="90"/>"#,
            r#"<Line Type="bytes" Pattern="11 22" At="x" Value="90"/>"#,
            r#"<Line Type="bytes32" Address="0x401041" Value="0x100000000"/>"#,
            r#"<Line Type="float32" Address="0x401041" Value="1e40"/>"#,
        ] {
            error(line, "");
        }
        assert_eq!(
            error(r#"<Line Type="mask" Address="0x401041" Value="90"/>"#, ""),
            "unsupported patch type 'mask'"
        );
        assert_eq!(
            error(r#"<Line Type="bytes" Address="0x3ff000" Value="90"/>"#, ""),
            "P: address 0x3ff000 is outside the eboot"
        );
        assert_eq!(
            error(
                r#"<Line Type="bytes" Address="0x4003ff" Value="9090"/>"#,
                ""
            ),
            "P: address 0x4003ff is outside the eboot"
        );
        assert_eq!(
            error(
                r#"<Line Type="bytes" Pattern="11 22" At="-0x100" Value="90"/>"#,
                ""
            ),
            "P: address 0x3fff40 is outside the eboot"
        );
        // A rel32 operand read past either end of the image.
        let line = r#"<Line Type="bytes" Address="0x400041" Value="e8{rel:t:0}"/>"#;
        error(
            line,
            r#"<Target Name="t" Pattern="c6" At="0x200" Ref="1"/>"#,
        );
        error(
            line,
            r#"<Target Name="t" Pattern="c6" At="-0x1000" Ref="1"/>"#,
        );
        error(line, r#"<Target Name="t" Line="x"/>"#);
        // A rel32 tail no 64-bit offset holds.
        for tail in [
            "170141183460469231731687303715884105727",
            "-170141183460469231731687303715884105727",
        ] {
            error(
                &format!(r#"<Line Type="bytes" Address="0x400041" Value="e8{{rel:t:{tail}}}"/>"#),
                r#"<Target Name="t" Line="0"/>"#,
            );
        }
        // Lookups the Python leaves out with the reason (its IndexError and KeyError texts).
        let skips = |lines: &str, targets: &str| {
            let (writes, skipped) = run(lines, targets).unwrap();
            assert!(writes.is_empty());
            skipped
        };
        assert_eq!(
            skips(line, r#"<Target Name="t" Line="7"/>"#),
            reasons(&[("P", "list index out of range")])
        );
        assert_eq!(
            skips(
                r#"<Line Type="bytes" Address="0x401041" Target="ghost" Value="90"/>"#,
                ""
            ),
            reasons(&[("P", "'ghost'")])
        );
        // A Line target counts from the end when negative, as a Python index: the call at 0x41
        // to the first line's 0x41.
        let (writes, _) = run(
            &format!(r#"<Line Type="bytes" Address="0x400041" Value="90"/>{line}"#),
            r#"<Target Name="t" Line="-2"/>"#,
        )
        .unwrap();
        assert_eq!(writes[1].bytes, [0xe8, 0xfb, 0xff, 0xff, 0xff]);
        // A patch the file does not have.
        match compile_patches(&path, &strings(&["Nope"]), None, &segments(0x400), None) {
            Err(PatchError::Invalid(text)) => {
                assert_eq!(text, "patches not found for app version None: ['Nope']")
            }
            other => panic!("{other:?}"),
        }
        // Not XML at all.
        fs::write(&path, "<Patch><Metadata").unwrap();
        assert!(compile_patches(&path, &[], None, &[], None).is_err());
        // Nesting deep enough to overflow roxmltree's recursion is refused before it parses,
        // for the read side too.
        let deep = format!("{}{}", "<a>".repeat(100_000), "</a>".repeat(100_000));
        fs::write(&path, deep).unwrap();
        let out = dir.path().join("merged.xml");
        assert!(matches!(
            merge_patch_files(&path, &[], &out, &mut Vec::new()),
            Err(PatchError::Xml(_))
        ));
        assert!(compile_patches(&path, &[], None, &[], None).is_err());
        assert!(game_patch_metas(&path, "01.02").is_err());
        assert!(resolution_names(std::slice::from_ref(&path)).is_empty());
        // What only counts more than roxmltree recurses: comments, quotes, CDATA, empty tags.
        assert_eq!(
            nesting_bound("<a><!-- <b><c> --><b x='>' y=\"<\"/><![CDATA[<d>]]></a>"),
            1
        );
        assert_eq!(
            nesting_bound("<?xml version='1.0'?><a><b><c/></b><b></b></a>"),
            2
        );
        // An entity may expand to markup: every `<` counts.
        let entity = r#"<!DOCTYPE a [<!ENTITY e "<b><b/></b>">]><a>&e;</a>"#;
        assert_eq!(nesting_bound(entity), 7);
        assert!(parse(entity).is_ok());
    }

    #[test]
    fn a_game_without_a_profile_gets_an_empty_patch_file() {
        let dir = tempfile::tempdir().unwrap();
        let game = game_folder(
            dir.path(),
            &[("TITLE_ID", "CUSA99999"), ("TITLE", "Another Game")],
        );
        let out = dir.path().join("out");
        let mut log = Vec::new();
        let job = PatchJob {
            out: &out,
            game: &game,
            builtin_dir: &repo("patches"),
            mod_patches: &[],
            extra: &[],
            only: false,
            patches_dir: None,
            patches_config: None,
        };
        write_game_patches(&job, &mut log).unwrap();
        let mut empty = b"G3PATCH2".to_vec();
        empty.extend(0x40_0000u64.to_le_bytes());
        empty.extend(0u64.to_le_bytes());
        assert_eq!(fs::read(out.join("patches.bin")).unwrap(), empty);
        assert!(!out.join("patch_status.json").exists());
        assert_eq!(
            String::from_utf8(log).unwrap(),
            "Patches: no patch profile for CUSA99999: none applied\n"
        );
    }

    #[test]
    fn third_party_patches_need_the_reference_build() {
        // The fixture's synthetic eboot.bin is not the reference build: the external files are
        // not even read.
        let input = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/runner/patches/input");
        let dir = tempfile::tempdir().unwrap();
        let game = game_folder(
            dir.path(),
            &[("TITLE_ID", "CUSA01623"), ("APP_VER", "01.02")],
        );
        fs::copy(input.join("eboot.bin"), game.join("eboot.bin")).unwrap();
        let builtin = dir.path().join("patches");
        fs::create_dir_all(&builtin).unwrap();
        fs::copy(input.join("patches.xml"), builtin.join("CUSA01623.xml")).unwrap();
        let out = dir.path().join("out");
        fs::create_dir_all(&out).unwrap();
        fs::copy(input.join("eboot.elf"), out.join("eboot.elf")).unwrap();
        let external = dir.path().join("external");
        fs::create_dir_all(&external).unwrap();
        fs::write(external.join("extra.xml"), EXTERNAL).unwrap();
        let mut log = Vec::new();
        let job = PatchJob {
            out: &out,
            game: &game,
            builtin_dir: &builtin,
            mod_patches: &[],
            extra: &strings(&["Found"]),
            only: true,
            patches_dir: Some(&external),
            patches_config: None,
        };
        write_game_patches(&job, &mut log).unwrap();
        let log = String::from_utf8(log).unwrap();
        assert!(!log.contains("external"), "{log}");
        assert!(
            log.ends_with("Patches: CUSA01623; 1 writes from ['Found']\n"),
            "{log}"
        );
        let status = patch_status(&out).unwrap();
        assert_eq!(status.applied, ["Found"]);
        assert_ne!(status.sha256.as_deref(), Some(REFERENCE_SHA256));
    }
}
