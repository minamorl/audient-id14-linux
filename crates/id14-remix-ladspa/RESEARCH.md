# Primary sources read

- https://www.ladspa.org/ladspa_sdk/ladspa.h.txt — LADSPA 1.1 descriptor ABI, port flags/hints, in-place buffers, instantiate/activate/run/deactivate/cleanup lifecycle.
- https://www.ladspa.org/ladspa_sdk/overview.html — LADSPA host/plugin communication specification.
- https://docs.rs/tract-onnx/0.22.1/tract_onnx/ — selected CPU ONNX runtime API.
- https://github.com/sonos/tract/blob/main/README.md — runtime design and CPU kernels; no GPU backend selected.
- https://github.com/sonos/tract/blob/main/CHANGELOG.md — 0.23 facade/API changes; this adapter explicitly pins the reviewed 0.22.1 API.
- https://github.com/sonos/tract/blob/v0.22.1/examples/onnx-mobilenet-v2/src/main.rs — load, optimize, runnable plan, tensor I/O.
- https://github.com/sonos/tract/blob/v0.22.1/onnx/src/model.rs — protobuf loading and graph translation without a sidecar directory.
- https://github.com/sonos/tract/blob/v0.22.1/onnx/src/prost/onnx.rs — metadata, named tensor signatures and concrete shape definitions.
- https://github.com/sonos/tract/blob/v0.22.1/data/src/tensor.rs — owned float32 tensor construction.
- https://github.com/sonos/tract/blob/v0.22.1/core/src/model/typed.rs — runnable model type alias.
- https://onnx.ai/onnx/operators/onnx__Constant.html — deterministic mask fixture operator.
- https://onnx.ai/onnx/operators/onnx__Identity.html — fixture state passthrough operator.
- https://onnx.ai/onnx/operators/onnx__Add.html — stateful fixture operator.
- https://onnx.ai/onnx/api/helper.html — fixture graph/tensor construction and metadata.
- https://docs.scipy.org/doc/scipy/reference/generated/scipy.signal.remez.html — reproducible minimax FIR design and response evaluation.
- https://docs.scipy.org/doc/scipy/reference/generated/scipy.signal.resample_poly.html — independent 32x true-peak verification.
- https://docs.rs/uuid/latest/uuid/struct.Uuid.html#method.now_v7 — UUIDv7 diagnostic correlation IDs.
- https://doc.rust-lang.org/std/fs/fn.rename.html — same-directory replacement of the published state file on Unix.
- https://doc.rust-lang.org/std/fs/struct.OpenOptions.html#method.create_new — atomic exclusive creation of the worker-owned temporary file.
- https://raw.githubusercontent.com/sonos/tract/v0.22.1/core/src/plan.rs — plan.run creates SimpleState per call; run_plan_with_eval supports per-node profiling and persistent execution-state comparison.
- https://github.com/sonos/tract/blob/main/CHANGELOG.md — reviewed again for the performance follow-up; 0.23 changes API/runtime architecture, not adopted speculatively.
- https://docs.rs/ort/2.0.0-rc.12/ort/session/struct.Session.html — persistent session and named tensor inference.
- https://docs.rs/ort/2.0.0-rc.12/ort/session/builder/struct.SessionBuilder.html — fixed CPU thread counts, sequential execution and graph optimization.
- https://raw.githubusercontent.com/pykeio/ort/v2.0.0-rc.12/src/lib.rs — fallible explicit dynamic loading, minimum runtime API version and implicit-load panic boundary.
- https://raw.githubusercontent.com/pykeio/ort/v2.0.0-rc.12/src/environment.rs — init_from, environment initialization and logger lifetime.
- https://raw.githubusercontent.com/pykeio/ort/v2.0.0-rc.12/src/value/impl_tensor/create.rs — allocator-backed persistent input tensors.
- https://raw.githubusercontent.com/pykeio/ort/v2.0.0-rc.12/src/value/impl_tensor/extract.rs — mutable input slices and typed output extraction.
- https://github.com/pykeio/ort/releases/tag/v2.0.0-rc.12 — selected Rust binding release notes (the repository has no CHANGELOG.md at the attempted location).
- https://onnxruntime.ai/docs/performance/tune-performance/threading.html — CPU sequential execution, intra-op count 1 and disabled worker spinning.
- https://github.com/microsoft/onnxruntime/releases/tag/v1.27.1 — selected Nix native runtime release notes.
- https://onnx.ai/onnx/api/reference.html — independent reference evaluator used to adjudicate the measured ORT/tract mask difference.
- https://raw.githubusercontent.com/onnx/onnx/main/onnx/reference/reference_evaluator.py — reference execution API and its specification-compliance limitations.
- https://raw.githubusercontent.com/pykeio/ort/v2.0.0-rc.12/src/error.rs — bootstrap error construction calls the native API; explains the reproduced missing-library initialization hang.
- https://docs.rs/libloading/latest/libloading/struct.Library.html — checked native-library/symbol loading and handle lifetime (0.9.0).
- https://onnxruntime.ai/docs/api/c/struct_ort_api_base.html — versioned OrtGetApiBase/GetApi function-table contract.

Read during this implementation run. These are external primary API sources, explicitly
authorized by the caller. No existing target code or tests were read.
