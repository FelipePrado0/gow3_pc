//! The Performance page: frame rate, shaders and GPU data.
//!
//! The Aspect ratio box is not saved: it follows the render resolution, and choosing another
//! aspect moves the resolution to that aspect's closest size.

use std::path::PathBuf;

use iced::widget::{button, column, pick_list, text};
use iced::Element;

use crate::core::gpu::{no_msaa_decision, MsaaDecision};
use crate::core::i18n::{tr, tr_fmt};
use crate::core::mods::{patch_files, selected};
use crate::core::options::{
    aspect_of, aspects, closest_resolution, resolution_label, resolutions_for, DRAW_PIPE,
    ENGINE_FPS_CHOICES, FPS_LIMITS, FRAMES_QUEUED, NO_MSAA, READBACKS, RESOLUTIONS, STREAM_MEMORY,
};
use crate::core::patches::resolution_names;
use crate::ui::app::{Message, State};
use crate::ui::pages::advanced::clear_cache_blocked;
use crate::ui::theme;
use crate::ui::widgets::{
    checkbox, checkbox_ini, choice_pairs, min_width, row, section, select_pairs, SELECT_MIN,
    SELECT_TEXT,
};

/// Suffix of the labels of the sizes a mod adds.
const MOD_SUFFIX: &str = " (mod)";

pub const NO_MSAA_HINT: &str = "The game's 2-sample depth buffer, read as a texture by its lighting. AMD's and Intel's Windows drivers read it wrong and the lighting shows black lines, so Automatic turns multisampling off on those cards (Radeon, Arc) and keeps it on elsewhere (NVIDIA). Off and On force the choice on any card.";

/// Reads the sizes the enabled mods add (the "Resolution Patch - <size>" entries of their patch
/// files) into `state.mod_resolutions`. Task 29 calls it again when a mod is toggled.
pub fn refresh_mod_resolutions(state: &mut State) {
    state.mod_resolutions = mod_sizes(state);
}

fn mod_sizes(state: &State) -> Vec<String> {
    if !state.settings.get_bool("mods_enabled") {
        return Vec::new();
    }
    let root = state.paths.mods_dir(&state.settings.get_str("mods_dir"));
    let game = state.settings.get_str("game_dir");
    let game = (!game.is_empty()).then(|| PathBuf::from(game));
    // The Mods page's order and switches, which Save (Task 32) has not written to mods.json yet.
    let names = if state.mods.order.is_empty() {
        selected(&root, Some(&state.paths.mods_json()), game.as_deref()).unwrap_or_default()
    } else {
        state
            .mods
            .order
            .iter()
            .filter(|name| state.mods.enabled.get(*name).copied().unwrap_or(false))
            .cloned()
            .collect()
    };
    let files: Vec<PathBuf> = names
        .iter()
        .flat_map(|name| patch_files(&root.join(name)))
        .collect();
    resolution_names(&files)
        .into_iter()
        .filter(|size| !RESOLUTIONS.contains(&size.as_str()))
        .collect()
}

/// A built-in render resolution's label in `lang`, as the Python's `_()` of it: the whole label
/// when the tables have it, else the English label with its "Native" word translated.
fn shown_label(lang: &str, name: &str, aspect: bool) -> String {
    let english = resolution_label(name, aspect);
    let whole = tr(lang, &english);
    if whole != english || name != "native" {
        return whole;
    }
    english.replacen("Native", &tr(lang, "Native"), 1)
}

/// The render resolutions to choose from: the built-in ones and those of the enabled mods.
pub fn resolution_options(state: &State) -> Vec<(String, String)> {
    let builtin = RESOLUTIONS
        .iter()
        .map(|name| (name.to_string(), shown_label(&state.lang, name, true)));
    let mods = state
        .mod_resolutions
        .iter()
        .map(|name| (name.clone(), resolution_label(name, true) + MOD_SUFFIX));
    builtin.chain(mods).collect()
}

fn resolution_values(state: &State) -> Vec<String> {
    resolution_options(state)
        .into_iter()
        .map(|(value, _)| value)
        .collect()
}

/// The aspect ratios the render resolutions have (built-in and mods).
pub fn aspect_options(state: &State) -> Vec<String> {
    aspects(&resolution_values(state))
}

