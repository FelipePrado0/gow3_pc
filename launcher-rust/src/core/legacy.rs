//! The project's former name, kept only to move players to the new one without losing anything.
//! Delete this module (and its callers) three releases after the rename: it is the only place in
//! the crate that still spells the old name.
//!
//! Everything here copies; nothing a player made is moved, overwritten or deleted. Each step runs
//! once (`MARKER` in the launcher's settings folder).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use indexmap::IndexMap;

pub const LEGACY_NAME: &str = "gow3";
pub const LEGACY_TITLE: &str = "God of War III";
/// The launcher; installed updaters look for this name.
pub const LEGACY_EXE: &str = "God of War III.exe";
/// The windowless start that shortcuts point to.
pub const LEGACY_PLAY_EXE: &str = "Play God of War III.exe";
pub const LEGACY_ENV: &str = "GOW3_";
/// An older build's patches, left in `patches/` by an update.
pub const LEGACY_PATCH_FILE: &str = "God_of_War_III_Remastered.xml";
pub const LEGACY_DESKTOP_MARKER: &str = "X-GOW3-Integration=true";
/// The launcher's settings, in AppData or `~/.config`.
pub const LEGACY_CONFIG_DIR: &str = "gow3-launcher";
pub const NEW_NAME: &str = "spartanport";
pub const NEW_ENV: &str = "SPARTAN_";
pub const MARKER: &str = "legacy-migrated";
pub const SHORTCUT_MARKER: &str = "shortcut-migrated";
pub const DATA_SKIP: [&str; 2] = ["user/cache", "out"];
/// Old binaries an update leaves behind, with the file that replaced each one.
pub const LEFTOVERS: [(&str, &str); 4] = [
    ("bin/gow3-probe.exe", "bin/spartanport-probe.exe"),
    (
        "bin/gow3-gpu-capabilities.exe",
        "bin/spartanport-gpu-capabilities.exe",
    ),
    ("launcher/gow3.ico", "launcher/spartanport.ico"),
    ("launcher/gow3.png", "launcher/spartanport.png"),
];

