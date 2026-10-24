# Launcher First-Run Setup Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** When the Rust launcher opens without `settings.json`, show a three-step setup (folders, graphics card, video) between the splash and the Home, and write the same files the pages write when the player finishes or skips it.

**Architecture:** A new Elm sub-module `src/ui/setup.rs` (`SetupState`, `Step`, `SetupMessage`, `update`, `view`) in the pattern of `splash.rs` and `play.rs`. `State::open` (new; `app::run` uses it) creates the setup when `read_app_settings` reports a missing file; `State::new` never does, so every existing test keeps its state. The setup's controls send the existing `Message::Set`, `Browse` and `Browsed`, so edits go straight into `state.settings` and `state.ini` through `apply_set` and `game::folder_chosen`; Finish is `persist::collect_or_report`, Skip restores the defaults captured at open and then finishes.

**Tech Stack:** Rust stable, iced 0.14, `iced_test` 0.14, `serde_json`, `tempfile` (dev). Same crate, no new dependency.

**Spec:** `docs/specs/2026-10-22-launcher-setup-design.md`. Parent spec: `docs/specs/2026-10-21-rust-launcher-design.md`. Read the setup spec before any task.

## Global Constraints

- Crate `launcher-rust/`; all commands below run from that folder. `src/core/**` never contains the string `iced`.
- No `unwrap`/`expect` outside `#[cfg(test)]` and `tests/` (clippy denies them at the crate root).
- No new key in `settings.json` or `spartanport.ini`; no new file.
- Every new English string shown by the setup goes into `UI_EXTRA` in `src/core/i18n.rs` with at least a `pt` entry (ruling R33, enforced by `every_rust_only_string_has_portuguese`), unless the string is already a key of `assets/i18n.json` (`no_rust_only_string_is_a_key_of_the_table` fails otherwise). Already in the table: `Folders`, `Graphics card`, `Game folder`, `Update folder (optional)`, `Display mode`, `Frame rate limit`, `Engine frame rate`, `Aspect ratio`, `Render resolution`, `restart`, the hints copied from the Graphics and Performance pages.
- Spec texts, verbatim: title `Initial setup`; step line `Step {} of {}` + ` · ` + step name; intros `Choose the folder of your game dump. The update folder is optional.` / `The game runs on this graphics card.` / `Resolution and frame rate. The in-game menu (Insert or R3+L2) changes the same values.`; buttons `Skip`, `Back`, `Next`, `Finish`; `Missing or empty files:`; `Graphics card not checked: the game picks one.`; footer `You can change all of this later in the Graphics, Performance and Game tabs.`; same-folder line `Choose two separate folders for the game and its update, or leave the update folder empty.` (already used by PLAY in `src/ui/play.rs`).
- Portuguese of the new strings (spec §4.7): `Configuração inicial`, `Passo {} de {}`, `Vídeo`, `Pular`, `Voltar`, `Avançar`, `Concluir`, `Escolha a pasta do dump do jogo. A pasta da atualização é opcional.`, `O jogo roda nesta placa de vídeo.`, `Resolução e taxa de quadros. O menu do jogo (Insert ou R3+L2) muda os mesmos valores.`, `Arquivos ausentes ou vazios:`, `Placa de vídeo não verificada: o jogo escolhe uma.`, `Tudo isso pode ser mudado depois nas abas Graphics, Performance e Game.`
- Card: 640 px wide, `theme::container_card`, padding 24, title `CINZEL_BOLD` 22 px, step line and intro `theme::text_muted`; Skip/Back/Next `theme::button_flat`, Finish `theme::button_play`.
- Commit messages: Conventional Commits with this repo's capitalised type and scope, imperative, ≤ 50 characters (`Feat(Launcher): open the setup without settings`). No AI attribution lines. Stage only the files named in the task: other agents commit on this branch at the same time; never `git add -A` or `git add .`.
- Never start the user's real game.

## Review Focus

- A legacy `%APPDATA%\gow3-launcher\settings.json` on a machine without a data-folder `settings.json`: the portable migration copies it first and the setup must not open. Test added to Task 1.
- A `spartanport.ini` present without `settings.json` (the ini survived, the settings did not): step 3 must preselect the ini's values and Finish must keep the ini's other lines. Test added to Task 6.
- The player chooses the update folder before the game folder and later points the game folder at the same place: the same-folder line must appear under the update folder and Next must switch off again. Test added to Task 4.
- The GPU list arrives after the player already reached step 2 (the splash ended on its 3 s maximum): the list must grow in place and a `gpu_id` already chosen must stay. Test added to Task 5.
- Finish with an unwritable data folder: the `SAVE_FAILED` dialog opens, Esc closes it, the setup stays open with the chosen values, and `settings.json` does not exist. Test added to Task 7.

---

## File Structure

```
launcher-rust/
  src/core/settings.rs        SettingsFile; read_app_settings reports Missing / Read / Unreadable
  src/core/i18n.rs            UI_EXTRA entries of the setup strings
  src/ui/setup.rs             NEW: SetupState, Step, SetupMessage, update, view, background,
                              can_advance, folder_problems, key_pressed, finish
  src/ui/mod.rs               pub mod setup
  src/ui/app.rs               State.setup, State.settings_file, State::open, Message::Setup,
                              view/update/tick/scene_size gates, run uses State::open
  src/ui/home.rs              corner_monogram and top_right made reusable
  src/ui/pages/game.rs        folder_row made pub
  src/ui/pages/performance.rs select_row helper
  tests/ui_setup.rs           NEW: the setup's UI tests
  tests/ui_snapshots.rs       Shot::Setup(Step)
  snapshots/setup{1,2,3}-{1280x800,1920x1080}-{en,pt,ar}.png  NEW references (18)
```

---

### Task 1: Tell a missing settings file apart

