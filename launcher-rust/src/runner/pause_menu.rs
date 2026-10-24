//! "Exit to Desktop" rows in the game's own pause menu and in its Options screen, an Enemy Health
//! Bar row in Display, and the native Cheats and Multipliers pages (Python `scripts/pause_menu.py`;
//! `docs/plans/2026-10-15-native-menu-feasibility.md`).
//!
//! All menus are one compiled state-machine script, the `SMI_R_UI` entry of `R_UI.WAD`. Its header
//! (LE) gives the code start (+0x0c) and the symbol table (+0x10); fields inside are big-endian:
//! - state records, a preorder tree up to the code: `04 <subtree size>`, parent and symbol words,
//!   then items `01 <entry code>`, `02 <update code>` and transitions `03 <code>` each followed by
//!   its trigger words (0x80 in the first byte: more follow). Code refs and literal operands count
//!   from code start + 4; state refs from SMI + 4; symbol refs from the symbol table + 4.
//! - code: `16 <u16 literal>`, `01 <i32>`, `09 e0NN` (local NN), `13 <provider:4 function:12>`
//!   (builtin call), `43` (pop), `49` (return), `4a <rel32>` (call), `47 <state>` (go to state).
//!
//! Pause menu: PauseMain shows line0..4 (Continue .. Quit Game) and clears line5..7, whose rows the
//! UI already has. WadChoose, a cheat-only level loader right after PauseMain's subtree, becomes its
//! sixth child, with its code written over WadChoose's.
//!
//! Options screen (reached from the pause menu and from the main menu): PauseOptions shows four rows
//! and clears line4. A fifth state record is inserted after HLDisplay, so the state table and
//! everything after it move: state refs, parents and subtree sizes are corrected, and the header and
//! the WAD entry grow. Its code and the label share WadChoose's dead code with the pause row, so the
//! literal pool and the symbol table keep their places relative to the code.
//!
//! Confirming a row asks Quit Game's question (the confirmation is the FailSafe machine of the same
//! script: a message to it shows the dialog, and its FailSafeYes state acts on "yes" by the kind of
//! action). The row marks its request with a third parameter of [`MARK`] (builtin 0x5e pops a
//! count, then up to 3 parameters, the message and the target); FailSafeDisplay keeps that mark in a
//! local nothing else uses, and FailSafeYes, given the mark, calls builtin 0xfff, which
//! `src/menu_hook.h` turns into the end of the process, instead of what Quit Game does. Both states
//! get a copy of their block with the few instructions added, written over WadChoose's dead code.
//!
//! [`build`] writes the patched copy to `out/pause_menu` for the runner to mount; the game folder is
//! never written.

use std::collections::{HashMap, HashSet};
use std::io;
use std::path::{Component, Path, PathBuf};

use indexmap::IndexMap;

use crate::core::fs::atomic_write;
use crate::core::patches::python_json;
use crate::runner::glyphs::entries;

/// Of the patch: a change rebuilds the cached copy.
pub const VERSION: u32 = 14;
pub const WAD: &str = "EXEC/WAD/PS4_LE/R_UI.WAD";

/// The Exit to Desktop row's text in one language.
#[derive(Debug, PartialEq, Eq)]
pub struct Label {
    /// The language's key, the one the stamp records.
    pub key: &'static str,
    /// The text, UTF-8.
    pub exit: &'static [u8],
}

/// The row's text by language (`LABELS` of the Python, in its order); the first is the fallback.
pub static LABELS: [Label; 10] = [
    Label {
        key: "en",
        exit: b"Exit to Desktop",
    },
    Label {
        key: "ja",
        exit: b"Exit to Desktop",
    },
    Label {
        key: "zh",
        exit: b"Exit to Desktop",
    },
    Label {
        key: "es",
        exit: "Salir al escritorio".as_bytes(),
    },
    Label {
        key: "fr",
        exit: "Quitter vers le bureau".as_bytes(),
    },
    Label {
        key: "de",
        exit: "Zurück zum Desktop".as_bytes(),
    },
    Label {
        key: "it",
        exit: "Esci al desktop".as_bytes(),
    },
    Label {
        key: "pl",
        exit: "Wyjdź do pulpitu".as_bytes(),
    },
    Label {
        key: "pt",
        exit: "Sair para a área de trabalho".as_bytes(),
    },
    Label {
        key: "ru",
        exit: b"Exit to Desktop",
    },
];

/// The game's language ids (`SPARTAN_LANGUAGE`) and other codes, to the keys of [`LABELS`]. The
/// Python lists each id as a number too; it maps the same way.
pub const LANGUAGE_ALIASES: &[(&str, &str)] = &[
    ("0", "ja"),
    ("1", "en"),
    ("2", "fr"),
    ("3", "es"),
    ("4", "de"),
    ("5", "it"),
    ("7", "pt"),
    ("8", "ru"),
    ("10", "zh"),
    ("16", "pl"),
    ("17", "pt"), // pt-BR (R_LANG_BR.WAD)
    ("18", "en"), // en-GB (R_LANG_UK.WAD)
    ("20", "es"), // es-419 (R_LANG_LA.WAD)
    ("br", "pt"),
    ("pt-br", "pt"),
    ("pt_br", "pt"),
    ("la", "es"),
    ("es-es", "es"),
    ("es_es", "es"),
    ("es-419", "es"),
    ("es_419", "es"),
    ("uk", "en"),
    ("en-us", "en"),
    ("en_us", "en"),
    ("en-gb", "en"),
    ("en_gb", "en"),
    ("zh-tw", "zh"),
    ("zh_tw", "zh"),
    ("zh-hant", "zh"),
    ("zh_hant", "zh"),
];

/// What Python's `str.isspace` takes for whitespace: Rust's plus the four separators 0x1c-0x1f.
fn is_python_space(c: char) -> bool {
    c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c)
}

/// The label of `language` (an id or a code; `SPARTAN_LANGUAGE` of `env` when `None`, else 1),
/// English for one it does not know.
pub fn resolve_label(
    language: Option<&str>,
    env: &dyn Fn(&str) -> Option<String>,
) -> &'static Label {
    let language = match language {
        Some(language) => language.to_string(),
        None => env("SPARTAN_LANGUAGE").unwrap_or_else(|| "1".to_string()),
    };
    let language = language.trim_matches(is_python_space).to_lowercase();
    let key = LANGUAGE_ALIASES
        .iter()
        .find(|(alias, _)| *alias == language)
        .map_or(language.as_str(), |(_, key)| key)
        .to_lowercase();
    LABELS
        .iter()
        .find(|label| label.key == key)
        .unwrap_or(&LABELS[0])
}

/// Builtin 0xfff (`src/menu_hook.h`), then pop.
pub const EXIT_CALL: [u8; 4] = [0x13, 0x0f, 0xff, 0x43];
/// Trigger ids: pad events and the dialog's answer.
pub const UP: [usize; 2] = [0xe0, 0xec];
pub const DOWN: [usize; 2] = [0xe4, 0xf0];
pub const CONFIRM: [usize; 1] = [0xe9];
pub const REPLY: [usize; 1] = [0x4b];
/// Of the row's state record.
pub const ROW_SIZE: usize = 0x3c;
/// Quit Game's reaction to the dialog being hidden by "no" (the message's first parameter is 0):
/// input back on.
pub const REPLY_TEMPLATE: [u8; 23] = [
    0x01, 0, 0, 0, 0, 0x13, 0x00, 0x47, 0x01, 0, 0, 0, 0, 0x3f, 0x17, 0x00, 0x05, 0x19, 0x13, 0x00,
    0x69, 0x43, 0x49,
];
/// The exit rows' third message parameter (Restart and Quit send two).
pub const MARK: usize = 5;
/// Opening of FailSafeDisplay's block for message 75 ("show" when the first parameter is 1).
pub const SHOW_BLOCK: [u8; 17] = [
    0x01, 0, 0, 0, 0, 0x13, 0x00, 0x47, 0x01, 0, 0, 0, 1, 0x3f, 0x17, 0x00, 0x84,
];
/// Opening of FailSafeYes' confirm block (it branches on the action's kind).
pub const YES_BLOCK: [u8; 15] = [
    0x03, 0xe0, 0x01, 0x0b, 0xe0, 0x02, 0x01, 0, 0, 0, 1, 0x3f, 0x17, 0x00, 0x2f,
];
/// FailSafeDisplay keeps the mark in its local 22 (failSafeMenu, unused).
pub const LOCAL: [u8; 2] = [0xe0, 0x16];
pub const SHOW_PREFIX: [u8; 12] = [
    0x01, 0, 0, 0, 2, 0x13, 0x00, 0x47, 0x07, LOCAL[0], LOCAL[1], 0x43,
];
/// The confirm block ends the process when the local is [`MARK`]. The jump skips the exit call, its
/// pop and the return.
pub const YES_PREFIX: [u8; 17] = [
    0x03,
    LOCAL[0],
    LOCAL[1],
    0x01,
    0,
    0,
    0,
    MARK as u8,
    0x3f,
    0x17,
    0x00,
    0x05,
    EXIT_CALL[0],
    EXIT_CALL[1],
    EXIT_CALL[2],
    EXIT_CALL[3],
    0x49,
];
/// Bytes taken by each opcode with its operands (the interpreter's handlers, image 0x1d5a4e).
pub const LENGTH: [usize; 0x4c] = {
    let mut table = [1; 0x4c];
    let mut op = 0;
    while op < 0x4c {
        table[op] = if matches!(op, 0x00 | 0x01 | 0x47 | 0x4a) {
            5
        } else if matches!(op, 0x02..=0x18) {
            3
        } else {
            1
        };
        op += 1;
    }
    table
};
/// Builtin 0x5e (send a message), then pop.
const SEND: [u8; 4] = [0x13, 0x00, 0x5e, 0x43];
/// The native Cheats page's rows.
const CHEAT_NAMES: [&str; 9] = [
    "Infinite Health",
    "Infinite Magic",
    "Infinite Item Meter",
    "Infinite Rage of Sparta",
    "Max Red Orbs",
    "Unlock Very Hard",
    "Infinite Double Jumps",
    "Automatic QTE",
    "Multipliers",
];
/// The native Multipliers page's rows.
const MULTIPLIER_NAMES: [&str; 7] = [
    "Red Orbs",
    "Green Orbs (Health)",
    "Blue Orbs (Magic)",
    "Gold Orbs (Rage)",
    "Damage Dealt",
    "Damage Taken",
    "Reset All to 1x",
];

/// Code offsets `[(start, end)]` of code blocks: the ones of an unpatched script, then the code
/// each step adds.
pub type Spans = Vec<(usize, usize)>;

