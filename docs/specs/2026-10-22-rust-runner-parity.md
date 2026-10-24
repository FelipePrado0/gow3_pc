# Rust runner parity: Python tests → Rust tests

Every test method of the 18 Python files that test the game runner (`tests/test_prepare.py`,
`test_link_libc.py`, `test_link_modules.py`, `test_isa_rewrite.py`, `test_run_isa.py`,
`test_patches.py`, `test_mods.py`, `test_mod_conflicts.py`, `test_glyphs.py`, `test_content.py`,
`test_guest_memory.py`, `test_graphics_options.py`, `test_save_backup.py`, `test_pause_menu.py`,
`test_native_cheats.py`, `test_legacy.py`, `test_discord_presence.py`, `test_game_update.py`; the
Python runner 2.7.0, reference `003e42b`) with the Rust test of `launcher-rust/` that covers its
behaviour (spec `2026-10-22-rust-runner-design.md` §7). `tests/test_runner_parity_doc.py` keeps this
map honest through `tests/parity_doc.py`, the checker it shares with the launcher map
(`2026-10-21-rust-launcher-parity.md`): it finds the methods by scanning those files, and fails when
one has no row, a row has no status, or a named Rust test does not exist. That checker is not itself a
row.

Release gate: checked on 2026-10-23 at the end of the runner work, on Windows, against the real game
dump `CUSA01623` (`APP_VER` 01.02, `VERSION` 01.00), read only:
`SPARTAN_GAME_DIR=<game> cargo test --no-fail-fast` in `launcher-rust/`.

