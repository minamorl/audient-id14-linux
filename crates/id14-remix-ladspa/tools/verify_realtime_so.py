"""Drive the actual trained model/plugin with absolute 256/48000-second deadlines."""
import argparse
import ctypes as C
import hashlib
import json
import time
from collections import Counter
from pathlib import Path
import numpy as np
from verify_so import Host, RATE, BLOCK

parser = argparse.ArgumentParser()
parser.add_argument("--so", type=Path, required=True)
parser.add_argument("--model", type=Path, required=True)
parser.add_argument("--seconds", type=float, default=30)
args = parser.parse_args()
data = args.model.read_bytes()
print(json.dumps({"model":str(args.model),"bytes":len(data),"sha256":hashlib.sha256(data).hexdigest()}),flush=True)
period = BLOCK / RATE
blocks = int(args.seconds / period)
warmup = int(1 / period)
t = np.arange((blocks + warmup) * BLOCK) / RATE
x = np.column_stack([sum(0.02*np.sin(2*np.pi*f*t+p) for f,p in [(170,0),(800,.3),(3100,.7)]),
                     sum(0.015*np.sin(2*np.pi*f*t+p) for f,p in [(170,.2),(800,.7),(3100,.4)])]).astype(np.float32)
with Host(args.so, model=args.model) as h:
    states, startup_states, callback_us, intervals, lateness = [], [], [], [], []
    start = time.perf_counter()
    previous = None
    for i in range(blocks+warmup):
        deadline = start + i*period
        remaining = deadline-time.perf_counter()
        if remaining>0:
            time.sleep(remaining)
        before = time.perf_counter()
        h.block(x[i*BLOCK:(i+1)*BLOCK],measure=False)
        after = time.perf_counter()
        if i>=warmup:
            states.append(h.state)
            callback_us.append((after-before)*1e6)
            intervals.append((before-previous)*1000)
            lateness.append(max(0,before-deadline)*1000)
        else:
            startup_states.append(h.state)
        previous = before
    report = {"check":"trained_model_realtime", "seconds":args.seconds,"blocks":blocks,
              "period_ms":period*1000,"states":dict(Counter(states)),
              "warmup_blocks":warmup,"warmup_states":dict(Counter(startup_states)),
              "overloaded_blocks":states.count(6),"latency_frames":h.latency,
              "callback_wall_us_p99":float(np.percentile(callback_us,99)),
              "callback_wall_us_max":max(callback_us),
              "interval_ms_p50":float(np.median(intervals)),"interval_ms_p99":float(np.percentile(intervals,99)),
              "deadline_lateness_ms_max":max(lateness)}
    print(json.dumps(report,sort_keys=True),flush=True)
    assert set(states)=={1}, report
    h.stop_worker()
    values = (C.c_uint64 * 8192)()
    h.lib.id14_remix_inference_times.argtypes = [C.c_void_p,C.POINTER(C.c_uint64),C.c_size_t]
    h.lib.id14_remix_inference_times.restype = C.c_size_t
    count = h.lib.id14_remix_inference_times(h.handle,values,8192)
    assert count >= blocks//2
    ms = np.array(values[:count],dtype=np.float64)/1e6
    print(json.dumps({"check":"plugin_worker_infer","samples":count,
                      "p50_ms":float(np.median(ms)),"p99_ms":float(np.percentile(ms,99)),"max_ms":float(ms.max())}),flush=True)
    assert np.percentile(ms,99) <= 5
print("REALTIME_SO_VERIFICATION_OK",flush=True)
