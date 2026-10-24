//! The control tables, key-name rules, `SPARTAN_*_MAP` builders and the settings migration.
//!
//! Key names are compared case-insensitively with `to_lowercase()`; the names involved are
//! ASCII/Latin, where it matches the Python launcher's `casefold()`.

use indexmap::IndexMap;
use serde_json::{Map, Value};

/// A PS4 button, its default key and its default gamepad button (SDL names).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ControlRow {
    pub button: &'static str,
    pub key: &'static str,
    pub pad: Option<&'static str>,
}

const fn row(button: &'static str, key: &'static str, pad: Option<&'static str>) -> ControlRow {
    ControlRow { button, key, pad }
}

/// L2/R2 come from the analog triggers and the left touchpad click (photo mode) from L1 + the
/// right click's button; neither is remapped on the gamepad.
pub static CONTROLS: [ControlRow; 17] = [
    row("cross", "Space", Some("a")),
    row("circle", "Left Shift", Some("b")),
    row("square", "E", Some("x")),
    row("triangle", "Q", Some("y")),
    row("l1", "1", Some("leftshoulder")),
    row("r1", "3", Some("rightshoulder")),
    row("l2", "R", None),
    row("r2", "F", None),
    row("l3", "Z", Some("leftstick")),
    row("r3", "C", Some("rightstick")),
    row("options", "Return", Some("start")),
    row("touchpad_left", "Tab", None),
    row("touchpad_right", "Backspace", Some("back")),
    row("up", "I", Some("dpup")),
    row("down", "K", Some("dpdown")),
    row("left", "J", Some("dpleft")),
    row("right", "L", Some("dpright")),
];

/// Gamepad buttons by SDL name, labelled as on a PlayStation, an Xbox or a Nintendo controller.
pub static PAD_STYLES: &[(&str, &[(&str, &str)])] = &[
    (
        "playstation",
        &[
            ("a", "Cross"),
            ("b", "Circle"),
            ("x", "Square"),
            ("y", "Triangle"),
            ("leftshoulder", "L1"),
            ("rightshoulder", "R1"),
            ("leftstick", "L3"),
            ("rightstick", "R3"),
            ("start", "Options"),
            ("back", "Share / Create"),
            ("touchpad", "Touchpad click"),
            ("guide", "PS button"),
            ("dpup", "D-pad up"),
            ("dpdown", "D-pad down"),
            ("dpleft", "D-pad left"),
            ("dpright", "D-pad right"),
            ("misc1", "Mute button"),
        ],
    ),
    (
        "xbox",
        &[
            ("a", "A"),
            ("b", "B"),
            ("x", "X"),
            ("y", "Y"),
            ("leftshoulder", "LB"),
            ("rightshoulder", "RB"),
            ("leftstick", "LS (left stick click)"),
            ("rightstick", "RS (right stick click)"),
            ("start", "Menu"),
            ("back", "View"),
            ("touchpad", "Touchpad click"),
            ("guide", "Xbox button"),
            ("dpup", "D-pad up"),
            ("dpdown", "D-pad down"),
            ("dpleft", "D-pad left"),
            ("dpright", "D-pad right"),
            ("misc1", "Share"),
        ],
    ),
    // By position, as printed on a Nintendo controller: the south button is B, the east one A.
    (
        "nintendo",
        &[
            ("a", "B"),
            ("b", "A"),
            ("x", "Y"),
            ("y", "X"),
            ("leftshoulder", "L"),
            ("rightshoulder", "R"),
            ("leftstick", "Left stick click"),
            ("rightstick", "Right stick click"),
            ("start", "Plus (+)"),
            ("back", "Minus (\u{2212})"),
            ("touchpad", "Touchpad click"),
            ("guide", "Home button"),
            ("dpup", "D-pad up"),
            ("dpdown", "D-pad down"),
            ("dpleft", "D-pad left"),
            ("dpright", "D-pad right"),
            ("misc1", "Capture"),
        ],
    ),
];

/// Defaults of the keyboard & mouse table where they differ from the game's own keys: the
/// attacks and magic on the mouse, the shoulder buttons next to WASD, the weapons on 1-4.
pub static KEY_DEFAULTS: &[(&str, &str)] = &[
    ("cross", "Space"),
    ("circle", "Left Shift"),
    ("square", "Mouse Left"),
    ("triangle", "Mouse Right"),
    ("l1", "Q"),
    ("r1", "E"),
    ("l2", "R"),
    ("r2", "Mouse Forward"),
    ("l3", "Z"),
    ("r3", "X"),
    ("options", "Tab"),
    ("touchpad_left", "P"),
    ("touchpad_right", "Escape"),
    ("up", "1"),
    ("right", "2"),
    ("down", "3"),
    ("left", "4"),
];

