"""Make local ignored A/B audio at worst, median and best held-out tracks."""
import hashlib
import json
import pathlib
import subprocess

import numpy as np
import soundfile as sf

from train_fma import N, HOP, WINDOW, decode

ROOT = pathlib.Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "corpus/audio/fma/ab"
OUTPUT.mkdir(parents=True, exist_ok=True)
RENDER = ROOT / "target/release/sr-render"
RATE = 48000


def cut(audio):
    frequencies = np.fft.rfftfreq(len(audio), 1 / RATE)
    spectrum = np.fft.rfft(audio, axis=0)
    spectrum[frequencies > 12000] = 0
    return np.fft.irfft(spectrum, n=len(audio), axis=0).astype(np.float32)


def metrics(audio, lowpass, enhanced):
    totals = {name: 0.0 for name in ("zero", "model", "lowband")}
    count = 0
    for channel in range(2):
        frames = [np.lib.stride_tricks.sliding_window_view(signal[:, channel], N)[::HOP]
                  for signal in (audio, lowpass, enhanced)]
        specs = [np.abs(np.fft.rfft(frame * WINDOW, axis=1)) for frame in frames]
        scale = np.maximum(specs[0][:, :257].mean(axis=1), 0.05)
        truth = np.log1p(specs[0][:, 257:427] / scale[:, None])
        baseline = np.log1p(specs[1][:, 257:427] / scale[:, None])
        prediction = np.log1p(specs[2][:, 257:427] / scale[:, None])
        totals["zero"] += float(np.square(baseline - truth).sum())
        totals["model"] += float(np.square(prediction - truth).sum())
        totals["lowband"] += float(np.square(specs[2][:, :235] - specs[1][:, :235]).sum()
                                   / max(np.square(specs[1][:, :235]).sum(), 1e-12))
        count += truth.size
    return {"zero_log_mse": totals["zero"] / count,
            "model_log_mse": totals["model"] / count,
            "model_vs_zero_percent": 100 * (1 - totals["model"] / totals["zero"]) if totals["zero"] else None,
            "lowband_relative_power_error_sum_channels": totals["lowband"]}


def main():
    report = json.loads((ROOT / "evidence/fma-trained-training.json").read_text())
    manifest = json.loads((ROOT / "corpus/fma-selected.json").read_text())
    by_id = {item["id"]: item for item in manifest["tracks"]}
    ranked = sorted(report["heldout"]["per_track"], key=lambda item: item["model_vs_zero_percent"])
    choices = {"worst": ranked[0], "median": ranked[len(ranked) // 2], "best": ranked[-1]}
    examples = []
    for label, result in choices.items():
        track = by_id[result["id"]]
        full = decode(track["id"], start=9, seconds=12)
        lowpass = cut(full)
        prefix = OUTPUT / f"{label}-{track['id']:06d}"
        full_path = prefix.with_name(prefix.name + "-full.wav")
        low_path = prefix.with_name(prefix.name + "-lowpass.wav")
        model_path = prefix.with_name(prefix.name + "-model.wav")
        for path, samples in ((full_path, full), (low_path, lowpass)):
            sf.write(path, samples, RATE, subtype="FLOAT")
        subprocess.run([str(RENDER), str(low_path), str(model_path)], check=True)
        enhanced, rate = sf.read(model_path, dtype="float32", always_2d=True)
        assert rate == RATE and enhanced.shape == full.shape
        item = {"selection": label, "selection_rule": "worst/median/best whole-track official-test log-spectral model-vs-zero result",
                "track": track, "heldout_track_metric": result, "excerpt_start_seconds": 9,
                "excerpt_duration_seconds": 12, "codec_note": "FMA 30-second MP3 decoded and resampled to 48 kHz; not original 48 kHz PCM",
                "files": {name: str(path.relative_to(ROOT)) for name, path in
                          (("full", full_path), ("lowpass", low_path), ("model", model_path))},
                "sha256": {name: hashlib.sha256(path.read_bytes()).hexdigest() for name, path in
                           (("full", full_path), ("lowpass", low_path), ("model", model_path))},
                "peak": {"full": float(np.abs(full).max()), "lowpass": float(np.abs(lowpass).max()),
                         "model": float(np.abs(enhanced).max())},
                "excerpt_metrics": metrics(full, lowpass, enhanced)}
        examples.append(item)
        print(json.dumps({"selection": label, "id": track["id"], "peak": item["peak"],
                          "excerpt_metrics": item["excerpt_metrics"]}), flush=True)
    (ROOT / "evidence/fma-ab-manifest.json").write_text(json.dumps({
        "source": "https://github.com/mdeff/fma", "license_scope": "each track carries its own CC0 or CC BY URL",
        "audio_distribution": "WAV files remain ignored in corpus/audio/fma/ab; regenerate locally",
        "examples": examples,
    }, indent=2, ensure_ascii=False) + "\n")


if __name__ == "__main__":
    main()
