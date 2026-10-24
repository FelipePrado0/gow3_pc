# Rust Runner Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move `run.py` and the 13 preparation modules of `scripts/` (plus `discord_presence.py`) into `launcher-rust/src/runner/`, so PLAY, the in-game restart and `--play` start the game through `<exe> --run` with no Python, writing the same files byte for byte.

**Architecture:** `src/runner/` is a second UI-free layer beside `src/core/` (the boundary test covers both). `runner::run` reproduces `run.py`'s sixteen steps in order and spawns `spartanport-probe`; the launcher's PLAY spawns the launcher's own executable with `--run` instead of `python run.py`, so the output reader, job object, STOP and restart detection stay as they are. Every file the probe or the game reads is compared byte for byte with fixtures the Python modules wrote; the DXT encoder of Pillow is ported for that, the glyph drawing is compared by PSNR.

**Tech Stack:** Rust stable, `iced-x86 = "=1.21.0"` (`decoder`), `image` (+ `bmp`), `ab_glyph`, `sha2`, `roxmltree`, `serde_json`, `indexmap`, `windows-sys` (+ `Win32_System_Ioctl`, `Win32_Storage_FileSystem`), `nix`, `tempfile`; Python 3 + Pillow 12.3.0 + `iced-x86==1.21.0` for the one-off fixture generator.

**Spec:** `docs/specs/2026-10-22-rust-runner-design.md`. Python reference: `run.py` and `scripts/*.py` at `003e42b` (2.7.0). Read the spec before any task; read the named Python function and its tests before porting them. The launcher's plan `docs/plans/2026-10-21-rust-launcher.md` and parity map `docs/specs/2026-10-21-rust-launcher-parity.md` set the house style.

## Global Constraints

- Crate `launcher-rust/`, package `spartanport-launcher` 2.7.0; `src/runner/**` and `src/core/**` never contain the string `iced` (`tests/core_boundary.rs` enforces it).
- No `unwrap`/`expect` outside `#[cfg(test)]`; `unsafe` only in `src/core/launch.rs`, `src/core/platform/windows.rs` and, for `GlobalMemoryStatusEx`, `src/runner/guest_memory.rs`.
- Byte for byte with the Python 2.7.0 modules: `boot.bin` (`G3PROBE2`), `eboot.elf`, `entry.bin`, `analysis.json`, `boot-libc.bin` (`G3PROBE4`), `libc-link.json`, `boot-linked.bin` (`G3PROBE5`), `link.json`, `isa-rewrite.bin` (`G3ISA1`), `content.bin` (`G3CONT01`, 28 bytes), `content-profile.json`, `patches-merged.xml`, `patches.bin` (`G3PATCH2`), `patch_status.json`, `graphics_state.json`, `out/pause_menu/R_UI.WAD`, and in `out/glyphs/*.WAD` everything but the painted DXT blocks. JSON as the Python writes it (`json.dump` with its `indent`/`ensure_ascii` per module: read the module).
- Painted glyph blocks: PSNR ≥ 40 dB per texture against the Pillow drawing; the DXT1/DXT5 compression itself is byte-exact (Task 11).
- Messages the runner prints stay in English and identical to `run.py`'s (the launcher derives `exit_hint` from them); exit codes: 1 (`fail`), 20 (CPU), 130 (Ctrl+C), else the probe's.
- `SPARTAN_RESTART_COMMAND`: `<exe> --run --after <pid> <args>` on Windows (`list2cmdline` quoting), `<exe> --run <args>` on Linux (`shlex.join` quoting).
- `run.py` and `scripts/` stay in the repository and the packages (sub-project 4 removes them); nothing in this plan edits them except `tools/` and `tests/`.
- Commit messages: Conventional Commits, this repo's capitalised type and scope, imperative, ≤ 50 characters (`Feat(Launcher): add the SELF loader`). No AI attribution.
- Never start the user's real game in tests: `fake-game` and the new `fake-probe` binaries only. Tests that need the dump read `SPARTAN_GAME_DIR` and return early without it.
- Every write of a derived file is atomic (`core::fs::atomic_write`), as the launcher's.

## Review Focus

- A stale `out/isa-rewrite.bin` from an older image (after a game update): the C loader rejects a table whose SHA-256 differs, so `run` always rewrites the table from the current `boot-linked.bin` and deletes it when no site is found. Test added to Task 15.
- A `mod-game-XXXX` overlay left by a crashed run: `build_overlay` removes every stale `mod-game-*` under `out/` before it creates a new one, and never follows links while removing. Test added to Task 8.
- An executable path with spaces and a quote in `SPARTAN_RESTART_COMMAND`: the Windows quoting must round-trip through `CreateProcess` as `subprocess.list2cmdline` does; the Linux quoting through `sh -c` as `shlex.join` does. Test added to Task 15.
- `patch_status.json` missing or corrupt after `write_patches` (a crash between the two writes): `SPARTAN_ENGINE_FPS` falls back to 60 and `SPARTAN_DMEM_MB` stays unset, never a panic. Test added to Task 15.
- A Discord pipe that answers nothing: the handshake read times out after 5 s, the game is not delayed, and `close` returns within 3 s. Test added to Task 14.

---

### Task 1: Runner scaffold, routes and boundary

**Files:**
- Create: `src/runner/mod.rs`
- Modify: `src/lib.rs` (`pub mod runner;`), `src/startup.rs:24-49` (routes), `src/main.rs:13-36`, `tests/core_boundary.rs:17-24`, `tests/startup.rs`

**Interfaces:**
- Produces `pub enum Route { Version, Run, Play, Python, Launcher }` in `startup.rs`: `--run` → `Run`, `--play` → `Play`, `--script` and `--install-update` → `Python` (they still count first, as today).
- Produces `pub struct RunContext<'a> { pub port_dir: &'a Path, pub env: IndexMap<String, String>, pub args: Vec<OsString>, pub out: &'a mut dyn Write }` and `pub fn run(context: RunContext<'_>) -> i32` in `src/runner/mod.rs`, returning 1 with the line `spartanport-probe[.exe] not found: build it with build.sh or use a packaged build.` until Task 15 fills it (Task 10 adds `find_executable`).
- `main.rs`: `Route::Run` → `std::process::exit(runner::run(context))` with `env` = `environment_of(std::env::vars_os())`, `args` = everything after `--run`; `Route::Play` → `runner::play(...)` (Task 15; until then it routes like `Run`).