/// Actions of the keyboard map only. Rage of Sparta (L3 + R3 on one input) follows R3.
pub static RAGE_ROW: ControlRow = row("rage", "Mouse Middle", None);
pub static GRAB_ROW: ControlRow = row("grab", "C", None);
pub static DODGE_ROW: ControlRow = row("dodge", "Mouse Back", None);
pub static KEY_ACTIONS: [&ControlRow; 3] = [&RAGE_ROW, &GRAB_ROW, &DODGE_ROW];
pub const RAGE_BUTTONS: [&str; 2] = ["l3", "r3"];
/// Settings layout: 2 = one keyboard & mouse table (`migrate_controls`).
pub const CONTROLS_LAYOUT: u64 = 2;

pub static MOUSE_NAMES: &[&str] = &[
    "Mouse Left",
    "Mouse Middle",
    "Mouse Right",
    "Mouse Back",
    "Mouse Forward",
    "Wheel Up",
    "Wheel Down",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Key,
    Pad,
}

impl Kind {
    fn name(self) -> &'static str {
        match self {
            Kind::Key => "key",
            Kind::Pad => "pad",
        }
    }
}

/// `CONTROLS` rows of a table, in display order; the keyboard one adds its actions.
pub fn table_rows(kind: Kind) -> Vec<&'static ControlRow> {
    if kind != Kind::Key {
        return CONTROLS.iter().collect();
    }
    let mut rows = Vec::new();
    for row in &CONTROLS {
        rows.push(row);
        if row.button == "r3" {
            rows.push(&RAGE_ROW);
        }
    }
    rows.push(&GRAB_ROW);
    rows.push(&DODGE_ROW);
    rows
}

pub fn is_mouse_name(name: &str) -> bool {
    let name = name.trim();
    MOUSE_NAMES.iter().any(|m| m.eq_ignore_ascii_case(name))
}

/// Default input of a table row (`None` when it has none).
pub fn default_input(row: &ControlRow, kind: Kind) -> Option<&'static str> {
    if kind == Kind::Pad {
        return row.pad;
    }
    if row.key.is_empty() {
        return None;
    }
    let default = KEY_DEFAULTS
        .iter()
        .find(|(button, _)| *button == row.button)
        .map_or(row.key, |(_, key)| *key);
    Some(default)
}

/// Whether every row of the `kind` table in `values` holds its default (an input without a
/// default holds nothing); case and outer spaces aside, as `control_changes` compares.
pub fn at_defaults(values: &IndexMap<String, String>, kind: Kind) -> bool {
    table_rows(kind).into_iter().all(|row| {
        let value = values.get(row.button).map_or("", |value| value.trim());
        value.eq_ignore_ascii_case(default_input(row, kind).unwrap_or_default())
    })
}

/// SDL name of a pressed key, from the neutral key names the keyboard layer reports, or `None`
/// when the key cannot be bound.
pub fn sdl_key_name(key: &str) -> Option<String> {
    const NAMED: &[&str] = &[
        "Space",
        "Return",
        "Tab",
        "Backspace",
        "Delete",
        "Escape",
        "Home",
        "End",
        "PageUp",
        "PageDown",
        "CapsLock",
        "Numlock",
        "ScrollLock",
        "Pause",
        "Application",
        "Up",
        "Down",
        "Left",
        "Right",
        "Left Shift",
        "Right Shift",
        "Left Ctrl",
        "Right Ctrl",
        "Left Alt",
        "Right Alt",
        "Equals",
        "Comma",
        "-",
        "[",
        "]",
        "\\",
        ";",
        "'",
        ".",
        "/",
        "`",
        "Keypad Enter",
        "Keypad +",
        "Keypad -",
        "Keypad *",
        "Keypad /",
        "Keypad .",
        "Keypad 0",
        "Keypad 1",
        "Keypad 2",
        "Keypad 3",
        "Keypad 4",
        "Keypad 5",
        "Keypad 6",
        "Keypad 7",
        "Keypad 8",
        "Keypad 9",
    ];
    // Comma and = are saved as Comma and Equals: their own names separate SPARTAN_KEY_MAP entries.
    match key {
        "=" => return Some("Equals".to_string()),
        "," => return Some("Comma".to_string()),
        _ => {}
    }
    if let Some(name) = NAMED.iter().find(|n| **n == key) {
        return Some((*name).to_string());
    }
    let mut chars = key.chars();
    if let (Some(c), None) = (chars.next(), chars.next()) {
        if c.is_ascii_alphanumeric() {
            return Some(c.to_ascii_uppercase().to_string());
        }
    }
    if let Some(digits) = key.strip_prefix('F') {
        if !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()) {
            if let Ok(n) = digits.parse::<u32>() {
                if (1..=24).contains(&n) {
                    return Some(key.to_string());
                }
            }
        }
    }
    None
}