**Files:**
- Modify: `launcher-rust/src/core/settings.rs` (`read_app_settings`, its doc comment and tests)
- Modify: `launcher-rust/src/ui/app.rs:140-150` (the `read_app_settings` call in `State::new`)

**Interfaces:**
- Produces:
  ```rust
  /// What `read_app_settings` found at `paths.config_file()` after the portable migration.
  #[derive(Debug, Clone, Copy, PartialEq, Eq)]
  pub enum SettingsFile { Missing, Read, Unreadable }
  pub fn read_app_settings(paths: &Paths) -> (Settings, SettingsFile);
  ```
  `load_app_settings(paths) -> Settings` unchanged. `State` gains `pub settings_file: SettingsFile` (set from the tuple; `settings_unreadable` stays and is `settings_file == SettingsFile::Unreadable || ini_unreadable`).

- [ ] **Step 1: Write the failing tests** in the `tests` module of `src/core/settings.rs`, replacing the two `unreadable` assertions of `an_unreadable_settings_file_is_reported_and_left_as_it_is` with the enum and adding:

```rust
#[test]
fn a_missing_file_is_told_apart_from_an_unreadable_one() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().to_string_lossy().into_owned();
    let paths = Paths::from_env(dir.path(), &|k| match k {
        "SPARTAN_DATA_DIR" => Some(data.clone()),
        "APPDATA" | "XDG_CONFIG_HOME" => Some(format!("{data}/nowhere")),
        _ => None,
    });
    assert_eq!(read_app_settings(&paths).1, SettingsFile::Missing);
    for broken in ["{not json", "[1, 2]", ""] {
        fs::write(paths.config_file(), broken).unwrap();
        assert_eq!(read_app_settings(&paths).1, SettingsFile::Unreadable, "{broken:?}");
    }
    fs::write(paths.config_file(), r#"{"ui_language": "pt"}"#).unwrap();
    let (settings, file) = read_app_settings(&paths);
    assert_eq!(file, SettingsFile::Read);
    assert_eq!(settings.get_str("ui_language"), "pt");
}

#[test]
fn a_legacy_settings_file_counts_as_read() {
    // Review Focus: the portable migration runs before the file is looked for.
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    // `Paths::legacy_config_file` is `<APPDATA>/gow3-launcher/settings.json` on Windows and
    // `<XDG_CONFIG_HOME>/gow3-launcher/settings.json` elsewhere.
    let legacy = dir.path().join("legacy").join("gow3-launcher");
    fs::create_dir_all(&legacy).unwrap();
    fs::write(legacy.join("settings.json"), r#"{"ui_language": "de"}"#).unwrap();
    let config_home = dir.path().join("legacy").to_string_lossy().into_owned();
    let paths = Paths::from_env(dir.path(), &|k| match k {
        "SPARTAN_DATA_DIR" => Some(data.to_string_lossy().into_owned()),
        "APPDATA" | "XDG_CONFIG_HOME" => Some(config_home.clone()),
        _ => None,
    });
    let (settings, file) = read_app_settings(&paths);
    assert_eq!(file, SettingsFile::Read);
    assert_eq!(settings.get_str("ui_language"), "de");
    assert!(paths.config_file().is_file(), "the migration copied it");
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --lib settings::`
Expected: compile error, `SettingsFile` not found.

- [ ] **Step 3: Implement** `SettingsFile` and the new return of `read_app_settings` in `src/core/settings.rs`; keep the doc comment's §6 note. In `src/ui/app.rs` `State::new`, bind `let (settings, settings_file) = read_app_settings(&paths);`, derive `settings_unreadable` from `settings_file == SettingsFile::Unreadable`, and store `settings_file` in the new field `pub settings_file: SettingsFile` (document it: "what `settings.json` was at the start; `State::open` opens the setup on `Missing`").

- [ ] **Step 4: Run the whole suite**

Run: `cargo test`
Expected: all pass (the `tests/` binaries compile against the new field; nothing reads it yet).

- [ ] **Step 5: Commit**

```bash
git add src/core/settings.rs src/ui/app.rs
git commit -m "Feat(Launcher): tell a missing settings file apart"
```

---

### Task 2: Open the setup without settings

**Files:**
- Create: `launcher-rust/src/ui/setup.rs`
- Modify: `launcher-rust/src/ui/mod.rs` (`pub mod setup;`)
- Modify: `launcher-rust/src/ui/app.rs` (`State.setup`, `State::open`, `Message::Setup`, `update`, `run`)
- Create: `launcher-rust/tests/ui_setup.rs`

**Interfaces:**
- Consumes: `SettingsFile` (Task 1), `persist::{gather, Persisted}`.
- Produces, in `src/ui/setup.rs`:
  ```rust
  #[derive(Debug, Clone, Copy, PartialEq, Eq)]
  pub enum Step { Folders, Gpu, Video }
  impl Step {
      pub const ALL: [Step; 3];
      pub fn number(self) -> usize;              // 1, 2, 3
      pub fn next(self) -> Option<Step>;         // None on Video
      pub fn previous(self) -> Option<Step>;     // None on Folders
      pub fn title_en(self) -> &'static str;     // "Folders" | "Graphics card" | "Video"
      pub fn intro_en(self) -> &'static str;     // the three intros of the Global Constraints
  }
  #[derive(Debug, Clone)]
  pub struct SetupState { pub step: Step, pub defaults: Persisted }
  impl SetupState { pub fn new(state: &State) -> SetupState }   // step Folders, defaults = persist::gather(state)
  #[derive(Debug, Clone)]
  pub enum SetupMessage { Next, Back, Skip, Finish }
  pub fn update(state: &mut State, message: SetupMessage) -> Task<Message>;
  ```
  In `src/ui/app.rs`: `pub setup: Option<SetupState>` on `State` (doc: "the first-run setup while it is open; `None` on every other start"); `impl State { pub fn open(paths: Paths) -> State }` = `new` then `setup = Some(SetupState::new(&state))` when `state.settings_file == SettingsFile::Missing`; `Message::Setup(SetupMessage)`; `run` builds the state with `State::open`.
  In this task `update` handles only `Next` and `Back` (step change; `Next` ignored when `can_advance` is false — `can_advance` arrives in Task 4, so for now `Next` always moves). `Skip` and `Finish` are `Task::none()` until Task 7.

