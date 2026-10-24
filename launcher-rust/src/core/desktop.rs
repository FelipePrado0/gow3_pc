//! The menu entry that gives the launcher and the game window their icon on Linux, the shortcut
//! buttons, and the few calls that hand a folder or a link to the desktop.

use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use indexmap::IndexMap;

pub const DESKTOP_MARKER: &str = "X-SPARTAN-Integration=true";
/// The menu entry's file name; it is also the window's application id.
const ENTRY_FILE: &str = "spartanport.desktop";
/// The variables of the bundle's own runtime, which programs outside it must not inherit.
const BUNDLE_VARIABLES: [&str; 5] = [
    "LD_LIBRARY_PATH",
    "PYTHONHOME",
    "PYTHONPATH",
    "TCL_LIBRARY",
    "TK_LIBRARY",
];

type Env<'a> = &'a dyn Fn(&str) -> Option<String>;

/// The menu entry; its `StartupWMClass` is the application id of the launcher and of the game
/// window.
pub fn desktop_entry(command: &str, icon: &str, marker: bool) -> String {
    let mut text = format!(
        "[Desktop Entry]\nType=Application\nName=SpartanPort\nExec={command}\nIcon={icon}\n\
         Terminal=false\nCategories=Game;\nStartupWMClass=spartanport\n"
    );
    if marker {
        text.push_str(DESKTOP_MARKER);
        text.push('\n');
    }
    text
}

/// `shlex.split` in POSIX mode: `None` for an unbalanced quote or a trailing backslash.
fn shlex_split(text: &str) -> Option<Vec<String>> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut started = false;
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        match c {
            ' ' | '\t' | '\r' | '\n' => {
                if started {
                    words.push(std::mem::take(&mut word));
                    started = false;
                }
            }
            '\\' => {
                word.push(chars.next()?);
                started = true;
            }
            '\'' => {
                loop {
                    match chars.next()? {
                        '\'' => break,
                        other => word.push(other),
                    }
                }
                started = true;
            }
            '"' => {
                loop {
                    match chars.next()? {
                        '"' => break,
                        '\\' => {
                            let next = chars.next()?;
                            if next != '"' && next != '\\' {
                                word.push('\\');
                            }
                            word.push(next);
                        }
                        other => word.push(other),
                    }
                }
                started = true;
            }
            other => {
                word.push(other);
                started = true;
            }
        }
    }
    if started {
        words.push(word);
    }
    Some(words)
}

/// `NAME=value`, the prefix of an `Exec` line that sets a variable.
fn is_assignment(word: &str) -> bool {
    let Some((name, _)) = word.split_once('=') else {
        return false;
    };
    let mut chars = name.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// The path with symlinks resolved; a path that does not exist stays as written, made absolute.
fn real_path(path: &str) -> PathBuf {
    fs::canonicalize(path)
        .or_else(|_| std::path::absolute(path))
        .unwrap_or_else(|_| PathBuf::from(path))
}

/// Whether one `Exec`/`TryExec` value starts the AppImage at `here`.
fn starts(value: &str, here: &Path) -> bool {
    let Some(mut words) = shlex_split(value) else {
        return false;
    };
    let skip = words
        .iter()
        .take_while(|w| *w == "env" || is_assignment(w))
        .count();
    words.drain(..skip);
    // A file another program left broken (a NUL byte in a path) is ignored, not fatal.
    words
        .first()
        .is_some_and(|program| !program.contains('\0') && real_path(program) == here)
}

/// Menu entries, other than ours, that start this very AppImage: another tool (Gear Lever,
/// AppImageLauncher, appimaged) has integrated it. The program is the first word of `Exec` or
/// `TryExec`, after any `env NAME=value` prefix and the quotes.
pub fn other_desktop_entries(applications: &Path, appimage: &str) -> Vec<PathBuf> {
    let here = real_path(appimage);
    let Ok(listing) = fs::read_dir(applications) else {
        return Vec::new();
    };
    let mut entries: Vec<PathBuf> = listing
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.extension().is_some_and(|e| e == "desktop")
                && p.file_name().is_some_and(|n| n != ENTRY_FILE)
        })
        .collect();
    entries.sort();
    entries
        .into_iter()
        .filter(|entry| {
            let Ok(bytes) = fs::read(entry) else {
                return false;
            };
            String::from_utf8_lossy(&bytes).lines().any(|line| {
                let (key, value) = line.split_once('=').unwrap_or((line, ""));
                matches!(key, "Exec" | "TryExec") && starts(value, &here)
            })
        })
        .collect()
}

