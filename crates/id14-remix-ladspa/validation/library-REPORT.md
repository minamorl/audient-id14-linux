domain: audient-id14-realtime-sr.spec@0.13

pins_read:
- audient-id14-realtime-sr.spec@0.13 — playback transparency, fallback, overload visibility, latency and audio constraints.
- house_style@4.0 — errors/log envelope, UTC and identifier conventions.
- prohibitions@1.0 — cross-cutting prohibitions.
- Explicit caller seam — runtime env ID14_ORT_LIBRARY, then HOME GC-root symlink, then normal libonnxruntime.so loader search.

implemented:
- src/model.rs — removed compile-time native library path; validates each runtime candidate before installing a process-global C API; retains the accepted handle.
- tools/verify_library_discovery.py — real release .so in fresh processes; usable-library precedence, missing/invalid fallback, unset HOME, absence, bit-exact neutral audio, State/JSON visibility and owned cleanup. Reads the source and built binary to reject compile-time env lookup and embedded build-path values.
- tools/check_library_paths.sh — repeatable sequential gates with runtime/temp files inside the worktree and raw command/output/exit records.
- IMPLEMENTATION.md, RESEARCH.md, tools/with_cargo.sh — discovery/delivery documentation, primary-source record and runtime-only environment clarification.

assumptions:
- Missing or incompatible candidates fall through to the next candidate. The first usable native API remains process-wide; installing a library after an all-absent result requires a process restart, as with the existing OnceLock behavior.

undecidable: []
contradictions: []
verdict: IMPLEMENTED

checks:
The commands below ran from the assigned worktree. Environment entries apply to the runner's children. Each section retains the actual combined stdout/stderr and captured exit status; workspace test source was not opened. The runtime symlink is a test fixture, not an installer deployment or an actual garbage-collection run.

```sh
ID14_ORT_LIBRARY=/nix/store/3k9p1crsk8faxgir3xb5jimz9wwsm0ih-onnxruntime-1.27.1/lib/libonnxruntime.so
XDG_RUNTIME_DIR=/home/minamorl/repos/.worktrees/audient-id14-linux/remix-runtime/crates/id14-remix-ladspa/.build/check-runtime
TMPDIR=/home/minamorl/repos/.worktrees/audient-id14-linux/remix-runtime/crates/id14-remix-ladspa/.build/tmp
bash /home/minamorl/repos/.worktrees/audient-id14-linux/remix-runtime/crates/id14-remix-ladspa/tools/with_cargo.sh test --workspace 
env ID14_ORT_LIBRARY=/home/minamorl/repos/.worktrees/audient-id14-linux/remix-runtime/crates/id14-remix-ladspa/.build/BUILD_ONLY_ORT_SENTINEL_MUST_NOT_BE_EMBEDDED.so bash /home/minamorl/repos/.worktrees/audient-id14-linux/remix-runtime/crates/id14-remix-ladspa/tools/with_cargo.sh build --release -p id14-remix-ladspa 
bash /home/minamorl/repos/.worktrees/audient-id14-linux/remix-runtime/crates/id14-remix-ladspa/tools/with_python.sh /home/minamorl/repos/.worktrees/audient-id14-linux/remix-runtime/crates/id14-remix-ladspa/tools/verify_library_discovery.py --so /home/minamorl/repos/.worktrees/audient-id14-linux/remix-runtime/crates/id14-remix-ladspa/.build/target/release/libid14_remix_ladspa.so --library /nix/store/3k9p1crsk8faxgir3xb5jimz9wwsm0ih-onnxruntime-1.27.1/lib/libonnxruntime.so --build-library /home/minamorl/repos/.worktrees/audient-id14-linux/remix-runtime/crates/id14-remix-ladspa/.build/BUILD_ONLY_ORT_SENTINEL_MUST_NOT_BE_EMBEDDED.so 
bash /home/minamorl/repos/.worktrees/audient-id14-linux/remix-runtime/crates/id14-remix-ladspa/tools/with_cargo.sh run --release -p id14-remix-ladspa --example bench_worker -- /home/minamorl/repos/.worktrees/audient-id14-linux/remix-runtime/crates/id14-remix-ladspa/.build/remix-2000.onnx 500 
bash /home/minamorl/repos/.worktrees/audient-id14-linux/remix-runtime/crates/id14-remix-ladspa/tools/with_python.sh /home/minamorl/repos/.worktrees/audient-id14-linux/remix-runtime/crates/id14-remix-ladspa/tools/verify_so.py --so /home/minamorl/repos/.worktrees/audient-id14-linux/remix-runtime/crates/id14-remix-ladspa/.build/target/release/libid14_remix_ladspa.so --output /home/minamorl/repos/.worktrees/audient-id14-linux/remix-runtime/crates/id14-remix-ladspa/validation/library-so-results.json 
bash /home/minamorl/repos/.worktrees/audient-id14-linux/remix-runtime/crates/id14-remix-ladspa/tools/with_python.sh /home/minamorl/repos/.worktrees/audient-id14-linux/remix-runtime/crates/id14-remix-ladspa/tools/verify_state_so.py --so /home/minamorl/repos/.worktrees/audient-id14-linux/remix-runtime/crates/id14-remix-ladspa/.build/target/release/libid14_remix_ladspa.so 
bash /home/minamorl/repos/.worktrees/audient-id14-linux/remix-runtime/crates/id14-remix-ladspa/tools/with_python.sh /home/minamorl/repos/.worktrees/audient-id14-linux/remix-runtime/crates/id14-remix-ladspa/tools/verify_realtime_so.py --so /home/minamorl/repos/.worktrees/audient-id14-linux/remix-runtime/crates/id14-remix-ladspa/.build/target/release/libid14_remix_ladspa.so --model /home/minamorl/repos/.worktrees/audient-id14-linux/remix-runtime/crates/id14-remix-ladspa/.build/remix-2000.onnx --seconds 30 
git diff --check 
```

