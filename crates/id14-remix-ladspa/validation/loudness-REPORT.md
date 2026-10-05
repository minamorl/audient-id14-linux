domain: audient-id14-realtime-sr.spec@0.13

pins_read:
- realtime_sr.remixing_switch_loudness_fairness / remixing_single_source_loudness_fairness — <=0.5 LU.
- realtime_sr.remixing_off_transparency / remixing_zero_amount_transparency / remixing_overload_fallback / remixing_switch_continuity — neutral audio and transitions.
- realtime_sr.remixing_bass_unchanged_band / remixing_stereo_position / remixing_true_peak_limit / remixing_silent_input_output / remixing_latency_amount_invariance — preserved playback boundaries.
- house_style@4.0 / prohibitions@1.0 — imported cross-cutting contract; the full domain and imports were reread before edits.

implemented:
- src/loudness.rs — linked K-weighted high-band energy predictor; 3 s exponential moments, 0.5 s following, 0.5 dB/s slew, +/-3 dB cap.
- src/dsp.rs / src/engine.rs / src/lib.rs — (c*g[k]-1)*X[k] correction, retained original dry path, existing bass guard, peak protection, fades and declared delay.
- src/ladspa.rs — diagnostic gain readout between run calls, with no new LADSPA port.
- src/verification.rs — actual callback-thread allocation counter and both-direction matching tests; the old permanent +3 dB single-source expectation is replaced by the pinned settled loudness tolerance. The original failed test output is preserved separately.
- tools/verify_loudness_so.py — real .so, real trained model, independently computed time-domain BS.1770 integrated/momentary/short-term measurements, SR 185 chain and synthetic fixtures.
- IMPLEMENTATION.md / RESEARCH.md — method, boundaries and fetched primary sources.

assumptions:
- The caller explicitly leaves the method/time constants free. The bounded controller uses a spectral K-weighting estimate and guard response; the acceptance meter uses independent time-domain filters and programme gating.
- Loudness measurements use default amounts (+3/0/0/-3), include startup, and cover the full 64.970667 s source files and 32 s synthetic signals. The slow bounded controller is not a guarantee of instant loudness matching for arbitrarily short material or every control combination.
- Synthetic voiced syllables use harmonics with formant/pitch/envelope variation; instruments use a plucked harmonic chord. They are synthetic proxies, not natural speech recordings.

undecidable: []
contradictions: []
verdict: IMPLEMENTED

checks:

Integrated loudness: ON minus OFF, LU. Both plugin states were actually rendered; OFF was also checked bitwise against input after delay compensation.

| material | before remix+SR185 | after remix | after remix+SR185 |
|---|---:|---:|---:|
| dry | +0.222257 | +0.111289 | +0.102643 |
| dry2 | +0.670138 | +0.021286 | +0.021323 |
| speech_synthetic | -2.646768 | -0.312810 | -0.312808 |
| instrument_synthetic | -0.494600 | -0.046270 | -0.046268 |
| stationary_pumping | +2.936311 | +0.312077 | +0.312099 |

Pumping probe, independent follow-up run. Settled interval starts 15 s into the 32 s stationary source. Loudness differences are measured on actual output, including the final signal boundary.

```json
{"chain": "remix", "delta_lu": 0.3120820271214768, "gain_max_db": 0.0, "gain_max_slew_db_s": 0.5000084638595581, "gain_min_db": -2.922699213027954, "input_difference_rms_db": -20.455780029296875, "material": "stationary_pumping", "off_lufs": -22.433880899841885, "on_lufs": -22.121798872720408, "seconds": 32.0, "settled_gain_peak_to_peak_db": 0.00029277801513671875, "settled_momentary_delta_peak_to_peak_lu": 0.05082049772353159, "settled_short_delta_peak_to_peak_lu": 0.006721300772536409, "short_delta_max_100ms_step_lu": 0.05032523833092739, "short_delta_p05_lu": -0.0007275318925437091, "short_delta_p95_lu": 1.4684953992049117, "source_sha256": "synthetic"}
{"chain": "remix_sr185", "delta_lu": 0.3121040562112931, "gain_max_db": 0.0, "gain_max_slew_db_s": 0.5000084638595581, "gain_min_db": -2.922699213027954, "input_difference_rms_db": -20.4537353515625, "material": "stationary_pumping", "off_lufs": -22.433880160194473, "on_lufs": -22.12177610398318, "seconds": 32.0, "settled_gain_peak_to_peak_db": 0.00029277801513671875, "settled_momentary_delta_peak_to_peak_lu": 0.05081742460557592, "settled_short_delta_peak_to_peak_lu": 0.006721130441675882, "short_delta_max_100ms_step_lu": 0.050324894279142995, "short_delta_p05_lu": -0.0007057037354147866, "short_delta_p95_lu": 1.4685180706059047, "source_sha256": "synthetic"}
```

Raw commands (assigned worktree cwd):

```sh
export ID14_ORT_LIBRARY=/nix/store/3k9p1crsk8faxgir3xb5jimz9wwsm0ih-onnxruntime-1.27.1/lib/libonnxruntime.so
export XDG_RUNTIME_DIR="/home/minamorl/repos/.worktrees/audient-id14-linux/remix-runtime/crates/id14-remix-ladspa/.build/loudness-runtime"
export TMPDIR="/home/minamorl/repos/.worktrees/audient-id14-linux/remix-runtime/crates/id14-remix-ladspa/.build/tmp"
bash crates/id14-remix-ladspa/tools/with_cargo.sh test --workspace -- --nocapture
bash crates/id14-remix-ladspa/tools/with_cargo.sh build --release -p id14-sr-ladspa
bash crates/id14-remix-ladspa/tools/with_python.sh crates/id14-remix-ladspa/tools/verify_loudness_so.py --so crates/id14-remix-ladspa/.build/loudness-before.so --sr-so crates/id14-remix-ladspa/.build/target/release/libid14_sr_ladspa.so --model /tmp/id14-remix-models/remix-20000.onnx --output crates/id14-remix-ladspa/validation/loudness-before.json --baseline
bash crates/id14-remix-ladspa/tools/with_python.sh crates/id14-remix-ladspa/tools/verify_loudness_so.py --so crates/id14-remix-ladspa/.build/target/release/libid14_remix_ladspa.so --sr-so crates/id14-remix-ladspa/.build/target/release/libid14_sr_ladspa.so --model /tmp/id14-remix-models/remix-20000.onnx --output crates/id14-remix-ladspa/validation/loudness-after.json 
bash crates/id14-remix-ladspa/tools/with_python.sh crates/id14-remix-ladspa/tools/verify_loudness_so.py --so crates/id14-remix-ladspa/.build/target/release/libid14_remix_ladspa.so --sr-so crates/id14-remix-ladspa/.build/target/release/libid14_sr_ladspa.so --model /tmp/id14-remix-models/remix-20000.onnx --output crates/id14-remix-ladspa/validation/loudness-pumping.json --synthetic-only --case stationary_pumping
bash crates/id14-remix-ladspa/tools/with_python.sh crates/id14-remix-ladspa/tools/verify_so.py --so crates/id14-remix-ladspa/.build/target/release/libid14_remix_ladspa.so --output crates/id14-remix-ladspa/validation/loudness-so-initial-results.json
bash /home/minamorl/repos/.worktrees/audient-id14-linux/remix-runtime/crates/id14-remix-ladspa/tools/with_cargo.sh build --release -p id14-remix-ladspa 
bash /home/minamorl/repos/.worktrees/audient-id14-linux/remix-runtime/crates/id14-remix-ladspa/tools/with_python.sh /home/minamorl/repos/.worktrees/audient-id14-linux/remix-runtime/crates/id14-remix-ladspa/tools/verify_state_so.py --so /home/minamorl/repos/.worktrees/audient-id14-linux/remix-runtime/crates/id14-remix-ladspa/.build/target/release/libid14_remix_ladspa.so 
bash /home/minamorl/repos/.worktrees/audient-id14-linux/remix-runtime/crates/id14-remix-ladspa/tools/with_python.sh /home/minamorl/repos/.worktrees/audient-id14-linux/remix-runtime/crates/id14-remix-ladspa/tools/verify_library_discovery.py --so /home/minamorl/repos/.worktrees/audient-id14-linux/remix-runtime/crates/id14-remix-ladspa/.build/target/release/libid14_remix_ladspa.so --library /nix/store/3k9p1crsk8faxgir3xb5jimz9wwsm0ih-onnxruntime-1.27.1/lib/libonnxruntime.so --build-library /nix/store/3k9p1crsk8faxgir3xb5jimz9wwsm0ih-onnxruntime-1.27.1/lib/libonnxruntime.so 
bash /home/minamorl/repos/.worktrees/audient-id14-linux/remix-runtime/crates/id14-remix-ladspa/tools/with_cargo.sh run --release -p id14-remix-ladspa --example bench_worker -- /tmp/id14-remix-models/remix-20000.onnx 500 
bash /home/minamorl/repos/.worktrees/audient-id14-linux/remix-runtime/crates/id14-remix-ladspa/tools/with_python.sh /home/minamorl/repos/.worktrees/audient-id14-linux/remix-runtime/crates/id14-remix-ladspa/tools/verify_realtime_so.py --so /home/minamorl/repos/.worktrees/audient-id14-linux/remix-runtime/crates/id14-remix-ladspa/.build/target/release/libid14_remix_ladspa.so --model /tmp/id14-remix-models/remix-20000.onnx --seconds 30 
git diff --check 
```

