"""Evaluate the actual Rust streaming output on every official-test FMA track."""
import json
import pathlib
import subprocess

import numpy as np
import soundfile as sf

from train_fma import N, HOP, WINDOW, decode, log

ROOT = pathlib.Path(__file__).resolve().parents[1]
TEMP = ROOT / "corpus/audio/fma/eval_tmp"
RENDER = ROOT / "target/release/sr-render"
TEMP.mkdir(parents=True, exist_ok=True)
RATE = 48000
BINS = 170


def lowpass(audio):
    spectrum = np.fft.rfft(audio, axis=0)
    spectrum[np.fft.rfftfreq(len(audio), 1 / RATE) > 12000] = 0
    return np.fft.irfft(spectrum, n=len(audio), axis=0).astype(np.float32)


def measure(reference, low, enhanced, copy_gain):
    sums = {name: {"frames": 0, "model": 0.0, "zero": 0.0, "copy": 0.0, "model_power": 0.0,
                   "truth_power": 0.0, "lowband_difference": 0.0, "lowband_power": 0.0}
            for name in ("all", "active", "inactive")}
    for channel in range(2):
        spectra = []
        for signal in (reference, low, enhanced):
            frames = np.lib.stride_tricks.sliding_window_view(signal[:, channel], N)[::HOP]
            spectra.append(np.fft.rfft(frames * WINDOW, axis=1))
        mags = [np.abs(spec).astype(np.float32) for spec in spectra]
        scale = np.maximum(mags[0][:, :257].mean(axis=1), 0.05)
        truth = np.log1p(mags[0][:, 257:427] / scale[:, None])
        zero = np.log1p(mags[1][:, 257:427] / scale[:, None])
        model = np.log1p(mags[2][:, 257:427] / scale[:, None])
        copied = np.log1p(copy_gain * mags[1][:, np.arange(257, 427) // 2] / scale[:, None])
        reference_low_power = np.square(mags[0][:, :257]).sum(axis=1)
        truth_high_power = np.square(mags[0][:, 257:427]).sum(axis=1)
        model_high_power = np.square(mags[2][:, 257:427]).sum(axis=1)
        flags = truth_high_power > 0.001 * np.maximum(reference_low_power, 1e-12)
        model_error = np.square(model - truth).sum(axis=1)
        zero_error = np.square(zero - truth).sum(axis=1)
        copy_error = np.square(copied - truth).sum(axis=1)
        low_difference = np.square(np.abs(spectra[2][:, :235] - spectra[1][:, :235])).sum(axis=1)
        low_power = np.square(np.abs(spectra[1][:, :235])).sum(axis=1)
        for name, mask in (("all", np.ones(len(flags), dtype=bool)), ("active", flags), ("inactive", ~flags)):
            group = sums[name]
            group["frames"] += int(mask.sum())
            group["model"] += float(model_error[mask].sum(dtype=np.float64))
            group["zero"] += float(zero_error[mask].sum(dtype=np.float64))
            group["copy"] += float(copy_error[mask].sum(dtype=np.float64))
            group["model_power"] += float(model_high_power[mask].sum(dtype=np.float64))
            group["truth_power"] += float(truth_high_power[mask].sum(dtype=np.float64))
            group["lowband_difference"] += float(low_difference[mask].sum(dtype=np.float64))
            group["lowband_power"] += float(low_power[mask].sum(dtype=np.float64))
    return {name: {"frames": group["frames"],
                   "model_log_mse": group["model"] / max(1, group["frames"] * BINS),
                   "zero_log_mse": group["zero"] / max(1, group["frames"] * BINS),
                   "copy_log_mse": group["copy"] / max(1, group["frames"] * BINS),
                   "model_vs_zero_percent": 100 * (1 - group["model"] / group["zero"]) if group["zero"] else None,
                   "model_vs_copy_percent": 100 * (1 - group["model"] / group["copy"]) if group["copy"] else None,
                   "model_to_truth_high_power": group["model_power"] / group["truth_power"] if group["truth_power"] else None,
                   "lowband_relative_power_error": group["lowband_difference"] / group["lowband_power"] if group["lowband_power"] else None}
            for name, group in sums.items()}


def main():
    training = json.loads((ROOT / "evidence/fma-trained-training.json").read_text())
    manifest = json.loads((ROOT / "corpus/fma-selected.json").read_text())
    by_id = {item["id"]: item for item in manifest["tracks"]}
    results = []
    for track_id in training["test_ids"]:
        track = by_id[track_id]
        source = decode(track_id)
        low = lowpass(source)
        input_path = TEMP / "input.wav"
        output_path = TEMP / "output.wav"
        try:
            sf.write(input_path, low, RATE, subtype="FLOAT")
            subprocess.run([str(RENDER), str(input_path), str(output_path)], check=True, capture_output=True)
            enhanced, rate = sf.read(output_path, dtype="float32", always_2d=True)
            if rate != RATE or enhanced.shape != source.shape:
                raise ValueError(f"renderer returned {rate} Hz / {enhanced.shape}, expected {source.shape}")
            metrics = measure(source, low, enhanced, training["copy_gain"])
            item = {"id": track_id, "genre": track["genre"], "metrics": metrics,
                    "peak": {"full": float(np.abs(source).max()), "lowpass": float(np.abs(low).max()),
                             "model": float(np.abs(enhanced).max())}}
            results.append(item)
            log("evaluated_stream_track", id=track_id, all=metrics["all"], peak=item["peak"])
        finally:
            input_path.unlink(missing_ok=True)
            output_path.unlink(missing_ok=True)
    summary = {}
    for name in ("all", "active", "inactive"):
        frames = sum(item["metrics"][name]["frames"] for item in results)
        model = sum(item["metrics"][name]["model_log_mse"] * item["metrics"][name]["frames"] for item in results)
        zero = sum(item["metrics"][name]["zero_log_mse"] * item["metrics"][name]["frames"] for item in results)
        copy = sum(item["metrics"][name]["copy_log_mse"] * item["metrics"][name]["frames"] for item in results)
        summary[name] = {"frames": frames, "model_log_mse": model / frames,
                         "zero_log_mse": zero / frames, "copy_log_mse": copy / frames,
                         "model_vs_zero_percent": 100 * (1 - model / zero) if zero else None,
                         "model_vs_copy_percent": 100 * (1 - model / copy) if copy else None}
    report = {"source": "official FMA small test split; selected CC0/CC BY tracks only",
              "input": "full MP3 decoded at 48 kHz, then ideal 12 kHz lowpass",
              "output": "actual Rust sr-render result, delay-aligned to input",
              "metrics": summary, "tracks": results}
    (ROOT / "evidence/fma-stream-heldout.json").write_text(json.dumps(report, indent=2) + "\n")
    log("finished_stream_eval", metrics=summary, tracks=len(results))


if __name__ == "__main__":
    main()
