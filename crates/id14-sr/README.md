# id14-sr phase 1

This crate supplies a self-trained **48 kHz stereo, 256-frame streaming**
high-frequency completion engine. `StreamingSr::process(input, output,
enabled, bandwidth)` accepts 256 interleaved stereo `f32` frames. The caller
declares whether its source is band-limited. `enabled=false` and
`Bandwidth::FullBandBypass` skip completion while keeping the same 768-frame
(16 ms) algorithm delay. The callback allocates no memory. Playback routing
and USB control are outside this crate.

The embedded version-4 model uses four causal STFT frames of pooled 0–12 kHz
log magnitudes (512 inputs), two 256-unit hidden layers, and 170 positive
12–20 kHz log-magnitude outputs. It is a trained predictor; harmonic phase
is synthesized from the doubled lower-octave phase. Its high-band energy is
limited to 5% of input low-band energy per STFT frame, with an additional
headroom bound. Existing lower-frequency STFT bins are preserved. The upper
20–24 kHz band is left empty. Inference uses a 1024-sample Hann window and
256-sample hop. Output quality is assessed below; listening remains a human
judgment.

## FMA source, selection and license

The [official FMA repository](https://github.com/mdeff/fma) lists
`fma_metadata.zip` (SHA-1
`f0df49ffe5f2a6008d7dc83c6915b31835dfe733`) and `fma_small.zip`
(SHA-1 `ade154f733639d52e35e32f5593efe5be76c6d70`). Both hashes are
verified before selection. `tracks.csv` identifies the 8,000-track small
subset and official training/validation/test split; `raw_tracks.csv` supplies
each track's exact `license_url`, artist, title, and source URL. The selection
script accepts only Creative Commons `.../licenses/by/...` and
`.../publicdomain/zero/1.0/` URLs. It excludes NC, ND, SA, ambiguous, and
missing license URLs. The selected 947 tracks span all eight genres, with
716 training, 180 validation, and 51 test tracks. Their 196 artist IDs do not
cross splits. [corpus/fma-selected.json](corpus/fma-selected.json) records
every ID, attribution, license URL, archive member and extracted MP3 hash.
CC BY tracks require attribution and a modification notice; the FMA metadata
is the source of each license assertion. The MP3 archive, selected audio, and
A/B WAVs are ignored by git. No source audio is distributed in this crate.

The source is **30-second MP3**, generally encoded from 44.1 kHz material;
decoding and resampling to 48 kHz does not recover an original 48 kHz master.
The high band is sparse above roughly 16 kHz on some tracks. The diagnostic
in `evidence/fma-activity-diagnostic.json` measures this by genre. It is a
limitation for a 12–20 kHz restoration target.

## Reproduce

From the workspace root on a machine with `uv`, `aria2c`, and `ffmpeg`:

```sh
UV_CACHE_DIR=crates/id14-sr/.uv-cache uv venv --python 3.12 crates/id14-sr/.venv
UV_CACHE_DIR=crates/id14-sr/.uv-cache uv pip install --python crates/id14-sr/.venv/bin/python torch==2.14.1 numpy==2.5.3 scipy==1.18.1 soundfile==0.14.0
mkdir -p crates/id14-sr/corpus/audio/fma
aria2c --no-conf=true --continue=true --max-connection-per-server=16 --split=16 --min-split-size=16M --checksum=sha-1=f0df49ffe5f2a6008d7dc83c6915b31835dfe733 --dir=crates/id14-sr/corpus/audio/fma --out=fma_metadata.zip 'https://os.unil.cloud.switch.ch/fma/fma_metadata.zip'
aria2c --no-conf=true --continue=true --max-connection-per-server=16 --split=16 --min-split-size=16M --checksum=sha-1=ade154f733639d52e35e32f5593efe5be76c6d70 --dir=crates/id14-sr/corpus/audio/fma --out=fma_small.zip 'https://os.unil.cloud.switch.ch/fma/fma_small.zip'
crates/id14-sr/.venv/bin/python crates/id14-sr/scripts/select_fma.py
crates/id14-sr/.venv/bin/python crates/id14-sr/scripts/scan_fma.py
crates/id14-sr/.venv/bin/python crates/id14-sr/scripts/train_fma.py --diagnostic --steps 1000 --batch 512 --seconds 8
crates/id14-sr/.venv/bin/python crates/id14-sr/scripts/train_fma.py --steps 4000 --batch 512 --seconds 8
CARGO_HOME=crates/id14-sr/.cargo-home CARGO_TARGET_DIR=crates/id14-sr/target cargo build --release -p id14-sr --bin sr-render --bin sr-bench
crates/id14-sr/.venv/bin/python crates/id14-sr/scripts/eval_fma_stream.py
crates/id14-sr/.venv/bin/python crates/id14-sr/scripts/make_fma_ab.py
```

Training used 2,143,704 stereo-channel STFT frames from 716 training tracks,
of which 584,021 met the declared high-band activity criterion. The model
ran for 4,000 AdamW steps, batch 512; step 3,000 was selected on complete
validation tracks. The official test tracks were not used for checkpoint or
spectral-copy gain selection. The gain `0.1` for a fixed lower-octave
spectral-copy baseline was selected on validation. The checkpoint is
`model/checkpoint.bin` (SHA-256
`6554b7497fd049e69d6039384b3bba1529bbdd39c332a53c3ab51f4df1c76d0c`,
963,264 bytes). The model input is a 12 kHz low-pass reconstruction, never
the full-band high-frequency target.

## Held-out evidence

The [training report](evidence/fma-trained-training.json) contains exact
train/validation/test IDs, per-track results and active/inactive metrics. On
all 51 complete official test tracks (573,144 stereo-channel frames), the
model's **phase-independent log-spectral MSE** was 0.007678, compared with
0.018519 for zero extension and 0.014320 for the spectral-copy baseline.
It improved active-frame log MSE by 60.63% versus zero and 49.03% versus
copy. It beat zero on 39 of 51 tracks and copy on 40 of 51; the other tracks
remain counterexamples. The modeled high-band power was 39.6% of reference
power on average, so this is a conservative restoration, not complete
reconstruction. The [actual Rust streaming evaluation](evidence/fma-stream-heldout.json)
measures delay-aligned output on the same complete tracks after synthesis,
overlap-add and safeguards. Its log-spectral MSE was **0.011944**, versus
0.018467 for zero and 0.014320 for fixed spectral copy: improvements of
35.32% and 16.59%. The result improved 48/51 tracks against zero and 44/51
against copy; the worst no-high-band track became worse. The maximum measured
relative low-band error was `3.81e-13`. On the Mac M4 Max, the release build
processed 256 stereo frames in median 176.75 microseconds (p95 211.83),
against a 5,333-microsecond chunk period. A Ryzen 7 7700 Linux CPU measurement
remains open. These objective results do not establish subjective preference.

Aligned 12-second full/low-pass/model WAVs from the worst, median and best
held-out tracks remain locally in ignored `corpus/audio/fma/ab/`; their
attribution, selection rule, hashes and objective excerpt metrics are in
`evidence/fma-ab-manifest.json`. They permit human A/B listening without
cherry-picking only favorable excerpts. No subjective pass is claimed.

The older [MusicNet report](evidence/phase1-report.md), Melomics baseline,
training logs and checkpoints remain in `evidence/` as failed experiments.
The MusicNet complex predictor was 0.08384% worse than zero extension on
complete held-out recordings, and an active-balanced variant collapsed to
near-zero calibrated output.

## Linux realtime playback integration

The sibling [`id14-sr-ladspa`](../id14-sr-ladspa/README.md) crate adapts this
fixed-size engine to arbitrary PipeWire/LADSPA host blocks, detects existing
full-band material, and provides the idempotent `id14-sr on|off|status`
command for Line and Headphones output. It is intentionally installed off.

## Primary documentation consulted

- <https://github.com/mdeff/fma>
- <https://creativecommons.org/licenses/by/4.0/>
- <https://creativecommons.org/licenses/by/3.0/>
- <https://creativecommons.org/publicdomain/zero/1.0/>
- <https://ffmpeg.org/ffmpeg.html>
- <https://docs.pytorch.org/docs/2.14/generated/torch.nn.Linear.html>
- <https://docs.pytorch.org/docs/2.14/generated/torch.nn.Softplus.html>
- <https://docs.pytorch.org/docs/2.14/generated/torch.optim.AdamW.html>
- <https://numpy.org/doc/stable/reference/generated/numpy.lib.stride_tricks.sliding_window_view.html>
- <https://docs.rs/rustfft/latest/rustfft/trait.Fft.html>
- <https://docs.rs/hound/latest/hound/struct.WavReader.html>
