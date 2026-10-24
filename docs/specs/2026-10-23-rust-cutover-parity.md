# Rust cutover parity: Python tests → Rust tests

Every test method of the two Python test files whose code moved into the Rust launcher in the cutover
(spec `2026-10-23-rust-cutover-design.md` §6): `tests/test_steam_shortcut.py` (13 methods, the Add to
Steam shortcut and its art, now `src/core/steam.rs` and `src/core/steam_art.rs`) and
`tests/test_xvag_voice_swap.py` (13 methods, the voice swap tool, now `src/core/voice_swap.rs`), each
against the Rust test of `launcher-rust/` that covers its behaviour. `tests/test_cutover_parity_doc.py`
keeps this map honest through `tests/parity_doc.py`, the checker it shares with the launcher map
(`2026-10-21-rust-launcher-parity.md`) and the runner map (`2026-10-22-rust-runner-parity.md`): it finds
the methods by scanning those files, and fails when one has no row, a row has no status, or a named
Rust test does not exist. That checker is not itself a row.

The cutover also closes two rows of the older maps: `test_user_files_includes_settings_and_icon`
(launcher map, now `src/core/update.rs::tests::user_files_include_settings_and_icon`) and
`test_the_update_installer_does_not_migrate` (runner map, now
`tests/startup.rs::the_update_installer_does_not_migrate`); `--install-update` is native and `--script`
no longer exists. The Python launcher stays at 2.7.0; the crate and the package are 2.8.0.

Release gate: checked on 2026-10-23 at the end of the cutover work, on Windows, against the real game
dump `CUSA01623`, read only: `SPARTAN_GAME_DIR=<game> cargo test --no-fail-fast` in `launcher-rust/`,
after `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` (both clean).

Result: 924 tests passed, 0 failed, in one clean run with the dump set, after the final-review fixes.

