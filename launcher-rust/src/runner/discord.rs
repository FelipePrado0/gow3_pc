//! The Discord presence (Python `scripts/discord_presence.py`): while the game runs, the player's
//! Discord profile shows the game and the time played. A thread of its own talks to the Discord
//! client of this machine over its local IPC (named pipes on Windows, Unix sockets elsewhere), so
//! the game never waits for it; without a Discord client nothing happens.
//!
//! Frames are `<u32 opcode><u32 length>` (little endian) and a compact UTF-8 JSON body. On
//! Windows the pipe is in non-blocking mode, as the Python's: a read that finds nothing returns
//! within POLL and a write the pipe cannot take fails at once. A blocking pipe read on a helper
//! thread would not do: Windows serialises the I/O of a synchronous handle, so the writes that
//! clear the activity (and even closing the handle) would wait behind the pending read.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::{json, Map, Value};

#[cfg(windows)]
use crate::core::platform;

/// The SpartanPort application on Discord; `SPARTAN_DISCORD_ID` replaces it.
pub const CLIENT_ID: &str = "1560048868677132431";
/// The wait before the next try while no connection is up.
pub const RETRY: Duration = Duration::from_secs(15);
/// How long `close` waits for the thread.
pub const JOIN: Duration = Duration::from_secs(3);
/// How long one read of a transport waits before the stop and the time limit are checked again.
pub const POLL: Duration = Duration::from_millis(100);
/// The time limit of Discord's answers: to the handshake, to the activity, and of a frame's body.
pub const READ: Duration = Duration::from_secs(5);

/// The frame header: the opcode and the body's length.
const HEADER: usize = 8;
/// The largest frame body taken.
const MAX_BODY: usize = 1024 * 1024;
/// What the runner prints when the presence thread cannot start (`run.py`).
const UNAVAILABLE: &str = "Discord presence unavailable; the game will continue";

/// The opcodes of Discord's IPC frames.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Opcode {
    Handshake = 0,
    Frame = 1,
    Close = 2,
    Ping = 3,
    Pong = 4,
}

impl Opcode {
    fn from_u32(value: u32) -> Option<Opcode> {
        [
            Opcode::Handshake,
            Opcode::Frame,
            Opcode::Close,
            Opcode::Ping,
            Opcode::Pong,
        ]
        .into_iter()
        .find(|opcode| *opcode as u32 == value)
    }
}

/// One frame: the header and `payload` as JSON written the Python's way (`separators=(',', ':')`,
/// `ensure_ascii=False`, the keys in their order).
pub fn pack(opcode: Opcode, payload: &Value) -> Vec<u8> {
    let body = payload.to_string();
    [&header(opcode, body.len())[..], body.as_bytes()].concat()
}

fn header(opcode: Opcode, length: usize) -> [u8; HEADER] {
    let mut header = [0; HEADER];
    header[..4].copy_from_slice(&(opcode as u32).to_le_bytes());
    header[4..].copy_from_slice(&(length as u32).to_le_bytes());
    header
}

/// `size` bytes of `transport`, from as many reads as it takes (Python `read_exact`): a read that
/// finds nothing within POLL is tried again until `stop` is set or `timeout` (None: no limit) has
/// passed.
fn read_exact(
    transport: &mut dyn Transport,
    size: usize,
    stop: &AtomicBool,
    timeout: Option<Duration>,
) -> io::Result<Vec<u8>> {
    let deadline = timeout.map(|timeout| Instant::now() + timeout);
    let mut data = Vec::with_capacity(size);
    while data.len() < size {
        if stop.load(Ordering::SeqCst) {
            return Err(io::Error::new(
                io::ErrorKind::ConnectionAborted,
                "Discord presence stopped",
            ));
        }
        if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "Discord response timed out",
            ));
        }
        match transport.read(size - data.len(), Some(POLL)) {
            Ok(chunk) if chunk.is_empty() => return Err(connection_closed()),
            Ok(chunk) => data.extend_from_slice(&chunk),
            Err(error) if found_nothing(&error) => {}
            Err(error) => return Err(error),
        }
    }
    Ok(data)
}

/// A read that found nothing in its time (a pipe's `TimedOut`, a socket's `WouldBlock`) or was
/// interrupted: it is tried again.
fn found_nothing(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
    )
}

fn connection_closed() -> io::Error {
    io::Error::new(
        io::ErrorKind::ConnectionReset,
        "Discord closed the connection",
    )
}

fn invalid(text: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, text)
}

/// The next FRAME of `transport` and its JSON object (Python `read_frame`). On the way, a PING is
/// answered with a PONG of the same body and a PONG is skipped; a CLOSE ends the connection.
/// `timeout` limits the wait for the header (None while the connection only listens); a body
/// gets READ.
pub fn read_frame(
    transport: &mut dyn Transport,
    stop: &AtomicBool,
    timeout: Option<Duration>,
) -> io::Result<(Opcode, Value)> {
    loop {
        let head = read_exact(transport, HEADER, stop, timeout)?;
        let word =
            |at: usize| u32::from_le_bytes([head[at], head[at + 1], head[at + 2], head[at + 3]]);
        let (opcode, length) = (word(0), word(4) as usize);
        if length > MAX_BODY {
            return Err(invalid("Discord frame is too large"));
        }
        let body = if length > 0 {
            read_exact(transport, length, stop, Some(READ))?
        } else {
            Vec::new()
        };
        match Opcode::from_u32(opcode) {
            Some(Opcode::Ping) => {
                transport.write(&[&header(Opcode::Pong, length)[..], &body].concat())?;
            }
            Some(Opcode::Pong) => {}
            Some(Opcode::Close) => return Err(connection_closed()),
            Some(Opcode::Frame) => {
                let data = if body.is_empty() {
                    json!({})
                } else {
                    serde_json::from_slice(&body)
                        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?
                };
                if !data.is_object() {
                    return Err(invalid("Discord frame must contain an object"));
                }
                return Ok((Opcode::Frame, data));
            }
            _ => return Err(invalid("Unexpected Discord opcode")),
        }
    }
}

/// The activity shown: only the start time of the game (Discord counts the time played from it).
pub fn activity(started: u64) -> Value {
    json!({"timestamps": {"start": started}})
}

/// The SET_ACTIVITY request for the process `pid`: `value` shown, or the activity cleared (None).
pub fn set_activity(pid: u32, value: Option<Value>) -> Vec<u8> {
    let mut args = Map::new();
    args.insert("pid".to_string(), pid.into());
    if let Some(value) = value {
        args.insert("activity".to_string(), value);
    }
    let request = json!({"cmd": "SET_ACTIVITY", "args": Value::Object(args), "nonce": nonce()});
    pack(Opcode::Frame, &request)
}

/// A random version 4 UUID, as Python's `str(uuid.uuid4())`, for a request's `nonce`. The bits come
/// from the standard library's randomly keyed hasher: a nonce only has to differ.
fn nonce() -> String {
    use std::fmt::Write;
    use std::hash::{BuildHasher, Hasher};
    let keys = std::collections::hash_map::RandomState::new();
    let mut bytes = [0u8; 16];
    for (index, half) in bytes.chunks_exact_mut(8).enumerate() {
        let mut hasher = keys.build_hasher();
        hasher.write_usize(index);
        half.copy_from_slice(&hasher.finish().to_le_bytes());
    }
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let mut text = String::with_capacity(36);
    for (index, byte) in bytes.iter().enumerate() {
        if matches!(index, 4 | 6 | 8 | 10) {
            text.push('-');
        }
        let _ = write!(text, "{byte:02x}");
    }
    text
}

