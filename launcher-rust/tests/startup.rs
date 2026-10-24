#![allow(clippy::unwrap_used, clippy::expect_used)]
//! The program's start: what the arguments ask for, the first language and the window.

use std::path::Path;

use iced::{window, Point, Size};
use spartanport_launcher::core::i18n::launcher_language;
use spartanport_launcher::core::paths::Paths;
use spartanport_launcher::core::settings::app_defaults;
use spartanport_launcher::startup::{migrates, route, route_on, Route};
use spartanport_launcher::ui::app::{
    self, centred_position, fitted_window, window_message, Message, State, WINDOW_SIZE,
};
use spartanport_launcher::ui::theme::window_zoom;

#[test]
fn arguments_route_to_their_roles() {
    let none: [&str; 0] = [];
    assert_eq!(route(&none), Route::Launcher);
    assert_eq!(route(&["--version"]), Route::Version);
    // The update installer: `--install-update TARGET PID`, on Windows only.
    let update = ["--install-update", r"C:\Port", "1234"];
    assert_eq!(route_on(&update, true), Route::InstallUpdate);
    assert_eq!(route_on(&update, false), Route::Launcher);
    assert_eq!(
        route_on(&["--install-update", "a.zip"], true),
        Route::Launcher
    );
    assert_eq!(
        route_on(&["--install-update", "a", "1", "b"], true),
        Route::Launcher
    );
    // `--script` is an ordinary argument now.
    assert_eq!(route(&["--script", "prepare.py"]), Route::Launcher);
    assert_eq!(route(&["--script", "x.py", "--version"]), Route::Version);
    // Anything else opens the window; the Python ignores unknown arguments too.
    assert_eq!(route(&["--fullscreen"]), Route::Launcher);
    assert_eq!(route(&["play"]), Route::Launcher);
}

#[test]
fn run_and_play_are_their_own_routes() {
    assert_eq!(route(&["--run"]), Route::Run);
    assert_eq!(route(&["--run", "--timeout", "5"]), Route::Run);
    // `--play` counts anywhere, as in the Python `main`.
    assert_eq!(route(&["--play"]), Route::Play);
    assert_eq!(route(&["--fullscreen", "--play"]), Route::Play);
}

#[test]
fn the_update_installer_does_not_migrate() {
    assert!(!migrates(&["--install-update", r"C:\P", "1"]));
    assert!(migrates(&["--play"]));
    assert!(migrates::<&str>(&[]));
}

#[cfg(windows)]
#[test]
fn install_update_copies_and_starts_the_new_launcher() {
    use spartanport_launcher::startup::install_update;
    let dir = tempfile::tempdir().unwrap();
    let (source, target) = (dir.path().join("source"), dir.path().join("target"));
    std::fs::create_dir_all(source.join("bin")).unwrap();
    std::fs::create_dir_all(&target).unwrap();
    std::fs::copy(
        env!("CARGO_BIN_EXE_fake-game"),
        source.join("SpartanPort.exe"),
    )
    .unwrap();
    std::fs::write(source.join("bin").join("x"), "new").unwrap();
    std::fs::write(target.join("settings.json"), "mine").unwrap();
    // A process that has already ended: waiting for it returns at once.
    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_fake-game"))
        .spawn()
        .unwrap();
    let pid = child.id();
    child.wait().unwrap();
    assert_eq!(install_update(&source, &target, pid, "en"), 0);
    assert!(target.join("bin").join("x").exists());
    assert_eq!(
        std::fs::read_to_string(target.join("settings.json")).unwrap(),
        "mine"
    );
}

#[test]
fn run_without_a_probe_fails_with_the_python_message() {
    let dir = tempfile::tempdir().unwrap();
    let out = std::process::Command::new(LAUNCHER)
        .arg("--run")
        .env("SPARTAN_PORT_DIR", dir.path())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("spartanport-probe"), "{stdout}");
    assert!(
        stdout.contains("not found: build it with build.sh"),
        "{stdout}"
    );
}

