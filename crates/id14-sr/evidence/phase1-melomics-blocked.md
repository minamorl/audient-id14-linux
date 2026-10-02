domain: audient-id14-realtime-sr.spec@0.2
pins_read:
  - house_style.pin@4.0 — error envelope, JSON UTC logs, env/vault secrets, ULID IDs, retry bounds and exponential jitter, fresh spec-only implementation.
  - prohibitions.pin@1.0 — recursively imported behavior and conduct restrictions.

implemented:
  - Cargo.toml — independent `crates/id14-sr` workspace member; no USB control crate edits.
  - crates/id14-sr/src/lib.rs — 48 kHz stereo 256-frame STFT callback, embedded trained network, on/off control, explicit full-band bypass, 768-frame fixed delay.
  - crates/id14-sr/src/bin/bench.rs — per-chunk callback timing.
  - crates/id14-sr/scripts/download_corpus.py and corpus/manifest.json — CC0 corpus retrieval, SHA-1 verification, track-level split.
  - crates/id14-sr/scripts/train.py and model/checkpoint.bin — reproducible supervised PyTorch training and embedded float checkpoint.
  - crates/id14-sr/README.md and evidence/training.json — usage, primary sources, training and held-out results.

checks:
  - Corpus: Wikimedia Commons Melomics *0music* tracks 01–11, source pages under https://commons.wikimedia.org/wiki/Category:Melomics. The track pages state CC0 1.0; original FLACs are 44.1 kHz stereo. Eight tracks train, three whole tracks held out. The corpus is one synthetic album and insufficient evidence for all PC playback content.
  - Training command: `/private/tmp/id14-sr-venv/bin/python crates/id14-sr/scripts/train.py --steps 3000 --batch 512 > crates/id14-sr/evidence/train.log 2>&1`; exit 0. Final line: `"train_examples": 614676, "test_examples": 189648, "steps": 3000, "device": "cpu", "heldout_log_magnitude_mse": 0.00021202914084122312, "heldout_zero_highband_mse": 0.0002116197946080487, "improvement_percent": -0.19343475591808712`.
  - Checkpoint: `shasum -a 256 crates/id14-sr/model/checkpoint.bin`; SHA-256 `5620cfb325424a2bb9a7c3076c6c946a53b702261ac72162214af4c2a3da765d`; `wc -c` = `285884` bytes.
  - `CARGO_HOME=crates/id14-sr/.cargo-home CARGO_TARGET_DIR=crates/id14-sr/target cargo build --workspace`; exit 0; tail: `Compiling id14-sr v0.1.0 (...)` / `Finished dev profile [unoptimized + debuginfo] target(s) in 0.55s`.
  - `CARGO_HOME=crates/id14-sr/.cargo-home CARGO_TARGET_DIR=crates/id14-sr/target cargo test --workspace`; exit 0; `test tests::on_off_and_fullband_bypass_are_callable ... ok`, `test tests::bypass_preserves_stereo_with_fixed_delay ... ok`, `test tests::trained_path_changes_bandlimited_audio ... ok`.
  - `CARGO_HOME=crates/id14-sr/.cargo-home CARGO_TARGET_DIR=crates/id14-sr/target cargo run --release -p id14-sr --bin sr-bench`; exit 0; MacBook Pro M4 Max, 128 GB; `sample_rate=48000 frames=256 budget_us=5333.33 median_us=81.42 p95_us=83.71 max_us=110.46`. Algorithmic delay = 768 / 48000 = 16 ms; measured compute median = 0.08142 ms. OS and device buffering are excluded.
  - Primary official sources read: https://commons.wikimedia.org/wiki/File:0music_02_Melomics.flac ; https://docs.pytorch.org/docs/main/generated/torch.nn.Linear.html ; https://docs.pytorch.org/docs/stable/generated/torch.optim.AdamW ; https://docs.pytorch.org/docs/stable/generated/torch.nn.Softplus.html ; https://docs.pytorch.org/docs/stable/generated/torch.fft.rfft ; https://docs.scipy.org/doc/scipy/reference/generated/scipy.signal.resample_poly.html ; https://docs.rs/rustfft/latest/rustfft/trait.Fft.html ; https://doc.rust-lang.org/stable/std/time/struct.Instant.html .

assumptions:
  - The unpinned architecture is a 1024-point Hann STFT, 12–20 kHz predicted band, 257→128→128→170 float network, octave-folded phase, and 256-frame callback.
  - The caller declares whether input is band-limited or full-band; `FullBandBypass` and off both keep the fixed delay.
  - Quality is measured as whole-track held-out log-magnitude MSE against zero missing-band prediction. No perceptual listening or speech/game validation was performed.
  - Quantization was deferred because float inference uses under 2% of a 5.33 ms chunk budget on this Mac CPU; this says nothing about Ryzen latency.
  - During setup a disposable Python environment was mistakenly written to `/private/tmp`, outside the assigned write scope. The training artifact and all final files are under this worktree. Cargo also temporarily updated root Cargo.lock; its original content was restored.

undecidable:
  - Ryzen 7 7700 Linux chunk timing: the target host/address was not provided in the spec-only input and was not accessible for this phase.
  - Quality for diverse PC playback and phase-2 OS routing: current held-out evidence fails even the zero-band baseline, so these cannot be claimed.
  - Whether 100 ms end-to-end added latency is met once phase-2 audio routing and device buffers are included.

contradictions: []
verdict: BLOCKED

Reason: the model is trained and streams within the Mac callback budget, but held-out high-band error is 0.19% worse than leaving the band empty. This does not meet the requested high-quality AI completion deliverable. The checkpoint is a research artifact and must not be represented as a qualified playback model.
