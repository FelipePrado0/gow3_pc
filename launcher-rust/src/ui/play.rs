//! PLAY, STOP and the running game.
//!
//! The game's output reaches the window through the receiver of its `GameHandle`, which lives in
//! the state. The app clock (`Message::Tick`, every 33 or 100 ms, kept running while a game is
//! live) calls [`drain_events`] and applies what it returns, so the lines arrive in batches well
//! under 250 ms apart without a subscription over the receiver (which cannot be shared).

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::mpsc::TryRecvError;
use std::time::Duration;

use iced::{window, Task};
use indexmap::IndexMap;

use crate::core::env::{environment_of, game_environment};
use crate::core::game::{
    adopt_system_files, game_info, game_sfo, missing_game_files, same_folder, system_files_beside,
    FileState, REQUIRED_GAME_FILES, REQUIRED_UPDATE_FILES,
};
use crate::core::i18n::{tr, tr_fmt};
use crate::core::launch::{run_command, spawn_game, GameEvent, GameHandle, LaunchError};
use crate::core::log::{exit_hint, line_status, ExitHint, LineStatus, RunLog};
use crate::core::paths::Paths;
use crate::ui::app::{show_page, Message, State};
use crate::ui::home;
use crate::ui::pages::game::game_changed;
use crate::ui::pages::{log, trophies, Page};
use crate::ui::persist;
use crate::ui::tasks::{blocking, blocking_or};
use crate::ui::widgets::{Modal, ModalKind};
use crate::APP_NAME;

/// Why a copy or a download ended when its thread stopped without an answer.
pub const WORKER_LOST: &str = "the work stopped unexpectedly";
/// How long after the start `close_on_play` hides the window.
pub const HIDE_DELAY: Duration = Duration::from_secs(5);
/// The most game events one clock tick takes, as Python's `drain_output`; the rest waits.
pub const DRAIN_LIMIT: usize = 500;

/// The game process as the launcher sees it.
#[derive(Default)]
pub enum GameState {
    #[default]
    Idle,
    /// The system files of a PKG extraction are being copied; PLAY waits.
    Copying,
    /// Started; `run.py` prepares the game.
    Preparing {
        handle: GameHandle,
        /// Why the game may close, from its output.
        hint: Option<ExitHint>,
        /// Whether `close_on_play` hid the window.
        hidden: bool,
    },
    /// The game said it entered its own code.
    Running {
        handle: GameHandle,
        hint: Option<ExitHint>,
        hidden: bool,
    },
}

impl fmt::Debug for GameState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let started = |f: &mut fmt::Formatter<'_>, name, handle: &GameHandle, hint, hidden| {
            f.debug_struct(name)
                .field("pid", &handle.pid)
                .field("hint", hint)
                .field("hidden", hidden)
                .finish()
        };
        match self {
            GameState::Idle => f.write_str("Idle"),
            GameState::Copying => f.write_str("Copying"),
            GameState::Preparing {
                handle,
                hint,
                hidden,
            } => started(f, "Preparing", handle, hint, hidden),
            GameState::Running {
                handle,
                hint,
                hidden,
            } => started(f, "Running", handle, hint, hidden),
        }
    }
}

/// Game process events.
#[derive(Debug, Clone)]
pub enum GameMessage {
    /// Output pieces, normally whole lines with their `\n`.
    Output(Vec<String>),
    /// The game ended with this code.
    Exited(i32),
    /// The `close_on_play` timer of the game with this pid.
    HideForGame(u32),
    /// The answer to the copy offer: yes or no, the Sc0 folder, the game (or update) folder.
    AdoptAnswer(bool, PathBuf, String),
    /// The copy of the system files finished, or why it failed.
    AdoptDone(Result<(), String>),
}

/// The handle of a started game whose exit has not been applied yet.
fn live(game: &GameState) -> Option<&GameHandle> {
    match game {
        GameState::Preparing { handle, .. } | GameState::Running { handle, .. } => Some(handle),
        GameState::Idle | GameState::Copying => None,
    }
}