workspace:

```text
    Finished `test` profile [unoptimized] target(s) in 0.07s
     Running unittests src/lib.rs (crates/id14-remix-ladspa/.build/target/debug/deps/id14_protocol-194b8cb0c917a2b5)

running 40 tests
test control_error::tests::dfu_missing_message_states_no_interface_0_fallback ... ok
test db::tests::formatting ... ok
test db::tests::unrepresentable_decibels_are_rejected ... ok
test db::tests::whole_and_fractional_decibels ... ok
test descriptor::tests::dfu_interface_is_found_from_descriptor ... ok
test descriptor::tests::ambiguous_dfu_interfaces_are_refused ... ok
test descriptor::tests::dfu_interface_numbered_0_to_2_is_refused ... ok
test descriptor::tests::missing_dfu_interface_is_an_error_without_interface_0_fallback ... ok
test descriptor::tests::extension_code_is_the_wire_field_not_the_unit_id ... ok
test descriptor::tests::missing_mixer_output_volume_is_an_error ... ok
test descriptor::tests::mk2_fixture_entities_parse ... ok
test descriptor::tests::non_uac2_audio_control_is_refused ... ok
test descriptor::tests::truncated_and_zero_length_descriptors_are_errors ... ok
test evidence::tests::never_hardware_confirmed ... ok
test evidence::tests::status_is_static_inferred_unverified ... ok
test extension::tests::pinned_codes ... ok
test extension::tests::round_trip ... ok
test plan::tests::dump_plan_covers_the_four_targets_and_nothing_else ... ok
test plan::tests::dump_plan_is_get_only_and_never_addresses_extension_units ... ok
test plan::tests::mixer_nodes_use_uac2_mixer_control_numbers ... ok
test plan::tests::mk2_declares_no_mute_control ... ok
test plan::tests::values_render_by_kind ... ok
test plan::tests::volume_on_undeclared_channel_is_refused ... ok
test plan::tests::volume_targets_the_mixer_output_feature_unit ... ok
test product::tests::lookup_by_vid_pid ... ok
test product::tests::pinned_vid_pid ... ok
test product::tests::table_covers_both_variants ... ok
test request::tests::header_wire_bytes_le ... ok
test request::tests::pinned_headers ... ok
test request::tests::request_bytes_are_header_then_body ... ok
test row_evidence::tests::get_direction_rows_are_mk2_observed ... ok
test row_evidence::tests::set_header_stays_static_inferred ... ok
test uac2::tests::cur_encoding_round_trips ... ok
test uac2::tests::feature_controls_map_selectors_and_capabilities ... ok
test uac2::tests::get_cur_setup_packet_bytes ... ok
test uac2::tests::get_range_and_set_cur_headers ... ok
test uac2::tests::header_evidence_follows_rows ... ok
test uac2::tests::range_block_decoding_ignores_trailing_bytes ... ok
test uac2::tests::range_block_rejects_short_and_empty ... ok
test uac2::tests::setup_headers_match_the_pinned_request_headers ... ok

test result: ok. 40 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running tests/pinned_values.rs (crates/id14-remix-ladspa/.build/target/debug/deps/pinned_values-79d86291a0c2a0e0)

running 4 tests
test extension_code_pins ... ok
test evidence_status_pins ... ok
test product_table_pins ... ok
test request_header_pins ... ok

test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running unittests src/lib.rs (crates/id14-remix-ladspa/.build/target/debug/deps/id14_remix_ladspa-9aa0eaed2d8d05a9)

running 16 tests
test state::tests::model_failure_visible_without_audio_callback ... ok
test dsp::tests::fft_roundtrip_and_weighted_overlap ... ok
test dsp::tests::bass_guard_stopband ... ok
test queue::tests::concurrent_order_and_backpressure ... ok
test state::tests::unavailable_runtime_preserves_audio_bits ... ok
test model::tests::rejects_contract_and_runtime_value_errors ... ok
test model::tests::accepts_valid_models_and_maps_reordered_names ... ok
test verification::exact_dry_paths_and_measured_delay ... ok
test verification::silence_adds_no_signal ... ok
test verification::peak_protection_keeps_hot_dc_input_unchanged ... ok
test state::tests::publication_updates_heartbeat_and_owned_cleanup ... ok
test verification::lookahead_masks_align_with_their_original_spectra ... ok
test verification::active_gain_stereo_and_callback_cost ... ok
test verification::stalled_worker_and_switches_fade_current_audio_to_dry ... ok
test verification::independent_true_peak_oracle ... ok
test verification::low_band_retention ... ok

test result: ok. 16 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 6.87s

     Running unittests src/lib.rs (crates/id14-remix-ladspa/.build/target/debug/deps/id14_sr-3473fdbe9005ff47)

running 11 tests
test tests::embedded_output_matches_training_runtime ... ok
test tests::on_off_and_fullband_bypass_are_callable ... ok
test tests::completion_keeps_stereo_channels_independent ... ok
test tests::boosted_high_level_transient_is_limited_without_flat_topping ... ok
test tests::bypass_preserves_stereo_with_fixed_delay ... ok
test tests::peak_guard_covers_the_overlap_tail_after_leaving_boosted_mix ... ok
test tests::trained_path_changes_bandlimited_audio ... ok
test source_safety_regression_tests::fullband_after_long_zero_mix_preserves_aligned_source_at_transition ... ok
test tests::enabled_path_preserves_existing_low_tone ... ok
test tests::continuous_mix_endpoints_match_existing_api ... ok
test source_safety_regression_tests::bandlimited_stereo_completion_is_swap_symmetric_and_mono_stays_mono ... ok

test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.31s

     Running unittests src/bin/bench.rs (crates/id14-remix-ladspa/.build/target/debug/deps/sr_bench-9f9dcfbb0c035f4e)

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running unittests src/bin/render.rs (crates/id14-remix-ladspa/.build/target/debug/deps/sr_render-fc32c600d0913887)

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running unittests src/lib.rs (crates/id14-remix-ladspa/.build/target/debug/deps/id14_sr_ladspa-146f2779152e3e38)

running 18 tests
test denormal_scope::tests::restores_callers_environment_after_nested_scopes ... ok
test denormal_scope::tests::restores_callers_environment_on_unwind ... ok
test tests::descriptor_has_one_stereo_plugin ... ok
test bass_guard::tests::high_frequency_residual_survives_and_nonfinite_input_recovers ... ok
test tests::unsupported_sample_rate_is_safe_passthrough ... ok
test tests::adapter_and_engine_delay_is_exactly_1024_frames ... ok
test bass_guard::tests::adapter_dry_impulse_retains_1024_frame_delay ... ok
test tests::zero_mix_matches_the_engine_bypass_with_adapter_delay ... ok
test tests::detector_preserves_fullband_material_and_releases_quickly ... ok
test tests::detector_enables_completion_for_12k_bandlimited_material ... ok
test tests::owner_mix_is_bounded_and_ramped_then_reaches_exact_bypass ... ok
test tests::above_hundred_never_emits_a_sample_beyond_full_scale ... ok
test bass_guard::tests::bass_criterion_known_contamination ... ok
test tests::arbitrary_host_blocks_match_fixed_host_blocks ... ok
test tests::manual_mix_remains_effective_on_bandlimited_input ... ok
test tests::manual_mix_protects_recorded_fullband_highs ... ok
test bass_guard::tests::adapter_block_partition_does_not_change_output ... ok
test bass_guard::tests::bass_criterion_adapter_all_mixes ... ok

test result: ok. 18 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.61s

     Running unittests src/bin/adapter-bench.rs (crates/id14-remix-ladspa/.build/target/debug/deps/adapter_bench-f3acf22a09b8e55a)

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running unittests src/bin/silent_load_probe.rs (crates/id14-remix-ladspa/.build/target/debug/deps/silent_load_probe-48ab240f901c5fd3)

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running unittests src/lib.rs (crates/id14-remix-ladspa/.build/target/debug/deps/id14ctl-caadaa83e7388d2c)

running 15 tests
test ops::tests::dry_run_line_shows_setup_payload_and_evidence ... ok
test ops::tests::dry_run_volume_prints_set_bytes_and_payload ... ok
test ops::tests::format_bytes_is_exact_lowercase_hex ... ok
test ops::tests::volume_in_range_sets_then_reads_back ... ok
test ops::tests::mute_fails_stating_no_declared_mute_control ... ok
test ops::tests::dump_reports_failed_reads_and_continues ... ok
test usb::tests::autodetect_prefers_mk2 ... ok
test ops::tests::dump_sends_only_get_requests_and_never_touches_extension_units ... ok
test usb::tests::differing_configuration_values_are_refused ... ok
test ops::tests::volume_out_of_range_is_rejected_without_set ... ok
test usb::tests::differing_interface_content_is_refused ... ok
test ops::tests::dry_run_dump_lists_requests ... ok
test usb::tests::identical_candidates_are_selected ... ok
test usb::tests::no_readable_candidate_is_not_guessed_around ... ok
test usb::tests::only_not_found_allows_the_indexed_fallback ... ok

test result: ok. 15 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running unittests src/main.rs (crates/id14-remix-ladspa/.build/target/debug/deps/id14ctl-3cb6c66015127a74)

running 5 tests
test tests::protocol_lookup_identifies_mk1_and_mk2_vid_pid_pairs ... ok
test tests::dry_run_format_is_exact_lowercase_hex ... ok
test tests::request_bytes_are_deterministic_for_all_pinned_headers ... ok
test tests::write_commands_are_disabled_without_explicit_gate ... ok
test tests::clap_definition_and_representative_arguments_are_valid ... ok

test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

   Doc-tests id14_protocol

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

   Doc-tests id14_remix_ladspa

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

   Doc-tests id14_sr

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

   Doc-tests id14_sr_ladspa

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

   Doc-tests id14ctl

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

EXIT=0
```