/// Why no copy is made.
#[derive(Debug, thiserror::Error)]
pub enum MenuError {
    /// Not the script the patch was made for (the Python's `ValueError`): nothing is changed.
    #[error("{0}")]
    Script(String),
    #[error("{0}")]
    Io(#[from] io::Error),
}

fn script(message: impl Into<String>) -> MenuError {
    MenuError::Script(message.into())
}

// -- Bytes ------------------------------------------------------------------------------------------

/// `value.to_bytes(size, 'big')`; an error when it does not fit, as the Python's `OverflowError`.
pub fn be(value: usize, size: usize) -> Result<Vec<u8>, MenuError> {
    let bytes = (value as u64).to_be_bytes();
    if size > 8 || (size < 8 && value as u64 >> (8 * size) != 0) {
        return Err(script("int too big to convert"));
    }
    Ok(bytes[8 - size..].to_vec())
}

/// `struct.pack('>i', value)`.
fn rel32(value: i64) -> Result<[u8; 4], MenuError> {
    i32::try_from(value)
        .map(i32::to_be_bytes)
        .map_err(|_| script("'i' format requires -2147483648 <= number <= 2147483647"))
}

/// `16 <ref>`: the instruction that pushes the literal at code offset `at`.
fn push(at: usize) -> Result<Vec<u8>, MenuError> {
    Ok([&[0x16][..], &be(at, 2)?].concat())
}

/// `4a <rel32>`: a call to code offset `target` from code offset `at`.
fn call(target: i64, at: usize) -> Result<Vec<u8>, MenuError> {
    Ok([&[0x4a][..], &rel32(target - at as i64 - 5)?].concat())
}

/// The trigger words of a transition: 0x80 in the first byte of every one but the last.
fn trigger_words(ids: &[usize]) -> Result<Vec<u8>, MenuError> {
    let mut out = Vec::new();
    for (i, id) in ids.iter().enumerate() {
        let more = if i + 1 < ids.len() { 0x8000_0000 } else { 0 };
        out.extend(be(id | more, 4)?);
    }
    Ok(out)
}

fn bytes_at(data: &[u8], start: usize, end: usize) -> Result<&[u8], MenuError> {
    data.get(start..end)
        .ok_or_else(|| script("index out of range"))
}

fn word<const N: usize>(data: &[u8], at: usize) -> Result<[u8; N], MenuError> {
    data.get(at..at + N)
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or_else(|| script("unpack_from requires a buffer of more bytes"))
}

fn byte(data: &[u8], at: usize) -> Result<u8, MenuError> {
    data.get(at)
        .copied()
        .ok_or_else(|| script("index out of range"))
}

fn le16(data: &[u8], at: usize) -> Result<usize, MenuError> {
    Ok(usize::from(u16::from_le_bytes(word(data, at)?)))
}

fn be16(data: &[u8], at: usize) -> Result<usize, MenuError> {
    Ok(usize::from(u16::from_be_bytes(word(data, at)?)))
}

fn le32(data: &[u8], at: usize) -> Result<usize, MenuError> {
    Ok(u32::from_le_bytes(word(data, at)?) as usize)
}

fn be32(data: &[u8], at: usize) -> Result<usize, MenuError> {
    Ok(u32::from_be_bytes(word(data, at)?) as usize)
}

/// `int.from_bytes(data[at:at + 3], 'big')`.
fn be24(data: &[u8], at: usize) -> usize {
    data.iter()
        .skip(at)
        .take(3)
        .fold(0, |value, b| value << 8 | usize::from(*b))
}

/// The signed value of a 4-byte operand.
fn signed32(operand: u64) -> i64 {
    i64::from(operand as u32 as i32)
}

/// Overwrites `data[at..]` with `value`.
fn put(data: &mut [u8], at: usize, value: &[u8]) -> Result<(), MenuError> {
    data.get_mut(at..at + value.len())
        .ok_or_else(|| script("index out of range"))?
        .copy_from_slice(value);
    Ok(())
}

/// `struct.pack_into('<I', data, at, value)`.
fn put_le32(data: &mut [u8], at: usize, value: usize) -> Result<(), MenuError> {
    let value = u32::try_from(value)
        .map_err(|_| script("'I' format requires 0 <= number <= 4294967295"))?;
    put(data, at, &value.to_le_bytes())
}

/// `data[start:end] = value`, with the Python's slice bounds.
fn splice(data: &mut Vec<u8>, start: usize, end: usize, value: &[u8]) {
    let start = start.min(data.len());
    let end = end.clamp(start, data.len());
    data.splice(start..end, value.iter().copied());
}

/// The zeros that pad an entry of `size` bytes to 16.
fn padding(size: usize) -> usize {
    (16 - size % 16) % 16
}

/// Replaces the padding of an entry that grew from `old` to `new` bytes, at `at` (its new end).
fn repad(data: &mut Vec<u8>, at: usize, old: usize, new: usize) {
    splice(data, at, at + padding(old), &vec![0; padding(new)]);
}

/// The first `needle` in `haystack[start:end]` (`bytes.find`).
fn find(haystack: &[u8], needle: &[u8], start: usize, end: usize) -> Option<usize> {
    let end = end.min(haystack.len());
    if start > end {
        return None;
    }
    if needle.is_empty() {
        return Some(start);
    }
    haystack[start..end]
        .windows(needle.len())
        .position(|window| window == needle)
        .map(|at| at + start)
}

/// `haystack.count(needle)`: occurrences that do not overlap.
fn count(haystack: &[u8], needle: &[u8]) -> usize {
    let (mut found, mut at) = (0, 0);
    while let Some(next) = find(haystack, needle, at, haystack.len()) {
        found += 1;
        at = next + needle.len().max(1);
    }
    found
}

/// `haystack.replace(from, to)` (`from` is never empty here).
fn replace(haystack: &[u8], from: &[u8], to: &[u8]) -> Vec<u8> {
    let (mut out, mut at) = (Vec::new(), 0);
    while let Some(next) = find(haystack, from, at, haystack.len()).filter(|_| !from.is_empty()) {
        out.extend_from_slice(&haystack[at..next]);
        out.extend_from_slice(to);
        at = next + from.len();
    }
    out.extend_from_slice(&haystack[at..]);
    out
}

/// `bytes(data[start:]).split(b'\0', 1)[0]`.
fn until_nul(data: &[u8], start: usize) -> &[u8] {
    let rest = data.get(start..).unwrap_or_default();
    &rest[..rest.iter().position(|b| *b == 0).unwrap_or(rest.len())]
}

/// `repr(bytes)` of the Python, for its messages.
fn bytes_repr(data: &[u8]) -> String {
    let quote = if data.contains(&b'\'') && !data.contains(&b'"') {
        '"'
    } else {
        '\''
    };
    let mut out = format!("b{quote}");
    for &b in data {
        match b {
            b'\\' => out.push_str("\\\\"),
            b'\t' => out.push_str("\\t"),
            b'\n' => out.push_str("\\n"),
            b'\r' => out.push_str("\\r"),
            _ if char::from(b) == quote => {
                out.push('\\');
                out.push(quote);
            }
            0x20..=0x7e => out.push(char::from(b)),
            _ => out.push_str(&format!("\\x{b:02x}")),
        }
    }
    out.push(quote);
    out
}

// -- The script -------------------------------------------------------------------------------------

/// One instruction: where it is, its opcode and its operand bytes as a big-endian number.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Op {
    pub offset: usize,
    pub opcode: u8,
    pub operand: u64,
}

/// The instructions of the code in `data[start..end]`; an error when it does not split exactly
/// into instructions.
pub fn decode(data: &[u8], start: usize, end: usize) -> Result<Vec<Op>, MenuError> {
    let (mut out, mut at) = (Vec::new(), start);
    while at < end {
        let opcode = byte(data, at)?;
        let length = *LENGTH
            .get(usize::from(opcode))
            .ok_or_else(|| script(format!("unknown opcode {opcode:#x}")))?;
        let operand = data
            .get(at + 1..at + length)
            .ok_or_else(|| script("code overruns its block"))?;
        out.push(Op {
            offset: at,
            opcode,
            operand: operand
                .iter()
                .fold(0, |value, b| value << 8 | u64::from(*b)),
        });
        at += length;
    }
    if at != end {
        return Err(script("code overruns its block"));
    }
    Ok(out)
}

/// A state record: where it starts (from the script's start), its subtree's size, its parent's
/// state ref, its symbol and its items `(kind, value)`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record {
    pub at: usize,
    pub size: usize,
    pub parent: usize,
    pub id: usize,
    pub items: Vec<(u8, usize)>,
}

/// A code block of a state: its kind (1 entry, 2 update, 3 transition), its code ref and, for a
/// transition, its triggers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Block {
    pub kind: u8,
    pub code: usize,
    pub triggers: Vec<usize>,
}

/// The `SMI_R_UI` script of a WAD.
pub struct Script<'a> {
    data: &'a [u8],
    /// Where the script starts in the WAD.
    pub base: usize,
    /// The code start and the symbol table, from the header.
    pub code: usize,
    pub symtab: usize,
    /// Code refs and literal operands count from here.
    pub lit: usize,
    /// The state records, in table order.
    pub records: Vec<Record>,
    names: HashMap<usize, String>,
    refs: Vec<usize>,
}

impl<'a> Script<'a> {
    /// The script of the WAD in `data`; an error when there is none or it does not parse.
    pub fn parse(data: &'a [u8]) -> Result<Self, MenuError> {
        let base = entries(data)
            .into_iter()
            .find(|entry| entry.name == "SMI_R_UI")
            .map(|entry| entry.offset + 0x40)
            .ok_or_else(|| script("no SMI_R_UI in this WAD"))?;
        let code = le32(data, base + 0x0c)?;
        let symtab = le32(data, base + 0x10)?;
        let mut s = Script {
            data,
            base,
            code,
            symtab,
            lit: code + 4,
            records: Vec::new(),
            names: HashMap::new(),
            refs: Vec::new(),
        };
        // Symbol records (kind, index, name) up to the name pool, which starts at the first name.
        let (mut i, mut pool) = (symtab + 8, None);
        while pool.is_none_or(|pool| i < pool) {
            let at = symtab + 4 + be32(data, base + i + 4)?;
            pool = Some(pool.map_or(at, |pool: usize| pool.min(at)));
            let name = s.string(at)?;
            s.names.insert(i - symtab - 4, name);
            i += 8;
        }
        s.records = s.parse_records()?;
        s.refs = s
            .records
            .iter()
            .flat_map(|r| &r.items)
            .filter(|(kind, _)| matches!(kind, 1..=3))
            .map(|(_, value)| *value)
            .collect();
        s.refs.sort_unstable();
        Ok(s)
    }

    /// The WAD the script is in.
    pub fn data(&self) -> &'a [u8] {
        self.data
    }

    /// The NUL-terminated string at `at` (from the script's start), as Latin-1.
    pub fn string(&self, at: usize) -> Result<String, MenuError> {
        let start = self.base + at;
        let rest = self
            .data
            .get(start..)
            .ok_or_else(|| script("subsection not found"))?;
        let end = rest
            .iter()
            .position(|b| *b == 0)
            .ok_or_else(|| script("subsection not found"))?;
        Ok(rest[..end].iter().map(|b| char::from(*b)).collect())
    }

    /// The tree starts after a header table of variable length: the first offset that parses
    /// exactly up to the code is it.
    fn parse_records(&self) -> Result<Vec<Record>, MenuError> {
        (0x18..0x400)
            .step_by(4)
            .find_map(|start| self.records_from(start).ok())
            .ok_or_else(|| script("state table not found"))
    }

    fn records_from(&self, start: usize) -> Result<Vec<Record>, MenuError> {
        let (d, b) = (self.data, self.base);
        let (mut out, mut at) = (Vec::new(), start);
        while at < self.code {
            if byte(d, b + at)? != 4 {
                return Err(script(format!("no state record at {at:#x}")));
            }
            let size = be24(d, b + at + 1);
            let (parent, id) = (be32(d, b + at + 4)?, be32(d, b + at + 8)?);
            let (mut j, mut items) = (at + 12, Vec::new());
            while j < self.code && byte(d, b + j)? != 4 {
                items.push((byte(d, b + j)?, be24(d, b + j + 1)));
                j += 4;
            }
            out.push(Record {
                at,
                size,
                parent,
                id,
                items,
            });
            at = j;
        }
        if at != self.code {
            return Err(script("state table overruns the code"));
        }
        Ok(out)
    }

    /// The name of `record`'s symbol.
    pub fn name_of(&self, record: &Record) -> Option<&str> {
        self.names.get(&record.id).map(String::as_str)
    }

    /// The states called `name`, in table order.
    pub fn by_name(&self, name: &str) -> Vec<&Record> {
        self.records
            .iter()
            .filter(|record| self.name_of(record) == Some(name))
            .collect()
    }

    /// The one state called `name`.
    pub fn one(&self, name: &str) -> Result<&Record, MenuError> {
        let found = self.by_name(name);
        match found[..] {
            [record] => Ok(record),
            _ => Err(script(format!("{name}: {} states", found.len()))),
        }
    }

    /// The code blocks of a state.
    pub fn blocks(record: &Record) -> Vec<Block> {
        let mut out: Vec<Block> = Vec::new();
        for &(kind, value) in &record.items {
            if matches!(kind, 1..=3) {
                out.push(Block {
                    kind,
                    code: value,
                    triggers: Vec::new(),
                });
            } else if let Some(last) = out.last_mut().filter(|last| last.kind == 3) {
                last.triggers.push(value);
            }
        }
        out
    }

    /// The code refs of every state's blocks, sorted (with repeats).
    pub fn code_refs(&self) -> Vec<usize> {
        self.refs.clone()
    }

    /// Where the code block at `code` ends: the next code ref.
    fn block_end(&self, code: usize) -> Result<usize, MenuError> {
        self.refs
            .iter()
            .copied()
            .find(|r| *r > code)
            .ok_or_else(|| script(format!("no code block after {code:#x}")))
    }

    /// The bytes of the code block at `code`.
    pub fn block_bytes(&self, code: usize) -> Result<&'a [u8], MenuError> {
        let first = self.base + self.lit;
        bytes_at(self.data, first + code, first + self.block_end(code)?)
    }

    /// The code offset of the literal `text`.
    pub fn literal(&self, text: &[u8]) -> Result<usize, MenuError> {
        let needle = [text, &[0]].concat();
        let start = self.base + self.lit;
        find(self.data, &needle, start, self.base + self.symtab)
            .map(|at| at - start)
            .ok_or_else(|| script(format!("literal {} not found", bytes_repr(text))))
    }

    /// The code offsets `[(start, end)]` of every code block of an unpatched script: the last one
    /// ends where the literal pool begins, at the smallest literal operand.
    pub fn spans(&self) -> Result<Spans, MenuError> {
        let mut refs = self.refs.clone();
        refs.dedup();
        let mut spans: Spans = refs.windows(2).map(|pair| (pair[0], pair[1])).collect();
        let first = self.base + self.lit;
        let mut pool: Option<u64> = None;
        for &(a, b) in &spans {
            for op in decode(self.data, first + a, first + b)? {
                if op.opcode == 0x16 {
                    pool = Some(pool.map_or(op.operand, |pool| pool.min(op.operand)));
                }
            }
        }
        let pool = pool.ok_or_else(|| script("min() iterable argument is empty"))? as usize;
        match refs.last() {
            Some(&last) if last < pool => {
                spans.push((last, pool));
                Ok(spans)
            }
            _ => Err(script("literal pool not found")),
        }
    }
}

