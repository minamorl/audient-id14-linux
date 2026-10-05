"""Generate deterministic multi-minute MUSDB-shaped audio for I/O benchmarks."""

from __future__ import annotations

import argparse
from contextlib import ExitStack
from pathlib import Path

import numpy as np
import soundfile as sf

from .audio import STEMS


def write_track(directory: Path, track: int, seconds: int, rate: int) -> None:
    directory.mkdir(parents=True, exist_ok=True)
    block = rate
    frequencies = (220.0, 83.0, 55.0, 330.0)
    with ExitStack() as stack:
        stem_files = [
            stack.enter_context(
                sf.SoundFile(
                    directory / f"{name}.wav",
                    mode="w",
                    samplerate=rate,
                    channels=2,
                    subtype="PCM_16",
                )
            )
            for name in STEMS
        ]
        mixture_file = stack.enter_context(
            sf.SoundFile(
                directory / "mixture.wav",
                mode="w",
                samplerate=rate,
                channels=2,
                subtype="PCM_16",
            )
        )
        for start in range(0, seconds * rate, block):
            count = min(block, seconds * rate - start)
            time = (start + np.arange(count, dtype=np.float32)) / rate
            mixture = np.zeros((count, 2), dtype=np.float32)
            for index, (frequency, handle) in enumerate(zip(frequencies, stem_files)):
                phase = track * 0.37 + index * 0.19
                mono = 0.08 * np.sin(2 * np.pi * (frequency + track * 7) * time + phase)
                stereo = np.stack((mono, mono * (0.95 - index * 0.05)), axis=1)
                handle.write(stereo)
                mixture += stereo
            mixture_file.write(mixture)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("output", type=Path)
    parser.add_argument("--tracks", type=int, default=3)
    parser.add_argument("--seconds", type=int, default=120)
    parser.add_argument("--sample-rate", type=int, default=44_100)
    args = parser.parse_args()
    for track in range(args.tracks):
        write_track(
            args.output / "train" / f"Synthetic-{track + 1:02d}",
            track,
            args.seconds,
            args.sample_rate,
        )
    print(
        f"root={args.output} tracks={args.tracks} seconds_per_track={args.seconds} "
        f"sample_rate={args.sample_rate}"
    )


if __name__ == "__main__":
    main()
