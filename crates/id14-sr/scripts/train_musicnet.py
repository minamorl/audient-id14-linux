"""Train/evaluate a phase-aware 48 kHz MusicNet high-band predictor.

The official MusicNet test recordings stay wholly held out. Twenty official
train recordings are also reserved for checkpoint selection. Only 24 seconds
per remaining train recording are sampled at fixed positions; test metrics use
the full held-out recordings, processed in bounded blocks.
"""
import argparse
import hashlib
import json
import math
import pathlib
import random
import secrets
import struct
import time
from datetime import datetime, timezone

import numpy as np
import soundfile as sf
import torch
from scipy.signal import resample_poly

ROOT = pathlib.Path(__file__).resolve().parents[1]
DATA = ROOT / "corpus/audio/musicnet"
RATE = 48000
N = 1024
HOP = 256
LOW = 257
HIGH_START = 257
HIGH_END = 427
BINS = HIGH_END - HIGH_START
OUT = BINS * 2
HIDDEN = 192
WINDOW = np.hanning(N + 1)[:-1].astype(np.float32)
SOURCE_BINS = np.arange(HIGH_START, HIGH_END) // 2
ALPHABET = "0123456789ABCDEFGHJKMNPQRSTVWXYZ"
value = (int(time.time() * 1000) << 80) | secrets.randbits(80)
TRACE_ID = "".join(ALPHABET[(value >> (5 * shift)) & 31] for shift in range(25, -1, -1))


def log(msg, **fields):
    print(json.dumps({"ts": datetime.now(timezone.utc).isoformat(), "level": "info", "trace_id": TRACE_ID, "msg": msg, **fields}), flush=True)


