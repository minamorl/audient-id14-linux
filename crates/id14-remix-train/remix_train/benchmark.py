"""Single-thread CPU hop latency measurement for an exported model."""

from __future__ import annotations

import argparse
import json
import time
from pathlib import Path

import numpy as np
import onnxruntime as ort


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("model", type=Path)
    parser.add_argument("--iterations", type=int, default=10_000)
    args = parser.parse_args()
    options = ort.SessionOptions()
    options.intra_op_num_threads = 1
    options.inter_op_num_threads = 1
    options.execution_mode = ort.ExecutionMode.ORT_SEQUENTIAL
    session = ort.InferenceSession(
        str(args.model), sess_options=options, providers=["CPUExecutionProvider"]
    )
    state_shape = session.get_inputs()[1].shape
    x = np.zeros((1, 4, 513), dtype=np.float32)
    state = np.zeros(state_shape, dtype=np.float32)
    for _ in range(200):
        _, state = session.run(None, {"x": x, "state": state})
    elapsed = np.empty(args.iterations, dtype=np.float64)
    for index in range(args.iterations):
        before = time.perf_counter_ns()
        _, state = session.run(None, {"x": x, "state": state})
        elapsed[index] = (time.perf_counter_ns() - before) / 1000.0
    print(
        json.dumps(
            {
                "model": str(args.model),
                "threads": 1,
                "iterations": args.iterations,
                "hop_period_us": 512 / 48_000 * 1e6,
                "p50_us": float(np.percentile(elapsed, 50)),
                "p99_us": float(np.percentile(elapsed, 99)),
            },
            indent=2,
        )
    )


if __name__ == "__main__":
    main()
