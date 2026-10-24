//! Controller button glyphs for the game's own prompts (Python `scripts/glyphs.py`): tutorials,
//! move list, QTE buttons, and the button discs of the Muses' lyre in the Chamber of the Flame.
//!
//! The prompts are DDS textures in `R_UI.WAD` (one more copy of the QTE circle in `R_PERM.WAD`);
//! the lyre's discs are a corner of five level textures in `OLY150.WAD` (a gold mechanism with the
//! PlayStation shapes shares that corner in two more). [`build`] writes patched copies of those
//! WADs, with the glyphs of an Xbox or Nintendo controller (or the keyboard keys of
//! `SPARTAN_KEY_MAP`) drawn over the PlayStation ones, and returns the guest paths the runner mounts
//! them on. The lyre keeps its letters with the keyboard style. The game folder is never written: a
//! texture keeps its size and format, so only its pixels change inside the copy.
//!
//! WAD (little-endian): entries of a 0x40 header (u16 tag, u16 flags, u32 size, name[56]) and
//! `size` bytes padded to 16 (the EntityCount marker of level WADs, tag 0, carries a count in its
//! size field and no data). A texture entry is named `TX<hash>.<hash>.dds` and holds a short prefix
//! and a plain DDS file (DXT1 or DXT5).
//!
//! The drawing is the Python's, step by step, over the Pillow operations of [`crate::core::draw`]; only
//! the text's coverage differs by a level here and there (a port of FreeType's auto-hinter and
//! another rasterizer instead of FreeType). Fonts and the controller pictures are read from the
//! port folder at run time ([`Assets`]).

use std::borrow::Cow;
use std::cell::OnceCell;
use std::io;
use std::path::{Path, PathBuf};

use indexmap::IndexMap;
use serde_json::{json, Value};

use crate::core::draw::{
    add, composite, difference, floodfill, invert, lighter, linear_gradient, multiply, subtract,
    Draw, Filter, Font, Mode, Picture, Transpose, Window,
};
use crate::core::fs::atomic_write;
use crate::runner::dxt;

/// Of the drawing: a change redraws the cached copies.
pub const VERSION: u32 = 11;
pub const WADS: [&str; 3] = [
    "EXEC/WAD/PS4_LE/R_UI.WAD",
    "EXEC/WAD/PS4_LE/R_PERM.WAD",
    "EXEC/WAD/PS4_LE/OLY150.WAD",
];
pub const CONTROLLER_TEXTURE: &str = "TXCE352DC5";

/// How a texture is drawn. Inline: the 64 px icons inside text; QTE: the large prompt on black;
/// atlas: two QTE labels, the first upside down in the top half; lyre: the Muses' lyre discs in a
/// corner of a level texture, repainted in place; gold: the shapes engraved into the gold
/// mechanism's discs in the same corner; controller: the controller diagram.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Form {
    Inline,
    Qte,
    Atlas,
    Lyre,
    Gold,
    Controller,
}

impl Form {
    /// One of the lyre corner's forms (`LYRE_FORMS`).
    pub fn is_lyre(self) -> bool {
        matches!(self, Form::Lyre | Form::Gold)
    }
}

/// A texture of [`TEXTURES`]: its form and the game button(s) it shows (the lyre forms: the kind of
/// level texture, diffuse, specular, decal, normal or glow).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Texture {
    pub form: Form,
    pub buttons: &'static [&'static str],
}

const fn texture(form: Form, buttons: &'static [&'static str]) -> Texture {
    Texture { form, buttons }
}

/// Texture (the first hash of its name) -> how it is drawn and the game button(s) it shows.
pub const TEXTURES: [(&str, Texture); 25] = [
    ("TX1BBACF4D", texture(Form::Inline, &["cross"])),
    ("TXBA40AFD2", texture(Form::Inline, &["circle"])),
    ("TX64F3D812", texture(Form::Inline, &["square"])),
    ("TX573C282A", texture(Form::Inline, &["triangle"])),
    ("TX91A47980", texture(Form::Inline, &["l1"])),
    ("TX5CED5AE9", texture(Form::Inline, &["r1"])),
    ("TX6FC83ACE", texture(Form::Inline, &["l2"])),
    ("TX67E86446", texture(Form::Inline, &["r2"])),
    ("TX9AE3BA81", texture(Form::Inline, &["l3"])),
    ("TXFB33513A", texture(Form::Inline, &["r3"])),
    ("TX192AE9D1", texture(Form::Inline, &["options"])),
    ("TXD43D1443", texture(Form::Inline, &["touchpad_right"])),
    ("TX7FF61BF7", texture(Form::Qte, &["cross"])),
    ("TX9D590CC8", texture(Form::Qte, &["circle"])),
    ("TX1F5FC983", texture(Form::Qte, &["square"])),
    ("TXE8889579", texture(Form::Qte, &["triangle"])),
    ("TXFB91136E", texture(Form::Atlas, &["l2", "l1"])),
    ("TX0FFF3CCB", texture(Form::Atlas, &["r2", "r1"])),
    ("TX2CDFA5F8", texture(Form::Lyre, &["diffuse"])),
    ("TX4158CBF9", texture(Form::Lyre, &["specular"])),
    ("TX0C09B6DB", texture(Form::Lyre, &["decal"])),
    ("TXF489D97F", texture(Form::Lyre, &["normal"])),
    ("TXEACA5E71", texture(Form::Lyre, &["glow"])),
    ("TXBD54B283", texture(Form::Gold, &["diffuse"])),
    ("TXCEE0EE2B", texture(Form::Gold, &["specular"])),
];

/// The lyre: the top-right 512 px of the 2048 px level textures (the glow mask is 512 px, the same
/// corner at a quarter), four discs of radius 110 at these centres, one game button each.
pub const LYRE_CORNER: [usize; 4] = [1536, 0, 2048, 512];
pub const LYRE_DISCS: [(&str, (i64, i64)); 4] = [
    ("cross", (167, 110)),
    ("circle", (395, 110)),
    ("square", (167, 335)),
    ("triangle", (395, 333)),
];
pub const LYRE_RADIUS: i64 = 110;

/// `LYRE_FORMS`: the discs' inner part, the letters' cap height and whether they are carved dark.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LyreForm {
    pub inner: f64,
    pub cap: i64,
    pub dark: bool,
}

pub const LYRE_FORMS: [(Form, LyreForm); 2] = [
    (
        Form::Lyre,
        LyreForm {
            inner: 0.76,
            cap: 104,
            dark: false,
        },
    ),
    (
        Form::Gold,
        LyreForm {
            inner: 0.68,
            cap: 80,
            dark: true,
        },
    ),
];
pub const LYRE_DIM: f64 = 0.62;
pub const LYRE_DECAL: f64 = 0.83;
pub const LYRE_SHINE: u8 = 190;
pub const LYRE_RIM: u8 = 80;
pub const LYRE_STROKE: i64 = 3;
pub const LYRE_RELIEF: f32 = 1.2;
pub const GOLD_LINE: i64 = 8;

/// Drawn only with the keyboard style: the stick icons (the left one also unlettered) and the D-pad
/// icon, whose gamepad look already fits every controller.
pub const KEYBOARD_TEXTURES: [(&str, Texture); 4] = [
    ("TXDCA1B818", texture(Form::Inline, &["lstick"])),
    ("TXE2BB1428", texture(Form::Inline, &["lstick"])),
    ("TX77AAD542", texture(Form::Inline, &["rstick"])),
    ("TXBF2035D0", texture(Form::Inline, &["up"])),
];

/// The gamepad button (SDL name) that presses each game button (`src/runtime_pad.c` pad_map).
pub const DEFAULT_MAP: [(&str, &str); 12] = [
    ("cross", "a"),
    ("circle", "b"),
    ("square", "x"),
    ("triangle", "y"),
    ("l1", "leftshoulder"),
    ("r1", "rightshoulder"),
    ("l2", "lefttrigger"),
    ("r2", "righttrigger"),
    ("l3", "leftstick"),
    ("r3", "rightstick"),
    ("options", "start"),
    ("touchpad_right", "back"),
];

/// The key (SDL scancode name) that presses each game button (`src/runtime_pad.c` key_map).
pub const DEFAULT_KEYS: [(&str, &str); 13] = [
    ("cross", "Space"),
    ("circle", "Left Shift"),
    ("square", "E"),
    ("triangle", "Q"),
    ("l1", "1"),
    ("r1", "3"),
    ("l2", "R"),
    ("r2", "F"),
    ("l3", "Z"),
    ("r3", "C"),
    ("options", "Return"),
    ("touchpad_right", "Backspace"),
    ("up", "I"),
];

/// The keys that move the sticks (`src/runtime_pad.c` key_axis), fixed as in the runtime.
pub const STICK_KEYS: [(&str, &str); 2] = [("lstick", "WASD"), ("rstick", "Arrows")];

/// Key caps for the longer SDL key names (casefolded); the others print their name in capitals.
pub const KEY_LABELS: [(&str, &str); 34] = [
    ("space", "SPACE"),
    ("left shift", "SHIFT"),
    ("right shift", "R SHIFT"),
    ("left ctrl", "CTRL"),
    ("right ctrl", "R CTRL"),
    ("left alt", "ALT"),
    ("right alt", "ALT GR"),
    ("left gui", "WIN"),
    ("right gui", "WIN"),
    ("return", "ENTER"),
    ("keypad enter", "ENTER"),
    ("return2", "ENTER"),
    ("backspace", "BKSP"),
    ("tab", "TAB"),
    ("escape", "ESC"),
    ("capslock", "CAPS"),
    ("delete", "DEL"),
    ("insert", "INS"),
    ("pageup", "PG UP"),
    ("pagedown", "PG DN"),
    ("home", "HOME"),
    ("end", "END"),
    ("up", "dpup"),
    ("down", "dpdown"),
    ("left", "dpleft"),
    ("right", "dpright"),
    ("mouse left", "LMB"),
    ("mouse right", "RMB"),
    ("mouse middle", "MMB"),
    ("mouse back", "M4"),
    ("mouse forward", "M5"),
    ("wheel up", "WH UP"),
    ("wheel down", "WH DN"),
    ("arrows", "arrows"),
];

/// SDL names are positions.
pub const POSITIONS: [(&str, &str); 4] =
    [("a", "south"), ("b", "east"), ("x", "west"), ("y", "north")];

/// A controller's look: the letters on its face buttons (by position), their colours, and the
/// labels of its other buttons.
#[derive(Clone, Copy, Debug)]
pub struct Style {
    pub name: &'static str,
    pub faces: &'static [(&'static str, &'static str)],
    pub colors: &'static [(&'static str, [u8; 3])],
    pub labels: &'static [(&'static str, &'static str)],
}

pub const STYLES: [Style; 3] = [
    Style {
        name: "xbox",
        faces: &[("south", "a"), ("east", "b"), ("west", "x"), ("north", "y")],
        colors: &[
            ("a", [108, 196, 74]),
            ("b", [232, 72, 66]),
            ("x", [66, 142, 232]),
            ("y", [244, 196, 48]),
        ],
        labels: &[
            ("leftshoulder", "LB"),
            ("rightshoulder", "RB"),
            ("lefttrigger", "LT"),
            ("righttrigger", "RT"),
            ("leftstick", "LS"),
            ("rightstick", "RS"),
            ("start", "menu"),
            ("back", "view"),
        ],
    },
    Style {
        name: "nintendo",
        faces: &[("south", "b"), ("east", "a"), ("west", "y"), ("north", "x")],
        colors: &[],
        labels: &[
            ("leftshoulder", "L"),
            ("rightshoulder", "R"),
            ("lefttrigger", "ZL"),
            ("righttrigger", "ZR"),
            ("leftstick", "LS"),
            ("rightstick", "RS"),
            ("start", "plus"),
            ("back", "minus"),
        ],
    },
    // Key caps: see key_label.
    Style {
        name: "keyboard",
        faces: &[],
        colors: &[],
        labels: &[],
    },
];

pub const ICONS: [&str; 9] = [
    "menu", "view", "plus", "minus", "dpup", "dpdown", "dpleft", "dpright", "arrows",
];
pub const INK: [u8; 3] = [240, 240, 240];
/// Drawn this many times larger, then reduced.
pub const SCALE: usize = 6;
/// The largest texture side `patch` draws (the game's glyph textures are 2048 at most): a DDS
/// header that claims more comes from a corrupt WAD, and is left as it is rather than drawn and
/// allocated at that size.
pub const MAX_SIDE: usize = 4096;

fn lookup<'a, V: Copy>(table: &'a [(&'a str, V)], key: &str) -> Option<V> {
    table.iter().find(|(k, _)| *k == key).map(|(_, v)| *v)
}

fn is_style(style: &str) -> bool {
    STYLES.iter().any(|s| s.name == style)
}

/// What failed while drawing or writing the copies.
#[derive(Debug, thiserror::Error)]
pub enum GlyphError {
    #[error("{0}")]
    Io(#[from] io::Error),
    /// `length` is negative for an entry that ends inside its DDS header, as the Python's
    /// `end - dds - 128` is.
    #[error("{drawn} bytes drawn for a texture of {length}")]
    Length { drawn: usize, length: i64 },
    #[error("a {width}x{height} texture is larger than {max}x{max}", max = MAX_SIDE)]
    TooLarge { width: usize, height: usize },
    #[error("{0}")]
    Picture(String),
}

/// The DXT formats of the glyph textures.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FourCc {
    Dxt1,
    Dxt5,
}

impl FourCc {
    fn block(self) -> usize {
        match self {
            FourCc::Dxt1 => 8,
            FourCc::Dxt5 => 16,
        }
    }

    /// The mode a picture is converted to before it is encoded (RGB for DXT1).
    fn mode(self) -> Mode {
        match self {
            FourCc::Dxt1 => Mode::Rgb,
            FourCc::Dxt5 => Mode::Rgba,
        }
    }

    fn encode(self, picture: &Picture) -> Vec<u8> {
        let rgba = picture.convert(Mode::Rgba);
        let (w, h) = (rgba.width as u32, rgba.height as u32);
        match self {
            FourCc::Dxt1 => dxt::encode_dxt1(&rgba.data, w, h),
            FourCc::Dxt5 => dxt::encode_dxt5(&rgba.data, w, h),
        }
    }
}

// -- Tables -----------------------------------------------------------------------------------------

/// A WAD entry: its header's offset, its data size and its name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub offset: usize,
    pub size: usize,
}