- [ ] **Step 1: Write the failing tests** in `tests/ui_setup.rs`:

```rust
#![allow(clippy::unwrap_used, clippy::expect_used)]
//! The first-run setup: when it opens, its steps, its edits and how it ends.

mod common;

use std::path::Path;

use spartanport_launcher::core::paths::Paths;
use spartanport_launcher::ui::app::{self, Message, State};
use spartanport_launcher::ui::setup::{SetupMessage, Step};

/// A state over `dir` only, as the program opens it, with the splash already gone.
fn opened(dir: &Path) -> State {
    let root = dir.display().to_string();
    let env = move |key: &str| match key {
        "APPDATA" | "USERPROFILE" | "HOME" | "XDG_CONFIG_HOME" => Some(root.clone()),
        _ => None,
    };
    let mut state = State::open(Paths::from_env(dir, &env));
    state.splash.done = true;
    state
}

fn setup(state: &mut State, message: SetupMessage) {
    let _ = app::update(state, Message::Setup(message));
}

#[test]
fn the_setup_opens_only_without_a_settings_file() {
    let dir = tempfile::tempdir().unwrap();
    let state = opened(dir.path());
    assert_eq!(state.setup.as_ref().map(|s| s.step), Some(Step::Folders));

    for text in ["{}", "{not json"] {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("settings.json"), text).unwrap();
        assert!(opened(dir.path()).setup.is_none(), "{text:?}");
    }
    // `new` is what every other test builds on: never the setup.
    let dir = tempfile::tempdir().unwrap();
    assert!(common::state(dir.path()).setup.is_none());
}

#[test]
fn next_and_back_walk_the_three_steps() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = opened(dir.path());
    setup(&mut state, SetupMessage::Back);
    assert_eq!(state.setup.as_ref().unwrap().step, Step::Folders);
    setup(&mut state, SetupMessage::Next);
    assert_eq!(state.setup.as_ref().unwrap().step, Step::Gpu);
    setup(&mut state, SetupMessage::Next);
    assert_eq!(state.setup.as_ref().unwrap().step, Step::Video);
    setup(&mut state, SetupMessage::Next);
    assert_eq!(state.setup.as_ref().unwrap().step, Step::Video);
    setup(&mut state, SetupMessage::Back);
    assert_eq!(state.setup.as_ref().unwrap().step, Step::Gpu);
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --test ui_setup`
Expected: compile error, `State::open` / `ui::setup` not found.

- [ ] **Step 3: Implement** `src/ui/setup.rs` with the Interfaces above (module doc: "The first-run setup (setup spec): three steps between the splash and the Home, shown while `State::setup` is `Some`."), `pub mod setup;` in `src/ui/mod.rs`, the `State` field, `State::open`, `Message::Setup(message) => return setup::update(state, message)` in `app::update`, and `State::open(paths.clone())` in `app::run`.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test --test ui_setup` then `cargo test`
Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add src/ui/setup.rs src/ui/mod.rs src/ui/app.rs tests/ui_setup.rs
git commit -m "Feat(Launcher): open the setup without settings"
```

---

### Task 3: Draw the setup card

**Files:**
- Modify: `launcher-rust/src/ui/setup.rs` (`background`, `view`, `card`)
- Modify: `launcher-rust/src/ui/app.rs:566-600` (`view`)
- Modify: `launcher-rust/src/ui/home.rs:701-770` (`corner_monogram`, `top_right`)
- Modify: `launcher-rust/src/core/i18n.rs` (`UI_EXTRA`)
- Modify: `launcher-rust/tests/ui_setup.rs`

**Interfaces:**
- Consumes: `splash::{SPLASH_BG, layers}`, `state.fire.view()`, `lang_menu::button_view`, `theme::{container_card, text_muted, button_flat, button_play, CINZEL_BOLD}`.
- Produces:
  ```rust
  // src/ui/setup.rs
  /// The splash's dark background, with the fire over it once the splash is gone.
  pub fn background(state: &State) -> Element<'_, Message>;
  /// Corner monogram, language button and the centred card of the current step.
  pub fn view(state: &State) -> Element<'_, Message>;
  // src/ui/home.rs
  pub fn corner_monogram(state: &State) -> Element<'_, Message>;     // what `block` drew inline
  pub fn top_right(state: &State, enabled: bool, update_pill: bool) -> Element<'_, Message>;
  ```
  `home::block` calls `corner_monogram(state)` and `top_right(state, enabled, true)`; the setup calls `top_right(state, true, false)` (no update pill, spec §4.1).
  `app::view`: the base layer is `setup::background(state)` when `state.setup.is_some()`, else `home::background(state)`; when the setup is open and `state.splash.done`, the scene gets `setup::view(state)` instead of the dim layer, `home::block` and the drawer; the language menu, the splash layer and the modal layers stay as today; the tab bar (`bar(state)`) is left out while the setup is open.
  The card's body for each step is a placeholder `column![]` in this task; Tasks 4–6 fill it. The footer: `Skip` (`button_flat`, text `text_muted`) at the start of the row, then a `space().width(Fill)`, then `Back` (only when `step.previous()` is `Some`), then `Next` (`button_flat`) or, on `Step::Video`, `Finish` (`button_play`). Each button's `on_press` is `Message::Setup(..)`; `Next` gets `on_press_maybe(can_advance)` from Task 4 on (always pressable in this task). Last line: the footer sentence in `text_muted`.

