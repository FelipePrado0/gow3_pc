# Rust Cutover Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship the Rust launcher in the Windows zip and the Linux AppImage with no Python on any product path. That means porting Add to Steam, `--install-update` and the voice tool, and dropping the Python bridge, PyInstaller and the Nix AppImage.

**Architecture:**
- The Pillow port `runner/draw.rs` moves to `core/draw.rs`, so `core/steam*.rs` can draw the Steam pictures without `core` importing `runner`.
- `core/update.rs` gains the copy-over installer.
- `core/psarc.rs` and `core/voice_swap.rs` port the voice tool.
- `startup.rs` routes `--install-update` and `--voice-swap` to them and loses `Route::Python`.
- The package scripts copy the cargo exe where PyInstaller's output went.

**Tech Stack:** Rust stable, `crc32fast`, `flate2`, `md-5` 0.10, `windows-sys` (+ `Win32_System_Registry`), `image`, `indexmap`; bash and PowerShell packaging scripts; GitHub Actions.

**Spec:** `docs/specs/2026-10-23-rust-cutover-design.md`. Python reference, frozen at `8db864e`: `scripts/steam_shortcut.py`, `tools/xvag_voice_swap.py`, and `install_update` / `USER_FILES` in `launcher/spartanport_launcher_win.py`. Before porting a function, read it and its Python tests (`tests/test_steam_shortcut.py`, `tests/test_xvag_voice_swap.py`). For house style, read `docs/plans/2026-10-22-rust-runner.md`.

## Global Constraints

- Crate `launcher-rust/`, package `spartanport-launcher`, version 2.7.0 until Task 13 sets 2.8.0. `src/core/**` and `src/runner/**` never contain the string `iced` (`tests/core_boundary.rs`).
- No `unwrap`/`expect` outside `#[cfg(test)]` and `tests/`. `unsafe` only in `src/core/launch.rs`, `src/core/platform/windows.rs` and `src/runner/guest_memory.rs`.
- `--install-update TARGET PID` keeps the Python's contract: the Python launcher 2.6/2.7 starts the first Rust package with it.
- `USER_FILES`, in this order: `user`, `out`, `mods`, `spartanport.ini`, `settings.json`, `game_icon.ico`, `mods.json`, `mods_presets.json`, `patches.json`, `last_run.log`.
- Frozen Python: no task edits `launcher/*.py`, `run.py`, `scripts/*.py`, `tools/xvag_voice_swap.py` or their tests. Task 12 deletes `launcher/spartanport_vulkan.py` and its test (spec §4.5). New Python files in `tests/` are allowed (the parity checker of Task 13).
- Texts: reuse the existing `assets/i18n.json` keys and add none. Lines printed to a terminal stay in English.
- Commits: a message from the `caveman-commit` skill, in this repo's form `Type(Scope): imperative summary` (≤ 50 characters). No AI attribution and no `Co-Authored-By`. The author is the git user.
- Never start the user's game, open a game window or write to the user's real Steam in tests. The `fake-game` binary stands in for any exe a test starts.
- Derived files are written with `core::fs::atomic_write` (`shortcuts.vdf` included).
- Pushing and dispatching workflows are outward actions: Task 13 asks the user first.
- Cargo builds load this PC heavily. `~/.cargo/config.toml` already caps `jobs = 8`; never raise it.

## Review Focus

- **A truncated `shortcuts.vdf` (Steam killed mid-write):** Add to Steam shows an error naming the file. It leaves that file's bytes and its old backup alone, and never panics. Test in Task 4.
- **Linked Steam roots on Linux:** `~/.steam/steam` is normally a symlink to `~/.local/share/Steam`. The same `userdata/<id>` must get one shortcut, not two. Test in Task 2.
- **A portable install (data folder = port folder) updated from 2.7:** `user/`, `out/`, `settings.json` and `spartanport.ini` beside `_internal/` survive the leftover cleanup. Test in Task 7.
- **An install folder with spaces and accents (`Jogos Ação/SpartanPort`):** the copy lands there and the relaunch starts there. Test in Task 5.
- **`--voice-swap` run from the AppImage with relative paths:** `AppRun` changes into the bundle, so relative paths resolve against `OWD`, the AppImage runtime's original folder. Test in Task 9.

---

### Task 1: Shared drawing in `core`

**Files:**
- Move: `src/runner/draw.rs` → `src/core/draw.rs` (`git mv`)
- Modify: `src/core/mod.rs` (`pub mod draw;`), `src/runner/mod.rs:5` (drop `pub mod draw;`), `src/runner/glyphs.rs:31` (`use crate::core::draw::{…}`), `tests/runner_fixtures.rs:15` (`use spartanport_launcher::core::draw; use spartanport_launcher::runner::{dxt, glyphs};`)

**Interfaces:**
- Produces `impl FontSize<'_> { pub fn length(&self, text: &str) -> f64 }`: Pillow's `ImageDraw.textlength`, the total advance of `layout(text)` (26.6) divided by 64.
- Produces `impl Picture { pub fn alpha_composite_at(&mut self, over: &Picture, x: i64, y: i64) }`: Pillow's `Image.alpha_composite(im, dest=(x, y))` for two RGBA pictures. The part of `over` inside `self` goes through the same `ImagingAlphaComposite` arithmetic as `alpha_composite`; what falls outside is cut.

- [ ] **Step 1: Move the file and fix the imports.** Run `cargo test --no-fail-fast` → the whole suite passes as before.
- [ ] **Step 2: Write the failing tests** in `src/core/draw.rs` `mod tests`. Font: `concat!(env!("CARGO_MANIFEST_DIR"), "/../launcher/Cinzel-Black.ttf")`.
  - `length_is_the_sum_of_the_advances`: at 40, `length("SP") == length("S") + length("P")`, `length("") == 0.0`, `length("S") > 0.0`.
  - `alpha_composite_at_cuts_what_falls_outside`: the base is 4×4 RGBA `(0,0,0,255)` and `over` is 2×2 `(255,0,0,255)`. At `(1,1)`, pixels x, y ∈ 1..=2 are red and the rest unchanged. At `(3,3)`, only `(3,3)` is red. At `(-1,-1)`, only `(0,0)` is red.
- [ ] **Step 3: Run** `cargo test --lib core::draw` → FAIL (no `length`, no `alpha_composite_at`).
- [ ] **Step 4: Implement** both methods.
- [ ] **Step 5: Run** `cargo test --lib core::draw` and then `cargo test --test runner_fixtures` → PASS.
- [ ] **Step 6: Commit** `Refactor(Launcher): move the drawing into core`

---