- [ ] **Step 1: Write the failing tests**: `tests/startup.rs::run_and_play_are_their_own_routes` (`route(&["--run"]) == Route::Run`, `route(&["--run", "--timeout", "5"]) == Route::Run`, `route(&["--play"]) == Route::Play`, `route(&["--script", "x.py"]) == Route::Python`, `route(&["--install-update", "--run"]) == Route::Python`); `tests/startup.rs::run_without_a_probe_fails_with_the_python_message` (spawn `LAUNCHER --run` with `SPARTAN_PORT_DIR` at an empty temp folder: exit code 1, stdout contains `spartanport-probe` and `not found: build it with build.sh`); `tests/core_boundary.rs::core_never_mentions_iced` walks `src/runner` too (rename to `core_and_runner_never_mention_iced`).
- [ ] **Step 2: Run** `cargo test --test startup --test core_boundary` → FAIL (no `Route::Run`, no module).
- [ ] **Step 3: Implement** the routes, the module with `run` returning the message and 1, the `main.rs` branch.
- [ ] **Step 4: Run** → PASS; `cargo test` whole suite still green.
- [ ] **Step 5: Commit** `Feat(Launcher): route --run and --play to the runner`

---

### Task 2: Fixture generator (Python, one-off)

**Files:**
- Create: `tools/runner_fixtures.py`, `tests/test_runner_generators.py`, `launcher-rust/fixtures/runner/<case>/…`
- Modify: `.gitattributes` (`launcher-rust/fixtures/runner/** text eol=lf` for `.json`, `.xml`, `.ini`; `*.bin`, `*.dds`, `*.png`, `*.WAD` binary), `README.md` "Rust launcher" test section (one sentence naming the generator)

