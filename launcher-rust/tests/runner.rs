#![allow(clippy::unwrap_used, clippy::expect_used)]
//! The runner from end to end (`--run`, `--after`, `--play`, the in-game restart), with
//! `fake-probe` in place of `spartanport-probe` and the synthetic game of the `prepare` fixture: the
//! real game is never started.

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use indexmap::IndexMap;
use serde_json::json;
use spartanport_launcher::core::log::ExitHint;
use spartanport_launcher::core::patches::{patch_status, PatchStatus};
use spartanport_launcher::core::paths::Paths;
use spartanport_launcher::core::settings::Settings;
use spartanport_launcher::runner::{
    isa_rewrite, patch_environment, play_with, restart_command, RunContext,
};

/// The launcher program cargo built for these tests.
const LAUNCHER: &str = env!("CARGO_BIN_EXE_spartanport-launcher");
const FAKE_PROBE: &str = env!("CARGO_BIN_EXE_fake-probe");
const FAKE_GAME: &str = env!("CARGO_BIN_EXE_fake-game");
const EXE: &str = if cfg!(windows) { ".exe" } else { "" };

fn manifest() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

/// Python's `Path.resolve()` of something that exists: links followed, long names, no `\\?\`
/// (`\\?\UNC\server\share` is `\\server\share`).
fn resolved(path: &Path) -> PathBuf {
    let path = fs::canonicalize(path).unwrap();
    let text = path.to_string_lossy().into_owned();
    if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        return PathBuf::from(format!(r"\\{rest}"));
    }
    match text.strip_prefix(r"\\?\") {
        Some(rest) => PathBuf::from(rest),
        None => path,
    }
}

/// `path`, on a local drive, through this machine's admin share (`\\localhost\C$\...`): a
/// network share without a network. `None` where the share is not served.
#[cfg(windows)]
fn through_admin_share(path: &Path) -> Option<PathBuf> {
    let (drive, rest) = path.to_str()?.split_once(r":\")?;
    let shared = PathBuf::from(format!(r"\\localhost\{drive}$\{rest}"));
    shared.exists().then_some(shared)
}

fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), &target).unwrap();
        }
    }
}

fn copy_into(from: &Path, folder: &Path) {
    fs::create_dir_all(folder).unwrap();
    fs::copy(from, folder.join(from.file_name().unwrap())).unwrap();
}

/// A port folder as a package holds it: `run.py`, the probe (`fake-probe`) in `bin/`, the icon and
/// the built-in patches, with the synthetic game of the `prepare` fixture (plus the `link`
/// fixture's `libSceFios2.prx`) as `game/`.
struct Port {
    _dir: tempfile::TempDir,
    root: PathBuf,
}

impl Port {
    fn new() -> Port {
        let dir = tempfile::tempdir().unwrap();
        let root = resolved(dir.path());
        fs::write(root.join("run.py"), b"").unwrap();
        fs::create_dir_all(root.join("bin")).unwrap();
        fs::copy(
            FAKE_PROBE,
            root.join("bin").join(format!("spartanport-probe{EXE}")),
        )
        .unwrap();
        let repo = manifest().join("..");
        copy_into(
            &repo.join("launcher/spartanport.png"),
            &root.join("launcher"),
        );
        copy_into(&repo.join("patches/CUSA01623.xml"), &root.join("patches"));
        let fixtures = manifest().join("fixtures/runner");
        copy_tree(&fixtures.join("prepare/input"), &root.join("game"));
        copy_into(
            &fixtures.join("link/input/sce_module/libSceFios2.prx"),
            &root.join("game/sce_module"),
        );
        Port { _dir: dir, root }
    }

    fn game(&self) -> PathBuf {
        self.root.join("game")
    }

    fn out(&self) -> PathBuf {
        self.root.join("out")
    }

    fn probe(&self) -> PathBuf {
        self.root
            .join("bin")
            .join(format!("spartanport-probe{EXE}"))
    }

    /// The environment a run in this port gets: [`clean_environment`], the port, its game and no
    /// Discord presence, then `extra`.
    fn environment(&self, extra: &[(&str, &str)]) -> IndexMap<String, String> {
        let mut env = clean_environment();
        let text = |path: PathBuf| path.to_string_lossy().into_owned();
        env.insert("SPARTAN_PORT_DIR".into(), text(self.root.clone()));
        env.insert("SPARTAN_GAME_DIR".into(), text(self.game()));
        env.insert("SPARTAN_DISCORD".into(), "0".into());
        for (key, value) in extra {
            env.insert(key.to_string(), value.to_string());
        }
        env
    }