/// True when another tool (Gear Lever sets `DESKTOPINTEGRATION`, AppImageLauncher and appimaged
/// leave an entry) already put this AppImage in the menu: an entry of ours would be a duplicate.
pub fn integrated_by_other_tool(applications: &Path, appimage: &str, env: Env) -> bool {
    env("DESKTOPINTEGRATION").is_some_and(|v| !v.is_empty())
        || (!appimage.is_empty() && !other_desktop_entries(applications, appimage).is_empty())
}

fn home_of(home: Option<&Path>, env: Env) -> Option<PathBuf> {
    let key = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    home.map(Path::to_path_buf)
        .or_else(|| env(key).filter(|v| !v.is_empty()).map(PathBuf::from))
}

fn mentions_marker(path: &Path) -> bool {
    fs::read(path).is_ok_and(|bytes| String::from_utf8_lossy(&bytes).contains(DESKTOP_MARKER))
}

fn integrate(
    enabled: bool,
    appimage: Option<&str>,
    icon_source: &Path,
    data_dir: &Path,
    home: &Path,
    env: Env,
) -> io::Result<Option<PathBuf>> {
    let applications = home.join(".local").join("share").join("applications");
    let entry = applications.join(ENTRY_FILE);
    let ours = entry.is_file() && mentions_marker(&entry);
    if !enabled {
        if ours {
            fs::remove_file(&entry)?;
        }
        return Ok(None);
    }
    let appimage = match appimage {
        Some(a) if !a.is_empty() && !(entry.is_file() && !ours) => a,
        _ => return Ok(None),
    };
    if integrated_by_other_tool(&applications, appimage, env) {
        if ours {
            fs::remove_file(&entry)?;
        }
        return Ok(None);
    }
    let icon = data_dir.join("spartanport.png");
    fs::create_dir_all(data_dir)?;
    if !icon.is_file() || fs::read(&icon)? != fs::read(icon_source)? {
        fs::copy(icon_source, &icon)?;
    }
    let text = desktop_entry(&format!("\"{appimage}\""), &icon.to_string_lossy(), true);
    fs::create_dir_all(&applications)?;
    if !entry.is_file() || fs::read_to_string(&entry)? != text {
        fs::write(&entry, text)?;
    }
    Ok(Some(entry))
}

/// AppImage on Linux: the menu entry the desktop matches the windows (application id
/// `spartanport`) to, which is what shows their icon in the dock. The entry is ours while it
/// carries the marker; one without it was written by hand (the Desktop shortcut button) and is
/// left alone. When another tool already integrated this AppImage (an entry that starts it, or
/// `DESKTOPINTEGRATION` in the environment, which Gear Lever sets), ours would be a duplicate:
/// it is not made, and removed.
pub fn integrate_desktop(
    enabled: bool,
    appimage: Option<&str>,
    icon_source: &Path,
    data_dir: &Path,
    home: Option<&Path>,
    env: Env,
) -> Option<PathBuf> {
    // Runs before the window opens: it must never stop the launcher.
    let home = home_of(home, env)?;
    integrate(enabled, appimage, icon_source, data_dir, &home, env)
        .ok()
        .flatten()
}

