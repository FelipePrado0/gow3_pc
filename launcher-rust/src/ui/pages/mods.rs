//! The Mods page: the switch, the load order with a details card for the selected mod, the
//! conflicts, presets, third-party patches and the folders.
//!
//! The folders are read on a thread (`load`, `refresh_conflicts`); `view` only draws what the
//! `Loaded` and `Conflicts` messages stored. Edits change the state only: `mods.json` is written
//! by the Save flow (Task 32). Presets are written at once, as the Python launcher does.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use iced::widget::image::Handle;
use iced::widget::{
    button, checkbox as check_box, column, container, image, row, space, text, text_input,
};
use iced::{Border, Center, Color, ContentFit, Element, Fill, Task};
use indexmap::IndexMap;
use serde_json::{json, Map, Value};

use crate::core::desktop::{open_path, open_url};
use crate::core::fs::{atomic_write, load_json, read_json};
use crate::core::game::{game_profile_of, meta_folder};
use crate::core::i18n::{tr, tr_fmt};
use crate::core::mods::{
    apply_preset, conflicts, discover, mod_files, mod_info, patch_files, ModInfo,
};
use crate::core::patches::{builtin_xml, external_enabled, external_patches, PatchMeta};
use crate::core::paths::Paths;
use crate::core::settings::Settings;
use crate::ui::app::{Message, State};
use crate::ui::pages::performance;
use crate::ui::tasks::blocking;
use crate::ui::theme::{self, palette};
use crate::ui::widgets::{checkbox_beside, folder, section, toggle_row, Modal, ModalKind};
use crate::APP_NAME;

/// Width of the details card picture, in logical pixels; it is at most 4:3.
const PICTURE_WIDTH: f32 = 268.0;
/// Conflict lines shown under the summary.
const CONFLICT_LINES: usize = 12;
/// Conflict lines shown in the details card.
const CARD_CONFLICTS: usize = 4;

/// Mod order, switches, conflicts and presets.
#[derive(Debug, Clone, Default)]
pub struct ModsView {
    /// The mod folders in load order; the last one wins.
    pub order: Vec<String>,
    pub enabled: IndexMap<String, bool>,
    pub infos: IndexMap<String, ModInfo>,
    pub selected: Option<String>,
    /// `(game path, mods that change it in load order)` for the enabled mods.
    pub conflicts: Vec<(String, Vec<String>)>,
    pub conflicts_open: bool,
    pub presets: Vec<String>,
    /// `mods_presets.json` is there but could not be read: no preset is listed, and saving or
    /// deleting one does not replace the file (spec §6).
    pub presets_unreadable: bool,
    pub preset_name: String,
    pub external: Vec<(String, PathBuf, PatchMeta)>,
    pub external_enabled: BTreeSet<String>,
    pub picture: Option<Handle>,
    /// Per mod: how many files it changes and whether it has game patch files.
    pub facts: BTreeMap<String, (usize, bool)>,
    /// The folders the read was made for (`FOLDERS`, in order).
    pub folders: Vec<String>,
}

/// The settings a read of the page depends on.
const FOLDERS: [&str; 4] = ["mods_dir", "game_dir", "update_dir", "patches_dir"];

/// The values of `FOLDERS` in `settings`.
fn folders(settings: &Settings) -> Vec<String> {
    FOLDERS.iter().map(|key| settings.get_str(key)).collect()
}

/// Mods page events.
#[derive(Debug, Clone)]
pub enum ModsMessage {
    Refresh,
    Toggle(String),
    Select(String),
    /// Moves the mod at this position up (-1) or down (1).
    Move(usize, i32),
    ToggleConflicts,
    PresetName(String),
    LoadPreset,
    SavePreset,
    DeletePreset,
    ToggleExternal(String),
    OpenFolder,
    OpenModFolder,
    Website,
    /// A read of the folders finished.
    Loaded(Box<ModsView>),
    /// The conflicts of the enabled mods were worked out again.
    /// The enabled mods in load order the result was worked out for, and the result.
    Conflicts(Vec<String>, Vec<(String, Vec<String>)>),
}

