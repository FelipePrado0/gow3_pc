//! The release list and the install of any listed release (spec 5.6).
//!
//! Progress: the download runs on a worker thread that sends percentages through an `mpsc`
//! channel kept in `State::update_progress`; the app clock (`Message::Tick`) drains it with
//! [`drain`], as it does for the game's output.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Sender};

use iced::Task;

use crate::core::desktop::{host_environment, open_url};
use crate::core::i18n::{tr, tr_fmt};
use crate::core::platform;
use crate::core::release::{newer_stable, version_tuple, Release, ReleaseId};
use crate::core::update::{
    download, list_releases, relaunch_appimage, release_checksum, replace_appimage, update_dir,
};
use crate::ui::app::{Message, State};
use crate::ui::pages::game::restore_blocked;
use crate::ui::play::{self, WORKER_LOST};
use crate::ui::tasks::{blocking, blocking_or};
use crate::ui::widgets::{Modal, ModalKind, ModalMessage};
use crate::APP_NAME;

/// The file name of the packaged Windows launcher (ruling R47: a source tree is not packaged).
const PACKAGED_EXE: &str = "SpartanPort.exe";

/// Where the update stands.
#[derive(Debug, Clone, Default)]
pub enum UpdateState {
    #[default]
    None,
    /// A check is running.
    Checking,
    /// The releases of this system, newest first.
    Ready(Vec<Release>),
    /// `release`'s package is downloading; the list stays.
    Downloading {
        releases: Vec<Release>,
        release: Release,
        percent: u8,
    },
    Failed(String),
}

/// Update events.
#[derive(Debug, Clone)]
pub enum UpdateMessage {
    /// A check: the Check for updates button (`manual`) or a silent one.
    Check {
        manual: bool,
    },
    /// The answer of a check; `manual` is what the check was started as.
    Checked {
        manual: bool,
        result: Result<Vec<Release>, String>,
    },
    /// A listed release was picked.
    Install(Release),
    /// The dialog's Yes to a read-only AppImage folder: open this release page.
    OpenPage(String),
    /// The dialog's Yes to the install.
    Confirmed(Release),
    Progress(u8),
    Done(Result<(), String>),
}

/// What a finished check means for the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// A newer stable is listed.
    Offer,
    /// Manual check: nothing newer.
    Latest,
    Nothing,
}

/// The listed releases: those of `Ready` or `Downloading`, else none.
pub fn releases(state: &State) -> &[Release] {
    match &state.update {
        UpdateState::Ready(releases) | UpdateState::Downloading { releases, .. } => releases,
        _ => &[],
    }
}

/// The newest listed stable above the installed release.
pub fn newer(state: &State) -> Option<&Release> {
    newer_stable(releases(state), &state.installed)
}

/// Keeps `releases` as the list; a running download keeps its state.
pub fn ready(state: &mut State, releases: Vec<Release>) {
    if !matches!(state.update, UpdateState::Downloading { .. }) {
        state.update = UpdateState::Ready(releases);
    }
}

/// Whether `release`'s version is below the installed one.
pub fn is_older(release: &Release, installed: &ReleaseId) -> bool {
    version_tuple(&release.id.version) < version_tuple(&installed.version)
}

/// The install question, with a warning when `release` is older than the installed one.
pub fn confirm_text(lang: &str, release: &Release, installed: &ReleaseId) -> String {
    let mut text = tr_fmt(
        lang,
        "Install version {} now? The launcher closes, installs it and opens again. Saves and settings are kept.",
        &[&release.id.short()],
    );
    if is_older(release, installed) {
        text.push('\n');
        text.push_str(&tr(lang, "This version is older than the installed one."));
    }
    text
}

/// Python `check_update`'s `show`, over the list.
pub fn offer_decision(newer: bool, manual: bool) -> Decision {
    if newer {
        Decision::Offer
    } else if manual {
        Decision::Latest
    } else {
        Decision::Nothing
    }
}

/// What installing a release does (Python `install_update`, before the download).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallPlan {
    Blocked,
    /// Download by hand: open the release page.
    OpenPage,
    /// The AppImage's folder cannot be written; offer the page.
    ReadOnly(PathBuf),
    /// Ask, then download and install.
    Confirm,
}