def extracts(path, fractions=None, seconds=8.0):
    """Yield whole recording or fixed nonoverlapping excerpts, resampled to 48k."""
    with sf.SoundFile(path) as source:
        length = len(source)
        count = round(source.samplerate * seconds)
        starts = [0] if fractions is None else [int((length - count) * fraction) for fraction in fractions]
        for start in starts:
            source.seek(max(0, start))
            data = source.read(count if fractions is not None else -1, dtype="float32", always_2d=True)
            if len(data) < N:
                continue
            if source.samplerate != RATE:
                factor = math.gcd(source.samplerate, RATE)
                data = resample_poly(data, RATE // factor, source.samplerate // factor, axis=0).astype(np.float32)
            yield data[:, :2]


def block_features(audio):
    result = []
    for channel in range(audio.shape[1]):
        data = audio[:, channel]
        for start in range(0, len(data) - N + 1, HOP * 4096):
            piece = data[start:start + N + HOP * 4095]
            frames = np.lib.stride_tricks.sliding_window_view(piece, N)[::HOP]
            spectrum = np.fft.rfft(frames * WINDOW, axis=1).astype(np.complex64)
            magnitude = np.abs(spectrum)
            scale = np.maximum(magnitude[:, :LOW].mean(axis=1, keepdims=True), 0.05)
            x = np.log1p(magnitude[:, :LOW] / scale).astype(np.float32)
            reference = spectrum[:, SOURCE_BINS].copy()
            odd = np.arange(HIGH_START, HIGH_END) % 2 == 1
            reference[:, odd] = 0.5 * (reference[:, odd] + spectrum[:, SOURCE_BINS[odd] + 1])
            phase = np.where(np.abs(reference) > 1e-6, reference / np.maximum(np.abs(reference), 1e-6), 1 + 0j)
            basis = phase * phase
            target = spectrum[:, HIGH_START:HIGH_END] * np.conj(basis) / scale
            y = np.concatenate([target.real, target.imag], axis=1).astype(np.float32)
            low_energy = np.square(magnitude[:, :LOW]).sum(axis=1)
            high_energy = np.square(magnitude[:, HIGH_START:HIGH_END]).sum(axis=1)
            active = high_energy > 0.001 * np.maximum(low_energy, 1e-12)
            result.append((x, y, active))
    return result


class Network(torch.nn.Module):
    def __init__(self):
        super().__init__()
        self.first = torch.nn.Linear(LOW, HIDDEN)
        self.second = torch.nn.Linear(HIDDEN, HIDDEN)
        self.third = torch.nn.Linear(HIDDEN, OUT)

    def forward(self, x):
        return self.third(torch.relu(self.second(torch.relu(self.first(x)))))


def export(net, path):
    with path.open("wb") as output:
        output.write(b"ID14SR02")
        output.write(struct.pack("<III", LOW, HIDDEN, OUT))
        for layer in (net.first, net.second, net.third):
            for tensor in (layer.weight, layer.bias):
                output.write(tensor.detach().cpu().numpy().astype("<f4").tobytes())


def evaluate(net, paths, device, sampled):
    numerators = np.zeros(3, dtype=np.float64)
    baselines = np.zeros(3, dtype=np.float64)
    counts = np.zeros(3, dtype=np.int64)
    by_recording = []
    net.eval()
    with torch.no_grad():
        for path in paths:
            track_error = 0.0
            track_baseline = 0.0
            track_frames = 0
            if sampled:
                chunks = extracts(path, fractions=(0.15, 0.45, 0.75))
            else:
                # Read the entire recording once, then bound FFT/evaluation memory.
                chunks = extracts(path)
            for audio in chunks:
                for x, y, active in block_features(audio):
                    for begin in range(0, len(x), 1024):
                        batch_x = torch.from_numpy(x[begin:begin + 1024]).to(device)
                        prediction = net(batch_x)
                        prediction = prediction.cpu().numpy()
                        truth = y[begin:begin + 1024]
                        flags = active[begin:begin + 1024]
                        squared_error = np.square(prediction - truth).sum(axis=1)
                        squared_zero = np.square(truth).sum(axis=1)
                        for index, mask in enumerate((np.ones(len(flags), dtype=bool), flags, ~flags)):
                            numerators[index] += squared_error[mask].sum()
                            baselines[index] += squared_zero[mask].sum()
                            counts[index] += mask.sum()
                        track_error += squared_error.sum()
                        track_baseline += squared_zero.sum()
                        track_frames += len(flags)
            log("evaluated_recording", file=path.name, sampled=sampled)
            if not sampled:
                by_recording.append({"id": path.stem, "frames": int(track_frames),
                                     "model_mse": float(track_error / max(1, track_frames * OUT)),
                                     "zero_mse": float(track_baseline / max(1, track_frames * OUT)),
                                     "improvement_percent": float(100 * (1 - track_error / track_baseline)) if track_baseline else None})
    names = ("all", "active", "inactive")
    result = {name: {
        "frames": int(counts[index]),
        "model_mse": float(numerators[index] / max(1, counts[index] * OUT)),
        "zero_mse": float(baselines[index] / max(1, counts[index] * OUT)),
        "improvement_percent": float(100 * (1 - numerators[index] / baselines[index])) if baselines[index] else None,
    } for index, name in enumerate(names)}
    if not sampled:
        result["by_recording"] = by_recording
    return result


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--steps", type=int, default=5000)
    parser.add_argument("--batch", type=int, default=512)
    parser.add_argument("--seed", type=int, default=1407)
    args = parser.parse_args()
    if args.steps < 500 or args.steps % 500:
        parser.error("--steps must be a positive multiple of 500")
    random.seed(args.seed)
    np.random.seed(args.seed)
    torch.manual_seed(args.seed)
    torch.set_num_threads(8)
    device = "mps" if torch.backends.mps.is_available() else "cpu"
    all_paths = sorted(DATA.rglob("*.wav"))
    official_train = [p for p in all_paths if "train_data" in p.parts]
    official_test = [p for p in all_paths if "test_data" in p.parts]
    if len(official_train) < 300 or len(official_test) < 10:
        raise RuntimeError(f"incomplete MusicNet extraction: train={len(official_train)} test={len(official_test)}")
    ordered = sorted(official_train, key=lambda p: hashlib.sha256(p.name.encode()).hexdigest())
    validation = ordered[:20]
    training = ordered[20:]
    log("split", train_recordings=len(training), validation_recordings=len(validation), test_recordings=len(official_test), device=device)
    features, targets, active_flags = [], [], []
    for path in training:
        for audio in extracts(path, fractions=(0.15, 0.45, 0.75)):
            for x, y, active in block_features(audio):
                features.append(x)
                targets.append(y)
                active_flags.append(active)
    train_x = np.concatenate(features)
    train_y = np.concatenate(targets)
    train_active = np.concatenate(active_flags)
    log("training_data", frames=len(train_x), active_frames=int(train_active.sum()), active_fraction=float(train_active.mean()))
    del features, targets, active_flags
    x = torch.from_numpy(train_x).to(device)
    y = torch.from_numpy(train_y).to(device)
    net = Network().to(device)
    optimizer = torch.optim.AdamW(net.parameters(), lr=0.0004)
    best_mse = float("inf")
    best_step = 0
    best_validation = None
    checkpoint = ROOT / "model/checkpoint.bin"
    for step in range(args.steps):
        net.train()
        indices = torch.randint(len(x), (args.batch,), device=device)
        prediction = net(x[indices])
        loss = torch.nn.functional.mse_loss(prediction, y[indices])
        optimizer.zero_grad(set_to_none=True)
        loss.backward()
        optimizer.step()
        if (step + 1) % 500 == 0:
            validation_result = evaluate(net, validation, device, sampled=True)
            current = validation_result["all"]["model_mse"]
            if current < best_mse:
                best_mse = current
                best_step = step + 1
                best_validation = validation_result
                export(net, checkpoint)
            log("step", step=step + 1, loss=float(loss.item()), validation=validation_result, best_step=best_step)
    # Restore selected weights before running the official held-out evaluation.
    net = Network().to(device)
    with checkpoint.open("rb") as source:
        raw = source.read()
    assert raw[:8] == b"ID14SR02"
    offset = 20
    with torch.no_grad():
        for layer in (net.first, net.second, net.third):
            for tensor in (layer.weight, layer.bias):
                count = tensor.numel()
                values = np.frombuffer(raw, dtype="<f4", count=count, offset=offset).copy().reshape(tensor.shape)
                tensor.copy_(torch.from_numpy(values).to(device))
                offset += count * 4
    heldout = evaluate(net, official_test, device, sampled=False)
    report = {
        "corpus": "MusicNet 1.0", "record": "https://zenodo.org/records/5120004", "license": "CC BY 4.0",
        "train_recordings": len(training), "validation_recordings": len(validation), "test_recordings": len(official_test),
        "train_excerpt_fractions": [0.15, 0.45, 0.75], "train_excerpt_seconds": 8,
        "active_definition": "frame high-band (12–20 kHz) squared magnitude exceeds 0.001 times low-band (0–12 kHz) squared magnitude",
        "validation_ids": [p.stem for p in validation],
        "train_frames": len(train_x), "train_active_frames": int(train_active.sum()),
        "steps": args.steps, "selected_step": best_step, "batch": args.batch, "seed": args.seed, "device": device,
        "validation": best_validation, "heldout": heldout,
        "checkpoint_sha256": hashlib.sha256(raw).hexdigest(), "checkpoint_bytes": len(raw),
        "test_ids": [p.stem for p in official_test],
    }
    (ROOT / "evidence/musicnet-training.json").write_text(json.dumps(report, indent=2) + "\n")
    log("finished", **report)


if __name__ == "__main__":
    main()