/// The launcher program cargo built for these tests.
const LAUNCHER: &str = env!("CARGO_BIN_EXE_spartanport-launcher");

#[test]
fn version_prints_the_name_and_the_version() {
    let out = std::process::Command::new(LAUNCHER)
        .arg("--version")
        .output()
        .unwrap();
    assert!(out.status.success());
    // The version itself is pinned to the Python launcher's in `core_boundary.rs`.
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        format!("SpartanPort {}", spartanport_launcher::VERSION)
    );
}

/// No console window opens with the launcher: Windows starts it as a windowed program
/// (`IMAGE_SUBSYSTEM_WINDOWS_GUI` in the PE optional header).
#[cfg(windows)]
#[test]
fn the_launcher_is_a_windowed_program() {
    let exe = std::fs::read(LAUNCHER).unwrap();
    let word = |at: usize| u16::from_le_bytes([exe[at], exe[at + 1]]);
    let pe = u32::from_le_bytes([exe[0x3c], exe[0x3d], exe[0x3e], exe[0x3f]]) as usize;
    assert_eq!(&exe[pe..pe + 4], b"PE\0\0");
    // The PE signature (4 bytes) and the file header (20) come first; Subsystem is at 68.
    assert_eq!(word(pe + 24 + 68), 2, "IMAGE_SUBSYSTEM_WINDOWS_GUI");
}

/// Explorer, the taskbar and a shortcut to the exe show its icon: `build.rs` embeds
/// `assets/spartanport.ico` as an `RT_GROUP_ICON` resource (R92, M14).
#[cfg(windows)]
#[test]
fn the_launcher_carries_its_icon() {
    let exe = std::fs::read(LAUNCHER).unwrap();
    let u16_at = |at: usize| u16::from_le_bytes([exe[at], exe[at + 1]]) as usize;
    let u32_at =
        |at: usize| u32::from_le_bytes([exe[at], exe[at + 1], exe[at + 2], exe[at + 3]]) as usize;
    let pe = u32_at(0x3c);
    let sections = u16_at(pe + 6);
    let optional = pe + 24;
    assert_eq!(u16_at(optional), 0x20b, "PE32+");
    // The resource table is the third data directory of the PE32+ optional header.
    let resources = u32_at(optional + 112 + 2 * 8);
    assert_ne!(resources, 0, "no resource table");
    let table = optional + u16_at(pe + 20);
    let offset = (0..sections)
        .map(|index| table + index * 40)
        .find_map(|section| {
            let (size, address, raw) = (
                u32_at(section + 8),
                u32_at(section + 12),
                u32_at(section + 20),
            );
            (address..address + size)
                .contains(&resources)
                .then(|| resources - address + raw)
        })
        .unwrap();
    // The root directory's entries name the resource types: 14 is RT_GROUP_ICON.
    let named = u16_at(offset + 12);
    let ids = u16_at(offset + 14);
    let types: Vec<usize> = (named..named + ids)
        .map(|entry| u32_at(offset + 16 + entry * 8))
        .collect();
    assert!(types.contains(&14), "resource types {types:?}");
}

#[test]
fn first_start_is_in_english_when_the_system_language_is_unknown() {
    // The translations are partial; 'System' stays a choice in Advanced.
    let defaults = app_defaults(&|_| None);
    let ui_language = defaults["ui_language"].as_str().unwrap();
    assert_eq!(ui_language, "en");
    assert_eq!(launcher_language(ui_language, || "xx".to_string()), "en");
    // 'System' (empty) follows the system language.
    assert_eq!(launcher_language("", || "pt".to_string()), "pt");
}

#[test]
fn the_window_zoom_leaves_out_the_desktop_scale() {
    // iced already draws at the desktop's scale; the zoom adds only what GUI Scale asks beyond it.
    assert_eq!(window_zoom(1.5, 1.5), 1.0);
    assert_eq!(window_zoom(1.0, 2.0), 0.5);
    assert!((window_zoom(2.0, 1.5) - 4.0 / 3.0).abs() < 1e-6);
    assert_eq!(window_zoom(2.0, 0.0), 2.0);
}

