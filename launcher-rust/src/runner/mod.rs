//! The runner (Python `run.py`): starts the game under the probe and reports what it saw.

pub mod content_profile;
pub mod discord;
pub mod dxt;
pub mod game_update;
pub mod glyphs;
pub mod graphics_options;
pub mod guest_memory;
pub mod isa_rewrite;
pub mod link_libc;
pub mod link_modules;
pub mod pause_menu;
pub mod prepare;
pub mod window_icon;

/// The synthetic probe inputs the runner's tests build (`tests/test_probe.py::package` and more).
#[cfg(test)]
#[path = "../../tests/common/packages.rs"]
mod packages;

use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

use crate::core::env::game_environment;
use crate::core::i18n::{launcher_language, system_language, tr};
use crate::core::legacy;
use crate::core::log::{exit_hint, RunLog};
use crate::core::mods::{self, Overlay};
use crate::core::options::RESOLUTIONS;
use crate::core::patches::{self, py_int, py_list, PatchJob, PatchStatus};
use crate::core::paths::Paths;
use crate::core::platform;
use crate::core::save_backup;
use crate::core::settings::Settings;
use crate::core::sfo::SfoValue;
use content_profile::Sku;
use pause_menu::python_path;

use indexmap::IndexMap;

/// What one run needs from the program around it.
pub struct RunContext<'a> {
    pub port_dir: &'a Path,
    pub env: IndexMap<String, String>,
    pub args: Vec<OsString>,
    pub out: &'a mut dyn Write,
}

/// How long `--after` waits for the previous game process (`run.py` `wait_for`).
pub const AFTER_LIMIT: Duration = Duration::from_secs(30);
/// The exit code after Ctrl+C (`run.py` on `KeyboardInterrupt`).
pub const INTERRUPTED: i32 = 130;
/// The probe's exit code for a CPU without an instruction set the ISA rewrite covers.
pub const UNSUPPORTED_CPU: i32 = 20;
const EXE_SUFFIX: &str = if cfg!(windows) { ".exe" } else { "" };
/// The patches' default engine and VBlank rate (`run.py`).
const DEFAULT_RATE: u32 = 60;

/// Prepares the game image and starts the probe on it (`run.py` `main`, spec §5): `--after <pid>`
/// first waits for that process; the other arguments go to the probe. Returns the probe's exit
/// code; 1 when the run stops on an error (its message printed, as `run.py`'s `fail`), 20 for an
/// unsupported CPU, 130 after Ctrl+C. Everything it prints goes to `context.out`, in English and
/// as `run.py` prints it (the launcher reads its Log and `exit_hint` from these lines); the probe
/// and the game print to this process's own output. The current folder becomes the port's, as
/// `run.sh` does: relative paths in the environment are taken from there.
pub fn run(context: RunContext<'_>) -> i32 {
    platform::catch_interrupt();
    let RunContext {
        port_dir,
        env,
        args,
        out,
    } = context;
    let mut runner = Runner::new(out, env);
    let code = runner.start(port_dir, args);
    if platform::interrupted() {
        INTERRUPTED
    } else {
        code
    }
}

/// The variables the applied patches decide after `patches.py` (`run.py`), in the order it sets
/// them: `SPARTAN_DMEM_MB` when the notes ask for more direct memory, `SPARTAN_VBLANK_HZ` (the
/// notes' rate, else 60) and `SPARTAN_ENGINE_FPS` (`engine_fps`, else the rate of the applied frame
/// rate patch, else 60). `None`, for a game without a patch profile or a `patch_status.json` that is
/// missing or cut short, gives the defaults. The first two are only set when the environment lacks
/// them; the engine rate always is.
pub fn patch_environment(status: Option<&PatchStatus>) -> Vec<(String, String)> {
    let mut pairs = Vec::new();
    if let Some(dmem) = status.and_then(|s| s.dmem_mb).filter(|&mb| mb != 0) {
        pairs.push(("SPARTAN_DMEM_MB".to_string(), dmem.to_string()));
    }
    let vblank = status
        .and_then(|s| s.vblank_hz)
        .filter(|&hz| hz != 0)
        .unwrap_or(DEFAULT_RATE);
    pairs.push(("SPARTAN_VBLANK_HZ".to_string(), vblank.to_string()));
    pairs.push((
        "SPARTAN_ENGINE_FPS".to_string(),
        engine_fps(status).to_string(),
    ));
    pairs
}

/// The highest engine rate a launch with `status` allows (`run.py`: 60 without a frame rate patch).
fn engine_fps(status: Option<&PatchStatus>) -> u32 {
    status.map_or(DEFAULT_RATE, |status| {
        status
            .engine_fps
            .unwrap_or_else(|| graphics_options::applied_engine_fps(&status.applied))
    })
}

/// `SPARTAN_RESTART_COMMAND`, what the in-game "Apply and restart" starts (`run.py`, from
/// `sys.executable`): on Windows `<exe> --run --after <pid> <args>` quoted as
/// `subprocess.list2cmdline` does (the probe starts it and ends, so the new launch waits for this
/// process); elsewhere `<exe> --run <args>` quoted as `shlex.join` does (the probe replaces itself
/// with it through `sh -c` while this process waits).
pub fn restart_command(exe: &Path, pid: u32, args: &[OsString]) -> String {
    let mut words = vec![exe.to_string_lossy().into_owned(), "--run".to_string()];
    if cfg!(windows) {
        words.extend(["--after".to_string(), pid.to_string()]);
    }
    words.extend(args.iter().map(|arg| arg.to_string_lossy().into_owned()));
    if cfg!(windows) {
        list2cmdline(&words)
    } else {
        shlex_join(&words)
    }
}

/// Python's `subprocess.list2cmdline`: the words as `CommandLineToArgvW` reads them back.
fn list2cmdline(words: &[String]) -> String {
    let mut line = String::new();
    for word in words {
        if !line.is_empty() {
            line.push(' ');
        }
        let quoted = word.is_empty() || word.contains([' ', '\t']);
        if quoted {
            line.push('"');
        }
        let mut backslashes = 0;
        for c in word.chars() {
            match c {
                '\\' => backslashes += 1,
                '"' => {
                    line.push_str(&"\\".repeat(backslashes * 2));
                    backslashes = 0;
                    line.push_str("\\\"");
                }
                c => {
                    line.push_str(&"\\".repeat(backslashes));
                    backslashes = 0;
                    line.push(c);
                }
            }
        }
        line.push_str(&"\\".repeat(backslashes));
        if quoted {
            line.push_str(&"\\".repeat(backslashes));
            line.push('"');
        }
    }
    line
}

