#![allow(clippy::unwrap_used, clippy::expect_used)]

use indexmap::IndexMap;
use spartanport_launcher::core::launch::{run_command, spawn_game, GameEvent, GameHandle};
use spartanport_launcher::core::log::RunLog;
use std::ffi::OsString;
use std::path::Path;
use std::sync::mpsc::RecvTimeoutError;
use std::time::{Duration, Instant};

const BOUND: Duration = Duration::from_secs(10);

fn start(args: &[&str], log: RunLog) -> GameHandle {
    let mut command = vec![env!("CARGO_BIN_EXE_fake-game").to_string()];
    command.extend(args.iter().map(|a| a.to_string()));
    let mut env = IndexMap::new();
    if let Some(root) = std::env::var_os("SystemRoot") {
        env.insert(
            "SystemRoot".to_string(),
            root.to_string_lossy().into_owned(),
        );
    }
    spawn_game(&command, Path::new("."), &env, log).unwrap()
}

/// Every event until `Exited`, each with the time it arrived.
fn collect(handle: &GameHandle) -> Vec<(GameEvent, Instant)> {
    let mut events = Vec::new();
    loop {
        match handle.events.recv_timeout(BOUND) {
            Ok(event) => {
                let done = matches!(event, GameEvent::Exited(_));
                events.push((event, Instant::now()));
                if done {
                    return events;
                }
            }
            Err(RecvTimeoutError::Timeout) => panic!("no Exited event in time"),
            Err(RecvTimeoutError::Disconnected) => panic!("channel closed before Exited"),
        }
    }
}

fn texts(events: &[(GameEvent, Instant)]) -> Vec<String> {
    events
        .iter()
        .filter_map(|(e, _)| match e {
            GameEvent::Output(t) => Some(t.replace("\r\n", "\n")),
            GameEvent::Exited(_) => None,
        })
        .collect()
}

#[test]
fn every_line_arrives_then_the_exit_code() {
    let handle = start(
        &["--lines", "5", "--delay-ms", "0", "--exit", "3"],
        RunLog::open(Path::new("")),
    );
    let events = collect(&handle);
    let expected: Vec<String> = (1..=5).map(|k| format!("line {k}\n")).collect();
    assert_eq!(texts(&events), expected);
    assert!(matches!(events.last().unwrap().0, GameEvent::Exited(3)));
}

#[test]
fn exit_arrives_after_stdout_closed_early() {
    let handle = start(
        &[
            "--lines",
            "2",
            "--silent-after",
            "2",
            "--exit",
            "4",
            "--delay-ms",
            "0",
        ],
        RunLog::open(Path::new("")),
    );
    let events = collect(&handle);
    let (last, exited_at) = events.last().unwrap();
    assert!(matches!(last, GameEvent::Exited(4)));
    let second_line_at = events
        .iter()
        .filter(|(e, _)| matches!(e, GameEvent::Output(_)))
        .map(|(_, t)| *t)
        .next_back()
        .unwrap();
    assert!(exited_at.duration_since(second_line_at) >= Duration::from_millis(400));
}

#[test]
fn terminate_ends_a_running_child_and_reports_exit() {
    let mut handle = start(
        &["--lines", "1000", "--delay-ms", "50", "--exit", "0"],
        RunLog::open(Path::new("")),
    );
    match handle.events.recv_timeout(BOUND).unwrap() {
        GameEvent::Output(_) => {}
        GameEvent::Exited(code) => panic!("exited early with {code}"),
    }
    handle.terminate();
    let events = collect(&handle);
    match events.last().unwrap().0 {
        GameEvent::Exited(code) => assert_eq!(code, if cfg!(windows) { 1 } else { -1 }),
        GameEvent::Output(_) => unreachable!(),
    }
    handle.close();
}

#[test]
fn run_log_holds_every_line() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("last_run.log");
    let handle = start(
        &["--lines", "5", "--delay-ms", "0", "--exit", "0"],
        RunLog::open(&path),
    );
    collect(&handle);
    let text = std::fs::read_to_string(&path)
        .unwrap()
        .replace("\r\n", "\n");
    let expected: String = (1..=5).map(|k| format!("line {k}\n")).collect();
    assert_eq!(text, expected);
}

#[test]
fn the_game_command_is_the_exe_with_run() {
    let exe = Path::new("port").join("spartanport-launcher");
    assert_eq!(
        run_command(&exe, &[]),
        vec![exe.to_string_lossy().into_owned(), "--run".to_string()]
    );
    let args = [OsString::from("--fullscreen"), OsString::from("two words")];
    assert_eq!(
        run_command(&exe, &args),
        vec![
            exe.to_string_lossy().into_owned(),
            "--run".to_string(),
            "--fullscreen".to_string(),
            "two words".to_string(),
        ]
    );
}
