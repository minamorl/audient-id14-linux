"""Render the owner-selected filter form, including the 300 Hz neutral band."""

from __future__ import annotations

import argparse
from pathlib import Path

import soundfile as sf
import torch

from .audio import HOP, SAMPLE_RATE, bass_protected_multiplier, istft, stft
from .data import read_audio
from .inference import StreamingOnnx


def render(model: Path, source: Path, output: Path, gains_db: list[float]) -> None:
    audio = read_audio(source)
    spectrum = stft(audio)
    runner = StreamingOnnx(model)
    masks = runner.masks(spectrum)
    count = masks.shape[0]
    gains = torch.pow(10.0, torch.tensor(gains_db) / 20.0).unsqueeze(0)
    multiplier = bass_protected_multiplier(masks, gains)
    remixed = spectrum[:, :count] * multiplier.unsqueeze(0)
    samples = 1024 + max(0, count - 1) * HOP
    result = istft(remixed, samples)
    output.parent.mkdir(parents=True, exist_ok=True)
    sf.write(output, result.T.numpy(), SAMPLE_RATE, subtype="FLOAT")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("model", type=Path)
    parser.add_argument("source", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--gains-db", type=float, nargs=4, default=(3.0, 0.0, 0.0, -3.0))
    args = parser.parse_args()
    render(args.model, args.source, args.output, list(args.gains_db))


if __name__ == "__main__":
    main()