#[cfg(unix)]
fn make_executable(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> io::Result<()> {
    Ok(())
}

/// The shortcut button: the desktop entry on the desktop folder (when it exists) and in the
/// applications menu, except in the menu when another tool already integrated this AppImage (a
/// second entry would show the game twice). Returns the files written.
pub fn write_linux_shortcut(
    command: &str,
    appimage: &str,
    icon_source: &Path,
    desktop: &Path,
    home: Option<&Path>,
    env: Env,
) -> io::Result<Vec<String>> {
    let home = home_of(home, env)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "the home folder is unknown"))?;
    let icon_dir = home.join(".local").join("share").join("icons");
    let icon_file = icon_dir.join("spartanport.png");
    let icon = match fs::create_dir_all(&icon_dir).and_then(|()| fs::copy(icon_source, &icon_file))
    {
        Ok(_) => icon_file.to_string_lossy().into_owned(),
        Err(_) => "spartanport".to_string(),
    };
    let entry = desktop_entry(command, &icon, false);
    let applications = home.join(".local").join("share").join("applications");
    let mut folders = Vec::new();
    if !integrated_by_other_tool(&applications, appimage, env) {
        folders.push(applications.clone());
    }
    folders.push(desktop.to_path_buf());
    let first = folders[0].clone();
    let mut created = Vec::new();
    for folder in &folders {
        // The first one is the point of the click.
        if !folder.is_dir() && *folder != first {
            continue;
        }
        fs::create_dir_all(folder)?;
        // In the applications menu the file is named after the window's application id.
        let name = if *folder == applications {
            ENTRY_FILE
        } else {
            "SpartanPort.desktop"
        };
        let target = folder.join(name);
        fs::write(&target, &entry)?;
        make_executable(&target)?;
        created.push(target.to_string_lossy().into_owned());
    }
    Ok(created)
}

#[cfg(windows)]
fn no_window(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    command.creation_flags(0x0800_0000);
}

#[cfg(not(windows))]
fn no_window(_command: &mut Command) {}

/// The shortcut's icon (R92): the port's `launcher/spartanport.ico` when it is there, as the
/// Python run from source sets it, else the first icon of `exe`.
fn shortcut_icon(port_dir: &Path, exe: &Path) -> String {
    let ico = port_dir.join("launcher").join("spartanport.ico");
    if ico.is_file() {
        ico.display().to_string()
    } else {
        format!("{},0", exe.display())
    }
}

/// The PowerShell that makes `SpartanPort.lnk` on the desktop.
fn windows_shortcut_script(port_dir: &Path, exe: &Path) -> String {
    format!(
        "$s=(New-Object -ComObject WScript.Shell).CreateShortcut([Environment]::GetFolderPath(\"Desktop\")\
         +\"\\SpartanPort.lnk\");$s.TargetPath=\"{exe}\";$s.WorkingDirectory=\"{port}\";\
         $s.IconLocation=\"{icon}\";$s.Save()",
        exe = exe.display(),
        port = port_dir.display(),
        icon = shortcut_icon(port_dir, exe)
    )
}

/// `SpartanPort.lnk` on the desktop, pointing to `exe` and starting in `port_dir`. The error is
/// what PowerShell said (its first 400 characters).
pub fn windows_shortcut(port_dir: &Path, exe: &Path) -> Result<(), String> {
    if !cfg!(windows) {
        return Err("desktop shortcuts of this kind exist only on Windows".to_string());
    }
    let mut command = Command::new("powershell");
    command
        .args([
            "-NoProfile",
            "-Command",
            &windows_shortcut_script(port_dir, exe),
        ])
        .stdin(Stdio::null());
    no_window(&mut command);
    let output = command.output().map_err(|e| e.to_string())?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr)
            .chars()
            .take(400)
            .collect())
    }
}

/// Which of the optional Linux integrations this system has.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LinuxTools {
    pub wayland: bool,
    pub gamemode: bool,
    pub mangohud: bool,
}

#[cfg(unix)]
fn is_runnable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_runnable(path: &Path) -> bool {
    path.is_file()
}

/// `shutil.which`: an executable called `name` in one of the folders of `path`.
fn on_path(name: &str, path: &OsStr) -> bool {
    std::env::split_paths(path).any(|dir| is_runnable(&dir.join(name)))
}

/// A Vulkan layer manifest of MangoHud (`*angoHud*.json`) in one of `dirs`.
fn has_mangohud_layer(dirs: &[PathBuf]) -> bool {
    dirs.iter().any(|dir| {
        fs::read_dir(dir).is_ok_and(|listing| {
            listing.flatten().any(|e| {
                let name = e.file_name().to_string_lossy().into_owned();
                name.contains("angoHud") && name.ends_with(".json")
            })
        })
    })
}

pub fn linux_tools() -> LinuxTools {
    let path = std::env::var_os("PATH").unwrap_or_default();
    let mut layers = vec![
        PathBuf::from("/usr/share/vulkan/implicit_layer.d"),
        PathBuf::from("/etc/vulkan/implicit_layer.d"),
    ];
    if let Some(home) = std::env::var_os("HOME") {
        layers.push(PathBuf::from(home).join(".local/share/vulkan/implicit_layer.d"));
    }
    LinuxTools {
        wayland: std::env::var_os("WAYLAND_DISPLAY").is_some_and(|v| !v.is_empty()),
        gamemode: on_path("gamemoderun", &path),
        mangohud: on_path("mangohud", &path) || has_mangohud_layer(&layers),
    }
}