/// Python's `shlex.join`: each word as `sh` reads it back.
fn shlex_join(words: &[String]) -> String {
    let safe = |c: char| c.is_ascii_alphanumeric() || "_@%+=:,./-".contains(c);
    words
        .iter()
        .map(|word| {
            if word.is_empty() {
                "''".to_string()
            } else if word.chars().all(safe) {
                word.clone()
            } else {
                format!("'{}'", word.replace('\'', "'\"'\"'"))
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// `--play` (Python `play_without_window`): the game with the saved settings, for a shortcut or
/// Steam. `<this exe> --run` runs in the port folder with the game's environment
/// (`core::env::game_environment`); its output goes to `context.out` and to the saves folder's
/// `last_run.log`. When the game closed on an error an output line explains, the advice is added to
/// the log and, on Windows without a console, shown in a dialog. Returns the game's exit code.
/// (`main` attached the output to a parent console first: Python's `attach_stdio`.)
pub fn play(context: RunContext<'_>, settings: &Settings, paths: &Paths) -> i32 {
    play_with(context, settings, paths, &this_exe(), &mut |advice| {
        #[cfg(windows)]
        if !platform::has_console() {
            platform::show_error(crate::APP_NAME, advice);
        }
        #[cfg(not(windows))]
        let _ = advice;
    })
}

/// [`play`] starting `exe --run` and handing the advice to `notify` instead of showing it.
pub fn play_with(
    context: RunContext<'_>,
    settings: &Settings,
    paths: &Paths,
    exe: &Path,
    notify: &mut dyn FnMut(&str),
) -> i32 {
    let RunContext {
        port_dir, env, out, ..
    } = context;
    let lang = launcher_language(&settings.get_str("ui_language"), system_language);
    let env = game_environment(settings, paths, &lang, &env, cfg!(windows));
    let mut log = RunLog::open(&paths.run_log_path(&settings.get_str("user_dir")));
    let started = io::pipe().and_then(|(reader, writer)| {
        let mut command = Command::new(exe);
        command
            .arg("--run")
            .current_dir(port_dir)
            .env_clear()
            .envs(&env)
            .stdin(Stdio::null())
            .stdout(writer.try_clone()?)
            .stderr(writer);
        platform::no_window(&mut command);
        // `command` holds the pipe's writing ends; once it is gone only the game's remain, so the
        // reader sees the end when the game and what it started are gone.
        let child = command.spawn()?;
        Ok((child, reader))
    });
    let (mut child, reader) = match started {
        Ok(started) => started,
        Err(error) => {
            say(out, &format!("{}: {error}", exe.display()));
            return 1;
        }
    };
    let mut hint = None;
    let mut reader = BufReader::new(reader);
    let mut line = Vec::new();
    loop {
        line.clear();
        match reader.read_until(b'\n', &mut line) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        let text = String::from_utf8_lossy(&line);
        hint = exit_hint(&text).or(hint);
        log.write(&text);
        // A console that went away is no reason to stop reading the game.
        let _ = out.write_all(text.as_bytes());
        let _ = out.flush();
    }
    let code = child.wait().map_or(1, exit_code);
    if let Some(hint) = hint.filter(|_| code != 0) {
        let advice = tr(&lang, hint.text_en());
        log.write(&format!("\n{advice}\n"));
        notify(&advice);
    }
    code
}

/// This program (Python's `sys.executable`), by its name when the system cannot say where it is.
fn this_exe() -> PathBuf {
    std::env::current_exe()
        .unwrap_or_else(|_| PathBuf::from(format!("spartanport-launcher{EXE_SUFFIX}")))
}

/// Writes `line` and flushes it, so it keeps its place among the probe's lines. A closed output is
/// nothing to stop for: the game still runs.
fn say(out: &mut dyn Write, line: &str) {
    let _ = writeln!(out, "{line}");
    let _ = out.flush();
}

/// The exit code of a process; on Linux one ended by a signal gives minus its number, as Python's
/// `returncode`.
fn exit_code(status: ExitStatus) -> i32 {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            return -signal;
        }
    }
    status.code().unwrap_or(1)
}

/// The temporary game folders of a run, removed on every way out (`run.py`'s `finally`): the mods
/// overlay, then the update view. What cannot go is left, as `rmtree(..., ignore_errors=True)`.
#[derive(Default)]
struct Views {
    overlay: Option<Overlay>,
    update: Option<PathBuf>,
}

impl Drop for Views {
    fn drop(&mut self) {
        if let Some(overlay) = self.overlay.take() {
            let _ = overlay.remove();
        }
        if let Some(view) = self.update.take() {
            let _ = game_update::remove_view(&view);
        }
    }
}

/// One run: its output and its environment (the Python's `os.environ`), with the keys it derived.
struct Runner<'a> {
    out: &'a mut dyn Write,
    env: IndexMap<String, String>,
    derived: Vec<String>,
    /// Whether Ctrl+C came (`platform::interrupted`; the tests set their own).
    interrupted: fn() -> bool,
}

impl<'a> Runner<'a> {
    /// On Windows the names are upper case, as `os.environ` makes them: the names there are not
    /// case sensitive, and `Path` must be found as `PATH`.
    fn new(out: &'a mut dyn Write, env: IndexMap<String, String>) -> Runner<'a> {
        let env = if cfg!(windows) {
            env.into_iter()
                .map(|(key, value)| (key.to_uppercase(), value))
                .collect()
        } else {
            env
        };
        Runner {
            out,
            env,
            derived: Vec::new(),
            interrupted: platform::interrupted,
        }
    }

    fn say(&mut self, line: &str) {
        say(self.out, line);
    }

    /// `run.py`'s `fail`: the message, then exit code 1.
    fn fail(&mut self, message: &str) -> i32 {
        self.say(message);
        1
    }

    fn get(&self, key: &str) -> Option<&str> {
        self.env.get(key).map(String::as_str)
    }

    /// `env.get(key, default)`: an empty value counts.
    fn get_or(&self, key: &str, default: &str) -> String {
        self.get(key).unwrap_or(default).to_string()
    }

    /// `Path(env.get(key, default))`.
    fn path_or(&self, key: &str, default: PathBuf) -> PathBuf {
        self.get(key).map_or(default, PathBuf::from)
    }

    fn set(&mut self, key: &str, value: impl Into<String>) {
        self.env.insert(key.to_string(), value.into());
    }

    /// `env.setdefault(key, value)`.
    fn set_default(&mut self, key: &str, value: impl Into<String>) {
        self.env
            .entry(key.to_string())
            .or_insert_with(|| value.into());
    }

    /// Sets a value this run derived: the next start ("Apply and restart") derives it again.
    fn derive(&mut self, key: &str, value: impl Into<String>) {
        self.set(key, value);
        self.derived.push(key.to_string());
    }

    fn lookup(&self) -> impl Fn(&str) -> Option<String> + '_ {
        let env = &self.env;
        move |key| env.get(key).cloned()
    }

    /// A program started with this run's environment, no input and, without a console, no window
    /// of its own (`run.py`'s `stdin=DEVNULL, creationflags=no_console()`).
    fn command(&self, program: impl AsRef<OsStr>) -> Command {
        let mut command = Command::new(program);
        command.env_clear().envs(&self.env).stdin(Stdio::null());
        platform::no_console_window(&mut command);
        command
    }

    /// Spec §5, steps 1 to 16, in `run.py`'s order.
    fn start(&mut self, port_dir: &Path, args: Vec<OsString>) -> i32 {
        // 1. `--after <pid>`: the previous game process has gone (its GPU device and memory).
        let args = match self.after(args) {
            Ok(args) => args,
            Err(code) => return code,
        };
        // 2. As run.sh: relative paths (SPARTAN_GAME_DIR, SPARTAN_DATA_DIR, fsr4_shaders/) are
        // from the port.
        let port = mods::resolve(port_dir).unwrap_or_else(|_| port_dir.to_path_buf());
        if let Err(error) = std::env::set_current_dir(&port) {
            return self.fail(&format!("{}: {error}", python_path(&port)));
        }
        legacy::forward_environment(&mut self.env);
        // 3. The data folder, its out/ and the ini.
        let data = self.path_or("SPARTAN_DATA_DIR", port.clone());
        legacy::copy_game_settings(&data);
        let out = data.join("out");
        if let Err(error) = fs::create_dir_all(&out) {
            return self.fail(&format!("{}: {error}", python_path(&out)));
        }
        self.set_default("SPARTAN_CONFIG", python_path(&data.join("spartanport.ini")));
        // Values the previous start derived ("Apply and restart" inherits them) are derived again
        // from the new options.
        if let Some(keys) = self.env.shift_remove("SPARTAN_DERIVED") {
            for key in keys.split(',') {
                self.env.shift_remove(key);
            }
        }
        // 4. The enabled mods' patch files: their "Resolution Patch - <size>" entries are render
        // resolutions too (SPARTAN_EXTRA_RESOLUTIONS tells the game). A selection that cannot be
        // read stops the start, as run.py's.
        let mods_dir = self.path_or("SPARTAN_MODS_DIR", data.join("mods"));
        let mods_config = self.path_or("SPARTAN_MODS_CONFIG", data.join("mods.json"));
        let mods_enabled = self.get_or("SPARTAN_MODS_ENABLED", "1");
        let parent = port.parent().unwrap_or(&port).to_path_buf();
        let mut game = self.path_or("SPARTAN_GAME_DIR", parent.join("CUSA01623"));
        let mod_patch_files: Vec<PathBuf> = if mods_enabled == "0" {
            Vec::new()
        } else {
            match mods::selected(&mods_dir, Some(&mods_config), Some(&game)) {
                Ok(names) => names
                    .iter()
                    .flat_map(|name| mods::patch_files(&mods_dir.join(name)))
                    .collect(),
                Err(error) => return self.fail(&format!("Mods: {error}")),
            }
        };
        let extra: Vec<String> = patches::resolution_names(&mod_patch_files)
            .into_iter()
            .filter(|size| !RESOLUTIONS.contains(&size.as_str()))
            .collect();
        self.derive("SPARTAN_EXTRA_RESOLUTIONS", extra.join(";"));
        // 5. The start-up graphics options of spartanport.ini.
        let config = PathBuf::from(self.get_or("SPARTAN_CONFIG", ""));
        let graphics = graphics_options::prepare(&config, &out.join("graphics_state.json"), &extra);
        if let Some(message) = &graphics.message {
            self.say(message);
        }
        for (key, value) in graphics_options::environment(&graphics.options) {
            self.set(&key, value);
        }
        if self.get("SPARTAN_FSR411_DIR").is_none_or(str::is_empty)
            && !port.join("fsr4_411").is_dir()
            && data.join("fsr4_411").is_dir()
        {
            self.set("SPARTAN_FSR411_DIR", python_path(&data.join("fsr4_411")));
        }
        // 6. The probe.
        let ports = Paths::from_env(&port, &|_| None);
        let probe = find_executable(&ports, &self.lookup()).filter(|probe| probe.is_file());
        let Some(probe) = probe else {
            return self.fail(&format!(
                "spartanport-probe{EXE_SUFFIX} not found: build it with build.sh or use a packaged \
                 build."
            ));
        };
        let probe_text = python_path(&probe);
        // Development builds run from the MSYS2 tree: its CLANG64 DLLs (SDL3, FFmpeg, ...).
        if cfg!(windows) && !probe.with_file_name("SDL3.dll").is_file() {
            let msys = PathBuf::from(self.get_or("MSYS2_ROOT", r"C:\msys64"));
            let clang64 = msys.join("clang64").join("bin");
            if clang64.is_dir() {
                let path = format!("{};{}", python_path(&clang64), self.get_or("PATH", ""));
                self.set("PATH", path);
            }
        }
        // 7. Instruction sets this CPU lacks (MOVBE, BMI1, LZCNT): the ISA rewrite covers them;
        // exit 20 is a set nothing covers.
        let features = self
            .command(&probe_text)
            .arg("--cpu-features")
            .stderr(Stdio::inherit())
            .output();
        let features = match features {
            Ok(features) => features,
            Err(error) => return self.fail(&format!("{probe_text}: {error}")),
        };
        let stdout = String::from_utf8_lossy(&features.stdout).into_owned();
        if features.status.code() == Some(UNSUPPORTED_CPU) {
            let _ = self.out.write_all(stdout.as_bytes());
            let _ = self.out.flush();
            return UNSUPPORTED_CPU;
        }
        let missing = parse_cpu_features(&stdout);
        if (self.interrupted)() {
            return INTERRUPTED;
        }
        // 8. The game, and the update over it.
        if !game.join("eboot.bin").is_file() {
            return self.fail(&format!(
                "No eboot.bin in {} (set SPARTAN_GAME_DIR).",
                python_path(&game)
            ));
        }
        let mut views = Views::default();
        if let Some(update) = self.get("SPARTAN_PATCH_DIR").filter(|dir| !dir.is_empty()) {
            let update = update.to_string();
            match game_update::create_view(&game, Path::new(&update), &out) {
                Ok(view) => {
                    game = view.clone();
                    views.update = Some(view);
                }
                Err(error) => return self.fail(&error.to_string()),
            }
            self.say(&format!("Game and update combined from {update}"));
        }
        // 9. The mods overlay, then what were the preparation scripts.
        let enabled = match mods_enabled.as_str() {
            "0" => false,
            "1" => true,
            other => {
                // mods.py's --enabled takes 0 or 1; its argument error ends with code 2.
                self.say(&format!(
                    "Mods: SPARTAN_MODS_ENABLED must be 0 or 1, not {other}"
                ));
                return 2;
            }
        };
        let layers = mods::overlay_layers(&game, &mods_dir, Some(&mods_config), enabled)
            .and_then(|layers| mods::build_overlay(&game, &out, &layers));
        match layers {
            Ok(overlay) => {
                game = overlay.root.clone();
                views.overlay = Some(overlay);
            }
            Err(error) => return self.fail(&format!("Mods: {error}")),
        }
        if let Err(code) = self.prepare_image(&game, &out) {
            return code;
        }
        // A failed rewrite prints its reason and the game starts without the table.
        let mut isa_args: Vec<OsString> = Vec::new();
        if isa_rewrite_wanted(&missing, &self.lookup()) {
            match isa_rewrite::rewrite(&out.join("boot-linked.bin"), &out) {
                Ok(sites) => {
                    self.say(&isa_rewrite::summary(&sites));
                    let table = python_path(&out.join("isa-rewrite.bin"));
                    isa_args = vec!["--isa-rewrite".into(), table.into()];
                }
                Err(error) => self.say(&format!("ISA: rewrite table unavailable ({error})")),
            }
        }
        if let Err(code) = self.content_profile(&game, &out) {
            return code;
        }
        if (self.interrupted)() {
            return INTERRUPTED;
        }
        // 10. The window icon (BMP: the format SDL loads without extra libraries); on Linux also
        // the application id the desktop entry matches.
        if !cfg!(windows) {
            self.set_default("SDL_APP_ID", "spartanport");
        }
        let icon = port.join("launcher").join("spartanport.png");
        if let Some(icon) = window_icon::write(&icon, &out) {
            self.set_default("SPARTAN_WINDOW_ICON", python_path(&icon));
        }
        // The temporal upscalers and motion vectors are not calibrated for this game yet: off
        // unless the launcher's experimental switch is on; linear image readbacks fix its
        // corrupted textures.
        if self.get("SPARTAN_EXPERIMENTAL_UPSCALER") != Some("1") {
            self.set_default("SPARTAN_UPSCALER", "none");
        }
        self.set_default("SPARTAN_READBACK_LINEAR", "1");
        // 11 and 12. The patches.
        let status = match self.patches(&port, &data, &game, &out, &mod_patch_files, &graphics) {
            Ok(status) => status,
            Err(code) => return code,
        };
        for (key, value) in patch_environment(status.as_ref()) {
            if key == "SPARTAN_ENGINE_FPS" || !self.env.contains_key(&key) {
                self.derive(&key, value);
            }
        }
        // The highest engine rate this launch allows (the menu picks a lower one live). Without
        // the frame rate patch the engine steps 1/60 s per frame: the present thread must not flip
        // faster, or the game speeds up.
        let engine_fps = engine_fps(status.as_ref());
        let chosen = graphics.options.engine_fps.parse().unwrap_or(engine_fps);
        let note = if engine_fps > DEFAULT_RATE || graphics.options.engine_fps == "60" {
            ""
        } else {
            " (the frame rate patch was not found in this eboot.bin)"
        };
        self.say(&format!(
            "Engine frame rate: {} FPS, live up to {engine_fps}{note}",
            chosen.min(engine_fps)
        ));
        // 13. The controller glyphs and the pause menu's Exit to Desktop row.
        let mount_args = self.mounts(&port, &probe_text, &game, &out);
        if (self.interrupted)() {
            return INTERRUPTED;
        }
        // 14. What this run derived; the guest memory check; the saves.
        self.set("SPARTAN_DERIVED", self.derived.join(","));
        let dmem = self
            .get("SPARTAN_DMEM_MB")
            .filter(|text| !text.is_empty())
            .and_then(|text| py_int(text, 10))
            .and_then(|mb| u64::try_from(mb).ok())
            .unwrap_or(0);
        let stream = self.get_or("SPARTAN_STREAM_MEMORY", "cached");
        let physical = guest_memory::physical_memory_mb();
        if let Some(warning) = guest_memory::memory_warning(dmem, physical, &stream) {
            self.say(&warning);
        }
        let user = self.path_or("SPARTAN_USER_DIR", data.join("user"));
        let user = mods::resolve(&user).unwrap_or(user);
        if let Err(error) = fs::create_dir_all(&user) {
            return self.fail(&format!("{}: {error}", python_path(&user)));
        }
        // GPU caches (shaders, pipelines) beside the saves.
        self.set_default("SPARTAN_GPU_USER_DIR", python_path(&user));
        match save_backup::backup(&user, None) {
            Ok(Some(made)) => self.say(&format!("Saves backed up to {}", python_path(&made))),
            Ok(None) => {}
            Err(error) => self.say(&format!("Save backup failed, starting anyway: {error}")),
        }
        // 15. The in-game restart starts this program again once this process is gone.
        let restart = restart_command(&this_exe(), std::process::id(), &args);
        self.set("SPARTAN_RESTART_COMMAND", restart);
        // 16. The game. MangoHud is a Vulkan layer (a variable), GameMode a command that starts
        // the game.
        if !cfg!(windows) && self.get("SPARTAN_MANGOHUD") == Some("1") {
            self.set_default("MANGOHUD", "1");
        }
        let prefix = launch_prefix(&self.lookup());
        if !cfg!(windows) && self.get("SPARTAN_GAMEMODE") == Some("1") && prefix.is_empty() {
            self.say("GameMode: gamemoderun not found, starting without it");
        }
        let timeout = self.get_or("SPARTAN_TIMEOUT", "0");
        let path = |name: &str| OsString::from(python_path(&out.join(name)));
        let mut command: Vec<OsString> = prefix.into_iter().map(OsString::from).collect();
        command.extend([
            probe_text.into(),
            path("boot-linked.bin"),
            "--content-profile".into(),
            path("content.bin"),
            "--patches".into(),
            path("patches.bin"),
            "--app0".into(),
            python_path(&game).into(),
            "--user".into(),
            python_path(&user).into(),
            "--timeout".into(),
            timeout.into(),
        ]);
        command.extend(isa_args);
        command.extend(mount_args.into_iter().map(OsString::from));
        command.extend(args);
        let code = self.play_game(&command);
        drop(views);
        code
    }

    /// `--after <pid>` at the front of `args`: waits for that process (at most 30 s) and returns
    /// the other arguments.
    fn after(&mut self, mut args: Vec<OsString>) -> Result<Vec<OsString>, i32> {
        if args.len() < 2 || args[0] != "--after" {
            return Ok(args);
        }
        let text = args[1].to_string_lossy().into_owned();
        let Some(pid) = py_int(&text, 10).and_then(|pid| u32::try_from(pid).ok()) else {
            return Err(self.fail(&format!("--after needs a process id, not {text}")));
        };
        wait_for_pid(pid, AFTER_LIMIT);
        args.drain(..2);
        Ok(args)
    }

    /// `prepare`, `link_libc` and `link_modules`, with the lines their scripts printed.
    fn prepare_image(&mut self, game: &Path, out: &Path) -> Result<(), i32> {
        let report = prepare::prepare(game, out)
            .map_err(|error| self.fail(&format!("prepare failed: {error}")))?;
        let title = match report.sfo.get("TITLE") {
            Some(SfoValue::Text(text)) => text.clone(),
            Some(SfoValue::Int(number)) => number.to_string(),
            None => "None".to_string(),
        };
        let relocations: u64 = report.relocation_counts.values().sum();
        let missing: Vec<String> = report
            .unavailable_metadata_headers
            .iter()
            .map(usize::to_string)
            .collect();
        let resolved = mods::resolve(out).unwrap_or_else(|_| out.to_path_buf());
        let ret = if report.libc_evidence.init_env_is_ret {
            "True"
        } else {
            "False"
        };
        for line in [
            format!(
                "{title} | entry={:#x} | image={} bytes",
                report.entry,
                thousands(report.image_bytes)
            ),
            format!(
                "{} imported symbols; {} relocations; {} required modules",
                report.import_count,
                thousands(relocations),
                report.needed.len()
            ),
            format!(
                "Unavailable non-loadable metadata headers: [{}]; not a byte-exact ELF \
                 reconstruction",
                missing.join(", ")
            ),
            format!("Output: {}", python_path(&resolved)),
            format!("libc _init_env verified RET: {ret}"),
        ] {
            self.say(&line);
        }
        let libc = link_libc::link(game, out)
            .map_err(|error| self.fail(&format!("link_libc failed: {error}")))?;
        self.say(&format!(
            "Linked native libc: base={:#x}, {} fallback exports, TLS={} bytes; eboot TLS={} \
             bytes, fs->gs patched={}",
            libc.base,
            libc.bindings,
            libc.tls_memory_bytes,
            libc.main_tls.memsz,
            libc.fs_loads_patched
        ));
        let linked = link_modules::link(game, out, &link_modules::DEFAULT_MODULES)
            .map_err(|error| self.fail(&format!("link_modules failed: {error}")))?;
        let summary: Vec<String> = linked
            .modules
            .iter()
            .map(|module| format!("{}@{:#x}", module.file, module.base))
            .collect();
        self.say(&format!(
            "Linked modules: {}; {} native bindings, {} imports left to the host runtime, \
             fs->gs patched={}",
            summary.join(", "),
            linked.bindings,
            linked.unresolved_imports.len(),
            linked.fs_loads_patched
        ));
        Ok(())
    }

    /// `content_profile` with `SPARTAN_CONTENT_SKU`, and its line.
    fn content_profile(&mut self, game: &Path, out: &Path) -> Result<(), i32> {
        let sku = self.get_or("SPARTAN_CONTENT_SKU", "full");
        let choice: Sku = match sku.parse() {
            Ok(choice) => choice,
            Err(error) => {
                // The script's argument error ends with code 2.
                self.say(&format!(
                    "content_profile.py: error: argument --sku: {error}"
                ));
                return Err(2);
            }
        };
        let profile = content_profile::prepare(game, out, choice)
            .map_err(|error| self.fail(&format!("content_profile failed: {error}")))?;
        let params: Vec<String> = profile.user_params.iter().map(u32::to_string).collect();
        self.say(&format!(
            "AppContent profile: SKU={sku} (probe setting), user params=[{}], mounted add-ons=0",
            params.join(", ")
        ));
        Ok(())
    }

    /// Steps 11 and 12: the patch selection (`SPARTAN_PATCHES`, `SPARTAN_PATCHES_ONLY`), then
    /// `patches.py`'s compile into `patches.bin` and `patch_status.json`. Returns the status with
    /// what `run.py` derives from it (the engine rate of the applied list, the notes' direct memory
    /// and VBlank rate); `None` for a game without a patch profile.
    fn patches(
        &mut self,
        port: &Path,
        data: &Path,
        game: &Path,
        out: &Path,
        mod_patch_files: &[PathBuf],
        graphics: &graphics_options::Prepared,
    ) -> Result<Option<PatchStatus>, i32> {
        let builtin = port.join("patches");
        let profile = patches::game_profile_for(game);
        let mut xml = profile.map(|profile| builtin.join(profile.xml));
        if let Some(base) = xml.clone().filter(|_| !mod_patch_files.is_empty()) {
            // The built-in patches plus the enabled mods' (the same file for the selection and
            // the notes).
            let merged = out.join("patches-merged.xml");
            patches::merge_patch_files(&base, mod_patch_files, &merged, &mut *self.out)
                .map_err(|error| self.fail(&format!("Patches: {error}")))?;
            xml = Some(merged);
        }
        if let (Some(profile), Some(xml)) = (profile, &xml) {
            let tag = profile.app_version;
            let chosen = split_patches(&self.get_or("SPARTAN_PATCHES", ""));
            let only = self.get("SPARTAN_PATCHES_ONLY") == Some("1");
            let mut names = patches::selected_patches(xml, tag, &chosen, only)
                .map_err(|error| self.fail(&format!("Patches: {error}")))?;
            // A mod's patches that are on by default stay on when the launcher chose the list.
            let defaults: Vec<String> = patches::default_enabled(mod_patch_files, tag)
                .into_iter()
                .filter(|name| !names.contains(name))
                .collect();
            names.extend(defaults);
            // spartanport.ini decides the resolution and frame rate patches.
            let names = graphics_options::patch_names(&names, &graphics.options, true);
            let available = patches::available_patches(xml, tag)
                .map_err(|error| self.fail(&format!("Patches: {error}")))?;
            let log = &mut *self.out;
            let names = graphics_options::restrict_with_log(&names, &available, &mut |line| {
                say(log, &line)
            });
            self.set("SPARTAN_PATCHES", names.join(";"));
            self.set("SPARTAN_PATCHES_ONLY", "1");
        }
        let status_file = out.join("patch_status.json");
        if let Err(error) = fs::remove_file(&status_file) {
            if error.kind() != io::ErrorKind::NotFound {
                return Err(self.fail(&format!("{}: {error}", python_path(&status_file))));
            }
        }
        let extra = split_patches(&self.get_or("SPARTAN_PATCHES", ""));
        let patches_dir = self.path_or("SPARTAN_PATCHES_DIR", data.join("patches"));
        let patches_config = self.path_or("SPARTAN_PATCHES_CONFIG", data.join("patches.json"));
        let job = PatchJob {
            out,
            game,
            builtin_dir: &builtin,
            mod_patches: mod_patch_files,
            extra: &extra,
            only: self.get("SPARTAN_PATCHES_ONLY") == Some("1"),
            patches_dir: Some(&patches_dir),
            patches_config: Some(&patches_config),
        };
        patches::write_game_patches(&job, &mut *self.out)
            .map_err(|error| self.fail(&format!("Patches: {error}")))?;
        let (Some(profile), Some(xml)) = (profile, &xml) else {
            return Ok(None);
        };
        // Only the patches found in this eboot.bin count: a frame rate patch that was skipped
        // leaves the engine at 60 FPS, and the notes' memory and VBlank needs follow the list.
        let mut status = patches::patch_status(out).unwrap_or_default();
        let needs = patches::patch_requirements(xml, &status.applied, profile.app_version)
            .map_err(|error| self.fail(&format!("Patches: {error}")))?;
        status.engine_fps = Some(graphics_options::applied_engine_fps(&status.applied));
        status.dmem_mb = needs.dmem_mb;
        status.vblank_hz = needs.vblank_hz;
        Ok(Some(status))
    }

    /// Step 13: the probe arguments of the glyph copies and of the pause menu's copy; sets
    /// `SPARTAN_GLYPHS` (the in-game menu names the buttons the same way). Failures only reach the
    /// output: the game then keeps its own icons, or starts without the row.
    fn mounts(&mut self, port: &Path, probe: &str, game: &Path, out: &Path) -> Vec<String> {
        let assets = glyphs::Assets::in_port(port);
        let (glyph_args, style) = {
            let (env, log) = (&self.env, &mut *self.out);
            let gamepad = |command: &[&str], limit: Duration| run_within(command, limit, env);
            glyph_mounts(
                env,
                Path::new(probe),
                game,
                out,
                &assets,
                &gamepad,
                &mut |line| say(log, &line),
            )
        };
        if let Some(style) = style {
            self.derive("SPARTAN_GLYPHS", style);
        }
        let language = self.get("SPARTAN_LANGUAGE").map(str::to_string);
        let log = &mut *self.out;
        let menu_args = pause_menu_args(game, out, &glyph_args, language.as_deref(), &mut |line| {
            say(log, &line)
        });
        [glyph_args, menu_args].concat()
    }

    /// Starts the probe with `command` and waits for it, with the Discord presence while it runs:
    /// its exit code; 130 when Ctrl+C came first. A Ctrl+C before the start starts nothing (the
    /// cleanup would remove the folders under a game that never got it); one while the game runs
    /// leaves the game to end on its own, as the console gave it the Ctrl+C too.
    fn play_game(&mut self, command: &[OsString]) -> i32 {
        let Some((program, rest)) = command.split_first() else {
            return 1;
        };
        if (self.interrupted)() {
            return INTERRUPTED;
        }
        let started = self.command(program).args(rest).spawn();
        let mut child = match started {
            Ok(child) => child,
            Err(error) => {
                let program = program.to_string_lossy().into_owned();
                return self.fail(&format!("{program}: {error}"));
            }
        };
        let (env, log, interrupted) = (&self.env, &mut *self.out, self.interrupted);
        let ended = discord::while_running(
            child.id(),
            &|key| env.get(key).cloned(),
            &mut |line| say(log, &line),
            || wait_or_interrupt(&mut child, interrupted),
        );
        match ended {
            Ok(Some(code)) => code,
            Ok(None) => INTERRUPTED,
            Err(error) => self.fail(&error.to_string()),
        }
    }
}

/// The patch names of a `;`-separated list (`SPARTAN_PATCHES`, `patches.py --extra`).
fn split_patches(list: &str) -> Vec<String> {
    list.split(';').map(str::to_string).collect()
}

/// `value` with Python's `{:,}` thousands separators.
fn thousands(value: u64) -> String {
    let digits = value.to_string();
    let mut text = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            text.push(',');
        }
        text.push(digit);
    }
    text
}

