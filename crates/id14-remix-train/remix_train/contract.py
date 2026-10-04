"""Executable remix-v1 ABI checks."""

from __future__ import annotations

import argparse
import json
from pathlib import Path

import numpy as np
import onnx
import onnxruntime as ort


def check(path: Path) -> dict:
    graph = onnx.load(path)
    onnx.checker.check_model(graph, full_check=True)
    metadata = {item.key: item.value for item in graph.metadata_props}
    if metadata.get("id14.contract") != "remix-v1":
        raise AssertionError("missing id14.contract=remix-v1")
    lookahead = int(metadata["id14.lookahead_frames"])
    if lookahead not in (0, 2, 4):
        raise AssertionError(f"invalid lookahead {lookahead}")
    if graph.opset_import[0].version < 17:
        raise AssertionError("opset must be at least 17")
    session = ort.InferenceSession(str(path), providers=["CPUExecutionProvider"])
    inputs = {item.name: item.shape for item in session.get_inputs()}
    outputs = {item.name: item.shape for item in session.get_outputs()}
    if inputs.get("x") != [1, 4, 513]:
        raise AssertionError(f"x shape {inputs.get('x')}")
    state_shape = inputs.get("state")
    if not state_shape or state_shape[0] != 1 or not isinstance(state_shape[1], int):
        raise AssertionError(f"state shape {state_shape}")
    if outputs.get("mask") != [1, 4, 513] or outputs.get("state_out") != state_shape:
        raise AssertionError(f"output shapes {outputs}")
    rng = np.random.default_rng(1407)
    state = np.zeros(state_shape, dtype=np.float32)
    max_sum_error = 0.0
    minimum = float("inf")
    for _ in range(16):
        x = rng.normal(0, 0.1, (1, 4, 513)).astype(np.float32)
        mask, state = session.run(None, {"x": x, "state": state})
        minimum = min(minimum, float(mask.min()))
        max_sum_error = max(max_sum_error, float(np.abs(mask.sum(axis=1) - 1).max()))
    if minimum < 0 or max_sum_error > 2e-6 or not np.isfinite(state).all():
        raise AssertionError(
            f"mask/state invariant failed: min={minimum} sum_error={max_sum_error}"
        )
    return {
        "path": str(path),
        "opset": graph.opset_import[0].version,
        "lookahead_frames": lookahead,
        "state_size": state_shape[1],
        "mask_min": minimum,
        "mask_max_sum_error": max_sum_error,
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("model", type=Path)
    print(json.dumps(check(parser.parse_args().model), indent=2))


if __name__ == "__main__":
    main()
