#!/usr/bin/env python3
"""Offline evaluation through the real id14 remix and SR LADSPA plugins."""

from __future__ import annotations

import argparse
import ctypes as C
import math
import os
from pathlib import Path
import shutil
import tempfile
import time

try:
    import numpy as np
    import pyloudnorm as pyln
    from scipy import signal
    import soundfile as sf
except ImportError as exc:  # pragma: no cover - exercised by dependency-less hosts
    raise SystemExit(
        f"missing Python dependency: {exc.name}; install requirements.txt or set "
        "REMIX_EVAL_PYTHON"
    ) from exc


RATE = 48_000
BLOCK = 256
REMIX_LATENCY = 3776
SR_LATENCY = 1024
STATE_NAMES = {
    0: "Loading",
    1: "Active",
    2: "Off",
    3: "ZeroAmounts",
    4: "ModelMissing",
    5: "ModelInvalid",
    6: "Overloaded",
    7: "PeakProtected",
    8: "UnsupportedRate",
}
SPATIAL_BANDS = (("150-500", 150, 500), ("500-2k", 500, 2000), ("2k-8k", 2000, 8000))
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
    ("unique_id", C.c_ulong),
    ("label", C.c_char_p),
    ("properties", C.c_int),
    ("name", C.c_char_p),
    ("maker", C.c_char_p),
    ("copyright", C.c_char_p),
    ("port_count", C.c_ulong),
    ("types", C.POINTER(C.c_int)),
    ("names", C.POINTER(C.c_char_p)),
    ("hints", C.POINTER(Hint)),
    ("data", C.c_void_p),
    ("instantiate", Instantiate),
    ("connect", Connect),
    ("activate", Lifecycle),
    ("run", Run),
    ("run_adding", Run),
    ("adding_gain", Gain),
    ("deactivate", Lifecycle),
    ("cleanup", Lifecycle),
]


class LadspaHost:
    def __init__(self, so: Path, label: str, controls: list[float]):
        self.lib = C.CDLL(str(so))
        self.lib.ladspa_descriptor.argtypes = [C.c_ulong]
        self.lib.ladspa_descriptor.restype = C.POINTER(Descriptor)
        self.ptr = self.lib.ladspa_descriptor(0)
        if not self.ptr or self.lib.ladspa_descriptor(1):
            raise RuntimeError(f"{so}: expected exactly one LADSPA descriptor")
        self.d = self.ptr.contents
        actual = self.d.label.decode()
        if actual != label:
            raise RuntimeError(f"{so}: label {actual!r}, expected {label!r}")
        self.handle = self.d.instantiate(self.ptr, RATE)
        if not self.handle:
            raise RuntimeError(f"{so}: instantiate failed")
        self.inputs = [np.zeros(BLOCK, np.float32) for _ in range(2)]
        self.outputs = [np.zeros(BLOCK, np.float32) for _ in range(2)]
        self.controls = [C.c_float(value) for value in controls]
        for port, array in enumerate(self.inputs + self.outputs):
            self.d.connect(self.handle, port, array.ctypes.data_as(F32P))
        for port, value in enumerate(self.controls, start=4):
            self.d.connect(self.handle, port, C.pointer(value))
        if self.d.activate:
            self.d.activate(self.handle)
        self.closed = False

    def block(self, frames: np.ndarray) -> np.ndarray:
        count = len(frames)
        for channel in range(2):
            self.inputs[channel].fill(0)
            self.inputs[channel][:count] = frames[:, channel]
        self.d.run(self.handle, count)
        return np.column_stack([channel[:count] for channel in self.outputs]).copy()

    def close(self) -> None:
        if not self.closed:
            if self.d.deactivate:
                self.d.deactivate(self.handle)
            self.d.cleanup(self.handle)
            self.closed = True

    def __enter__(self) -> "LadspaHost":
        return self

    def __exit__(self, *_: object) -> None:
        self.close()


def descriptor_names(host: LadspaHost) -> list[str]:
    return [host.d.names[index].decode() for index in range(host.d.port_count)]