fn without_bundle_variables(mut env: IndexMap<String, String>) -> IndexMap<String, String> {
    for name in BUNDLE_VARIABLES {
        env.shift_remove(name);
    }
    env
}

/// The environment of the user's session for programs outside the bundle (`xdg-open`).
pub fn host_environment() -> IndexMap<String, String> {
    without_bundle_variables(
        std::env::vars_os()
            .map(|(k, v)| {
                (
                    k.to_string_lossy().into_owned(),
                    v.to_string_lossy().into_owned(),
                )
            })
            .collect(),
    )
}

/// Hands `target` to the desktop's opener, which keeps running on its own.
fn open_with_desktop(target: &OsStr, is_url: bool) -> io::Result<()> {
    let mut command;
    if cfg!(windows) {
        // `os.startfile` for a folder or file; the URL handler runs no shell, so `&` is safe.
        command = if is_url {
            let mut c = Command::new("rundll32");
            c.arg("url.dll,FileProtocolHandler");
            c
        } else {
            Command::new("explorer")
        };
        command.arg(target);
        no_window(&mut command);
    } else {
        command = Command::new("xdg-open");
        command.arg(target).env_clear().envs(host_environment());
    }
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(drop)
}

/// Opens a folder or file with the desktop's default program.
pub fn open_path(path: &Path) -> io::Result<()> {
    open_with_desktop(path.as_os_str(), false)
}

