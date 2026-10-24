use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use spartanport_launcher::runner::wait_for_pid;

#[test]
fn wait_for_pid_returns_when_the_process_ends() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_fake-game"))
        .args(["--exit", "0", "--lines", "1", "--delay-ms", "300"])
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    // On Linux an exited child stays a zombie until it is reaped, and `kill(pid, 0)` still
    // succeeds on it: a thread reaps it, as the real caller's parent would.
    let pid = child.id();
    let started = Instant::now();
    let reaper = std::thread::spawn(move || child.wait());
    assert!(wait_for_pid(pid, Duration::from_secs(10)));
    assert!(started.elapsed() < Duration::from_secs(10));
    let _ = reaper.join();
    // A pid that never existed: true at once.
    let started = Instant::now();
    assert!(wait_for_pid(u32::MAX - 1, Duration::from_secs(10)));
    assert!(started.elapsed() < Duration::from_secs(2));
}
