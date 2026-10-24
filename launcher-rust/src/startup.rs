//! The program's start (Python `main`): what the arguments ask for, and the steps that run before
//! the window opens. None of these steps stops the launcher.

use std::ffi::OsStr;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

use indexmap::IndexMap;

use crate::core::env::start_up_environment;
use crate::core::i18n::tr;
use crate::core::paths::Paths;
use crate::core::settings::load_app_settings;
use crate::core::update::{self, update_dir};
use crate::core::{desktop, legacy, platform};
use crate::ui::update::is_packaged;
use crate::APP_NAME;

/// What the command line asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    /// `--version`: the name and the version.
    Version,
    /// `--run`: the Rust runner.
    Run,
    /// `--play`: the Rust runner's play role.
    Play,
    /// `--install-update TARGET PID`: the downloaded version installs itself over the old one.
    InstallUpdate,
    /// `--voice-swap`: the voice-line tool.
    VoiceSwap,
    /// Anything else: the window.
    Launcher,
}

/// [`route_on`] for the system this program runs on.
pub fn route<S: AsRef<OsStr>>(args: &[S]) -> Route {
    route_on(args, cfg!(windows))
}

/// `--run`, `--voice-swap` and `--install-update` count first (what follows them is theirs); `--install-update`
/// needs exactly `TARGET PID` and Windows, the only system that updates itself this way. `--play`
/// counts anywhere, as in the Python `main`. Other arguments, `--script` included, are ignored.
pub fn route_on<S: AsRef<OsStr>>(args: &[S], windows: bool) -> Route {
    let is = |arg: &S, flag: &str| arg.as_ref() == OsStr::new(flag);
    match args.first() {
        Some(first) if is(first, "--run") => return Route::Run,
        Some(first) if is(first, "--voice-swap") => return Route::VoiceSwap,
        Some(first) if is(first, "--install-update") && windows && args.len() == 3 => {
            return Route::InstallUpdate;
        }
        _ => {}
    }
    if args.iter().any(|arg| is(arg, "--version")) {
        return Route::Version;
    }
    if args.iter().any(|arg| is(arg, "--play")) {
        return Route::Play;
    }
    Route::Launcher
}

/// Whether [`prepare`] moves the old name's files: not for the downloaded copy that installs an
/// update, which leaves the player's folders alone.
pub fn migrates<S: AsRef<OsStr>>(args: &[S]) -> bool {
    !args
        .first()
        .is_some_and(|first| first.as_ref() == OsStr::new("--install-update"))
}

/// Python `install_update`: waits for the old launcher (`pid`) to close, copies this version
/// (`source`) over `target` and starts it. Returns the exit code.
pub fn install_update(source: &Path, target: &Path, pid: u32, lang: &str) -> i32 {
    platform::wait_for_process(pid, Duration::from_secs(60));
    let mut pause = || std::thread::sleep(Duration::from_secs(1));
    if update::install_over(source, target, 30, &mut pause).is_err() {
        platform::show_error(
            APP_NAME,
            &tr(
                lang,
                "Could not install the update. Download it from the releases page.",
            ),
        );
        let _ = desktop::open_url(update::RELEASES_PAGE);
        return 1;
    }
    let started = Command::new(target.join("SpartanPort.exe"))
        .current_dir(target)
        .stdin(Stdio::null())
        .spawn();
    match started {
        Ok(_) => 0,
        Err(error) => {
            platform::show_error(APP_NAME, &error.to_string());
            1
        }
    }
}

/// `env::start_up_environment` on this process's environment, before anything reads it (the
/// Python does it on import). Runs while the program has a single thread.
pub fn forward_environment() {
    let mut env: IndexMap<String, String> = std::env::vars_os()
        .filter_map(|(key, value)| Some((key.into_string().ok()?, value.into_string().ok()?)))
        .collect();
    let before = env.clone();
    start_up_environment(&mut env, cfg!(windows));
    for (key, value) in env {
        if before.get(&key) != Some(&value) {
            std::env::set_var(key, value);
        }
    }
}

/// The steps between the arguments and the window, in the Python `main`'s order: the old name's
/// files, the settings, the Linux menu entry and what a finished update left behind. The
/// language is the window's (`State::new`).
pub fn prepare(paths: &Paths, migrate: bool) {
    // Python `WINDOWS and FROZEN and UPDATE_DIR not in PORT_DIR.parents`: the installed Windows
    // launcher, not the copy that runs from a downloaded update.
    let installed = cfg!(windows) && is_packaged() && !runs_from_update_dir(&paths.port_dir);
    // Before the settings are loaded: their copy must not mask the old data folder's.
    if migrate {
        move_from_legacy_name(paths, installed);
    }
    let settings = load_app_settings(paths);
    if !cfg!(windows) {
        let env = |key: &str| std::env::var(key).ok();
        let appimage = env("APPIMAGE");
        let icon = paths.port_dir.join("launcher").join("spartanport.png");
        desktop::integrate_desktop(
            settings.get_bool("desktop_integration"),
            appimage.as_deref(),
            &icon,
            &paths.data_dir,
            None,
            &env,
        );
    }
    if installed {
        // In use or already gone: tried again at the next start.
        let _ = std::fs::remove_dir_all(update_dir());
    }
}

/// Python `move_from_legacy_name`: settings, saves and shortcuts from before the rename; the
/// installed Windows launcher (`installed`) also cleans its folder and desktop shortcut. A failed
/// copy is tried again at the next start.
fn move_from_legacy_name(paths: &Paths, installed: bool) {
    for line in legacy::migrate(&paths.data_dir, None, cfg!(windows)) {
        println!("Rename: {line}");
    }
    if installed {
        legacy::remove_leftovers(&paths.port_dir);
        legacy::remove_python_leftovers(&paths.port_dir);
        legacy::retarget_desktop_shortcut(&paths.port_dir, None, Some(&paths.data_dir));
    }
}

/// Python `UPDATE_DIR in PORT_DIR.parents`: this copy runs from a downloaded update. Both folders
/// are compared in their canonical form, so a short (8.3) temporary folder still matches.
fn runs_from_update_dir(port_dir: &Path) -> bool {
    let port = port_dir
        .canonicalize()
        .unwrap_or_else(|_| port_dir.to_path_buf());
    let update = update_dir();
    let update = update.canonicalize().unwrap_or(update);
    port != update && port.starts_with(&update)
}
