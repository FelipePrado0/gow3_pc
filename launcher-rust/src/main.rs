// A windowed program on Windows: no console opens with the launcher. From a terminal it attaches
// to the terminal's console (`platform::attach_parent_console`).
#![windows_subsystem = "windows"]

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use spartanport_launcher::core::env::{environment_of, start_up_environment};
use spartanport_launcher::core::game::ready_to_play;
use spartanport_launcher::core::i18n::{launcher_language, system_language};
use spartanport_launcher::core::paths::{default_port_dir, Paths};
use spartanport_launcher::core::platform;
use spartanport_launcher::core::settings::load_app_settings;
use spartanport_launcher::core::voice_swap;
use spartanport_launcher::runner::{self, RunContext};
use spartanport_launcher::startup::{self, Route};
use spartanport_launcher::{ui, APP_NAME, VERSION};

fn main() {
    // First, so `--version`, the update installer and start-up errors print in a terminal (and
    // `--play`'s output: the Python's `attach_stdio`).
    platform::attach_parent_console();
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."));
    // A packaged port, or the checkout a cargo build sits in (`launcher-rust/target/…`).
    let port_dir = default_port_dir(&exe_dir);
    match startup::route(&args) {
        Route::Version => println!("{APP_NAME} {VERSION}"),
        Route::Run => {
            let paths = Paths::from_env(&port_dir, &|key| std::env::var(key).ok());
            // What the packaged Python launcher did on import, for its `--run` too.
            let mut env = environment_of(std::env::vars_os());
            start_up_environment(&mut env, cfg!(windows));
            let context = RunContext {
                port_dir: &paths.port_dir,
                env,
                // Everything after `--run`.
                args: args.iter().skip(1).cloned().collect(),
                out: &mut std::io::stdout(),
            };
            std::process::exit(runner::run(context))
        }
        Route::Play => {
            // As the Python `main`: the old name's files and the settings first.
            startup::forward_environment();
            let paths = Paths::from_env(&port_dir, &|key| std::env::var(key).ok());
            startup::prepare(&paths, startup::migrates(&args));
            let settings = load_app_settings(&paths);
            // Without a usable game folder there is nothing to play yet: the window opens instead.
            if ready_to_play(
                &settings.get_str("game_dir"),
                &settings.get_str("update_dir"),
            ) {
                let context = RunContext {
                    port_dir: &paths.port_dir,
                    env: environment_of(std::env::vars_os()),
                    // The Python's `--play` starts the runner without arguments.
                    args: Vec::new(),
                    out: &mut std::io::stdout(),
                };
                std::process::exit(runner::play(context, &settings, &paths));
            }
            open_window(paths);
        }
        Route::InstallUpdate => {
            let paths = Paths::from_env(&port_dir, &|key| std::env::var(key).ok());
            let target = PathBuf::from(&args[1]);
            let pid = args[2].to_str().and_then(|pid| pid.parse().ok());
            let lang = launcher_language(
                &load_app_settings(&paths).get_str("ui_language"),
                system_language,
            );
            std::process::exit(startup::install_update(
                &exe_dir,
                &target,
                pid.unwrap_or(0),
                &lang,
            ))
        }
        Route::VoiceSwap => {
            // As the AppImage runs from a mount, `OWD` is the folder the player typed paths in.
            let base = std::env::var_os("OWD");
            let code = voice_swap::main(
                &args[1..],
                base.as_deref().map(Path::new),
                &mut std::io::stdout(),
                &mut std::io::stderr(),
            );
            std::process::exit(code)
        }
        Route::Launcher => {
            // Before the folders below are read from the environment.
            startup::forward_environment();
            let paths = Paths::from_env(&port_dir, &|key| std::env::var(key).ok());
            startup::prepare(&paths, startup::migrates(&args));
            open_window(paths);
        }
    }
}

/// The launcher window; exit code 1 when it cannot open.
fn open_window(paths: Paths) {
    if let Err(error) = ui::app::run(paths) {
        eprintln!("{APP_NAME}: {error}");
        std::process::exit(1);
    }
}