build:

```text
    Finished `release` profile [optimized] target(s) in 0.06s
EXIT=0
```

discovery:

```text
{"check": "no_baked_runtime_path", "forbidden_paths": ["/home/minamorl/repos/.worktrees/audient-id14-linux/remix-runtime/crates/id14-remix-ladspa/.build/BUILD_ONLY_ORT_SENTINEL_MUST_NOT_BE_EMBEDDED.so", "/nix/store/3k9p1crsk8faxgir3xb5jimz9wwsm0ih-onnxruntime-1.27.1/lib/libonnxruntime.so", "/nix/store/3k9p1crsk8faxgir3xb5jimz9wwsm0ih-onnxruntime-1.27.1/lib/libonnxruntime.so.1.27.1"], "matches": 0, "compile_time_env_macros": 0}
{"bit_mismatches": null, "check": "env_before_home_and_loader", "cleanup_removed_state": true, "mapped_libraries": ["/home/minamorl/repos/.worktrees/audient-id14-linux/remix-runtime/crates/id14-remix-ladspa/.build/library-discovery-4ug3ltpy/explicit/libonnxruntime.so"], "published_state": 1, "published_state_name": "Active", "state": 1}
{"bit_mismatches": null, "check": "home_before_loader", "cleanup_removed_state": true, "mapped_libraries": ["/nix/store/3k9p1crsk8faxgir3xb5jimz9wwsm0ih-onnxruntime-1.27.1/lib/libonnxruntime.so.1.27.1"], "published_state": 1, "published_state_name": "Active", "state": 1}
{"bit_mismatches": null, "check": "missing_env_falls_through_to_home", "cleanup_removed_state": true, "mapped_libraries": ["/nix/store/3k9p1crsk8faxgir3xb5jimz9wwsm0ih-onnxruntime-1.27.1/lib/libonnxruntime.so.1.27.1"], "published_state": 1, "published_state_name": "Active", "state": 1}
{"bit_mismatches": null, "check": "invalid_env_falls_through_to_home", "cleanup_removed_state": true, "mapped_libraries": ["/nix/store/3k9p1crsk8faxgir3xb5jimz9wwsm0ih-onnxruntime-1.27.1/lib/libonnxruntime.so.1.27.1"], "published_state": 1, "published_state_name": "Active", "state": 1}
{"bit_mismatches": null, "check": "normal_loader_search", "cleanup_removed_state": true, "mapped_libraries": ["/home/minamorl/repos/.worktrees/audient-id14-linux/remix-runtime/crates/id14-remix-ladspa/.build/library-discovery-4ug3ltpy/loader/libonnxruntime.so"], "published_state": 1, "published_state_name": "Active", "state": 1}
{"bit_mismatches": null, "check": "missing_env_and_home_fall_through", "cleanup_removed_state": true, "mapped_libraries": ["/home/minamorl/repos/.worktrees/audient-id14-linux/remix-runtime/crates/id14-remix-ladspa/.build/library-discovery-4ug3ltpy/loader/libonnxruntime.so"], "published_state": 1, "published_state_name": "Active", "state": 1}
{"bit_mismatches": null, "check": "unset_home_uses_loader", "cleanup_removed_state": true, "mapped_libraries": ["/home/minamorl/repos/.worktrees/audient-id14-linux/remix-runtime/crates/id14-remix-ladspa/.build/library-discovery-4ug3ltpy/loader/libonnxruntime.so"], "published_state": 1, "published_state_name": "Active", "state": 1}
{"error":{"code":"ModelInvalid","details":"remix-v1 signature, metadata, operators or outputs are invalid","message":"Remix model unavailable","trace_id":"01a10ad4-5934-7205-a67f-55841c294424"},"level":"warn","msg":"Remix model unavailable; continuing with neutral playback","trace_id":"01a10ad4-5934-7205-a67f-55841c294424","ts":1791183051.060946}
{"bit_mismatches": 0, "check": "all_absent_visible_neutral", "cleanup_removed_state": true, "mapped_libraries": [], "published_state": 5, "published_state_name": "ModelInvalid", "state": 5}
{"error":{"code":"ModelInvalid","details":"remix-v1 signature, metadata, operators or outputs are invalid","message":"Remix model unavailable","trace_id":"01a10ad4-5b36-71e3-abbb-4884953d25fa"},"level":"warn","msg":"Remix model unavailable; continuing with neutral playback","trace_id":"01a10ad4-5b36-71e3-abbb-4884953d25fa","ts":1791183051.5747812}
{"bit_mismatches": 0, "check": "bad_override_all_absent_visible_neutral", "cleanup_removed_state": true, "mapped_libraries": [], "published_state": 5, "published_state_name": "ModelInvalid", "state": 5}
{"error":{"code":"ModelInvalid","details":"remix-v1 signature, metadata, operators or outputs are invalid","message":"Remix model unavailable","trace_id":"01a10ad4-5d6b-7654-9e05-9e538649cfa6"},"level":"warn","msg":"Remix model unavailable; continuing with neutral playback","trace_id":"01a10ad4-5d6b-7654-9e05-9e538649cfa6","ts":1791183052.1397605}
{"check": "unavailable_native_runtime", "library": "/home/minamorl/repos/.worktrees/audient-id14-linux/remix-runtime/crates/id14-remix-ladspa/.build/library-discovery-4ug3ltpy/missing.so", "state": 5, "bit_mismatches": 0}
RUNTIME_FAILURE_VERIFICATION_OK
LIBRARY_DISCOVERY_VERIFICATION_OK
EXIT=0
```

