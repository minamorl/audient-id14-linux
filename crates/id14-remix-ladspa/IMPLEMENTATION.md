# iD14 remix-v1 LADSPA playback runtime

`id14_remix_stereo` is a stereo LADSPA 1.1 plugin. The normal build is
`libid14_remix_ladspa.so`; it loads an ONNX separation model on a dedicated worker.
The model defaults to `~/.local/share/id14-sr/remix.onnx`.
`ID14_REMIX_MODEL` can select a different file, including verification fixtures.
The ONNX fixtures in this crate are synthetic tests, not a trained separation model.

Source: `audient-id14-realtime-sr@0.13`, imported `house_style@4.0` /
`prohibitions@1.0`, the supplied remix-v1 seam, and the caller's later API/lockfile
permissions. External primary sources are recorded in `RESEARCH.md`. No existing
target code or tests were used as implementation input.

## Host interface

| Index | Port | Direction/type | Default/range |
|---|---|---|---|
| 0 / 1 | Input L / Input R | input audio | float32 |
| 2 / 3 | Output L / Output R | output audio | float32 |
| 4 | Vocals | input control | +3 dB, -6..+6 |
| 5 | Drums | input control | 0 dB, -6..+6 |
| 6 | Bass | input control | 0 dB, -6..+6 |
| 7 | Other | input control | -3 dB, -6..+6 |
| 8 | Enabled | input control | on; >0 is on |
| 9 | State | output control | numeric status below |
| 10 | latency | output control | 3776 frames |

The graph order is remix then the unchanged SR node. At 48000 Hz the remix delay
is 3776 frames (78.667 ms); with the seam's 1024-frame SR delay, the total is 4800
frames / 100 ms. This delay remains constant when amounts, activation, model
availability, or overload state changes. The latency port reports only the remix
node's contribution. The graph owner must account for both nodes.

Status codes: Loading=0, Active=1, Off=2, ZeroAmounts=3, ModelMissing=4,
ModelInvalid/unsupported=5, Overloaded=6, PeakProtected=7, UnsupportedRate=8.
Missing/unreadable/invalid models and unsupported sample rates keep the dry path.
OFF and zero-amount controls take precedence in the displayed state.

Control hints encode the defaults, including +3/-3 via LADSPA's high/low hints.
Nonfinite amount controls become zero and out-of-range values are clamped.
Stereo in-place operation is supported. Reactivation resets audio and model
history; cleanup releases the worker. Numeric ID 0x145201 is a local identifier,
not a claim of allocation in the public LADSPA ID registry; identify by file/label.

## Streaming and real-time boundary

The audio thread owns STFT, synthesis, overlap-add, bass guard, delay, and peak
protection. Two bounded SPSC queues transfer features and frame-tagged masks.
All ONNX loading, optimization, allocation and inference happen on the worker.
The inference runtime is ONNX Runtime 1.27.1 (Nix), via ort 2.0.0-rc.12.
Only the CPU provider is selected: sequential execution, intra/inter thread count
1, graph optimization level 3, no spinning, GPU, OpenVINO or JIT provider.
The session and both input tensors persist across hops. ONNX state_out is copied
into the next state input; reset zeros this state. Tract 0.22.1 remains for
protobuf/contract validation and the explicit comparison benchmark, not inference.

`tools/with_cargo.sh` resolves nixpkgs#onnxruntime and embeds its absolute library
path through the build environment variable `ID14_ORT_LIBRARY`. The same variable
can override the path at process startup. Builds outside this wrapper should set
it to the packaged `libonnxruntime.so`, or provide that library on the dynamic
loader search path. The deployment must retain this native runtime in its Nix
closure; copying the plugin alone to another machine does not provide ONNX Runtime.
Missing/incompatible libraries produce ModelInvalid and neutral audio. Explicit
fallible loading happens on the inference worker before calling any ORT API.

`examples/bench_worker.rs` times the exact model adapter on a worker thread, with
separate startup and per-hop phases. `--tract-profile` additionally profiles the
old runtime's operators, tests execution-state reuse and compares streaming mask
values. `tools/verify_realtime_so.py` measures the actual plugin worker and drives
256-frame blocks on absolute 5.333 ms deadlines. Its diagnostic C entrypoint
`id14_remix_inference_times(handle, u64_buffer, capacity)` returns up to 8192 latest
hop durations (nanoseconds); call outside run after `id14_remix_stop_worker` for a
stable snapshot. Timing clocks and writes run only on the inference worker.
Initialization may allocate; `run` does not allocate, load files, take a mutex,
join a worker, or perform inference. The plugin does not advertise LADSPA's
stronger HARD_RT_CAPABLE timing guarantee.

STFT uses 1024 points, 512-sample hops and periodic square-root Hann windows.
The dry signal bypasses every transform and is copied without arithmetic when
correction is zero, including negative zero and subnormals. A missing worker mask
retains the last spectral shape only while fading the correction of current audio
back to dry; old audio blocks are not repeated. User/overload fades take 480 samples.
State advances on each processed feature hop, including silence. Sequence gaps
reset recurrent state and discard Q warmup masks before resuming aligned output.