### Task 2: Steam folders and the binary VDF

**Files:**
- Create: `src/core/steam.rs`
- Modify: `src/core/mod.rs`, `src/core/platform/windows.rs` and `src/core/platform/unix.rs` (`steam_path_from_registry`), `Cargo.toml` (`crc32fast = "1"`; `windows-sys` feature `Win32_System_Registry`)

**Interfaces:**
- `pub enum Vdf { Map(IndexMap<String, Vdf>), Str(String), U32(u32) }`
- `pub fn parse_vdf(data: &[u8]) -> Result<IndexMap<String, Vdf>, String>`. Error texts: `unknown VDF type {kind} at {pos}` (`pos` is just past the type byte, as in the Python) and `truncated VDF at {pos}`.
- `pub fn dump_vdf(tree: &IndexMap<String, Vdf>) -> Vec<u8>`
- `pub fn shortcut_appid(exe: &str, name: &str) -> u32`: `crc32fast::hash((exe + name).as_bytes()) | 0x8000_0000`.
- `pub enum SteamError { Running, NoSteam, NoArt(PathBuf), Io(io::Error) }` with `From<io::Error>` and `Display` (`running`, `no_steam`, `no_art: <path>`, the io message).
- `pub fn steam_query(windows: bool) -> Vec<&'static str>`: `["tasklist", "/FI", "IMAGENAME eq steam.exe", "/NH"]` or `["pgrep", "-x", "steam"]`.
- `pub fn running_from(windows: bool, code: Option<i32>, stdout: &str) -> bool`. Windows: `stdout.to_lowercase()` contains `steam.exe`. Elsewhere: `code == Some(0)`.
- `pub fn steam_running() -> bool`: runs `steam_query(cfg!(windows))` with `platform::no_window`. A command that cannot start counts as not running.
- `pub fn steam_roots_from(windows: bool, registry: Option<PathBuf>, home: &Path) -> Vec<PathBuf>`. Windows: the registry folder first, then `C:/Program Files (x86)/Steam` and `C:/Program Files/Steam`. Elsewhere: `home` joined with `.local/share/Steam`, `.steam/steam` and `.var/app/com.valvesoftware.Steam/.local/share/Steam`.
- `pub fn steam_roots() -> Vec<PathBuf>`: `platform::steam_path_from_registry()` and `HOME`.
- `pub fn steam_path_from_registry() -> Option<PathBuf>` in `platform`: `RegGetValueW(HKEY_CURRENT_USER, "Software\\Valve\\Steam", "SteamPath", RRF_RT_REG_SZ)` on Windows, `None` elsewhere.
- `pub fn account_folders(roots: &[PathBuf]) -> Vec<PathBuf>`: for each root, the directories of `userdata/` whose name is all digits and not `0`, sorted by name, skipping a canonical path already seen.

- [ ] **Step 1: Write the failing tests** in `src/core/steam.rs` `mod tests`:
  - `round_trip_keeps_strings_numbers_and_nested_maps`: the tree is `shortcuts → "0" → {appid: U32(0x80000001), AppName: Str("A"), tags: Map{"0": Str("favorite")}, IsHidden: U32(0)}`. Assert `parse(dump(tree)) == tree` and `dump(parse(bytes)) == bytes`.
  - `empty_file_of_steam`: `b"\x00shortcuts\x00\x08\x08"` parses to `{shortcuts: Map{}}`, and the dump of that is the same bytes.
  - `broken_files_are_errors`: `parse_vdf(b"\x05x\x00") == Err("unknown VDF type 5 at 1")`, and `parse_vdf(b"\x00shortcuts\x00\x01AppName\x00Spar")` is an `Err` containing `truncated`.
  - `app_id_is_the_crc_of_exe_and_name_with_the_top_bit`: `shortcut_appid("\"/home/user/SpartanPort.AppImage\"", "SpartanPort") == 0xa02add32`.
  - `windows_looks_in_the_registry_folder_then_the_usual_ones`: the first root is `D:/Games/Steam` and the list contains `C:/Program Files (x86)/Steam`. The Linux list is the three above under `/home/u`.
  - `steam_is_running_by_the_process_list`: `running_from(true, Some(0), "steam.exe   1234 Console   1   150,000 K")` is true; `(true, Some(0), "INFO: No tasks are running which match the specified criteria.")` is false; `(false, Some(0), "")` is true; `(false, Some(1), "")` is false. `steam_query(false)[0] == "pgrep"`.
  - `accounts_are_the_numbered_folders_but_0`: under `userdata/`, folders `1111`, `2222`, `0` and `abc`, plus a file `3333`, give `[…/1111, …/2222]`. A missing root is skipped.
  - `#[cfg(unix)] a_linked_root_is_counted_once`: root B is a symlink to root A, and `account_folders(&[A, B])` lists each account once.
- [ ] **Step 2: Run** `cargo test --lib core::steam` → FAIL (module missing).
- [ ] **Step 3: Implement** the items above.
- [ ] **Step 4: Run** `cargo test --lib core::steam` → PASS. Run `cargo clippy --all-targets -- -D warnings` → clean.
- [ ] **Step 5: Commit** `Feat(Launcher): read and write Steam shortcuts`

---

### Task 3: The Steam pictures

**Files:**
- Create: `src/core/steam_art.rs`
- Modify: `src/core/mod.rs`

**Interfaces:**
- Consumes `core::draw` (`Picture`, `Mode`, `Filter::Lanczos`, `Draw::text`, `Font::open`, `Font::at`, `FontSize::length`, `Picture::gaussian_blur`, `Picture::alpha_composite_at`, `draw::composite`) and `SteamError` (Task 2).
- `pub const CAPSULE: (usize, usize) = (600, 900); pub const WIDE: (usize, usize) = (920, 430); pub const HERO: (usize, usize) = (1920, 620); pub const GOLD: [u8; 3] = [233, 214, 164];`
- `pub fn fonts(port_dir: &Path) -> Vec<PathBuf>`: the Python's `FONTS` in order, then `<port_dir>/launcher/Cinzel-Black.ttf`. That is 8 paths, the first `C:/Windows/Fonts/georgiab.ttf`.
- `pub fn open_font(candidates: &[PathBuf]) -> io::Result<Font>`: the first candidate that opens. With none: `NotFound`, `no font for the Steam pictures`.
- `pub fn cover(image: &Picture, width: usize, height: usize, focus: (f64, f64)) -> Picture`
- `pub fn usable_emblem(image: &Picture) -> bool`: true when the RGBA width is ≥ 200 and the alpha at `(1, 1)` is < 40.
- `pub fn make_logo(emblem: Option<&Picture>, width: usize, font: &Font) -> Picture`: RGBA.
- `pub fn darken_bottom(image: &Picture, strength: f64, start: f64) -> Picture`
- `pub fn make_art(game: &Path, icon: &Path, out_dir: &Path, app_id: u32, font: &Font) -> Result<PathBuf, SteamError>`: writes the five pictures and `<id>.json`, and returns `<out_dir>/<id>_icon.png`.
- Port each function line by line from `scripts/steam_shortcut.py`. Rounding uses half to even, as `draw.rs`'s `py_round`. `fit_face` and `spaced` use `length` per character. `darken_bottom` fills each row with the Python's gradient value instead of resizing a 1-pixel column. Pictures are loaded and saved with the `image` crate: RGB for the hero and the two capsules, RGBA for the logo.

