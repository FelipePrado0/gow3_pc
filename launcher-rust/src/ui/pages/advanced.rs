//! The Advanced page: launcher options, Linux integrations, files, the shader cache and the
//! diagnostics. The Log is its own page.
//!
//! `view` only draws. The slow work (the shortcut, the Steam bridge, the cache removal) runs on a
//! thread and comes back as a message; the Linux tools were probed once in `State::new`.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use iced::widget::{button, column, row as layout, text};
use iced::{Center, Element, Task};
use serde_json::Value;

use crate::core::desktop::{host_environment, windows_shortcut, write_linux_shortcut};
use crate::core::i18n::{tr, tr_fmt};
use crate::core::options::{ui_scale_choice, UI_LANGUAGES, UI_SCALES, VIDEO_BACKENDS};
use crate::core::paths::Paths;
use crate::core::settings::Settings;
use crate::core::steam::{self, Added, Shortcut, SteamError};
use crate::core::steam_art;
use crate::ui::app::{Message, State};
use crate::ui::pages::performance::SelectRow;
use crate::ui::persist;
use crate::ui::play::GameState;
use crate::ui::tasks::blocking;
use crate::ui::theme;
use crate::ui::update::UpdateMessage;
use crate::ui::widgets::row::actions;
use crate::ui::widgets::{
    checkbox, choice_pairs, row, section, select_pairs, select_pairs_small, toggle_row, Modal,
    ModalKind, ModalMessage,
};
use crate::{APP_NAME, VERSION};

/// What names the box of the Files row's buttons (`row::control_id`).
pub const FILES_ROW: &str = "files";

/// One check box of the page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckRow {
    pub key: &'static str,
    pub label: String,
    pub hint: String,
    pub checked: bool,
    /// False for a tool this system does not have.
    pub enabled: bool,
}

/// Whether the Home fire burns: on unless `launcher_effects` is explicitly off.
pub fn effects_enabled(settings: &Settings) -> bool {
    match settings.get("launcher_effects") {
        Some(Value::Bool(on)) => *on,
        Some(Value::Number(n)) => n.as_f64() != Some(0.0),
        Some(Value::String(s)) => s != "0" && s != "false",
        _ => true,
    }
}

/// Whether the game runs, which blocks clearing its shader cache.
pub fn clear_cache_blocked(state: &State) -> bool {
    // As `if self.process: return` does.
    match state.game {
        GameState::Idle | GameState::Copying => false,
        GameState::Preparing { .. } | GameState::Running { .. } => true,
    }
}

fn restart(lang: &str, english: &str) -> String {
    format!("{}  ·  {}", tr(lang, english), tr(lang, "restart"))
}

/// The drop-downs of the page: the language, the scale and (off Windows) the window system.
pub fn rows(state: &State, lang: &str) -> Vec<SelectRow> {
    let app = |key: &str| state.settings.get_str(key);
    let mut rows = vec![
        SelectRow {
            key: "ui_language",
            pairs: choice_pairs(UI_LANGUAGES, lang),
            current: app("ui_language"),
        },
        SelectRow {
            key: "ui_scale",
            pairs: choice_pairs(UI_SCALES, lang),
            // An older file's "auto" or "2" shows as the choice it now stands for (R90).
            current: ui_scale_choice(&app("ui_scale")).to_string(),
        },
    ];
    if !cfg!(windows) {
        rows.push(SelectRow {
            key: "video_backend",
            pairs: choice_pairs(VIDEO_BACKENDS, lang),
            current: app("video_backend"),
        });
    }
    rows
}

fn check_row(
    state: &State,
    key: &'static str,
    label: String,
    hint: String,
    enabled: bool,
) -> CheckRow {
    CheckRow {
        key,
        label,
        hint,
        checked: if key == "launcher_effects" {
            effects_enabled(&state.settings)
        } else {
            state.settings.get_bool(key)
        },
        enabled,
    }
}

