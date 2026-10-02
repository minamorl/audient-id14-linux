"""Test positive high-band magnitude prediction on stratified MusicNet tracks.

This is an experimental alternative to the complex predictor. The official test
recordings are untouched until checkpoint selection is finished.
"""
import argparse
import hashlib
import json
import pathlib
import random

import numpy as np
import torch

import train_musicnet_balanced as common

ROOT = pathlib.Path(__file__).resolve().parents[1]
BINS = common.BINS


class Network(torch.nn.Module):
    def __init__(self):
        super().__init__()
        self.first = torch.nn.Linear(common.LOW, common.HIDDEN)
        self.second = torch.nn.Linear(common.HIDDEN, common.HIDDEN)
        self.third = torch.nn.Linear(common.HIDDEN, BINS)

    def forward(self, x):
        return self.third(torch.relu(self.second(torch.relu(self.first(x))))).clamp_min(0)


def sample_features(paths):
    for path in paths:
        for audio in common.extracts(path, fractions=(0.15, 0.45, 0.75)):
            yield from common.block_features(audio)


def collect(paths):
    batches = list(sample_features(paths))
    return tuple(np.concatenate([batch[i] for batch in batches]) for i in range(3))


def evaluate(net, paths, device, sampled, gain=1.0):
    error = np.zeros(3, dtype=np.float64)
    baseline = np.zeros(3, dtype=np.float64)
    frames = np.zeros(3, dtype=np.int64)
    by_recording = []
    net.eval()
    with torch.no_grad():
        for path in paths:
            record_error = record_baseline = 0.0
            record_frames = 0
            chunks = common.extracts(path, fractions=(0.15, 0.45, 0.75)) if sampled else common.extracts(path)
            for audio in chunks:
                for x, y, active in common.block_features(audio):
                    for start in range(0, len(x), 1024):
                        pred = net(torch.from_numpy(x[start:start + 1024]).to(device)).cpu().numpy() * gain
                        truth = y[start:start + 1024]
                        active_batch = active[start:start + 1024]
                        # Predicted positive amplitude rides the doubled lower-octave phase.
                        # Complex STFT error therefore includes unpredicted imaginary part.
                        squared = (np.square(pred - truth[:, :BINS]) + np.square(truth[:, BINS:])).sum(axis=1)
                        zero = np.square(truth).sum(axis=1)
                        for index, mask in enumerate((np.ones(len(active_batch), dtype=bool), active_batch, ~active_batch)):
                            error[index] += squared[mask].sum(dtype=np.float64)
                            baseline[index] += zero[mask].sum(dtype=np.float64)
                            frames[index] += mask.sum()
                        record_error += squared.sum(dtype=np.float64)
                        record_baseline += zero.sum(dtype=np.float64)
                        record_frames += len(x[start:start + 1024])
            common.log("evaluated_recording", file=path.name, sampled=sampled)
            if not sampled:
                by_recording.append({"id": path.stem, "frames": record_frames,
                                     "improvement_percent": 100 * (1 - record_error / record_baseline) if record_baseline else None})
    result = {name: {"frames": int(frames[index]),
                     "model_mse": float(error[index] / max(1, frames[index] * 2 * BINS)),
                     "zero_mse": float(baseline[index] / max(1, frames[index] * 2 * BINS)),
                     "improvement_percent": float(100 * (1 - error[index] / baseline[index])) if baseline[index] else None}
              for index, name in enumerate(("all", "active", "inactive"))}
    if not sampled:
        result["by_recording"] = by_recording
    return result


def calibrate(net, paths, device):
    cross = power = 0.0
    net.eval()
    with torch.no_grad():
        for x, y, _ in sample_features(paths):
            for start in range(0, len(x), 1024):
                pred = net(torch.from_numpy(x[start:start + 1024]).to(device)).cpu().numpy().astype(np.float64)
                real = y[start:start + 1024, :BINS].astype(np.float64)
                cross += float(np.sum(pred * real))
                power += float(np.sum(pred * pred))
    return float(np.clip(cross / power, 0, 2)) if power else 0.0


