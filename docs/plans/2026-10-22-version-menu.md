# Version Menu Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the "Version X is available" pill of the Rust launcher with a version menu in the top-right corner of the Home (beside the language menu) that lists the stable releases and the test builds of this system and installs the one the user picks, and stamp each package with the release it came from.

**Architecture:** `src/core/release.rs` holds the pure data: the tag parser, the release list filter and the "newer stable" rule, with no network. `src/core/update.rs` keeps the network and the install (one `GET /releases?per_page=100` replaces `/releases/latest`) and learns to read the `release.json` stamp. The hover timers of `lang_menu.rs` move to `src/ui/hover_menu.rs` so `src/ui/version_menu.rs` (button and list) shares them. `src/ui/update.rs` becomes a state machine over a list (`Ready(Vec<Release>)`) that installs any listed release through the unchanged download and `--install-update` path. The packaging scripts and the two release workflows write and ship the stamp.

**Tech Stack:** Rust stable, iced 0.14 (`iced_test` headless), `ureq` 3, `serde_json`, `tempfile`; Python 3 `unittest` for the stamp script; GitHub Actions.

**Spec:** `docs/specs/2026-10-22-version-menu-design.md`. Context: `docs/specs/2026-10-21-rust-launcher-design.md` §4.5 (language menu) and §5.6 (update).

## Global Constraints

- Work on branch `rust-launcher`, in `launcher-rust/` for the Rust tasks and at the repo root for the packaging ones. Another session commits on this checkout: stage files by name, never `git add -A`.
- `src/core/` stays free of iced. No `unwrap`/`expect` outside tests. Each task passes `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` and `cargo test` (run in `launcher-rust/`) before its commit.
- Commit subjects follow the branch: `Feat(Launcher): …`, `Refactor(Launcher): …`, `Test(Launcher): …`, `Build(Packaging): …`, `Ci(Packaging): …`; imperative, ≤ 72 characters, no AI attribution lines.
- User-facing strings go through `tr`/`tr_fmt` with the English text as the key. Rust-only strings live in `UI_EXTRA` of `src/core/i18n.rs` with every non-English language (`ar es pt fr de it pl tr zh ja ko id ru`). Keys already in `assets/i18n.json` are reused, never duplicated.
- Spec values: hover opens after 450 ms and leaves close after 300 ms (shared with the language menu); 3 stables; every pre-release, 8 rows visible then scroll; API `https://api.github.com/repos/FelipePrado0/SpartanPort/releases?per_page=100`, timeout 15 s; stamp asset and file name `release.json`; labels `Release · 2.6.0`, `Pre-release · 2.6.0 · issue209-test1`, `· 2.7.0 available`.
- `Release.url` is a `String`: `parse_releases` drops every release without the package of this system (spec §3.2), so the option of the old `Release` goes.
- A test build is never "newer": only a stable whose `version_tuple` beats the installed version marks the button.
- Snapshot references (`launcher-rust/snapshots/*.png`) are made on Windows with the placeholder art and must stay small (≤ 10 MB in all); refresh by deleting the PNGs and running `cargo test --test ui_snapshots` twice.

## Review Focus

- A stable published under a tag with a build suffix (`prerelease: false`): the channel follows the API, the row sits under Releases and the label keeps the build (`Release · 2.6.0 · hotfix1`). Pinned in Task 2.
- A stamp with an unknown `channel` or a missing/empty `version`: treated as absent, `Release · VERSION`, no dialog. Pinned in Task 3.
- An installed id that matches no row (old install, list only has newer Rust-era releases): no `✓` anywhere, "new" still on the newest stable, every row still installable. Pinned in Task 7.
- The pointer sliding from the language button onto the version button: after 450 ms the version menu is open and the language one closed, never both. Pinned in Task 6.
- `Install` arriving while a download runs (a queued click): ignored, the download goes on. Pinned in Task 5.

---

### Task 1: Version menu strings

**Files:**
- Modify: `launcher-rust/src/core/i18n.rs` (`UI_EXTRA`, around line 89, and its `tests`)

**Interfaces:**
- Produces: `tr(lang, key)` for the keys `Release`, `Pre-release`, `Releases`, `Pre-releases`, `new`, `{} available`, `Fetching…`, `Could not fetch the versions`, `No test builds`, `No test builds for Linux`, `This version is older than the installed one.` in every language.

- [ ] **Step 1: Write the failing test** in the `tests` module of `src/core/i18n.rs`

```rust
#[test]
fn every_version_menu_string_has_every_language() {
    const KEYS: [&str; 11] = [
        "Release", "Pre-release", "Releases", "Pre-releases", "new", "{} available",
        "Fetching…", "Could not fetch the versions", "No test builds",
        "No test builds for Linux", "This version is older than the installed one.",
    ];
    for en in KEYS {
        for lang in ["ar", "es", "pt", "fr", "de", "it", "pl", "tr", "zh", "ja", "ko", "id", "ru"] {
            let text = tr(lang, en);
            assert!(!text.is_empty() && text != en, "no {lang} for {en:?}");
        }
    }
    assert_eq!(tr_fmt("pt", "{} available", &["2.7.0"]), "2.7.0 disponível");
    assert_eq!(tr("pt", "Pre-release"), "Pré-lançamento");
}
```

- [ ] **Step 2: Run it to see it fail**

Run: `cargo test --lib every_version_menu_string_has_every_language`
Expected: FAIL with `no ar for "Release"`

