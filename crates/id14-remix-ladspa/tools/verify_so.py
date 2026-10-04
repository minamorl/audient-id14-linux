"""An independent LADSPA host: load the actual cdylib and exercise its ABI."""
import argparse
import ctypes as C
import json
import os
from pathlib import Path
import time
import numpy as np
from scipy.signal import resample_poly

ROOT = Path(__file__).resolve().parents[1]
RATE, BLOCK = 48000, 256
F32P = C.POINTER(C.c_float)
class Hint(C.Structure):
    _fields_ = [("descriptor", C.c_int), ("lower", C.c_float), ("upper", C.c_float)]
class Descriptor(C.Structure):
    pass
Instantiate = C.CFUNCTYPE(C.c_void_p, C.POINTER(Descriptor), C.c_ulong)
Connect = C.CFUNCTYPE(None, C.c_void_p, C.c_ulong, F32P)
Lifecycle = C.CFUNCTYPE(None, C.c_void_p)
Run = C.CFUNCTYPE(None, C.c_void_p, C.c_ulong)
Gain = C.CFUNCTYPE(None, C.c_void_p, C.c_float)
Descriptor._fields_ = [
    ("unique_id", C.c_ulong), ("label", C.c_char_p), ("properties", C.c_int),
    ("name", C.c_char_p), ("maker", C.c_char_p), ("copyright", C.c_char_p),
    ("port_count", C.c_ulong), ("types", C.POINTER(C.c_int)),
    ("names", C.POINTER(C.c_char_p)), ("hints", C.POINTER(Hint)), ("data", C.c_void_p),
    ("instantiate", Instantiate), ("connect", Connect), ("activate", Lifecycle),
    ("run", Run), ("run_adding", Run), ("adding_gain", Gain),
    ("deactivate", Lifecycle), ("cleanup", Lifecycle),
]

class Host:
    def __init__(self, so, fixture="voice", amounts=(3,0,0,-3), enabled=1, inplace=False, rate=RATE):
        self.lib = C.CDLL(str(so))
        self.lib.ladspa_descriptor.argtypes = [C.c_ulong]
        self.lib.ladspa_descriptor.restype = C.POINTER(Descriptor)
        self.ptr = self.lib.ladspa_descriptor(0)
        self.d = self.ptr.contents
        assert self.d.label == b"id14_remix_stereo" and not self.lib.ladspa_descriptor(1)
        assert self.d.port_count == 11
        assert [self.d.names[i].decode() for i in range(11)] == ["Input L","Input R","Output L","Output R","Vocals","Drums","Bass","Other","Enabled","State","latency"]
        assert [self.d.types[i] for i in range(11)] == [9,9,10,10,5,5,5,5,5,6,6]
        assert [self.d.hints[i].lower for i in range(4,8)] == [-6]*4
        assert [self.d.hints[i].upper for i in range(4,8)] == [6]*4
        os.environ["ID14_REMIX_MODEL"] = str(ROOT/"fixtures"/(fixture+".onnx"))
        self.handle = self.d.instantiate(self.ptr, rate)
        assert self.handle
        self.inputs = [np.zeros(BLOCK,np.float32) for _ in range(2)]
        self.outputs = self.inputs if inplace else [np.zeros(BLOCK,np.float32) for _ in range(2)]
        self.controls = [C.c_float(v) for v in (*amounts,enabled,0,0)]
        for i, array in enumerate(self.inputs+self.outputs):
            self.d.connect(self.handle,i,array.ctypes.data_as(F32P))
        for i, value in enumerate(self.controls):
            self.d.connect(self.handle,i+4,C.pointer(value))
        self.d.activate(self.handle)
        self.wall_us, self.cpu_us, self.states = [], [], []
        self.closed = False
        deadline = time.monotonic()+10
        expected = 8 if rate != RATE else (4 if fixture=="missing" else 5 if fixture.startswith(("bad_","negative","nan_","unsupported_")) else 1)
        while time.monotonic()<deadline:
            self.block(np.zeros((BLOCK,2),np.float32), measure=False)
            if self.state in (expected,2,3):
                # OFF/zero override model state; allow the loader to finish independently.
                if self.state in (2,3): time.sleep(0.15)
                break
            time.sleep(0.005)
        else:
            raise AssertionError(("model startup", fixture, self.state, expected))
        self.latency = int(self.controls[6].value)
        assert self.latency > 0

    @property
    def state(self): return int(self.controls[5].value)

    def block(self, data, measure=True):
        n = len(data)
        for ch in range(2): self.inputs[ch][:n] = data[:,ch]
        wall = time.perf_counter_ns()
        cpu = time.thread_time_ns()
        self.d.run(self.handle,n)
        cpu = (time.thread_time_ns()-cpu)/1000
        wall = (time.perf_counter_ns()-wall)/1000
        if measure and n == BLOCK:
            self.wall_us.append(wall)
            self.cpu_us.append(cpu)
            self.states.append(self.state)
        return np.column_stack([y[:n] for y in self.outputs])

    def process(self, data, pace=True):
        result=[]
        for start in range(0,len(data),BLOCK):
            result.append(self.block(data[start:start+BLOCK]))
            if pace: time.sleep(0.0005)
        return np.concatenate(result)

    def stop_worker(self):
        self.lib.id14_remix_stop_worker.argtypes=[C.c_void_p]
        self.lib.id14_remix_stop_worker(self.handle)

    def close(self):
        if not self.closed:
            self.d.deactivate(self.handle)
            self.d.cleanup(self.handle)
            self.closed=True
    def __enter__(self): return self
    def __exit__(self,*args): self.close()