def save(net, path, gain):
    import struct
    with path.open("wb") as output:
        output.write(b"ID14SR03")
        output.write(struct.pack("<III", common.LOW, common.HIDDEN, BINS))
        for layer in (net.first, net.second, net.third):
            for tensor in (layer.weight, layer.bias):
                values = tensor.detach().cpu().numpy().astype("<f4")
                output.write((values * gain if layer is net.third else values).astype("<f4").tobytes())


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--steps", type=int, default=5000)
    parser.add_argument("--batch", type=int, default=512)
    args = parser.parse_args()
    random.seed(1407)
    np.random.seed(1407)
    torch.manual_seed(1407)
    torch.set_num_threads(8)
    device = "mps" if torch.backends.mps.is_available() else "cpu"
    paths = sorted(common.DATA.rglob("*.wav"))
    official_train = [p for p in paths if "train_data" in p.parts]
    official_test = [p for p in paths if "test_data" in p.parts]
    if len(official_train) != 320 or len(official_test) != 10:
        raise RuntimeError("MusicNet official split is incomplete")
    # Activity statistics are computed only on official-train recordings.
    activity = []
    for path in official_train:
        total = active = 0
        for _, _, flags in sample_features([path]):
            total += len(flags)
            active += int(flags.sum())
        activity.append({"id": path.stem, "frames": total, "active_frames": active,
                         "active_fraction": active / total if total else 0.0})
    (ROOT / "corpus/musicnet-activity.json").write_text(json.dumps(activity, indent=2) + "\n")
    # Two activity strata keep calibration and validation sensitive to the
    # sparse target, while their recordings stay separate from training.
    ranked = sorted(official_train, key=lambda p: next(item["active_fraction"] for item in activity if item["id"] == p.stem), reverse=True)
    calibration = ranked[0:5] + ranked[160:165]
    validation = ranked[5:10] + ranked[165:170]
    excluded = set(calibration + validation)
    training = [p for p in official_train if p not in excluded]
    common.log("split", training=len(training), calibration=[p.stem for p in calibration],
               validation=[p.stem for p in validation], test=len(official_test), device=device)
    train_x, train_y, train_active = collect(training)
    common.log("training_data", frames=len(train_x), active_frames=int(train_active.sum()))
    x = torch.from_numpy(train_x).to(device)
    target = torch.from_numpy(train_y[:, :BINS].copy()).to(device)
    active_ids = torch.from_numpy(np.flatnonzero(train_active)).to(device)
    inactive_ids = torch.from_numpy(np.flatnonzero(~train_active)).to(device)
    net = Network().to(device)
    optimizer = torch.optim.AdamW(net.parameters(), lr=0.0004)
    best = float("inf")
    best_step = 0
    best_gain = 0.0
    best_validation = None
    best_state = None
    for step in range(args.steps):
        net.train()
        active_count = args.batch // 2
        ids = torch.cat((active_ids[torch.randint(len(active_ids), (active_count,), device=device)],
                         inactive_ids[torch.randint(len(inactive_ids), (args.batch - active_count,), device=device)]))
        pred = net(x[ids])
        loss = torch.nn.functional.mse_loss(pred, target[ids])
        optimizer.zero_grad(set_to_none=True)
        loss.backward()
        optimizer.step()
        if (step + 1) % 500 == 0:
            gain = calibrate(net, calibration, device)
            validation_result = evaluate(net, validation, device, sampled=True, gain=gain)
            current = validation_result["all"]["model_mse"]
            if gain > 0 and current < best:
                best, best_step, best_gain, best_validation = current, step + 1, gain, validation_result
                best_state = {key: value.detach().cpu().clone() for key, value in net.state_dict().items()}
            common.log("step", step=step + 1, loss=float(loss.item()), gain=gain,
                       validation=validation_result, best_step=best_step)
    if best_state is None:
        raise RuntimeError("no positive calibrated checkpoint")
    net.load_state_dict(best_state)
    checkpoint = ROOT / "evidence/musicnet-magnitude-checkpoint.bin"
    save(net, checkpoint, best_gain)
    heldout = evaluate(net, official_test, device, sampled=False, gain=best_gain)
    raw = checkpoint.read_bytes()
    report = {"corpus": "MusicNet 1.0", "license": "CC BY 4.0", "train_recordings": len(training),
              "calibration_ids": [p.stem for p in calibration], "validation_ids": [p.stem for p in validation],
              "test_ids": [p.stem for p in official_test], "train_frames": len(train_x),
              "train_active_frames": int(train_active.sum()), "steps": args.steps, "batch": args.batch,
              "selected_step": best_step, "calibrated_gain": best_gain, "device": device,
              "validation": best_validation, "heldout": heldout,
              "checkpoint_sha256": hashlib.sha256(raw).hexdigest(), "checkpoint_bytes": len(raw)}
    (ROOT / "evidence/musicnet-magnitude-training.json").write_text(json.dumps(report, indent=2) + "\n")
    common.log("finished", **report)


if __name__ == "__main__":
    main()