before:

```text
{"meter_reference_997_hz_lkfs": -3.0102645220112825}
/home/minamorl/repos/.worktrees/audient-id14-linux/remix-runtime/crates/id14-remix-ladspa/tools/verify_loudness_so.py:140: WavFileWarning: Chunk (non-data) not understood, skipping it.
  rate, x = wavfile.read(path)
{"chain": "remix", "delta_lu": 0.2309046358177156, "input_difference_rms_db": -27.007095336914062, "material": "dry", "off_lufs": -40.01309728140575, "on_lufs": -39.78219264558803, "seconds": 64.97066666666667, "short_delta_max_100ms_step_lu": 1.8982306058231302, "short_delta_p05_lu": -2.3217591554366503, "short_delta_p95_lu": 0.16870970194057655, "source_sha256": "1e8d417a388411986e9cb354b132ffabe77abce8ef764eb0af02b21d99b8b5fd"}
{"chain": "remix_sr185", "delta_lu": 0.2222574925722185, "input_difference_rms_db": -27.007068634033203, "material": "dry", "off_lufs": -40.00443436841068, "on_lufs": -39.78217687583846, "seconds": 64.97066666666667, "short_delta_max_100ms_step_lu": 1.8982377810156734, "short_delta_p05_lu": -2.3217591570367233, "short_delta_p95_lu": 0.16871675206950876, "source_sha256": "1e8d417a388411986e9cb354b132ffabe77abce8ef764eb0af02b21d99b8b5fd"}
{"chain": "remix", "delta_lu": 0.6700732513606269, "input_difference_rms_db": -18.479204177856445, "material": "dry2", "off_lufs": -37.07713541382188, "on_lufs": -36.40706216246125, "seconds": 64.97066666666667, "short_delta_max_100ms_step_lu": 0.07983534498801959, "short_delta_p05_lu": 0.01845676961015564, "short_delta_p95_lu": 1.0617630844459836, "source_sha256": "4741ee3aa4f8b19f329b5a4fa1567b75741c469cdd9d3b505379c314b0d6267b"}
{"chain": "remix_sr185", "delta_lu": 0.6701383612252059, "input_difference_rms_db": -18.478879928588867, "material": "dry2", "off_lufs": -37.076856249357164, "on_lufs": -36.40671788813196, "seconds": 64.97066666666667, "short_delta_max_100ms_step_lu": 0.07983143558880101, "short_delta_p05_lu": 0.018639364303355787, "short_delta_p95_lu": 1.061775077388763, "source_sha256": "4741ee3aa4f8b19f329b5a4fa1567b75741c469cdd9d3b505379c314b0d6267b"}
{"chain": "remix", "delta_lu": -2.6467697793842255, "input_difference_rms_db": -11.287023544311523, "material": "speech_synthetic", "off_lufs": -29.89509528234134, "on_lufs": -32.54186506172557, "seconds": 32.0, "short_delta_max_100ms_step_lu": 0.23142989906621025, "short_delta_p05_lu": -2.8416060909383773, "short_delta_p95_lu": -2.3294266240941486, "source_sha256": "synthetic"}
{"chain": "remix_sr185", "delta_lu": -2.6467677423523988, "input_difference_rms_db": -11.28701400756836, "material": "speech_synthetic", "off_lufs": -29.895095165325504, "on_lufs": -32.5418629076779, "seconds": 32.0, "short_delta_max_100ms_step_lu": 0.23143043829766086, "short_delta_p05_lu": -2.841604899926862, "short_delta_p95_lu": -2.329424597252178, "source_sha256": "synthetic"}
{"chain": "remix", "delta_lu": -0.4946039296682052, "input_difference_rms_db": -18.512165069580078, "material": "instrument_synthetic", "off_lufs": -31.515176056099097, "on_lufs": -32.0097799857673, "seconds": 32.0, "short_delta_max_100ms_step_lu": 0.009678055664824825, "short_delta_p05_lu": -0.5469403215218129, "short_delta_p95_lu": -0.4424323897004854, "source_sha256": "synthetic"}
{"chain": "remix_sr185", "delta_lu": -0.4945995211846963, "input_difference_rms_db": -18.512046813964844, "material": "instrument_synthetic", "off_lufs": -31.515175852302228, "on_lufs": -32.009775373486924, "seconds": 32.0, "short_delta_max_100ms_step_lu": 0.009678301168907666, "short_delta_p05_lu": -0.5469359386711226, "short_delta_p95_lu": -0.4424277879122407, "source_sha256": "synthetic"}
{"chain": "remix", "delta_lu": 2.9362880301715535, "input_difference_rms_db": -7.912520885467529, "material": "stationary_pumping", "off_lufs": -22.433880899841885, "on_lufs": -19.49759286967033, "seconds": 32.0, "short_delta_max_100ms_step_lu": 0.0067127408006300016, "short_delta_p05_lu": 2.936421271452023, "short_delta_p95_lu": 2.936540388257903, "source_sha256": "synthetic"}
{"chain": "remix_sr185", "delta_lu": 2.9363114830093195, "input_difference_rms_db": -7.912282943725586, "material": "stationary_pumping", "off_lufs": -22.433880160194473, "on_lufs": -19.497568677185154, "seconds": 32.0, "short_delta_max_100ms_step_lu": 0.00671306597419985, "short_delta_p05_lu": 2.9364446573780096, "short_delta_p95_lu": 2.9365638189793124, "source_sha256": "synthetic"}
LOUDNESS_SO_MEASUREMENT_OK
EXIT=0
```

after:

```text
{"plugin_sha256": "1db87bfe5c0fefbd500266515522da26b4c1102af7184d7313ebb515e77043fa", "model_sha256": "b299f8ff621997c4412214755e1cd33c80d6fc889631fc6ec13a8e6a6a988e4d"}
{"meter_reference_997_hz_lkfs": -3.0102645220112825}
/home/minamorl/repos/.worktrees/audient-id14-linux/remix-runtime/crates/id14-remix-ladspa/tools/verify_loudness_so.py:142: WavFileWarning: Chunk (non-data) not understood, skipping it.
  rate, x = wavfile.read(path)
{"chain": "remix", "delta_lu": 0.11128940210278415, "gain_max_db": 2.065605640411377, "gain_max_slew_db_s": 0.5000084638595581, "gain_min_db": -0.32634252309799194, "input_difference_rms_db": -28.24248504638672, "material": "dry", "off_lufs": -40.01309728140575, "on_lufs": -39.901807879302964, "seconds": 64.97066666666667, "short_delta_max_100ms_step_lu": 1.368760958378239, "short_delta_p05_lu": -0.9214027115661931, "short_delta_p95_lu": 0.9009710909451591, "source_sha256": "1e8d417a388411986e9cb354b132ffabe77abce8ef764eb0af02b21d99b8b5fd"}
{"chain": "remix_sr185", "delta_lu": 0.10264327804632956, "gain_max_db": 2.065605640411377, "gain_max_slew_db_s": 0.5000084638595581, "gain_min_db": -0.32634252309799194, "input_difference_rms_db": -28.242416381835938, "material": "dry", "off_lufs": -40.00443436841068, "on_lufs": -39.90179109036435, "seconds": 64.97066666666667, "short_delta_max_100ms_step_lu": 1.3687689892023585, "short_delta_p05_lu": -0.9214027116458983, "short_delta_p95_lu": 0.9009869967509445, "source_sha256": "1e8d417a388411986e9cb354b132ffabe77abce8ef764eb0af02b21d99b8b5fd"}
{"chain": "remix", "delta_lu": 0.021286137502237068, "gain_max_db": 0.0, "gain_max_slew_db_s": 0.5000084638595581, "gain_min_db": -1.6086293458938599, "input_difference_rms_db": -23.857799530029297, "material": "dry2", "off_lufs": -37.07713541382188, "on_lufs": -37.05584927631964, "seconds": 64.97066666666667, "short_delta_max_100ms_step_lu": 0.08350887722576061, "short_delta_p05_lu": -0.31902633386315277, "short_delta_p95_lu": 0.3491531492285976, "source_sha256": "4741ee3aa4f8b19f329b5a4fa1567b75741c469cdd9d3b505379c314b0d6267b"}
{"chain": "remix_sr185", "delta_lu": 0.021322814880271324, "gain_max_db": 0.0, "gain_max_slew_db_s": 0.5000084638595581, "gain_min_db": -1.6086293458938599, "input_difference_rms_db": -23.857423782348633, "material": "dry2", "off_lufs": -37.076856249357164, "on_lufs": -37.05553343447689, "seconds": 64.97066666666667, "short_delta_max_100ms_step_lu": 0.08350956681842092, "short_delta_p05_lu": -0.31893798059655576, "short_delta_p95_lu": 0.34915673933404323, "source_sha256": "4741ee3aa4f8b19f329b5a4fa1567b75741c469cdd9d3b505379c314b0d6267b"}
{"chain": "remix", "delta_lu": -0.31281015415331126, "gain_max_db": 2.840770959854126, "gain_max_slew_db_s": 0.5000084638595581, "gain_min_db": 0.0, "input_difference_rms_db": -19.12436294555664, "material": "speech_synthetic", "off_lufs": -29.89509528234134, "on_lufs": -30.207905436494652, "seconds": 32.0, "short_delta_max_100ms_step_lu": 0.2718367297164441, "short_delta_p05_lu": -1.9181544778431174, "short_delta_p95_lu": 0.4001551427916592, "source_sha256": "synthetic"}
{"chain": "remix_sr185", "delta_lu": -0.31280814190528616, "gain_max_db": 2.840770959854126, "gain_max_slew_db_s": 0.5000084638595581, "gain_min_db": 0.0, "input_difference_rms_db": -19.124298095703125, "material": "speech_synthetic", "off_lufs": -29.895095165325504, "on_lufs": -30.20790330723079, "seconds": 32.0, "short_delta_max_100ms_step_lu": 0.2718369369400442, "short_delta_p05_lu": -1.9181523605483086, "short_delta_p95_lu": 0.40015732351997, "source_sha256": "synthetic"}
{"chain": "remix", "delta_lu": -0.04627006404544076, "gain_max_db": 2.7105889320373535, "gain_max_slew_db_s": 0.5000084638595581, "gain_min_db": 0.0, "input_difference_rms_db": -29.484352111816406, "material": "instrument_synthetic", "off_lufs": -31.515176056099097, "on_lufs": -31.561446120144538, "seconds": 32.0, "short_delta_max_100ms_step_lu": 0.024444791082064654, "short_delta_p05_lu": -0.2575095378365564, "short_delta_p95_lu": 0.0005577386072381785, "source_sha256": "synthetic"}
{"chain": "remix_sr185", "delta_lu": -0.04626789583447177, "gain_max_db": 2.7105889320373535, "gain_max_slew_db_s": 0.5000084638595581, "gain_min_db": 0.0, "input_difference_rms_db": -29.483539581298828, "material": "instrument_synthetic", "off_lufs": -31.515175852302228, "on_lufs": -31.5614437481367, "seconds": 32.0, "short_delta_max_100ms_step_lu": 0.024444763868725516, "short_delta_p05_lu": -0.2575052955241688, "short_delta_p95_lu": 0.0005594334202498885, "source_sha256": "synthetic"}
{"chain": "remix", "delta_lu": 0.3120772534050893, "gain_max_db": 0.0, "gain_max_slew_db_s": 0.5000084638595581, "gain_min_db": -2.922699213027954, "input_difference_rms_db": -20.45699119567871, "material": "stationary_pumping", "off_lufs": -22.433880899841885, "on_lufs": -22.121803646436796, "seconds": 32.0, "settled_gain_peak_to_peak_db": 0.00029277801513671875, "short_delta_max_100ms_step_lu": 0.05032523814321266, "short_delta_p05_lu": -0.0007270194413795394, "short_delta_p95_lu": 1.4684954016611513, "source_sha256": "synthetic"}
{"chain": "remix_sr185", "delta_lu": 0.31209927757774736, "gain_max_db": 0.0, "gain_max_slew_db_s": 0.5000084638595581, "gain_min_db": -2.922699213027954, "input_difference_rms_db": -20.45494842529297, "material": "stationary_pumping", "off_lufs": -22.433880160194473, "on_lufs": -22.121780882616726, "seconds": 32.0, "settled_gain_peak_to_peak_db": 0.00029277801513671875, "short_delta_max_100ms_step_lu": 0.05032488372305366, "short_delta_p05_lu": -0.0007051785330460802, "short_delta_p95_lu": 1.468518033173087, "source_sha256": "synthetic"}
LOUDNESS_SO_VERIFICATION_OK
EXIT=0
```

pumping:

```text
{"plugin_sha256": "1db87bfe5c0fefbd500266515522da26b4c1102af7184d7313ebb515e77043fa", "model_sha256": "b299f8ff621997c4412214755e1cd33c80d6fc889631fc6ec13a8e6a6a988e4d"}
{"meter_reference_997_hz_lkfs": -3.0102645220112825}
{"chain": "remix", "delta_lu": 0.3120820271214768, "gain_max_db": 0.0, "gain_max_slew_db_s": 0.5000084638595581, "gain_min_db": -2.922699213027954, "input_difference_rms_db": -20.455780029296875, "material": "stationary_pumping", "off_lufs": -22.433880899841885, "on_lufs": -22.121798872720408, "seconds": 32.0, "settled_gain_peak_to_peak_db": 0.00029277801513671875, "settled_momentary_delta_peak_to_peak_lu": 0.05082049772353159, "settled_short_delta_peak_to_peak_lu": 0.006721300772536409, "short_delta_max_100ms_step_lu": 0.05032523833092739, "short_delta_p05_lu": -0.0007275318925437091, "short_delta_p95_lu": 1.4684953992049117, "source_sha256": "synthetic"}
{"chain": "remix_sr185", "delta_lu": 0.3121040562112931, "gain_max_db": 0.0, "gain_max_slew_db_s": 0.5000084638595581, "gain_min_db": -2.922699213027954, "input_difference_rms_db": -20.4537353515625, "material": "stationary_pumping", "off_lufs": -22.433880160194473, "on_lufs": -22.12177610398318, "seconds": 32.0, "settled_gain_peak_to_peak_db": 0.00029277801513671875, "settled_momentary_delta_peak_to_peak_lu": 0.05081742460557592, "settled_short_delta_peak_to_peak_lu": 0.006721130441675882, "short_delta_max_100ms_step_lu": 0.050324894279142995, "short_delta_p05_lu": -0.0007057037354147866, "short_delta_p95_lu": 1.4685180706059047, "source_sha256": "synthetic"}
LOUDNESS_SO_VERIFICATION_OK
EXIT=0
```