#[test]
fn the_window_fits_in_90_percent_of_the_monitor() {
    assert_eq!(fitted_window(None), WINDOW_SIZE);
    assert_eq!(fitted_window(Some(Size::new(1920.0, 1080.0))), WINDOW_SIZE);
    assert_eq!(
        fitted_window(Some(Size::new(1366.0, 768.0))),
        Size::new(1229.0, 691.0)
    );
    // A monitor without a size (Wayland: an output with no current mode) counts as unknown.
    for unknown in [
        Size::ZERO,
        Size::new(-1920.0, 1080.0),
        Size::new(1920.0, 0.0),
        Size::new(f32::NAN, 1080.0),
        Size::new(f32::INFINITY, 1080.0),
    ] {
        assert_eq!(fitted_window(Some(unknown)), WINDOW_SIZE, "{unknown:?}");
    }
    // Centred, in the desktop's units: at zoom 2 a logical pixel is two of them.
    let monitor = Size::new(1920.0, 1080.0);
    assert_eq!(
        centred_position(monitor, WINDOW_SIZE, 1.0),
        Point::new(320.0, 140.0)
    );
    let monitor = Size::new(960.0, 540.0);
    let size = fitted_window(Some(monitor));
    assert_eq!(size, Size::new(864.0, 486.0));
    assert_eq!(centred_position(monitor, size, 2.0), Point::new(96.0, 54.0));
}

#[test]
fn window_events_become_messages() {
    let id = window::Id::unique();
    assert!(matches!(
        window_message((id, window::Event::Focused)),
        Some(Message::WindowFocused(true))
    ));
    assert!(matches!(
        window_message((id, window::Event::Unfocused)),
        Some(Message::WindowFocused(false))
    ));
    let opened = window::Event::Opened {
        position: None,
        size: Size::new(1000.0, 700.0),
    };
    assert!(matches!(
        window_message((id, opened)),
        Some(Message::WindowOpened(at, size)) if at == id && size == Size::new(1000.0, 700.0)
    ));
    assert!(matches!(
        window_message((id, window::Event::Resized(Size::new(1300.0, 820.0)))),
        Some(Message::Resized(_))
    ));
    assert!(matches!(
        window_message((id, window::Event::Rescaled(1.25))),
        Some(Message::DesktopRescaled(factor)) if factor == 1.25
    ));
    assert!(window_message((id, window::Event::CloseRequested)).is_none());
}

/// A state over `dir` only, with `settings` as its `settings.json`.
fn state_with(dir: &Path, settings: &str) -> State {
    std::fs::write(dir.join("settings.json"), settings).unwrap();
    let root = dir.display().to_string();
    let env = move |key: &str| match key {
        "APPDATA" | "USERPROFILE" | "HOME" | "XDG_CONFIG_HOME" => Some(root.clone()),
        _ => None,
    };
    State::new(Paths::from_env(dir, &env))
}

