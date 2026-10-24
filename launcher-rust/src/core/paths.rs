//! Where the launcher keeps its files, resolved once from the environment.

use std::path::{Path, PathBuf};

/// Folder name of the previous launcher's settings (`scripts/legacy.py`).
const LEGACY_CONFIG_DIR: &str = "gow3-launcher";

/// Every launcher folder and file, derived from the executable's folder and the environment:
/// `SPARTAN_PORT_DIR` names the port folder (default: the executable's; absolute, even when the
/// variable is relative), whose `bin/` and `scripts/` the launcher uses;
/// `SPARTAN_DATA_DIR` the data folder (default: the port folder, as the Python's
/// `DATA_DIR = PORT_DIR`); `SPARTAN_CONFIG` the ini.
#[derive(Debug, Clone)]
pub struct Paths {
    pub port_dir: PathBuf,
    pub data_dir: PathBuf,
    ini: PathBuf,
    legacy_config_dir: PathBuf,
    /// `SPARTAN_GAME_DIR` as the environment given to `from_env` had it: the game folder a new
    /// `settings.json` starts with.
    game_dir_default: String,
}

/// The port folder of a launcher started without `SPARTAN_PORT_DIR`: the exe's folder when
/// `launcher/spartanport.png` is beside it (a packaged port); else the nearest folder above
/// holding it (a cargo build in `launcher-rust/target/<profile>/`); else the exe's folder, whose
/// missing probe PLAY reports.
pub fn default_port_dir(exe_dir: &Path) -> PathBuf {
    let is_port = |dir: &Path| dir.join("launcher").join("spartanport.png").is_file();
    if is_port(exe_dir) {
        return exe_dir.to_path_buf();
    }
    exe_dir
        .ancestors()
        .skip(1)
        .find(|dir| is_port(dir))
        .map_or_else(|| exe_dir.to_path_buf(), Path::to_path_buf)
}

impl Paths {
    /// `env` reads one environment variable; a relative `SPARTAN_PORT_DIR` is also joined to the
    /// current folder. Nothing else touches the OS.
    pub fn from_env(exe_dir: &Path, env: &dyn Fn(&str) -> Option<String>) -> Paths {
        // Made absolute here, once: the game and the Python launcher start inside it.
        let port_dir = env("SPARTAN_PORT_DIR")
            .filter(|dir| !dir.is_empty())
            .map_or_else(
                || exe_dir.to_path_buf(),
                |dir| std::path::absolute(&dir).unwrap_or_else(|_| PathBuf::from(dir)),
            );
        let data_dir = env("SPARTAN_DATA_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| port_dir.clone());
        let ini = env("SPARTAN_CONFIG")
            .map(PathBuf::from)
            .unwrap_or_else(|| data_dir.join("spartanport.ini"));
        let home = || {
            let key = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
            env(key)
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("."))
        };
        let base = if cfg!(windows) {
            env("APPDATA").map(PathBuf::from).unwrap_or_else(home)
        } else {
            env("XDG_CONFIG_HOME")
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
                .unwrap_or_else(|| home().join(".config"))
        };
        Paths {
            port_dir,
            data_dir,
            ini,
            legacy_config_dir: base.join(LEGACY_CONFIG_DIR),
            game_dir_default: env("SPARTAN_GAME_DIR").unwrap_or_default(),
        }
    }

    pub fn game_dir_default(&self) -> &str {
        &self.game_dir_default
    }

    pub fn config_file(&self) -> PathBuf {
        self.data_dir.join("settings.json")
    }

    pub fn legacy_config_file(&self) -> PathBuf {
        self.legacy_config_dir.join("settings.json")
    }

    pub fn ini_path(&self) -> PathBuf {
        self.ini.clone()
    }

    /// The saves folder: the setting, or `data_dir/user` when it is empty.
    pub fn user_dir(&self, settings_user_dir: &str) -> PathBuf {
        self.setting_or_default(settings_user_dir, "user")
    }

    pub fn mods_dir(&self, settings_mods_dir: &str) -> PathBuf {
        self.setting_or_default(settings_mods_dir, "mods")
    }

    pub fn patches_dir(&self, settings_patches_dir: &str) -> PathBuf {
        self.setting_or_default(settings_patches_dir, "patches")
    }

    pub fn mod_presets(&self) -> PathBuf {
        self.data_dir.join("mods_presets.json")
    }

    pub fn mods_json(&self) -> PathBuf {
        self.data_dir.join("mods.json")
    }

    pub fn patches_json(&self) -> PathBuf {
        self.data_dir.join("patches.json")
    }

    /// `<saves folder>/last_run.log`, where `user_dir` is the saves-folder setting.
    pub fn run_log_path(&self, user_dir: &str) -> PathBuf {
        self.user_dir(user_dir).join("last_run.log")
    }