/// The check boxes of the Launcher section, the Effects row last.
pub fn launcher_checks(state: &State, lang: &str) -> Vec<CheckRow> {
    let mut checks = vec![
        check_row(
            state,
            "discord_rpc",
            tr(lang, "Show Discord Rich Presence"),
            tr(
                lang,
                "Shows only the game name and the time played on your Discord profile.",
            ),
            true,
        ),
        check_row(
            state,
            "close_on_play",
            tr(lang, "Close the launcher when the game starts"),
            String::new(),
            true,
        ),
    ];
    if state.self_update {
        checks.push(check_row(
            state,
            "check_updates",
            tr(lang, "Check for updates when the launcher opens"),
            String::new(),
            true,
        ));
    }
    checks.push(check_row(
        state,
        "launcher_effects",
        tr(lang, "Effects"),
        tr(
            lang,
            "Embers and flames on the Home screen. Off saves GPU time.",
        ),
        true,
    ));
    checks
}

/// The check boxes of the Linux section; empty on Windows.
pub fn linux_checks(state: &State, lang: &str) -> Vec<CheckRow> {
    if cfg!(windows) {
        return Vec::new();
    }
    let tools = state.linux_tools;
    let missing = |present: bool| {
        if present {
            String::new()
        } else {
            format!("  {}", tr(lang, "Not installed."))
        }
    };
    vec![
        check_row(
            state,
            "desktop_integration",
            tr(lang, "Menu entry and icon (AppImage)"),
            tr(
                lang,
                "Creates spartanport.desktop in ~/.local/share/applications so the launcher and the game show their icon in the dock and the menu. Turning it off removes the entry.",
            ),
            true,
        ),
        check_row(
            state,
            "gamemode",
            restart(lang, "GameMode"),
            tr(
                lang,
                "Asks GameMode (gamemoderun) for the performance governor while the game runs.",
            ) + &missing(tools.gamemode),
            tools.gamemode,
        ),
        check_row(
            state,
            "mangohud",
            restart(lang, "MangoHud overlay"),
            tr(
                lang,
                "Shows MangoHud (FPS, frame times, temperatures) through its Vulkan layer.",
            ) + &missing(tools.mangohud),
            tools.mangohud,
        ),
    ]
}

/// The check boxes of the Diagnostics section.
pub fn diagnostic_checks(state: &State, lang: &str) -> Vec<CheckRow> {
    vec![
        check_row(
            state,
            "frame_stats",
            tr(lang, "Frame statistics in the log (every 5 s)"),
            String::new(),
            true,
        ),
        check_row(
            state,
            "gpu_profile",
            tr(lang, "GPU time per pass in the log"),
            String::new(),
            true,
        ),
        check_row(
            state,
            "perf_diag",
            tr(lang, "GPU wait and readback report in the log (every 5 s)"),
            tr(
                lang,
                "Shows where the frame waits for the GPU and which reads cause it. For performance tests.",
            ),
            true,
        ),
        check_row(
            state,
            "frame_capture_key",
            tr(lang, "F11 captures a frame for analysis"),
            tr(
                lang,
                "Records how the next frame is drawn into the captures folder.",
            ),
            true,
        ),
        check_row(
            state,
            "vk_validation",
            tr(
                lang,
                "Vulkan validation layers (needs the Vulkan SDK; much slower)",
            ),
            String::new(),
            true,
        ),
    ]
}

/// The setting keys of every check box that is shown.
pub fn check_keys(state: &State) -> Vec<&'static str> {
    let lang = state.lang.as_str();
    let mut all = launcher_checks(state, lang);
    all.extend(linux_checks(state, lang));
    all.extend(diagnostic_checks(state, lang));
    all.into_iter().map(|check| check.key).collect()
}

// --- View ---

fn check_view<'a>(rtl: bool, check: &CheckRow) -> Element<'a, Message> {
    let hint = Some(check.hint.as_str());
    if check.enabled {
        return checkbox(rtl, check.key, &check.label, check.checked, hint);
    }
    // A tool this system lacks: the box is drawn, but nothing toggles it.
    toggle_row(
        rtl,
        &check.label,
        check.checked,
        None::<fn(bool) -> Message>,
        hint,
    )
}

fn flat<'a>(label: String, message: Message) -> Element<'a, Message> {
    button(text(label))
        .style(theme::button_flat)
        .on_press(message)
        .into()
}