- [ ] **Step 1: Write the failing tests** in `tests/ui_setup.rs` (add `use iced_test::simulator::simulator;` and `common::messages`):

```rust
#[test]
fn the_setup_replaces_the_home_and_the_bar() {
    let dir = tempfile::tempdir().unwrap();
    let state = opened(dir.path());
    let mut ui = simulator(app::view(&state));
    assert!(ui.find("Initial setup").is_ok());
    assert!(ui.find("Step 1 of 3 · Folders").is_ok());
    assert!(ui.find("Skip").is_ok());
    assert!(ui.find("Next").is_ok());
    assert!(ui.find("Back").is_err());
    assert!(ui.find("PLAY").is_err());
    assert!(ui.find("Graphics").is_err(), "the tab bar is not drawn");
    assert!(ui.find("English ▾").is_ok(), "the language button stays");
}

#[test]
fn the_last_step_offers_finish_and_back() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = opened(dir.path());
    state.setup.as_mut().unwrap().step = Step::Video;
    let mut ui = simulator(app::view(&state));
    assert!(ui.find("Step 3 of 3 · Video").is_ok());
    assert!(ui.find("Finish").is_ok());
    assert!(ui.find("Back").is_ok());
    assert!(ui.find("Next").is_err());
    assert!(matches!(
        common::messages(&state, "Back").as_slice(),
        [Message::Setup(SetupMessage::Back)]
    ));
}

#[test]
fn the_setup_reads_in_portuguese() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = opened(dir.path());
    state.lang = "pt".into();
    let mut ui = simulator(app::view(&state));
    assert!(ui.find("Configuração inicial").is_ok());
    assert!(ui.find("Passo 1 de 3 · Pastas").is_ok());
    assert!(ui.find("Pular").is_ok());
    assert!(ui.find("Avançar").is_ok());
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --test ui_setup`
Expected: FAIL, `find("Initial setup")` is `Err`.

- [ ] **Step 3: Implement** `background`, `view` and the card in `src/ui/setup.rs`; `corner_monogram` and the `update_pill` parameter in `src/ui/home.rs`; the `view` gates in `src/ui/app.rs`; and the `UI_EXTRA` entries for `Initial setup`, `Step {} of {}`, `Video`, `Skip`, `Back`, `Next`, `Finish`, the three intros and the footer sentence, each with its `pt` text from the Global Constraints. The step line is `format!("{} · {}", tr_fmt(lang, "Step {} of {}", &[&n, "3"]), tr(lang, step.title_en()))`. Centre the card as `widgets::modal` does (`container(card).width(Fill).height(Fill).align_x(Center).align_y(Center)`), without the dim layer and without a `mouse_area`.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test --test ui_setup` then `cargo test`
Expected: all pass, including `i18n::tests::every_rust_only_string_has_portuguese` and the Home snapshot tests (the Home view is unchanged).

- [ ] **Step 5: Commit**

```bash
git add src/ui/setup.rs src/ui/app.rs src/ui/home.rs src/core/i18n.rs tests/ui_setup.rs
git commit -m "Feat(Launcher): draw the setup card"
```

---

### Task 4: Validate the setup folders

**Files:**
- Modify: `launcher-rust/src/ui/setup.rs` (`folder_problems`, `can_advance`, the Folders body, `update` gate)
- Modify: `launcher-rust/src/ui/pages/game.rs:280-296` (`folder_row` made `pub`)
- Modify: `launcher-rust/src/core/i18n.rs` (`Missing or empty files:`)
- Modify: `launcher-rust/tests/ui_setup.rs`

**Interfaces:**
- Consumes: `core::game::{missing_game_files, same_folder, REQUIRED_GAME_FILES, REQUIRED_UPDATE_FILES}`, `play::describe_missing(lang, &missing) -> Vec<String>`, `game::folder_row(state, key, label, note)` (now `pub`), `game::browse_prompt`.
- Produces:
  ```rust
  /// The red lines under the folder `key` (`game_dir` or `update_dir`); empty when the folder passes.
  pub fn folder_problems(state: &State, key: &str) -> Vec<String>;
  /// Whether Next may leave the current step: both folders pass on Folders, always on the others.
  pub fn can_advance(state: &State) -> bool;
  ```
  `folder_problems`: for `game_dir`, when `missing_game_files(game_dir, REQUIRED_GAME_FILES)` is not empty, the heading `Missing or empty files:` followed by `describe_missing`; for `update_dir`, nothing when empty, else the same-folder sentence alone when `same_folder(update_dir, game_dir)` and `game_dir` is not empty, else heading plus `describe_missing` of `REQUIRED_UPDATE_FILES` when something is missing. The Folders body: `folder_row` for `game_dir` (label `Game folder`, note = the Game page's hint) and `update_dir` (label `Update folder (optional)`, its hint), each followed by its problem lines as `text(line).size(12).color(palette::RED_HI)`. `update`: `Next` does nothing when `!can_advance(state)`; the Next button gets `on_press_maybe(can_advance(state).then_some(..))`.

- [ ] **Step 1: Write the failing tests** in `tests/ui_setup.rs` (add `use common::{browsed, dump_with_patches, messages};` and `use spartanport_launcher::ui::setup::{can_advance, folder_problems};`):

```rust
#[test]
fn next_waits_for_a_complete_game_folder() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = opened(dir.path());
    assert!(!can_advance(&state));
    assert!(messages(&state, "Next").is_empty(), "Next has no on_press");
    let lines = folder_problems(&state, "game_dir");
    assert_eq!(lines.first().map(String::as_str), Some("Missing or empty files:"));
    assert!(lines.iter().any(|line| line.contains("eboot.bin")), "{lines:?}");
    assert!(folder_problems(&state, "update_dir").is_empty());
    let mut ui = simulator(app::view(&state));
    assert!(ui.find("Missing or empty files:").is_ok());

    let dump = dump_with_patches(dir.path());
    browsed(&mut state, "game_dir", &dump);
    assert!(folder_problems(&state, "game_dir").is_empty());
    assert!(can_advance(&state));
    assert!(state.home.info.is_some(), "the Home facts follow the folder");
    assert!(matches!(
        messages(&state, "Next").as_slice(),
        [Message::Setup(SetupMessage::Next)]
    ));
    setup(&mut state, SetupMessage::Next);
    assert_eq!(state.setup.as_ref().unwrap().step, Step::Gpu);
}