def run_remix_attempt(
    so: Path, audio: np.ndarray, amounts: tuple[float, float, float, float], wait: float
) -> tuple[np.ndarray, dict[str, object]]:
    controls = [*amounts, 1.0, 0.0, 0.0]
    with LadspaHost(so, "id14_remix_stereo", controls) as host:
        expected = [
            "Input L", "Input R", "Output L", "Output R", "Vocals", "Drums",
            "Bass", "Other", "Enabled", "State", "latency",
        ]
        if descriptor_names(host) != expected:
            raise RuntimeError("remix LADSPA ports do not match remix-v1")
        deadline = time.monotonic() + 30
        prewarm_blocks = 0
        state = 0
        while time.monotonic() < deadline:
            host.block(np.zeros((BLOCK, 2), np.float32))
            prewarm_blocks += 1
            state = int(host.controls[5].value)
            if state == 1:
                break
            if state == 6:
                raise WorkerOverload(wait, {state: 1})
            if state in (4, 5, 8):
                raise RuntimeError(f"remix model startup failed: {STATE_NAMES[state]}")
            time.sleep(wait)
        else:
            raise RuntimeError(f"remix did not reach Active; last state={STATE_NAMES.get(state, state)}")
        latency = int(host.controls[6].value)
        if latency != REMIX_LATENCY:
            raise RuntimeError(f"remix reported latency {latency}, expected {REMIX_LATENCY}")

        padded = np.concatenate([audio, np.zeros((latency + BLOCK, 2), np.float32)])
        output: list[np.ndarray] = []
        states: dict[int, int] = {}
        for start in range(0, len(padded), BLOCK):
            chunk = padded[start : start + BLOCK]
            output.append(host.block(chunk))
            state = int(host.controls[5].value)
            states[state] = states.get(state, 0) + 1
            if state == 6:
                raise WorkerOverload(wait, states)
            if state in (4, 5, 8):
                raise RuntimeError(f"remix failed while rendering: {STATE_NAMES[state]}")
            time.sleep(wait)
        rendered = np.concatenate(output)
        aligned = rendered[latency : latency + len(audio)]
        evidence = {
            "active_confirmed": True,
            "reported_latency_frames": latency,
            "prewarm_blocks": prewarm_blocks,
            "worker_wait_ms": wait * 1000,
            "state_blocks": {STATE_NAMES.get(key, str(key)): value for key, value in sorted(states.items())},
        }
        return aligned.astype(np.float32), evidence


class WorkerOverload(RuntimeError):
    def __init__(self, wait: float, states: dict[int, int]):
        super().__init__(f"worker overloaded at wait={wait * 1000:.3f} ms states={states}")
        self.wait = wait


def run_remix(
    so: Path, audio: np.ndarray, amounts: tuple[float, float, float, float], initial_wait: float
) -> tuple[np.ndarray, dict[str, object]]:
    wait = initial_wait
    failures: list[str] = []
    for _ in range(6):
        try:
            output, evidence = run_remix_attempt(so, audio, amounts, wait)
            evidence["discarded_attempts"] = failures
            return output, evidence
        except WorkerOverload as exc:
            failures.append(str(exc))
            wait *= 2
    raise RuntimeError("remix worker did not keep up after retries: " + "; ".join(failures))


def run_sr(so: Path, audio: np.ndarray, mix: float) -> np.ndarray:
    with LadspaHost(so, "id14_sr_stereo", [mix]) as host:
        expected = ["Input L", "Input R", "Output L", "Output R", "Mix"]
        if descriptor_names(host) != expected:
            raise RuntimeError("SR LADSPA ports do not match expected ABI")
        padded = np.concatenate([audio, np.zeros((SR_LATENCY + BLOCK, 2), np.float32)])
        output = [host.block(padded[start : start + BLOCK]) for start in range(0, len(padded), BLOCK)]
    rendered = np.concatenate(output)
    return rendered[SR_LATENCY : SR_LATENCY + len(audio)].astype(np.float32)


def integrated_loudness(audio: np.ndarray) -> float:
    return float(pyln.Meter(RATE).integrated_loudness(audio.astype(np.float64)))


def loudness_range(audio: np.ndarray) -> float:
    data = audio.astype(np.float64).copy()
    meter = pyln.Meter(RATE)
    for filter_stage in meter._filters.values():  # same K-weighting used by pyloudnorm
        for channel in range(data.shape[1]):
            data[:, channel] = filter_stage.apply_filter(data[:, channel])
    window = 3 * RATE
    hop = RATE
    values = []
    for start in range(0, len(data) - window + 1, hop):
        power = np.mean(np.square(data[start : start + window]), axis=0).sum()
        values.append(-0.691 + 10 * math.log10(max(power, 1e-30)))
    values = np.asarray(values)
    absolute = values[values >= -70]
    if not len(absolute):
        return 0.0
    relative_gate = -0.691 + 10 * math.log10(np.mean(10 ** ((absolute + 0.691) / 10))) - 20
    gated = absolute[absolute > relative_gate]
    return float(np.percentile(gated, 95) - np.percentile(gated, 10)) if len(gated) else 0.0


