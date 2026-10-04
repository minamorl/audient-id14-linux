"""Compare a rendered remix with the offline HTDemucs B_mild reference."""

from __future__ import annotations

import argparse
import json
from pathlib import Path

import numpy as np
import soundfile as sf


def load(path: Path) -> tuple[np.ndarray, int]:
    audio, rate = sf.read(path, dtype="float32", always_2d=True)
    return audio[:, :2], rate


def metrics(candidate: Path, reference: Path) -> dict:
    left, left_rate = load(candidate)
    right, right_rate = load(reference)
    if left_rate != right_rate:
        raise ValueError("sample rates differ")
    length = min(len(left), len(right))
    left, right = left[:length], right[:length]
    error = left - right
    reference_power = np.mean(np.square(right), axis=0)
    error_power = np.mean(np.square(error), axis=0)
    channel_sdr = 10 * np.log10(reference_power / np.maximum(error_power, 1e-12))
    mid_left, side_left = left.mean(axis=1), (left[:, 0] - left[:, 1]) * 0.5
    mid_right, side_right = right.mean(axis=1), (right[:, 0] - right[:, 1]) * 0.5
    eps = 1e-12
    return {
        "candidate": str(candidate),
        "reference": str(reference),
        "sample_rate": left_rate,
        "samples": length,
        "difference_rms_dbfs": float(20 * np.log10(np.sqrt(np.mean(error**2)) + eps)),
        "sdr_db": {"left": float(channel_sdr[0]), "right": float(channel_sdr[1])},
        "side_mid_db_difference": float(
            10
            * np.log10(
                (np.mean(side_left**2) / (np.mean(mid_left**2) + eps) + eps)
                / (np.mean(side_right**2) / (np.mean(mid_right**2) + eps) + eps)
            )
        ),
        "lr_correlation_difference": float(
            np.corrcoef(left.T)[0, 1] - np.corrcoef(right.T)[0, 1]
        ),
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("candidate", type=Path)
    parser.add_argument("reference", type=Path)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    result = metrics(args.candidate, args.reference)
    text = json.dumps(result, indent=2) + "\n"
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(text)
    print(text, end="")


if __name__ == "__main__":
    main()