    /// `LAUNCHER --run <args>` in this port with `extra` variables.
    fn run(&self, extra: &[(&str, &str)], args: &[&str]) -> Ran {
        let output = Command::new(LAUNCHER)
            .arg("--run")
            .args(args)
            .env_clear()
            .envs(self.environment(extra))
            .stdin(Stdio::null())
            .output()
            .unwrap();
        Ran {
            code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        }
    }
}

/// This process's environment without the variables a run reads (`SPARTAN_*`, the old `GOW3_*`
/// names and the fake probe's), so the developer's own settings never reach a test.
fn clean_environment() -> IndexMap<String, String> {
    std::env::vars_os()
        .filter_map(|(key, value)| Some((key.into_string().ok()?, value.into_string().ok()?)))
        .filter(|(key, _)| {
            let key = key.to_uppercase();
            !["SPARTAN_", "GOW3_", "FAKE_PROBE_"]
                .iter()
                .any(|prefix| key.starts_with(prefix))
        })
        .collect()
}

/// What a run of the launcher gave.
struct Ran {
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

impl std::fmt::Display for Ran {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "code {:?}\n--- stdout\n{}--- stderr\n{}",
            self.code, self.stdout, self.stderr
        )
    }
}

/// The argv of every game start the fake probe printed, in order.
fn probe_runs(stdout: &str) -> Vec<Vec<String>> {
    let mut runs = Vec::new();
    let mut lines = stdout.lines();
    while lines.any(|line| line == "fake-probe argv:") {
        let argv = lines
            .by_ref()
            .take_while(|line| *line != "fake-probe argv end")
            .map(str::to_string)
            .collect();
        runs.push(argv);
    }
    runs
}

/// Every value the fake probe printed for the variable `name`, in order.
fn printed(stdout: &str, name: &str) -> Vec<String> {
    let prefix = format!("{name}=");
    stdout
        .lines()
        .filter_map(|line| line.strip_prefix(&prefix))
        .map(str::to_string)
        .collect()
}

fn text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// The folder the probe got after `--app0`.
fn app0(argv: &[String]) -> PathBuf {
    let at = argv.iter().position(|arg| arg == "--app0").unwrap();
    PathBuf::from(&argv[at + 1])
}

/// Asserts that lines holding each of `parts` appear in `stdout` in this order.
#[track_caller]
fn in_order(stdout: &str, parts: &[&str]) {
    let all: Vec<&str> = stdout.lines().collect();
    let mut from = 0;
    for wanted in parts {
        match all[from..].iter().position(|line| line.contains(wanted)) {
            Some(at) => from += at + 1,
            None => panic!("no line {wanted:?} after line {from} in\n{stdout}"),
        }
    }
}