- [ ] **Step 1: Write the failing tests** in `src/core/steam_art.rs` `mod tests`. Font: `open_font(&[<repo>/launcher/Cinzel-Black.ttf])`. Background: a 480 × 270 `pic1.png` of `(90, 20, 20)`; the Python's 1920 × 1080 only slows the debug build, and the output sizes are fixed.
  - `only_a_large_icon_with_a_transparent_background_is_an_emblem`: a transparent 256 × 256 is true, a transparent 128 × 128 is false, and an RGB 512 × 512 of `(200, 200, 200)` is false.
  - `the_logo_without_an_emblem_is_just_the_title_lines`: emblem 256 × 256 `(230, 180, 40, 255)`. `make_logo(Some, 600)` and `make_logo(None, 600)` have equal widths. The one without the emblem is shorter, and its `band(3).bbox()` is `Some` with a bottom edge > 0.
  - `make_art_writes_the_five_pictures`: returns `<out>/<id>_icon.png`. `<id>p.png` is 600 × 900, `<id>.png` is 920 × 430 and `<id>_hero.png` is 1920 × 620, all `Rgb8`. `<id>_logo.png` is `Rgba8` and 1000 wide. `<id>_icon.png` is 256 × 256. `<id>.json` holds exactly `{"nVersion": 1, "logoPosition": {"pinnedPosition": "BottomLeft", "nWidthPct": 42, "nHeightPct": 56}}`.
  - `pic0_stands_in_for_pic1`: with only `pic0.png`, the result is `Ok`.
  - `no_background_is_no_art`: with neither picture, `Err(SteamError::NoArt(<game>/sce_sys))`.
  - `a_missing_icon_falls_back_to_icon0`: with `icon` absent and `sce_sys/icon0.png` present, the result is `Ok`.
  - `fonts_end_with_the_ports_cinzel`: the list has 8 entries, the first is `C:/Windows/Fonts/georgiab.ttf` and the last is `<port>/launcher/Cinzel-Black.ttf`.
- [ ] **Step 2: Run** `cargo test --lib core::steam_art` → FAIL.
- [ ] **Step 3: Implement** the items above.
- [ ] **Step 4: Run** `cargo test --lib core::steam_art` → PASS.
- [ ] **Step 5: Commit** `Feat(Launcher): draw the Steam pictures`

---

### Task 4: `add_to_steam` and the Advanced page

**Files:**
- Modify: `src/core/steam.rs`, `src/ui/pages/advanced.rs:505-620` (the bridge), `tests/ui_pages.rs:2113-2190`, and `docs/specs/2026-10-23-rust-cutover-design.md` §3.3 ("os 18 campos" becomes "os 17 campos").

**Interfaces:**
- Consumes Task 2 (`parse_vdf`, `dump_vdf`, `shortcut_appid`, `account_folders`, `steam_roots`, `steam_running`, `SteamError`) and Task 3 (`make_art`, `open_font`, `fonts`).
- `pub struct Shortcut { pub exe: PathBuf, pub options: String, pub start_dir: PathBuf }`
- `pub struct Added { pub accounts: usize, pub updated: bool, pub app_id: u32 }`
- `pub fn add_to_steam(game: &Path, shortcut: &Shortcut, icon: &Path, fonts: &[PathBuf], steam_root: Option<&Path>, running: Option<bool>) -> Result<Added, SteamError>`. The order is the Python's: refuse `Running`; find the accounts or refuse `NoSteam`; open the font once; then, per account, `make_art`, parse, choose the slot, back up, `atomic_write`. A parse error becomes `SteamError::Io(InvalidData)` with the text `<file>: <parse error>`.
- The entry has 17 fields in this order: `appid`, `AppName`, `Exe`, `StartDir`, `icon`, `ShortcutPath`, `LaunchOptions`, `IsHidden`, `AllowDesktopConfig`, `AllowOverlay`, `OpenVR`, `Devkit`, `DevkitGameID`, `DevkitOverrideAppID`, `LastPlayTime`, `FlatpakAppID`, `tags`. The values are the Python's `shortcut_entry`.
- In `advanced.rs`:
  - `pub fn steam_shortcut(launcher: &Path) -> Shortcut`: `options` is `--play` and `start_dir` is `launcher.parent()`.
  - `pub fn steam_message(lang: &str, result: &Result<Added, SteamError>) -> Result<String, String>`: the three existing texts, or the error's text cut to 400 characters.
  - `add_to_steam(paths, settings, lang)` keeps its `eboot.bin` check and calls the core with `steam_shortcut(&launcher_path())`, `<port>/launcher/spartanport.png` and `steam_art::fonts(&paths.port_dir)`.
  - `steam_bridge_command`, `python_literal`, `accounts_of`, `steam_outcome` and the `python_command` import are deleted.