fn u32_at(data: &[u8], at: usize) -> Option<u32> {
    data.get(at..at + 4)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

/// The entries of the WAD in `data`; stops at the first one that does not fit.
pub fn entries(data: &[u8]) -> Vec<Entry> {
    let mut found = Vec::new();
    let mut offset = 0;
    while offset + 0x40 <= data.len() {
        let tag = u16::from_le_bytes([data[offset], data[offset + 1]]);
        let mut size = u32_at(data, offset + 4).unwrap_or(0) as usize;
        let raw = &data[offset + 8..offset + 0x40];
        let end = raw.iter().position(|b| *b == 0).unwrap_or(raw.len());
        let name: String = raw[..end].iter().map(|b| char::from(*b)).collect();
        if tag == 0 && name == "EntityCount" {
            size = 0;
        }
        if offset + 0x40 + size > data.len() {
            break;
        }
        found.push(Entry { name, offset, size });
        offset += 0x40 + ((size + 15) & !15);
    }
    found
}

/// `entry.partition('=')` with both sides stripped.
fn partition(entry: &str) -> (String, String) {
    match entry.split_once('=') {
        Some((game, host)) => (game.trim().to_string(), host.trim().to_string()),
        None => (entry.trim().to_string(), String::new()),
    }
}

/// Game button -> gamepad button, with a `SPARTAN_PAD_MAP` value (`cross=b,circle=a`) applied.
/// L2/R2 stay on the triggers, as in the runtime.
pub fn pad_buttons(pad_map: &str) -> IndexMap<String, String> {
    let mut buttons: IndexMap<String, String> = DEFAULT_MAP
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    for entry in pad_map.split(',') {
        let (game, host) = partition(entry);
        let (game, host) = (game.to_lowercase(), host.to_lowercase());
        if buttons.contains_key(&game) && !host.is_empty() && game != "l2" && game != "r2" {
            buttons.insert(game, host);
        }
    }
    buttons
}

/// Game button -> keyboard key, with a `SPARTAN_KEY_MAP` value (`cross=Q,circle=Left Shift`)
/// applied. Unlike the gamepad, L2/R2 are keys too. `None` (a button left without a key) keeps the
/// default, except L3/R3 with Rage of Sparta on one input: they show the rage key, which presses
/// both.
pub fn key_buttons(key_map: &str) -> IndexMap<String, String> {
    let mut buttons: IndexMap<String, String> = DEFAULT_KEYS
        .iter()
        .chain(STICK_KEYS.iter())
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    let mut unbound = Vec::new();
    let mut rage = String::new();
    for entry in key_map.split(',') {
        let (game, host) = partition(entry);
        let game = game.to_lowercase();
        let none = host.to_lowercase() == "none";
        if game == "rage" {
            rage = if none { String::new() } else { host };
        } else if buttons.contains_key(&game) && none {
            unbound.push(game);
        } else if buttons.contains_key(&game) && !host.is_empty() {
            buttons.insert(game, host);
        }
    }
    if !rage.is_empty() {
        for game in ["l3", "r3"] {
            if unbound.iter().any(|u| u == game) {
                buttons.insert(game.to_string(), rage.clone());
            }
        }
    }
    buttons
}

/// What a key cap shows for an SDL key name: `Left Shift` -> `SHIFT`, `Up` -> the dpup arrow,
/// `Keypad 4` -> `NUM 4`, `e` -> `E`.
pub fn key_label(name: &str) -> String {
    let key = name.trim().to_lowercase();
    if let Some(label) = lookup(&KEY_LABELS, &key) {
        return label.to_string();
    }
    if key.starts_with("keypad ") {
        let rest: String = name.trim().chars().skip(7).collect();
        return format!("NUM {}", rest.to_uppercase());
    }
    name.trim().to_uppercase()
}

/// The letters a controller prints on its face buttons (`Gamepad:` line of the probe); a position
/// the probe did not name is `None`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Faces {
    pub south: Option<String>,
    pub east: Option<String>,
    pub west: Option<String>,
    pub north: Option<String>,
}

impl Faces {
    fn get(&self, position: &str) -> Option<&str> {
        match position {
            "south" => self.south.as_deref(),
            "east" => self.east.as_deref(),
            "west" => self.west.as_deref(),
            "north" => self.north.as_deref(),
            _ => None,
        }
    }

    fn json(&self) -> Value {
        json!({"south": self.south, "east": self.east, "west": self.west, "north": self.north})
    }
}

/// A button's look: a letter or an [`ICONS`] name on a disc, a plate or a key cap.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    Disc,
    Plate,
    Key,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Glyph {
    pub shape: Shape,
    pub label: String,
    pub color: [u8; 3],
}

/// The glyph of a gamepad button: `faces` are the letters this controller prints on
/// south/east/west/north. With the keyboard style `host` is a key name, drawn on a square cap (one
/// or two characters) or a wide one.
pub fn glyph(host: &str, style: &str, faces: Option<&Faces>) -> Glyph {
    if style == "keyboard" {
        let label = key_label(host);
        let shape = if label.chars().count() <= 2 || ICONS.contains(&label.as_str()) {
            Shape::Key
        } else {
            Shape::Plate
        };
        return Glyph {
            shape,
            label,
            color: INK,
        };
    }
    let look = STYLES
        .iter()
        .find(|s| s.name == style)
        .unwrap_or(&STYLES[0]);
    if let Some(position) = lookup(&POSITIONS, host) {
        let letter = faces
            .and_then(|f| f.get(position))
            .filter(|l| ["a", "b", "x", "y"].contains(l))
            .unwrap_or_else(|| lookup(look.faces, position).unwrap_or("a"));
        return Glyph {
            shape: Shape::Disc,
            label: letter.to_uppercase(),
            color: lookup(look.colors, letter).unwrap_or(INK),
        };
    }
    let label = lookup(look.labels, host)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| {
            if ICONS.contains(&host) {
                host.to_string()
            } else {
                host.chars().take(2).collect::<String>().to_uppercase()
            }
        });
    let round = host.ends_with("stick") || label == "plus" || label == "minus";
    Glyph {
        shape: if round { Shape::Disc } else { Shape::Plate },
        label,
        color: INK,
    }
}

/// The connected gamepad, from the probe's `Gamepad:` line: its style, letters and name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Gamepad {
    pub kind: String,
    pub faces: Faces,
    pub name: String,
}

/// The gamepad of the line `spartanport-probe --gamepad` prints, `None` without a gamepad.
pub fn parse_gamepad(output: &str) -> Option<Gamepad> {
    let line = output
        .lines()
        .find(|line| line.starts_with("Gamepad: type="))?;
    let rest = &line["Gamepad: ".len()..];
    let (fields, name) = rest.split_once(" name=").unwrap_or((rest, ""));
    let mut values: IndexMap<&str, &str> = IndexMap::new();
    for field in fields.split_whitespace() {
        if let Some((key, value)) = field.split_once('=') {
            values.insert(key, value);
        }
    }
    let kind = values.get("type").copied().unwrap_or("");
    let style = if kind.starts_with("ps") {
        "playstation"
    } else if ["switch", "joycon", "gamecube"]
        .iter()
        .any(|p| kind.starts_with(p))
    {
        "nintendo"
    } else {
        "xbox"
    };
    let get = |position: &str| values.get(position).map(|v| v.to_string());
    Some(Gamepad {
        kind: style.to_string(),
        faces: Faces {
            south: get("south"),
            east: get("east"),
            west: get("west"),
            north: get("north"),
        },
        name: name.trim().to_string(),
    })
}

/// The style a `SPARTAN_GLYPH_STYLE` value gives, with the controller's letters and name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resolved {
    pub style: String,
    pub faces: Option<Faces>,
    pub name: String,
}

/// `auto` follows the connected gamepad (the keyboard keys without one); a chosen style uses its
/// own letters; anything else is the PlayStation look.
pub fn resolve(choice: &str, probe_output: &str) -> Resolved {
    let plain = |style: &str| Resolved {
        style: style.to_string(),
        faces: None,
        name: String::new(),
    };
    if is_style(choice) {
        return plain(choice);
    }
    if choice != "auto" {
        return plain("playstation");
    }
    match parse_gamepad(probe_output) {
        Some(pad) => Resolved {
            style: pad.kind,
            faces: Some(pad.faces),
            name: pad.name,
        },
        None => plain("keyboard"),
    }
}

// -- Drawing ----------------------------------------------------------------------------------------

/// Where the drawing reads its fonts and controller pictures: the port's `launcher/` and
/// `scripts/controller_diagrams/` (the Python's paths beside its module).
#[derive(Clone, Debug)]
pub struct Assets {
    pub font: PathBuf,
    pub lyre_font: PathBuf,
    pub controller_art: PathBuf,
}

impl Assets {
    pub fn in_port(port: &Path) -> Assets {
        Assets {
            font: port.join("launcher").join("Cinzel-Black.ttf"),
            lyre_font: port.join("launcher").join("BodoniModa.ttf"),
            controller_art: port.join("scripts").join("controller_diagrams"),
        }
    }
}

/// The fonts of one patch, read once when first drawn with.
pub struct Painter<'a> {
    assets: &'a Assets,
    font: OnceCell<Result<Font, String>>,
    lyre_font: OnceCell<Result<Font, String>>,
}

impl<'a> Painter<'a> {
    pub fn new(assets: &'a Assets) -> Painter<'a> {
        Painter {
            assets,
            font: OnceCell::new(),
            lyre_font: OnceCell::new(),
        }
    }

    fn open<'c>(
        cell: &'c OnceCell<Result<Font, String>>,
        path: &Path,
    ) -> Result<&'c Font, GlyphError> {
        cell.get_or_init(|| Font::open(path).map_err(|e| format!("{}: {e}", path.display())))
            .as_ref()
            .map_err(|e| GlyphError::Picture(e.clone()))
    }

    fn font(&self) -> Result<&Font, GlyphError> {
        Self::open(&self.font, &self.assets.font)
    }

    fn lyre_font(&self) -> Result<&Font, GlyphError> {
        Self::open(&self.lyre_font, &self.assets.lyre_font)
    }
}

/// What a build draws: the style, the controller's letters and the two button maps.
#[derive(Clone, Copy, Debug)]
pub struct Choice<'a> {
    pub style: &'a str,
    pub faces: Option<&'a Faces>,
    pub pad_map: &'a str,
    pub key_map: &'a str,
}

/// Draws a letter label or an [`ICONS`] shape centred in `area`.
fn draw_label(
    image: &mut Picture,
    area: [f64; 4],
    label: &str,
    color: [u8; 3],
    painter: &Painter,
) -> Result<(), GlyphError> {
    let [x0, y0, x1, y1] = area;
    let (cx, cy, r) = (
        (x0 + x1) / 2.0,
        (y0 + y1) / 2.0,
        (x1 - x0).min(y1 - y0) / 2.0,
    );
    let t = (r * 0.26).max(1.0);
    let mut draw = Draw::new(image);
    if ["plus", "minus", "menu"].contains(&label) {
        let rows: &[f64] = if label == "menu" {
            &[-0.55, 0.0, 0.55]
        } else {
            &[0.0]
        };
        for row in rows {
            let area = [
                cx - r * 0.8,
                cy + row * r - t / 2.0,
                cx + r * 0.8,
                cy + row * r + t / 2.0,
            ];
            draw.rounded_rectangle(area, t / 3.0, Some(&color), None, 1);
        }
        if label == "plus" {
            let area = [cx - t / 2.0, cy - r * 0.8, cx + t / 2.0, cy + r * 0.8];
            draw.rounded_rectangle(area, t / 3.0, Some(&color), None, 1);
        }
    } else if label == "view" {
        for (dx, dy) in [(-0.25, -0.2), (0.25, 0.2)] {
            let area = [
                cx + (dx - 0.55) * r,
                cy + (dy - 0.45) * r,
                cx + (dx + 0.55) * r,
                cy + (dy + 0.45) * r,
            ];
            draw.rectangle(area, None, Some(&color), (t * 0.7) as i32);
        }
    } else if ICONS.contains(&label) {
        // A d-pad direction: an arrow head; arrows: the four of them, small.
        let heads: Vec<(&str, f64)> = if label == "arrows" {
            ["dpup", "dpdown", "dpleft", "dpright"]
                .iter()
                .map(|a| (*a, 0.5))
                .collect()
        } else {
            vec![(label, 1.0)]
        };
        for (name, k) in heads {
            let (ax, ay) = match name {
                "dpup" => (0.0, -1.0),
                "dpdown" => (0.0, 1.0),
                "dpleft" => (-1.0, 0.0),
                _ => (1.0, 0.0),
            };
            let (ox, oy) = (cx + ax * r * (1.0 - k), cy + ay * r * (1.0 - k));
            draw.polygon(
                &[
                    (ox + ax * r * 0.8 * k, oy + ay * r * 0.8 * k),
                    (
                        ox - ax * r * 0.5 * k - ay * r * 0.7 * k,
                        oy - ay * r * 0.5 * k - ax * r * 0.7 * k,
                    ),
                    (
                        ox - ax * r * 0.5 * k + ay * r * 0.7 * k,
                        oy - ay * r * 0.5 * k + ax * r * 0.7 * k,
                    ),
                ],
                &color,
            );
        }
    } else {
        let font = painter.font()?;
        let mut size = (y1 - y0) as i64;
        let (sized, (left, top, right, bottom)) = loop {
            let sized = font.at(size);
            let bbox = sized.bbox(label, 0);
            let (left, top, right, bottom) = bbox;
            if ((right - left) as f64 <= x1 - x0 && (bottom - top) as f64 <= y1 - y0) || size <= 6 {
                break (sized, bbox);
            }
            size = (size as f64 * 0.92) as i64;
        };
        let xy = (
            cx - (left + right) as f64 / 2.0,
            cy - (top + bottom) as f64 / 2.0,
        );
        draw.text(xy, label, &sized, &color, 0);
    }
    Ok(())
}