/// Whether a game runs: the clock keeps draining its output while this holds.
pub fn game_live(state: &State) -> bool {
    live(&state.game).is_some()
}

/// Whether the launcher closes once the running game exits: `close_on_play` hid its window.
pub fn exit_after_game(state: &State) -> bool {
    match &state.game {
        GameState::Preparing { hidden, .. } | GameState::Running { hidden, .. } => *hidden,
        GameState::Idle | GameState::Copying => false,
    }
}

/// The entries of `missing_game_files` as shown to the player (Python `describe_missing`).
pub fn describe_missing(lang: &str, missing: &[(String, FileState)]) -> Vec<String> {
    missing
        .iter()
        .map(|(name, state)| {
            let label = match state {
                FileState::Empty => "{} (empty)",
                FileState::Unreadable => "{} (unreadable)",
                FileState::Invalid => "{} (not a param.sfo)",
                FileState::Folder => "{} (a folder, not a file)",
                FileState::Missing | FileState::Ok => return name.clone(),
            };
            tr_fmt(lang, label, &[name])
        })
        .collect()
}

fn show_error(state: &mut State, text: String) {
    state.modal = Some(Modal {
        kind: ModalKind::Error,
        title: APP_NAME.to_string(),
        text,
    });
}

/// The error dialog over the Game page, where the folders are chosen.
fn refuse(state: &mut State, text: String) -> Task<Message> {
    show_error(state, text);
    show_page(state, Page::Game)
}

fn folder_or_here(folder: &str) -> &str {
    if folder.is_empty() {
        "."
    } else {
        folder
    }
}

/// The first folder (the game, then an update with an `eboot.bin`) whose system files sit in an
/// Sc0 folder beside it, with that folder (Python `offer_system_files`). The folders up to
/// `declined`, whose offer was just answered No, are passed over, as Python's loop goes on.
fn system_files_offer(
    game_dir: &str,
    update_dir: &str,
    declined: Option<&str>,
) -> Option<(String, PathBuf)> {
    let game = folder_or_here(game_dir);
    let mut folders = vec![(game, false)];
    if !update_dir.is_empty()
        && !same_folder(update_dir, game)
        && Path::new(update_dir).join("eboot.bin").is_file()
    {
        folders.push((update_dir, true));
    }
    let after = declined
        .and_then(|declined| folders.iter().position(|(folder, _)| *folder == declined))
        .map_or(0, |at| at + 1);
    folders
        .into_iter()
        .skip(after)
        .find_map(|(folder, is_update)| {
            // The update's candidate must be the base game's title.
            let reference = is_update
                .then(|| game_sfo(game))
                .filter(|values| !values.is_empty());
            let base = is_update.then(|| Path::new(game));
            system_files_beside(folder, reference.as_ref(), base)
                .map(|source| (folder.to_string(), source))
        })
}

/// PLAY (Python `play`): saves the settings, checks the folders in the 2.6.0 order and starts
/// the game. Nothing happens while a game runs or system files are copied.
pub fn play(state: &mut State) -> Task<Message> {
    if !matches!(state.game, GameState::Idle) {
        return Task::none();
    }
    if !persist::collect_or_report(state) {
        return Task::none();
    }
    if game_info(&state.settings.get_str("game_dir")).is_none() {
        let text = tr(
            &state.lang,
            "Choose the game folder with eboot.bin (CUSA01623).",
        );
        return refuse(state, text);
    }
    checks(state, None)
}