- [ ] **Step 3: Add the 11 entries to `UI_EXTRA`**, each with the 13 languages, as the existing entries do. Keep `{}` placeholders in every translation of `{} available`.

- [ ] **Step 4: Run the i18n tests**

Run: `cargo test --lib i18n`
Expected: PASS, including `no_rust_only_string_is_a_key_of_the_table` (none of the 11 keys is in `assets/i18n.json`).

- [ ] **Step 5: Commit**

```bash
git add src/core/i18n.rs
git commit -m "Feat(Launcher): translate the version menu strings"
```

---

### Task 2: Release identity and list (`core/release.rs`)

**Files:**
- Create: `launcher-rust/src/core/release.rs`
- Modify: `launcher-rust/src/core/mod.rs` (add `pub mod release;`), `launcher-rust/src/core/update.rs` (move `version_tuple` and `digit_runs` to `release.rs`; keep `pub use crate::core::release::version_tuple;` and the test `versions_are_their_digit_runs` in `update.rs`, which the parity doc names)

**Interfaces:**
- Consumes: `crate::core::update::{UpdateError, RELEASES_PAGE}`, `crate::core::i18n::tr`, `crate::VERSION`.
- Produces (all `Debug, Clone, PartialEq, Eq`; `Channel` also `Copy`):

```rust
pub enum Channel { Release, PreRelease }
pub struct ReleaseId { pub channel: Channel, pub version: String, pub build: Option<String> }
impl ReleaseId {
    /// `<system>-v<version>[-<build>]`; the channel is the API's `prerelease`. None for a tag
    /// without `-v<digit>` or with text after the version that does not start with `-`.
    pub fn parse(tag: &str, prerelease: bool) -> Option<ReleaseId>;
    /// "Release · 2.6.0", "Pre-release · 2.6.0 · issue209-test1" (channel names through `tr`).
    pub fn label(&self, lang: &str) -> String;
    /// The label without the channel: "2.6.0", "2.6.0 · issue209-test1" (rows, dialogs, progress).
    pub fn short(&self) -> String;
    /// `Release · VERSION`, no build.
    pub fn fallback() -> ReleaseId;
}
pub struct Release { pub id: ReleaseId, pub published: String, pub url: String, pub page: String }
pub const STAMP_ASSET: &str = "release.json";
/// The releases of this system, newest first. Err(UpdateError::Json) when `json` is not an array.
pub fn parse_releases(json: &serde_json::Value, windows: bool) -> Result<Vec<Release>, UpdateError>;
pub fn stables(releases: &[Release]) -> Vec<&Release>;      // at most 3, list order
pub fn prereleases(releases: &[Release]) -> Vec<&Release>;  // all, list order
pub fn newer_stable<'a>(releases: &'a [Release], installed: &ReleaseId) -> Option<&'a Release>;
pub fn version_tuple(text: &str) -> Vec<u64>;
```

`parse_releases` keeps an element only when: `draft` is not true; `tag_name` parses; `published_at` is a string; the assets hold one named exactly `release.json` and one whose lowercased name ends with `.zip` (windows) or `.appimage` (otherwise), whose `browser_download_url` becomes `url`. `page` is `html_url` or `RELEASES_PAGE`. Sort by `published` descending (ISO 8601 strings compare as dates).

- [ ] **Step 1: Write the failing tests** in `mod tests` of `release.rs`

```rust
fn asset(name: &str) -> Value { json!({"name": name, "browser_download_url": format!("https://x.test/{name}")}) }
fn entry(tag: &str, prerelease: bool, published: &str, assets: &[&str]) -> Value {
    json!({"tag_name": tag, "prerelease": prerelease, "draft": false, "published_at": published,
           "html_url": format!("https://x.test/{tag}"), "assets": assets.iter().map(|a| asset(a)).collect::<Vec<_>>()})
}
const WIN: [&str; 2] = ["spartanport-windows.zip", "release.json"];

#[test] fn a_stable_tag_has_no_build()            // parse("windows-v2.7.0", false) == Release, "2.7.0", None
#[test] fn a_test_build_tag_keeps_its_build()     // parse("windows-v2.6.0-issue209-test1", true) == PreRelease, "2.6.0", Some("issue209-test1")
#[test] fn a_linux_tag_and_an_old_tag_parse()     // "linux-v2.2.1-test1" -> "2.2.1"/"test1"; "windows-v2.1.7-teste3" -> "2.1.7"/"teste3"; "windows-v2.1" -> "2.1"
#[test] fn a_stable_under_a_build_tag_follows_the_api() // parse("windows-v2.6.0-hotfix1", false): channel Release, build Some; label(en) == "Release · 2.6.0 · hotfix1"
#[test] fn bad_tags_are_none()                    // "v2.7.0", "windows-2.7.0", "windows-v", "windows-v2.6.0rc", "" -> None
#[test] fn labels_name_the_channel()              // en: "Release · 2.6.0", "Pre-release · 2.6.0 · issue209-test1"; pt starts with "Pré-lançamento · "; short(): "2.6.0", "2.6.0 · issue209-test1"
#[test] fn the_fallback_is_this_version()         // fallback() == ReleaseId { Release, VERSION, None }
// The next three keep the names the parity doc lists for the old `parse_release` tests.
#[test] fn windows_takes_the_zip()
    // entries: stable 2.7.0 (WIN + AppImage + release.json), test build (WIN), draft (WIN), python-era 2.6.0 (zip only),
    // linux-only "linux-v2.7.1-test1" (AppImage + release.json), bad tag (WIN), no published_at (WIN)
    // windows=true -> exactly [2.7.0 with the zip url, test build]
#[test] fn linux_takes_the_appimage_not_its_checksum()  // same entries plus "spartanport-x86_64.AppImage.sha256" on 2.7.0; windows=false -> [2.7.0 with the AppImage url, linux-only]
#[test] fn linux_has_no_package_while_the_release_has_no_appimage()  // a stable with WIN assets only is absent with windows=false and present with windows=true
#[test] fn the_list_is_newest_first()             // three stables published out of order -> sorted by published desc
#[test] fn a_non_array_body_is_an_error()         // json!({"message": "API rate limit exceeded"}) -> Err, to_string contains "array"
#[test] fn stables_stop_at_three_and_prereleases_do_not() // 5 stables + 10 prereleases -> 3 / 10, order kept
#[test] fn the_newer_stable_beats_the_installed_version()
    // list [2.8.0, 2.7.0, 2.6.0, pre 2.6.0-x]; installed 2.6.0 -> Some(2.8.0); installed 2.8.0 -> None;
    // installed PreRelease 2.6.0/issue209-test1 -> Some(2.8.0); list with only prereleases -> None; empty -> None
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test --lib release::`
Expected: compile error, `release` module missing