workspace:

```text
   Compiling id14-remix-ladspa v0.1.0 (/home/minamorl/repos/.worktrees/audient-id14-linux/remix-runtime/crates/id14-remix-ladspa)
    Finished `test` profile [unoptimized] target(s) in 2.92s
     Running unittests src/lib.rs (crates/id14-remix-ladspa/.build/target/debug/deps/id14_protocol-194b8cb0c917a2b5)

running 40 tests
test control_error::tests::dfu_missing_message_states_no_interface_0_fallback ... ok
test db::tests::formatting ... ok
test db::tests::whole_and_fractional_decibels ... ok
test db::tests::unrepresentable_decibels_are_rejected ... ok
test descriptor::tests::ambiguous_dfu_interfaces_are_refused ... ok
test descriptor::tests::dfu_interface_numbered_0_to_2_is_refused ... ok
test descriptor::tests::dfu_interface_is_found_from_descriptor ... ok
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
test plan::tests::mixer_nodes_use_uac2_mixer_control_numbers ... ok
test plan::tests::dump_plan_is_get_only_and_never_addresses_extension_units ... ok
test plan::tests::values_render_by_kind ... ok
test plan::tests::mk2_declares_no_mute_control ... ok
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
test evidence_status_pins ... ok
test extension_code_pins ... ok
test product_table_pins ... ok
test request_header_pins ... ok

test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running unittests src/lib.rs (crates/id14-remix-ladspa/.build/target/debug/deps/id14_remix_ladspa-9aa0eaed2d8d05a9)

running 18 tests
onnx_rejected=bad_contract
test state::tests::model_failure_visible_without_audio_callback ... ok
fft_max_error=7.771561172376096e-16 sqrt_hann_cola_error=4.440892098500626e-16
onnx_rejected=bad_q
onnx_rejected=unsupported_q
onnx_rejected=bad_shape
bass_guard_0_300_hz_worst_db=-74.90667323991724
test dsp::tests::fft_roundtrip_and_weighted_overlap ... ok
test dsp::tests::bass_guard_stopband ... ok
test queue::tests::concurrent_order_and_backpressure ... ok
onnx_rejected=bad_sum
onnx_valid=voice q=0 state_size=3
onnx_rejected=negative
onnx_valid=uniform q=0 state_size=3
onnx_rejected=nan_mask
onnx_valid=voice_bins q=0 state_size=3
onnx_rejected=nan_state
test model::tests::rejects_contract_and_runtime_value_errors ... ok
onnx_valid=q3 q=3 state_size=3
onnx_valid=stateful_reordered q=0 state_size=3
onnx_valid=zero_state q=0 state_size=0
test model::tests::accepts_valid_models_and_maps_reordered_names ... ok
dry mode=missing bit_mismatches=0 reported=3776 cross_correlation_lag=3776 sr_inclusive_ms=100.000
test state::tests::unavailable_runtime_preserves_audio_bits ... ok
loudness part=0 settled_db=-3 max_hop_step_db=0.0053334236
loudness part=3 settled_db=3 max_hop_step_db=0.0053334236
test loudness::tests::both_gain_directions_converge_without_fast_modulation ... ok
dry mode=off bit_mismatches=0 reported=3776 cross_correlation_lag=3776 sr_inclusive_ms=100.000
dry mode=zero bit_mismatches=0 reported=3776 cross_correlation_lag=3776 sr_inclusive_ms=100.000
test verification::exact_dry_paths_and_measured_delay ... ok
hot_dc_protection_bit_mismatches=0 state=PeakProtected
test verification::peak_protection_keeps_hot_dc_input_unchanged ... ok
silent_output_nonzero_samples=0
test verification::silence_adds_no_signal ... ok
test state::tests::publication_updates_heartbeat_and_owned_cleanup ... ok
transition event=worker_pause max_envelope_sample_step=7.056823446954796e-5 min_amplitude=0.099999999 settle_ms=59.312 final_bit_mismatches=0 state=Overloaded
lookahead_q0_vs_q3_max_sample_error=0e0
test verification::lookahead_masks_align_with_their_original_spectra ... ok
bass hz=50 difference_relative_db=-149.920814 error_rms_dbfs=-100.218311
callback_allocations=0 loudness_gain_db=-1.0453334
test verification::loudness_matching_callback_allocates_nothing ... ok
true_peak high_hz=6000 input_dbtp=-1.415640 output_dbtp=-1.154986 excess_linear=0.000000000
peak_protected_bass_150_hz_difference_relative_db=-86.147896
transition event=off max_envelope_sample_step=7.14424937841851e-5 min_amplitude=0.099999999 settle_ms=9.979 final_bit_mismatches=0 state=Off
bass hz=100 difference_relative_db=-133.735477 error_rms_dbfs=-97.219363
true_peak high_hz=17000 input_dbtp=-1.938164 output_dbtp=-1.251493 excess_linear=0.000000000
transition event=zero max_envelope_sample_step=7.14424937841851e-5 min_amplitude=0.099999999 settle_ms=9.979 final_bit_mismatches=0 state=ZeroAmounts
test verification::stalled_worker_and_switches_fade_current_audio_to_dry ... ok
bass hz=200 difference_relative_db=-125.930585 error_rms_dbfs=-89.920567
true_peak high_hz=3000 input_dbtp=-0.630341 output_dbtp=-0.630341 excess_linear=0.000000000
test verification::independent_true_peak_oracle ... ok
bass hz=299 difference_relative_db=-94.807000 error_rms_dbfs=-75.958520
test verification::low_band_retention ... ok
active gain_3000_hz=1.000082798 target=1 loudness_delta_db=0.000719 stereo_ratio_error=0e0 callback_256_us_p50=589.610 p99=14012.801 max=16319.672 final_status=Active
test verification::active_gain_stereo_and_callback_cost ... ok

test result: ok. 18 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 8.73s

     Running unittests src/lib.rs (crates/id14-remix-ladspa/.build/target/debug/deps/id14_sr-3473fdbe9005ff47)

running 11 tests
test tests::embedded_output_matches_training_runtime ... ok
test tests::on_off_and_fullband_bypass_are_callable ... ok
test tests::completion_keeps_stereo_channels_independent ... ok
transient_peak=0.999999881 loud_samples=96 flat_triples=0 gain_reduction_seen=true
test tests::boosted_high_level_transient_is_limited_without_flat_topping ... ok
test tests::bypass_preserves_stereo_with_fixed_delay ... ok
test tests::peak_guard_covers_the_overlap_tail_after_leaving_boosted_mix ... ok
test tests::trained_path_changes_bandlimited_audio ... ok
test source_safety_regression_tests::fullband_after_long_zero_mix_preserves_aligned_source_at_transition ... ok
test tests::enabled_path_preserves_existing_low_tone ... ok
test tests::continuous_mix_endpoints_match_existing_api ... ok
test source_safety_regression_tests::bandlimited_stereo_completion_is_swap_symmetric_and_mono_stays_mono ... ok

test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.29s

     Running unittests src/bin/bench.rs (crates/id14-remix-ladspa/.build/target/debug/deps/sr_bench-9f9dcfbb0c035f4e)

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running unittests src/bin/render.rs (crates/id14-remix-ladspa/.build/target/debug/deps/sr_render-fc32c600d0913887)

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running unittests src/lib.rs (crates/id14-remix-ladspa/.build/target/debug/deps/id14_sr_ladspa-146f2779152e3e38)

running 18 tests

thread 'denormal_scope::tests::restores_callers_environment_on_unwind' (389) panicked at crates/id14-sr-ladspa/src/denormal_scope.rs:96:13:
exercise unwind
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
test denormal_scope::tests::restores_callers_environment_after_nested_scopes ... ok
test denormal_scope::tests::restores_callers_environment_on_unwind ... ok
test tests::descriptor_has_one_stereo_plugin ... ok
test bass_guard::tests::high_frequency_residual_survives_and_nonfinite_input_recovers ... ok
test tests::unsupported_sample_rate_is_safe_passthrough ... ok
adapter delay_frames=1024 delay_ms=21.333333 added_guard_frames=0
test bass_guard::tests::adapter_dry_impulse_retains_1024_frame_delay ... ok
test tests::adapter_and_engine_delay_is_exactly_1024_frames ... ok
test tests::zero_mix_matches_the_engine_bypass_with_adapter_delay ... ok
synthetic mix=0 bass_error=0.000000000->0.000000000 width_error=0.000000000->0.000000000
test tests::detector_preserves_fullband_material_and_releases_quickly ... ok
test tests::detector_enables_completion_for_12k_bandlimited_material ... ok
test tests::owner_mix_is_bounded_and_ramped_then_reaches_exact_bypass ... ok
test tests::above_hundred_never_emits_a_sample_beyond_full_scale ... ok
synthetic mix=100 bass_error=0.271200524->0.017038986 width_error=0.176605131->0.007971116
synthetic mix=150 bass_error=0.406800784->0.025558480 width_error=0.258806967->0.011996126
synthetic mix=200 bass_error=0.542401046->0.034077972 width_error=0.333485014->0.016045106
test bass_guard::tests::bass_criterion_known_contamination ... ok
test tests::arbitrary_host_blocks_match_fixed_host_blocks ... ok
bandlimited diff_20=0.020512 diff_100=0.102593 diff_200_vs_100=0.102587
test tests::manual_mix_remains_effective_on_bandlimited_input ... ok
adapter mix=0 bass_error=0.000000069->0.000000069 width_error=0.000000001->0.000000001
fullband_ratio=0.010000 diff_20=0.000014 diff_100=0.000024 diff_200_vs_100=0.000001
test tests::manual_mix_protects_recorded_fullband_highs ... ok
test bass_guard::tests::adapter_block_partition_does_not_change_output ... ok
adapter mix=100 bass_error=0.000000070->0.000000005 width_error=0.000000001->0.000000003
adapter mix=150 bass_error=0.000000067->0.000000005 width_error=0.000000003->0.000000002
adapter mix=200 bass_error=0.000000069->0.000000006 width_error=0.000000001->0.000000003
test bass_guard::tests::bass_criterion_adapter_all_mixes ... ok

test result: ok. 18 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.52s

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
test ops::tests::mute_fails_stating_no_declared_mute_control ... ok
test ops::tests::volume_in_range_sets_then_reads_back ... ok
test ops::tests::volume_out_of_range_is_rejected_without_set ... ok
test usb::tests::autodetect_prefers_mk2 ... ok
test ops::tests::dump_reports_failed_reads_and_continues ... ok
test usb::tests::differing_configuration_values_are_refused ... ok
test ops::tests::dump_sends_only_get_requests_and_never_touches_extension_units ... ok
test usb::tests::differing_interface_content_is_refused ... ok
test usb::tests::identical_candidates_are_selected ... ok
test usb::tests::no_readable_candidate_is_not_guessed_around ... ok
test usb::tests::only_not_found_allows_the_indexed_fallback ... ok
test ops::tests::dry_run_dump_lists_requests ... ok

test result: ok. 15 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running unittests src/main.rs (crates/id14-remix-ladspa/.build/target/debug/deps/id14ctl-3cb6c66015127a74)

running 5 tests
test tests::dry_run_format_is_exact_lowercase_hex ... ok
test tests::request_bytes_are_deterministic_for_all_pinned_headers ... ok
test tests::protocol_lookup_identifies_mk1_and_mk2_vid_pid_pairs ... ok
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
    Finished `release` profile [optimized] target(s) in 0.05s
EXIT=0
```

