"""Adjudicate first-hop runtime differences using ONNX's reference evaluator."""
import argparse
import json
import numpy as np
from onnx.reference import ReferenceEvaluator

parser=argparse.ArgumentParser()
parser.add_argument("--model",required=True)
parser.add_argument("--vectors",required=True)
args=parser.parse_args()
with open(args.vectors) as stream:
    data=json.load(stream)
inputs={"x":np.array(data["x"],np.float32).reshape(1,4,513),
        "state":np.array(data["state"],np.float32).reshape(1,-1)}
reference=ReferenceEvaluator(args.model).run(["mask"],inputs)[0].ravel()
errors={name:float(np.max(np.abs(reference-np.array(data[name],np.float32)))) for name in ("ort","tract")}
print(json.dumps({"check":"onnx_reference_mask","max_absolute_error":errors}),flush=True)
assert errors["ort"]<1e-4
print("ONNX_REFERENCE_VERIFICATION_OK",flush=True)