- [ ] **Step 3: Create `release.rs` with the interfaces above**, move `version_tuple`/`digit_runs` from `update.rs` and re-export `version_tuple` there.

- [ ] **Step 4: Run the core tests**

Run: `cargo test --lib`
Expected: PASS, including `update::tests::versions_are_their_digit_runs`

- [ ] **Step 5: Point the parity doc at the new tests.** In `docs/specs/2026-10-21-rust-launcher-parity.md` lines 365-367, change `src/core/update.rs::tests::` to `src/core/release.rs::tests::` for `windows_takes_the_zip`, `linux_takes_the_appimage_not_its_checksum` and `linux_has_no_package_while_the_release_has_no_appimage` (Task 5 deletes the old ones).

Run (repo root): `python -X utf8 -I -m unittest discover -s tests -p "test_launcher_parity_doc.py"`
Expected: OK

- [ ] **Step 6: Commit**

```bash
git add src/core/mod.rs src/core/release.rs src/core/update.rs ../docs/specs/2026-10-21-rust-launcher-parity.md
git commit -m "Feat(Launcher): parse the release list"
```

---

### Task 3: Release list request and the installed stamp (`core/update.rs`)

**Files:**
- Modify: `launcher-rust/src/core/update.rs` (`RELEASES_API`, new functions and tests; the old `Release`, `parse_release` and `latest_release` stay until Task 5)

**Interfaces:**
- Consumes: Task 2's `Release`, `ReleaseId`, `Channel`, `parse_releases`.
- Produces:

```rust
pub const RELEASES_API: &str = "https://api.github.com/repos/FelipePrado0/SpartanPort/releases?per_page=100";
/// One GET of the list (Accept `application/vnd.github+json`, global timeout 15 s), then `parse_releases`.
pub fn list_releases(windows: bool) -> Result<Vec<Release>, UpdateError>;
/// `port_dir/release.json` on Windows; `appdir/release.json` (or `port_dir` without an APPDIR) elsewhere.
pub fn stamp_path(port_dir: &Path, appdir: Option<&Path>, windows: bool) -> PathBuf;
/// The stamp, or `ReleaseId::fallback()` when the file is missing or invalid.
pub fn installed_release(stamp: &Path) -> ReleaseId;
```

The stamp is `{"channel": "release" | "pre-release", "version": "2.6.0", "build": "issue209-test1"}`; `build` may be absent. Any other `channel`, a missing or empty `version`, or unparsable JSON means fallback.

- [ ] **Step 1: Write the failing tests** in `mod tests` of `update.rs`

```rust
#[test] fn the_stamp_names_the_installed_release()
    // write {"channel":"pre-release","version":"2.6.0","build":"issue209-test1"} -> PreRelease/2.6.0/Some
    // write {"channel":"release","version":"2.7.0"} -> Release/2.7.0/None
#[test] fn a_missing_or_broken_stamp_is_this_version()
    // missing file, "not json", {"channel":"beta","version":"2.6.0"}, {"channel":"release"}, {"channel":"release","version":""} -> fallback()
#[test] fn the_stamp_sits_beside_the_launcher()
    // stamp_path(port, None, true) == port/release.json; (port, Some(appdir), false) == appdir/release.json; (port, None, false) == port/release.json
#[test] fn the_api_lists_one_page_of_releases()  // RELEASES_API ends with "/releases?per_page=100"
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test --lib update::tests`
Expected: compile error, `installed_release` not found

- [ ] **Step 3: Implement the three functions and change the constant.** `list_releases` reuses the private `get` with `Some(Duration::from_secs(15))`.

- [ ] **Step 4: Run the core tests**

