//! The launcher updating itself: the zip on Windows, the AppImage on Linux, checked against the
//! checksum the release publishes.

use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use indexmap::IndexMap;
use serde_json::Value;
use sha2::{Digest, Sha256};
use thiserror::Error;

pub use crate::core::release::version_tuple;
use crate::core::release::{self, Channel, ReleaseId};

pub const RELEASES_API: &str =
    "https://api.github.com/repos/FelipePrado0/SpartanPort/releases?per_page=100";
pub const RELEASES_PAGE: &str = "https://github.com/FelipePrado0/SpartanPort/releases/latest";
const USER_AGENT: &str = "spartanport-launcher";
const CHUNK: usize = 1 << 20;

#[derive(Debug, Error)]
pub enum UpdateError {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error("{0}")]
    Http(String),
    #[error("{0}")]
    Json(String),
    #[error("{0}")]
    BadChecksum(String),
}

fn http_error(error: ureq::Error) -> UpdateError {
    UpdateError::Http(error.to_string())
}

/// A GET with the launcher's user agent. `global` bounds the whole request; without it only
/// connecting and the wait for the response are bounded (a download may take a while).
fn get(
    url: &str,
    accept: Option<&str>,
    global: Option<Duration>,
) -> Result<(Box<dyn Read>, Option<u64>), UpdateError> {
    let mut config = ureq::Agent::config_builder();
    config = match global {
        Some(limit) => config.timeout_global(Some(limit)),
        None => config
            .timeout_connect(Some(Duration::from_secs(60)))
            .timeout_recv_response(Some(Duration::from_secs(60))),
    };
    let agent: ureq::Agent = config.build().into();
    let mut request = agent.get(url).header("User-Agent", USER_AGENT);
    if let Some(accept) = accept {
        request = request.header("Accept", accept);
    }
    let response = request.call().map_err(http_error)?;
    let body = response.into_body();
    let total = body.content_length();
    Ok((Box::new(body.into_reader()), total))
}

/// One GET of the list (Accept `application/vnd.github+json`, global timeout 15 s), then `parse_releases`.
pub fn list_releases(windows: bool) -> Result<Vec<release::Release>, UpdateError> {
    let (mut reader, _) = get(
        RELEASES_API,
        Some("application/vnd.github+json"),
        Some(Duration::from_secs(15)),
    )?;
    let mut text = String::new();
    reader.read_to_string(&mut text)?;
    let json = serde_json::from_str(&text).map_err(|e| UpdateError::Json(e.to_string()))?;
    release::parse_releases(&json, windows)
}

/// `port_dir/release.json` on Windows; `appdir/release.json` (or `port_dir` without an APPDIR) elsewhere.
pub fn stamp_path(port_dir: &Path, appdir: Option<&Path>, windows: bool) -> PathBuf {
    let dir = if windows {
        port_dir
    } else {
        appdir.unwrap_or(port_dir)
    };
    dir.join(release::STAMP_FILE)
}

/// The stamp, or `ReleaseId::fallback()` when the file is missing or invalid.
pub fn installed_release(stamp: &Path) -> ReleaseId {
    read_stamp(stamp).unwrap_or_else(ReleaseId::fallback)
}

fn read_stamp(stamp: &Path) -> Option<ReleaseId> {
    let json: Value = serde_json::from_str(&fs::read_to_string(stamp).ok()?).ok()?;
    let channel = match json.get("channel")?.as_str()? {
        "release" => Channel::Release,
        "pre-release" => Channel::PreRelease,
        _ => return None,
    };
    let version = json.get("version")?.as_str().filter(|v| !v.is_empty())?;
    let build = json
        .get("build")
        .and_then(Value::as_str)
        .map(str::to_string);
    Some(ReleaseId {
        channel,
        version: version.to_string(),
        build,
    })
}