/// The user's home folder as `env` (or the process) names it.
fn home_of(env: &IndexMap<String, String>) -> Option<PathBuf> {
    let key = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    env.get(key)
        .cloned()
        .or_else(|| std::env::var(key).ok())
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

/// The data folder AppRun picks when none is set (`$XDG_DATA_HOME/spartanport`).
fn appimage_data_dir(env: &IndexMap<String, String>) -> Option<String> {
    let base = match env.get("XDG_DATA_HOME").filter(|v| !v.is_empty()) {
        Some(xdg) => PathBuf::from(xdg),
        None => home_of(env)?.join(".local").join("share"),
    };
    Some(base.join(NEW_NAME).to_string_lossy().into_owned())
}

/// Old `GOW3_*` variables a player set by hand become `SPARTAN_*` ones; a `SPARTAN_*` already set
/// wins, except the AppImage's default data folder, which only stands in for a choice not made.
pub fn forward_environment(env: &mut IndexMap<String, String>) {
    let snapshot: Vec<(String, String)> = env.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
    for (key, value) in snapshot {
        let Some(rest) = key.strip_prefix(LEGACY_ENV) else {
            continue;
        };
        let new = format!("{NEW_ENV}{rest}");
        let stands_in = new == "SPARTAN_DATA_DIR"
            && appimage_data_dir(env).is_some_and(|d| env.get(&new) == Some(&d));
        if !env.contains_key(&new) || stands_in {
            env.insert(new, value);
        }
    }
}

/// Copies the files of folder `old` into `new` that `new` does not have yet, leaving out the
/// folders in `skip` (relative to `old`). Returns how many it copied and whether nothing failed.
fn merge(old: &Path, new: &Path, skip: &[&str]) -> (usize, bool) {
    let mut state = (0, true);
    merge_into(old, new, Path::new(""), skip, &mut state);
    state
}

fn merge_into(old: &Path, new: &Path, relative: &Path, skip: &[&str], state: &mut (usize, bool)) {
    let Ok(entries) = fs::read_dir(old.join(relative)) else {
        state.1 = false;
        return;
    };
    for entry in entries.flatten() {
        let relative = relative.join(entry.file_name());
        if skip.iter().any(|s| relative.starts_with(s)) {
            continue;
        }
        let (source, target) = (old.join(&relative), new.join(&relative));
        let kind = entry.file_type();
        if kind.as_ref().is_ok_and(|t| t.is_dir()) {
            if fs::create_dir_all(&target).is_err() {
                state.1 = false;
                continue;
            }
            merge_into(old, new, &relative, skip, state);
        } else if source.is_dir() {
            // A link to a folder: the folder itself is made, not followed.
            state.1 &= fs::create_dir_all(&target).is_ok();
        } else if !target.exists() {
            let copied = target
                .parent()
                .map_or(Ok(()), fs::create_dir_all)
                .and_then(|()| copy_with_time(&source, &target));
            match copied {
                Ok(()) => state.0 += 1,
                Err(_) => state.1 = false,
            }
        }
    }
}

/// `shutil.copy2`: the contents, the permissions and the modification time.
fn copy_with_time(source: &Path, target: &Path) -> std::io::Result<()> {
    fs::copy(source, target)?;
    if let Ok(modified) = fs::metadata(source).and_then(|m| m.modified()) {
        if let Ok(file) = fs::OpenOptions::new().write(true).open(target) {
            let _ = file.set_modified(modified);
        }
    }
    Ok(())
}

/// The old game settings file copied to the new name when that one does not exist yet (the game
/// started without the launcher). Returns whether it copied.
pub fn copy_game_settings(data_dir: &Path) -> bool {
    let old = data_dir.join(format!("{LEGACY_NAME}.ini"));
    let new = data_dir.join(format!("{NEW_NAME}.ini"));
    // The game still starts (with defaults) when this fails; the copy is tried again next time.
    old.is_file() && !new.exists() && copy_with_time(&old, &new).is_ok()
}

/// Copies the data folder (Linux) and the game settings file from their old names. The
/// launcher's settings come from `LEGACY_CONFIG_DIR` afterwards. Returns what was done, as text
/// lines for the log. A copy that failed leaves no marker, so the next start tries again.
pub fn migrate(data_dir: &Path, home: Option<&Path>, windows: bool) -> Vec<String> {
    if data_dir.join(MARKER).is_file() {
        return Vec::new();
    }
    let mut done = Vec::new();
    let mut complete = true;
    let old_data = data_dir
        .parent()
        .map(|p| p.join(LEGACY_NAME))
        .unwrap_or_default();
    let is_new_name = data_dir.file_name().is_some_and(|n| n == NEW_NAME);
    if is_new_name && old_data.is_dir() {
        // The GPU caches (rebuilt while playing, can be gigabytes) and the run views in out/
        // (links into the game dump) stay behind: copying them would stall the first start.
        let (copied, ok) = merge(&old_data, data_dir, &DATA_SKIP);
        complete &= ok;
        done.push(format!("data: {copied} files from {}", old_data.display()));
    }
    let old_ini = data_dir.join(format!("{LEGACY_NAME}.ini"));
    if copy_game_settings(data_dir) {
        done.push(format!(
            "game settings: {} copied to {NEW_NAME}.ini",
            old_ini.display()
        ));
    }
    if !windows {
        let home = home
            .map(Path::to_path_buf)
            .or_else(|| home_of(&IndexMap::new()));
        if let Some(home) = home {
            let entry = home
                .join(".local/share/applications")
                .join(format!("{LEGACY_NAME}.desktop"));
            let ours = fs::read(&entry)
                .is_ok_and(|bytes| String::from_utf8_lossy(&bytes).contains(LEGACY_DESKTOP_MARKER));
            // The launcher writes the new menu entry itself.
            if entry.is_file() && ours && fs::remove_file(&entry).is_ok() {
                done.push(format!("menu: removed {}", entry.display()));
            }
        }
    }
    if complete && (!done.is_empty() || old_data.is_dir() || old_ini.is_file()) {
        let text = format!("{}\n", done.join("\n"));
        if fs::create_dir_all(data_dir).is_ok() {
            let _ = fs::write(data_dir.join(MARKER), text);
        }
    }
    done
}

/// Deletes the old binaries an update left next to their replacements. The launcher's copies
/// under the old names (`LEGACY_EXE`, `LEGACY_PLAY_EXE`) stay: shortcuts still point to them.
pub fn remove_leftovers(port_dir: &Path) {
    for (old, new) in LEFTOVERS {
        if port_dir.join(new).is_file() && port_dir.join(old).is_file() {
            // In use or read-only: tried again at the next start.
            let _ = fs::remove_file(port_dir.join(old));
        }
    }
}

/// `\\?\C:\x` becomes `C:\x`, the form PowerShell and shortcuts compare with.
fn plain(path: &Path) -> PathBuf {
    let resolved = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let text = resolved.to_string_lossy();
    match text.strip_prefix(r"\\?\") {
        Some(rest) if rest.as_bytes().get(1) == Some(&b':') => PathBuf::from(rest),
        _ => resolved,
    }
}

#[cfg(windows)]
fn no_window(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    command.creation_flags(0x0800_0000);
}

#[cfg(not(windows))]
fn no_window(_command: &mut Command) {}

#[cfg(windows)]
fn windows_desktop() -> PathBuf {
    let mut command = Command::new("powershell");
    command
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "[Environment]::GetFolderPath('DesktopDirectory')",
        ])
        .stdin(Stdio::null());
    no_window(&mut command);
    if let Ok(output) = command.output() {
        let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if output.status.success() && !text.is_empty() {
            return PathBuf::from(text);
        }
    }
    let home = std::env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .unwrap_or_default();
    home.join("Desktop")
}