/// `record`, then its parent, its parent's parent and so on, as long as the parent word names a
/// state of `s`. A chain longer than the state table goes round a cycle: an error, where the
/// Python would never return.
fn ancestors<'r>(s: &'r Script, record: &'r Record) -> Result<Vec<&'r Record>, MenuError> {
    let mut chain = vec![record];
    while let Some(parent) = chain
        .last()
        .and_then(|last| s.records.iter().find(|r| r.at - 4 == last.parent))
    {
        if chain.len() >= s.records.len() {
            return Err(script("state tree has a cycle"));
        }
        chain.push(parent);
    }
    Ok(chain)
}

/// The code of `record`'s first block of `kind`.
fn first_block(record: &Record, kind: u8) -> Result<usize, MenuError> {
    Script::blocks(record)
        .into_iter()
        .find(|b| b.kind == kind)
        .map(|b| b.code)
        .ok_or_else(|| script(format!("no block of kind {kind}")))
}

/// The code of `record`'s transition on `triggers`.
pub fn transition(record: &Record, triggers: &[usize]) -> Result<usize, MenuError> {
    let found: Vec<Block> = Script::blocks(record)
        .into_iter()
        .filter(|b| b.kind == 3 && b.triggers == triggers)
        .collect();
    match found[..] {
        [ref block] => Ok(block.code),
        _ => {
            let ids: Vec<String> = triggers.iter().map(usize::to_string).collect();
            Err(script(format!("transition [{}] not found", ids.join(", "))))
        }
    }
}

/// `(offset, bytes)` that make the state `record` go to the state ref `new` instead of `old` when
/// `triggers` fire.
pub fn retarget(
    s: &Script,
    record: &Record,
    triggers: &[usize],
    old: usize,
    new: usize,
) -> Result<(usize, Vec<u8>), MenuError> {
    let code = s.base + s.lit + transition(record, triggers)?;
    let needle = [&[0x47][..], &be(old, 4)?].concat();
    let at = find(s.data, &needle, code, code + 0x20)
        .ok_or_else(|| script("transition target not found"))?;
    Ok((at + 1, be(new, 4)?))
}

/// The code offsets of an Exit row's blocks.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Pieces {
    pub entry: usize,
    pub update: usize,
    pub up: usize,
    pub down: usize,
    pub confirm: usize,
    pub reply: usize,
}

/// `(code, pieces)` of an Exit row: `label` (with its NUL) when given, then Quit Game's blocks as
/// the template, with `slot` as its highlight, laid from code offset `start`; `up` and `down` are
/// the state refs its arrows go to. Confirming asks Quit Game's own question ("Are you sure you
/// want to Quit?") with the exit mark; the row's handler for the dialog being hidden is Quit
/// Game's.
pub fn row_code(
    s: &Script,
    quit: &Record,
    start: usize,
    up: usize,
    down: usize,
    slot: &[u8],
    label: Option<&[u8]>,
) -> Result<(Vec<u8>, Pieces), MenuError> {
    // The highlight blocks (the last of each kind, as the Python's dict keeps it).
    let highlight_of = |kind: u8| {
        Script::blocks(quit)
            .into_iter()
            .rev()
            .find(|b| b.kind == kind)
            .map(|b| b.code)
            .ok_or_else(|| script("Quit Game highlight not found"))
    };
    let highlight = [
        s.block_bytes(highlight_of(1)?)?,
        s.block_bytes(highlight_of(2)?)?,
    ];
    let old = push(s.literal(b"menuItem4Select")?)?;
    let new = push(s.literal(slot)?)?;
    if highlight.iter().any(|code| count(code, &old) != 1) {
        return Err(script("Quit Game highlight not found"));
    }
    let arrow = s.block_bytes(transition(quit, &UP)?)?;
    let ask = s.block_bytes(transition(quit, &CONFIRM)?)?;
    let answer = s.block_bytes(transition(quit, &REPLY)?)?;
    if arrow.first() != Some(&0x4a)
        || ask.first() != Some(&0x16)
        || ask.get(3..7) != Some(&[0x13, 0x00, 0x5c, 0x43][..])
        || count(ask, &SEND) == 0
    {
        return Err(script("Quit Game transitions not as expected"));
    }
    if answer != REPLY_TEMPLATE {
        return Err(script("Quit Game's answer handler not as expected"));
    }
    // The argument count (target, message, 2 parameters), then the send.
    let request = [&[0x01][..], &be(4, 4)?, &SEND].concat();
    if count(ask, &request) != 1 {
        return Err(script("Quit Game's request not as expected"));
    }
    // A third parameter goes in before the count, which becomes 5 (the count alone underflows the
    // stack).
    let marked = [&[0x01][..], &be(MARK, 4)?, &[0x01], &be(5, 4)?, &SEND].concat();
    let ask = replace(ask, &request, &marked);
    // The menu move sound.
    let sound = transition(quit, &UP)? as i64 + 5 + i64::from(i32::from_be_bytes(word(arrow, 1)?));

    let mut code = label.map_or_else(Vec::new, |label| [label, &[0]].concat());
    let mut pieces = Pieces {
        entry: start + code.len(),
        ..Pieces::default()
    };
    code.extend(replace(highlight[0], &old, &new));
    pieces.update = start + code.len();
    code.extend(replace(highlight[1], &old, &new));
    let arrow_code = |at: usize, target: usize| -> Result<Vec<u8>, MenuError> {
        Ok([call(sound, at)?, vec![0x47], be(target, 4)?, vec![0x49]].concat())
    };
    pieces.up = start + code.len();
    code.extend(arrow_code(pieces.up, up)?);
    pieces.down = start + code.len();
    code.extend(arrow_code(pieces.down, down)?);
    pieces.confirm = start + code.len();
    code.extend(ask);
    pieces.reply = start + code.len();
    code.extend_from_slice(answer);
    Ok((code, pieces))
}

/// The row's state record: highlight, the two arrows, confirm and the dialog's answer.
pub fn row_record(parent: usize, sid: usize, pieces: &Pieces) -> Result<Vec<u8>, MenuError> {
    let record = [
        be(0x0400_0000 | ROW_SIZE, 4)?,
        be(parent, 4)?,
        be(sid, 4)?,
        be(0x0100_0000 | pieces.entry, 4)?,
        be(0x0200_0000 | pieces.update, 4)?,
        be(0x0300_0000 | pieces.up, 4)?,
        trigger_words(&UP)?,
        be(0x0300_0000 | pieces.down, 4)?,
        trigger_words(&DOWN)?,
        be(0x0300_0000 | pieces.confirm, 4)?,
        trigger_words(&CONFIRM)?,
        be(0x0300_0000 | pieces.reply, 4)?,
        trigger_words(&REPLY)?,
    ]
    .concat();
    if record.len() != ROW_SIZE {
        return Err(script("record size mismatch"));
    }
    Ok(record)
}

/// The code block at `code`, laid at code offset `at`: states that move (`relocated` gives where
/// the record at an offset ends up) are followed and calls are rebased.
pub fn moved_copy(
    s: &Script,
    code: usize,
    at: usize,
    relocated: &dyn Fn(usize) -> usize,
) -> Result<Vec<u8>, MenuError> {
    let first = s.base + s.lit + code;
    let last = s.base + s.lit + s.block_end(code)?;
    let mut out = bytes_at(s.data, first, last)?.to_vec();
    for op in decode(s.data, first, last)? {
        let i = op.offset - first;
        match op.opcode {
            0x47 => {
                let state = relocated(op.operand as usize + 4) - 4;
                put(&mut out, i + 1, &be(state, 4)?)?;
            }
            0x4a => {
                let target = (code + i + 5) as i64 + signed32(op.operand);
                put(&mut out, i + 1, &rel32(target - (at + i + 5) as i64)?)?;
            }
            _ => {}
        }
    }
    Ok(out)
}

/// A FailSafe state, the code ref of the block it ran and the code offset of its new one.
pub type Redirect = (Record, usize, usize);

/// `(code, redirects)`: the FailSafe states' two blocks with the exit mark's instructions added,
/// to be laid from code offset `start`.
pub fn failsafe_code(
    s: &Script,
    start: usize,
    relocated: &dyn Fn(usize) -> usize,
) -> Result<(Vec<u8>, Vec<Redirect>), MenuError> {
    let (mut code, mut redirects) = (Vec::new(), Vec::new());
    for (name, trigger, opening, prefix) in [
        (
            "FailSafeDisplay",
            &REPLY[..],
            &SHOW_BLOCK[..],
            &SHOW_PREFIX[..],
        ),
        ("FailSafeYes", &CONFIRM[..], &YES_BLOCK[..], &YES_PREFIX[..]),
    ] {
        let record = s.one(name)?;
        let old = transition(record, trigger)?;
        if !s.block_bytes(old)?.starts_with(opening) {
            return Err(script(format!("{name} not as expected")));
        }
        let at = start + code.len();
        code.extend_from_slice(prefix);
        code.extend(moved_copy(s, old, at + prefix.len(), relocated)?);
        redirects.push((record.clone(), old, at));
    }
    Ok((code, redirects))
}

/// The offset (in the WAD) of the 16-bit literal that `record`'s entry assigns to the local `local`
/// while it is empty, to be replaced by the label.
pub fn blank_line(s: &Script, record: &Record, local: u8) -> Result<usize, MenuError> {
    let entry_ref = first_block(record, 1)?;
    let entry = s.block_bytes(entry_ref)?;
    let setter = [0x09, 0xe0, local, 0x43];
    let mut found = Vec::new();
    for (i, window) in entry.windows(7).enumerate() {
        if window[0] == 0x16
            && window[3..] == setter
            && s.string(s.lit + be16(window, 1)?)?.is_empty()
        {
            found.push(i);
        }
    }
    match found[..] {
        [i] => Ok(s.base + s.lit + entry_ref + i + 1),
        _ => Err(script(format!(
            "line {} assignment not found",
            i32::from(local) - 1
        ))),
    }
}

// -- The steps --------------------------------------------------------------------------------------

/// Code and text added after the script's last block, from code offset `start`.
struct Appended {
    start: usize,
    bytes: Vec<u8>,
}

impl Appended {
    fn here(&self) -> usize {
        self.start + self.bytes.len()
    }