Run: `cargo test --lib`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/core/update.rs
git commit -m "Feat(Launcher): read the release stamp and list"
```

---

### Task 4: Shared hover menu timers (`ui/hover_menu.rs`)

**Files:**
- Create: `launcher-rust/src/ui/hover_menu.rs`
- Modify: `launcher-rust/src/ui/lang_menu.rs` (use the shared timers; same public API), `launcher-rust/src/ui/mod.rs`

**Interfaces:**
- Produces:

```rust
pub const HOVER_OPEN: Duration = Duration::from_millis(450);
pub const LEAVE_CLOSE: Duration = Duration::from_millis(300);
#[derive(Debug, Clone, Default)]
pub struct HoverMenu { pub open: bool, pub hover_since: Option<Instant>, pub leave_since: Option<Instant> }
#[derive(Debug, Clone, Copy)]
pub enum HoverMessage { Enter, Leave, Toggle, Tick(Instant) }
/// `Enter`/`Leave` are stamped with `now`; `Tick` opens after HOVER_OPEN and closes after LEAVE_CLOSE.
pub fn apply(menu: &mut HoverMenu, message: HoverMessage, now: Instant);
pub fn timing(menu: &HoverMenu) -> bool;
```

- `lang_menu.rs` keeps `LangMenu` (now `pub type LangMenu = HoverMenu;`), `LangMessage`, `HOVER_OPEN`, `LEAVE_CLOSE` (re-exported), `update`, `timing`, `button_view`, `menu`; `update` forwards `Enter`/`Leave`/`Toggle`/`Tick` to `hover_menu::apply(&mut state.lang_menu, …, state.now)`.

- [ ] **Step 1: Write the failing tests** in `mod tests` of `hover_menu.rs`

```rust
#[test] fn hover_opens_at_450ms_and_leave_closes_at_300ms()  // Enter at t0; Tick t0+449 closed; Tick t0+450 open; Leave at t1; Tick t1+299 open; Tick t1+300 closed
#[test] fn coming_back_before_300ms_keeps_it_open()          // Toggle; Leave; Enter at +100; Tick +400 -> open
#[test] fn toggle_flips_and_clears_the_timers()              // Toggle -> open, both timers None; Toggle -> closed
#[test] fn timing_is_true_only_while_a_timer_runs()
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test --lib hover_menu`
Expected: compile error, module missing

- [ ] **Step 3: Create `hover_menu.rs`** with the logic now in `lang_menu::update` (lines 49-88), then make `lang_menu.rs` use it.

- [ ] **Step 4: Run the language menu tests unchanged**

Run: `cargo test --lib hover_menu && cargo test --test ui_home`
Expected: PASS (`hover_opens_after_450ms_and_leave_closes_after_300ms`, `coming_back_before_300ms_keeps_the_menu_open`, `click_toggles_immediately` untouched)

- [ ] **Step 5: Commit**

```bash
git add src/ui/hover_menu.rs src/ui/lang_menu.rs src/ui/mod.rs
git commit -m "Refactor(Launcher): share the hover menu timers"
```

---

### Task 5: Install any listed release (`ui/update.rs`)

**Files:**
- Modify: `launcher-rust/src/ui/update.rs`, `launcher-rust/src/ui/splash.rs:146-170`, `launcher-rust/src/ui/app.rs` (`State.installed`, `State::new`), `launcher-rust/src/ui/home.rs:669-716` (drop the pill), `launcher-rust/src/core/update.rs` (delete the old `Release`, `parse_release`, `latest_release` and their four tests)
- Test: `launcher-rust/tests/ui_pages.rs` (`mod update_flow`, lines 1640-1840), `launcher-rust/tests/ui_play.rs:630-650`, `launcher-rust/tests/ui_save.rs:180-200`, `launcher-rust/tests/ui_home.rs:481-500` (remove `update_pill_only_when_available`)

**Interfaces:**
- Consumes: Task 2 (`Release`, `ReleaseId`, `newer_stable`, `version_tuple`), Task 3 (`list_releases`, `stamp_path`, `installed_release`).
- Produces:

```rust
pub enum UpdateState {
    None, Checking,
    Ready(Vec<Release>),
    Downloading { releases: Vec<Release>, release: Release, percent: u8 },
    Failed(String),
}
pub enum UpdateMessage {
    Check { manual: bool },
    Checked { manual: bool, result: Result<Vec<Release>, String> },
    Install(Release),
    OpenPage(String),      // the page to open (dialog Yes of a read-only AppImage folder)
    Confirmed(Release),    // dialog Yes of the install
    Progress(u8),
    Done(Result<(), String>),
}
pub fn releases(state: &State) -> &[Release];          // Ready or Downloading, else empty
pub fn newer(state: &State) -> Option<&Release>;        // newer_stable(releases(state), &state.installed)
pub fn ready(state: &mut State, releases: Vec<Release>); // -> Ready, unless Downloading (kept)
pub fn is_older(release: &Release, installed: &ReleaseId) -> bool; // version_tuple less than
pub fn confirm_text(lang: &str, release: &Release, installed: &ReleaseId) -> String;
pub enum Decision { Offer, Latest, Nothing }
pub fn offer_decision(newer: bool, manual: bool) -> Decision;  // Offer when newer; Latest when manual && !newer; else Nothing
pub fn install_decision(busy: bool, windows: bool, packaged: bool, appimage: Option<&Path>, writable: &dyn Fn(&Path) -> bool) -> InstallPlan;
pub fn is_packaged() -> bool;  // unchanged
pub fn drain(state: &mut State);  // unchanged
// app.rs
pub struct State { pub installed: ReleaseId, /* … */ }  // State::new: installed_release(&stamp_path(&paths.port_dir, env APPDIR, cfg!(windows)))
// splash.rs
pub fn start_up_offer(state: &mut State, releases: Vec<Release>);  // = update::ready
pub enum Check { /* … */ Update(Option<Vec<Release>>) }
```

Behaviour of `update`:
- `Check`: `None`/`Failed` → `Checking`; task `list_releases(cfg!(windows))` → `Checked`.
- `Checked { Ok(list) }`: `ready(state, list)`; `offer_decision(newer(state).is_some(), manual)`: `Latest` → Info modal `You have the latest version ({})` with `state.installed.version`; `Offer`/`Nothing` → nothing else (the button of Task 6 shows it). `Checked { Err }`: `Failed(error)` unless `Downloading`; manual → Error modal `Could not check for updates: {}`.
- `Install(release)`: ignored while `Downloading` or when `release.id == state.installed`. Else `install_decision(restore_blocked(state), cfg!(windows), is_packaged(), appimage, &can_write)`: `Blocked` → Info `Close the game before updating.`; `OpenPage` → open `release.page`; `ReadOnly(folder)` → the YesNo of today with yes `OpenPage(release.page)`; `Confirm` → YesNo with `confirm_text` and yes `Confirmed(release)`.
- `confirm_text`: `tr_fmt(lang, "Install version {} now? The launcher closes, installs it and opens again. Saves and settings are kept.", &[&release.id.short()])`, plus `"\n"` and `tr(lang, "This version is older than the installed one.")` when `is_older`.
- `OpenPage(page)`: modal None; open the page. `Confirmed(release)`: modal None; game live → Info modal as today; else `Downloading { releases: releases(state).to_vec(), release, percent: 0 }` and the worker of today with `release.url`. `Done(Err)` → `Ready(releases)` + Error modal. `offer`, `is_newer` and `Available` go; `state.status` is no longer set by updates.
- `start_checks`: `list_releases(cfg!(windows)).ok()` → `Check::Update`; `CheckDone(Update(Some(list)))` → `start_up_offer`.
- `home::top_right`: only the language button remains; `update_pill_text` goes.

- [ ] **Step 1: Rewrite the failing tests.** In `tests/ui_pages.rs` `mod update_flow`, replace the helpers and tests:

```rust
fn release(version: &str, build: Option<&str>, prerelease: bool) -> Release  // url "https://example.test/{version}.zip", page "https://example.test/page", published "2026-10-22T00:00:00Z"
fn checked(manual: bool, result: Result<Vec<Release>, String>) -> Message

