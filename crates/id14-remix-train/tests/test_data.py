import json

import numpy as np
import soundfile as sf

from remix_train.data import (
    SequenceSampler,
    Track,
    build_audio_cache,
    discover_moises,
    discover_slakh,
    load_track,
)


def test_official_moises_layout(tmp_path):
    track = tmp_path / "provider" / "uuid"
    stems = []
    for index, name in enumerate(("vocals", "drums", "bass", "guitar")):
        directory = track / name
        directory.mkdir(parents=True)
        (directory / f"s{index}.wav").touch()
        stems.append(
            {
                "stemName": name,
                "tracks": [{"id": f"s{index}", "extension": "wav"}],
            }
        )
    (track / "data.json").write_text(json.dumps({"stems": stems}))
    found = discover_moises(tmp_path)
    assert len(found) == 1
    assert [len(group) for group in found[0].stems] == [1, 1, 1, 1]


def test_official_slakh_layout(tmp_path):
    track = tmp_path / "train" / "Track00001"
    stems = track / "stems"
    stems.mkdir(parents=True)
    for name in ("S00", "S01", "S02"):
        (stems / f"{name}.flac").touch()
    (track / "mix.flac").touch()
    (track / "metadata.yaml").write_text(
        """stems:
  S00:
    inst_class: Drums
  S01:
    inst_class: Bass
  S02:
    inst_class: Piano
"""
    )
    found = discover_slakh(tmp_path)
    assert len(found) == 1
    assert found[0].split == "train"
    assert [len(group) for group in found[0].stems] == [0, 1, 1, 1]


def _write_track(root, seconds=1.0, rate=24_000):
    song = root / "train" / "song"
    song.mkdir(parents=True)
    samples = int(seconds * rate)
    time = np.arange(samples, dtype=np.float32) / rate
    stems = []
    for index, name in enumerate(("vocals", "drums", "bass", "other")):
        mono = 0.03 * np.sin(2 * np.pi * (110 + 37 * index) * time)
        audio = np.stack((mono, mono * (0.9 - index * 0.1)), axis=1)
        sf.write(song / f"{name}.wav", audio, rate, subtype="FLOAT")
        stems.append(audio)
    sf.write(song / "mixture.wav", sum(stems), rate, subtype="FLOAT")
    return Track(
        "song",
        song / "mixture.wav",
        tuple(((song / f"{name}.wav"),) for name in ("vocals", "drums", "bass", "other")),
        "train",
        "synthetic",
    )


def test_audio_cache_is_memmapped_and_reused(tmp_path, monkeypatch):
    track = _write_track(tmp_path)
    expected_mixture, expected_stems = load_track(track)
    cache_dir = tmp_path / "cache"
    cached, first = build_audio_cache([track], cache_dir)
    mapped = np.load(cached[track].path, mmap_mode="r", allow_pickle=False)
    assert isinstance(mapped, np.memmap)
    assert mapped.dtype == np.float16
    assert mapped.shape == (5, 2, expected_mixture.shape[1])
    assert np.max(np.abs(mapped[0].astype(np.float32) - expected_mixture.numpy())) < 5e-4
    assert np.max(np.abs(mapped[1:].astype(np.float32) - expected_stems.numpy())) < 5e-4
    assert first["built"] == 1

    def unexpected_load(_track):
        raise AssertionError("completed cache must not decode the source WAV")

    monkeypatch.setattr("remix_train.data.load_track", unexpected_load)
    sampler = SequenceSampler([track], frames=4, cache_dir=cache_dir)
    mixture, sources = sampler.sample(1)
    assert mixture.shape == (1, 2, 4, 513)
    assert sources.shape == (1, 4, 2, 4, 513)
    assert sampler.cache_stats["built"] == 0
    assert sampler.cache_stats["reused"] == 1


def test_corrupt_cache_is_atomically_rebuilt(tmp_path):
    track = _write_track(tmp_path)
    cache_dir = tmp_path / "cache"
    cached, _ = build_audio_cache([track], cache_dir)
    final = cached[track].path
    final.write_bytes(b"interrupted")
    final.with_name(f".{final.name}.tmp-abandoned").write_bytes(b"partial")
    rebuilt, stats = build_audio_cache([track], cache_dir)
    mapped = np.load(rebuilt[track].path, mmap_mode="r", allow_pickle=False)
    assert mapped.shape[0:2] == (5, 2)
    assert stats["built"] == 1
    assert stats["reused"] == 0