Result: 864 tests passed, 0 failed, with the dump set. All 27 tests of `tests/dump.rs` ran on the real files (the image files `prepare`, `link_libc`, `link_modules` and `isa_rewrite` make from the real `eboot.bin`, `boot-linked.bin` included, its patches, the pause menu and cheat scripts of `R_UI.WAD` and the glyph WADs, each compared with the Python's). The launcher's settings take `SPARTAN_GAME_DIR` from the environment the `Paths` were built with, so the UI tests, which build theirs without it, do not see the dump.

The tests that need the dump return at once without `SPARTAN_GAME_DIR`, as the Python ones skip; with
it they run on the real `eboot.bin` and `R_UI.WAD`. The Unix paths (Linux sockets, `kill(pid, 0)`,
Ctrl+C) ran in WSL during development only; the whole suite has not run on Linux yet.

Status:

- `ported`: the Rust test is the port of the Python one (same cut, often the same name).
- `replaced by <test>`: Rust tests that cover the behaviour under a different cut; the Rust column
  names the same tests.
- `dropped: <reason>`: mechanics with no Rust equivalent and no behaviour to port (the C runtime's own
  test, the Python-only guard for a missing binding).
- `dropped: spec §<section> <reason>`: behaviour the spec replaced on purpose (none in this map).
- `dropped: no Rust counterpart — <what it tested>`: behaviour no Rust test covers. Each is listed
  again under [Gaps](#gaps).

Rust tests are named `file::name` under `launcher-rust/`: `tests/runner.rs::name` for integration
tests, `src/runner/prepare.rs::tests::name` for unit tests (the `tests` segment is the module).
`tests/dump.rs` rows run only with `SPARTAN_GAME_DIR` set. Rows whose Rust tests leave part of the
Python assertions uncovered are listed under [Partly covered](#partly-covered).
`tests/test_graphics_options.py` defines `test_corrupt_state_file_preserves_current_settings` twice;
the second definition is the live one and has the only row (28 definitions, 27 rows).
`test_patches.py::test_unavailable_frame_rate_patch_falls_back_to_the_highest_lower_one` tests
`graphics_options.restrict_to_available`; its row names the Rust test of `graphics_options.rs`.
`test_game_update.py` has its own rows against `src/runner/game_update.rs`.
The rows against `src/core/legacy.rs`, `src/core/save_backup.rs` and the discovery, conflict and info tests of `src/core/mods.rs` are ports from the launcher work (sub-project 1) that the runner kept using.

## Counts

| File | Rows | Ported | Replaced | Dropped (C runtime/Python-only) | Dropped (spec) | Gaps |
|---|---|---|---|---|---|---|
| `tests/test_prepare.py` | 13 | 13 | 0 | 0 | 0 | 0 |
| `tests/test_link_libc.py` | 9 | 9 | 0 | 0 | 0 | 0 |
| `tests/test_link_modules.py` | 2 | 2 | 0 | 0 | 0 | 0 |
| `tests/test_isa_rewrite.py` | 21 | 19 | 1 | 1 | 0 | 0 |
| `tests/test_run_isa.py` | 4 | 4 | 0 | 0 | 0 | 0 |
| `tests/test_patches.py` | 30 | 30 | 0 | 0 | 0 | 0 |
| `tests/test_mods.py` | 43 | 41 | 1 | 0 | 0 | 1 |
| `tests/test_mod_conflicts.py` | 5 | 5 | 0 | 0 | 0 | 0 |
| `tests/test_glyphs.py` | 37 | 37 | 0 | 0 | 0 | 0 |
| `tests/test_content.py` | 5 | 2 | 0 | 3 | 0 | 0 |
| `tests/test_guest_memory.py` | 8 | 8 | 0 | 0 | 0 | 0 |
| `tests/test_graphics_options.py` | 27 | 21 | 6 | 0 | 0 | 0 |
| `tests/test_save_backup.py` | 10 | 10 | 0 | 0 | 0 | 0 |
| `tests/test_pause_menu.py` | 19 | 19 | 0 | 0 | 0 | 0 |
| `tests/test_native_cheats.py` | 9 | 9 | 0 | 0 | 0 | 0 |
| `tests/test_legacy.py` | 34 | 18 | 6 | 0 | 0 | 10 |
| `tests/test_discord_presence.py` | 26 | 23 | 2 | 0 | 0 | 1 |
| `tests/test_game_update.py` | 6 | 6 | 0 | 0 | 0 | 0 |
| **Total** | **308** | **276** | **16** | **4** | **0** | **12** |

## Map

| Python test | Rust test (file::name) | Status |
|---|---|---|
| `tests/test_prepare.py::SelfTests::test_unpacked_elf_has_the_same_headers_and_payload_as_self` | `src/runner/prepare.rs::tests::unpacked_elf_has_the_same_headers_and_payload_as_self` | ported |
| `tests/test_prepare.py::SelfTests::test_unpacked_elf_rejects_truncated_segments` | `src/runner/prepare.rs::tests::unpacked_elf_rejects_truncated_segments` | ported |
| `tests/test_prepare.py::SelfTests::test_unpacked_elf_rejects_other_architectures_and_byte_orders` | `src/runner/prepare.rs::tests::unpacked_elf_rejects_other_architectures_and_byte_orders` | ported |
| `tests/test_prepare.py::SelfTests::test_unpacked_elf_rejects_truncated_program_headers` | `src/runner/prepare.rs::tests::unpacked_elf_rejects_truncated_program_headers` | ported |
| `tests/test_prepare.py::SelfTests::test_unpacked_elf_checks_image_limit_before_allocating` | `src/runner/prepare.rs::tests::unpacked_elf_checks_image_limit_before_allocating` | ported |
| `tests/test_prepare.py::SelfTests::test_libc_ret_contract_is_verified_from_symbol_and_code` | `src/runner/prepare.rs::tests::libc_ret_contract_is_verified_from_symbol_and_code` | ported |
| `tests/test_prepare.py::SelfTests::test_report_with_non_latin_title_is_written_as_utf8` | `src/runner/prepare.rs::tests::report_with_non_latin_title_is_written_as_utf8` | ported |
| `tests/test_prepare.py::SelfTests::test_file_offset_is_not_virtual_address` | `src/runner/prepare.rs::tests::file_offset_is_not_virtual_address` | ported |
| `tests/test_prepare.py::SelfTests::test_truncated_payload_rejected` | `src/runner/prepare.rs::tests::truncated_payload_rejected` | ported |
| `tests/test_prepare.py::SelfTests::test_encrypted_or_compressed_payload_rejected` | `src/runner/prepare.rs::tests::encrypted_or_compressed_payload_rejected` | ported |
| `tests/test_prepare.py::SelfTests::test_missing_required_segment_rejected` | `src/runner/prepare.rs::tests::missing_required_segment_rejected` | ported |
| `tests/test_prepare.py::SelfTests::test_bad_program_header_index_rejected` | `src/runner/prepare.rs::tests::bad_program_header_index_rejected` | ported |
| `tests/test_prepare.py::SelfTests::test_empty_game_files_are_named` | `src/runner/prepare.rs::tests::empty_game_files_are_named` | ported |
| `tests/test_link_libc.py::ModuleTests::test_module_accepts_unpacked_elf_with_the_same_exports` | `src/runner/link_libc.rs::tests::module_accepts_unpacked_elf_with_the_same_exports` | ported |
| `tests/test_link_libc.py::ModuleTests::test_module_reads_the_file_once_and_reports_its_hash` | `src/runner/link_libc.rs::tests::module_reads_the_file_once_and_reports_its_hash` | ported |
| `tests/test_link_libc.py::LinkTests::test_different_local_ids_bind_same_identity` | `src/runner/link_libc.rs::tests::different_local_ids_bind_same_identity` | ported |
| `tests/test_link_libc.py::LinkTests::test_same_nid_wrong_library_version_does_not_bind` | `src/runner/link_libc.rs::tests::same_nid_wrong_library_version_does_not_bind` | ported |
| `tests/test_link_libc.py::LinkTests::test_tls_module_id_is_literal_not_image_pointer` | `src/runner/link_libc.rs::tests::tls_module_id_is_literal_not_image_pointer` | ported |
| `tests/test_link_libc.py::LinkTests::test_unsupported_tls_symbol_rejected` | `src/runner/link_libc.rs::tests::unsupported_tls_symbol_rejected` | ported |
| `tests/test_link_libc.py::LinkTests::test_relocation_outside_segments_rejected` | `src/runner/link_libc.rs::tests::relocation_outside_segments_rejected` | ported |
| `tests/test_link_libc.py::LinkTests::test_fs_thread_pointer_loads_are_rewritten_to_gs` | `src/runner/link_libc.rs::tests::fs_thread_pointer_loads_are_rewritten_to_gs` | ported |
| `tests/test_link_libc.py::LinkTests::test_fs_loads_outside_executable_segments_are_kept` | `src/runner/link_libc.rs::tests::fs_loads_outside_executable_segments_are_kept` | ported |
| `tests/test_link_modules.py::ModuleTests::test_module_accepts_unpacked_elf_with_the_same_exports` | `src/runner/link_modules.rs::tests::module_accepts_unpacked_elf_with_the_same_exports` | ported |
| `tests/test_link_modules.py::ModuleTests::test_module_reads_the_file_once_and_reports_its_hash` | `src/runner/link_modules.rs::tests::module_reads_the_file_once_and_reports_its_hash` | ported |
| `tests/test_isa_rewrite.py::ScanSegmentTests::test_blsr_is_a_jmp_site` | `src/runner/isa_rewrite.rs::tests::blsr_is_a_jmp_site` | ported |
| `tests/test_isa_rewrite.py::ScanSegmentTests::test_bextr_rip_relative_memory_source` | `src/runner/isa_rewrite.rs::tests::bextr_rip_relative_memory_source` | ported |
| `tests/test_isa_rewrite.py::ScanSegmentTests::test_sixteen_bit_lzcnt_and_sixty_four_bit_tzcnt` | `src/runner/isa_rewrite.rs::tests::sixteen_bit_lzcnt_and_sixty_four_bit_tzcnt` | ported |
| `tests/test_isa_rewrite.py::ScanSegmentTests::test_four_byte_store_relocates_a_plain_next_instruction` | `src/runner/isa_rewrite.rs::tests::four_byte_store_relocates_a_plain_next_instruction` | ported |
| `tests/test_isa_rewrite.py::ScanSegmentTests::test_four_byte_site_falls_back_to_int3` | `src/runner/isa_rewrite.rs::tests::four_byte_site_falls_back_to_int3` | ported |
| `tests/test_isa_rewrite.py::ScanSegmentTests::test_next_instruction_that_is_a_branch_target_blocks_relocation` | `src/runner/isa_rewrite.rs::tests::next_instruction_that_is_a_branch_target_blocks_relocation` | ported |
| `tests/test_isa_rewrite.py::ScanSegmentTests::test_unsupported_encodings` | `src/runner/isa_rewrite.rs::tests::unsupported_encodings` | ported |
| `tests/test_isa_rewrite.py::ScanSegmentTests::test_negative_displacement_is_signed` | `src/runner/isa_rewrite.rs::tests::negative_displacement_is_signed` | ported |
| `tests/test_isa_rewrite.py::ScanSegmentTests::test_sib_memory_operand` | `src/runner/isa_rewrite.rs::tests::sib_memory_operand` | ported |
| `tests/test_isa_rewrite.py::ScanSegmentTests::test_non_sites_are_ignored` | `src/runner/isa_rewrite.rs::tests::non_sites_are_ignored` | ported |
| `tests/test_isa_rewrite.py::ReadBootLinkedTests::test_read_boot_linked_returns_segments_and_image` | `src/runner/isa_rewrite.rs::tests::read_boot_linked_returns_segments_and_image` | ported |
| `tests/test_isa_rewrite.py::ReadBootLinkedTests::test_read_boot_linked_rejects_bad_magic_and_truncation` | `src/runner/isa_rewrite.rs::tests::read_boot_linked_rejects_bad_magic_and_truncation` | ported |
| `tests/test_isa_rewrite.py::ReadBootLinkedTests::test_records_are_sixty_four_bytes` | `src/runner/isa_rewrite.rs::tests::records_are_sixty_four_bytes` | ported |
| `tests/test_isa_rewrite.py::ReadBootLinkedTests::test_main_without_iced_x86` | — | dropped: Python-only: the message of isa_rewrite.py when the iced-x86 binding is not installed; the Rust links iced-x86 in |
| `tests/test_isa_rewrite.py::TableTests::test_encode_table_header_and_record` | `src/runner/isa_rewrite.rs::tests::encode_table_header_and_record` | ported |
| `tests/test_isa_rewrite.py::TableTests::test_encode_table_keeps_negative_displacement` | `src/runner/isa_rewrite.rs::tests::encode_table_keeps_negative_displacement` | ported |
| `tests/test_isa_rewrite.py::TableTests::test_encode_table_sorts_records_by_offset` | `src/runner/isa_rewrite.rs::tests::encode_table_sorts_records_by_offset` | ported |
| `tests/test_isa_rewrite.py::TableTests::test_summary_line` | `src/runner/isa_rewrite.rs::tests::summary_line` | ported |
| `tests/test_isa_rewrite.py::TableTests::test_main_writes_table_for_executable_segments_only` | `src/runner/isa_rewrite.rs::tests::main_writes_table_for_executable_segments_only` | ported |
| `tests/test_isa_rewrite.py::TableTests::test_main_reports_unavailable_input` | `src/runner/isa_rewrite.rs::tests::main_reports_unavailable_input` | ported |
| `tests/test_isa_rewrite.py::TableTests::test_fixture_file_is_current` | `tests/runner_fixtures.rs::isa_table_matches_python` | replaced by `tests/runner_fixtures.rs::isa_table_matches_python` |
| `tests/test_run_isa.py::RunIsaTests::test_parse_cpu_features` | `src/runner/mod.rs::tests::parse_cpu_features_reads_the_missing_line` | ported |
| `tests/test_run_isa.py::RunIsaTests::test_rewrite_wanted_follows_cpu_by_default` | `src/runner/mod.rs::tests::rewrite_wanted_follows_cpu_by_default` | ported |
| `tests/test_run_isa.py::RunIsaTests::test_rewrite_forced` | `src/runner/mod.rs::tests::rewrite_forced` | ported |
| `tests/test_run_isa.py::RunIsaTests::test_rewrite_disabled` | `src/runner/mod.rs::tests::rewrite_disabled` | ported |
| `tests/test_patches.py::ExternalPatchTests::test_selection_and_unsupported_lines` | `src/core/patches.rs::compile_tests::selection_and_unsupported_lines` | ported |
| `tests/test_patches.py::ExternalPatchTests::test_an_older_builds_patch_file_left_by_an_update_is_not_external` | `src/core/patches.rs::compile_tests::an_older_builds_patch_file_left_by_an_update_is_not_external` | ported |
| `tests/test_patches.py::ExternalPatchTests::test_built_in_file_is_not_external` | `src/core/patches.rs::compile_tests::built_in_file_is_not_external` | ported |
| `tests/test_patches.py::GameProfileTests::test_profiles_by_title_id` | `src/core/patches.rs::compile_tests::profiles_by_title_id` | ported |
| `tests/test_patches.py::GameProfileTests::test_unavailable_frame_rate_patch_falls_back_to_the_highest_lower_one` | `src/runner/graphics_options.rs::tests::unavailable_frame_rate_patch_falls_back_to_the_highest_lower_one` | ported |
| `tests/test_patches.py::GameProfileTests::test_spartanport_defaults_and_extras` | `src/core/patches.rs::compile_tests::spartanport_defaults_and_extras` | ported |
| `tests/test_patches.py::GameProfileTests::test_spartanport_resolution_patch_replaces_texture_fix` | `src/core/patches.rs::compile_tests::spartanport_resolution_patch_replaces_texture_fix` | ported |
| `tests/test_patches.py::GameProfileTests::test_profile_by_title_for_any_region` | `src/core/patches.rs::compile_tests::profile_by_title_for_any_region` | ported |
| `tests/test_patches.py::SignatureTests::test_sites_found_anywhere_and_rel32_recomputed` | `src/core/patches.rs::compile_tests::sites_found_anywhere_and_rel32_recomputed` | ported |
| `tests/test_patches.py::SignatureTests::test_ambiguous_signature_is_skipped` | `src/core/patches.rs::compile_tests::ambiguous_signature_is_skipped` | ported |
| `tests/test_patches.py::SignatureTests::test_without_the_image_the_reference_addresses_are_used` | `src/core/patches.rs::compile_tests::without_the_image_the_reference_addresses_are_used` | ported |
| `tests/test_patches.py::SignatureTests::test_data_line_writes_at_its_target` | `src/core/patches.rs::compile_tests::data_line_writes_at_its_target` | ported |
| `tests/test_patches.py::SignatureTests::test_signatures_match_the_full_update_eboot` | `src/core/patches.rs::compile_tests::signatures_match_the_full_update_eboot` | ported |
| `tests/test_patches.py::SignatureTests::test_spartanport_signatures_in_the_reference_and_a_moved_build` | `tests/dump.rs::spartanport_signatures_in_the_reference_and_a_moved_build` | ported |
| `tests/test_patches.py::SignatureTests::test_spartanport_patches_compile` | `src/core/patches.rs::compile_tests::spartanport_patches_compile` | ported |
| `tests/test_patches.py::SignatureTests::test_spartanport_startup_black_screen` | `src/core/patches.rs::compile_tests::spartanport_startup_black_screen` | ported |
| `tests/test_patches.py::SignatureTests::test_spartanport_skip_intro_targets` | `src/core/patches.rs::compile_tests::spartanport_skip_intro_targets` | ported |
| `tests/test_patches.py::SignatureTests::test_spartanport_skip_intro_original_bytes` | `tests/dump.rs::spartanport_skip_intro_original_bytes` | ported |
| `tests/test_patches.py::SignatureTests::test_8k_patch_dimensions_and_memory` | `src/core/patches.rs::compile_tests::the_8k_patch_dimensions_and_memory` | ported |
| `tests/test_patches.py::SignatureTests::test_requirements_from_patch_notes` | `src/core/patches.rs::compile_tests::requirements_from_patch_notes` | ported |
| `tests/test_patches.py::SignatureTests::test_144_frame_rate_patch_has_the_reference_sites_and_value` | `src/core/patches.rs::compile_tests::the_144_frame_rate_patch_has_the_reference_sites_and_value` | ported |
| `tests/test_patches.py::ModPatchTests::test_resolution_names_of_patch_files_smallest_first` | `src/core/patches.rs::compile_tests::resolution_names_of_patch_files_smallest_first` | ported |
| `tests/test_patches.py::ModPatchTests::test_merge_adds_replaces_and_skips_other_titles` | `src/core/patches.rs::compile_tests::merge_adds_replaces_and_skips_other_titles` | ported |
| `tests/test_patches.py::ModPatchTests::test_a_broken_mod_file_is_left_out` | `src/core/patches.rs::compile_tests::a_broken_mod_file_is_left_out` | ported |
| `tests/test_patches.py::ModPatchTests::test_default_enabled_patches_of_a_mod` | `src/core/patches.rs::compile_tests::default_enabled_patches_of_a_mod` | ported |
| `tests/test_patches.py::ModPatchTests::test_the_ultrawide_mod_lists_its_resolutions_and_memory_needs` | `src/core/patches.rs::compile_tests::the_ultrawide_mod_lists_its_resolutions_and_memory_needs` | ported |
| `tests/test_patches.py::ModPatchTests::test_the_ultrawide_mod_camera_caves_form_closed_chains` | `src/core/patches.rs::compile_tests::the_ultrawide_mod_camera_caves_form_closed_chains` | ported |
| `tests/test_patches.py::ModPatchTests::test_taller_than_16_9_changes_no_camera` | `src/core/patches.rs::compile_tests::taller_than_16_9_changes_no_camera` | ported |
| `tests/test_patches.py::ModPatchTests::test_the_ultrawide_mod_is_off_until_a_resolution_is_chosen` | `src/core/patches.rs::compile_tests::the_ultrawide_mod_is_off_until_a_resolution_is_chosen` | ported |
| `tests/test_patches.py::HalfResolutionTests::test_every_resolution_patch_halves_the_output` | `src/core/patches.rs::compile_tests::every_resolution_patch_halves_the_output` | ported |
| `tests/test_mods.py::ModTests::test_merge_replacement_new_file_and_directory_listing` | `src/core/mods.rs::tests::merge_replacement_new_file_and_directory_listing` | ported |
| `tests/test_mods.py::ModTests::test_a_network_share_without_symlinks_or_junctions_still_overlays` | `src/core/mods.rs::tests::a_network_share_without_symlinks_or_junctions_still_overlays` | ported |
| `tests/test_mods.py::ModTests::test_last_mod_wins_and_reordering_changes_winner` | `src/core/mods.rs::tests::last_mod_wins_and_reordering_changes_winner` | ported |
| `tests/test_mods.py::ModTests::test_selection_disable_and_new_mod` | `src/core/mods.rs::tests::selection_disable_and_new_mod` | ported |
| `tests/test_mods.py::ModTests::test_a_mod_with_only_patches_is_a_mod_and_replaces_no_file` | `src/core/mods.rs::tests::a_mod_with_only_patches_is_a_mod_and_replaces_no_file` | ported |
| `tests/test_mods.py::ModTests::test_patch_files_of_a_mod_and_its_wrapper_folder` | `src/core/mods.rs::tests::patch_files_of_a_mod_and_its_wrapper_folder` | ported |
| `tests/test_mods.py::ModTests::test_enabled_patch_files_follow_the_selection` | `src/core/mods.rs::tests::enabled_patch_files_follow_the_selection` | ported |
| `tests/test_mods.py::ModTests::test_a_zip_in_the_mods_folder_is_unpacked_and_is_a_mod` | `src/core/mods.rs::tests::a_zip_in_the_mods_folder_is_unpacked_and_is_a_mod` | ported |
| `tests/test_mods.py::ModTests::test_a_zip_with_a_folder_around_the_mod` | `src/core/mods.rs::tests::a_zip_with_a_folder_around_the_mod` | ported |
| `tests/test_mods.py::ModTests::test_a_changed_zip_is_unpacked_again_without_leftovers` | `src/core/mods.rs::tests::a_changed_zip_is_unpacked_again_without_leftovers` | ported |
| `tests/test_mods.py::ModTests::test_a_folder_the_user_made_is_never_replaced_by_a_zip` | `src/core/mods.rs::tests::a_folder_the_user_made_is_never_replaced_by_a_zip` | ported |
| `tests/test_mods.py::ModTests::test_unsafe_zips_are_refused` | `src/core/mods.rs::tests::unsafe_zips_are_refused` | ported |
| `tests/test_mods.py::ModTests::test_a_zip_that_is_not_a_mod_or_not_a_zip_is_skipped` | `src/core/mods.rs::tests::a_zip_that_is_not_a_mod_or_not_a_zip_is_skipped` | ported |
| `tests/test_mods.py::ModTests::test_nested_drive_and_alternate_stream_paths_are_refused` | `src/core/mods.rs::tests::nested_drive_and_alternate_stream_paths_are_refused` | ported |
| `tests/test_mods.py::ModTests::test_windows_normalized_traversal_paths_are_refused` | `src/core/mods.rs::tests::windows_normalized_traversal_paths_are_refused` | ported |
| `tests/test_mods.py::ModTests::test_unsafe_zip_is_refused_before_any_member_is_extracted` | `src/core/mods.rs::tests::unsafe_zip_is_refused_before_any_member_is_extracted` | ported |
| `tests/test_mods.py::ModTests::test_unresolvable_zip_target_does_not_stop_other_mods` | — | dropped: no Rust counterpart — a zip whose extraction folder cannot be resolved (an unreadable target, a symlink loop) is skipped and the other zips still unpack |
| `tests/test_mods.py::ModTests::test_no_mod_returns_original_game` | `src/core/mods.rs::tests::no_mod_returns_original_game` | ported |
| `tests/test_mods.py::ModTests::test_directory_conflict_does_not_modify_base` | `src/core/mods.rs::tests::directory_conflict_does_not_modify_base` | ported |
| `tests/test_mods.py::ModTests::test_mod_symlinks_rejected` | `src/core/mods.rs::tests::mod_symlinks_rejected` | ported |
| `tests/test_mods.py::ModTests::test_executable_replacement_rejected` | `src/core/mods.rs::tests::executable_replacement_rejected` | ported |
| `tests/test_mods.py::ModTests::test_shadps4_sibling_folder_and_global_disable` | `src/core/mods.rs::tests::shadps4_sibling_folder_and_global_disable` | ported |
| `tests/test_mods.py::ModTests::test_app0_wrapped_mod_discovered` | `src/core/mods.rs::tests::app0_wrapped_mod_discovered` | ported |
| `tests/test_mods.py::ModTests::test_other_case_replaces_the_game_file` | `src/core/mods.rs::tests::other_case_replaces_the_game_file` | ported |
| `tests/test_mods.py::ModTests::test_wrapped_layouts` | `src/core/mods.rs::tests::wrapped_layouts` | ported |
| `tests/test_mods.py::ModTests::test_invalid_profile_fails` | `src/core/mods.rs::tests::invalid_profile_fails` | ported |
| `tests/test_mods.py::ModTests::test_run_uses_overlay_propagates_exit_and_cleans_view` | `tests/runner.rs::the_probes_exit_code_is_returned` | ported |
| `tests/test_mods.py::GameLayout::test_exec_and_language_folders_replace_game_files` | `src/core/mods.rs::tests::exec_and_language_folders_replace_game_files` | ported |
| `tests/test_mods.py::GameLayout::test_dvdroot_prefix_is_dropped_when_the_game_has_none` | `src/core/mods.rs::tests::dvdroot_prefix_is_dropped_when_the_game_has_none` | ported |
| `tests/test_mods.py::GameLayout::test_wrapper_folder_around_exec` | `src/core/mods.rs::tests::wrapper_folder_around_exec` | ported |
| `tests/test_mods.py::GameLayout::test_system_files_are_refused_with_or_without_the_prefix` | `src/core/mods.rs::tests::system_files_are_refused_with_or_without_the_prefix` | ported |
| `tests/test_mods.py::GameLayout::test_conflicts_match_across_layouts` | `src/core/mods.rs::tests::conflicts_match_across_layouts` | ported |
| `tests/test_mods.py::GameLayout::test_region_folders_are_used_for_discovery_and_overlay` | `src/core/mods.rs::tests::region_folders_are_used_for_discovery_and_overlay` | ported |
| `tests/test_mods.py::GameLayout::test_region_folders_do_not_leak_to_another_game` | `src/core/mods.rs::tests::region_folders_do_not_leak_to_another_game` | ported |
| `tests/test_mods.py::GameLayout::test_region_mods_keep_metadata_conflicts_and_selection` | `src/core/mods.rs::tests::region_folders_are_used_for_discovery_and_overlay`, `src/core/mods.rs::tests::region_folders_do_not_leak_to_another_game`, `src/core/mods.rs::tests::conflicts_match_across_layouts` | replaced by `src/core/mods.rs::tests::region_folders_are_used_for_discovery_and_overlay`, `src/core/mods.rs::tests::region_folders_do_not_leak_to_another_game`, `src/core/mods.rs::tests::conflicts_match_across_layouts` |
| `tests/test_mods.py::GameLayout::test_learning_game_folders_does_not_allow_system_replacements` | `src/core/mods.rs::tests::learning_game_folders_does_not_allow_system_replacements` | ported |
| `tests/test_mods.py::ModInfo::test_mod_json_fields` | `src/core/mods.rs::tests::mod_json_fields` | ported |
| `tests/test_mods.py::ModInfo::test_without_mod_json_the_folder_name_and_readme_are_used` | `src/core/mods.rs::tests::without_mod_json_the_folder_name_and_readme_are_used` | ported |
| `tests/test_mods.py::ModInfo::test_invalid_mod_json_falls_back` | `src/core/mods.rs::tests::invalid_mod_json_falls_back` | ported |
| `tests/test_mods.py::ModInfo::test_only_web_links_are_kept` | `src/core/mods.rs::tests::only_web_links_are_kept` | ported |
| `tests/test_mods.py::ModInfo::test_preview_image_wins_over_other_images` | `src/core/mods.rs::tests::preview_image_wins_over_other_images` | ported |
| `tests/test_mods.py::ModInfo::test_metadata_inside_a_wrapper_folder` | `src/core/mods.rs::tests::metadata_inside_a_wrapper_folder` | ported |
| `tests/test_mods.py::ModInfo::test_mod_json_beside_the_wrapper_folder_keeps_the_mod` | `src/core/mods.rs::tests::mod_json_beside_the_wrapper_folder_keeps_the_mod` | ported |
| `tests/test_mod_conflicts.py::ModConflictTest::test_last_in_order_wins` | `src/core/mods.rs::tests::last_in_order_wins` | ported |
| `tests/test_mod_conflicts.py::ModConflictTest::test_only_given_mods_count` | `src/core/mods.rs::tests::only_given_mods_count` | ported |
| `tests/test_mod_conflicts.py::ModConflictTest::test_no_conflict_and_broken_mod` | `src/core/mods.rs::tests::no_conflict_and_broken_mod` | ported |
| `tests/test_mod_conflicts.py::ModConflictTest::test_preset_skips_missing_and_appends_new` | `src/core/mods.rs::tests::preset_skips_missing_and_appends_new` | ported |
| `tests/test_mod_conflicts.py::ModConflictTest::test_preset_with_bad_shape` | `src/core/mods.rs::tests::preset_with_bad_shape` | ported |
| `tests/test_glyphs.py::GlyphTableTest::test_entries_walk_the_whole_file_and_stop_at_a_truncated_entry` | `src/runner/glyphs.rs::entries_walk_the_whole_file_and_stop_at_a_truncated_entry` | ported |
| `tests/test_glyphs.py::GlyphTableTest::test_entity_count_marker_carries_no_data` | `src/runner/glyphs.rs::entity_count_marker_carries_no_data` | ported |
| `tests/test_glyphs.py::GlyphTableTest::test_glyph_of_each_style` | `src/runner/glyphs.rs::glyph_of_each_style` | ported |
| `tests/test_glyphs.py::GlyphTableTest::test_the_letters_of_the_controller_win_over_the_style` | `src/runner/glyphs.rs::the_letters_of_the_controller_win_over_the_style` | ported |
| `tests/test_glyphs.py::GlyphTableTest::test_pad_map_moves_the_glyph` | `src/runner/glyphs.rs::pad_map_moves_the_glyph` | ported |
| `tests/test_glyphs.py::GlyphTableTest::test_key_map_moves_the_key` | `src/runner/glyphs.rs::key_map_moves_the_key` | ported |
| `tests/test_glyphs.py::GlyphTableTest::test_rage_on_one_input_draws_its_key_on_l3_and_r3` | `src/runner/glyphs.rs::rage_on_one_input_draws_its_key_on_l3_and_r3` | ported |
| `tests/test_glyphs.py::GlyphTableTest::test_key_caps` | `src/runner/glyphs.rs::key_caps` | ported |
| `tests/test_glyphs.py::GlyphTableTest::test_gamepad_line` | `src/runner/glyphs.rs::gamepad_line` | ported |
| `tests/test_glyphs.py::GlyphTableTest::test_resolve` | `src/runner/glyphs.rs::resolve` | ported |
| `tests/test_glyphs.py::GlyphPatchTest::test_patch_keeps_every_size_and_only_touches_glyph_pixels` | `src/runner/glyphs.rs::patch_keeps_every_size_and_only_touches_glyph_pixels` | ported |
| `tests/test_glyphs.py::GlyphPatchTest::test_lyre_discs_are_repainted_in_place` | `src/runner/glyphs.rs::lyre_discs_are_repainted_in_place` | ported |
| `tests/test_glyphs.py::GlyphPatchTest::test_encode_region_touches_only_changed_blocks` | `src/runner/glyphs.rs::encode_region_touches_only_changed_blocks` | ported |
| `tests/test_glyphs.py::GlyphPatchTest::test_remap_and_style_change_the_pixels` | `src/runner/glyphs.rs::remap_and_style_change_the_pixels` | ported |
| `tests/test_glyphs.py::GlyphPatchTest::test_controller_diagram_follows_the_gamepad_style` | `src/runner/glyphs.rs::controller_diagram_follows_the_gamepad_style` | ported |
| `tests/test_glyphs.py::GlyphPatchTest::test_keyboard_keeps_the_original_controller_diagram` | `src/runner/glyphs.rs::keyboard_keeps_the_original_controller_diagram` | ported |
| `tests/test_glyphs.py::GlyphPatchTest::test_diagram_is_mounted_and_the_original_is_untouched` | `src/runner/glyphs.rs::diagram_is_mounted_and_the_original_is_untouched` | ported |
| `tests/test_glyphs.py::GlyphPatchTest::test_missing_diagram_art_keeps_the_prompts_and_is_retried_when_restored` | `src/runner/glyphs.rs::missing_diagram_art_keeps_the_prompts_and_is_retried_when_restored` | ported |
| `tests/test_glyphs.py::GlyphPatchTest::test_the_keyboard_follows_only_the_key_map` | `src/runner/glyphs.rs::the_keyboard_follows_only_the_key_map` | ported |
| `tests/test_glyphs.py::GlyphPatchTest::test_a_texture_of_another_size_is_left_alone` | `src/runner/glyphs.rs::a_texture_of_another_size_is_left_alone` | ported |
| `tests/test_glyphs.py::GlyphPatchTest::test_encode_rejects_a_length_it_cannot_fill` | `src/runner/glyphs.rs::encode_rejects_a_length_it_cannot_fill` | ported |
| `tests/test_glyphs.py::GlyphPatchTest::test_build_writes_copies_and_leaves_the_game_alone` | `src/runner/glyphs.rs::build_writes_copies_and_leaves_the_game_alone` | ported |
| `tests/test_glyphs.py::GlyphPatchTest::test_build_reuses_the_copies_until_something_changes` | `src/runner/glyphs.rs::build_reuses_the_copies_until_something_changes` | ported |
| `tests/test_glyphs.py::GlyphPatchTest::test_playstation_and_off_leave_no_copies` | `src/runner/glyphs.rs::playstation_and_off_leave_no_copies` | ported |
| `tests/test_glyphs.py::GlyphPatchTest::test_a_wad_without_glyphs_is_not_mounted` | `src/runner/glyphs.rs::a_wad_without_glyphs_is_not_mounted` | ported |
| `tests/test_glyphs.py::RunGlyphsTest::test_off_runs_nothing_and_removes_old_copies` | `src/runner/mod.rs::tests::off_runs_nothing_and_removes_old_copies` | ported |
| `tests/test_glyphs.py::RunGlyphsTest::test_auto_asks_the_probe_and_mounts_the_copies` | `src/runner/mod.rs::tests::auto_asks_the_probe_and_mounts_the_copies` | ported |
| `tests/test_glyphs.py::RunGlyphsTest::test_a_chosen_style_does_not_ask_the_probe` | `src/runner/mod.rs::tests::a_chosen_style_does_not_ask_the_probe` | ported |
| `tests/test_glyphs.py::RunGlyphsTest::test_keyboard_passes_the_key_map` | `src/runner/mod.rs::tests::keyboard_passes_the_key_map` | ported |
| `tests/test_glyphs.py::RunGlyphsTest::test_auto_without_a_gamepad_shows_the_keys` | `src/runner/mod.rs::tests::auto_without_a_gamepad_shows_the_keys` | ported |
| `tests/test_glyphs.py::RunGlyphsTest::test_a_failure_starts_the_game_with_its_own_glyphs` | `src/runner/mod.rs::tests::a_failure_starts_the_game_with_its_own_glyphs` | ported |
| `tests/test_glyphs.py::KeyboardStickTest::test_stick_and_dpad_textures_are_keyboard_only` | `src/runner/glyphs.rs::stick_and_dpad_textures_are_keyboard_only` | ported |
| `tests/test_glyphs.py::KeyboardStickTest::test_sticks_show_the_movement_and_camera_keys` | `src/runner/glyphs.rs::sticks_show_the_movement_and_camera_keys` | ported |
| `tests/test_glyphs.py::KeyboardStickTest::test_dpad_follows_the_weapon_key` | `src/runner/glyphs.rs::dpad_follows_the_weapon_key` | ported |
| `tests/test_glyphs.py::KeyboardStickTest::test_arrows_render` | `src/runner/glyphs.rs::arrows_render` | ported |
| `tests/test_glyphs.py::AtlasAlignmentTest::test_atlas_prompts_are_centered_towards_midline` | `src/runner/glyphs.rs::atlas_prompts_are_centered_towards_midline` | ported |
| `tests/test_glyphs.py::AtlasAlignmentTest::test_qte_face_buttons_remain_centered` | `src/runner/glyphs.rs::qte_face_buttons_remain_centered` | ported |
| `tests/test_content.py::ProfileTests::test_sfo_parameters_and_explicit_trial_profile` | `src/runner/content_profile.rs::tests::sfo_parameters_and_explicit_trial_profile` | ported |
| `tests/test_content.py::ProfileTests::test_invalid_sfo_parameter_rejected` | `src/runner/content_profile.rs::tests::invalid_sfo_parameter_rejected` | ported |
| `tests/test_content.py::ContentTests::test_lifecycle_and_output_boundaries` | — | dropped: C runtime: the case tests content.c, which the Rust runner does not contain (it writes content.bin; the C side keeps tests/test_content.c) |
| `tests/test_content.py::ContentTests::test_missing_profile_does_not_report_loaded` | — | dropped: C runtime: the case tests content.c, which the Rust runner does not contain (it writes content.bin; the C side keeps tests/test_content.c) |
| `tests/test_content.py::ContentTests::test_other_modules_are_counted_independently` | — | dropped: C runtime: the case tests content.c, which the Rust runner does not contain (it writes content.bin; the C side keeps tests/test_content.c) |
| `tests/test_guest_memory.py::GuestMemoryTest::test_pool_matches_runtime_memory` | `src/runner/guest_memory.rs::tests::pool_matches_runtime_memory` | ported |
| `tests/test_guest_memory.py::GuestMemoryTest::test_reported_pc` | `src/runner/guest_memory.rs::tests::reported_pc` | ported |
| `tests/test_guest_memory.py::GuestMemoryTest::test_device_advice_is_not_repeated` | `src/runner/guest_memory.rs::tests::device_advice_is_not_repeated` | ported |
| `tests/test_guest_memory.py::GuestMemoryTest::test_enough_ram_is_silent` | `src/runner/guest_memory.rs::tests::enough_ram_is_silent` | ported |
| `tests/test_guest_memory.py::GuestMemoryTest::test_default_patches_are_silent_on_8_gb` | `src/runner/guest_memory.rs::tests::default_patches_are_silent_on_8_gb` | ported |
| `tests/test_guest_memory.py::GuestMemoryTest::test_1440p_warns_on_8_gb` | `src/runner/guest_memory.rs::tests::warns_for_1440p_on_8_gb` | ported |
| `tests/test_guest_memory.py::GuestMemoryTest::test_unknown_ram_is_silent` | `src/runner/guest_memory.rs::tests::unknown_ram_is_silent` | ported |
| `tests/test_guest_memory.py::GuestMemoryTest::test_unknown_stream_memory_counts_as_cached` | `src/runner/guest_memory.rs::tests::unknown_stream_memory_counts_as_cached` | ported |
| `tests/test_graphics_options.py::GraphicsOptionsTest::test_defaults_and_invalid_values` | `src/runner/graphics_options.rs::tests::defaults_and_invalid_values` | ported |
| `tests/test_graphics_options.py::GraphicsOptionsTest::test_corrupt_state_file_preserves_current_settings` | `src/runner/graphics_options.rs::tests::corrupt_state_file_preserves_current_settings` | ported |
| `tests/test_graphics_options.py::GraphicsOptionsTest::test_8k_selection_replaces_previous_resolution` | `src/runner/graphics_options.rs::tests::the_8k_selection_replaces_previous_resolution` | ported |
| `tests/test_graphics_options.py::GraphicsOptionsTest::test_write_keys_keeps_other_lines` | `src/runner/graphics_options.rs::tests::write_keys_keeps_other_lines` | ported |
| `tests/test_graphics_options.py::GraphicsOptionsTest::test_unconfirmed_restart_is_tried_once_then_reverted` | `src/runner/graphics_options.rs::tests::unconfirmed_restart_is_tried_once_then_reverted` | ported |
| `tests/test_graphics_options.py::GraphicsOptionsTest::test_confirmed_restart_becomes_the_last_good` | `src/runner/graphics_options.rs::tests::confirmed_restart_becomes_the_last_good` | ported |
| `tests/test_graphics_options.py::GraphicsOptionsTest::test_patch_names` | `src/runner/graphics_options.rs::tests::patch_names_follow_the_options` | ported |
| `tests/test_graphics_options.py::GraphicsOptionsTest::test_resolution_labels_show_the_size_and_aspect_ratio` | `src/core/options.rs::tests::labels_match_python`, `src/core/options.rs::tests::beta_resolutions_are_the_untested_custom_ones` | replaced by `src/core/options.rs::tests::labels_match_python`, `src/core/options.rs::tests::beta_resolutions_are_the_untested_custom_ones` |
| `tests/test_graphics_options.py::GraphicsOptionsTest::test_every_built_in_resolution_has_a_patch` | `src/runner/graphics_options.rs::tests::every_built_in_resolution_has_a_patch` | ported |
| `tests/test_graphics_options.py::GraphicsOptionsTest::test_900p_is_the_built_in_size_between_720p_and_1440p` | `src/runner/graphics_options.rs::tests::the_900p_is_the_built_in_size_between_720p_and_1440p` | ported |
| `tests/test_graphics_options.py::GraphicsOptionsTest::test_resolutions_of_enabled_mods` | `src/runner/graphics_options.rs::tests::resolutions_of_enabled_mods` | ported |
| `tests/test_graphics_options.py::GraphicsOptionsTest::test_a_confirmed_mod_resolution_is_restored_while_its_mod_is_on` | `src/runner/graphics_options.rs::tests::a_confirmed_mod_resolution_is_restored_while_its_mod_is_on` | ported |
| `tests/test_graphics_options.py::GraphicsOptionsTest::test_engine_rate_100_for_100hz_displays` | `src/runner/graphics_options.rs::tests::engine_rate_100_for_100hz_displays` | ported |
| `tests/test_graphics_options.py::GraphicsOptionsTest::test_engine_rate_180_for_high_refresh_displays` | `src/runner/graphics_options.rs::tests::engine_rate_180_for_high_refresh_displays` | ported |
| `tests/test_graphics_options.py::GraphicsOptionsTest::test_engine_rate_144_for_144hz_displays` | `src/runner/graphics_options.rs::tests::engine_rate_144_for_144hz_displays` | ported |
| `tests/test_graphics_options.py::GraphicsOptionsTest::test_applied_engine_fps` | `src/runner/graphics_options.rs::tests::applied_engine_fps_reads_the_highest_frame_rate_patch` | ported |
| `tests/test_graphics_options.py::GraphicsOptionsTest::test_environment` | `src/runner/graphics_options.rs::tests::environment_carries_the_readback_switches` | ported |
| `tests/test_graphics_options.py::WindowSizeTest::test_the_game_knows_the_launcher_sizes` | `src/runner/graphics_options.rs::tests::the_game_knows_the_launcher_sizes` | ported |
| `tests/test_graphics_options.py::AspectRatioTest::test_aspect_of_a_resolution` | `src/core/options.rs::tests::aspect_of_2560x1080_is_21_9` | replaced by `src/core/options.rs::tests::aspect_of_2560x1080_is_21_9` |
| `tests/test_graphics_options.py::AspectRatioTest::test_aspects_in_a_fixed_order_with_the_others_last` | `src/core/options.rs::tests::closest_resolution_moves_to_the_nearest_pixel_count`, `tests/ui_pages.rs::aspect_follows_and_filters_the_render_resolution` | replaced by `src/core/options.rs::tests::closest_resolution_moves_to_the_nearest_pixel_count`, `tests/ui_pages.rs::aspect_follows_and_filters_the_render_resolution` |
| `tests/test_graphics_options.py::AspectRatioTest::test_resolutions_of_an_aspect_smallest_first_native_on_top` | `src/core/options.rs::tests::closest_resolution_moves_to_the_nearest_pixel_count`, `tests/ui_pages.rs::aspect_follows_and_filters_the_render_resolution` | replaced by `src/core/options.rs::tests::closest_resolution_moves_to_the_nearest_pixel_count`, `tests/ui_pages.rs::aspect_follows_and_filters_the_render_resolution` |
| `tests/test_graphics_options.py::AspectRatioTest::test_switching_aspect_keeps_the_closest_size` | `src/core/options.rs::tests::closest_resolution_moves_to_the_nearest_pixel_count`, `tests/ui_pages.rs::aspect_follows_and_filters_the_render_resolution` | replaced by `src/core/options.rs::tests::closest_resolution_moves_to_the_nearest_pixel_count`, `tests/ui_pages.rs::aspect_follows_and_filters_the_render_resolution` |
| `tests/test_graphics_options.py::AspectRatioTest::test_labels_without_the_aspect_ratio_for_the_filtered_list` | `src/core/options.rs::tests::labels_match_python`, `tests/ui_pages.rs::aspect_follows_and_filters_the_render_resolution` | replaced by `src/core/options.rs::tests::labels_match_python`, `tests/ui_pages.rs::aspect_follows_and_filters_the_render_resolution` |
| `tests/test_graphics_options.py::SteamDeckTest::test_lcd_and_oled_models_default_to_1280x800` | `src/runner/graphics_options.rs::tests::lcd_and_oled_models_default_to_1280x800` | ported |
| `tests/test_graphics_options.py::SteamDeckTest::test_other_machines_stay_native` | `src/runner/graphics_options.rs::tests::other_machines_stay_native` | ported |
| `tests/test_graphics_options.py::SteamDeckTest::test_no_dmi_names_stays_native` | `src/runner/graphics_options.rs::tests::no_dmi_names_stays_native` | ported |
| `tests/test_graphics_options.py::SteamDeckTest::test_the_default_resolution_is_a_built_in_one` | `src/runner/graphics_options.rs::tests::the_default_resolution_is_a_built_in_one` | ported |
| `tests/test_save_backup.py::SaveBackupTest::test_copies_savedata` | `src/core/save_backup.rs::tests::copies_savedata` | ported |
| `tests/test_save_backup.py::SaveBackupTest::test_no_backup_without_saves` | `src/core/save_backup.rs::tests::no_backup_without_saves` | ported |
| `tests/test_save_backup.py::SaveBackupTest::test_same_second_gets_unique_name` | `src/core/save_backup.rs::tests::same_second_gets_unique_name` | ported |
| `tests/test_save_backup.py::SaveBackupTest::test_same_second_backups_sort_by_creation_sequence` | `src/core/save_backup.rs::tests::same_second_backups_sort_by_creation_sequence` | ported |
| `tests/test_save_backup.py::SaveBackupTest::test_same_second_retention_keeps_latest_contents_and_unique_names` | `src/core/save_backup.rs::tests::same_second_retention_keeps_latest_contents_and_unique_names` | ported |
| `tests/test_save_backup.py::SaveBackupTest::test_keeps_newest_ten` | `src/core/save_backup.rs::tests::keeps_newest_ten` | ported |
| `tests/test_save_backup.py::SaveBackupTest::test_same_second_restore_keeps_the_safety_backup` | `src/core/save_backup.rs::tests::same_second_restore_keeps_the_safety_backup` | ported |
| `tests/test_save_backup.py::SaveBackupTest::test_restore_backs_up_current_first` | `src/core/save_backup.rs::tests::restore_backs_up_current_first` | ported |
| `tests/test_save_backup.py::SaveBackupTest::test_restore_oldest_when_full` | `src/core/save_backup.rs::tests::restore_oldest_when_full` | ported |
| `tests/test_save_backup.py::SaveBackupTest::test_restore_rejects_unknown_or_unsafe_names` | `src/core/save_backup.rs::tests::restore_rejects_unknown_or_unsafe_names` | ported |
| `tests/test_pause_menu.py::Refusals::test_wad_without_the_script` | `src/runner/pause_menu.rs::tests::wad_without_the_script` | ported |
| `tests/test_pause_menu.py::Refusals::test_build_leaves_no_copy` | `src/runner/pause_menu.rs::tests::build_leaves_no_copy` | ported |
| `tests/test_pause_menu.py::Labels::test_resolve_label_by_id` | `src/runner/pause_menu.rs::tests::resolve_label_by_id` | ported |
| `tests/test_pause_menu.py::Labels::test_resolve_label_by_code` | `src/runner/pause_menu.rs::tests::resolve_label_by_code` | ported |
| `tests/test_pause_menu.py::Labels::test_resolve_label_fallback` | `src/runner/pause_menu.rs::tests::resolve_label_fallback` | ported |
| `tests/test_pause_menu.py::Labels::test_resolve_label_environment` | `src/runner/pause_menu.rs::tests::resolve_label_environment` | ported |
| `tests/test_pause_menu.py::Labels::test_build_cache_stamp_tracks_language` | `src/runner/pause_menu.rs::tests::build_cache_stamp_tracks_language` | ported |
| `tests/test_pause_menu.py::GameScript::test_enemy_health_bar_follows_screen_y_with_native_animation` | `tests/dump.rs::real_menu_enemy_health_bar_follows_screen_y_with_native_animation` | ported |
| `tests/test_pause_menu.py::GameScript::test_display_initializes_health_value_before_the_row_is_selected` | `tests/dump.rs::real_menu_display_initializes_health_value_before_the_row_is_selected` | ported |
| `tests/test_pause_menu.py::GameScript::test_pause_row` | `tests/dump.rs::real_menu_pause_row` | ported |
| `tests/test_pause_menu.py::GameScript::test_exit_asks_quit_games_question_with_the_mark` | `tests/dump.rs::real_menu_exit_asks_quit_games_question_with_the_mark` | ported |
| `tests/test_pause_menu.py::GameScript::test_failsafe_states_end_the_process_on_the_mark` | `tests/dump.rs::real_menu_failsafe_states_end_the_process_on_the_mark` | ported |
| `tests/test_pause_menu.py::GameScript::test_options_row` | `tests/dump.rs::real_menu_options_row` | ported |
| `tests/test_pause_menu.py::GameScript::test_every_other_goto_is_unchanged` | `tests/dump.rs::real_menu_every_other_goto_is_unchanged` | ported |
| `tests/test_pause_menu.py::GameScript::test_japanese_fallback` | `tests/dump.rs::real_menu_japanese_fallback` | ported |
| `tests/test_pause_menu.py::GameScript::test_chinese_fallback` | `tests/dump.rs::real_menu_chinese_fallback` | ported |
| `tests/test_pause_menu.py::GameScript::test_russian_fallback` | `tests/dump.rs::real_menu_russian_fallback` | ported |
| `tests/test_pause_menu.py::GameScript::test_polish_translated` | `tests/dump.rs::real_menu_polish_translated` | ported |
| `tests/test_pause_menu.py::GameScript::test_all_supported_languages` | `tests/dump.rs::real_menu_all_supported_languages` | ported |
| `tests/test_native_cheats.py::NativeCheats::test_cheats_is_between_game_settings_and_audio` | `tests/dump.rs::real_menu_cheats_is_between_game_settings_and_audio` | ported |
| `tests/test_native_cheats.py::NativeCheats::test_every_cheat_has_native_selection_and_initial_value` | `tests/dump.rs::real_menu_every_cheat_has_native_selection_and_initial_value` | ported |
| `tests/test_native_cheats.py::NativeCheats::test_qte_precedes_multipliers_and_last_row_scrolls` | `tests/dump.rs::real_menu_qte_precedes_multipliers_and_last_row_scrolls` | ported |
| `tests/test_native_cheats.py::NativeCheats::test_multipliers_stays_visible_and_qte_uses_a_visible_value_slot` | `tests/dump.rs::real_menu_multipliers_stays_visible_and_qte_uses_a_visible_value_slot` | ported |
| `tests/test_native_cheats.py::NativeCheats::test_toggling_scrolled_cheats_updates_their_visible_row` | `tests/dump.rs::real_menu_toggling_scrolled_cheats_updates_their_visible_row` | ported |
| `tests/test_native_cheats.py::NativeCheats::test_six_multipliers_have_native_bars_and_directional_controls` | `tests/dump.rs::real_menu_six_multipliers_have_native_bars_and_directional_controls` | ported |
| `tests/test_native_cheats.py::NativeCheats::test_custom_pages_preserve_the_native_footer` | `tests/dump.rs::real_menu_custom_pages_preserve_the_native_footer` | ported |
| `tests/test_native_cheats.py::NativeCheats::test_multiplier_bars_are_separate_from_numbers_and_original_sliders` | `tests/dump.rs::real_menu_multiplier_bars_are_separate_from_numbers_and_original_sliders` | ported |
| `tests/test_native_cheats.py::NativeCheats::test_every_state_reference_and_relative_call_stays_in_the_script` | `tests/dump.rs::real_menu_every_state_reference_and_relative_call_stays_in_the_script` | ported |
| `tests/test_legacy.py::LegacyTest::test_game_settings_file_is_copied_and_kept` | `src/core/legacy.rs::tests::game_settings_file_is_copied_and_kept` | ported |
| `tests/test_legacy.py::LegacyTest::test_game_settings_file_is_not_overwritten` | `src/core/legacy.rs::tests::game_settings_file_is_not_overwritten` | ported |
| `tests/test_legacy.py::LegacyTest::test_linux_data_folder_with_saves_is_copied` | `src/core/legacy.rs::tests::linux_data_folder_with_saves_is_copied` | ported |
| `tests/test_legacy.py::LegacyTest::test_linux_data_copy_leaves_caches_and_run_folders` | `src/core/legacy.rs::tests::linux_data_copy_leaves_caches_and_run_folders` | ported |
| `tests/test_legacy.py::LegacyTest::test_linux_data_copy_keeps_newer_files` | `src/core/legacy.rs::tests::linux_data_copy_keeps_newer_files` | ported |
| `tests/test_legacy.py::LegacyTest::test_runs_once` | `src/core/legacy.rs::tests::runs_once` | ported |
| `tests/test_legacy.py::LegacyTest::test_nothing_to_move_is_quiet` | `src/core/legacy.rs::tests::nothing_to_move_is_quiet` | ported |
| `tests/test_legacy.py::LegacyTest::test_old_menu_entry_of_ours_is_removed_and_a_hand_made_one_kept` | `src/core/legacy.rs::tests::old_menu_entry_of_ours_is_removed_and_a_hand_made_one_kept` | ported |
| `tests/test_legacy.py::LegacyTest::test_environment_is_forwarded_without_overriding` | `src/core/legacy.rs::tests::environment_is_forwarded_without_overriding` | ported |
| `tests/test_legacy.py::LegacyTest::test_custom_data_folder_wins_over_the_appimage_default` | `src/core/legacy.rs::tests::custom_data_folder_wins_over_the_appimage_default` | ported |
| `tests/test_legacy.py::LegacyTest::test_leftover_binaries_go_only_when_the_new_ones_exist` | `src/core/legacy.rs::tests::leftover_binaries_go_only_when_the_new_ones_exist` | ported |
| `tests/test_legacy.py::LegacyTest::test_old_executable_copies_stay` | `src/core/legacy.rs::tests::old_executable_copies_stay` | ported |
| `tests/test_legacy.py::UpgradeFromFormerNameTest::test_the_old_settings_folder_name_is_the_former_one` | `src/core/paths.rs::tests::legacy_config_file_is_appdata_gow3_launcher_on_windows`, `src/core/paths.rs::tests::legacy_config_file_is_xdg_config_gow3_launcher_on_unix` | replaced by `src/core/paths.rs::tests::legacy_config_file_is_appdata_gow3_launcher_on_windows`, `src/core/paths.rs::tests::legacy_config_file_is_xdg_config_gow3_launcher_on_unix` |
| `tests/test_legacy.py::UpgradeFromFormerNameTest::test_windows_settings_move_and_saves_stay` | `src/core/settings.rs::tests::portable_migration_copies_only_settings`, `src/core/legacy.rs::tests::game_settings_file_is_copied_and_kept` | replaced by `src/core/settings.rs::tests::portable_migration_copies_only_settings`, `src/core/legacy.rs::tests::game_settings_file_is_copied_and_kept` |
| `tests/test_legacy.py::UpgradeFromFormerNameTest::test_linux_appimage_settings_and_saves_are_copied` | `src/core/legacy.rs::tests::linux_data_folder_with_saves_is_copied`, `src/core/settings.rs::tests::portable_migration_copies_only_settings`, `src/core/legacy.rs::tests::game_settings_file_is_copied_and_kept` | replaced by `src/core/legacy.rs::tests::linux_data_folder_with_saves_is_copied`, `src/core/settings.rs::tests::portable_migration_copies_only_settings`, `src/core/legacy.rs::tests::game_settings_file_is_copied_and_kept` |
| `tests/test_legacy.py::UpgradeFromFormerNameTest::test_settings_already_in_the_old_data_folder_win` | — | dropped: no Rust counterpart — a settings.json already in the old data folder (a build between the release and the rename) is read instead of the older one in the legacy config folder |
| `tests/test_legacy.py::UpgradeFromFormerNameTest::test_start_moves_before_reading_settings` | — | dropped: no Rust counterpart — the start moves the legacy files before it reads the settings (no test pins the order in startup.rs) |
| `tests/test_legacy.py::LauncherStartTest::test_the_update_installer_does_not_migrate` | `tests/startup.rs::the_update_installer_does_not_migrate`, `tests/startup.rs::arguments_route_to_their_roles` | replaced by `tests/startup.rs::the_update_installer_does_not_migrate`, `tests/startup.rs::arguments_route_to_their_roles` |
| `tests/test_legacy.py::CrossPlatformShortcutMarkerTest::test_shortcut_migration_skips_when_marker_exists` | `src/core/legacy.rs::tests::shortcut_migration_skips_when_marker_exists` | ported |
| `tests/test_legacy.py::CrossPlatformShortcutMarkerTest::test_shortcut_migration_returns_false_on_non_windows` | `src/core/legacy.rs::tests::shortcut_migration_returns_false_on_non_windows` | ported |
| `tests/test_legacy.py::WindowsDesktopShortcutTest::test_user_created_shortcut_with_custom_name` | `src/core/legacy.rs::tests::windows_shortcuts::user_created_shortcut_with_custom_name` | ported |
| `tests/test_legacy.py::WindowsDesktopShortcutTest::test_shortcut_manually_renamed_to_legacy_title` | — | dropped: no Rust counterpart — a shortcut with the old title that points to SpartanPort.exe is kept and no SpartanPort.lnk is made (needs PowerShell) |
| `tests/test_legacy.py::WindowsDesktopShortcutTest::test_user_shortcut_targeting_play_legacy_executable` | — | dropped: no Rust counterpart — a shortcut with the old title that points to the Play executable is kept, issue 824 (needs PowerShell) |
| `tests/test_legacy.py::WindowsDesktopShortcutTest::test_shortcut_with_custom_arguments_is_preserved` | `src/core/legacy.rs::tests::windows_shortcuts::shortcut_with_custom_arguments_is_preserved` | ported |
| `tests/test_legacy.py::WindowsDesktopShortcutTest::test_existing_spartanport_shortcut_is_preserved` | — | dropped: no Rust counterpart — an existing SpartanPort.lnk is kept and the old shortcut stays (needs PowerShell) |
| `tests/test_legacy.py::WindowsDesktopShortcutTest::test_repeated_launcher_starts_do_not_retrigger_migration` | `src/core/legacy.rs::tests::windows_shortcuts::first_launch_after_upgrading_from_legacy_name` | replaced by `src/core/legacy.rs::tests::windows_shortcuts::first_launch_after_upgrading_from_legacy_name` |
| `tests/test_legacy.py::WindowsDesktopShortcutTest::test_first_launch_after_upgrading_from_legacy_name` | `src/core/legacy.rs::tests::windows_shortcuts::first_launch_after_upgrading_from_legacy_name` | ported |
| `tests/test_legacy.py::WindowsDesktopShortcutTest::test_ambiguous_ownership_preserves_existing_shortcut` | `src/core/legacy.rs::tests::windows_shortcuts::first_launch_after_upgrading_from_legacy_name` | replaced by `src/core/legacy.rs::tests::windows_shortcuts::first_launch_after_upgrading_from_legacy_name` |
| `tests/test_legacy.py::WindowsDesktopShortcutTest::test_failed_shortcut_creation_preserves_old_shortcut_and_does_not_write_marker` | — | dropped: no Rust counterpart — a failed PowerShell save keeps the old shortcut and writes no marker (the Python mocks subprocess.run; the Rust has no seam) |
| `tests/test_legacy.py::WindowsDesktopShortcutTest::test_powershell_error_does_not_write_marker_and_allows_recovery` | — | dropped: no Rust counterpart — a PowerShell crash writes no marker and the next start recovers (the Python mocks subprocess.run; the Rust has no seam) |
| `tests/test_legacy.py::WindowsDesktopShortcutTest::test_shortcut_targeting_unrelated_executable` | — | dropped: no Rust counterpart — a shortcut with the old title that points to an unrelated executable is kept (needs PowerShell) |
| `tests/test_legacy.py::WindowsDesktopShortcutTest::test_missing_shortcuts_and_missing_targets` | `src/core/legacy.rs::tests::windows_shortcuts::missing_shortcuts_and_missing_targets` | ported |
| `tests/test_legacy.py::WindowsDesktopShortcutTest::test_windows_paths_containing_spaces` | — | dropped: no Rust counterpart — the shortcut migration works for a port folder and a desktop folder whose paths contain spaces (needs PowerShell) |
| `tests/test_legacy.py::WindowsDesktopShortcutTest::test_launcher_move_from_legacy_name_integration` | — | dropped: no Rust counterpart — the launcher start runs the shortcut migration, keeps a custom Play shortcut and writes the marker (needs PowerShell) |
| `tests/test_discord_presence.py::Framing::test_pack_has_opcode_length_and_utf8_body` | `src/runner/discord.rs::tests::pack_has_opcode_length_and_utf8_body` | ported |
| `tests/test_discord_presence.py::Framing::test_read_frame_reassembles_split_reads` | `src/runner/discord.rs::tests::read_frame_reassembles_split_reads` | ported |
| `tests/test_discord_presence.py::Framing::test_ping_echoes_raw_bytes_before_the_next_frame` | `src/runner/discord.rs::tests::ping_echoes_raw_bytes_before_the_next_frame` | ported |
| `tests/test_discord_presence.py::Framing::test_partial_frame_survives_a_read_timeout` | `src/runner/discord.rs::tests::partial_frame_survives_a_read_timeout` | ported |
| `tests/test_discord_presence.py::Framing::test_read_frame_raises_when_connection_ends` | `src/runner/discord.rs::tests::read_frame_raises_when_connection_ends` | ported |
| `tests/test_discord_presence.py::Activity::test_only_the_start_time_is_shown` | `src/runner/discord.rs::tests::only_the_start_time_is_shown` | ported |
| `tests/test_discord_presence.py::Activity::test_clearing_omits_the_activity` | `src/runner/discord.rs::tests::clearing_omits_the_activity` | ported |
| `tests/test_discord_presence.py::PresenceLifecycle::test_disabled_without_client_id` | `src/runner/discord.rs::tests::disabled_without_client_id` | ported |
| `tests/test_discord_presence.py::PresenceLifecycle::test_disabled_by_switch` | `src/runner/discord.rs::tests::disabled_by_switch` | ported |
| `tests/test_discord_presence.py::PresenceLifecycle::test_handshake_activity_and_clean_close` | `src/runner/discord.rs::tests::handshake_activity_and_clean_close` | ported |
| `tests/test_discord_presence.py::PresenceLifecycle::test_reconnect_republishes_original_start_time` | `src/runner/discord.rs::tests::reconnect_republishes_original_start_time` | ported |
| `tests/test_discord_presence.py::PresenceLifecycle::test_connected_session_answers_ping_and_keeps_listening` | `src/runner/discord.rs::tests::connected_session_answers_ping_and_keeps_listening` | ported |
| `tests/test_discord_presence.py::PresenceLifecycle::test_close_interrupts_a_silent_handshake_without_reconnecting` | `src/runner/discord.rs::tests::close_interrupts_a_silent_handshake_without_reconnecting` | ported |
| `tests/test_discord_presence.py::PresenceLifecycle::test_native_pid_is_used_for_publish_and_clear` | `src/runner/discord.rs::tests::native_pid_is_used_for_publish_and_clear` | ported |
| `tests/test_discord_presence.py::PresenceLifecycle::test_rejected_handshake_closes_and_retries_later` | `src/runner/discord.rs::tests::rejected_handshake_closes_and_retries_later` | ported |
| `tests/test_discord_presence.py::PresenceLifecycle::test_discord_not_running_does_not_raise` | `src/runner/discord.rs::tests::discord_not_running_does_not_raise` | ported |
| `tests/test_discord_presence.py::PresenceLifecycle::test_close_without_connection_is_harmless` | `src/runner/discord.rs::tests::close_without_connection_is_harmless` | ported |
| `tests/test_discord_presence.py::GameProcessPresence::test_game_pid_is_used_and_presence_closes_after_wait` | `src/runner/discord.rs::tests::game_pid_is_used_and_presence_closes_after_wait` | ported |
| `tests/test_discord_presence.py::GameProcessPresence::test_presence_failure_does_not_interrupt_the_game` | `src/runner/discord.rs::tests::presence_failure_does_not_interrupt_the_game` | ported |
| `tests/test_discord_presence.py::GameProcessPresence::test_wait_exception_still_cleans_up_presence` | `src/runner/discord.rs::tests::wait_exception_still_cleans_up_presence` | ported |
| `tests/test_discord_presence.py::SocketIntegration::test_real_socket_reconnect_ping_and_shutdown` | `src/runner/discord.rs::tests::real_socket_reconnect_ping_and_shutdown` | ported |
| `tests/test_discord_presence.py::WindowsPipeTransport::test_native_pipe_reads_and_writes_without_blocking` | `src/runner/discord.rs::tests::native_pipe_reads_and_writes_without_blocking` | ported |
| `tests/test_discord_presence.py::WindowsPipeTransport::test_close_stops_worker_waiting_for_a_native_pipe_handshake` | `src/runner/discord.rs::tests::close_stops_worker_waiting_for_a_native_pipe_handshake` | ported |
| `tests/test_discord_presence.py::LauncherSwitch::test_off_by_default` | `src/core/env.rs::tests::discord_rich_presence_is_on_by_default` | replaced by `src/core/env.rs::tests::discord_rich_presence_is_on_by_default` |
| `tests/test_discord_presence.py::LauncherSwitch::test_checked_enables_it` | `src/core/env.rs::tests::discord_rich_presence_is_on_by_default` | replaced by `src/core/env.rs::tests::discord_rich_presence_is_on_by_default` |
| `tests/test_discord_presence.py::LauncherSwitch::test_label_is_translated` | — | dropped: no Rust counterpart — the Spanish and Portuguese texts of the Discord switch, its note and Game multisampling (they come from assets/i18n.json; only whole-table completeness is tested) |
| `tests/test_game_update.py::GameUpdateTests::test_update_files_replace_base_files_and_the_rest_stays` | `src/runner/game_update.rs::tests::update_files_replace_base_files_and_the_rest_stays` | ported |
| `tests/test_game_update.py::GameUpdateTests::test_folders_the_update_leaves_alone_are_linked_whole` | `src/runner/game_update.rs::tests::folders_the_update_leaves_alone_are_linked_whole` | ported |
| `tests/test_game_update.py::GameUpdateTests::test_removing_the_view_leaves_the_dumps_whole` | `src/runner/game_update.rs::tests::removing_the_view_leaves_the_dumps_whole` | ported |
| `tests/test_game_update.py::GameUpdateTests::test_dumps_are_not_modified` | `src/runner/game_update.rs::tests::dumps_are_not_modified` | ported |
| `tests/test_game_update.py::GameUpdateTests::test_a_network_share_without_junctions_mirrors_the_tree` | `src/runner/game_update.rs::tests::a_network_share_without_junctions_mirrors_the_tree` | ported |
| `tests/test_game_update.py::GameUpdateTests::test_different_titles_and_nested_folders_are_refused` | `src/runner/game_update.rs::tests::different_titles_and_nested_folders_are_refused` | ported |

## Known deviations from the Python

1. `link_libc` is kept: `out/boot-libc.bin` and `out/libc-link.json` are still written although nothing reads them (spec §2).
2. The pixels the glyph code paints are compared with Pillow's by tolerance, not byte for byte: PSNR ≥ 40 dB per texture (`tests/runner_fixtures.rs::painted_textures_match_pillow_within_tolerance`; measured minimum 52.79 dB, the Nintendo lyre decal), the shape-only textures are byte-identical, and on the real WADs everything outside the painted DXT blocks is byte-identical (`tests/dump.rs::real_glyph_wads_match_python_outside_the_painted_blocks`). The text is drawn through `skrifa`'s port of FreeType's auto-hinter (hinted outlines and advances) filled by `ab_glyph_rasterizer`, not through FreeType and not through `ab_glyph`'s plain outlines (their stems and advances were up to two pixels off); no FreeType is compiled.
3. A missing launcher font skips that texture (Pillow falls back to its default font). A lyre texture wider than 2048 px logs that texture only, where the Python raises `ZeroDivisionError` and loses every glyph. A texture whose DDS header claims more than 4096 px a side is left as it is with `Glyphs: <name>: a <w>x<h> texture is larger than 4096x4096, left as it is`, where the Python draws it at that size (the game's glyph textures are 2048 px at most; a corrupt header that claims millions would abort the process on allocation).
4. A gamepad probe that times out (15 s) or fails keeps the game's own button icons, with no glyph mounts, as `run.py` does: the plan's `a_hanging_gamepad_probe_falls_back_to_keyboard_glyphs` became `tests/runner.rs::a_hanging_gamepad_probe_keeps_the_game_icons`.
5. Discord Rich Presence lives in the runner (`runner/discord.rs`), where the Python is `scripts/discord_presence.py` called by `run.py`. The Windows pipe is opened `PIPE_NOWAIT` and polled with `PeekNamedPipe`, as the Python does, with no reader thread (a pending read on a synchronous pipe blocks the writes and the close). A Unix socket `connect` has no timeout (the Python's `settimeout` also covered it). The launcher sends `SPARTAN_DISCORD=1` by default (launcher deviation 16).
6. `game_icon.ico` is no longer migrated from the legacy launcher folder: the settings migration copies `settings.json` only, and the runner writes the window icon from `launcher/spartanport.png` (spec §2 "Ícone").
7. The multi-line files the runner derives (`analysis.json`, `libc-link.json`, `link.json`, `content-profile.json`, `graphics_state.json`, `patches-merged.xml`) are written with LF where the Python on Windows writes CRLF; the fixtures and the dump tests compare after turning the Python's CRLF into LF. The binary files and the one-line JSON files (`patch_status.json`, the `stamp.json` files) are byte-identical.
8. The XML element nesting is capped at 32 before `roxmltree` parses a patch file (the parser recurses per level and overflows the stack at depth 128 in a debug build; patch files nest 4 deep). The cap also protects the launcher's Home and Mods pages, which parse the same files.
9. Pause menu: script errors the Python raises as `IndexError`, `StopIteration`, `KeyError`, `struct.error` or `OverflowError` (they escape `run.py`'s `except OSError` and crash the runner) are logged like its `ValueError`s (`Pause menu: …; no Exit to Desktop row`) and the game starts without the row. A cyclic state tree is an error, not a hang.
10. Where the Python stops on a built-in exception inside `prepare`, the linkers or the ISA table, the Rust gives a descriptive message instead of the traceback (exit code 1 as the Python's); a binary file is built whole before it is written, so a late error leaves no partial file where the Python left one. Inputs the Python crashes on and the Rust handles: a RIP-relative target at or above 2^63 (stored wrapped), a truncated `PT_LOAD` (an error where the Python silently shortens the image), a pattern byte above `0xff` outside the anchor (an error where the Python never matches).
11. `--play` takes no arguments, as the Python's `play_without_window`, and the runner started by `--play` shows the `MessageBoxW` advice only when no console is attached (started from a terminal it has one and prints there). The spec's `--play [args]` was corrected.
12. Messages: the runner's lines go to its output (`RunContext.out`, stdout under `--run`), among them `fail`'s, the mods step's `Mods: <error>`, and `patches.py`'s `Patches: <file>: <error>` (a mod's patch file that does not parse) and `Patches: skipped <key>: <error>`, all of which the Python writes to stderr. Two kinds of lines bypass that output and go to stderr, as the Python prints them: the `Mods:` lines of `core/mods.rs` (a zip unpacked or refused, a file two mods replace, each mod's file count, the overlay's summary, a stale overlay that cannot go; `mods.py` prints them to stderr) and `Patches: <file>: <error>` for a third-party patch file that does not parse (`core/patches.rs`; `patches.py` prints it to stderr). The launcher merges both streams. Python tracebacks become one line and exit 1 (`link_libc failed: …`, `link_modules failed: …`, `content_profile failed: …`, `Patches: …`, `--after needs a process id, not …`); the two argparse failures exit 2 as the scripts do, but a `SPARTAN_MODS_ENABLED` other than 0 or 1 prints `Mods: SPARTAN_MODS_ENABLED must be 0 or 1, not …` where `mods.py` prints argparse's `invalid choice` text.
13. The update view (`game-update-*`) is also removed when the mods step fails (in `run.py` the `finally` starts after `mods.py`, so a mods failure leaves it behind). Stale `mod-game-*` overlays in `out/` are removed before a new overlay is made, and a refused junction leaves no empty folder.
14. Windows environment names are upper-cased, as `os.environ` stores them. A derived variable whose value is empty stays set (empty) for the probe, where the Python on Windows deletes it (`SPARTAN_EXTRA_RESOLUTIONS` when no mod adds a size); the game treats both alike.
15. Ctrl+C returns 130 from the next check point (after the CPU probe, the image steps and the glyph probe, or at once while the game runs) and does not wait for the game; the Python stops mid-step, so a step can finish first (seconds at most, up to 30 s in `--after` and 15 s in the gamepad probe). Ctrl+Break is handled as Ctrl+C, where the Python dies with `STATUS_CONTROL_C_EXIT` and no cleanup. The 130 path is tested on Linux only.
16. DXT: the transparency branch of Pillow's BC1 encoder is not ported (a DXT1 from RGB reads alpha as 255; its padding pixels read uninitialised memory in the C) and the unused DXT3 encoder is absent. `window_icon.bmp` has the Python's size and 54 header bytes; its pixels are not compared and the resize does not premultiply alpha as Pillow does (spec §2).

## Partly covered

Rows marked `ported` or `replaced by` whose Rust tests cover the behaviour but not every assertion of
the Python test:

- `tests/test_isa_rewrite.py::TableTests::test_fixture_file_is_current`: the Rust test compares the table and the sites with the generator fixture; whether the fixture is current is tests/test_runner_generators.py's job.
- `tests/test_mods.py::ModTests::test_run_uses_overlay_propagates_exit_and_cleans_view`: the test runs the Rust runner with a fake probe (a binary, not the Python shebang script that is skipped on Windows).
- `tests/test_mods.py::GameLayout::test_dvdroot_prefix_is_dropped_when_the_game_has_none`: the count of the lines printed on stderr is not asserted (the Rust test asserts the path mapping).
- `tests/test_mods.py::GameLayout::test_region_mods_keep_metadata_conflicts_and_selection`: the selection and the conflicts of region mods, and the count of their mod_files, are not asserted.
- `tests/test_glyphs.py::GlyphPatchTest::test_patch_keeps_every_size_and_only_touches_glyph_pixels`: the painted pixels are compared with Pillow by tolerance (PSNR), in tests/runner_fixtures.rs::painted_textures_match_pillow_within_tolerance, not byte for byte.
- `tests/test_graphics_options.py::GraphicsOptionsTest::test_resolution_labels_show_the_size_and_aspect_ratio`: the labels of 480p, 900p, 5120x1440, 2560x1600, 1280x800, 1280x960, 1920x1440, 2048x1536, 3840x1080, 1600x1200 and custom are not asserted.
- `tests/test_graphics_options.py::AspectRatioTest::test_aspect_of_a_resolution`: only 2560x1080, an unknown name and a 5:4 size are asserted, not the eight names of the Python.
- `tests/test_graphics_options.py::AspectRatioTest::test_aspects_in_a_fixed_order_with_the_others_last`: the order of the aspect list is asserted by its first entry and by 4:3 being offered; the full list and the 5:4 tail are not.
- `tests/test_graphics_options.py::AspectRatioTest::test_resolutions_of_an_aspect_smallest_first_native_on_top`: native on top and the 16:10 list are asserted; the 16:9 and 4:3 lists are not.
- `tests/test_graphics_options.py::AspectRatioTest::test_switching_aspect_keeps_the_closest_size`: the 21:9 and 16:10 moves are asserted; the 4:3 and 16:9 cases of the Python are not.
- `tests/test_graphics_options.py::AspectRatioTest::test_labels_without_the_aspect_ratio_for_the_filtered_list`: 1440p and 3440x1440 are asserted; 1800p and the (beta) suffix of 2560x1080 without the ratio are not.
- `tests/test_legacy.py::UpgradeFromFormerNameTest::test_the_old_settings_folder_name_is_the_former_one`: each platform asserts only its own path; the other one runs on that OS.
- `tests/test_legacy.py::UpgradeFromFormerNameTest::test_windows_settings_move_and_saves_stay`: game_icon.ico is no longer copied (the runner spec, §2 "Ícone"), and the saves staying in place is not asserted.
- `tests/test_legacy.py::UpgradeFromFormerNameTest::test_linux_appimage_settings_and_saves_are_copied`: game_icon.ico is no longer copied (the runner spec, §2 "Ícone"); the copy of the game ini is asserted apart.
- `tests/test_legacy.py::WindowsDesktopShortcutTest::test_repeated_launcher_starts_do_not_retrigger_migration`: the Rust test covers the migration, the kept original and the second start in one test.
- `tests/test_legacy.py::WindowsDesktopShortcutTest::test_ambiguous_ownership_preserves_existing_shortcut`: the Rust test covers the migration, the kept original and the marker in one test.
- `tests/test_discord_presence.py::PresenceLifecycle::test_close_without_connection_is_harmless`: the Rust test uses an opener with no Discord; the Python starts a real thread and could reach a running Discord. Close twice is shutdown() then close().
- `tests/test_discord_presence.py::GameProcessPresence::test_wait_exception_still_cleans_up_presence`: the wait panics in the Rust test where the Python raises KeyboardInterrupt.
- `tests/test_discord_presence.py::SocketIntegration::test_real_socket_reconnect_ping_and_shutdown`: the Rust test runs on Unix only (a UnixListener); on Windows the pipe tests run instead.
- `tests/test_discord_presence.py::LauncherSwitch::test_off_by_default`: the Rust default is on (R91, launcher deviation 16): the Rust test asserts that default, and that a file without the key and an explicit false give 1 and 0.
- `tests/test_discord_presence.py::LauncherSwitch::test_checked_enables_it`: an explicit true is not set apart from the default, which is true.

## Gaps

The 12 rows marked `dropped: no Rust counterpart`: Python behaviour that no Rust test covers.

- `tests/test_mods.py::ModTests::test_unresolvable_zip_target_does_not_stop_other_mods` — a zip whose extraction folder cannot be resolved (an unreadable target, a symlink loop) is skipped and the other zips still unpack
- `tests/test_legacy.py::UpgradeFromFormerNameTest::test_settings_already_in_the_old_data_folder_win` — a settings.json already in the old data folder (a build between the release and the rename) is read instead of the older one in the legacy config folder
- `tests/test_legacy.py::UpgradeFromFormerNameTest::test_start_moves_before_reading_settings` — the start moves the legacy files before it reads the settings (no test pins the order in startup.rs)
- `tests/test_legacy.py::WindowsDesktopShortcutTest::test_shortcut_manually_renamed_to_legacy_title` — a shortcut with the old title that points to SpartanPort.exe is kept and no SpartanPort.lnk is made (needs PowerShell)
- `tests/test_legacy.py::WindowsDesktopShortcutTest::test_user_shortcut_targeting_play_legacy_executable` — a shortcut with the old title that points to the Play executable is kept, issue 824 (needs PowerShell)
- `tests/test_legacy.py::WindowsDesktopShortcutTest::test_existing_spartanport_shortcut_is_preserved` — an existing SpartanPort.lnk is kept and the old shortcut stays (needs PowerShell)
- `tests/test_legacy.py::WindowsDesktopShortcutTest::test_failed_shortcut_creation_preserves_old_shortcut_and_does_not_write_marker` — a failed PowerShell save keeps the old shortcut and writes no marker (the Python mocks subprocess.run; the Rust has no seam)
- `tests/test_legacy.py::WindowsDesktopShortcutTest::test_powershell_error_does_not_write_marker_and_allows_recovery` — a PowerShell crash writes no marker and the next start recovers (the Python mocks subprocess.run; the Rust has no seam)
- `tests/test_legacy.py::WindowsDesktopShortcutTest::test_shortcut_targeting_unrelated_executable` — a shortcut with the old title that points to an unrelated executable is kept (needs PowerShell)
- `tests/test_legacy.py::WindowsDesktopShortcutTest::test_windows_paths_containing_spaces` — the shortcut migration works for a port folder and a desktop folder whose paths contain spaces (needs PowerShell)
- `tests/test_legacy.py::WindowsDesktopShortcutTest::test_launcher_move_from_legacy_name_integration` — the launcher start runs the shortcut migration, keeps a custom Play shortcut and writes the marker (needs PowerShell)
- `tests/test_discord_presence.py::LauncherSwitch::test_label_is_translated` — the Spanish and Portuguese texts of the Discord switch, its note and Game multisampling (they come from assets/i18n.json; only whole-table completeness is tested)