/// PLAY from the copy offers on: the offers, the files of the game and of the update, then the
/// start. `declined` is the folder whose offer was just answered No.
fn checks(state: &mut State, declined: Option<&str>) -> Task<Message> {
    let lang = state.lang.clone();
    let game_dir = state.settings.get_str("game_dir");
    let update_dir = state.settings.get_str("update_dir");
    if let Some((folder, source)) = system_files_offer(&game_dir, &update_dir, declined) {
        let text = tr_fmt(
            &lang,
            "The files in {} look like a PKG extraction: the system files (param.sfo, icon0.png, \
             pic1.png and the rest) are in\n\n{}\n\nCopy all of them into its sce_sys folder now? \
             Files already there are kept.",
            &[&folder, &source.display().to_string()],
        );
        state.modal = Some(Modal {
            kind: ModalKind::YesNo {
                yes: Message::Game(GameMessage::AdoptAnswer(
                    true,
                    source.clone(),
                    folder.clone(),
                )),
                no: Message::Game(GameMessage::AdoptAnswer(false, source, folder)),
            },
            title: APP_NAME.to_string(),
            text,
        });
        return Task::none();
    }
    let missing = missing_game_files(&game_dir, REQUIRED_GAME_FILES);
    if !missing.is_empty() {
        let text = tr_fmt(
            &lang,
            "The game dump is incomplete. Missing or empty files:\n\n{}\n\nDump the game again, \
             or copy these files from a complete dump.",
            &[&describe_missing(&lang, &missing).join("\n")],
        );
        return refuse(state, text);
    }
    if !update_dir.is_empty() && same_folder(&update_dir, folder_or_here(&game_dir)) {
        let text = tr(
            &lang,
            "Choose two separate folders for the game and its update, or leave the update \
             folder empty.",
        );
        return refuse(state, text);
    }
    let missing = if update_dir.is_empty() {
        Vec::new()
    } else {
        missing_game_files(&update_dir, REQUIRED_UPDATE_FILES)
    };
    if !missing.is_empty() {
        let text = tr_fmt(
            &lang,
            "The update folder {} is incomplete. Missing or empty files:\n\n{}\n\nChoose the \
             folder of the update (eboot.bin, sce_sys), or leave it empty.",
            &[&update_dir, &describe_missing(&lang, &missing).join("\n")],
        );
        return refuse(state, text);
    }
    // An explicit command (the tests' fake game) replaces the runner and needs none of its files.
    if state.launch_command.is_none() {
        let missing = missing_port_files(&state.paths);
        if !missing.is_empty() {
            let mut text = tr_fmt(
                &lang,
                "The port folder is incomplete. Missing from {}:\n\n{}\n\nDownload the package \
                 again from the releases page and extract all of it; the launcher must stay \
                 inside that folder.",
                &[
                    &state.paths.port_dir.display().to_string(),
                    &missing.join("\n"),
                ],
            );
            if state.paths.port_dir.join("build.sh").is_file() {
                text.push_str("\n\n");
                text.push_str(&tr(&lang, "From a source checkout, build.sh builds them."));
            }
            return refuse(state, text);
        }
    }
    start(state)
}

/// What the runner needs from the port folder and does not find: the probe it starts (`bin/` in
/// a package, `out/` in a checkout). An exe moved out of its package or a package half extracted
/// lack it; PLAY says so instead of failing inside the runner.
pub fn missing_port_files(paths: &Paths) -> Vec<String> {
    let mut missing = Vec::new();
    if paths.find_tool("spartanport-probe").is_none() {
        let file = if cfg!(windows) {
            "spartanport-probe.exe"
        } else {
            "spartanport-probe"
        };
        missing.push(Path::new("bin").join(file).display().to_string());
    }
    missing
}

/// The text of a failed start, as Python prints the `OSError`.
fn launch_error_text(error: &LaunchError) -> String {
    match error {
        LaunchError::Spawn(io) => io.to_string(),
        other => other.to_string(),
    }
}

/// The game's environment: the launcher's whole environment (Python `dict(os.environ)`, so the
/// variables of a bundle, `LD_LIBRARY_PATH` included, reach it) plus what the settings ask for.
pub fn launch_environment(state: &State) -> IndexMap<String, String> {
    game_environment(
        &state.settings,
        &state.paths,
        &state.lang,
        &environment_of(std::env::vars_os()),
        cfg!(windows),
    )
}