- [ ] **Step 1: Write the failing tests** in `src/core/steam.rs` `mod tests`, over a fixture like the Python `setUp`: a game with a 480 × 270 `pic1.png`, an opaque icon, accounts `1111`, `2222` and `0`, and `2222`'s `shortcuts.vdf` holding `"0": {appid: 5, AppName: "Other", Exe: "\"/x\"", tags: {"0": "keep"}}`. The shortcut is `("/home/user/spartanport.AppImage", "--play", "/home/user")`, and `running` is `Some(false)`.
  - `every_account_gets_the_shortcut_and_the_five_pictures`: `accounts == 2` and `!updated`. `1111`'s entry has `AppName` `SpartanPort`, `Exe` `"\"/home/user/spartanport.AppImage\""`, `LaunchOptions` `--play`, `StartDir` `"\"/home/user\""` and `appid == app_id`. `icon` is the `<grid>/<id>_icon.png` string, and the keys follow the 17-field order.
  - `other_shortcuts_stay_and_a_backup_is_made`: in `2222`, `"0"` is still `Other` with tags `{"0": "keep"}`, `"1"` is `SpartanPort`, and `shortcuts.vdf.spartanport-backup` exists.
  - `adding_again_updates_the_same_shortcut`: the second run returns `updated`, and `2222` has exactly one `SpartanPort`.
  - `a_shortcut_from_before_the_rename_is_updated_not_doubled`: when `1111` holds `{appid: 7, AppName: "God of War III", Exe: "\"/old\""}`, the names afterwards are `["SpartanPort"]`.
  - `steam_open_and_steam_missing_are_refused`: `Some(true)` gives `Running`. A root `<tmp>/none` gives `NoSteam`. `1111/config/shortcuts.vdf` is never created.
  - `a_game_folder_without_the_background_is_refused`: `NoArt`.
  - `a_corrupt_shortcuts_file_is_left_as_it_is` (Review Focus): `2222`'s file is `b"\x00shortcuts\x00\x01AppName\x00Spar"` and an old backup holds `b"old"`. The result is `Err(Io)`, its text contains the file's path, the file's bytes are unchanged, and the backup still holds `b"old"`.
- [ ] **Step 2: Write the failing UI tests** in `tests/ui_pages.rs`, replacing `steam_bridge_command_matches_the_spec`, `python_literals_quote_every_path` and `steam_answers_become_the_python_texts`:
  - `steam_shortcut_starts_in_the_programs_folder`: `/home/u/Apps/spartanport.AppImage` gives `start_dir` `/home/u/Apps` and `options` `--play`.
  - `steam_results_become_the_texts`: `Added{accounts: 2, updated: false}` gives "The shortcut "SpartanPort" was added to Steam (accounts: 2), with its icon, capsules, background and logo. Open Steam to see it in the library." With `updated: true`, the same text with "was updated in Steam". `Running`, `NoSteam` and `NoArt` give the three error texts of today's `steam_outcome`. An `Io` error of 500 `x` gives 400 characters.
- [ ] **Step 3: Run** `cargo test --lib core::steam` and `cargo test --test ui_pages steam` → FAIL.
- [ ] **Step 4: Implement** the core function and the page changes, and fix the spec line.
- [ ] **Step 5: Run** `cargo test --lib core::steam` and `cargo test --test ui_pages` → PASS. Run `cargo clippy --all-targets -- -D warnings` → clean.
- [ ] **Step 6: Commit** `Feat(Launcher): add the game to Steam natively`

---

### Task 5: The copy-over installer

**Files:**
- Modify: `src/core/update.rs` (new items and `mod tests` cases)

**Interfaces:**
- `pub const USER_FILES: [&str; 10]` (Global Constraints order).
- `fn is_user_file(name: &OsStr, windows: bool) -> bool`: ASCII case is ignored when `windows` (`fnmatch` normcase).
- `pub fn install_over(source: &Path, target: &Path, attempts: u32, pause: &mut dyn FnMut()) -> io::Result<()>`: the `copytree(…, dirs_exist_ok=True, ignore=ignore_patterns(*USER_FILES))` of spec §3.4. A user file is skipped at every level. Folders are created and files overwritten with `fs::copy`; nothing in `target` is deleted. A failed pass calls `pause` and starts over, up to `attempts` passes, and the last error is returned.

- [ ] **Step 1: Write the failing tests** in `src/core/update.rs` `mod tests`:
  - `user_files_include_settings_and_icon`: equals the ten names in order.
  - `installing_skips_the_players_files_at_any_depth`: the source has `SpartanPort.exe` (`new`), `bin/a.exe`, `user/x`, `docs/out/y` and `settings.json` (`theirs`). The target has `SpartanPort.exe` (`old`), `settings.json` (`mine`) and `user/save`. Afterwards the target has `SpartanPort.exe` == `new`, `bin/a.exe`, `settings.json` == `mine` and `user/save`, and no `user/x` and no `docs/out`.
  - `names_match_without_case_on_windows`: `is_user_file("Settings.JSON", true)` is true and `is_user_file("Settings.JSON", false)` is false.
  - `files_only_in_the_target_stay`: `fsr4_411/a.bin` stays.
  - `a_failed_copy_is_tried_again`: the target has a directory `SpartanPort.exe/`, which blocks the file. With `attempts` 3, the `pause` closure removes that directory and counts. Result: `Ok`, one pause, and the file copied.
  - `copying_gives_up_after_the_last_attempt`: the blocker stays. Result: `Err`, two pauses.
  - `a_folder_with_spaces_and_accents_installs` (Review Focus): the target is `<tmp>/Jogos Ação/SpartanPort` and gets the files.
- [ ] **Step 2: Run** `cargo test --lib core::update` → FAIL.
- [ ] **Step 3: Implement** the items above.
- [ ] **Step 4: Run** `cargo test --lib core::update` → PASS.
- [ ] **Step 5: Commit** `Feat(Launcher): install updates over the old copy`

---

### Task 6: `--install-update` in Rust, and no more Python bridge

**Files:**
- Modify: `src/startup.rs`, `src/main.rs`, `src/core/launch.rs:33-50` (delete `python_command` and its tests), `src/core/platform/unix.rs` (`pub fn show_error(title: &str, text: &str)` printing `{title}: {text}` to stderr), `tests/dump.rs:13,176,300` (a private `python_command` helper moves into this test file, unchanged), `tests/startup.rs`

**Interfaces:**
- Consumes `update::install_over`, `update::RELEASES_PAGE`, `desktop::open_url`, `platform::wait_for_process`, `platform::show_error`, `i18n::tr`.
- `pub enum Route { Version, Run, Play, InstallUpdate, Launcher }`: `Python` is gone.
- `pub fn route_on<S: AsRef<OsStr>>(args: &[S], windows: bool) -> Route`. A first `--run` gives `Run`. A first `--install-update` with `windows` and exactly 3 arguments gives `InstallUpdate`. Otherwise `--version` anywhere gives `Version`, `--play` anywhere gives `Play`, and anything else gives `Launcher`. `--script` is an ordinary argument.
- `pub fn route<S: AsRef<OsStr>>(args: &[S]) -> Route`: `route_on(args, cfg!(windows))`.
- `pub fn migrates<S: AsRef<OsStr>>(args: &[S]) -> bool`: false when the first argument is `--install-update`.
- `pub fn prepare(paths: &Paths, migrate: bool)`: `migrate == false` skips `move_from_legacy_name`. `main` passes `startup::migrates(&args)` on the `Play` and `Launcher` routes.
- `pub fn install_update(source: &Path, target: &Path, pid: u32, lang: &str) -> i32`:
  - Waits for `pid` (60 s), then runs `install_over(source, target, 30, sleep 1 s)`.
  - On success, starts `target/SpartanPort.exe` from `target` with stdin null and returns 0. If that start fails, `show_error(APP_NAME, <error>)` and 1.
  - On failure, `show_error(APP_NAME, tr(lang, "Could not install the update. Download it from the releases page."))`, then `open_url(RELEASES_PAGE)`, and 1.