    fn add(&mut self, body: &[u8]) -> usize {
        let at = self.here();
        self.bytes.extend_from_slice(body);
        at
    }

    /// Adds the text `value` and returns the instruction that pushes it.
    fn text(&mut self, value: &[u8]) -> Result<Vec<u8>, MenuError> {
        let at = self.here();
        if at > 0xffff {
            return Err(script("Display text exceeds the script literal range"));
        }
        self.bytes.extend_from_slice(value);
        self.bytes.push(0);
        push(at)
    }
}

/// The code offset where the symbol table starts (the end of the literal pool).
fn pool_end(s: &Script) -> Result<usize, MenuError> {
    s.symtab
        .checked_sub(s.lit)
        .ok_or_else(|| script("the symbol table is before the code"))
}

/// A fourth Display row, using PauseText's existing line3 and menuItem3Select animation; its code
/// goes on `spans`.
pub fn display_health_bar(data: &mut Vec<u8>, spans: &mut Spans) -> Result<(), MenuError> {
    const ROW: usize = 0x44;
    const OPT_SET3: [u8; 4] = [0x09, 0xe0, 0x14, 0x43]; // PauseMenu.optSet3
    let s = Script::parse(data)?;
    let base = s.base;
    let display = s.one("PauseDisplay")?.clone();
    let y = s.one("HLScreenLocY")?.clone();
    let brightness = s.one("HLBrightness")?.clone();
    let template = s.one("HLSubtitleDis")?.clone();
    let insert = display.at + display.size;
    if y.at + y.size != insert {
        return Err(script("Display does not end with Screen Location Y"));
    }
    let relocated = |at: usize| at + if at >= insert { ROW } else { 0 };
    let row_ref = insert - 4;
    let mut append = Appended {
        start: pool_end(&s)?,
        bytes: Vec::new(),
    };
    let label = append.text(b"Enemy Health Bar")?;
    let off = append.text(b"Off")?;
    let on = append.text(b"On")?;
    let unavailable = append.text(b"Unavailable")?;
    let selected = append.text(b"[ArrowSelectionL][PauseMenu:optSet3][ArrowSelectionR]")?;
    let unselected = append.text(b"[ArrowSpacer][PauseMenu:optSet3][ArrowSpacer]")?;
    // A subroutine used on entry and after an edit. Query again so the Insert menu stays in sync.
    let refresh = append.here();
    let set = |value: &[u8]| [value, &OPT_SET3, &[0x4b]].concat();
    let enabled = [
        vec![0x13, 0x0f, 0xfe, 0x17],
        be(set(&on).len(), 2)?,
        set(&on),
        set(&off),
    ]
    .concat();
    let unsupported = set(&unavailable);
    append.add(
        &[
            vec![0x13, 0x0f, 0xfe, 0x01],
            be(2, 4)?,
            vec![0x3f, 0x17],
            be(unsupported.len(), 2)?,
            unsupported,
            enabled,
        ]
        .concat(),
    );
    let refresh_from = |at: usize| call(refresh as i64, at);

    // Display clears its rows whenever it opens, before any individual row gets focus.
    let entry_item = display
        .items
        .iter()
        .position(|(kind, _)| *kind == 1)
        .ok_or_else(|| script("PauseDisplay has no entry"))?;
    let entry_ref = display.items[entry_item].1;
    let at = append.here();
    let mut body = moved_copy(&s, entry_ref, at + 5, &relocated)?;
    for (local, value) in [(4u8, &label), (12, &unselected)] {
        let operand = blank_line(&s, &display, local)?;
        let setter = [0x09, 0xe0, local, 0x43];
        let empty = [
            &[0x16][..],
            bytes_at(s.data, operand, operand + 2)?,
            &setter,
        ]
        .concat();
        body = replace(&body, &empty, &[&value[..], &setter].concat());
    }
    let display_entry = append.add(&[refresh_from(at)?, body].concat());

    let mut highlight = Vec::new();
    for kind in [1u8, 2] {
        let old = first_block(&template, kind)?;
        let at = append.here();
        let prefix = if kind == 1 {
            refresh_from(at)?
        } else {
            Vec::new()
        };
        let mut body = moved_copy(&s, old, at + prefix.len(), &relocated)?;
        let (old_value, new_value) = if kind == 1 {
            (
                &b"[ArrowSelectionL][PauseMenu:optSet1][ArrowSelectionR]"[..],
                &selected,
            )
        } else {
            (
                &b"[ArrowSpacer][PauseMenu:optSet1][ArrowSpacer]"[..],
                &unselected,
            )
        };
        body = replace(&body, &push(s.literal(old_value)?)?, new_value);
        body = replace(&body, &[0x09, 0xe0, 0x0a, 0x43], &[0x09, 0xe0, 0x0c, 0x43]); // line3 value
        body = replace(
            &body,
            &push(s.literal(b"menuItem1Select")?)?,
            &push(s.literal(b"menuItem3Select")?)?,
        );
        highlight.push(append.add(&[prefix, body].concat()));
    }
    let old = transition(&template, &[0xe6, 0xe2, 0xee, 0xf2])?;
    let at = append.here();
    let gate = moved_copy(&s, old, at, &relocated)?;
    let mut body = gate[..gate.len().min(5)].to_vec(); // the native left/right repeat gate
    body.extend([0x13, 0x0f, 0xfd, 0x43]);
    body.extend(refresh_from(at + body.len())?);
    body.extend_from_slice(&selected);
    body.extend([0x09, 0xe0, 0x0c, 0x43]);
    body.extend(push(s.literal(b"SND_MM_NAV_LR")?)?);
    body.extend([0x13, 0x00, 0x5c, 0x43, 0x49]);
    let toggle = append.add(&body);
    let mut arrows = Vec::new();
    for (trigger, target) in [(&UP[..], y.at - 4), (&DOWN[..], brightness.at - 4)] {
        let at = append.here();
        let old = transition(&y, trigger)?;
        let body = moved_copy(&s, old, at, &relocated)?;
        let body = [
            &body[..body.len().min(5)],
            &[0x47],
            &be(target, 4)?,
            &[0x49],
        ]
        .concat();
        arrows.push(append.add(&body));
    }

    // Rebase existing states once; code/string offsets do not move relative to the code base.
    let mut edits = Vec::new();
    for &(a, b) in spans.iter() {
        for op in decode(s.data, base + s.lit + a, base + s.lit + b)? {
            if op.opcode == 0x47 && op.operand as usize + 4 >= insert {
                let state = relocated(op.operand as usize + 4) - 4;
                edits.push((op.offset + 1, be(state, 4)?));
            }
        }
    }
    edits.push(retarget(&s, &y, &DOWN, brightness.at - 4, row_ref)?);
    edits.push(retarget(&s, &brightness, &UP, y.at - 4, row_ref)?);
    edits.push((blank_line(&s, &display, 4)?, label[1..].to_vec()));
    edits.push((
        base + display.at + 12 + 4 * entry_item,
        be(0x0100_0000 | display_entry, 4)?,
    ));
    for record in ancestors(&s, &display)? {
        edits.push((base + record.at, be(0x0400_0000 | (record.size + ROW), 4)?));
    }
    for record in &s.records {
        if record.parent + 4 >= insert {
            let parent = relocated(record.parent + 4) - 4;
            edits.push((base + record.at + 4, be(parent, 4)?));
        }
    }
    let symtab = s.symtab;
    for (at, value) in edits {
        put(data, at, &value)?;
    }

    let event = |piece: usize, triggers: &[usize]| -> Result<Vec<u8>, MenuError> {
        Ok([be(0x0300_0000 | piece, 4)?, trigger_words(triggers)?].concat())
    };
    let record = [
        be(0x0400_0000 | ROW, 4)?,
        be(display.at - 4, 4)?,
        be(template.id, 4)?,
        be(0x0100_0000 | highlight[0], 4)?,
        be(0x0200_0000 | highlight[1], 4)?,
        event(toggle, &[0xe6, 0xe2, 0xee, 0xf2, 0xe9])?,
        event(arrows[0], &UP)?,
        event(arrows[1], &DOWN)?,
    ]
    .concat();
    if record.len() != ROW {
        return Err(script("Display row size mismatch"));
    }
    let old_size = le32(data, base - 0x40 + 4)?;
    splice(data, base + symtab, base + symtab, &append.bytes);
    splice(data, base + insert, base + insert, &record);
    let grown = ROW + append.bytes.len();
    for (offset, delta) in [(0x08, grown), (0x0c, ROW), (0x10, grown)] {
        let value = le32(data, base + offset)? + delta;
        put_le32(data, base + offset, value)?;
    }
    put_le32(data, base - 0x40 + 4, old_size + grown)?;
    repad(data, base + old_size + grown, old_size, old_size + grown);
    spans.push((refresh, append.here()));
    Ok(())
}

/// A state `native_cheats` adds: its name, its parent (a name, or `options`/`pause` for the
/// existing PauseOptions and EnterPause), its transitions' triggers and its record's size.
struct Spec {
    name: String,
    parent: &'static str,
    events: Vec<Vec<usize>>,
    size: usize,
}

impl Spec {
    fn new(name: &str, parent: &'static str, events: Vec<Vec<usize>>) -> Spec {
        let size = 20 + events.iter().map(|t| 4 + 4 * t.len()).sum::<usize>();
        Spec {
            name: name.to_string(),
            parent,
            events,
            size,
        }
    }
}

/// The code of the native pages, as `native_cheats` writes it: texts not in the script go after
/// its literal pool (`strings`), the code after them.
struct Emitter<'s, 'a> {
    s: &'s Script<'a>,
    start: usize,
    code: Vec<u8>,
    strings: Vec<u8>,
    string_refs: HashMap<Vec<u8>, usize>,
    /// `(offset in code, target code offset)` of the calls, resolved once `strings` is complete.
    fixups: Vec<(usize, i64)>,
    /// Where each state's handler starts in `code`, by state and kind.
    handlers: HashMap<(String, &'static str), usize>,
    refs: &'s HashMap<String, usize>,
    relocated: &'s dyn Fn(usize) -> usize,
}

impl Emitter<'_, '_> {
    fn emit(&mut self, bytes: &[u8]) {
        self.code.extend_from_slice(bytes);
    }

    fn string(&mut self, value: &[u8]) -> Result<(), MenuError> {
        let at = match self.string_refs.get(value) {
            Some(at) => *at,
            None => {
                let at = match self.s.literal(value) {
                    Ok(at) => at,
                    Err(_) => {
                        let at = self.start + self.strings.len();
                        if at > 0xffff {
                            return Err(script(
                                "Native menu text exceeds the script literal range",
                            ));
                        }
                        self.strings.extend_from_slice(value);
                        self.strings.push(0);
                        at
                    }
                };
                self.string_refs.insert(value.to_vec(), at);
                at
            }
        };
        self.emit(&push(at)?);
        Ok(())
    }

    fn integer(&mut self, value: usize) -> Result<(), MenuError> {
        self.emit(&[&[0x01][..], &be(value, 4)?].concat());
        Ok(())
    }

    fn builtin(&mut self, function: usize, pop: bool) -> Result<(), MenuError> {
        self.emit(&[&[0x13][..], &be(function, 2)?].concat());
        if pop {
            self.emit(&[0x43]);
        }
        Ok(())
    }

    fn assign(&mut self, local: usize) -> Result<(), MenuError> {
        let local = u8::try_from(local).map_err(|_| script("bytes must be in range(0, 256)"))?;
        self.emit(&[0x09, 0xe0, local, 0x43]);
        Ok(())
    }

    fn call(&mut self, target: i64) {
        self.fixups.push((self.code.len(), target));
        self.emit(&[0x4a, 0, 0, 0, 0]);
    }

    /// Goes to one of the new states (or PauseOptions, EnterPause).
    fn goto_new(&mut self, name: &str) -> Result<(), MenuError> {
        let state = *self
            .refs
            .get(name)
            .ok_or_else(|| script(format!("no state {name}")))?;
        self.emit(&[&[0x47][..], &be(state, 4)?].concat());
        Ok(())
    }

