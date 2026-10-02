"""Train a causal four-frame log-spectral high-band predictor on licensed FMA.

The official FMA test tracks are touched only after validation checkpoint
selection. Diagnostic mode uses a small artist-disjoint subset and no test.
"""
import argparse
import hashlib
import json
import pathlib
import secrets
import struct
import subprocess
import time
from collections import defaultdict
from datetime import datetime, timezone

import numpy as np
import torch

ROOT = pathlib.Path(__file__).resolve().parents[1]
DATA = ROOT / "corpus/audio/fma/selected"
RATE = 48000
N = 1024
HOP = 256
LOW = 257
FIRST = 257
LAST = 427
BINS = LAST - FIRST
POOLED = 128
CONTEXT = 4
INPUT = POOLED * CONTEXT
HIDDEN = 256
WINDOW = np.hanning(N + 1)[:-1].astype(np.float32)
SOURCE = np.arange(FIRST, LAST) // 2
ALPHABET = "0123456789ABCDEFGHJKMNPQRSTVWXYZ"
number = (int(time.time() * 1000) << 80) | secrets.randbits(80)
TRACE_ID = "".join(ALPHABET[(number >> (5 * shift)) & 31] for shift in range(25, -1, -1))


def log(msg, **fields):
    print(json.dumps({"ts": datetime.now(timezone.utc).isoformat(), "level": "info",
                      "trace_id": TRACE_ID, "msg": msg, **fields}), flush=True)


def decode(track_id, start=0, seconds=None):
    command = ["ffmpeg", "-v", "error", "-ss", str(start), "-i", str(DATA / f"{track_id:06d}.mp3")]
    if seconds is not None:
        command += ["-t", str(seconds)]
    command += ["-ar", "48000", "-ac", "2", "-f", "f32le", "-"]
    result = subprocess.run(command, check=True, capture_output=True)
    data = np.frombuffer(result.stdout, dtype="<f4")
    return data.reshape(-1, 2)


def transform(audio):
    for channel in range(2):
        if len(audio) < N:
            continue
        frames = np.lib.stride_tricks.sliding_window_view(audio[:, channel], N)[::HOP]
        spectrum = np.fft.rfft(frames * WINDOW, axis=1)
        magnitude = np.abs(spectrum).astype(np.float32)
        low = magnitude[:, :LOW]
        scale = np.maximum(low.mean(axis=1), 0.05)
        spectral = np.log1p(low / scale[:, None])
        pooled = spectral[:, 1:257].reshape(-1, POOLED, 2).mean(axis=2)
        padded = np.pad(pooled, ((CONTEXT - 1, 0), (0, 0)))
        x = np.concatenate([padded[i:i + len(pooled)] for i in range(CONTEXT)], axis=1).astype(np.float32)
        high = magnitude[:, FIRST:LAST]
        target = np.log1p(high / scale[:, None]).astype(np.float32)
        source = (magnitude[:, SOURCE] / scale[:, None]).astype(np.float32)
        low_power = np.square(low).sum(axis=1)
        high_power = np.square(high).sum(axis=1)
        active = high_power > 0.001 * np.maximum(low_power, 1e-12)
        yield x, target, source, scale, low_power, high_power, active


class Network(torch.nn.Module):
    def __init__(self):
        super().__init__()
        self.first = torch.nn.Linear(INPUT, HIDDEN)
        self.second = torch.nn.Linear(HIDDEN, HIDDEN)
        self.third = torch.nn.Linear(HIDDEN, BINS)
        torch.nn.init.constant_(self.third.bias, -4.0)

    def forward(self, x):
        return torch.nn.functional.softplus(self.third(torch.relu(self.second(torch.relu(self.first(x))))))


def grouped(tracks):
    groups = defaultdict(list)
    for track in tracks:
        groups[track["genre"]].append(track)
    return {genre: sorted(group, key=lambda item: item["id"]) for genre, group in groups.items()}


def select(tracks, per_genre):
    groups = grouped(tracks)
    return [track for genre in sorted(groups) for track in groups[genre][:per_genre]]


def training_arrays(tracks, seconds):
    xs, ys, flags = [], [], []
    for index, track in enumerate(tracks):
        # The same offset is used across runs, and only training tracks are sampled.
        start = int(hashlib.sha256(str(track["id"]).encode()).hexdigest()[:4], 16) % max(1, 25 - seconds)
        for x, y, _, _, _, _, active in transform(decode(track["id"], start, seconds)):
            xs.append(x)
            ys.append(y)
            flags.append(active)
        if (index + 1) % 40 == 0:
            log("loaded_train_tracks", tracks=index + 1)
    return np.concatenate(xs), np.concatenate(ys), np.concatenate(flags)


