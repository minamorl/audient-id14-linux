"""Export a trained checkpoint as the single remix-v1 ONNX artifact."""

from __future__ import annotations

import argparse
from pathlib import Path

import onnx
import torch

from .model import BINS, RemixNet


def export(checkpoint: Path, output: Path, size: str, lookahead: int) -> None:
    saved = torch.load(checkpoint, map_location="cpu", weights_only=True)
    if saved.get("size") != size or int(saved.get("lookahead", -1)) != lookahead:
        raise ValueError("checkpoint size/lookahead does not match export arguments")
    model = RemixNet(size).eval()
    model.load_state_dict(saved["model"])
    x = torch.zeros(1, 4, BINS)
    state = torch.zeros(1, model.state_size)
    output.parent.mkdir(parents=True, exist_ok=True)
    torch.onnx.export(
        model,
        (x, state),
        output,
        input_names=("x", "state"),
        output_names=("mask", "state_out"),
        opset_version=17,
        do_constant_folding=True,
        dynamic_axes=None,
        dynamo=False,
    )
    graph = onnx.load(output)
    del graph.metadata_props[:]
    onnx.helper.set_model_props(
        graph,
        {
            "id14.contract": "remix-v1",
            "id14.lookahead_frames": str(lookahead),
            "id14.sample_rate": "48000",
            "id14.stft": "n_fft=1024,hop=512,sqrt-periodic-hann",
            "id14.training": "self-trained",
        },
    )
    onnx.checker.check_model(graph, full_check=True)
    onnx.save(graph, output)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("checkpoint", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--size", choices=("131k", "444k"), required=True)
    parser.add_argument("--lookahead", type=int, choices=(0, 2, 4), required=True)
    args = parser.parse_args()
    export(args.checkpoint, args.output, args.size, args.lookahead)


if __name__ == "__main__":
    main()
