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

## Data and legal boundary

[`sources.json`](sources.json) records the primary official pages, licenses,
archive hash where published, and intended use. MoisesDB is the preferred
real-recorded training corpus and Slakh2100 is optional synthetic augmentation.
MUSDB18-HQ is the held-out evaluator. Its current Zenodo record exposes the
archive but explicitly declares a license agreement; this repository does not
accept that agreement for the owner. Place owner-obtained corpora outside git.

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
  --moises /path/to/moisesdb_v0.1 \
  --size 131k --lookahead 2 --output runs

# All six variants (131K/444K x Q=0/2/4), sequentially.
PATH="$PWD/.venv/bin:$PATH" scripts/run_matrix.sh \
  --moises /path/to/moisesdb_v0.1 runs
```

Re-running the same command resumes `runs/SIZE-qQ/latest.pt`. Use `--fresh` only
to intentionally discard the resume point. Each run keeps its exact discovered
track manifest and append-only JSONL training log.

## Export and verification

```sh
.venv/bin/python -m remix_train.export runs/131k-q2/latest.pt \
  models/q2/remix.onnx --size 131k --lookahead 2
.venv/bin/python -m remix_train.contract models/q2/remix.onnx
.venv/bin/python -m remix_train.benchmark models/q2/remix.onnx
.venv/bin/python -m remix_train.evaluate models/q2/remix.onnx \
  --musdb /path/to/MUSDB18-HQ --output runs/131k-q2/musdb-test.json
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
