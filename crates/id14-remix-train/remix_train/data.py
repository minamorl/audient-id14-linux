"""Dataset discovery and continuous STFT sequence sampling."""

from __future__ import annotations

import json
import random
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable

import numpy as np
import soundfile as sf
import torch
from torch import Tensor
import torch.nn.functional as F
import yaml

from .audio import SAMPLE_RATE, STEMS, stft


@dataclass(frozen=True)
class Track:
    name: str
    mixture: Path | None
    stems: tuple[tuple[Path, ...], ...]
    split: str
    corpus: str


def _audio_files(path: Path) -> list[Path]:
    return sorted(p for p in path.rglob("*") if p.suffix.lower() in {".wav", ".flac"})


def discover_musdb(root: Path) -> list[Track]:
    tracks = []
    for split_dir in (root / "train", root / "test"):
        if not split_dir.is_dir():
            continue
        for song in sorted(p for p in split_dir.iterdir() if p.is_dir()):
            stems = tuple(((song / f"{name}.wav"),) for name in STEMS)
            mixture = song / "mixture.wav"
            if mixture.is_file() and all(paths[0].is_file() for paths in stems):
                tracks.append(Track(song.name, mixture, stems, split_dir.name, "musdb18-hq"))
    return tracks


def _classify(path: Path) -> int:
    text = " ".join(part.lower() for part in path.parts)
    if any(word in text for word in ("vocal", "voice", "choir", "singer")):
        return 0
    if any(word in text for word in ("drum", "percussion", "kick", "snare", "cymbal")):
        return 1
    if "bass" in text and not any(word in text for word in ("drum", "kick")):
        return 2
    return 3


def discover_multistem(root: Path, corpus: str, split: str = "train") -> list[Track]:
    """Discover MoisesDB or Slakh tracks without requiring their Python SDK."""
    tracks = []
    for directory in sorted(p for p in root.rglob("*") if p.is_dir()):
        files = _audio_files(directory)
        direct = [p for p in files if p.parent == directory]
        if len(files) < 2 or not direct:
            continue
        # Only accept a directory when it owns a mixture or multiple source files;
        # parent directories containing whole corpora are deliberately rejected.
        mix = next((p for p in direct if p.stem.lower() in {"mixture", "mix"}), None)
        source_files = [p for p in files if p != mix and "mixture" not in p.stem.lower()]
        if not source_files or (mix is None and len(direct) < 2):
            continue
        grouped = [[] for _ in STEMS]
        for source in source_files:
            grouped[_classify(source)].append(source)
        if sum(bool(group) for group in grouped) < 2:
            continue
        tracks.append(
            Track(
                directory.name,
                mix,
                tuple(tuple(group) for group in grouped),
                split,
                corpus,
            )
        )
    # Remove nested duplicates by retaining the deepest valid representation.
    names = {(track.name, tuple(path for group in track.stems for path in group)) for track in tracks}
    return [track for track in tracks if (track.name, tuple(p for g in track.stems for p in g)) in names]


def discover_moises(root: Path) -> list[Track]:
    """Read the official provider/track/data.json representation."""
    tracks = []
    for metadata in sorted(root.glob("*/*/data.json")):
        raw = json.loads(metadata.read_text())
        grouped = [[] for _ in STEMS]
        for stem in raw.get("stems", []):
            index = _classify(Path(str(stem.get("stemName", "other"))))
            stem_dir = metadata.parent / str(stem.get("stemName", ""))
            for source in stem.get("tracks", []):
                path = stem_dir / f"{source.get('id')}.{source.get('extension')}"
                if path.is_file():
                    grouped[index].append(path)
        if sum(bool(group) for group in grouped) >= 2:
            tracks.append(
                Track(
                    metadata.parent.name,
                    None,
                    tuple(tuple(group) for group in grouped),
                    "train",
                    "moisesdb",
                )
            )
    return tracks