/// Decides an install. `appimage` is the `APPIMAGE` path (Linux), `packaged` whether this is the
/// packaged Windows launcher, `writable` the folder probe.
pub fn install_decision(
    busy: bool,
    windows: bool,
    packaged: bool,
    appimage: Option<&Path>,
    writable: &dyn Fn(&Path) -> bool,
) -> InstallPlan {
    if busy {
        return InstallPlan::Blocked;
    }
    if windows {
        return if packaged {
            InstallPlan::Confirm
        } else {
            InstallPlan::OpenPage
        };
    }
    let Some(image) = appimage else {
        return InstallPlan::OpenPage;
    };
    let real = fs::canonicalize(image).unwrap_or_else(|_| image.to_path_buf());
    let folder = real.parent().map(Path::to_path_buf).unwrap_or_default();
    if writable(&folder) {
        InstallPlan::Confirm
    } else {
        InstallPlan::ReadOnly(folder)
    }
}

fn show(state: &mut State, kind: ModalKind, text: String) {
    state.modal = Some(Modal {
        kind,
        title: APP_NAME.to_string(),
        text,
    });
}

fn can_write(folder: &Path) -> bool {
    tempfile::NamedTempFile::new_in(folder).is_ok()
}

/// The Python's `FROZEN`: whether this is the packaged launcher (ruling R47).
pub fn is_packaged() -> bool {
    std::env::current_exe().is_ok_and(|exe| {
        exe.file_name()
            .is_some_and(|name| name.to_string_lossy().eq_ignore_ascii_case(PACKAGED_EXE))
    })
}

fn open_page(page: String) -> Task<Message> {
    // A failed open is ignored, as the Python launcher ignores it.
    blocking(move || {
        let _ = open_url(&page);
    })
    .map(|()| Message::Noop)
}

/// Applies an update event.
pub fn update(state: &mut State, message: UpdateMessage) -> Task<Message> {
    let lang = state.lang.clone();
    match message {
        UpdateMessage::Check { manual } => {
            if matches!(state.update, UpdateState::None | UpdateState::Failed(_)) {
                state.update = UpdateState::Checking;
            }
            blocking(|| list_releases(cfg!(windows)).map_err(|error| error.to_string()))
                .map(move |result| Message::Update(UpdateMessage::Checked { manual, result }))
        }
        UpdateMessage::Checked { manual, result } => {
            match result {
                Ok(releases) => {
                    ready(state, releases);
                    if offer_decision(newer(state).is_some(), manual) == Decision::Latest {
                        let text = tr_fmt(
                            &lang,
                            "You have the latest version ({}).",
                            &[&state.installed.version],
                        );
                        show(state, ModalKind::Info, text);
                    }
                }
                Err(error) => {
                    // A list already shown and a running download stay.
                    if matches!(
                        state.update,
                        UpdateState::None | UpdateState::Checking | UpdateState::Failed(_)
                    ) {
                        state.update = UpdateState::Failed(error.clone());
                    }
                    if manual {
                        let text = tr_fmt(&lang, "Could not check for updates: {}", &[&error]);
                        show(state, ModalKind::Error, text);
                    }
                }
            }
            Task::none()
        }
        UpdateMessage::Install(release) => {
            if matches!(state.update, UpdateState::Downloading { .. })
                || release.id.same_build(&state.installed)
            {
                return Task::none();
            }
            let appimage = if cfg!(windows) {
                None
            } else {
                std::env::var_os("APPIMAGE")
                    .filter(|path| !path.is_empty())
                    .map(PathBuf::from)
            };
            let plan = install_decision(
                restore_blocked(state),
                cfg!(windows),
                is_packaged(),
                appimage.as_deref(),
                &can_write,
            );
            match plan {
                InstallPlan::Blocked => {
                    let text = tr(&lang, "Close the game before updating.");
                    show(state, ModalKind::Info, text);
                    Task::none()
                }
                InstallPlan::OpenPage => open_page(release.page),
                InstallPlan::ReadOnly(folder) => {
                    let text = tr_fmt(
                        &lang,
                        "The launcher cannot write to {}, so it cannot replace itself there. Open the releases page to download the new version by hand?",
                        &[&folder.display().to_string()],
                    );
                    let kind = ModalKind::YesNo {
                        yes: Message::Update(UpdateMessage::OpenPage(release.page)),
                        no: Message::Modal(ModalMessage::Dismiss),
                    };
                    show(state, kind, text);
                    Task::none()
                }
                InstallPlan::Confirm => {
                    let text = confirm_text(&lang, &release, &state.installed);
                    let kind = ModalKind::YesNo {
                        yes: Message::Update(UpdateMessage::Confirmed(release)),
                        no: Message::Modal(ModalMessage::Dismiss),
                    };
                    show(state, kind, text);
                    Task::none()
                }
            }
        }
        UpdateMessage::OpenPage(page) => {
            state.modal = None;
            open_page(page)
        }
        UpdateMessage::Confirmed(release) => {
            state.modal = None;
            // The question may have been asked before the game started.
            if play::game_live(state) {
                show(
                    state,
                    ModalKind::Info,
                    tr(&lang, "Close the game before updating."),
                );
                return Task::none();
            }
            let url = release.url.clone();
            state.update = UpdateState::Downloading {
                releases: releases(state).to_vec(),
                release,
                percent: 0,
            };
            let (sender, receiver) = mpsc::channel();
            state.update_progress = Some(receiver);
            let port_dir = state.paths.port_dir.clone();
            // A download that dies still ends `Downloading`, so the list comes back.
            blocking_or(
                move || install(&url, &port_dir, &sender),
                Err(WORKER_LOST.to_string()),
            )
            .map(|result| Message::Update(UpdateMessage::Done(result)))
        }
        UpdateMessage::Progress(percent) => {
            if let UpdateState::Downloading { percent: shown, .. } = &mut state.update {
                *shown = percent;
            }
            Task::none()
        }
        UpdateMessage::Done(Ok(())) => iced::exit(),
        UpdateMessage::Done(Err(error)) => {
            state.update_progress = None;
            if let UpdateState::Downloading { releases, .. } = &mut state.update {
                state.update = UpdateState::Ready(std::mem::take(releases));
            }
            let text = tr_fmt(&lang, "Update failed: {}", &[&error]);
            show(state, ModalKind::Error, text);
            Task::none()
        }
    }
}