pub fn view(state: &State) -> Element<'_, Message> {
    let lang = state.lang.as_str();
    let hint = |english: &str| tr(lang, english);
    let selects = rows(state, lang);
    let pick = |key: &str| match selects.iter().find(|row| row.key == key) {
        // Mockup `.sel.sm`: the scale's short choices take a narrower box.
        Some(row) if key == "ui_scale" => select_pairs_small(key, &row.pairs, &row.current),
        Some(row) => select_pairs(key, &row.pairs, &row.current),
        None => select_pairs(key, &[], ""),
    };
    let mut page = column![
        section(state.rtl, &tr(lang, "Launcher")),
        row(
            state.rtl,
            &tr(lang, "Launcher language"),
            pick("ui_language"),
            Some(&hint("Applies when the launcher opens again.")),
        ),
        row(
            state.rtl,
            &tr(lang, "GUI Scale"),
            pick("ui_scale"),
            Some(&hint("For launcher only. Applies after Save.")),
        ),
    ]
    .spacing(2);
    for check in launcher_checks(state, lang) {
        page = page.push(check_view(state.rtl, &check));
    }

    let mut buttons = layout![].spacing(6).align_y(Center);
    if state.self_update {
        buttons = buttons.push(flat(
            tr(lang, "Check for updates"),
            Message::Update(UpdateMessage::Check { manual: true }),
        ));
    }
    buttons = buttons
        .push(flat(tr(lang, "Desktop shortcut"), Message::DesktopShortcut))
        .push(flat(tr(lang, "Add to Steam"), Message::AddToSteam))
        .push(text(format!("v{VERSION}")).style(theme::text_muted));
    // A narrow drawer in a long language moves the last buttons to a second line.
    page = page.push(column![buttons.wrap().vertical_spacing(6)].padding([10, 0]));

    if !cfg!(windows) {
        page = page.push(section(state.rtl, &tr(lang, "Linux")));
        let linux = linux_checks(state, lang);
        if let Some(first) = linux.first() {
            page = page.push(check_view(state.rtl, first));
        }
        let wayland = if state.linux_tools.wayland {
            String::new()
        } else {
            format!("  {}", tr(lang, "No Wayland session detected."))
        };
        page = page.push(row(
            state.rtl,
            &restart(lang, "Window system"),
            pick("video_backend"),
            Some(
                &(hint(
                    "Automatic lets SDL choose. Pick X11 if the window misbehaves on Wayland (it runs through XWayland).",
                ) + &wayland),
            ),
        ));
        for check in linux.iter().skip(1) {
            page = page.push(check_view(state.rtl, check));
        }
    }

    let clear = button(text(tr(lang, "Clear shader cache"))).style(theme::button_flat);
    let clear = if clear_cache_blocked(state) {
        clear
    } else {
        clear.on_press(Message::ClearShaderCache)
    };
    // One row (R93): the three buttons where every row's control ends, the note under them.
    let files = layout![
        flat(
            tr(lang, "Port folder"),
            Message::OpenFolder(state.paths.data_dir.clone())
        ),
        flat("spartanport.ini".to_string(), Message::OpenIni),
        clear,
    ]
    .spacing(6)
    .align_y(Center);
    page = page
        .push(section(state.rtl, &tr(lang, "Files")))
        .push(actions(
            state.rtl,
            FILES_ROW,
            files.into(),
            Some(&hint(
                "If the game only shows a black screen, this usually helps.",
            )),
        ));

    // A plain section, last on the page (R93; the Python folded it away).
    page = page.push(section(
        state.rtl,
        &tr(lang, "Diagnostics (for development)"),
    ));
    for check in diagnostic_checks(state, lang) {
        page = page.push(check_view(state.rtl, &check));
    }
    page.into()
}

// --- Actions ---

fn modal(state: &mut State, kind: ModalKind, body: String) {
    state.modal = Some(Modal {
        kind,
        title: APP_NAME.to_string(),
        text: body,
    });
}

/// The shader and pipeline caches of `user_dir`.
fn cache_dir(user_dir: &Path) -> PathBuf {
    user_dir.join("cache")
}

