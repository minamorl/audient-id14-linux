"""Short train-only diagnostic for FMA high-band activity and MP3 ceiling."""
import json
import pathlib
import subprocess

import numpy as np

ROOT = pathlib.Path(__file__).resolve().parents[1]
DATA = ROOT / "corpus/audio/fma/selected"
N = 1024
HOP = 256
WINDOW = np.hanning(N + 1)[:-1].astype(np.float32)


def decode(track_id, seconds=8):
    command = ["ffmpeg", "-v", "error", "-i", str(DATA / f"{track_id:06d}.mp3"),
               "-t", str(seconds), "-ar", "48000", "-ac", "2", "-f", "f32le", "-"]
    result = subprocess.run(command, check=True, capture_output=True)
    samples = np.frombuffer(result.stdout, dtype="<f4")
    return samples.reshape(-1, 2)


def analyze(audio):
    powers = []
    for channel in range(2):
        frames = np.lib.stride_tricks.sliding_window_view(audio[:, channel], N)[::HOP]
        spectrum = np.fft.rfft(frames * WINDOW, axis=1)
        powers.append(np.square(np.abs(spectrum)))
    power = np.concatenate(powers)
    low = power[:, :257].sum(axis=1)
    high = power[:, 257:427].sum(axis=1)
    upper = power[:, 342:427].sum(axis=1)
    active = high > 0.001 * np.maximum(low, 1e-12)
    return {"frames": int(len(low)), "active_frames": int(active.sum()),
            "active_fraction": float(active.mean()),
            "high_low_energy_ratio": float(high.sum() / max(low.sum(), 1e-12)),
            "upper_low_energy_ratio": float(upper.sum() / max(low.sum(), 1e-12)),
            "high_median_power": float(np.median(high)), "high_p95_power": float(np.quantile(high, 0.95))}


def main():
    manifest = json.loads((ROOT / "corpus/fma-selected.json").read_text())
    groups = {}
    for track in manifest["tracks"]:
        if track["split"] == "training":
            groups.setdefault(track["genre"], []).append(track)
    tracks = []
    for genre, group in sorted(groups.items()):
        tracks.extend(sorted(group, key=lambda item: item["id"])[:8])
    results = []
    for track in tracks:
        stats = analyze(decode(track["id"]))
        results.append({"id": track["id"], "genre": track["genre"], **stats})
        print(json.dumps(results[-1]), flush=True)
    report = {"selection": "first 8 training tracks by ID from each genre; initial 8 seconds per track, both channels",
              "active_definition": "12-20 kHz power > 0.001 times 0-12 kHz power",
              "tracks": results}
    (ROOT / "evidence/fma-activity-diagnostic.json").write_text(json.dumps(report, indent=2) + "\n")
    print("overall_frames", sum(item["frames"] for item in results),
          "active_frames", sum(item["active_frames"] for item in results), flush=True)


if __name__ == "__main__":
    main()