/// Starts the game with a fresh log and `last_run.log` (the saves backup is `run.py`'s).
fn start(state: &mut State) -> Task<Message> {
    let lang = state.lang.clone();
    state.log.clear();
    let run_log = RunLog::open(
        &state
            .paths
            .run_log_path(&state.settings.get_str("user_dir")),
    );
    let env = launch_environment(state);
    let command = match &state.launch_command {
        Some(command) => Ok(command.clone()),
        None => state
            .launcher_exe
            .clone()
            .map_or_else(std::env::current_exe, Ok)
            .map(|exe| run_command(&exe, &[])),
    };
    let started = command
        .map_err(LaunchError::from)
        .and_then(|command| spawn_game(&command, &state.paths.port_dir, &env, run_log));
    let handle = match started {
        Ok(handle) => handle,
        Err(error) => {
            let text = tr_fmt(&lang, "Could not start: {}", &[&launch_error_text(&error)]);
            state.log.push_text(&(text + "\n"));
            return log::scroll_to_end();
        }
    };
    let pid = handle.pid;
    state.game = GameState::Preparing {
        handle,
        hint: None,
        hidden: false,
    };
    state.status = tr(&lang, "Preparing the game; it opens in its own window…");
    if state.settings.get_bool("close_on_play") {
        return blocking(|| std::thread::sleep(HIDE_DELAY))
            .map(move |()| Message::Game(GameMessage::HideForGame(pid)));
    }
    Task::none()
}

/// STOP: ends the game and everything it started; `Exited` follows from its output reader.
pub fn stop(state: &mut State) {
    if let GameState::Preparing { handle, .. } | GameState::Running { handle, .. } = &mut state.game
    {
        handle.terminate();
    }
}

/// What the running game reported since the last call, at most `DRAIN_LIMIT` events and without
/// waiting: consecutive output in one `Output`, then `Exited` when the game ended. A reader that
/// vanished without reporting the exit counts as an exit with code -1, so PLAY comes back.
pub fn drain_events(state: &State) -> Vec<GameMessage> {
    let Some(handle) = live(&state.game) else {
        return Vec::new();
    };
    let mut messages = Vec::new();
    let mut lines = Vec::new();
    let mut exited = None;
    for _ in 0..DRAIN_LIMIT {
        match handle.events.try_recv() {
            Ok(GameEvent::Output(text)) => lines.push(text),
            Ok(GameEvent::Exited(code)) => exited = Some(code),
            Err(TryRecvError::Empty) => break,
            Err(TryRecvError::Disconnected) => exited = Some(-1),
        }
        if exited.is_some() {
            break;
        }
    }
    if !lines.is_empty() {
        messages.push(GameMessage::Output(lines));
    }
    if let Some(code) = exited {
        messages.push(GameMessage::Exited(code));
    }
    messages
}

/// Applies the pending game events, as the clock does.
pub fn drain(state: &mut State) -> Task<Message> {
    let tasks: Vec<_> = drain_events(state)
        .into_iter()
        .map(|message| update(state, message))
        .collect();
    Task::batch(tasks)
}

/// Applies a game event.
pub fn update(state: &mut State, message: GameMessage) -> Task<Message> {
    match message {
        GameMessage::Output(lines) => output(state, lines),
        GameMessage::Exited(code) => exited(state, code),
        GameMessage::HideForGame(pid) => hide_for_game(state, pid),
        GameMessage::AdoptAnswer(yes, source, folder) => {
            state.modal = None;
            if !matches!(state.game, GameState::Idle) {
                return Task::none();
            }
            // No: PLAY goes on with the next offer and the checks, as Python's loop does.
            if !yes {
                return checks(state, Some(&folder));
            }
            state.game = GameState::Copying;
            state.status = tr(&state.lang, "Copying the system files…");
            // A copy that dies still ends `Copying`, so PLAY comes back.
            blocking_or(
                move || {
                    adopt_system_files(&source, Path::new(&folder))
                        .map(drop)
                        .map_err(|error| error.to_string())
                },
                Err(WORKER_LOST.to_string()),
            )
            .map(|result| Message::Game(GameMessage::AdoptDone(result)))
        }
        GameMessage::AdoptDone(result) => {
            if matches!(state.game, GameState::Copying) {
                state.game = GameState::Idle;
            }
            state.status.clear();
            // Whatever was copied already shows.
            let changed = game_changed(state);
            let next = match result {
                // PLAY again: the next offer or check follows from what is on disk.
                Ok(()) => play(state),
                Err(error) => {
                    let text = tr_fmt(&state.lang, "Could not copy the files: {}", &[&error]);
                    refuse(state, text)
                }
            };
            Task::batch([changed, next])
        }
    }
}

