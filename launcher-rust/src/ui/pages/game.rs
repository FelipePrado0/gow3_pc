//! The Game page: cheats, folders, save backups, the game language and the game patches.
//!
//! Every read of the disk happens in `game_changed`, `refresh_backups` and
//! `refresh_game_patches`; `view` only draws what they stored in the state.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use iced::widget::{button, column, pick_list, row as layout, text};
use iced::{Center, Element, Left, Right, Task};
use serde_json::{Map, Value};

use crate::core::game::{game_profile_of, meta_folder};
use crate::core::i18n::{tr, tr_fmt};
use crate::core::options::LANGUAGES;
use crate::core::patches::{builtin_xml, game_patch_metas, selectable, PatchMeta};
use crate::core::save_backup::{list_backups, restore, KEEP};
use crate::ui::app::{Message, State};
use crate::ui::home;
use crate::ui::pages::{mods, performance, trophies};
use crate::ui::play::GameState;
use crate::ui::tasks::blocking;
use crate::ui::theme;
use crate::ui::widgets::{
    checkbox_ini, folder, min_width, row, section, select, toggle_row, Modal, ModalKind,
    ModalMessage, SELECT_MIN, SELECT_TEXT,
};
use crate::APP_NAME;

/// The note under the Game language row; the translation tables key on this text.
pub const LANGUAGE_NOTE: &str = "Used on the game's first start. After that, the game keeps its own text and speech languages in its settings save: change them in the game, in Options → Game Settings → Game Text and Options → Audio → Speech.";

/// Game patches of the current dump.
#[derive(Debug, Clone, Default)]
pub struct PatchesView {
    /// The title ID of the game the patches belong to; `None` without a known game.
    pub title_id: Option<String>,
    /// The patches the page offers, in file order.
    pub selectable: Vec<PatchMeta>,
    /// The names that are ticked.
    pub chosen: BTreeSet<String>,
    /// The game version the patches are for.
    pub version: String,
    /// The patch file the list came from.
    pub xml_name: String,
}

/// What the save backups row can do.
#[derive(Debug, Clone)]
pub enum BackupMessage {
    Refresh,
    Choose(String),
    Restore,
    RestoreConfirmed,
    /// The restore finished: the backup it made of the replaced saves, or why it failed.
    Restored(Result<Option<String>, String>),
}

/// The dialog title of a folder key's `Browse…`.
pub fn browse_prompt(key: &str) -> &'static str {
    match key {
        "update_dir" => "Choose the folder of the update (eboot.bin, sce_sys)",
        "user_dir" => "Choose the saves folder",
        "mods_dir" => "Choose the mods folder",
        "patches_dir" => "Choose the patches folder",
        _ => "Choose the folder with eboot.bin",
    }
}

fn user_dir(state: &State) -> PathBuf {
    state.paths.user_dir(&state.settings.get_str("user_dir"))
}

// --- Folders ---

/// A folder was chosen in the dialog: store it and refresh what depends on it.
pub fn folder_chosen(state: &mut State, key: &str, path: &Path) -> Task<Message> {
    crate::ui::app::apply_set(state, key, Value::String(path.display().to_string()));
    match key {
        "game_dir" | "update_dir" => return game_changed(state),
        "user_dir" => {
            home::refresh_facts(state);
            refresh_backups(state);
            // The saves folder holds trophies.dat.
            return trophies::refresh(state);
        }
        "mods_dir" | "patches_dir" => return mods::refresh(state),
        _ => {}
    }
    Task::none()
}

/// The game or update folder changed: the title, the patches and the lists follow.
pub fn game_changed(state: &mut State) -> Task<Message> {
    home::refresh_facts(state);
    performance::refresh_mod_resolutions(state);
    refresh_game_patches(state);
    refresh_backups(state);
    Task::batch([trophies::refresh(state), mods::refresh(state)])
}

// --- Save backups ---

/// Lists the backups of the saves folder and chooses the newest.
pub fn refresh_backups(state: &mut State) {
    state.backups = list_backups(&user_dir(state));
    state.backup_choice = state.backups.first().cloned().unwrap_or_default();
}

/// Whether the game is running, which blocks a restore of the saves.
pub fn restore_blocked(state: &State) -> bool {
    match state.game {
        GameState::Idle | GameState::Copying => false,
        GameState::Preparing { .. } | GameState::Running { .. } => true,
    }
}

/// Replaces the saves with backup `name`; returns the name of the backup of the replaced saves.
pub fn restore_saves(user_dir: &Path, name: &str) -> Result<Option<String>, String> {
    restore(user_dir, name, None)
        .map(|safety| {
            safety.and_then(|path| path.file_name().map(|n| n.to_string_lossy().into_owned()))
        })
        .map_err(|error| error.to_string())
}

fn info(state: &mut State, body: String) {
    state.modal = Some(Modal {
        kind: ModalKind::Info,
        title: APP_NAME.to_string(),
        text: body,
    });
}