#[test]
fn next_is_ignored_while_the_folder_is_incomplete() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = opened(dir.path());
    setup(&mut state, SetupMessage::Next);
    assert_eq!(state.setup.as_ref().unwrap().step, Step::Folders);
}

#[test]
fn the_update_folder_must_differ_from_the_game_folder() {
    // Review Focus: the update folder chosen first, the game folder pointed at it later.
    let dir = tempfile::tempdir().unwrap();
    let mut state = opened(dir.path());
    let dump = dump_with_patches(dir.path());
    browsed(&mut state, "update_dir", &dump);
    assert!(folder_problems(&state, "update_dir").is_empty(), "no game folder yet");
    browsed(&mut state, "game_dir", &dump);
    assert_eq!(
        folder_problems(&state, "update_dir"),
        ["Choose two separate folders for the game and its update, or leave the update folder empty."]
    );
    assert!(!can_advance(&state));

    let other = dir.path().join("update");
    std::fs::create_dir_all(&other).unwrap();
    browsed(&mut state, "update_dir", &other);
    let lines = folder_problems(&state, "update_dir");
    assert_eq!(lines.first().map(String::as_str), Some("Missing or empty files:"));
    assert!(!can_advance(&state));
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --test ui_setup`
Expected: compile error, `can_advance` / `folder_problems` not found.

- [ ] **Step 3: Implement** `folder_problems`, `can_advance`, the Folders body and the `Next` gate in `src/ui/setup.rs`; make `folder_row` `pub` in `src/ui/pages/game.rs`; add `Missing or empty files:` to `UI_EXTRA`.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test --test ui_setup` then `cargo test`
Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add src/ui/setup.rs src/ui/pages/game.rs src/core/i18n.rs tests/ui_setup.rs
git commit -m "Feat(Launcher): validate the setup folders"
```

---

### Task 5: Pick the graphics card in the setup

**Files:**
- Modify: `launcher-rust/src/ui/setup.rs` (the Gpu body)
- Modify: `launcher-rust/src/ui/pages/performance.rs` (`select_row`)
- Modify: `launcher-rust/src/core/i18n.rs` (`Graphics card not checked: the game picks one.`)
- Modify: `launcher-rust/tests/ui_setup.rs`

**Interfaces:**
- Consumes: `performance::rows(state, lang) -> Vec<SelectRow>`, `widgets::{row, select_pairs}`.
- Produces, in `src/ui/pages/performance.rs`:
  ```rust
  /// The drop-down of `key` as the Performance page draws it (its pairs and current value).
  pub fn select_row<'a>(state: &State, lang: &str, key: &str) -> Element<'a, Message>;
  ```
  (the `pick` closure of `performance::view` becomes a call to it.) The Gpu body: `row(state.rtl, &tr(lang, "Graphics card"), select_row(state, lang, "gpu_id"), Some(&hint))` (no `restart` note: the game is not running) with the Performance hint `The GPU the game runs on. Choose your dedicated card if the game uses the integrated one.`; below it, when `state.gpus.is_empty()`, `text(tr(lang, "Graphics card not checked: the game picks one.")).size(13).style(theme::text_muted)`.

- [ ] **Step 1: Write the failing tests** in `tests/ui_setup.rs` (add `use common::gpu_rows; use common::ARC_LAPTOP; use spartanport_launcher::ui::splash::{Check, SplashMessage};`):

```rust
fn at_step(dir: &Path, step: Step) -> State {
    let mut state = opened(dir);
    state.setup.as_mut().unwrap().step = step;
    state
}

#[test]
fn the_card_step_lists_automatic_and_the_cards() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = at_step(dir.path(), Step::Gpu);
    let mut ui = simulator(app::view(&state));
    assert!(ui.find("Step 2 of 3 · Graphics card").is_ok());
    assert!(ui.find("Automatic").is_ok());
    assert!(ui.find("Graphics card not checked: the game picks one.").is_ok());

    // Review Focus: the list arrives after the player reached this step.
    let _ = app::update(&mut state, Message::Set("gpu_id".into(), "1".into()));
    let _ = app::update(
        &mut state,
        Message::Splash(SplashMessage::CheckDone(Check::Gpus(gpu_rows(&ARC_LAPTOP)))),
    );
    assert_eq!(state.settings.get_str("gpu_id"), "1");
    let mut ui = simulator(app::view(&state));
    assert!(ui.find("NVIDIA GeForce RTX 4060 Laptop GPU").is_ok());
    assert!(ui.find("Graphics card not checked: the game picks one.").is_err());
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test --test ui_setup the_card_step`
Expected: FAIL, `find("Automatic")` is `Err`.

- [ ] **Step 3: Implement** `select_row` in `src/ui/pages/performance.rs` (and use it in its `view`), the Gpu body in `src/ui/setup.rs`, the `UI_EXTRA` entry.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test --test ui_setup` then `cargo test`
Expected: all pass (the Performance snapshots are unchanged: same pairs, same widget).

- [ ] **Step 5: Commit**

```bash
git add src/ui/setup.rs src/ui/pages/performance.rs src/core/i18n.rs tests/ui_setup.rs
git commit -m "Feat(Launcher): pick the graphics card in the setup"
```

---

### Task 6: Pick the video options in the setup

**Files:**
- Modify: `launcher-rust/src/ui/setup.rs` (the Video body)
- Modify: `launcher-rust/tests/ui_setup.rs`

**Interfaces:**
- Consumes: `performance::{select_row, aspect_options}`, `widgets::{row, select}`, `options::DISPLAY_MODES`, `Message::Aspect`.
- Produces: the Video body, in this order, each a `row(state.rtl, label, control, Some(hint))` with the label and hint of the page it comes from: `Display mode` (`select("display_mode", DISPLAY_MODES, &ini("display_mode"), lang)`, Graphics hint `Borderless covers the screen without changing the display mode.`), `Aspect ratio` (`pick_list(aspect_options(state), Some(state.aspect.clone()), Message::Aspect).style(theme::pick_list_dark)`, Performance hint `Shows the render resolutions of this shape only.`), `Render resolution` with the `restart` note (`select_row(state, lang, "render_resolution")`, Performance hint), `Frame rate limit` (`select_row(.., "fps_limit")`, Performance hint `The engine frame rate (below) is the highest it can go.`), `Engine frame rate` (`select_row(.., "engine_fps")`, Performance hint).

- [ ] **Step 1: Write the failing tests** in `tests/ui_setup.rs`:

```rust
#[test]
fn the_video_step_edits_the_ini_values() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = at_step(dir.path(), Step::Video);
    let mut ui = simulator(app::view(&state));
    for label in ["Display mode", "Aspect ratio", "Frame rate limit", "Engine frame rate"] {
        assert!(ui.find(label).is_ok(), "{label}");
    }
    assert!(ui.find("Windowed").is_ok());
    assert!(ui.find("Unlimited").is_ok());
    assert!(ui.find("120").is_ok());
    for (key, value) in [("fps_limit", "60"), ("display_mode", "fullscreen"), ("engine_fps", "144")] {
        let _ = app::update(&mut state, Message::Set(key.into(), value.into()));
        assert_eq!(state.ini.values.get(key).map(String::as_str), Some(value));
    }
    let mut ui = simulator(app::view(&state));
    assert!(ui.find("Fullscreen").is_ok());
}

#[test]
fn an_ini_without_settings_is_shown_and_kept() {
    // Review Focus: the ini survived, settings.json did not.
    let dir = tempfile::tempdir().unwrap();
    // The ini lives where `Paths::ini_path` says for this folder (`opened` builds the same Paths).
    let ini = Paths::from_env(dir.path(), &|_| None).ini_path();
    std::fs::write(&ini, "# mine\nfps_limit=60\nextra=kept\n").unwrap();
    let mut state = at_step(dir.path(), Step::Video);
    assert!(state.setup.is_some());
    assert_eq!(state.ini.values.get("fps_limit").map(String::as_str), Some("60"));
    let mut ui = simulator(app::view(&state));
    assert!(ui.find("60").is_ok());
    setup(&mut state, SetupMessage::Finish);
    let text = std::fs::read_to_string(&ini).unwrap();
    assert!(text.starts_with("# mine\nfps_limit=60\nextra=kept\n"), "{text}");
}
```

(`Finish` arrives in Task 7; until then leave the last three lines of the second test out and add them in Task 7.)

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --test ui_setup the_video_step`
Expected: FAIL, `find("Display mode")` is `Err`.

- [ ] **Step 3: Implement** the Video body in `src/ui/setup.rs`.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test --test ui_setup` then `cargo test`
Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add src/ui/setup.rs tests/ui_setup.rs
git commit -m "Feat(Launcher): pick the video options in the setup"
```

---

### Task 7: Finish, skip and close

**Files:**
- Modify: `launcher-rust/src/ui/setup.rs` (`finish`, `Finish`, `Skip`)
- Modify: `launcher-rust/src/ui/app.rs:445-450` (`CloseRequested` gate)
- Modify: `launcher-rust/tests/ui_setup.rs`

**Interfaces:**
- Consumes: `persist::collect_or_report(state) -> bool`, `persist::Persisted`, `game::game_changed`, `core::settings::Settings`.
- Produces:
  ```rust
  /// Writes the files as Save does; on success the setup closes and the Home shows.
  pub fn finish(state: &mut State) -> Task<Message>;
  ```
  `SetupMessage::Finish` → `finish`. `SetupMessage::Skip` → `state.settings = Settings(defaults.app)`, `state.ini.values = defaults.ini` (from `setup.defaults.clone()`), then `state.settings.set("ui_language", json!(state.lang))`, then `game::game_changed(state)` batched with `finish(state)`. `CloseRequested(id)` with `state.setup.is_some()` returns `window::close(id)` before any save.

- [ ] **Step 1: Write the failing tests** in `tests/ui_setup.rs` (add `use iced::keyboard::key::Named; use iced::keyboard::{Key, Location, Modifiers}; use iced::window;`):

```rust
fn settings_file(dir: &Path) -> serde_json::Value {
    serde_json::from_slice(&std::fs::read(dir.join("settings.json")).unwrap()).unwrap()
}

#[test]
fn finish_writes_the_choices_and_shows_the_home() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = opened(dir.path());
    let dump = dump_with_patches(dir.path());
    browsed(&mut state, "game_dir", &dump);
    let _ = app::update(&mut state, Message::Set("gpu_id".into(), "1".into()));
    let _ = app::update(&mut state, Message::Set("fps_limit".into(), "60".into()));
    state.setup.as_mut().unwrap().step = Step::Video;
    setup(&mut state, SetupMessage::Finish);
    assert!(state.setup.is_none());
    let saved = settings_file(dir.path());
    assert_eq!(saved["game_dir"], dump.display().to_string());
    assert_eq!(saved["gpu_id"], "1");
    let ini = std::fs::read_to_string(state.paths.ini_path()).unwrap();
    assert!(ini.contains("fps_limit=60"), "{ini}");
    let mut ui = simulator(app::view(&state));
    assert!(ui.find("PLAY").is_ok());
    assert!(ui.find("Initial setup").is_err());
    assert!(!spartanport_launcher::ui::persist::dirty(&state));
}