/// The bytes of every file under the shader cache; 0 without one.
pub fn cache_size(user_dir: &Path) -> u64 {
    fn walk(folder: &Path) -> u64 {
        let Ok(entries) = std::fs::read_dir(folder) else {
            return 0;
        };
        entries
            .flatten()
            .map(|entry| {
                let path = entry.path();
                if path.is_dir() {
                    walk(&path)
                } else {
                    entry.metadata().map(|meta| meta.len()).unwrap_or(0)
                }
            })
            .sum()
    }
    walk(&cache_dir(user_dir))
}

/// Whole megabytes, halves rounded to even as the Python's `round(size / 2**20)`.
pub fn megabytes(bytes: u64) -> u64 {
    (bytes as f64 / 1_048_576.0).round_ties_even() as u64
}

/// Removes the shader and pipeline caches; they are rebuilt while playing. A missing folder is
/// fine; a folder that stays (a file open elsewhere) is the error, by its path.
pub fn clear_cache(user_dir: &Path) -> Result<(), String> {
    let cache = cache_dir(user_dir);
    let _ = std::fs::remove_dir_all(&cache);
    if cache.exists() {
        Err(cache.display().to_string())
    } else {
        Ok(())
    }
}

fn env_var(key: &str) -> Option<String> {
    std::env::var(key).ok()
}

/// The launcher the shortcuts start: the AppImage when there is one, else this executable.
fn launcher_path() -> PathBuf {
    env_var("APPIMAGE")
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::current_exe().ok())
        .unwrap_or_default()
}

/// The desktop shortcut; runs on a thread. Windows makes `SpartanPort.lnk` and returns no files.
fn make_shortcut(paths: &Paths) -> Result<Vec<String>, String> {
    if cfg!(windows) {
        let exe = std::env::current_exe().map_err(|error| error.to_string())?;
        return windows_shortcut(&paths.port_dir, &exe).map(|()| Vec::new());
    }
    let launcher = launcher_path().display().to_string();
    let mut probe = Command::new("xdg-user-dir");
    probe
        .arg("DESKTOP")
        .env_clear()
        .envs(host_environment())
        .stdin(Stdio::null());
    let desktop = probe
        .output()
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
        .unwrap_or_default();
    let desktop = if desktop.is_empty() {
        PathBuf::from(env_var("HOME").unwrap_or_default()).join("Desktop")
    } else {
        PathBuf::from(desktop)
    };
    write_linux_shortcut(
        &format!("\"{launcher}\""),
        &launcher,
        &paths.port_dir.join("launcher").join("spartanport.png"),
        &desktop,
        None,
        &env_var,
    )
    .map_err(|error| error.to_string())
}

/// What Steam starts for `launcher`: it with `--play`, in its own folder.
pub fn steam_shortcut(launcher: &Path) -> Shortcut {
    Shortcut {
        exe: launcher.to_path_buf(),
        options: "--play".to_string(),
        start_dir: launcher.parent().map(Path::to_path_buf).unwrap_or_default(),
    }
}

/// What `add_to_steam` said, as the text of the dialog: `Ok` for the info, `Err` for the error.
pub fn steam_message(lang: &str, result: &Result<Added, SteamError>) -> Result<String, String> {
    match result {
        Ok(added) => {
            let english = if added.updated {
                "The shortcut \"{}\" was updated in Steam (accounts: {}), with its icon, capsules, background and logo. Open Steam to see it in the library."
            } else {
                "The shortcut \"{}\" was added to Steam (accounts: {}), with its icon, capsules, background and logo. Open Steam to see it in the library."
            };
            Ok(tr_fmt(
                lang,
                english,
                &[APP_NAME, &added.accounts.to_string()],
            ))
        }
        Err(SteamError::Running) => Err(tr(
            lang,
            "Steam is open. Close it completely (Steam > Exit) and click again: Steam rewrites its shortcuts when it closes.",
        )),
        Err(SteamError::NoSteam) => Err(tr(lang, "Steam was not found for this user.")),
        Err(SteamError::NoArt(_)) => Err(tr(
            lang,
            "The game folder has no sce_sys/pic1.png for the pictures.",
        )),
        Err(error) => Err(error.to_string().chars().take(400).collect()),
    }
}