/// Applies a save backups event.
pub fn backup_update(state: &mut State, message: BackupMessage) -> Task<Message> {
    let lang = state.lang.clone();
    match message {
        BackupMessage::Refresh => refresh_backups(state),
        BackupMessage::Choose(name) => state.backup_choice = name,
        BackupMessage::Restore => {
            let name = state.backup_choice.clone();
            if name.is_empty() {
                return Task::none();
            }
            if restore_blocked(state) {
                info(state, tr(&lang, "Close the game before restoring saves."));
                return Task::none();
            }
            state.modal = Some(Modal {
                kind: ModalKind::YesNo {
                    yes: Message::Backup(BackupMessage::RestoreConfirmed),
                    no: Message::Modal(ModalMessage::Dismiss),
                },
                title: APP_NAME.to_string(),
                text: tr_fmt(
                    &lang,
                    "Replace the current saves with the backup {}? The current saves are backed up first.",
                    &[&name],
                ),
            });
        }
        BackupMessage::RestoreConfirmed => {
            state.modal = None;
            let user = user_dir(state);
            let name = state.backup_choice.clone();
            return blocking(move || restore_saves(&user, &name))
                .map(|result| Message::Backup(BackupMessage::Restored(result)));
        }
        BackupMessage::Restored(Ok(safety)) => {
            // The list is rebuilt below, which moves the choice: name the backup first.
            let name = state.backup_choice.clone();
            refresh_backups(state);
            home::refresh_facts(state);
            let mut body = tr_fmt(&lang, "Saves restored from {}.", &[&name]);
            if let Some(safety) = safety {
                body += &tr_fmt(&lang, " Previous saves: {}.", &[&safety]);
            }
            info(state, body);
        }
        BackupMessage::Restored(Err(error)) => {
            state.modal = Some(Modal {
                kind: ModalKind::Error,
                title: APP_NAME.to_string(),
                text: tr_fmt(&lang, "Could not restore: {}", &[&error]),
            });
        }
    }
    Task::none()
}

// --- Game patches ---

fn saved_choice(state: &State, title_id: &str) -> Option<BTreeSet<String>> {
    let list = state
        .settings
        .get("game_patches")?
        .get(title_id)?
        .as_array()?;
    Some(
        list.iter()
            .filter_map(|name| name.as_str().map(str::to_string))
            .collect(),
    )
}

/// The page's selection into the settings: the whole list for this game, in file order, without
/// the resolution and frame rate patches (the start adds those from `spartanport.ini`).
pub fn store_game_patches(state: &mut State) {
    let Some(title_id) = state.patches.title_id.clone() else {
        return;
    };
    let names: Vec<Value> = state
        .patches
        .selectable
        .iter()
        .filter(|meta| state.patches.chosen.contains(&meta.name))
        .map(|meta| Value::String(meta.name.clone()))
        .collect();
    let mut all = match state.settings.get("game_patches") {
        Some(Value::Object(map)) => map.clone(),
        _ => Map::new(),
    };
    all.insert(title_id, Value::Array(names));
    state.settings.set("game_patches", Value::Object(all));
}

/// Reads the built-in patches of the game folder into `state.patches`, keeping the selection of
/// the game that was shown until now.
pub fn refresh_game_patches(state: &mut State) {
    store_game_patches(state);
    let meta = meta_folder(
        &state.settings.get_str("game_dir"),
        &state.settings.get_str("update_dir"),
    );
    let (title_id, profile) = game_profile_of(&meta);
    let (Some(title_id), Some(profile)) = (title_id, profile) else {
        state.patches = PatchesView::default();
        return;
    };
    let xml = builtin_xml(&state.paths, profile);
    let metas = game_patch_metas(&xml, profile.app_version).unwrap_or_default();
    let chosen = saved_choice(state, &title_id).unwrap_or_else(|| {
        metas
            .iter()
            .filter(|meta| meta.enabled_by_default)
            .map(|meta| meta.name.clone())
            .collect()
    });
    state.patches = PatchesView {
        title_id: Some(title_id),
        selectable: selectable(&metas),
        chosen,
        version: profile.app_version.to_string(),
        xml_name: xml
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default(),
    };
}

/// Ticks or unticks a patch.
pub fn patch_toggled(state: &mut State, name: &str, on: bool) {
    if on {
        state.patches.chosen.insert(name.to_string());
    } else {
        state.patches.chosen.remove(name);
    }
    store_game_patches(state);
}

// --- View ---

/// An ini flag is on when its value is `"1"`.
fn ini_on(state: &State, key: &str) -> bool {
    state.ini.values.get(key).is_some_and(|value| value == "1")
}