    /// Goes to the existing state whose ref was `state`.
    fn goto_old(&mut self, state: usize) -> Result<(), MenuError> {
        let state = (self.relocated)(state + 4) - 4;
        self.emit(&[&[0x47][..], &be(state, 4)?].concat());
        Ok(())
    }

    fn sound(&mut self, name: &[u8]) -> Result<(), MenuError> {
        self.string(name)?;
        self.builtin(0x5c, true)
    }

    fn animation(&mut self, index: usize, layer: usize) -> Result<(), MenuError> {
        self.string(b"PauseText")?;
        self.integer(layer)?;
        self.string(format!("menuItem{index}Select").as_bytes())?;
        self.integer(3)?;
        self.builtin(0x5a, true)
    }

    fn value(&mut self, index: usize, selected: bool) -> Result<(), MenuError> {
        let (left, right) = if selected {
            ("ArrowSelectionL", "ArrowSelectionR")
        } else {
            ("ArrowSpacer", "ArrowSpacer")
        };
        self.string(format!("[{left}][PauseMenu:optSet{index}][{right}]").as_bytes())?;
        self.assign(9 + index)
    }

    fn query(&mut self, index: usize, function: usize, selected: bool) -> Result<(), MenuError> {
        self.builtin(function, false)?;
        self.assign(0x11 + index)?;
        self.value(index, selected)
    }

    fn slider(&mut self, index: usize) -> Result<(), MenuError> {
        self.string(format!("sliderBar{}", index + 4).as_bytes())?;
        self.integer(64)?;
        self.string(b"")?;
        self.emit(&[&[0x00][..], &0.0f32.to_be_bytes()].concat());
        self.builtin(0xf50 + index, false)?;
        // Timeline seconds = percent / 30.
        self.emit(&[&[0x2b, 0x00][..], &30.0f32.to_be_bytes(), &[0x24]].concat());
        self.integer(5)?;
        self.builtin(0x5a, true)
    }

    fn bars(&mut self, show: bool) -> Result<(), MenuError> {
        for index in 0..6 {
            self.string(format!("sliderBar{}", index + 4).as_bytes())?;
        }
        self.integer(6)?;
        self.builtin(if show { 0x58 } else { 0x59 }, true)
    }

    fn begin(&mut self, name: &str, kind: &'static str) {
        self.handlers
            .insert((name.to_string(), kind), self.code.len());
    }

    fn end(&mut self) {
        self.emit(&[0x49]);
    }

    /// The Cheats page's eight visible rows from cheat `first`; the last always shows Multipliers.
    fn cheat_window(&mut self, first: usize) -> Result<(), MenuError> {
        for slot in 0..8 {
            let row = if slot < 7 { first + slot } else { 8 };
            self.string(CHEAT_NAMES[row].as_bytes())?;
            self.assign(1 + slot)?;
            if row < 8 {
                self.query(slot, 0xf00 + row, false)?;
            } else {
                self.string(b"")?;
                self.assign(9 + slot)?;
            }
        }
        Ok(())
    }

    fn selection(
        &mut self,
        name: &str,
        index: usize,
        function: Option<usize>,
        selected: bool,
        cheat: bool,
    ) -> Result<(), MenuError> {
        self.begin(name, if selected { "entry" } else { "update" });
        if selected {
            self.builtin(0x6b, true)?;
            if cheat {
                let scrolled = name == "NativeMultipliersLink"
                    || name
                        .chars()
                        .last()
                        .and_then(|c| c.to_digit(10))
                        .is_some_and(|digit| digit >= 6);
                self.cheat_window(usize::from(scrolled))?;
            }
            if let Some(function) = function {
                self.query(index, function, true)?;
            }
        } else if function.is_some() {
            self.value(index, false)?;
        }
        self.animation(index, if selected { 8 } else { 9 })?;
        self.end();
        Ok(())
    }