def loudness_match(audio: np.ndarray, target_lufs: float) -> tuple[np.ndarray, float]:
    gain_db = target_lufs - integrated_loudness(audio)
    return (audio * np.float32(10 ** (gain_db / 20))).astype(np.float32), gain_db


def true_peak(audio: np.ndarray) -> float:
    oversampled = signal.resample_poly(audio.astype(np.float64), 4, 1, axis=0, window=("kaiser", 8.0))
    peak = float(np.max(np.abs(oversampled)))
    return 20 * math.log10(max(peak, 1e-30))


def band_energy(audio: np.ndarray, low: float, high: float) -> float:
    frequencies, power = signal.welch(audio, fs=RATE, nperseg=8192, axis=0)
    selected = (frequencies >= low) & (frequencies < high)
    return float(np.sum(power[selected]))


def ratio_db(audio: np.ndarray) -> float:
    return 10 * math.log10(max(band_energy(audio, 150, 500), 1e-30) / max(band_energy(audio, 2000, 5000), 1e-30))


def low_frequency_difference(reference: np.ndarray, test: np.ndarray) -> float:
    # Select bins at the contractual boundary. A low-pass filter at 300 Hz would
    # include its transition band above 300 Hz and misattribute valid correction.
    frequencies, base = signal.welch(reference, fs=RATE, nperseg=65536, axis=0)
    _, error = signal.welch(test - reference, fs=RATE, nperseg=65536, axis=0)
    selected = frequencies <= 300
    return 10 * math.log10(
        max(float(np.sum(error[selected])), 1e-30)
        / max(float(np.sum(base[selected])), 1e-30)
    )


def si_sdr(reference: np.ndarray, test: np.ndarray) -> float:
    ref = (reference - np.mean(reference, axis=0)).ravel().astype(np.float64)
    est = (test - np.mean(test, axis=0)).ravel().astype(np.float64)
    scale = np.dot(est, ref) / max(np.dot(ref, ref), 1e-30)
    target = scale * ref
    noise = est - target
    return 10 * math.log10(max(float(np.dot(target, target)), 1e-30) / max(float(np.dot(noise, noise)), 1e-30))


def sdr(reference: np.ndarray, test: np.ndarray) -> float:
    ref = reference.astype(np.float64)
    error = test.astype(np.float64) - ref
    return 10 * math.log10(max(float(np.sum(ref * ref)), 1e-30) / max(float(np.sum(error * error)), 1e-30))


def spatial(audio: np.ndarray, low: float, high: float) -> tuple[float, float, float]:
    sos = signal.butter(6, (low, high), btype="bandpass", fs=RATE, output="sos")
    filtered = signal.sosfilt(sos, audio.astype(np.float64), axis=0)
    left, right = filtered[:, 0], filtered[:, 1]
    mid = 0.5 * (left + right)
    side = 0.5 * (left - right)
    side_mid = 10 * math.log10(max(float(np.sum(side * side)), 1e-30) / max(float(np.sum(mid * mid)), 1e-30))
    correlation = float(np.corrcoef(left, right)[0, 1])
    stereo_energy = 0.5 * (float(np.sum(left * left)) + float(np.sum(right * right)))
    mono_loss = 10 * math.log10(max(float(np.sum(mid * mid)), 1e-30) / max(stereo_energy, 1e-30))
    return side_mid, correlation, mono_loss


def metrics(dry: np.ndarray, reference: np.ndarray, test: np.ndarray) -> dict[str, object]:
    input_lufs = integrated_loudness(dry)
    lufs = integrated_loudness(test)
    input_peak = true_peak(dry)
    peak = true_peak(test)
    detailed = {}
    maximums = [0.0, 0.0, 0.0]
    for name, low, high in SPATIAL_BANDS:
        ref_values = spatial(reference, low, high)
        test_values = spatial(test, low, high)
        delta = tuple(test_values[index] - ref_values[index] for index in range(3))
        detailed[name] = delta
        maximums = [max(maximums[index], abs(delta[index])) for index in range(3)]
    return {
        "lufs": lufs,
        "loudness_delta": lufs - input_lufs,
        "lf_difference": low_frequency_difference(dry, test),
        "mud_delta": ratio_db(test) - ratio_db(dry),
        "lra": loudness_range(test),
        "lra_delta": loudness_range(test) - loudness_range(dry),
        "true_peak": peak,
        "true_peak_limit": max(-1.0, input_peak),
        "si_sdr_input": si_sdr(dry, test),
        "sdr_reference": sdr(reference, test),
        "spatial_max": tuple(maximums),
        "spatial": detailed,
    }