so-initial:

```text
{"error":{"code":"ModelMissing","details":"file is missing or unreadable","message":"Remix model unavailable","trace_id":"01a10af5-dd0c-7679-9205-96dd43f3eff1"},"level":"warn","msg":"Remix model unavailable; continuing with neutral playback","trace_id":"01a10af5-dd0c-7679-9205-96dd43f3eff1","ts":1791185247.5003946}
{"bit_mismatches": 0, "check": "transparency_and_delay", "measured_frames": 3776, "mode": "missing", "reported_frames": 3776, "sr_inclusive_ms": 100.0, "state": 4}
{"bit_mismatches": 0, "check": "transparency_and_delay", "measured_frames": 3776, "mode": "off", "reported_frames": 3776, "sr_inclusive_ms": 100.0, "state": 2}
{"bit_mismatches": 0, "check": "transparency_and_delay", "measured_frames": 3776, "mode": "zero", "reported_frames": 3776, "sr_inclusive_ms": 100.0, "state": 3}
{"bit_mismatches": 0, "check": "transparency_and_delay", "measured_frames": 3776, "mode": "inplace", "reported_frames": 3776, "sr_inclusive_ms": 100.0, "state": 2}
{"error":{"code":"ModelInvalid","details":"remix-v1 signature, metadata, operators or outputs are invalid","message":"Remix model unavailable","trace_id":"01a10af5-e14d-7265-9cf6-3ae333de9f11"},"level":"warn","msg":"Remix model unavailable; continuing with neutral playback","trace_id":"01a10af5-e14d-7265-9cf6-3ae333de9f11","ts":1791185248.589473}
{"bit_mismatches": 0, "check": "invalid_model_neutral", "fixture": "bad_contract", "state": 5}
{"error":{"code":"ModelInvalid","details":"remix-v1 signature, metadata, operators or outputs are invalid","message":"Remix model unavailable","trace_id":"01a10af5-e17a-757c-92c2-7751d12b1d69"},"level":"warn","msg":"Remix model unavailable; continuing with neutral playback","trace_id":"01a10af5-e17a-757c-92c2-7751d12b1d69","ts":1791185248.6344488}
{"bit_mismatches": 0, "check": "invalid_model_neutral", "fixture": "bad_q", "state": 5}
{"error":{"code":"ModelInvalid","details":"remix-v1 signature, metadata, operators or outputs are invalid","message":"Remix model unavailable","trace_id":"01a10af5-e1a7-7477-92c0-50e2f844a3fe"},"level":"warn","msg":"Remix model unavailable; continuing with neutral playback","trace_id":"01a10af5-e1a7-7477-92c0-50e2f844a3fe","ts":1791185248.6793966}
{"bit_mismatches": 0, "check": "invalid_model_neutral", "fixture": "unsupported_q", "state": 5}
{"error":{"code":"ModelInvalid","details":"remix-v1 signature, metadata, operators or outputs are invalid","message":"Remix model unavailable","trace_id":"01a10af5-e1d4-724b-89e3-1864f1a19a29"},"level":"warn","msg":"Remix model unavailable; continuing with neutral playback","trace_id":"01a10af5-e1d4-724b-89e3-1864f1a19a29","ts":1791185248.7243118}
{"bit_mismatches": 0, "check": "invalid_model_neutral", "fixture": "bad_shape", "state": 5}
{"error":{"code":"ModelInvalid","details":"remix-v1 signature, metadata, operators or outputs are invalid","message":"Remix model unavailable","trace_id":"01a10af5-e201-70e4-9c4b-fdca56fea8db"},"level":"warn","msg":"Remix model unavailable; continuing with neutral playback","trace_id":"01a10af5-e201-70e4-9c4b-fdca56fea8db","ts":1791185248.7699223}
{"bit_mismatches": 0, "check": "invalid_model_neutral", "fixture": "bad_sum", "state": 5}
{"error":{"code":"ModelInvalid","details":"remix-v1 signature, metadata, operators or outputs are invalid","message":"Remix model unavailable","trace_id":"01a10af5-e22e-7419-8db5-e7efdd6abe6e"},"level":"warn","msg":"Remix model unavailable; continuing with neutral playback","trace_id":"01a10af5-e22e-7419-8db5-e7efdd6abe6e","ts":1791185248.8146658}
{"bit_mismatches": 0, "check": "invalid_model_neutral", "fixture": "negative", "state": 5}
{"error":{"code":"ModelInvalid","details":"remix-v1 signature, metadata, operators or outputs are invalid","message":"Remix model unavailable","trace_id":"01a10af5-e25b-773a-8b50-c0a468ed13d9"},"level":"warn","msg":"Remix model unavailable; continuing with neutral playback","trace_id":"01a10af5-e25b-773a-8b50-c0a468ed13d9","ts":1791185248.8596158}
{"bit_mismatches": 0, "check": "invalid_model_neutral", "fixture": "nan_mask", "state": 5}
{"error":{"code":"ModelInvalid","details":"remix-v1 signature, metadata, operators or outputs are invalid","message":"Remix model unavailable","trace_id":"01a10af5-e288-743d-a2ee-9c6ed53e5efc"},"level":"warn","msg":"Remix model unavailable; continuing with neutral playback","trace_id":"01a10af5-e288-743d-a2ee-9c6ed53e5efc","ts":1791185248.9047742}
{"bit_mismatches": 0, "check": "invalid_model_neutral", "fixture": "nan_state", "state": 5}
{"bit_mismatches": 0, "check": "unsupported_rate", "state": 8}
{"bit_mismatches": 0, "check": "reactivation", "history_reset": true}
{"check": "continuity", "event": "stop_worker", "max_envelope_sample_step": 7.039642927889056e-05, "min_amplitude": 0.09999999852325936, "settle_ms": 64.64583333333333, "state": 6}
{"check": "continuity", "event": "off", "max_envelope_sample_step": 7.133720962715362e-05, "min_amplitude": 0.09999999852325936, "settle_ms": 9.979166666666666, "state": 2}
{"check": "continuity", "event": "zero", "max_envelope_sample_step": 7.133720962715362e-05, "min_amplitude": 0.09999999852325936, "settle_ms": 9.979166666666666, "state": 3}
{"check": "continuity", "event": "on", "max_envelope_sample_step": 7.112093627076321e-05, "min_amplitude": 0.09999999852325936, "settle_ms": 9.625, "state": 1}
{"check": "bass", "difference_relative_db": -160.13168010863586, "error_rms_dbfs": -100.14462513094551, "hz": 50}
{"check": "bass", "difference_relative_db": -135.1315029056618, "error_rms_dbfs": -97.2356149312022, "hz": 100}
{"check": "bass", "difference_relative_db": -126.51876514797985, "error_rms_dbfs": -89.93500519360586, "hz": 200}
{"check": "bass", "difference_relative_db": -95.3573894640318, "error_rms_dbfs": -75.9755770376592, "hz": 299}
{"check": "bass", "difference_relative_db": -92.09977794941244, "error_rms_dbfs": -77.6329359891483, "hz": 300}
{"check": "stereo_multitone", "complex_lr_ratio_error": 1.060364171162394e-05, "fixture": "voice", "hz": 500}
{"check": "stereo_multitone", "complex_lr_ratio_error": 3.1884394672269446e-06, "fixture": "voice", "hz": 1000}
{"check": "stereo_multitone", "complex_lr_ratio_error": 2.8585773186814945e-06, "fixture": "voice", "hz": 3000}
{"check": "callback_256", "fixture": "voice", "overload_callbacks": 0, "thread_cpu_us_max": 755.62, "thread_cpu_us_p50": 654.4, "thread_cpu_us_p99": 685.6936, "wall_us_max": 761.23, "wall_us_p99": 689.8507999999999}
{"check": "stereo_multitone", "complex_lr_ratio_error": 1.7652496776185195e-06, "fixture": "uniform", "hz": 500}
{"check": "stereo_multitone", "complex_lr_ratio_error": 5.629265295665129e-07, "fixture": "uniform", "hz": 1000}
{"check": "stereo_multitone", "complex_lr_ratio_error": 4.555943650253145e-07, "fixture": "uniform", "hz": 3000}
{"check": "callback_256", "fixture": "uniform", "overload_callbacks": 0, "thread_cpu_us_max": 1185.781, "thread_cpu_us_p50": 655.27, "thread_cpu_us_p99": 769.4863999999989, "wall_us_max": 1191.47, "wall_us_p99": 771.8653999999988}
{"check": "stereo_multitone", "complex_lr_ratio_error": 1.2287580936832827e-05, "fixture": "voice_bins", "hz": 500}
{"check": "stereo_multitone", "complex_lr_ratio_error": 1.9922467109187345e-05, "fixture": "voice_bins", "hz": 1000}
{"check": "stereo_multitone", "complex_lr_ratio_error": 2.948122150538813e-06, "fixture": "voice_bins", "hz": 3000}
{"check": "callback_256", "fixture": "voice_bins", "overload_callbacks": 0, "thread_cpu_us_max": 770.611, "thread_cpu_us_p50": 660.73, "thread_cpu_us_p99": 704.3929999999999, "wall_us_max": 778.46, "wall_us_p99": 708.9649999999999}
{"check": "stereo_multitone", "complex_lr_ratio_error": 1.060364171162394e-05, "fixture": "q3", "hz": 500}
{"check": "stereo_multitone", "complex_lr_ratio_error": 3.1884394672269446e-06, "fixture": "q3", "hz": 1000}
{"check": "stereo_multitone", "complex_lr_ratio_error": 2.8585773186814945e-06, "fixture": "q3", "hz": 3000}
{"check": "callback_256", "fixture": "q3", "overload_callbacks": 0, "thread_cpu_us_max": 898.451, "thread_cpu_us_p50": 656.621, "thread_cpu_us_p99": 772.0962599999993, "wall_us_max": 903.241, "wall_us_p99": 773.3802599999995}
{"check": "stereo_bin", "complex_lr_ratio_error": 5.732081792574166e-06, "hz": 500}
{"check": "stereo_bin", "complex_lr_ratio_error": 2.8687531306397106e-06, "hz": 1000}
{"check": "stereo_bin", "complex_lr_ratio_error": 9.70803613477814e-07, "hz": 3000}
{"check": "true_peak", "dc": 0, "excess_linear": 0, "hz": 6000, "input_dbtp": -1.4156527925060667, "output_dbtp": -1.1549995037918777}
{"check": "true_peak", "dc": 0, "excess_linear": 0, "hz": 17000, "input_dbtp": -1.9384090832539362, "output_dbtp": -1.2517346871293291}
{"check": "true_peak", "dc": 0, "excess_linear": 0, "hz": 19000, "input_dbtp": -1.9384771591029333, "output_dbtp": -1.2515708906106977}
{"check": "true_peak", "dc": 0, "excess_linear": 0, "hz": 21000, "input_dbtp": -1.9684655780606175, "output_dbtp": -1.2216129277482852}
{"check": "true_peak", "dc": 0, "excess_linear": 0, "hz": 23000, "input_dbtp": -1.9384771591029346, "output_dbtp": -1.0219759044523926}
{"check": "true_peak", "dc": 0, "excess_linear": 0, "hz": 3000, "input_dbtp": -0.6303656851902566, "output_dbtp": -0.6303656851902566}
{"check": "true_peak", "dc": 0.95, "excess_linear": 0, "hz": 6000, "input_dbtp": -0.2645840068909675, "output_dbtp": -0.2645840068909675}
{"check": "silence", "nonzero_samples": 0}
SO_VERIFICATION_OK
EXIT=0
```