#[test]
fn run_writes_every_derived_file_and_starts_the_probe_with_the_python_command_line() {
    let port = Port::new();
    let printed_names = "SPARTAN_DERIVED,SPARTAN_EXTRA_RESOLUTIONS,SPARTAN_VBLANK_HZ,\
                         SPARTAN_ENGINE_FPS,SPARTAN_DMEM_MB,SPARTAN_CONFIG,SPARTAN_UPSCALER,\
                         SPARTAN_READBACK_LINEAR,SPARTAN_WINDOW_ICON,SPARTAN_GPU_USER_DIR,\
                         SPARTAN_PATCHES_ONLY,SPARTAN_DEFERRED_READBACK,SPARTAN_STALE_READBACK,\
                         SPARTAN_ASYNC_SHADERS,SPARTAN_GLYPHS";
    let ran = port.run(
        &[
            ("SPARTAN_TIMEOUT", "5"),
            ("FAKE_PROBE_PRINT", printed_names),
        ],
        &["--fullscreen", "two words"],
    );
    assert_eq!(ran.code, Some(0), "{ran}");
    let out = port.out();
    for name in [
        "boot.bin",
        "eboot.elf",
        "entry.bin",
        "analysis.json",
        "boot-libc.bin",
        "libc-link.json",
        "boot-linked.bin",
        "link.json",
        "content.bin",
        "content-profile.json",
        "patches.bin",
        "patch_status.json",
        "graphics_state.json",
        "window_icon.bmp",
    ] {
        assert!(out.join(name).is_file(), "out/{name}\n{ran}");
    }
    // run.py's command line (spec §5 step 16), the arguments after --run at its end.
    let user = resolved(&port.root.join("user"));
    let expected: Vec<String> = [
        text(&port.probe()),
        text(&out.join("boot-linked.bin")),
        "--content-profile".into(),
        text(&out.join("content.bin")),
        "--patches".into(),
        text(&out.join("patches.bin")),
        "--app0".into(),
        text(&resolved(&port.game())),
        "--user".into(),
        text(&user),
        "--timeout".into(),
        "5".into(),
        "--fullscreen".into(),
        "two words".into(),
    ]
    .into();
    assert_eq!(probe_runs(&ran.stdout), [expected], "{ran}");
    // What this run derived, so "Apply and restart" derives it again from the new options.
    let one = |name: &str| printed(&ran.stdout, name).join("|");
    assert_eq!(
        one("SPARTAN_DERIVED"),
        "SPARTAN_EXTRA_RESOLUTIONS,SPARTAN_VBLANK_HZ,SPARTAN_ENGINE_FPS"
    );
    assert_eq!(one("SPARTAN_EXTRA_RESOLUTIONS"), "");
    assert_eq!(one("SPARTAN_VBLANK_HZ"), "60");
    assert_eq!(one("SPARTAN_ENGINE_FPS"), "60");
    assert!(ran.stdout.contains("SPARTAN_DMEM_MB is unset"), "{ran}");
    assert!(ran.stdout.contains("SPARTAN_GLYPHS is unset"), "{ran}");
    assert_eq!(
        one("SPARTAN_CONFIG"),
        text(&port.root.join("spartanport.ini"))
    );
    assert_eq!(one("SPARTAN_UPSCALER"), "none");
    assert_eq!(one("SPARTAN_READBACK_LINEAR"), "1");
    assert_eq!(
        one("SPARTAN_WINDOW_ICON"),
        text(&out.join("window_icon.bmp"))
    );
    assert_eq!(one("SPARTAN_GPU_USER_DIR"), text(&user));
    assert_eq!(one("SPARTAN_PATCHES_ONLY"), "1");
    for name in [
        "SPARTAN_DEFERRED_READBACK",
        "SPARTAN_STALE_READBACK",
        "SPARTAN_ASYNC_SHADERS",
    ] {
        assert_eq!(one(name), "1", "{name}");
    }
    // The lines of the Python's steps, in its order, before the game's.
    in_order(
        &ran.stdout,
        &[
            "ゴッド・オブ・ウォーIII リマスタード | entry=0x",
            " imported symbols; ",
            "Unavailable non-loadable metadata headers: ",
            &format!("Output: {}", out.display()),
            "libc _init_env verified RET: ",
            "Linked native libc: base=0x",
            "Linked modules: libc.prx@0x",
            "AppContent profile: SKU=full (probe setting), user params=[",
            "Patches: CUSA01623 01.02, eboot.bin sha256 ",
            "Patches: CUSA01623; ",
            "Engine frame rate: 60 FPS, live up to 60 (the frame rate patch was not found in this eboot.bin)",
            "fake-probe argv:",
        ],
    );
}

#[test]
fn a_restarted_run_derives_the_inherited_values_again() {
    // "Apply and restart": the new runner inherits the old one's environment, its derived values
    // included. The keys SPARTAN_DERIVED lists are derived again from the new options; the same
    // keys set by the player (not listed) are kept, as run.py's `setdefault`s keep them.
    let port = Port::new();
    let names = "SPARTAN_DERIVED,SPARTAN_EXTRA_RESOLUTIONS,SPARTAN_VBLANK_HZ,SPARTAN_DMEM_MB,\
                 SPARTAN_ENGINE_FPS";
    let first = port.run(&[("FAKE_PROBE_PRINT", names)], &[]);
    assert_eq!(first.code, Some(0), "{first}");
    let derived = printed(&first.stdout, "SPARTAN_DERIVED").join("|");
    assert_eq!(
        derived,
        "SPARTAN_EXTRA_RESOLUTIONS,SPARTAN_VBLANK_HZ,SPARTAN_ENGINE_FPS"
    );
    // What a start with other options left: a mod's size, a patch's memory and VBlank needs and
    // its frame rate (SPARTAN_DMEM_MB is derived only when a patch needs it).
    let stale = [
        ("FAKE_PROBE_PRINT", names),
        ("SPARTAN_EXTRA_RESOLUTIONS", "3440x1440"),
        ("SPARTAN_VBLANK_HZ", "144"),
        ("SPARTAN_DMEM_MB", "12288"),
        ("SPARTAN_ENGINE_FPS", "120"),
    ];
    let listed = format!("{derived},SPARTAN_DMEM_MB");
    let restarted = port.run(&[&stale[..], &[("SPARTAN_DERIVED", &listed)]].concat(), &[]);
    assert_eq!(restarted.code, Some(0), "{restarted}");
    let one = |ran: &Ran, name: &str| printed(&ran.stdout, name).join("|");
    assert_eq!(one(&restarted, "SPARTAN_DERIVED"), derived);
    assert_eq!(one(&restarted, "SPARTAN_EXTRA_RESOLUTIONS"), "");
    assert_eq!(one(&restarted, "SPARTAN_VBLANK_HZ"), "60");
    assert_eq!(one(&restarted, "SPARTAN_ENGINE_FPS"), "60");
    assert!(
        restarted.stdout.contains("SPARTAN_DMEM_MB is unset"),
        "{restarted}"
    );
    // Not listed: the player's own VBlank rate and memory stay.
    let own = port.run(&stale, &[]);
    assert_eq!(own.code, Some(0), "{own}");
    assert_eq!(one(&own, "SPARTAN_VBLANK_HZ"), "144");
    assert_eq!(one(&own, "SPARTAN_DMEM_MB"), "12288");
    assert_eq!(one(&own, "SPARTAN_EXTRA_RESOLUTIONS"), "");
    assert_eq!(one(&own, "SPARTAN_ENGINE_FPS"), "60");
}