/// An icon as the game draws them inside text: a dark button with the label on it.
fn draw_inline(
    glyph: &Glyph,
    width: usize,
    height: usize,
    painter: &Painter,
) -> Result<Picture, GlyphError> {
    let (w, h, k) = (width * SCALE, height * SCALE, SCALE);
    let mut image = Picture::new(Mode::Rgba, w, h, &[0, 0, 0, 0]);
    let round = matches!(glyph.shape, Shape::Disc | Shape::Key);
    let area = if round {
        let d = w.min(h) as i64 - 2 * k as i64;
        let (w, h) = (w as i64, h as i64);
        [
            (w - d).div_euclid(2),
            (h - d).div_euclid(2),
            (w + d).div_euclid(2),
            (h + d).div_euclid(2),
        ]
    } else {
        let bh = (h as i64 - 2 * k as i64).min(36 * k as i64);
        let h = h as i64;
        [
            2 * k as i64,
            (h - bh).div_euclid(2),
            w as i64 - 2 * k as i64,
            (h + bh).div_euclid(2),
        ]
    };
    let size = (
        (area[2] - area[0]).max(0) as usize,
        (area[3] - area[1]).max(0) as usize,
    );
    let shade = linear_gradient().resize(size.0, size.1, Filter::Bicubic); // lit from above
    let band =
        |a: i64, b: i64| shade.point(|v| (a + ((b - a) * i64::from(v)).div_euclid(255)) as u8);
    let body = Picture::merge(Mode::Rgb, &[&band(84, 26), &band(84, 26), &band(86, 28)]);
    let mut mask = Picture::new(Mode::L, size.0, size.1, &[0]);
    let outline = [0.0, 0.0, size.0 as f64 - 1.0, size.1 as f64 - 1.0];
    if glyph.shape == Shape::Disc {
        Draw::new(&mut mask).ellipse(outline, Some(&[255]), None, 1);
    } else {
        Draw::new(&mut mask).rounded_rectangle(outline, (5 * k) as f64, Some(&[255]), None, 1);
    }
    image.paste(&body, area[0], area[1], Some(&mask));
    let edge = area.map(|v| v as f64);
    let rim: [u8; 4] = [14, 14, 14, 255];
    if glyph.shape == Shape::Disc {
        Draw::new(&mut image).ellipse(edge, None, Some(&rim), (k * 3 / 2) as i32);
    } else {
        Draw::new(&mut image).rounded_rectangle(
            edge,
            (5 * k) as f64,
            None,
            Some(&rim),
            (k * 3 / 2) as i32,
        );
    }
    // A key name (SPACE, SHIFT, M4) is wider than a face letter: on a 64 px icon it needs most of the cap.
    let (fx, fy) = match glyph.shape {
        Shape::Disc => (0.2, 0.2),
        Shape::Key => (0.12, 0.14),
        Shape::Plate => (0.1, 0.16),
    };
    let (inset_x, inset_y) = (size.0 as f64 * fx, size.1 as f64 * fy);
    draw_label(
        &mut image,
        [
            edge[0] + inset_x,
            edge[1] + inset_y,
            edge[2] - inset_x,
            edge[3] - inset_y,
        ],
        &glyph.label,
        glyph.color,
        painter,
    )?;
    Ok(image.resize(width, height, Filter::Lanczos))
}

/// A QTE label: bright on black (the game adds these textures to the picture). `cap` frames it as a
/// keyboard key.
fn draw_prompt(
    label: &str,
    color: [u8; 3],
    width: usize,
    height: usize,
    cap: bool,
    painter: &Painter,
) -> Result<Picture, GlyphError> {
    let (w, h) = ((width * SCALE) as f64, (height * SCALE) as f64);
    let mut layer = Picture::new(Mode::Rgba, width * SCALE, height * SCALE, &[0, 0, 0, 0]);
    if cap {
        let side = w.min(h);
        // The cap fills the prompt, leaving room for the glow, so the key reads at a glance mid-fight.
        let width = ((side * 0.035) as i32).max(1);
        Draw::new(&mut layer).rounded_rectangle(
            [w * 0.06, h * 0.06, w * 0.94, h * 0.94],
            side * 0.1,
            None,
            Some(&color),
            width,
        );
        draw_label(
            &mut layer,
            [w * 0.15, h * 0.22, w * 0.85, h * 0.78],
            label,
            color,
            painter,
        )?;
    } else {
        draw_label(
            &mut layer,
            [w * 0.24, h * 0.2, w * 0.76, h * 0.8],
            label,
            color,
            painter,
        )?;
    }
    let mut image = Picture::new(Mode::Rgba, width * SCALE, height * SCALE, &[0, 0, 0, 255]);
    image.alpha_composite(&layer.gaussian_blur(SCALE as f32 * 2.5));
    image.alpha_composite(&layer);
    Ok(image.resize(width, height, Filter::Lanczos))
}

/// The picture that replaces a texture of [`TEXTURES`] (not the lyre's), at its size.
pub fn render(
    form: Form,
    buttons: &[&str],
    choice: &Choice,
    width: usize,
    height: usize,
    painter: &Painter,
) -> Result<Picture, GlyphError> {
    if form == Form::Controller {
        let path = painter
            .assets
            .controller_art
            .join(format!("{}.png", choice.style));
        let art = image::open(&path)
            .map_err(|e| GlyphError::Picture(format!("{}: {e}", path.display())))?
            .to_rgba8();
        return Ok(Picture::from_rgba(&art).resize(width, height, Filter::Lanczos));
    }
    let keyboard = choice.style == "keyboard";
    let hosts = if keyboard {
        key_buttons(choice.key_map)
    } else {
        pad_buttons(choice.pad_map)
    };
    let host = |button: &str| hosts.get(button).cloned().unwrap_or_default();
    if form == Form::Atlas {
        let mut image = Picture::new(Mode::Rgba, width, height, &[0, 0, 0, 255]);
        // In the shoulder-button atlas, prompts are centered ~52 px from the midline (y=128), not
        // in the middle of each 128 px half (y=64), matching the game's HUD quad alignment.
        let offset = (height as f64 * 12.0 / 256.0) as i64;
        for (row, button) in buttons.iter().enumerate() {
            let mut half = Picture::new(Mode::Rgba, width, height / 2, &[0, 0, 0, 255]);
            let label = glyph(&host(button), choice.style, choice.faces).label;
            half.paste(
                &draw_prompt(&label, INK, width, height / 2, keyboard, painter)?,
                0,
                -offset,
                None,
            );
            let half = if row == 0 {
                half.transpose(Transpose::Rotate180)
            } else {
                half
            };
            image.paste(&half, 0, (row * (height / 2)) as i64, None);
        }
        return Ok(image);
    }
    let glyph = glyph(
        &host(buttons.first().copied().unwrap_or("")),
        choice.style,
        choice.faces,
    );
    if form == Form::Inline {
        draw_inline(&glyph, width, height, painter)
    } else {
        draw_prompt(&glyph.label, glyph.color, width, height, keyboard, painter)
    }
}

// -- The lyre -----------------------------------------------------------------------------------------

/// An RGBA layer of `size` with `label` in the lyre's face, `cap` pixels of cap height (a longer
/// label shrinks to the disc), centred on `centre`, `margin` pixels fatter.
fn lyre_letter(
    size: usize,
    label: &str,
    centre: (f64, f64),
    cap: i64,
    color: [u8; 3],
    margin: i64,
    painter: &Painter,
) -> Result<Picture, GlyphError> {
    let scale: i64 = 4;
    let font = painter.lyre_font()?;
    let mut px = cap * scale;
    let (_, top, _, bottom) = font.at(px).bbox("H", 0);
    px = (px as f64 * px as f64 / (bottom - top) as f64) as i64; // the cap height of H is `cap`
    let width_limit = LYRE_RADIUS as f64 * 1.5 * scale as f64;
    let stroke = (LYRE_STROKE + margin) * scale;
    let (sized, (left, top, right, bottom)) = loop {
        let sized = font.at(px);
        let bbox = sized.bbox(label, stroke);
        if (bbox.2 - bbox.0) as f64 <= width_limit || px <= 8 * scale {
            break (sized, bbox);
        }
        px = (px as f64 * 0.9) as i64;
    };
    let xy = (
        centre.0 * scale as f64 - (left + right) as f64 / 2.0,
        centre.1 * scale as f64 - (top + bottom) as f64 / 2.0,
    );
    // The text's box (left and top are at most the position: the bbox starts at the pen).
    let (l, t, r, b) = (left as f64, top as f64, right as f64, bottom as f64);
    let mut big = Layer::around(
        size * scale as usize,
        xy.0 + l,
        (xy.1 + t).min(xy.1),
        xy.0 + r,
        xy.1 + b,
    );
    let at = big.at(xy);
    Draw::new(&mut big.picture).text(
        at,
        label,
        &sized,
        &[color[0], color[1], color[2], 255],
        stroke,
    );
    Ok(big.reduce(size))
}

/// A part of a `full` x `full` transparent layer (the Python's `big`), enough to hold what is
/// drawn in the box it is made around: drawing there at [`Layer::at`] gives the pixels the whole
/// layer gets (every coordinate stays non-negative, so the C's truncations agree), and
/// [`Layer::reduce`] the whole layer's reduction.
struct Layer {
    picture: Picture,
    left: usize,
    top: usize,
    full: usize,
}

impl Layer {
    fn around(full: usize, x0: f64, y0: f64, x1: f64, y1: f64) -> Layer {
        let clip = |v: f64| (v.floor().max(0.0) as usize).min(full);
        let (left, top) = (clip(x0 - 4.0), clip(y0 - 4.0));
        let (right, bottom) = (clip(x1 + 5.0).max(left), clip(y1 + 5.0).max(top));
        Layer {
            picture: Picture::new(Mode::Rgba, right - left, bottom - top, &[0, 0, 0, 0]),
            left,
            top,
            full,
        }
    }

    fn at(&self, (x, y): (f64, f64)) -> (f64, f64) {
        (x - self.left as f64, y - self.top as f64)
    }

    /// `big.resize((size, size), Image.LANCZOS)`.
    fn reduce(&self, size: usize) -> Picture {
        self.picture.resize_placed(
            self.left,
            self.top,
            self.full,
            self.full,
            size,
            size,
            Filter::Lanczos,
        )
    }
}

