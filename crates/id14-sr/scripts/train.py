"""Train a causal-frame spectral high-frequency completion network at 48 kHz.

Requires Python 3.12, numpy 2.5, scipy 1.18, soundfile 0.14, torch 2.14.
Run download_corpus.py first. Entire-track train/test partition is in manifest.json.
"""
import argparse
import hashlib
import json
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
N = 1024
HOP = 256
LOW = 257
HIGH_START = 257
HIGH_END = 427
OUT = HIGH_END - HIGH_START
HIDDEN = 128
WINDOW = np.hanning(N + 1)[:-1].astype(np.float32)
ALPHABET = "0123456789ABCDEFGHJKMNPQRSTVWXYZ"


def ulid():
    value = (int(time.time() * 1000) << 80) | secrets.randbits(80)
    return "".join(ALPHABET[(value >> (5 * shift)) & 31] for shift in range(25, -1, -1))


TRACE_ID = ulid()


def log(msg, **fields):
    print(json.dumps({"ts": datetime.now(timezone.utc).isoformat(), "level": "info", "trace_id": TRACE_ID, "msg": msg, **fields}), flush=True)


def features(path):
    audio, rate = sf.read(path, dtype="float32", always_2d=True)
    if rate != 48000:
        from math import gcd
        factor = gcd(rate, 48000)
        audio = resample_poly(audio, 48000 // factor, rate // factor, axis=0).astype(np.float32)
    if audio.shape[1] > 2:
        audio = audio[:, :2]
    xs, ys = [], []
    for channel in range(audio.shape[1]):
        samples = audio[:, channel]
        for start in range(0, max(0, len(samples) - N), HOP * 256):
            piece = samples[start:start + N + HOP * 255]
            if len(piece) < N:
                continue
            frames = np.lib.stride_tricks.sliding_window_view(piece, N)[::HOP]
            mag = np.abs(np.fft.rfft(frames * WINDOW, axis=1)).astype(np.float32)
            scale = np.maximum(mag[:, :LOW].mean(axis=1, keepdims=True), 0.01)
            xs.append(np.log1p(mag[:, :LOW] / scale))
            ys.append(np.log1p(mag[:, HIGH_START:HIGH_END] / scale))
    return np.concatenate(xs), np.concatenate(ys)


class Network(torch.nn.Module):
    def __init__(self):
        super().__init__()
        self.first = torch.nn.Linear(LOW, HIDDEN)
        self.second = torch.nn.Linear(HIDDEN, HIDDEN)
        self.third = torch.nn.Linear(HIDDEN, OUT)

    def forward(self, x):
        return torch.nn.functional.softplus(self.third(torch.relu(self.second(torch.relu(self.first(x))))))


def export(net, path):
    with path.open("wb") as output:
        output.write(b"ID14SR01")
        output.write(struct.pack("<III", LOW, HIDDEN, OUT))
        for layer in (net.first, net.second, net.third):
            for tensor in (layer.weight, layer.bias):
                output.write(tensor.detach().cpu().numpy().astype("<f4").tobytes())


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--steps", type=int, default=4000)
    parser.add_argument("--batch", type=int, default=512)
    parser.add_argument("--seed", type=int, default=1407)
    args = parser.parse_args()
    random.seed(args.seed)
    np.random.seed(args.seed)
    torch.manual_seed(args.seed)
    torch.set_num_threads(8)
    device = "mps" if torch.backends.mps.is_available() else "cpu"
    manifest = json.loads((ROOT / "corpus/downloaded.json").read_text())
    split = {"train": [], "test": []}
    for entry in manifest["files"]:
        path = ROOT / "corpus/audio" / entry["title"].removeprefix("File:").replace(" ", "_")
        x, y = features(path)
        split[entry["split"]].append((x, y))
        log("features", split=entry["split"], file=path.name, frames=len(x))
    train_x = np.concatenate([p[0] for p in split["train"]])
    train_y = np.concatenate([p[1] for p in split["train"]])
    test_x = np.concatenate([p[0] for p in split["test"]])
    test_y = np.concatenate([p[1] for p in split["test"]])
    log("dataset", train_examples=len(train_x), test_examples=len(test_x), device=device)
    x = torch.from_numpy(train_x).to(device)
    y = torch.from_numpy(train_y).to(device)
    net = Network().to(device)
    optimizer = torch.optim.AdamW(net.parameters(), lr=0.0005)
    net.train()
    for step in range(args.steps):
        indices = torch.randint(len(x), (args.batch,), device=device)
        prediction = net(x[indices])
        loss = torch.nn.functional.mse_loss(prediction, y[indices])
        optimizer.zero_grad(set_to_none=True)
        loss.backward()
        optimizer.step()
        if (step + 1) % 250 == 0:
            log("step", step=step + 1, total_steps=args.steps, loss=loss.item())
    net.eval()
    errors = []
    baseline = []
    with torch.no_grad():
        for i in range(0, len(test_x), 512):
            batch_x = torch.from_numpy(test_x[i:i + 512]).to(device)
            batch_y = torch.from_numpy(test_y[i:i + 512]).to(device)
            result = net(batch_x)
            errors.append(float(torch.square(result - batch_y).sum().cpu()))
            baseline.append(float(torch.square(batch_y).sum().cpu()))
    mse = sum(errors) / (len(test_x) * OUT)
    zero_mse = sum(baseline) / (len(test_x) * OUT)
    checkpoint = ROOT / "model/checkpoint.bin"
    export(net, checkpoint)
    report = {
        "source": manifest["source"], "license": manifest["license"],
        "train_tracks": len(split["train"]), "test_tracks": len(split["test"]),
        "train_examples": len(train_x), "test_examples": len(test_x),
        "steps": args.steps, "batch": args.batch, "seed": args.seed,
        "device": device, "heldout_log_magnitude_mse": mse,
        "heldout_zero_highband_mse": zero_mse,
        "improvement_percent": 100 * (1 - mse / zero_mse),
        "checkpoint_sha256": hashlib.sha256(checkpoint.read_bytes()).hexdigest(),
        "checkpoint_bytes": checkpoint.stat().st_size,
    }
    (ROOT / "evidence/training.json").write_text(json.dumps(report, indent=2) + "\n")
    log("evaluation", **report)


if __name__ == "__main__":
    main()