#[test] fn a_list_with_a_newer_stable_is_ready_and_newer()   // Checked Ok([999.0.0, VERSION]) silent -> Ready, newer() == 999.0.0, no modal, status unchanged
#[test] fn manual_check_with_latest_shows_the_info()         // Checked Ok([VERSION]) manual -> Info "You have the latest version ({VERSION})."; silent -> no modal
#[test] fn a_failed_manual_check_reports_and_a_silent_one_does_not()  // Err("offline") manual -> Error modal; silent -> Failed("offline"), no modal
#[test] fn offer_decision_follows_newer_and_manual()         // (true,_) Offer; (false,true) Latest; (false,false) Nothing
#[test] fn the_start_up_list_is_ready()                      // splash::start_up_offer(&mut state, vec![...]) -> Ready; Downloading stays Downloading
#[test] fn installing_the_installed_release_does_nothing()   // Install(release equal to state.installed) -> no modal, task.units() == 0
#[test] fn install_is_blocked_while_running()                // install_decision(true, …) == Blocked; (false, true, true, None, _) == Confirm
#[test] fn windows_source_tree_opens_the_page()              // (false, true, false, None, _) == OpenPage
#[test] fn linux_needs_a_writable_appimage_folder()          // as today without the release argument
#[test] fn install_asks_and_marks_an_older_release()         // confirm_text(en, 2.5.0, installed 2.6.0) ends with "This version is older than the installed one."; 2.7.0 does not; pre-release 2.6.0/x over stable 2.6.0 does not
#[test] fn install_is_ignored_while_downloading()            // Downloading 2.7.0 -> Install(2.5.0): state still Downloading 2.7.0, no modal, task.units() == 0
#[test] fn a_failed_download_goes_back_to_the_list()         // Downloading{releases:[2.7.0]} -> Done(Err("boom")) -> Ready([2.7.0]), Error modal "Update failed: boom", update_progress None
#[test] fn progress_updates_the_percent()                    // Progress(42) -> Downloading percent 42
```

Adapt `tests/ui_play.rs::a_confirmed_update_waits_for_the_game` to `UpdateState::Ready(vec![release])` and `UpdateMessage::Confirmed(release.clone())` (assert `Ready` after). Adapt `tests/ui_save.rs:180-200` (the language rebuild keeps the update state) to `Ready` with a 2.7.0 release and `newer(&state)` still 2.7.0. Delete `tests/ui_home.rs::update_pill_only_when_available`.

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test --test ui_pages update_flow`
Expected: compile errors on `UpdateState::Ready`, `Install(_)`

- [ ] **Step 3: Implement the interfaces above** in `ui/update.rs`, `splash.rs`, `app.rs`, `home.rs`; delete the old `Release` API from `core/update.rs`.

- [ ] **Step 4: Run every test**

Run: `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`
Expected: PASS. `ui_snapshots` passes too (the pill is not in the references: no update is pending in the fixtures).

- [ ] **Step 5: Commit**

```bash
git add src/core/update.rs src/ui/update.rs src/ui/splash.rs src/ui/app.rs src/ui/home.rs tests/ui_pages.rs tests/ui_play.rs tests/ui_save.rs tests/ui_home.rs
git commit -m "Feat(Launcher): install any listed release"
```

---

### Task 6: Version button on the Home (`ui/version_menu.rs`)