/// An RGBA layer of `size` with `label` as a line of `width` pixels with round ends, like the
/// shapes engraved into the gold discs: A, B, X and Y are drawn; another label is set in the
/// lyre's face.
#[allow(clippy::too_many_arguments)]
fn gold_letter(
    size: usize,
    label: &str,
    centre: (f64, f64),
    cap: i64,
    width: i64,
    color: [u8; 3],
    painter: &Painter,
) -> Result<Picture, GlyphError> {
    let scale = 4.0;
    let height = cap as f64;
    let mut wide = 0.8 * height;
    let strokes: Vec<Vec<(f64, f64)>> = match label {
        "X" => vec![
            vec![(0.0, 0.0), (wide, height)],
            vec![(wide, 0.0), (0.0, height)],
        ],
        "A" => vec![
            vec![(0.0, height), (wide / 2.0, 0.0), (wide, height)],
            vec![(wide * 0.2, height * 0.64), (wide * 0.8, height * 0.64)],
        ],
        "Y" => vec![
            vec![(0.0, 0.0), (wide / 2.0, height / 2.0), (wide, 0.0)],
            vec![(wide / 2.0, height / 2.0), (wide / 2.0, height)],
        ],
        "B" => vec![vec![(0.0, 0.0), (0.0, height)]],
        _ => return lyre_letter(size, label, centre, cap, color, -LYRE_STROKE, painter),
    };
    if label == "B" {
        wide = height * 0.62;
    }
    let fill = [color[0], color[1], color[2], 255];
    let thick = (width * 4) as f64;
    let at = |p: (f64, f64)| {
        (
            (centre.0 - wide / 2.0 + p.0) * scale,
            (centre.1 - height / 2.0 + p.1) * scale,
        )
    };
    // Everything is drawn within `thick` of the letter's box (the B's bowls reach `height / 4` past
    // their points).
    let reach = thick + height / 4.0 * scale;
    let (x0, y0) = at((0.0, 0.0));
    let (x1, y1) = at((wide.max(height * 0.32 + height / 4.0), height));
    let mut big = Layer::around(size * 4, x0 - reach, y0 - reach, x1 + reach, y1 + reach);
    let (left, top) = (big.left as f64, big.top as f64);
    let at = |p: (f64, f64)| {
        let (x, y) = at(p);
        (x - left, y - top)
    };
    let mut draw = Draw::new(&mut big.picture);
    let dot = |draw: &mut Draw, p: (f64, f64)| {
        let (x, y) = at(p);
        draw.ellipse(
            [
                x - thick / 2.0,
                y - thick / 2.0,
                x + thick / 2.0,
                y + thick / 2.0,
            ],
            Some(&fill),
            None,
            1,
        );
    };
    for stroke in &strokes {
        let points: Vec<(f64, f64)> = stroke.iter().map(|p| at(*p)).collect();
        draw.line(&points, &fill, thick as i32, true);
        if let (Some(first), Some(last)) = (stroke.first(), stroke.last()) {
            dot(&mut draw, *first);
            dot(&mut draw, *last);
        }
    }
    if label == "B" {
        // Two bowls on the stem: a straight top, a half circle, a straight return.
        for (top, reach) in [(0.0, height * 0.28), (height / 2.0, height * 0.32)] {
            let radius = height / 4.0;
            draw.line(
                &[at((0.0, top)), at((reach, top))],
                &fill,
                thick as i32,
                false,
            );
            draw.line(
                &[
                    at((0.0, top + 2.0 * radius)),
                    at((reach, top + 2.0 * radius)),
                ],
                &fill,
                thick as i32,
                false,
            );
            let (x, y) = at((reach, top + radius));
            let r = radius * scale;
            draw.arc(
                [x - r, y - r, x + r, y + r],
                -90.0,
                90.0,
                &fill,
                thick as i32,
            );
        }
    }
    Ok(big.reduce(size))
}

/// The value below which `fraction` of the pixels of `band` (L) under `mask` fall.
fn lyre_level(band: &Picture, mask: &Picture, fraction: f64) -> u8 {
    let histogram = band.histogram(Some(mask));
    let wanted = histogram.iter().sum::<u64>() as f64 * fraction;
    let mut seen = 0u64;
    for (value, count) in histogram.iter().enumerate() {
        seen += count;
        if seen as f64 >= wanted {
            return value as u8;
        }
    }
    255
}

/// An L mask of the letter on `disc` (a square around one disc) and its carved edge, a 5 px
/// margin: the pixels inside `inner` brighter (`dark`: darker) than the disc's own interior, which
/// is the larger part of it, in the brightest channel. An engraved shape (`dark`) takes what it
/// encloses with it, unless `enclosed` is off.
fn lyre_letter_mask(disc: &Picture, inner: &Picture, dark: bool, enclosed: bool) -> Picture {
    let bright = lighter(&disc.band(0), &lighter(&disc.band(1), &disc.band(2)));
    let interior = f64::from(lyre_level(&bright, inner, 0.5));
    let mask = if dark {
        let threshold = interior - (interior - f64::from(lyre_level(&bright, inner, 0.001))) * 0.35;
        bright.point(|v| if f64::from(v) < threshold { 255 } else { 0 })
    } else {
        let threshold = interior + (f64::from(lyre_level(&bright, inner, 0.999)) - interior) * 0.35;
        bright.point(|v| if f64::from(v) > threshold { 255 } else { 0 })
    };
    let mut mask = multiply(&multiply(&mask, inner).max_filter(11), inner);
    if dark && enclosed {
        let mut outside = lighter(&mask, &invert(inner));
        if let Some((left, _top, right, _bottom)) = inner.bbox() {
            // Flood from clear points towards the edge, off the diagonals, in every part of the
            // disc the shape's lines divide; what stays unreached is enclosed.
            for f in [0.9, 0.85, 0.8] {
                for angle in [15.0f64, 75.0, 105.0, 165.0, 195.0, 255.0, 285.0, 345.0] {
                    let reach = f * (right - left) as f64 / 2.0;
                    let seed = (
                        (disc.width as f64 / 2.0 + reach * angle.to_radians().cos()) as usize,
                        (disc.height as f64 / 2.0 - reach * angle.to_radians().sin()) as usize,
                    );
                    if seed.0 < outside.width
                        && seed.1 < outside.height
                        && outside.pixel(seed.0, seed.1)[0] == 0
                    {
                        floodfill(&mut outside, seed, 128);
                    }
                }
            }
        }
        mask = lighter(&mask, &outside.point(|v| if v == 0 { 255 } else { 0 }));
    }
    mask
}

/// `disc` with the pixels under `mask` replaced by the disc's own interior texture: taken from
/// `others`, the other discs (each with its mask), matched to this disc's shading, where they show
/// no letter, then from the disc turned about its centre, then from the median colour. `slide`,
/// the mask of the letter alone, puts the disc's own texture slid sideways first (the engraved gold
/// discs). Returns the disc and its median colour.
fn lyre_fill(
    disc: &Picture,
    mask: &Picture,
    inner: &Picture,
    others: &[(&Picture, &Picture)],
    slide: Option<&Picture>,
) -> (Picture, [u8; 3]) {
    let clean = subtract(inner, mask, 1.0, 0);
    let medians = [0, 1, 2].map(|band| lyre_level(&disc.band(band), &clean, 0.5));
    if mask.bbox().is_none() {
        // Nothing to replace: every paste below goes through an empty mask.
        return (disc.clone(), medians);
    }
    // The slow changes of tone across `image`, with `hole` filled by the tone around it.
    let shading = |image: &Picture, hole: &Picture| {
        let around = subtract(&multiply(&hole.max_filter(25), inner), hole, 1.0, 0);
        let mut flat = image.clone();
        let levels: Vec<u8> = (0..image.bands())
            .map(|b| lyre_level(&image.band(b), &around, 0.5))
            .collect();
        flat.fill(
            &levels,
            0,
            0,
            hole.width as i64,
            hole.height as i64,
            Some(hole),
        );
        flat.gaussian_blur(10.0)
    };
    // Another disc's pixels are brought to this disc's shading: lighter centre, darker edge.
    let tone = shading(disc, mask);
    let mut sources: Vec<(Picture, Picture)> = others
        .iter()
        .map(|(other, other_mask)| {
            let matched = add(
                other,
                &subtract(&tone, &shading(other, other_mask), 1.0, 128),
                1.0,
                -128,
            );
            (matched, (*other_mask).clone())
        })
        .collect();
    let firsts = sources.clone();
    for (source, source_mask) in &firsts {
        for turn in [
            Transpose::Rotate90,
            Transpose::Rotate180,
            Transpose::Rotate270,
        ] {
            sources.push((source.transpose(turn), source_mask.transpose(turn)));
        }
    }
    for angle in [
        90.0, 270.0, 180.0, 45.0, 135.0, 225.0, 315.0, 30.0, 60.0, 120.0, 150.0, 210.0, 240.0,
        300.0, 330.0,
    ] {
        let turned_mask = mask.rotate(angle).point(|v| if v != 0 { 255 } else { 0 });
        sources.push((disc.rotate(angle), turned_mask));
    }
    sources.reverse();
    if let Some(slide) = slide {
        let matched = add(
            disc,
            &subtract(&tone, &shading(disc, slide), 1.0, 128),
            1.0,
            -128,
        );
        for (dx, dy) in [
            (40, 0),
            (-40, 0),
            (0, 40),
            (0, -40),
            (28, 28),
            (-28, 28),
            (28, -28),
            (-28, -28),
        ] {
            let source_mask = lighter(&mask.offset(dx, dy), &invert(&inner.offset(dx, dy)));
            sources.push((matched.offset(dx, dy), source_mask));
        }
    }
    let mut filled = disc.clone();
    filled.fill(
        &medians,
        0,
        0,
        mask.width as i64,
        mask.height as i64,
        Some(mask),
    );
    // The best source last, each blended in over the earlier ones with a soft edge.
    for (source, source_mask) in &sources {
        let usable = subtract(mask, source_mask, 1.0, 0).min_filter(5);
        if usable.bbox().is_some() {
            filled = composite(source, &filled, &multiply(&usable.gaussian_blur(2.0), mask));
        }
    }
    let mut result = disc.clone();
    result.paste(&filled, 0, 0, Some(&mask.gaussian_blur(1.0)));
    (result, medians)
}

/// The normal map `disc` (RGBA: X in alpha, Y in green) with the raised letter smoothed away: the
/// disc is a dome, so its normals point outward by an amount that depends on the distance from the
/// centre; that profile, the median at each distance, replaces the pixels that stray from it.
fn lyre_dome(disc: &Picture, inner: &Picture) -> Picture {
    let (green, alpha) = (disc.band(1), disc.band(3));
    let size = disc.width;
    let centre = size as f64 / 2.0 - 0.5;
    let at = |x: usize, y: usize| y * size + x;
    let mut slant: Vec<Vec<f64>> = vec![Vec::new(); size]; // outward tilt by distance, in pixels
    for y in 0..size {
        for x in 0..size {
            if inner.data[at(x, y)] != 0 {
                let (dx, dy) = (x as f64 - centre, y as f64 - centre);
                let distance = (dx * dx + dy * dy).sqrt();
                if distance >= 1.0 {
                    let tilt = ((128.0 - f64::from(alpha.data[at(x, y)])) * dx
                        + (f64::from(green.data[at(x, y)]) - 128.0) * dy)
                        / distance;
                    if let Some(values) = slant.get_mut(distance as usize) {
                        values.push(tilt);
                    }
                }
            }
        }
    }
    let profile: Vec<f64> = slant
        .iter_mut()
        .map(|values| {
            values.sort_by(f64::total_cmp);
            values.get(values.len() / 2).copied().unwrap_or(0.0)
        })
        .collect();
    let mut dome_x = Picture::new(Mode::L, size, size, &[128]);
    let mut dome_y = Picture::new(Mode::L, size, size, &[128]);
    for y in 0..size {
        for x in 0..size {
            if inner.data[at(x, y)] != 0 {
                let (dx, dy) = (x as f64 - centre, y as f64 - centre);
                let distance = (dx * dx + dy * dy).sqrt();
                if distance >= 1.0 {
                    let tilt = profile.get(distance as usize).copied().unwrap_or(0.0) / distance;
                    dome_x.data[at(x, y)] =
                        (128.0 - tilt * dx).round_ties_even().clamp(0.0, 255.0) as u8;
                    dome_y.data[at(x, y)] =
                        (128.0 + tilt * dy).round_ties_even().clamp(0.0, 255.0) as u8;
                }
            }
        }
    }
    let strays = lighter(&difference(&alpha, &dome_x), &difference(&green, &dome_y));
    let mask = multiply(&strays.point(|v| if v > 24 { 255 } else { 0 }), inner);
    let mask = multiply(&mask.max_filter(11), inner).gaussian_blur(1.0);
    let (mut alpha, mut green) = (alpha, green);
    alpha.paste(&dome_x, 0, 0, Some(&mask));
    green.paste(&dome_y, 0, 0, Some(&mask));
    Picture::merge(Mode::Rgba, &[&disc.band(0), &green, &disc.band(2), &alpha])
}

/// The normal map `image` (RGBA) with `height` (an L picture: the letters) raised on it: the
/// slopes of its edges tilt the normals, as the game's own letters do.
fn lyre_relief(image: &Picture, height: &Picture) -> Picture {
    let smooth = height.gaussian_blur(1.5);
    let tilt_x = subtract(
        &smooth.offset(-1, 0),
        &smooth.offset(1, 0),
        LYRE_RELIEF,
        128,
    );
    let tilt_y = subtract(
        &smooth.offset(0, -1),
        &smooth.offset(0, 1),
        LYRE_RELIEF,
        128,
    );
    let alpha = add(&image.band(3), &tilt_x, 1.0, -128);
    let green = subtract(&image.band(1), &tilt_y, 1.0, 128);
    Picture::merge(
        Mode::Rgba,
        &[&image.band(0), &green, &image.band(2), &alpha],
    )
}

