"""Independent BS.1770 time-domain meter and real LADSPA remix -> SR host."""
import argparse
import ctypes as C
import hashlib
import json
import os
from pathlib import Path
import time

import numpy as np
from scipy.io import wavfile
from scipy.signal import lfilter
from verify_so import Host, Descriptor, F32P, BLOCK, RATE, ROOT


def k_power(audio):
    # ITU-R BS.1770-5 Annex 1, Tables 1/2, for exactly 48 kHz.
    y = lfilter([1.53512485958697, -2.69169618940638, 1.19839281085285],
                [1, -1.69065929318241, 0.73248077421585], audio.astype(float), axis=0)
    y = lfilter([1, -2, 1], [1, -1.99004745483398, 0.99007225036621], y, axis=0)
    return np.sum(y*y, axis=1)


def windows(power, seconds):
    size, step = round(RATE*seconds), RATE//10
    starts = np.arange(0, len(power)-size+1, step)
    integral = np.r_[0, np.cumsum(power)]
    return (integral[starts+size]-integral[starts])/size


def lu(power):
    return -0.691 + 10*np.log10(np.maximum(power, 1e-30))


def integrated(power):
    blocks = windows(power, 0.4)
    absolute = blocks[lu(blocks) > -70]
    assert len(absolute)
    relative = lu(np.mean(absolute))-10
    return float(lu(np.mean(blocks[(lu(blocks) > -70) & (lu(blocks) > relative)])))


class Sr:
    def __init__(self, path):
        self.lib = C.CDLL(str(path))
        self.lib.ladspa_descriptor.argtypes = [C.c_ulong]
        self.lib.ladspa_descriptor.restype = C.POINTER(Descriptor)
        self.ptr = self.lib.ladspa_descriptor(0)
        self.d = self.ptr.contents
        assert self.d.label == b"id14_sr_stereo" and self.d.port_count == 5
        self.handle = self.d.instantiate(self.ptr, RATE)
        assert self.handle
        self.arrays = [np.zeros(BLOCK, np.float32) for _ in range(4)]
        self.mix = C.c_float(185)
        for i, array in enumerate(self.arrays):
            self.d.connect(self.handle, i, array.ctypes.data_as(F32P))
        self.d.connect(self.handle, 4, C.pointer(self.mix))
        if self.d.activate: self.d.activate(self.handle)

    def process(self, data):
        padded = np.pad(data, ((0, 1024), (0, 0)))
        output = np.zeros_like(padded)
        for start in range(0, len(padded), BLOCK):
            x = padded[start:start+BLOCK]
            for ch in range(2): self.arrays[ch][:len(x)] = x[:, ch]
            self.d.run(self.handle, len(x))
            for ch in range(2): output[start:start+len(x), ch] = self.arrays[ch+2][:len(x)]
        return output[1024:]

    def close(self):
        if self.d.deactivate: self.d.deactivate(self.handle)
        self.d.cleanup(self.handle)


def synth(kind, seconds=32):
    t = np.arange(RATE*seconds)/RATE
    if kind == "speech_synthetic":
        # Voiced syllables, pitch glide and alternating formant envelopes.
        phase = 2*np.pi*np.cumsum(125+15*np.sin(2*np.pi*0.31*t))/RATE
        vowel = (np.sin(2*np.pi*0.45*t) > 0)
        scalar = np.zeros(len(t))
        for h in range(1, 38):
            hz = h*125
            amplitude = (np.exp(-((hz-(550+250*vowel))/130)**2)
                         + 0.6*np.exp(-((hz-(1300+800*vowel))/190)**2)
                         + 0.25*np.exp(-((hz-2800)/280)**2))/h
            scalar += amplitude*np.sin(h*phase)
        syllable = np.maximum(0, np.sin(2*np.pi*2.2*t))**0.4
        scalar *= syllable
    elif kind == "instrument_synthetic":
        scalar = np.zeros(len(t))
        for fundamental in [220, 277.1826, 329.6276]:
            for h in range(1, 16):
                scalar += np.sin(2*np.pi*fundamental*h*t+0.2*h)/h**1.4
        scalar *= (1-np.exp(-100*(t % 1.3)))*np.exp(-2*(t % 1.3))
    else:
        # Constant spectrum/level after startup: gain modulation here is pumping.
        scalar = sum(np.sin(2*np.pi*hz*t+0.31*i)/3 for i, hz in enumerate([700, 1400, 3000]))
    scalar *= 0.12/max(abs(scalar))
    return np.column_stack([scalar, scalar*0.7]).astype(np.float32)