def yn(value: bool) -> str:
    return "PASS" if value else "FAIL"


def write_metrics(
    path: Path,
    rows: list[tuple[str, str, dict[str, object]]],
    evidence: dict[str, dict[str, object]],
    listening: list[tuple[str, float, float, float]],
) -> None:
    lines = [
        "# Remix model offline evaluation", "",
        "All WAV metrics use delay-compensated, sample-aligned audio. Listening files are matched to A_dry integrated loudness.",
        "LF delta is Welch-spectrum error energy in bins <=300 Hz relative to input energy in the same bins.",
        "Spatial deltas are test minus offline reference in each band. Mono loss is mono-mid energy relative to stereo channel energy.",
        "True peak uses 4x oversampling. LRA uses 3 s short-term K-weighted loudness, -70 LUFS absolute and -20 LU relative gates.",
        "SI-SDR is against input; SDR is the direct aligned error ratio against the offline reference.", "",
        "## LADSPA execution evidence", "",
        "| Input | Active | Remix latency | SR latency | Worker wait | State blocks | Discarded attempts |",
        "|---|---:|---:|---:|---:|---|---|",
    ]
    for name, item in evidence.items():
        states = ", ".join(f"{key}={value}" for key, value in item["state_blocks"].items())
        discarded = "<br>".join(item["discarded_attempts"]) or "none"
        lines.append(
            f"| {name} | {item['active_confirmed']} | {item['reported_latency_frames']} | {SR_LATENCY} | "
            f"{item['worker_wait_ms']:.3f} ms | {states} | {discarded} |"
        )
    lines += ["", "## Spec-oriented summary", "",
        "| Input | Render | Loudness Δ LU (abs<=.5) | LF <=300 dB (<=-60) | Mud ratio delta dB (<=+0.5) | LRA delta LU (abs<=1) | True peak dBTP (<=limit) | SI-SDR input dB (<=30) | SDR ref dB | Spatial max Δ S/M dB (<=1) | corr (<=.05) | mono dB (<=.3) |",
        "|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|",
    ]
    for input_name, render, item in rows:
        sm, corr, mono = item["spatial_max"]
        lines.append(
            f"| {input_name} | {render} | {item['loudness_delta']:+.3f} {yn(abs(item['loudness_delta']) <= .5)} | "
            f"{item['lf_difference']:.3f} {yn(item['lf_difference'] <= -60)} | "
            f"{item['mud_delta']:+.3f} {yn(item['mud_delta'] <= .5)} | {item['lra_delta']:+.3f} {yn(abs(item['lra_delta']) <= 1)} | "
            f"{item['true_peak']:.3f} / {item['true_peak_limit']:.3f} {yn(item['true_peak'] <= item['true_peak_limit'] + .05)} | "
            f"{item['si_sdr_input']:.3f} {yn(item['si_sdr_input'] <= 30)} | {item['sdr_reference']:.3f} | "
            f"{sm:.3f} {yn(sm <= 1)} | {corr:.4f} {yn(corr <= .05)} | {mono:.3f} {yn(mono <= .3)} |"
        )
    lines += ["", "## Spatial deltas by band", "",
        "| Input | Render | Band Hz | Side/mid Δ dB | L/R corr Δ | Mono-loss Δ dB |",
        "|---|---|---|---:|---:|---:|",
    ]
    for input_name, render, item in rows:
        for band, values in item["spatial"].items():
            lines.append(f"| {input_name} | {render} | {band} | {values[0]:+.3f} | {values[1]:+.4f} | {values[2]:+.3f} |")
    lines += ["", "## Listening-set loudness", "",
        "| Input | A dry LUFS | M model LUFS | R reference LUFS | Max difference LU |",
        "|---|---:|---:|---:|---:|",
    ]
    for name, a, m, r in listening:
        spread = max(a, m, r) - min(a, m, r)
        lines.append(f"| {name} | {a:.3f} | {m:.3f} | {r:.3f} | {spread:.3f} {yn(spread <= .5)} |")
    path.write_text("\n".join(lines) + "\n")


