//! The start-up graphics options of `spartanport.ini` (Python `scripts/graphics_options.py`): the
//! in-game menu and the launcher write them; the runner turns them into game patches and
//! environment variables at every start, so "Apply and restart" uses the new values. After a
//! restart the game confirms them (`restart_unconfirmed=0`) once it shows frames; when a launch
//! never did, the next start goes back to the last confirmed values.

use std::io;
use std::path::Path;

use indexmap::IndexMap;
use serde_json::{Map, Value};

use crate::core::fs::atomic_write;
use crate::core::ini::{read_ini, save_ini};
use crate::core::options::{ENGINE_FPS_CHOICES, RESOLUTIONS};
use crate::core::patches::ensure_ascii;

/// The Steam Deck's DMI names: LCD (Jupiter) and OLED (Galileo), both with a 1280 × 800 screen (#855).
const STEAM_DECK_PRODUCTS: [&str; 2] = ["Jupiter", "Galileo"];
pub const STEAM_DECK_RESOLUTION: &str = "1280x800";
const DMI: &str = "/sys/devices/virtual/dmi/id";
const RESOLUTION_PREFIX: &str = "Resolution Patch";
const FPS_PREFIX: &str = "Frame Rate Patch";
const TEXTURE_FIX: &str = "Bug Fix - Texture Corruption Fix";
const DEFAULT_ENGINE_FPS: &str = "120";
const REVERT_MESSAGE: &str =
    "Graphics: the last restart never reached the game; previous options restored";

/// True on a Steam Deck running Linux (SteamOS or another distribution), from its DMI names.
pub fn steam_deck(dmi: &Path) -> bool {
    let read =
        |name: &str| std::fs::read_to_string(dmi.join(name)).map(|text| text.trim().to_string());
    match (read("sys_vendor"), read("product_name")) {
        (Ok(vendor), Ok(product)) => {
            vendor == "Valve" && STEAM_DECK_PRODUCTS.contains(&product.as_str())
        }
        _ => false,
    }
}

/// The render resolution before the player chooses one: the Steam Deck's own screen, or native.
/// A choice saved in `spartanport.ini` always wins, so this only changes a first start.
pub fn default_render_resolution(dmi: &Path) -> &'static str {
    if steam_deck(dmi) {
        STEAM_DECK_RESOLUTION
    } else {
        "native"
    }
}

/// The `key=value` lines of an ini text (comments and lines without `=` left out).
fn file_values(text: &str) -> IndexMap<String, String> {
    text.lines()
        .filter(|line| line.contains('=') && !line.trim_start().starts_with('#'))
        .filter_map(|line| line.split_once('='))
        .map(|(key, value)| (key.trim().to_string(), value.trim().to_string()))
        .collect()
}

/// The keys the file itself holds, without the launcher's defaults (the Steam Deck's default
/// resolution is decided here, not by the table of defaults).
fn read_file_values(path: &Path) -> IndexMap<String, String> {
    file_values(&read_ini(path).0.lines.join("\n"))
}

/// Rewrites `updates` in place and appends missing keys, keeping every other line.
pub fn write_keys(path: &Path, updates: &IndexMap<String, String>) -> io::Result<()> {
    save_ini(path, updates, &read_ini(path).0.lines)
}

/// The render resolutions: the built-in ones and those the enabled mods' patch files add.
pub fn resolutions(extra: &[String]) -> Vec<String> {
    let mut all: Vec<String> = RESOLUTIONS.iter().map(|r| (*r).to_string()).collect();
    for name in extra {
        if !all.contains(name) {
            all.push(name.clone());
        }
    }
    all
}

/// The start-up options, normalized (every value is the text the ini holds).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    pub render_resolution: String,
    pub engine_fps: String,
    pub deferred_readback: String,
    pub stale_readback: String,
    pub async_shaders: String,
}

impl Options {
    /// The options as ini keys, in the Python's order.
    fn keys(&self) -> IndexMap<String, String> {
        [
            ("render_resolution", &self.render_resolution),
            ("engine_fps", &self.engine_fps),
            ("deferred_readback", &self.deferred_readback),
            ("stale_readback", &self.stale_readback),
            ("async_shaders", &self.async_shaders),
        ]
        .into_iter()
        .map(|(key, value)| (key.to_string(), value.clone()))
        .collect()
    }
}

