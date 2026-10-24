//! A stand-in for `spartanport-probe` used by the runner tests (`tests/runner.rs`).
//!
//! - `--cpu-features`: prints `CPU: missing <FAKE_PROBE_MISSING>` when that list is set, and
//!   exits with `FAKE_PROBE_CPU_EXIT` (default 0).
//! - `--gamepad`: prints `FAKE_PROBE_GAMEPAD`; hangs when it is `hang`.
//! - `--displays`: prints nothing.
//! - Any other run (the game): prints `fake-probe argv:`, its argv one per line and
//!   `fake-probe argv end`; then `NAME=value` (or `NAME is unset`) for each variable named in
//!   `FAKE_PROBE_PRINT` (comma-separated); with `FAKE_PROBE_SHOW=<file>`, the text of that file
//!   under its `--app0` folder (`fake-probe show <file>: <text>`, or `missing`). With
//!   `FAKE_PROBE_RESTART=1` and `SPARTAN_RESTART_COMMAND` set it prints
//!   `Runtime: restarting through run.py` and starts that command as the probe does (Windows: the
//!   command line, with this program's output handles, then exit 0; Linux: `sh -c` in place of
//!   itself), with `FAKE_PROBE_RESTART=0` so the new launch does not restart again. Otherwise it
//!   sleeps `FAKE_PROBE_SLEEP_MS` (default 0) and exits with `FAKE_PROBE_EXIT` (default 0).
//!
//! What it cannot do (no `--app0` to show from, a restart that cannot start) it says on stderr
//! and exits with 1.

use std::io::Write;
use std::path::Path;
use std::time::Duration;

fn var(name: &str) -> String {
    std::env::var(name).unwrap_or_default()
}

fn code(name: &str) -> i32 {
    var(name).parse().unwrap_or(0)
}

/// Says why on stderr and exits with 1.
fn give_up(why: &str) -> ! {
    eprintln!("fake-probe: {why}");
    std::process::exit(1);
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut out = std::io::stdout();
    match args.get(1).map(String::as_str) {
        Some("--cpu-features") => {
            let missing = var("FAKE_PROBE_MISSING");
            if !missing.is_empty() {
                let _ = writeln!(out, "CPU: missing {missing}");
            }
            let _ = out.flush();
            std::process::exit(code("FAKE_PROBE_CPU_EXIT"));
        }
        Some("--gamepad") => {
            let pad = var("FAKE_PROBE_GAMEPAD");
            if pad == "hang" {
                loop {
                    std::thread::sleep(Duration::from_secs(60));
                }
            }
            let _ = writeln!(out, "{pad}");
            let _ = out.flush();
            std::process::exit(0);
        }
        Some("--displays") => std::process::exit(0),
        _ => {}
    }
    let _ = writeln!(out, "fake-probe argv:");
    for arg in &args {
        let _ = writeln!(out, "{arg}");
    }
    let _ = writeln!(out, "fake-probe argv end");
    for name in var("FAKE_PROBE_PRINT").split(',').filter(|n| !n.is_empty()) {
        let _ = match std::env::var(name) {
            Ok(value) => writeln!(out, "{name}={value}"),
            Err(_) => writeln!(out, "{name} is unset"),
        };
    }
    let show = var("FAKE_PROBE_SHOW");
    if !show.is_empty() {
        let app0 = args
            .iter()
            .position(|arg| arg == "--app0")
            .and_then(|at| args.get(at + 1));
        let Some(app0) = app0 else {
            give_up("FAKE_PROBE_SHOW needs --app0 <folder>");
        };
        let text = std::fs::read_to_string(Path::new(app0).join(&show))
            .unwrap_or_else(|_| "missing".to_string());
        let _ = writeln!(out, "fake-probe show {show}: {text}");
    }
    let command = var("SPARTAN_RESTART_COMMAND");
    if var("FAKE_PROBE_RESTART") == "1" && !command.is_empty() {
        let _ = writeln!(out, "Runtime: restarting through run.py");
        let _ = out.flush();
        restart(&command);
    }
    let _ = out.flush();
    std::thread::sleep(Duration::from_millis(
        var("FAKE_PROBE_SLEEP_MS").parse().unwrap_or(0),
    ));
    std::process::exit(code("FAKE_PROBE_EXIT"));
}

/// `CreateProcess(NULL, command)` with the inherited output handles, then exit 0, as the probe's
/// `runtime_restart`: the program is the first word of the line (quoted or not), the rest is
/// passed as written.
#[cfg(windows)]
fn restart(command: &str) {
    use std::os::windows::process::CommandExt;
    let (program, rest) = match command.strip_prefix('"') {
        Some(quoted) => quoted.split_once('"').unwrap_or((quoted, "")),
        None => command.split_once([' ', '\t']).unwrap_or((command, "")),
    };
    let started = std::process::Command::new(program)
        .raw_arg(rest.trim_start())
        .env("FAKE_PROBE_RESTART", "0")
        .spawn();
    match started {
        Ok(_) => std::process::exit(0),
        Err(error) => give_up(&format!("restart: {error}")),
    }
}

/// `sh -c command` in place of this process, as the probe's `runtime_restart`.
#[cfg(not(windows))]
fn restart(command: &str) {
    use std::os::unix::process::CommandExt;
    let error = std::process::Command::new("/bin/sh")
        .arg("-c")
        .arg(command)
        .env("FAKE_PROBE_RESTART", "0")
        .exec();
    give_up(&format!("exec: {error}"));
}