#[cfg(not(windows))]
fn windows_desktop() -> PathBuf {
    PathBuf::new()
}

/// The PowerShell that makes `SpartanPort.lnk` from the old shortcut, only when that one is the
/// unmodified shortcut of the old launcher.
const RETARGET_SCRIPT: &str = concat!(
    "$w = New-Object -ComObject WScript.Shell;",
    "$s = $w.CreateShortcut($env:SP_OLD);",
    "if (![string]::IsNullOrWhiteSpace($s.TargetPath)) {",
    "  $target = [System.IO.Path]::GetFullPath($s.TargetPath);",
    "  $legacy = [System.IO.Path]::GetFullPath($env:SP_LEGACY);",
    "  $port = [System.IO.Path]::GetFullPath($env:SP_PORT);",
    "  $work = if ([string]::IsNullOrWhiteSpace($s.WorkingDirectory)) { $port } else { [System.IO.Path]::GetFullPath($s.WorkingDirectory) };",
    "  $args = if ($s.Arguments) { $s.Arguments.Trim() } else { \"\" };",
    "  if (($target -ieq $legacy) -and ($args -eq \"\") -and ($work -ieq $port)) {",
    "    $n = $w.CreateShortcut($env:SP_NEW);",
    "    $n.TargetPath = $env:SP_TARGET;",
    "    $n.WorkingDirectory = $env:SP_PORT;",
    "    $n.IconLocation = $env:SP_TARGET + \",0\";",
    "    $n.Save();",
    "    if (!(Test-Path -LiteralPath $env:SP_NEW)) { exit 1; }",
    "  }",
    "}"
);

/// An install updated from the Python package keeps the files only that package had. When
/// `_internal/` shows the Python launcher's runtime was there, removes it, `run.py`, the
/// scripts (not `scripts/controller_diagrams/`, which the runner reads) and the voice-swap tool,
/// then `tools/` if that leaves it empty. A source checkout has no `_internal/` and keeps its
/// Python. Errors are ignored: what stays is tried again at the next start.
pub fn remove_python_leftovers(port_dir: &Path) {
    let internal = port_dir.join("_internal");
    if !internal.is_dir() {
        return;
    }
    let gone = |result: std::io::Result<()>| {
        result.is_ok() || result.is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound)
    };
    let mut all_gone = gone(fs::remove_file(port_dir.join("run.py")));
    let scripts = port_dir.join("scripts");
    if let Ok(entries) = fs::read_dir(&scripts) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "py") && path.is_file() {
                all_gone &= gone(fs::remove_file(path));
            }
        }
    }
    all_gone &= gone(fs::remove_dir_all(scripts.join("__pycache__")));
    let tools = port_dir.join("tools");
    all_gone &= gone(fs::remove_file(tools.join("xvag_voice_swap.py")));
    // `remove_dir` only removes an empty folder; a tools/ with other files stays on purpose.
    let _ = fs::remove_dir(&tools);
    // `_internal/` is what brings this cleanup back at the next start: it goes last, and only
    // when nothing above failed.
    if all_gone {
        let _ = fs::remove_dir_all(&internal);
    }
}