/// The level texture `image` (the lyre's or the gold mechanism's, of `kind`) with its four discs
/// showing the gamepad buttons: the letter is replaced by the disc's own interior (black in the
/// glow mask) and the host button's letter drawn in its place.
pub fn lyre_paint(
    image: &Picture,
    form: Form,
    kind: &str,
    choice: &Choice,
    painter: &Painter,
) -> Result<Picture, GlyphError> {
    let hosts = pad_buttons(choice.pad_map);
    let look = LYRE_FORMS
        .iter()
        .find(|(f, _)| *f == form)
        .map_or(LYRE_FORMS[0].1, |(_, look)| *look);
    let shrink = 2048usize
        .checked_div(image.width)
        .filter(|s| *s > 0)
        .ok_or_else(|| GlyphError::Picture("integer division or modulo by zero".to_string()))?;
    let [x0, y0, x1, y1] = LYRE_CORNER.map(|v| (v / shrink) as i64);
    let normal = kind == "normal";
    let mut corner =
        image
            .crop(x0, y0, x1, y1)
            .convert(if normal { Mode::Rgba } else { Mode::Rgb });
    let radius = LYRE_RADIUS / shrink as i64;
    let r = radius as f64;
    let mut inner = Picture::new(Mode::L, 2 * radius as usize, 2 * radius as usize, &[0]);
    let ring = [
        r * (1.0 - look.inner),
        r * (1.0 - look.inner),
        r * (1.0 + look.inner),
        r * (1.0 + look.inner),
    ];
    Draw::new(&mut inner).ellipse(ring, Some(&[255]), None, 1);
    let shrink = shrink as i64;
    let boxes: Vec<(&str, [i64; 4])> = LYRE_DISCS
        .iter()
        .map(|(button, (cx, cy))| {
            (
                *button,
                [
                    cx / shrink - radius,
                    cy / shrink - radius,
                    cx / shrink + radius,
                    cy / shrink + radius,
                ],
            )
        })
        .collect();
    let mut medians: IndexMap<&str, [u8; 3]> = IndexMap::new();
    if kind == "glow" {
        for (_, area) in &boxes {
            corner.fill(&[0, 0, 0], area[0], area[1], area[2], area[3], Some(&inner));
        }
    } else if normal {
        for (_, area) in &boxes {
            let disc = corner.crop(area[0], area[1], area[2], area[3]);
            corner.paste(&lyre_dome(&disc, &inner), area[0], area[1], None);
        }
    } else {
        let discs: Vec<Picture> = boxes
            .iter()
            .map(|(_, a)| corner.crop(a[0], a[1], a[2], a[3]))
            .collect();
        let masks: Vec<Picture> = discs
            .iter()
            .map(|d| lyre_letter_mask(d, &inner, look.dark, true))
            .collect();
        // As sources the discs only lose their letters: what a shape encloses is good texture.
        let lettered: Vec<Picture> = if look.dark {
            discs
                .iter()
                .map(|d| lyre_letter_mask(d, &inner, true, false))
                .collect()
        } else {
            masks.clone()
        };
        for (i, (button, area)) in boxes.iter().enumerate() {
            let others: Vec<(&Picture, &Picture)> = (0..discs.len())
                .filter(|j| *j != i)
                .map(|j| (&discs[j], &lettered[j]))
                .collect();
            let slide = look.dark.then_some(&lettered[i]);
            let (filled, median) = lyre_fill(&discs[i], &masks[i], &inner, &others, slide);
            medians.insert(button, median);
            corner.paste(&filled, area[0], area[1], None);
        }
    }
    let size = LYRE_CORNER[2] - LYRE_CORNER[0];
    let mut letters = Picture::new(Mode::Rgba, size, size, &[0, 0, 0, 0]);
    for (button, (cx, cy)) in LYRE_DISCS {
        let centre = (cx as f64, cy as f64);
        let label = glyph(
            hosts.get(button).map(String::as_str).unwrap_or(""),
            choice.style,
            choice.faces,
        );
        if form == Form::Gold {
            // A line engraved into the gold: a light bevel around a dark groove.
            let median = medians.get(button).copied().unwrap_or([156, 116, 60]);
            let groove = if kind == "diffuse" {
                // The far wall of the groove catches the light.
                let light = median.map(|c| (f64::from(c) * 1.2).min(255.0) as u8);
                let lit = (centre.0 + 2.0, centre.1 + 2.0);
                let bevel = gold_letter(
                    size,
                    &label.label,
                    lit,
                    look.cap,
                    GOLD_LINE + 5,
                    light,
                    painter,
                )?;
                letters.alpha_composite(&bevel.gaussian_blur(1.5));
                median.map(|c| (f64::from(c) * 0.5) as u8)
            } else {
                median.map(|c| (f64::from(c) * 0.9) as u8)
            };
            let line = gold_letter(
                size,
                &label.label,
                centre,
                look.cap,
                GOLD_LINE,
                groove,
                painter,
            )?;
            letters.alpha_composite(&line.gaussian_blur(0.6));
            continue;
        }
        let color = match kind {
            "diffuse" => label.color.map(|c| (f64::from(c) * LYRE_DIM) as u8),
            "decal" => label.color.map(|c| (f64::from(c) * LYRE_DECAL) as u8),
            "specular" => {
                // Light grey with the dark rim of a carved edge.
                let rim = lyre_letter(
                    size,
                    &label.label,
                    centre,
                    look.cap,
                    [LYRE_RIM; 3],
                    2,
                    painter,
                )?;
                letters.alpha_composite(&rim);
                [LYRE_SHINE; 3]
            }
            _ => [255, 255, 255],
        };
        letters.alpha_composite(&lyre_letter(
            size,
            &label.label,
            centre,
            look.cap,
            color,
            0,
            painter,
        )?);
    }
    if shrink > 1 {
        letters = letters.resize(corner.width, corner.height, Filter::Lanczos);
    }
    let mut out = image.convert(if normal { Mode::Rgba } else { Mode::Rgb });
    if normal {
        out.paste(&lyre_relief(&corner, &letters.band(3)), x0, y0, None);
    } else {
        let mut base = corner.convert(Mode::Rgba);
        base.alpha_composite(&letters);
        out.paste(&base.convert(Mode::Rgb), x0, y0, None);
    }
    Ok(out)
}

// -- Encoding ---------------------------------------------------------------------------------------

/// Whether the 4 x 4 block at `x`, `y` of `a` and `b` differs in luma (`ImageChops.difference(a,
/// b).convert('L')` holds a non-zero pixel there).
fn block_changed(a: &Picture, b: &Picture, x: usize, y: usize) -> bool {
    (y..(y + 4).min(a.height)).any(|row| {
        (x..(x + 4).min(a.width)).any(|column| {
            let (p, q) = (a.pixel(column, row), b.pixel(column, row));
            let d = [
                p[0].abs_diff(q[0]),
                p[1].abs_diff(q[1]),
                p[2].abs_diff(q[2]),
            ];
            (u32::from(d[0]) * 19595 + u32::from(d[1]) * 38470 + u32::from(d[2]) * 7471 + 0x8000)
                >> 16
                != 0
        })
    })
}

/// Re-encodes the blocks of `region` (x0, y0, x1, y1 in the top level) of the texture at `dds` in
/// `data`, in every mip level, from `image` (the whole top level); the other blocks are kept, and so
/// are the region's blocks whose pixels `original` (the texture as it was) still has. Levels too
/// small to hold the region's blocks are re-encoded whole.
pub fn encode_region(
    data: &mut [u8],
    dds: usize,
    end: usize,
    image: &Picture,
    fourcc: FourCc,
    region: [usize; 4],
    original: Option<&Picture>,
) -> Result<(), GlyphError> {
    let short = || GlyphError::Picture("texture header is truncated".to_string());
    let height = u32_at(data, dds + 12).ok_or_else(short)?;
    let width = u32_at(data, dds + 16).ok_or_else(short)?;
    let block = fourcc.block();
    let image = image.convert(fourcc.mode());
    let original = original.map(|o| o.convert(fourcc.mode()));
    let mut offset = dds + 128;
    let mut level = 0u32;
    while offset < end {
        let w = width.checked_shr(level).unwrap_or(0).max(1) as usize;
        let h = height.checked_shr(level).unwrap_or(0).max(1) as usize;
        let (bw, bh) = (w.div_ceil(4), h.div_ceil(4));
        let size = bw * bh * block;
        if offset + size > end || end > data.len() {
            break;
        }
        let shift = |v: usize| v.checked_shr(level).unwrap_or(0);
        let (mut x0, mut y0) = (shift(region[0]) / 4 * 4, shift(region[1]) / 4 * 4);
        let (mut x1, mut y1) = (
            w.min(shift(region[2]).div_ceil(4) * 4),
            h.min(shift(region[3]).div_ceil(4) * 4),
        );
        if w <= 8 || h <= 8 || x1 < x0 + 4 || y1 < y0 + 4 {
            (x0, y0, x1, y1) = (0, 0, w, h);
        }
        let window = Window { x0, y0, x1, y1 };
        // The smaller levels from the top one; painted and original share most rows.
        let (small, before): (Cow<Picture>, Option<Cow<Picture>>) = match (&original, level) {
            (Some(original), 0) => (Cow::Borrowed(&image), Some(Cow::Borrowed(original))),
            (Some(original), _) => {
                let (small, before) =
                    image.resize_window_pair(original, w, h, Filter::Lanczos, window);
                (Cow::Owned(small), Some(Cow::Owned(before)))
            }
            (None, 0) => (Cow::Borrowed(&image), None),
            (None, _) => (
                Cow::Owned(image.resize_window(w, h, Filter::Lanczos, window)),
                None,
            ),
        };
        for y in (y0..y1).step_by(4) {
            let rows = (y + 4).min(h);
            let row = offset + (y / 4) * bw * block;
            match &before {
                // Only the changed blocks: a block's bytes depend on its own pixels alone.
                Some(before) => {
                    for x in (x0..x1).step_by(4) {
                        if !block_changed(&small, before, x, y) {
                            continue;
                        }
                        let pixels = fourcc.encode(&small.crop(
                            x as i64,
                            y as i64,
                            (x + 4).min(x1) as i64,
                            rows as i64,
                        ));
                        let at = row + (x / 4) * block;
                        data[at..at + block].copy_from_slice(&pixels[..block]);
                    }
                }
                None => {
                    let pixels =
                        fourcc.encode(&small.crop(x0 as i64, y as i64, x1 as i64, rows as i64));
                    for x in (x0..x1).step_by(4) {
                        let i = (x - x0) / 4 * block;
                        let at = row + (x / 4) * block;
                        data[at..at + block].copy_from_slice(&pixels[i..i + block]);
                    }
                }
            }
        }
        offset += size;
        level += 1;
        if w == 1 && h == 1 {
            break;
        }
    }
    Ok(())
}

/// DXT pixel data of `image` and its smaller levels (each half the previous, bicubic), exactly
/// `length` bytes as in the original texture.
pub fn encode(image: &Picture, fourcc: FourCc, length: usize) -> Result<Vec<u8>, GlyphError> {
    let mut image = image.convert(fourcc.mode());
    let mut pixels = Vec::new();
    while pixels.len() < length {
        // Independent block rows, as the Python encodes them.
        for y in (0..image.height).step_by(4) {
            let rows = (y + 4).min(image.height);
            pixels.extend(fourcc.encode(&image.crop(0, y as i64, image.width as i64, rows as i64)));
        }
        if (image.width, image.height) == (1, 1) {
            break;
        }
        image = image.resize(
            (image.width / 2).max(1),
            (image.height / 2).max(1),
            Filter::Bicubic,
        );
    }
    if pixels.len() != length {
        return Err(GlyphError::Length {
            drawn: pixels.len(),
            length: i64::try_from(length).unwrap_or(i64::MAX),
        });
    }
    Ok(pixels)
}

/// The top level of the DDS file `dds` (header and pixels) as Pillow opens it, RGB for DXT1.
pub fn decode_texture(dds: &[u8], fourcc: FourCc) -> Result<Picture, GlyphError> {
    let truncated = || GlyphError::Picture("image file is truncated".to_string());
    let height = u32_at(dds, 12).ok_or_else(truncated)?;
    let width = u32_at(dds, 16).ok_or_else(truncated)?;
    let pixels = dds.get(128..).ok_or_else(truncated)?;
    let rgba = match fourcc {
        FourCc::Dxt1 => dxt::decode_dxt1(pixels, width, height),
        FourCc::Dxt5 => dxt::decode_dxt5(pixels, width, height),
    }
    .ok_or_else(truncated)?;
    let picture = Picture {
        width: width as usize,
        height: height as usize,
        mode: Mode::Rgba,
        data: rgba,
    };
    Ok(picture.convert(fourcc.mode()))
}

/// The textures a style draws: the keyboard adds the stick and D-pad icons and leaves the lyre.
pub fn textures(style: &str) -> Vec<(&'static str, Texture)> {
    if style == "keyboard" {
        let mut drawn: Vec<(&str, Texture)> = TEXTURES
            .iter()
            .filter(|(_, t)| !t.form.is_lyre())
            .copied()
            .collect();
        drawn.extend(KEYBOARD_TEXTURES);
        return drawn;
    }
    TEXTURES.to_vec()
}