#[test]
fn skip_writes_the_defaults_but_keeps_the_language() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = opened(dir.path());
    let dump = dump_with_patches(dir.path());
    browsed(&mut state, "game_dir", &dump);
    let _ = app::update(&mut state, Message::Set("fps_limit".into(), "60".into()));
    state.lang = "pt".into();
    setup(&mut state, SetupMessage::Skip);
    assert!(state.setup.is_none());
    let saved = settings_file(dir.path());
    assert_eq!(saved["game_dir"], "");
    assert_eq!(saved["ui_language"], "pt");
    assert_eq!(state.ini.values.get("fps_limit").map(String::as_str), Some("0"));
    assert!(state.home.info.is_none(), "the Home facts follow the restored folder");
}

#[test]
fn closing_the_window_writes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = opened(dir.path());
    let _ = app::update(&mut state, Message::Set("fps_limit".into(), "60".into()));
    let _ = app::update(&mut state, Message::CloseRequested(window::Id::unique()));
    assert!(!dir.path().join("settings.json").exists());
    assert!(state.setup.is_some());
}

#[test]
fn a_failed_finish_keeps_the_setup_open() {
    // Review Focus: the data folder cannot be written.
    let dir = tempfile::tempdir().unwrap();
    let mut state = opened(dir.path());
    // A folder where the file should be: the atomic write cannot rename over it.
    std::fs::create_dir_all(state.paths.config_file()).unwrap();
    let _ = app::update(&mut state, Message::Set("fps_limit".into(), "60".into()));
    setup(&mut state, SetupMessage::Finish);
    assert!(state.setup.is_some());
    assert!(state.modal.is_some(), "the SAVE_FAILED dialog");
    let _ = app::update(
        &mut state,
        Message::Key(Key::Named(Named::Escape), Modifiers::default(), Location::Standard),
    );
    assert!(state.modal.is_none());
    assert!(state.setup.is_some());
    assert_eq!(state.ini.values.get("fps_limit").map(String::as_str), Some("60"));
}
```

`iced::window::Id::unique()` is what `tests/ui_save.rs` feeds to `CloseRequested`.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --test ui_setup finish`
Expected: FAIL, `state.setup.is_none()` is false.