bench:

```text
    Finished `release` profile [optimized] target(s) in 0.05s
     Running `crates/id14-remix-ladspa/.build/target/release/examples/bench_worker /home/minamorl/repos/.worktrees/audient-id14-linux/remix-runtime/crates/id14-remix-ladspa/.build/remix-2000.onnx 500`
{"load_stage":"actual_adapter_load_and_probe","ms":24.627261}
{"adapter_load_stages_ms":{"decode_validate":0.914501,"probe":1.094611,"runtime_init":6.260797,"session_optimize_build":15.961621}}
{"max_ms":0.0014399999999999999,"p50_ms":0.00019999999999999998,"p99_ms":0.00039999999999999996,"phase":"adapter_input_copy","samples":500}
{"max_ms":0.695581,"p50_ms":0.670211,"p99_ms":0.6872309999999999,"phase":"adapter_session_run","samples":500}
{"max_ms":0.00625,"p50_ms":0.00268,"p99_ms":0.0029000000000000002,"phase":"adapter_output_copy_validate","samples":500}
{"max_ms":0.699531,"p50_ms":0.662811,"p99_ms":0.6806610000000001,"phase":"actual_worker_infer","samples":500}
EXIT=0
```

so:

```text
{"error":{"code":"ModelMissing","details":"file is missing or unreadable","message":"Remix model unavailable","trace_id":"01a10ad4-6893-7638-81db-48f7574ad09e"},"level":"warn","msg":"Remix model unavailable; continuing with neutral playback","trace_id":"01a10ad4-6893-7638-81db-48f7574ad09e","ts":1791183054.9953833}
{"bit_mismatches": 0, "check": "transparency_and_delay", "measured_frames": 3776, "mode": "missing", "reported_frames": 3776, "sr_inclusive_ms": 100.0, "state": 4}
{"bit_mismatches": 0, "check": "transparency_and_delay", "measured_frames": 3776, "mode": "off", "reported_frames": 3776, "sr_inclusive_ms": 100.0, "state": 2}
{"bit_mismatches": 0, "check": "transparency_and_delay", "measured_frames": 3776, "mode": "zero", "reported_frames": 3776, "sr_inclusive_ms": 100.0, "state": 3}
{"bit_mismatches": 0, "check": "transparency_and_delay", "measured_frames": 3776, "mode": "inplace", "reported_frames": 3776, "sr_inclusive_ms": 100.0, "state": 2}
{"error":{"code":"ModelInvalid","details":"remix-v1 signature, metadata, operators or outputs are invalid","message":"Remix model unavailable","trace_id":"01a10ad4-6cca-724f-a961-56ba5f255b20"},"level":"warn","msg":"Remix model unavailable; continuing with neutral playback","trace_id":"01a10ad4-6cca-724f-a961-56ba5f255b20","ts":1791183056.0744042}
{"bit_mismatches": 0, "check": "invalid_model_neutral", "fixture": "bad_contract", "state": 5}
{"error":{"code":"ModelInvalid","details":"remix-v1 signature, metadata, operators or outputs are invalid","message":"Remix model unavailable","trace_id":"01a10ad4-6cfd-73ce-ab46-41dedcdd43a8"},"level":"warn","msg":"Remix model unavailable; continuing with neutral playback","trace_id":"01a10ad4-6cfd-73ce-ab46-41dedcdd43a8","ts":1791183056.1250613}
{"bit_mismatches": 0, "check": "invalid_model_neutral", "fixture": "bad_q", "state": 5}
{"error":{"code":"ModelInvalid","details":"remix-v1 signature, metadata, operators or outputs are invalid","message":"Remix model unavailable","trace_id":"01a10ad4-6d2f-72db-895d-65b2963fb10d"},"level":"warn","msg":"Remix model unavailable; continuing with neutral playback","trace_id":"01a10ad4-6d2f-72db-895d-65b2963fb10d","ts":1791183056.1759572}
{"bit_mismatches": 0, "check": "invalid_model_neutral", "fixture": "unsupported_q", "state": 5}
{"error":{"code":"ModelInvalid","details":"remix-v1 signature, metadata, operators or outputs are invalid","message":"Remix model unavailable","trace_id":"01a10ad4-6d58-7146-8b88-bc30e5035687"},"level":"warn","msg":"Remix model unavailable; continuing with neutral playback","trace_id":"01a10ad4-6d58-7146-8b88-bc30e5035687","ts":1791183056.216811}
{"bit_mismatches": 0, "check": "invalid_model_neutral", "fixture": "bad_shape", "state": 5}
{"error":{"code":"ModelInvalid","details":"remix-v1 signature, metadata, operators or outputs are invalid","message":"Remix model unavailable","trace_id":"01a10ad4-6d8c-76c8-b664-74a39b9fc8ac"},"level":"warn","msg":"Remix model unavailable; continuing with neutral playback","trace_id":"01a10ad4-6d8c-76c8-b664-74a39b9fc8ac","ts":1791183056.268035}
{"bit_mismatches": 0, "check": "invalid_model_neutral", "fixture": "bad_sum", "state": 5}
{"error":{"code":"ModelInvalid","details":"remix-v1 signature, metadata, operators or outputs are invalid","message":"Remix model unavailable","trace_id":"01a10ad4-6dbe-70ca-8a8b-77ae9d8ac0d5"},"level":"warn","msg":"Remix model unavailable; continuing with neutral playback","trace_id":"01a10ad4-6dbe-70ca-8a8b-77ae9d8ac0d5","ts":1791183056.318753}
{"bit_mismatches": 0, "check": "invalid_model_neutral", "fixture": "negative", "state": 5}
{"error":{"code":"ModelInvalid","details":"remix-v1 signature, metadata, operators or outputs are invalid","message":"Remix model unavailable","trace_id":"01a10ad4-6df1-7499-abbf-da0498c432a7"},"level":"warn","msg":"Remix model unavailable; continuing with neutral playback","trace_id":"01a10ad4-6df1-7499-abbf-da0498c432a7","ts":1791183056.3696659}
{"bit_mismatches": 0, "check": "invalid_model_neutral", "fixture": "nan_mask", "state": 5}
{"error":{"code":"ModelInvalid","details":"remix-v1 signature, metadata, operators or outputs are invalid","message":"Remix model unavailable","trace_id":"01a10ad4-6e25-749f-8590-60b55b21b1cf"},"level":"warn","msg":"Remix model unavailable; continuing with neutral playback","trace_id":"01a10ad4-6e25-749f-8590-60b55b21b1cf","ts":1791183056.4212725}
{"bit_mismatches": 0, "check": "invalid_model_neutral", "fixture": "nan_state", "state": 5}
{"bit_mismatches": 0, "check": "unsupported_rate", "state": 8}
{"bit_mismatches": 0, "check": "reactivation", "history_reset": true}
{"check": "continuity", "event": "stop_worker", "max_envelope_sample_step": 8.685695671814542e-05, "min_amplitude": 0.09999999852325936, "settle_ms": 64.64583333333333, "state": 6}
{"check": "continuity", "event": "off", "max_envelope_sample_step": 8.700021638405331e-05, "min_amplitude": 0.09999999852325936, "settle_ms": 9.979166666666666, "state": 2}
{"check": "continuity", "event": "zero", "max_envelope_sample_step": 8.700021638405331e-05, "min_amplitude": 0.09999999852325936, "settle_ms": 9.979166666666666, "state": 3}
{"check": "continuity", "event": "on", "max_envelope_sample_step": 8.678293259295478e-05, "min_amplitude": 0.09999999852325936, "settle_ms": 9.958333333333334, "state": 1}
{"check": "bass", "difference_relative_db": -154.4690008924507, "error_rms_dbfs": -98.39755507447686, "hz": 50}
{"check": "bass", "difference_relative_db": -132.58393633944615, "error_rms_dbfs": -95.37833409233285, "hz": 100}
{"check": "bass", "difference_relative_db": -124.48128599013788, "error_rms_dbfs": -88.07848232759447, "hz": 200}
{"check": "bass", "difference_relative_db": -93.20405793571398, "error_rms_dbfs": -74.11870225384304, "hz": 299}
{"check": "bass", "difference_relative_db": -90.05755353529824, "error_rms_dbfs": -75.77656275602371, "hz": 300}
{"check": "stereo_multitone", "complex_lr_ratio_error": 6.68213993737612e-07, "fixture": "voice", "hz": 500}
{"check": "stereo_multitone", "complex_lr_ratio_error": 6.734923224282662e-07, "fixture": "voice", "hz": 1000}
{"check": "stereo_multitone", "complex_lr_ratio_error": 8.365412954337746e-09, "fixture": "voice", "hz": 3000}
{"check": "callback_256", "fixture": "voice", "overload_callbacks": 0, "thread_cpu_us_max": 847.451, "thread_cpu_us_p50": 549.5, "thread_cpu_us_p99": 700.6524000000001, "wall_us_max": 854.691, "wall_us_p99": 703.1507399999999}
{"check": "stereo_multitone", "complex_lr_ratio_error": 6.760819863042626e-08, "fixture": "uniform", "hz": 500}
{"check": "stereo_multitone", "complex_lr_ratio_error": 6.733776471867578e-08, "fixture": "uniform", "hz": 1000}
{"check": "stereo_multitone", "complex_lr_ratio_error": 1.017676313835119e-09, "fixture": "uniform", "hz": 3000}
{"check": "callback_256", "fixture": "uniform", "overload_callbacks": 0, "thread_cpu_us_max": 866.991, "thread_cpu_us_p50": 547.281, "thread_cpu_us_p99": 768.8237999999999, "wall_us_max": 873.551, "wall_us_p99": 776.2717999999998}
{"check": "stereo_multitone", "complex_lr_ratio_error": 2.535994083899106e-05, "fixture": "voice_bins", "hz": 500}
{"check": "stereo_multitone", "complex_lr_ratio_error": 2.289903085634304e-05, "fixture": "voice_bins", "hz": 1000}
{"check": "stereo_multitone", "complex_lr_ratio_error": 2.1938681407505174e-07, "fixture": "voice_bins", "hz": 3000}
{"check": "callback_256", "fixture": "voice_bins", "overload_callbacks": 0, "thread_cpu_us_max": 790.101, "thread_cpu_us_p50": 547.52, "thread_cpu_us_p99": 740.3583399999999, "wall_us_max": 795.211, "wall_us_p99": 743.8985999999998}
{"check": "stereo_multitone", "complex_lr_ratio_error": 6.68213993737612e-07, "fixture": "q3", "hz": 500}
{"check": "stereo_multitone", "complex_lr_ratio_error": 6.734923224282662e-07, "fixture": "q3", "hz": 1000}
{"check": "stereo_multitone", "complex_lr_ratio_error": 8.365412954337746e-09, "fixture": "q3", "hz": 3000}
{"check": "callback_256", "fixture": "q3", "overload_callbacks": 0, "thread_cpu_us_max": 860.851, "thread_cpu_us_p50": 548.811, "thread_cpu_us_p99": 797.30046, "wall_us_max": 868.421, "wall_us_p99": 802.261}
{"check": "stereo_bin", "complex_lr_ratio_error": 1.4713994710067667e-09, "hz": 500}
{"check": "stereo_bin", "complex_lr_ratio_error": 1.2523686193050435e-09, "hz": 1000}
{"check": "stereo_bin", "complex_lr_ratio_error": 3.33290391502436e-08, "hz": 3000}
{"check": "true_peak", "dc": 0, "excess_linear": 0, "hz": 6000, "input_dbtp": -1.4156527925060667, "output_dbtp": -1.154999500057135}
{"check": "true_peak", "dc": 0, "excess_linear": 0, "hz": 17000, "input_dbtp": -1.9384090832539362, "output_dbtp": -1.2517285889946792}
{"check": "true_peak", "dc": 0, "excess_linear": 0, "hz": 19000, "input_dbtp": -1.9384771591029333, "output_dbtp": -1.2515607252842664}
{"check": "true_peak", "dc": 0, "excess_linear": 0, "hz": 21000, "input_dbtp": -1.9684655780606175, "output_dbtp": -1.2216057337842732}
{"check": "true_peak", "dc": 0, "excess_linear": 0, "hz": 23000, "input_dbtp": -1.9384771591029346, "output_dbtp": -1.0219706631973735}
{"check": "true_peak", "dc": 0, "excess_linear": 0, "hz": 3000, "input_dbtp": -0.6303656851902566, "output_dbtp": -0.6303656851902566}
{"check": "true_peak", "dc": 0.95, "excess_linear": 0, "hz": 6000, "input_dbtp": -0.2645840068909675, "output_dbtp": -0.2645840068909675}
{"check": "silence", "nonzero_samples": 0}
SO_VERIFICATION_OK
EXIT=0
```