#[test]
fn the_probes_exit_code_is_returned() {
    // tests/test_mods.py::test_run_uses_overlay_propagates_exit_and_cleans_view: the probe gets
    // the overlay with the mod's file, its exit code is the runner's and the overlay goes after.
    let port = Port::new();
    let wad = port.game().join("dvdroot_ps4/PART1.WAD");
    let original = fs::read(&wad).unwrap();
    let mod_wad = port.root.join("mods/A/dvdroot_ps4/PART1.WAD");
    fs::create_dir_all(mod_wad.parent().unwrap()).unwrap();
    fs::write(&mod_wad, b"mod").unwrap();
    let ran = port.run(
        &[
            ("FAKE_PROBE_EXIT", "7"),
            ("FAKE_PROBE_SHOW", "dvdroot_ps4/PART1.WAD"),
        ],
        &[],
    );
    assert_eq!(ran.code, Some(7), "{ran}");
    let runs = probe_runs(&ran.stdout);
    let overlay = app0(&runs[0]);
    assert!(
        overlay
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("mod-game-"),
        "{ran}"
    );
    assert_eq!(overlay.parent(), Some(port.out().as_path()));
    assert!(
        ran.stdout
            .contains("fake-probe show dvdroot_ps4/PART1.WAD: mod\n"),
        "{ran}"
    );
    assert!(!overlay.exists(), "the overlay is left in out/");
    assert_eq!(fs::read(&wad).unwrap(), original);
}

#[test]
fn an_unsupported_cpu_stops_with_20() {
    let port = Port::new();
    let ran = port.run(
        &[
            ("FAKE_PROBE_CPU_EXIT", "20"),
            ("FAKE_PROBE_MISSING", "AVX2"),
        ],
        &[],
    );
    assert_eq!(ran.code, Some(20), "{ran}");
    assert!(ran.stdout.contains("CPU: missing AVX2\n"), "{ran}");
    assert!(probe_runs(&ran.stdout).is_empty(), "{ran}");
    assert!(!port.out().join("boot.bin").exists());
}

#[test]
fn a_missing_eboot_fails_with_the_python_message() {
    let port = Port::new();
    let empty = port.root.join("not a game");
    fs::create_dir_all(&empty).unwrap();
    let ran = port.run(&[("SPARTAN_GAME_DIR", &text(&empty))], &[]);
    assert_eq!(ran.code, Some(1), "{ran}");
    let message = format!(
        "No eboot.bin in {} (set SPARTAN_GAME_DIR).",
        empty.display()
    );
    assert!(ran.stdout.contains(&message), "{ran}");
    assert!(probe_runs(&ran.stdout).is_empty());
}