/// The render resolutions of the chosen aspect, smallest first, without the ratio in the label.
pub fn aspect_resolution_options(state: &State) -> Vec<(String, String)> {
    let options = resolution_options(state);
    resolutions_for(&state.aspect, &resolution_values(state))
        .into_iter()
        .map(|name| {
            let is_mod = options
                .iter()
                .any(|(value, label)| *value == name && label.ends_with(MOD_SUFFIX));
            let label = if is_mod {
                resolution_label(&name, false) + MOD_SUFFIX
            } else {
                shown_label(&state.lang, &name, false)
            };
            (name, label)
        })
        .collect()
}

fn current_resolution(state: &State) -> String {
    state
        .ini
        .values
        .get("render_resolution")
        .cloned()
        .unwrap_or_default()
}

/// Chooses an aspect ratio and moves the render resolution to its closest size.
pub fn choose_aspect(state: &mut State, aspect: &str) {
    state.aspect = aspect.to_string();
    let current = current_resolution(state);
    if let Some(closest) = closest_resolution(aspect, &current, &resolution_values(state)) {
        if closest != current {
            state.ini.values.insert("render_resolution".into(), closest);
        }
    }
}

/// After the render resolution changed: the aspect box follows it.
pub fn resolution_chosen(state: &mut State) {
    if let Some(aspect) = aspect_of(&current_resolution(state)) {
        if aspect != state.aspect {
            state.aspect = aspect;
        }
    }
}

/// `NO_MSAA` with Automatic naming the chosen card and the game's decision for it.
pub fn no_msaa_label(state: &State, lang: &str) -> Vec<(String, String)> {
    let decision = no_msaa_decision(&state.gpus, &state.settings.get_str("gpu_id"));
    NO_MSAA
        .iter()
        .map(|choice| {
            let label = match &decision {
                MsaaDecision::Known { name, off } if choice.value.is_empty() => {
                    let english = if *off {
                        "Automatic ({}: off)"
                    } else {
                        "Automatic ({}: on)"
                    };
                    tr_fmt(lang, english, &[name])
                }
                _ => tr(lang, choice.en),
            };
            (choice.value.to_string(), label)
        })
        .collect()
}

/// Automatic, then the cards in the order the game numbers them.
pub fn gpu_pairs(state: &State) -> Vec<(String, String)> {
    std::iter::once((String::new(), "Automatic".to_string()))
        .chain(
            state
                .gpus
                .iter()
                .map(|gpu| (gpu.index.clone(), gpu.name.clone())),
        )
        .collect()
}

/// One drop-down of the page: the key it stores, its (value, label) choices and the current value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectRow {
    pub key: &'static str,
    pub pairs: Vec<(String, String)>,
    pub current: String,
}

/// The drop-downs of the page; `view` builds them from this list.
pub fn rows(state: &State, lang: &str) -> Vec<SelectRow> {
    let ini = |key: &str| state.ini.values.get(key).cloned().unwrap_or_default();
    let app = |key: &str| state.settings.get_str(key);
    let cards = gpu_pairs(state)
        .into_iter()
        .map(|(value, label)| {
            let label = if value.is_empty() {
                tr(lang, &label)
            } else {
                label
            };
            (value, label)
        })
        .collect();
    let row = |key, pairs, current| SelectRow {
        key,
        pairs,
        current,
    };
    vec![
        row(
            "fps_limit",
            choice_pairs(FPS_LIMITS, lang),
            ini("fps_limit"),
        ),
        row(
            "frames_queued",
            choice_pairs(FRAMES_QUEUED, lang),
            ini("frames_queued"),
        ),
        row(
            "engine_fps",
            choice_pairs(ENGINE_FPS_CHOICES, lang),
            ini("engine_fps"),
        ),
        row(
            "render_resolution",
            aspect_resolution_options(state),
            ini("render_resolution"),
        ),
        row("gpu_id", cards, app("gpu_id")),
        row(
            "stream_memory",
            choice_pairs(STREAM_MEMORY, lang),
            app("stream_memory"),
        ),
        row("draw_pipe", choice_pairs(DRAW_PIPE, lang), app("draw_pipe")),
        row("readbacks", choice_pairs(READBACKS, lang), app("readbacks")),
        row("no_msaa", no_msaa_label(state, lang), app("no_msaa")),
    ]
}