def render(args, x, enabled):
    with Host(args.so, model=args.model, enabled=enabled) as h:
        gain_fn = getattr(h.lib, "id14_remix_loudness_gain_db", None)
        if gain_fn:
            gain_fn.argtypes = [C.c_void_p]
            gain_fn.restype = C.c_float
        padded = np.pad(x, ((0, h.latency), (0, 0)))
        y = np.zeros_like(padded)
        gains = []
        for start in range(0, len(padded), BLOCK):
            data = padded[start:start+BLOCK]
            y[start:start+len(data)] = h.block(data)
            if gain_fn: gains.append(gain_fn(h.handle))
            # Faster than realtime, but enough wall time for the asynchronous worker.
            if enabled: time.sleep(0.0015)
        assert h.states.count(6) == 0, ("overload invalidates loudness run", h.states.count(6))
        return y[h.latency:], np.asarray(gains)


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--so", type=Path, required=True)
    p.add_argument("--sr-so", type=Path, required=True)
    p.add_argument("--model", type=Path, required=True)
    p.add_argument("--output", type=Path, required=True)
    p.add_argument("--baseline", action="store_true")
    p.add_argument("--synthetic-only", action="store_true")
    p.add_argument("--case", choices=["dry", "dry2", "speech_synthetic", "instrument_synthetic", "stationary_pumping"])
    args = p.parse_args()
    print(json.dumps({"plugin_sha256": hashlib.sha256(args.so.read_bytes()).hexdigest(),
                      "model_sha256": hashlib.sha256(args.model.read_bytes()).hexdigest()}), flush=True)
    os.environ["XDG_RUNTIME_DIR"] = str(ROOT / ".build/loudness-runtime")
    # Standard single-channel 997 Hz, full-scale sine reference: -3.01 LKFS.
    t = np.arange(RATE*3)/RATE
    reference = integrated(k_power(np.column_stack([np.sin(2*np.pi*997*t), np.zeros(len(t))])))
    assert abs(reference+3.01) < 0.005, reference
    print(json.dumps({"meter_reference_997_hz_lkfs": reference}), flush=True)
    cases = []
    if not args.synthetic_only:
        for path in [Path("/tmp/sr-eval/dry.wav"), Path("/tmp/sr-eval/dry2.wav")]:
            rate, x = wavfile.read(path)
            assert rate == RATE and x.dtype == np.float32 and x.shape[1] == 2
            cases.append((path.stem, x, hashlib.sha256(path.read_bytes()).hexdigest()))
    cases.extend((name, synth(name), "synthetic") for name in
                 ["speech_synthetic", "instrument_synthetic", "stationary_pumping"])
    if args.case:
        cases = [case for case in cases if case[0] == args.case]
    assert cases
    records = []
    for name, x, digest in cases:
        wet, gains = render(args, x, 1)
        dry, _ = render(args, x, 0)
        assert np.array_equal(dry.view(np.uint32), x.view(np.uint32))
        for chain in ["remix", "remix_sr185"]:
            a, b = dry, wet
            if chain == "remix_sr185":
                sr = Sr(args.sr_so)
                a = sr.process(dry)
                sr.close()
                sr = Sr(args.sr_so)
                b = sr.process(wet)
                sr.close()
            pa, pb = k_power(a), k_power(b)
            off, on = integrated(pa), integrated(pb)
            active = lu(windows(pa, 3)) > off-20
            short_delta = (lu(windows(pb, 3))-lu(windows(pa, 3)))[active]
            record = {"material": name, "chain": chain, "seconds": len(x)/RATE,
                      "source_sha256": digest, "off_lufs": off, "on_lufs": on,
                      "delta_lu": on-off, "short_delta_p05_lu": float(np.percentile(short_delta, 5)),
                      "short_delta_p95_lu": float(np.percentile(short_delta, 95)),
                      "short_delta_max_100ms_step_lu": float(np.max(abs(np.diff(short_delta)))),
                      "input_difference_rms_db": float(20*np.log10(max(np.linalg.norm(b-a)/np.linalg.norm(a), 1e-30)))}
            if len(gains):
                record.update(gain_min_db=float(gains.min()), gain_max_db=float(gains.max()),
                              gain_max_slew_db_s=float(np.max(abs(np.diff(gains[1::2])))/(2*BLOCK/RATE)))
                if name == "stationary_pumping":
                    settled = gains[int(15*RATE/BLOCK):]
                    record["settled_gain_peak_to_peak_db"] = float(np.ptp(settled))
                    record["settled_short_delta_peak_to_peak_lu"] = float(np.ptp(short_delta[150:]))
                    momentary = lu(windows(pb, 0.4))-lu(windows(pa, 0.4))
                    record["settled_momentary_delta_peak_to_peak_lu"] = float(np.ptp(momentary[150:]))
            records.append(record)
            print(json.dumps(record, sort_keys=True), flush=True)
        args.output.write_text(json.dumps(records, indent=2)+"\n")
    if not args.baseline:
        assert all(abs(r["delta_lu"]) <= 0.5 for r in records), "loudness fairness"
        assert all(r["gain_max_slew_db_s"] <= 0.5001 for r in records), "gain slew"
        assert all(-3.0001 <= r["gain_min_db"] <= r["gain_max_db"] <= 3.0001 for r in records)
    print("LOUDNESS_SO_MEASUREMENT_OK" if args.baseline else "LOUDNESS_SO_VERIFICATION_OK", flush=True)


if __name__ == "__main__":
    main()
