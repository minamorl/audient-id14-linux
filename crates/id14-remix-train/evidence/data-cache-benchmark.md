# Data-cache benchmark

Measured on the Linux worker with `OMP_NUM_THREADS=1`, `MKL_NUM_THREADS=1`,
and `OPENBLAS_NUM_THREADS=1`. The input was three deterministic MUSDB-shaped
tracks, each 120 seconds at 44.1 kHz, with `mixture.wav` plus four stem WAVs.
Training used the 131K model, Q=2, batch 8, 64 frames, and one optimization
step. The model seed and data seed were identical in both runs.

Generate the input:

```sh
.venv/bin/python -m remix_train.generate_synthetic_musdb \
  runs/cache-benchmark/musdb --tracks 3 --seconds 120 --sample-rate 44100
```

Cold full-WAV loading (`--no-audio-cache`):

```sh
OMP_NUM_THREADS=1 MKL_NUM_THREADS=1 OPENBLAS_NUM_THREADS=1 \
  .venv/bin/python -m remix_train.train \
  --musdb runs/cache-benchmark/musdb --no-audio-cache \
  --size 131k --lookahead 2 --frames 64 --batch 8 --steps 1 \
  --artifact-every 0 --device cpu --fresh \
  --output runs/cache-benchmark/uncached
```

```json
{"step": 1, "loss": 0.1788015067577362, "lr": 0.0, "seconds": 6.2593014590092935, "sample_seconds": 3.350842485000612, "transfer_seconds": 0.00010441000631544739, "forward_seconds": 1.202647526995861, "backward_seconds": 1.7039217270066729, "optimizer_seconds": 0.0017853099998319522, "size": "131k", "lookahead": 2, "parameters": 130993}
```

First cached run (one-time construction is outside the optimization-step time):

```sh
OMP_NUM_THREADS=1 MKL_NUM_THREADS=1 OPENBLAS_NUM_THREADS=1 \
  .venv/bin/python -m remix_train.train \
  --musdb runs/cache-benchmark/musdb \
  --cache-dir runs/cache-benchmark/cache \
  --size 131k --lookahead 2 --frames 64 --batch 8 --steps 1 \
  --artifact-every 0 --device cpu --fresh \
  --output runs/cache-benchmark/cached-first
```

```json
{"event": "audio_cache", "cache_dir": "runs/cache-benchmark/cache", "tracks": 3, "built": 3, "reused": 0, "bytes": 345600384, "seconds": 3.539337147012702}
{"step": 1, "loss": 0.17880132794380188, "lr": 0.0, "seconds": 2.7003430269978708, "sample_seconds": 0.033118525010650046, "transfer_seconds": 0.00011085000005550683, "forward_seconds": 1.185245237997151, "backward_seconds": 1.4801011340023251, "optimizer_seconds": 0.0017672799876891077, "size": "131k", "lookahead": 2, "parameters": 130993}
```

Second process, reusing the completed cache:

```sh
OMP_NUM_THREADS=1 MKL_NUM_THREADS=1 OPENBLAS_NUM_THREADS=1 \
  .venv/bin/python -m remix_train.train \
  --musdb runs/cache-benchmark/musdb \
  --cache-dir runs/cache-benchmark/cache \
  --size 131k --lookahead 2 --frames 64 --batch 8 --steps 1 \
  --artifact-every 0 --device cpu --fresh \
  --output runs/cache-benchmark/cached-reuse
```

```json
{"event": "audio_cache", "cache_dir": "runs/cache-benchmark/cache", "tracks": 3, "built": 0, "reused": 3, "bytes": 345600384, "seconds": 0.006502039002953097}
{"step": 1, "loss": 0.17880132794380188, "lr": 0.0, "seconds": 2.7068804150039796, "sample_seconds": 0.03396499500377104, "transfer_seconds": 0.0002129399945260957, "forward_seconds": 1.1808439300075406, "backward_seconds": 1.4901471710036276, "optimizer_seconds": 0.0017113789945142344, "size": "131k", "lookahead": 2, "parameters": 130993}
```

The measured sampling phase fell from 3.350842 seconds to 0.033119 seconds
(101.2x), while the whole optimization step fell from 6.259301 seconds to
2.700343 seconds (2.32x). The float16 cache changed the deterministic one-step
loss by 0.000000179; forward/backward time remained in the same range, which
isolates the improvement to sampling rather than model computation.