/// Replaces the glyph textures inside the WAD in `data`; returns how many. A texture that cannot be
/// drawn is left as it is, with a Log line.
pub fn patch(
    data: &mut [u8],
    style: &str,
    faces: Option<&Faces>,
    pad_map: &str,
    key_map: &str,
    assets: &Assets,
    log: &mut dyn FnMut(String),
) -> usize {
    let painter = Painter::new(assets);
    let choice = Choice {
        style,
        faces,
        pad_map,
        key_map,
    };
    let drawn = textures(style);
    let mut count = 0;
    for entry in entries(data) {
        let id: String = entry.name.chars().take(10).collect();
        let controller = id == CONTROLLER_TEXTURE && (style == "xbox" || style == "nintendo");
        let texture = drawn.iter().find(|(name, _)| *name == id).map(|(_, t)| *t);
        if !entry.name.ends_with(".dds") || (texture.is_none() && !controller) {
            continue;
        }
        let end = entry.offset + 0x40 + entry.size;
        let start = entry.offset + 0x40;
        let dds = data[start..end]
            .windows(4)
            .position(|w| w == b"DDS ")
            .map(|i| i + start);
        let fourcc = dds.and_then(|dds| match data.get(dds + 84..dds + 88) {
            Some(b"DXT1") => Some(FourCc::Dxt1),
            Some(b"DXT5") => Some(FourCc::Dxt5),
            _ => None,
        });
        let (Some(dds), Some(fourcc)) = (dds, fourcc) else {
            log(format!(
                "Glyphs: {}: not a DXT texture, left as it is",
                entry.name
            ));
            continue;
        };
        let height = u32_at(data, dds + 12).unwrap_or(0) as usize;
        let width = u32_at(data, dds + 16).unwrap_or(0) as usize;
        let texture = match texture {
            Some(texture) if !controller => texture,
            _ => Texture {
                form: Form::Controller,
                buttons: &[],
            },
        };
        let result = if width > MAX_SIDE || height > MAX_SIDE {
            Err(GlyphError::TooLarge { width, height })
        } else if texture.form.is_lyre() {
            decode_texture(&data[dds..end], fourcc).and_then(|original| {
                let kind = texture.buttons.first().copied().unwrap_or("");
                let painted = lyre_paint(&original, texture.form, kind, &choice, &painter)?;
                let shrink = (2048 / width.max(1)).max(1);
                let region = LYRE_CORNER.map(|v| v / shrink);
                encode_region(data, dds, end, &painted, fourcc, region, Some(&original))
            })
        } else {
            render(
                texture.form,
                texture.buttons,
                &choice,
                width,
                height,
                &painter,
            )
            .and_then(|picture| {
                // The DDS magic lies inside the entry; its header may not.
                let room = end - dds;
                let length = room.checked_sub(128).ok_or(GlyphError::Length {
                    drawn: 0,
                    length: room as i64 - 128,
                })?;
                encode(&picture, fourcc, length)
            })
            .map(|pixels| data[dds + 128..end].copy_from_slice(&pixels))
        };
        if let Err(error) = result {
            log(format!("Glyphs: {}: {error}, left as it is", entry.name));
            continue;
        }
        count += 1;
    }
    count
}

fn mtime_ns(meta: &std::fs::Metadata) -> Option<u64> {
    let since = meta
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?;
    u64::try_from(since.as_nanos()).ok()
}

/// `{guest path: patched copy}` for the runner to mount; nothing (and no copies) for the PlayStation
/// glyphs. The copies in `out/glyphs` are kept until the style, the mapping (the keyboard one for
/// the keyboard style, the gamepad one otherwise) or the game's WADs change; `stamp.json` records
/// them as the Python's `json.dumps` does.
#[allow(clippy::too_many_arguments)]
pub fn build(
    game: &Path,
    out: &Path,
    style: &str,
    faces: Option<&Faces>,
    pad_map: &str,
    key_map: &str,
    assets: &Assets,
    log: &mut dyn FnMut(String),
) -> Result<IndexMap<String, PathBuf>, GlyphError> {
    let target = out.join("glyphs");
    if !is_style(style) {
        let _ = std::fs::remove_dir_all(&target);
        return Ok(IndexMap::new());
    }
    let (pad_map, key_map) = if style == "keyboard" {
        ("", key_map)
    } else {
        (pad_map, "")
    };
    let sources: Vec<(&str, PathBuf)> = WADS
        .iter()
        .map(|name| (*name, game.join(name)))
        .filter(|(_, path)| path.is_file())
        .collect();
    let art = assets.controller_art.join(format!("{style}.png"));
    let art_mtime = if style == "xbox" || style == "nintendo" {
        std::fs::metadata(&art)
            .ok()
            .filter(|m| m.is_file())
            .and_then(|m| mtime_ns(&m))
    } else {
        None
    };
    let mut stamp = vec![
        json!(VERSION),
        json!(style),
        faces.map_or(Value::Null, Faces::json),
        json!(pad_map),
    ];
    if !key_map.is_empty() {
        stamp.push(json!(key_map));
    }
    stamp.push(json!(art_mtime));
    let mut listed = Vec::new();
    for (name, path) in &sources {
        let meta = std::fs::metadata(path)?;
        listed.push(json!([name, meta.len(), mtime_ns(&meta)]));
    }
    stamp.push(Value::Array(listed));
    let stamp = crate::core::patches::python_json(&Value::Array(stamp))?;
    let stamp_file = target.join("stamp.json");
    let file_name = |name: &str| {
        Path::new(name)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    };
    let mut made: Vec<String> = std::fs::read_dir(&target)
        .map(|dir| {
            dir.filter_map(|e| e.ok())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|n| n.ends_with(".WAD"))
                .collect()
        })
        .unwrap_or_default();
    if std::fs::read_to_string(&stamp_file).ok().as_deref() != Some(stamp.as_str()) {
        let _ = std::fs::remove_dir_all(&target);
        std::fs::create_dir_all(&target)?;
        made.clear();
        for (name, path) in &sources {
            let mut data = std::fs::read(path)?;
            if patch(&mut data, style, faces, pad_map, key_map, assets, log) > 0 {
                atomic_write(&target.join(file_name(name)), &data)?;
                made.push(file_name(name));
            }
        }
        atomic_write(&stamp_file, stamp.as_bytes())?; // last: an interrupted build is redone
    }
    Ok(sources
        .iter()
        .filter(|(name, _)| made.contains(&file_name(name)))
        .map(|(name, _)| (format!("/app0/{name}"), target.join(file_name(name))))
        .collect())
}

#[cfg(test)]
pub(crate) mod tests {
    //! `tests/test_glyphs.py` (GlyphTable, GlyphPatch, KeyboardStick, AtlasAlignment) on synthetic
    //! WADs; `RunGlyphs` is in `runner/mod.rs`.
    use super::*;

    fn port() -> PathBuf {
        PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/.."))
    }

    fn assets() -> Assets {
        Assets::in_port(&port())
    }

    fn entry(name: &str, payload: &[u8], tag: u16) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend(tag.to_le_bytes());
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

    /// The DDS header Pillow writes for a 4 x 4 texture, with `width` and `height`.
    fn dds_header(width: u32, height: u32, fourcc: &str) -> Vec<u8> {
        let mut header = vec![0u8; 128];
        header[..4].copy_from_slice(b"DDS ");
        header[4..8].copy_from_slice(&124u32.to_le_bytes());
        header[8..12].copy_from_slice(&0x0008_1007u32.to_le_bytes());
        header[12..16].copy_from_slice(&height.to_le_bytes());
        header[16..20].copy_from_slice(&width.to_le_bytes());
        header[20..24].copy_from_slice(&0x1cu32.to_le_bytes());
        header[76..80].copy_from_slice(&32u32.to_le_bytes());
        header[80..84].copy_from_slice(&4u32.to_le_bytes());
        header[84..88].copy_from_slice(fourcc.as_bytes());
        header[108..112].copy_from_slice(&0x1000u32.to_le_bytes());
        header
    }

    /// A texture entry as the game stores it: a 0x4C prefix and a DDS with `levels` mip levels
    /// (or `length` bytes of pixels).
    fn texture(
        name: &str,
        width: u32,
        height: u32,
        fourcc: &str,
        levels: u32,
        length: usize,
    ) -> Vec<u8> {
        let mut length = length;
        let block = if fourcc == "DXT5" { 16 } else { 8 };
        for level in 0..if length != 0 { 0 } else { levels } {
            let blocks = (width >> level).max(1).div_ceil(4) * (height >> level).max(1).div_ceil(4);
            length += blocks as usize * block;
        }
        let mut payload = b"\x07\x00\x01\x00".to_vec();
        let mut field = name.as_bytes().to_vec();
        field.resize(0x48, 0);
        payload.extend(field);
        payload.extend(dds_header(width, height, fourcc));
        payload.extend(vec![0xee; length]);
        entry(name, &payload, 1)
    }

    pub(crate) fn wad() -> Vec<u8> {
        let mut material = b"TX1BBACF4D.2E0253CF".to_vec();
        material.extend([1u8; 30]);
        [
            entry("WAD_R_UI", &[0; 8], 0x14),
            entry("GroupStart", &[], 2),
            entry("MAT_macroxicon", &material, 1),
            texture("TX1BBACF4D.2E0253CF.dds", 64, 64, "DXT5", 1, 0), // cross, inline
            texture("TX7C8D1BEB.350710D4.dds", 64, 64, "DXT5", 1, 0), // not a glyph
            texture("TX7FF61BF7.CC842B4F.dds", 128, 128, "DXT1", 7, 0), // cross, QTE
            texture("TXD43D1443.2B12201C.dds", 64, 40, "DXT5", 3, 0), // select
            texture("TXFB91136E.B22AD3A4.dds", 256, 256, "DXT1", 8, 0), // L2/L1 atlas
            entry("GroupEnd", &[], 3),
        ]
        .concat()
    }

    fn names(data: &[u8]) -> Vec<String> {
        entries(data).into_iter().map(|e| e.name).collect()
    }

    fn quiet() -> impl FnMut(String) {
        |_| {}
    }

    fn patched(data: &[u8], style: &str, pad_map: &str, key_map: &str) -> (Vec<u8>, usize) {
        let mut data = data.to_vec();
        let count = patch(
            &mut data,
            style,
            None,
            pad_map,
            key_map,
            &assets(),
            &mut quiet(),
        );
        (data, count)
    }

    fn find(data: &[u8], needle: &[u8], from: usize, to: usize) -> Option<usize> {
        data[from..to]
            .windows(needle.len())
            .position(|w| w == needle)
            .map(|i| i + from)
    }

    fn faces(
        south: Option<&str>,
        east: Option<&str>,
        west: Option<&str>,
        north: Option<&str>,
    ) -> Faces {
        let own = |v: Option<&str>| v.map(str::to_string);
        Faces {
            south: own(south),
            east: own(east),
            west: own(west),
            north: own(north),
        }
    }

    // -- GlyphTable -------------------------------------------------------------------------------

    #[test]
    fn entries_walk_the_whole_file_and_stop_at_a_truncated_entry() {
        let data = [
            entry("A", b"xxxxx", 1),
            entry("B", &[], 1),
            entry("C", &[b'y'; 16], 1),
        ]
        .concat();
        let found: Vec<(usize, usize, String)> = entries(&data)
            .into_iter()
            .map(|e| (e.offset, e.size, e.name))
            .collect();
        assert_eq!(
            found,
            [
                (0, 5, "A".to_string()),
                (0x50, 0, "B".to_string()),
                (0x90, 16, "C".to_string())
            ]
        );
        assert_eq!(names(&data[..data.len() - 8]), ["A", "B"]);
    }

    #[test]
    fn entity_count_marker_carries_no_data() {
        // level WADs: the marker's size field is a count; the next entry follows its header at once
        let mut data = entry("WAD_Oly150", &[0; 8], 0x14);
        data.extend(&entry("EntityCount", &[], 0)[..0x40]);
        let cut = data.len() - 4;
        data.truncate(cut);
        data.extend(86u32.to_le_bytes());
        data.extend(entry("SBP_OLY150", &[1; 40], 1));
        data.extend(entry("GroupEnd", &[], 3));
        data[0x40 + 16 + 4..0x40 + 16 + 8].copy_from_slice(&86u32.to_le_bytes()); // the count where the size goes
        assert_eq!(
            names(&data),
            ["WAD_Oly150", "EntityCount", "SBP_OLY150", "GroupEnd"]
        );
    }

    #[test]
    fn glyph_of_each_style() {
        let xbox = |host: &str| glyph(host, "xbox", None);
        let a = xbox("a");
        assert_eq!(
            (a.shape, a.label.as_str(), a.color),
            (Shape::Disc, "A", [108, 196, 74])
        );
        let short = |g: Glyph| (g.shape, g.label);
        assert_eq!(
            short(glyph("a", "nintendo", None)),
            (Shape::Disc, "B".to_string())
        ); // south on a Switch pad
        assert_eq!(
            short(xbox("leftshoulder")),
            (Shape::Plate, "LB".to_string())
        );
        assert_eq!(
            short(glyph("righttrigger", "nintendo", None)),
            (Shape::Plate, "ZR".to_string())
        );
        assert_eq!(short(xbox("leftstick")), (Shape::Disc, "LS".to_string()));
        assert_eq!(short(xbox("start")), (Shape::Plate, "menu".to_string()));
        assert_eq!(
            short(glyph("back", "nintendo", None)),
            (Shape::Disc, "minus".to_string())
        );
        assert_eq!(short(xbox("dpup")), (Shape::Plate, "dpup".to_string()));
        assert_eq!(short(xbox("paddle1")), (Shape::Plate, "PA".to_string()));
    }

    #[test]
    fn the_letters_of_the_controller_win_over_the_style() {
        let faces = faces(Some("b"), Some("a"), Some("unknown"), None);
        assert_eq!(glyph("a", "xbox", Some(&faces)).label, "B");
        assert_eq!(glyph("x", "xbox", Some(&faces)).label, "X");
        assert_eq!(glyph("y", "xbox", Some(&faces)).label, "Y");
    }

    #[test]
    fn pad_map_moves_the_glyph() {
        let buttons = pad_buttons("cross=b, Circle = a,l2=x,nonsense=y,square=");
        assert_eq!(
            (buttons["cross"].as_str(), buttons["circle"].as_str()),
            ("b", "a")
        );
        assert_eq!(
            (buttons["l2"].as_str(), buttons["square"].as_str()),
            ("lefttrigger", "x")
        );
        let defaults: IndexMap<String, String> = DEFAULT_MAP
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        assert_eq!(pad_buttons(""), defaults);
        for (_, texture) in TEXTURES.iter().filter(|(_, t)| !t.form.is_lyre()) {
            for button in texture.buttons {
                assert!(defaults.contains_key(*button), "{button}");
            }
        }
        for (button, _) in LYRE_DISCS {
            assert!(defaults.contains_key(button), "{button}");
        }
    }