The model must have exactly float32 x [1,4,513], state [1,S], mask [1,4,513], and
state_out [1,S], located by name rather than position. S must be concrete.
The metadata contract must be remix-v1 and lookahead must be a nonnegative integer.
This latency-bounded implementation supports Q=0..3; larger Q is reported as an
unsupported model. Masks must be finite, nonnegative, and sum to one per bin within
1e-4; state_out must be finite. External tensor sidecars are rejected.

## Low-band and peak protection

Bins below 300 Hz have no correction. A symmetric 1025-tap high-pass additionally
rejects synthesis-window leakage. `tools/design_guard.py` searches passband edges
400/425/450 Hz and chooses 450 Hz with <=0.1 dB ripple and >=66 dB rejection margin.
The 425 Hz candidate achieved about 66.7 dB rejection but 0.20 dB passband ripple;
the 400 Hz candidate missed 60 dB rejection. These are measured design candidates,
not a claim that no other design could narrow the band further.

The chosen coefficients reject 0..300 Hz by at least 74.9 dB, with about 0.079 dB
passband ripple. FIR group delay is 512 frames and is included in dry alignment.
The old 257-tap / 1100 Hz guard is available only through the explicit
`comparison-legacy-guard` feature, which reports its old 3584-frame latency.
`guard_comparison.json` reports the FIR transfer comparison; the dynamic `.so`
host also measures the complete correction transfer, including STFT.

Peak protection attenuates only correction, using a common stereo envelope.
It uses an 8-phase / 32-tap interpolation estimate and a triangle-inequality bound
on correction terms, with an amplitude ceiling of 0.889 (margin below -1 dBTP).
A 128-sample lookahead constrains envelope slopes. Already-hot input falls back
towards dry, never to a limiter on the input. Independent 32x interpolation checks
are part of verification; this is finite measured coverage, not a proof for every
possible signal or reconstruction filter.

## Reproduction

From the workspace root:

```sh
bash crates/id14-remix-ladspa/tools/with_python.sh crates/id14-remix-ladspa/tools/make_fixtures.py
bash crates/id14-remix-ladspa/tools/with_python.sh crates/id14-remix-ladspa/tools/design_guard.py
bash crates/id14-remix-ladspa/tools/with_cargo.sh test --workspace
bash crates/id14-remix-ladspa/tools/with_cargo.sh build --release -p id14-remix-ladspa
bash crates/id14-remix-ladspa/tools/with_python.sh crates/id14-remix-ladspa/tools/verify_so.py \
  --so "$PWD/crates/id14-remix-ladspa/.build/target/release/libid14_remix_ladspa.so"
```

The wrappers put Cargo/Nix caches and build outputs inside this crate's `.build`.
For the A/B test, build with `--features comparison-legacy-guard`, copy that .so to
`.build/legacy.so`, then rebuild without the feature and pass
`--legacy-so "$PWD/crates/id14-remix-ladspa/.build/legacy.so"` to the host.

The diagnostic export `id14_remix_stop_worker(handle)` terminates and joins the
actual inference worker. It is exclusively for tests, must be called outside the
audio callback, and is not a user control. Re-activate to create a fresh worker.

`validation/so-results.json` contains actual host measurements when generated.
These checks cover the playback runtime. Training, CLI/bar, PipeWire graph setup,
shipping a trained model, listening quality, and a 30-minute iD14 hardware/xrun
run belong to their respective integration lanes and are not claimed here.
# remix-state-v1 publication

Each LADSPA instance owns a separate `id14-remix-state` worker. It writes
`$XDG_RUNTIME_DIR/id14-sr/remix-state/<pid>-<instance>.json` via an exclusively
created temporary file in the same directory followed by rename. Cleanup joins
the publication worker, which removes its own file. Reactivation retains the
instance number, filename and overload counter.

The audio callback only updates a packed atomic status/counter. It performs no
publication allocation, filesystem operation, lock or worker wakeup syscall.
The publisher samples the latest status every 10 ms, coalescing shorter changes;
overload entries are counted even if the corresponding status was shorter than
the polling interval. `overloads` counts transitions into Overloaded, not audio
samples or elapsed blocks. The unchanged-state heartbeat is no more frequent
than once per second. `updated_unix_ms` is UTC Unix time of the write attempt.

The publication worker is independent of inference so stalled/stopped inference
does not prevent Overloaded from becoming visible. Missing/empty runtime env or
filesystem errors are silent and do not affect the audio path. An unsuccessful
write does not replace the last complete JSON; the next state change or heartbeat
attempts a current snapshot. The `model` field is the configured model path (also
when missing), or null when no path could be selected. For non-UTF-8 paths the
published text uses replacement characters.

`tools/verify_state_so.py` checks the actual shared object, including state changes,
heartbeat, stopped inference, multiple instances, cleanup and bit-identical audio
with absent/unusable/unwritable runtime directories. Follow-up evidence is in
`validation/state-*.log`, `validation/state-*.txt` and `validation/state-REPORT.md`.
