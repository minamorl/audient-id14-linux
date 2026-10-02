# Phase 1 quality gate: BLOCKED

The user-space Rust streaming engine accepts 48 kHz stereo blocks of 256 frames,
has explicit off and full-band bypass, and embeds a self-trained 608,868-byte
checkpoint. Its measured Mac M4 Max CPU median is 90.71 microseconds per block
(p95 92.96 microseconds) against a 5,333.33-microsecond block period. The
fixed algorithm delay is 768 frames (16 ms). These are implementation and
latency results only; they do not establish useful audio quality. A Ryzen 7
7700 Linux measurement remains open for phase 2.

## Source and split

MusicNet 1.0, <https://zenodo.org/records/5120004>, is reported as CC BY 4.0
by the official record API. The official 11,097,394,998-byte archive verified
against MD5 `844764911fa0d5b97c97da944a057590`; its metadata CSV verified
against MD5 `1caef62cee9c875235e62aac368b49d8`. The raw 44.1 kHz mono PCM
recordings stay outside git. The official split has 320 train and 10 test
recordings. Each experiment held out complete recordings; the official test
recordings were not used to select a checkpoint. Each MusicNet run used 300
train recordings, 1,347,300 sampled frames, and 5,000 optimizer steps at
batch size 512. Per-record test metrics and exact IDs are in the JSON reports.

| Experiment | Selected step | Official test frames | Model MSE | Zero-band MSE | Relative improvement |
| --- | ---: | ---: | ---: | ---: | ---: |
| Complex, uniform (embedded) | 4,000 | 277,668 | 0.00088169362 | 0.00088095504 | −0.08384% |
| Complex, 50% active batches | 1,000 | 277,668 | 0.00088095458 | 0.00088095504 | +0.000052% |
| Positive amplitude, activity-stratified validation | 500 | 277,668 | 0.00088210929 | 0.00088095504 | −0.13102% |

The balanced complex run calibrated to gain 0.00092855, so its tiny numerical
advantage is effectively zero output. Its validation set had only 36 active
frames in 44,910, while activity-stratified validation in the magnitude run
had 4,807 active frames in 44,910. The latter still lost on the official test
set. In that test set, 6,695 of 277,668 frames met the 12–20 kHz activity
criterion (2.41%). The original Melomics softplus run is retained as a failed
baseline in `melomics-softplus-training.json`; its held-out MSE was 0.1934%
worse than zero-band prediction.

Two official test-recording excerpts are supplied as full, 12 kHz low-pass,
and processed 48 kHz stereo WAVs in `ab/`, with CC BY attribution and hashes
in `ab/manifest.json`. Their local high-band spectral errors improve slightly,
but both were selected for unusually high activity and cannot overturn the
full-recording test result. Human listening was not recorded as a quality
judgment. The streaming checkpoint SHA-256 is
`33aabc7fcca4349d720aef7cf1c3b71921a07a5502c33d63c874d9a08ab01a65`.

## Next evidence needed

This corpus is mostly low-band-inactive under the defined 12–20 kHz criterion,
and a single-frame feed-forward predictor could not infer reliable complex
high-band content. A further candidate needs a legally distributable,
spectrally diverse full-band 48 kHz music corpus containing cymbals, dense
percussion, and modern mixes, with whole-recording held-out evaluation and
explicit license attribution. It also needs a temporal context model with
phase-coherent synthesis, plus objective and listening comparisons against
zero-band output on high-band-active segments and entire recordings. Do not
claim a phase-1 quality pass until those comparisons show meaningful gains.