#[test]
fn a_new_desktop_scale_keeps_the_chosen_zoom() {
    let dir = tempfile::tempdir().unwrap();
    // GUI Scale 1 on a 150 % laptop, then the window is dragged to a 100 % monitor.
    let mut state = state_with(dir.path(), r#"{"ui_scale": "1"}"#);
    let id = window::Id::unique();
    let _ = app::update(&mut state, Message::WindowOpened(id, WINDOW_SIZE));
    let _ = app::update(&mut state, Message::DesktopScale(1.5));
    assert!((window_zoom(state.scale, state.desktop_scale) - 1.0 / 1.5).abs() < 1e-6);
    let _ = app::update(&mut state, Message::DesktopRescaled(1.0));
    assert_eq!((state.desktop_scale, state.scale), (1.0, 1.0));
    assert_eq!(window_zoom(state.scale, state.desktop_scale), 1.0);
    // The window keeps its place and size: only a new zoom, no refit.
    assert_eq!(state.window_size, WINDOW_SIZE);

    // GUI Scale 1.5 (R90) stays 1.5 screen pixels per logical pixel on either desktop.
    let dir = tempfile::tempdir().unwrap();
    let mut state = state_with(dir.path(), r#"{"ui_scale": "1.5"}"#);
    let _ = app::update(&mut state, Message::WindowOpened(id, WINDOW_SIZE));
    let _ = app::update(&mut state, Message::DesktopScale(1.5));
    assert_eq!(state.scale, 1.5);
    assert_eq!(window_zoom(state.scale, state.desktop_scale), 1.0);
    let _ = app::update(&mut state, Message::DesktopRescaled(1.0));
    assert_eq!((state.desktop_scale, state.scale), (1.0, 1.5));
    assert_eq!(window_zoom(state.scale, state.desktop_scale), 1.5);

    // A file from before R90: "auto" no longer follows the desktop, it is 1x; "2" is 1.5x.
    for (stored, scale) in [("auto", 1.0), ("2", 1.5)] {
        let dir = tempfile::tempdir().unwrap();
        let mut state = state_with(dir.path(), &format!(r#"{{"ui_scale": "{stored}"}}"#));
        let _ = app::update(&mut state, Message::WindowOpened(id, WINDOW_SIZE));
        let _ = app::update(&mut state, Message::DesktopScale(1.5));
        let _ = app::update(&mut state, Message::DesktopRescaled(2.0));
        assert_eq!((state.desktop_scale, state.scale), (2.0, scale), "{stored}");
    }
}

#[test]
fn the_window_fits_the_monitor_at_1_5x() {
    // R49/R70 at GUI Scale 1.5 on a 100 % 1920 × 1080 monitor: iced reports the monitor in
    // logical pixels of the total zoom (1280 × 720), and the window takes 90 % of that.
    let dir = tempfile::tempdir().unwrap();
    let mut state = state_with(dir.path(), r#"{"ui_scale": "1.5"}"#);
    let id = window::Id::unique();
    let _ = app::update(&mut state, Message::WindowOpened(id, WINDOW_SIZE));
    let _ = app::update(&mut state, Message::DesktopScale(1.0));
    assert_eq!(window_zoom(state.scale, state.desktop_scale), 1.5);
    let _ = app::update(
        &mut state,
        Message::MonitorSize(Some(Size::new(1280.0, 720.0))),
    );
    assert_eq!(state.window_size, Size::new(1152.0, 648.0));
}

#[test]
fn the_desktop_scale_is_read_when_the_window_opens() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().display().to_string();
    let env = move |key: &str| match key {
        "APPDATA" | "USERPROFILE" | "HOME" | "XDG_CONFIG_HOME" => Some(root.clone()),
        _ => None,
    };
    let mut state = State::new(Paths::from_env(dir.path(), &env));
    let id = window::Id::unique();
    let _ = app::update(
        &mut state,
        Message::WindowOpened(id, Size::new(1000.0, 700.0)),
    );
    assert_eq!(state.window_size, Size::new(1000.0, 700.0));
    assert_eq!(state.scale, 1.0);
    // The default GUI Scale is 1x (R90): the zoom takes the desktop's 150 % back out.
    let _ = app::update(&mut state, Message::DesktopScale(1.5));
    assert_eq!(state.desktop_scale, 1.5);
    assert_eq!(state.scale, 1.0);
    assert!((window_zoom(state.scale, state.desktop_scale) - 1.0 / 1.5).abs() < 1e-6);
    let _ = app::update(
        &mut state,
        Message::MonitorSize(Some(Size::new(1366.0, 768.0))),
    );
    assert_eq!(state.window_size, Size::new(1229.0, 691.0));
    // A monitor that reports no size leaves the window at its usual size, minimum included.
    let _ = app::update(&mut state, Message::MonitorSize(Some(Size::ZERO)));
    assert_eq!(state.window_size, WINDOW_SIZE);
}

#[test]
fn voice_swap_is_its_own_role() {
    assert_eq!(route(&["--voice-swap", "--help"]), Route::VoiceSwap);
    assert_eq!(route(&["--voice-swap", "a", "--play"]), Route::VoiceSwap);
}