state:

```text
{"check": "file_created", "contract": "remix-state-v1", "instance": 1, "latency_frames": 3776, "model": "/home/minamorl/repos/.worktrees/audient-id14-linux/remix-runtime/crates/id14-remix-ladspa/fixtures/voice.onnx", "overloads": 0, "pid": 28, "state": 1, "state_name": "Active", "updated_unix_ms": 1791185404866}
{"check": "state_change_and_heartbeat", "heartbeat_ms": 1007, "state": 2, "state_name": "Off"}
{"check": "stopped_inference_visible", "contract": "remix-state-v1", "instance": 1, "latency_frames": 3776, "model": "/home/minamorl/repos/.worktrees/audient-id14-linux/remix-runtime/crates/id14-remix-ladspa/fixtures/voice.onnx", "overloads": 1, "pid": 28, "state": 6, "state_name": "Overloaded", "updated_unix_ms": 1791185406025}
{"error":{"code":"ModelMissing","details":"file is missing or unreadable","message":"Remix model unavailable","trace_id":"01a10af8-484d-72ba-a372-a4ac8ef17b56"},"level":"warn","msg":"Remix model unavailable; continuing with neutral playback","trace_id":"01a10af8-484d-72ba-a372-a4ac8ef17b56","ts":1791185406.0293899}
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

discovery:

```text
{"check": "no_baked_runtime_path", "forbidden_paths": ["/nix/store/3k9p1crsk8faxgir3xb5jimz9wwsm0ih-onnxruntime-1.27.1/lib/libonnxruntime.so", "/nix/store/3k9p1crsk8faxgir3xb5jimz9wwsm0ih-onnxruntime-1.27.1/lib/libonnxruntime.so", "/nix/store/3k9p1crsk8faxgir3xb5jimz9wwsm0ih-onnxruntime-1.27.1/lib/libonnxruntime.so.1.27.1"], "matches": 0, "compile_time_env_macros": 0}
{"bit_mismatches": null, "check": "env_before_home_and_loader", "cleanup_removed_state": true, "mapped_libraries": ["/home/minamorl/repos/.worktrees/audient-id14-linux/remix-runtime/crates/id14-remix-ladspa/.build/library-discovery-6z1x9e60/explicit/libonnxruntime.so"], "published_state": 1, "published_state_name": "Active", "state": 1}
{"bit_mismatches": null, "check": "home_before_loader", "cleanup_removed_state": true, "mapped_libraries": ["/nix/store/3k9p1crsk8faxgir3xb5jimz9wwsm0ih-onnxruntime-1.27.1/lib/libonnxruntime.so.1.27.1"], "published_state": 1, "published_state_name": "Active", "state": 1}
{"bit_mismatches": null, "check": "missing_env_falls_through_to_home", "cleanup_removed_state": true, "mapped_libraries": ["/nix/store/3k9p1crsk8faxgir3xb5jimz9wwsm0ih-onnxruntime-1.27.1/lib/libonnxruntime.so.1.27.1"], "published_state": 1, "published_state_name": "Active", "state": 1}
{"bit_mismatches": null, "check": "invalid_env_falls_through_to_home", "cleanup_removed_state": true, "mapped_libraries": ["/nix/store/3k9p1crsk8faxgir3xb5jimz9wwsm0ih-onnxruntime-1.27.1/lib/libonnxruntime.so.1.27.1"], "published_state": 1, "published_state_name": "Active", "state": 1}
{"bit_mismatches": null, "check": "normal_loader_search", "cleanup_removed_state": true, "mapped_libraries": ["/home/minamorl/repos/.worktrees/audient-id14-linux/remix-runtime/crates/id14-remix-ladspa/.build/library-discovery-6z1x9e60/loader/libonnxruntime.so"], "published_state": 1, "published_state_name": "Active", "state": 1}
{"bit_mismatches": null, "check": "missing_env_and_home_fall_through", "cleanup_removed_state": true, "mapped_libraries": ["/home/minamorl/repos/.worktrees/audient-id14-linux/remix-runtime/crates/id14-remix-ladspa/.build/library-discovery-6z1x9e60/loader/libonnxruntime.so"], "published_state": 1, "published_state_name": "Active", "state": 1}
{"bit_mismatches": null, "check": "unset_home_uses_loader", "cleanup_removed_state": true, "mapped_libraries": ["/home/minamorl/repos/.worktrees/audient-id14-linux/remix-runtime/crates/id14-remix-ladspa/.build/library-discovery-6z1x9e60/loader/libonnxruntime.so"], "published_state": 1, "published_state_name": "Active", "state": 1}
{"error":{"code":"ModelInvalid","details":"remix-v1 signature, metadata, operators or outputs are invalid","message":"Remix model unavailable","trace_id":"01a10af8-678a-76c9-b265-846b68794a86"},"level":"warn","msg":"Remix model unavailable; continuing with neutral playback","trace_id":"01a10af8-678a-76c9-b265-846b68794a86","ts":1791185414.0260785}
{"bit_mismatches": 0, "check": "all_absent_visible_neutral", "cleanup_removed_state": true, "mapped_libraries": [], "published_state": 5, "published_state_name": "ModelInvalid", "state": 5}
{"error":{"code":"ModelInvalid","details":"remix-v1 signature, metadata, operators or outputs are invalid","message":"Remix model unavailable","trace_id":"01a10af8-698c-7708-903a-5278fc6ab0f2"},"level":"warn","msg":"Remix model unavailable; continuing with neutral playback","trace_id":"01a10af8-698c-7708-903a-5278fc6ab0f2","ts":1791185414.540745}
{"bit_mismatches": 0, "check": "bad_override_all_absent_visible_neutral", "cleanup_removed_state": true, "mapped_libraries": [], "published_state": 5, "published_state_name": "ModelInvalid", "state": 5}
{"error":{"code":"ModelInvalid","details":"remix-v1 signature, metadata, operators or outputs are invalid","message":"Remix model unavailable","trace_id":"01a10af8-6b92-7060-9e29-eee58bac4d80"},"level":"warn","msg":"Remix model unavailable; continuing with neutral playback","trace_id":"01a10af8-6b92-7060-9e29-eee58bac4d80","ts":1791185415.0582824}
{"check": "unavailable_native_runtime", "library": "/home/minamorl/repos/.worktrees/audient-id14-linux/remix-runtime/crates/id14-remix-ladspa/.build/library-discovery-6z1x9e60/missing.so", "state": 5, "bit_mismatches": 0}
RUNTIME_FAILURE_VERIFICATION_OK
LIBRARY_DISCOVERY_VERIFICATION_OK
EXIT=0
```

bench:

```text
   Compiling id14-remix-ladspa v0.1.0 (/home/minamorl/repos/.worktrees/audient-id14-linux/remix-runtime/crates/id14-remix-ladspa)
    Finished `release` profile [optimized] target(s) in 2.55s
     Running `crates/id14-remix-ladspa/.build/target/release/examples/bench_worker /tmp/id14-remix-models/remix-20000.onnx 500`
{"load_stage":"actual_adapter_load_and_probe","ms":25.408163000000002}
{"adapter_load_stages_ms":{"decode_validate":0.92182,"probe":1.040881,"runtime_init":6.747411,"session_optimize_build":16.319191}}
{"max_ms":0.00082,"p50_ms":0.00021,"p99_ms":0.00023999999999999998,"phase":"adapter_input_copy","samples":500}
{"max_ms":0.87228,"p50_ms":0.651621,"p99_ms":0.66918,"phase":"adapter_session_run","samples":500}
{"max_ms":0.00596,"p50_ms":0.00258,"p99_ms":0.0034300000000000003,"phase":"adapter_output_copy_validate","samples":500}
{"max_ms":0.66474,"p50_ms":0.64168,"p99_ms":0.6545099999999999,"phase":"actual_worker_infer","samples":500}
EXIT=0
```

realtime:

```text
{"model": "/tmp/id14-remix-models/remix-20000.onnx", "bytes": 544901, "sha256": "b299f8ff621997c4412214755e1cd33c80d6fc889631fc6ec13a8e6a6a988e4d"}
{"blocks": 5625, "callback_wall_us_max": 1228.0700029805303, "callback_wall_us_p99": 889.0630368841812, "check": "trained_model_realtime", "deadline_lateness_ms_max": 0.42768333514686674, "interval_ms_p50": 5.333319990313612, "interval_ms_p99": 5.582262196694502, "latency_frames": 3776, "overloaded_blocks": 0, "period_ms": 5.333333333333333, "seconds": 30.0, "states": {"1": 5625}, "warmup_blocks": 187, "warmup_states": {"1": 187}}
{"check": "plugin_worker_infer", "samples": 2911, "p50_ms": 0.79305, "p99_ms": 1.006431, "max_ms": 1.06761}
REALTIME_SO_VERIFICATION_OK
EXIT=0
```

diff:

```text
EXIT=0
```

Initial workspace run (archived failure, before updating the single-source expectation):

```text
   Compiling id14-remix-ladspa v0.1.0 (/home/minamorl/repos/.worktrees/audient-id14-linux/remix-runtime/crates/id14-remix-ladspa)
    Finished `test` profile [unoptimized] target(s) in 6.22s
     Running unittests src/lib.rs (crates/id14-remix-ladspa/.build/target/debug/deps/id14_protocol-194b8cb0c917a2b5)

