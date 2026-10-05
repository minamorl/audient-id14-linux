# id14-remix-eval

The runner loads the real remix and SR LADSPA shared objects through their C ABI,
feeds 256-frame blocks, and writes aligned float WAV files plus `metrics.md`.
It never installs a model or touches the live audio route: the model is copied to
an isolated temporary `HOME` for the lifetime of the process.

## Dependencies

Use Python 3 with the packages in `requirements.txt`. `run.sh` uses
`$REMIX_EVAL_PYTHON` when set and otherwise reuses
`/tmp/metric-harness/venv/bin/python` when that supplied harness exists.

Build the plugins first:

```sh
nix shell nixpkgs#rustc nixpkgs#cargo -c \
  cargo build --release -p id14-sr-ladspa -p id14-remix-ladspa
```

## One-command checkpoint evaluation

From the repository root:

```sh
crates/id14-remix-eval/run.sh \
  --model /tmp/id14-remix-models/remix-2000.onnx \
  --input /tmp/sr-eval/dry.wav /tmp/sr-eval/dry2.wav \
  --reference-dir /tmp/id14-remix-ab/labeled \
  --output-dir /tmp/id14-remix-ab/model-2000 \
  --tag model2000
```

The defaults are voice `+3 dB`, drums `0 dB`, bass `0 dB`, other `-3 dB`,
SR mixes `150` and `185`, and shared objects under `target/release`.
For another checkpoint only `--model`, `--output-dir`, and optionally `--tag`
need to change. `--help` lists individual amount and plugin-path overrides.

For each input, `renders/` receives remix-only and remix+SR 150/185 outputs.
The output directory also receives the loudness-matched listening triplet:
`A_dry`, `M_<tag>_remix_sr` (SR 185), and `R_offline_ref`. All three have the
same frame count and zero compensated plugin delay. `metrics.md` records metric
definitions, measured State/latency evidence, and spec-oriented tables.

The remix worker is prewarmed until LADSPA State reports Active. Processing is
paced between blocks; if State ever reports Overloaded, that attempt is discarded
and repeated with a longer worker wait, so a dry fallback cannot silently enter an
accepted offline render.
