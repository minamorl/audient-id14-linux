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

Read during this implementation run. These are external primary API sources, explicitly
authorized by the caller. No existing target code or tests were read.