running 40 tests
test control_error::tests::dfu_missing_message_states_no_interface_0_fallback ... ok
test db::tests::formatting ... ok
test db::tests::unrepresentable_decibels_are_rejected ... ok
test descriptor::tests::ambiguous_dfu_interfaces_are_refused ... ok
test db::tests::whole_and_fractional_decibels ... ok
test descriptor::tests::dfu_interface_is_found_from_descriptor ... ok
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
test evidence_status_pins ... ok
test extension_code_pins ... ok
test product_table_pins ... ok
test request_header_pins ... ok

test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running unittests src/lib.rs (crates/id14-remix-ladspa/.build/target/debug/deps/id14_remix_ladspa-9aa0eaed2d8d05a9)

running 18 tests
fft_max_error=7.771561172376096e-16 sqrt_hann_cola_error=4.440892098500626e-16
test state::tests::model_failure_visible_without_audio_callback ... okonnx_rejected=bad_contract

test dsp::tests::fft_roundtrip_and_weighted_overlap ... ok
onnx_rejected=bad_q
onnx_rejected=unsupported_q
onnx_rejected=bad_shape
bass_guard_0_300_hz_worst_db=-74.90667323991724
test dsp::tests::bass_guard_stopband ... ok
test queue::tests::concurrent_order_and_backpressure ... ok
onnx_rejected=bad_sum
onnx_valid=voice q=0 state_size=3
onnx_rejected=negative
onnx_valid=uniform q=0 state_size=3
onnx_rejected=nan_mask
onnx_valid=voice_bins q=0 state_size=3
onnx_rejected=nan_state
test model::tests::rejects_contract_and_runtime_value_errors ... ok
onnx_valid=q3 q=3 state_size=3
onnx_valid=stateful_reordered q=0 state_size=3
onnx_valid=zero_state q=0 state_size=0
test model::tests::accepts_valid_models_and_maps_reordered_names ... ok
dry mode=missing bit_mismatches=0 reported=3776 cross_correlation_lag=3776 sr_inclusive_ms=100.000
test state::tests::unavailable_runtime_preserves_audio_bits ... ok
loudness part=0 settled_db=-3 max_hop_step_db=0.0053334236
loudness part=3 settled_db=3 max_hop_step_db=0.0053334236
test loudness::tests::both_gain_directions_converge_without_fast_modulation ... ok
dry mode=off bit_mismatches=0 reported=3776 cross_correlation_lag=3776 sr_inclusive_ms=100.000
dry mode=zero bit_mismatches=0 reported=3776 cross_correlation_lag=3776 sr_inclusive_ms=100.000
test verification::exact_dry_paths_and_measured_delay ... ok
silent_output_nonzero_samples=0
test verification::silence_adds_no_signal ... ok
hot_dc_protection_bit_mismatches=0 state=PeakProtected
test verification::peak_protection_keeps_hot_dc_input_unchanged ... ok
test state::tests::publication_updates_heartbeat_and_owned_cleanup ... ok
transition event=worker_pause max_envelope_sample_step=7.056823446954796e-5 min_amplitude=0.099999999 settle_ms=59.312 final_bit_mismatches=0 state=Overloaded
active gain_3000_hz=1.335771588 target=1.412537545 stereo_ratio_error=0e0 callback_256_us_p50=592.310 p99=14197.541 max=14302.840 final_status=Active