    #[test]
    fn key_map_moves_the_key() {
        let keys = key_buttons("Cross = Q, square=Left Shift,l2=G,nonsense=Y,circle=");
        assert_eq!(
            (
                keys["cross"].as_str(),
                keys["square"].as_str(),
                keys["l2"].as_str()
            ),
            ("Q", "Left Shift", "G")
        );
        assert_eq!(keys["circle"], "Left Shift"); // empty: the default
        assert_eq!(key_buttons("l3=None")["l3"], "Z");
        let defaults: IndexMap<String, String> = DEFAULT_KEYS
            .iter()
            .chain(STICK_KEYS.iter())
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        assert_eq!(key_buttons(""), defaults);
        let keyboard_only: Vec<&str> = DEFAULT_KEYS
            .iter()
            .map(|(k, _)| *k)
            .filter(|k| !DEFAULT_MAP.iter().any(|(m, _)| m == k))
            .collect();
        assert_eq!(keyboard_only, ["up"]); // the keyboard-only D-pad icon
    }

    #[test]
    fn rage_on_one_input_draws_its_key_on_l3_and_r3() {
        let keys = key_buttons("l3=None,r3=None,rage=Mouse Middle,grab=C");
        assert_eq!(
            (keys["l3"].as_str(), keys["r3"].as_str()),
            ("Mouse Middle", "Mouse Middle")
        );
        let keys = key_buttons("l3=None,r3=V"); // no rage key: an unbound L3 keeps the default
        assert_eq!((keys["l3"].as_str(), keys["r3"].as_str()), ("Z", "V"));
    }

    #[test]
    fn key_caps() {
        let space = glyph("Space", "keyboard", None);
        assert_eq!(
            (space.shape, space.label.as_str(), space.color),
            (Shape::Plate, "SPACE", INK)
        );
        let short = |host: &str| {
            let g = glyph(host, "keyboard", None);
            (g.shape, g.label)
        };
        let of = |shape: Shape, label: &str| (shape, label.to_string());
        assert_eq!(short("Left Shift"), of(Shape::Plate, "SHIFT"));
        assert_eq!(short("e"), of(Shape::Key, "E"));
        assert_eq!(short("a"), of(Shape::Key, "A")); // a key, not the south button
        assert_eq!(short("F10"), of(Shape::Plate, "F10"));
        assert_eq!(short("Up"), of(Shape::Key, "dpup"));
        assert_eq!(short("Keypad 4"), of(Shape::Plate, "NUM 4"));
        assert_eq!(short("Return"), of(Shape::Plate, "ENTER"));
        assert_eq!(short("Mouse Left"), of(Shape::Plate, "LMB"));
        assert_eq!(short("Mouse Back"), of(Shape::Key, "M4"));
    }

    #[test]
    fn gamepad_line() {
        let line = "CPU: complete\nGamepad: type=xboxone south=a east=b west=x north=y name=Xbox One Controller\n";
        assert_eq!(
            parse_gamepad(line),
            Some(Gamepad {
                kind: "xbox".to_string(),
                faces: faces(Some("a"), Some("b"), Some("x"), Some("y")),
                name: "Xbox One Controller".to_string(),
            })
        );
        let kind = |line: &str| parse_gamepad(line).map(|g| g.kind);
        assert_eq!(
            kind("Gamepad: type=ps5 south=cross east=circle west=square north=triangle name=x")
                .as_deref(),
            Some("playstation")
        );
        assert_eq!(
            kind("Gamepad: type=switchpro south=b east=a west=y north=x name=Pro").as_deref(),
            Some("nintendo")
        );
        assert_eq!(
            kind("Gamepad: type=standard south=a east=b west=x north=y name=").as_deref(),
            Some("xbox")
        );
        assert_eq!(parse_gamepad("Gamepad: none\n"), None);
        assert_eq!(parse_gamepad("Usage: spartanport-probe boot.bin\n"), None); // a probe without --gamepad
    }

    #[test]
    fn resolve() {
        let pad = "Gamepad: type=switchpro south=b east=a west=y north=x name=Pro Controller";
        let auto = super::resolve("auto", pad);
        assert_eq!(
            (auto.style.as_str(), auto.name.as_str()),
            ("nintendo", "Pro Controller")
        );
        let none = Resolved {
            style: "keyboard".to_string(),
            faces: None,
            name: String::new(),
        };
        assert_eq!(super::resolve("auto", "Gamepad: none"), none); // no gamepad: the keys
        assert_eq!(super::resolve("keyboard", pad), none);
        let xbox = super::resolve("xbox", pad); // chosen: its own letters
        assert_eq!(
            (xbox.style.as_str(), xbox.faces, xbox.name.as_str()),
            ("xbox", None, "")
        );
        assert_eq!(super::resolve("off", pad).style, "playstation");
        assert_eq!(super::resolve("", pad).style, "playstation");
    }

    // -- GlyphPatch -------------------------------------------------------------------------------

    struct Game {
        _dir: tempfile::TempDir,
        game: PathBuf,
        out: PathBuf,
        source: PathBuf,
    }

    fn game() -> Game {
        let dir = tempfile::tempdir().unwrap();
        let game = dir.path().join("game");
        let out = dir.path().join("out");
        std::fs::create_dir_all(game.join("EXEC/WAD/PS4_LE")).unwrap();
        let source = game.join("EXEC/WAD/PS4_LE/R_UI.WAD");
        std::fs::write(&source, wad()).unwrap();
        Game {
            _dir: dir,
            game,
            out,
            source,
        }
    }

    const R_UI: &str = "/app0/EXEC/WAD/PS4_LE/R_UI.WAD";

    fn build_in(g: &Game, style: &str, pad_map: &str, key_map: &str) -> IndexMap<String, PathBuf> {
        build(
            &g.game,
            &g.out,
            style,
            None,
            pad_map,
            key_map,
            &assets(),
            &mut quiet(),
        )
        .unwrap()
    }

    #[test]
    fn patch_keeps_every_size_and_only_touches_glyph_pixels() {
        let original = wad();
        let (data, count) = patched(&original, "xbox", "", "");
        assert_eq!(count, 4);
        assert_eq!(data.len(), original.len());
        assert_eq!(entries(&data), entries(&original));
        for e in entries(&original) {
            let span = e.offset..e.offset + 0x40 + e.size;
            let (before, after) = (&original[span.clone()], &data[span]);
            if TEXTURES.iter().any(|(id, _)| e.name.starts_with(id)) && e.name.ends_with(".dds") {
                // The entry and DDS headers (size, format, mip count) are kept; every level redrawn.
                let pixels = find(before, b"DDS ", 0, before.len()).unwrap() + 128;
                assert_eq!(before[..pixels], after[..pixels], "{}", e.name);
                assert!(
                    !after[pixels..]
                        .windows(64)
                        .any(|w| w.iter().all(|b| *b == 0xee)),
                    "{}",
                    e.name
                );
                let width =
                    u32::from_le_bytes(before[pixels - 112..pixels - 108].try_into().unwrap());
                let height =
                    u32::from_le_bytes(before[pixels - 116..pixels - 112].try_into().unwrap());
                let decode = if &before[pixels - 44..pixels - 40] == b"DXT5" {
                    crate::runner::dxt::decode_dxt5
                } else {
                    crate::runner::dxt::decode_dxt1
                };
                assert!(
                    decode(&after[pixels..], width, height).is_some(),
                    "{}",
                    e.name
                );
            } else {
                assert_eq!(before, after, "{}", e.name);
            }
        }
    }

    #[test]
    fn lyre_discs_are_repainted_in_place() {
        let original = [
            texture("TX2CDFA5F8.F2C4FECB.dds", 2048, 2048, "DXT1", 12, 0),
            texture("TX4158CBF9.70725051.dds", 2048, 2048, "DXT1", 12, 0),
            texture("TX0C09B6DB.7485CA2D.dds", 2048, 2048, "DXT1", 12, 0),
            texture("TXF489D97F.2CBA0C02.dds", 2048, 2048, "DXT5", 12, 0),
            texture("TXEACA5E71.B9CB5700.dds", 512, 512, "DXT1", 10, 0),
            texture("TXBD54B283.5551218B.dds", 2048, 2048, "DXT1", 12, 0),
            texture("TXCEE0EE2B.BC09F12D.dds", 2048, 2048, "DXT1", 12, 0),
        ]
        .concat();
        let (keyboard, count) = patched(&original, "keyboard", "", "");
        assert_eq!(count, 0); // the keys are the player's: left alone
        assert_eq!(keyboard, original);
        let mut copies = Vec::new();
        for style in ["xbox", "nintendo"] {
            let (data, count) = patched(&original, style, "", "");
            assert_eq!(count, 7);
            assert_eq!(data.len(), original.len());
            assert_eq!(entries(&data), entries(&original));
            for e in entries(&original) {
                let end = e.offset + 0x40 + e.size;
                let dds = find(&original, b"DDS ", e.offset + 0x40, end).unwrap();
                assert_eq!(
                    data[e.offset..dds + 128],
                    original[e.offset..dds + 128],
                    "{}",
                    e.name
                );
                let width =
                    u32::from_le_bytes(original[dds + 16..dds + 20].try_into().unwrap()) as usize;
                let corner = if width == 2048 {
                    LYRE_CORNER
                } else {
                    LYRE_CORNER.map(|v| v / 4)
                };
                // top level: blocks outside the corner are untouched, blocks inside are redrawn
                let blocks = width / 4;
                let block = if &original[dds + 84..dds + 88] == b"DXT5" {
                    16
                } else {
                    8
                };
                for by in 0..blocks {
                    let row = dds + 128 + by * blocks * block;
                    for bx in 0..blocks {
                        let inside = corner[0] <= bx * 4
                            && bx * 4 < corner[2]
                            && corner[1] <= by * 4
                            && by * 4 < corner[3];
                        let at = row + bx * block;
                        if !inside {
                            assert_eq!(
                                data[at..at + block],
                                original[at..at + block],
                                "{} {bx} {by}",
                                e.name
                            );
                        }
                    }
                }
                assert_ne!(data[dds + 128..end], original[dds + 128..end], "{}", e.name);
                let w = width as u32;
                let decode = if block == 16 {
                    crate::runner::dxt::decode_dxt5
                } else {
                    crate::runner::dxt::decode_dxt1
                };
                assert!(decode(&data[dds + 128..end], w, w).is_some(), "{}", e.name);
            }
            copies.push(data);
        }
        assert_ne!(copies[0], copies[1]);
    }

    #[test]
    fn encode_region_touches_only_changed_blocks() {
        let original = texture("TX2CDFA5F8.F2C4FECB.dds", 64, 64, "DXT1", 7, 0);
        let mut data = original.clone();
        let dds = find(&original, b"DDS ", 0, original.len()).unwrap();
        let before = decode_texture(&original[dds..], FourCc::Dxt1).unwrap();
        let mut painted = before.clone();
        painted.fill(&[255, 255, 255], 8, 8, 16, 16, None); // four blocks of the 16 x 16 region at (0, 0)
        let length = original.len();
        encode_region(
            &mut data,
            dds,
            length,
            &painted,
            FourCc::Dxt1,
            [0, 0, 16, 16],
            Some(&before),
        )
        .unwrap();
        assert_eq!(data.len(), original.len());
        let top = dds + 128;
        let changed: Vec<usize> = (top..top + 16 * 16 * 8)
            .step_by(8)
            .filter(|i| data[*i..*i + 8] != original[*i..*i + 8])
            .map(|i| (i - top) / 8)
            .collect();
        assert_eq!(changed, [2 * 16 + 2, 2 * 16 + 3, 3 * 16 + 2, 3 * 16 + 3]);
        // the smaller levels: only the blocks the white square reaches, nothing beyond the region
        let rest: Vec<usize> = (top + 16 * 16 * 8..original.len())
            .step_by(8)
            .filter(|i| {
                data[*i..(*i + 8).min(data.len())] != original[*i..(*i + 8).min(original.len())]
            }) // Python slices past the end
            .collect();
        assert!(!rest.is_empty() && rest.len() <= 12, "{rest:?}"); // 3 at level 1, then the tiny levels
        let level1 = top + 16 * 16 * 8;
        assert_eq!(
            data[level1 + 2 * 8 * 8..level1 + 8 * 8 * 8],
            original[level1 + 2 * 8 * 8..level1 + 8 * 8 * 8]
        ); // level 1, rows 2+
    }

    #[test]
    fn remap_and_style_change_the_pixels() {
        let mut drawn = Vec::new();
        for (style, pad_map, key_map) in [
            ("xbox", "", ""),
            ("nintendo", "", ""),
            ("xbox", "cross=b", ""),
            ("keyboard", "", ""),
            ("keyboard", "", "cross=Q,l1=G"),
        ] {
            drawn.push(patched(&wad(), style, pad_map, key_map).0);
        }
        for i in 0..drawn.len() {
            for j in i + 1..drawn.len() {
                assert_ne!(drawn[i], drawn[j], "{i} {j}");
            }
        }
    }

    #[test]
    fn controller_diagram_follows_the_gamepad_style() {
        let original = texture("TXCE352DC5.12345678.dds", 1024, 512, "DXT5", 3, 0);
        let mut copies = Vec::new();
        for style in ["xbox", "nintendo"] {
            let (data, count) = patched(&original, style, "", "");
            assert_eq!(count, 1);
            assert_eq!(entries(&data), entries(&original));
            let pixels = find(&original, b"DDS ", 0, original.len()).unwrap() + 128;
            assert_eq!(data[..pixels], original[..pixels]);
            assert_ne!(data[pixels..], original[pixels..]);
            assert!(crate::runner::dxt::decode_dxt5(&data[pixels..], 1024, 512).is_some());
            copies.push(data);
        }
        assert_ne!(copies[0], copies[1]);
    }