#[cfg(windows)]
#[test]
fn a_game_on_a_share_reaches_the_probe_as_python_spells_it() {
    // \\server\share\..., as Python's resolve() gives it: the probe opens "<app0>/..." paths, and
    // Windows does not normalize a / under \\?\UNC\.
    let port = Port::new();
    let Some(game) = through_admin_share(&port.game()) else {
        eprintln!(r"skipped: \\localhost\ is not reachable");
        return;
    };
    let ran = port.run(&[("SPARTAN_GAME_DIR", &text(&game))], &[]);
    assert_eq!(ran.code, Some(0), "{ran}");
    let runs = probe_runs(&ran.stdout);
    assert_eq!(runs.len(), 1, "{ran}");
    let app0 = app0(&runs[0]);
    assert!(text(&app0).starts_with(r"\\localhost\"), "{ran}");
    assert_eq!(app0, resolved(&game), "{ran}");
}

#[test]
fn a_broken_mods_json_stops_with_the_mods_message() {
    // run.py:192 reads the selection before anything else and stops on its error.
    let port = Port::new();
    fs::write(port.root.join("mods.json"), "{").unwrap();
    let ran = port.run(&[], &[]);
    assert_eq!(ran.code, Some(1), "{ran}");
    assert!(
        ran.stdout.lines().any(|line| line.starts_with("Mods: ")),
        "{ran}"
    );
    assert!(probe_runs(&ran.stdout).is_empty());
}

#[test]
fn a_stale_isa_table_is_replaced() {
    // Review Focus: a table left by an older image (a game update) is rewritten from the current
    // boot-linked.bin, since the C loader refuses a table whose SHA-256 differs.
    let port = Port::new();
    fs::create_dir_all(port.out()).unwrap();
    let table = port.out().join("isa-rewrite.bin");
    fs::write(&table, b"the table of an older image").unwrap();
    // A CPU without MOVBE asks for the table.
    let ran = port.run(&[("FAKE_PROBE_MISSING", "MOVBE")], &[]);
    assert_eq!(ran.code, Some(0), "{ran}");
    let check = tempfile::tempdir().unwrap();
    let sites = isa_rewrite::rewrite(&port.out().join("boot-linked.bin"), check.path()).unwrap();
    let fresh = fs::read(check.path().join("isa-rewrite.bin")).unwrap();
    assert_eq!(fs::read(&table).unwrap(), fresh);
    assert!(
        ran.stdout
            .lines()
            .any(|line| line == isa_rewrite::summary(&sites)),
        "{ran}"
    );
    let argv = &probe_runs(&ran.stdout)[0];
    let at = argv.iter().position(|arg| arg == "--isa-rewrite").unwrap();
    assert_eq!(argv[at + 1], text(&table));
    assert_eq!(argv[at - 2], "--timeout");
}

#[test]
fn a_corrupt_patch_status_defaults_the_engine_fps() {
    // Review Focus: patch_status.json missing or cut short (a crash between the two writes).
    let defaults = [
        ("SPARTAN_VBLANK_HZ".to_string(), "60".to_string()),
        ("SPARTAN_ENGINE_FPS".to_string(), "60".to_string()),
    ];
    assert_eq!(patch_environment(None), defaults);
    let out = tempfile::tempdir().unwrap();
    fs::write(out.path().join("patch_status.json"), "{").unwrap();
    let status = patch_status(out.path());
    assert_eq!(status, None);
    assert_eq!(patch_environment(status.as_ref()), defaults);
    // An applied frame rate patch and the notes' needs, in run.py's order.
    let status = PatchStatus {
        applied: vec!["Frame Rate Patch - 120 FPS".to_string()],
        dmem_mb: Some(7104),
        vblank_hz: Some(120),
        ..PatchStatus::default()
    };
    let pairs: Vec<(String, String)> = [
        ("SPARTAN_DMEM_MB", "7104"),
        ("SPARTAN_VBLANK_HZ", "120"),
        ("SPARTAN_ENGINE_FPS", "120"),
    ]
    .iter()
    .map(|(k, v)| (k.to_string(), v.to_string()))
    .collect();
    assert_eq!(patch_environment(Some(&status)), pairs);
}

/// `CommandLineToArgvW`: how Windows programs read their command line.
#[cfg(windows)]
fn command_line_to_argv(line: &str) -> Vec<String> {
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::UI::Shell::CommandLineToArgvW;
    let wide: Vec<u16> = line.encode_utf16().chain([0]).collect();
    let mut count = 0i32;
    // SAFETY: `wide` is NUL-terminated and outlives the call; `count` is a valid i32 to write.
    let argv = unsafe { CommandLineToArgvW(wide.as_ptr(), &mut count) };
    assert!(!argv.is_null());
    let args = (0..usize::try_from(count).unwrap())
        .map(|index| {
            // SAFETY: CommandLineToArgvW returned `count` NUL-terminated strings.
            unsafe {
                let arg = *argv.add(index);
                let length = (0..).take_while(|&at| *arg.add(at) != 0).count();
                String::from_utf16(std::slice::from_raw_parts(arg, length)).unwrap()
            }
        })
        .collect();
    // SAFETY: the array was allocated by CommandLineToArgvW and is freed once.
    unsafe { LocalFree(argv.cast()) };
    args
}

/// The words `sh -c` makes of `line`.
#[cfg(not(windows))]
fn command_line_to_argv(line: &str) -> Vec<String> {
    let output = Command::new("sh")
        .arg("-c")
        .arg(format!("printf '%s\\n' {line}"))
        .output()
        .unwrap();
    assert!(output.status.success());
    String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(str::to_string)
        .collect()
}

#[test]
fn restart_command_quotes_spaces_and_quotes() {
    // Review Focus: the in-game restart's command line, with a space in the program's folder.
    let dir = tempfile::tempdir().unwrap();
    let os = |args: &[&str]| -> Vec<OsString> { args.iter().map(OsString::from).collect() };
    if cfg!(windows) {
        // Windows paths cannot hold a quote; the arguments do.
        let exe = dir.path().join("Spartan Port").join("SpartanPort.exe");
        let args = [
            "--timeout",
            "5",
            "a\"b",
            "c\\",
            "d e\\",
            "",
            "tab\there",
            "back\\\\\"q",
            "x\\y",
        ];
        let command = restart_command(&exe, 42, &os(&args));
        let mut expected = vec![text(&exe), "--run".into(), "--after".into(), "42".into()];
        expected.extend(args.iter().map(|arg| arg.to_string()));
        assert_eq!(command_line_to_argv(&command), expected);
        // subprocess.list2cmdline's text.
        assert_eq!(
            restart_command(
                Path::new(r"C:\Spartan Port\SpartanPort.exe"),
                42,
                &os(&args)
            ),
            "\"C:\\Spartan Port\\SpartanPort.exe\" --run --after 42 --timeout 5 a\\\"b c\\ \
             \"d e\\\\\" \"\" \"tab\there\" back\\\\\\\\\\\"q x\\y"
        );
    } else {
        let exe = dir.path().join("Spartan \"Port's\"").join("spartanport");
        let args = [
            "--timeout",
            "5",
            "a\"b",
            "",
            "it's",
            "$HOME",
            "*",
            "x=y,z:a.b/c-d_e@f%g+h",
            "café",
        ];
        let command = restart_command(&exe, 42, &os(&args));
        // No --after: on Linux the probe replaces itself with the new launch.
        let mut expected = vec![text(&exe), "--run".into()];
        expected.extend(args.iter().map(|arg| arg.to_string()));
        assert_eq!(command_line_to_argv(&command), expected);
        // shlex.join's text.
        assert_eq!(
            restart_command(
                Path::new("/tmp/Spartan \"Port's\"/spartanport"),
                42,
                &os(&args)
            ),
            "'/tmp/Spartan \"Port'\"'\"'s\"/spartanport' --run --timeout 5 'a\"b' '' \
             'it'\"'\"'s' '$HOME' '*' x=y,z:a.b/c-d_e@f%g+h 'café'"
        );
    }
}

#[test]
fn after_waits_for_the_old_process() {
    let empty = tempfile::tempdir().unwrap();
    let run_after = |pid: u32| {
        Command::new(LAUNCHER)
            .args(["--run", "--after", &pid.to_string()])
            .env_clear()
            .envs(clean_environment())
            .env("SPARTAN_PORT_DIR", empty.path())
            .stdin(Stdio::null())
            .output()
            .unwrap()
    };
    let started = Instant::now();
    let mut old = Command::new(FAKE_GAME)
        .args(["--lines", "1", "--delay-ms", "1500", "--exit", "0"])
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    let pid = old.id();
    // On Linux an exited child stays a zombie until reaped (`kill(pid, 0)` still succeeds).
    let reaper = std::thread::spawn(move || old.wait());
    let output = run_after(pid);
    let waited = started.elapsed();
    let _ = reaper.join();
    assert!(waited >= Duration::from_millis(1500), "{waited:?}");
    // Then the run goes on (here to the missing probe).
    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("not found: build it with build.sh"),
        "{stdout}"
    );
    // A process already gone is not waited for.
    let started = Instant::now();
    assert_eq!(run_after(u32::MAX - 1).status.code(), Some(1));
    assert!(started.elapsed() < Duration::from_secs(10));
}

#[test]
fn a_restart_from_the_game_starts_the_exe_again() {
    let port = Port::new();
    let child = Command::new(LAUNCHER)
        .arg("--run")
        .env_clear()
        .envs(port.environment(&[
            ("FAKE_PROBE_RESTART", "1"),
            ("FAKE_PROBE_PRINT", "SPARTAN_RESTART_COMMAND"),
        ]))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let pid = child.id();
    // The output ends once the restarted launch, which inherits it, has ended too.
    let output = child.wait_with_output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    assert_eq!(output.status.code(), Some(0), "{stdout}");
    assert_eq!(
        stdout.matches("Runtime: restarting through run.py").count(),
        1,
        "{stdout}"
    );
    // Two starts of the game, with the same command line (--after is the runner's own).
    let runs = probe_runs(&stdout);
    assert_eq!(runs.len(), 2, "{stdout}");
    assert_eq!(runs[0], runs[1]);
    // The first launch's restart command is this launcher's, waiting for the first runner on
    // Windows.
    let commands = printed(&stdout, "SPARTAN_RESTART_COMMAND");
    assert_eq!(commands.len(), 2, "{stdout}");
    let argv = command_line_to_argv(&commands[0]);
    assert_eq!(resolved(Path::new(&argv[0])), resolved(Path::new(LAUNCHER)));
    if cfg!(windows) {
        assert_eq!(argv[1..], ["--run", "--after", &pid.to_string()]);
    } else {
        assert_eq!(argv[1..], ["--run"]);
    }
}

/// A port's settings for `--play`: its game, `user_dir`, English and no Discord presence.
fn play_settings(port: &Port, user: &Path) -> Settings {
    let values = json!({
        "game_dir": text(&port.game()),
        "user_dir": text(user),
        "ui_language": "en",
        "discord_rpc": false,
        "mods_enabled": false,
    });
    Settings(values.as_object().unwrap().clone())
}

#[test]
fn play_writes_last_run_log_with_the_advice() {
    let port = Port::new();
    let user = port.root.join("saves");
    let settings = play_settings(&port, &user);
    let paths = Paths::from_env(&port.root, &|_| None);
    let line = "Runtime: cannot create 6080 MiB of guest memory (error 12)";
    let play = |exit: &str| {
        let env = port.environment(&[
            ("FAKE_PROBE_PRINT", "FAKE_LINE"),
            ("FAKE_LINE", line),
            ("FAKE_PROBE_EXIT", exit),
        ]);
        let mut console = Vec::new();
        let mut told = Vec::new();
        let context = RunContext {
            port_dir: &port.root,
            env,
            args: Vec::new(),
            out: &mut console,
        };
        let code = play_with(
            context,
            &settings,
            &paths,
            Path::new(LAUNCHER),
            &mut |hint| told.push(hint.to_string()),
        );
        let log = fs::read_to_string(user.join("last_run.log")).unwrap();
        (code, String::from_utf8(console).unwrap(), log, told)
    };
    let (code, console, log, told) = play("21");
    assert_eq!(code, 21, "{log}");
    assert!(log.contains(&format!("FAKE_LINE={line}\n")), "{log}");
    let advice = if cfg!(windows) {
        ExitHint::GuestMemory
    } else {
        ExitHint::LinuxMemory
    }
    .text_en();
    // The game's output on the console and in the log; the advice after it, in the log only.
    assert_eq!(log, format!("{console}\n{advice}\n"));
    assert_eq!(told, [advice]);
    // A game that ends well needs no advice.
    let (code, console, log, told) = play("0");
    assert_eq!(code, 0, "{log}");
    assert_eq!(log, console);
    assert!(told.is_empty());
}

#[test]
fn play_starts_the_game_with_the_saved_stream_memory() {
    // The saved choice, whatever the launcher inherited.
    let port = Port::new();
    let user = port.root.join("saves");
    let paths = Paths::from_env(&port.root, &|_| None);
    for value in ["cached", "host", "device"] {
        let mut settings = play_settings(&port, &user);
        settings.0.insert("stream_memory".into(), json!(value));
        let env = port.environment(&[
            ("FAKE_PROBE_PRINT", "SPARTAN_STREAM_MEMORY"),
            ("SPARTAN_STREAM_MEMORY", "inherited"),
        ]);
        let mut console = Vec::new();
        let context = RunContext {
            port_dir: &port.root,
            env,
            args: Vec::new(),
            out: &mut console,
        };
        let code = play_with(context, &settings, &paths, Path::new(LAUNCHER), &mut |_| {});
        let log = fs::read_to_string(user.join("last_run.log")).unwrap();
        assert_eq!(code, 0, "{log}");
        assert_eq!(printed(&log, "SPARTAN_STREAM_MEMORY"), [value], "{log}");
    }
}

#[test]
fn a_hanging_gamepad_probe_keeps_the_game_icons() {
    // run.py: a probe that does not answer within 15 s is a glyph failure; the game starts with
    // its own button icons. This test takes those 15 s.
    let port = Port::new();
    let started = Instant::now();
    let ran = port.run(
        &[
            ("SPARTAN_GLYPH_STYLE", "auto"),
            ("FAKE_PROBE_GAMEPAD", "hang"),
            ("FAKE_PROBE_PRINT", "SPARTAN_GLYPHS"),
        ],
        &[],
    );
    assert!(started.elapsed() >= Duration::from_secs(15));
    assert_eq!(ran.code, Some(0), "{ran}");
    let runs = probe_runs(&ran.stdout);
    assert_eq!(runs.len(), 1, "{ran}");
    assert!(!runs[0].iter().any(|arg| arg == "--mount"), "{ran}");
    // subprocess.TimeoutExpired's text, the command as Python's repr of the list.
    let probe = text(&port.probe()).replace('\\', "\\\\");
    let line = format!(
        "Glyphs: Command '['{probe}', '--gamepad']' timed out after 15 seconds; the game keeps \
         its own button icons"
    );
    assert!(ran.stdout.lines().any(|l| l == line), "{line}\n{ran}");
    assert!(ran.stdout.contains("SPARTAN_GLYPHS is unset"), "{ran}");
}

#[test]
fn the_gamepad_probe_answers_the_auto_glyphs() {
    // No gamepad: the keyboard keys; this synthetic game has no button icons to repaint.
    let port = Port::new();
    let ran = port.run(
        &[
            ("SPARTAN_GLYPH_STYLE", "auto"),
            ("FAKE_PROBE_GAMEPAD", "Gamepad: none"),
        ],
        &[],
    );
    assert_eq!(ran.code, Some(0), "{ran}");
    assert!(
        ran.stdout
            .lines()
            .any(|l| l == "Glyphs: no button icons found in this game folder"),
        "{ran}"
    );
}

#[test]
fn an_update_folder_is_combined_into_a_view() {
    let port = Port::new();
    let update = port.root.join("update");
    copy_into(&port.game().join("eboot.bin"), &update);
    copy_into(
        &port.game().join("sce_sys/param.sfo"),
        &update.join("sce_sys"),
    );
    fs::create_dir_all(update.join("dvdroot_ps4")).unwrap();
    fs::write(update.join("dvdroot_ps4/PART1.WAD"), b"update").unwrap();
    let ran = port.run(
        &[
            ("SPARTAN_PATCH_DIR", &text(&update)),
            ("FAKE_PROBE_SHOW", "dvdroot_ps4/PART1.WAD"),
        ],
        &[],
    );
    assert_eq!(ran.code, Some(0), "{ran}");
    let line = format!("Game and update combined from {}", update.display());
    assert!(ran.stdout.lines().any(|l| l == line), "{ran}");
    let view = app0(&probe_runs(&ran.stdout)[0]);
    assert!(
        view.file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("game-update-"),
        "{ran}"
    );
    assert_eq!(view.parent(), Some(port.out().as_path()));
    assert!(
        ran.stdout
            .contains("fake-probe show dvdroot_ps4/PART1.WAD: update\n"),
        "{ran}"
    );
    assert!(!view.exists(), "the view is left in out/");
    assert_ne!(
        fs::read(port.game().join("dvdroot_ps4/PART1.WAD")).unwrap(),
        b"update"
    );
}

/// Ctrl+C (SIGINT) while the game runs: the overlay goes and the runner ends with 130, as run.py's
/// `KeyboardInterrupt`.
#[cfg(unix)]
#[test]
fn ctrl_c_cleans_up_and_ends_with_130() {
    use nix::sys::signal::{kill, Signal};
    use nix::unistd::Pid;
    use std::io::{BufRead, BufReader};
    let port = Port::new();
    let mod_wad = port.root.join("mods/A/dvdroot_ps4/PART1.WAD");
    fs::create_dir_all(mod_wad.parent().unwrap()).unwrap();
    fs::write(&mod_wad, b"mod").unwrap();
    let mut child = Command::new(LAUNCHER)
        .arg("--run")
        .env_clear()
        .envs(port.environment(&[("FAKE_PROBE_SLEEP_MS", "3000")]))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    let mut seen = String::new();
    while !seen.contains("fake-probe argv end\n") {
        let mut line = String::new();
        assert_ne!(stdout.read_line(&mut line).unwrap(), 0, "{seen}");
        seen.push_str(&line);
    }
    let pid = Pid::from_raw(i32::try_from(child.id()).unwrap());
    kill(pid, Signal::SIGINT).unwrap();
    assert_eq!(child.wait().unwrap().code(), Some(130), "{seen}");
    let overlay = app0(&probe_runs(&seen)[0]);
    assert!(!overlay.exists());
}