/// New output: the log, the exit advice and the status line follow it.
fn output(state: &mut State, lines: Vec<String>) -> Task<Message> {
    for line in &lines {
        state.log.push_text(line);
        let found = exit_hint(line);
        match line_status(line) {
            Some(LineStatus::Running) => {
                // Already Running after an in-game restart: the game stays as it is.
                state.game = match std::mem::take(&mut state.game) {
                    GameState::Preparing {
                        handle,
                        hint,
                        hidden,
                    } => GameState::Running {
                        handle,
                        hint,
                        hidden,
                    },
                    other => other,
                };
                state.status = tr(&state.lang, "The game is running.");
            }
            Some(LineStatus::Restarting) => {
                state.status = tr(&state.lang, "Restarting with the new settings…");
            }
            None => {}
        }
        if let (Some(found), GameState::Preparing { hint, .. } | GameState::Running { hint, .. }) =
            (found, &mut state.game)
        {
            *hint = Some(found);
        }
    }
    log::scroll_to_end()
}

/// The game ended: the exit line and its advice, PLAY back, and the Home chips and trophies read
/// again; a launcher hidden for the game closes instead.
fn exited(state: &mut State, code: i32) -> Task<Message> {
    let exit = exit_after_game(state);
    let (mut handle, hint) = match std::mem::take(&mut state.game) {
        GameState::Preparing { handle, hint, .. } | GameState::Running { handle, hint, .. } => {
            (handle, hint)
        }
        other => {
            state.game = other;
            return Task::none();
        }
    };
    handle.close();
    let lang = state.lang.clone();
    state.log.push_text(&tr_fmt(
        &lang,
        "\nThe game exited (code {}).\n",
        &[&code.to_string()],
    ));
    if let Some(hint) = hint {
        state.log.push_text(&(tr(&lang, hint.text_en()) + "\n"));
    }
    state.status.clear();
    if exit && hint.is_none() {
        return iced::exit();
    }
    home::refresh_facts(state);
    let mut tasks = vec![log::scroll_to_end(), trophies::refresh(state)];
    if hint.is_some() {
        // The advice is read on the Log page (the Python's `show_log`), and a window hidden for
        // the game comes back to show it instead of closing.
        tasks.push(show_page(state, Page::Log));
        if exit {
            state.visible = true;
            tasks
                .push(window::latest().and_then(|id| window::set_mode(id, window::Mode::Windowed)));
        }
    }
    Task::batch(tasks)
}

/// The `close_on_play` timer: hides the window when the game it was set for still runs. An early
/// failure keeps its error visible, and the timer of an earlier start does nothing.
fn hide_for_game(state: &mut State, pid: u32) -> Task<Message> {
    // What the game reported so far first, so a game that already ended is seen as ended.
    let pending = drain(state);
    match &mut state.game {
        GameState::Preparing { handle, hidden, .. } | GameState::Running { handle, hidden, .. }
            if handle.pid == pid =>
        {
            *hidden = true;
            state.visible = false;
            let hide = window::latest().and_then(|id| window::set_mode(id, window::Mode::Hidden));
            Task::batch([pending, hide])
        }
        _ => pending,
    }
}