**Files:**
- Create: `launcher-rust/src/ui/version_menu.rs`
- Modify: `launcher-rust/src/ui/mod.rs`, `launcher-rust/src/ui/app.rs` (`Message::VersionMenu`, `State.version_menu`, dispatch near line 413, `Tick` near line 368, the clock near line 707), `launcher-rust/src/ui/home.rs` (`top_right`, lines 669-716)
- Test: `launcher-rust/tests/ui_home.rs`; refresh `launcher-rust/snapshots/*.png`

**Interfaces:**
- Consumes: Task 4 (`HoverMenu`, `HoverMessage`, `apply`, `timing`), Task 5 (`UpdateState`, `UpdateMessage::Check`, `newer`).
- Produces:

```rust
#[derive(Debug, Clone)]
pub enum VersionMessage { Enter, Leave, Toggle, Tick(Instant) }   // Pick comes in Task 7
pub const BUTTON_ID: &str = "version-button";
/// Applies the hover timers; when the menu just opened over `UpdateState::None` or `Failed`, returns
/// the task of `UpdateMessage::Check { manual: false }`.
pub fn update(state: &mut State, message: VersionMessage) -> Task<Message>;
pub fn timing(state: &State) -> bool;
pub fn button_text(state: &State) -> String;
pub fn button_view(state: &State) -> Element<'_, Message>;
// app.rs
pub enum Message { /* … */ VersionMenu(VersionMessage) }
pub struct State { /* … */ pub version_menu: HoverMenu }
```

`button_text`: `Downloading` → `tr_fmt(lang, "Downloading version {}… {}%", &[&release.id.short(), &percent])`; otherwise `state.installed.label(lang)`, plus ` · ` and `tr_fmt(lang, "{} available", &[&newer.id.version])` when `newer(state)` is `Some`, plus ` ▾`. The button is `button_flat`, padding `[3, 10]`, size 12, id `BUTTON_ID`; pressable only when `home_interactive_enabled(state)` and not `Downloading`; wrapped in a `mouse_area` with `Enter`/`Leave` like the language button. `home::top_right` places it before the language button (after it, right to left) and the line uses `.wrap()`.

`app.rs`: `Message::VersionMenu(m)` → `version_menu::update`; afterwards, if `state.version_menu.open` then `state.lang_menu.open = false`; symmetric after `Message::Lang`. `Tick` also runs `version_menu::update(state, VersionMessage::Tick(at))` and, when both menus are open, closes the language one; the task it returns is batched with the tick's. The fast clock condition adds `|| version_menu::timing(state)`.

- [ ] **Step 1: Write the failing tests** in `tests/ui_home.rs`

```rust
fn version(state: &mut State, message: VersionMessage) -> Task<Message>
fn ready_with(state: &mut State, releases: Vec<Release>)   // state.update = Ready(releases)

#[test] fn the_version_button_names_the_installed_release()   // fallback: find("Release · {VERSION} ▾") ok; installed = PreRelease 2.6.0/issue209-test1: find("Pre-release · 2.6.0 · issue209-test1 ▾")
#[test] fn the_version_button_offers_a_newer_stable()         // Ready([2.7.0 stable]) with VERSION 2.6.0 -> "Release · 2.6.0 · 2.7.0 available ▾"; pt: "… · 2.7.0 disponível ▾"; a Ready([pre 2.6.0/x]) shows no "available"
#[test] fn the_version_button_sits_before_the_language_one()  // bounds().x of BUTTON_ID < bounds().x of "English ▾"; with state.rtl it is greater
#[test] fn the_version_button_is_not_pressable_while_downloading_or_dimmed() // Downloading: text "Downloading version 2.7.0… 42%", click emits nothing; modal Some: click emits nothing
#[test] fn hovering_the_version_button_opens_and_fetches()    // UpdateState::None; Enter; Tick +450 -> open, state.update Checking; Toggle on Ready -> no Check (update stays Ready)
#[test] fn opening_one_menu_closes_the_other()                // lang Toggle then version Toggle -> only version open; version Toggle then lang Toggle -> only lang open
#[test] fn sliding_from_language_to_version_opens_only_version() // lang Toggle (open); lang Leave at t0; version Enter at t0; Tick t0+450 -> version open, lang closed
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test --test ui_home version_button`
Expected: compile error, `version_menu` missing

- [ ] **Step 3: Create `version_menu.rs` (button half) and wire `app.rs` and `home.rs`.**

- [ ] **Step 4: Run the UI tests, then refresh the snapshots**

Run: `cargo test --test ui_home && cargo test --test ui_pages`
Expected: PASS
Run: delete `snapshots/*.png`, then `cargo test --test ui_snapshots` (fails with `reference created` lines), then `cargo test --test ui_snapshots` again
Expected: second run PASS; `git status` shows only the references whose corner changed; the folder stays ≤ 10 MB. Open two of them (`home-1920x1080-en.png`, `home-1280x800-ar.png`) and check the button reads `Release · 2.6.0 ▾` beside the language button, mirrored in `ar`.

- [ ] **Step 5: Full check and commit**

Run: `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`
Expected: PASS

```bash
git add src/ui/version_menu.rs src/ui/mod.rs src/ui/app.rs src/ui/home.rs tests/ui_home.rs snapshots
git commit -m "Feat(Launcher): show the version button on the Home"
```

---

### Task 7: The version list (`ui/version_menu.rs`, `menu`)

**Files:**
- Modify: `launcher-rust/src/ui/version_menu.rs`, `launcher-rust/src/ui/app.rs` (`view`, near line 572: push `version_menu::menu(state)` when `state.version_menu.open`)
- Test: `launcher-rust/tests/ui_home.rs`