/// A connection to the Discord client.
pub trait Transport {
    /// Writes all of `data`, or fails.
    fn write(&mut self, data: &[u8]) -> io::Result<()>;
    /// At most `size` bytes, waiting up to `timeout` (None: as long as it takes) for some. A read
    /// that finds nothing in time fails with `TimedOut` or `WouldBlock`; once the connection has
    /// ended a read returns nothing.
    fn read(&mut self, size: usize, timeout: Option<Duration>) -> io::Result<Vec<u8>>;
    /// Lets the connection go.
    fn close(&mut self);
}

/// A Discord named pipe (Python `PipeTransport`), in non-blocking mode: a read polls the pipe
/// every 10 ms until something arrives or its time is up, and a write the pipe cannot take whole
/// fails at once, so a silent or stalled Discord client holds up neither the thread nor `close`.
#[cfg(windows)]
pub struct PipeTransport {
    pipe: Option<std::fs::File>,
}

#[cfg(windows)]
impl PipeTransport {
    pub fn open(path: &Path) -> io::Result<PipeTransport> {
        let pipe = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)?;
        platform::set_pipe_nowait(&pipe)?;
        Ok(PipeTransport { pipe: Some(pipe) })
    }

    fn pipe(&mut self) -> io::Result<&mut std::fs::File> {
        self.pipe
            .as_mut()
            .ok_or_else(|| io::ErrorKind::NotConnected.into())
    }
}

#[cfg(windows)]
impl Transport for PipeTransport {
    fn write(&mut self, data: &[u8]) -> io::Result<()> {
        use std::io::Write;
        if self.pipe()?.write(data)? != data.len() {
            return Err(io::Error::new(
                io::ErrorKind::WriteZero,
                "Discord pipe could not accept the frame",
            ));
        }
        Ok(())
    }

    fn read(&mut self, size: usize, timeout: Option<Duration>) -> io::Result<Vec<u8>> {
        use std::io::Read;
        let deadline = timeout.map(|timeout| Instant::now() + timeout);
        let pipe = self.pipe()?;
        loop {
            let available = platform::pipe_bytes_available(pipe)? as usize;
            if available > 0 {
                let mut data = vec![0; size.min(available)];
                let read = pipe.read(&mut data)?;
                data.truncate(read);
                return Ok(data);
            }
            if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
                return Err(io::ErrorKind::TimedOut.into());
            }
            thread::sleep(Duration::from_millis(10));
        }
    }

    fn close(&mut self) {
        self.pipe = None;
    }
}

/// A Discord Unix socket (Python `SocketTransport`); a write gives up after POLL.
#[cfg(not(windows))]
pub struct SocketTransport {
    stream: Option<std::os::unix::net::UnixStream>,
}

#[cfg(not(windows))]
impl SocketTransport {
    pub fn open(path: &Path) -> io::Result<SocketTransport> {
        let stream = std::os::unix::net::UnixStream::connect(path)?;
        stream.set_write_timeout(Some(POLL))?;
        Ok(SocketTransport {
            stream: Some(stream),
        })
    }

    fn stream(&mut self) -> io::Result<&mut std::os::unix::net::UnixStream> {
        self.stream
            .as_mut()
            .ok_or_else(|| io::ErrorKind::NotConnected.into())
    }
}

#[cfg(not(windows))]
impl Transport for SocketTransport {
    fn write(&mut self, data: &[u8]) -> io::Result<()> {
        use std::io::Write;
        self.stream()?.write_all(data)
    }

    fn read(&mut self, size: usize, timeout: Option<Duration>) -> io::Result<Vec<u8>> {
        use std::io::Read;
        let stream = self.stream()?;
        stream.set_read_timeout(timeout)?;
        let mut data = vec![0; size];
        let read = stream.read(&mut data)?;
        data.truncate(read);
        Ok(data)
    }

    fn close(&mut self) {
        self.stream = None;
    }
}

/// Where a Discord client listens (Python `candidates`): the pipes `\\?\pipe\discord-ipc-0` … `9`
/// on Windows; elsewhere the sockets `discord-ipc-0` … `9` in `XDG_RUNTIME_DIR`, `TMPDIR`, `TMP`,
/// `TEMP` and `/tmp` (each folder once), and in their `app/com.discordapp.Discord` (Flatpak) and
/// `snap.discord` (Snap) folders.
pub fn candidates(env: &dyn Fn(&str) -> Option<String>) -> Vec<PathBuf> {
    if cfg!(windows) {
        return (0..10)
            .map(|index| PathBuf::from(format!(r"\\?\pipe\discord-ipc-{index}")))
            .collect();
    }
    socket_candidates(env)
}

fn socket_candidates(env: &dyn Fn(&str) -> Option<String>) -> Vec<PathBuf> {
    let named = ["XDG_RUNTIME_DIR", "TMPDIR", "TMP", "TEMP"]
        .into_iter()
        .filter_map(env);
    let mut bases: Vec<String> = Vec::new();
    for base in named.chain(["/tmp".to_string()]) {
        if !base.is_empty() && !bases.contains(&base) {
            bases.push(base);
        }
    }
    let mut paths = Vec::new();
    for base in &bases {
        for sub in ["", "app/com.discordapp.Discord", "snap.discord"] {
            let folder = if sub.is_empty() {
                PathBuf::from(base)
            } else {
                Path::new(base).join(sub)
            };
            paths.extend((0..10).map(|index| folder.join(format!("discord-ipc-{index}"))));
        }
    }
    paths
}

/// A connection to the first of `paths` that takes one (Python `open_transport`), or None: no
/// Discord client listens there, or `stop` was set.
pub fn open_transport(paths: &[PathBuf], stop: &AtomicBool) -> Option<Box<dyn Transport>> {
    for path in paths {
        if stop.load(Ordering::SeqCst) {
            break;
        }
        if let Ok(transport) = open(path) {
            return Some(transport);
        }
    }
    None
}

#[cfg(windows)]
fn open(path: &Path) -> io::Result<Box<dyn Transport>> {
    Ok(Box::new(PipeTransport::open(path)?))
}

#[cfg(not(windows))]
fn open(path: &Path) -> io::Result<Box<dyn Transport>> {
    Ok(Box::new(SocketTransport::open(path)?))
}

/// Introduces the application `client_id` and waits up to READ for Discord's READY.
pub fn handshake(
    transport: &mut dyn Transport,
    client_id: &str,
    stop: &AtomicBool,
) -> io::Result<()> {
    transport.write(&pack(
        Opcode::Handshake,
        &json!({"v": 1, "client_id": client_id}),
    ))?;
    let (opcode, data) = read_frame(transport, stop, Some(READ))?;
    if opcode != Opcode::Frame || event(&data) != Some("READY") {
        return Err(io::Error::new(
            io::ErrorKind::ConnectionRefused,
            "Discord rejected the handshake",
        ));
    }
    Ok(())
}

/// Shows `value` for the process `pid` and waits up to READ for Discord's answer.
pub fn send_activity(
    transport: &mut dyn Transport,
    value: Value,
    pid: u32,
    stop: &AtomicBool,
) -> io::Result<()> {
    transport.write(&set_activity(pid, Some(value)))?;
    let (_, data) = read_frame(transport, stop, Some(READ))?;
    if event(&data) == Some("ERROR") {
        return Err(activity_rejected());
    }
    Ok(())
}

fn event(data: &Value) -> Option<&str> {
    data.get("evt").and_then(Value::as_str)
}

fn activity_rejected() -> io::Error {
    io::Error::new(
        io::ErrorKind::ConnectionRefused,
        "Discord rejected the activity",
    )
}

/// The way of the presence thread to a Discord client: `open_transport` over `candidates`.
type Opener = Box<dyn FnMut(&AtomicBool) -> Option<Box<dyn Transport>> + Send>;

