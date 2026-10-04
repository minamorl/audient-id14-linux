"""Exercise remix-state-v1 through the actual LADSPA shared object."""
import argparse
import json
import os
from pathlib import Path
import tempfile
import time
import numpy as np
from verify_so import Host, ROOT, BLOCK, tone


def record(check, **values):
    print(json.dumps({"check": check, **values}, sort_keys=True), flush=True)


def read_state(path):
    value = json.loads(path.read_text())  # A partial final file must fail the test.
    assert set(value) == {"contract", "pid", "instance", "state", "state_name", "overloads", "model", "latency_frames", "updated_unix_ms"}
    assert value["contract"] == "remix-state-v1"
    assert value["pid"] == os.getpid()
    assert path.name == f'{value["pid"]}-{value["instance"]}.json'
    assert value["latency_frames"] == 3776
    return value


def wait_state(path, number, host=None):
    deadline = time.monotonic() + 5
    while time.monotonic() < deadline:
        if host is not None:
            host.block(np.zeros((BLOCK, 2), np.float32), measure=False)
        if path.exists():
            value = read_state(path)
            if value["state"] == number:
                return value
        time.sleep(0.005)
    raise AssertionError((path, number, "state timeout"))


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--so", type=Path, required=True)
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix="publication-", dir=ROOT / ".build") as scratch:
        root = Path(scratch)
        runtime = root / "runtime"
        os.environ["XDG_RUNTIME_DIR"] = str(runtime)
        directory = runtime / "id14-sr/remix-state"
        with Host(args.so) as h:
            deadline = time.monotonic() + 5
            while not list(directory.glob("*.json")):
                assert time.monotonic() < deadline
                time.sleep(0.005)
            path, = directory.glob("*.json")
            active = wait_state(path, 1, h)
            assert active["state_name"] == "Active"
            assert active["model"] == str(ROOT / "fixtures/voice.onnx")
            record("file_created", **active)

            h.controls[4].value = 0
            off = wait_state(path, 2, h)
            assert off["state_name"] == "Off"
            stamp = off["updated_unix_ms"]
            time.sleep(0.15)
            assert read_state(path)["updated_unix_ms"] == stamp
            deadline = time.monotonic() + 3
            while read_state(path)["updated_unix_ms"] == stamp:
                assert time.monotonic() < deadline
                time.sleep(0.005)
            elapsed = read_state(path)["updated_unix_ms"] - stamp
            assert elapsed >= 1000
            record("state_change_and_heartbeat", state=off["state"], state_name=off["state_name"], heartbeat_ms=elapsed)

            # Reactivation retains the instance filename and overload counter.
            h.d.deactivate(h.handle)
            h.d.activate(h.handle)
            wait_state(path, 2, h)
            assert list(directory.glob("*.json")) == [path]
            h.controls[4].value = 1
            wait_state(path, 1, h)
            count = read_state(path)["overloads"]
            h.stop_worker()
            overloaded = wait_state(path, 6, h)
            assert overloaded["overloads"] > count
            assert overloaded["state_name"] == "Overloaded"
            record("stopped_inference_visible", **overloaded)

            with Host(args.so, fixture="missing") as other:
                paths = list(directory.glob("*.json"))
                deadline = time.monotonic() + 5
                while len(paths) < 2:
                    assert time.monotonic() < deadline
                    time.sleep(0.005)
                    paths = list(directory.glob("*.json"))
                other_path, = [p for p in paths if p != path]
                wait_state(other_path, 4, other)
                h.close()
                assert not path.exists() and other_path.exists()
                record("owned_cleanup", removed=not path.exists(), other_instance_preserved=other_path.exists())
            assert not other_path.exists()
        assert not list(directory.iterdir())

        # Missing env, ENOTDIR and EACCES all leave the audio bit-identical to
        # the writable-publication run, including active correction after startup.
        blocked = root / "not-directory"
        blocked.write_text("blocked")
        readonly = root / "readonly"
        readonly.mkdir(mode=0o500)
        x = tone(3000, 0.025, seconds=1)
        try:
            for enabled in (0, 1):
                reference = None
                for mode, location in [("writable", runtime), ("unset", None), ("not_directory", blocked), ("permission_denied", readonly)]:
                    if location is None:
                        os.environ.pop("XDG_RUNTIME_DIR", None)
                    else:
                        os.environ["XDG_RUNTIME_DIR"] = str(location)
                    with Host(args.so, enabled=enabled) as h:
                        y = h.process(x)
                        # Exclude startup; masks and time-domain history are settled.
                        y = y[h.latency + 8192:]
                        if reference is None:
                            reference = y.copy()
                        mismatches = int(np.count_nonzero(y.view(np.uint32) != reference.view(np.uint32)))
                        record("publication_failure_audio", mode=mode, enabled=enabled, bit_mismatches=mismatches)
                        assert mismatches == 0
                assert not list(readonly.iterdir())
        finally:
            readonly.chmod(0o700)
    print("STATE_SO_VERIFICATION_OK", flush=True)


if __name__ == "__main__":
    main()