**Interfaces:**
- Consumes: Task 2 (`stables`, `prereleases`), Task 5 (`releases`, `newer`, `UpdateMessage::Install`), Task 6.
- Produces:

```rust
pub enum VersionMessage { Enter, Leave, Toggle, Tick(Instant), Pick(Release) }
pub const MENU_ID: &str = "version-menu";
pub const VISIBLE_PRERELEASES: usize = 8;
pub const ROW_HEIGHT: f32 = 29.0;   // 12.5 pt text + [5, 12] padding + 2 spacing
pub fn menu(state: &State) -> Element<'_, Message>;
```

`menu`: a `container_card` with id `MENU_ID`, same placement constants as `lang_menu::menu` (`EDGE_TOP + MENU_DROP`, `EDGE_RIGHT`, `MENU_WIDTH`), wrapped in the same `mouse_area` (`VersionMessage::Enter`/`Leave`). Content by state:
- `None`/`Checking`: header Releases, one muted row `Fetching…`.
- `Failed`: header Releases, one muted row `Could not fetch the versions`.
- `Ready`/`Downloading`: header `Releases` (`CINZEL_BOLD`, 11, `RED_HI`), one `button_flat` row per `stables()`: text `✓ ` + `r.id.short()` when `r.id == state.installed`, else `r.id.short()`; the `newer` row carries a trailing `new` text in `RED_HI` after a `horizontal_space()`. Header `Pre-releases`, then `prereleases()` rows `r.id.short()` (`2.6.0 · issue209-test1`) inside `scrollable(column)` whose container has `.max_height(VISIBLE_PRERELEASES as f32 * ROW_HEIGHT)`; no pre-release → one muted row `No test builds` (Windows) / `No test builds for Linux`. Every row presses `VersionMessage::Pick(release)`.
- `update(Pick(release))`: closes the menu (open false, timers None); the installed release → `Task::none()`; else returns `update::update(state, UpdateMessage::Install(release))`.

- [ ] **Step 1: Write the failing tests** in `tests/ui_home.rs`

```rust
fn five_stables_and_ten_prereleases() -> Vec<Release>  // 2.9.0 … 2.5.0 stable; 2.6.0-t1 … 2.6.0-t10 pre-release

#[test] fn the_open_menu_lists_three_stables_and_every_prerelease() // open on that list: find "Releases", "Pre-releases", "2.9.0", "2.8.0", "2.7.0"; "2.6.0" (stable row) absent; "2.6.0 · t1" and "2.6.0 · t10" present
#[test] fn the_menu_marks_the_installed_and_the_new()               // installed 2.7.0 (stamp), list [2.8.0, 2.7.0, 2.6.0]: "✓ 2.7.0", "2.8.0" row contains "new", "2.6.0" plain
#[test] fn an_unmatched_install_has_no_tick_but_still_a_new()      // installed fallback 2.6.0, list [2.8.0, 2.7.0]: no text starting with "✓", "new" present
#[test] fn the_prerelease_list_scrolls_after_eight_rows()          // MENU_ID bounds height with 8 pre-releases == with 20; with 3 it is smaller
#[test] fn the_menu_says_when_there_are_no_test_builds()           // Ready([2.7.0]) -> "No test builds" (cfg windows) / "No test builds for Linux"
#[test] fn the_menu_says_fetching_and_failed()                     // None -> "Fetching…"; Failed -> "Could not fetch the versions"
#[test] fn picking_a_row_closes_and_installs()                     // click "2.8.0" emits [VersionMenu(Pick(r))] with r.id.version 2.8.0; update(Pick) -> menu closed and (Windows source tree) a task with units() > 0 or a modal; Pick(installed) -> closed, task.units() == 0, no modal
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test --test ui_home the_open_menu_lists`
Expected: compile error, `Pick` missing

- [ ] **Step 3: Implement `menu`, `Pick` and the `view` layer.**

- [ ] **Step 4: Full check**

Run: `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`
Expected: PASS (the snapshots do not change: the fixtures keep the menu closed)

- [ ] **Step 5: Commit**

```bash
git add src/ui/version_menu.rs src/ui/app.rs tests/ui_home.rs
git commit -m "Feat(Launcher): list the releases in the version menu"
```

---

### Task 8: The stamp script (`packaging/release_stamp.py`)

Run from the repo root.

**Files:**
- Create: `packaging/release_stamp.py`
- Test: `tests/test_release_stamp.py`

**Interfaces:**
- Produces: `python packaging/release_stamp.py TAG --prerelease true|false` prints the stamp JSON (`channel`, `version`, `build` only when the tag has one) and exits 0; an invalid tag prints `invalid release tag: TAG` to stderr and exits 1. The tag rule is Task 2's (`<system>-v<version>[-<build>]`).

- [ ] **Step 1: Write the failing tests** in `tests/test_release_stamp.py` (unittest, `from paths import ROOT` like the other launcher tests)

```python
def stamp(tag, prerelease):  # subprocess.run([sys.executable, '-I', ROOT/'packaging/release_stamp.py', tag, '--prerelease', prerelease], capture_output=True, text=True)

class ReleaseStampTest(unittest.TestCase):
    def test_a_stable_tag(self):            # 'windows-v2.7.0', 'false' -> {"channel": "release", "version": "2.7.0"}
    def test_a_test_build_tag(self):        # 'windows-v2.6.0-issue209-test1', 'true' -> {"channel": "pre-release", "version": "2.6.0", "build": "issue209-test1"}
    def test_a_linux_tag(self):             # 'linux-v2.2.1-test1', 'true' -> version 2.2.1, build test1
    def test_a_stable_under_a_build_tag(self):  # 'windows-v2.6.0-hotfix1', 'false' -> channel release, build hotfix1
    def test_an_invalid_tag_fails(self):    # 'v2.7.0' -> returncode 1, stderr 'invalid release tag: v2.7.0'
```