/// Opens a link in the default browser.
pub fn open_url(url: &str) -> io::Result<()> {
    open_with_desktop(OsStr::new(url), true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    const APPIMAGE: &str = "/games/spartanport.AppImage";

    struct Fixture {
        _temp: tempfile::TempDir,
        home: PathBuf,
        icon: PathBuf,
        data: PathBuf,
        entry: PathBuf,
    }

    fn fixture() -> Fixture {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().to_path_buf();
        let icon = home.join("source.png");
        fs::write(&icon, b"png one").unwrap();
        let entry = home
            .join(".local")
            .join("share")
            .join("applications")
            .join("spartanport.desktop");
        Fixture {
            data: home.join("data"),
            _temp: temp,
            home,
            icon,
            entry,
        }
    }

    fn no_env(_: &str) -> Option<String> {
        None
    }

    fn env_with(name: &'static str, value: &'static str) -> impl Fn(&str) -> Option<String> {
        move |key| (key == name).then(|| value.to_string())
    }

    impl Fixture {
        fn applications(&self) -> PathBuf {
            self.entry.parent().unwrap().to_path_buf()
        }

        // The environment is ours, not the shell's: DESKTOPINTEGRATION in the real one must not
        // decide a test.
        fn integrate_with(
            &self,
            enabled: bool,
            appimage: Option<&str>,
            env: &dyn Fn(&str) -> Option<String>,
        ) -> Option<PathBuf> {
            integrate_desktop(
                enabled,
                appimage,
                &self.icon,
                &self.data,
                Some(&self.home),
                env,
            )
        }

        fn integrate(&self) -> Option<PathBuf> {
            self.integrate_with(true, Some(APPIMAGE), &no_env)
        }

        fn other_entry(&self, text: &str, name: &str) -> PathBuf {
            let path = self.applications().join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, format!("[Desktop Entry]\nName=SpartanPort\n{text}")).unwrap();
            path
        }

        fn other_bytes(&self, content: &[u8], name: &str) -> PathBuf {
            let path = self.applications().join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, content).unwrap();
            path
        }

        fn others(&self) -> Vec<PathBuf> {
            other_desktop_entries(&self.applications(), APPIMAGE)
        }
    }

    #[test]
    fn entry_names_the_appimage_the_icon_and_the_window_class() {
        let f = fixture();
        assert_eq!(f.integrate(), Some(f.entry.clone()));
        let text = fs::read_to_string(&f.entry).unwrap();
        assert!(text.contains(&format!("Exec=\"{APPIMAGE}\"")));
        assert!(text.contains(&format!(
            "Icon={}",
            f.data.join("spartanport.png").display()
        )));
        assert!(text.contains("StartupWMClass=spartanport"));
        assert_eq!(
            fs::read(f.data.join("spartanport.png")).unwrap(),
            b"png one"
        );
    }

    #[test]
    fn the_entry_text_is_exactly_the_menu_entry() {
        assert_eq!(
            desktop_entry("\"/a b\"", "icon.png", true),
            "[Desktop Entry]\nType=Application\nName=SpartanPort\nExec=\"/a b\"\nIcon=icon.png\n\
             Terminal=false\nCategories=Game;\nStartupWMClass=spartanport\nX-SPARTAN-Integration=true\n"
        );
        assert!(!desktop_entry("x", "y", false).contains(DESKTOP_MARKER));
    }

    #[test]
    fn a_moved_appimage_or_a_new_icon_updates_the_entry() {
        let f = fixture();
        f.integrate();
        fs::write(&f.icon, b"png two").unwrap();
        f.integrate_with(true, Some("/elsewhere/spartanport.AppImage"), &no_env);
        assert!(fs::read_to_string(&f.entry)
            .unwrap()
            .contains("Exec=\"/elsewhere/spartanport.AppImage\""));
        assert_eq!(
            fs::read(f.data.join("spartanport.png")).unwrap(),
            b"png two"
        );
    }

    #[test]
    fn turning_it_off_removes_only_our_entry() {
        let f = fixture();
        f.integrate();
        f.integrate_with(false, Some(APPIMAGE), &no_env);
        assert!(!f.entry.exists());
        fs::write(&f.entry, "[Desktop Entry]\nName=made by hand\n").unwrap();
        f.integrate_with(false, Some(APPIMAGE), &no_env);
        assert!(f.entry.exists());
    }

    #[test]
    fn an_entry_made_by_hand_is_left_alone() {
        let f = fixture();
        fs::create_dir_all(f.applications()).unwrap();
        fs::write(&f.entry, "[Desktop Entry]\nName=made by hand\n").unwrap();
        assert_eq!(f.integrate(), None);
        assert_eq!(
            fs::read_to_string(&f.entry).unwrap(),
            "[Desktop Entry]\nName=made by hand\n"
        );
    }

    #[test]
    fn an_entry_of_another_tool_for_the_same_file_means_none_of_ours() {
        let f = fixture();
        for line in [
            format!("Exec=env DESKTOPINTEGRATION=1 {APPIMAGE}"),
            format!("Exec=\"{APPIMAGE}\" %U"),
            format!("TryExec={APPIMAGE}"),
        ] {
            let other = f.other_entry(&line, "gearlever.desktop");
            assert_eq!(f.integrate(), None, "{line}");
            assert!(!f.entry.exists());
            fs::remove_file(other).unwrap();
        }
    }

    #[test]
    fn our_entry_is_removed_once_another_tool_integrates_the_file() {
        let f = fixture();
        f.integrate();
        assert!(f.entry.exists());
        f.other_entry(
            &format!("Exec=env DESKTOPINTEGRATION=1 {APPIMAGE}"),
            "gearlever.desktop",
        );
        assert_eq!(f.integrate(), None);
        assert!(!f.entry.exists());
    }

    #[test]
    fn an_entry_for_another_file_does_not_count() {
        let f = fixture();
        f.other_entry("Exec=/games/elsewhere.AppImage", "gearlever.desktop");
        assert_eq!(f.integrate(), Some(f.entry.clone()));
    }

    #[test]
    fn the_variable_gear_lever_sets_skips_it_too() {
        let f = fixture();
        let env = env_with("DESKTOPINTEGRATION", "1");
        assert_eq!(f.integrate_with(true, Some(APPIMAGE), &env), None);
        assert!(!f.entry.exists());
    }

    #[test]
    fn an_empty_variable_does_not_count() {
        let f = fixture();
        let env = env_with("DESKTOPINTEGRATION", "");
        assert_eq!(
            f.integrate_with(true, Some(APPIMAGE), &env),
            Some(f.entry.clone())
        );
    }

    #[test]
    fn a_nul_byte_in_another_entry_is_ignored() {
        let f = fixture();
        for key in ["Exec", "TryExec"] {
            let broken = f.other_bytes(
                format!("[Desktop Entry]\n{key}=/games/\0spartanport.AppImage\n").as_bytes(),
                "broken.desktop",
            );
            assert_eq!(f.integrate(), Some(f.entry.clone()));
            assert!(f.others().is_empty());
            fs::remove_file(&f.entry).unwrap();
            fs::remove_file(broken).unwrap();
        }
    }

    #[test]
    fn a_broken_entry_does_not_hide_a_good_one() {
        let f = fixture();
        f.other_bytes(b"[Desktop Entry]\nExec=/a/\0b\n", "a-broken.desktop");
        let good = f.other_entry(
            &format!("Exec=env DESKTOPINTEGRATION=1 {APPIMAGE}"),
            "z-good.desktop",
        );
        assert_eq!(f.others(), vec![good]);
        assert_eq!(f.integrate(), None);
    }

    #[test]
    fn entries_that_are_not_text_or_not_files_are_ignored() {
        let f = fixture();
        f.other_bytes(b"\xff\xfe\x00\x80Exec=\xff", "binary.desktop");
        f.other_bytes(b"", "empty.desktop");
        fs::create_dir(f.applications().join("folder.desktop")).unwrap();
        assert_eq!(f.integrate(), Some(f.entry.clone()));
    }

    #[cfg(unix)]
    #[test]
    fn a_dangling_symlink_entry_is_ignored() {
        let f = fixture();
        fs::create_dir_all(f.applications()).unwrap();
        std::os::unix::fs::symlink(
            f.home.join("nowhere"),
            f.applications().join("dangling.desktop"),
        )
        .unwrap();
        assert_eq!(f.integrate(), Some(f.entry.clone()));
    }

    #[test]
    fn lines_that_do_not_parse_are_ignored() {
        let f = fixture();
        for line in [
            "Exec=\"/games/spartanport.AppImage",
            "Exec=",
            "Exec=env",
            "Exec=env A=1 B=2",
            "Exec",
            "TryExec=   ",
            "Exec='unbalanced",
            "Exec=%U",
        ] {
            f.other_entry(line, "odd.desktop");
            assert!(f.others().is_empty(), "{line}");
        }
        assert_eq!(f.integrate(), Some(f.entry.clone()));
    }

    #[test]
    fn a_path_with_spaces_counts() {
        let f = fixture();
        let real = f.home.join("My Games/spartanport.AppImage");
        fs::create_dir_all(real.parent().unwrap()).unwrap();
        fs::write(&real, b"x").unwrap();
        f.other_entry(
            &format!("Exec=env DESKTOPINTEGRATION=1 \"{}\" %U", real.display()),
            "spaces.desktop",
        );
        let appimage = real.to_string_lossy().into_owned();
        assert_eq!(f.integrate_with(true, Some(&appimage), &no_env), None);
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_to_the_file_counts() {
        let f = fixture();
        let real = f.home.join("My Games/spartanport.AppImage");
        fs::create_dir_all(real.parent().unwrap()).unwrap();
        fs::write(&real, b"x").unwrap();
        let link = f.home.join("link.AppImage");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        f.other_entry(
            &format!("Exec=env DESKTOPINTEGRATION=1 \"{}\" %U", real.display()),
            "spaces.desktop",
        );
        let real = real.to_string_lossy().into_owned();
        let link = link.to_string_lossy().into_owned();
        assert_eq!(f.integrate_with(true, Some(&real), &no_env), None);
        assert_eq!(f.integrate_with(true, Some(&link), &no_env), None);
    }

    #[test]
    fn our_own_entry_never_counts_as_another_tool() {
        let f = fixture();
        f.integrate();
        assert!(f.others().is_empty());
        assert_eq!(f.integrate(), Some(f.entry.clone()));
    }

    #[test]
    fn a_missing_applications_folder_means_no_other_tool() {
        let f = fixture();
        assert!(other_desktop_entries(&f.home.join("nowhere"), APPIMAGE).is_empty());
        assert!(!integrated_by_other_tool(
            &f.home.join("nowhere"),
            APPIMAGE,
            &no_env
        ));
    }

    #[test]
    fn our_entry_survives_a_broken_neighbour_across_starts() {
        let f = fixture();
        f.other_bytes(b"[Desktop Entry]\nExec=/a/\0b\n", "broken.desktop");
        for _ in 0..3 {
            assert_eq!(f.integrate(), Some(f.entry.clone()));
        }
        assert!(fs::read_to_string(&f.entry)
            .unwrap()
            .contains(DESKTOP_MARKER));
    }

    #[test]
    fn without_an_appimage_nothing_is_written() {
        let f = fixture();
        assert_eq!(f.integrate_with(true, None, &no_env), None);
        assert!(!f.entry.exists());
    }

    // ---- The shortcut button ----------------------------------------------------------------

    struct Click {
        f: Fixture,
        desktop: PathBuf,
    }

    fn click_fixture() -> Click {
        let f = fixture();
        let desktop = f.home.join("Desktop");
        Click { f, desktop }
    }

    impl Click {
        fn click_with(&self, env: &dyn Fn(&str) -> Option<String>) -> Vec<String> {
            write_linux_shortcut(
                &format!("\"{APPIMAGE}\""),
                APPIMAGE,
                &self.f.icon,
                &self.desktop,
                Some(&self.f.home),
                env,
            )
            .unwrap()
        }
        fn click(&self) -> Vec<String> {
            self.click_with(&no_env)
        }
        fn menu(&self) -> PathBuf {
            self.f.entry.clone()
        }
        fn on_desktop(&self) -> PathBuf {
            self.desktop.join("SpartanPort.desktop")
        }
    }

    fn shown(path: &Path) -> String {
        path.to_string_lossy().into_owned()
    }

    #[test]
    fn it_makes_the_menu_entry_and_the_desktop_one_again_after_they_are_deleted() {
        let c = click_fixture();
        fs::create_dir(&c.desktop).unwrap();
        for _ in 0..2 {
            assert_eq!(c.click(), vec![shown(&c.menu()), shown(&c.on_desktop())]);
            fs::remove_dir_all(c.f.applications()).unwrap();
            fs::remove_file(c.on_desktop()).unwrap();
        }
        assert!(c
            .f
            .home
            .join(".local")
            .join("share")
            .join("icons")
            .join("spartanport.png")
            .is_file());
    }

    #[test]
    fn the_desktop_folder_is_not_made_when_it_does_not_exist() {
        let c = click_fixture();
        assert_eq!(c.click(), vec![shown(&c.menu())]);
        assert!(!c.desktop.exists());
    }

    #[test]
    fn another_tools_entry_keeps_the_menu_free_of_a_duplicate() {
        let c = click_fixture();
        c.f.other_entry(
            &format!("Exec=env DESKTOPINTEGRATION=1 {APPIMAGE}"),
            "tool.desktop",
        );
        fs::create_dir(&c.desktop).unwrap();
        assert_eq!(c.click(), vec![shown(&c.on_desktop())]);
        assert!(!c.menu().exists());
    }

    #[test]
    fn a_broken_entry_of_another_program_does_not_stop_the_button() {
        let c = click_fixture();
        c.f.other_bytes(b"[Desktop Entry]\nExec=/a/\0b\n", "broken.desktop");
        fs::create_dir(&c.desktop).unwrap();
        assert_eq!(c.click(), vec![shown(&c.menu()), shown(&c.on_desktop())]);
    }

    #[test]
    fn the_files_run_the_appimage_with_its_icon() {
        let c = click_fixture();
        fs::create_dir(&c.desktop).unwrap();
        let icon =
            c.f.home
                .join(".local")
                .join("share")
                .join("icons")
                .join("spartanport.png");
        for path in c.click() {
            let text = fs::read_to_string(path).unwrap();
            assert!(text.contains(&format!("Exec=\"{APPIMAGE}\"")));
            assert!(text.contains(&format!("Icon={}", icon.display())));
            assert!(text.contains("StartupWMClass=spartanport"));
            assert!(!text.contains(DESKTOP_MARKER));
        }
    }

    #[cfg(unix)]
    #[test]
    fn the_files_are_executable() {
        use std::os::unix::fs::PermissionsExt;
        let c = click_fixture();
        fs::create_dir(&c.desktop).unwrap();
        for path in c.click() {
            let mode = fs::metadata(path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o755);
        }
    }

    #[test]
    fn a_missing_icon_falls_back_to_the_theme_name() {
        let c = click_fixture();
        fs::remove_file(&c.f.icon).unwrap();
        let created = c.click();
        assert!(fs::read_to_string(&created[0])
            .unwrap()
            .contains("Icon=spartanport\n"));
    }

    #[test]
    fn clicking_again_rewrites_the_same_files() {
        let c = click_fixture();
        fs::create_dir(&c.desktop).unwrap();
        let first = c.click();
        assert_eq!(c.click(), first);
    }

    #[test]
    fn the_environment_variable_of_gear_lever_does_the_same() {
        let c = click_fixture();
        let env = env_with("DESKTOPINTEGRATION", "1");
        fs::create_dir(&c.desktop).unwrap();
        assert_eq!(c.click_with(&env), vec![shown(&c.on_desktop())]);
        assert!(!c.menu().exists());
    }

    // ---- Tools, the host environment and the Windows shortcut --------------------------------

    #[test]
    fn the_host_environment_drops_the_bundles_variables() {
        let env: IndexMap<String, String> = [
            ("LD_LIBRARY_PATH", "/tmp/.mount_x/usr/lib"),
            ("PYTHONHOME", "/p"),
            ("PYTHONPATH", "/p"),
            ("TCL_LIBRARY", "/t"),
            ("TK_LIBRARY", "/t"),
            ("HOME", "/h"),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
        let kept = without_bundle_variables(env);
        assert_eq!(kept.keys().collect::<Vec<_>>(), ["HOME"]);
    }

    #[test]
    fn mangohud_is_found_by_its_vulkan_layer_file() {
        let f = fixture();
        let layers = f.home.join("implicit_layer.d");
        fs::create_dir(&layers).unwrap();
        assert!(!has_mangohud_layer(std::slice::from_ref(&layers)));
        fs::write(layers.join("VkLayer_MESA_overlay.json"), b"{}").unwrap();
        assert!(!has_mangohud_layer(std::slice::from_ref(&layers)));
        fs::write(layers.join("MangoHud.json"), b"{}").unwrap();
        assert!(has_mangohud_layer(&[f.home.join("nowhere"), layers]));
    }

    #[cfg(unix)]
    #[test]
    fn which_finds_an_executable_on_the_path_only() {
        use std::os::unix::fs::PermissionsExt;
        let f = fixture();
        let program = f.home.join("mangohud");
        fs::write(&program, b"#!/bin/sh\n").unwrap();
        let path = std::env::join_paths([&f.home]).unwrap();
        assert!(!on_path("mangohud", &path));
        fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(on_path("mangohud", &path));
        assert!(!on_path("gamemoderun", &path));
    }

    #[test]
    fn the_shortcut_takes_the_ports_icon_file_when_there_is_one() {
        // The user's screenshot: the shortcut of a launcher without an embedded icon showed the
        // generic window. The Python (from source) points it at `launcher/spartanport.ico` (R92).
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("SpartanPort.exe");
        let ico = dir.path().join("launcher").join("spartanport.ico");
        let icon_of = |script: &str| {
            let start = script.find("$s.IconLocation=\"").unwrap() + "$s.IconLocation=\"".len();
            script[start..start + script[start..].find('"').unwrap()].to_string()
        };
        assert_eq!(
            icon_of(&windows_shortcut_script(dir.path(), &exe)),
            format!("{},0", exe.display())
        );
        fs::create_dir_all(ico.parent().unwrap()).unwrap();
        fs::write(&ico, b"ico").unwrap();
        assert_eq!(
            icon_of(&windows_shortcut_script(dir.path(), &exe)),
            ico.display().to_string()
        );
    }

    #[test]
    fn the_windows_shortcut_script_names_the_target_folder_and_icon() {
        let script = windows_shortcut_script(
            Path::new("C:\\Games\\Port"),
            Path::new("C:\\Games\\Port\\SpartanPort.exe"),
        );
        assert_eq!(
            script,
            "$s=(New-Object -ComObject WScript.Shell).CreateShortcut([Environment]::GetFolderPath(\"Desktop\")+\"\\SpartanPort.lnk\");\
             $s.TargetPath=\"C:\\Games\\Port\\SpartanPort.exe\";$s.WorkingDirectory=\"C:\\Games\\Port\";\
             $s.IconLocation=\"C:\\Games\\Port\\SpartanPort.exe,0\";$s.Save()"
        );
    }
}