state:

```text
{"check": "file_created", "contract": "remix-state-v1", "instance": 1, "latency_frames": 3776, "model": "/home/minamorl/repos/.worktrees/audient-id14-linux/remix-runtime/crates/id14-remix-ladspa/fixtures/voice.onnx", "overloads": 0, "pid": 403, "state": 1, "state_name": "Active", "updated_unix_ms": 1791183068636}
{"check": "state_change_and_heartbeat", "heartbeat_ms": 1006, "state": 2, "state_name": "Off"}
{"check": "stopped_inference_visible", "contract": "remix-state-v1", "instance": 1, "latency_frames": 3776, "model": "/home/minamorl/repos/.worktrees/audient-id14-linux/remix-runtime/crates/id14-remix-ladspa/fixtures/voice.onnx", "overloads": 1, "pid": 403, "state": 6, "state_name": "Overloaded", "updated_unix_ms": 1791183069795}
{"error":{"code":"ModelMissing","details":"file is missing or unreadable","message":"Remix model unavailable","trace_id":"01a10ad4-a264-74c4-8f5e-f5e8e39d7dde"},"level":"warn","msg":"Remix model unavailable; continuing with neutral playback","trace_id":"01a10ad4-a264-74c4-8f5e-f5e8e39d7dde","ts":1791183069.7965932}
{"check": "owned_cleanup", "other_instance_preserved": true, "removed": true}
{"bit_mismatches": 0, "check": "publication_failure_audio", "enabled": 0, "mode": "writable"}
{"bit_mismatches": 0, "check": "publication_failure_audio", "enabled": 0, "mode": "unset"}
{"bit_mismatches": 0, "check": "publication_failure_audio", "enabled": 0, "mode": "not_directory"}
{"bit_mismatches": 0, "check": "publication_failure_audio", "enabled": 0, "mode": "permission_denied"}
{"bit_mismatches": 0, "check": "publication_failure_audio", "enabled": 1, "mode": "writable"}
{"bit_mismatches": 0, "check": "publication_failure_audio", "enabled": 1, "mode": "unset"}
{"bit_mismatches": 0, "check": "publication_failure_audio", "enabled": 1, "mode": "not_directory"}
{"bit_mismatches": 0, "check": "publication_failure_audio", "enabled": 1, "mode": "permission_denied"}
STATE_SO_VERIFICATION_OK
EXIT=0
```