- [ ] **Step 3: Implement** `finish`, `Finish`, `Skip` in `src/ui/setup.rs` and the `CloseRequested` gate in `src/ui/app.rs`.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test --test ui_setup` then `cargo test`
Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add src/ui/setup.rs src/ui/app.rs tests/ui_setup.rs
git commit -m "Feat(Launcher): finish or skip the setup"
```

---

### Task 8: Keys and language during the setup

**Files:**
- Modify: `launcher-rust/src/ui/setup.rs` (`key_pressed`)
- Modify: `launcher-rust/src/ui/app.rs:325-345` (the `Message::Key` arm) and `:417-424` (the `Message::Lang` arm)
- Modify: `launcher-rust/tests/ui_setup.rs`

**Interfaces:**
- Consumes: `persist::rebuild(state, language, scale) -> Task<Message>`, `i18n::{launcher_language, system_language}`.
- Produces:
  ```rust
  /// Enter is Next (when allowed) or Finish on the last step; every other key does nothing.
  pub fn key_pressed(state: &mut State, key: &Key) -> Task<Message>;
  ```
  In `app::update`'s `Message::Key` arm, after the splash check and before the Controls capture: `if state.setup.is_some() { return setup::key_pressed(state, &key); }`. In the `Message::Lang` arm: when `picked && state.setup.is_some()`, `let language = launcher_language(&state.settings.get_str("ui_language"), system_language); let scale = state.scale; return persist::rebuild(state, language, scale);` instead of `persist::save`.

- [ ] **Step 1: Write the failing tests** in `tests/ui_setup.rs` (add `use spartanport_launcher::ui::lang_menu::LangMessage; use spartanport_launcher::ui::pages::Page;`):

```rust
fn press(state: &mut State, key: Key) {
    let _ = app::update(
        state,
        Message::Key(key, Modifiers::default(), Location::Standard),
    );
}

#[test]
fn keys_stay_inside_the_setup() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = opened(dir.path());
    press(&mut state, Key::Named(Named::ArrowRight));
    assert_eq!(state.page, Page::Home);
    press(&mut state, Key::Named(Named::Escape));
    assert!(state.setup.is_some());
    press(&mut state, Key::Named(Named::Enter));
    assert_eq!(state.setup.as_ref().unwrap().step, Step::Folders, "Next not allowed yet");
    assert!(matches!(state.game, spartanport_launcher::ui::play::GameState::Idle));

    let dump = dump_with_patches(dir.path());
    browsed(&mut state, "game_dir", &dump);
    press(&mut state, Key::Named(Named::Enter));
    assert_eq!(state.setup.as_ref().unwrap().step, Step::Gpu);
    press(&mut state, Key::Named(Named::Enter));
    assert_eq!(state.setup.as_ref().unwrap().step, Step::Video);
    press(&mut state, Key::Named(Named::Enter));
    assert!(state.setup.is_none(), "Enter on the last step is Finish");
    assert!(dir.path().join("settings.json").exists());
}

#[test]
fn a_language_pick_rebuilds_without_writing() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = at_step(dir.path(), Step::Gpu);
    let _ = app::update(&mut state, Message::Lang(LangMessage::Pick("pt".into())));
    assert_eq!(state.lang, "pt");
    assert_eq!(state.setup.as_ref().unwrap().step, Step::Gpu);
    assert!(!dir.path().join("settings.json").exists());
    let mut ui = simulator(app::view(&state));
    assert!(ui.find("Passo 2 de 3 · Placa de vídeo").is_ok());
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --test ui_setup keys_stay`
Expected: FAIL (ArrowRight changed `state.page`, or Enter did not advance).