Manual checks on the built package (CI zip and the user's Steam): pending, to be recorded here.

Status:

- `ported`: the Rust test is the port of the Python one (same cut, the same name without `test_`).
- `replaced by <test>`: Rust tests that cover the behaviour under a different cut (none in this map).
- `dropped: <reason>`: mechanics with no Rust equivalent (none in this map).
- `dropped: spec §<section> <reason>`: behaviour the spec replaced on purpose (none in this map).
- `dropped: no Rust counterpart — <what it tested>`: behaviour no Rust test covers (none in this map).

Rust tests are named `file::name` under `launcher-rust/`: `src/core/steam.rs::tests::name` for unit
tests (the `tests` segment is the module).

## Counts

| File | Rows | Ported | Replaced | Dropped (Python-only) | Dropped (spec) | Gaps |
|---|---|---|---|---|---|---|
| `tests/test_steam_shortcut.py` | 13 | 13 | 0 | 0 | 0 | 0 |
| `tests/test_xvag_voice_swap.py` | 13 | 13 | 0 | 0 | 0 | 0 |
| **Total** | **26** | **26** | **0** | **0** | **0** | **0** |

## Map

| Python test | Rust test (file::name) | Status |
|---|---|---|
| `tests/test_steam_shortcut.py::VdfTests::test_round_trip_keeps_strings_numbers_and_nested_maps` | `src/core/steam.rs::tests::round_trip_keeps_strings_numbers_and_nested_maps` | ported |
| `tests/test_steam_shortcut.py::VdfTests::test_empty_file_of_steam` | `src/core/steam.rs::tests::empty_file_of_steam` | ported |
| `tests/test_steam_shortcut.py::VdfTests::test_app_id_is_the_crc_of_exe_and_name_with_the_top_bit` | `src/core/steam.rs::tests::app_id_is_the_crc_of_exe_and_name_with_the_top_bit` | ported |
| `tests/test_steam_shortcut.py::PlatformTests::test_windows_looks_in_the_registry_folder_then_the_usual_ones` | `src/core/steam.rs::tests::windows_looks_in_the_registry_folder_then_the_usual_ones` | ported |
| `tests/test_steam_shortcut.py::PlatformTests::test_steam_is_running_by_the_process_list` | `src/core/steam.rs::tests::steam_is_running_by_the_process_list` | ported |
| `tests/test_steam_shortcut.py::LogoTests::test_only_a_large_icon_with_a_transparent_background_is_an_emblem` | `src/core/steam_art.rs::tests::only_a_large_icon_with_a_transparent_background_is_an_emblem` | ported |
| `tests/test_steam_shortcut.py::LogoTests::test_the_logo_without_an_emblem_is_just_the_title_lines` | `src/core/steam_art.rs::tests::the_logo_without_an_emblem_is_just_the_title_lines` | ported |
| `tests/test_steam_shortcut.py::AddToSteamTests::test_every_account_gets_the_shortcut_and_the_five_pictures` | `src/core/steam.rs::tests::every_account_gets_the_shortcut_and_the_five_pictures` | ported |
| `tests/test_steam_shortcut.py::AddToSteamTests::test_other_shortcuts_stay_and_a_backup_is_made` | `src/core/steam.rs::tests::other_shortcuts_stay_and_a_backup_is_made` | ported |
| `tests/test_steam_shortcut.py::AddToSteamTests::test_adding_again_updates_the_same_shortcut` | `src/core/steam.rs::tests::adding_again_updates_the_same_shortcut` | ported |
| `tests/test_steam_shortcut.py::AddToSteamTests::test_a_shortcut_from_before_the_rename_is_updated_not_doubled` | `src/core/steam.rs::tests::a_shortcut_from_before_the_rename_is_updated_not_doubled` | ported |
| `tests/test_steam_shortcut.py::AddToSteamTests::test_steam_open_and_steam_missing_are_refused` | `src/core/steam.rs::tests::steam_open_and_steam_missing_are_refused` | ported |
| `tests/test_steam_shortcut.py::AddToSteamTests::test_a_game_folder_without_the_background_is_refused` | `src/core/steam.rs::tests::a_game_folder_without_the_background_is_refused` | ported |
| `tests/test_xvag_voice_swap.py::SwapVoiceTests::test_only_the_frames_change` | `src/core/voice_swap.rs::tests::only_the_frames_change` | ported |
| `tests/test_xvag_voice_swap.py::SwapVoiceTests::test_padding_after_the_frames_is_kept` | `src/core/voice_swap.rs::tests::padding_after_the_frames_is_kept` | ported |
| `tests/test_xvag_voice_swap.py::SwapVoiceTests::test_an_xvag_recording_works_like_a_vagp_one` | `src/core/voice_swap.rs::tests::an_xvag_recording_works_like_a_vagp_one` | ported |
| `tests/test_xvag_voice_swap.py::SwapVoiceTests::test_truncated_xvag_recordings_are_refused` | `src/core/voice_swap.rs::tests::truncated_xvag_recordings_are_refused` | ported |
| `tests/test_xvag_voice_swap.py::SwapVoiceTests::test_xvag_payload_cannot_overlap_the_header` | `src/core/voice_swap.rs::tests::xvag_payload_cannot_overlap_the_header` | ported |
| `tests/test_xvag_voice_swap.py::SwapVoiceTests::test_what_does_not_fit_is_refused_with_a_reason` | `src/core/voice_swap.rs::tests::what_does_not_fit_is_refused_with_a_reason` | ported |
| `tests/test_xvag_voice_swap.py::RunTests::test_writes_the_fitting_lines_to_the_update_folder_and_leaves_the_game_alone` | `src/core/voice_swap.rs::tests::writes_the_fitting_lines_to_the_update_folder_and_leaves_the_game_alone` | ported |
| `tests/test_xvag_voice_swap.py::RunTests::test_dry_run_writes_nothing` | `src/core/voice_swap.rs::tests::dry_run_writes_nothing` | ported |
| `tests/test_xvag_voice_swap.py::RunTests::test_reference_leaves_unchanged_recordings_alone` | `src/core/voice_swap.rs::tests::reference_leaves_unchanged_recordings_alone` | ported |
| `tests/test_xvag_voice_swap.py::RunTests::test_only_limits_the_lines` | `src/core/voice_swap.rs::tests::only_limits_the_lines` | ported |
| `tests/test_xvag_voice_swap.py::RunTests::test_the_update_folder_cannot_be_the_game_or_inside_it` | `src/core/voice_swap.rs::tests::the_update_folder_cannot_be_the_game_or_inside_it` | ported |
| `tests/test_xvag_voice_swap.py::RunTests::test_a_psarc_source_is_read_by_name_whatever_the_entry_order` | `src/core/voice_swap.rs::tests::a_psarc_source_is_read_by_name_whatever_the_entry_order` | ported |
| `tests/test_xvag_voice_swap.py::RunTests::test_cli_reports_and_returns_zero` | `src/core/voice_swap.rs::tests::cli_reports_and_returns_zero` | ported |

## Gaps

None: the 0 rows marked `dropped: no Rust counterpart`.