- `main`, `Route::InstallUpdate`:
  - `source` is the exe's folder and `target` is `args[1]`. `pid` is `args[2]` parsed as `u32`; if it does not parse, use 0, which waits for nothing.
  - `lang` is `launcher_language(&load_app_settings(&paths).get_str("ui_language"), system_language)`.
  - The route does not call `prepare`.
- Deleted: `Route::Python`, `forward_to_python` and `python_command`.

- [ ] **Step 1: Write the failing tests** in `tests/startup.rs`:
  - Rename `arguments_route_to_python` to `arguments_route_to_their_roles`.
    - `route_on(&["--install-update", "C:\\Port", "1234"], true) == InstallUpdate`, but `Launcher` with `windows` false.
    - `route_on(&["--install-update", "a.zip"], true) == Launcher`, and the same with four arguments.
    - `route(&["--script", "prepare.py"]) == Launcher` and `route(&["--script", "x.py", "--version"]) == Version`.
  - In `run_and_play_are_their_own_routes`, drop the `Route::Python` lines.
  - `the_update_installer_does_not_migrate`: `!migrates(&["--install-update", "C:\\P", "1"])`, `migrates(&["--play"])` and `migrates::<&str>(&[])`.
  - `#[cfg(windows)] install_update_copies_and_starts_the_new_launcher`:
    - `source` holds `SpartanPort.exe`, a copy of `env!("CARGO_BIN_EXE_fake-game")`, plus `bin/x`.
    - `target` holds `settings.json` (`mine`).
    - `pid` comes from a `fake-game` child that has already exited.
    - Assert: `install_update(source, target, pid, "en") == 0`, `target/bin/x` exists, and `settings.json` is still `mine`.
    - Never test the failure path: `show_error` blocks on a dialog.
- [ ] **Step 2: Run** `cargo test --test startup` → FAIL.
- [ ] **Step 3: Implement** the items above.
- [ ] **Step 4: Run** `cargo test --test startup` and `cargo test --no-run --test dump` → PASS. `rg -n "python_command|forward_to_python|Route::Python" src` → no match.
- [ ] **Step 5: Commit** `Feat(Launcher): install updates without Python`

---

### Task 7: A port folder without `run.py`, and the Python leftovers

**Files:**
- Modify: `src/core/paths.rs:28-38` and its test `a_cargo_build_runs_from_its_checkout`, `src/ui/play.rs:304-318`, `tests/ui_play.rs:512-565` (`play_refuses_an_incomplete_port`), `src/core/legacy.rs`, `src/startup.rs` (`prepare`)

**Interfaces:**
- `default_port_dir`: a folder is a port when it holds `launcher/spartanport.png`. The ancestor search for a cargo build stays.
- `missing_port_files`: only the probe, `bin/spartanport-probe[.exe]`.
- `pub fn remove_python_leftovers(port_dir: &Path)` in `legacy.rs`:
  - Acts only when `<port>/_internal` is a directory.
  - Removes `_internal/`, `run.py`, every `scripts/*.py`, `scripts/__pycache__/` and `tools/xvag_voice_swap.py`, then `tools/` if it is left empty.
  - Errors are ignored.
- `prepare`: in the `installed` block, call it after `legacy::remove_leftovers`.

- [ ] **Step 1: Write the failing tests:**
  - `a_cargo_build_runs_from_its_checkout`: rewritten with `launcher/spartanport.png` as the marker, with the same five cases. The last case becomes "a folder with `launcher/` but no `spartanport.png` is not the checkout".
  - `play_refuses_an_incomplete_port`: `missing_port_files == vec![probe_file()]`, and the dialog lists only the probe. With the probe present, nothing is missing.
  - `legacy.rs` `python_leftovers_go_once_the_rust_launcher_is_installed` (Review Focus):
    - The port has `_internal/base_library.zip`, `run.py`, `scripts/glyphs.py`, `scripts/__pycache__/glyphs.pyc`, `scripts/controller_diagrams/xbox.png`, `tools/xvag_voice_swap.py`, `SpartanPort.exe`, `user/save`, `out/x`, `settings.json` and `spartanport.ini`.
    - After the call, `_internal/`, `run.py`, `scripts/glyphs.py`, `scripts/__pycache__/` and `tools/` are gone.
    - Everything else stays.
  - `a_checkout_keeps_its_python`: without `_internal/`, all those files stay.
  - `tools_with_other_files_stays`: with `tools/mine.txt` present, only `tools/xvag_voice_swap.py` goes.
- [ ] **Step 2: Run** `cargo test --lib core::paths core::legacy` and `cargo test --test ui_play play_refuses` → FAIL.
- [ ] **Step 3: Implement** the items above.
- [ ] **Step 4: Run** the same commands → PASS.
- [ ] **Step 5: Commit** `Feat(Launcher): drop run.py and clean its leftovers`

---

### Task 8: The PSARC reader

**Files:**
- Create: `src/core/psarc.rs`
- Modify: `src/core/mod.rs`, `Cargo.toml` (`flate2 = "1"`, `md-5 = "0.10"`)

**Interfaces:**
- `pub struct Psarc` (private fields).
- `pub fn open(path: &Path) -> io::Result<Psarc>`
- `pub fn names(&self) -> &IndexMap<String, usize>`: the upper-case base name maps to the entry index, as the Python `names`.
- `pub fn read(&mut self, index: usize) -> io::Result<Vec<u8>>`
- Error texts, as `InvalidData`: `{path}: not a PSARC archive` and `PSARC compression '{name}' is not supported (zlib only)`. Zlib errors carry the `flate2` message.
- `#[cfg(test)] pub(crate) fn build(files: &[(&str, &[u8])], shuffle: bool, block_size: u32) -> Vec<u8>`: the Python test's `psarc()` builder, extended to split an entry into `block_size` blocks. A stored full block is written as 0 in the 2-byte table, as Sony does. Task 9 reuses it.