/// Delete shader cache: the Advanced page's Clear shader cache under the Python's other name,
/// held while the game runs.
fn delete_cache_button<'a>(state: &State, lang: &str) -> Element<'a, Message> {
    let delete = button(text(tr(lang, "Delete shader cache"))).style(theme::button_flat);
    if clear_cache_blocked(state) {
        delete.into()
    } else {
        delete.on_press(Message::ClearShaderCache).into()
    }
}

/// An ini flag is on when its value is `"1"`.
fn ini_on(state: &State, key: &str) -> bool {
    state.ini.values.get(key).is_some_and(|value| value == "1")
}

/// The drop-down of `key` as the Performance page draws it (its pairs and current value).
pub fn select_row<'a>(state: &State, lang: &str, key: &str) -> Element<'a, Message> {
    select_from(&rows(state, lang), key)
}

/// The drop-down of `key` among rows already built; an empty one when no row has that key.
fn select_from<'a>(rows: &[SelectRow], key: &str) -> Element<'a, Message> {
    match rows.iter().find(|row| row.key == key) {
        Some(row) => select_pairs(key, &row.pairs, &row.current),
        None => select_pairs(key, &[], ""),
    }
}

pub fn view(state: &State) -> Element<'_, Message> {
    let lang = state.lang.as_str();
    let hint = |english: &str| tr(lang, english);
    // A row that needs a new start of the game carries a small `restart` note.
    let restart = |english: &str| format!("{}  ·  {}", tr(lang, english), tr(lang, "restart"));

    let aspect = min_width(
        SELECT_MIN,
        pick_list(
            aspect_options(state),
            Some(state.aspect.clone()),
            Message::Aspect,
        )
        .text_size(SELECT_TEXT)
        .style(theme::pick_list_dark),
    );
    // The rows are built once for the whole page.
    let selects = rows(state, lang);
    let pick = |key: &str| select_from(&selects, key);

    column![
        section(state.rtl, &tr(lang, "Frame rate")),
        row(
            state.rtl,
            &tr(lang, "Frame rate limit"),
            pick("fps_limit"),
            Some(&hint("The engine frame rate (below) is the highest it can go.")),
        ),
        row(
            state.rtl,
            &tr(lang, "Frames queued"),
            pick("frames_queued"),
            Some(&hint(
                "1 is the lowest input latency; more can raise FPS when the graphics card is the limit."
            )),
        ),
        row(
            state.rtl,
            &tr(lang, "Engine frame rate"),
            pick("engine_fps"),
            Some(&hint(
                "The highest frame rate the game itself runs at (a game patch)."
            )),
        ),
        row(
            state.rtl,
            &tr(lang, "Aspect ratio"),
            aspect.into(),
            Some(&hint("Shows the render resolutions of this shape only.")),
        ),
        row(
            state.rtl,
            &restart("Render resolution"),
            pick("render_resolution"),
            Some(&hint(
                "The size the game draws at; higher is sharper and slower. A resolution patch replaces the texture fix and reserves the memory it needs."
            )),
        ),
        section(state.rtl, &tr(lang, "Shaders")),
        checkbox(
            state.rtl,
            "pipeline_binary_cache",
            &tr(lang, "Use pipeline binary cache (A/B test)"),
            state.settings.get_bool("pipeline_binary_cache"),
            Some(&hint(
                "Reuse saved Vulkan pipelines. Disable to compare FPS with normal compilation. Takes effect on the next launch; saved caches are preserved."
            )),
        ),
        checkbox(
            state.rtl,
            "background_warmup",
            &tr(lang, "Load shader cache in the background"),
            state.settings.get_bool("background_warmup"),
            Some(&hint(
                "The game opens at once and the saved shaders load while you play; a small note in the corner shows the progress. Switch it off to wait on the loading screen instead."
            )),
        ),
        checkbox_ini(
            state.rtl,
            "async_shaders",
            &tr(lang, "Compile new shaders in the background"),
            ini_on(state, "async_shaders"),
            Some(&hint(
                "Fewer stutters in new areas: the game keeps running while new shaders compile. An object or effect may appear a moment later. Switch it off if something stays missing."
            )),
        ),
        checkbox(
            state.rtl,
            "disk_shaders",
            &restart("Reuse compiled shaders from disk"),
            state.settings.get_bool("disk_shaders"),
            Some(&hint(
                "Shaders compiled in earlier sessions load from the shader cache instead of compiling again, so objects no longer appear a few frames late after starting or loading a save, and the cache is not rewritten every session (antivirus scans each written file). The first launch with it on rebuilds the cache once. Switch it off if an object, effect or color looks wrong only after a restart and is fine on the first run of a new cache."
            )),
        ),
        checkbox(
            state.rtl,
            "shader_prefetch",
            &restart("Read the shader cache into memory at start"),
            state.settings.get_bool("shader_prefetch"),
            Some(&hint(
                "A background thread reads the saved shaders into memory while the game starts; a note in the lower right corner shows the progress. Loading a shader then does not hold up a frame. Needs the option above. Switch it off on a slow hard drive or with little free RAM if the first minute stutters more or the game starts slower."
            )),
        ),
        checkbox(
            state.rtl,
            "shader_notes",
            &restart("Show shader notes in the corner"),
            state.settings.get_bool("shader_notes"),
            Some(&hint(
                "A note in the lower right corner shows the shader cache being read at start and the shaders being compiled, so a short stutter has a visible reason. Switch it off to keep the screen clean."
            )),
        ),
        row(
            state.rtl,
            &tr(lang, "Shader cache"),
            delete_cache_button(state, lang),
            Some(&hint(
                "Deletes the saved shaders and pipelines of this saves folder. Use it if graphics look wrong after an update or the cache grew too big; the next start compiles everything again."
            )),
        ),
        checkbox(
            state.rtl,
            "parallel_warmup",
            &tr(lang, "Fast shader cache loading (parallel)"),
            state.settings.get_bool("parallel_warmup"),
            Some(&hint(
                "Loading screen only: builds the cached pipelines on several CPU threads."
            )),
        ),
        section(state.rtl, &tr(lang, "GPU data")),
        row(
            state.rtl,
            &restart("Graphics card"),
            pick("gpu_id"),
            Some(&hint(
                "The GPU the game runs on. Choose your dedicated card if the game uses the integrated one."
            )),
        ),
        row(
            state.rtl,
            &restart("Stream buffer memory"),
            pick("stream_memory"),
            Some(&hint(
                "System RAM can improve FPS on NVIDIA laptops. Choose GPU memory to restore the previous behavior. Applies on the next game launch."
            )),
        ),
        checkbox_ini(
            state.rtl,
            "stale_readback",
            &restart("Game reads GPU data without waiting (big FPS gain)"),
            ini_on(state, "stale_readback"),
            Some(&hint(
                "About 40 to 66 FPS in a heavy scene. Needs deferred GPU readbacks. Switch it off if lighting or objects look wrong."
            )),
        ),
        checkbox_ini(
            state.rtl,
            "deferred_readback",
            &restart("Deferred GPU readbacks"),
            ini_on(state, "deferred_readback"),
            Some(&hint(
                "Fewer waits for the GPU. Switch it off if textures or shadows look wrong."
            )),
        ),
        checkbox(
            state.rtl,
            "partial_height_views",
            &restart("Fast screen effects (Rage of Sparta, weapons)"),
            state.settings.get_bool("partial_height_views"),
            Some(&hint(
                "Keeps powers and weapon effects at full FPS. Switch it off if a screen effect (blur, distortion, the color change of a power) looks stretched, shifted or wrong."
            )),
        ),
        row(
            state.rtl,
            &tr(lang, "Two-stage GPU pipeline"),
            pick("draw_pipe"),
            Some(&hint("20–30% faster; switch it off if the game is unstable.")),
        ),
        row(
            state.rtl,
            &tr(lang, "GPU readbacks"),
            pick("readbacks"),
            Some(&hint(
                "How exactly data the GPU writes is copied back for the game."
            )),
        ),
        row(
            state.rtl,
            &restart("Game multisampling"),
            pick("no_msaa"),
            Some(&hint(NO_MSAA_HINT)),
        ),
    ]
    .spacing(2)
    .into()
}