/// Applies the percentages the worker sent since the last clock tick.
pub fn drain(state: &mut State) {
    let Some(receiver) = &state.update_progress else {
        return;
    };
    let latest = receiver.try_iter().last();
    if let Some(percent) = latest {
        let _ = update(state, UpdateMessage::Progress(percent));
    }
}

/// The worker: downloads and installs, reporting the percent done.
fn install(url: &str, port_dir: &Path, progress: &Sender<u8>) -> Result<(), String> {
    let mut report = |percent: u8| {
        let _ = progress.send(percent);
    };
    if cfg!(windows) {
        install_windows(url, port_dir, &mut report)
    } else {
        install_appimage(url, &mut report)
    }
}

fn install_appimage(url: &str, report: &mut dyn FnMut(u8)) -> Result<(), String> {
    let appimage = std::env::var_os("APPIMAGE")
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| "APPIMAGE is not set".to_string())?;
    replace_appimage(url, &appimage, report).map_err(|error| error.to_string())?;
    relaunch_appimage(&appimage, &host_environment()).map_err(|error| error.to_string())
}

/// Python `work()`: the zip is fetched and unpacked, then the new launcher copies itself over
/// this installation once this one has closed. The published `.sha256` is checked when the
/// release has one (the Python launcher never checked it, spec 5.6 does). Unpacking uses the
/// `zip` crate's `extract`, which refuses names that leave the folder.
fn install_windows(url: &str, port_dir: &Path, report: &mut dyn FnMut(u8)) -> Result<(), String> {
    fn text(error: impl std::fmt::Display) -> String {
        error.to_string()
    }
    let dir = update_dir();
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).map_err(text)?;
    let archive = dir.join("update.zip");
    let expected = release_checksum(url).ok();
    download(url, &archive, expected.as_deref(), report).map_err(text)?;
    let new = dir.join("new");
    let file = fs::File::open(&archive).map_err(text)?;
    zip::ZipArchive::new(file)
        .and_then(|mut package| package.extract(&new))
        .map_err(text)?;
    let _ = fs::remove_file(&archive);
    let folder = find_launcher(&new)
        .ok_or_else(|| format!("{PACKAGED_EXE} is missing from the download"))?;
    let mut command = Command::new(folder.join(PACKAGED_EXE));
    command
        .arg("--install-update")
        .arg(port_dir)
        .arg(std::process::id().to_string())
        .current_dir(&folder)
        .stdin(Stdio::null());
    platform::prepare(&mut command);
    command.spawn().map(drop).map_err(text)
}

/// The folder under `root` that holds the launcher (Python `rglob`).
fn find_launcher(root: &Path) -> Option<PathBuf> {
    for entry in fs::read_dir(root).ok()?.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = find_launcher(&path) {
                return Some(found);
            }
        } else if entry
            .file_name()
            .to_string_lossy()
            .eq_ignore_ascii_case(PACKAGED_EXE)
        {
            return Some(root.to_path_buf());
        }
    }
    None
}