/// A folder row with its note under it.
pub fn folder_row<'a>(state: &State, key: &str, label: &str, note: &str) -> Element<'a, Message> {
    let value = state.settings.get_str(key);
    // Python `open_path`: an empty saves folder opens `DATA_DIR/user`; an unset game or update
    // folder has nothing to open.
    let open = if key == "user_dir" {
        Some(state.paths.user_dir(&value))
    } else {
        (!value.is_empty()).then(|| PathBuf::from(&value))
    };
    column![
        folder(state.rtl, key, label, &value, "", open, &state.lang),
        text(note.to_string()).size(13).style(theme::text_muted)
    ]
    .spacing(2)
    .align_x(if state.rtl { Right } else { Left })
    .into()
}

fn backups_row(state: &State) -> Element<'_, Message> {
    let lang = state.lang.as_str();
    let choose = min_width(
        SELECT_MIN,
        pick_list(
            state.backups.clone(),
            (!state.backup_choice.is_empty()).then(|| state.backup_choice.clone()),
            |name| Message::Backup(BackupMessage::Choose(name)),
        )
        .text_size(SELECT_TEXT)
        .style(theme::pick_list_dark),
    );
    let restore = button(text(tr(lang, "Restore")))
        .style(theme::button_flat)
        .on_press(Message::Backup(BackupMessage::Restore));
    let refresh = button(text(tr(lang, "Refresh")))
        .style(theme::button_flat)
        .on_press(Message::Backup(BackupMessage::Refresh));
    row(
        state.rtl,
        &tr(lang, "Save backups"),
        layout![choose, restore, refresh]
            .spacing(6)
            .align_y(Center)
            .into(),
        Some(&tr_fmt(
            lang,
            "Made before every start, newest first; the last {} are kept. Restoring backs up the current saves first.",
            &[&KEEP.to_string()],
        )),
    )
}

fn patches_section(state: &State) -> Element<'_, Message> {
    let lang = state.lang.as_str();
    let mut body = column![section(state.rtl, &tr(lang, "Game patches"))].spacing(2);
    let patches = &state.patches;
    let Some(title_id) = patches.title_id.as_deref() else {
        return body
            .push(text(tr(lang, "No built-in patches for this game.")).style(theme::text_muted))
            .into();
    };
    body = body.push(
        text(tr_fmt(
            lang,
            "{} · version {} · {}. Resolution and frame rate patches follow Performance.",
            &[title_id, &patches.version, &patches.xml_name],
        ))
        .style(theme::text_muted),
    );
    for meta in &patches.selectable {
        let label = match &meta.author {
            Some(author) => format!("{}  ({author})", meta.name),
            None => meta.name.clone(),
        };
        let name = meta.name.clone();
        let note = meta.note.as_deref().map(|note| tr(lang, note));
        body = body.push(toggle_row(
            state.rtl,
            &label,
            patches.chosen.contains(&meta.name),
            Some(move |on| Message::PatchToggled(name.clone(), on)),
            note.as_deref(),
        ));
    }
    body.into()
}

pub fn view(state: &State) -> Element<'_, Message> {
    let lang = state.lang.as_str();
    let hint = |english: &str| tr(lang, english);
    let language = state.settings.get_str("language");
    let default_saves = state.paths.data_dir.join("user").display().to_string();

    column![
        section(state.rtl, &tr(lang, "Cheats")),
        checkbox_ini(
            state.rtl,
            "cheat_double_jumps",
            &tr(lang, "Infinite Double Jumps"),
            ini_on(state, "cheat_double_jumps"),
            Some(&hint(
                "Repeat air jumps without landing. Also available in the in-game Cheats menu."
            )),
        ),
        checkbox_ini(
            state.rtl,
            "cheat_very_hard",
            &tr(lang, "Unlock Very Hard"),
            ini_on(state, "cheat_very_hard"),
            Some(&hint(
                "Select Very Hard without completing the game. Does not change existing save difficulty. In-game: enable before opening New Game."
            )),
        ),
        section(state.rtl, &tr(lang, "Folders")),
        folder_row(
            state,
            "game_dir",
            &tr(lang, "Game folder"),
            &hint(
                "Your own dump of the game (eboot.bin, sce_module, sce_sys): SpartanPort, any region and version."
            ),
        ),
        folder_row(
            state,
            "update_dir",
            &tr(lang, "Update folder (optional)"),
            &hint(
                "Only if the update is dumped apart from the game: it is laid over the game folder without changing either. Leave empty when the game folder already has the update."
            ),
        ),
        folder_row(
            state,
            "user_dir",
            &tr(lang, "Saves folder"),
            &tr_fmt(
                lang,
                "Empty: {} (shader caches are kept there too).",
                &[&default_saves],
            ),
        ),
        section(state.rtl, &tr(lang, "Saves")),
        backups_row(state),
        section(state.rtl, &tr(lang, "Language")),
        row(
            state.rtl,
            &tr(lang, "Game language"),
            select("language", LANGUAGES, &language, lang),
            Some(&hint(LANGUAGE_NOTE)),
        ),
        patches_section(state),
    ]
    .spacing(2)
    .into()
}