    /// A port executable: `bin/` in a packaged build, `out/` in a source checkout.
    pub fn find_tool(&self, name: &str) -> Option<PathBuf> {
        let file = if cfg!(windows) {
            format!("{name}.exe")
        } else {
            name.to_string()
        };
        ["bin", "out"]
            .iter()
            .map(|folder| self.port_dir.join(folder).join(&file))
            .find(|candidate| candidate.is_file())
    }

    /// Next to the port; in the data folder when the port folder is read-only (AppImage).
    pub fn fsr4_folder(&self) -> PathBuf {
        let writable = cfg!(windows) || folder_is_writable(&self.port_dir);
        if writable {
            self.port_dir.join("fsr4_shaders")
        } else {
            self.data_dir.join("fsr4_shaders")
        }
    }

    fn setting_or_default(&self, setting: &str, default: &str) -> PathBuf {
        if setting.is_empty() {
            self.data_dir.join(default)
        } else {
            PathBuf::from(setting)
        }
    }
}

/// Like `os.access(W_OK)`: whether a file can really be created in `dir` (probe is removed on drop).
fn folder_is_writable(dir: &Path) -> bool {
    tempfile::NamedTempFile::new_in(dir).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn env_of(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |key| map.get(key).cloned()
    }

    #[test]
    fn data_dir_defaults_to_the_executables_folder() {
        let paths = Paths::from_env(Path::new("exe"), &env_of(&[]));
        assert_eq!(paths.port_dir, Path::new("exe"));
        assert_eq!(paths.data_dir, Path::new("exe"));
        assert_eq!(paths.config_file(), Path::new("exe").join("settings.json"));
        assert_eq!(
            paths.mod_presets(),
            Path::new("exe").join("mods_presets.json")
        );
    }

    fn mark_port(dir: &Path) {
        std::fs::create_dir_all(dir.join("launcher")).unwrap();
        std::fs::write(dir.join("launcher").join("spartanport.png"), b"").unwrap();
    }

    #[test]
    fn a_cargo_build_runs_from_its_checkout() {
        // A cargo build sits in `launcher-rust/target/<profile>/` and climbs to the checkout,
        // the nearest folder holding `launcher/spartanport.png`: no run.py is needed.
        let dir = tempfile::tempdir().unwrap();
        let checkout = dir.path().join("checkout");
        let exe_dir = checkout
            .join("launcher-rust")
            .join("target")
            .join("release");
        std::fs::create_dir_all(&exe_dir).unwrap();
        mark_port(&checkout);
        assert_eq!(default_port_dir(&exe_dir), checkout);
        // A packaged port has the picture beside the exe: the exe's folder.
        let port = dir.path().join("port");
        mark_port(&port);
        assert_eq!(default_port_dir(&port), port);
        // A port extracted inside the checkout is still its own folder.
        let nested = checkout.join("dist").join("port");
        mark_port(&nested);
        assert_eq!(default_port_dir(&nested), nested);
        // Nothing above: the exe's folder, and the missing probe is reported by PLAY.
        let alone = dir.path().join("alone").join("target").join("debug");
        std::fs::create_dir_all(&alone).unwrap();
        assert_eq!(default_port_dir(&alone), alone);
        // A folder with launcher/ but no spartanport.png is not the checkout.
        let other = dir.path().join("other");
        std::fs::create_dir_all(other.join("launcher")).unwrap();
        std::fs::create_dir_all(other.join("x")).unwrap();
        assert_eq!(default_port_dir(&other.join("x")), other.join("x"));
    }

    #[test]
    fn the_port_folder_follows_spartan_port_dir() {
        // A cargo build runs from target/: the variable points it at the repository.
        // A relative folder is taken from the current one, once: the game and the Python
        // launcher start inside the port folder, where it would point somewhere else.
        let repo = std::env::current_dir().unwrap().join("repo");
        let paths = Paths::from_env(Path::new("exe"), &env_of(&[("SPARTAN_PORT_DIR", "repo")]));
        assert_eq!(paths.port_dir, repo);
        // Python `DATA_DIR = PORT_DIR` unless `SPARTAN_DATA_DIR` says otherwise.
        assert_eq!(paths.data_dir, repo);
        assert_eq!(paths.ini_path(), repo.join("spartanport.ini"));
        let both = Paths::from_env(
            Path::new("exe"),
            &env_of(&[("SPARTAN_PORT_DIR", "repo"), ("SPARTAN_DATA_DIR", "data")]),
        );
        assert_eq!(both.port_dir, repo);
        assert_eq!(both.data_dir, Path::new("data"));
        // An absolute folder stays as it is.
        let absolute = repo.join("port").display().to_string();
        let given = Paths::from_env(
            Path::new("exe"),
            &env_of(&[("SPARTAN_PORT_DIR", absolute.as_str())]),
        );
        assert_eq!(given.port_dir, repo.join("port"));
        // An empty value counts as unset.
        let empty = Paths::from_env(Path::new("exe"), &env_of(&[("SPARTAN_PORT_DIR", "")]));
        assert_eq!(empty.port_dir, Path::new("exe"));
    }

    #[test]
    fn data_dir_follows_spartan_data_dir() {
        let paths = Paths::from_env(Path::new("exe"), &env_of(&[("SPARTAN_DATA_DIR", "data")]));
        assert_eq!(paths.port_dir, Path::new("exe"));
        assert_eq!(paths.data_dir, Path::new("data"));
        assert_eq!(paths.mods_json(), Path::new("data").join("mods.json"));
        assert_eq!(paths.patches_json(), Path::new("data").join("patches.json"));
        assert_eq!(paths.user_dir(""), Path::new("data").join("user"));
        assert_eq!(paths.user_dir("saves"), Path::new("saves"));
        assert_eq!(paths.mods_dir(""), Path::new("data").join("mods"));
        assert_eq!(paths.patches_dir("p"), Path::new("p"));
        assert_eq!(
            paths.run_log_path(""),
            Path::new("data").join("user").join("last_run.log")
        );
    }

    #[test]
    fn ini_path_follows_spartan_config() {
        let default = Paths::from_env(Path::new("exe"), &env_of(&[("SPARTAN_DATA_DIR", "data")]));
        assert_eq!(
            default.ini_path(),
            Path::new("data").join("spartanport.ini")
        );
        let custom = Paths::from_env(Path::new("exe"), &env_of(&[("SPARTAN_CONFIG", "my.ini")]));
        assert_eq!(custom.ini_path(), Path::new("my.ini"));
    }

    #[test]
    fn find_tool_prefers_bin_over_out() {
        let dir = tempfile::tempdir().unwrap();
        let file = if cfg!(windows) { "tool.exe" } else { "tool" };
        let paths = Paths::from_env(dir.path(), &env_of(&[]));
        assert_eq!(paths.find_tool("tool"), None);
        std::fs::create_dir_all(dir.path().join("out")).unwrap();
        std::fs::write(dir.path().join("out").join(file), b"").unwrap();
        assert_eq!(
            paths.find_tool("tool"),
            Some(dir.path().join("out").join(file))
        );
        std::fs::create_dir_all(dir.path().join("bin")).unwrap();
        std::fs::write(dir.path().join("bin").join(file), b"").unwrap();
        assert_eq!(
            paths.find_tool("tool"),
            Some(dir.path().join("bin").join(file))
        );
    }

    #[cfg(windows)]
    #[test]
    fn legacy_config_file_is_appdata_gow3_launcher_on_windows() {
        let paths = Paths::from_env(Path::new("exe"), &env_of(&[("APPDATA", "roaming")]));
        assert_eq!(
            paths.legacy_config_file(),
            Path::new("roaming")
                .join("gow3-launcher")
                .join("settings.json")
        );
        let no_appdata = Paths::from_env(Path::new("exe"), &env_of(&[("USERPROFILE", "home")]));
        assert_eq!(
            no_appdata.legacy_config_file(),
            Path::new("home")
                .join("gow3-launcher")
                .join("settings.json")
        );
    }

    #[cfg(unix)]
    #[test]
    fn legacy_config_file_is_xdg_config_gow3_launcher_on_unix() {
        let paths = Paths::from_env(Path::new("exe"), &env_of(&[("XDG_CONFIG_HOME", "xdg")]));
        assert_eq!(
            paths.legacy_config_file(),
            Path::new("xdg").join("gow3-launcher").join("settings.json")
        );
        let fallback = Paths::from_env(
            Path::new("exe"),
            &env_of(&[("HOME", "/h"), ("XDG_CONFIG_HOME", "")]),
        );
        assert_eq!(
            fallback.legacy_config_file(),
            Path::new("/h")
                .join(".config")
                .join("gow3-launcher")
                .join("settings.json")
        );
    }

    #[test]
    fn fsr4_folder_is_next_to_the_port_when_writable() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::from_env(dir.path(), &env_of(&[("SPARTAN_DATA_DIR", "data")]));
        assert_eq!(paths.fsr4_folder(), dir.path().join("fsr4_shaders"));
    }

    #[cfg(unix)]
    #[test]
    fn fsr4_folder_falls_back_to_data_dir_when_port_is_read_only() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o500)).unwrap();
        let paths = Paths::from_env(dir.path(), &env_of(&[("SPARTAN_DATA_DIR", "data")]));
        let root_ignores_modes = tempfile::NamedTempFile::new_in(dir.path()).is_ok();
        let folder = paths.fsr4_folder();
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        if !root_ignores_modes {
            assert_eq!(folder, Path::new("data").join("fsr4_shaders"));
        }
    }
}
