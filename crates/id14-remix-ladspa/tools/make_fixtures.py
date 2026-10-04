"""Own synthetic fixtures only; no trained/third-party model is shipped here."""
from pathlib import Path
import numpy as np
import onnx
from onnx import TensorProto, helper, numpy_helper

ROOT = Path(__file__).resolve().parents[1]
DEST = ROOT / "fixtures"
DEST.mkdir(exist_ok=True)

def fixture(name, mask, q=0, contract="remix-v1", stateful=False, reverse=False, x_bins=513, state_nan=False):
    info = helper.make_tensor_value_info
    inputs = [info("x", TensorProto.FLOAT, [1, 4, x_bins]), info("state", TensorProto.FLOAT, [1, 3])]
    outputs = [info("mask", TensorProto.FLOAT, [1, 4, 513]), info("state_out", TensorProto.FLOAT, [1, 3])]
    nodes = [helper.make_node("Constant", [], ["mask"], value=numpy_helper.from_array(mask.astype(np.float32).reshape(1, 4, 513)))]
    if state_nan:
        nodes.append(helper.make_node("Constant", [], ["state_out"], value=numpy_helper.from_array(np.full((1, 3), np.nan, np.float32))))
    elif stateful:
        nodes.extend([
            helper.make_node("Constant", [], ["one"], value=numpy_helper.from_array(np.ones((1, 3), np.float32))),
            helper.make_node("Add", ["state", "one"], ["state_out"]),
        ])
    else:
        nodes.append(helper.make_node("Identity", ["state"], ["state_out"]))
    if reverse:
        inputs.reverse()
        outputs.reverse()
    model = helper.make_model(helper.make_graph(nodes, name, inputs, outputs),
                              opset_imports=[helper.make_opsetid("", 13)], producer_name="id14-remix synthetic verification", ir_version=8)
    helper.set_model_props(model, {"id14.contract": contract, "id14.lookahead_frames": str(q)})
    onnx.checker.check_model(model, full_check=True)
    path = DEST / (name + ".onnx")
    onnx.save_model(model, path)
    print(f"fixture={path.relative_to(ROOT)} bytes={path.stat().st_size}")

voice = np.zeros((4, 513), np.float32)
voice[0] = 1
fixture("voice", voice)
fixture("uniform", np.full((4, 513), 0.25, np.float32))
bins = np.zeros((4, 513), np.float32)
bins[3] = 1
selected = (np.arange(513)*48000/1024 >= 500) & (np.arange(513)*48000/1024 <= 5000)
bins[0, selected] = 1
bins[3, selected] = 0
fixture("voice_bins", bins)
fixture("q3", voice, q=3)
fixture("stateful_reordered", voice, stateful=True, reverse=True)
fixture("bad_contract", voice, contract="other")
fixture("bad_q", voice, q=-1)
fixture("unsupported_q", voice, q=4)
fixture("bad_shape", voice, x_bins=512)
fixture("bad_sum", voice*0.5)
fixture("negative", voice-0.1)
fixture("nan_mask", voice*np.nan)
fixture("nan_state", voice, state_nan=True)

