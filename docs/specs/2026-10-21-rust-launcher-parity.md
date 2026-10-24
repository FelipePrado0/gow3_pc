# Rust launcher parity: Python tests → Rust tests

Every test method of `tests/test_launcher_*.py`, `tests/test_trophies.py` and `tests/test_button_icons.py`
(the Python launcher 2.7.0, reference `003e42b`, plus the generator checks of Task 2) with the Rust test
of `launcher-rust/` that covers its behaviour (spec §7). `tests/test_launcher_parity_doc.py` keeps this
map honest: it finds the methods by scanning those files, and fails when one has no row, a row has no
status, or a named Rust test does not exist. That checker is not itself a row.

Status:

- `ported`: the Rust test is the port of the Python one (same cut, often the same name).
- `replaced by <test>`: Rust tests that cover the behaviour under a different cut; the Rust column
  names the same tests.
- `dropped: <reason>`: Tk-only or Python-only mechanics with no Rust equivalent (font registration,
  the X11/Tk scale queries, the frozen-Python stdio, the Tk grid of the Diagnostics section).
- `dropped: spec §<section> <reason>`: behaviour the spec replaced on purpose (any file).
- `dropped: no Rust counterpart — <what it tested>`: behaviour no Rust test covers. Each is listed
  again under [Gaps](#gaps).

Rust tests are named `file::name` under `launcher-rust/`: `tests/ui_save.rs::name` for integration
tests, `src/core/ini.rs::tests::name` for unit tests (the `tests` segment is the module).
Rows that rest on `tests/ui_snapshots.rs` were checked against the reference images; those pixels
are compared on Windows only (see the deviations). Rows whose Rust tests leave part of the Python
assertions uncovered are listed under [Partly covered](#partly-covered).

## Counts

| File | Rows | Ported | Replaced | Dropped (Tk/Python-only) | Dropped (spec) | Gaps |
|---|---|---|---|---|---|---|
| `tests/test_launcher_artwork.py` | 7 | 0 | 3 | 0 | 0 | 4 |
| `tests/test_launcher_aspect.py` | 1 | 1 | 0 | 0 | 0 | 0 |
| `tests/test_launcher_cache.py` | 3 | 3 | 0 | 0 | 0 | 0 |
| `tests/test_launcher_close_on_play.py` | 5 | 4 | 1 | 0 | 0 | 0 |
| `tests/test_launcher_controls.py` | 51 | 20 | 18 | 0 | 0 | 13 |
| `tests/test_launcher_desktop.py` | 33 | 29 | 3 | 0 | 0 | 1 |
| `tests/test_launcher_diagnostics_layout.py` | 3 | 0 | 0 | 3 | 0 | 0 |
| `tests/test_launcher_exit_hint.py` | 17 | 10 | 7 | 0 | 0 | 0 |
| `tests/test_launcher_font.py` | 19 | 0 | 6 | 6 | 0 | 7 |
| `tests/test_launcher_game_files.py` | 2 | 0 | 2 | 0 | 0 | 0 |
| `tests/test_launcher_game_language.py` | 5 | 4 | 0 | 0 | 0 | 1 |
| `tests/test_launcher_generators.py` | 3 | 0 | 3 | 0 | 0 | 0 |
| `tests/test_launcher_icon.py` | 3 | 0 | 0 | 0 | 0 | 3 |
| `tests/test_launcher_mod_resolutions.py` | 3 | 1 | 2 | 0 | 0 | 0 |
| `tests/test_launcher_monitor.py` | 73 | 17 | 16 | 0 | 1 | 39 |
| `tests/test_launcher_no_msaa.py` | 8 | 3 | 4 | 0 | 0 | 1 |
| `tests/test_launcher_pages.py` | 3 | 0 | 1 | 0 | 0 | 2 |
| `tests/test_launcher_polish_language.py` | 5 | 0 | 1 | 0 | 0 | 4 |
| `tests/test_launcher_portable_settings.py` | 7 | 1 | 3 | 0 | 0 | 3 |
| `tests/test_launcher_run_log.py` | 7 | 4 | 3 | 0 | 0 | 0 |
| `tests/test_launcher_save.py` | 17 | 12 | 2 | 0 | 0 | 3 |
| `tests/test_launcher_scaling.py` | 17 | 0 | 4 | 9 | 1 | 3 |
| `tests/test_launcher_stdio.py` | 2 | 0 | 0 | 2 | 0 | 0 |
| `tests/test_launcher_stream_memory.py` | 4 | 1 | 3 | 0 | 0 | 0 |
| `tests/test_launcher_translations.py` | 9 | 0 | 1 | 0 | 0 | 8 |
| `tests/test_launcher_update.py` | 15 | 11 | 1 | 0 | 0 | 3 |
| `tests/test_launcher_upscale.py` | 2 | 0 | 1 | 0 | 0 | 1 |
| `tests/test_trophies.py` | 8 | 5 | 1 | 0 | 0 | 2 |
| `tests/test_button_icons.py` | 4 | 3 | 1 | 0 | 0 | 0 |
| **Total** | **336** | **129** | **87** | **20** | **2** | **98** |

## Map

| Python test | Rust test (file::name) | Status |
|---|---|---|
| `tests/test_launcher_artwork.py::LauncherArtworkTest::test_cover_falls_back_to_base_but_version_stays_from_update` | — | dropped: no Rust counterpart — with an update that has no pic1.png the cover falls back to the base game's while the version shown stays the update's |
| `tests/test_launcher_artwork.py::LauncherArtworkTest::test_window_icon_is_the_ports_own_whatever_the_dump` | `src/ui/app.rs::tests::the_window_icon_is_the_monogram_artwork` | replaced by `src/ui/app.rs::tests::the_window_icon_is_the_monogram_artwork` |
| `tests/test_launcher_artwork.py::LauncherArtworkTest::test_update_artwork_overrides_base_when_present` | — | dropped: no Rust counterpart — the update's pic1.png is the cover when it has one |
| `tests/test_launcher_artwork.py::LauncherArtworkTest::test_cover_falls_back_to_base_when_only_the_icon_was_updated` | — | dropped: no Rust counterpart — an update with only icon0.png keeps the base game's pic1.png as the cover |
| `tests/test_launcher_artwork.py::LauncherArtworkTest::test_without_an_update_the_base_cover_is_used` | — | dropped: no Rust counterpart — without an update folder the base game's pic1.png is the cover |
| `tests/test_launcher_artwork.py::LauncherArtworkTest::test_missing_artwork_still_uses_the_existing_placeholder` | `tests/ui_snapshots.rs::home` | replaced by `tests/ui_snapshots.rs::home` |
| `tests/test_launcher_artwork.py::LauncherArtworkTest::test_missing_artwork_shows_the_ports_logo` | `tests/ui_snapshots.rs::home` | replaced by `tests/ui_snapshots.rs::home` |
| `tests/test_launcher_aspect.py::AspectFilterTest::test_aspect_follows_and_filters_the_render_resolution` | `tests/ui_pages.rs::aspect_follows_and_filters_the_render_resolution` | ported |
| `tests/test_launcher_cache.py::ClearShaderCache::test_asks_then_deletes` | `tests/ui_pages.rs::deleting_the_shader_cache_asks_then_deletes` | ported |
| `tests/test_launcher_cache.py::ClearShaderCache::test_refuses_while_the_game_runs` | `tests/ui_play.rs::a_running_game_blocks_restore_reset_and_cache_clear` | ported |
| `tests/test_launcher_cache.py::ClearShaderCache::test_empty_cache_needs_no_question` | `tests/ui_pages.rs::deleting_the_shader_cache_asks_then_deletes` | ported |
| `tests/test_launcher_close_on_play.py::CloseOnPlayTest::test_active_game_hides_window_without_destroying_the_reader_owner` | `tests/ui_play.rs::close_on_play_hides_only_while_running` | ported |
| `tests/test_launcher_close_on_play.py::CloseOnPlayTest::test_early_failure_and_stale_timer_do_not_hide_the_launcher` | `tests/ui_play.rs::close_on_play_hides_only_while_running` | ported |
| `tests/test_launcher_close_on_play.py::CloseOnPlayTest::test_hidden_launcher_exits_after_game_output_is_finished` | `tests/ui_play.rs::a_hidden_launcher_exits_after_the_game` | ported |
| `tests/test_launcher_close_on_play.py::CloseOnPlayTest::test_visible_launcher_stays_open_after_game_exits` | `tests/ui_play.rs::a_hidden_launcher_exits_after_the_game` | ported |
| `tests/test_launcher_close_on_play.py::CloseOnPlayTest::test_real_child_keeps_writing_after_window_is_hidden` | `tests/ui_play.rs::a_hidden_launcher_exits_after_the_game`, `tests/ui_play.rs::run_log_is_written` | replaced by `tests/ui_play.rs::a_hidden_launcher_exits_after_the_game`, `tests/ui_play.rs::run_log_is_written` |
| `tests/test_launcher_controls.py::DiskModsTest::test_frozen_launcher_uses_disk_loader_in_every_import` | — | dropped: no Rust counterpart — the frozen launcher imports scripts/mods.py from the port folder over its bundled copy (the Rust mods code is compiled in) |
| `tests/test_launcher_controls.py::DiskModsTest::test_missing_disk_loader_uses_the_bundle` | — | dropped: no Rust counterpart — without scripts/mods.py on disk the frozen launcher uses its bundled copy |
| `tests/test_launcher_controls.py::DiskModsTest::test_invalid_disk_loader_has_a_clear_error_and_keeps_the_bundle` | — | dropped: no Rust counterpart — a broken scripts/mods.py raises an error naming the file and keeps the bundled copy |
| `tests/test_launcher_controls.py::ControlMapTest::test_shader_notes_are_off_without_a_saved_choice` | `src/core/env.rs::tests::shader_notes_are_off_without_a_saved_choice` | ported |
| `tests/test_launcher_controls.py::ControlMapTest::test_saved_shader_notes_choice_is_preserved` | `src/core/env.rs::tests::saved_shader_notes_choice_is_preserved` | ported |
| `tests/test_launcher_controls.py::ControlMapTest::test_new_cheats_default_to_disabled` | `tests/fixtures.rs::collect_matches_python_for_edited_case` | replaced by `tests/fixtures.rs::collect_matches_python_for_edited_case` |
| `tests/test_launcher_controls.py::ControlMapTest::test_pipeline_binary_cache_switch` | — | dropped: no Rust counterpart — pipeline_binary_cache off sends SPARTAN_PIPELINE_BINARIES=0 (the default 1 is only in the environment fixtures) |
| `tests/test_launcher_controls.py::ControlMapTest::test_changed_buttons_in_table_order` | `src/core/controls.rs::tests::changed_buttons_in_table_order` | ported |
| `tests/test_launcher_controls.py::ControlMapTest::test_rejects_separators_unknown_buttons_and_bad_shapes` | `src/core/controls.rs::tests::rejects_separators_unknown_buttons_and_bad_shapes` | ported |
| `tests/test_launcher_controls.py::ControlMapTest::test_only_changes_from_the_defaults_are_kept` | `src/core/controls.rs::tests::only_changes_from_the_defaults_are_kept` | ported |
| `tests/test_launcher_controls.py::ControlMapTest::test_touchpad_sides_are_separate_buttons` | `src/core/controls.rs::tests::touchpad_sides_are_separate_buttons` | ported |
| `tests/test_launcher_controls.py::ControlMapTest::test_saved_touchpad_key_moves_to_the_left_side` | `src/core/controls.rs::tests::saved_touchpad_key_moves_to_the_left_side` | ported |
| `tests/test_launcher_controls.py::ControlMapTest::test_both_styles_name_every_default_gamepad_button` | `src/core/controls.rs::tests::both_styles_name_every_default_gamepad_button` | ported |
| `tests/test_launcher_controls.py::ControlMapTest::test_pressed_keys_become_sdl_names` | `tests/ui_pages.rs::pressed_keys_become_sdl_names` | ported |
| `tests/test_launcher_controls.py::ControlMapTest::test_keys_that_cannot_be_bound` | `tests/ui_pages.rs::keys_that_cannot_be_bound` | ported |
| `tests/test_launcher_controls.py::ControlMapTest::test_more_keys_can_be_bound` | `tests/ui_pages.rs::pressed_keys_become_sdl_names` | replaced by `tests/ui_pages.rs::pressed_keys_become_sdl_names` |
| `tests/test_launcher_controls.py::ControlMapTest::test_captured_names_reach_the_game` | `tests/ui_pages.rs::pressed_keys_become_sdl_names`, `src/core/controls.rs::tests::changed_buttons_in_table_order` | replaced by `tests/ui_pages.rs::pressed_keys_become_sdl_names`, `src/core/controls.rs::tests::changed_buttons_in_table_order` |
| `tests/test_launcher_controls.py::ControlMapTest::test_mouse_inputs_become_game_names` | `tests/ui_pages.rs::capture_takes_the_mouse_and_gives_up_in_time`, `src/core/controls.rs::tests::sdl_names_and_mouse_names` | replaced by `tests/ui_pages.rs::capture_takes_the_mouse_and_gives_up_in_time`, `src/core/controls.rs::tests::sdl_names_and_mouse_names` |
| `tests/test_launcher_controls.py::ControlMapTest::test_the_mouse_binds_in_the_keyboard_table` | `tests/ui_pages.rs::capture_takes_the_mouse_and_gives_up_in_time`, `src/core/controls.rs::tests::keyboard_defaults_put_the_attacks_on_the_mouse` | replaced by `tests/ui_pages.rs::capture_takes_the_mouse_and_gives_up_in_time`, `src/core/controls.rs::tests::keyboard_defaults_put_the_attacks_on_the_mouse` |
| `tests/test_launcher_controls.py::ControlMapTest::test_keyboard_defaults_put_the_attacks_on_the_mouse` | `src/core/controls.rs::tests::keyboard_defaults_put_the_attacks_on_the_mouse` | ported |
| `tests/test_launcher_controls.py::ControlMapTest::test_actions_belong_to_the_keyboard_table` | `tests/fixtures.rs::environment_matches_python_for_case_defaults`, `tests/fixtures.rs::settings_match_python_for_case_defaults`, `src/core/controls.rs::tests::saved_keyboard_binds_keep_their_layout` | replaced by `tests/fixtures.rs::environment_matches_python_for_case_defaults`, `tests/fixtures.rs::settings_match_python_for_case_defaults`, `src/core/controls.rs::tests::saved_keyboard_binds_keep_their_layout` |
| `tests/test_launcher_controls.py::ControlMapTest::test_rage_switch_moves_l3_and_r3_to_one_input` | `src/core/controls.rs::tests::rage_switch_moves_l3_and_r3_to_one_input` | ported |
| `tests/test_launcher_controls.py::ControlMapTest::test_stick_prompts_reach_the_game` | — | dropped: no Rust counterpart — stick_prompts reaches SPARTAN_STICK_PROMPTS (mouse is sent, an unknown value is dropped, an inherited value is replaced); only the keyboard default is in the environment fixtures |
| `tests/test_launcher_controls.py::ControlMapTest::test_saved_keyboard_binds_keep_their_layout` | `src/core/controls.rs::tests::saved_keyboard_binds_keep_their_layout` | ported |
| `tests/test_launcher_controls.py::ControlMapTest::test_native_translation_carries_over` | `src/core/controls.rs::tests::native_translation_carries_over` | ported |
| `tests/test_launcher_controls.py::ControlMapTest::test_new_players_get_the_new_defaults` | `src/core/controls.rs::tests::new_players_get_the_new_defaults` | ported |
| `tests/test_launcher_controls.py::ControlMapTest::test_binding_a_used_key_swaps_the_buttons` | `src/core/controls.rs::tests::binding_a_used_key_swaps_the_buttons` | ported |
| `tests/test_launcher_controls.py::ControlMapTest::test_environment` | `tests/fixtures.rs::environment_matches_python_for_case_defaults`, `tests/fixtures.rs::environment_matches_python_for_case_edited` | replaced by `tests/fixtures.rs::environment_matches_python_for_case_defaults`, `tests/fixtures.rs::environment_matches_python_for_case_edited` |
| `tests/test_launcher_controls.py::ControlMapTest::test_temporal_upscaler_stays_off_with_a_saved_2_1_8_switch` | — | dropped: no Rust counterpart — a saved experimental_upscaler=true still sends SPARTAN_EXPERIMENTAL_UPSCALER=0 |
| `tests/test_launcher_controls.py::ControlMapTest::test_glyph_style_is_off_by_default` | `tests/fixtures.rs::settings_match_python_for_case_defaults`, `tests/fixtures.rs::environment_matches_python_for_case_defaults` | replaced by `tests/fixtures.rs::settings_match_python_for_case_defaults`, `tests/fixtures.rs::environment_matches_python_for_case_defaults` |
| `tests/test_launcher_controls.py::ControlMapTest::test_glyph_style_choices_reach_the_game` | — | dropped: no Rust counterpart — glyph_style auto, xbox, nintendo and keyboard reach SPARTAN_GLYPH_STYLE |
| `tests/test_launcher_controls.py::ControlMapTest::test_unknown_glyph_style_is_off` | — | dropped: no Rust counterpart — an unknown glyph_style sends no SPARTAN_GLYPH_STYLE |
| `tests/test_launcher_controls.py::ControlMapTest::test_inherited_glyph_style_is_dropped_when_off` | — | dropped: no Rust counterpart — an inherited SPARTAN_GLYPH_STYLE is dropped while the style is off and replaced when one is chosen |
| `tests/test_launcher_controls.py::ControlMapTest::test_settings_keys_kept` | `tests/fixtures.rs::settings_match_python_for_case_defaults` | replaced by `tests/fixtures.rs::settings_match_python_for_case_defaults` |
| `tests/test_launcher_controls.py::ControlMapTest::test_graphics_defaults_match_the_game` | `tests/fixtures.rs::collect_matches_python_for_edited_case` | replaced by `tests/fixtures.rs::collect_matches_python_for_edited_case` |
| `tests/test_launcher_controls.py::GameFilesTest::test_copy_in_progress_blocks_another_play_or_status_refresh` | `tests/ui_play.rs::play_offers_the_system_files_of_a_pkg_extraction` | replaced by `tests/ui_play.rs::play_offers_the_system_files_of_a_pkg_extraction` |
| `tests/test_launcher_controls.py::GameFilesTest::test_unreadable_executable_is_named_before_starting` | — | dropped: no Rust counterpart — a game file that cannot be opened is reported as unreadable |
| `tests/test_launcher_controls.py::GameFilesTest::test_missing_game_files` | `src/core/game.rs::tests::an_empty_eboot_counts_as_missing`, `src/core/game.rs::tests::a_folder_named_param_sfo_is_reported_as_folder`, `src/core/game.rs::tests::a_param_sfo_that_is_not_one_is_invalid`, `tests/ui_play.rs::describe_missing_labels_every_state` | replaced by `src/core/game.rs::tests::an_empty_eboot_counts_as_missing`, `src/core/game.rs::tests::a_folder_named_param_sfo_is_reported_as_folder`, `src/core/game.rs::tests::a_param_sfo_that_is_not_one_is_invalid`, `tests/ui_play.rs::describe_missing_labels_every_state` |
| `tests/test_launcher_controls.py::GameFilesTest::test_ready_to_play_needs_a_complete_dump` | `src/core/game.rs::tests::a_complete_dump_is_ready`, `src/core/game.rs::tests::update_in_the_same_folder_is_not_ready` | replaced by `src/core/game.rs::tests::a_complete_dump_is_ready`, `src/core/game.rs::tests::update_in_the_same_folder_is_not_ready` |
| `tests/test_launcher_controls.py::GameFilesTest::test_system_files_beside_a_pkg_extraction` | `src/core/game.rs::tests::sc0_beside_a_pkg_extraction_is_found_only_without_a_usable_param_sfo`, `src/core/game.rs::tests::adoption_copies_missing_files_keeps_usable_ones_and_writes_param_sfo_last` | replaced by `src/core/game.rs::tests::sc0_beside_a_pkg_extraction_is_found_only_without_a_usable_param_sfo`, `src/core/game.rs::tests::adoption_copies_missing_files_keeps_usable_ones_and_writes_param_sfo_last` |
| `tests/test_launcher_controls.py::GameFilesTest::test_play_offers_to_adopt_the_system_files` | `tests/ui_play.rs::play_offers_the_system_files_of_a_pkg_extraction`, `tests/ui_play.rs::declining_the_game_offer_offers_the_update_files_next`, `tests/ui_play.rs::play_names_missing_files` | replaced by `tests/ui_play.rs::play_offers_the_system_files_of_a_pkg_extraction`, `tests/ui_play.rs::declining_the_game_offer_offers_the_update_files_next`, `tests/ui_play.rs::play_names_missing_files` |
| `tests/test_launcher_controls.py::GpuChoiceTest::test_parse_gpu_list` | `src/core/gpu.rs::tests::parse_gpu_list_reads_rows` | ported |
| `tests/test_launcher_controls.py::GpuChoiceTest::test_list_gpus_failures_leave_only_automatic` | `src/core/gpu.rs::tests::a_missing_tool_lists_nothing` | replaced by `src/core/gpu.rs::tests::a_missing_tool_lists_nothing` |
| `tests/test_launcher_controls.py::GpuChoiceTest::test_gpu_choice_reaches_the_game` | `src/core/env.rs::tests::inherited_gpu_id_and_monitor_are_dropped_when_not_digits` | replaced by `src/core/env.rs::tests::inherited_gpu_id_and_monitor_are_dropped_when_not_digits` |
| `tests/test_launcher_controls.py::ReleaseVersionTest::test_port_release_is_newer_than_the_inherited_tags` | `tests/core_boundary.rs::version_is_the_crates`, `src/core/update.rs::tests::versions_are_their_digit_runs` | replaced by `tests/core_boundary.rs::version_is_the_crates`, `src/core/update.rs::tests::versions_are_their_digit_runs` |
| `tests/test_launcher_controls.py::ReleaseVersionTest::test_first_start_is_in_english` | `tests/startup.rs::first_start_is_in_english_when_the_system_language_is_unknown` | ported |
| `tests/test_launcher_controls.py::PadStyleTest::test_nintendo_names_follow_the_button_positions` | `tests/ui_pages.rs::nintendo_names_follow_the_button_positions` | ported |
| `tests/test_launcher_controls.py::PadStyleTest::test_device_is_its_own_choice` | `tests/ui_pages.rs::device_is_its_own_choice` | ported |
| `tests/test_launcher_controls.py::PadStyleTest::test_connected_gamepad_skips_a_probe_without_its_dlls` | — | dropped: no Rust counterpart — on Windows the gamepad probe is not run without its SDL3.dll |
| `tests/test_launcher_controls.py::PadStyleTest::test_connected_gamepad_reads_the_probe` | — | dropped: no Rust counterpart — the probe's "Gamepad: type=" line means a gamepad, "No gamepad" none, a timeout unknown (only a missing probe is asserted) |
| `tests/test_launcher_controls.py::PadStyleTest::test_nintendo_icons` | — | dropped: no Rust counterpart — the Nintendo icons put B on the south button and ZL on the left trigger |
| `tests/test_launcher_desktop.py::DesktopIntegrationTest::test_entry_names_the_appimage_the_icon_and_the_window_class` | `src/core/desktop.rs::tests::entry_names_the_appimage_the_icon_and_the_window_class` | ported |
| `tests/test_launcher_desktop.py::DesktopIntegrationTest::test_a_moved_appimage_or_a_new_icon_updates_the_entry` | `src/core/desktop.rs::tests::a_moved_appimage_or_a_new_icon_updates_the_entry` | ported |
| `tests/test_launcher_desktop.py::DesktopIntegrationTest::test_turning_it_off_removes_only_our_entry` | `src/core/desktop.rs::tests::turning_it_off_removes_only_our_entry` | ported |
| `tests/test_launcher_desktop.py::DesktopIntegrationTest::test_an_entry_made_by_hand_is_left_alone` | `src/core/desktop.rs::tests::an_entry_made_by_hand_is_left_alone` | ported |
| `tests/test_launcher_desktop.py::DesktopIntegrationTest::test_an_entry_of_another_tool_for_the_same_file_means_none_of_ours` | `src/core/desktop.rs::tests::an_entry_of_another_tool_for_the_same_file_means_none_of_ours` | ported |
| `tests/test_launcher_desktop.py::DesktopIntegrationTest::test_our_entry_is_removed_once_another_tool_integrates_the_file` | `src/core/desktop.rs::tests::our_entry_is_removed_once_another_tool_integrates_the_file` | ported |
| `tests/test_launcher_desktop.py::DesktopIntegrationTest::test_an_entry_for_another_file_does_not_count` | `src/core/desktop.rs::tests::an_entry_for_another_file_does_not_count` | ported |
| `tests/test_launcher_desktop.py::DesktopIntegrationTest::test_the_variable_gear_lever_sets_skips_it_too` | `src/core/desktop.rs::tests::the_variable_gear_lever_sets_skips_it_too` | ported |
| `tests/test_launcher_desktop.py::DesktopIntegrationTest::test_an_empty_variable_does_not_count` | `src/core/desktop.rs::tests::an_empty_variable_does_not_count` | ported |
| `tests/test_launcher_desktop.py::DesktopIntegrationTest::test_the_shells_variable_does_not_leak_into_these_tests` | `src/core/desktop.rs::tests::the_variable_gear_lever_sets_skips_it_too` | replaced by `src/core/desktop.rs::tests::the_variable_gear_lever_sets_skips_it_too` |
| `tests/test_launcher_desktop.py::DesktopIntegrationTest::test_without_the_environment_argument_the_real_one_is_read` | — | dropped: no Rust counterpart — the start-up integration honours DESKTOPINTEGRATION from the real environment |
| `tests/test_launcher_desktop.py::DesktopIntegrationTest::test_a_nul_byte_in_another_entry_is_ignored` | `src/core/desktop.rs::tests::a_nul_byte_in_another_entry_is_ignored` | ported |
| `tests/test_launcher_desktop.py::DesktopIntegrationTest::test_a_broken_entry_does_not_hide_a_good_one` | `src/core/desktop.rs::tests::a_broken_entry_does_not_hide_a_good_one` | ported |
| `tests/test_launcher_desktop.py::DesktopIntegrationTest::test_entries_that_are_not_text_or_not_files_are_ignored` | `src/core/desktop.rs::tests::entries_that_are_not_text_or_not_files_are_ignored` | ported |
| `tests/test_launcher_desktop.py::DesktopIntegrationTest::test_a_dangling_symlink_entry_is_ignored` | `src/core/desktop.rs::tests::a_dangling_symlink_entry_is_ignored` | ported |
| `tests/test_launcher_desktop.py::DesktopIntegrationTest::test_lines_that_do_not_parse_are_ignored` | `src/core/desktop.rs::tests::lines_that_do_not_parse_are_ignored` | ported |
| `tests/test_launcher_desktop.py::DesktopIntegrationTest::test_a_path_with_spaces_counts` | `src/core/desktop.rs::tests::a_path_with_spaces_counts` | ported |
| `tests/test_launcher_desktop.py::DesktopIntegrationTest::test_a_symlink_to_the_file_counts` | `src/core/desktop.rs::tests::a_symlink_to_the_file_counts` | ported |
| `tests/test_launcher_desktop.py::DesktopIntegrationTest::test_our_own_entry_never_counts_as_another_tool` | `src/core/desktop.rs::tests::our_own_entry_never_counts_as_another_tool` | ported |
| `tests/test_launcher_desktop.py::DesktopIntegrationTest::test_a_missing_or_unreadable_applications_folder_means_no_other_tool` | `src/core/desktop.rs::tests::a_missing_applications_folder_means_no_other_tool` | ported |
| `tests/test_launcher_desktop.py::DesktopIntegrationTest::test_a_broken_file_never_stops_the_start` | `src/core/desktop.rs::tests::our_entry_survives_a_broken_neighbour_across_starts` | replaced by `src/core/desktop.rs::tests::our_entry_survives_a_broken_neighbour_across_starts` |
| `tests/test_launcher_desktop.py::DesktopIntegrationTest::test_our_entry_survives_a_broken_neighbour_across_starts` | `src/core/desktop.rs::tests::our_entry_survives_a_broken_neighbour_across_starts` | ported |
| `tests/test_launcher_desktop.py::DesktopIntegrationTest::test_without_an_appimage_nothing_is_written` | `src/core/desktop.rs::tests::without_an_appimage_nothing_is_written` | ported |
| `tests/test_launcher_desktop.py::LinuxShortcutTest::test_it_makes_the_menu_entry_and_the_desktop_one_again_after_they_are_deleted` | `src/core/desktop.rs::tests::it_makes_the_menu_entry_and_the_desktop_one_again_after_they_are_deleted` | ported |
| `tests/test_launcher_desktop.py::LinuxShortcutTest::test_the_desktop_folder_is_not_made_when_it_does_not_exist` | `src/core/desktop.rs::tests::the_desktop_folder_is_not_made_when_it_does_not_exist` | ported |
| `tests/test_launcher_desktop.py::LinuxShortcutTest::test_another_tools_entry_keeps_the_menu_free_of_a_duplicate` | `src/core/desktop.rs::tests::another_tools_entry_keeps_the_menu_free_of_a_duplicate` | ported |
| `tests/test_launcher_desktop.py::LinuxShortcutTest::test_a_broken_entry_of_another_program_does_not_stop_the_button` | `src/core/desktop.rs::tests::a_broken_entry_of_another_program_does_not_stop_the_button` | ported |
| `tests/test_launcher_desktop.py::LinuxShortcutTest::test_the_files_run_the_appimage_with_its_icon` | `src/core/desktop.rs::tests::the_files_run_the_appimage_with_its_icon` | ported |
| `tests/test_launcher_desktop.py::LinuxShortcutTest::test_the_files_are_executable` | `src/core/desktop.rs::tests::the_files_are_executable` | ported |
| `tests/test_launcher_desktop.py::LinuxShortcutTest::test_a_missing_icon_falls_back_to_the_theme_name` | `src/core/desktop.rs::tests::a_missing_icon_falls_back_to_the_theme_name` | ported |
| `tests/test_launcher_desktop.py::LinuxShortcutTest::test_clicking_again_rewrites_the_same_files` | `src/core/desktop.rs::tests::clicking_again_rewrites_the_same_files` | ported |
| `tests/test_launcher_desktop.py::LinuxShortcutTest::test_the_shells_variable_is_not_read_when_the_environment_is_given` | `src/core/desktop.rs::tests::the_environment_variable_of_gear_lever_does_the_same` | replaced by `src/core/desktop.rs::tests::the_environment_variable_of_gear_lever_does_the_same` |
| `tests/test_launcher_desktop.py::LinuxShortcutTest::test_the_environment_variable_of_gear_lever_does_the_same` | `src/core/desktop.rs::tests::the_environment_variable_of_gear_lever_does_the_same` | ported |
| `tests/test_launcher_diagnostics_layout.py::DiagnosticsLayoutTest::test_english` | — | dropped: Tk-only: the expanded Diagnostics body must not share the Log section's grid row (Tk grid geometry); the iced page stacks its sections in one column (collapsing is in tests/ui_pages.rs::diagnostics_start_collapsed) |
| `tests/test_launcher_diagnostics_layout.py::DiagnosticsLayoutTest::test_portuguese` | — | dropped: Tk-only: the same grid-row check in Portuguese |
| `tests/test_launcher_diagnostics_layout.py::DiagnosticsLayoutTest::test_scaled_fonts` | — | dropped: Tk-only: the same grid-row check at Tk scaling 1.5 |
| `tests/test_launcher_exit_hint.py::ExitHintTest::test_guest_memory_failure_points_to_the_page_file_on_windows` | `src/core/log.rs::tests::guest_memory_failure_points_to_the_page_file_on_windows` | ported |
| `tests/test_launcher_exit_hint.py::ExitHintTest::test_guest_memory_failure_points_to_linux_memory_on_linux` | `src/core/log.rs::tests::guest_memory_failure_points_to_linux_memory_on_linux` | ported |
| `tests/test_launcher_exit_hint.py::ExitHintTest::test_guest_memory_mapping_failure_on_linux` | `src/core/log.rs::tests::guest_memory_mapping_failure_on_linux` | ported |
| `tests/test_launcher_exit_hint.py::ExitHintTest::test_windows_environment_uses_windows_advice_for_generic_error` | `src/core/log.rs::tests::windows_environment_uses_windows_advice_for_generic_error` | ported |
| `tests/test_launcher_exit_hint.py::ExitHintTest::test_linux_environment_uses_linux_advice_for_generic_error` | `src/core/log.rs::tests::linux_environment_uses_linux_advice_for_generic_error` | ported |
| `tests/test_launcher_exit_hint.py::ExitHintTest::test_debug_exception_while_reading_files_points_to_the_dump` | `src/core/log.rs::tests::hints_and_statuses_match_the_python_strings` | replaced by `src/core/log.rs::tests::hints_and_statuses_match_the_python_strings` |
| `tests/test_launcher_exit_hint.py::ExitHintTest::test_other_lines_have_no_hint` | `src/core/log.rs::tests::hints_and_statuses_match_the_python_strings` | replaced by `src/core/log.rs::tests::hints_and_statuses_match_the_python_strings` |
| `tests/test_launcher_exit_hint.py::ExitHintTest::test_the_hint_follows_the_exit_line` | `tests/ui_play.rs::play_runs_the_fake_game_and_shows_its_lines_then_the_exit` | replaced by `tests/ui_play.rs::play_runs_the_fake_game_and_shows_its_lines_then_the_exit` |
| `tests/test_launcher_exit_hint.py::ExitHintTest::test_exit_hint_switches_to_advanced_page_and_scrolls_log` | `tests/ui_play.rs::an_exit_hint_opens_the_log_and_unhides_the_launcher` | ported |
| `tests/test_launcher_exit_hint.py::ExitHintTest::test_exit_hint_unhides_launcher_when_hidden_for_game` | `tests/ui_play.rs::an_exit_hint_opens_the_log_and_unhides_the_launcher` | ported |
| `tests/test_launcher_exit_hint.py::ExitHintTest::test_clean_exit_destroys_root_when_hidden_for_game` | `tests/ui_play.rs::an_exit_hint_opens_the_log_and_unhides_the_launcher` | ported |
| `tests/test_launcher_exit_hint.py::ExitHintTest::test_play_without_window_logs_hint_and_shows_messagebox` | `tests/runner.rs::play_writes_last_run_log_with_the_advice` | replaced by `tests/runner.rs::play_writes_last_run_log_with_the_advice` |
| `tests/test_launcher_exit_hint.py::ExitHintTest::test_play_without_window_clean_exit_does_not_show_messagebox` | `tests/runner.rs::play_writes_last_run_log_with_the_advice` | replaced by `tests/runner.rs::play_writes_last_run_log_with_the_advice` |
| `tests/test_launcher_exit_hint.py::HybridGpuTest::test_windows_hides_the_amd_switchable_layer` | `src/core/env.rs::tests::windows_hides_the_amd_switchable_layer` | ported |
| `tests/test_launcher_exit_hint.py::HybridGpuTest::test_a_value_set_by_the_player_is_kept` | `src/core/env.rs::tests::a_value_set_by_the_player_is_kept` | ported |
| `tests/test_launcher_exit_hint.py::RealLauncherExitTest::test_real_launcher_memory_failure_navigates_to_advanced_without_key_error` | `tests/ui_play.rs::an_exit_hint_opens_the_log_and_unhides_the_launcher` | replaced by `tests/ui_play.rs::an_exit_hint_opens_the_log_and_unhides_the_launcher` |
| `tests/test_launcher_exit_hint.py::RealLauncherExitTest::test_real_launcher_restores_from_hidden_state_on_memory_failure` | `tests/ui_play.rs::an_exit_hint_opens_the_log_and_unhides_the_launcher` | replaced by `tests/ui_play.rs::an_exit_hint_opens_the_log_and_unhides_the_launcher` |
| `tests/test_launcher_font.py::FontFileTest::test_font_file_is_a_launcher_asset_under_port_dir` | `src/ui/theme.rs::tests::fonts_are_embedded` | replaced by `src/ui/theme.rs::tests::fonts_are_embedded` |
| `tests/test_launcher_font.py::FontFileTest::test_windows_package_ships_the_font` | `src/ui/theme.rs::tests::fonts_are_embedded` | replaced by `src/ui/theme.rs::tests::fonts_are_embedded` |
| `tests/test_launcher_font.py::FontFileTest::test_packages_ship_the_font_license` | — | dropped: no Rust counterpart — the Cinzel OFL license text ships with the face (launcher-rust/assets/OFL-Cinzel.txt exists; no test checks it or its packaging) |
| `tests/test_launcher_font.py::FontFileTest::test_linux_package_ships_the_font` | `src/ui/theme.rs::tests::fonts_are_embedded` | replaced by `src/ui/theme.rs::tests::fonts_are_embedded` |
| `tests/test_launcher_font.py::RegisterFontTest::test_missing_file_is_not_fatal` | — | dropped: Tk-only: registers the font file with GDI or fontconfig so Tk can name it; iced loads the embedded face bytes |
| `tests/test_launcher_font.py::RegisterFontTest::test_file_that_is_not_a_font_is_refused` | — | dropped: Tk-only: GDI font registration refusing a file that is not a font |
| `tests/test_launcher_font.py::RegisterFontTest::test_font_file_registers` | — | dropped: Tk-only: GDI font registration of Cinzel-Black.ttf |
| `tests/test_launcher_font.py::RegisterFontTest::test_font_file_registers_through_fontconfig` | — | dropped: Tk-only: fontconfig font registration of Cinzel-Black.ttf |
| `tests/test_launcher_font.py::RegisterFontTest::test_a_missing_fontconfig_is_not_fatal` | — | dropped: Tk-only: font registration without libfontconfig |
| `tests/test_launcher_font.py::ChineseFontTest::test_chinese_prefers_windows_cjk_font_for_body_and_headings` | — | dropped: no Rust counterpart — Chinese uses an installed CJK face (Microsoft YaHei UI) for the body and the headings |
| `tests/test_launcher_font.py::ChineseFontTest::test_chinese_can_use_noto_on_linux` | — | dropped: no Rust counterpart — Chinese uses Noto Sans CJK SC on Linux for the body and serif text |
| `tests/test_launcher_font.py::ChineseFontTest::test_english_font_preferences_are_unchanged` | — | dropped: no Rust counterpart — English keeps Segoe UI for the body and Georgia for serif text |
| `tests/test_launcher_font.py::HeadingFontTest::test_english_titles_use_the_loaded_font` | `tests/ui_snapshots.rs::controls` | replaced by `tests/ui_snapshots.rs::controls` |
| `tests/test_launcher_font.py::HeadingFontTest::test_other_languages_keep_the_fallback` | — | dropped: no Rust counterpart — Russian headings use the fallback face instead of Cinzel, and an explicit fallback is kept |
| `tests/test_launcher_font.py::HeadingFontTest::test_unloaded_font_keeps_the_fallback` | — | dropped: Tk-only: headings fall back when registering the face failed; the Rust face is embedded and always loaded |
| `tests/test_launcher_font.py::HeadingFontTest::test_game_name_uses_the_font_in_every_language` | `tests/ui_snapshots.rs::home` | replaced by `tests/ui_snapshots.rs::home` |
| `tests/test_launcher_font.py::LatinHeadingsTest::test_latin_script_languages_use_the_loaded_font` | `tests/ui_snapshots.rs::graphics` | replaced by `tests/ui_snapshots.rs::graphics` |
| `tests/test_launcher_font.py::LatinHeadingsTest::test_other_scripts_keep_the_fallback` | — | dropped: no Rust counterpart — Russian and Arabic headings use the fallback face instead of Cinzel (the Rust asks for Cinzel in every language and falls back per glyph) |
| `tests/test_launcher_font.py::LatinHeadingsTest::test_the_face_has_every_latin_ui_letter` | — | dropped: no Rust counterpart — every letter of the Latin-script translations is in the Cinzel face |
| `tests/test_launcher_game_files.py::LauncherGameFilesTests::test_cancel_then_copy_keeps_existing_files_and_reports_missing_module` | `tests/ui_play.rs::play_offers_the_system_files_of_a_pkg_extraction`, `src/core/game.rs::tests::adoption_copies_missing_files_keeps_usable_ones_and_writes_param_sfo_last` | replaced by `tests/ui_play.rs::play_offers_the_system_files_of_a_pkg_extraction`, `src/core/game.rs::tests::adoption_copies_missing_files_keeps_usable_ones_and_writes_param_sfo_last` |
| `tests/test_launcher_game_files.py::LauncherGameFilesTests::test_slow_copy_blocks_reentry_and_failure_allows_retry` | `tests/ui_play.rs::play_offers_the_system_files_of_a_pkg_extraction` | replaced by `tests/ui_play.rs::play_offers_the_system_files_of_a_pkg_extraction` |
| `tests/test_launcher_game_language.py::GameLanguageTest::test_brazilian_portuguese_is_offered_with_its_system_id` | `tests/ui_pages.rs::brazilian_portuguese_is_offered_with_its_system_id` | ported |
| `tests/test_launcher_game_language.py::GameLanguageTest::test_choosing_brazilian_portuguese_starts_the_game_with_id_17` | — | dropped: no Rust counterpart — choosing Brazilian Portuguese starts the game with SPARTAN_LANGUAGE=17 (the Rust test stops at the stored setting 17) |
| `tests/test_launcher_game_language.py::GameLanguageTest::test_brazilian_portuguese_label_is_translated_in_portuguese_and_spanish` | `tests/ui_pages.rs::brazilian_portuguese_label_is_translated_in_portuguese_and_spanish` | ported |
| `tests/test_launcher_game_language.py::GameLanguageNoteTest::test_the_note_names_the_game_menus` | `tests/ui_pages.rs::the_note_names_the_game_menus` | ported |
| `tests/test_launcher_game_language.py::GameLanguageNoteTest::test_the_note_uses_the_game_menu_names_of_each_language` | `tests/ui_pages.rs::the_note_uses_the_game_menu_names_of_each_language` | ported |
| `tests/test_launcher_generators.py::LauncherGeneratorOutput::test_i18n_json_has_every_language_and_the_russian_pairs` | `src/core/i18n.rs::tests::embedded_table_parses`, `src/core/i18n.rs::tests::every_language_table_is_complete`, `src/core/i18n.rs::tests::russian_uses_the_inline_pairs`, `src/core/i18n.rs::tests::trophies_fall_back_to_english` | replaced by `src/core/i18n.rs::tests::embedded_table_parses`, `src/core/i18n.rs::tests::every_language_table_is_complete`, `src/core/i18n.rs::tests::russian_uses_the_inline_pairs`, `src/core/i18n.rs::tests::trophies_fall_back_to_english` |
| `tests/test_launcher_generators.py::LauncherGeneratorOutput::test_fixture_cases_exist_with_every_file` | `tests/fixtures.rs::settings_match_python_for_case_defaults`, `tests/fixtures.rs::environment_matches_python_for_case_legacy_controls`, `tests/fixtures.rs::ini_matches_python_for_case_edited` | replaced by `tests/fixtures.rs::settings_match_python_for_case_defaults`, `tests/fixtures.rs::environment_matches_python_for_case_legacy_controls`, `tests/fixtures.rs::ini_matches_python_for_case_edited` |
| `tests/test_launcher_generators.py::LauncherGeneratorOutput::test_collected_case_holds_what_collect_wrote` | `tests/fixtures.rs::collect_matches_python_for_edited_case` | replaced by `tests/fixtures.rs::collect_matches_python_for_edited_case` |
| `tests/test_launcher_icon.py::WindowIconTest::test_own_ico_is_the_window_icon` | — | dropped: no Rust counterpart — the window carries the port's icon in its big and small sizes |
| `tests/test_launcher_icon.py::WindowIconTest::test_own_png_when_the_ico_is_missing` | — | dropped: no Rust counterpart — the PNG stands in as the window icon when the .ico is missing |
| `tests/test_launcher_icon.py::WindowIconTest::test_icon_has_every_windows_size` | — | dropped: no Rust counterpart — spartanport.ico holds the 16, 24, 32, 48, 64, 128 and 256 px sizes |
| `tests/test_launcher_mod_resolutions.py::ModResolutionsTest::test_region_mods_follow_the_game_selected_in_the_launcher` | `src/core/mods.rs::tests::region_folders_do_not_leak_to_another_game` | replaced by `src/core/mods.rs::tests::region_folders_do_not_leak_to_another_game` |
| `tests/test_launcher_mod_resolutions.py::ModResolutionsTest::test_the_list_follows_the_mods` | `tests/ui_pages.rs::the_list_follows_the_mods` | ported |
| `tests/test_launcher_mod_resolutions.py::ModResolutionsTest::test_a_saved_mod_resolution_stays_chosen_while_its_mod_is_on` | `tests/ui_pages.rs::a_saved_mod_resolution_stays_chosen_while_its_mod_is_on` | replaced by `tests/ui_pages.rs::a_saved_mod_resolution_stays_chosen_while_its_mod_is_on` |
| `tests/test_launcher_monitor.py::DisplayParserTest::test_seven_columns_are_read` | `src/core/gpu.rs::tests::seven_columns_are_read` | ported |
| `tests/test_launcher_monitor.py::DisplayParserTest::test_names_keep_spaces_quotes_and_utf8` | `src/core/gpu.rs::tests::names_keep_spaces_quotes_and_utf8` | ported |
| `tests/test_launcher_monitor.py::DisplayParserTest::test_a_name_with_a_straight_quote_is_preserved` | `src/core/gpu.rs::tests::a_name_with_a_straight_quote_is_preserved` | ported |
| `tests/test_launcher_monitor.py::DisplayParserTest::test_a_gap_and_an_empty_name_are_kept` | `src/core/gpu.rs::tests::a_gap_and_an_empty_name_are_kept` | ported |
| `tests/test_launcher_monitor.py::DisplayParserTest::test_a_list_without_a_primary_is_not_a_failure` | `src/core/gpu.rs::tests::a_list_without_a_primary_is_not_a_failure` | ported |
| `tests/test_launcher_monitor.py::DisplayParserTest::test_malformed_lines_are_skipped` | `src/core/gpu.rs::tests::malformed_lines_are_skipped` | ported |
| `tests/test_launcher_monitor.py::DisplayParserTest::test_geometry_must_be_integers_with_positive_sizes` | `src/core/gpu.rs::tests::geometry_must_be_integers_with_positive_sizes` | ported |
| `tests/test_launcher_monitor.py::DisplayParserTest::test_empty_output_is_no_monitors` | `src/core/gpu.rs::tests::empty_output_is_no_monitors` | ported |
| `tests/test_launcher_monitor.py::DisplayParserTest::test_a_record_ends_on_a_line_feed_or_a_crlf` | `src/core/gpu.rs::tests::a_record_ends_on_a_line_feed_or_a_crlf` | ported |
| `tests/test_launcher_monitor.py::DisplayParserTest::test_a_unicode_line_separator_in_a_name_is_not_a_record_boundary` | `src/core/gpu.rs::tests::a_unicode_line_separator_in_a_name_is_not_a_record_boundary` | ported |
| `tests/test_launcher_monitor.py::MonitorOptionsTest::test_the_default_is_first_and_the_rest_come_by_index` | `src/core/gpu.rs::tests::the_default_is_first_and_the_rest_come_by_index` | ported |
| `tests/test_launcher_monitor.py::MonitorOptionsTest::test_a_gap_in_the_indices_is_kept` | `src/core/gpu.rs::tests::a_gap_in_the_indices_is_kept` | ported |
| `tests/test_launcher_monitor.py::MonitorOptionsTest::test_no_monitors_leaves_only_the_default` | `src/core/gpu.rs::tests::no_monitors_leaves_only_the_default` | ported |
| `tests/test_launcher_monitor.py::MonitorOptionsTest::test_primary_at_nonzero_index` | `src/core/gpu.rs::tests::primary_at_nonzero_index` | ported |
| `tests/test_launcher_monitor.py::MonitorOptionsTest::test_negative_coordinates` | `src/core/gpu.rs::tests::negative_coordinates` | ported |
| `tests/test_launcher_monitor.py::ListDisplaysTest::test_a_missing_tool_lists_nothing` | `src/core/gpu.rs::tests::a_missing_tool_lists_nothing` | ported |
| `tests/test_launcher_monitor.py::ListDisplaysTest::test_failures_and_a_bad_exit_list_nothing` | — | dropped: no Rust counterpart — a probe that times out, cannot start or exits non-zero lists no monitors |
| `tests/test_launcher_monitor.py::ListDisplaysTest::test_a_good_run_is_parsed_and_captured_as_utf8` | — | dropped: no Rust counterpart — the probe runs as --displays with a 5 s timeout and its output is read as UTF-8 with replacement |
| `tests/test_launcher_monitor.py::ListDisplaysTest::test_the_game_backend_sets_the_sdl_driver_for_the_probe` | — | dropped: no Rust counterpart — on Linux the saved window system (x11, wayland) is passed to the probe as SDL_VIDEODRIVER and SDL_VIDEO_DRIVER; Automatic leaves the inherited value |
| `tests/test_launcher_monitor.py::MonitorEnvironmentTest::test_the_default_is_the_primary_and_sends_no_variable` | `tests/fixtures.rs::settings_match_python_for_case_defaults`, `tests/fixtures.rs::environment_matches_python_for_case_defaults` | replaced by `tests/fixtures.rs::settings_match_python_for_case_defaults`, `tests/fixtures.rs::environment_matches_python_for_case_defaults` |
| `tests/test_launcher_monitor.py::MonitorEnvironmentTest::test_a_saved_index_reaches_the_game` | `src/core/env.rs::tests::inherited_gpu_id_and_monitor_are_dropped_when_not_digits`, `tests/fixtures.rs::environment_matches_python_for_case_edited` | replaced by `src/core/env.rs::tests::inherited_gpu_id_and_monitor_are_dropped_when_not_digits`, `tests/fixtures.rs::environment_matches_python_for_case_edited` |
| `tests/test_launcher_monitor.py::MonitorEnvironmentTest::test_an_invalid_choice_sends_nothing` | `src/core/env.rs::tests::inherited_gpu_id_and_monitor_are_dropped_when_not_digits` | replaced by `src/core/env.rs::tests::inherited_gpu_id_and_monitor_are_dropped_when_not_digits` |
| `tests/test_launcher_monitor.py::MonitorEnvironmentTest::test_an_old_setting_without_the_key_is_the_default` | `tests/fixtures.rs::environment_matches_python_for_case_legacy_controls` | replaced by `tests/fixtures.rs::environment_matches_python_for_case_legacy_controls` |
| `tests/test_launcher_monitor.py::MonitorEnvironmentTest::test_an_inherited_variable_is_overridden` | `src/core/env.rs::tests::inherited_gpu_id_and_monitor_are_dropped_when_not_digits` | replaced by `src/core/env.rs::tests::inherited_gpu_id_and_monitor_are_dropped_when_not_digits` |
| `tests/test_launcher_monitor.py::MonitorSelectionTest::test_a_saved_index_the_list_holds_is_kept` | `src/core/gpu.rs::tests::primary_at_nonzero_index` | replaced by `src/core/gpu.rs::tests::primary_at_nonzero_index` |
| `tests/test_launcher_monitor.py::MonitorSelectionTest::test_an_index_the_list_lost_is_the_primary_default` | `src/core/gpu.rs::tests::primary_at_nonzero_index`, `tests/ui_pages.rs::graphics_page_offers_the_choice` | replaced by `src/core/gpu.rs::tests::primary_at_nonzero_index`, `tests/ui_pages.rs::graphics_page_offers_the_choice` |
| `tests/test_launcher_monitor.py::MonitorSelectionTest::test_an_empty_or_unknown_choice_is_the_default` | `src/core/gpu.rs::tests::primary_at_nonzero_index`, `tests/ui_pages.rs::graphics_page_offers_the_choice` | replaced by `src/core/gpu.rs::tests::primary_at_nonzero_index`, `tests/ui_pages.rs::graphics_page_offers_the_choice` |
| `tests/test_launcher_monitor.py::MonitorSelectionTest::test_a_shorter_list_never_lands_on_a_monitor_by_position` | `tests/ui_pages.rs::graphics_page_offers_the_choice` | replaced by `tests/ui_pages.rs::graphics_page_offers_the_choice` |
| `tests/test_launcher_monitor.py::MonitorRefreshTest::test_an_unchanged_backend_refreshes_nothing` | — | dropped: no Rust counterpart — refresh_monitors does not list the monitors again while the window system is unchanged |
| `tests/test_launcher_monitor.py::MonitorRefreshTest::test_without_a_window_system_choice_nothing_happens` | — | dropped: no Rust counterpart — refresh_monitors does nothing where no window system is selectable (Windows) |
| `tests/test_launcher_monitor.py::MonitorRefreshTest::test_a_changed_backend_lists_under_the_new_one` | — | dropped: no Rust counterpart — a changed window system lists the monitors again under it and refreshes the Monitor row |
| `tests/test_launcher_monitor.py::MonitorRefreshTest::test_a_choice_the_new_list_still_holds_is_kept` | — | dropped: no Rust counterpart — after a window system change a monitor index the new list holds stays chosen |
| `tests/test_launcher_monitor.py::MonitorRefreshTest::test_a_choice_the_new_list_lost_is_the_default` | — | dropped: no Rust counterpart — after a window system change an index the new list lost becomes the primary default |
| `tests/test_launcher_monitor.py::MonitorRefreshTest::test_another_monitor_at_the_old_position_is_not_selected` | — | dropped: no Rust counterpart — after a window system change the monitor now at the old position is not chosen |
| `tests/test_launcher_monitor.py::MonitorRefreshTest::test_an_unsaved_choice_survives_the_backend_change` | — | dropped: no Rust counterpart — a monitor picked but not saved survives a window system change |
| `tests/test_launcher_monitor.py::MonitorRefreshTest::test_the_unsaved_choice_beats_the_different_collected_one` | — | dropped: no Rust counterpart — the monitor on screen wins over the last saved one after a window system change |
| `tests/test_launcher_monitor.py::MonitorRefreshTest::test_an_unsaved_choice_lost_by_the_new_list_falls_back_to_the_collected_one` | — | dropped: no Rust counterpart — an unsaved monitor the new list lost falls back to the last saved one |
| `tests/test_launcher_monitor.py::MonitorRefreshTest::test_with_neither_choice_in_the_new_list_it_is_the_default` | — | dropped: no Rust counterpart — with neither the shown nor the saved monitor in the new list the choice is the primary default |
| `tests/test_launcher_monitor.py::MonitorRefreshTest::test_only_the_monitor_setting_changes` | — | dropped: no Rust counterpart — refresh_monitors changes no setting but the monitor |
| `tests/test_launcher_monitor.py::MonitorRefreshTest::test_the_refreshed_choice_is_what_a_launch_sends` | — | dropped: no Rust counterpart — the monitor kept by refresh_monitors is what SPARTAN_MONITOR carries |
| `tests/test_launcher_monitor.py::CollectMonitorConsistencyTest::test_an_unchanged_backend_lists_nothing` | — | dropped: no Rust counterpart — Save with an unchanged window system does not list the monitors again |
| `tests/test_launcher_monitor.py::CollectMonitorConsistencyTest::test_a_changed_backend_lists_again_before_gathering` | — | dropped: no Rust counterpart — Save with a changed window system lists the monitors again before it writes the window system and monitor |
| `tests/test_launcher_monitor.py::CollectMonitorConsistencyTest::test_a_choice_the_new_list_lost_becomes_the_default_in_the_file` | — | dropped: no Rust counterpart — Save writes the primary default when the new list lost the chosen monitor |
| `tests/test_launcher_monitor.py::CollectMonitorConsistencyTest::test_a_second_collect_lists_nothing_more` | — | dropped: no Rust counterpart — a second Save does not list the monitors again |
| `tests/test_launcher_monitor.py::CollectMonitorConsistencyTest::test_a_choice_not_saved_yet_is_what_collect_writes` | — | dropped: no Rust counterpart — Save writes the monitor on screen after a window system change |
| `tests/test_launcher_monitor.py::CollectMonitorConsistencyTest::test_a_choice_not_saved_yet_that_the_new_list_lost_is_the_default` | — | dropped: no Rust counterpart — Save writes the primary default when the new list lost the monitor on screen |
| `tests/test_launcher_monitor.py::PlayMonitorConsistencyTest::test_play_without_a_backend_change_lists_nothing` | — | dropped: no Rust counterpart — PLAY with an unchanged window system does not list the monitors again |
| `tests/test_launcher_monitor.py::PlayMonitorConsistencyTest::test_play_lists_again_when_the_backend_changed` | — | dropped: no Rust counterpart — PLAY with a changed window system lists the monitors again |
| `tests/test_launcher_monitor.py::PlayMonitorConsistencyTest::test_play_validates_the_choice_before_the_game_reads_it` | — | dropped: no Rust counterpart — PLAY resets a monitor the new list lost before the game reads it |
| `tests/test_launcher_monitor.py::PlayMonitorConsistencyTest::test_play_delivers_the_backend_and_the_monitor_it_kept` | — | dropped: no Rust counterpart — PLAY sends the new window system (SDL_VIDEODRIVER) with the monitor it kept |
| `tests/test_launcher_monitor.py::PlayMonitorConsistencyTest::test_play_delivers_the_default_when_the_choice_was_reset` | — | dropped: no Rust counterpart — PLAY sends no SPARTAN_MONITOR when the choice was reset |
| `tests/test_launcher_monitor.py::PlayMonitorConsistencyTest::test_play_keeps_a_choice_not_saved_yet` | — | dropped: no Rust counterpart — PLAY keeps the monitor on screen across a window system change |
| `tests/test_launcher_monitor.py::PlayMonitorConsistencyTest::test_play_drops_a_choice_not_saved_yet_that_the_new_list_lost` | — | dropped: no Rust counterpart — PLAY drops an unsaved monitor the new list lost |
| `tests/test_launcher_monitor.py::PlayMonitorConsistencyTest::test_a_persisted_pair_reaches_a_play_as_one_environment` | — | dropped: no Rust counterpart — on Linux a saved window system and monitor reach the game as SDL_VIDEODRIVER and SPARTAN_MONITOR |
| `tests/test_launcher_monitor.py::CollectCallersTest::test_save_lets_collect_do_the_refresh` | — | dropped: no Rust counterpart — Save reaches the monitor refresh only through collect (no Rust test changes the window system) |
| `tests/test_launcher_monitor.py::CollectCallersTest::test_play_lets_collect_do_the_refresh` | — | dropped: no Rust counterpart — PLAY reaches the monitor refresh only through collect (PLAY saving first is in tests/ui_play.rs::play_refuses_without_eboot_and_opens_game_page) |
| `tests/test_launcher_monitor.py::CollectCallersTest::test_play_still_skips_collect_while_a_game_runs` | `tests/ui_play.rs::play_runs_the_fake_game_and_shows_its_lines_then_the_exit` | replaced by `tests/ui_play.rs::play_runs_the_fake_game_and_shows_its_lines_then_the_exit` |
| `tests/test_launcher_monitor.py::CollectCallersTest::test_play_still_skips_collect_while_the_system_files_copy` | `tests/ui_play.rs::play_offers_the_system_files_of_a_pkg_extraction` | replaced by `tests/ui_play.rs::play_offers_the_system_files_of_a_pkg_extraction` |
| `tests/test_launcher_monitor.py::CollectCallersTest::test_close_collects_then_destroys_the_window` | — | dropped: no Rust counterpart — closing the window saves the settings first (the Rust launcher does not save on close) |
| `tests/test_launcher_monitor.py::CollectCallersTest::test_close_keeps_the_window_when_the_save_fails` | — | dropped: no Rust counterpart — a failed save on close keeps the window open |
| `tests/test_launcher_monitor.py::CollectCallersTest::test_open_path_collects_before_opening_a_settings_file` | — | dropped: no Rust counterpart — opening spartanport.ini saves first and opens nothing when the save fails (the Rust Advanced page opens it without saving) |
| `tests/test_launcher_monitor.py::CollectCallersTest::test_open_path_opens_a_folder_without_collecting` | — | dropped: no Rust counterpart — opening a folder starts the system opener without saving first (the Rust test only sees the screenshots button emit OpenPath) |
| `tests/test_launcher_monitor.py::CollectCallersTest::test_add_to_steam_collects_first` | — | dropped: no Rust counterpart — Add to Steam saves first and stops when the save fails (the Rust Advanced page does not save first) |
| `tests/test_launcher_monitor.py::MonitorLauncherTest::test_graphics_page_offers_the_choice` | `tests/ui_pages.rs::graphics_page_offers_the_choice` | ported |
| `tests/test_launcher_monitor.py::QuickSettingsMonitorTest::test_quick_settings_offers_the_monitor_choice` | `tests/ui_pages.rs::monitor_row_lists_the_probe_indices`, `src/core/gpu.rs::tests::the_default_is_first_and_the_rest_come_by_index` | replaced by `tests/ui_pages.rs::monitor_row_lists_the_probe_indices`, `src/core/gpu.rs::tests::the_default_is_first_and_the_rest_come_by_index` |
| `tests/test_launcher_monitor.py::QuickSettingsMonitorTest::test_the_indices_of_the_probe_are_what_quick_settings_carries` | `tests/ui_pages.rs::graphics_page_offers_the_choice` | replaced by `tests/ui_pages.rs::graphics_page_offers_the_choice` |
| `tests/test_launcher_monitor.py::QuickSettingsMonitorTest::test_picking_in_quick_settings_follows_on_the_graphics_page` | — | dropped: no Rust counterpart — the Graphics row shows a monitor picked elsewhere, as the entry of its index (Quick-settings part dropped by spec §4.3) |
| `tests/test_launcher_monitor.py::QuickSettingsMonitorTest::test_picking_on_the_graphics_page_follows_in_quick_settings` | `tests/ui_pages.rs::monitor_row_lists_the_probe_indices` | replaced by `tests/ui_pages.rs::monitor_row_lists_the_probe_indices` |
| `tests/test_launcher_monitor.py::QuickSettingsMonitorTest::test_the_choice_is_saved_and_comes_back_on_reopening` | `tests/ui_save.rs::every_setting_lights_save_persists_and_comes_back`, `tests/ui_pages.rs::graphics_page_offers_the_choice` | replaced by `tests/ui_save.rs::every_setting_lights_save_persists_and_comes_back`, `tests/ui_pages.rs::graphics_page_offers_the_choice` |
| `tests/test_launcher_monitor.py::QuickSettingsMonitorTest::test_a_new_window_system_drops_an_index_the_new_list_lost` | — | dropped: no Rust counterpart — both monitor rows drop an index the new window system's list lost |
| `tests/test_launcher_monitor.py::QuickSettingsMonitorTest::test_both_rows_follow_a_reordered_list_that_still_holds_the_index` | — | dropped: no Rust counterpart — both monitor rows follow an index that moved in the new list |
| `tests/test_launcher_monitor.py::QuickSettingsMonitorTest::test_the_choice_of_quick_settings_reaches_the_game` | — | dropped: spec §4.3 (layout A2) replaced the Home quick settings with a read-only line |
| `tests/test_launcher_monitor.py::QuickSettingsMonitorTest::test_without_monitors_quick_settings_still_builds` | `tests/ui_pages.rs::graphics_page_offers_the_choice`, `src/core/gpu.rs::tests::no_monitors_leaves_only_the_default` | replaced by `tests/ui_pages.rs::graphics_page_offers_the_choice`, `src/core/gpu.rs::tests::no_monitors_leaves_only_the_default` |
| `tests/test_launcher_no_msaa.py::NoMsaaEnvironmentTest::test_auto_leaves_the_game_default_and_keeps_an_inherited_value` | `tests/fixtures.rs::settings_match_python_for_case_defaults`, `tests/fixtures.rs::environment_matches_python_for_case_defaults` | replaced by `tests/fixtures.rs::settings_match_python_for_case_defaults`, `tests/fixtures.rs::environment_matches_python_for_case_defaults` |
| `tests/test_launcher_no_msaa.py::NoMsaaEnvironmentTest::test_off_and_on_set_the_variable` | — | dropped: no Rust counterpart — no_msaa Off (1) and On (0) reach SPARTAN_NO_MSAA |
| `tests/test_launcher_no_msaa.py::NoMsaaEnvironmentTest::test_choices_and_legacy_settings` | `tests/ui_pages.rs::no_msaa_auto_label_names_the_card_and_the_decision`, `tests/fixtures.rs::environment_matches_python_for_case_legacy_controls` | replaced by `tests/ui_pages.rs::no_msaa_auto_label_names_the_card_and_the_decision`, `tests/fixtures.rs::environment_matches_python_for_case_legacy_controls` |
| `tests/test_launcher_no_msaa.py::ChosenGpuTest::test_the_graphics_card_setting_wins_else_the_games_pick` | `src/core/gpu.rs::tests::chosen_gpu_follows_the_setting_then_the_tools_pick` | ported |
| `tests/test_launcher_no_msaa.py::ChosenGpuTest::test_without_the_decision_columns_or_a_list_there_is_no_answer` | `src/core/gpu.rs::tests::chosen_gpu_follows_the_setting_then_the_tools_pick`, `src/core/gpu.rs::tests::no_msaa_decision_names_the_chosen_card` | replaced by `src/core/gpu.rs::tests::chosen_gpu_follows_the_setting_then_the_tools_pick`, `src/core/gpu.rs::tests::no_msaa_decision_names_the_chosen_card` |
| `tests/test_launcher_no_msaa.py::ChosenGpuTest::test_auto_label_names_the_card_and_the_decision` | `tests/ui_pages.rs::no_msaa_auto_label_names_the_card_and_the_decision` | ported |
| `tests/test_launcher_no_msaa.py::ChosenGpuTest::test_portuguese_texts` | `tests/ui_pages.rs::performance_page_offers_the_choice`, `tests/ui_pages.rs::no_msaa_auto_label_names_the_card_and_the_decision` | replaced by `tests/ui_pages.rs::performance_page_offers_the_choice`, `tests/ui_pages.rs::no_msaa_auto_label_names_the_card_and_the_decision` |
| `tests/test_launcher_no_msaa.py::NoMsaaLauncherTest::test_performance_page_offers_the_choice` | `tests/ui_pages.rs::performance_page_offers_the_choice` | ported |
| `tests/test_launcher_pages.py::PagesTest::test_sidebar_lists_eight_pages_in_order` | `tests/ui_navigation.rs::every_tab_opens_its_drawer_with_the_right_width` | replaced by `tests/ui_navigation.rs::every_tab_opens_its_drawer_with_the_right_width` |
| `tests/test_launcher_pages.py::PagesTest::test_showing_a_page_refreshes_what_it_holds` | — | dropped: no Rust counterpart — showing Home refreshes the status, Game the patch list and Mods the mod list (only the Trophies load on opening is asserted) |
| `tests/test_launcher_pages.py::PagesTest::test_every_refreshed_page_is_a_sidebar_page` | — | dropped: no Rust counterpart — every page with a refresh is a sidebar page (in the Rust the refresh matches on the Page enum) |
| `tests/test_launcher_polish_language.py::PolishLanguageTest::test_polish_is_offered_with_system_id_16` | — | dropped: no Rust counterpart — Polish is offered as game language 16 with the Russian label Польский |
| `tests/test_launcher_polish_language.py::PolishLanguageTest::test_choosing_polish_starts_the_game_with_id_16` | — | dropped: no Rust counterpart — choosing Polish starts the game with SPARTAN_LANGUAGE=16 |
| `tests/test_launcher_polish_language.py::PolishLanguageTest::test_polish_label_is_translated_across_ui_languages` | — | dropped: no Rust counterpart — the Polish label in the twelve translated interface languages |
| `tests/test_launcher_polish_language.py::PolishLanguageTest::test_russian_and_english_fallbacks` | `tests/ui_widgets.rs::select_emits_set_with_the_value_not_the_label`, `src/core/i18n.rs::tests::russian_uses_the_inline_pairs` | replaced by `tests/ui_widgets.rs::select_emits_set_with_the_value_not_the_label`, `src/core/i18n.rs::tests::russian_uses_the_inline_pairs` |
| `tests/test_launcher_polish_language.py::PolishLanguageTest::test_system_service_contract_maps_parameter_1_to_spartanport_language` | — | dropped: no Rust counterpart — the C runtime answers system parameter 1 with SPARTAN_LANGUAGE (a runtime source check, not launcher code) |
| `tests/test_launcher_portable_settings.py::PortableSettingsPathsTest::test_config_dir_is_data_dir` | `src/core/paths.rs::tests::data_dir_defaults_to_the_executables_folder` | replaced by `src/core/paths.rs::tests::data_dir_defaults_to_the_executables_folder` |
| `tests/test_launcher_portable_settings.py::PortableSettingsPathsTest::test_user_files_includes_settings_and_icon` | `src/core/update.rs::tests::user_files_include_settings_and_icon` | ported |
| `tests/test_launcher_portable_settings.py::PortableSettingsMigrationTest::test_copies_legacy_config_and_icon_when_portable_missing` | `src/core/settings.rs::tests::portable_migration_copies_only_settings` | replaced by `src/core/settings.rs::tests::portable_migration_copies_only_settings` |
| `tests/test_launcher_portable_settings.py::PortableSettingsMigrationTest::test_does_not_overwrite_existing_portable_config` | `src/core/settings.rs::tests::portable_migration_copies_only_settings` | replaced by `src/core/settings.rs::tests::portable_migration_copies_only_settings` |
| `tests/test_launcher_portable_settings.py::PortableSettingsMigrationTest::test_missing_legacy_config_is_a_noop` | — | dropped: no Rust counterpart — without a legacy settings.json the migration creates nothing |
| `tests/test_launcher_portable_settings.py::PortableSettingsMigrationTest::test_load_app_settings_migrates_and_merges_defaults` | — | dropped: no Rust counterpart — loading the settings first copies the legacy settings.json, then merges the defaults over it |
| `tests/test_launcher_portable_settings.py::PortableSettingsMigrationTest::test_packaging_scripts_preserve_portable_files` | — | dropped: no Rust counterpart — the Windows package and its smoke test keep settings.json and game_icon.ico (packaging is a later sub-project) |
| `tests/test_launcher_run_log.py::RunLogTest::test_play_passes_the_run_log_to_the_output_reader` | `tests/ui_play.rs::run_log_is_written` | ported |
| `tests/test_launcher_run_log.py::RunLogTest::test_log_path_follows_the_saves_folder_setting` | `tests/ui_play.rs::run_log_is_written` | ported |
| `tests/test_launcher_run_log.py::RunLogTest::test_open_run_log_creates_the_folder_and_truncates` | `src/core/log.rs::tests::run_log_truncates_and_creates_the_folder` | ported |
| `tests/test_launcher_run_log.py::RunLogTest::test_open_run_log_failure_returns_a_null_log` | `src/core/log.rs::tests::run_log_keeps_going_after_a_write_error` | replaced by `src/core/log.rs::tests::run_log_keeps_going_after_a_write_error` |
| `tests/test_launcher_run_log.py::RunLogTest::test_read_output_writes_every_line_then_closes_and_reports_the_exit_code` | `tests/launch.rs::every_line_arrives_then_the_exit_code`, `tests/launch.rs::run_log_holds_every_line` | replaced by `tests/launch.rs::every_line_arrives_then_the_exit_code`, `tests/launch.rs::run_log_holds_every_line` |
| `tests/test_launcher_run_log.py::RunLogTest::test_read_output_keeps_draining_when_the_log_fails` | `tests/launch.rs::every_line_arrives_then_the_exit_code`, `src/core/log.rs::tests::run_log_keeps_going_after_a_write_error` | replaced by `tests/launch.rs::every_line_arrives_then_the_exit_code`, `src/core/log.rs::tests::run_log_keeps_going_after_a_write_error` |
| `tests/test_launcher_run_log.py::RunLogTest::test_play_log_is_kept_when_the_launcher_hides_on_play` | `tests/ui_play.rs::run_log_is_written` | ported |
| `tests/test_launcher_save.py::SaveButtonTest::test_failed_save_does_not_close_the_launcher` | — | dropped: no Rust counterpart — a failed save on close keeps the launcher open (the Rust launcher does not save on close) |
| `tests/test_launcher_save.py::SaveButtonTest::test_failed_save_does_not_start_the_game` | — | dropped: no Rust counterpart — a failed save stops PLAY before the game starts |
| `tests/test_launcher_save.py::SaveButtonTest::test_partial_save_failure_keeps_pending_state` | `tests/ui_save.rs::partial_save_failure_keeps_pending_state` | ported |
| `tests/test_launcher_save.py::SaveButtonTest::test_save_preserves_external_changes_and_refreshes_the_controls` | `tests/ui_save.rs::save_preserves_external_changes_and_refreshes_the_controls` | ported |
| `tests/test_launcher_save.py::SaveButtonTest::test_failed_replace_keeps_files_and_changes_pending` | `tests/ui_save.rs::failed_save_keeps_pending_state` | ported |
| `tests/test_launcher_save.py::SaveButtonTest::test_nothing_to_save_after_opening` | `tests/ui_save.rs::nothing_to_save_after_opening` | ported |
| `tests/test_launcher_save.py::SaveButtonTest::test_a_changed_setting_enables_saving_until_it_is_reverted` | `tests/ui_save.rs::a_changed_setting_enables_saving_until_it_is_reverted` | ported |
| `tests/test_launcher_save.py::SaveButtonTest::test_saving_writes_the_files_and_disables_the_button` | `tests/ui_save.rs::saving_writes_the_files_and_disables_the_button` | ported |
| `tests/test_launcher_save.py::SaveButtonTest::test_saving_a_new_launcher_language_rebuilds_the_window_in_it` | `tests/ui_save.rs::saving_a_new_launcher_language_rebuilds_the_window_in_it` | ported |
| `tests/test_launcher_save.py::SaveButtonTest::test_rebuilding_keeps_the_status_of_a_running_game` | — | dropped: no Rust counterpart — a language rebuild keeps the status line of a running game (the Rust test keeps the log and the version menu's release list of an idle game) |
| `tests/test_launcher_save.py::SaveButtonTest::test_saving_without_language_or_scale_change_does_not_rebuild` | `tests/ui_save.rs::saving_without_language_or_scale_change_does_not_rebuild` | ported |
| `tests/test_launcher_save.py::SaveButtonTest::test_repeated_language_switching_preserves_page_and_cleans_traces` | `tests/ui_save.rs::repeated_language_switching_preserves_the_page` | ported |
| `tests/test_launcher_save.py::SaveButtonTest::test_save_button_event_generates_clean_language_rebuild` | `tests/ui_save.rs::repeated_language_switching_preserves_the_page` | replaced by `tests/ui_save.rs::repeated_language_switching_preserves_the_page` |
| `tests/test_launcher_save.py::SaveButtonTest::test_nothing_is_written_before_saving` | `tests/ui_save.rs::nothing_is_written_before_saving` | ported |
| `tests/test_launcher_save.py::HiddenUpscalingTest::test_hidden_settings_are_preserved_without_migration` | `tests/ui_save.rs::hidden_settings_are_preserved_without_migration` | ported |
| `tests/test_launcher_save.py::EverySettingSavesTest::test_all_settings_save_without_detected_gpus` | `tests/ui_save.rs::every_setting_lights_save_persists_and_comes_back` | replaced by `tests/ui_save.rs::every_setting_lights_save_persists_and_comes_back` |
| `tests/test_launcher_save.py::EverySettingSavesTest::test_every_setting_lights_save_persists_and_comes_back` | `tests/ui_save.rs::every_setting_lights_save_persists_and_comes_back` | ported |
| `tests/test_launcher_scaling.py::ScaleSelectionTest::test_manual_factors_are_absolute_even_on_a_scaled_desktop` | `src/ui/theme.rs::tests::ui_scale_rules` | replaced by `src/ui/theme.rs::tests::ui_scale_rules` |
| `tests/test_launcher_scaling.py::ScaleSelectionTest::test_auto_uses_the_desktop_scale_once` | — | dropped: spec §4.7 GUI Scale has no Auto since R90 (1x or 1.5x, absolute on any desktop; a stored "auto" loads as 1x) |
| `tests/test_launcher_scaling.py::ScaleSelectionTest::test_ui_scale_does_not_change_the_game_environment` | — | dropped: no Rust counterpart — the GUI scale does not change the game environment |
| `tests/test_launcher_scaling.py::DesktopScaleTest::test_gdk_scale_is_used_without_querying_xfce` | — | dropped: Tk-only: Tk on X11 reads the desktop scale from GDK_SCALE; iced takes the scale factor from winit |
| `tests/test_launcher_scaling.py::DesktopScaleTest::test_xfce_window_scaling_is_read_when_gdk_scale_is_unset` | — | dropped: Tk-only: Tk on X11 asks xfconf-query for the Xfce window scaling |
| `tests/test_launcher_scaling.py::DesktopScaleTest::test_missing_or_unresponsive_xfconf_is_not_fatal` | — | dropped: Tk-only: the xfconf-query fallback of the X11 scale query |
| `tests/test_launcher_scaling.py::DesktopScaleTest::test_invalid_environment_values_are_ignored` | — | dropped: Tk-only: GDK_SCALE values the X11 scale query rejects |
| `tests/test_launcher_scaling.py::NativeScaleTest::test_windows_uses_native_scale_without_x11_queries` | `tests/startup.rs::the_desktop_scale_is_read_when_the_window_opens` | replaced by `tests/startup.rs::the_desktop_scale_is_read_when_the_window_opens` |
| `tests/test_launcher_scaling.py::NativeScaleTest::test_aqua_leaves_retina_to_macos_and_never_changes_native_scaling` | — | dropped: Tk-only: Tk's aqua windowing system (macOS is out of scope) |
| `tests/test_launcher_scaling.py::NativeScaleTest::test_x11_uses_font_dpi_instead_of_physical_screen_size` | — | dropped: Tk-only: Tk on X11 takes the scale from Xft.dpi and sets tk scaling |
| `tests/test_launcher_scaling.py::NativeScaleTest::test_tk9_native_scale_is_not_lost_when_font_dpi_is_smaller` | — | dropped: Tk-only: Tk 9's tk::scalingPct against Xft.dpi |
| `tests/test_launcher_scaling.py::NativeScaleTest::test_missing_resources_use_tk_and_then_desktop_fallback` | — | dropped: Tk-only: the X11 resource, tk scaling and desktop fallbacks of the Tk scale query |
| `tests/test_launcher_scaling.py::LauncherScalingTest::test_save_scales_the_whole_interface_and_preserves_game_settings` | — | dropped: no Rust counterpart — Save with GUI Scale 2 doubles the interface, keeps the page, the ini and the game environment, and back to 1 restores the sizes |
| `tests/test_launcher_scaling.py::LauncherScalingTest::test_saved_scale_is_applied_when_a_new_window_starts` | `tests/startup.rs::a_new_desktop_scale_keeps_the_chosen_zoom` | replaced by `tests/startup.rs::a_new_desktop_scale_keeps_the_chosen_zoom` |
| `tests/test_launcher_scaling.py::LauncherScalingTest::test_entry_and_combobox_fonts_belong_to_the_launcher` | — | dropped: Tk-only: entries and comboboxes must use the launcher's own Tk named fonts to scale; iced scales every widget with the window |
| `tests/test_launcher_scaling.py::LauncherScalingTest::test_hint_labels_keep_their_smaller_font` | `tests/ui_snapshots.rs::graphics` | replaced by `tests/ui_snapshots.rs::graphics` |
| `tests/test_launcher_scaling.py::LauncherScalingTest::test_failed_save_keeps_the_current_scale` | — | dropped: no Rust counterpart — a failed save keeps the current GUI scale |
| `tests/test_launcher_stdio.py::UnicodePreparationTest::test_frozen_script_prints_unicode_paths_with_gbk_streams` | — | dropped: frozen-Python only: --script runs a script inside the frozen Python with UTF-8 streams; the Rust launcher has no --script (it is an ordinary argument that opens the window) |
| `tests/test_launcher_stdio.py::UnicodePreparationTest::test_captured_preparation_output_uses_utf8_despite_gbk_environment` | — | dropped: Python-only: run.py captures its scripts' output as UTF-8 whatever PYTHONIOENCODING says; run.py stays Python (the Rust launcher passes PYTHONIOENCODING=utf-8, tests/fixtures.rs) |
| `tests/test_launcher_stream_memory.py::StreamMemoryEnvironmentTest::test_legacy_settings_and_invalid_values_use_cached_memory` | `src/core/env.rs::tests::stream_memory_falls_back_to_cached`, `tests/fixtures.rs::environment_matches_python_for_case_legacy_controls` | replaced by `src/core/env.rs::tests::stream_memory_falls_back_to_cached`, `tests/fixtures.rs::environment_matches_python_for_case_legacy_controls` |
| `tests/test_launcher_stream_memory.py::StreamMemoryEnvironmentTest::test_every_selection_overrides_inherited_environment_without_changing_it` | `src/core/env.rs::tests::stream_memory_falls_back_to_cached` | replaced by `src/core/env.rs::tests::stream_memory_falls_back_to_cached` |
| `tests/test_launcher_stream_memory.py::StreamMemoryEnvironmentTest::test_play_without_window_uses_saved_selection` | `tests/runner.rs::play_starts_the_game_with_the_saved_stream_memory` | ported |
| `tests/test_launcher_stream_memory.py::StreamMemoryLauncherTest::test_ui_selection_persists_reopens_and_reverts_on_next_launch` | `tests/ui_save.rs::every_setting_lights_save_persists_and_comes_back`, `src/core/env.rs::tests::stream_memory_falls_back_to_cached` | replaced by `tests/ui_save.rs::every_setting_lights_save_persists_and_comes_back`, `src/core/env.rs::tests::stream_memory_falls_back_to_cached` |
| `tests/test_launcher_translations.py::PortugueseTest::test_every_literal_text_is_translated` | — | dropped: no Rust counterpart — every literal text the launcher translates has a Portuguese text (no Rust test scans the tr() literals of the code) |
| `tests/test_launcher_translations.py::PortugueseTest::test_button_names_are_translated` | — | dropped: no Rust counterpart — the controller button names have Portuguese texts |
| `tests/test_launcher_translations.py::PortugueseTest::test_the_built_in_patch_notes_are_translated` | — | dropped: no Rust counterpart — the notes of the built-in patches have Portuguese texts |
| `tests/test_launcher_translations.py::PortugueseTest::test_resolution_labels` | — | dropped: no Rust counterpart — the native resolution reads Nativa · 1920 × 1080 · 16:9 in Portuguese (the Rust resolution labels are not translated) |
| `tests/test_launcher_translations.py::SpanishTest::test_resolution_labels` | — | dropped: no Rust counterpart — the native resolution reads Nativa · 1920 × 1080 · 16:9 in Spanish |
| `tests/test_launcher_translations.py::ChineseTest::test_resolution_labels` | — | dropped: no Rust counterpart — the native resolution reads 原生 · 1920 × 1080 · 16:9 in Chinese |
| `tests/test_launcher_translations.py::ChineseTest::test_all_known_labels_and_format_fields` | — | dropped: no Rust counterpart — every known text has a Chinese translation with the same {} fields (only the table sizes are compared) |
| `tests/test_launcher_translations.py::ChineseTest::test_menu_language_is_independent_from_game_language` | `tests/fixtures.rs::environment_matches_python_for_case_edited` | replaced by `tests/fixtures.rs::environment_matches_python_for_case_edited` |
| `tests/test_launcher_translations.py::IndonesianTest::test_resolution_labels` | — | dropped: no Rust counterpart — the native resolution reads Asli · 1920 × 1080 · 16:9 in Indonesian |
| `tests/test_launcher_update.py::ReplaceAppImageTest::test_a_matching_download_replaces_the_appimage_and_makes_it_executable` | `src/core/update.rs::tests::a_matching_download_replaces_the_target` | replaced by `src/core/update.rs::tests::a_matching_download_replaces_the_target` |
| `tests/test_launcher_update.py::ReplaceAppImageTest::test_a_symlink_to_the_appimage_replaces_the_file_behind_it` | — | dropped: no Rust counterpart — updating through a symlink replaces the AppImage behind it and keeps the link |
| `tests/test_launcher_update.py::ReplaceAppImageTest::test_progress_reaches_one_hundred_percent` | `src/core/update.rs::tests::progress_reaches_one_hundred_percent` | ported |
| `tests/test_launcher_update.py::ReplaceAppImageTest::test_a_download_that_does_not_match_the_checksum_leaves_the_appimage_alone` | `src/core/update.rs::tests::a_download_that_does_not_match_the_checksum_leaves_the_target_alone` | ported |
| `tests/test_launcher_update.py::ReplaceAppImageTest::test_a_release_without_a_checksum_is_not_installed` | — | dropped: no Rust counterpart — a release that publishes no .sha256 is not installed |
| `tests/test_launcher_update.py::ReplaceAppImageTest::test_a_garbled_checksum_file_is_not_installed` | `src/core/update.rs::tests::a_garbled_checksum_file_is_not_installed` | ported |
| `tests/test_launcher_update.py::ReplaceAppImageTest::test_a_missing_download_leaves_the_appimage_alone` | — | dropped: no Rust counterpart — a download that fails leaves the AppImage as it was, with no partial file |
| `tests/test_launcher_update.py::RelaunchTest::test_the_old_mount_does_not_leak_into_the_new_appimage` | `src/core/update.rs::tests::the_old_mount_does_not_leak_into_the_new_appimage` | ported |
| `tests/test_launcher_update.py::RelaunchTest::test_a_library_path_that_was_only_the_mount_is_dropped` | `src/core/update.rs::tests::a_library_path_that_was_only_the_mount_is_dropped` | ported |
| `tests/test_launcher_update.py::RelaunchTest::test_a_library_path_is_kept_without_an_appdir` | `src/core/update.rs::tests::a_library_path_is_kept_without_an_appdir` | ported |
| `tests/test_launcher_update.py::LatestReleaseTest::test_windows_takes_the_zip` | `src/core/release.rs::tests::windows_takes_the_zip` | ported |
| `tests/test_launcher_update.py::LatestReleaseTest::test_linux_takes_the_appimage_not_its_checksum` | `src/core/release.rs::tests::linux_takes_the_appimage_not_its_checksum` | ported |
| `tests/test_launcher_update.py::LatestReleaseTest::test_linux_has_no_package_while_the_release_has_no_appimage` | `src/core/release.rs::tests::linux_has_no_package_while_the_release_has_no_appimage` | ported |
| `tests/test_launcher_update.py::CanSelfUpdateTest::test_linux_updates_only_as_an_appimage` | `src/core/update.rs::tests::linux_updates_only_as_an_appimage` | ported |
| `tests/test_launcher_update.py::CanSelfUpdateTest::test_windows_always_updates` | `src/core/update.rs::tests::windows_always_updates` | ported |
| `tests/test_launcher_upscale.py::UpscalerSettingsTest::test_remaining_choices_have_no_spatial_upscaler` | — | dropped: no Rust counterpart — the upscaler choices are fsr3, taa and off, with no spatial upscaler |
| `tests/test_launcher_upscale.py::UpscalerSettingsTest::test_removed_controls_do_not_supply_defaults_to_save` | `tests/ui_save.rs::hidden_settings_are_preserved_without_migration` | replaced by `tests/ui_save.rs::hidden_settings_are_preserved_without_migration` |
| `tests/test_trophies.py::TrophyTexts::test_every_language_lists_all_trophies` | — | dropped: no Rust counterpart — every language has 35 non-empty trophy texts without the separator bar, within the runtime's 63- and 159-byte buffers (src/core/env.rs::tests::trophy_texts_file_has_35_rows_and_the_labels checks the 35 rows in Portuguese only) |
| `tests/test_trophies.py::TrophyTexts::test_text_falls_back_to_english` | `src/core/i18n.rs::tests::trophies_fall_back_to_english` | ported |
| `tests/test_trophies.py::PageTexts::test_every_language_has_the_same_texts` | — | dropped: no Rust counterpart — every language has the same trophy page texts, non-empty and with the same {} fields |
| `tests/test_trophies.py::PageTexts::test_launcher_tables_carry_them` | `src/core/i18n.rs::tests::trophies_fall_back_to_english` | replaced by `src/core/i18n.rs::tests::trophies_fall_back_to_english` |
| `tests/test_trophies.py::TrophyPackage::test_icons_are_read_by_id` | `src/core/trophies.rs::tests::icons_are_read_by_id` | ported |
| `tests/test_trophies.py::TrophyPackage::test_bad_packages_give_nothing` | `src/core/trophies.rs::tests::bad_packages_give_nothing` | ported |
| `tests/test_trophies.py::TrophyPackage::test_unlocked_bits` | `src/core/trophies.rs::tests::unlocked_bits` | ported |
| `tests/test_trophies.py::TrophyPackage::test_tier_follows_the_colour_of_the_icon` | `src/core/trophies.rs::tests::tier_follows_the_colour_of_the_icon` | ported |
| `tests/test_button_icons.py::ButtonIconTest::test_every_game_button_has_a_playstation_icon` | `src/ui/widgets/button_icon.rs::tests::every_game_button_has_a_playstation_icon` | ported |
| `tests/test_button_icons.py::ButtonIconTest::test_every_gamepad_button_has_an_icon_in_both_styles` | `src/ui/widgets/button_icon.rs::tests::every_gamepad_button_has_an_icon_in_both_styles` | ported |
| `tests/test_button_icons.py::ButtonIconTest::test_styles_differ_where_the_controllers_do` | `src/ui/widgets/button_icon.rs::tests::styles_differ_where_the_controllers_do` | ported |
| `tests/test_button_icons.py::ButtonIconTest::test_draw_fills_and_clears_the_canvas` | `tests/ui_snapshots.rs::controls`, `src/ui/widgets/button_icon.rs::tests::styles_differ_where_the_controllers_do` | replaced by `tests/ui_snapshots.rs::controls`, `src/ui/widgets/button_icon.rs::tests::styles_differ_where_the_controllers_do` |

## Known deviations from the Python

1. Snapshot pixel comparison runs on Windows only; other hosts render and smoke-check (fonts: `Font::MONOSPACE` and Arabic glyphs come from the OS) — spec §7 asked for both CI runners.
2. `--install-update` is native (`src/core/update.rs`, the 2.8.0 cutover); `--script` no longer exists and is an ordinary argument that opens the window. `--run` and `--play` start the Rust runner (`src/runner/`), which needs no Python.
3. The Steam Deck `render_resolution` default is not ported.
4. On Save the Python resets a stored choice that is absent from its option list to the first option and converts value types; the Rust keeps stored values except the monitor.
5. In Arabic the bar, the drawer side, the drawer header and the setting rows (name, control and note, check boxes, folder rows and section headings) are mirrored (R68). What stays left to right: the bar's tab order inside the right-aligned group and the pages' own layouts (the Trophies list, the Controls mapping table, the Mods list and presets, the Log button row and the order of the Files buttons, whose row is mirrored like the others).
6. The fire shader draws its particles additively while the canvas fallback is source-over.
7. `UpdateState::Failed` is unused — the update worker reports failures through the status line.
8. Tab traversal (spec §4.6) is not implemented: iced 0.14 cannot focus buttons or pick lists; a focus ring is follow-up work (R67).
9. `launcher-rust/build.rs` embeds `assets/spartanport.ico` in the Windows exe (R92, M14); the exe has no application manifest until packaging. The desktop shortcut takes the port's `launcher/spartanport.ico` when it is there (as the Python run from source does) and the exe's icon otherwise (the frozen Python always took the exe's).
10. `--version` prints only when started from a terminal: the launcher is a windowed program on Windows and attaches to the parent console when there is one (R65).
11. An unreadable `settings.json` or `spartanport.ini` gives one warning at start (spec §6, R69; the Python said nothing, and stopped on an ini that is not UTF-8), and until a Save or PLAY closing the window, opening the ini and Add to Steam leave both files as they are (R75; the Python wrote them); a corrupt `mods_presets.json` lists no preset and is not replaced by Save or Delete preset (the Python replaced it).
12. The Home background is the launcher's key art (`launcher-rust/assets/background.png`), not the dump's `sce_sys/pic1.png` (R72).
13. `game_dir` starts empty on Windows too (the Python guesses `<port parent>/CUSA01623`); the player browses to the folder (R83).
14. Without a game in the game folder the Home has no "Choose the game folder first." line: the game chip ("✗ No eboot.bin in the game folder") says it, and the line under the settings shows the in-game menu hint (R85). With a game, that line names the patches the last start left out ("{} patches not found in this eboot.bin at the last start (see Log)") instead of the Python's whole summary.
15. GUI Scale offers 1x (the default) and 1.5x, both absolute on any desktop (R90; the Python offered Auto, the default, 1x and 2x). A stored "auto" loads as 1x and a stored "2" as 1.5x; `tests/fixtures.rs` expects `"ui_scale": "1"` where the Python's fixtures hold its default.
16. Discord Rich Presence is on by default (R91; the Python's default was off): a new `settings.json` holds `"discord_rpc": true`, and the game gets `SPARTAN_DISCORD=1` unless the player switches it off. `tests/fixtures.rs` lists it with `ui_scale` in `DECIDED_DEFAULTS`, the only defaults allowed to differ from the Python's fixtures.
17. When the game exits with advice the Rust opens the Log page and scrolls to its end (the 2.7.0 Python scrolls the Advanced page to its log section). The Delete shader cache button of Performance and the Clear shader cache button of Advanced ask the same question, as the Python's two buttons do.
18. PLAY checks the port folder for `run.py` and `bin/spartanport-probe` (or `out/`) before starting and names what is missing, with the releases page as the fix (the Python lets run.py fail with its developer message). The Windows exe links the C runtime statically (`.cargo/config.toml`), so it needs no VCRUNTIME140.dll.

## Partly covered

Rows marked `ported` or `replaced by` whose Rust tests cover the behaviour but not every assertion of
the Python test:

- `tests/test_launcher_artwork.py::LauncherArtworkTest::test_missing_artwork_still_uses_the_existing_placeholder`: only the Home snapshot covers it (its fixture has no pic1.png, and the Home shows the launcher's key art whatever the dump has, R72; the pixels are compared on Windows only); the Rust draws the logotype instead of a text placeholder.
- `tests/test_launcher_artwork.py::LauncherArtworkTest::test_missing_artwork_shows_the_ports_logo`: only the Home snapshot covers it (its fixture has no pic1.png, and the Home shows the launcher's key art whatever the dump has, R72; the pixels are compared on Windows only).
- `tests/test_launcher_close_on_play.py::CloseOnPlayTest::test_real_child_keeps_writing_after_window_is_hidden`: no Rust test writes more than a pipe buffer while the window is hidden.
- `tests/test_launcher_controls.py::ControlMapTest::test_new_cheats_default_to_disabled`: the cheats being ini flags is not asserted.
- `tests/test_launcher_controls.py::ControlMapTest::test_more_keys_can_be_bound`: Pause, Scroll Lock and Keypad 0 are not pressed.
- `tests/test_launcher_controls.py::ControlMapTest::test_captured_names_reach_the_game`: Keypad Enter, minus and F5 are not run through control_map.
- `tests/test_launcher_controls.py::ControlMapTest::test_actions_belong_to_the_keyboard_table`: dodge being absent from the gamepad table, and dodge=V in the key map, are not asserted.
- `tests/test_launcher_controls.py::ControlMapTest::test_environment`: the switches (partial_height_views, parallel_warmup and background_warmup off, frame_capture_key on) and the ignored extra_env are not asserted; the defaults are.
- `tests/test_launcher_controls.py::ControlMapTest::test_settings_keys_kept`: an old player_name never reaching SPARTAN_USER_NAME is not asserted.
- `tests/test_launcher_controls.py::ControlMapTest::test_graphics_defaults_match_the_game`: the FPS limit, engine FPS and frames-queued value lists are not asserted (only that their values are unique).
- `tests/test_launcher_controls.py::GameFilesTest::test_missing_game_files`: the unreadable state and the Portuguese labels are not asserted.
- `tests/test_launcher_controls.py::GameFilesTest::test_system_files_beside_a_pkg_extraction`: a copy cut short, an unreadable param.sfo, a file landing on a game folder and a drive root are not asserted.
- `tests/test_launcher_controls.py::GameFilesTest::test_play_offers_to_adopt_the_system_files`: the placeholder param.sfo cases, an update at the base version and an update folder without eboot.bin are not run.
- `tests/test_launcher_controls.py::GpuChoiceTest::test_list_gpus_failures_leave_only_automatic`: only a missing tool is asserted (not a timeout, a start failure or the 5 s limit).
- `tests/test_launcher_desktop.py::DesktopIntegrationTest::test_the_shells_variable_does_not_leak_into_these_tests`: holds because integrate_desktop reads only the environment it is given; no Rust test sets the shell's variable.
- `tests/test_launcher_desktop.py::DesktopIntegrationTest::test_a_missing_or_unreadable_applications_folder_means_no_other_tool`: an unreadable applications folder is not asserted.
- `tests/test_launcher_desktop.py::DesktopIntegrationTest::test_a_broken_file_never_stops_the_start`: a broken neighbour across starts stands in for a failing scan.
- `tests/test_launcher_desktop.py::LinuxShortcutTest::test_the_shells_variable_is_not_read_when_the_environment_is_given`: holds because write_linux_shortcut reads only the environment it is given.
- `tests/test_launcher_exit_hint.py::ExitHintTest::test_other_lines_have_no_hint`: the other debug exception code (0xa0020014) is not asserted.
- `tests/test_launcher_exit_hint.py::ExitHintTest::test_play_without_window_logs_hint_and_shows_messagebox`: the advice reaches the test's callback (`play_with`'s `notify`); `play`'s own callback, `MessageBoxW` when no console is attached, is not run.
- `tests/test_launcher_exit_hint.py::ExitHintTest::test_play_without_window_clean_exit_does_not_show_messagebox`: the test's callback (`play_with`'s `notify`) stands for the message box and is not called after a clean exit; `play`'s own callback, `MessageBoxW` when no console is attached, is not run.
- `tests/test_launcher_font.py::FontFileTest::test_font_file_is_a_launcher_asset_under_port_dir`: the Rust embeds the face instead of reading launcher/Cinzel-Black.ttf.
- `tests/test_launcher_font.py::HeadingFontTest::test_english_titles_use_the_loaded_font`: only the snapshots show it (pixels compared on Windows only).
- `tests/test_launcher_font.py::HeadingFontTest::test_game_name_uses_the_font_in_every_language`: the Rust name is the logotype image, the same in every language (snapshots).
- `tests/test_launcher_font.py::LatinHeadingsTest::test_latin_script_languages_use_the_loaded_font`: only Portuguese is in the snapshots.
- `tests/test_launcher_game_files.py::LauncherGameFilesTests::test_cancel_then_copy_keeps_existing_files_and_reports_missing_module`: the missing module named after the copy is not asserted.
- `tests/test_launcher_generators.py::LauncherGeneratorOutput::test_fixture_cases_exist_with_every_file`: the spartanport.ini of the defaults and legacy_controls cases is read by no Rust test.
- `tests/test_launcher_mod_resolutions.py::ModResolutionsTest::test_region_mods_follow_the_game_selected_in_the_launcher`: the Mods page dropping the selected mod is not asserted.
- `tests/test_launcher_mod_resolutions.py::ModResolutionsTest::test_a_saved_mod_resolution_stays_chosen_while_its_mod_is_on`: the Home no longer shows the Python's summary (R77), so the "3840 × 1080 · 32:9 (mod)" label is asserted on the Performance resolution list instead.
- `tests/test_launcher_monitor.py::MonitorEnvironmentTest::test_a_saved_index_reaches_the_game`: the first monitor (0) and a number value are not asserted.
- `tests/test_launcher_monitor.py::MonitorEnvironmentTest::test_an_invalid_choice_sends_nothing`: only -1 is asserted among the invalid values.
- `tests/test_launcher_monitor.py::MonitorLauncherTest::test_graphics_page_offers_the_choice`: the probe's window system, the Portuguese label and the refresh after a window system change are not asserted.
- `tests/test_launcher_monitor.py::QuickSettingsMonitorTest::test_quick_settings_offers_the_monitor_choice`: Quick-settings part dropped by spec §4.3; the Graphics row lists the probe's entries (its monitor_pairs, with the labels of the same two monitors); one enumeration under the saved window system is not asserted.
- `tests/test_launcher_monitor.py::QuickSettingsMonitorTest::test_the_indices_of_the_probe_are_what_quick_settings_carries`: Quick-settings part dropped by spec §4.3; the Graphics list keeps the gap (indices 0 and 2, with their labels); picking the entry of index 2 is not asserted.
- `tests/test_launcher_monitor.py::QuickSettingsMonitorTest::test_picking_on_the_graphics_page_follows_in_quick_settings`: Quick-settings part dropped by spec §4.3; picking a Graphics entry stores its index (the pick is built from the row's pairs, as its on_select does, since iced_test cannot open a pick_list).
- `tests/test_launcher_monitor.py::QuickSettingsMonitorTest::test_the_choice_is_saved_and_comes_back_on_reopening`: Quick-settings part dropped by spec §4.3; the monitor setting is saved and reopened (a placeholder value in the every-setting test) and a saved index the list holds is kept; the entry the row shows is not asserted.
- `tests/test_launcher_monitor.py::QuickSettingsMonitorTest::test_without_monitors_quick_settings_still_builds`: Quick-settings part dropped by spec §4.3; an empty list leaves only the primary default as the choice.
- `tests/test_launcher_no_msaa.py::NoMsaaEnvironmentTest::test_auto_leaves_the_game_default_and_keeps_an_inherited_value`: a SPARTAN_NO_MSAA set before the launcher reaching the game on Automatic is not asserted.
- `tests/test_launcher_no_msaa.py::ChosenGpuTest::test_portuguese_texts`: only the title, the hint and the Automatic labels are checked in Portuguese.
- `tests/test_launcher_polish_language.py::PolishLanguageTest::test_russian_and_english_fallbacks`: the Russian label of Polish itself is not asserted.
- `tests/test_launcher_portable_settings.py::PortableSettingsMigrationTest::test_copies_legacy_config_and_icon_when_portable_missing`: `game_icon.ico` is no longer copied (the runner spec, §2 "Ícone"); only `settings.json` is.
- `tests/test_launcher_portable_settings.py::PortableSettingsMigrationTest::test_does_not_overwrite_existing_portable_config`: the icon half of the Python test is gone with the copy of `game_icon.ico`.
- `tests/test_launcher_run_log.py::RunLogTest::test_read_output_keeps_draining_when_the_log_fails`: the Rust log fails at opening (an empty path), not at a later write, while the game's lines are drained.
- `tests/test_launcher_save.py::SaveButtonTest::test_failed_replace_keeps_files_and_changes_pending`: the Rust test runs on Unix only (a read-only folder).
- `tests/test_launcher_save.py::SaveButtonTest::test_repeated_language_switching_preserves_page_and_cleans_traces`: the Tk variable traces have no Rust counterpart.
- `tests/test_launcher_save.py::EverySettingSavesTest::test_all_settings_save_without_detected_gpus`: the graphics card list holding only Automatic is not asserted.
- `tests/test_launcher_scaling.py::ScaleSelectionTest::test_manual_factors_are_absolute_even_on_a_scaled_desktop`: the factors are 1 and 1.5 since R90; a stored "2" loads as 1.5.
- `tests/test_launcher_scaling.py::NativeScaleTest::test_windows_uses_native_scale_without_x11_queries`: the scale comes from winit; the Rust test starts from the scale message.
- `tests/test_launcher_scaling.py::LauncherScalingTest::test_saved_scale_is_applied_when_a_new_window_starts`: a saved 1 on a 150 % desktop, not a saved 2.
- `tests/test_launcher_scaling.py::LauncherScalingTest::test_hint_labels_keep_their_smaller_font`: only the snapshots show it (pixels compared on Windows only).
- `tests/test_launcher_stream_memory.py::StreamMemoryEnvironmentTest::test_every_selection_overrides_inherited_environment_without_changing_it`: only host is asserted; the launcher's own environment is a copy in the Rust.
- `tests/test_launcher_stream_memory.py::StreamMemoryLauncherTest::test_ui_selection_persists_reopens_and_reverts_on_next_launch`: the generic every-setting test stores a placeholder value; the choice reaching PLAY is not asserted.
- `tests/test_launcher_update.py::ReplaceAppImageTest::test_a_matching_download_replaces_the_appimage_and_makes_it_executable`: the Rust test runs the shared download helper; replace_appimage and the executable bit are not asserted.
- `tests/test_trophies.py::PageTexts::test_launcher_tables_carry_them`: only the Portuguese Trophies title is asserted; the Rust reads the page texts from the trophy table itself.
- `tests/test_button_icons.py::ButtonIconTest::test_draw_fills_and_clears_the_canvas`: the snapshot draws the PlayStation icons only.

## Gaps

The 98 rows marked `dropped: no Rust counterpart`: Python behaviour that no Rust test covers.

- `tests/test_launcher_artwork.py::LauncherArtworkTest::test_cover_falls_back_to_base_but_version_stays_from_update` — with an update that has no pic1.png the cover falls back to the base game's while the version shown stays the update's
- `tests/test_launcher_artwork.py::LauncherArtworkTest::test_update_artwork_overrides_base_when_present` — the update's pic1.png is the cover when it has one
- `tests/test_launcher_artwork.py::LauncherArtworkTest::test_cover_falls_back_to_base_when_only_the_icon_was_updated` — an update with only icon0.png keeps the base game's pic1.png as the cover
- `tests/test_launcher_artwork.py::LauncherArtworkTest::test_without_an_update_the_base_cover_is_used` — without an update folder the base game's pic1.png is the cover
- `tests/test_launcher_controls.py::DiskModsTest::test_frozen_launcher_uses_disk_loader_in_every_import` — the frozen launcher imports scripts/mods.py from the port folder over its bundled copy (the Rust mods code is compiled in)
- `tests/test_launcher_controls.py::DiskModsTest::test_missing_disk_loader_uses_the_bundle` — without scripts/mods.py on disk the frozen launcher uses its bundled copy
- `tests/test_launcher_controls.py::DiskModsTest::test_invalid_disk_loader_has_a_clear_error_and_keeps_the_bundle` — a broken scripts/mods.py raises an error naming the file and keeps the bundled copy
- `tests/test_launcher_controls.py::ControlMapTest::test_pipeline_binary_cache_switch` — pipeline_binary_cache off sends SPARTAN_PIPELINE_BINARIES=0 (the default 1 is only in the environment fixtures)
- `tests/test_launcher_controls.py::ControlMapTest::test_stick_prompts_reach_the_game` — stick_prompts reaches SPARTAN_STICK_PROMPTS (mouse is sent, an unknown value is dropped, an inherited value is replaced); only the keyboard default is in the environment fixtures
- `tests/test_launcher_controls.py::ControlMapTest::test_temporal_upscaler_stays_off_with_a_saved_2_1_8_switch` — a saved experimental_upscaler=true still sends SPARTAN_EXPERIMENTAL_UPSCALER=0
- `tests/test_launcher_controls.py::ControlMapTest::test_glyph_style_choices_reach_the_game` — glyph_style auto, xbox, nintendo and keyboard reach SPARTAN_GLYPH_STYLE
- `tests/test_launcher_controls.py::ControlMapTest::test_unknown_glyph_style_is_off` — an unknown glyph_style sends no SPARTAN_GLYPH_STYLE
- `tests/test_launcher_controls.py::ControlMapTest::test_inherited_glyph_style_is_dropped_when_off` — an inherited SPARTAN_GLYPH_STYLE is dropped while the style is off and replaced when one is chosen
- `tests/test_launcher_controls.py::GameFilesTest::test_unreadable_executable_is_named_before_starting` — a game file that cannot be opened is reported as unreadable
- `tests/test_launcher_controls.py::PadStyleTest::test_connected_gamepad_skips_a_probe_without_its_dlls` — on Windows the gamepad probe is not run without its SDL3.dll
- `tests/test_launcher_controls.py::PadStyleTest::test_connected_gamepad_reads_the_probe` — the probe's "Gamepad: type=" line means a gamepad, "No gamepad" none, a timeout unknown (only a missing probe is asserted)
- `tests/test_launcher_controls.py::PadStyleTest::test_nintendo_icons` — the Nintendo icons put B on the south button and ZL on the left trigger
- `tests/test_launcher_desktop.py::DesktopIntegrationTest::test_without_the_environment_argument_the_real_one_is_read` — the start-up integration honours DESKTOPINTEGRATION from the real environment
- `tests/test_launcher_font.py::FontFileTest::test_packages_ship_the_font_license` — the Cinzel OFL license text ships with the face (launcher-rust/assets/OFL-Cinzel.txt exists; no test checks it or its packaging)
- `tests/test_launcher_font.py::ChineseFontTest::test_chinese_prefers_windows_cjk_font_for_body_and_headings` — Chinese uses an installed CJK face (Microsoft YaHei UI) for the body and the headings
- `tests/test_launcher_font.py::ChineseFontTest::test_chinese_can_use_noto_on_linux` — Chinese uses Noto Sans CJK SC on Linux for the body and serif text
- `tests/test_launcher_font.py::ChineseFontTest::test_english_font_preferences_are_unchanged` — English keeps Segoe UI for the body and Georgia for serif text
- `tests/test_launcher_font.py::HeadingFontTest::test_other_languages_keep_the_fallback` — Russian headings use the fallback face instead of Cinzel, and an explicit fallback is kept
- `tests/test_launcher_font.py::LatinHeadingsTest::test_other_scripts_keep_the_fallback` — Russian and Arabic headings use the fallback face instead of Cinzel (the Rust asks for Cinzel in every language and falls back per glyph)
- `tests/test_launcher_font.py::LatinHeadingsTest::test_the_face_has_every_latin_ui_letter` — every letter of the Latin-script translations is in the Cinzel face
- `tests/test_launcher_game_language.py::GameLanguageTest::test_choosing_brazilian_portuguese_starts_the_game_with_id_17` — choosing Brazilian Portuguese starts the game with SPARTAN_LANGUAGE=17 (the Rust test stops at the stored setting 17)
- `tests/test_launcher_icon.py::WindowIconTest::test_own_ico_is_the_window_icon` — the window carries the port's icon in its big and small sizes
- `tests/test_launcher_icon.py::WindowIconTest::test_own_png_when_the_ico_is_missing` — the PNG stands in as the window icon when the .ico is missing
- `tests/test_launcher_icon.py::WindowIconTest::test_icon_has_every_windows_size` — spartanport.ico holds the 16, 24, 32, 48, 64, 128 and 256 px sizes
- `tests/test_launcher_monitor.py::ListDisplaysTest::test_failures_and_a_bad_exit_list_nothing` — a probe that times out, cannot start or exits non-zero lists no monitors
- `tests/test_launcher_monitor.py::ListDisplaysTest::test_a_good_run_is_parsed_and_captured_as_utf8` — the probe runs as --displays with a 5 s timeout and its output is read as UTF-8 with replacement
- `tests/test_launcher_monitor.py::ListDisplaysTest::test_the_game_backend_sets_the_sdl_driver_for_the_probe` — on Linux the saved window system (x11, wayland) is passed to the probe as SDL_VIDEODRIVER and SDL_VIDEO_DRIVER; Automatic leaves the inherited value
- `tests/test_launcher_monitor.py::MonitorRefreshTest::test_an_unchanged_backend_refreshes_nothing` — refresh_monitors does not list the monitors again while the window system is unchanged
- `tests/test_launcher_monitor.py::MonitorRefreshTest::test_without_a_window_system_choice_nothing_happens` — refresh_monitors does nothing where no window system is selectable (Windows)
- `tests/test_launcher_monitor.py::MonitorRefreshTest::test_a_changed_backend_lists_under_the_new_one` — a changed window system lists the monitors again under it and refreshes the Monitor row
- `tests/test_launcher_monitor.py::MonitorRefreshTest::test_a_choice_the_new_list_still_holds_is_kept` — after a window system change a monitor index the new list holds stays chosen
- `tests/test_launcher_monitor.py::MonitorRefreshTest::test_a_choice_the_new_list_lost_is_the_default` — after a window system change an index the new list lost becomes the primary default
- `tests/test_launcher_monitor.py::MonitorRefreshTest::test_another_monitor_at_the_old_position_is_not_selected` — after a window system change the monitor now at the old position is not chosen
- `tests/test_launcher_monitor.py::MonitorRefreshTest::test_an_unsaved_choice_survives_the_backend_change` — a monitor picked but not saved survives a window system change
- `tests/test_launcher_monitor.py::MonitorRefreshTest::test_the_unsaved_choice_beats_the_different_collected_one` — the monitor on screen wins over the last saved one after a window system change
- `tests/test_launcher_monitor.py::MonitorRefreshTest::test_an_unsaved_choice_lost_by_the_new_list_falls_back_to_the_collected_one` — an unsaved monitor the new list lost falls back to the last saved one
- `tests/test_launcher_monitor.py::MonitorRefreshTest::test_with_neither_choice_in_the_new_list_it_is_the_default` — with neither the shown nor the saved monitor in the new list the choice is the primary default
- `tests/test_launcher_monitor.py::MonitorRefreshTest::test_only_the_monitor_setting_changes` — refresh_monitors changes no setting but the monitor
- `tests/test_launcher_monitor.py::MonitorRefreshTest::test_the_refreshed_choice_is_what_a_launch_sends` — the monitor kept by refresh_monitors is what SPARTAN_MONITOR carries
- `tests/test_launcher_monitor.py::CollectMonitorConsistencyTest::test_an_unchanged_backend_lists_nothing` — Save with an unchanged window system does not list the monitors again
- `tests/test_launcher_monitor.py::CollectMonitorConsistencyTest::test_a_changed_backend_lists_again_before_gathering` — Save with a changed window system lists the monitors again before it writes the window system and monitor
- `tests/test_launcher_monitor.py::CollectMonitorConsistencyTest::test_a_choice_the_new_list_lost_becomes_the_default_in_the_file` — Save writes the primary default when the new list lost the chosen monitor
- `tests/test_launcher_monitor.py::CollectMonitorConsistencyTest::test_a_second_collect_lists_nothing_more` — a second Save does not list the monitors again
- `tests/test_launcher_monitor.py::CollectMonitorConsistencyTest::test_a_choice_not_saved_yet_is_what_collect_writes` — Save writes the monitor on screen after a window system change
- `tests/test_launcher_monitor.py::CollectMonitorConsistencyTest::test_a_choice_not_saved_yet_that_the_new_list_lost_is_the_default` — Save writes the primary default when the new list lost the monitor on screen
- `tests/test_launcher_monitor.py::PlayMonitorConsistencyTest::test_play_without_a_backend_change_lists_nothing` — PLAY with an unchanged window system does not list the monitors again
- `tests/test_launcher_monitor.py::PlayMonitorConsistencyTest::test_play_lists_again_when_the_backend_changed` — PLAY with a changed window system lists the monitors again
- `tests/test_launcher_monitor.py::PlayMonitorConsistencyTest::test_play_validates_the_choice_before_the_game_reads_it` — PLAY resets a monitor the new list lost before the game reads it
- `tests/test_launcher_monitor.py::PlayMonitorConsistencyTest::test_play_delivers_the_backend_and_the_monitor_it_kept` — PLAY sends the new window system (SDL_VIDEODRIVER) with the monitor it kept
- `tests/test_launcher_monitor.py::PlayMonitorConsistencyTest::test_play_delivers_the_default_when_the_choice_was_reset` — PLAY sends no SPARTAN_MONITOR when the choice was reset
- `tests/test_launcher_monitor.py::PlayMonitorConsistencyTest::test_play_keeps_a_choice_not_saved_yet` — PLAY keeps the monitor on screen across a window system change
- `tests/test_launcher_monitor.py::PlayMonitorConsistencyTest::test_play_drops_a_choice_not_saved_yet_that_the_new_list_lost` — PLAY drops an unsaved monitor the new list lost
- `tests/test_launcher_monitor.py::PlayMonitorConsistencyTest::test_a_persisted_pair_reaches_a_play_as_one_environment` — on Linux a saved window system and monitor reach the game as SDL_VIDEODRIVER and SPARTAN_MONITOR
- `tests/test_launcher_monitor.py::CollectCallersTest::test_save_lets_collect_do_the_refresh` — Save reaches the monitor refresh only through collect (no Rust test changes the window system)
- `tests/test_launcher_monitor.py::CollectCallersTest::test_play_lets_collect_do_the_refresh` — PLAY reaches the monitor refresh only through collect (PLAY saving first is in tests/ui_play.rs::play_refuses_without_eboot_and_opens_game_page)
- `tests/test_launcher_monitor.py::CollectCallersTest::test_close_collects_then_destroys_the_window` — closing the window saves the settings first (the Rust launcher does not save on close)
- `tests/test_launcher_monitor.py::CollectCallersTest::test_close_keeps_the_window_when_the_save_fails` — a failed save on close keeps the window open
- `tests/test_launcher_monitor.py::CollectCallersTest::test_open_path_collects_before_opening_a_settings_file` — opening spartanport.ini saves first and opens nothing when the save fails (the Rust Advanced page opens it without saving)
- `tests/test_launcher_monitor.py::CollectCallersTest::test_open_path_opens_a_folder_without_collecting` — opening a folder starts the system opener without saving first (the Rust test only sees the screenshots button emit OpenPath)
- `tests/test_launcher_monitor.py::CollectCallersTest::test_add_to_steam_collects_first` — Add to Steam saves first and stops when the save fails (the Rust Advanced page does not save first)
- `tests/test_launcher_monitor.py::QuickSettingsMonitorTest::test_picking_in_quick_settings_follows_on_the_graphics_page` — the Graphics row shows a monitor picked elsewhere, as the entry of its index (Quick-settings part dropped by spec §4.3)
- `tests/test_launcher_monitor.py::QuickSettingsMonitorTest::test_a_new_window_system_drops_an_index_the_new_list_lost` — both monitor rows drop an index the new window system's list lost
- `tests/test_launcher_monitor.py::QuickSettingsMonitorTest::test_both_rows_follow_a_reordered_list_that_still_holds_the_index` — both monitor rows follow an index that moved in the new list
- `tests/test_launcher_no_msaa.py::NoMsaaEnvironmentTest::test_off_and_on_set_the_variable` — no_msaa Off (1) and On (0) reach SPARTAN_NO_MSAA
- `tests/test_launcher_pages.py::PagesTest::test_showing_a_page_refreshes_what_it_holds` — showing Home refreshes the status, Game the patch list and Mods the mod list (only the Trophies load on opening is asserted)
- `tests/test_launcher_pages.py::PagesTest::test_every_refreshed_page_is_a_sidebar_page` — every page with a refresh is a sidebar page (in the Rust the refresh matches on the Page enum)
- `tests/test_launcher_polish_language.py::PolishLanguageTest::test_polish_is_offered_with_system_id_16` — Polish is offered as game language 16 with the Russian label Польский
- `tests/test_launcher_polish_language.py::PolishLanguageTest::test_choosing_polish_starts_the_game_with_id_16` — choosing Polish starts the game with SPARTAN_LANGUAGE=16
- `tests/test_launcher_polish_language.py::PolishLanguageTest::test_polish_label_is_translated_across_ui_languages` — the Polish label in the twelve translated interface languages
- `tests/test_launcher_polish_language.py::PolishLanguageTest::test_system_service_contract_maps_parameter_1_to_spartanport_language` — the C runtime answers system parameter 1 with SPARTAN_LANGUAGE (a runtime source check, not launcher code)
- `tests/test_launcher_portable_settings.py::PortableSettingsMigrationTest::test_missing_legacy_config_is_a_noop` — without a legacy settings.json the migration creates nothing
- `tests/test_launcher_portable_settings.py::PortableSettingsMigrationTest::test_load_app_settings_migrates_and_merges_defaults` — loading the settings first copies the legacy settings.json, then merges the defaults over it
- `tests/test_launcher_portable_settings.py::PortableSettingsMigrationTest::test_packaging_scripts_preserve_portable_files` — the Windows package and its smoke test keep settings.json and game_icon.ico (packaging is a later sub-project)
- `tests/test_launcher_save.py::SaveButtonTest::test_failed_save_does_not_close_the_launcher` — a failed save on close keeps the launcher open (the Rust launcher does not save on close)
- `tests/test_launcher_save.py::SaveButtonTest::test_failed_save_does_not_start_the_game` — a failed save stops PLAY before the game starts
- `tests/test_launcher_save.py::SaveButtonTest::test_rebuilding_keeps_the_status_of_a_running_game` — a language rebuild keeps the status line of a running game (the Rust test keeps the log and the version menu's release list of an idle game)
- `tests/test_launcher_scaling.py::ScaleSelectionTest::test_ui_scale_does_not_change_the_game_environment` — the GUI scale does not change the game environment
- `tests/test_launcher_scaling.py::LauncherScalingTest::test_save_scales_the_whole_interface_and_preserves_game_settings` — Save with GUI Scale 2 doubles the interface, keeps the page, the ini and the game environment, and back to 1 restores the sizes
- `tests/test_launcher_scaling.py::LauncherScalingTest::test_failed_save_keeps_the_current_scale` — a failed save keeps the current GUI scale
- `tests/test_launcher_translations.py::PortugueseTest::test_every_literal_text_is_translated` — every literal text the launcher translates has a Portuguese text (no Rust test scans the tr() literals of the code)
- `tests/test_launcher_translations.py::PortugueseTest::test_button_names_are_translated` — the controller button names have Portuguese texts
- `tests/test_launcher_translations.py::PortugueseTest::test_the_built_in_patch_notes_are_translated` — the notes of the built-in patches have Portuguese texts
- `tests/test_launcher_translations.py::PortugueseTest::test_resolution_labels` — the native resolution reads Nativa · 1920 × 1080 · 16:9 in Portuguese (the Rust resolution labels are not translated)
- `tests/test_launcher_translations.py::SpanishTest::test_resolution_labels` — the native resolution reads Nativa · 1920 × 1080 · 16:9 in Spanish
- `tests/test_launcher_translations.py::ChineseTest::test_resolution_labels` — the native resolution reads 原生 · 1920 × 1080 · 16:9 in Chinese
- `tests/test_launcher_translations.py::ChineseTest::test_all_known_labels_and_format_fields` — every known text has a Chinese translation with the same {} fields (only the table sizes are compared)
- `tests/test_launcher_translations.py::IndonesianTest::test_resolution_labels` — the native resolution reads Asli · 1920 × 1080 · 16:9 in Indonesian
- `tests/test_launcher_update.py::ReplaceAppImageTest::test_a_symlink_to_the_appimage_replaces_the_file_behind_it` — updating through a symlink replaces the AppImage behind it and keeps the link
- `tests/test_launcher_update.py::ReplaceAppImageTest::test_a_release_without_a_checksum_is_not_installed` — a release that publishes no .sha256 is not installed
- `tests/test_launcher_update.py::ReplaceAppImageTest::test_a_missing_download_leaves_the_appimage_alone` — a download that fails leaves the AppImage as it was, with no partial file
- `tests/test_launcher_upscale.py::UpscalerSettingsTest::test_remaining_choices_have_no_spatial_upscaler` — the upscaler choices are fsr3, taa and off, with no spatial upscaler
- `tests/test_trophies.py::TrophyTexts::test_every_language_lists_all_trophies` — every language has 35 non-empty trophy texts without the separator bar, within the runtime's 63- and 159-byte buffers (src/core/env.rs::tests::trophy_texts_file_has_35_rows_and_the_labels checks the 35 rows in Portuguese only)
- `tests/test_trophies.py::PageTexts::test_every_language_has_the_same_texts` — every language has the same trophy page texts, non-empty and with the same {} fields