/// What a presence starts from besides the environment: the client id when `SPARTAN_DISCORD_ID`
/// gives none, the wait between tries and the way to a Discord client. The tests set them as the
/// Python tests patch `CLIENT_ID`, `RETRY_SECONDS` and `open_transport`.
struct Setup {
    default_id: String,
    retry: Duration,
    opener: Opener,
}

impl Setup {
    fn system(env: &dyn Fn(&str) -> Option<String>) -> Setup {
        let paths = candidates(env);
        Setup {
            default_id: CLIENT_ID.to_string(),
            retry: RETRY,
            opener: Box::new(move |stop: &AtomicBool| open_transport(&paths, stop)),
        }
    }
}

/// The game's presence on Discord (Python `Presence`). A thread connects to the Discord client,
/// shows the time the game process `pid` has been running, keeps the connection (answering
/// PINGs) and tries again every RETRY while there is none. `close` clears the activity and ends
/// the thread; dropping the presence does the same.
pub struct Presence {
    stop: Arc<AtomicBool>,
    worker: Option<Worker>,
}

/// The presence thread and its two channels: dropping `wake` ends its wait between tries, and
/// `done` disconnects when the thread is over.
struct Worker {
    handle: JoinHandle<()>,
    wake: mpsc::Sender<()>,
    done: mpsc::Receiver<()>,
}

impl Presence {
    /// The presence of the game process `pid`. No thread starts when `SPARTAN_DISCORD=0`, when
    /// there is no client id, or when the system cannot start one (`while_running` reports that).
    pub fn new(pid: u32, env: &dyn Fn(&str) -> Option<String>) -> Presence {
        Presence::start(pid, env, Setup::system(env)).unwrap_or_else(|_| Presence {
            stop: Arc::new(AtomicBool::new(true)),
            worker: None,
        })
    }

    /// `new`, failing when the thread cannot start.
    fn start(pid: u32, env: &dyn Fn(&str) -> Option<String>, setup: Setup) -> io::Result<Presence> {
        let stop = Arc::new(AtomicBool::new(false));
        let client_id = env("SPARTAN_DISCORD_ID")
            .filter(|id| !id.is_empty())
            .unwrap_or(setup.default_id);
        if client_id.is_empty() || env("SPARTAN_DISCORD").as_deref() == Some("0") {
            return Ok(Presence { stop, worker: None });
        }
        let mut session = Session {
            pid,
            client_id,
            started: unix_time(),
            stop: Arc::clone(&stop),
            opener: setup.opener,
        };
        let retry = setup.retry;
        let (wake, asleep) = mpsc::channel();
        let (finished, done) = mpsc::channel();
        let handle = thread::Builder::new()
            .name("discord-presence".to_string())
            .spawn(move || {
                // Dropped when the thread is over, however it ends: `done` then disconnects.
                let _finished: mpsc::Sender<()> = finished;
                session.run(retry, &asleep);
            })?;
        Ok(Presence {
            stop,
            worker: Some(Worker { handle, wake, done }),
        })
    }

    /// Clears the activity and ends the thread, waiting up to JOIN for it (Python `close`).
    pub fn close(mut self) {
        self.shutdown();
    }

    /// `close`; whether the thread ended within JOIN. Its reads return within POLL and its writes
    /// do not wait (a pipe in non-blocking mode; a socket write gives up after POLL), so it sees
    /// the stop well within JOIN. Should it not, it is left to end on its own, holding its
    /// connection, its way to Discord and the shared stop: it lets the connection go at its next
    /// check of the stop and never connects again.
    fn shutdown(&mut self) -> bool {
        self.stop.store(true, Ordering::SeqCst);
        let Some(Worker { handle, wake, done }) = self.worker.take() else {
            return true;
        };
        drop(wake);
        match done.recv_timeout(JOIN) {
            Err(mpsc::RecvTimeoutError::Timeout) => false,
            _ => {
                let _ = handle.join();
                true
            }
        }
    }
}

impl Drop for Presence {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// What the presence thread works with.
struct Session {
    pid: u32,
    client_id: String,
    /// The start time shown, in seconds since 1970: every reconnection shows the same one.
    started: u64,
    stop: Arc<AtomicBool>,
    opener: Opener,
}

impl Session {
    /// Python `run`: a connection, then RETRY (or until `close`) before the next.
    fn run(&mut self, retry: Duration, wake: &mpsc::Receiver<()>) {
        while !self.stop.load(Ordering::SeqCst) {
            self.connect();
            if !matches!(
                wake.recv_timeout(retry),
                Err(mpsc::RecvTimeoutError::Timeout)
            ) {
                break;
            }
        }
    }

    /// Python `connect`: one connection, from the handshake to its end. Whatever ends it only
    /// means Discord is not there for now. When `close` ends a connection that showed the
    /// activity, the activity is cleared and Discord told goodbye before it goes.
    fn connect(&mut self) {
        let stop = Arc::clone(&self.stop);
        let Some(mut transport) = (self.opener)(&stop) else {
            return;
        };
        let mut published = false;
        let _ = self.publish(&mut *transport, &stop, &mut published);
        if published
            && stop.load(Ordering::SeqCst)
            && transport.write(&set_activity(self.pid, None)).is_ok()
        {
            let _ = transport.write(&pack(Opcode::Close, &json!({})));
        }
        transport.close();
    }