- [ ] **Step 2: Run them to see them fail**

Run: `python -X utf8 -I -m unittest discover -s tests -p "test_release_stamp.py"`
Expected: FAIL (script missing, returncode 2)

- [ ] **Step 3: Write `packaging/release_stamp.py`** (argparse, `re.fullmatch(r'[a-z]+-v(\d[\d.]*)(?:-(.+))?', tag)`, `json.dumps` with `ensure_ascii=False`).

- [ ] **Step 4: Run the tests**

Run: `python -X utf8 -I -m unittest discover -s tests -p "test_release_stamp.py"`
Expected: PASS (5 tests)

- [ ] **Step 5: Commit**

```bash
git add packaging/release_stamp.py tests/test_release_stamp.py
git commit -m "Build(Packaging): write the release stamp"
```

---

### Task 9: Ship the stamp in the packages and the releases

Run from the repo root. Configuration and shell: no unit tests; verification is the parse checks below.

**Files:**
- Modify: `packaging/windows/package.sh` (after line 63: `[[ -f release.json ]] && cp release.json "$dest/"`), `packaging/linux/build.sh` (after line 46: `[[ -f release.json ]] && install -m644 release.json "$appdir/release.json"`), `packaging/windows/smoke-test.ps1` (when `$env:SPARTAN_RELEASE_TAG` is set, fail unless `release.json` is in the extracted package root and parses as JSON with a `version`), `packaging/linux/smoke-test.sh` (same check on `squashfs-root/release.json` after `--appimage-extract`), `.github/workflows/windows-zip.yml`, `.github/workflows/linux-appimage.yml`

**Interfaces:**
- Consumes: Task 8's script. Task 3 reads `port_dir/release.json` (Windows) and `$APPDIR/release.json` (AppImage); Task 2 lists only releases with the `release.json` asset.

Workflow changes, both files:
- Build job, before the package step, a step `Stamp` with `if: inputs.tag != ''`, env `GH_TOKEN: ${{ github.token }}`, `GH_REPO: ${{ github.repository }}`, `TAG: ${{ inputs.tag }}`:

```bash
pre=$(gh release view "$TAG" --json isPrerelease -q .isPrerelease 2>/dev/null || echo true)
python packaging/release_stamp.py "$TAG" --prerelease "$pre" > release.json
mkdir -p dist && cp release.json dist/
cat release.json
```

  (`python3` on the Linux runner; on Windows the step runs in the `msys2` shell with `$PYTHON_EXE` like the package step.)
- `upload-artifact` `path` gains a line `dist/release.json`.
- Smoke job: env `SPARTAN_RELEASE_TAG: ${{ inputs.tag }}` on the smoke step.
- Release step: `gh release upload "$TAG" … dist/release.json --clobber`.

- [ ] **Step 1: Make the script and workflow edits above.**

- [ ] **Step 2: Parse checks**

Run: `bash -n packaging/windows/package.sh && bash -n packaging/linux/build.sh && bash -n packaging/linux/smoke-test.sh`
Expected: no output
Run: `powershell -NoProfile -Command "[scriptblock]::Create((Get-Content -Raw packaging/windows/smoke-test.ps1)) | Out-Null; 'ok'"`
Expected: `ok`
Run: `python -X utf8 -I -c "import yaml,sys; [yaml.safe_load(open(p, encoding='utf-8')) for p in sys.argv[1:]]; print('ok')" .github/workflows/windows-zip.yml .github/workflows/linux-appimage.yml` (if PyYAML is missing, `actionlint` or a careful diff read replaces it; say which in the commit body)
Expected: `ok`
Run: `git diff --check`
Expected: no output

- [ ] **Step 3: Dry-run the smoke check branch locally.** Build a zip holding only `SpartanPort.exe` (empty file) and a `release.json` from Task 8, run `smoke-test.ps1` with `SPARTAN_RELEASE_TAG=windows-v9.9.9-test1` and confirm it reaches the stamp check (it may fail later on the missing binaries: the stamp line must come first and pass); run again without `release.json` and confirm the failure names `release.json`.

- [ ] **Step 4: Commit**

```bash
git add packaging/windows/package.sh packaging/linux/build.sh packaging/windows/smoke-test.ps1 packaging/linux/smoke-test.sh .github/workflows/windows-zip.yml .github/workflows/linux-appimage.yml
git commit -m "Ci(Packaging): ship release.json with each package"
```

---

### Task 10: Record the deviations

**Files:**
- Modify: `docs/specs/2026-10-22-version-menu-design.md` (state line and a short "Deviations" section at the end)

- [ ] **Step 1: Update the spec** to "Estado: implementado" and list what the code does differently from the text: `Release.url` is a `String`; `UpdateState::Ready(Vec<Release>)` derives `newer` instead of storing it; the manual "Check for updates" with a newer stable only updates the button (no dialog); `ReleaseId::label` instead of `Display`; `core/release.rs` split out of `core/update.rs`; anything else the implementers recorded.

- [ ] **Step 2: Commit**

```bash
git add docs/specs/2026-10-22-version-menu-design.md
git commit -m "Docs(Launcher): record the version menu deviations"
```