fn game_path(settings: &Settings) -> Option<PathBuf> {
    Some(settings.get_str("game_dir"))
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
}

fn names_in(profile: &Value, key: &str) -> Vec<String> {
    profile
        .get(key)
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(|name| name.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn active(view: &ModsView) -> Vec<String> {
    view.order
        .iter()
        .filter(|name| view.enabled.get(*name).copied().unwrap_or(false))
        .cloned()
        .collect()
}

/// What a broken `mods_presets.json` shows, under Presets and when a preset is saved or deleted.
const PRESETS_UNREADABLE: &str = "mods_presets.json could not be read, so no preset is listed and the file is left as it is. Fix or delete it to save presets again.";

/// The saved presets (none when the file is missing); `None` when `mods_presets.json` is there
/// but is not a readable JSON object.
fn read_presets(paths: &Paths) -> Option<Map<String, Value>> {
    match read_json::<Value>(&paths.mod_presets()) {
        Ok(None) => Some(Map::new()),
        Ok(Some(Value::Object(presets))) => Some(presets),
        Ok(Some(_)) | Err(_) => None,
    }
}

/// The preset names, sorted, and whether the file could not be read.
fn preset_names(paths: &Paths) -> (Vec<String>, bool) {
    let Some(presets) = read_presets(paths) else {
        return (Vec::new(), true);
    };
    let mut names: Vec<String> = presets.keys().cloned().collect();
    names.sort();
    (names, false)
}

/// Reads the mods folder, the saved order and switches, the conflicts, the presets and the
/// third-party patches; runs on a thread.
pub fn load(paths: &Paths, settings: &Settings) -> ModsView {
    let root = paths.mods_dir(&settings.get_str("mods_dir"));
    let game = game_path(settings);
    let profile: Value = load_json(&paths.mods_json(), Value::Null);
    let available = discover(&root, game.as_deref());
    let mut order: Vec<String> = names_in(&profile, "order")
        .into_iter()
        .filter(|name| available.contains(name))
        .collect();
    for name in &available {
        if !order.contains(name) {
            order.push(name.clone());
        }
    }
    let disabled = names_in(&profile, "disabled");
    let (presets, presets_unreadable) = preset_names(paths);
    let mut view = ModsView {
        enabled: order
            .iter()
            .map(|name| (name.clone(), !disabled.contains(name)))
            .collect(),
        order,
        presets,
        presets_unreadable,
        folders: folders(settings),
        ..ModsView::default()
    };
    for name in &view.order {
        let folder = root.join(name);
        let files = mod_files(&folder, game.as_deref()).map_or(0, |files| files.len());
        view.facts
            .insert(name.clone(), (files, !patch_files(&folder).is_empty()));
        view.infos
            .insert(name.clone(), mod_info(&folder, game.as_deref()));
    }
    view.conflicts = conflicts(&root, &active(&view), game.as_deref());
    let meta = meta_folder(
        &settings.get_str("game_dir"),
        &settings.get_str("update_dir"),
    );
    if let (_, Some(profile)) = game_profile_of(&meta) {
        view.external = external_patches(
            &paths.patches_dir(&settings.get_str("patches_dir")),
            profile.app_version,
            &builtin_xml(paths, profile),
            profile.ids,
        );
        let chosen: Value = load_json(&paths.patches_json(), Value::Null);
        view.external_enabled = external_enabled(&view.external, &chosen)
            .into_iter()
            .collect();
    }
    view
}

/// Reads the lists again in the background.
pub fn refresh(state: &State) -> Task<Message> {
    let paths = state.paths.clone();
    let settings = state.settings.clone();
    blocking(move || load(&paths, &settings))
        .map(|view| Message::Mods(ModsMessage::Loaded(Box::new(view))))
}

fn mods_root(state: &State) -> PathBuf {
    state.paths.mods_dir(&state.settings.get_str("mods_dir"))
}

/// Works the conflicts of the enabled mods out again in the background.
fn refresh_conflicts(state: &State) -> Task<Message> {
    let root = mods_root(state);
    let game = game_path(&state.settings);
    let names = active(&state.mods);
    blocking(move || {
        let found = conflicts(&root, &names, game.as_deref());
        (names, found)
    })
    .map(|(names, found)| Message::Mods(ModsMessage::Conflicts(names, found)))
}

fn set_picture(state: &mut State) {
    state.mods.picture = state
        .mods
        .selected
        .as_ref()
        .and_then(|name| state.mods.infos.get(name))
        .and_then(|info| info.image.clone())
        .map(Handle::from_path);
}

/// Takes a finished read, keeping what the player changed since: the switches, the order while
/// the set of mods is the same, the patch ticks, the selection and the open conflict list.
fn loaded(state: &mut State, mut view: ModsView) -> Task<Message> {
    let before = active(&view);
    let old = std::mem::take(&mut state.mods);
    let same: BTreeSet<&String> = old.order.iter().collect();
    if !old.order.is_empty() && same == view.order.iter().collect() {
        view.order = old.order.clone();
    }
    for (name, on) in view.enabled.iter_mut() {
        if let Some(was) = old.enabled.get(name) {
            *on = *was;
        }
    }
    let known: BTreeSet<&String> = old.external.iter().map(|(key, _, _)| key).collect();
    for (key, _, _) in &view.external {
        if known.contains(key) {
            if old.external_enabled.contains(key) {
                view.external_enabled.insert(key.clone());
            } else {
                view.external_enabled.remove(key);
            }
        }
    }
    view.selected = old
        .selected
        .filter(|name| view.infos.contains_key(name.as_str()));
    view.conflicts_open = old.conflicts_open;
    view.preset_name = old.preset_name;
    state.mods = view;
    set_picture(state);
    performance::refresh_mod_resolutions(state);
    if active(&state.mods) == before {
        Task::none()
    } else {
        refresh_conflicts(state)
    }
}

/// After a switch or the order changed: the Performance sizes and the conflicts follow.
fn edited(state: &mut State) -> Task<Message> {
    // Task 32 writes mods.json; until then the Performance page reads the order from the state.
    performance::refresh_mod_resolutions(state);
    refresh_conflicts(state)
}

fn dialog(state: &mut State, kind: ModalKind, body: String) {
    state.modal = Some(Modal {
        kind,
        title: APP_NAME.to_string(),
        text: body,
    });
}

fn write_presets(state: &mut State, presets: &Map<String, Value>) {
    let written = serde_json::to_string_pretty(presets)
        .map_err(std::io::Error::other)
        .and_then(|json| atomic_write(&state.paths.mod_presets(), json.as_bytes()));
    if written.is_err() {
        let body = tr(
            &state.lang,
            "Could not save all settings. Check write permissions and free disk space, then try again.",
        );
        dialog(state, ModalKind::Error, body);
    }
}

fn save_preset(state: &mut State) {
    let name = state.mods.preset_name.trim().to_string();
    if name.is_empty() {
        let body = tr(&state.lang, "Type a name for the preset first.");
        dialog(state, ModalKind::Info, body);
        return;
    }
    let Some(mut presets) = read_presets(&state.paths) else {
        let body = tr(&state.lang, PRESETS_UNREADABLE);
        dialog(state, ModalKind::Error, body);
        return;
    };
    let disabled: Vec<&String> = state
        .mods
        .order
        .iter()
        .filter(|mod_name| !state.mods.enabled.get(*mod_name).copied().unwrap_or(false))
        .collect();
    presets.insert(
        name,
        json!({"order": state.mods.order, "disabled": disabled}),
    );
    write_presets(state, &presets);
    (state.mods.presets, state.mods.presets_unreadable) = preset_names(&state.paths);
}

fn load_preset(state: &mut State) -> Task<Message> {
    let presets = read_presets(&state.paths).unwrap_or_default();
    let Some(preset) = presets
        .get(state.mods.preset_name.trim())
        .filter(|preset| preset.is_object())
    else {
        return Task::none();
    };
    let (order, disabled) = apply_preset(preset, &state.mods.order);
    state.mods.enabled = order
        .iter()
        .map(|name| (name.clone(), !disabled.contains(name)))
        .collect();
    state.mods.order = order;
    edited(state)
}

fn delete_preset(state: &mut State) {
    let Some(mut presets) = read_presets(&state.paths) else {
        let body = tr(&state.lang, PRESETS_UNREADABLE);
        dialog(state, ModalKind::Error, body);
        return;
    };
    if presets
        .shift_remove(state.mods.preset_name.trim())
        .is_some()
    {
        write_presets(state, &presets);
    }
    (state.mods.presets, state.mods.presets_unreadable) = preset_names(&state.paths);
    state.mods.preset_name.clear();
}

/// Applies a Mods page event.
pub fn update(state: &mut State, message: ModsMessage) -> Task<Message> {
    match message {
        ModsMessage::Refresh => return refresh(state),
        // A read for folders typed over since is dropped: each keystroke starts one, and they
        // finish in any order (the read of the current folders is on its way or already in).
        ModsMessage::Loaded(view) if view.folders != folders(&state.settings) => {}
        ModsMessage::Loaded(view) => return loaded(state, *view),
        // A result for an older order or set of switches is dropped: threads finish out of order.
        ModsMessage::Conflicts(names, found) => {
            if names == active(&state.mods) {
                state.mods.conflicts = found;
            }
        }
        ModsMessage::Toggle(name) => {
            if let Some(on) = state.mods.enabled.get_mut(&name) {
                *on = !*on;
                return edited(state);
            }
        }
        ModsMessage::Select(name) => {
            // Clicking the selected mod again closes its details.
            let same = state.mods.selected.as_ref() == Some(&name);
            state.mods.selected = (!same).then_some(name);
            set_picture(state);
        }
        ModsMessage::Move(index, step) => {
            let other = i64::try_from(index).unwrap_or(i64::MAX) + i64::from(step);
            if let Ok(other) = usize::try_from(other) {
                if index < state.mods.order.len() && other < state.mods.order.len() {
                    state.mods.order.swap(index, other);
                    return edited(state);
                }
            }
        }
        ModsMessage::ToggleConflicts => state.mods.conflicts_open = !state.mods.conflicts_open,
        ModsMessage::PresetName(name) => state.mods.preset_name = name,
        ModsMessage::LoadPreset => return load_preset(state),
        ModsMessage::SavePreset => save_preset(state),
        ModsMessage::DeletePreset => delete_preset(state),
        ModsMessage::ToggleExternal(key) => {
            if !state.mods.external_enabled.remove(&key) {
                state.mods.external_enabled.insert(key);
            }
        }
        ModsMessage::OpenFolder => {
            let root = mods_root(state);
            return blocking(move || {
                let _ = std::fs::create_dir_all(&root);
                let _ = open_path(&root);
            })
            .map(|()| Message::Noop);
        }
        ModsMessage::OpenModFolder => {
            if let Some(name) = &state.mods.selected {
                let folder = mods_root(state).join(name);
                return blocking(move || {
                    let _ = open_path(&folder);
                })
                .map(|()| Message::Noop);
            }
        }
        ModsMessage::Website => {
            let url = state
                .mods
                .selected
                .as_ref()
                .and_then(|name| state.mods.infos.get(name))
                .map(|info| info.url.clone())
                .filter(|url| !url.is_empty());
            if let Some(url) = url {
                return blocking(move || {
                    let _ = open_url(&url);
                })
                .map(|()| Message::Noop);
            }
        }
    }
    Task::none()
}

// --- View ---

fn flat<'a>(label: String, message: ModsMessage) -> Element<'a, Message> {
    button(text(label))
        .style(theme::button_flat)
        .on_press(Message::Mods(message))
        .into()
}

fn muted<'a>(label: String) -> Element<'a, Message> {
    text(label).size(13).style(theme::text_muted).into()
}

/// A mod's name as a click target without a button frame.
fn name_button<'a>(label: String, ink: Color, bold: bool, name: String) -> Element<'a, Message> {
    let font = if bold {
        theme::CINZEL_BOLD
    } else {
        theme::UI_FONT
    };
    button(text(label).font(font))
        .padding(0)
        .width(Fill)
        .style(move |_, _| button::Style {
            text_color: ink,
            ..button::Style::default()
        })
        .on_press(Message::Mods(ModsMessage::Select(name)))
        .into()
}

fn mod_row<'a>(state: &State, index: usize) -> Element<'a, Message> {
    let view = &state.mods;
    let name = view.order[index].clone();
    let on = view.enabled.get(&name).copied().unwrap_or(false);
    let chosen = view.selected.as_ref() == Some(&name);
    let conflicted = view
        .conflicts
        .iter()
        .any(|(_, owners)| owners.contains(&name));
    let label = view
        .infos
        .get(&name)
        .map_or_else(|| name.clone(), |info| info.name.clone());
    let ink = if chosen {
        palette::RED_HI
    } else if on {
        palette::TEXT
    } else {
        palette::MUTED
    };
    let toggled = name.clone();
    let boxed = check_box(on)
        .on_toggle(move |_| Message::Mods(ModsMessage::Toggle(toggled.clone())))
        .style(theme::checkbox_red);
    let mark = text(if conflicted { "⚠" } else { "" }).color(palette::GOLD);
    let step = |glyph: &'static str, by: i32| {
        button(text(glyph).size(12))
            .padding([2, 8])
            .style(theme::button_flat)
            .on_press(Message::Mods(ModsMessage::Move(index, by)))
    };
    let line = row![
        boxed,
        text((index + 1).to_string())
            .width(24)
            .align_x(iced::alignment::Horizontal::Right)
            .style(theme::text_muted),
        name_button(label, ink, chosen, name),
        mark,
        step("▲", -1),
        step("▼", 1),
    ]
    .spacing(10)
    .align_y(Center);
    let edge = if chosen {
        palette::RED_HI
    } else {
        palette::LINE
    };
    container(line)
        .padding([5, 12])
        .width(Fill)
        .style(move |_| container::Style {
            background: chosen.then_some(iced::Background::Color(palette::CARD)),
            border: Border {
                color: edge,
                width: if chosen { 1.0 } else { 0.0 },
                radius: 0.0.into(),
            },
            ..container::Style::default()
        })
        .into()
}

fn empty_state<'a>(state: &State) -> Element<'a, Message> {
    let lang = state.lang.as_str();
    container(
        column![
            text(tr(lang, "No mods yet"))
                .font(theme::CINZEL_BOLD)
                .size(18),
            text(tr(lang, "Drop a mod folder or .zip in the mods folder."))
                .style(theme::text_muted),
            flat(tr(lang, "Open mods folder"), ModsMessage::OpenFolder),
        ]
        .spacing(10)
        .align_x(Center),
    )
    .padding([28, 0])
    .center_x(Fill)
    .into()
}

fn list<'a>(state: &State) -> Element<'a, Message> {
    if state.mods.order.is_empty() {
        return container(empty_state(state))
            .width(Fill)
            .style(theme::container_card)
            .into();
    }
    let rows = (0..state.mods.order.len()).fold(column![].spacing(1), |rows, index| {
        rows.push(mod_row(state, index))
    });
    container(rows)
        .width(Fill)
        .style(|_| container::Style {
            background: Some(iced::Background::Color(palette::PANEL)),
            border: Border {
                color: palette::LINE,
                width: 1.0,
                radius: 0.0.into(),
            },
            ..container::Style::default()
        })
        .into()
}

fn conflict_summary<'a>(state: &State) -> Element<'a, Message> {
    let lang = state.lang.as_str();
    let found = &state.mods.conflicts;
    let Some(first) = found.first() else {
        return space().into();
    };
    let summary = if found.len() == 1 {
        let winner = first.1.last().cloned().unwrap_or_default();
        tr_fmt(
            lang,
            "⚠ 1 file is changed by two mods. {} wins.",
            &[&winner],
        )
    } else {
        tr_fmt(
            lang,
            "⚠ {} files are changed by more than one mod. The lower one in the list wins.",
            &[&found.len().to_string()],
        )
    };
    let toggle = if state.mods.conflicts_open {
        "Hide"
    } else {
        "Details"
    };
    let mut body = column![row![
        text(summary).color(palette::GOLD),
        flat(tr(lang, toggle), ModsMessage::ToggleConflicts),
    ]
    .spacing(10)
    .align_y(Center)]
    .spacing(2);
    if state.mods.conflicts_open {
        for (path, owners) in found.iter().take(CONFLICT_LINES) {
            body = body.push(muted(format!("{path}: {}", owners.join(" > "))));
        }
        if found.len() > CONFLICT_LINES {
            let more = (found.len() - CONFLICT_LINES).to_string();
            body = body.push(muted(tr_fmt(lang, "… and {} more", &[&more])));
        }
    }
    body.into()
}

/// The details card of the selected mod; nothing when it has no more than its name.
fn details<'a>(state: &State) -> Option<Element<'a, Message>> {
    let lang = state.lang.as_str();
    let name = state.mods.selected.as_ref()?;
    let info = state.mods.infos.get(name)?;
    let described = [
        info.image.is_some(),
        !info.description.is_empty(),
        !info.author.is_empty(),
        !info.version.is_empty(),
        !info.url.is_empty(),
    ];
    if !described.contains(&true) {
        return None;
    }
    let mut card = column![].spacing(6).width(PICTURE_WIDTH);
    if let Some(handle) = &state.mods.picture {
        card = card.push(
            container(image(handle.clone()).content_fit(ContentFit::Contain))
                .max_width(PICTURE_WIDTH)
                .max_height(PICTURE_WIDTH * 3.0 / 4.0),
        );
    }
    card = card.push(text(info.name.clone()).font(theme::CINZEL_BOLD).size(18));
    let mut byline = Vec::new();
    if !info.author.is_empty() {
        byline.push(tr_fmt(lang, "by {}", &[&info.author]));
    }
    if !info.version.is_empty() {
        byline.push(format!("v{}", info.version));
    }
    if !byline.is_empty() {
        card = card.push(muted(byline.join("  ·  ")));
    }
    if !info.description.is_empty() {
        card = card.push(text(info.description.clone()).size(14));
    }
    let (count, patched) = state.mods.facts.get(name).copied().unwrap_or_default();
    let mut facts = if count == 1 {
        tr(lang, "1 file")
    } else {
        tr_fmt(lang, "{} files", &[&count.to_string()])
    };
    if patched {
        facts += &format!("  ·  {}", tr(lang, "game patches"));
    }
    card = card.push(muted(facts));
    for (path, owners) in state
        .mods
        .conflicts
        .iter()
        .filter(|(_, owners)| owners.contains(name))
        .take(CARD_CONFLICTS)
    {
        let others: Vec<&str> = owners
            .iter()
            .filter(|owner| *owner != name)
            .map(String::as_str)
            .collect();
        let file = path.rsplit('/').next().unwrap_or(path);
        let winner = owners.last().map_or("", String::as_str);
        card = card.push(
            text(tr_fmt(
                lang,
                "⚠ {} also in {}. {} wins.",
                &[file, &others.join(", "), winner],
            ))
            .size(13)
            .color(palette::GOLD),
        );
    }
    let mut buttons = row![flat(
        tr(lang, "Open mod folder"),
        ModsMessage::OpenModFolder
    )]
    .spacing(6);
    if !info.url.is_empty() {
        buttons = buttons.push(flat(tr(lang, "Website"), ModsMessage::Website));
    }
    card = card.push(buttons);
    Some(
        container(card)
            .padding(16)
            .style(theme::container_card)
            .into(),
    )
}

fn presets_section<'a>(state: &State) -> Element<'a, Message> {
    let lang = state.lang.as_str();
    let name = text_input("", &state.mods.preset_name)
        .on_input(|name| Message::Mods(ModsMessage::PresetName(name)))
        .width(220);
    let controls = row![
        name,
        flat(tr(lang, "Load"), ModsMessage::LoadPreset),
        flat(tr(lang, "Save"), ModsMessage::SavePreset),
        flat(tr(lang, "Delete"), ModsMessage::DeletePreset),
    ]
    .spacing(6)
    .align_y(Center);
    let mut body = column![section(state.rtl, &tr(lang, "Presets")), controls].spacing(6);
    if state.mods.presets_unreadable {
        body = body.push(
            text(tr(lang, PRESETS_UNREADABLE))
                .size(13)
                .style(theme::text_muted),
        );
    }
    if !state.mods.presets.is_empty() {
        let saved = state
            .mods
            .presets
            .iter()
            .fold(row![].spacing(6), |saved, name| {
                saved.push(
                    button(text(name.clone()).size(13))
                        .padding([2, 8])
                        .style(theme::button_flat)
                        .on_press(Message::Mods(ModsMessage::PresetName(name.clone()))),
                )
            });
        body = body.push(saved);
    }
    body.into()
}

fn patches_section<'a>(state: &State) -> Element<'a, Message> {
    let lang = state.lang.as_str();
    let mut body = column![section(state.rtl, &tr(lang, "Third-party patches"))].spacing(2);
    if state.mods.external.is_empty() {
        return body
            .push(muted(tr(lang, "No patch files for this game version yet.")))
            .into();
    }
    for (key, _, meta) in &state.mods.external {
        let label = match &meta.author {
            Some(author) if !author.is_empty() => format!("{key}  ·  {author}"),
            _ => key.clone(),
        };
        let toggled = key.clone();
        body = body.push(toggle_row(
            state.rtl,
            &label,
            state.mods.external_enabled.contains(key),
            Some(move |_| Message::Mods(ModsMessage::ToggleExternal(toggled.clone()))),
            None,
        ));
    }
    body.into()
}

pub fn view(state: &State) -> Element<'_, Message> {
    let lang = state.lang.as_str();
    let view = &state.mods;
    let count = if view.order.is_empty() {
        String::new()
    } else {
        let on = active(view).len().to_string();
        tr_fmt(
            lang,
            "{} of {} active",
            &[&on, &view.order.len().to_string()],
        )
    };
    // R94: the box beside "Load mods", the count and Open folder at the row's end.
    let top = checkbox_beside(
        state.rtl,
        "mods_enabled",
        &tr(lang, "Load mods"),
        state.settings.get_bool("mods_enabled"),
        row![
            text(count).style(theme::text_muted),
            flat(tr(lang, "Open folder"), ModsMessage::OpenFolder),
        ]
        .spacing(18)
        .align_y(Center)
        .into(),
    );
    let head = row![
        container(section(state.rtl, &tr(lang, "Load order"))).width(Fill),
        muted(tr(lang, "Lower in the list wins")),
        flat(tr(lang, "Refresh"), ModsMessage::Refresh),
    ]
    .spacing(10)
    .align_y(Center);
    let left = column![list(state), conflict_summary(state)]
        .spacing(10)
        .width(Fill);
    let mut body = row![left].spacing(16);
    if let Some(card) = details(state) {
        body = body.push(container(card).align_y(iced::alignment::Vertical::Top));
    }
    // Open opens the setting, or the data folder's one when it is empty (Python `open_path`).
    let folder_row = |key: &str, label: &str, default: &Path| {
        let value = state.settings.get_str(key);
        let open = if value.is_empty() {
            default.to_path_buf()
        } else {
            PathBuf::from(&value)
        };
        folder(
            state.rtl,
            key,
            &tr(lang, label),
            &value,
            &default.display().to_string(),
            Some(open),
            lang,
        )
    };
    column![
        top,
        head,
        body,
        presets_section(state),
        patches_section(state),
        section(state.rtl, &tr(lang, "Folders")),
        folder_row("mods_dir", "Mods folder", &state.paths.mods_dir("")),
        folder_row(
            "patches_dir",
            "Patches folder",
            &state.paths.patches_dir("")
        ),
    ]
    .spacing(2)
    .into()
}