    /// The handshake and the activity, then listening (`published` once the activity shows)
    /// until `stop` or the connection's end.
    fn publish(
        &self,
        transport: &mut dyn Transport,
        stop: &AtomicBool,
        published: &mut bool,
    ) -> io::Result<()> {
        handshake(transport, &self.client_id, stop)?;
        send_activity(transport, activity(self.started), self.pid, stop)?;
        *published = true;
        while !stop.load(Ordering::SeqCst) {
            let (_, data) = read_frame(transport, stop, None)?;
            if event(&data) == Some("ERROR") {
                return Err(activity_rejected());
            }
        }
        Ok(())
    }
}

/// Seconds since 1970, cut to whole ones (Python `int(time.time())`).
fn unix_time() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

/// The presence while the runner waits for the game (`run.py`): made for the game process `pid`,
/// alive while `wait` (the wait for the game) runs, and closed after it, also when `wait` panics.
/// When the thread cannot start, the Python's line goes to `log` and the game goes on.
pub fn while_running<T>(
    pid: u32,
    env: &dyn Fn(&str) -> Option<String>,
    log: &mut dyn FnMut(String),
    wait: impl FnOnce() -> T,
) -> T {
    while_running_with(
        pid,
        |pid| Presence::start(pid, env, Setup::system(env)),
        log,
        wait,
    )
}

fn while_running_with<T>(
    pid: u32,
    make: impl FnOnce(u32) -> io::Result<Presence>,
    log: &mut dyn FnMut(String),
    wait: impl FnOnce() -> T,
) -> T {
    let presence = match make(pid) {
        Ok(presence) => Some(presence),
        Err(_) => {
            log(UNAVAILABLE.to_string());
            None
        }
    };
    let result = wait();
    if let Some(presence) = presence {
        presence.close();
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::sync::atomic::AtomicUsize;
    use std::sync::{Condvar, Mutex};

    const SECOND: Duration = Duration::from_secs(1);

    fn env_of(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let pairs: Vec<(String, String)> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |key| pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone())
    }

    fn never() -> AtomicBool {
        AtomicBool::new(false)
    }

    fn ready() -> Value {
        json!({"evt": "READY"})
    }

    /// The answer to SET_ACTIVITY.
    fn answered() -> Value {
        json!({"evt": null})
    }

    /// Python `decode`: the opcode and the JSON body of one written frame.
    fn decode(data: &[u8]) -> (u32, Value) {
        let word = |at: usize| u32::from_le_bytes(data[at..at + 4].try_into().unwrap());
        let length = word(4) as usize;
        let body = serde_json::from_slice(&data[HEADER..HEADER + length]).unwrap();
        (word(0), body)
    }

    fn count(frames: &[(u32, Value)], opcode: Opcode) -> usize {
        frames.iter().filter(|(op, _)| *op == opcode as u32).count()
    }

    // -- In-memory transports (Python FakeTransport, LiveTransport and their variants) -------------

    /// Both ends of an in-memory transport: what the presence sent and what it may still read.
    #[derive(Default)]
    struct Wire {
        incoming: Vec<u8>,
        written: Vec<Vec<u8>>,
        closed: bool,
        reads: usize,
        // The events of Python's `LiveTransport`.
        reading: bool,
        published: bool,
        pong: bool,
    }

    /// The test's hold on a `Fake`'s wire while the presence thread owns the transport.
    #[derive(Clone, Default)]
    struct Shared(Arc<(Mutex<Wire>, Condvar)>);

    impl Shared {
        fn with<R>(&self, change: impl FnOnce(&mut Wire) -> R) -> R {
            let (wire, changed) = &*self.0;
            let result = change(&mut wire.lock().unwrap());
            changed.notify_all();
            result
        }

        /// Python `Event.wait(limit)`: whether `done` holds within `limit`.
        fn wait(&self, limit: Duration, done: impl Fn(&Wire) -> bool) -> bool {
            let (wire, changed) = &*self.0;
            let (wire, _) = changed
                .wait_timeout_while(wire.lock().unwrap(), limit, |wire| !done(wire))
                .unwrap();
            done(&wire)
        }

        fn frames(&self) -> Vec<(u32, Value)> {
            self.with(|wire| wire.written.iter().map(|data| decode(data)).collect())
        }

        fn closed(&self) -> bool {
            self.with(|wire| wire.closed)
        }
    }

    /// Python `FakeTransport(replies)`: the replies as frames, then the end of the connection (a
    /// read returns nothing). `live` (Python `LiveTransport`): after the replies, reads time out
    /// until the transport is closed.
    struct Fake {
        wire: Shared,
        live: bool,
        /// The most one read returns (Python `Slow` and `Interrupted`: 3).
        chunk: usize,
        /// The read call that times out (Python `Interrupted`: the second).
        timeout_at: Option<usize>,
    }

    impl Fake {
        fn new(replies: &[Value]) -> Fake {
            let wire = Shared::default();
            wire.with(|wire| {
                wire.incoming = replies
                    .iter()
                    .flat_map(|reply| pack(Opcode::Frame, reply))
                    .collect()
            });
            Fake {
                wire,
                live: false,
                chunk: usize::MAX,
                timeout_at: None,
            }
        }

        fn live(replies: &[Value]) -> Fake {
            Fake {
                live: true,
                ..Fake::new(replies)
            }
        }
    }

    impl Transport for Fake {
        fn write(&mut self, data: &[u8]) -> io::Result<()> {
            self.wire.with(|wire| {
                wire.pong |= data[..4] == (Opcode::Pong as u32).to_le_bytes();
                wire.written.push(data.to_vec());
            });
            Ok(())
        }

        fn read(&mut self, size: usize, _timeout: Option<Duration>) -> io::Result<Vec<u8>> {
            let (live, chunk, timeout_at) = (self.live, self.chunk, self.timeout_at);
            let read = self.wire.with(|wire| {
                wire.reads += 1;
                wire.reading = true;
                if Some(wire.reads) == timeout_at {
                    return None;
                }
                if wire.incoming.is_empty() {
                    return (!live || wire.closed).then(Vec::new);
                }
                let take = size.min(chunk).min(wire.incoming.len());
                let data: Vec<u8> = wire.incoming.drain(..take).collect();
                wire.published |= live && wire.incoming.is_empty();
                Some(data)
            });
            match read {
                Some(data) => Ok(data),
                None => {
                    if live {
                        thread::sleep(Duration::from_millis(10));
                    }
                    Err(io::ErrorKind::TimedOut.into())
                }
            }
        }

        fn close(&mut self) {
            self.wire.with(|wire| wire.closed = true);
        }
    }

    /// Python `mock.patch.object(dp, 'open_transport', side_effect=transports)`: hands out
    /// `transports` in order and then none, counting the calls.
    fn opener(transports: Vec<Fake>) -> (Opener, Arc<AtomicUsize>) {
        let calls = Arc::new(AtomicUsize::new(0));
        let counted = Arc::clone(&calls);
        let mut transports = VecDeque::from(transports);
        let opener: Opener = Box::new(move |_stop: &AtomicBool| {
            counted.fetch_add(1, Ordering::SeqCst);
            transports
                .pop_front()
                .map(|transport| Box::new(transport) as Box<dyn Transport>)
        });
        (opener, calls)
    }

    /// The Python tests' `Presence(client_id='123')` with `opener` in place of `open_transport`.
    fn setup(opener: Opener, retry: Duration) -> Setup {
        Setup {
            default_id: "123".to_string(),
            retry,
            opener,
        }
    }

    fn start(pid: u32, env: &[(&str, &str)], setup: Setup) -> Presence {
        Presence::start(pid, &env_of(env), setup).unwrap()
    }

    /// Python `presence.thread.is_alive()`.
    fn running(presence: &Presence) -> bool {
        presence
            .worker
            .as_ref()
            .is_some_and(|worker| !worker.handle.is_finished())
    }

    // -- Framing ------------------------------------------------------------------------------------

    #[test]
    fn pack_has_opcode_length_and_utf8_body() {
        let data = pack(Opcode::Frame, &json!({"a": "ñ"}));
        let (opcode, body) = decode(&data);
        assert_eq!(opcode, Opcode::Frame as u32);
        let length = u32::from_le_bytes(data[4..8].try_into().unwrap()) as usize;
        assert_eq!(length, data.len() - 8);
        assert_eq!(body, json!({"a": "ñ"}));
        // `separators=(',', ':')` and `ensure_ascii=False`: compact, the ñ as UTF-8.
        assert_eq!(&data[8..], "{\"a\":\"ñ\"}".as_bytes());
    }

    #[test]
    fn read_frame_reassembles_split_reads() {
        let mut transport = Fake {
            chunk: 3,
            ..Fake::new(&[ready()])
        };
        let frame = read_frame(&mut transport, &never(), Some(READ)).unwrap();
        assert_eq!(frame, (Opcode::Frame, ready()));
        assert!(transport.wire.with(|wire| wire.reads) > 2);
    }

    #[test]
    fn ping_echoes_raw_bytes_before_the_next_frame() {
        let mut transport = Fake::new(&[ready()]);
        let payload = b"\xff\x00ping";
        transport.wire.with(|wire| {
            let rest = std::mem::take(&mut wire.incoming);
            wire.incoming = [&header(Opcode::Ping, payload.len())[..], payload, &rest].concat();
        });
        let frame = read_frame(&mut transport, &never(), Some(READ)).unwrap();
        assert_eq!(frame, (Opcode::Frame, ready()));
        let pong = [&header(Opcode::Pong, payload.len())[..], payload].concat();
        assert_eq!(transport.wire.with(|wire| wire.written.clone()), [pong]);
    }

    #[test]
    fn partial_frame_survives_a_read_timeout() {
        let mut transport = Fake {
            chunk: 3,
            timeout_at: Some(2),
            ..Fake::new(&[ready()])
        };
        let frame = read_frame(&mut transport, &never(), Some(READ)).unwrap();
        assert_eq!(frame, (Opcode::Frame, ready()));
    }

    #[test]
    fn read_frame_raises_when_connection_ends() {
        let error = read_frame(&mut Fake::new(&[]), &never(), Some(READ)).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::ConnectionReset);
    }