/// Waits for the game: its exit code, or `None` once `interrupted` says Ctrl+C came.
fn wait_or_interrupt(child: &mut Child, interrupted: fn() -> bool) -> io::Result<Option<i32>> {
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(Some(exit_code(status)));
        }
        if interrupted() {
            return Ok(None);
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Runs `command` with `env`, no input and, without a console, no window of its own, and returns
/// what it printed; the error to print when it cannot start or has not ended within `limit` (it is
/// then stopped), with the text of Python's `subprocess.TimeoutExpired`.
fn run_within(
    command: &[&str],
    limit: Duration,
    env: &IndexMap<String, String>,
) -> Result<String, String> {
    let Some((program, args)) = command.split_first() else {
        return Err("no command".to_string());
    };
    let mut cmd = Command::new(program);
    cmd.args(args)
        .env_clear()
        .envs(env)
        .stdin(Stdio::null())
        .stdout(Stdio::piped());
    platform::no_console_window(&mut cmd);
    let mut child = cmd.spawn().map_err(|error| error.to_string())?;
    let Some(mut stdout) = child.stdout.take() else {
        return Err("no output".to_string());
    };
    let reader = std::thread::spawn(move || {
        let mut text = Vec::new();
        let _ = stdout.read_to_end(&mut text);
        text
    });
    let deadline = Instant::now() + limit;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "Command '{}' timed out after {} seconds",
                    py_list(command),
                    limit.as_secs()
                ));
            }
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error.to_string());
            }
        }
    }
    let text = reader.join().unwrap_or_default();
    Ok(String::from_utf8_lossy(&text).into_owned())
}