    fn navigation(&mut self, name: &str, index: usize, rows: &[String]) -> Result<(), MenuError> {
        let n = rows.len();
        for (kind, target) in [
            ("up", &rows[(index + n - 1) % n]),
            ("down", &rows[(index + 1) % n]),
        ] {
            self.begin(name, kind);
            self.sound(b"SND_MM_NAV_UPDN")?;
            self.goto_new(target)?;
            self.end();
        }
        Ok(())
    }
}

/// The literal operand `entry` assigns to `local` (the instruction before `09 e0 <local> 43`).
fn assigned(entry: &[u8], local: u8) -> Result<usize, MenuError> {
    let at = find(entry, &[0x09, 0xe0, local, 0x43], 0, entry.len())
        .ok_or_else(|| script("subsection not found"))?;
    be16(
        entry,
        at.checked_sub(2)
            .ok_or_else(|| script("index out of range"))?,
    )
}

/// Native Options > Cheats > Multipliers, sharing the port's existing settings; the code of the
/// script so far is at `spans`.
pub fn native_cheats(data: &mut Vec<u8>, spans: &[(usize, usize)]) -> Result<(), MenuError> {
    let s = Script::parse(data)?;
    let base = s.base;
    let options = s.one("PauseOptions")?.clone();
    let game = s.one("HLGSettings")?.clone();
    let audio = s.one("HLAudio")?.clone();
    let enter = s.one("EnterPause")?.clone();
    let (opt_at, pages_at) = (options.at + options.size, enter.at + enter.size);
    let (up, down, confirm) = (UP.to_vec(), DOWN.to_vec(), CONFIRM.to_vec());
    let (left, right) = (vec![0xe6, 0xf2], vec![0xe2, 0xee]);
    let toggle = vec![0xe6, 0xe2, 0xee, 0xf2, 0xe9];
    let link = || vec![up.clone(), down.clone(), confirm.clone()];
    let mut specs = vec![
        Spec::new("HLNativeCheats", "options", link()),
        Spec::new("NativeCheats", "pause", vec![vec![0xeb]]),
    ];
    for i in 0..8 {
        let events = vec![toggle.clone(), up.clone(), down.clone()];
        specs.push(Spec::new(
            &format!("NativeCheat{i}"),
            "NativeCheats",
            events,
        ));
    }
    specs.push(Spec::new("NativeMultipliersLink", "NativeCheats", link()));
    specs.push(Spec::new("NativeMultipliers", "pause", vec![vec![0xeb]]));
    for i in 0..6 {
        let events = vec![left.clone(), right.clone(), up.clone(), down.clone()];
        specs.push(Spec::new(
            &format!("NativeMultiplier{i}"),
            "NativeMultipliers",
            events,
        ));
    }
    specs.push(Spec::new(
        "NativeMultiplierReset",
        "NativeMultipliers",
        link(),
    ));
    let option_size = specs[0].size;
    let pages_size = specs.iter().map(|spec| spec.size).sum::<usize>() - option_size;
    let relocated = |at: usize| {
        at + (if at >= opt_at { option_size } else { 0 })
            + (if at >= pages_at { pages_size } else { 0 })
    };
    let mut refs: HashMap<String, usize> = HashMap::new();
    refs.insert("options".to_string(), relocated(options.at) - 4);
    refs.insert("pause".to_string(), relocated(enter.at) - 4);
    refs.insert(specs[0].name.clone(), opt_at - 4);
    let mut at = pages_at + option_size;
    for spec in &specs[1..] {
        refs.insert(spec.name.clone(), at - 4);
        at += spec.size;
    }

    // New symbol records go before the old name pool; existing symbol ids stay valid.
    let old_size = le32(data, base - 0x40 + 4)?;
    let mut symbols = bytes_at(data, base + s.symtab, base + old_size)?.to_vec();
    let mut pool = be32(&symbols, 12)? + 4;
    let mut pos = 16;
    while pos < pool {
        pool = pool.min(be32(&symbols, pos + 4)? + 4);
        pos += 8;
    }
    let symbol_ids: HashMap<&str, usize> = specs
        .iter()
        .enumerate()
        .map(|(i, spec)| (spec.name.as_str(), pool - 4 + i * 8))
        .collect();
    let delta = specs.len() * 8;
    for pos in (8..pool).step_by(8) {
        let name = be32(&symbols, pos + 4)? + delta;
        put(&mut symbols, pos + 4, &be(name, 4)?)?;
    }
    let (mut new_symbols, mut names) = (Vec::new(), Vec::new());
    for spec in &specs {
        new_symbols.extend([0, 0, 0, 0]);
        new_symbols.extend(be(symbols.len() + delta + names.len() - 4, 4)?);
        names.extend_from_slice(spec.name.as_bytes());
        names.push(0);
    }
    splice(&mut symbols, pool, pool, &new_symbols);
    symbols.extend(names);

    // Reuse the native repeat gate used by the subtitle toggle and volume sliders.
    let template = s
        .by_name("HLSubtitleDis")
        .first()
        .map(|record| (*record).clone())
        .ok_or_else(|| script("HLSubtitleDis: 0 states"))?;
    let gate = transition(&template, &[0xe6, 0xe2, 0xee, 0xf2])?;
    let body = s.block_bytes(gate)?;
    if body.first() != Some(&0x4a) {
        return Err(script("Native repeat gate not found"));
    }
    let repeat = gate as i64 + 5 + i64::from(i32::from_be_bytes(word(body, 1)?));

    let mut e = Emitter {
        s: &s,
        start: pool_end(&s)?,
        code: Vec::new(),
        strings: Vec::new(),
        string_refs: HashMap::new(),
        fixups: Vec::new(),
        handlers: HashMap::new(),
        refs: &refs,
        relocated: &relocated,
    };
    e.selection("HLNativeCheats", 2, None, true, false)?;
    e.selection("HLNativeCheats", 2, None, false, false)?;
    for (kind, target) in [("up", game.at - 4), ("down", audio.at - 4)] {
        e.begin("HLNativeCheats", kind);
        e.sound(b"SND_MM_NAV_UPDN")?;
        e.goto_old(target)?;
        e.end();
    }
    e.begin("HLNativeCheats", "confirm");
    e.sound(b"SND_MM_SELECT")?;
    e.goto_new("NativeCheats")?;
    e.end();

    let cheat_rows: Vec<String> = (0..8)
        .map(|i| format!("NativeCheat{i}"))
        .chain(["NativeMultipliersLink".to_string()])
        .collect();
    let multiplier_rows: Vec<String> = (0..6)
        .map(|i| format!("NativeMultiplier{i}"))
        .chain(["NativeMultiplierReset".to_string()])
        .collect();
    for (page, title, row_names, rows, is_multiplier) in [
        (
            "NativeCheats",
            "Cheats",
            &CHEAT_NAMES[..],
            &cheat_rows,
            false,
        ),
        (
            "NativeMultipliers",
            "Multipliers",
            &MULTIPLIER_NAMES[..],
            &multiplier_rows,
            true,
        ),
    ] {
        let functions = if is_multiplier { 0xf20 } else { 0xf00 };
        e.begin(page, "entry");
        e.string(title.as_bytes())?;
        e.assign(0)?;
        for index in 0..8 {
            e.string(
                row_names
                    .get(index)
                    .map_or(&b""[..], |name| name.as_bytes()),
            )?;
            e.assign(1 + index)?;
            e.string(b"")?;
            e.assign(9 + index)?;
        }
        for local in 0x19..0x1d {
            e.string(b"")?;
            e.assign(local)?;
        }
        let shown = if is_multiplier { 6 } else { 8 };
        for index in 0..shown {
            e.query(index, functions + index, false)?;
            if is_multiplier {
                e.slider(index)?;
            }
        }
        if is_multiplier {
            e.bars(true)?;
        } else {
            e.cheat_window(0)?;
        }
        e.goto_new(&rows[0])?;
        e.end();
        e.begin(page, "update");
        if is_multiplier {
            e.bars(false)?;
        }
        e.end();
        e.begin(page, "back");
        e.sound(b"SND_MM_BACK")?;
        if is_multiplier {
            e.goto_new("NativeCheats")?;
        } else {
            e.goto_old(options.at - 4)?;
        }
        e.end();
        for (index, name) in rows.iter().enumerate() {
            let function = (index + 1 < rows.len()).then_some(functions + index);
            let slot = if !is_multiplier && index == 8 {
                7
            } else {
                index - usize::from(!is_multiplier && (6..8).contains(&index))
            };
            e.selection(name, slot, function, true, !is_multiplier)?;
            e.selection(name, slot, function, false, false)?;
            e.navigation(name, index, rows)?;
            if function.is_none() {
                e.begin(name, "confirm");
                e.sound(b"SND_MM_SELECT")?;
                if is_multiplier {
                    e.builtin(0xf60, true)?;
                    for i in 0..6 {
                        e.query(i, 0xf20 + i, false)?;
                        e.slider(i)?;
                    }
                } else {
                    e.goto_new("NativeMultipliers")?;
                }
                e.end();
            } else if is_multiplier {
                for (kind, function) in [("left", 0xf30 + index), ("right", 0xf40 + index)] {
                    e.begin(name, kind);
                    e.call(repeat);
                    e.query(index, function, true)?;
                    e.slider(index)?;
                    e.sound(b"SND_MM_NAV_LR")?;
                    e.end();
                }
            } else {
                e.begin(name, "toggle");
                e.call(repeat);
                e.query(slot, 0xf10 + index, true)?;
                e.sound(b"SND_MM_NAV_LR")?;
                e.end();
            }
        }
    }

    // Clone Options' entry, adding Cheats at line2 and shifting Audio/Display/Exit down one row.
    let old_entry = first_block(&options, 1)?;
    let original_entry = s.block_bytes(old_entry)?;
    let mut replacements: HashMap<u8, Vec<u8>> = HashMap::from([(3, b"Cheats".to_vec())]);
    for old_local in [3u8, 4, 5] {
        let value = assigned(original_entry, old_local)?;
        // The Python reads it as Latin-1 and writes it back as UTF-8.
        replacements.insert(old_local + 1, s.string(s.lit + value)?.into_bytes());
    }
    // Keep existing non-ASCII Exit labels as bytes (their source encoding is UTF-8).
    let exit = assigned(original_entry, 5)?;
    replacements.insert(6, until_nul(s.data, base + s.lit + exit).to_vec());
    let options_entry = e.code.len();
    let first = base + s.lit + old_entry;
    for op in decode(s.data, first, first + original_entry.len())? {
        let relative = op.offset - base - s.lit;
        let setter = s.data.get(op.offset + 3..op.offset + 5) == Some(&[0x09, 0xe0][..]);
        let replacement = s
            .data
            .get(op.offset + 5)
            .filter(|_| setter && op.opcode == 0x16)
            .and_then(|local| replacements.get(local));
        match (op.opcode, replacement) {
            (0x16, Some(text)) => e.string(text)?,
            (0x47, _) => e.goto_old(op.operand as usize)?,
            (0x4a, _) => e.call(relative as i64 + 5 + signed32(op.operand)),
            _ => e.emit(bytes_at(
                s.data,
                op.offset,
                op.offset + LENGTH[usize::from(op.opcode)],
            )?),
        }
    }
    let Emitter {
        start,
        mut code,
        strings,
        fixups,
        handlers,
        ..
    } = e;

    let code_start = start + strings.len();
    for (at, target) in fixups {
        put(
            &mut code,
            at + 1,
            &rel32(target - (code_start + at + 5) as i64)?,
        )?;
    }
    let mut edits = Vec::new();
    for &(a, b) in spans {
        for op in decode(s.data, base + s.lit + a, base + s.lit + b)? {
            if op.opcode == 0x47 && op.operand as usize + 4 >= opt_at {
                let state = relocated(op.operand as usize + 4) - 4;
                edits.push((op.offset + 1, be(state, 4)?));
            }
        }
    }
    let cheats_ref = opt_at - 4;
    edits.push(retarget(&s, &game, &DOWN, audio.at - 4, cheats_ref)?);
    edits.push(retarget(&s, &audio, &UP, game.at - 4, cheats_ref)?);
    let entry_item = options
        .items
        .iter()
        .position(|(kind, _)| *kind == 1)
        .ok_or_else(|| script("PauseOptions has no entry"))?;
    edits.push((
        base + options.at + 12 + 4 * entry_item,
        be(0x0100_0000 | (code_start + options_entry), 4)?,
    ));
    let displays = s.by_name("HLDisplay");
    let display_rows = [displays.first(), displays.get(1)];
    let [Some(display), Some(exit_row)] = display_rows else {
        return Err(script("HLDisplay: not two states"));
    };
    for (record, old_index) in [(&audio, 2), (*display, 3), (*exit_row, 4)] {
        for block in Script::blocks(record) {
            if !matches!(block.kind, 1 | 2) {
                continue;
            }
            let bytes = s.block_bytes(block.code)?;
            let source = push(s.literal(format!("menuItem{old_index}Select").as_bytes())?)?;
            let destination =
                push(s.literal(format!("menuItem{}Select", old_index + 1).as_bytes())?)?;
            let at = find(bytes, &source, 0, bytes.len())
                .filter(|_| count(bytes, &source) == 1)
                .ok_or_else(|| script("Options selection animation not found"))?;
            edits.push((base + s.lit + block.code + at, destination));
        }
    }
    let mut growth: HashMap<usize, usize> = HashMap::new();
    for (record, amount) in [(&options, option_size), (&enter, pages_size)] {
        for r in ancestors(&s, record)? {
            *growth.entry(r.at).or_insert(0) += amount;
        }
    }
    for record in &s.records {
        if let Some(grown) = growth.get(&record.at) {
            edits.push((
                base + record.at,
                be(0x0400_0000 | (record.size + grown), 4)?,
            ));
        }
        if record.parent + 4 >= opt_at {
            let parent = relocated(record.parent + 4) - 4;
            edits.push((base + record.at + 4, be(parent, 4)?));
        }
    }
    let symtab = s.symtab;
    for (at, value) in edits {
        put(data, at, &value)?;
    }

    let mut records = Vec::new();
    for spec in &specs {
        let mut size = spec.size;
        if spec.name == "NativeCheats" || spec.name == "NativeMultipliers" {
            size += specs
                .iter()
                .filter(|child| child.parent == spec.name)
                .map(|child| child.size)
                .sum::<usize>();
        }
        let handler = |kind: &'static str| {
            handlers
                .get(&(spec.name.clone(), kind))
                .map(|at| code_start + at)
                .ok_or_else(|| script(format!("{}: no {kind} handler", spec.name)))
        };
        let parent = refs
            .get(spec.parent)
            .ok_or_else(|| script(format!("no state {}", spec.parent)))?;
        let symbol = symbol_ids
            .get(spec.name.as_str())
            .ok_or_else(|| script(format!("no symbol {}", spec.name)))?;
        let mut record = [
            be(0x0400_0000 | size, 4)?,
            be(*parent, 4)?,
            be(*symbol, 4)?,
            be(0x0100_0000 | handler("entry")?, 4)?,
            be(0x0200_0000 | handler("update")?, 4)?,
        ]
        .concat();
        let numbered = spec.name.chars().last().is_some_and(|c| c.is_ascii_digit());
        let event_names: &[&'static str] = if spec.parent == "pause" {
            &["back"]
        } else if spec.name.starts_with("NativeMultiplier") && numbered {
            &["left", "right", "up", "down"]
        } else if spec.name.starts_with("NativeCheat") && numbered {
            &["toggle", "up", "down"]
        } else {
            &["up", "down", "confirm"]
        };
        for (event, triggers) in event_names.iter().copied().zip(&spec.events) {
            record.extend(be(0x0300_0000 | handler(event)?, 4)?);
            record.extend(trigger_words(triggers)?);
        }
        if record.len() != spec.size {
            return Err(script(format!(
                "{}: native record size mismatch",
                spec.name
            )));
        }
        records.extend(record);
    }
    let (option_record, pages) = records.split_at(option_size.min(records.len()));
    splice(
        data,
        base + symtab,
        base + old_size,
        &[strings.as_slice(), &code, &symbols].concat(),
    );
    splice(data, base + pages_at, base + pages_at, pages);
    splice(data, base + opt_at, base + opt_at, option_record);
    let states = option_size + pages_size;
    let grown = states + strings.len() + code.len() + symbols.len() - (old_size - symtab);
    for (offset, amount) in [
        (0x08, grown),
        (0x0c, states),
        (0x10, states + strings.len() + code.len()),
    ] {
        let value = le32(data, base + offset)? + amount;
        put_le32(data, base + offset, value)?;
    }
    put_le32(data, base - 0x40 + 4, old_size + grown)?;
    repad(data, base + old_size + grown, old_size, old_size + grown);
    native_multiplier_bars(data)
}

/// Dedicated sliders left of the values, preserving the original audio/display controls.
pub fn native_multiplier_bars(data: &mut Vec<u8>) -> Result<(), MenuError> {
    let all = entries(data);
    let source = all
        .iter()
        .find(|e| e.name == "gosliderbar3" && e.size != 0)
        .ok_or_else(|| script("Native slider template not recognized"))?;
    if source.size != 148 || le16(data, source.offset + 64 + 0x76)? != 3 {
        return Err(script("Native slider template not recognized"));
    }
    let source = source.offset;
    let group = bytes_at(
        data,
        source
            .checked_sub(64)
            .ok_or_else(|| script("index out of range"))?,
        source + 64 + 160 + 64,
    )?
    .to_vec();
    let reference = all
        .iter()
        .find(|e| e.name == "gosliderbar3" && e.size == 0)
        .ok_or_else(|| script("Native slider template not recognized"))?
        .offset;
    let mut instance_ids = Vec::new();
    for entry in &all {
        if entry.size == 148 && le32(data, entry.offset)? == 0x1f_0001 {
            instance_ids.push(le32(data, entry.offset + 64 + 0x3c)?);
        }
    }
    let last = instance_ids
        .into_iter()
        .filter(|id| id >> 16 == 1)
        .max()
        .ok_or_else(|| script("max() iterable argument is empty"))?;
    let (mut groups, mut references) = (Vec::new(), Vec::new());
    for index in 4..10usize {
        let mut name = format!("gosliderbar{index}").into_bytes();
        name.resize(56, 0);
        let mut copy = group.clone();
        put(&mut copy, 72, &name)?;
        put_le32(&mut copy, 128 + 0x3c, last + index - 3)?;
        put(&mut copy, 128 + 0x40, &0.75f32.to_le_bytes())?;
        put(&mut copy, 128 + 0x64, &399.5f32.to_le_bytes())?;
        let y = (2.95 - 0.75 * (index - 4) as f64) as f32;
        put(&mut copy, 128 + 0x68, &y.to_le_bytes())?;
        put(&mut copy, 128 + 0x76, &(index as u16).to_le_bytes())?;
        groups.extend(copy);
        references.extend_from_slice(bytes_at(data, reference, reference + 8)?);
        references.extend(name);
    }
    splice(data, reference + 64, reference + 64, &references);
    let after = source + 64 + 160 + 64;
    splice(data, after, after, &groups);
    Ok(())
}

