"""Check runtime discovery using fresh processes and the actual release LADSPA ABI."""
import argparse
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import time

from verify_so import Host, ROOT


def probe(args):
    # Each child gets its own native API OnceLock and loader search environment.
    import numpy as np
    from verify_state_so import read_state

    expected_state = 1 if args.expected_library else 5
    directory = Path(os.environ["XDG_RUNTIME_DIR"]) / "id14-sr/remix-state"
    with Host(args.so, expected_state=expected_state) as host:
        mapped = {
            line.split()[-1]
            for line in Path("/proc/self/maps").read_text().splitlines()
            if "libonnxruntime.so" in line
        }
        if args.expected_library:
            assert mapped == {str(args.expected_library.resolve())}, mapped
        else:
            assert not mapped, mapped
        deadline = time.monotonic() + 5
        while True:
            paths = list(directory.glob("*.json"))
            state = read_state(paths[0]) if len(paths) == 1 else None
            if state is not None and state["state"] == expected_state:
                break
            assert time.monotonic() < deadline, (paths, state)
            time.sleep(0.005)
        mismatch = None
        if expected_state == 5:
            x = np.random.default_rng(17).uniform(-0.1, 0.1, (16384, 2)).astype(np.float32)
            x[::17, 0] = -0.0
            y = host.process(x)
            mismatch = int(np.count_nonzero(
                y[host.latency:].view(np.uint32) != x[:-host.latency].view(np.uint32)
            ))
            assert mismatch == 0 and host.state == 5
            assert state["state_name"] == "ModelInvalid"
        result = {
            "check": args.case, "mapped_libraries": sorted(mapped),
            "state": host.state, "published_state": state["state"],
            "published_state_name": state["state_name"], "bit_mismatches": mismatch,
        }
    assert not list(directory.glob("*.json"))
    result["cleanup_removed_state"] = True
    print(json.dumps(result, sort_keys=True), flush=True)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--so", type=Path, required=True)
    parser.add_argument("--library", type=Path)
    parser.add_argument("--build-library", help="Exact ID14_ORT_LIBRARY used for the release build")
    parser.add_argument("--probe", action="store_true")
    parser.add_argument("--case")
    parser.add_argument("--expected-library", type=Path)
    args = parser.parse_args()
    if args.probe:
        probe(args)
        return
    assert args.library and args.library.is_file() and args.build_library
    source = (ROOT / "src/model.rs").read_text()
    assert not re.search(r'(?:option_env|env)!\s*\(\s*"ID14_ORT_LIBRARY"', source)
    binary = args.so.read_bytes()
    forbidden = [args.build_library, str(args.library), str(args.library.resolve())]
    for value in forbidden:
        assert value.encode() not in binary, ("embedded build runtime path", value)
    print(json.dumps({"check": "no_baked_runtime_path", "forbidden_paths": forbidden,
                      "matches": 0, "compile_time_env_macros": 0}), flush=True)

    with tempfile.TemporaryDirectory(prefix="library-discovery-", dir=ROOT / ".build") as scratch:
        root = Path(scratch)
        # Distinct copies let /proc/self/maps prove precedence even when all
        # candidates are usable; the HOME installation is an actual symlink.
        explicit = root / "explicit/libonnxruntime.so"
        normal = root / "loader/libonnxruntime.so"
        for path in (explicit, normal):
            path.parent.mkdir()
            shutil.copyfile(args.library, path)
        installed_home = root / "installed-home"
        link = installed_home / ".local/lib/id14-sr/onnxruntime"
        link.parent.mkdir(parents=True)
        link.symlink_to(args.library.resolve().parent.parent, target_is_directory=True)
        empty_home = root / "empty-home"
        empty_home.mkdir()
        empty_search = root / "empty-search"
        empty_search.mkdir()
        invalid = root / "invalid.so"
        invalid.write_text("not a shared library")
        missing = root / "missing.so"
        cases = [
            ("env_before_home_and_loader", explicit, installed_home, normal.parent, explicit),
            ("home_before_loader", None, installed_home, normal.parent, args.library),
            ("missing_env_falls_through_to_home", missing, installed_home, normal.parent, args.library),
            ("invalid_env_falls_through_to_home", invalid, installed_home, normal.parent, args.library),
            ("normal_loader_search", None, empty_home, normal.parent, normal),
            ("missing_env_and_home_fall_through", missing, empty_home, normal.parent, normal),
            ("unset_home_uses_loader", None, None, normal.parent, normal),
            ("all_absent_visible_neutral", None, empty_home, empty_search, None),
            ("bad_override_all_absent_visible_neutral", missing, empty_home, empty_search, None),
        ]
        for name, override, home, search, expected in cases:
            env = dict(os.environ)
            env.pop("ID14_ORT_LIBRARY", None)
            env.pop("HOME", None)
            if override is not None:
                env["ID14_ORT_LIBRARY"] = str(override)
            if home is not None:
                env["HOME"] = str(home)
            env["LD_LIBRARY_PATH"] = str(search)
            env["XDG_RUNTIME_DIR"] = str(root / name)
            command = [sys.executable, str(Path(__file__).resolve()), "--so", str(args.so.resolve()),
                       "--probe", "--case", name]
            if expected is not None:
                command.extend(["--expected-library", str(expected.resolve())])
            subprocess.run(command, env=env, check=True, timeout=30)
        # Keep the original native-runtime failure regression under the same
        # isolated HOME/loader environment as the all-absent probe above.
        subprocess.run([
            sys.executable, str(ROOT / "tools/verify_runtime_failure.py"),
            "--so", str(args.so.resolve()), "--library", str(missing),
        ], env=env, check=True, timeout=30)
    print("LIBRARY_DISCOVERY_VERIFICATION_OK", flush=True)


if __name__ == "__main__":
    main()