def parse_args() -> argparse.Namespace:
    root = Path(__file__).resolve().parents[2]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--model", type=Path, required=True)
    parser.add_argument("--input", type=Path, nargs="+", required=True)
    parser.add_argument("--reference-dir", type=Path, required=True)
    parser.add_argument("--output-dir", type=Path, required=True)
    parser.add_argument("--tag", default="model")
    parser.add_argument("--remix-so", type=Path, default=root / "target/release/libid14_remix_ladspa.so")
    parser.add_argument("--sr-so", type=Path, default=root / "target/release/libid14_sr_ladspa.so")
    parser.add_argument("--vocals-db", type=float, default=3.0)
    parser.add_argument("--drums-db", type=float, default=0.0)
    parser.add_argument("--bass-db", type=float, default=0.0)
    parser.add_argument("--other-db", type=float, default=-3.0)
    parser.add_argument("--worker-wait-ms", type=float, default=0.75)
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    amounts = (args.vocals_db, args.drums_db, args.bass_db, args.other_db)
    if any(not -6 <= value <= 6 for value in amounts):
        raise SystemExit("all remix amounts must be between -6 and +6 dB")
    for path in (args.model, args.remix_so, args.sr_so, *args.input):
        if not path.is_file():
            raise SystemExit(f"missing file: {path}")
    args.output_dir.mkdir(parents=True, exist_ok=True)
    render_dir = args.output_dir / "renders"
    render_dir.mkdir(exist_ok=True)
    metric_rows: list[tuple[str, str, dict[str, object]]] = []
    execution: dict[str, dict[str, object]] = {}
    listening: list[tuple[str, float, float, float]] = []

    with tempfile.TemporaryDirectory(prefix="id14-remix-eval-") as temporary:
        isolated_home = Path(temporary) / "home"
        model_path = isolated_home / ".local/share/id14-sr/remix.onnx"
        model_path.parent.mkdir(parents=True)
        shutil.copy2(args.model, model_path)
        runtime = Path(temporary) / "runtime"
        runtime.mkdir()
        os.environ["HOME"] = str(isolated_home)
        os.environ["XDG_RUNTIME_DIR"] = str(runtime)
        os.environ.pop("ID14_REMIX_MODEL", None)

        for input_path in args.input:
            dry, rate = sf.read(input_path, dtype="float32", always_2d=True)
            if rate != RATE or dry.shape[1] != 2:
                raise RuntimeError(f"{input_path}: expected 48 kHz stereo")
            name = input_path.stem
            reference_path = args.reference_dir / f"{name}_B_mild.wav"
            if not reference_path.is_file():
                raise RuntimeError(f"missing offline reference: {reference_path}")
            reference, reference_rate = sf.read(reference_path, dtype="float32", always_2d=True)
            if reference_rate != RATE or reference.shape != dry.shape:
                raise RuntimeError(f"{reference_path}: reference shape/rate differs from input")

            remix, item_evidence = run_remix(
                args.remix_so, dry, amounts, args.worker_wait_ms / 1000
            )
            sr150 = run_sr(args.sr_so, remix, 150)
            sr185 = run_sr(args.sr_so, remix, 185)
            execution[name] = item_evidence
            variants = (("remix", remix), ("remix+SR150", sr150), ("remix+SR185", sr185))
            for label, audio in variants:
                filename = label.replace("+", "_").lower()
                sf.write(render_dir / f"{name}_{filename}.wav", audio, RATE, subtype="FLOAT")
                metric_rows.append((name, label, metrics(dry, reference, audio)))

            target_lufs = integrated_loudness(dry)
            matched_model, _ = loudness_match(sr185, target_lufs)
            matched_reference, _ = loudness_match(reference, target_lufs)
            sf.write(args.output_dir / f"{name}_A_dry.wav", dry, RATE, subtype="FLOAT")
            sf.write(args.output_dir / f"{name}_M_{args.tag}_remix_sr.wav", matched_model, RATE, subtype="FLOAT")
            sf.write(args.output_dir / f"{name}_R_offline_ref.wav", matched_reference, RATE, subtype="FLOAT")
            listening.append((
                name,
                integrated_loudness(dry),
                integrated_loudness(matched_model),
                integrated_loudness(matched_reference),
            ))
            print(
                f"rendered {name}: Active confirmed, remix_latency={REMIX_LATENCY}, "
                f"sr_latency={SR_LATENCY}, wait={item_evidence['worker_wait_ms']:.3f} ms",
                flush=True,
            )

    write_metrics(args.output_dir / "metrics.md", metric_rows, execution, listening)
    print(f"wrote {args.output_dir / 'metrics.md'}", flush=True)


if __name__ == "__main__":
    main()