realtime:

```text
{"model": "/home/minamorl/repos/.worktrees/audient-id14-linux/remix-runtime/crates/id14-remix-ladspa/.build/remix-2000.onnx", "bytes": 544901, "sha256": "d22d567981f3d895d36a99a74d934462f79f03555ef74ec94ded855d4a5dd760"}
{"blocks": 5625, "callback_wall_us_max": 1318.8820012146607, "callback_wall_us_p99": 915.965795866214, "check": "trained_model_realtime", "deadline_lateness_ms_max": 0.37268901360221207, "interval_ms_p50": 5.333316992619075, "interval_ms_p99": 5.374515042058192, "latency_frames": 3776, "overloaded_blocks": 0, "period_ms": 5.333333333333333, "seconds": 30.0, "states": {"1": 5625}, "warmup_blocks": 187, "warmup_states": {"1": 187}}
{"check": "plugin_worker_infer", "samples": 2911, "p50_ms": 0.807271, "p99_ms": 1.073143, "max_ms": 2.767243}
REALTIME_SO_VERIFICATION_OK
EXIT=0
```

diff:

```text
EXIT=0
```

release_artifact:

```text
80d17737b6db37f8642699598a2570b9e5bce24ee4b5d9bcbd7d8441f44071bd  /home/minamorl/repos/.worktrees/audient-id14-linux/remix-runtime/crates/id14-remix-ladspa/.build/target/release/libid14_remix_ladspa.so
```