fn write_marker(data_dir: &Path, marker: &Path) {
    if fs::create_dir_all(data_dir).is_ok() {
        let _ = fs::write(marker, "");
    }
}

/// Windows: the desktop shortcut the old launcher made (named after the old title, pointing to
/// `God of War III.exe` with no arguments) is migrated non-destructively by creating
/// `SpartanPort.lnk` pointing to `SpartanPort.exe`. User-created, renamed, or customized
/// shortcuts are preserved. Runs once. `false` elsewhere.
pub fn retarget_desktop_shortcut(
    port_dir: &Path,
    desktop: Option<&Path>,
    data_dir: Option<&Path>,
) -> bool {
    if !cfg!(windows) {
        return false;
    }
    let data_dir = data_dir.unwrap_or(port_dir);
    let marker = data_dir.join(SHORTCUT_MARKER);
    if marker.is_file() {
        return false;
    }
    let desktop = desktop.map_or_else(windows_desktop, Path::to_path_buf);
    let old = desktop.join(format!("{LEGACY_TITLE}.lnk"));
    let new = desktop.join("SpartanPort.lnk");
    let target = port_dir.join("SpartanPort.exe");
    let legacy_target = port_dir.join(LEGACY_EXE);
    if new.exists() || !old.is_file() {
        write_marker(data_dir, &marker);
        return false;
    }
    if !target.is_file() {
        return false;
    }
    let mut command = Command::new("powershell");
    command
        .args(["-NoProfile", "-NonInteractive", "-Command", RETARGET_SCRIPT])
        .env("SP_OLD", &old)
        .env("SP_NEW", &new)
        .env("SP_PORT", plain(port_dir))
        .env("SP_TARGET", plain(&target))
        .env("SP_LEGACY", plain(&legacy_target))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    no_window(&mut command);
    let Ok(mut child) = command.spawn() else {
        return false;
    };
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() < Duration::from_secs(20) => {
                std::thread::sleep(Duration::from_millis(50));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return false;
            }
        }
    };
    if !status.success() {
        return false;
    }
    write_marker(data_dir, &marker);
    new.is_file()
}

#[cfg(test)]
mod tests {
    use super::*;

    const OLD: &str = LEGACY_NAME;

    struct Fixture {
        _temp: tempfile::TempDir,
        root: PathBuf,
        data: PathBuf,
    }

    fn fixture() -> Fixture {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().to_path_buf();
        let data = root.join("port");
        fs::create_dir(&data).unwrap();
        Fixture {
            _temp: temp,
            root,
            data,
        }
    }