/// Adds the rows to the `R_UI.WAD` in `data`; an error when this is not the script the patch was
/// made for. The label is `language`'s (`SPARTAN_LANGUAGE` of the process when `None`).
///
/// Both rows are new state records. The pause row's replaces WadChoose's, a cheat-only level
/// loader right after PauseMain's subtree; the Options row's goes after HLDisplay. Each record is a
/// little bigger than what it replaces or follows, so the state table grows and everything after
/// it moves: the header, the sizes of the subtrees the records join and leave, the parent of later
/// records, every goto to a later state and the WAD entry are corrected. The code of both rows and
/// their label go over WadChoose's dead code, which leaves the literal pool and the symbol table
/// where they were relative to the code. Then [`display_health_bar`] and [`native_cheats`].
pub fn patch(data: &[u8], language: Option<&str>) -> Result<Vec<u8>, MenuError> {
    let mut work = data.to_vec();
    let s = Script::parse(&work)?;
    let base = s.base;
    let main = s.one("PauseMain")?.clone();
    let cont = s.one("HLContinue")?.clone();
    let quit = s.one("HLQuitGame")?.clone();
    let debug = s.one("WadChoose")?.clone();
    let opt = s.one("PauseOptions")?.clone();
    let ctl = s.one("HLControls")?.clone();
    let dsp = s.one("HLDisplay")?.clone();
    let n = ROW_SIZE;
    if main.at + main.size != debug.at || debug.parent != main.parent || debug.size > n {
        return Err(script("WadChoose is not where the patch expects it"));
    }
    if dsp.at + dsp.size != opt.at + opt.size || dsp.parent != opt.at - 4 {
        return Err(script("PauseOptions is not laid out as expected"));
    }
    let starts: HashSet<usize> = s.records.iter().map(|r| r.at).collect();

    // The two splices, in table order: WadChoose's record is replaced, the Options row is added.
    let (pause_at, options_at) = (debug.at, opt.at + opt.size);
    // (first offset it moves, bytes)
    let deltas = [(debug.at + debug.size, n - debug.size), (options_at, n)];
    // Where the old record starting at `at` ends up.
    let relocated = |at: usize| {
        at + deltas
            .iter()
            .filter(|(end, _)| *end <= at)
            .map(|(_, delta)| delta)
            .sum::<usize>()
    };
    // The state refs of the new records.
    let (pause_ref, options_ref) = (pause_at - 4, options_at + deltas[0].1 - 4);
    let state_ref = |record: &Record| relocated(record.at) - 4;

    // WadChoose's code is dead once its record is replaced: nothing else may use it.
    let refs = s.code_refs();
    let own: Vec<usize> = debug
        .items
        .iter()
        .filter(|(kind, _)| matches!(kind, 1..=3))
        .map(|(_, code)| *code)
        .collect();
    let (Some(&start), Some(&last)) = (own.iter().min(), own.iter().max()) else {
        return Err(script("min() iterable argument is empty"));
    };
    let end = s.block_end(last)?;
    if refs
        .iter()
        .any(|r| !own.contains(r) && (start..end).contains(r))
    {
        return Err(script("WadChoose code is shared"));
    }
    // No call into it from elsewhere (a 0x4a byte that is not an opcode can only refuse too much).
    let code = bytes_at(&work, base + s.lit, base + s.symtab)?;
    for (relative, window) in code
        .windows(5)
        .take(code.len().saturating_sub(5))
        .enumerate()
    {
        if window[0] == 0x4a && !(start..end).contains(&relative) {
            let rel = i32::from_be_bytes([window[1], window[2], window[3], window[4]]);
            let target = relative as i64 + 5 + i64::from(rel);
            if (start as i64..end as i64).contains(&target) {
                return Err(script("WadChoose code is called from elsewhere"));
            }
        }
    }
    let label = resolve_label(language, &|key| std::env::var(key).ok());
    let (pause_code, pause_pieces) = row_code(
        &s,
        &quit,
        start,
        state_ref(&quit),
        state_ref(&cont),
        b"menuItem5Select",
        Some(label.exit),
    )?;
    let options_start = start + pause_code.len();
    let (options_code, options_pieces) = row_code(
        &s,
        &quit,
        options_start,
        state_ref(&dsp),
        state_ref(&ctl),
        b"menuItem4Select",
        None,
    )?;
    let failsafe_start = options_start + options_code.len();
    let (failsafe, redirects) = failsafe_code(&s, failsafe_start, &relocated)?;
    let new_code_end = failsafe_start + failsafe.len();
    if new_code_end > end {
        return Err(script("no room for the new rows"));
    }

    // Edits in the layout as it is now. First every goto to a state that moves (or to WadChoose,
    // which the pause row replaces), then the rows' own wiring.
    let mut edits = Vec::new();
    let spans = s.spans()?;
    for &(a, b) in &spans {
        if (start..end).contains(&a) {
            continue; // WadChoose's dead code: the new code goes over it
        }
        for op in decode(&work, base + s.lit + a, base + s.lit + b)? {
            if op.opcode == 0x47 {
                let target = op.operand as usize + 4;
                let moved = if target == debug.at {
                    pause_ref
                } else {
                    relocated(target) - 4
                };
                if moved != target - 4 {
                    edits.push((op.offset + 1, be(moved, 4)?));
                }
            }
        }
    }
    edits.push(retarget(&s, &cont, &UP, quit.at - 4, pause_ref)?);
    edits.push(retarget(&s, &quit, &DOWN, cont.at - 4, pause_ref)?);
    edits.push(retarget(&s, &ctl, &UP, dsp.at - 4, options_ref)?);
    edits.push(retarget(&s, &dsp, &DOWN, ctl.at - 4, options_ref)?);
    // PauseMain clears line5 (local 6), PauseOptions line4 (local 5): the label goes there.
    edits.push((blank_line(&s, &main, 6)?, be(start, 2)?));
    edits.push((blank_line(&s, &opt, 5)?, be(start, 2)?));
    edits.push((
        base + s.lit + start,
        [pause_code, options_code, failsafe].concat(),
    ));
    // The FailSafe states run their new blocks.
    for (record, old, at) in &redirects {
        let item = record
            .items
            .iter()
            .position(|(kind, code)| *kind == 3 && code == old)
            .ok_or_else(|| script("FailSafe transition not found"))?;
        edits.push((base + record.at + 12 + 4 * item, be(0x0300_0000 | at, 4)?));
    }
    let parents: HashMap<usize, &Record> = s.records.iter().map(|r| (r.at, r)).collect();
    // Sizes of the subtrees the new records join (+) and the replaced one leaves (-).
    let mut grow: IndexMap<usize, i64> = IndexMap::new();
    let left = parents
        .get(&(debug.parent + 4))
        .copied()
        .ok_or_else(|| script("WadChoose has no parent"))?;
    for (record, amount) in [
        (&main, n as i64),
        (&opt, n as i64),
        (left, -(debug.size as i64)),
    ] {
        for r in ancestors(&s, record)? {
            *grow.entry(r.at).or_insert(0) += amount;
        }
    }
    for (&at, &g) in &grow {
        let record = parents.get(&at).ok_or_else(|| script("state not found"))?;
        if g != 0 {
            let size = usize::try_from(record.size as i64 + g)
                .map_err(|_| script("int too big to convert"))?;
            edits.push((base + at, be(0x0400_0000 | size, 4)?));
        }
    }
    for r in &s.records {
        let parent = r.parent + 4;
        if r.at != debug.at && starts.contains(&parent) && relocated(parent) != parent {
            edits.push((base + r.at + 4, be(relocated(parent) - 4, 4)?));
        }
    }
    for (at, value) in edits {
        put(&mut work, at, &value)?;
    }

    let old_size = le32(&work, base - 0x40 + 4)?;
    let grown: usize = deltas.iter().map(|(_, delta)| delta).sum();
    let options_row = row_record(state_ref(&opt), dsp.id, &options_pieces)?;
    splice(
        &mut work,
        base + options_at,
        base + options_at,
        &options_row,
    );
    let pause_row = row_record(state_ref(&main), debug.id, &pause_pieces)?;
    splice(
        &mut work,
        base + pause_at,
        base + pause_at + debug.size,
        &pause_row,
    );
    // The script's size, code start and symbol table.
    for offset in [0x08, 0x0c, 0x10] {
        let value = le32(&work, base + offset)? + grown;
        put_le32(&mut work, base + offset, value)?;
    }
    put_le32(&mut work, base - 0x40 + 4, old_size + grown)?;
    repad(
        &mut work,
        base + old_size + grown,
        old_size,
        old_size + grown,
    );
    let mut spans: Spans = spans
        .into_iter()
        .filter(|(a, _)| !(start..end).contains(a))
        .collect();
    spans.push((pause_pieces.entry, new_code_end));
    display_health_bar(&mut work, &mut spans)?;
    native_cheats(&mut work, &spans)?;
    Ok(work)
}

/// `str(Path(path))` of the Python: the platform's separators, without `.` parts or doubled
/// separators.
pub fn python_path(path: &Path) -> String {
    let parts: PathBuf = path
        .components()
        .filter(|part| *part != Component::CurDir)
        .collect();
    if parts.as_os_str().is_empty() {
        ".".to_string()
    } else {
        parts.to_string_lossy().into_owned()
    }
}

/// `st_mtime_ns` of the Python.
fn mtime_ns(meta: &std::fs::Metadata) -> io::Result<i64> {
    let modified = meta.modified()?;
    match modified.duration_since(std::time::UNIX_EPOCH) {
        Ok(after) => i64::try_from(after.as_nanos()),
        Err(before) => i64::try_from(before.duration().as_nanos()).map(|nanos| -nanos),
    }
    .map_err(io::Error::other)
}

/// The patched copy of `source` (the game's own `R_UI.WAD` when `None`, else another patched copy)
/// for the runner to mount; `None` when there is no source or the script is not the one this patch
/// knows (said on `log`). The copy in `out/pause_menu` is kept until the source or the language
/// changes; `stamp.json` records them as the Python's `json.dumps` does.
pub fn build(
    game: &Path,
    out: &Path,
    source: Option<&Path>,
    language: Option<&str>,
    log: &mut dyn FnMut(String),
) -> Result<Option<PathBuf>, MenuError> {
    let source = match source.filter(|source| !source.as_os_str().is_empty()) {
        Some(source) => source.to_path_buf(),
        None => game.join(WAD),
    };
    let target = out.join("pause_menu");
    let copy = target.join("R_UI.WAD");
    if !source.is_file() {
        return Ok(None);
    }
    let label = resolve_label(language, &|key| std::env::var(key).ok());
    let meta = std::fs::metadata(&source)?;
    let stamp = python_json(&serde_json::json!([
        VERSION,
        python_path(&source),
        meta.len(),
        mtime_ns(&meta)?,
        label.key
    ]))?;
    let stamp_file = target.join("stamp.json");
    if std::fs::read_to_string(&stamp_file).ok().as_deref() != Some(stamp.as_str()) {
        let _ = std::fs::remove_dir_all(&target);
        // As the Python's mkdir(parents=True): a folder that could not be removed is an error.
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::create_dir(&target)?;
        let data = match patch(&std::fs::read(&source)?, Some(label.key)) {
            Ok(data) => data,
            Err(MenuError::Script(error)) => {
                log(format!("Pause menu: {error}; no Exit to Desktop row"));
                return Ok(None);
            }
            Err(error) => return Err(error),
        };
        atomic_write(&copy, &data)?;
        atomic_write(&stamp_file, stamp.as_bytes())?; // last: an interrupted build is redone
    }
    if !copy.is_file() {
        return Ok(None);
    }
    Ok(Some(copy))
}

#[cfg(test)]
mod tests {
    //! `tests/test_pause_menu.py::Refusals` and `Labels`, which need no files of the game; the tests
    //! on the real `R_UI.WAD` (`GameScript`, `tests/test_native_cheats.py`) are in `tests/dump.rs`.
    use super::*;

    /// A WAD of one entry (`tests/test_pause_menu.py::wad`).
    fn wad(name: &str, payload: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend(1u16.to_le_bytes());
        out.extend(0u16.to_le_bytes());
        out.extend((payload.len() as u32).to_le_bytes());
        let mut field = name.as_bytes().to_vec();
        field.resize(56, 0);
        out.extend(field);
        let mut data = payload.to_vec();
        data.resize((payload.len() + 15) & !15, 0);
        out.extend(data);
        out
    }

