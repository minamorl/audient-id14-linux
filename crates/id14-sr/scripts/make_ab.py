"""Render attributed MusicNet full/12-kHz-cut/enhanced held-out A/B WAVs."""
import hashlib
import csv
import json
import math
import pathlib
import secrets
import subprocess
import time
from datetime import datetime, timezone

import numpy as np
import soundfile as sf
from scipy.signal import resample_poly

ROOT = pathlib.Path(__file__).resolve().parents[1]
DATA = ROOT / "corpus/audio/musicnet"
OUTPUT = ROOT / "evidence/ab"
RENDER = ROOT / "target/release/sr-render"
RATE = 48000
SECONDS = 12
OUTPUT.mkdir(parents=True, exist_ok=True)
ALPHABET = "0123456789ABCDEFGHJKMNPQRSTVWXYZ"
value = (int(time.time() * 1000) << 80) | secrets.randbits(80)
TRACE_ID = "".join(ALPHABET[(value >> (5 * shift)) & 31] for shift in range(25, -1, -1))


def log(msg, **fields):
    print(json.dumps({"ts": datetime.now(timezone.utc).isoformat(), "level": "info", "trace_id": TRACE_ID, "msg": msg, **fields}), flush=True)


def excerpt(path, fraction):
    with sf.SoundFile(path) as source:
        needed = source.samplerate * SECONDS
        source.seek(max(0, int((len(source) - needed) * fraction)))
        audio = source.read(needed, dtype="float32", always_2d=True)
        if source.samplerate != RATE:
            factor = math.gcd(source.samplerate, RATE)
            audio = resample_poly(audio, RATE // factor, source.samplerate // factor, axis=0).astype(np.float32)
        if audio.shape[1] == 1:
            audio = np.repeat(audio, 2, axis=1)
        return audio[:, :2]


def cut(audio):
    spectrum = np.fft.rfft(audio, axis=0)
    frequencies = np.fft.rfftfreq(len(audio), 1 / RATE)
    spectrum[frequencies > 12000] = 0
    return np.fft.irfft(spectrum, n=len(audio), axis=0).astype(np.float32)


def band_error(estimate, truth, minimum, maximum):
    frequencies = np.fft.rfftfreq(len(truth), 1 / RATE)
    bins = (frequencies >= minimum) & (frequencies <= maximum)
    difference = np.fft.rfft(estimate - truth, axis=0)[bins]
    return float(np.square(np.abs(difference)).mean())


report = json.loads((ROOT / "evidence/musicnet-training.json").read_text())
with (ROOT / "corpus/audio/musicnet_metadata.csv").open(newline="") as source:
    metadata = {row["id"]: row for row in csv.DictReader(source)}
paths = sorted(path for path in DATA.rglob("*.wav") if path.stem in report["test_ids"] and "test_data" in path.parts)
candidates = []
for path in paths:
    for fraction in (0.2, 0.5, 0.8):
        audio = excerpt(path, fraction)
        spectrum = np.fft.rfft(audio, axis=0)
        frequencies = np.fft.rfftfreq(len(audio), 1 / RATE)
        low = np.square(np.abs(spectrum[(frequencies >= 100) & (frequencies <= 12000)])).sum()
        high = np.square(np.abs(spectrum[(frequencies > 12000) & (frequencies <= 20000)])).sum()
        candidates.append((float(high / max(low, 1e-12)), path, fraction, audio))
candidates.sort(key=lambda item: item[0], reverse=True)
selected = []
used = set()
for item in candidates:
    if item[1].stem not in used:
        selected.append(item)
        used.add(item[1].stem)
    if len(selected) == 2:
        break

examples = []
for ratio, path, fraction, full in selected:
    base = OUTPUT / path.stem
    lowpass = cut(full)
    full_path = base.with_name(base.name + "-full.wav")
    low_path = base.with_name(base.name + "-lowpass.wav")
    enhanced_path = base.with_name(base.name + "-enhanced.wav")
    sf.write(full_path, full, RATE, subtype="FLOAT")
    sf.write(low_path, lowpass, RATE, subtype="FLOAT")
    subprocess.run([str(RENDER), str(low_path), str(enhanced_path)], check=True)
    enhanced, _ = sf.read(enhanced_path, dtype="float32", always_2d=True)
    item = {
        "recording_id": path.stem, "source": str(path.relative_to(DATA)), "fraction": fraction,
        "source_metadata": metadata.get(path.stem),
        "selection": "highest 12–20 kHz energy ratio among 0.2/0.5/0.8 excerpts of distinct official test recordings",
        "high_to_low_energy_ratio": ratio,
        "full": str(full_path.relative_to(ROOT)), "lowpass": str(low_path.relative_to(ROOT)),
        "enhanced": str(enhanced_path.relative_to(ROOT)),
        "highband_zero_mse": band_error(lowpass, full, 12000, 20000),
        "highband_enhanced_mse": band_error(enhanced, full, 12000, 20000),
        "lowband_preservation_mse": band_error(enhanced, lowpass, 0, 11000),
        "enhanced_peak": float(np.abs(enhanced).max()),
        "sha256": {name: hashlib.sha256(file.read_bytes()).hexdigest() for name, file in (("full", full_path), ("lowpass", low_path), ("enhanced", enhanced_path))},
    }
    examples.append(item)
    log("ab_example", **item)

(ROOT / "evidence/ab/manifest.json").write_text(json.dumps({
    "license": "MusicNet CC BY 4.0; John Thickstun, Zaid Harchaoui, and Sham M. Kakade, University of Washington MusicNet 1.0, https://zenodo.org/records/5120004; original recording providers in source_metadata; lowpass and enhanced are transformations",
    "examples": examples,
}, indent=2) + "\n")