    fn put(path: &Path, bytes: &[u8]) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }

    fn text(path: &Path) -> String {
        fs::read_to_string(path).unwrap()
    }

    impl Fixture {
        fn migrate_at(&self, data: &Path, home: Option<&Path>) -> Vec<String> {
            let home = home.map_or_else(|| self.root.join("home"), Path::to_path_buf);
            migrate(data, Some(&home), false)
        }
        fn migrate(&self) -> Vec<String> {
            self.migrate_at(&self.data, None)
        }
    }

    fn map(pairs: &[(&str, &str)]) -> IndexMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn game_settings_file_is_copied_and_kept() {
        let f = fixture();
        put(&f.data.join("gow3.ini"), b"display_mode=borderless\n");
        f.migrate();
        assert_eq!(
            text(&f.data.join("spartanport.ini")),
            "display_mode=borderless\n"
        );
        assert!(f.data.join("gow3.ini").is_file());
    }

    #[test]
    fn game_settings_file_is_not_overwritten() {
        let f = fixture();
        put(&f.data.join("gow3.ini"), b"old");
        put(&f.data.join("spartanport.ini"), b"new");
        f.migrate();
        assert_eq!(text(&f.data.join("spartanport.ini")), "new");
    }

    #[test]
    fn copy_game_settings_copies_once() {
        let f = fixture();
        assert!(!copy_game_settings(&f.data));
        put(&f.data.join("gow3.ini"), b"engine_fps=60\n");
        assert!(copy_game_settings(&f.data));
        assert_eq!(text(&f.data.join("spartanport.ini")), "engine_fps=60\n");
        put(&f.data.join("spartanport.ini"), b"changed");
        assert!(!copy_game_settings(&f.data));
        assert_eq!(text(&f.data.join("spartanport.ini")), "changed");
    }

    #[test]
    fn linux_data_folder_with_saves_is_copied() {
        let f = fixture();
        let old = f.root.join("share").join(OLD);
        put(&old.join("user/savedata/1/slot0.bin"), b"save");
        put(&old.join("gow3.ini"), b"engine_fps=60\n");
        let new = f.root.join("share/spartanport");
        fs::create_dir_all(&new).unwrap(); // AppRun makes it before the launcher starts
        f.migrate_at(&new, None);
        assert_eq!(
            fs::read(new.join("user/savedata/1/slot0.bin")).unwrap(),
            b"save"
        );
        assert_eq!(text(&new.join("spartanport.ini")), "engine_fps=60\n");
        assert!(old.join("user/savedata/1/slot0.bin").is_file());
    }

    #[test]
    fn linux_data_copy_leaves_caches_and_run_folders() {
        let f = fixture();
        let old = f.root.join("share").join(OLD);
        put(&old.join("user/cache/shaders/pipelines.bin"), b"cache");
        put(&old.join("out/mod-game-abc/eboot.bin"), b"overlay");
        put(&old.join("user/savedata/slot0.bin"), b"save");
        let new = f.root.join("share/spartanport");
        fs::create_dir_all(&new).unwrap();
        f.migrate_at(&new, None);
        assert_eq!(
            fs::read(new.join("user/savedata/slot0.bin")).unwrap(),
            b"save"
        );
        assert!(!new.join("user/cache").exists());
        assert!(!new.join("out").exists());
        assert!(old.join("user/cache/shaders/pipelines.bin").is_file());
    }

    #[test]
    fn linux_data_copy_keeps_newer_files() {
        let f = fixture();
        let share = f.root.join("share");
        put(&share.join(OLD).join("user/a.bin"), b"old");
        let new = share.join("spartanport");
        put(&new.join("user/a.bin"), b"new");
        f.migrate_at(&new, None);
        assert_eq!(fs::read(new.join("user/a.bin")).unwrap(), b"new");
    }

    #[test]
    fn runs_once() {
        // A player who deletes a migrated file later (to reset it) does not get it back.
        let f = fixture();
        put(&f.data.join("gow3.ini"), b"old");
        f.migrate();
        fs::remove_file(f.data.join("spartanport.ini")).unwrap();
        f.migrate();
        assert!(!f.data.join("spartanport.ini").exists());
    }

    #[test]
    fn nothing_to_move_is_quiet() {
        let f = fixture();
        assert!(f.migrate().is_empty());
        assert!(!f.data.join(MARKER).exists());
    }

    #[test]
    fn old_menu_entry_of_ours_is_removed_and_a_hand_made_one_kept() {
        let f = fixture();
        let home = f.root.join("home");
        let applications = home.join(".local/share/applications");
        fs::create_dir_all(&applications).unwrap();
        let ours = applications.join("gow3.desktop");
        fs::write(&ours, format!("[Desktop Entry]\n{LEGACY_DESKTOP_MARKER}\n")).unwrap();
        f.migrate_at(&f.data, Some(&home));
        assert!(!ours.exists());
        fs::write(&ours, "[Desktop Entry]\nExec=by hand\n").unwrap();
        fs::remove_file(f.data.join(MARKER)).unwrap();
        f.migrate_at(&f.data, Some(&home));
        assert!(ours.exists());
    }

    #[test]
    fn environment_is_forwarded_without_overriding() {
        let mut env = map(&[
            ("GOW3_GAME_DIR", "E:/game"),
            ("GOW3_FPS_LIMIT", "60"),
            ("SPARTAN_FPS_LIMIT", "120"),
        ]);
        forward_environment(&mut env);
        assert_eq!(env["SPARTAN_GAME_DIR"], "E:/game");
        assert_eq!(env["SPARTAN_FPS_LIMIT"], "120");
    }

    #[test]
    fn custom_data_folder_wins_over_the_appimage_default() {
        let home_key = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
        let mut env = map(&[(home_key, "/home/player")]);
        let default = appimage_data_dir(&env).unwrap();
        assert!(default.ends_with("spartanport"));
        env.insert("GOW3_DATA_DIR".into(), "/mnt/games/data".into());
        env.insert("SPARTAN_DATA_DIR".into(), default);
        forward_environment(&mut env);
        assert_eq!(env["SPARTAN_DATA_DIR"], "/mnt/games/data");
        // A folder the player chose is kept.
        let mut env = map(&[
            (home_key, "/home/player"),
            ("GOW3_DATA_DIR", "/old"),
            ("SPARTAN_DATA_DIR", "/chosen"),
        ]);
        forward_environment(&mut env);
        assert_eq!(env["SPARTAN_DATA_DIR"], "/chosen");
    }

    #[test]
    fn leftover_binaries_go_only_when_the_new_ones_exist() {
        let f = fixture();
        let port = f.root.join("install");
        let old_probe = port.join("bin/gow3-probe.exe");
        put(&old_probe, b"old");
        remove_leftovers(&port);
        assert!(old_probe.exists()); // no new probe: keep the old one
        put(&port.join("bin/spartanport-probe.exe"), b"new");
        remove_leftovers(&port);
        assert!(!old_probe.exists());
    }

    #[test]
    fn old_executable_copies_stay() {
        // The launcher under its old name keeps desktop and Steam shortcuts working.
        let f = fixture();
        let port = f.root.join("install");
        put(&port.join(LEGACY_EXE), b"launcher");
        put(&port.join("SpartanPort.exe"), b"launcher");
        remove_leftovers(&port);
        assert!(port.join(LEGACY_EXE).exists());
    }

    fn python_package(port: &Path) {
        put(&port.join("_internal/base_library.zip"), b"z");
        put(&port.join("run.py"), b"py");
        put(&port.join("scripts/glyphs.py"), b"py");
        put(&port.join("scripts/__pycache__/glyphs.pyc"), b"pyc");
        put(&port.join("scripts/controller_diagrams/xbox.png"), b"png");
        put(&port.join("tools/xvag_voice_swap.py"), b"py");
        put(&port.join("SpartanPort.exe"), b"exe");
        put(&port.join("user/save"), b"save");
        put(&port.join("out/x"), b"x");
        put(&port.join("settings.json"), b"{}");
        put(&port.join("spartanport.ini"), b"ini");
    }

    #[test]
    fn python_leftovers_go_once_the_rust_launcher_is_installed() {
        let f = fixture();
        let port = f.root.join("install");
        python_package(&port);
        remove_python_leftovers(&port);
        for gone in [
            "_internal",
            "run.py",
            "scripts/glyphs.py",
            "scripts/__pycache__",
            "tools",
        ] {
            assert!(!port.join(gone).exists(), "{gone} stays");
        }
        for kept in [
            "scripts/controller_diagrams/xbox.png",
            "SpartanPort.exe",
            "user/save",
            "out/x",
            "settings.json",
            "spartanport.ini",
        ] {
            assert!(port.join(kept).exists(), "{kept} went");
        }
    }

    #[test]
    fn a_checkout_keeps_its_python() {
        let f = fixture();
        let port = f.root.join("checkout");
        python_package(&port);
        fs::remove_dir_all(port.join("_internal")).unwrap();
        remove_python_leftovers(&port);
        for kept in [
            "run.py",
            "scripts/glyphs.py",
            "scripts/__pycache__/glyphs.pyc",
            "tools/xvag_voice_swap.py",
        ] {
            assert!(port.join(kept).exists(), "{kept} went");
        }
    }

    #[test]
    fn tools_with_other_files_stays() {
        let f = fixture();
        let port = f.root.join("install");
        python_package(&port);
        put(&port.join("tools/mine.txt"), b"mine");
        remove_python_leftovers(&port);
        assert!(!port.join("tools/xvag_voice_swap.py").exists());
        assert!(port.join("tools/mine.txt").is_file());
    }

    #[test]
    fn a_failed_removal_keeps_the_cleanup_for_the_next_start() {
        // `_internal/` is what brings the cleanup back: it must outlive any other failure.
        let f = fixture();
        let port = f.root.join("install");
        python_package(&port);
        // A non-empty folder named run.py: remove_file fails on it.
        fs::remove_file(port.join("run.py")).unwrap();
        put(&port.join("run.py/blocker"), b"x");
        remove_python_leftovers(&port);
        assert!(port.join("_internal").is_dir());
        assert!(!port.join("scripts/glyphs.py").exists());
        assert!(!port.join("tools").exists());
        // The blocker goes; the next start finishes the job.
        fs::remove_dir_all(port.join("run.py")).unwrap();
        remove_python_leftovers(&port);
        assert!(!port.join("_internal").exists());
    }

    #[test]
    fn shortcut_migration_skips_when_marker_exists() {
        let f = fixture();
        put(&f.root.join(SHORTCUT_MARKER), b"");
        assert!(!retarget_desktop_shortcut(&f.root, None, None));
    }

    #[cfg(not(windows))]
    #[test]
    fn shortcut_migration_returns_false_on_non_windows() {
        let f = fixture();
        assert!(!retarget_desktop_shortcut(&f.root, None, None));
    }

    #[cfg(windows)]
    mod windows_shortcuts {
        use super::*;

        struct Setup {
            f: Fixture,
            port: PathBuf,
            desktop: PathBuf,
        }

        fn setup() -> Setup {
            let f = fixture();
            let port = f.root.join("port2");
            let desktop = f.root.join("desktop");
            fs::create_dir_all(&desktop).unwrap();
            put(&port.join("SpartanPort.exe"), b"new launcher");
            put(&port.join(LEGACY_EXE), b"old launcher copy");
            put(&port.join(LEGACY_PLAY_EXE), b"play copy");
            Setup { f, port, desktop }
        }

        fn create_shortcut(path: &Path, target: &Path, arguments: &str, working: Option<&Path>) {
            let mut script = format!(
                "$w = New-Object -ComObject WScript.Shell;$s = $w.CreateShortcut(\"{}\");$s.TargetPath = \"{}\";$s.Arguments = \"{arguments}\";",
                path.display(),
                target.display()
            );
            if let Some(working) = working {
                script.push_str(&format!("$s.WorkingDirectory = \"{}\";", working.display()));
            }
            script.push_str("$s.Save()");
            let status = Command::new("powershell")
                .args(["-NoProfile", "-NonInteractive", "-Command", &script])
                .status()
                .unwrap();
            assert!(status.success());
        }

        #[test]
        fn user_created_shortcut_with_custom_name() {
            let s = setup();
            let custom = s.desktop.join("God of War 3.lnk");
            create_shortcut(&custom, &s.port.join(LEGACY_PLAY_EXE), "", None);
            assert!(!retarget_desktop_shortcut(&s.port, Some(&s.desktop), None));
            assert!(custom.is_file());
            assert!(!s.desktop.join("SpartanPort.lnk").exists());
            let _ = &s.f;
        }

        #[test]
        fn shortcut_with_custom_arguments_is_preserved() {
            let s = setup();
            let old = s.desktop.join("God of War III.lnk");
            let work = plain(&s.port);
            create_shortcut(&old, &s.port.join(LEGACY_EXE), "--play", Some(&work));
            assert!(!retarget_desktop_shortcut(&s.port, Some(&s.desktop), None));
            assert!(old.is_file());
            assert!(!s.desktop.join("SpartanPort.lnk").exists());
        }

        #[test]
        fn first_launch_after_upgrading_from_legacy_name() {
            let s = setup();
            let old = s.desktop.join("God of War III.lnk");
            let work = plain(&s.port);
            create_shortcut(&old, &s.port.join(LEGACY_EXE), "", Some(&work));
            assert!(retarget_desktop_shortcut(&s.port, Some(&s.desktop), None));
            assert!(old.is_file(), "the original shortcut is kept");
            assert!(s.desktop.join("SpartanPort.lnk").is_file());
            assert!(s.port.join(SHORTCUT_MARKER).is_file());
            // The player deletes the new one: later starts do not make it again.
            fs::remove_file(s.desktop.join("SpartanPort.lnk")).unwrap();
            assert!(!retarget_desktop_shortcut(&s.port, Some(&s.desktop), None));
            assert!(!s.desktop.join("SpartanPort.lnk").exists());
        }

        #[test]
        fn missing_shortcuts_and_missing_targets() {
            let s = setup();
            fs::remove_file(s.port.join("SpartanPort.exe")).unwrap();
            let old = s.desktop.join("God of War III.lnk");
            create_shortcut(&old, &s.port.join(LEGACY_EXE), "", None);
            assert!(!retarget_desktop_shortcut(&s.port, Some(&s.desktop), None));
            assert!(old.is_file());
            put(&s.port.join("SpartanPort.exe"), b"exe");
            fs::remove_file(&old).unwrap();
            assert!(!retarget_desktop_shortcut(&s.port, Some(&s.desktop), None));
            assert!(s.port.join(SHORTCUT_MARKER).is_file());
        }
    }
}
