# id14 remix-v1 training

This directory trains the one-file, self-owned `remix-v1` causal separator.
It is deliberately separate from the playback runtime: no third-party model is
embedded or exported. The network is a frequency U-Net with a shared TDF layer
and a causal GRU. It is called exactly once per 512-sample hop. Its real masks
are a four-way softmax shared by L/R, so stereo position cannot be independently
moved by the model.

The exact analysis and synthesis transform is 48 kHz, `n_fft=1024`, `hop=512`,
`center=False`, and a square-root periodic Hann window on both sides. Models are
trained on contiguous 64-frame sequences. For lookahead `Q`, call `t` is scored
against source frame `t-Q`; state is never reset at evaluation block boundaries.
Training level is randomized uniformly from -40 to 0 dBFS. The sole objective is
the normalized complex error of the required filter-form remix with each of the
four gains independently randomized from -6 to +6 dB.

Training calls `forward_sequence` once per contiguous context. Frequency U-Net
layers fold time into the batch and the causal GRU consumes the whole time axis
in one `nn.GRU` call. The exported `forward` remains one-frame streaming and is
numerically checked against repeated sequence output. Checkpoints written before
this optimization retain the same parameter shapes and are mapped from the old
GRUCell key names when loaded.

## Data and legal boundary

[`sources.json`](sources.json) records the primary official record, license,
archive checksum, and intended use. The owner accepted the MUSDB18-HQ
educational/non-commercial terms on 2026-10-05. Its train split is the only
training corpus for this run and its test split is the held-out evaluator. The
audio is not redistributed and every trained model remains private-use only.
MoisesDB is explicitly excluded.

HTDemucs and any other third-party separator may only produce an offline teacher
or reference. Such weights are never loaded by `export.py`; `remix.onnx` contains
only this architecture and a checkpoint produced by `train.py`.

## Environment and training

Use Python 3.11 or newer with a PyTorch wheel available for the host. From this directory:

```sh
python3 -m venv .venv
.venv/bin/pip install -e '.[test]'
.venv/bin/pytest -q

# One resumable variant. latest.pt is atomically replaced every 100 steps.
.venv/bin/python -m remix_train.train \
  --musdb "$HOME/datasets/musdb18hq" \
  --size 131k --lookahead 2 --output runs

# All six variants, with 131k-q2 first.
PATH="$PWD/.venv/bin:$PATH" scripts/run_matrix.sh \
  --musdb "$HOME/datasets/musdb18hq" runs
```

Re-running the same command resumes `runs/SIZE-qQ/latest.pt`. Use `--fresh` only
to intentionally discard the resume point. Each run keeps its exact discovered
track manifest and append-only JSONL training log.

Every 2,000 steps, and at the final step, training atomically saves a checkpoint,
exports `runs/SIZE-qQ/remix-STEP.onnx`, and writes
`runs/SIZE-qQ/evaluation-STEP.json`. The JSON contains a fixed monitor batch's
training-corpus frame SDR and executable ONNX contract results; it is explicitly
not the final MUSDB18-HQ test score. The fixed seed makes successive checkpoints
comparable. Set `--artifact-every N` to change the
interval or `--artifact-every 0` to disable intermediate artifacts.

The Linux one-thread CPU forward/backward benchmark recommends
`--frames 32 --batch 2` for CPU fallback: it measured 1.20x over the frame loop,
while large flattened CPU batches lost cache efficiency. For the 128 GB M4 Max
MPS training,
keep the default `--frames 64 --batch 8` initially so the GPU sees 512 frames per
call; the Mac measurement is the final authority and can reduce batch first if
memory pressure appears. `--frames 128` is not recommended: CPU per-frame time
rose from 3.21 ms at 64 frames to 4.00 ms at 128 frames for batch 2.

Reproduce the CPU comparison with:

```sh
.venv/bin/python -m remix_train.benchmark_training \
  --size 131k --batches 1 2 4 8 --frames 32 64 --iterations 3 --threads 1
```

## Mac background operation

From a dedicated Mac worktree containing this directory, create the environment
and verify MPS before training:

```sh
python3 -m venv .venv
.venv/bin/pip install -e '.[test]'
.venv/bin/python -c 'import torch; print(torch.__version__); print(torch.backends.mps.is_built(), torch.backends.mps.is_available())'
```

Start the resumable matrix under macOS power assertions. The outer shell PID is
recorded in `runs/matrix.pid`, the active trainer PID in `runs/training.pid`,
combined launcher output goes to `runs/matrix.log`, and each variant appends
structured progress to `runs/SIZE-qQ/train.jsonl`.

```sh
mkdir -p runs
nohup sh -c 'trap '\''kill "$child" 2>/dev/null; wait "$child" 2>/dev/null'\'' TERM INT; \
  /usr/bin/caffeinate -dimsu scripts/run_matrix.sh --musdb "$HOME/datasets/musdb18hq" runs & \
  child=$!; wait "$child"' >runs/matrix.log 2>&1 </dev/null &
echo "$!" >runs/matrix.pid
```

Confirm the process and first loss records:

```sh
pid=$(cat runs/matrix.pid)
kill -0 "$pid" && ps -p "$pid" -o pid=,ppid=,etime=,command=
tail -n 5 runs/131k-q2/train.jsonl
```

Stop cleanly with `kill -TERM "$(cat runs/training.pid)"`. The trainer finishes
its current step, atomically writes `latest.pt`, records a `stopped` event, and
exits with code 143 so the matrix does not start the next variant. Re-running
the start command resumes existing checkpoints; it does not erase logs.

## Export and verification

```sh
.venv/bin/python -m remix_train.export runs/131k-q2/latest.pt \
  models/q2/remix.onnx --size 131k --lookahead 2
.venv/bin/python -m remix_train.contract models/q2/remix.onnx
.venv/bin/python -m remix_train.benchmark models/q2/remix.onnx
.venv/bin/python -m remix_train.evaluate models/q2/remix.onnx \
  --musdb "$HOME/datasets/musdb18hq" --output runs/131k-q2/musdb-test.json
```

Render the requested voice +3 dB / other -3 dB filter form, then compare with
the already-created offline HTDemucs reference:

```sh
.venv/bin/python -m remix_train.render models/q2/remix.onnx \
  /tmp/sr-eval/dry.wav runs/131k-q2/dry-remix.wav
.venv/bin/python -m remix_train.compare_ab runs/131k-q2/dry-remix.wav \
  /tmp/id14-remix-ab/labeled/dry_B_mild.wav \
  --output runs/131k-q2/dry-vs-b-mild.json
```

The model lookahead adds `Q * 10.667 ms`; the transform itself needs 1024
samples (`21.333 ms`). Even Q=4 remains 64 ms before the existing SR/runtime
queue. End-to-end compliance still belongs to the playback lane and must be
measured there; a model-only calculation is not reported as measured latency.