- [ ] **Step 1: Write the failing tests** in `src/core/psarc.rs` `mod tests`:
  - `names_are_found_by_their_md5_whatever_the_order`: `/SOUNDS/STREAMS/A.XVAG`, `/SOUNDS/STREAMS/B.XVAG` and `/SOUNDS/STREAMS/CINEMATICS/Z.XVAG`, shuffled, give the names `A.XVAG`, `B.XVAG` and `Z.XVAG`. Each `read` returns its bytes.
  - `stored_blocks_are_read_as_they_are`: incompressible bytes.
  - `a_file_spans_blocks_and_a_full_stored_block_is_zero`: 150 000 random-looking bytes at block size 65 536 read back whole.
  - `not_a_psarc_is_refused` and `other_compression_is_refused`: a header with `lzma` gives `PSARC compression 'lzma' is not supported (zlib only)`.
- [ ] **Step 2: Run** `cargo test --lib core::psarc` → FAIL.
- [ ] **Step 3: Implement** a port of the Python `Psarc` class.
- [ ] **Step 4: Run** `cargo test --lib core::psarc` → PASS.
- [ ] **Step 5: Commit** `Feat(Launcher): read PSARC archives`

---

### Task 9: `--voice-swap`

**Files:**
- Create: `src/core/voice_swap.rs`
- Modify: `src/core/mod.rs`, `src/startup.rs` (`Route::VoiceSwap`), `src/main.rs`, `tests/startup.rs`, `docs/VOICE_REPLACEMENT.md` (EN and PT-BR)

**Interfaces:**
- `pub struct XvagHeader { pub big_endian: bool, pub start: usize, pub channels: u32, pub codec: u32, pub samples: u32, pub rate: u32 }`
- `pub fn read_xvag_header(data: &[u8]) -> Result<XvagHeader, String>`
- `pub fn replacement_frames(data: &[u8], rate: u32) -> Result<Vec<[u8; 16]>, String>`
- `pub fn swap_voice(original: &[u8], recording: &[u8]) -> Result<Vec<u8>, String>`: the Python's error texts, verbatim.
- `pub struct Options { pub language: String, pub only: Option<PathBuf>, pub dry_run: bool, pub reference: Option<PathBuf> }`, with `Default` language `PORTUGUESE`.
- `pub struct Report { pub written: Vec<String>, pub same: Vec<String>, pub refused: IndexMap<String, Vec<String>> }`
- `pub fn run(game: &Path, source: &Path, update: &Path, options: &Options) -> io::Result<Report>`: the Python `run`. `ValueError` texts become `InvalidInput` errors with the same messages. `is_inside` compares the canonical form of the deepest existing ancestor, plus the rest.
- `pub fn main(args: &[OsString], base: Option<&Path>, out: &mut dyn Write, err: &mut dyn Write) -> i32`:
  - A relative path argument is joined to `base` when `base` is given.
  - `-h`/`--help` prints the usage and the help to `out` and returns 0. The usage line is `usage: SpartanPort --voice-swap GAME_DIR SOURCE UPDATE_DIR [--language LANGUAGE] [--only NAMES.txt] [--reference ORIGINAL] [--dry-run]`, followed by the Python argparse help strings.
  - An argument error prints the usage plus `voice-swap: error: <what>` to `err` and returns 2.
  - A run error prints `voice-swap: <error>` to `err` and returns 1.
  - Otherwise it prints the Python's four report lines to `out`, with `UPDATE_DIR` as typed, and returns 0.
- `route_on`: a first `--voice-swap` gives `VoiceSwap`. `main` runs `voice_swap::main(&args[1..], std::env::var_os("OWD").as_deref().map(Path::new), stdout, stderr)` and exits with the result.

- [ ] **Step 1: Write the failing tests** in `src/core/voice_swap.rs` `mod tests`. Port the Python builders `frame`, `xvag`, `vagp` and `ORIGINAL`/`DUBBED` as test functions, and use `psarc::build` (Task 8).
  - The 6 `SwapVoiceTests`, with the Python names: `only_the_frames_change`, `padding_after_the_frames_is_kept`, `an_xvag_recording_works_like_a_vagp_one`, `truncated_xvag_recordings_are_refused`, `xvag_payload_cannot_overlap_the_header` and `what_does_not_fit_is_refused_with_a_reason` (the five reasons verbatim).
  - The 7 `RunTests`, over the same `setUp` tree: `writes_the_fitting_lines_to_the_update_folder_and_leaves_the_game_alone`, `dry_run_writes_nothing`, `reference_leaves_unchanged_recordings_alone`, `only_limits_the_lines`, `the_update_folder_cannot_be_the_game_or_inside_it`, `a_psarc_source_is_read_by_name_whatever_the_entry_order` and `cli_reports_and_returns_zero`. The last one checks `main([game, source, update, "--dry-run"]) == 0` with `out` containing `1 files would be written to`, and `main([game, game, game/x]) == 1` with `err` starting with `voice-swap: `.
  - `relative_paths_follow_the_appimages_folder` (Review Focus): with `base = Some(<tmp>)` and the relative arguments `game source update`, the result is 0 and `<tmp>/update/PORTUGUESE/SOUNDS/STREAMS_PS4/A.XVAG` exists.
  - `bad_arguments_print_the_usage`: `main(["game"]) == 2`, and `err` contains `usage: SpartanPort --voice-swap`.
- [ ] **Step 2: Write the failing route test** in `tests/startup.rs`: `voice_swap_is_its_own_role` checks `route(&["--voice-swap", "--help"]) == VoiceSwap` and `route(&["--voice-swap", "a", "--play"]) == VoiceSwap`.
- [ ] **Step 3: Run** `cargo test --lib core::voice_swap` and `cargo test --test startup voice` → FAIL.
- [ ] **Step 4: Implement** the items above, and update `docs/VOICE_REPLACEMENT.md`. The command becomes `SpartanPort.exe --voice-swap GAME_DIR SOURCE UPDATE_DIR --language PORTUGUESE [--reference ORIGINAL] [--dry-run]`, and `spartanport-x86_64.AppImage --voice-swap …` on Linux. Mentions of `tools/xvag_voice_swap.py` name the subcommand instead, in both languages.
- [ ] **Step 5: Run** the same commands → PASS. Then run `cargo build` and `target/debug/spartanport-launcher --voice-swap --help` → the usage printed, exit 0.
- [ ] **Step 6: Commit** `Feat(Launcher): swap voice lines natively`

---

### Task 10: The Windows package

**Files:**
- Modify: `packaging/windows/package.sh`, `packaging/windows/smoke-test.ps1`, `.github/workflows/windows-zip.yml`, `packaging/windows/README.md`, `packaging/windows/README-Windows.txt`
- Create: `launcher-rust/tests/packaging.rs`