/// Adds the game to Steam; on a thread.
fn add_to_steam(paths: &Paths, settings: &Settings, lang: &str) -> Result<String, String> {
    let game = settings.get_str("game_dir");
    if !Path::new(&game).join("eboot.bin").is_file() {
        return Err(tr(
            lang,
            "Choose the game folder with eboot.bin (CUSA01623).",
        ));
    }
    let result = steam::add_to_steam(
        Path::new(&game),
        &steam_shortcut(&launcher_path()),
        &paths.port_dir.join("launcher").join("spartanport.png"),
        &steam_art::fonts(&paths.port_dir),
        None,
        None,
    );
    steam_message(lang, &result)
}

/// Applies the page's events.
pub fn update(state: &mut State, message: Message) -> Task<Message> {
    let lang = state.lang.clone();
    match message {

        Message::ClearShaderCache => {
            if clear_cache_blocked(state) {
                let body = tr(&lang, "Close the game before deleting the shader cache.");
                modal(state, ModalKind::Info, body);
                return Task::none();
            }
            let user = state.paths.user_dir(&state.settings.get_str("user_dir"));
            return blocking(move || cache_size(&user)).map(Message::ShaderCacheSized);
        }
        Message::ShaderCacheSized(0) => modal(
            state,
            ModalKind::Info,
            tr(&lang, "The shader cache is already empty."),
        ),
        Message::ShaderCacheSized(bytes) => {
            let cache = cache_dir(&state.paths.user_dir(&state.settings.get_str("user_dir")));
            let body = tr_fmt(
                &lang,
                "Delete the shader cache ({} MB in {})? The game compiles the shaders again, so the next start stutters for a few minutes.",
                &[&megabytes(bytes).to_string(), &cache.display().to_string()],
            );
            let kind = ModalKind::YesNo {
                yes: Message::ClearShaderCacheConfirmed,
                no: Message::Modal(ModalMessage::Dismiss),
            };
            modal(state, kind, body);
        }
        Message::ClearShaderCacheConfirmed => {
            state.modal = None;
            let user = state.paths.user_dir(&state.settings.get_str("user_dir"));
            return blocking(move || clear_cache(&user)).map(Message::ShaderCacheCleared);
        }
        Message::ShaderCacheCleared(Ok(())) => modal(
            state,
            ModalKind::Info,
            tr(
                &lang,
                "Shader cache cleared. The next start stutters for a few minutes while it is rebuilt.",
            ),
        ),
        Message::ShaderCacheCleared(Err(cache)) => {
            let body = tr_fmt(
                &lang,
                "Could not delete all of the shader cache; another program may be using it: {}",
                &[&cache],
            );
            modal(state, ModalKind::Error, body);
        }
        Message::DesktopShortcut => {
            let paths = state.paths.clone();
            return blocking(move || make_shortcut(&paths)).map(Message::ShortcutDone);
        }
        Message::ShortcutDone(Ok(files)) => {
            let body = if files.is_empty() {
                tr(&lang, "Shortcut created on the desktop.")
            } else {
                format!("{}\n{}", tr(&lang, "Shortcut created:"), files.join("\n"))
            };
            modal(state, ModalKind::Info, body);
        }
        Message::ShortcutDone(Err(error)) => modal(state, ModalKind::Error, error),
        Message::AddToSteam => {
            // The shortcut starts `--play`, which reads the files: they get the window's settings
            // first (Python `add_to_steam`).
            if !persist::collect_unless_unreadable(state) {
                return Task::none();
            }
            let (paths, settings) = (state.paths.clone(), state.settings.clone());
            return blocking(move || add_to_steam(&paths, &settings, &lang))
                .map(Message::SteamDone);
        }
        Message::SteamDone(Ok(body)) => modal(state, ModalKind::Info, body),
        Message::SteamDone(Err(error)) => modal(state, ModalKind::Error, error),
        _ => {}
    }
    Task::none()
}