    #[test]
    fn keyboard_keeps_the_original_controller_diagram() {
        let original = texture("TXCE352DC5.12345678.dds", 1024, 512, "DXT5", 1, 0);
        let (data, count) = patched(&original, "keyboard", "", "");
        assert_eq!(count, 0);
        assert_eq!(data, original);
    }

    #[test]
    fn diagram_is_mounted_and_the_original_is_untouched() {
        let g = game();
        let original = texture("TXCE352DC5.12345678.dds", 1024, 512, "DXT5", 1, 0);
        std::fs::write(&g.source, &original).unwrap();
        for style in ["xbox", "nintendo"] {
            let copies = build_in(&g, style, "", "");
            assert_ne!(std::fs::read(&copies[R_UI]).unwrap(), original);
            assert_eq!(std::fs::read(&g.source).unwrap(), original);
        }
        assert!(build_in(&g, "playstation", "", "").is_empty());
    }

    #[test]
    fn missing_diagram_art_keeps_the_prompts_and_is_retried_when_restored() {
        let g = game();
        let original = [
            wad(),
            texture("TXCE352DC5.12345678.dds", 1024, 512, "DXT5", 1, 0),
        ]
        .concat();
        std::fs::write(&g.source, &original).unwrap();
        let missing = Assets {
            controller_art: g.out.join("missing-art"),
            ..assets()
        };
        let copies = build(
            &g.game,
            &g.out,
            "xbox",
            None,
            "",
            "",
            &missing,
            &mut quiet(),
        )
        .unwrap();
        let incomplete = std::fs::read(&copies[R_UI]).unwrap();
        let copies = build_in(&g, "xbox", "", "");
        assert_ne!(std::fs::read(&copies[R_UI]).unwrap(), incomplete);
        assert_ne!(incomplete, original);
        assert_eq!(std::fs::read(&g.source).unwrap(), original);
    }

    #[test]
    fn the_keyboard_follows_only_the_key_map() {
        let g = game();
        let copy = build_in(&g, "keyboard", "", "")[R_UI].clone();
        let first = std::fs::read(&copy).unwrap();
        // A copy that is not redrawn keeps what it holds: the gamepad map changes nothing here.
        std::fs::write(&copy, b"kept").unwrap();
        build_in(&g, "keyboard", "cross=b", "");
        assert_eq!(std::fs::read(&copy).unwrap(), b"kept");
        std::fs::write(&copy, &first).unwrap();
        build_in(&g, "keyboard", "", "cross=Q");
        assert_ne!(std::fs::read(&copy).unwrap(), first);
        assert_eq!(std::fs::read(&g.source).unwrap(), wad());
    }

    #[test]
    fn a_texture_of_another_size_is_left_alone() {
        let mut data = texture("TX7FF61BF7.CC842B4F.dds", 128, 128, "DXT1", 0, 1000);
        let before = data.clone();
        let mut log = Vec::new();
        let count = patch(&mut data, "xbox", None, "", "", &assets(), &mut |line| {
            log.push(line)
        });
        assert_eq!(count, 0);
        assert!(log.last().unwrap().contains("left as it is"), "{log:?}");
        assert_eq!(data, before);
    }

    /// What `patch` logs for `data`, a WAD of one glyph texture, which it must leave as it is.
    fn refused(data: &[u8]) -> Vec<String> {
        let mut copy = data.to_vec();
        let mut log = Vec::new();
        let count = patch(&mut copy, "xbox", None, "", "", &assets(), &mut |line| {
            log.push(line)
        });
        assert_eq!(count, 0, "{log:?}");
        assert_eq!(copy, data);
        log
    }

    #[test]
    fn a_texture_cut_short_inside_its_header_is_left_alone() {
        // The entry ends 28 bytes before the DDS header does: the Python's encode gets a
        // negative length and says so.
        let name = "TX7FF61BF7.CC842B4F.dds";
        let mut payload = b"\x07\x00\x01\x00".to_vec();
        let mut field = name.as_bytes().to_vec();
        field.resize(0x48, 0);
        payload.extend(field);
        payload.extend(&dds_header(128, 128, "DXT1")[..100]);
        assert_eq!(
            refused(&entry(name, &payload, 1)),
            [format!(
                "Glyphs: {name}: 0 bytes drawn for a texture of -28, left as it is"
            )]
        );
    }

    #[test]
    fn a_texture_larger_than_4096_a_side_is_left_alone() {
        // A corrupt WAD's DDS header can claim any size: nothing is drawn, nor allocated, at it.
        let name = "TX7FF61BF7.CC842B4F.dds";
        for (width, height) in [(1_000_000, 1_000_000), (4097, 4), (4, 4097)] {
            assert_eq!(
                refused(&texture(name, width, height, "DXT1", 0, 64)),
                [format!(
                    "Glyphs: {name}: a {width}x{height} texture is larger than 4096x4096, left \
                     as it is"
                )]
            );
        }
    }

    #[test]
    fn encode_rejects_a_length_it_cannot_fill() {
        let image = Picture::new(Mode::Rgba, 64, 64, &[]);
        assert_eq!(encode(&image, FourCc::Dxt5, 4096).unwrap().len(), 4096);
        assert_eq!(
            encode(&image, FourCc::Dxt1, 2048 + 512).unwrap().len(),
            2560
        );
        assert!(encode(&image, FourCc::Dxt5, 4000).is_err());
    }

    #[test]
    fn build_writes_copies_and_leaves_the_game_alone() {
        let g = game();
        let before = std::fs::read(&g.source).unwrap();
        let mounts = build_in(&g, "xbox", "", "");
        assert_eq!(mounts.keys().collect::<Vec<_>>(), [R_UI]); // no R_PERM.WAD in this game
        let copy = &mounts[R_UI];
        assert_eq!(copy, &g.out.join("glyphs").join("R_UI.WAD"));
        assert_eq!(std::fs::metadata(copy).unwrap().len(), before.len() as u64);
        assert_ne!(std::fs::read(copy).unwrap(), before);
        assert_eq!(std::fs::read(&g.source).unwrap(), before);
    }

    #[test]
    fn build_reuses_the_copies_until_something_changes() {
        let g = game();
        let copy = build_in(&g, "xbox", "", "")[R_UI].clone();
        let first = std::fs::read(&copy).unwrap();
        // Not redrawn: the copy keeps what it holds.
        std::fs::write(&copy, b"kept").unwrap();
        let again = build_in(&g, "xbox", "", "");
        assert_eq!(
            again.into_iter().collect::<Vec<_>>(),
            [(R_UI.to_string(), copy.clone())]
        );
        assert_eq!(std::fs::read(&copy).unwrap(), b"kept");
        std::fs::write(&copy, &first).unwrap();
        build_in(&g, "xbox", "cross=b", "");
        assert_ne!(std::fs::read(&copy).unwrap(), first);
        build_in(&g, "nintendo", "", "");
        assert_ne!(std::fs::read(&copy).unwrap(), first);
    }

    #[test]
    fn playstation_and_off_leave_no_copies() {
        let g = game();
        build_in(&g, "xbox", "", "");
        assert!(build_in(&g, "playstation", "", "").is_empty());
        assert!(!g.out.join("glyphs").exists());
    }

    #[test]
    fn a_wad_without_glyphs_is_not_mounted() {
        let g = game();
        let perm = [entry("WAD_R_Perm", &[], 0x14), entry("MAT_x", b"1234", 1)].concat();
        std::fs::write(g.game.join("EXEC/WAD/PS4_LE/R_PERM.WAD"), perm).unwrap();
        assert_eq!(
            build_in(&g, "xbox", "", "").keys().collect::<Vec<_>>(),
            [R_UI]
        );
        assert_eq!(
            build_in(&g, "xbox", "", "").keys().collect::<Vec<_>>(),
            [R_UI]
        );
    }

    #[test]
    fn stamp_is_the_python_json() {
        let g = game();
        build_in(&g, "keyboard", "cross=b", "cross=Q");
        let stamp = std::fs::read_to_string(g.out.join("glyphs/stamp.json")).unwrap();
        let meta = std::fs::metadata(&g.source).unwrap();
        let mtime = meta
            .modified()
            .unwrap()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        assert_eq!(
            stamp,
            format!(
                "[11, \"keyboard\", null, \"\", \"cross=Q\", null, [[\"EXEC/WAD/PS4_LE/R_UI.WAD\", {}, {mtime}]]]",
                meta.len()
            )
        );
    }

    // -- KeyboardStick ----------------------------------------------------------------------------

    #[test]
    fn stick_and_dpad_textures_are_keyboard_only() {
        let mut buttons: Vec<&str> = KEYBOARD_TEXTURES
            .iter()
            .flat_map(|(_, t)| t.buttons.iter().copied())
            .collect();
        buttons.sort();
        buttons.dedup();
        assert_eq!(buttons, ["lstick", "rstick", "up"]);
        assert!(!KEYBOARD_TEXTURES
            .iter()
            .any(|(id, _)| TEXTURES.iter().any(|(t, _)| t == id)));
        assert_eq!(textures("xbox"), TEXTURES.to_vec());
        let mut prompts: Vec<(&str, Texture)> = TEXTURES
            .iter()
            .filter(|(_, t)| !t.form.is_lyre())
            .copied()
            .collect();
        prompts.extend(KEYBOARD_TEXTURES);
        assert_eq!(textures("keyboard"), prompts); // the lyre keeps its letters
    }

    #[test]
    fn sticks_show_the_movement_and_camera_keys() {
        let keys = key_buttons("");
        let lstick = glyph(&keys["lstick"], "keyboard", None);
        assert_eq!(
            (lstick.shape, lstick.label.as_str()),
            (Shape::Plate, "WASD")
        );
        let rstick = glyph(&keys["rstick"], "keyboard", None);
        assert_eq!(
            (rstick.shape, rstick.label.as_str()),
            (Shape::Key, "arrows")
        );
    }

    #[test]
    fn dpad_follows_the_weapon_key() {
        assert_eq!(key_buttons("")["up"], "I");
        assert_eq!(key_buttons("up=1")["up"], "1");
    }

    fn painter() -> Painter<'static> {
        static ASSETS: std::sync::OnceLock<Assets> = std::sync::OnceLock::new();
        Painter::new(ASSETS.get_or_init(assets))
    }

    fn choice(style: &str) -> Choice<'_> {
        Choice {
            style,
            faces: None,
            pad_map: "",
            key_map: "",
        }
    }

    #[test]
    fn arrows_render() {
        let image = render(
            Form::Inline,
            &["rstick"],
            &choice("keyboard"),
            64,
            64,
            &painter(),
        )
        .unwrap();
        assert_eq!((image.width, image.height), (64, 64));
        let mut colours: Vec<&[u8]> = image.data.chunks_exact(4).collect();
        colours.sort();
        colours.dedup();
        assert!(colours.len() > 4);
    }

    // -- AtlasAlignment ---------------------------------------------------------------------------

    /// The rows (or columns) of `image` holding a pixel whose red is above 30.
    fn lit(image: &Picture, rows: std::ops::Range<usize>, by_column: bool) -> Vec<usize> {
        rows.filter(|i| {
            let across = if by_column { image.height } else { image.width };
            (0..across).any(|j| {
                let (x, y) = if by_column { (*i, j) } else { (j, *i) };
                image.pixel(x, y)[0] > 30
            })
        })
        .collect()
    }

    #[test]
    fn atlas_prompts_are_centered_towards_midline() {
        for style in ["xbox", "nintendo", "keyboard"] {
            for pair in [["r2", "r1"], ["l2", "l1"]] {
                let image =
                    render(Form::Atlas, &pair, &choice(style), 256, 256, &painter()).unwrap();
                assert_eq!((image.width, image.height), (256, 256));
                let top = lit(&image, 0..128, false);
                let bottom = lit(&image, 128..256, false);
                assert!(
                    !top.is_empty(),
                    "{style} {pair:?} top half should contain pixels"
                );
                assert!(
                    !bottom.is_empty(),
                    "{style} {pair:?} bottom half should contain pixels"
                );
                let centre = |rows: &[usize]| (rows[0] + rows[rows.len() - 1]) as f64 / 2.0;
                // In each half, the prompt should be ~52 px from midline (y=128), within 3 px tolerance.
                assert!(
                    (128.0 - centre(&top) - 52.0).abs() <= 3.0,
                    "{style} {pair:?}"
                );
                assert!(
                    (centre(&bottom) - 128.0 - 52.0).abs() <= 3.0,
                    "{style} {pair:?}"
                );
            }
        }
    }

    #[test]
    fn qte_face_buttons_remain_centered() {
        let image = render(Form::Qte, &["cross"], &choice("xbox"), 128, 128, &painter()).unwrap();
        assert_eq!((image.width, image.height), (128, 128));
        let columns = lit(&image, 0..128, true);
        let rows = lit(&image, 0..128, false);
        let centre = |v: &[usize]| (v[0] + v[v.len() - 1]) as f64 / 2.0;
        assert!((centre(&columns) - 63.5).abs() <= 2.0);
        assert!((centre(&rows) - 63.5).abs() <= 2.0);
    }
}