/// The SHA-256 in a `<package>.sha256` file (`sha256sum` format).
pub fn parse_checksum(text: &str) -> Result<String, UpdateError> {
    match text.split_whitespace().next() {
        Some(word) if word.len() == 64 && word.bytes().all(|b| b.is_ascii_hexdigit()) => {
            Ok(word.to_ascii_lowercase())
        }
        _ => Err(UpdateError::BadChecksum(
            "the release has no valid checksum".to_string(),
        )),
    }
}

/// The SHA-256 the release publishes beside a package.
pub fn release_checksum(url: &str) -> Result<String, UpdateError> {
    let (reader, _) = get(
        &format!("{url}.sha256"),
        None,
        Some(Duration::from_secs(30)),
    )?;
    let mut bytes = Vec::new();
    reader.take(4096).read_to_end(&mut bytes)?;
    parse_checksum(&String::from_utf8_lossy(&bytes))
}

fn partial_path(target: &Path) -> PathBuf {
    let mut name = target.file_name().unwrap_or_default().to_os_string();
    name.push(".update");
    target.with_file_name(name)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Streams `source` into `partial` in 1 MiB chunks, reporting the percent when it changes, and
/// checks the digest. The caller removes `partial` on failure.
fn fetch_into(
    source: &mut dyn Read,
    total: Option<u64>,
    partial: &Path,
    expected: Option<&str>,
    progress: &mut dyn FnMut(u8),
) -> Result<(), UpdateError> {
    if let Some(parent) = partial.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    let mut out = fs::File::create(partial)?;
    let mut digest = Sha256::new();
    let mut buffer = vec![0u8; CHUNK];
    let (mut done, mut shown) = (0u64, None);
    loop {
        let read = source.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        out.write_all(&buffer[..read])?;
        digest.update(&buffer[..read]);
        done += read as u64;
        let percent = match total {
            Some(total) if total > 0 => (done.saturating_mul(100) / total).min(100) as u8,
            _ => 0,
        };
        if shown != Some(percent) {
            shown = Some(percent);
            progress(percent);
        }
    }
    out.sync_all()?;
    drop(out);
    if let Some(expected) = expected {
        if hex(&digest.finalize()) != expected.to_ascii_lowercase() {
            return Err(UpdateError::BadChecksum(
                "the download does not match the published checksum".to_string(),
            ));
        }
    }
    Ok(())
}

/// Saves `source` as `target` through a temp file beside it: the target is replaced only by a
/// complete download that matches `expected`.
fn download_from(
    source: &mut dyn Read,
    total: Option<u64>,
    target: &Path,
    expected: Option<&str>,
    progress: &mut dyn FnMut(u8),
) -> Result<(), UpdateError> {
    let partial = partial_path(target);
    let result = fetch_into(source, total, &partial, expected, progress)
        .and_then(|()| fs::rename(&partial, target).map_err(UpdateError::from));
    if result.is_err() {
        let _ = fs::remove_file(&partial);
    }
    result
}

/// Downloads `url` to `target`, checking `expected_sha256` when given.
pub fn download(
    url: &str,
    target: &Path,
    expected_sha256: Option<&str>,
    progress: &mut dyn FnMut(u8),
) -> Result<(), UpdateError> {
    let (mut reader, total) = get(url, None, None)?;
    download_from(&mut *reader, total, target, expected_sha256, progress)
}

/// Windows installs replace the folder; elsewhere only an AppImage can replace itself.
pub fn can_self_update(env: &dyn Fn(&str) -> Option<String>, windows: bool) -> bool {
    windows || env("APPIMAGE").is_some_and(|v| !v.is_empty())
}

pub fn update_dir() -> PathBuf {
    std::env::temp_dir().join("spartanport-update")
}

/// Downloads the AppImage beside `target`, checks it against the release checksum and swaps it
/// in. The swap is a rename, so a running copy keeps its own file until it exits; nothing is
/// replaced when the download or the checksum fails.
pub fn replace_appimage(
    url: &str,
    target: &Path,
    progress: &mut dyn FnMut(u8),
) -> Result<(), UpdateError> {
    // Opened through a symlink, the file behind it is the one to replace.
    let target = fs::canonicalize(target).unwrap_or_else(|_| target.to_path_buf());
    let expected = release_checksum(url)?;
    let partial = partial_path(&target);
    let result = (|| {
        let (mut reader, total) = get(url, None, None)?;
        fetch_into(&mut *reader, total, &partial, Some(&expected), progress)?;
        make_executable(&partial)?;
        fs::rename(&partial, &target)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&partial);
    }
    result
}

#[cfg(unix)]
fn make_executable(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mode = fs::metadata(path)?.permissions().mode();
    fs::set_permissions(path, fs::Permissions::from_mode(mode | 0o755))
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> io::Result<()> {
    Ok(())
}

/// The environment for the new AppImage: the old mount's variables must not leak into it.
fn relaunch_environment(env: &IndexMap<String, String>) -> IndexMap<String, String> {
    let mut env = env.clone();
    let appdir = env.get("APPDIR").cloned().unwrap_or_default();
    for name in ["APPIMAGE", "APPDIR", "ARGV0", "OWD"] {
        env.shift_remove(name);
    }
    if let Some(paths) = env
        .get("LD_LIBRARY_PATH")
        .filter(|p| !appdir.is_empty() && !p.is_empty())
    {
        let kept: Vec<&str> = paths
            .split(':')
            .filter(|entry| !entry.is_empty() && !entry.starts_with(&appdir))
            .collect();
        if kept.is_empty() {
            env.shift_remove("LD_LIBRARY_PATH");
        } else {
            let joined = kept.join(":");
            env.insert("LD_LIBRARY_PATH".to_string(), joined);
        }
    }
    env
}

/// Starts the new AppImage apart from this one (its own process group on Unix).
pub fn relaunch_appimage(target: &Path, env: &IndexMap<String, String>) -> io::Result<()> {
    let mut command = Command::new(target);
    command
        .current_dir(target.parent().unwrap_or_else(|| Path::new(".")))
        .env_clear()
        .envs(relaunch_environment(env))
        .stdin(Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    command.spawn().map(drop)
}

/// Names the player owns: an update never copies them over, at any depth.
pub const USER_FILES: [&str; 10] = [
    "user",
    "out",
    "mods",
    "spartanport.ini",
    "settings.json",
    "game_icon.ico",
    "mods.json",
    "mods_presets.json",
    "patches.json",
    "last_run.log",
];

fn is_user_file(name: &std::ffi::OsStr, windows: bool) -> bool {
    let name = name.to_string_lossy();
    USER_FILES.iter().any(|user| {
        if windows {
            name.eq_ignore_ascii_case(user)
        } else {
            name == *user
        }
    })
}

fn copy_tree(source: &Path, target: &Path) -> io::Result<()> {
    fs::create_dir_all(target)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        if is_user_file(&entry.file_name(), cfg!(windows)) {
            continue;
        }
        let (from, to) = (entry.path(), target.join(entry.file_name()));
        if from.is_dir() {
            copy_tree(&from, &to)?;
        } else {
            fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

/// Copies the downloaded package over the installed one, keeping the
/// player's files and anything only the target has. A failed pass waits
/// through `pause` and starts over, up to `attempts` passes.
pub fn install_over(
    source: &Path,
    target: &Path,
    attempts: u32,
    pause: &mut dyn FnMut(),
) -> io::Result<()> {
    let mut pass = 1;
    loop {
        match copy_tree(source, target) {
            Ok(()) => return Ok(()),
            Err(error) if pass >= attempts => return Err(error),
            Err(_) => {
                pause();
                pass += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};
    use std::io::Cursor;

    const NEW_APPIMAGE: &[u8] = b"\x7fELF new appimage";

    #[test]
    fn versions_are_their_digit_runs() {
        assert_eq!(version_tuple("windows-v2.6.10"), vec![2, 6, 10]);
        assert!(version_tuple("none").is_empty());
    }

    #[test]
    fn a_garbled_checksum_file_is_not_installed() {
        for text in ["not a hash at all\n", "", "   \n", &"g".repeat(64)] {
            let error = parse_checksum(text).unwrap_err();
            assert_eq!(error.to_string(), "the release has no valid checksum");
        }
        let digest = "AB".repeat(32);
        let text = format!("{digest} *spartanport-x86_64.AppImage\n");
        assert_eq!(parse_checksum(&text).unwrap(), digest.to_lowercase());
    }

    fn sha(bytes: &[u8]) -> String {
        Sha256::digest(bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }

    fn target_in(dir: &Path) -> PathBuf {
        let target = dir.join("games").join("spartanport.AppImage");
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::write(&target, b"old appimage").unwrap();
        target
    }

    fn leftovers(target: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(target.parent().unwrap())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn a_matching_download_replaces_the_target() {
        let dir = tempfile::tempdir().unwrap();
        let target = target_in(dir.path());
        let mut source = Cursor::new(NEW_APPIMAGE.to_vec());
        let expected = sha(NEW_APPIMAGE);
        download_from(
            &mut source,
            Some(NEW_APPIMAGE.len() as u64),
            &target,
            Some(&expected),
            &mut |_| {},
        )
        .unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), NEW_APPIMAGE);
        assert_eq!(leftovers(&target), ["spartanport.AppImage"]);
    }

    #[test]
    fn progress_reaches_one_hundred_percent() {
        let dir = tempfile::tempdir().unwrap();
        let target = target_in(dir.path());
        let mut seen = Vec::new();
        let mut source = Cursor::new(NEW_APPIMAGE.to_vec());
        download_from(
            &mut source,
            Some(NEW_APPIMAGE.len() as u64),
            &target,
            None,
            &mut |p| seen.push(p),
        )
        .unwrap();
        assert_eq!(seen.last(), Some(&100));
    }

    #[test]
    fn progress_is_reported_only_when_it_changes() {
        let dir = tempfile::tempdir().unwrap();
        let target = target_in(dir.path());
        let data = vec![7u8; (3 << 20) + 5];
        let mut seen = Vec::new();
        download_from(
            &mut Cursor::new(data.clone()),
            Some(data.len() as u64),
            &target,
            None,
            &mut |p| seen.push(p),
        )
        .unwrap();
        assert_eq!(seen, vec![33, 66, 99, 100]);
        seen.clear();
        download_from(&mut Cursor::new(data), None, &target, None, &mut |p| {
            seen.push(p)
        })
        .unwrap();
        assert_eq!(seen, vec![0]);
    }

    #[test]
    fn a_download_that_does_not_match_the_checksum_leaves_the_target_alone() {
        let dir = tempfile::tempdir().unwrap();
        let target = target_in(dir.path());
        let mut source = Cursor::new(NEW_APPIMAGE.to_vec());
        let wrong = "0".repeat(64);
        let error = download_from(
            &mut source,
            Some(NEW_APPIMAGE.len() as u64),
            &target,
            Some(&wrong),
            &mut |_| {},
        )
        .unwrap_err();
        assert!(error.to_string().contains("checksum"));
        assert_eq!(std::fs::read(&target).unwrap(), b"old appimage");
        assert_eq!(leftovers(&target), ["spartanport.AppImage"]);
    }

    #[test]
    fn linux_updates_only_as_an_appimage() {
        let with = |k: &str| (k == "APPIMAGE").then(|| "/games/spartanport.AppImage".to_string());
        let without = |_: &str| None;
        let empty = |_: &str| Some(String::new());
        assert!(can_self_update(&with, false));
        assert!(!can_self_update(&without, false));
        assert!(!can_self_update(&empty, false));
    }

    #[test]
    fn windows_always_updates() {
        assert!(can_self_update(&|_| None, true));
    }

    #[test]
    fn the_update_folder_is_under_the_temp_folder() {
        assert_eq!(
            update_dir(),
            std::env::temp_dir().join("spartanport-update")
        );
    }

    fn map(pairs: &[(&str, &str)]) -> IndexMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn the_old_mount_does_not_leak_into_the_new_appimage() {
        let env = map(&[
            ("APPIMAGE", "/games/spartanport.AppImage"),
            ("APPDIR", "/tmp/.mount_spartanportabc"),
            ("ARGV0", "x"),
            ("OWD", "/home"),
            (
                "LD_LIBRARY_PATH",
                "/tmp/.mount_spartanportabc/usr/lib:/opt/lib",
            ),
            ("HOME", "/h"),
        ]);
        let out = relaunch_environment(&env);
        for name in ["APPIMAGE", "APPDIR", "ARGV0", "OWD"] {
            assert!(!out.contains_key(name));
        }
        assert_eq!(out["LD_LIBRARY_PATH"], "/opt/lib");
        assert_eq!(out["HOME"], "/h");
    }

    #[test]
    fn a_library_path_that_was_only_the_mount_is_dropped() {
        let env = map(&[
            ("APPDIR", "/tmp/.mount_spartanportabc"),
            ("LD_LIBRARY_PATH", "/tmp/.mount_spartanportabc/usr/lib"),
        ]);
        assert!(!relaunch_environment(&env).contains_key("LD_LIBRARY_PATH"));
    }

    #[test]
    fn a_library_path_is_kept_without_an_appdir() {
        let env = map(&[("LD_LIBRARY_PATH", "/opt/lib")]);
        assert_eq!(relaunch_environment(&env)["LD_LIBRARY_PATH"], "/opt/lib");
    }

    #[test]
    fn the_stamp_names_the_installed_release() {
        let dir = tempfile::tempdir().unwrap();
        let stamp = dir.path().join("release.json");
        std::fs::write(
            &stamp,
            r#"{"channel":"pre-release","version":"2.6.0","build":"issue209-test1"}"#,
        )
        .unwrap();
        let id = installed_release(&stamp);
        assert_eq!(id.channel, Channel::PreRelease);
        assert_eq!(id.version, "2.6.0");
        assert_eq!(id.build.as_deref(), Some("issue209-test1"));
        std::fs::write(&stamp, r#"{"channel":"release","version":"2.7.0"}"#).unwrap();
        let id = installed_release(&stamp);
        assert_eq!(id.channel, Channel::Release);
        assert_eq!(id.version, "2.7.0");
        assert_eq!(id.build, None);
    }

    #[test]
    fn a_missing_or_broken_stamp_is_this_version() {
        let dir = tempfile::tempdir().unwrap();
        let stamp = dir.path().join("release.json");
        assert_eq!(installed_release(&stamp), ReleaseId::fallback());
        for text in [
            "not json",
            r#"{"channel":"beta","version":"2.6.0"}"#,
            r#"{"channel":"release"}"#,
            r#"{"channel":"release","version":""}"#,
        ] {
            std::fs::write(&stamp, text).unwrap();
            assert_eq!(installed_release(&stamp), ReleaseId::fallback(), "{text}");
        }
    }

    #[test]
    fn the_stamp_sits_beside_the_launcher() {
        let port = Path::new("port");
        let appdir = Path::new("appdir");
        assert_eq!(stamp_path(port, None, true), port.join("release.json"));
        assert_eq!(
            stamp_path(port, Some(appdir), false),
            appdir.join("release.json")
        );
        assert_eq!(stamp_path(port, None, false), port.join("release.json"));
    }

    #[test]
    fn the_api_lists_one_page_of_releases() {
        assert!(RELEASES_API.ends_with("/releases?per_page=100"));
    }

    fn put(root: &Path, rel: &str, text: &str) {
        let path = root.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    fn read(root: &Path, rel: &str) -> String {
        fs::read_to_string(root.join(rel)).unwrap()
    }

    #[test]
    fn user_files_include_settings_and_icon() {
        assert_eq!(
            USER_FILES,
            [
                "user",
                "out",
                "mods",
                "spartanport.ini",
                "settings.json",
                "game_icon.ico",
                "mods.json",
                "mods_presets.json",
                "patches.json",
                "last_run.log",
            ]
        );
    }

    #[test]
    fn installing_skips_the_players_files_at_any_depth() {
        let dir = tempfile::tempdir().unwrap();
        let (source, target) = (dir.path().join("src"), dir.path().join("dst"));
        put(&source, "SpartanPort.exe", "new");
        put(&source, "bin/a.exe", "a");
        put(&source, "user/x", "x");
        put(&source, "docs/out/y", "y");
        put(&source, "settings.json", "theirs");
        put(&target, "SpartanPort.exe", "old");
        put(&target, "settings.json", "mine");
        put(&target, "user/save", "save");
        install_over(&source, &target, 1, &mut || {}).unwrap();
        assert_eq!(read(&target, "SpartanPort.exe"), "new");
        assert_eq!(read(&target, "bin/a.exe"), "a");
        assert_eq!(read(&target, "settings.json"), "mine");
        assert_eq!(read(&target, "user/save"), "save");
        assert!(!target.join("user/x").exists());
        assert!(!target.join("docs/out").exists());
    }

    #[test]
    fn names_match_without_case_on_windows() {
        assert!(is_user_file(std::ffi::OsStr::new("Settings.JSON"), true));
        assert!(!is_user_file(std::ffi::OsStr::new("Settings.JSON"), false));
    }

    #[test]
    fn files_only_in_the_target_stay() {
        let dir = tempfile::tempdir().unwrap();
        let (source, target) = (dir.path().join("src"), dir.path().join("dst"));
        put(&source, "SpartanPort.exe", "new");
        put(&target, "fsr4_411/a.bin", "keep");
        install_over(&source, &target, 1, &mut || {}).unwrap();
        assert_eq!(read(&target, "fsr4_411/a.bin"), "keep");
    }

    #[test]
    fn a_failed_copy_is_tried_again() {
        let dir = tempfile::tempdir().unwrap();
        let (source, target) = (dir.path().join("src"), dir.path().join("dst"));
        put(&source, "SpartanPort.exe", "new");
        fs::create_dir_all(target.join("SpartanPort.exe")).unwrap();
        let mut pauses = 0;
        let blocker = target.join("SpartanPort.exe");
        install_over(&source, &target, 3, &mut || {
            pauses += 1;
            fs::remove_dir(&blocker).unwrap();
        })
        .unwrap();
        assert_eq!(pauses, 1);
        assert_eq!(read(&target, "SpartanPort.exe"), "new");
    }

    #[test]
    fn copying_gives_up_after_the_last_attempt() {
        let dir = tempfile::tempdir().unwrap();
        let (source, target) = (dir.path().join("src"), dir.path().join("dst"));
        put(&source, "SpartanPort.exe", "new");
        fs::create_dir_all(target.join("SpartanPort.exe")).unwrap();
        let mut pauses = 0;
        let result = install_over(&source, &target, 3, &mut || pauses += 1);
        assert!(result.is_err());
        assert_eq!(pauses, 2);
    }

    #[test]
    fn a_folder_with_spaces_and_accents_installs() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("src");
        let target = dir.path().join("Jogos Ação").join("SpartanPort");
        put(&source, "bin/a.exe", "a");
        install_over(&source, &target, 1, &mut || {}).unwrap();
        assert_eq!(read(&target, "bin/a.exe"), "a");
    }
}