def discover_slakh(root: Path) -> list[Track]:
    """Read Slakh2100 metadata so opaque S00 filenames keep their class."""
    tracks = []
    for metadata in sorted(root.glob("**/metadata.yaml")):
        raw = yaml.safe_load(metadata.read_text()) or {}
        grouped = [[] for _ in STEMS]
        for stem_id, details in (raw.get("stems") or {}).items():
            label = " ".join(
                str(details.get(key, ""))
                for key in ("inst_class", "midi_program_name", "plugin_name")
            )
            index = _classify(Path(label or "other"))
            candidates = list((metadata.parent / "stems").glob(f"{stem_id}.*"))
            grouped[index].extend(
                path for path in candidates if path.suffix in {".wav", ".flac"}
            )
        mixture = next(
            (
                path
                for path in (metadata.parent / name for name in ("mix.flac", "mix.wav"))
                if path.is_file()
            ),
            None,
        )
        if mixture and sum(bool(group) for group in grouped) >= 2:
            split = next(
                (part for part in metadata.parts if part in {"train", "validation", "test"}),
                "train",
            )
            tracks.append(
                Track(
                    metadata.parent.name,
                    mixture,
                    tuple(tuple(group) for group in grouped),
                    split,
                    "slakh2100",
                )
            )
    return tracks


def read_audio(path: Path) -> Tensor:
    audio, rate = sf.read(path, dtype="float32", always_2d=True)
    tensor = torch.from_numpy(audio.T.copy())
    if tensor.shape[0] == 1:
        tensor = tensor.repeat(2, 1)
    elif tensor.shape[0] > 2:
        tensor = tensor[:2]
    if rate != SAMPLE_RATE:
        length = round(tensor.shape[1] * SAMPLE_RATE / rate)
        tensor = F.interpolate(tensor.unsqueeze(0), size=length, mode="linear", align_corners=False)[0]
    return tensor


def load_track(track: Track) -> tuple[Tensor, Tensor]:
    grouped = []
    for paths in track.stems:
        signals = [read_audio(path) for path in paths]
        if signals:
            length = min(signal.shape[1] for signal in signals)
            grouped.append(sum(signal[:, :length] for signal in signals))
        else:
            grouped.append(None)
    available = [signal for signal in grouped if signal is not None]
    if not available:
        raise ValueError(f"{track.name}: no stems")
    length = min(signal.shape[1] for signal in available)
    stems = torch.stack(
        [signal[:, :length] if signal is not None else torch.zeros(2, length) for signal in grouped]
    )
    mixture = read_audio(track.mixture)[:, :length] if track.mixture else stems.sum(dim=0)
    length = min(length, mixture.shape[1])
    return mixture[:, :length], stems[:, :, :length]


class SequenceSampler:
    def __init__(self, tracks: list[Track], frames: int, seed: int = 1407):
        if not tracks:
            raise ValueError("no training tracks discovered")
        self.tracks = tracks
        self.frames = frames
        self.random = random.Random(seed)
        self.cache: dict[Path | str, tuple[Tensor, Tensor]] = {}

    def sample(self, batch: int) -> tuple[Tensor, Tensor]:
        mixtures, stems = [], []
        samples = 1024 + (self.frames - 1) * 512
        for _ in range(batch):
            track = self.random.choice(self.tracks)
            key = track.mixture or track.name
            if key not in self.cache:
                self.cache[key] = load_track(track)
                if len(self.cache) > 8:
                    self.cache.pop(next(iter(self.cache)))
            mixture, sources = self.cache[key]
            if mixture.shape[1] < samples:
                raise ValueError(f"{track.name}: shorter than {samples} samples")
            start = self.random.randrange(0, mixture.shape[1] - samples + 1)
            mix_clip = mixture[:, start : start + samples]
            source_clip = sources[:, :, start : start + samples]
            target_peak = 10.0 ** self.random.uniform(-2.0, 0.0)  # -40..0 dBFS
            scale = target_peak / float(mix_clip.abs().max().clamp_min(1e-5))
            mixtures.append(stft(mix_clip * scale))
            stems.append(
                torch.stack([stft(source * scale) for source in source_clip])
            )
        # mixture [B,2,T,F], stems [B,4,2,T,F]
        return torch.stack(mixtures), torch.stack(stems)


def write_manifest(path: Path, tracks: Iterable[Track]) -> None:
    path.write_text(
        json.dumps(
            [
                {
                    "name": track.name,
                    "corpus": track.corpus,
                    "split": track.split,
                    "mixture": str(track.mixture) if track.mixture else None,
                    "stems": [[str(path) for path in group] for group in track.stems],
                }
                for track in tracks
            ],
            indent=2,
        )
        + "\n"
    )
