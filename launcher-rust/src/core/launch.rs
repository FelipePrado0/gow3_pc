//! Starting the game, reading its merged output and stopping it.

use super::log::RunLog;
use super::platform;
use indexmap::IndexMap;
use std::ffi::OsString;
use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc::{channel, Receiver};

/// The longest piece of output delivered at once. A line longer than this is split into chunks
/// of at most this many bytes, so a huge line without a newline never grows the buffer unbounded.
const MAX_CHUNK: u64 = 64 * 1024;

/// What the reader thread reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GameEvent {
    /// A piece of output: normally one whole line including its `\n`.
    Output(String),
    /// The process ended; `-1` when it was killed by a signal.
    Exited(i32),
}

#[derive(Debug, thiserror::Error)]
pub enum LaunchError {
    #[error("the command is empty")]
    EmptyCommand,
    #[error("could not start the game: {0}")]
    Spawn(#[from] std::io::Error),
}

/// The command that runs the game: the launcher's own `exe` with `--run` and `args`.
pub fn run_command(exe: &Path, args: &[OsString]) -> Vec<String> {
    let mut command = vec![exe.to_string_lossy().into_owned(), "--run".to_string()];
    command.extend(args.iter().map(|arg| arg.to_string_lossy().into_owned()));
    command
}

/// A running game: its events and the means to stop it.
pub struct GameHandle {
    pub pid: u32,
    pub events: Receiver<GameEvent>,
    job: platform::Job,
}

impl GameHandle {
    /// Stops the game and everything it started.
    pub fn terminate(&mut self) {
        self.job.terminate();
    }

    /// Releases the job control resources; the game keeps running if it was not terminated.
    pub fn close(&mut self) {
        self.job.close();
    }
}

/// Starts `command` in `cwd` with exactly `env`, stdin closed and stdout and stderr merged into
/// one pipe. A thread reads that pipe and sends the events; it also writes `run_log`.
pub fn spawn_game(
    command: &[String],
    cwd: &Path,
    env: &IndexMap<String, String>,
    run_log: RunLog,
) -> Result<GameHandle, LaunchError> {
    let (program, args) = command.split_first().ok_or(LaunchError::EmptyCommand)?;
    let (reader, writer) = std::io::pipe()?;
    let mut cmd = Command::new(program);
    cmd.args(args)
        .current_dir(cwd)
        .env_clear()
        .envs(env)
        .stdin(Stdio::null())
        .stdout(Stdio::from(writer.try_clone()?))
        .stderr(Stdio::from(writer));
    platform::prepare(&mut cmd);
    // `cmd` owns the writer ends; dropping it after the spawn closes them in this process, so the
    // reader sees EOF once the child and any descendant holding the pipe are gone.
    let child = cmd.spawn();
    drop(cmd);
    let mut child = child?;
    let pid = child.id();
    let job = platform::Job::attach(&child);
    let (tx, events) = channel();
    std::thread::spawn(move || {
        let mut run_log = run_log;
        let mut reader = BufReader::new(reader);
        let mut buf = Vec::new();
        loop {
            buf.clear();
            match reader.by_ref().take(MAX_CHUNK).read_until(b'\n', &mut buf) {
                Ok(0) | Err(_) => break,
                Ok(_) => {}
            }
            let text = String::from_utf8_lossy(&buf).into_owned();
            run_log.write(&text);
            let _ = tx.send(GameEvent::Output(text));
        }
        drop(run_log);
        let code = child.wait().ok().and_then(|s| s.code()).unwrap_or(-1);
        let _ = tx.send(GameEvent::Exited(code));
    });
    Ok(GameHandle { pid, events, job })
}
