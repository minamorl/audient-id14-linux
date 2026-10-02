# FMA phase-1 evidence for supervisor judgment

## Provenance and licensing

Primary source: <https://github.com/mdeff/fma>. The official SHA-1 values
verified locally: `fma_metadata.zip`
`f0df49ffe5f2a6008d7dc83c6915b31835dfe733`; `fma_small.zip`
`ade154f733639d52e35e32f5593efe5be76c6d70`. The raw metadata's exact
`license_url` allowed 947 of 8,000 small-subset tracks: CC BY 4.0 547,
CC BY 3.0 241, CC BY 3.0 US 101, older explicit CC BY 4, and CC0 54.
NC, ND, SA, unclear, and missing URLs were excluded. The selected MP3s total
930,130,845 bytes. `corpus/fma-selected.json` lists every ID, artist, title,
track URL, exact license URL and SHA-256. The official split yields 716 train,
180 validation and 51 test tracks; artist IDs do not cross the splits.
CC BY source metadata requires attribution and identifying transformed audio;
only the manifest and checkpoint are distributed. Raw and A/B audio remain
ignored locally.

FMA small distributes 30-second MP3, not original 48 kHz PCM. The decoded
audio was resampled to 48 kHz. A 64-track training diagnostic found 45,693
active stereo-channel frames of 191,616 (23.85%) using 12–20 kHz power >
0.001 times 0–12 kHz power. The 16–20 kHz band is thin in several genres;
see `fma-activity-diagnostic.json` for per-track values. This codec ceiling
limits claims about full-band reconstruction.

## Trained model and baselines

The causal four-frame model is a self-built 512 → 256 → 256 → 170 network,
trained on log1p high-band magnitude with extra weight for active frames.
Its 1024-point Hann STFT and 256-sample hop preserve input bins below 12 kHz;
phase is synthesized from the lower octave. The selected checkpoint is step
3,000 of 4,000 AdamW updates (batch 512), from 2,143,704 training frames,
584,021 of them active. The checkpoint is 963,264 bytes, SHA-256
`6554b7497fd049e69d6039384b3bba1529bbdd39c332a53c3ab51f4df1c76d0c`.
The reference model evaluates zero extension and a fixed lower-octave
spectral-copy baseline; the copy gain of 0.1 was selected on validation.
Checkpoint and gain selection did not use official test tracks.

On the 51 complete official test tracks, the Python checkpoint evaluation
used 573,144 stereo-channel STFT frames. Phase-independent log-spectral MSE:

| Segment | Model | Zero extension | Spectral copy | Model gain vs zero | Model gain vs copy |
| --- | ---: | ---: | ---: | ---: | ---: |
| All | 0.00767841 | 0.01851946 | 0.01431977 | 58.54% | 46.38% |
| Active (140,531 frames) | 0.02747412 | 0.06977647 | 0.05389727 | 60.63% | 49.03% |
| Inactive (432,613 frames) | 0.00124793 | 0.00186901 | 0.00146333 | 33.23% | 14.72% |

The Python predictor beat zero on 39/51 tracks and copy on 40/51. Model
high-band power averaged 0.00292 of low-band power, versus 0.00737 in the
reference; it was not merely zero output. Some tracks were worse, especially
those with almost no original high band. Full per-track values are in
`fma-trained-training.json`.

The **actual Rust streaming output**, delay-aligned after harmonic-phase
synthesis, overlap-add, 5% per-frame high-band energy cap and peak-headroom
bound, was evaluated on the same complete official test tracks. Its metric
is recomputed from the rendered WAV and differs from the direct predictor
metric above:

| Segment | Rust output | Zero extension | Spectral copy | Gain vs zero | Gain vs copy |
| --- | ---: | ---: | ---: | ---: | ---: |
| All | 0.01194417 | 0.01846717 | 0.01431977 | 35.32% | 16.59% |
| Active | 0.04494855 | 0.06946715 | 0.05389727 | 35.30% | 16.60% |
| Inactive | 0.00122295 | 0.00190022 | 0.00146333 | 35.64% | 16.43% |

The streaming output beat zero on 48/51 tracks and copy on 44/51. The
worst near-empty-high-band track, ID 132448, was worse than zero by 248.5%
in relative terms, though its absolute zero MSE was only 0.000000873.
Across test tracks, maximum relative low-band spectral error was
`3.81e-13`. The largest observed output peak increase over the low-pass
input was 0.01253 at track 140922. Source tracks already above unit peak
remain above unit peak; the high-band headroom guard does not limit the
pre-existing low band. Full per-track and peak values are in
`fma-stream-heldout.json`.

## Runtime and listening boundary

Mac M4 Max release benchmark: 256 stereo frames at 48 kHz had median
176.75 microseconds, p95 211.83 microseconds, and maximum 334.46
microseconds; the block period is 5,333.33 microseconds. The fixed DSP delay
is 768 frames (16 ms), under the pinned ~100 ms goal. No Ryzen 7 7700 Linux
measurement was accessible. The callback allocates no memory and has explicit
off and full-band bypass paths.

`fma-ab-manifest.json` identifies ignored local full/low-pass/model WAVs from
the worst, median and best official-test tracks under whole-track Python
model score. Their fixed central 12-second excerpts expose a failure as well
as improvements. No subjective listening result is claimed. A listener
still needs to judge artifacts and preference before a consumer-facing
quality claim. The older Melomics and MusicNet failures are retained in
their original reports and checkpoints.