thread 'verification::active_gain_stereo_and_callback_cost' (625) panicked at crates/id14-remix-ladspa/src/verification.rs:165:5:
assertion failed: (measured_gain - 10_f64.powf(3.0 / 20.0)).abs() < 0.005
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
test verification::active_gain_stereo_and_callback_cost ... FAILED
lookahead_q0_vs_q3_max_sample_error=0e0
test verification::lookahead_masks_align_with_their_original_spectra ... ok
bass hz=50 difference_relative_db=-149.920814 error_rms_dbfs=-100.218311
true_peak high_hz=6000 input_dbtp=-1.415640 output_dbtp=-1.154986 excess_linear=0.000000000
callback_allocations=0 loudness_gain_db=-1.0453334
test verification::loudness_matching_callback_allocates_nothing ... ok
peak_protected_bass_150_hz_difference_relative_db=-86.147896
transition event=off max_envelope_sample_step=7.14424937841851e-5 min_amplitude=0.099999999 settle_ms=9.979 final_bit_mismatches=0 state=Off
bass hz=100 difference_relative_db=-133.735477 error_rms_dbfs=-97.219363
true_peak high_hz=17000 input_dbtp=-1.938164 output_dbtp=-1.251493 excess_linear=0.000000000
transition event=zero max_envelope_sample_step=7.14424937841851e-5 min_amplitude=0.099999999 settle_ms=9.979 final_bit_mismatches=0 state=ZeroAmounts
test verification::stalled_worker_and_switches_fade_current_audio_to_dry ... ok
bass hz=200 difference_relative_db=-125.930585 error_rms_dbfs=-89.920567
true_peak high_hz=3000 input_dbtp=-0.630341 output_dbtp=-0.630341 excess_linear=0.000000000
test verification::independent_true_peak_oracle ... ok
bass hz=299 difference_relative_db=-94.807000 error_rms_dbfs=-75.958520
test verification::low_band_retention ... ok

failures:

failures:
    verification::active_gain_stereo_and_callback_cost

test result: FAILED. 17 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 7.00s

error: test failed, to rerun pass `-p id14-remix-ladspa --lib`
EXIT=101
```

Artifacts (SHA256):

```text
80d17737b6db37f8642699598a2570b9e5bce24ee4b5d9bcbd7d8441f44071bd  /home/minamorl/repos/.worktrees/audient-id14-linux/remix-runtime/crates/id14-remix-ladspa/.build/loudness-before.so
1db87bfe5c0fefbd500266515522da26b4c1102af7184d7313ebb515e77043fa  /home/minamorl/repos/.worktrees/audient-id14-linux/remix-runtime/crates/id14-remix-ladspa/.build/target/release/libid14_remix_ladspa.so
72e0663cc9f0dd43c7a2f280aa8c369cfccac46961e4bcb232b146387f4bdab8  /home/minamorl/repos/.worktrees/audient-id14-linux/remix-runtime/crates/id14-remix-ladspa/.build/target/release/libid14_sr_ladspa.so
```