    #[test]
    fn read_frame_refuses_what_discord_never_sends() {
        let frame_of = |bytes: Vec<u8>| {
            let transport = Fake::new(&[]);
            transport.wire.with(|wire| wire.incoming = bytes);
            let mut transport = transport;
            read_frame(&mut transport, &never(), Some(READ)).map_err(|error| error.kind())
        };
        let raw = |opcode: u32, body: &[u8]| {
            let length = (body.len() as u32).to_le_bytes();
            [&opcode.to_le_bytes()[..], &length, body].concat()
        };
        let too_large = [&1u32.to_le_bytes()[..], &(1024 * 1024 + 1u32).to_le_bytes()].concat();
        assert_eq!(frame_of(too_large), Err(io::ErrorKind::InvalidData));
        assert_eq!(frame_of(raw(0, b"{}")), Err(io::ErrorKind::InvalidData));
        assert_eq!(frame_of(raw(9, b"{}")), Err(io::ErrorKind::InvalidData));
        assert_eq!(frame_of(raw(1, b"[1]")), Err(io::ErrorKind::InvalidData));
        assert_eq!(frame_of(raw(1, b"{")), Err(io::ErrorKind::InvalidData));
        assert_eq!(frame_of(raw(2, b"{}")), Err(io::ErrorKind::ConnectionReset));
        // A PONG is skipped; an empty FRAME body is an empty object.
        let pong_then_empty = [raw(4, b"{}"), raw(1, b"")].concat();
        assert_eq!(frame_of(pong_then_empty), Ok((Opcode::Frame, json!({}))));
    }

    #[test]
    fn frames_are_laid_out_as_the_python_writes_them() {
        let mut transport = Fake::new(&[ready()]);
        handshake(&mut transport, CLIENT_ID, &never()).unwrap();
        let hello = br#"{"v":1,"client_id":"1560048868677132431"}"#;
        let length = hello.len() as u8;
        let expected = [&[0, 0, 0, 0, length, 0, 0, 0][..], hello].concat();
        assert_eq!(transport.wire.with(|wire| wire.written.clone()), [expected]);
        let request = set_activity(7, Some(activity(5)));
        let text = std::str::from_utf8(&request[HEADER..]).unwrap();
        let prefix = r#"{"cmd":"SET_ACTIVITY","args":{"pid":7,"activity":{"timestamps":{"start":5}}},"nonce":""#;
        assert!(text.starts_with(prefix) && text.ends_with("\"}"), "{text}");
    }

    // -- Activity -----------------------------------------------------------------------------------

    #[test]
    fn only_the_start_time_is_shown() {
        assert_eq!(activity(100), json!({"timestamps": {"start": 100}}));
    }

    #[test]
    fn clearing_omits_the_activity() {
        let (_, body) = decode(&set_activity(7, None));
        assert_eq!(body["args"], json!({"pid": 7}));
        assert_eq!(body["cmd"], "SET_ACTIVITY");
    }

    #[test]
    fn each_request_has_its_own_uuid4_nonce() {
        let nonce_of = || {
            let (_, body) = decode(&set_activity(7, Some(activity(1))));
            body["nonce"].as_str().unwrap().to_string()
        };
        let (first, second) = (nonce_of(), nonce_of());
        assert_ne!(first, second);
        for nonce in [first, second] {
            let groups: Vec<&str> = nonce.split('-').collect();
            let lengths: Vec<usize> = groups.iter().map(|group| group.len()).collect();
            assert_eq!(lengths, [8, 4, 4, 4, 12], "{nonce}");
            assert!(
                nonce
                    .chars()
                    .all(|c| c == '-' || c.is_ascii_digit() || ('a'..='f').contains(&c)),
                "{nonce}"
            );
            assert!(groups[2].starts_with('4'), "{nonce}");
            assert!("89ab".contains(&groups[3][..1]), "{nonce}");
        }
    }

    // -- Where Discord listens ----------------------------------------------------------------------

    #[test]
    fn candidates_are_discords_ten_pipes_or_its_sockets() {
        let env = env_of(&[
            ("XDG_RUNTIME_DIR", "/run/user/1000"),
            ("TMPDIR", "/tmp"),
            ("TMP", ""),
            ("TEMP", "/var/tmp"),
        ]);
        let sockets = socket_candidates(&env);
        if cfg!(windows) {
            let pipes: Vec<PathBuf> = (0..10)
                .map(|index| PathBuf::from(format!(r"\\?\pipe\discord-ipc-{index}")))
                .collect();
            assert_eq!(candidates(&env), pipes);
        } else {
            assert_eq!(candidates(&env), sockets);
        }
        // Each base once, empty ones skipped and /tmp last; each also under Flatpak's and Snap's folder.
        let socket = |base: &str, sub: &str, index: usize| {
            let folder = if sub.is_empty() {
                PathBuf::from(base)
            } else {
                Path::new(base).join(sub)
            };
            folder.join(format!("discord-ipc-{index}"))
        };
        let mut expected = Vec::new();
        for base in ["/run/user/1000", "/tmp", "/var/tmp"] {
            for sub in ["", "app/com.discordapp.Discord", "snap.discord"] {
                expected.extend((0..10).map(|index| socket(base, sub, index)));
            }
        }
        assert_eq!(sockets, expected);
        let only_tmp = socket_candidates(&env_of(&[]));
        assert_eq!(only_tmp.len(), 30);
        assert_eq!(only_tmp[0], socket("/tmp", "", 0));
    }

    /// Somewhere the presence can connect to, held open while the guard lives.
    #[cfg(windows)]
    fn listening() -> (PipeServer, PathBuf) {
        let server = PipeServer::new();
        let path = server.path.clone();
        (server, path)
    }