/// The start-up options of `ini`, normalized; unknown values fall back to the defaults. `extra`: the
/// resolutions of the enabled mods (a mod that is off leaves its resolution out: native).
pub fn startup_options(ini: &IndexMap<String, String>, extra: &[String]) -> Options {
    let get = |key: &str, default: &str| ini.get(key).map_or(default, String::as_str).to_string();
    let flag = |key: &str| if get(key, "1") == "0" { "0" } else { "1" }.to_string();
    let mut render_resolution = get(
        "render_resolution",
        default_render_resolution(Path::new(DMI)),
    );
    if !resolutions(extra).contains(&render_resolution) {
        render_resolution = "native".to_string();
    }
    let mut engine_fps = get("engine_fps", DEFAULT_ENGINE_FPS);
    if !ENGINE_FPS_CHOICES.iter().any(|c| c.value == engine_fps) {
        engine_fps = DEFAULT_ENGINE_FPS.to_string();
    }
    Options {
        render_resolution,
        engine_fps,
        deferred_readback: flag("deferred_readback"),
        stale_readback: flag("stale_readback"),
        async_shaders: flag("async_shaders"),
    }
}

/// What `prepare` decided for this launch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prepared {
    pub options: Options,
    /// The previous launch after a restart never confirmed its options: the last confirmed ones came back.
    pub reverted: bool,
    /// The line the Log shows when `reverted`.
    pub message: Option<String>,
}

/// Python truthiness of a JSON value.
fn truthy(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) | Some(Value::Bool(false)) => false,
        Some(Value::Number(n)) => n.as_f64() != Some(0.0),
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Array(a)) => !a.is_empty(),
        Some(Value::Object(o)) => !o.is_empty(),
        Some(Value::Bool(true)) => true,
    }
}

/// The last confirmed options as the ini-like map `startup_options` reads (a value that is not
/// text is none of the choices).
fn state_values(last_good: Option<&Value>) -> IndexMap<String, String> {
    match last_good {
        Some(Value::Object(map)) => map
            .iter()
            .map(|(key, value)| (key.clone(), value.as_str().unwrap_or("").to_string()))
            .collect(),
        _ => IndexMap::new(),
    }
}

/// `json.dumps(state, indent=2)` of the Python: two-space indent, `ensure_ascii`.
fn python_json(state: &Map<String, Value>) -> String {
    ensure_ascii(&serde_json::to_string_pretty(state).unwrap_or_else(|_| "{}".to_string()))
}

