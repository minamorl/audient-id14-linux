"""Frame-wise MUSDB-HQ SDR evaluation (streaming, never block-reset)."""

from __future__ import annotations

import argparse
import json
from pathlib import Path

import numpy as np
import torch

from .audio import stft
from .data import discover_musdb, load_track
from .inference import StreamingOnnx


def frame_sdr(reference: torch.Tensor, estimate: torch.Tensor) -> np.ndarray:
    """Stereo complex frame SDR, with silent reference frames omitted."""
    signal = reference.abs().square().sum(dim=(1, 2))
    error = (reference - estimate).abs().square().sum(dim=(1, 2))
    valid = signal > 1e-8
    return (10.0 * torch.log10(signal[valid] / error[valid].clamp_min(1e-12))).numpy()


def evaluate(model_path: Path, musdb: Path) -> dict:
    runner = StreamingOnnx(model_path)
    tracks = [track for track in discover_musdb(musdb) if track.split == "test"]
    if not tracks:
        raise ValueError(f"no MUSDB-HQ test tracks below {musdb}")
    per_track, all_values = [], [[] for _ in range(4)]
    for track in tracks:
        mixture, sources = load_track(track)
        mix_spec = stft(mixture)
        source_spec = torch.stack([stft(source) for source in sources])
        masks = runner.masks(mix_spec)
        count = masks.shape[0]
        aligned_mix = mix_spec[:, :count]
        aligned_sources = source_spec[:, :, :count]
        row = {"track": track.name, "frames": count, "sdr_db": {}}
        for index, name in enumerate(("vocals", "drums", "bass", "other")):
            estimate = aligned_mix * masks[:, index].unsqueeze(0)
            reference = aligned_sources[index]
            values = frame_sdr(
                reference.transpose(0, 1), estimate.transpose(0, 1)
            )
            all_values[index].append(values)
            row["sdr_db"][name] = {
                "frames": int(values.size),
                "median": float(np.median(values)) if values.size else None,
                "mean": float(np.mean(values)) if values.size else None,
            }
        per_track.append(row)
        print(json.dumps(row), flush=True)
    aggregate = {}
    for index, name in enumerate(("vocals", "drums", "bass", "other")):
        values = np.concatenate(all_values[index]) if all_values[index] else np.empty(0)
        aggregate[name] = {
            "frames": int(values.size),
            "median": float(np.median(values)) if values.size else None,
            "mean": float(np.mean(values)) if values.size else None,
        }
    return {
        "model": str(model_path),
        "lookahead_frames": runner.lookahead,
        "metric": "frame-wise stereo complex-mask SDR; silent-reference frames omitted",
        "aggregate": aggregate,
        "tracks": per_track,
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("model", type=Path)
    parser.add_argument("--musdb", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    report = evaluate(args.model, args.musdb)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + "\n")


if __name__ == "__main__":
    main()
