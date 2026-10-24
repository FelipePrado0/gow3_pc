//! Calls for systems other than Windows.

use nix::errno::Errno;
use nix::sys::signal::{kill, killpg, SigSet, Signal};
use nix::unistd::Pid;
use std::io;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Child, Command};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// Time between SIGTERM and SIGKILL.
const KILL_AFTER: Duration = Duration::from_secs(3);

/// There is no Windows UI language here.
pub fn user_default_ui_language() -> Option<u32> {
    None
}

/// Steam keeps no registry entry here.
pub fn steam_path_from_registry() -> Option<std::path::PathBuf> {
    None
}

/// Programs share the terminal here: nothing to hide.
pub fn no_window(_cmd: &mut Command) {}

/// The error goes to the terminal here: there is no dialog to block on.
pub fn show_error(title: &str, text: &str) {
    eprintln!("{title}: {text}");
}

/// The output already reaches the terminal here.
pub fn attach_parent_console() {}

/// Console programs never open a window of their own here.
pub fn no_console_window(_cmd: &mut Command) {}

/// Set once SIGINT came after `catch_interrupt`.
static INTERRUPTED: AtomicBool = AtomicBool::new(false);

/// From now on SIGINT (Ctrl+C) ends nothing: it sets [`interrupted`], which the caller checks to
/// end in order (the Python's `KeyboardInterrupt`). The signal is blocked in the calling thread,
/// and so in the threads it starts afterwards, and taken by a thread of its own; the programs it
/// starts get the usual handling back (the standard library resets their signal mask).
pub fn catch_interrupt() {
    let mut set = SigSet::empty();
    set.add(Signal::SIGINT);
    if set.thread_block().is_ok() {
        std::thread::spawn(move || {
            while set.wait().is_ok() {
                INTERRUPTED.store(true, Ordering::SeqCst);
            }
        });
    }
}

/// Whether Ctrl+C came since `catch_interrupt`.
pub fn interrupted() -> bool {
    INTERRUPTED.load(Ordering::SeqCst)
}

/// Puts the child in its own process group.
pub fn prepare(cmd: &mut Command) {
    cmd.process_group(0);
}

/// The game's process group.
pub struct Job {
    pgid: i32,
}

fn signal_group(pgid: i32, signal: Signal) {
    // A group that is gone (ESRCH) or not ours (EPERM) is not worth reporting; nothing to undo.
    let _ = killpg(Pid::from_raw(pgid), signal);
}

impl Job {
    pub fn attach(child: &Child) -> Job {
        Job {
            pgid: i32::try_from(child.id()).unwrap_or(0),
        }
    }

    /// SIGTERM to the group, then SIGKILL after three seconds.
    pub fn terminate(&mut self) {
        if self.pgid <= 0 {
            return;
        }
        let pgid = self.pgid;
        signal_group(pgid, Signal::SIGTERM);
        std::thread::spawn(move || {
            std::thread::sleep(KILL_AFTER);
            signal_group(pgid, Signal::SIGKILL);
        });
    }

    pub fn close(&mut self) {}
}

/// Points this process's stdout and stderr at `/dev/null`, which closes the pipe the game's
/// reader holds the other end of. Errors are ignored.
pub fn close_std_streams() {
    if let Ok(null) = std::fs::File::options().write(true).open("/dev/null") {
        let _ = nix::unistd::dup2_stdout(&null);
        let _ = nix::unistd::dup2_stderr(&null);
    }
}

/// Junctions exist only on Windows: links here are symlinks.
pub fn create_junction(_link: &Path, _target: &Path) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "junctions exist only on Windows",
    ))
}

/// Waits up to `limit` for process `pid` to end, polling every 100 ms. `true` when it ended or
/// does not exist, `false` when it still runs after `limit`.
pub fn wait_for_process(pid: u32, limit: Duration) -> bool {
    let Ok(raw) = i32::try_from(pid) else {
        return true;
    };
    let deadline = std::time::Instant::now() + limit;
    loop {
        // Signal 0 only checks the process; a process of another user (EPERM) still runs.
        if kill(Pid::from_raw(raw), None) == Err(Errno::ESRCH) {
            return true;
        }
        if std::time::Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn there_are_no_junctions_here() {
        let temp = tempfile::tempdir().unwrap();
        let link = temp.path().join("link");
        let error = create_junction(&link, temp.path()).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::Unsupported);
        assert!(std::fs::symlink_metadata(&link).is_err());
    }
}
