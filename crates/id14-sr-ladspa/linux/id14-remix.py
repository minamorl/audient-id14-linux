#!/usr/bin/env python3
"""Remix CLI integration. Runtime telemetry contract: remix-state-v1.

PipeWire's LADSPA output controls are not Props. Status therefore comes only
from the runtime's atomic telemetry files, never from desired control values.
"""

import ctypes
import datetime
import fcntl
import hashlib
import json
import math
import os
from pathlib import Path
import re
import secrets
import shutil
import subprocess
import sys
import tempfile
import time
import uuid


DEFAULT = {"enabled": True, "amounts_db": {"vocals": 3, "drums": 0, "bass": 0, "other": -3}}
PORTS = {"vocals": "Vocals", "drums": "Drums", "bass": "Bass", "other": "Other"}
STATES = ("Loading", "Active", "Off", "ZeroAmounts", "ModelMissing", "ModelInvalid",
          "Overloaded", "PeakProtected", "UnsupportedRate")
MAX_AGE_MS = 5000


def error(message, code="REMIX_ERROR"):
    # UUIDv7: 48-bit Unix milliseconds, version, variant, random remainder.
    bits = ((time.time_ns() // 1_000_000) << 80) | (7 << 76)
    bits |= secrets.randbits(12) << 64 | (2 << 62) | secrets.randbits(62)
    return {"code": code, "message": message, "details": {},
            "trace_id": str(uuid.UUID(int=bits)),
            "ts": datetime.datetime.now(datetime.timezone.utc).isoformat(),
            "level": "error", "msg": message}


def plugin_path():
    return Path(os.environ.get("ID14_REMIX_PLUGIN", str(Path.home() / ".local/lib/ladspa/libid14_remix_ladspa.so")))


def state_dir():
    return Path(os.environ.get("XDG_STATE_HOME") or Path.home() / ".local/state") / "id14-sr"


def amount(value):
    try:
        result = float(value)
    except (ValueError, TypeError):
        raise ValueError("amount must be a number from -6 to +6 dB") from None
    if isinstance(value, bool) or not math.isfinite(result) or not -6 <= result <= 6:
        raise ValueError("amount must be a number from -6 to +6 dB")
    return result


def read_state():
    try:
        data = json.loads((state_dir() / "remix.json").read_text())
    except FileNotFoundError:
        return {"enabled": DEFAULT["enabled"], "amounts_db": dict(DEFAULT["amounts_db"])}
    if not isinstance(data, dict) or type(data.get("enabled")) is not bool:
        raise ValueError("invalid saved remix state; file retained")
    values = data.get("amounts_db")
    if not isinstance(values, dict) or set(values) != set(PORTS):
        raise ValueError("invalid saved remix amounts; file retained")
    return {"enabled": data["enabled"], "amounts_db": {k: amount(v) for k, v in values.items()}}


def save_change(key, value):
    directory = state_dir()
    directory.mkdir(parents=True, exist_ok=True)
    with (directory / "remix.lock").open("a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        state = read_state()
        if key == "enabled":
            state[key] = value
        else:
            state["amounts_db"][key] = value
        temporary = None
        try:
            with tempfile.NamedTemporaryFile(mode="w", dir=directory, prefix=".remix-", delete=False) as stream:
                temporary = Path(stream.name)
                json.dump(state, stream, allow_nan=False)
                stream.write("\n")
                stream.flush()
                os.fsync(stream.fileno())
            os.replace(temporary, directory / "remix.json")
            descriptor = os.open(directory, os.O_RDONLY | os.O_DIRECTORY)
            try:
                os.fsync(descriptor)
            finally:
                os.close(descriptor)
        finally:
            if temporary is not None:
                temporary.unlink(missing_ok=True)
        return state


def graph(mix):
    mix = float(mix)
    if not math.isfinite(mix):
        raise ValueError("invalid SR mix")
    sr = f'''{{
                    type = ladspa
                    name = sr
                    plugin = libid14_sr_ladspa
                    label = id14_sr_stereo
                    control = {{ Mix = {mix:g} }}
                }}'''
    if not plugin_path().is_file():
        return f'''                nodes = [ {sr} ]
                inputs = [ "sr:Input L" "sr:Input R" ]
                outputs = [ "sr:Output L" "sr:Output R" ]'''
    state = read_state()
    controls = " ".join(f"{port} = {state['amounts_db'][key]:g}" for key, port in PORTS.items())
    return f'''                nodes = [ {{
                    type = ladspa
                    name = remix
                    plugin = {json.dumps(str(plugin_path()))}
                    label = id14_remix_stereo
                    control = {{ {controls} Enabled = {int(state['enabled'])} }}
                }} {sr} ]
                links = [
                    {{ output = "remix:Output L" input = "sr:Input L" }}
                    {{ output = "remix:Output R" input = "sr:Input R" }}
                ]
                inputs = [ "remix:Input L" "remix:Input R" ]
                outputs = [ "sr:Output L" "sr:Output R" ]'''


def command(arguments, timeout=10):
    try:
        result = subprocess.run(arguments, text=True, capture_output=True, timeout=timeout)
    except subprocess.TimeoutExpired:
        raise RuntimeError(f"{arguments[0]} timed out; saved settings retained") from None
    if result.returncode:
        raise RuntimeError(f"{arguments[0]} failed (exit {result.returncode}); saved settings retained")
    return result.stdout


def apply(node_id, state=None):
    if not node_id or not plugin_path().is_file():
        return
    if not str(node_id).isdigit():
        raise ValueError("invalid filter node id")
    state = read_state() if state is None else state
    controls = " ".join(f'"remix:{port}" {state["amounts_db"][key]:.6f}' for key, port in PORTS.items())
    controls += f' "remix:Enabled" {float(state["enabled"]):.1f}'
    command(["pw-cli", "set-param", str(node_id), "Props", "{ params = [ " + controls + " ] }"])
    # 3776 remix + 1024 SR; switching does not rebuild the graph or change delay.
    command(["pw-cli", "set-param", str(node_id), "ProcessLatency", "{ quantum = 0 rate = 4800 ns = 0 }"])


def active_nodes():
    # Discover by the actual input controls, independent of output selection/names.
    data = json.loads(command(["pw-dump"]))
    if not isinstance(data, list):
        raise ValueError("invalid pw-dump node list")
    result = set()
    for node in data:
        if not isinstance(node, dict) or node.get("type") != "PipeWire:Interface:Node":
            continue
        for props in node.get("info", {}).get("params", {}).get("Props", []):
            params = props.get("params", {})
            if isinstance(params, (dict, list)) and "remix:Enabled" in params and "sr:Mix" in params:
                result.add(node["id"])
    return sorted(result)


def apply_all(state):
    if not plugin_path().is_file():
        return 0
    nodes = active_nodes()
    failures = []
    for node in nodes:
        try:
            apply(node, state)
        except (RuntimeError, OSError, subprocess.TimeoutExpired) as exc:
            failures.append(str(exc))
    if failures:
        raise RuntimeError("; ".join(failures))
    return len(nodes)


def runtime_instances():
    runtime = os.environ.get("XDG_RUNTIME_DIR")
    if not runtime:
        return []
    now = time.time_ns() // 1_000_000
    result = []
    for path in sorted((Path(runtime) / "id14-sr/remix-state").glob("*.json")):
        try:
            item = json.loads(path.read_text())
            if not isinstance(item, dict) or item.get("contract") != "remix-state-v1":
                continue
            if any(type(item.get(k)) is not int for k in ("pid", "instance", "state", "overloads", "latency_frames", "updated_unix_ms")):
                continue
            if not 0 < item["pid"] <= 2_147_483_647 or item["instance"] < 0 or item["overloads"] < 0:
                continue
            if path.name != f"{item['pid']}-{item['instance']}.json":
                continue
            if not 0 <= item["state"] < len(STATES) or item.get("state_name") != STATES[item["state"]]:
                continue
            if item["latency_frames"] != 3776 or not 0 <= now - item["updated_unix_ms"] <= MAX_AGE_MS:
                continue
            if "model" not in item or item["model"] is not None and not isinstance(item["model"], str):
                continue
            os.kill(item["pid"], 0)
        except (OSError, ValueError, TypeError):
            continue
        result.append(item)
    return result


def status():
    state = read_state()
    instances = runtime_instances()
    priorities = (6, 5, 4, 8, 7, 0, 1, 3, 2)
    current = next((n for n in priorities if any(i["state"] == n for i in instances)), None)
    name = STATES[current] if current is not None else ("Unknown" if plugin_path().is_file() else "PluginMissing")
    state.update({"contract": "remix-cli-v1", "state": current, "state_name": name,
                  "overloaded": any(i["state"] == 6 for i in instances),
                  "instances": instances, "declared_latency_frames": 4800 if plugin_path().is_file() else 1024,
                  "text": f"Remix: {name}", "class": "remix-" + name.lower()})
    state["tooltip"] = "\n".join([f"Requested: {'ON' if state['enabled'] else 'OFF'}", f"Runtime: {name}"] +
                                    [f"{k}: {v:+g} dB" for k, v in state["amounts_db"].items()])
    return state


def print_status(as_json=False, applied=None):
    data = status()
    if applied is not None:
        data["applied_nodes"] = applied
    if as_json:
        print(json.dumps(data, ensure_ascii=False, separators=(",", ":")))
    else:
        values = " ".join(f"{key}={value:+g}dB" for key, value in data["amounts_db"].items())
        print(f"remix requested={'on' if data['enabled'] else 'off'} state={data['state_name']} {values} "
              f"declared_latency_frames={data['declared_latency_frames']}")
        for item in data["instances"]:
            print(f"  pid={item['pid']} instance={item['instance']} state={item['state_name']} overloads={item['overloads']}")


def probe_plugin(path):
    # Descriptor ABI from PipeWire's bundled LADSPA SDK header (LADSPA 1.1).
    class Hint(ctypes.Structure):
        _fields_ = [("descriptor", ctypes.c_int), ("lower", ctypes.c_float), ("upper", ctypes.c_float)]

    class Descriptor(ctypes.Structure):
        _fields_ = [("unique_id", ctypes.c_ulong), ("label", ctypes.c_char_p),
                    ("properties", ctypes.c_int), ("name", ctypes.c_char_p),
                    ("maker", ctypes.c_char_p), ("copyright", ctypes.c_char_p),
                    ("count", ctypes.c_ulong), ("flags", ctypes.POINTER(ctypes.c_int)),
                    ("names", ctypes.POINTER(ctypes.c_char_p)), ("hints", ctypes.POINTER(Hint))]

    library = ctypes.CDLL(str(Path(path).resolve()))
    descriptor = library.ladspa_descriptor
    descriptor.argtypes = [ctypes.c_ulong]
    descriptor.restype = ctypes.POINTER(Descriptor)
    expected = [b"Input L", b"Input R", b"Output L", b"Output R", b"Vocals", b"Drums", b"Bass", b"Other", b"Enabled", b"State", b"latency"]
    for index in range(256):
        pointer = descriptor(index)
        if not pointer:
            break
        value = pointer.contents
        if value.label != b"id14_remix_stereo":
            continue
        if value.count != 11 or not value.flags or not value.names or not value.hints:
            raise ValueError("remix LADSPA descriptor must have 11 ports")
        if [value.names[i] for i in range(11)] != expected:
            raise ValueError("remix LADSPA port names/order mismatch")
        if [value.flags[i] for i in range(11)] != [9, 9, 10, 10, 5, 5, 5, 5, 5, 6, 6]:
            raise ValueError("remix LADSPA port directions/types mismatch")
        for i, bounds in [(4, (-6, 6)), (5, (-6, 6)), (6, (-6, 6)), (7, (-6, 6))]:
            hint = value.hints[i]
            if hint.descriptor & 3 != 3 or (hint.lower, hint.upper) != bounds:
                raise ValueError("remix LADSPA control range mismatch")
        # LADSPA TOGGLED permits only DEFAULT_0/DEFAULT_1 alongside it.
        # Its boolean semantics do not use the bounded-range flags or fields.
        if value.hints[8].descriptor != (0x4 | 0x240):
            raise ValueError("remix LADSPA Enabled must be TOGGLED with DEFAULT_1")
        return
    raise ValueError("LADSPA label id14_remix_stereo missing")


def probe_ort_library(path):
    # OrtApiBase ABI: https://onnxruntime.ai/docs/api/c/struct_ort_api_base.html
    class ApiBase(ctypes.Structure):
        _fields_ = [("get_api", ctypes.c_void_p), ("get_version", ctypes.CFUNCTYPE(ctypes.c_char_p))]

    library = ctypes.CDLL(str(Path(path).resolve()))
    entry = library.OrtGetApiBase
    entry.argtypes = []
    entry.restype = ctypes.POINTER(ApiBase)
    base = entry()
    if not base or not base.contents.get_version:
        raise ValueError("ONNX Runtime version API missing")
    raw = base.contents.get_version()
    version = raw.decode("utf-8") if raw else ""
    if not re.fullmatch(r"1\.27\.\d+(?:[-+][A-Za-z0-9.-]+)?", version):
        raise ValueError(f"ONNX Runtime 1.27.x required; found {version!r}")
    return version


def prepare_ort_library(home):
    nix = shutil.which("nix")
    if nix is None:
        return
    destination = home / ".local/lib/id14-sr/onnxruntime"
    destination.parent.mkdir(parents=True, exist_ok=True)
    # Keep staging on the destination filesystem so publication is one rename.
    with tempfile.TemporaryDirectory(prefix=".onnxruntime-stage-", dir=destination.parent) as temporary:
        candidate = Path(temporary) / "result"
        command([nix, "build", "nixpkgs#onnxruntime", "--out-link", str(candidate)], timeout=1800)
        if not candidate.is_symlink():
            raise ValueError("nix did not create an ONNX Runtime out-link")
        target = candidate.resolve(strict=True)
        # A faulty native library must not crash the installer process.
        command([sys.executable, str(Path(__file__).resolve()), "_probe_ort",
                 str(target / "lib/libonnxruntime.so")])
        # Nix's indirect GC registration names the out-link, so moving that link
        # alone loses GC protection. Root the verified store output at a stable,
        # content-derived path as well. Keep older outputs rooted across failures
        # in the later plugin transaction; repeated installs reuse the same root.
        roots = destination.parent / ".onnxruntime-roots"
        roots.mkdir(exist_ok=True)
        root = roots / hashlib.sha256(os.fsencode(target)).hexdigest()
        command([nix, "build", str(target), "--out-link", str(root)], timeout=1800)
        if not root.is_symlink() or root.resolve(strict=True) != target:
            raise ValueError("ONNX Runtime GC root does not match the verified output")
        os.replace(candidate, destination)


def installation_files(sr_plugin, home):
    helper = Path(__file__).resolve()
    files = [(helper, home / ".local/bin/id14-remix.py", 0o755)]
    supplied = os.environ.get("ID14_REMIX_PLUGIN_SOURCE")
    plugin = Path(supplied).expanduser().resolve() if supplied else sr_plugin.with_name("libid14_remix_ladspa.so")
    install_remix = bool(supplied) or plugin.exists()
    if install_remix:
        if not plugin.is_file():
            raise ValueError("remix plugin source is not a file")
        # Isolate invalid native libraries from the install transaction process.
        command([sys.executable, str(helper), "_probe", str(plugin)])
        files.append((plugin, home / ".local/lib/ladspa/libid14_remix_ladspa.so", 0o755))
    model = os.environ.get("ID14_REMIX_MODEL")
    if model:
        model = Path(model).expanduser().resolve()
        if not model.is_file():
            raise ValueError("remix model source is not a file")
        files.append((model, home / ".local/share/id14-sr/remix.onnx", 0o644))
    if install_remix:
        prepare_ort_library(home)
    return files


def main(args):
    if args == ["check"]:
        if plugin_path().is_file():
            read_state()
    elif args[:1] == ["_probe"] and len(args) == 2:
        probe_plugin(args[1])
    elif args[:1] == ["_probe_ort"] and len(args) == 2:
        print(probe_ort_library(args[1]))
    elif args[:1] == ["graph"] and len(args) == 2:
        print(graph(args[1]))
    elif args[:1] == ["apply"] and len(args) == 2:
        apply(args[1])
    elif args[:1] == ["remix"]:
        args = args[1:]
        if args in ([], ["status"], ["json"], ["status", "--json"]):
            print_status(args in (["json"], ["status", "--json"]))
        elif args in (["on"], ["off"]):
            print_status(applied=apply_all(save_change("enabled", args[0] == "on")))
        else:
            if args[:1] == ["amount"]:
                args = args[1:]
            if len(args) != 2 or args[0] not in (*PORTS, "voice"):
                raise ValueError("usage: id14-sr remix [status [--json]|json|on|off|vocals|drums|bass|other DB]")
            key = "vocals" if args[0] == "voice" else args[0]
            print_status(applied=apply_all(save_change(key, amount(args[1]))))
    else:
        raise ValueError("invalid remix helper command")


if __name__ == "__main__":
    try:
        main(sys.argv[1:])
    except (ValueError, OSError, RuntimeError, AttributeError, subprocess.TimeoutExpired) as exc:
        print(json.dumps(error(str(exc))), file=sys.stderr)
        sys.exit(1)