/// Gives `name` to `button`; a button that had it takes the old key of `button`, so no key
/// presses two buttons. Returns the button that swapped.
pub fn bind_key(
    bindings: &mut IndexMap<String, String>,
    button: &str,
    name: &str,
) -> Option<String> {
    let old = bindings.get(button).cloned().unwrap_or_default();
    let wanted = name.to_lowercase();
    let other = bindings
        .iter()
        .find(|(b, k)| b.as_str() != button && k.trim().to_lowercase() == wanted)
        .map(|(b, _)| b.clone());
    bindings.insert(button.to_string(), name.to_string());
    match other {
        Some(other) if !other.is_empty() => {
            bindings.insert(other.clone(), old);
            Some(other)
        }
        _ => None,
    }
}

/// The saved inputs (key) or gamepad names (pad) by game button, for the rows of that table.
/// `touchpad`, saved before the sides were split, is the left click.
pub fn control_values(controls: &Value, kind: Kind) -> IndexMap<String, String> {
    let chosen = match controls.get(kind.name()) {
        Some(Value::Object(chosen)) => chosen,
        _ => return IndexMap::new(),
    };
    let mut values: Map<String, Value> = chosen
        .iter()
        .filter(|(button, _)| button.as_str() != "touchpad")
        .map(|(b, v)| (b.clone(), v.clone()))
        .collect();
    if let Some(touchpad) = chosen.get("touchpad") {
        if !values.contains_key("touchpad_left") {
            values.insert("touchpad_left".to_string(), touchpad.clone());
        }
    }
    let mut out = IndexMap::new();
    for row in table_rows(kind) {
        let has_input = match kind {
            Kind::Pad => row.pad.is_some(),
            Kind::Key => !row.key.is_empty(),
        };
        if !has_input {
            continue;
        }
        if let Some(Value::String(text)) = values.get(row.button) {
            if !text.trim().is_empty() {
                out.insert(row.button.to_string(), text.clone());
            }
        }
    }
    out
}

/// The buttons whose chosen key or SDL gamepad name differs from the default.
pub fn control_changes(values: &IndexMap<String, String>, kind: Kind) -> IndexMap<String, String> {
    let mut out = IndexMap::new();
    for row in table_rows(kind) {
        let Some(default) = default_input(row, kind) else {
            continue;
        };
        let Some(value) = values.get(row.button) else {
            continue;
        };
        let value = value.trim();
        if !value.is_empty() && value.to_lowercase() != default.to_lowercase() {
            out.insert(row.button.to_string(), value.to_string());
        }
    }
    out
}

/// SPARTAN_PAD_MAP value: `cross=a,circle=b` for the changed buttons.
pub fn control_map(controls: &Value, kind: Kind) -> String {
    let chosen = control_values(controls, kind);
    CONTROLS
        .iter()
        .filter_map(|row| {
            let value = chosen.get(row.button)?;
            if value.contains([',', '=']) {
                return None;
            }
            Some(format!("{}={}", row.button, value.trim()))
        })
        .collect::<Vec<_>>()
        .join(",")
}

/// SPARTAN_KEY_MAP of the keyboard & mouse table: each button on its chosen input, written where
/// that differs from the game's own key. With `rage` (one input for Rage of Sparta) L3 and R3 get
/// none and the Rage row is used; otherwise the Rage row is left out.
pub fn keyboard_map(controls: &Value, rage: bool) -> String {
    let chosen = control_values(controls, Kind::Key);
    let mut entries = Vec::new();
    for row in table_rows(Kind::Key) {
        if row.key.is_empty() || (row.button == "rage" && !rage) {
            continue;
        }
        if rage && RAGE_BUTTONS.contains(&row.button) {
            entries.push(format!("{}=None", row.button));
            continue;
        }
        let value = chosen
            .get(row.button)
            .map(String::as_str)
            .filter(|v| !v.is_empty())
            .or_else(|| default_input(row, Kind::Key))
            .unwrap_or("")
            .trim();
        // the game has no key of its own for an action
        let game_default = if CONTROLS.contains(row) { row.key } else { "" };
        if value.to_lowercase() != game_default.to_lowercase() && !value.contains([',', '=']) {
            entries.push(format!("{}={}", row.button, value));
        }
    }
    entries.join(",")
}

