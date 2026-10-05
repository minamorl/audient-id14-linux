"""Compare the old per-frame training path with sequence batching on CPU."""

from __future__ import annotations

import argparse
import json
import statistics
import time

import torch

from .model import BINS, RemixNet


def repeated_steps(model, x, state):
    masks = []
    for frame in range(x.shape[1]):
        mask, state = model(x[:, frame], state)
        masks.append(mask)
    return torch.stack(masks, dim=1), state


def iteration(model, x, path):
    model.zero_grad(set_to_none=True)
    state = torch.zeros(x.shape[0], model.state_size)
    if path == "sequence":
        masks, state = model.forward_sequence(x, state)
    else:
        masks, state = repeated_steps(model, x, state)
    loss = masks.square().mean() + state.square().mean()
    loss.backward()


def measure(model, x, path, iterations):
    iteration(model, x, path)
    elapsed = []
    for _ in range(iterations):
        before = time.perf_counter()
        iteration(model, x, path)
        elapsed.append(time.perf_counter() - before)
    return {
        "median_seconds": statistics.median(elapsed),
        "minimum_seconds": min(elapsed),
        "samples_seconds": elapsed,
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--size", choices=("131k", "444k"), default="131k")
    parser.add_argument("--batches", type=int, nargs="+", default=(1, 2, 4, 8))
    parser.add_argument("--frames", type=int, nargs="+", default=(8, 32, 64, 128))
    parser.add_argument("--iterations", type=int, default=3)
    parser.add_argument("--threads", type=int, default=1)
    args = parser.parse_args()
    torch.set_num_threads(args.threads)
    torch.manual_seed(1407)
    rows = []
    for batch in args.batches:
        for frames in args.frames:
            model = RemixNet(args.size).train()
            x = torch.randn(batch, frames, 4, BINS)
            old = measure(model, x, "steps", args.iterations)
            sequence = measure(model, x, "sequence", args.iterations)
            rows.append(
                {
                    "frames": frames,
                    "batch": batch,
                    "legacy_step_loop_median_seconds": old["median_seconds"],
                    "sequence_median_seconds": sequence["median_seconds"],
                    "speedup": old["median_seconds"] / sequence["median_seconds"],
                    "sequence_microseconds_per_batch_frame": (
                        sequence["median_seconds"] * 1e6 / (batch * frames)
                    ),
                    "legacy_samples_seconds": old["samples_seconds"],
                    "sequence_samples_seconds": sequence["samples_seconds"],
                }
            )
    print(
        json.dumps(
            {
                "device": "cpu",
                "threads": args.threads,
                "size": args.size,
                "backward": True,
                "rows": rows,
            },
            indent=2,
        )
    )


if __name__ == "__main__":
    main()
