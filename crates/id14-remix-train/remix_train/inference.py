"""Frame-by-frame ONNX inference and delay alignment."""

from __future__ import annotations

from pathlib import Path

import numpy as np
import onnx
import onnxruntime as ort
import torch
from torch import Tensor

from .audio import pack_x


class StreamingOnnx:
    def __init__(self, path: Path, threads: int = 1):
        options = ort.SessionOptions()
        options.intra_op_num_threads = threads
        options.inter_op_num_threads = 1
        options.execution_mode = ort.ExecutionMode.ORT_SEQUENTIAL
        self.session = ort.InferenceSession(
            str(path), sess_options=options, providers=["CPUExecutionProvider"]
        )
        graph = onnx.load(path, load_external_data=False)
        metadata = {item.key: item.value for item in graph.metadata_props}
        self.lookahead = int(metadata["id14.lookahead_frames"])
        self.state_shape = tuple(self.session.get_inputs()[1].shape)

    def masks(self, spectrum: Tensor) -> Tensor:
        """Return masks aligned to target frames as [T-Q,4,F]."""
        if spectrum.ndim != 3 or spectrum.shape[0] != 2:
            raise ValueError("spectrum must be [2,frames,bins]")
        state = np.zeros(self.state_shape, dtype=np.float32)
        masks = []
        for frame in range(spectrum.shape[1]):
            x = pack_x(spectrum[:, frame].unsqueeze(0)).numpy()
            mask, state = self.session.run(None, {"x": x, "state": state})
            if frame >= self.lookahead:
                masks.append(torch.from_numpy(mask[0]))
        return torch.stack(masks) if masks else torch.empty(0, 4, 513)