/// Python truthiness of a JSON value.
fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}

/// Settings saved before the keyboard & mouse table (`CONTROLS_LAYOUT`). A player who saved keys
/// in Keyboard Mapping keeps the whole layout they had (the game's keys for the rest); one who
/// used Native Translation keeps that table. A new action whose default input is already taken
/// stays without one.
pub fn migrate_controls(saved: Map<String, Value>) -> Map<String, Value> {
    let current = saved
        .get("controls_layout")
        .and_then(Value::as_f64)
        .is_some_and(|n| n == CONTROLS_LAYOUT as f64);
    if saved.is_empty() || current {
        return saved;
    }
    let mut saved = saved;
    let mut controls = match saved.get("controls") {
        Some(Value::Object(map)) => map.clone(),
        _ => Map::new(),
    };
    let native = controls.shift_remove("native");
    let native_input = saved.shift_remove("native_input");
    let native_on = native_input.as_ref().is_some_and(truthy);
    if let (true, Some(Value::Object(native))) = (native_on, &native) {
        controls.insert("key".to_string(), Value::Object(native.clone())); // its defaults are the new ones
    } else if controls.get("key").is_some_and(truthy) {
        let kept = control_values(&Value::Object(controls.clone()), Kind::Key);
        let mut layout: IndexMap<String, String> = CONTROLS
            .iter()
            .filter(|row| !row.key.is_empty())
            .map(|row| {
                let value = kept
                    .get(row.button)
                    .filter(|v| !v.is_empty())
                    .map_or(row.key, String::as_str);
                (row.button.to_string(), value.to_string())
            })
            .collect();
        let rage = saved.get("native_rage").is_none_or(truthy);
        let taken: Vec<String> = layout
            .iter()
            .filter(|(button, _)| !(rage && RAGE_BUTTONS.contains(&button.as_str())))
            .map(|(_, value)| value.trim().to_lowercase())
            .collect();
        for action in KEY_ACTIONS {
            let value = if taken.contains(&action.key.to_lowercase()) {
                "None"
            } else {
                action.key
            };
            layout.insert(action.button.to_string(), value.to_string());
        }
        let layout: Map<String, Value> = layout
            .into_iter()
            .map(|(button, value)| (button, Value::String(value)))
            .collect();
        controls.insert("key".to_string(), Value::Object(layout));
    }
    if let Some(rage) = saved.shift_remove("native_rage") {
        saved.insert("rage_one_input".to_string(), rage);
    }
    saved.insert("controls".to_string(), Value::Object(controls));
    saved.insert("controls_layout".to_string(), Value::from(CONTROLS_LAYOUT));
    saved
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn map(pairs: &[(&str, &str)]) -> IndexMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn defaults_of(kind: Kind) -> IndexMap<String, String> {
        table_rows(kind)
            .into_iter()
            .map(|r| {
                (
                    r.button.to_string(),
                    default_input(r, kind).unwrap_or("").to_string(),
                )
            })
            .collect()
    }

    fn object(value: Value) -> Map<String, Value> {
        value.as_object().unwrap().clone()
    }

    #[test]
    fn changed_buttons_in_table_order() {
        let controls = json!({"key": {"circle": " Left Shift ", "cross": "Q", "square": ""},
                              "pad": {"cross": "b"}});
        assert_eq!(
            control_map(&controls, Kind::Key),
            "cross=Q,circle=Left Shift"
        );
        assert_eq!(control_map(&controls, Kind::Pad), "cross=b");
    }

    #[test]
    fn rejects_separators_unknown_buttons_and_bad_shapes() {
        let controls = json!({"key": {"cross": "a,b", "circle": "x=y", "jump": "Q", "square": 5}});
        assert_eq!(control_map(&controls, Kind::Key), "");
        assert_eq!(control_map(&json!([]), Kind::Key), "");
        assert_eq!(control_map(&json!({}), Kind::Pad), "");
    }

    #[test]
    fn only_changes_from_the_defaults_are_kept() {
        let mut keys = defaults_of(Kind::Key);
        keys.insert("cross".into(), "q".into());
        keys.insert("circle".into(), "LEFT SHIFT".into());
        keys.insert("square".into(), "  ".into());
        assert_eq!(control_changes(&keys, Kind::Key), map(&[("cross", "q")]));
        let mut pads: IndexMap<String, String> = CONTROLS
            .iter()
            .map(|r| (r.button.to_string(), r.pad.unwrap_or("").to_string()))
            .collect();
        pads.insert("cross".into(), "b".into());
        pads.insert("l2".into(), "a".into());
        // L2/R2: triggers only
        assert_eq!(control_changes(&pads, Kind::Pad), map(&[("cross", "b")]));
    }

    #[test]
    fn touchpad_sides_are_separate_buttons() {
        let row = |b: &str| CONTROLS.iter().find(|r| r.button == b);
        assert!(row("touchpad").is_none());
        let left = row("touchpad_left").unwrap();
        assert_eq!((left.key, left.pad), ("Tab", None));
        let right = row("touchpad_right").unwrap();
        assert_eq!((right.key, right.pad), ("Backspace", Some("back")));
        assert_eq!(
            control_map(&json!({"pad": {"touchpad_right": "guide"}}), Kind::Pad),
            "touchpad_right=guide"
        );
        assert_eq!(
            control_map(&json!({"key": {"touchpad_right": "Delete"}}), Kind::Key),
            "touchpad_right=Delete"
        );
    }

    #[test]
    fn saved_touchpad_key_moves_to_the_left_side() {
        assert_eq!(
            control_map(&json!({"key": {"touchpad": "T"}}), Kind::Key),
            "touchpad_left=T"
        );
        assert_eq!(
            control_map(
                &json!({"key": {"touchpad": "T", "touchpad_left": "Y"}}),
                Kind::Key
            ),
            "touchpad_left=Y"
        );
        assert_eq!(
            control_values(&json!({"key": {"touchpad": "T"}}), Kind::Key),
            map(&[("touchpad_left", "T")])
        );
        assert!(control_values(&json!({"pad": {"touchpad": "misc1"}}), Kind::Pad).is_empty());
    }

    #[test]
    fn both_styles_name_every_default_gamepad_button() {
        assert_eq!(PAD_STYLES.len(), 3);
        for (_, labels) in PAD_STYLES {
            for pad in CONTROLS.iter().filter_map(|r| r.pad) {
                assert!(labels.iter().any(|(name, _)| *name == pad), "{pad}");
            }
            let mut shown: Vec<&str> = labels.iter().map(|(_, l)| *l).collect();
            shown.sort_unstable();
            shown.dedup();
            assert_eq!(shown.len(), labels.len());
        }
    }

    #[test]
    fn keyboard_defaults_put_the_attacks_on_the_mouse() {
        let text = keyboard_map(&json!({}), false);
        let mut pairs: IndexMap<String, String> = text
            .split(',')
            .map(|e| {
                let (k, v) = e.split_once('=').unwrap();
                (k.to_string(), v.to_string())
            })
            .collect();
        assert_eq!(pairs["square"], "Mouse Left");
        assert_eq!(pairs["triangle"], "Mouse Right");
        assert_eq!(pairs["r2"], "Mouse Forward");
        for row in &CONTROLS {
            let default = KEY_DEFAULTS
                .iter()
                .find(|(b, _)| *b == row.button)
                .map_or(row.key, |(_, k)| *k);
            if default.to_lowercase() != row.key.to_lowercase() {
                assert_eq!(pairs.shift_remove(row.button).unwrap(), default);
            }
        }
        assert_eq!(pairs, map(&[("grab", "C"), ("dodge", "Mouse Back")]));
        assert!(keyboard_map(&json!({}), true).contains("l3=None,r3=None,rage=Mouse Middle"));
        let controls = json!({"key": {"square": "Wheel Up", "cross": "Q", "triangle": "E"}});
        assert!(keyboard_map(&controls, true).contains("cross=Q,square=Wheel Up,triangle=E,"));
        let mut values = defaults_of(Kind::Key);
        assert!(control_changes(&values, Kind::Key).is_empty());
        values.insert("square".into(), "E".into());
        assert_eq!(control_changes(&values, Kind::Key), map(&[("square", "E")]));
    }

    #[test]
    fn rage_switch_moves_l3_and_r3_to_one_input() {
        let controls = json!({"key": {"l3": "Z", "r3": "X", "rage": "Mouse Middle"}});
        let apart = keyboard_map(&controls, false);
        assert!(apart.contains("r3=X"));
        assert!(!apart.contains("rage="));
        assert!(keyboard_map(&controls, true).contains("l3=None,r3=None,rage=Mouse Middle"));
    }

    #[test]
    fn saved_keyboard_binds_keep_their_layout() {
        let old = object(
            json!({"controls": {"key": {"cross": "Q"}, "pad": {"cross": "b"}}, "pad_style": "xbox"}),
        );
        let new = migrate_controls(old);
        let key = new["controls"]["key"].as_object().unwrap();
        assert_eq!(key["cross"], "Q");
        for row in &CONTROLS {
            if row.button != "cross" {
                // the game's keys, not the new defaults
                assert_eq!(key[row.button], row.key);
            }
        }
        assert_eq!(key["rage"], "Mouse Middle");
        assert_eq!(key["grab"], "C");
        assert_eq!(key["dodge"], "Mouse Back");
        assert_eq!(new["controls"]["pad"], json!({"cross": "b"}));
        assert_eq!(new["controls_layout"], json!(CONTROLS_LAYOUT));
        assert_eq!(migrate_controls(new.clone()), new); // once
        let game = keyboard_map(&new["controls"], true);
        assert!(game.contains("cross=Q"));
        assert!(!game.contains("square=")); // still E, the game's own key
        let apart = migrate_controls(object(
            json!({"controls": {"key": {"cross": "Q"}}, "native_rage": false}),
        ));
        assert_eq!(apart["controls"]["key"]["grab"], "None"); // C stays R3's
        assert_eq!(apart["rage_one_input"], json!(false));
        let taken = migrate_controls(object(
            json!({"controls": {"key": {"cross": "Mouse Back"}}}),
        ));
        assert_eq!(taken["controls"]["key"]["dodge"], "None");
    }

    #[test]
    fn native_translation_carries_over() {
        let old = object(json!({"controls": {"key": {"cross": "Q"},
                                             "native": {"circle": "Mouse Middle"}},
                                "native_input": true, "native_rage": true}));
        let new = migrate_controls(old);
        assert_eq!(new["controls"]["key"], json!({"circle": "Mouse Middle"}));
        assert!(new["controls"].get("native").is_none());
        assert!(!new.contains_key("native_input"));
        assert_eq!(new["rage_one_input"], json!(true));
        let unused = migrate_controls(object(
            json!({"controls": {"native": {"circle": "Mouse Middle"}}, "native_input": false}),
        ));
        assert_eq!(unused["controls"], json!({}));
    }

    #[test]
    fn new_players_get_the_new_defaults() {
        assert!(migrate_controls(Map::new()).is_empty());
        let fresh = migrate_controls(object(json!({"game_dir": "D:/games"})));
        assert_eq!(fresh["controls"], json!({}));
        assert!(keyboard_map(&fresh["controls"], true).contains("square=Mouse Left"));
    }

    #[test]
    fn binding_a_used_key_swaps_the_buttons() {
        let mut bindings = map(&[
            ("cross", "Space"),
            ("circle", "Left Shift"),
            ("square", "E"),
        ]);
        assert_eq!(
            bind_key(&mut bindings, "cross", "e"),
            Some("square".to_string())
        );
        assert_eq!(
            bindings,
            map(&[
                ("cross", "e"),
                ("circle", "Left Shift"),
                ("square", "Space")
            ])
        );
        assert_eq!(bind_key(&mut bindings, "circle", "Q"), None);
        assert_eq!(bindings["circle"], "Q");
    }

    #[test]
    fn sdl_names_and_mouse_names() {
        assert_eq!(sdl_key_name("q").as_deref(), Some("Q"));
        assert_eq!(sdl_key_name("7").as_deref(), Some("7"));
        assert_eq!(sdl_key_name("F12").as_deref(), Some("F12"));
        assert_eq!(sdl_key_name("Left Shift").as_deref(), Some("Left Shift"));
        assert_eq!(sdl_key_name("Keypad 5").as_deref(), Some("Keypad 5"));
        assert_eq!(sdl_key_name("=").as_deref(), Some("Equals"));
        for key in ["Insert", "Print", "F25", "F0", "eacute", ""] {
            assert_eq!(sdl_key_name(key), None, "{key}");
        }
        assert!(is_mouse_name(" wheel down "));
        assert!(!is_mouse_name("Left Shift"));
    }
}