def tone(hz, amplitude=0.05, seconds=2):
    t=np.arange(int(RATE*seconds))/RATE
    x=(amplitude*np.sin(2*np.pi*hz*t)).astype(np.float32)
    return np.column_stack([x,x*np.float32(0.375)])

def phasor(x,hz):
    return 2*np.dot(np.asarray(x,np.float64),np.exp(-2j*np.pi*hz*np.arange(len(x))/RATE))/len(x)

def peak(x):
    oversampled=resample_poly(np.asarray(x,np.float64),32,1,axis=0,window=("kaiser",10.0))
    return float(np.max(np.abs(oversampled[2048:-2048])))

def main():
    parser=argparse.ArgumentParser()
    parser.add_argument("--so",type=Path,required=True)
    parser.add_argument("--legacy-so",type=Path)
    parser.add_argument("--output",type=Path,default=ROOT/"validation"/"so-results.json")
    args=parser.parse_args()
    records=[]
    def record(check,**data):
        value={"check":check,**data}
        records.append(value)
        print(json.dumps(value,sort_keys=True),flush=True)

    rng=np.random.default_rng(914)
    noise=(rng.uniform(-0.1,0.1,(RATE,2))).astype(np.float32)
    noise[::17,0]=-0.0
    noise[::17,1]=np.array([1],np.uint32).view(np.float32)[0]
    for mode in ["missing","off","zero","inplace"]:
        with Host(args.so,fixture="missing" if mode=="missing" else "voice", enabled=int(mode not in ("off","inplace")),amounts=(0,0,0,0) if mode=="zero" else (3,0,0,-3), inplace=mode=="inplace") as h:
            y=h.process(noise)
            count=int(np.count_nonzero(y[h.latency:].view(np.uint32)!=noise[:-h.latency].view(np.uint32)))
            lags=np.arange(h.latency-32,h.latency+33)
            scores=[np.dot(noise[:8192,0].astype(float),y[lag:lag+8192,0].astype(float)) for lag in lags]
            measured=int(lags[np.argmax(scores)])
            record("transparency_and_delay",mode=mode,bit_mismatches=count,reported_frames=h.latency,measured_frames=measured,sr_inclusive_ms=(h.latency+1024)/48,state=h.state)
            assert count==0 and measured==h.latency
            assert h.latency+1024 <= 4800

    for fixture in ["bad_contract","bad_q","unsupported_q","bad_shape","bad_sum","negative","nan_mask","nan_state"]:
        with Host(args.so,fixture=fixture) as h:
            y=h.process(noise[:16384])
            differences=int(np.count_nonzero(y[h.latency:].view(np.uint32)!=noise[:16384-h.latency].view(np.uint32)))
            record("invalid_model_neutral",fixture=fixture,state=h.state,bit_mismatches=differences)
            assert differences==0 and h.state==5

    with Host(args.so,rate=44100) as h:
        y=h.process(noise[:16384])
        assert np.array_equal(y[h.latency:].view(np.uint32),noise[:16384-h.latency].view(np.uint32))
        record("unsupported_rate",state=h.state,bit_mismatches=0)
        assert h.state==8

    with Host(args.so,enabled=0) as h:
        h.process(noise[:8192])
        h.d.deactivate(h.handle)
        h.d.activate(h.handle)
        y=h.process(noise[:16384])
        assert np.count_nonzero(y[:h.latency])==0
        assert np.array_equal(y[h.latency:].view(np.uint32),noise[:16384-h.latency].view(np.uint32))
        record("reactivation",history_reset=True,bit_mismatches=0)

    for event in ["stop_worker","off","zero","on"]:
        n=np.arange(RATE*2)
        phase=2*np.pi*3000*n/RATE
        x=(0.1*np.column_stack([np.sin(phase),np.cos(phase)])).astype(np.float32)
        with Host(args.so,enabled=int(event!="on")) as h:
            first=h.process(x[:RATE])
            if event=="stop_worker": h.stop_worker()
            elif event=="off": h.controls[4].value=0
            elif event=="on": h.controls[4].value=1
            else:
                for control in h.controls[:4]: control.value=0
            y=np.concatenate([first,h.process(x[RATE:])])
            envelope=np.linalg.norm(y[RATE-1024:RATE+8192].astype(float),axis=1)
            step=float(np.max(np.abs(np.diff(envelope))))
            minimum=float(envelope.min())
            if event!="on":
                equal=np.all(y[RATE:RATE+8192].view(np.uint32)==x[RATE-h.latency:RATE+8192-h.latency].view(np.uint32),axis=1)
                counts=np.convolve(equal.astype(int),np.ones(1024,dtype=int),mode="valid")
                indices=np.flatnonzero(counts==1024)
                assert len(indices)
                settle=float(indices[0]/48)
                assert settle<=100
                assert np.array_equal(y[RATE+8192:].view(np.uint32),x[RATE+8192-h.latency:-h.latency].view(np.uint32))
            else:
                target=np.median(envelope[-1024:])
                indices=np.flatnonzero(np.abs(envelope[1024:]-target)<0.0001)
                assert len(indices)
                settle=float(indices[0]/48)
                assert target>0.13 and settle<=100
            record("continuity",event=event,max_envelope_sample_step=step,min_amplitude=minimum,settle_ms=settle,state=h.state)
            assert step<0.0005 and minimum>=0.099
            if event=="stop_worker": assert h.state==6

    for hz in [50,100,200,299,300]:
        x=tone(hz,0.1)
        with Host(args.so) as h:
            y=h.process(x)
            start=h.latency+RATE//2
            error=y[start:start+RATE]-x[start-h.latency:start+RATE-h.latency]
            relative_db=20*np.log10(max(abs(phasor(error[:,0],hz))/0.1,1e-30))
            rms=float(np.sqrt(np.mean(error[:,0].astype(float)**2)))
            record("bass",hz=hz,difference_relative_db=float(relative_db),error_rms_dbfs=float(20*np.log10(max(rms,1e-30))))
            assert relative_db<=-60 and rms<=0.001

    t=np.arange(RATE*2)/RATE
    left=sum(0.025*np.sin(2*np.pi*f*t+p) for f,p in [(500,0.1),(1000,0.3),(3000,0.7)])
    right=sum(0.025*r*np.sin(2*np.pi*f*t+p+offset) for f,p,r,offset in [(500,0.1,0.3,0.0),(1000,0.3,0.8,0.4),(3000,0.7,0.5,-0.7)])
    x=np.column_stack([left,right]).astype(np.float32)
    for fixture in ["voice","uniform","voice_bins","q3"]:
        with Host(args.so,fixture=fixture) as h:
            y=h.process(x)
            start=h.latency+RATE//2
            for hz in [500,1000,3000]:
                xr=phasor(x[start-h.latency:start+RATE-h.latency,1],hz)/phasor(x[start-h.latency:start+RATE-h.latency,0],hz)
                yr=phasor(y[start:start+RATE,1],hz)/phasor(y[start:start+RATE,0],hz)
                error=float(abs(yr-xr))
                # Mixed tones also expose finite-window cross-bin leakage. Keep
                # its raw error; the isolated-bin check below has the strict gate.
                record("stereo_multitone",fixture=fixture,hz=hz,complex_lr_ratio_error=error)
                assert error<1e-4
            record("callback_256",fixture=fixture,thread_cpu_us_p50=float(np.median(h.cpu_us)),thread_cpu_us_p99=float(np.percentile(h.cpu_us,99)),thread_cpu_us_max=max(h.cpu_us),wall_us_p99=float(np.percentile(h.wall_us,99)),wall_us_max=max(h.wall_us),overload_callbacks=h.states.count(6))
            assert np.percentile(h.cpu_us,99)<BLOCK/RATE*1e6

    for hz in [500,1000,3000]:
        phase=2*np.pi*hz*t
        x=np.column_stack([0.025*np.sin(phase),0.02*np.sin(phase+0.4)]).astype(np.float32)
        with Host(args.so,fixture="voice_bins") as h:
            y=h.process(x)
            start=h.latency+RATE//2
            # 49152 samples spans whole input and hop periods for these tones.
            a=x[start-h.latency:start+49152-h.latency]
            b=y[start:start+49152]
            error=float(abs(phasor(b[:,1],hz)/phasor(b[:,0],hz)-phasor(a[:,1],hz)/phasor(a[:,0],hz)))
            record("stereo_bin",hz=hz,complex_lr_ratio_error=error)
            assert error<1e-5

    for dc,low,high,hz in [(0,0.6,0.25,6000),(0,0,0.8,17000),(0,0,0.8,19000),(0,0,0.8,21000),(0,0,0.8,23000),(0,0,0.93,3000),(0.95,0,0.02,6000)]:
        scalar=dc+low*np.sin(2*np.pi*150*t)+high*np.sin(2*np.pi*hz*t+0.123)
        x=np.column_stack([scalar,scalar*0.5]).astype(np.float32)
        with Host(args.so) as h:
            y=h.process(x)
            start=RATE//2
            dry=peak(x[start:start+RATE//2])
            wet=peak(y[start+h.latency:start+h.latency+RATE//2])
            record("true_peak",dc=dc,hz=hz,input_dbtp=float(20*np.log10(dry)),output_dbtp=float(20*np.log10(wet)),excess_linear=max(0,wet-max(dry,10**(-1/20))))
            assert wet<=max(dry,10**(-1/20))+1e-5

    with Host(args.so) as h:
        y=h.process(np.zeros((RATE,2),np.float32))
        nonzero=int(np.count_nonzero(y.view(np.uint32)))
        record("silence",nonzero_samples=nonzero)
        assert nonzero==0

    if args.legacy_so:
        for hz in [300,500,700,1000,1500]:
            gains=[]
            for so in [args.legacy_so,args.so]:
                x=tone(hz,0.1)
                with Host(so) as h:
                    y=h.process(x)
                    start=h.latency+RATE//2
                    delta=y[start:start+RATE,0]-x[start-h.latency:start+RATE-h.latency,0]
                    ratio=abs(phasor(delta,hz))/(0.1*(10**(3/20)-1))
                    gains.append(float(20*np.log10(max(ratio,1e-30))))
            record("correction_transfer_ab",hz=hz,before_db=gains[0],after_db=gains[1])

    args.output.parent.mkdir(parents=True,exist_ok=True)
    args.output.write_text(json.dumps(records,indent=2)+"\n")
    print("SO_VERIFICATION_OK",flush=True)

if __name__=="__main__": main()