**Interfaces:**
- `package.sh` requires `launcher-rust/target/release/spartanport-launcher.exe`. Without it, it stops with `Build the launcher first, outside MSYS2: cargo build --release --locked --manifest-path launcher-rust/Cargo.toml`.
- The script copies that exe to `SpartanPort.exe`, then copies `SpartanPort.exe` and `Play SpartanPort.exe` to `God of War III.exe` and `Play God of War III.exe`. The legacy names are written in the script, with a comment naming `core::legacy::LEGACY_EXE`/`LEGACY_PLAY_EXE`.
- Kept: `scripts/controller_diagrams/` (alone under `scripts/`), `patches/`, the `launcher/` files and `docs/VOICE_REPLACEMENT.md`.
- Deleted: the Python detection, the venv, pip, the hidden imports, PyInstaller, `tools/`, `run.py`, `scripts/*.py` and the `discord_presence.py` check.
- The forbidden list gains `-o -iname '*.py' -o -iname _internal`.
- `smoke-test.ps1`:
  - The `--script` block is replaced by `SpartanPort.exe --version`: stdout redirected to a file, within 30 s, exit 0, output matching `^SpartanPort \d+\.\d+\.\d+`.
  - `SpartanPort.exe --voice-swap --help` must exit 0.
  - Any `*.py` in the tree is a failure.
- `windows-zip.yml`:
  - Insert `- uses: dtolnay/rust-toolchain@stable` and a `shell: pwsh` step `cargo build --release --locked --manifest-path launcher-rust/Cargo.toml` after `Build`.
  - The `Package` step drops `PYTHON_EXE`/`WINPYTHON`.
  - `setup-python` stays, but only for `Stamp`.
  - `stamp` gets `default: true`, and its description says the package ships the Rust launcher.
  - The header comment drops Python.
- READMEs: the steps become rustup with VS Build Tools, then `cargo build --release --locked` in PowerShell, then `bash packaging/windows/package.sh` in CLANG64. `README-Windows.txt` says no Python is needed.

- [ ] **Step 1: Write the failing test** `tests/packaging.rs::the_windows_package_ships_the_rust_launcher`. It reads `../packaging/windows/package.sh` and asserts:
  - It contains `launcher-rust/target/release/spartanport-launcher.exe`, `legacy::LEGACY_EXE` and `legacy::LEGACY_PLAY_EXE` (the values, through `spartanport_launcher::core::legacy`), `scripts/controller_diagrams`, `-iname '*.py'` and `-iname _internal`.
  - It contains none of `PyInstaller`, `pyenv`, `run.py` and `cp -r scripts`.
  - `smoke-test.ps1` contains `--version` and `--voice-swap` and not `--script`.
  - `windows-zip.yml` contains `dtolnay/rust-toolchain@stable` and `cargo build --release --locked`.
- [ ] **Step 2: Run** `cargo test --test packaging` → FAIL.
- [ ] **Step 3: Implement** the script, smoke test, workflow and README changes.
- [ ] **Step 4: Run** the checks:
  - `cargo test --test packaging` → PASS.
  - `bash -n packaging/windows/package.sh` → no output.
  - `powershell -NoProfile -Command "$null = [scriptblock]::Create((Get-Content -Raw packaging/windows/smoke-test.ps1)); 'ok'"` → `ok`.
  - The real package run is Task 13's CI dispatch.
- [ ] **Step 5: Commit** `Build(Packaging): ship the Rust launcher on Windows`

---

### Task 11: The AppImage

**Files:**
- Modify: `packaging/linux/build.sh`, `packaging/linux/AppRun` (comment only), `packaging/linux/smoke-test.sh`, `packaging/linux/ci-deps.sh`, `.github/workflows/linux-appimage.yml`, `.github/workflows/rust-launcher.yml`, `launcher-rust/tests/packaging.rs`

**Interfaces:**
- `build.sh`:
  - Runs `"${CARGO:-cargo}" build --release --locked --manifest-path launcher-rust/Cargo.toml` and installs `launcher-rust/target/release/spartanport-launcher` (755) in `usr/share/spartanport/`.
  - Copies `patches/` and `scripts/controller_diagrams/`, plus the existing `launcher/` files.
  - Runs `bundle` over the launcher too, and the glibc check covers it unchanged.
  - Stops when the AppDir holds `*.py` or `_internal`.
  - Deleted: the PyInstaller block, the `__pycache__` sweep, `run.py` and `scripts/*.py`.
- `smoke-test.sh`:
  - Before the window, `squashfs-root/AppRun --version` must print a line matching `^SpartanPort [0-9]+\.[0-9]+\.[0-9]+$`.
  - The failure pattern becomes `panicked|GLIBC_|error while loading shared libraries`.
- `ci-deps.sh`: drops `python3-venv python3-tk python3-dev` and the venv block. `python3` stays for `release_stamp.py`.
- `linux-appimage.yml`: `dtolnay/rust-toolchain@stable` before `AppImage`, the step without `PATH=…venv`, and `stamp` `default: true`.
- `rust-launcher.yml`: `libgtk-3-dev` leaves the apt line. `rfd` 0.17 defaults to `xdg-portal` with `zenity` as the fallback. If Task 13's CI run fails on a missing `gtk`/`gdk`/`glib`, put it back there and add it to `ci-deps.sh`.

- [ ] **Step 1: Write the failing test** `tests/packaging.rs::the_appimage_ships_the_rust_launcher`:
  - `build.sh` contains `launcher-rust/target/release/spartanport-launcher` and `scripts/controller_diagrams`, and contains neither `PyInstaller` nor `run.py`.
  - `smoke-test.sh` contains `--version` and `panicked`.
  - `linux-appimage.yml` contains `dtolnay/rust-toolchain@stable`.
- [ ] **Step 2: Run** `cargo test --test packaging` → FAIL.
- [ ] **Step 3: Implement** the script, workflow and dependency changes.
- [ ] **Step 4: Run** `cargo test --test packaging` → PASS. Then `bash -n` on `build.sh`, `smoke-test.sh`, `ci-deps.sh` and `AppRun` → no output.
- [ ] **Step 5: Commit** `Build(Packaging): ship the Rust launcher in the AppImage`

---

### Task 12: The developer entry, the Nix removal and the READMEs

**Files:**
- Modify: `run.sh`, `README.md`, `.gitignore:13` (the `packaging/runtime-paths.nix` line)
- Delete: `packaging/appimage.sh`, `packaging/default.nix`, `packaging/spartanport-entry.c` (only `default.nix` builds it), `launcher/spartanport_vulkan.py`, `tests/test_packaged_vulkan.py`