def assess(net, tracks, device, copy_gain, seconds=None):
    totals = {name: {"n": 0, "model": 0.0, "zero": 0.0, "copy": 0.0,
                     "model_power": 0.0, "truth_power": 0.0, "copy_power": 0.0}
              for name in ("all", "active", "inactive")}
    per_track = []
    net.eval()
    with torch.inference_mode():
        for track in tracks:
            local = {key: 0.0 for key in ("model", "zero", "copy", "model_power", "truth_power")}
            local["n"] = 0
            for x, target, source, scale, low_power, high_power, active in transform(decode(track["id"], 0, seconds)):
                for start in range(0, len(x), 1024):
                    stop = start + 1024
                    pred = net(torch.from_numpy(x[start:stop]).to(device)).cpu().numpy()
                    pred[low_power[start:stop] < 1e-5] = 0.0
                    truth = target[start:stop]
                    copy = np.log1p(copy_gain * source[start:stop])
                    flags = active[start:stop]
                    squared = np.square(pred - truth).sum(axis=1)
                    zero = np.square(truth).sum(axis=1)
                    copied = np.square(copy - truth).sum(axis=1)
                    model_power = np.square(np.expm1(pred) * scale[start:stop, None]).sum(axis=1) / np.maximum(low_power[start:stop], 1e-4)
                    copy_power = np.square(copy_gain * source[start:stop] * scale[start:stop, None]).sum(axis=1) / np.maximum(low_power[start:stop], 1e-4)
                    truth_power = high_power[start:stop] / np.maximum(low_power[start:stop], 1e-4)
                    for name, mask in (("all", np.ones(len(flags), dtype=bool)), ("active", flags), ("inactive", ~flags)):
                        group = totals[name]
                        group["n"] += int(mask.sum())
                        group["model"] += float(squared[mask].sum(dtype=np.float64))
                        group["zero"] += float(zero[mask].sum(dtype=np.float64))
                        group["copy"] += float(copied[mask].sum(dtype=np.float64))
                        group["model_power"] += float(model_power[mask].sum(dtype=np.float64))
                        group["truth_power"] += float(truth_power[mask].sum(dtype=np.float64))
                        group["copy_power"] += float(copy_power[mask].sum(dtype=np.float64))
                    local["n"] += len(flags)
                    for key, values in (("model", squared), ("zero", zero), ("copy", copied),
                                        ("model_power", model_power), ("truth_power", truth_power)):
                        local[key] += float(values.sum(dtype=np.float64))
            if local["n"]:
                per_track.append({"id": track["id"], "genre": track["genre"], "frames": local["n"],
                                  "model_log_mse": local["model"] / (local["n"] * BINS),
                                  "zero_log_mse": local["zero"] / (local["n"] * BINS),
                                  "copy_log_mse": local["copy"] / (local["n"] * BINS),
                                  "model_vs_zero_percent": 100 * (1 - local["model"] / local["zero"]) if local["zero"] else None,
                                  "mean_model_high_low_power": local["model_power"] / local["n"],
                                  "mean_truth_high_low_power": local["truth_power"] / local["n"]})
    summary = {}
    for name, group in totals.items():
        n = max(1, group["n"])
        summary[name] = {"frames": group["n"],
                         "model_log_mse": group["model"] / (n * BINS),
                         "zero_log_mse": group["zero"] / (n * BINS),
                         "copy_log_mse": group["copy"] / (n * BINS),
                         "model_vs_zero_percent": 100 * (1 - group["model"] / group["zero"]) if group["zero"] else None,
                         "model_vs_copy_percent": 100 * (1 - group["model"] / group["copy"]) if group["copy"] else None,
                         "mean_model_high_low_power": group["model_power"] / n,
                         "mean_truth_high_low_power": group["truth_power"] / n,
                         "mean_copy_high_low_power": group["copy_power"] / n}
    return {"summary": summary, "per_track": per_track}


def choose_copy_gain(tracks, seconds):
    candidates = (0.01, 0.03, 0.1, 0.3)
    errors = {gain: 0.0 for gain in candidates}
    for track in tracks:
        for _, target, source, _, _, _, _ in transform(decode(track["id"], 0, seconds)):
            for gain in candidates:
                errors[gain] += float(np.square(np.log1p(gain * source) - target).sum(dtype=np.float64))
    return min(errors, key=errors.get), errors