/// The start-up options of this launch, restoring the last confirmed ones when the previous launch
/// after a restart never confirmed its own. Keeps `state_path` (`graphics_state.json`) up to date.
/// A file that cannot be written is left as it was: the options of this launch are still returned.
pub fn prepare(config: &Path, state_path: &Path, extra: &[String]) -> Prepared {
    let ini = read_file_values(config);
    let mut state = match std::fs::read(state_path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
    {
        Some(Value::Object(map)) => map,
        _ => Map::new(),
    };
    let mut options = startup_options(&ini, extra);
    let mut reverted = false;
    if ini.get("restart_unconfirmed").map(String::as_str) == Some("1") {
        if truthy(state.get("attempt")) && truthy(state.get("last_good")) {
            options = startup_options(&state_values(state.get("last_good")), extra);
            let mut keys = options.keys();
            keys.insert("restart_unconfirmed".to_string(), "0".to_string());
            let _ = write_keys(config, &keys);
            state.insert("attempt".to_string(), Value::Bool(false));
            reverted = true;
        } else {
            state.insert("attempt".to_string(), Value::Bool(true));
        }
    } else {
        state = Map::new();
        let last_good: Map<String, Value> = options
            .keys()
            .into_iter()
            .map(|(key, value)| (key, Value::String(value)))
            .collect();
        state.insert("last_good".to_string(), Value::Object(last_good));
        state.insert("attempt".to_string(), Value::Bool(false));
    }
    let _ = atomic_write(state_path, python_json(&state).as_bytes());
    Prepared {
        options,
        reverted,
        message: reverted.then(|| REVERT_MESSAGE.to_string()),
    }
}

/// The launcher's patch list with the resolution and frame rate patches of `options`.
pub fn patch_names(
    names: &[String],
    options: &Options,
    texture_fix_available: bool,
) -> Vec<String> {
    let mut names: Vec<String> = names
        .iter()
        .filter(|n| !n.starts_with(RESOLUTION_PREFIX) && !n.starts_with(FPS_PREFIX))
        .cloned()
        .collect();
    let resolution = &options.render_resolution;
    if resolution != "native" {
        names.retain(|n| n != TEXTURE_FIX);
        names.push(format!("{RESOLUTION_PREFIX} - {resolution}"));
    } else if texture_fix_available && !names.iter().any(|n| n == TEXTURE_FIX) {
        names.insert(0, TEXTURE_FIX.to_string());
    }
    // The engine rate is chosen live (spartanport_settings.cpp SyncEngineFps): the highest patch gives
    // the swing energy fix and a vblank every lower rate fits in.
    names.push(format!("{FPS_PREFIX} - {} FPS", highest_engine_fps()));
    names
}

fn highest_engine_fps() -> u32 {
    ENGINE_FPS_CHOICES
        .iter()
        .filter_map(|c| c.value.parse().ok())
        .max()
        .unwrap_or(60)
}

/// The rate of a "Frame Rate Patch - <rate> FPS" name (`None` for any other name).
fn frame_rate(name: &str) -> Option<u32> {
    name.strip_prefix(FPS_PREFIX)?
        .strip_prefix(" - ")?
        .split_whitespace()
        .next()?
        .parse()
        .ok()
}

/// `names` without the patches this XML lacks. A missing frame rate patch is replaced by the highest
/// one it has below the requested rate (the 240 FPS patch of the first XML has no counterpart in the
/// second).
pub fn restrict_to_available(names: &[String], available: &[String]) -> Vec<String> {
    restrict_with_log(names, available, &mut |_| {})
}

/// `restrict_to_available` that also hands over the Log lines the Python prints.
pub fn restrict_with_log(
    names: &[String],
    available: &[String],
    log: &mut dyn FnMut(String),
) -> Vec<String> {
    let mut kept: Vec<String> = names
        .iter()
        .filter(|n| available.contains(n))
        .cloned()
        .collect();
    for name in names.iter().filter(|n| !available.contains(n)) {
        log(format!(
            "Patches: {name} is not available for this game build: skipped"
        ));
        if let Some(wanted) = frame_rate(name) {
            let lower = available
                .iter()
                .filter_map(|n| frame_rate(n))
                .filter(|rate| *rate <= wanted)
                .max();
            if let Some(rate) = lower {
                let replacement = format!("{FPS_PREFIX} - {rate} FPS");
                log(format!("Patches: using {replacement} instead"));
                kept.push(replacement);
            }
        }
    }
    kept
}

/// The engine rate the applied patch list gives: 60 without a frame rate patch.
pub fn applied_engine_fps(names: &[String]) -> u32 {
    names
        .iter()
        .filter_map(|n| frame_rate(n))
        .max()
        .unwrap_or(60)
}

/// The environment variables the game reads the options from.
pub fn environment(options: &Options) -> IndexMap<String, String> {
    [
        ("SPARTAN_DEFERRED_READBACK", &options.deferred_readback),
        ("SPARTAN_STALE_READBACK", &options.stale_readback),
        ("SPARTAN_ASYNC_SHADERS", &options.async_shaders),
    ]
    .into_iter()
    .map(|(key, value)| (key.to_string(), value.clone()))
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::options::{resolution_label, SIZES};
    use crate::core::patches::resolution_names;
    use std::fs;
    use std::path::PathBuf;

    struct Folder {
        _dir: tempfile::TempDir,
        ini: PathBuf,
        state: PathBuf,
    }

    fn folder() -> Folder {
        let dir = tempfile::tempdir().unwrap();
        let ini = dir.path().join("spartanport.ini");
        let state = dir.path().join("state.json");
        Folder {
            _dir: dir,
            ini,
            state,
        }
    }

    fn map(pairs: &[(&str, &str)]) -> IndexMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|n| n.to_string()).collect()
    }

    fn startup(pairs: &[(&str, &str)]) -> Options {
        startup_options(&map(pairs), &[])
    }

    fn repo(path: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join(path)
    }

    fn ini_values(path: &Path) -> IndexMap<String, String> {
        file_values(&fs::read_to_string(path).unwrap())
    }

    #[test]
    fn defaults_and_invalid_values() {
        let f = folder();
        fs::write(
            &f.ini,
            "render_resolution=invalid\nengine_fps=90\nstale_readback=yes\n",
        )
        .unwrap();
        let prepared = prepare(&f.ini, &f.state, &[]);
        assert!(!prepared.reverted);
        assert_eq!(
            prepared.options,
            Options {
                render_resolution: "native".into(),
                engine_fps: "120".into(),
                deferred_readback: "1".into(),
                stale_readback: "1".into(),
                async_shaders: "1".into(),
            }
        );
    }

    #[test]
    fn corrupt_state_file_preserves_current_settings() {
        let f = folder();
        fs::write(&f.ini, "render_resolution=1440p\nengine_fps=240\n").unwrap();
        fs::write(&f.state, "{invalid json").unwrap();
        let prepared = prepare(&f.ini, &f.state, &[]);
        assert!(!prepared.reverted);
        assert_eq!(prepared.options.render_resolution, "1440p");
        assert_eq!(prepared.options.engine_fps, "240");
    }

    #[test]
    fn the_8k_selection_replaces_previous_resolution() {
        let options = startup(&[("render_resolution", "8K")]);
        assert_eq!(options.render_resolution, "8K");
        assert_eq!(
            SIZES.iter().find(|(n, _)| *n == "8K").map(|(_, s)| *s),
            Some((7680, 4320))
        );
        assert_eq!(
            resolution_label("8K", true),
            "8K · 7680 × 4320 · 16:9 (beta)"
        );
        let found = patch_names(
            &names(&["Resolution Patch - 4K", TEXTURE_FIX]),
            &options,
            true,
        );
        let resolutions: Vec<&String> = found
            .iter()
            .filter(|n| n.starts_with(RESOLUTION_PREFIX))
            .collect();
        assert_eq!(resolutions, ["Resolution Patch - 8K"]);
        assert!(!found.iter().any(|n| n == TEXTURE_FIX));
    }

    #[test]
    fn write_keys_keeps_other_lines() {
        let f = folder();
        fs::write(&f.ini, "# comment\nshow_fps=1\nengine_fps=60\n").unwrap();
        write_keys(&f.ini, &map(&[("engine_fps", "240"), ("vsync", "0")])).unwrap();
        assert_eq!(
            fs::read_to_string(&f.ini).unwrap(),
            "# comment\nshow_fps=1\nengine_fps=240\nvsync=0\n"
        );
    }

    #[test]
    fn unconfirmed_restart_is_tried_once_then_reverted() {
        let f = folder();
        fs::write(&f.ini, "render_resolution=1440p\nengine_fps=120\n").unwrap();
        let good = prepare(&f.ini, &f.state, &[]).options;
        write_keys(
            &f.ini,
            &map(&[
                ("render_resolution", "4K"),
                ("engine_fps", "240"),
                ("restart_unconfirmed", "1"),
            ]),
        )
        .unwrap();

        let tried = prepare(&f.ini, &f.state, &[]);
        assert!(!tried.reverted);
        assert_eq!(
            (
                tried.options.render_resolution.as_str(),
                tried.options.engine_fps.as_str()
            ),
            ("4K", "240")
        );

        let back = prepare(&f.ini, &f.state, &[]);
        assert!(back.reverted);
        assert_eq!(back.options, good);
        assert_eq!(
            back.message.as_deref(),
            Some("Graphics: the last restart never reached the game; previous options restored")
        );

        let ini = ini_values(&f.ini);
        assert_eq!(ini["render_resolution"], "1440p");
        assert_eq!(ini["restart_unconfirmed"], "0");
    }

    #[test]
    fn confirmed_restart_becomes_the_last_good() {
        let f = folder();
        prepare(&f.ini, &f.state, &[]);
        write_keys(
            &f.ini,
            &map(&[("engine_fps", "240"), ("restart_unconfirmed", "1")]),
        )
        .unwrap();
        prepare(&f.ini, &f.state, &[]);
        write_keys(&f.ini, &map(&[("restart_unconfirmed", "0")])).unwrap();
        prepare(&f.ini, &f.state, &[]);

        let state: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&f.state).unwrap()).unwrap();
        assert_eq!(state["last_good"]["engine_fps"], "240");
    }

    #[test]
    fn patch_names_follow_the_options() {
        let launcher = names(&[
            "Bug Fix - Texture Corruption Fix",
            "Frame Rate Patch - 120 FPS",
            "Skip Any Video With X Button",
        ]);
        let options = startup(&[("render_resolution", "1440p"), ("engine_fps", "240")]);
        assert_eq!(
            patch_names(&launcher, &options, true),
            [
                "Skip Any Video With X Button",
                "Resolution Patch - 1440p",
                "Frame Rate Patch - 240 FPS"
            ]
        );

        // The engine rate changes live (in-game menu, R3 + L1 + R1): every launch takes the
        // highest frame rate patch, even for 60 or 100 FPS.
        let native = startup(&[("engine_fps", "60")]);
        assert_eq!(
            patch_names(
                &names(&["Resolution Patch - 4K", "Skip Any Video With X Button"]),
                &native,
                true
            ),
            [
                "Bug Fix - Texture Corruption Fix",
                "Skip Any Video With X Button",
                "Frame Rate Patch - 240 FPS"
            ]
        );
        assert_eq!(startup(&[("engine_fps", "100")]).engine_fps, "100");
    }

    #[test]
    fn every_built_in_resolution_has_a_patch() {
        let mut built_in: Vec<String> = RESOLUTIONS
            .iter()
            .filter(|r| **r != "native")
            .map(|r| r.to_string())
            .collect();
        built_in.sort();
        let mut found = resolution_names(&[repo("patches/CUSA01623.xml")]);
        found.sort();
        assert_eq!(built_in, found);
    }

    #[test]
    fn the_900p_is_the_built_in_size_between_720p_and_1440p() {
        // #238: 900p (1600x900) sits between 720p and 1440p, with its own resolution patch.
        assert_eq!(RESOLUTIONS.iter().position(|r| *r == "900p"), Some(3));
        assert_eq!(
            SIZES.iter().find(|(n, _)| *n == "900p").map(|(_, s)| *s),
            Some((1600, 900))
        );
        let xml = fs::read_to_string(repo("patches/CUSA01623.xml")).unwrap();
        let doc = roxmltree::Document::parse(&xml).unwrap();
        assert!(doc
            .descendants()
            .filter(|n| n.has_tag_name("Metadata"))
            .any(|n| n.attribute("Name") == Some("Resolution Patch - 900p")));
    }

    #[test]
    fn resolutions_of_enabled_mods() {
        // A mod's "Resolution Patch - <size>" is a render resolution only while its mod is on.
        assert_eq!(resolutions(&[]), RESOLUTIONS);
        let mut with_mod: Vec<String> = RESOLUTIONS.iter().map(|r| r.to_string()).collect();
        with_mod.push("3840x1080".into());
        assert_eq!(resolutions(&names(&["3840x1080", "4K"])), with_mod);
        let ini = map(&[("render_resolution", "3840x1080"), ("engine_fps", "60")]);
        assert_eq!(startup_options(&ini, &[]).render_resolution, "native");
        let options = startup_options(&ini, &names(&["3840x1080"]));
        assert_eq!(options.render_resolution, "3840x1080");
        assert_eq!(
            patch_names(
                &names(&["Bug Fix - Texture Corruption Fix"]),
                &options,
                true
            ),
            ["Resolution Patch - 3840x1080", "Frame Rate Patch - 240 FPS"]
        );
    }

    #[test]
    fn a_confirmed_mod_resolution_is_restored_while_its_mod_is_on() {
        let f = folder();
        let extra = names(&["3840x1080"]);
        write_keys(
            &f.ini,
            &map(&[("render_resolution", "3840x1080"), ("engine_fps", "60")]),
        )
        .unwrap();
        let tried = prepare(&f.ini, &f.state, &extra);
        assert_eq!(tried.options.render_resolution, "3840x1080");
        write_keys(
            &f.ini,
            &map(&[("render_resolution", "4K"), ("restart_unconfirmed", "1")]),
        )
        .unwrap();
        // The attempt.
        assert_eq!(
            prepare(&f.ini, &f.state, &extra).options.render_resolution,
            "4K"
        );
        // The restart never confirmed.
        let back = prepare(&f.ini, &f.state, &extra);
        assert!(back.reverted);
        assert_eq!(back.options.render_resolution, "3840x1080");
    }

    #[test]
    fn engine_rate_100_for_100hz_displays() {
        assert!(ENGINE_FPS_CHOICES.iter().any(|c| c.value == "100"));
        let options = startup(&[("render_resolution", "native"), ("engine_fps", "100")]);
        assert_eq!(options.engine_fps, "100");
        // 100 is written live over the 240 FPS patch (spartanport_settings.cpp SyncEngineFps).
        assert_eq!(
            patch_names(
                &names(&["Bug Fix - Texture Corruption Fix"]),
                &options,
                true
            ),
            [
                "Bug Fix - Texture Corruption Fix",
                "Frame Rate Patch - 240 FPS"
            ]
        );
        // Not a choice: the default.
        assert_eq!(startup(&[("engine_fps", "90")]).engine_fps, "120");
    }

    #[test]
    fn engine_rate_180_for_high_refresh_displays() {
        let rates: Vec<&str> = ENGINE_FPS_CHOICES.iter().map(|c| c.value).collect();
        assert_eq!(rates, ["60", "100", "120", "144", "180", "240"]);
        let options = startup(&[("render_resolution", "native"), ("engine_fps", "180")]);
        assert_eq!(options.engine_fps, "180");
        assert!(patch_names(&[], &options, true)
            .iter()
            .any(|n| n == "Frame Rate Patch - 240 FPS"));
    }

    #[test]
    fn engine_rate_144_for_144hz_displays() {
        assert!(ENGINE_FPS_CHOICES.iter().any(|c| c.value == "144"));
        let options = startup(&[("render_resolution", "native"), ("engine_fps", "144")]);
        assert_eq!(options.engine_fps, "144");
        assert_eq!(
            patch_names(
                &names(&["Bug Fix - Texture Corruption Fix"]),
                &options,
                true
            ),
            [
                "Bug Fix - Texture Corruption Fix",
                "Frame Rate Patch - 240 FPS"
            ]
        );
    }

    #[test]
    fn applied_engine_fps_reads_the_highest_frame_rate_patch() {
        assert_eq!(
            applied_engine_fps(&names(&[
                "Skip Any Video With X Button",
                "Frame Rate Patch - 240 FPS"
            ])),
            240
        );
        assert_eq!(
            applied_engine_fps(&names(&["Frame Rate Patch - 120 FPS"])),
            120
        );
        assert_eq!(
            applied_engine_fps(&names(&["Frame Rate Patch - 100 FPS"])),
            100
        );
        assert_eq!(
            applied_engine_fps(&names(&["Frame Rate Patch - 144 FPS"])),
            144
        );
        assert_eq!(
            applied_engine_fps(&names(&["Bug Fix - Texture Corruption Fix"])),
            60
        );
        assert_eq!(applied_engine_fps(&[]), 60);
    }

    #[test]
    fn environment_carries_the_readback_switches() {
        let env = environment(&startup(&[("stale_readback", "0")]));
        assert_eq!(
            env,
            map(&[
                ("SPARTAN_DEFERRED_READBACK", "1"),
                ("SPARTAN_STALE_READBACK", "0"),
                ("SPARTAN_ASYNC_SHADERS", "1"),
            ])
        );
        assert_eq!(
            env.keys().map(String::as_str).collect::<Vec<_>>(),
            [
                "SPARTAN_DEFERRED_READBACK",
                "SPARTAN_STALE_READBACK",
                "SPARTAN_ASYNC_SHADERS"
            ]
        );
    }

    #[test]
    fn the_game_knows_the_launcher_sizes() {
        // gpu/shim/spartanport_graphics.h ResolutionSizes, which sizes the window, matches SIZES.
        let header = fs::read_to_string(repo("gpu/shim/spartanport_graphics.h")).unwrap();
        let table = header
            .split("ResolutionSizes = {{")
            .nth(1)
            .and_then(|rest| rest.split("}}};").next())
            .unwrap_or_default();
        let mut found: Vec<(String, (u32, u32))> = Vec::new();
        for entry in table.split("{\"").skip(1) {
            let (name, rest) = entry.split_once('"').unwrap_or_default();
            let numbers: Vec<u32> = rest
                .split(|c: char| !c.is_ascii_digit())
                .filter_map(|s| s.parse().ok())
                .collect();
            if let [w, h] = numbers[..] {
                found.push((name.to_string(), (w, h)));
            }
        }
        let mut expected: Vec<(String, (u32, u32))> =
            SIZES.iter().map(|(n, s)| (n.to_string(), *s)).collect();
        found.sort();
        expected.sort();
        assert_eq!(found, expected);
    }

    fn dmi(vendor: &str, product: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("sys_vendor"), format!("{vendor}\n")).unwrap();
        fs::write(dir.path().join("product_name"), format!("{product}\n")).unwrap();
        dir
    }

    #[test]
    fn lcd_and_oled_models_default_to_1280x800() {
        for product in ["Jupiter", "Galileo"] {
            let dir = dmi("Valve", product);
            assert!(steam_deck(dir.path()), "{product}");
            assert_eq!(default_render_resolution(dir.path()), "1280x800");
        }
    }

    #[test]
    fn other_machines_stay_native() {
        let dir = dmi("LENOVO", "Jupiter");
        assert_eq!(default_render_resolution(dir.path()), "native");
        let dir = dmi("Valve", "Other");
        assert_eq!(default_render_resolution(dir.path()), "native");
    }

    #[test]
    fn no_dmi_names_stays_native() {
        // Windows, containers, macOS.
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("missing");
        assert!(!steam_deck(&missing));
        assert_eq!(default_render_resolution(&missing), "native");
    }

    #[test]
    fn the_default_resolution_is_a_built_in_one() {
        assert!(RESOLUTIONS.contains(&STEAM_DECK_RESOLUTION));
        assert!(crate::core::options::TESTED_RESOLUTIONS.contains(&STEAM_DECK_RESOLUTION));
        assert_eq!(
            crate::core::options::aspect_of(STEAM_DECK_RESOLUTION).as_deref(),
            Some("16:10")
        );
    }

    #[test]
    fn unavailable_frame_rate_patch_falls_back_to_the_highest_lower_one() {
        let available = names(&[
            "Bug Fix - Texture Corruption Fix",
            "Frame Rate Patch - 120 FPS",
        ]);
        assert_eq!(
            restrict_to_available(
                &names(&[
                    "Bug Fix - Texture Corruption Fix",
                    "Frame Rate Patch - 240 FPS"
                ]),
                &available
            ),
            [
                "Bug Fix - Texture Corruption Fix",
                "Frame Rate Patch - 120 FPS"
            ]
        );
        assert!(restrict_to_available(&names(&["Resolution Patch - 8K"]), &available).is_empty());
    }

    #[test]
    fn restrict_to_available_says_what_it_skipped() {
        let available = names(&["Frame Rate Patch - 120 FPS"]);
        let mut lines = Vec::new();
        let kept = restrict_with_log(
            &names(&["Frame Rate Patch - 240 FPS"]),
            &available,
            &mut |line| lines.push(line),
        );
        assert_eq!(kept, ["Frame Rate Patch - 120 FPS"]);
        assert_eq!(
            lines,
            [
                "Patches: Frame Rate Patch - 240 FPS is not available for this game build: skipped",
                "Patches: using Frame Rate Patch - 120 FPS instead"
            ]
        );
    }

    #[test]
    fn the_state_file_is_written_as_python_json() {
        let f = folder();
        fs::write(&f.ini, "render_resolution=1440p\n").unwrap();
        prepare(&f.ini, &f.state, &[]);
        // indent 2, no trailing newline; unknown keys of a state that is kept stay in place.
        fs::write(&f.state, "{\"x\": \"é\", \"attempt\": false}").unwrap();
        write_keys(&f.ini, &map(&[("restart_unconfirmed", "1")])).unwrap();
        prepare(&f.ini, &f.state, &[]);
        assert_eq!(
            fs::read_to_string(&f.state).unwrap(),
            "{\n  \"x\": \"\\u00e9\",\n  \"attempt\": true\n}"
        );
    }

    #[test]
    fn a_del_in_the_state_file_is_escaped_as_python_does() {
        // json.dumps escapes every character outside ' '..'~', DEL (0x7f) too.
        let f = folder();
        fs::write(&f.ini, "render_resolution=1440p\n").unwrap();
        prepare(&f.ini, &f.state, &[]);
        fs::write(&f.state, "{\"x\": \"A\\u007fB\", \"attempt\": false}").unwrap();
        write_keys(&f.ini, &map(&[("restart_unconfirmed", "1")])).unwrap();
        prepare(&f.ini, &f.state, &[]);
        assert_eq!(
            fs::read_to_string(&f.state).unwrap(),
            "{\n  \"x\": \"A\\u007fB\",\n  \"attempt\": true\n}"
        );
    }
}