**Interfaces:**
- `run.sh`:
  - Keeps `--software`, the nix-shell re-entry, the FSR 4 step and `build.sh`.
  - Builds the launcher when `launcher-rust/target/release/spartanport-launcher` is missing.
  - Ends with `export SPARTAN_DATA_DIR=$data` and `exec launcher-rust/target/release/spartanport-launcher --run "$@"`.
  - Deleted: the Python lookup and the `SPARTAN_PREBUILT` branches.
- `README.md`:
  - The Windows and Linux "from source" sections use `cargo run --release --manifest-path launcher-rust/Cargo.toml` (window) and `bash run.sh` (direct start).
  - The Rust launcher section drops the `SPARTANPORT_PYTHON` sentence and "the Python launcher stays until…".
  - The Python section says the launcher and runner Python code is frozen: not packaged, not run, kept as the parity reference. It keeps the remaining test commands.
  - Line 296 says `run.sh` instead of `run.py`.

- [ ] **Step 1: Run** `rg -n "appimage\.sh|default\.nix|spartanport_vulkan|spartanport-entry|SPARTAN_PREBUILT|SPARTANPORT_PYTHON" --glob '!docs/specs/**' --glob '!docs/plans/**' --glob '!dist/**' --glob '!.claude/**'` → note every hit.
- [ ] **Step 2: Make the edits and deletions** until the same command prints nothing.
- [ ] **Step 3: Run** `bash -n run.sh` → no output. Then `rg -n "python|PYTHON" run.sh` → no match, and `tail -1 run.sh` → the `exec launcher-rust/target/release/spartanport-launcher --run "$@"` line. Do not execute `run.sh`: it runs `build.sh`, the full native build.
- [ ] **Step 4: Commit** `Chore(Packaging): drop the Nix AppImage and Python`

---

### Task 13: The parity map, version 2.8.0 and the release gate

**Files:**
- Create: `docs/specs/2026-10-23-rust-cutover-parity.md`, `tests/test_cutover_parity_doc.py`
- Modify: `docs/specs/2026-10-21-rust-launcher-parity.md` (row 125, row 310, the gap at 563, the counts, the §"Known deviations" note at 405), `docs/specs/2026-10-22-rust-runner-parity.md` (row 334, the gap at 437, the counts), `launcher-rust/Cargo.toml` and `Cargo.lock` (2.8.0), `launcher-rust/src/lib.rs:10`, `launcher-rust/tests/core_boundary.rs:35-55`

**Interfaces:**
- The cutover map follows the runner map's layout (header, Status, Counts, Map, Gaps). It has one row for each of the 13 `test_steam_shortcut.py` methods and the 13 `test_xvag_voice_swap.py` methods, against the Task 2–4 and 8–9 tests.
- `tests/test_cutover_parity_doc.py`: `check(DOC, [TESTS / 'test_steam_shortcut.py', TESTS / 'test_xvag_voice_swap.py'], self, allowed_drops=set())`.
- `test_user_files_includes_settings_and_icon` becomes `ported` against `src/core/update.rs::tests::user_files_include_settings_and_icon`.
- `test_the_update_installer_does_not_migrate` becomes `ported` against `tests/startup.rs::the_update_installer_does_not_migrate`.
- `version_is_the_python_launchers` becomes `version_is_the_crates`: only `CARGO_PKG_VERSION == VERSION` remains, because the Python stays at 2.7.0. Row 125 names the new test.

- [ ] **Step 1: Write the map and the checker.** Run `python -m unittest discover -s tests -p "test_*parity_doc.py"` → PASS (3 tests).
- [ ] **Step 2: Bump to 2.8.0** and rename the version test. Then `cargo test --test core_boundary` → PASS.
- [ ] **Step 3: Run the gate** in `launcher-rust/`:
  - `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` → clean.
  - `SPARTAN_GAME_DIR=<CUSA01623 from settings.json game_dir> cargo test --no-fail-fast` → 0 failed. Record the count in the cutover map header.
- [ ] **Step 4: Commit** `Docs(Launcher): map the cutover and bump to 2.8.0`
- [ ] **Step 5: Ask the user.** Once they say yes, run:
  - `git push origin rust-launcher`
  - `gh workflow run windows-zip.yml -R FelipePrado0/SpartanPort --ref rust-launcher`
  - `gh workflow run linux-appimage.yml -R FelipePrado0/SpartanPort --ref rust-launcher`

  These run with no tag: build and smoke only, nothing published. Watch them with `gh run watch`.

  Expected: `rust-launcher.yml` green on both systems, `windows-zip` green through `smoke`, and `linux-appimage` green on the five distros. A gtk failure follows Task 11's fallback.
- [ ] **Step 6: Run the manual checks with the user**, each one only after their yes:
  1. Extract the CI zip to a temporary folder. Copy `dist/spartanport-windows-2.7.0/spartanport-windows` to a second temporary folder. Run `<new>\SpartanPort.exe --install-update <copy> <pid of a finished process>`. Check that the copy opens (the Rust window), and that on that first start `_internal/`, `run.py` and `scripts/*.py` are gone while `settings.json` and `user/` stay.
  2. Add to Steam with Steam closed, on the user's Steam. Check that the art looks right in the library.
  3. `--voice-swap --dry-run` over the real dump.

  Record the outcomes in the cutover map header.

---

## Self-review notes

- **Spec coverage:**
  - §3.1 → Tasks 1–9. §3.2 → Tasks 6 and 9. §3.3 → Tasks 2–4. §3.4 → Tasks 5–6. §3.5 → Tasks 8–9.
  - §3.6 and §3.7 → Task 7. §3.8 → Tasks 2 and 8, and Task 11 for `rfd`.
  - §4.1 → Task 10. §4.2 → Task 11. §4.3 → Tasks 10, 11 and 13. §4.4 and §4.5 → Task 12.
  - §5 → Tasks 4, 6 and 9. §6 → each task's tests, plus Task 13.
- **Deviations from the spec text, decided here:**
  - `add_to_steam` takes the font list explicitly, so tests use the repo's Cinzel.
  - The entry has 17 fields, not 18; Task 4 fixes the spec.
  - `packaging/spartanport-entry.c` and `tests/test_packaged_vulkan.py` go with the Nix path.
  - `tests/dump.rs` keeps a private `python_command`: the dump tests compare against the frozen Python.
  - `--voice-swap` resolves relative paths against `OWD`.
- **Names used across tasks:**
  - `SteamError`, `Shortcut`, `Added`, `steam_art::fonts`, `open_font`, `make_art`.
  - `install_over`, `USER_FILES`, `install_update`, `route_on`, `migrates`, `prepare(paths, migrate)`.
  - `remove_python_leftovers`, `psarc::build`, `voice_swap::main`.