def export(net, path):
    with path.open("wb") as output:
        output.write(b"ID14SR04")
        output.write(struct.pack("<IIII", INPUT, HIDDEN, BINS, CONTEXT))
        for layer in (net.first, net.second, net.third):
            for tensor in (layer.weight, layer.bias):
                output.write(tensor.detach().cpu().numpy().astype("<f4").tobytes())


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--diagnostic", action="store_true")
    parser.add_argument("--steps", type=int, default=4000)
    parser.add_argument("--batch", type=int, default=512)
    parser.add_argument("--seconds", type=int, default=8)
    args = parser.parse_args()
    np.random.seed(1407)
    torch.manual_seed(1407)
    torch.set_num_threads(8)
    device = "mps" if torch.backends.mps.is_available() else "cpu"
    manifest = json.loads((ROOT / "corpus/fma-selected.json").read_text())
    train = [item for item in manifest["tracks"] if item["split"] == "training"]
    validation = [item for item in manifest["tracks"] if item["split"] == "validation"]
    test = [item for item in manifest["tracks"] if item["split"] == "test"]
    if args.diagnostic:
        train = select(train, 20)
        validation = select(validation, 5)
    log("split", train=len(train), validation=len(validation), test=len(test), diagnostic=args.diagnostic, device=device)
    x_np, y_np, active_np = training_arrays(train, args.seconds)
    log("training_data", frames=len(x_np), active_frames=int(active_np.sum()),
        active_fraction=float(active_np.mean()), target_mean=float(y_np.mean()),
        target_p95=float(np.quantile(y_np, 0.95)))
    x = torch.from_numpy(x_np).to(device)
    y = torch.from_numpy(y_np).to(device)
    active = torch.from_numpy(active_np.astype(np.float32)).to(device)
    net = Network().to(device)
    optimizer = torch.optim.AdamW(net.parameters(), lr=0.0003)
    # Baseline copy gain is chosen only on validation tracks, before test use.
    copy_gain, copy_errors = choose_copy_gain(validation, args.seconds if args.diagnostic else None)
    log("copy_baseline", gain=copy_gain, validation_errors=copy_errors)
    best = float("inf")
    best_step = 0
    best_state = None
    best_validation = None
    every = 250 if args.diagnostic else 1000
    for step in range(args.steps):
        net.train()
        ids = torch.randint(len(x), (args.batch,), device=device)
        pred = net(x[ids])
        weights = 1.0 + 4.0 * active[ids, None]
        loss = (weights * torch.square(pred - y[ids])).mean()
        optimizer.zero_grad(set_to_none=True)
        loss.backward()
        optimizer.step()
        if (step + 1) % every == 0:
            result = assess(net, validation, device, copy_gain, seconds=args.seconds if args.diagnostic else None)
            current = result["summary"]["all"]["model_log_mse"]
            if current < best:
                best = current
                best_step = step + 1
                best_state = {key: value.detach().cpu().clone() for key, value in net.state_dict().items()}
                best_validation = result
            log("step", step=step + 1, loss=float(loss.item()), validation=result["summary"], best_step=best_step)
    net.load_state_dict(best_state)
    suffix = "diagnostic" if args.diagnostic else "trained"
    checkpoint = ROOT / f"evidence/fma-{suffix}-checkpoint.bin"
    export(net, checkpoint)
    heldout = None if args.diagnostic else assess(net, test, device, copy_gain)
    raw = checkpoint.read_bytes()
    report = {"source": manifest["source"], "archive_sha1": manifest["archive_sha1"],
              "diagnostic": args.diagnostic, "train_ids": [item["id"] for item in train],
              "validation_ids": [item["id"] for item in validation], "test_ids": [item["id"] for item in test],
              "train_frames": len(x_np), "train_active_frames": int(active_np.sum()),
              "steps": args.steps, "selected_step": best_step, "batch": args.batch, "seconds_per_train_track": args.seconds,
              "copy_gain": copy_gain, "copy_validation_errors": copy_errors,
              "validation": best_validation, "heldout": heldout, "device": device,
              "checkpoint_sha256": hashlib.sha256(raw).hexdigest(), "checkpoint_bytes": len(raw)}
    (ROOT / f"evidence/fma-{suffix}-training.json").write_text(json.dumps(report, indent=2) + "\n")
    log("finished", report_file=f"evidence/fma-{suffix}-training.json", frames=len(x_np), steps=args.steps,
        selected_step=best_step, validation=best_validation["summary"],
        heldout=heldout["summary"] if heldout else None, checkpoint_sha256=report["checkpoint_sha256"])


if __name__ == "__main__":
    main()