**Interfaces:**
- Produces the cases of spec §7.2, each folder holding `input/` and `expected/`:
  - `prepare`: `input/eboot.bin` (the synthetic SELF `tests/test_prepare.py` builds, with `sce_module/libc.prx` and `param.sfo`), `expected/{boot.bin, eboot.elf, entry.bin, analysis.json}`.
  - `link`: `input/` with eboot, `libc.prx`, `libSceFios2.prx` from `tests/test_probe.package` (the same packages `test_link_libc.py::LinkTests` use), `expected/{boot-libc.bin, libc-link.json, boot-linked.bin, link.json}`.
  - `isa`: `input/boot-linked.bin` = `tests/data/isa_fixture.bin`, `expected/isa-rewrite.bin`, `expected/sites.json` (`eboot_6e72_sites.json`'s shape: offset, kind, registers).
  - `content`: `input/param.sfo` synthetic; `expected/full/{content.bin, content-profile.json}`, `expected/trial/…`.
  - `patches`: `input/eboot.elf` synthetic with the signatures of `tests/test_patches.py::SignatureTests` planted, `input/patches.xml` (the small XML those tests use) and `input/mod.xml`; `expected/{patches-merged.xml, patches.bin, patch_status.json}` for the names those tests select.
  - `graphics`: `input/spartanport.ini`, `input/graphics_state.json` for the three states of `test_unconfirmed_restart_is_tried_once_then_reverted`; `expected/<state>/{spartanport.ini, graphics_state.json, environment.json}`.
  - `dxt`: `input/<name>.png` for `gradient`, `edges`, `alpha`, `noise`, `flat` (deterministic, 64 × 64 RGBA, drawn by the generator with a fixed seed); `expected/<name>.dxt1`, `expected/<name>.dxt5` = the concatenation of Pillow's 4-row strips exactly as `glyphs.encode_region` makes them (`Image.save(buffer, 'DDS', pixel_format=…)`, DDS header stripped).
  - `icon`: `expected/window_icon.bmp` from `launcher/spartanport.png`.
- Produces `tests/test_runner_generators.py` (unittest, run from `tests/`): every case folder has its files; `dxt` strips are a multiple of 8 (DXT1) and 16 (DXT5) bytes; `content.bin` is 28 bytes.

- [ ] **Step 1: Write the failing test** `tests/test_runner_generators.py::RunnerFixtures::test_every_case_has_its_files` (the list above).
- [ ] **Step 2: Run** `python -m unittest discover -s tests -p "test_runner_generators.py"` → FAIL (folders missing).
- [ ] **Step 3: Write `tools/runner_fixtures.py`**: imports `scripts/` and the builders of `tests/test_prepare.py`, `tests/test_probe.py`, `tests/test_patches.py` (import the test modules; do not copy their code); each case writes its `input/` then runs the Python module and copies the outputs into `expected/`; the temporary data folder appears in no expected file (replace with `<DATA_DIR>` as `tools/launcher_fixtures.py` does).
- [ ] **Step 4: Run the generator, then the test**: `python tools/runner_fixtures.py && python -m unittest discover -s tests -p "test_runner_generators.py"` → PASS. Check `git status` shows only the fixture folders, `.gitattributes`, the tool and the test.
- [ ] **Step 5: Commit** `Test(Launcher): generate the runner parity fixtures`

---

### Task 3: SELF loader (`prepare`)

**Files:**
- Create: `src/runner/prepare.rs`
- Test: inline `#[cfg(test)]`, `tests/runner_fixtures.rs` (new; shared helpers `fixture_dir(case)`, `expected(case, name) -> Vec<u8>`)

**Interfaces:**
- Produces `pub enum PrepareError` (thiserror: `Truncated`, `NotX86_64`, `BigEndian`, `BadProgramHeader`, `ImageTooLarge`, `Encrypted`, `MissingSegment(&'static str)`, `EmptyFile(PathBuf)`, `Io`), with messages equal to the Python exception texts the tests assert.
- `pub struct SelfInfo { pub elf: Vec<u8>, pub entry: u64, pub segments: Vec<Segment> }`, `pub struct Segment { pub offset: u64, pub vaddr: u64, pub filesz: u64, pub memsz: u64, pub flags: u32 }`.
- `pub fn parse_self(data: &[u8]) -> Result<SelfInfo, PrepareError>`, `pub fn read_self(path: &Path) -> Result<SelfInfo, PrepareError>`, `pub fn nid(name: &str) -> String` (the 11-character base64 of the SHA-1 as `prepare.nid`), `pub fn inspect_libc(path: &Path) -> Result<LibcReport, PrepareError>` (`_init_env` must be `RET`), `pub fn prepare(game: &Path, out: &Path) -> Result<Report, PrepareError>` writing `boot.bin`, `eboot.elf`, `entry.bin`, `analysis.json` (UTF-8, non-ASCII titles kept), `pub struct Report` mirroring `analysis.json`.

- [ ] **Step 1: Write the failing tests**: port the 13 of `tests/test_prepare.py::SelfTests` by name (`unpacked_elf_has_the_same_headers_and_payload_as_self`, `unpacked_elf_rejects_truncated_segments`, `…_rejects_other_architectures_and_byte_orders`, `…_rejects_truncated_program_headers`, `…_checks_image_limit_before_allocating`, `libc_ret_contract_is_verified_from_symbol_and_code`, `report_with_non_latin_title_is_written_as_utf8`, `file_offset_is_not_virtual_address`, `truncated_payload_rejected`, `encrypted_or_compressed_payload_rejected`, `missing_required_segment_rejected`, `bad_program_header_index_rejected`, `empty_game_files_are_named`), with the same synthetic SELF bytes (a `fn synthetic_self(...)` test helper that does what the Python builder does) and the same expected error texts; `tests/runner_fixtures.rs::prepare_matches_python` compares the four files of case `prepare` byte for byte.
- [ ] **Step 2: Run** `cargo test runner::prepare --test runner_fixtures` → FAIL.
- [ ] **Step 3: Implement** `src/runner/prepare.rs` with checked slicing everywhere (`get(..)`), `SFO_LIMIT = 1 << 20` through `core/sfo.rs`.
- [ ] **Step 4: Run** → PASS.
- [ ] **Step 5: Commit** `Feat(Launcher): add the SELF loader`

---

### Task 4: Linkers (`link_libc`, `link_modules`)

**Files:**
- Create: `src/runner/link_libc.rs`, `src/runner/link_modules.rs`
- Test: inline, `tests/runner_fixtures.rs`

**Interfaces:**
- `link_libc`: `pub fn encode_id(value: u64) -> String`, `pub struct Module { pub name: String, pub sha256: [u8; 32], pub exports: Vec<Export>, pub image: Vec<u8>, pub segments: Vec<Segment> }`, `pub fn module(path: &Path) -> Result<Module, LinkError>` (reads the file once), `pub fn link(game: &Path, out: &Path) -> Result<LinkReport, LinkError>` → `boot-libc.bin` (`G3PROBE4`), `libc-link.json`.
- `link_modules`: `pub const DEFAULT_MODULES: [&str; 2] = ["libc.prx", "libSceFios2.prx"]`, `pub const FS_LOAD: [u8; 9]` = `64 48 8b 04 25 00 00 00 00`, `pub fn canonical(identity: &str, fallback: Option<&str>) -> String` (`NID#lib`), `pub fn patch_fs_loads(image: &mut [u8], ph: &[Segment], base: u64) -> usize`, `pub fn link(game: &Path, out: &Path, modules: &[&str]) -> Result<LinkReport, LinkError>` → `boot-linked.bin` (`G3PROBE5`), `link.json`.
- `pub enum LinkError` with the Python messages (`unsupported TLS symbol`, `relocation outside segments`, `same NID, wrong library version`…).

- [ ] **Step 1: Write the failing tests**: port the 9 of `tests/test_link_libc.py` (`module_accepts_unpacked_elf_with_the_same_exports`, `module_reads_the_file_once_and_reports_its_hash`, `different_local_ids_bind_same_identity`, `same_nid_wrong_library_version_does_not_bind`, `tls_module_id_is_literal_not_image_pointer`, `unsupported_tls_symbol_rejected`, `relocation_outside_segments_rejected`, `fs_thread_pointer_loads_are_rewritten_to_gs`, `fs_loads_outside_executable_segments_are_kept`) and the 2 of `tests/test_link_modules.py`, building the packages with a Rust port of `tests/test_probe.py::package` in `tests/common/packages.rs`; `tests/runner_fixtures.rs::link_matches_python` compares the four files of case `link`.
- [ ] **Step 2: Run** → FAIL. **Step 3: Implement** both modules. **Step 4: Run** → PASS.
- [ ] **Step 5: Commit** `Feat(Launcher): link the eboot with its modules`

---

### Task 5: ISA rewrite table

**Files:**
- Create: `src/runner/isa_rewrite.rs`
- Modify: `Cargo.toml` (`iced-x86 = { version = "=1.21.0", default-features = false, features = ["std", "decoder"] }`)

**Interfaces:**
- `pub enum Kind { MovbeLoad, MovbeStore, Lzcnt, Tzcnt, Bmi1(Bmi1Op), F16c(F16cOp) }` with the Python `KIND_*` numbering as `as u8`; `pub const NO_REG: u8 = 0xFF`.
- `pub struct Site` mirroring `isa_rewrite.Site` (offset, length, kind, registers, displacement `i64`, original bytes `[u8; 15]`, relocated next instruction).
- `pub fn scan_segment(code: &[u8], address: u64) -> Vec<Site>`, `pub fn read_boot_linked(data: &[u8]) -> Result<(Vec<Segment>, Vec<u8>), IsaError>`, `pub fn encode_table(image: &[u8], sites: &[Site]) -> Vec<u8>` (header `<8sQ32sII8x`: magic `G3ISA1`, image size, SHA-256, sites, unsupported; records `<QBBBBBBBBBBBB4xq15sB15sx`, 64 bytes, sorted by offset), `pub fn summary(sites: &[Site]) -> String`, `pub fn rewrite(boot_linked: &Path, out: &Path) -> Result<Vec<Site>, IsaError>` (the table is written even with zero sites, as the Python, which exits 0 and lets `run.py` mount it; an unreadable input is the only error).

- [ ] **Step 1: Write the failing tests**: port the 19 of `tests/test_isa_rewrite.py` except `test_main_without_iced_x86` (dropped: Python-only) and `test_fixture_file_is_current` (becomes `tests/runner_fixtures.rs::isa_table_matches_python`: `rewrite` over `fixtures/runner/isa/input/boot-linked.bin` equals `expected/isa-rewrite.bin`, and the sites equal `expected/sites.json`). The 4 of `tests/test_run_isa.py` belong to Task 10.
- [ ] **Step 2: Run** `cargo test runner::isa_rewrite --test runner_fixtures` → FAIL.
- [ ] **Step 3: Implement** with `iced_x86::Decoder` (64-bit, `DecoderOptions::NONE`), one `Instruction` at a time, the same kind classification as `kind_of`/`site_of`/`is_plain`.
- [ ] **Step 4: Run** → PASS.
- [ ] **Step 5: Commit** `Feat(Launcher): find the ISA rewrite sites`

---

### Task 6: Content profile

**Files:**
- Create: `src/runner/content_profile.rs`

**Interfaces:**
- `pub enum Sku { Full, Trial }` (`full` = 3, `trial` = 1 in the file), `pub fn prepare(game: &Path, out: &Path, sku: Sku) -> Result<Profile, ContentError>` → `content.bin` (28 bytes `G3CONT01`), `content-profile.json`; `pub struct Profile` with the SFO parameters it holds.

- [ ] **Step 1: Write the failing tests**: port `tests/test_content.py::ProfileTests` (`sfo_parameters_and_explicit_trial_profile`, `invalid_sfo_parameter_rejected`); `tests/runner_fixtures.rs::content_matches_python` (both SKUs). `ContentTests` (3) need the native `content-test`: rows `dropped: no Rust counterpart — the C runtime's own test` in the parity map (Task 17).
- [ ] **Step 2–4:** FAIL → implement → PASS.
- [ ] **Step 5: Commit** `Feat(Launcher): write the content profile`

---

### Task 7: Patch compiler

**Files:**
- Modify: `src/core/patches.rs` (keep the read side as it is; add the compile side below)
- Test: inline, `tests/runner_fixtures.rs`, dump-gated tests in `tests/dump.rs` (new; every test starts with `let Some(game) = dump() else { return };` where `dump()` reads `SPARTAN_GAME_DIR` and checks `eboot.bin`)

**Interfaces:**
- `pub fn game_app_version(game: &Path) -> String`, `pub fn eboot_segments(elf: &[u8]) -> Vec<Segment>`, `pub fn eboot_image(elf: &[u8]) -> Vec<u8>`, `pub fn parse_pattern(text: &str) -> Pattern`, `pub fn find_pattern(image: &[u8], pattern: &Pattern, limit: usize) -> Vec<u64>`, `pub fn locate(image: &[u8], element: roxmltree::Node, prefix: &str) -> Option<u64>`, `pub fn resolve_targets(...)`, `pub fn fill(value: &str, at: u64, targets: &Targets) -> Vec<u8>`, `pub fn encode(line: &str) -> Vec<u8>`, `pub struct Write { pub address: u64, pub bytes: Vec<u8> }`, `pub fn compile_patches(xml: &Path, names: &[String], app_version: &str, segments: &[Segment], image: Option<&[u8]>) -> Result<(Vec<Write>, Vec<String>), PatchError>` (writes, skipped), `pub fn game_sfo`, `game_title_id`, `game_profile(title_id, title) -> GameProfile` (the `GameProfile` of `core/game.rs`), `game_profile_for`, `eboot_sha256`, `game_patchable`, `available_patches`, `selected_patches(xml, app_version, extra: &[String], only: bool) -> Vec<String>`, `merge_patch_files(base: &Path, extras: &[PathBuf], out: &Path) -> io::Result<()>`, `default_enabled(xmls, app_version) -> Vec<String>`, `patch_requirements(xml, names, app_version) -> Requirements` (DMEM MB, VBlank Hz from the notes), `external_selection`, `compile_external`, `write_patches(out: &Path, writes: &[Write]) -> io::Result<()>` (`G3PATCH2`), `pub fn write_status(out: &Path, status: &PatchStatus)`; `PatchStatus` gains `engine_fps`, `dmem_mb`, `vblank_hz`.

- [ ] **Step 1: Write the failing tests**: port the 30 of `tests/test_patches.py` by name; the ones that read the real eboot (`signatures_match_the_full_update_eboot`, `spartanport_signatures_in_the_reference_and_a_moved_build`, `spartanport_patches_compile`, `spartanport_startup_black_screen`, `spartanport_skip_intro_targets`, `spartanport_skip_intro_original_bytes`, `144_frame_rate_patch_has_the_reference_sites_and_value`, `every_resolution_patch_halves_the_output` if it needs the image) go to `tests/dump.rs`; `tests/runner_fixtures.rs::patches_match_python` compares the three files of case `patches`; `tests/dump.rs::real_patches_bin_matches_python` compares `patches.bin` compiled from the dump with the one `python scripts/patches.py` writes into a temp folder when `python` is on `PATH` (skip otherwise).
- [ ] **Step 2–4:** FAIL → implement → PASS (`cargo test`; then `SPARTAN_GAME_DIR=… cargo test --test dump` locally).
- [ ] **Step 5: Commit** `Feat(Launcher): compile the game patches`

---

### Task 8: Mods overlay, junctions and the update view

**Files:**
- Create: `src/runner/game_update.rs`
- Modify: `src/core/mods.rs`, `src/core/platform/windows.rs` (+ `src/core/platform/unix.rs`), `Cargo.toml` (`windows-sys` features `Win32_System_Ioctl`, `Win32_Storage_FileSystem`)

**Interfaces:**
- `game_update` (port of `scripts/game_update.py`, 76 lines, on top of `make_link`/`is_link`/`remove_link`): `pub fn overlay(source: &Path, dest: &Path) -> io::Result<()>` (case-insensitive names; a linked folder both have becomes a real folder of links), `pub fn remove(path: &Path) -> io::Result<()>` (never follows links into the dumps), `pub fn create_view(base: &Path, update: &Path, out: &Path) -> Result<PathBuf, UpdateViewError>` (temp folder `game-update-XXXX` under `out`; errors with the Python texts `Game and update are not the same title: {} / {}`, `Choose two separate folders for the game and its update`, `The update folder needs eboot.bin and sce_sys/param.sfo`; `game_title_id` from `core/patches.rs`), `pub fn remove_view(path: &Path) -> io::Result<()>`.
- `pub fn enabled_patch_files(root: &Path, config: &Path, enabled: bool, game: Option<&Path>) -> Vec<PathBuf>`.
- `pub enum LinkKind { Symlink, Junction, HardLink, Mirror }`, `pub fn make_link(link: &Path, target: &Path) -> io::Result<LinkKind>` (the Python order and criteria: symlink, then junction for folders on Windows, then hard link for files, then a mirrored copy; one decision cached per overlay as `LINKED`), `pub fn symlinks_work(dir: &Path) -> bool`, `pub fn is_link(path: &Path) -> bool`, `pub fn link_target(path: &Path) -> Option<PathBuf>`, `pub fn remove_link(path: &Path) -> io::Result<()>`, `pub fn expand(dir: &Path) -> Vec<PathBuf>`, `pub struct Overlay { pub root: PathBuf, owned: bool }` with `Overlay::remove(self)`, `pub fn build_overlay(game: &Path, out: &Path, mods: &[PathBuf]) -> Result<Overlay, ModsError>` (returns the game folder itself as a non-owned overlay when `mods` is empty, as `build_overlay` prints the game path).
- `platform::windows::create_junction(link: &Path, target: &Path) -> io::Result<()>` with `DeviceIoControl(FSCTL_SET_REPARSE_POINT)` and a mount-point `REPARSE_DATA_BUFFER`; `platform::unix::create_junction` returns `ErrorKind::Unsupported`.

- [ ] **Step 1: Write the failing tests**: port the `tests/test_mods.py` cases the Rust lacks (`merge_replacement_new_file_and_directory_listing`, `a_network_share_without_symlinks_or_junctions_still_overlays`, `last_mod_wins_and_reordering_changes_winner`, `no_mod_returns_original_game`, `directory_conflict_does_not_modify_base`, `mod_symlinks_rejected`, `executable_replacement_rejected`, `other_case_replaces_the_game_file`, `enabled_patch_files_follow_the_selection`, `invalid_profile_fails`, `GameLayout::exec_and_language_folders_replace_game_files`, `region_folders_are_used_for_discovery_and_overlay`, `learning_game_folders_does_not_allow_system_replacements`), each strategy forced in turn (`make_link` takes a `&dyn Fn(LinkKind) -> bool` allowed-set in tests through `build_overlay_with`), plus `a_stale_overlay_is_removed_before_a_new_one` (Review Focus: an existing `out/mod-game-old/` with a junction inside is removed without touching its target); port the 6 of `tests/test_game_update.py::GameUpdateTests` by name (`update_files_replace_base_files_and_the_rest_stays`, `folders_the_update_leaves_alone_are_linked_whole`, `removing_the_view_leaves_the_dumps_whole`, `dumps_are_not_modified`, `a_network_share_without_junctions_mirrors_the_tree`, `different_titles_and_nested_folders_are_refused`) in `src/runner/game_update.rs`.
- [ ] **Step 2–4:** FAIL → implement → PASS on Windows and Linux (junction tests `#[cfg(windows)]`).
- [ ] **Step 5: Commit** `Feat(Launcher): build the mods overlay`

---

### Task 9: Graphics options

**Files:**
- Create: `src/runner/graphics_options.rs`
- Modify: `src/core/options.rs` (only if a table is missing: `ENGINE_FPS = [60, 100, 120, 144, 180, 240]`, `DEFAULTS`, `STEAM_DECK_PRODUCTS`, `STEAM_DECK_RESOLUTION = "1280x800"`, `RESOLUTION_PREFIX`, `FPS_PREFIX`, `TEXTURE_FIX`)

**Interfaces:**
- `pub fn steam_deck(dmi: &Path) -> bool`, `pub fn default_render_resolution(dmi: &Path) -> &'static str`, `pub fn write_keys(path: &Path, updates: &IndexMap<String, String>) -> io::Result<()>` (through `core::ini::save_ini`), `pub fn startup_options(ini: &IniFile, extra: &[String]) -> Options`, `pub struct Options` (render_resolution, engine_fps, deferred_readback, stale_readback, async_shaders, restart_unconfirmed), `pub struct Prepared { pub options: Options, pub reverted: bool, pub message: Option<String> }`, `pub fn prepare(config: &Path, state_path: &Path, extra: &[String]) -> Prepared` (writes `graphics_state.json`; the revert message is the Python's `Graphics: the last restart never reached the game…` line verbatim), `pub fn patch_names(names: &[String], options: &Options, texture_fix_available: bool) -> Vec<String>`, `pub fn restrict_to_available(names: &[String], available: &[String]) -> Vec<String>`, `pub fn applied_engine_fps(names: &[String]) -> u32`, `pub fn environment(options: &Options) -> IndexMap<String, String>`.

- [ ] **Step 1: Write the failing tests**: port the 28 of `tests/test_graphics_options.py` (the five `AspectRatio` and `resolution_labels_show_the_size_and_aspect_ratio` are already `src/core/options.rs::tests`: reference them in the parity map, do not duplicate); `tests/runner_fixtures.rs::graphics_matches_python` over the three states of case `graphics`.
- [ ] **Step 2–4:** FAIL → implement → PASS.
- [ ] **Step 5: Commit** `Feat(Launcher): apply the graphics start-up options`

---

### Task 10: Guest memory, window icon and the run helpers

**Files:**
- Create: `src/runner/guest_memory.rs`, `src/runner/window_icon.rs`
- Modify: `src/runner/mod.rs`, `Cargo.toml` (`image` features `+ "bmp"`)

**Interfaces:**
- `guest_memory`: `pub const DMEM_RETAIL_MB: u64 = 5056`, `FLEX_SPAN_MB = 1024`, `DMEM_DEFAULT_MB = 7104`, `HEADROOM_MB` (device 2048, cached 4096, host 4096), `pub fn pool_mb(dmem_mb: u64) -> u64`, `pub fn physical_memory_mb() -> Option<u64>` (`GlobalMemoryStatusEx` on Windows; `/proc/meminfo` `MemTotal` elsewhere), `pub fn memory_warning(dmem_mb: u64, physical_mb: Option<u64>, stream_memory: &str) -> Option<String>` (the Python's text).
- `window_icon`: `pub fn write(png: &Path, out: &Path) -> Option<PathBuf>` (64 × 64 RGBA, `image::imageops::FilterType::Lanczos3`, `out/window_icon.bmp`; `None` on any error).
- `runner/mod.rs`: `pub fn parse_cpu_features(output: &str) -> Vec<String>`, `pub fn isa_rewrite_wanted(missing: &[String], env: &dyn Fn(&str) -> Option<String>) -> bool` (`SPARTAN_ISA_REWRITE`: `1` forces, `0` disables, else follows `missing`), `pub fn launch_prefix(env: &dyn Fn(&str) -> Option<String>) -> Vec<String>` (`gamemoderun` when `SPARTAN_GAMEMODE=1` and it is on `PATH`, Linux only), `pub fn find_executable(paths: &Paths, env: ...) -> Option<PathBuf>` (`SPARTAN_PROBE`, then `Paths::find_tool("spartanport-probe")`), `pub fn wait_for_pid(pid: u32, limit: Duration) -> bool`.

- [ ] **Step 1: Write the failing tests**: port the 8 of `tests/test_guest_memory.py` and the 4 of `tests/test_run_isa.py`; `window_icon::tests::icon_is_64_by_64_bmp` (header and size against `fixtures/runner/icon/expected/window_icon.bmp`; pixels not compared); `wait_for_pid_returns_when_the_process_ends` (spawn `fake-game --exit 0 --lines 1 --delay-ms 300`, wait its pid: true before 10 s; a never-existing pid: true at once).
- [ ] **Step 2–4:** FAIL → implement → PASS.
- [ ] **Step 5: Commit** `Feat(Launcher): add the runner helpers`

---

### Task 11: DXT encoder (Pillow port)

**Files:**
- Create: `src/runner/dxt.rs`, `launcher-rust/assets/LICENSE-Pillow.txt` (HPND text and the file's origin: Pillow 12.3.0 `src/libImaging/BcnEncode.c`)

**Interfaces:**
- `pub fn encode_dxt1(rgba: &[u8], width: u32, height: u32) -> Vec<u8>` and `pub fn encode_dxt5(rgba: &[u8], width: u32, height: u32) -> Vec<u8>`: the bytes Pillow writes after the DDS header for `pixel_format` `DXT1`/`DXT5`, for any width and height (partial blocks padded as Pillow pads them).

- [ ] **Step 1: Write the failing tests**: `tests/runner_fixtures.rs::dxt1_blocks_match_pillow` and `dxt5_blocks_match_pillow`: for each PNG of case `dxt`, encode every 4-row strip as `glyphs.encode_region` does (`encode_dxt*` over the strip) and compare the concatenation byte for byte with `expected/<name>.dxt1` / `.dxt5`; `dxt::tests::odd_sizes_pad_like_pillow` (a 5 × 3 image against a strip the generator also stores as `expected/odd.dxt1`).
- [ ] **Step 2: Run** → FAIL.
- [ ] **Step 3: Implement** `dxt.rs` as a line-by-line port of `BcnEncode.c` (fetch the 12.3.0 file from the Pillow repository tag `12.3.0`; keep its block colour selection, its error metric and its rounding exactly, including the alpha block of BC3; name the helpers after the C ones).
- [ ] **Step 4: Run** → PASS; `cargo clippy` clean.
- [ ] **Step 5: Commit** `Feat(Launcher): port the DXT encoder of Pillow`

---

### Task 12: Glyphs

**Files:**
- Create: `src/runner/glyphs.rs`, `src/runner/draw.rs` (text and shapes with `ab_glyph` + `image`)
- Modify: `Cargo.toml` (`ab_glyph`), `launcher-rust/assets/` reads the fonts from the port's `launcher/` folder at run time (not embedded: the Python reads `launcher/Cinzel-Black.ttf` and `launcher/BodoniModa.ttf`)

**Interfaces:**
- `pub const VERSION: u32 = 11`, `pub const WADS: [&str; 3]`, `TEXTURES`, `KEYBOARD_TEXTURES`, `DEFAULT_MAP`, `DEFAULT_KEYS`, `STICK_KEYS`, `KEY_LABELS`, `POSITIONS`, `STYLES`, `ICONS`, `LYRE_*`, `SCALE = 6`, `INK`, as the Python tables.
- `pub struct Faces { pub south: String, pub east: String, pub west: String, pub north: String }` (the letters the connected controller prints on its face buttons, from the probe's `Gamepad:` line), `pub struct Gamepad { pub kind: String, pub faces: Faces, pub name: String }`.
- `pub fn entries(data: &[u8]) -> Vec<Entry>` (`Entry { name, offset, size }`), `pub fn pad_buttons(pad_map: &str) -> IndexMap<String, String>`, `pub fn key_buttons(key_map: &str) -> IndexMap<String, String>`, `pub fn key_label(name: &str) -> String`, `pub fn glyph(host: &str, style: &str, faces: Option<&Faces>) -> String`, `pub fn parse_gamepad(output: &str) -> Option<Gamepad>`, `pub fn resolve(choice: &str, probe_output: &str) -> Resolved { style, faces }`, `pub fn render(...)`, `pub fn encode_region(data: &mut Vec<u8>, dds: usize, end: usize, image: &RgbaImage, fourcc: FourCc, region: Rect, original: Option<&[u8]>) -> Result<(), GlyphError>`, `pub fn encode(image: &RgbaImage, fourcc: FourCc, length: usize) -> Result<Vec<u8>, GlyphError>`, `pub fn textures(style: &str) -> Vec<&'static str>`, `pub fn patch(data: &[u8], style: &str, faces: Option<&Faces>, pad_map: &str, key_map: &str) -> Result<Option<Vec<u8>>, GlyphError>`, `pub fn build(game: &Path, out: &Path, style: &str, faces: Option<&Faces>, pad_map: &str, key_map: &str) -> Result<IndexMap<String, PathBuf>, GlyphError>` (guest path → copy; `stamp.json` with VERSION, source path, size, mtime, style, faces, maps).
- `pub fn glyph_mounts(env: &IndexMap<String, String>, probe: &Path, game: &Path, out: &Path, run_probe: &dyn Fn(&[&str], Duration) -> Option<String>) -> Vec<String>` in `runner/mod.rs` (`--mount` arguments; the probe call injected for tests).

- [ ] **Step 1: Write the failing tests**: port the 37 of `tests/test_glyphs.py` by name; the pixel assertions of `GlyphPatch` become structural (`patch_keeps_every_size_and_only_touches_glyph_pixels` checks sizes, formats, mip counts and that untouched blocks are byte-equal) plus `painted_textures_match_pillow_within_tolerance` (PSNR ≥ 40 dB against `fixtures/runner/glyphs/expected/<texture>.png`, which Task 2's generator gains now: the Pillow drawing of each texture for the `xbox`, `nintendo` and `keyboard` styles with the default maps); `RunGlyphs` (6) against `glyph_mounts` with a closure standing in for the probe (`auto_asks_the_probe_and_mounts_the_copies`, `a_chosen_style_does_not_ask_the_probe`, `keyboard_passes_the_key_map`, `auto_without_a_gamepad_shows_the_keys`, `a_failure_starts_the_game_with_its_own_glyphs`, `off_runs_nothing_and_removes_old_copies`); `tests/dump.rs::real_glyph_wads_match_python_outside_the_painted_blocks`.
- [ ] **Step 2–4:** FAIL → implement (`draw.rs` first: `text`, `ellipse`, `rectangle`, `line`, `resize_lanczos` on `RgbaImage`) → PASS.
- [ ] **Step 5: Commit** `Feat(Launcher): repaint the button glyphs`

---

### Task 13: Pause menu

**Files:**
- Create: `src/runner/pause_menu.rs`

**Interfaces:**
- `pub const VERSION: u32 = 14`, `WAD = "EXEC/WAD/PS4_LE/R_UI.WAD"`, `LABELS` (the 10 languages), `LANGUAGE_ALIASES`, `EXIT_CALL = [0x13, 0x0f, 0xff, 0x43]`, `ROW_SIZE = 0x3c`, `REPLY_TEMPLATE`, `MARK = 5`, `SHOW_BLOCK`, `YES_BLOCK`, `LOCAL`, `SHOW_PREFIX`, `YES_PREFIX`, `LENGTH` (opcode lengths: 5 for `00 01 47 4a`, 3 for `02..=18`, else 1, up to `0x4b`).
- `pub struct Label` (the per-language strings of `LABELS`: exit row, options row, health bar row, the cheat and multiplier rows, the quit question), `pub struct Op { pub offset: usize, pub opcode: u8, pub operand: u64 }`, `pub struct Spans` (the byte ranges of the script's header, state records and code blocks that `patch` moves; `Script::spans()`).
- `pub fn resolve_label(language: Option<&str>, env: &dyn Fn(&str) -> Option<String>) -> &'static Label`, `pub fn be(value: u64, size: usize) -> Vec<u8>`, `pub fn decode(data: &[u8], start: usize, end: usize) -> Vec<Op>`, `pub struct Script` (`Script::parse(data: &[u8]) -> Result<Script, MenuError>`, states, code blocks, `transition`, `retarget`, `row_code`, `row_record`, `moved_copy`, `failsafe_code`, `blank_line`), `pub fn display_health_bar(data: &mut Vec<u8>, spans: &mut Spans) -> Result<(), MenuError>`, `pub fn native_cheats(...)`, `pub fn native_multiplier_bars(...)`, `pub fn patch(data: &[u8], language: Option<&str>) -> Result<Vec<u8>, MenuError>`, `pub fn build(game: &Path, out: &Path, source: Option<&Path>, language: Option<&str>) -> Result<Option<PathBuf>, MenuError>` (stamp: VERSION, source path, size, mtime, language).
- `pub fn pause_menu_args(game: &Path, out: &Path, mounts: &[String], language: Option<&str>) -> Vec<String>` in `runner/mod.rs` (`--pause-menu <path>` or nothing; the source is the glyph copy of `R_UI.WAD` when mounted).

- [ ] **Step 1: Write the failing tests**: port `tests/test_pause_menu.py::Refusals` (2) and `Labels` (5) and the synthetic-script parts of `GameScript`; the 12 that open the real `R_UI.WAD` and the 9 of `tests/test_native_cheats.py` go to `tests/dump.rs` (`real_menu_wad_matches_python`: `patch` over the dump's `R_UI.WAD` equals the bytes `python scripts/pause_menu.py` writes, when `python` is on `PATH`; otherwise the structural assertions of those tests).
- [ ] **Step 2–4:** FAIL → implement → PASS; `SPARTAN_GAME_DIR=… cargo test --test dump` locally.
- [ ] **Step 5: Commit** `Feat(Launcher): rewrite the pause menu script`

---

### Task 14: Discord presence

**Files:**
- Create: `src/runner/discord.rs`

**Interfaces:**
- `pub const CLIENT_ID: &str = "1560048868677132431"`, `RETRY = 15 s`, `JOIN = 3 s`, `POLL = 100 ms`, `READ = 5 s`; `pub enum Opcode { Handshake = 0, Frame = 1, Close = 2, Ping = 3, Pong = 4 }`.
- `pub fn pack(opcode: Opcode, payload: &serde_json::Value) -> Vec<u8>`, `pub fn read_frame(transport: &mut dyn Transport, stop: &AtomicBool, timeout: Option<Duration>) -> io::Result<(Opcode, serde_json::Value)>`, `pub fn activity(started: u64) -> serde_json::Value` (only the start timestamp, as the Python), `pub fn set_activity(pid: u32, value: Option<serde_json::Value>) -> Vec<u8>`, `pub trait Transport { fn write(&mut self, data: &[u8]) -> io::Result<()>; fn read(&mut self, size: usize, timeout: Option<Duration>) -> io::Result<Vec<u8>>; fn close(&mut self); }`, `pub fn candidates(env: &dyn Fn(&str) -> Option<String>) -> Vec<PathBuf>`, `pub fn open_transport(...) -> Option<Box<dyn Transport>>`, `pub fn handshake(...)`, `pub fn send_activity(...)`, `pub struct Presence` with `Presence::new(pid: u32, env: &dyn Fn(&str) -> Option<String>) -> Presence` (no thread when `SPARTAN_DISCORD=0` or no client id) and `Presence::close(self)`.
- Windows pipe: `std::fs::OpenOptions::new().read(true).write(true).open(r"\\?\pipe\discord-ipc-N")`, reads on a worker thread with a channel for the timeout; Unix: `std::os::unix::net::UnixStream` with `set_read_timeout`.

- [ ] **Step 1: Write the failing tests**: port the 23 of `tests/test_discord_presence.py` except `LauncherSwitch` (3: launcher UI, already `tests/ui_pages.rs` rows in the parity map) with an in-memory `Transport` for `Framing`, `Activity`, `PresenceLifecycle`, `GameProcessPresence`; `SocketIntegration::real_socket_reconnect_ping_and_shutdown` with a `UnixListener` (`#[cfg(unix)]`); `WindowsPipeTransport` (2) with a named pipe server made through `windows-sys` `CreateNamedPipeW` in the test (`#[cfg(windows)]`); Review Focus: `a_silent_pipe_times_out_and_close_returns_in_time` (server accepts and never answers: `Presence::close` returns within 3 s, the game's wait was not delayed).
- [ ] **Step 2–4:** FAIL → implement → PASS.
- [ ] **Step 5: Commit** `Feat(Launcher): publish the Discord presence`

---

### Task 15: `run`, `--after`, `--play` and the fake probe

**Files:**
- Create: `src/bin/fake_probe.rs`, `tests/runner.rs`
- Modify: `src/runner/mod.rs` (`run`, `play`), `src/startup.rs`, `src/main.rs`, `Cargo.toml` (`[[bin]] fake-probe`)

**Interfaces:**
- `pub fn run(context: RunContext<'_>) -> i32`: the sixteen steps of spec §5, in order, calling Tasks 3–14; `pub fn restart_command(exe: &Path, pid: u32, args: &[OsString]) -> String` (Windows `list2cmdline`, Linux `shlex.join`); `pub fn play(context: RunContext<'_>, settings: &Settings, paths: &Paths) -> i32` (`attach_stdio`, `user/last_run.log`, the exit advice appended, `MessageBoxW` without a console on Windows; the game environment from `core::env::game_environment`).
- `fake-probe` (`src/bin/fake_probe.rs`): `--cpu-features` prints `CPU: missing <list from FAKE_PROBE_MISSING>` and exits with `FAKE_PROBE_CPU_EXIT` (default 0); `--gamepad` prints `FAKE_PROBE_GAMEPAD` or hangs when it is `hang`; `--displays` prints nothing; any other run prints its argv one per line, every variable named in `FAKE_PROBE_PRINT`, then, when `FAKE_PROBE_RESTART=1` and `SPARTAN_RESTART_COMMAND` is set, prints `Runtime: restarting through run.py`, starts that command (Windows `CreateProcess` of the string; Linux `sh -c`) and exits 0; else exits with `FAKE_PROBE_EXIT` (default 0).

- [ ] **Step 1: Write the failing tests** in `tests/runner.rs` with a temp port folder holding `run.py` (empty), `bin/spartanport-probe[.exe]` = a copy of `CARGO_BIN_EXE_fake-probe`, `launcher/spartanport.png`, `patches/CUSA01623.xml` (the repo's), and the synthetic game of `tests/runner_fixtures.rs` case `prepare` as `game/`: `run_writes_every_derived_file_and_starts_the_probe_with_the_python_command_line` (the `out/` files of Global Constraints exist; the probe's argv equals spec §5 step 16 for that folder; `SPARTAN_DERIVED` lists the derived keys; exit 0), `the_probes_exit_code_is_returned` (`FAKE_PROBE_EXIT=7` → 7, the `run_uses_overlay_propagates_exit_and_cleans_view` case: a mod overlay exists during the run and is gone after), `an_unsupported_cpu_stops_with_20`, `a_missing_eboot_fails_with_the_python_message`, `a_stale_isa_table_is_replaced` (Review Focus), `a_corrupt_patch_status_defaults_the_engine_fps` (Review Focus; `pub fn patch_environment(status: Option<&PatchStatus>) -> Vec<(String, String)>` in `runner/mod.rs`: `None`, or `patch_status(out)` over a folder whose `patch_status.json` holds `{`, gives `SPARTAN_ENGINE_FPS=60`, `SPARTAN_VBLANK_HZ=60` and no `SPARTAN_DMEM_MB`), `restart_command_quotes_spaces_and_quotes` (Review Focus; a temp exe path with a space and a `"` round-trips: on Windows parse it back with `CommandLineToArgvW`, on Linux with `sh -c 'printf %s\0 "$@"'`), `after_waits_for_the_old_process`, `a_restart_from_the_game_starts_the_exe_again` (`FAKE_PROBE_RESTART=1`: the second run's `--after` sees the first pid; both exit 0; the log has the restart line once), `play_writes_last_run_log_with_the_advice` (`FAKE_PROBE_PRINT` a guest memory line, exit 21: `last_run.log` ends with the Windows or Linux advice), `a_hanging_gamepad_probe_falls_back_to_keyboard_glyphs` (`FAKE_PROBE_GAMEPAD=hang`: the run still starts the probe, with keyboard glyphs; the test takes the 15 s of the real timeout), `an_update_folder_is_combined_into_a_view` (`SPARTAN_PATCH_DIR` at a second synthetic dump of the same title: the log has `Game and update combined from <folder>`, the probe's `--app0` is a `game-update-*` folder under `out/`, and that folder is gone after the exit).
- [ ] **Step 2: Run** `cargo test --test runner` → FAIL.
- [ ] **Step 3: Implement** `run`, `restart_command`, `play`, the fake probe; `startup::route` and `main.rs` call them.
- [ ] **Step 4: Run** → PASS; whole `cargo test` green on Windows; push for the Linux runner.
- [ ] **Step 5: Commit** `Feat(Launcher): run the game without Python`

---

### Task 16: PLAY through the executable, and the icon

**Files:**
- Modify: `src/core/launch.rs:53-62` (`run_py_command` → `run_command`), `src/ui/play.rs:300-336` (`start`), `src/core/settings.rs:103-120` (`migrate_portable_settings` stops copying `game_icon.ico`), `tests/ui_play.rs`, `tests/launch.rs`, `launcher-rust/README.md` (`SPARTANPORT_PYTHON` now only for the forwarded commands), `README.md`, `README.pt-BR.md`, `README.es.md` ("Rust launcher" paragraphs: PLAY no longer needs Python)

**Interfaces:**
- `pub fn run_command(exe: &Path, args: &[OsString]) -> Vec<String>` = `[exe, "--run", args…]`; `start` uses `std::env::current_exe()`; `python_command` stays for `forward_to_python`.
- `missing_port_files` (Task 3d of PR #959) keeps checking `run.py` until sub-project 4 removes it from the package, and gains `bin/spartanport-probe` only (it already does).

- [ ] **Step 1: Write the failing tests**: `tests/launch.rs::the_game_command_is_the_exe_with_run` (`run_command` shape); `tests/ui_play.rs::play_starts_the_launcher_exe_with_run` (with `launch_command` `None` and a port folder holding the fake probe as Task 15's, PLAY spawns the real `LAUNCHER --run`: the log shows the fake probe's argv line `--app0`, then `The game exited (code 0).`); `src/core/settings.rs::tests::portable_migration_copies_only_settings` (replaces `portable_migration_copies_once_and_never_overwrites`'s icon assertions: `game_icon.ico` is not copied).
- [ ] **Step 2–4:** FAIL → implement → PASS; `cargo test` whole suite; the READMEs updated.
- [ ] **Step 5: Commit** `Feat(Launcher): start the game through the exe`

---

### Task 17: Runner parity map and release gate

**Files:**
- Create: `docs/specs/2026-10-22-rust-runner-parity.md`, `tests/test_runner_parity_doc.py`
- Modify: `tests/test_launcher_parity_doc.py` (extract the checker into `tests/parity_doc.py` with `check(doc, files, this)`, both tests call it), `docs/specs/2026-10-22-rust-runner-design.md` §7 (the final counts), `launcher-rust/README.md` (how to run the dump-gated tests: `SPARTAN_GAME_DIR=<game> cargo test --test dump`)

**Interfaces:**
- The map covers `tests/test_prepare.py`, `test_link_libc.py`, `test_link_modules.py`, `test_isa_rewrite.py`, `test_run_isa.py`, `test_patches.py`, `test_mods.py`, `test_mod_conflicts.py`, `test_glyphs.py`, `test_content.py`, `test_guest_memory.py`, `test_graphics_options.py`, `test_save_backup.py`, `test_pause_menu.py`, `test_native_cheats.py`, `test_legacy.py`, `test_discord_presence.py`, `test_game_update.py`, with the launcher map's statuses and sections (Counts, Map, Known deviations, Partly covered, Gaps). Allowed drops: `tests/test_content.py::ContentTests` (C runtime), `test_isa_rewrite.py::ReadBootLinkedTests::test_main_without_iced_x86` (Python-only), `test_legacy.py::WindowsDesktopShortcut` rows that need PowerShell (already `core/legacy.rs` rows or gaps), `test_mods.py::ModTests::test_run_uses_overlay_propagates_exit_and_cleans_view` → `tests/runner.rs::the_probes_exit_code_is_returned`.

- [ ] **Step 1: Write the failing test** `tests/test_runner_parity_doc.py` (the checker over the new map; the map does not exist yet).
- [ ] **Step 2: Run** `python -m unittest discover -s tests -p "test_runner_parity_doc.py"` → FAIL (no row).
- [ ] **Step 3: Write the map**: one row per Python test naming the Rust test of Tasks 3–16; the Counts table; deviations (link_libc kept, glyph pixels by tolerance, Discord here, `game_icon.ico` gone); Gaps listed.
- [ ] **Step 4: Run** both parity tests and `python -m unittest discover -s tests -p "test_launcher_parity_doc.py"` → PASS; locally `SPARTAN_GAME_DIR=<game> cargo test` whole suite → PASS (record the date and the dump's `APP_VER` in the map's header).
- [ ] **Step 5: Commit** `Docs(Launcher): map the runner tests to Rust tests`

---

## Self-review notes

- Spec §3.3 (`--play`), §3.4 (PLAY), §3.5 (Discord), §3.7 (formats), §5 (steps), §6 (errors), §7 (tests) each map to a task above; §2 "Ícone" to Task 16; §8 "fica fora" has no task on purpose.
- Task order for execution: 1, 2, 3, 4, 10, 5, 6, 7, 8, 9, 11, 12, 13, 14, 15, 16, 17 (Task 10's helpers come before the table of Task 5; Task 11 before the glyphs).
- Task 2's generator gains the `glyphs` case when Task 12 starts (the painted textures); the executor of Task 12 reruns `python tools/runner_fixtures.py` and commits the new folder with the task.