    #[cfg(not(windows))]
    fn listening() -> (
        (tempfile::TempDir, std::os::unix::net::UnixListener),
        PathBuf,
    ) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("discord-ipc-0");
        let listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
        ((dir, listener), path)
    }

    fn missing() -> PathBuf {
        if cfg!(windows) {
            PathBuf::from(format!(r"\\.\pipe\spartanport-rpc-missing-{}", nonce()))
        } else {
            std::env::temp_dir().join(format!("spartanport-rpc-missing-{}", nonce()))
        }
    }

    #[test]
    fn open_transport_skips_missing_candidates_and_stops_when_asked() {
        let (_server, path) = listening();
        assert!(open_transport(&[missing()], &never()).is_none());
        assert!(open_transport(&[missing(), path], &never()).is_some());
        let (_server, path) = listening();
        let stopped = AtomicBool::new(true);
        assert!(open_transport(std::slice::from_ref(&path), &stopped).is_none());
        assert!(open_transport(&[path], &never()).is_some());
    }

    // -- PresenceLifecycle --------------------------------------------------------------------------

    #[test]
    fn disabled_without_client_id() {
        let (opener, calls) = opener(vec![]);
        let presence = start(
            1,
            &[],
            Setup {
                default_id: String::new(),
                ..setup(opener, RETRY)
            },
        );
        assert!(presence.worker.is_none());
        presence.close();
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn disabled_by_switch() {
        let (opener, calls) = opener(vec![]);
        let presence = start(1, &[("SPARTAN_DISCORD", "0")], setup(opener, RETRY));
        assert!(presence.worker.is_none());
        presence.close();
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        // The runner's entry points read the switch before anything is opened.
        let off = env_of(&[("SPARTAN_DISCORD", "0")]);
        assert!(Presence::new(1, &off).worker.is_none());
        let mut log = Vec::new();
        assert_eq!(
            while_running(1, &off, &mut |line| log.push(line), || 17),
            17
        );
        assert!(log.is_empty(), "{log:?}");
    }

    #[test]
    fn the_client_id_variable_replaces_the_default() {
        for (env, id) in [
            (vec![("SPARTAN_DISCORD_ID", "999")], "999"),
            (vec![("SPARTAN_DISCORD_ID", "")], "123"),
        ] {
            let transport = Fake::live(&[ready(), answered()]);
            let wire = transport.wire.clone();
            let (opener, _) = opener(vec![transport]);
            let presence = start(1, &env, setup(opener, RETRY));
            assert!(wire.wait(SECOND, |wire| wire.published));
            presence.close();
            assert_eq!(wire.frames()[0].1["client_id"], id);
        }
    }

    #[test]
    fn handshake_activity_and_clean_close() {
        let transport = Fake::live(&[ready(), answered()]);
        let wire = transport.wire.clone();
        let (opener, _) = opener(vec![transport]);
        let pid = std::process::id();
        let before = unix_time();
        let presence = start(pid, &[], setup(opener, RETRY));
        let after = unix_time();
        assert!(wire.wait(SECOND, |wire| wire.published));
        presence.close();
        let frames = wire.frames();
        let hello = json!({"v": 1, "client_id": "123"});
        assert_eq!(frames[0], (Opcode::Handshake as u32, hello));
        let (opcode, body) = &frames[1];
        assert_eq!(*opcode, Opcode::Frame as u32);
        assert_eq!(body["args"]["pid"], pid);
        // Python `int(presence.started)`: the whole seconds of when the presence was made.
        let started = body["args"]["activity"]["timestamps"]["start"]
            .as_u64()
            .unwrap();
        assert!((before..=after).contains(&started), "{started}");
        let shown = json!({"timestamps": {"start": started}});
        assert_eq!(body["args"]["activity"], shown);
        assert!(frames[2].1["args"].get("activity").is_none());
        assert_eq!(frames[3].0, Opcode::Close as u32);
        assert!(wire.closed());
    }

    #[test]
    fn reconnect_republishes_original_start_time() {
        let first = Fake::new(&[ready(), answered()]);
        let second = Fake::live(&[ready(), answered()]);
        let (one, two) = (first.wire.clone(), second.wire.clone());
        let (opener, calls) = opener(vec![first, second]);
        let presence = start(1, &[], setup(opener, Duration::from_millis(10)));
        assert!(
            two.wait(SECOND, |wire| wire.published),
            "presence was not republished"
        );
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        assert!(one.closed());
        let shown = |wire: &Shared| wire.frames()[1].1["args"]["activity"].clone();
        assert_eq!(shown(&one), shown(&two));
        presence.close();
    }

    #[test]
    fn connected_session_answers_ping_and_keeps_listening() {
        let transport = Fake::live(&[ready(), answered()]);
        let wire = transport.wire.clone();
        let (opener, _) = opener(vec![transport]);
        let presence = start(1, &[], setup(opener, RETRY));
        assert!(wire.wait(SECOND, |wire| wire.published));
        let ping = pack(Opcode::Ping, &json!({"probe": "alive"}));
        wire.with(|wire| wire.incoming.extend(ping));
        assert!(wire.wait(SECOND, |wire| wire.pong), "PING was not answered");
        assert!(running(&presence));
        let pong = (Opcode::Pong as u32, json!({"probe": "alive"}));
        assert!(wire.frames().contains(&pong));
        presence.close();
    }

    #[test]
    fn close_interrupts_a_silent_handshake_without_reconnecting() {
        let transport = Fake::live(&[]);
        let wire = transport.wire.clone();
        let (opener, calls) = opener(vec![transport]);
        let mut presence = start(1, &[], setup(opener, RETRY));
        assert!(wire.wait(SECOND, |wire| wire.reading));
        let began = Instant::now();
        let finished = presence.shutdown();
        assert!(began.elapsed() < Duration::from_secs(3));
        assert!(finished);
        assert!(wire.closed());
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn native_pid_is_used_for_publish_and_clear() {
        let transport = Fake::live(&[ready(), answered()]);
        let wire = transport.wire.clone();
        let (opener, _) = opener(vec![transport]);
        let presence = start(4321, &[], setup(opener, RETRY));
        assert!(wire.wait(SECOND, |wire| wire.published));
        presence.close();
        let frames = wire.frames();
        assert_eq!(frames[1].1["args"]["pid"], 4321);
        assert_eq!(frames[2].1["args"], json!({"pid": 4321}));
    }

    #[test]
    fn rejected_handshake_closes_and_retries_later() {
        let transport = Fake::new(&[json!({"evt": "ERROR"})]);
        let wire = transport.wire.clone();
        let (opener, calls) = opener(vec![transport]);
        let presence = start(1, &[], setup(opener, Duration::from_secs(60)));
        let closed = wire.wait(Duration::from_secs(5), |wire| wire.closed);
        presence.close();
        assert!(closed);
        // The next try waits for RETRY (60 s here).
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn an_error_event_ends_the_session_without_clearing() {
        // Refused at SET_ACTIVITY, or later while listening: the connection ends, nothing is
        // cleared (nothing was shown), and the next try waits for RETRY.
        let refused = Fake::live(&[ready(), json!({"evt": "ERROR"})]);
        let later = Fake::live(&[ready(), answered()]);
        for (transport, published) in [(refused, false), (later, true)] {
            let wire = transport.wire.clone();
            let (opener, calls) = opener(vec![transport]);
            let presence = start(1, &[], setup(opener, Duration::from_secs(60)));
            if published {
                assert!(wire.wait(SECOND, |wire| wire.published));
                let error = pack(Opcode::Frame, &json!({"evt": "ERROR"}));
                wire.with(|wire| wire.incoming.extend(error));
            }
            assert!(wire.wait(SECOND, |wire| wire.closed));
            presence.close();
            assert_eq!(wire.frames().len(), 2);
            assert_eq!(calls.load(Ordering::SeqCst), 1);
        }
    }

    #[test]
    fn discord_not_running_does_not_raise() {
        let (opener, _) = opener(vec![]);
        let mut presence = start(1, &[], setup(opener, Duration::from_secs(60)));
        assert!(presence.shutdown());
        assert!(!running(&presence));
    }

    #[test]
    fn close_without_connection_is_harmless() {
        let (opener, _) = opener(vec![]);
        let mut presence = start(1, &[], setup(opener, RETRY));
        presence.shutdown();
        presence.close();
    }

    // -- GameProcessPresence (run.py: the presence lives while the runner waits for the game) -------

    #[test]
    fn game_pid_is_used_and_presence_closes_after_wait() {
        let transport = Fake::live(&[ready(), answered()]);
        let wire = transport.wire.clone();
        let (opener, _) = opener(vec![transport]);
        let (mut made, mut waits, mut log) = (Vec::new(), 0, Vec::new());
        let code = while_running_with(
            4321,
            |pid| {
                made.push(pid);
                Presence::start(pid, &env_of(&[]), setup(opener, RETRY))
            },
            &mut |line| log.push(line),
            || {
                waits += 1;
                assert!(wire.wait(SECOND, |wire| wire.published));
                // The game still runs: the presence is not closed yet.
                assert!(!wire.closed());
                17
            },
        );
        assert_eq!(code, 17);
        assert_eq!(made, [4321]);
        assert_eq!(waits, 1);
        assert!(log.is_empty(), "{log:?}");
        let frames = wire.frames();
        assert_eq!(frames[1].1["args"]["pid"], 4321);
        // Closed once, after the wait: one clear and one CLOSE, and the connection let go.
        assert_eq!(count(&frames, Opcode::Close), 1);
        assert!(frames[frames.len() - 2].1["args"].get("activity").is_none());
        assert!(wire.closed());
    }

    #[test]
    fn presence_failure_does_not_interrupt_the_game() {
        let (mut waits, mut log) = (0, Vec::new());
        let code = while_running_with(
            4321,
            |_| Err(io::Error::other("thread unavailable")),
            &mut |line| log.push(line),
            || {
                waits += 1;
                17
            },
        );
        assert_eq!(code, 17);
        assert_eq!(waits, 1);
        assert_eq!(
            log,
            ["Discord presence unavailable; the game will continue"]
        );
    }

    #[test]
    fn wait_exception_still_cleans_up_presence() {
        let transport = Fake::live(&[ready(), answered()]);
        let wire = transport.wire.clone();
        let (opener, _) = opener(vec![transport]);
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            while_running_with(
                4321,
                |pid| Presence::start(pid, &env_of(&[]), setup(opener, RETRY)),
                &mut |_| {},
                || -> i32 {
                    assert!(wire.wait(SECOND, |wire| wire.published));
                    panic!("the wait was interrupted");
                },
            )
        }));
        assert!(outcome.is_err());
        assert_eq!(count(&wire.frames(), Opcode::Close), 1);
        assert!(wire.closed());
    }

    // -- SocketIntegration (Unix sockets) -----------------------------------------------------------

    /// The listener's next connection within `limit` (Python `listener.settimeout(2)`).
    #[cfg(not(windows))]
    fn accept(
        listener: &std::os::unix::net::UnixListener,
        limit: Duration,
    ) -> std::os::unix::net::UnixStream {
        listener.set_nonblocking(true).unwrap();
        let deadline = Instant::now() + limit;
        loop {
            match listener.accept() {
                Ok((conn, _)) => {
                    conn.set_nonblocking(false).unwrap();
                    return conn;
                }
                Err(error)
                    if error.kind() == io::ErrorKind::WouldBlock && Instant::now() < deadline =>
                {
                    thread::sleep(Duration::from_millis(10));
                }
                Err(error) => panic!("no connection within {limit:?}: {error}"),
            }
        }
    }

    #[cfg(not(windows))]
    #[test]
    fn real_socket_reconnect_ping_and_shutdown() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("discord-ipc-0");
        let listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
        let (pong_sent, pong) = mpsc::channel();
        let server = thread::spawn(move || {
            let never = never();
            let mut starts = Vec::new();
            for index in 0..2 {
                let conn = accept(&listener, Duration::from_secs(2));
                let mut transport = SocketTransport { stream: Some(conn) };
                let hello = pack(Opcode::Handshake, &json!({"v": 1, "client_id": "123"}));
                let got = read_exact(&mut transport, hello.len(), &never, Some(READ)).unwrap();
                assert_eq!(got, hello);
                transport.write(&pack(Opcode::Frame, &ready())).unwrap();
                let (_, request) = read_frame(&mut transport, &never, Some(READ)).unwrap();
                let start = &request["args"]["activity"]["timestamps"]["start"];
                starts.push(start.as_u64().unwrap());
                transport.write(&pack(Opcode::Frame, &answered())).unwrap();
                if index == 0 {
                    continue;
                }
                let ping = [&header(Opcode::Ping, 2)[..], b"\xff\x00"].concat();
                transport.write(&ping).unwrap();
                let echo = [&header(Opcode::Pong, 2)[..], b"\xff\x00"].concat();
                assert_eq!(
                    read_exact(&mut transport, 10, &never, Some(READ)).unwrap(),
                    echo
                );
                pong_sent.send(()).unwrap();
                let (_, clear) = read_frame(&mut transport, &never, Some(READ)).unwrap();
                assert!(clear["args"].get("activity").is_none());
                let close = pack(Opcode::Close, &json!({}));
                assert_eq!(
                    read_exact(&mut transport, 10, &never, Some(READ)).unwrap(),
                    close
                );
            }
            starts
        });
        let opener: Opener =
            Box::new(move |stop: &AtomicBool| open_transport(std::slice::from_ref(&path), stop));
        let env = [("SPARTAN_DISCORD", "1")];
        let before = unix_time();
        let mut presence = start(1, &env, setup(opener, Duration::from_millis(10)));
        let after = unix_time();
        let ponged = pong.recv_timeout(Duration::from_secs(2)).is_ok();
        let finished = presence.shutdown();
        assert!(ponged);
        let starts = server.join().unwrap();
        assert!(finished);
        // Python `[int(presence.started)] * 2`: both connections show when the presence was made.
        assert_eq!(starts.len(), 2);
        assert_eq!(starts[0], starts[1]);
        assert!((before..=after).contains(&starts[0]), "{starts:?}");
    }

    // -- WindowsPipeTransport (named pipes) ---------------------------------------------------------

    /// The server end of a uniquely named pipe (Python `WindowsPipeTransport.setUp`): duplex, byte
    /// mode, blocking, one instance, 1024-byte buffers.
    #[cfg(windows)]
    struct PipeServer {
        handle: windows_sys::Win32::Foundation::HANDLE,
        path: PathBuf,
    }

    #[cfg(windows)]
    impl PipeServer {
        fn new() -> PipeServer {
            use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
            use windows_sys::Win32::Storage::FileSystem::PIPE_ACCESS_DUPLEX;
            use windows_sys::Win32::System::Pipes::CreateNamedPipeW;
            let path = format!(r"\\.\pipe\spartanport-rpc-test-{}", nonce());
            let name: Vec<u16> = path.encode_utf16().chain([0]).collect();
            // SAFETY: `name` is a NUL-terminated UTF-16 string that outlives the call; there are no
            // security attributes (null).
            let handle = unsafe {
                CreateNamedPipeW(
                    name.as_ptr(),
                    PIPE_ACCESS_DUPLEX,
                    0,
                    1,
                    1024,
                    1024,
                    0,
                    std::ptr::null(),
                )
            };
            assert_ne!(
                handle,
                INVALID_HANDLE_VALUE,
                "{}",
                io::Error::last_os_error()
            );
            PipeServer {
                handle,
                path: PathBuf::from(path),
            }
        }

        /// Python `ConnectNamedPipe` once the client opened its end: "already connected" (535)
        /// counts as connected.
        fn connect(&self) {
            use windows_sys::Win32::Foundation::ERROR_PIPE_CONNECTED;
            use windows_sys::Win32::System::Pipes::ConnectNamedPipe;
            // SAFETY: the handle is the live server end, opened without FILE_FLAG_OVERLAPPED, so
            // the OVERLAPPED pointer is null.
            let connected = unsafe { ConnectNamedPipe(self.handle, std::ptr::null_mut()) };
            let error = io::Error::last_os_error();
            let already = error.raw_os_error() == Some(ERROR_PIPE_CONNECTED as i32);
            assert!(connected != 0 || already, "{error}");
        }

        /// The bytes the client sent that nobody read yet; an error before it connects and once
        /// it is gone.
        fn available(&self) -> io::Result<u32> {
            use windows_sys::Win32::System::Pipes::PeekNamedPipe;
            let mut available = 0u32;
            // SAFETY: the handle is the live server end; there is no buffer (null, 0) and
            // `available` is a valid u32 to write; the other two out pointers are null.
            let peeked = unsafe {
                PeekNamedPipe(
                    self.handle,
                    std::ptr::null_mut(),
                    0,
                    std::ptr::null_mut(),
                    &mut available,
                    std::ptr::null_mut(),
                )
            };
            if peeked == 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(available)
        }

        fn read(&self, size: usize) -> Vec<u8> {
            use windows_sys::Win32::Storage::FileSystem::ReadFile;
            let mut data = vec![0u8; size];
            let mut read = 0u32;
            // SAFETY: the pointer and size describe `data`; `read` is a valid u32 to write; the
            // handle is synchronous, so the OVERLAPPED pointer is null.
            let done = unsafe {
                ReadFile(
                    self.handle,
                    data.as_mut_ptr(),
                    size as u32,
                    &mut read,
                    std::ptr::null_mut(),
                )
            };
            assert_ne!(done, 0, "{}", io::Error::last_os_error());
            data.truncate(read as usize);
            data
        }

        fn write(&self, data: &[u8]) {
            use windows_sys::Win32::Storage::FileSystem::WriteFile;
            let mut written = 0u32;
            // SAFETY: the pointer and size describe `data`, which the call only reads; `written`
            // is a valid u32 to write; the handle is synchronous, so the OVERLAPPED pointer is null.
            let done = unsafe {
                WriteFile(
                    self.handle,
                    data.as_ptr(),
                    data.len() as u32,
                    &mut written,
                    std::ptr::null_mut(),
                )
            };
            assert!(
                done != 0 && written as usize == data.len(),
                "{}",
                io::Error::last_os_error()
            );
        }
    }

    #[cfg(windows)]
    impl Drop for PipeServer {
        fn drop(&mut self) {
            // SAFETY: the handle is owned here and closed once.
            unsafe { windows_sys::Win32::Foundation::CloseHandle(self.handle) };
        }
    }

    /// Python `WindowsPipeTransport.setUp`: the server, and the presence's transport on the client
    /// end, connected.
    #[cfg(windows)]
    fn pipe_pair() -> (PipeServer, PipeTransport) {
        let server = PipeServer::new();
        let transport = PipeTransport::open(&server.path).unwrap();
        server.connect();
        (server, transport)
    }

    #[cfg(windows)]
    #[test]
    fn native_pipe_reads_and_writes_without_blocking() {
        let (server, mut transport) = pipe_pair();
        transport.write(b"client").unwrap();
        assert_eq!(server.read(6), b"client");
        server.write(b"server");
        assert_eq!(transport.read(6, Some(POLL)).unwrap(), b"server");
        let began = Instant::now();
        let error = transport.read(1, Some(POLL)).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        assert!(began.elapsed() < SECOND);
        let began = Instant::now();
        assert!(transport.write(&vec![b'x'; 1024 * 1024]).is_err());
        assert!(began.elapsed() < SECOND);
    }

    #[cfg(windows)]
    #[test]
    fn close_stops_worker_waiting_for_a_native_pipe_handshake() {
        let (server, transport) = pipe_pair();
        let mut handed = Some(transport);
        let calls = Arc::new(AtomicUsize::new(0));
        let counted = Arc::clone(&calls);
        let opener: Opener = Box::new(move |_stop: &AtomicBool| {
            counted.fetch_add(1, Ordering::SeqCst);
            handed
                .take()
                .map(|transport| Box::new(transport) as Box<dyn Transport>)
        });
        let mut presence = start(1, &[("SPARTAN_DISCORD", "1")], setup(opener, RETRY));
        let deadline = Instant::now() + SECOND;
        while server.available().unwrap() == 0 && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
        assert!(server.available().unwrap() > 0);
        let began = Instant::now();
        let finished = presence.shutdown();
        assert!(began.elapsed() < Duration::from_secs(3));
        assert!(finished);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    // -- Review Focus: a Discord pipe that answers nothing ------------------------------------------

    /// A Discord client that takes the connection, reads the first `size` bytes and never answers.
    #[cfg(windows)]
    struct Silent {
        server: PipeServer,
        size: usize,
    }

    #[cfg(windows)]
    impl Silent {
        fn new(size: usize) -> Silent {
            Silent {
                server: PipeServer::new(),
                size,
            }
        }

        fn path(&self) -> PathBuf {
            self.server.path.clone()
        }

        /// The bytes the presence sent, once it sent them within `limit`.
        fn received(&self, limit: Duration) -> Option<Vec<u8>> {
            let deadline = Instant::now() + limit;
            while Instant::now() < deadline {
                if self
                    .server
                    .available()
                    .is_ok_and(|available| available as usize >= self.size)
                {
                    self.server.connect();
                    return Some(self.server.read(self.size));
                }
                thread::sleep(Duration::from_millis(10));
            }
            None
        }

        /// Whether the presence let its end of the pipe go within `limit`.
        fn closed(&self, limit: Duration) -> bool {
            let deadline = Instant::now() + limit;
            while Instant::now() < deadline {
                if self.server.available().is_err() {
                    return true;
                }
                thread::sleep(Duration::from_millis(10));
            }
            false
        }
    }

    #[cfg(not(windows))]
    struct Silent {
        _dir: tempfile::TempDir,
        path: PathBuf,
        received: mpsc::Receiver<Vec<u8>>,
        gone: mpsc::Receiver<()>,
    }

    #[cfg(not(windows))]
    impl Silent {
        fn new(size: usize) -> Silent {
            use std::io::Read;
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("discord-ipc-0");
            let listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
            let (sent, received) = mpsc::channel();
            let (left, gone) = mpsc::channel();
            thread::spawn(move || {
                let mut conn = accept(&listener, Duration::from_secs(2));
                let mut hello = vec![0; size];
                conn.read_exact(&mut hello).unwrap();
                let _ = sent.send(hello);
                let mut rest = [0u8; 64];
                while matches!(conn.read(&mut rest), Ok(read) if read > 0) {}
                let _ = left.send(());
            });
            Silent {
                _dir: dir,
                path,
                received,
                gone,
            }
        }

        fn path(&self) -> PathBuf {
            self.path.clone()
        }

        fn received(&self, limit: Duration) -> Option<Vec<u8>> {
            self.received.recv_timeout(limit).ok()
        }

        fn closed(&self, limit: Duration) -> bool {
            self.gone.recv_timeout(limit).is_ok()
        }
    }

    #[test]
    fn a_silent_pipe_times_out_and_close_returns_in_time() {
        let hello = pack(Opcode::Handshake, &json!({"v": 1, "client_id": CLIENT_ID}));
        // A presence on this system's real transport, at the test's own pipe.
        let presence_at = |server: &Silent| {
            let path = server.path();
            let calls = Arc::new(AtomicUsize::new(0));
            let counted = Arc::clone(&calls);
            let opener: Opener = Box::new(move |stop: &AtomicBool| {
                counted.fetch_add(1, Ordering::SeqCst);
                open_transport(std::slice::from_ref(&path), stop)
            });
            let setup = Setup {
                default_id: CLIENT_ID.to_string(),
                retry: RETRY,
                opener,
            };
            let began = Instant::now();
            let presence = start(4321, &[], setup);
            // The runner waits for the game right after this: nothing here waits for Discord. A
            // start that waited for the silent pipe's handshake would take READ at least.
            assert!(began.elapsed() < READ);
            (presence, calls, began)
        };

        // No answer: the handshake read gives up after READ and lets the pipe go; the next try
        // would come after RETRY, which close does not wait for.
        let server = Silent::new(hello.len());
        let (mut presence, calls, began) = presence_at(&server);
        assert_eq!(server.received(2 * SECOND), Some(hello.clone()));
        let asked = Instant::now();
        assert!(
            server.closed(READ + 2 * SECOND),
            "the handshake read never gave up"
        );
        // The read's deadline starts after the hello is written, so after `began`: it cannot
        // give up sooner than READ from there, however late this thread runs. The hello was
        // written before `asked`, so the read gave up at most 1.5 s past READ.
        let (since_start, since_hello) = (began.elapsed(), asked.elapsed());
        assert!(since_start >= READ, "{since_start:?}");
        assert!(
            since_hello < READ + Duration::from_millis(1500),
            "{since_hello:?}"
        );
        let began = Instant::now();
        assert!(presence.shutdown());
        assert!(began.elapsed() < JOIN);
        assert_eq!(calls.load(Ordering::SeqCst), 1);

        // Closed while the handshake read still waits: close returns in time and the pipe goes.
        let server = Silent::new(hello.len());
        let (mut presence, calls, _) = presence_at(&server);
        assert_eq!(server.received(2 * SECOND), Some(hello));
        let began = Instant::now();
        assert!(presence.shutdown());
        assert!(began.elapsed() < JOIN);
        assert!(server.closed(SECOND));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
}