    fn label(key: &str) -> &'static Label {
        LABELS.iter().find(|label| label.key == key).unwrap()
    }

    fn no_env(_: &str) -> Option<String> {
        None
    }

    /// The stamp `build` keeps beside the copy (`json.dumps([VERSION, str(source), size, mtime, key])`).
    fn stamp_of(source: &Path, key: &str) -> String {
        let meta = std::fs::metadata(source).unwrap();
        crate::core::patches::python_json(&serde_json::json!([
            VERSION,
            python_path(source),
            meta.len(),
            mtime_ns(&meta).unwrap(),
            key
        ]))
        .unwrap()
    }

    /// A folder with a WAD without the script at `R_UI.WAD`, and `pause_menu/` holding a fake copy
    /// and the stamp of it for `key`.
    fn cached(key: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("R_UI.WAD");
        std::fs::write(&source, wad("TXRX_R_UI", b"xxxxxxxx")).unwrap();
        let target = dir.path().join("pause_menu");
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(target.join("R_UI.WAD"), b"dummy_en").unwrap();
        std::fs::write(target.join("stamp.json"), stamp_of(&source, key)).unwrap();
        (dir, source)
    }

    // -- Refusals -----------------------------------------------------------------------------------

    #[test]
    fn wad_without_the_script() {
        assert!(matches!(
            patch(&wad("TXRX_R_UI", b"xxxxxxxx"), None),
            Err(MenuError::Script(_))
        ));
    }

    #[test]
    fn build_leaves_no_copy() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("R_UI.WAD");
        std::fs::write(&source, wad("TXRX_R_UI", b"xxxxxxxx")).unwrap();
        let mut log = Vec::new();
        let made = build(dir.path(), dir.path(), Some(&source), None, &mut |line| {
            log.push(line)
        });
        assert_eq!(made.unwrap(), None);
        assert!(!dir.path().join("pause_menu").join("R_UI.WAD").exists());
        assert_eq!(
            log,
            ["Pause menu: no SMI_R_UI in this WAD; no Exit to Desktop row"]
        );
    }

    // -- Labels -------------------------------------------------------------------------------------

    #[test]
    fn resolve_label_by_id() {
        // The Python takes the ids as numbers too; they are the same keys of LANGUAGE_ALIASES.
        for (id, key) in [
            ("0", "ja"),
            ("10", "zh"),
            ("1", "en"),
            ("16", "pl"),
            ("8", "ru"),
            ("7", "pt"),
            ("17", "pt"),
        ] {
            assert_eq!(resolve_label(Some(id), &no_env), label(key), "{id}");
        }
    }

    #[test]
    fn resolve_label_by_code() {
        for (code, key) in [
            ("ja", "ja"),
            ("JA", "ja"),
            ("zh", "zh"),
            ("ZH", "zh"),
            ("zh-tw", "zh"),
            ("en", "en"),
            ("es", "es"),
            ("es-419", "es"),
            ("fr", "fr"),
            ("de", "de"),
            ("it", "it"),
            ("pl", "pl"),
            ("pt", "pt"),
            ("pt-br", "pt"),
            ("ru", "ru"),
        ] {
            assert_eq!(resolve_label(Some(code), &no_env), label(key), "{code}");
        }
    }

    #[test]
    fn resolve_label_fallback() {
        assert_eq!(resolve_label(Some("999"), &no_env), label("en"));
        assert_eq!(resolve_label(Some("unknown"), &no_env), label("en"));
    }

    #[test]
    fn resolve_label_environment() {
        for (value, key) in [
            (Some("0"), "ja"),
            (Some("10"), "zh"),
            (Some("16"), "pl"),
            (None, "en"),
        ] {
            let env = |name: &str| {
                if name == "SPARTAN_LANGUAGE" {
                    value.map(str::to_string)
                } else {
                    None
                }
            };
            assert_eq!(resolve_label(None, &env), label(key), "{value:?}");
        }
    }

    #[test]
    fn build_cache_stamp_tracks_language() {
        let (dir, source) = cached("en");
        // Switching the language to 'ja' invalidates the copy: the rebuild finds no script here.
        let made = build(
            dir.path(),
            dir.path(),
            Some(&source),
            Some("ja"),
            &mut |_| {},
        );
        assert_eq!(made.unwrap(), None);
        assert!(!dir.path().join("pause_menu").join("R_UI.WAD").exists());
    }

    // -- New ----------------------------------------------------------------------------------------

    #[test]
    fn build_reuses_the_copy_while_the_stamp_matches() {
        let (dir, source) = cached("en");
        let mut log = Vec::new();
        let made = build(
            dir.path(),
            dir.path(),
            Some(&source),
            Some("EN "),
            &mut |line| log.push(line),
        );
        let copy = dir.path().join("pause_menu").join("R_UI.WAD");
        assert_eq!(made.unwrap(), Some(copy.clone()));
        assert_eq!(std::fs::read(copy).unwrap(), b"dummy_en");
        assert!(log.is_empty(), "{log:?}");
    }

    #[test]
    fn decode_splits_the_code_by_opcode_length() {
        let code = [
            0x16, 0x00, 0x05, 0x01, 0, 0, 0, 7, 0x43, 0x4a, 0xff, 0xff, 0xff, 0xf0, 0x49,
        ];
        let op = |offset, opcode, operand| Op {
            offset,
            opcode,
            operand,
        };
        assert_eq!(
            decode(&code, 0, code.len()).unwrap(),
            [
                op(0, 0x16, 5),
                op(3, 0x01, 7),
                op(8, 0x43, 0),
                op(9, 0x4a, 0xffff_fff0),
                op(14, 0x49, 0)
            ]
        );
        assert_eq!(
            decode(&code, 0, 12).unwrap_err().to_string(),
            "code overruns its block"
        );
        assert_eq!(
            decode(&[0x4c], 0, 1).unwrap_err().to_string(),
            "unknown opcode 0x4c"
        );
        let lengths: Vec<(usize, usize)> = [0x00, 0x01, 0x02, 0x18, 0x19, 0x47, 0x4a, 0x4b]
            .iter()
            .map(|op| (*op, LENGTH[*op]))
            .collect();
        assert_eq!(
            lengths,
            [
                (0x00, 5),
                (0x01, 5),
                (0x02, 3),
                (0x18, 3),
                (0x19, 1),
                (0x47, 5),
                (0x4a, 5),
                (0x4b, 1)
            ]
        );
        assert_eq!(LENGTH.len(), 0x4c);
    }

    #[test]
    fn be_refuses_a_value_that_does_not_fit() {
        assert_eq!(be(0x1234, 2).unwrap(), [0x12, 0x34]);
        assert_eq!(be(5, 4).unwrap(), [0, 0, 0, 5]);
        assert!(be(0x1_0000, 2).is_err());
    }

    /// A WAD with a two-state `SMI_R_UI`: Root (entry 0 clears local 6) and its child Child (entry
    /// 8 pushes "Hi"; UP at 13 goes to Root), then the literal pool ("" at 19, "Hi" at 20) and the
    /// symbol table (Root id 4, Child id 12).
    fn tiny_script() -> Vec<u8> {
        tiny_script_with_root_parent(0xffff_ffff)
    }

    /// [`tiny_script`] with `parent` as Root's parent word (0x14 is Root's own state ref).
    fn tiny_script_with_root_parent(parent: u32) -> Vec<u8> {
        let mut smi = vec![0u8; 0x18];
        let (code, symtab) = (0x44u32, 0x48u32 + 23);
        smi[0x0c..0x10].copy_from_slice(&code.to_le_bytes());
        smi[0x10..0x14].copy_from_slice(&symtab.to_le_bytes());
        // Root at 0x18 (subtree 44 bytes), Child at 0x28 (28 bytes, parent ref 0x14).
        smi.extend([4, 0, 0, 44]);
        smi.extend(parent.to_be_bytes());
        smi.extend([0, 0, 0, 4, 1, 0, 0, 0]);
        smi.extend([4, 0, 0, 28, 0, 0, 0, 0x14, 0, 0, 0, 12]);
        smi.extend([1, 0, 0, 8, 3, 0, 0, 13, 0x80, 0, 0, 0xe0, 0, 0, 0, 0xec]);
        smi.extend([0; 4]); // code start .. code refs' base
        smi.extend([0x16, 0x00, 0x13, 0x09, 0xe0, 0x06, 0x43, 0x49]); // 0: line5 = ""
        smi.extend([0x16, 0x00, 0x14, 0x43, 0x49]); // 8
        smi.extend([0x47, 0, 0, 0, 0x14, 0x49]); // 13: go to Root
        smi.extend(b"\0Hi\0"); // 19, 20
        smi.extend([0; 8]);
        smi.extend([0, 0, 0, 0, 0, 0, 0, 20, 0, 0, 0, 0, 0, 0, 0, 25]);
        smi.extend(b"Root\0Child\0");
        wad("SMI_R_UI", &smi)
    }

    #[test]
    fn a_synthetic_script_parses_into_states_blocks_and_literals() {
        let data = tiny_script();
        let s = Script::parse(&data).unwrap();
        assert_eq!((s.base, s.code, s.lit, s.symtab), (0x40, 0x44, 0x48, 0x5f));
        assert_eq!(s.records.len(), 2);
        let (root, child) = (s.one("Root").unwrap(), s.one("Child").unwrap());
        assert_eq!(
            (root.at, root.size, child.at, child.parent),
            (0x18, 44, 0x28, 0x14)
        );
        assert_eq!(s.name_of(child), Some("Child"));
        assert_eq!(
            s.one("Nothing").unwrap_err().to_string(),
            "Nothing: 0 states"
        );
        assert_eq!(
            Script::blocks(child),
            [
                Block {
                    kind: 1,
                    code: 8,
                    triggers: vec![]
                },
                Block {
                    kind: 3,
                    code: 13,
                    triggers: vec![0xe0, 0xec]
                }
            ]
        );
        assert_eq!(s.code_refs(), [0, 8, 13]);
        assert_eq!(s.block_bytes(8).unwrap(), [0x16, 0x00, 0x14, 0x43, 0x49]);
        assert_eq!(s.literal(b"Hi").unwrap(), 20);
        assert_eq!(s.string(s.lit + 20).unwrap(), "Hi");
        assert_eq!(
            s.literal(b"it's").unwrap_err().to_string(),
            "literal b\"it's\" not found"
        );
        // The last block ends where the pool starts, at the smallest literal operand.
        assert_eq!(s.spans().unwrap(), [(0, 8), (8, 13), (13, 19)]);
        assert_eq!(transition(child, &UP).unwrap(), 13);
        assert_eq!(
            transition(child, &DOWN).unwrap_err().to_string(),
            "transition [228, 240] not found"
        );
        let at = s.base + s.lit + 13 + 1;
        assert_eq!(
            retarget(&s, child, &UP, 0x14, 0x99).unwrap(),
            (at, vec![0, 0, 0, 0x99])
        );
        assert_eq!(blank_line(&s, root, 6).unwrap(), s.base + s.lit + 1);
        assert_eq!(
            blank_line(&s, child, 6).unwrap_err().to_string(),
            "line 5 assignment not found"
        );
    }

    #[test]
    fn a_cyclic_state_tree_is_an_error_not_a_hang() {
        // Root names itself as its parent: the walk up from Child never reaches the top. It runs on
        // a thread so that a walk without a bound fails the test instead of hanging it.
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let data = tiny_script_with_root_parent(0x14);
            let outcome = Script::parse(&data).and_then(|s| {
                let child = s.one("Child")?;
                ancestors(&s, child).map(|chain| chain.len())
            });
            let _ = sender.send(outcome.map_err(|error| error.to_string()));
        });
        match receiver.recv_timeout(std::time::Duration::from_millis(300)) {
            Ok(outcome) => assert_eq!(outcome, Err("state tree has a cycle".to_string())),
            Err(_) => panic!("ancestors did not return within 300 ms on a cyclic tree"),
        }
        // A tree: the record, then its ancestors up to the root.
        let data = tiny_script();
        let s = Script::parse(&data).unwrap();
        let chain: Vec<usize> = ancestors(&s, s.one("Child").unwrap())
            .unwrap()
            .iter()
            .map(|record| record.at)
            .collect();
        assert_eq!(chain, [0x28, 0x18]);
    }

    #[test]
    fn bytes_repr_is_the_pythons() {
        assert_eq!(bytes_repr(b"menuItem4Select"), "b'menuItem4Select'");
        assert_eq!(bytes_repr(b"a'b"), "b\"a'b\"");
        assert_eq!(bytes_repr(b"a\x00\xff\\"), r"b'a\x00\xff\\'");
        assert_eq!(bytes_repr(b"a'\"b"), r#"b'a\'"b'"#);
    }

    #[cfg(windows)]
    #[test]
    fn python_path_is_the_str_of_a_python_path() {
        assert_eq!(
            python_path(Path::new(r"C:\game/EXEC//WAD/./R_UI.WAD")),
            r"C:\game\EXEC\WAD\R_UI.WAD"
        );
        assert_eq!(python_path(Path::new(".")), ".");
        assert_eq!(python_path(Path::new("./a")), "a");
    }
}
