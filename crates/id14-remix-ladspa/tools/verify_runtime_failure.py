"""Fresh-process check: absent native runtime is a visible neutral fallback."""
import argparse
import json
import os
from pathlib import Path
import numpy as np
from verify_so import Host, ROOT

parser=argparse.ArgumentParser()
parser.add_argument("--so",type=Path,required=True)
parser.add_argument("--library",type=Path,default=ROOT/".build/absent-onnxruntime.so")
args=parser.parse_args()
os.environ["ID14_ORT_LIBRARY"]=str(args.library)
with Host(args.so,expected_state=5) as h:
    x=np.random.default_rng(17).uniform(-.1,.1,(16384,2)).astype(np.float32)
    x[::17,0]=-0.0
    y=h.process(x)
    mismatch=int(np.count_nonzero(y[h.latency:].view(np.uint32)!=x[:-h.latency].view(np.uint32)))
    print(json.dumps({"check":"unavailable_native_runtime","library":str(args.library),"state":h.state,"bit_mismatches":mismatch}),flush=True)
    assert h.state==5 and mismatch==0
print("RUNTIME_FAILURE_VERIFICATION_OK",flush=True)