/// Instruction sets `spartanport-probe --cpu-features` reports missing (empty when complete).
pub fn parse_cpu_features(output: &str) -> Vec<String> {
    output
        .lines()
        .find_map(|line| line.strip_prefix("CPU: missing "))
        .map(|names| {
            names
                .split(',')
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// `SPARTAN_ISA_REWRITE=1` forces the ISA rewrite table, `0` disables it; otherwise the CPU decides.
pub fn isa_rewrite_wanted(missing: &[String], env: &dyn Fn(&str) -> Option<String>) -> bool {
    match env("SPARTAN_ISA_REWRITE").as_deref() {
        Some("1") => true,
        Some("0") => false,
        _ => !missing.is_empty(),
    }
}

/// The command that starts the game on Linux when GameMode is chosen in the launcher (empty on
/// Windows, without the choice, or when `gamemoderun` is not on `PATH`).
pub fn launch_prefix(env: &dyn Fn(&str) -> Option<String>) -> Vec<String> {
    if cfg!(windows) || env("SPARTAN_GAMEMODE").as_deref() != Some("1") {
        return Vec::new();
    }
    match env("PATH").and_then(|path| which("gamemoderun", &path)) {
        Some(found) => vec![found.to_string_lossy().into_owned()],
        None => Vec::new(),
    }
}

/// The first executable file called `name` in the directories of `path`.
fn which(name: &str, path: &str) -> Option<PathBuf> {
    std::env::split_paths(path)
        .map(|dir| dir.join(name))
        .find(|candidate| is_executable(candidate))
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata()
        .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}

/// The probe: `SPARTAN_PROBE` when set (taken as is, like `run.py`), else `bin/` or `out/` of the port.
pub fn find_executable(paths: &Paths, env: &dyn Fn(&str) -> Option<String>) -> Option<PathBuf> {
    env("SPARTAN_PROBE")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| paths.find_tool("spartanport-probe"))
}

/// Runs a command within a time limit: its output, or the error to print.
pub type ProbeRunner<'a> = dyn Fn(&[&str], Duration) -> Result<String, String> + 'a;

/// How long `glyph_mounts` waits for `spartanport-probe --gamepad`.
pub const GAMEPAD_PROBE_LIMIT: Duration = Duration::from_secs(15);

/// The probe arguments and style of the controller glyphs (`run.py` `glyph_mounts`): patched copies
/// of the WADs that hold the game's button icons, mounted over them (`glyphs::build`). Opt-in:
/// unless `SPARTAN_GLYPH_STYLE` is auto, xbox, nintendo or keyboard nothing is asked, drawn or
/// mounted, and old copies are removed. The keyboard keys follow `SPARTAN_KEY_MAP`; auto shows them
/// when no gamepad is connected. Whatever fails only reaches the Log: the game then keeps its own
/// icons. `run_probe` runs a command (`[probe, --gamepad]`) within a time limit and returns its
/// output or the error to print.
pub fn glyph_mounts(
    env: &IndexMap<String, String>,
    probe: &Path,
    game: &Path,
    out: &Path,
    assets: &glyphs::Assets,
    run_probe: &ProbeRunner<'_>,
    log: &mut dyn FnMut(String),
) -> (Vec<String>, Option<String>) {
    let get = |key: &str| env.get(key).map(String::as_str).unwrap_or("");
    let choice = env
        .get("SPARTAN_GLYPH_STYLE")
        .map(String::as_str)
        .unwrap_or("off");
    if !["auto", "xbox", "nintendo", "keyboard"].contains(&choice) {
        let _ = std::fs::remove_dir_all(out.join("glyphs"));
        return (Vec::new(), None);
    }
    let failed = |log: &mut dyn FnMut(String), error: &dyn std::fmt::Display| {
        log(format!(
            "Glyphs: {error}; the game keeps its own button icons"
        ));
        (Vec::new(), None)
    };
    let pad = if choice == "auto" {
        let program = probe.to_string_lossy();
        match run_probe(&[&program, "--gamepad"], GAMEPAD_PROBE_LIMIT) {
            Ok(output) => output,
            Err(error) => return failed(log, &error),
        }
    } else {
        String::new()
    };
    let resolved = glyphs::resolve(choice, &pad);
    let mounts = match glyphs::build(
        game,
        out,
        &resolved.style,
        resolved.faces.as_ref(),
        get("SPARTAN_PAD_MAP"),
        get("SPARTAN_KEY_MAP"),
        assets,
        log,
    ) {
        Ok(mounts) => mounts,
        Err(error) => return failed(log, &error),
    };
    if mounts.is_empty() {
        log(if resolved.style == "playstation" {
            "Glyphs: a PlayStation controller, the game keeps its own button icons".to_string()
        } else {
            "Glyphs: no button icons found in this game folder".to_string()
        });
        return (Vec::new(), None);
    }
    let name = if resolved.name.is_empty() {
        String::new()
    } else {
        format!(" ({})", resolved.name)
    };
    log(format!(
        "Glyphs: {}{name}, on copies in {}; game files untouched",
        resolved.style,
        out.join("glyphs").display()
    ));
    let arguments = mounts
        .iter()
        .flat_map(|(guest, path)| ["--mount".to_string(), format!("{guest}={}", path.display())])
        .collect();
    (arguments, Some(resolved.style))
}

/// Probe arguments for the copy of `R_UI.WAD` with the pause menu's Exit to Desktop row
/// (`pause_menu::build`), made from the glyphs' copy when `mounts` (the probe arguments of
/// [`glyph_mounts`]) has one (`run.py` `pause_menu_args`). The probe mounts it over both once its
/// hook is in (`src/menu_hook.h`). The game folder is never written; what fails only reaches the
/// Log, and the game then starts without the row.
pub fn pause_menu_args(
    game: &Path,
    out: &Path,
    mounts: &[String],
    language: Option<&str>,
    log: &mut dyn FnMut(String),
) -> Vec<String> {
    let guest = format!("/app0/{}=", pause_menu::WAD);
    let source = mounts
        .iter()
        .find_map(|part| part.strip_prefix(guest.as_str()))
        .map(Path::new);
    match pause_menu::build(game, out, source, language, log) {
        Ok(made) => made
            .into_iter()
            .flat_map(|path| ["--pause-menu".to_string(), pause_menu::python_path(&path)])
            .collect(),
        Err(error) => {
            log(format!("Pause menu: {error}; no Exit to Desktop row"));
            Vec::new()
        }
    }
}

/// Waits up to `limit` for process `pid` to end (the previous game, for `--after`). `true` when it
/// ended or never existed.
pub fn wait_for_pid(pid: u32, limit: Duration) -> bool {
    platform::wait_for_process(pid, limit)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env_of(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let pairs: Vec<(String, String)> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |key| pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone())
    }

    #[test]
    fn parse_cpu_features_reads_the_missing_line() {
        assert_eq!(
            parse_cpu_features("CPU: missing MOVBE, BMI1, LZCNT\n"),
            ["MOVBE", "BMI1", "LZCNT"]
        );
        assert!(parse_cpu_features("CPU: complete\n").is_empty());
    }

    #[test]
    fn rewrite_wanted_follows_cpu_by_default() {
        assert!(isa_rewrite_wanted(&["MOVBE".to_string()], &env_of(&[])));
        assert!(!isa_rewrite_wanted(&[], &env_of(&[])));
    }

    #[test]
    fn rewrite_forced() {
        assert!(isa_rewrite_wanted(
            &[],
            &env_of(&[("SPARTAN_ISA_REWRITE", "1")])
        ));
    }

    #[test]
    fn rewrite_disabled() {
        let missing = ["MOVBE".to_string(), "BMI1".to_string()];
        assert!(!isa_rewrite_wanted(
            &missing,
            &env_of(&[("SPARTAN_ISA_REWRITE", "0")])
        ));
    }

    #[test]
    fn launch_prefix_is_empty_without_gamemode() {
        assert!(launch_prefix(&env_of(&[])).is_empty());
    }

    fn words(items: &[&str]) -> Vec<String> {
        items.iter().map(|item| item.to_string()).collect()
    }

    /// Both quotings on every system (`restart_command` takes the one of the system it runs on);
    /// the texts are Python 3.14's `subprocess.list2cmdline` and `shlex.join` of the same words.
    #[test]
    fn list2cmdline_is_the_pythons() {
        let line = list2cmdline(&words(&[
            r"C:\Spartan Port\SpartanPort.exe",
            "--run",
            "--after",
            "42",
            "a\"b",
            "c\\",
            "d e\\",
            "",
            "tab\there",
            "back\\\\\"q",
            "x\\y",
        ]));
        assert_eq!(
            line,
            "\"C:\\Spartan Port\\SpartanPort.exe\" --run --after 42 a\\\"b c\\ \"d e\\\\\" \"\" \
             \"tab\there\" back\\\\\\\\\\\"q x\\y"
        );
    }

    #[test]
    fn shlex_join_is_the_pythons() {
        let line = shlex_join(&words(&[
            "/tmp/Spartan \"Port's\"/spartanport",
            "--run",
            "a\"b",
            "",
            "it's",
            "$HOME",
            "*",
            "x=y,z:a.b/c-d_e@f%g+h",
            "café",
        ]));
        assert_eq!(
            line,
            "'/tmp/Spartan \"Port'\"'\"'s\"/spartanport' --run 'a\"b' '' 'it'\"'\"'s' '$HOME' \
             '*' x=y,z:a.b/c-d_e@f%g+h 'café'"
        );
    }

    /// A Ctrl+C that came before the game started: the game is not started, as run.py's
    /// `KeyboardInterrupt` before `Popen`, so no game runs on the folders the cleanup removes. The
    /// program does not exist: any try to start it would print its error and give 1.
    #[test]
    fn a_ctrl_c_before_the_start_does_not_start_the_game() {
        let dir = tempfile::tempdir().unwrap();
        let command = [dir.path().join("no-such-probe").into_os_string()];
        let mut out = Vec::new();
        let mut runner = Runner::new(&mut out, IndexMap::new());
        runner.interrupted = || true;
        assert_eq!(runner.play_game(&command), INTERRUPTED);
        assert!(out.is_empty(), "{}", String::from_utf8_lossy(&out));
        // Without Ctrl+C the start is tried, and here fails.
        let mut out = Vec::new();
        let mut runner = Runner::new(&mut out, IndexMap::new());
        runner.interrupted = || false;
        assert_eq!(runner.play_game(&command), 1);
        assert!(!out.is_empty());
    }

    #[test]
    fn thousands_are_separated_as_python_does() {
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(999), "999");
        assert_eq!(thousands(1000), "1,000");
        assert_eq!(thousands(1_234_567), "1,234,567");
    }

    // -- RunGlyphs (tests/test_glyphs.py): the option is off unless SPARTAN_GLYPH_STYLE asks ----------

    struct GlyphRun {
        _dir: tempfile::TempDir,
        game: PathBuf,
        out: PathBuf,
    }

    /// `out/glyphs/R_UI.WAD` left from an earlier run, and a game folder holding the synthetic
    /// `R_UI.WAD` of the glyph tests when `with_wad`.
    fn glyph_run(with_wad: bool) -> GlyphRun {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("out");
        std::fs::create_dir_all(out.join("glyphs")).unwrap();
        std::fs::write(out.join("glyphs/R_UI.WAD"), b"old").unwrap();
        let game = dir.path().join("game");
        std::fs::create_dir_all(game.join("EXEC/WAD/PS4_LE")).unwrap();
        if with_wad {
            std::fs::write(game.join("EXEC/WAD/PS4_LE/R_UI.WAD"), glyphs::tests::wad()).unwrap();
        }
        GlyphRun {
            _dir: dir,
            game,
            out,
        }
    }

    fn glyph_assets() -> glyphs::Assets {
        glyphs::Assets::in_port(Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/..")))
    }

    fn env_map(pairs: &[(&str, &str)]) -> IndexMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    /// What `glyph_mounts` returned, the probe calls and the Log lines.
    type GlyphOutcome = (
        (Vec<String>, Option<String>),
        Vec<(Vec<String>, Duration)>,
        Vec<String>,
    );

    /// Runs `glyph_mounts` with `probe` standing in for `spartanport-probe --gamepad`.
    fn mounts_with(
        run: &GlyphRun,
        env: &[(&str, &str)],
        probe: Result<&str, &str>,
    ) -> GlyphOutcome {
        let calls = std::cell::RefCell::new(Vec::new());
        let answer = |args: &[&str], limit: Duration| {
            calls
                .borrow_mut()
                .push((args.iter().map(|a| a.to_string()).collect(), limit));
            probe.map(str::to_string).map_err(str::to_string)
        };
        let mut log = Vec::new();
        let result = glyph_mounts(
            &env_map(env),
            Path::new("probe"),
            &run.game,
            &run.out,
            &glyph_assets(),
            &answer,
            &mut |line| log.push(line),
        );
        (result, calls.into_inner(), log)
    }

    fn stamp(run: &GlyphRun) -> String {
        std::fs::read_to_string(run.out.join("glyphs/stamp.json")).unwrap()
    }

    #[test]
    fn off_runs_nothing_and_removes_old_copies() {
        let run = glyph_run(true);
        for env in [
            vec![],
            vec![("SPARTAN_GLYPH_STYLE", "off")],
            vec![("SPARTAN_GLYPH_STYLE", "playstation")],
        ] {
            std::fs::create_dir_all(run.out.join("glyphs")).unwrap();
            let (result, calls, _) = mounts_with(&run, &env, Ok(""));
            assert_eq!(result, (vec![], None));
            assert!(calls.is_empty());
            assert!(!run.out.join("glyphs").exists());
        }
    }

    #[test]
    fn auto_asks_the_probe_and_mounts_the_copies() {
        let run = glyph_run(true);
        let pad = "Gamepad: type=xboxone south=a east=b west=x north=y name=Xbox Controller\n";
        let env = [
            ("SPARTAN_GLYPH_STYLE", "auto"),
            ("SPARTAN_PAD_MAP", "cross=b"),
        ];
        let ((arguments, style), calls, log) = mounts_with(&run, &env, Ok(pad));
        assert_eq!(
            calls,
            [(
                vec!["probe".to_string(), "--gamepad".to_string()],
                Duration::from_secs(15)
            )]
        );
        // glyphs::build got the controller's style and letters and the gamepad map, no key map.
        let faces = r#"{"south": "a", "east": "b", "west": "x", "north": "y"}"#;
        assert!(
            stamp(&run).starts_with(&format!("[11, \"xbox\", {faces}, \"cross=b\", 1")),
            "{}",
            stamp(&run)
        );
        let copy = run.out.join("glyphs").join("R_UI.WAD");
        assert_eq!(
            arguments,
            [
                "--mount".to_string(),
                format!("/app0/EXEC/WAD/PS4_LE/R_UI.WAD={}", copy.display())
            ]
        );
        assert_eq!(style.as_deref(), Some("xbox"));
        let folder = run.out.join("glyphs");
        assert_eq!(
            log,
            [format!(
                "Glyphs: xbox (Xbox Controller), on copies in {}; game files untouched",
                folder.display()
            )]
        );
    }

    #[test]
    fn a_chosen_style_does_not_ask_the_probe() {
        let run = glyph_run(false);
        let (result, calls, log) =
            mounts_with(&run, &[("SPARTAN_GLYPH_STYLE", "nintendo")], Ok(""));
        assert_eq!(result, (vec![], None));
        assert!(calls.is_empty());
        assert!(
            stamp(&run).starts_with("[11, \"nintendo\", null, \"\", 1"),
            "{}",
            stamp(&run)
        );
        assert_eq!(log, ["Glyphs: no button icons found in this game folder"]);
    }

    #[test]
    fn keyboard_passes_the_key_map() {
        let run = glyph_run(true);
        let env = [
            ("SPARTAN_GLYPH_STYLE", "keyboard"),
            ("SPARTAN_KEY_MAP", "cross=Q"),
        ];
        let ((_, style), calls, _) = mounts_with(&run, &env, Ok(""));
        assert_eq!(style.as_deref(), Some("keyboard"));
        assert!(calls.is_empty());
        assert!(
            stamp(&run).starts_with("[11, \"keyboard\", null, \"\", \"cross=Q\", null, "),
            "{}",
            stamp(&run)
        );
    }

    #[test]
    fn auto_without_a_gamepad_shows_the_keys() {
        let run = glyph_run(true);
        let env = [
            ("SPARTAN_GLYPH_STYLE", "auto"),
            ("SPARTAN_KEY_MAP", "square=G"),
        ];
        let ((_, style), _, _) = mounts_with(&run, &env, Ok("Gamepad: none\n"));
        assert_eq!(style.as_deref(), Some("keyboard"));
        assert!(
            stamp(&run).starts_with("[11, \"keyboard\", null, \"\", \"square=G\", null, "),
            "{}",
            stamp(&run)
        );
    }

    #[test]
    fn a_failure_starts_the_game_with_its_own_glyphs() {
        let run = glyph_run(true);
        let (result, _, log) =
            mounts_with(&run, &[("SPARTAN_GLYPH_STYLE", "auto")], Err("no probe"));
        assert_eq!(result, (vec![], None));
        assert_eq!(
            log,
            ["Glyphs: no probe; the game keeps its own button icons"]
        );
        // glyphs.build failing (here: out/ is a file, so out/glyphs cannot be made).
        let blocked = GlyphRun {
            _dir: tempfile::tempdir().unwrap(),
            game: run.game.clone(),
            out: run.out.join("glyphs").join("R_UI.WAD"),
        };
        let (result, _, log) = mounts_with(&blocked, &[("SPARTAN_GLYPH_STYLE", "xbox")], Ok(""));
        assert_eq!(result, (vec![], None));
        assert!(
            log.len() == 1
                && log[0].starts_with("Glyphs: ")
                && log[0].ends_with("; the game keeps its own button icons"),
            "{log:?}"
        );
    }

    // -- pause_menu_args (run.py): the copy with the Exit to Desktop row ---------------------------

    /// What `pause_menu_args` returned and the Log lines.
    fn menu_args(game: &Path, out: &Path, mounts: &[String]) -> (Vec<String>, Vec<String>) {
        let mut log = Vec::new();
        let args = pause_menu_args(game, out, mounts, Some("en"), &mut |line| log.push(line));
        (args, log)
    }

    #[test]
    fn pause_menu_args_patch_the_glyph_copy_when_it_is_mounted() {
        // The game folder has no R_UI.WAD: only the glyph copy can be the source.
        let run = glyph_run(false);
        let copy = run.out.join("glyphs").join("R_UI.WAD");
        std::fs::write(&copy, glyphs::tests::wad()).unwrap();
        let mounts = [
            "--mount".to_string(),
            "/app0/EXEC/WAD/PS4_LE/R_UI.WAD.bak=elsewhere".to_string(),
            "--mount".to_string(),
            format!("/app0/EXEC/WAD/PS4_LE/R_UI.WAD={}", copy.display()),
        ];
        let (args, log) = menu_args(&run.game, &run.out, &mounts);
        assert!(args.is_empty());
        assert_eq!(
            log,
            ["Pause menu: no SMI_R_UI in this WAD; no Exit to Desktop row"]
        );
        // Without the mount the game's own R_UI.WAD is the source; there is none here.
        let (args, log) = menu_args(&run.game, &run.out, &[]);
        assert!(args.is_empty());
        assert!(log.is_empty(), "{log:?}");
    }

    #[test]
    fn pause_menu_args_report_what_fails_and_start_without_the_row() {
        let run = glyph_run(true);
        // out/ is a file, so out/pause_menu cannot be made.
        let out = run.out.join("glyphs").join("R_UI.WAD");
        let (args, log) = menu_args(&run.game, &out, &[]);
        assert!(args.is_empty());
        assert!(
            log.len() == 1
                && log[0].starts_with("Pause menu: ")
                && log[0].ends_with("; no Exit to Desktop row"),
            "{log:?}"
        );
    }

    #[test]
    fn find_executable_prefers_the_probe_variable() {
        let dir = tempfile::tempdir().unwrap();
        let probe = dir.path().join("my-probe");
        std::fs::write(&probe, b"").unwrap();
        let paths = Paths::from_env(dir.path(), &|_| None);
        let env = env_of(&[("SPARTAN_PROBE", probe.to_str().unwrap_or(""))]);
        assert_eq!(find_executable(&paths, &env), Some(probe));
        assert_eq!(find_executable(&paths, &env_of(&[])), None);
    }
}