- [ ] **Step 3: Implement** `key_pressed` in `src/ui/setup.rs` and the two gates in `src/ui/app.rs`.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test --test ui_setup` then `cargo test`
Expected: all pass (`tests/ui_navigation.rs` and `tests/ui_home.rs` build their state with `State::new`, so their keys still reach the tabs).

- [ ] **Step 5: Commit**

```bash
git add src/ui/setup.rs src/ui/app.rs tests/ui_setup.rs
git commit -m "Feat(Launcher): keys and language in the setup"
```

---

### Task 9: Burn the fire under the setup

**Files:**
- Modify: `launcher-rust/src/ui/app.rs:503-530` (`tick`, `scene_size`)
- Modify: `launcher-rust/tests/ui_setup.rs`

**Interfaces:**
- Consumes: `fire::intensity_for(splash, focused, visible, effects)`, `splash::layers`.
- Produces: `tick` passes `!state.splash.done || state.setup.is_some()` as the `splash` argument; `scene_size` subtracts `BAR_HEIGHT` only when `state.setup.is_none()` (no bar under the setup, so the fire reaches the window's bottom edge). `setup::background` (Task 3) draws `state.fire.view()` over `SPLASH_BG` when `splash::layers(state).home_draws_fire`.

- [ ] **Step 1: Write the failing test** in `tests/ui_setup.rs` (add `use iced::Size; use spartanport_launcher::ui::app::{scene_size, BAR_HEIGHT};` and `use std::time::Duration;`):

```rust
#[test]
fn the_setup_scene_is_the_whole_window() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = opened(dir.path());
    let _ = app::update(&mut state, Message::Resized(Size::new(1280.0, 800.0)));
    assert_eq!(scene_size(&state), Size::new(1280.0, 800.0));
    state.setup = None;
    assert_eq!(scene_size(&state), Size::new(1280.0, 800.0 - BAR_HEIGHT));
}

#[test]
fn the_fire_burns_at_splash_strength_under_the_setup() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = opened(dir.path());
    let start = state.now;
    let _ = app::update(&mut state, Message::Tick(start + Duration::from_millis(200)));
    assert_eq!(state.fire.intensity(), 1.0);
    state.setup = None;
    let _ = app::update(&mut state, Message::Tick(start + Duration::from_millis(400)));
    assert_eq!(state.fire.intensity(), 0.8);
}
```

`Fire` has no getter today: add `pub fn intensity(&self) -> f32` to `Fire` in `src/ui/fire.rs`, delegating to the same method on `FireCanvas` and `FireShader`, which return the value their `set_intensity` stored.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --test ui_setup the_setup_scene the_fire_burns`
Expected: FAIL (`scene_size` still subtracts the bar; intensity is 0.8).

- [ ] **Step 3: Implement** the two changes in `src/ui/app.rs` (and the getter if needed).

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test --test ui_setup` then `cargo test`
Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add src/ui/app.rs src/ui/fire.rs tests/ui_setup.rs
git commit -m "Feat(Launcher): burn the fire under the setup"
```

---

### Task 10: Snapshot the setup steps

**Files:**
- Modify: `launcher-rust/tests/ui_snapshots.rs` (`Shot::Setup(Step)`, `fixture`, three tests)
- Create: `launcher-rust/snapshots/setup1-1280x800-en.png` and the other 17 references

**Interfaces:**
- Consumes: `setup::{SetupState, Step}`, `persist::gather`, `common::{gpu_rows, ARC_LAPTOP}`.
- Produces: `Shot::Setup(Step)` named `setup1`, `setup2`, `setup3` (`Step::number`). In `fixture`, a `Shot::Setup(step)` keeps the splash done and sets `state.setup = Some(SetupState { step, defaults: persist::gather(&state) })` after `show_fixed_paths` (so step 1 shows `C:\SpartanPort\dump` and, that folder not existing, its red list; the fixture's `gpu_rows(&ARC_LAPTOP)` fills step 2).

- [ ] **Step 1: Add the shots and tests** in `tests/ui_snapshots.rs`:

```rust
#[test]
fn setup_folders() {
    check(Shot::Setup(Step::Folders));
}

#[test]
fn setup_gpu() {
    check(Shot::Setup(Step::Gpu));
}

#[test]
fn setup_video() {
    check(Shot::Setup(Step::Video));
}
```

- [ ] **Step 2: Run them once to write the references**

Run: `cargo test --test ui_snapshots setup_`
Expected: FAIL three times with "reference written" for each of the 18 files (a missing reference is written and fails the test, as the module doc says).

- [ ] **Step 3: Look at the 18 PNGs** in `snapshots/`: dark background, monogram in the corner (right corner in `ar`), language button, the centred card with the right step, no tab bar, no PLAY. Fix the view and rerun Step 2 (delete the wrong references first) until they look right.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test --test ui_snapshots` then `cargo test`
Expected: all pass; the existing references are untouched (`git status` shows only the 18 new files and `tests/ui_snapshots.rs`).

- [ ] **Step 5: Commit**

```bash
git add tests/ui_snapshots.rs snapshots/setup1-*.png snapshots/setup2-*.png snapshots/setup3-*.png
git commit -m "Test(Launcher): snapshot the setup steps"
```
