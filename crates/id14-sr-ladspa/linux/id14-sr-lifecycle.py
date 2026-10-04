#!/usr/bin/env python3
"""Journaled user installation and output-file-gated USB restoration.

Reinstallation restarts a running filter only after publishing all files. A
failed start restores the prior files and restarts the prior plugin. A detached
guard recovers after installer death. USB connection starts a bounded oneshot;
there is no timer or idle observer process.
"""

import contextlib
import ctypes
import datetime
import fcntl
import json
import os
from pathlib import Path
import random
import resource
import shutil
import signal
import subprocess
import sys
import tempfile
import time


FILTER = "id14-sr-filter.service"
RESTORE = "id14-sr-restore.service"
LEGACY_TIMER = "id14-sr-restore.timer"
DEVICE = r"dev-snd-by\x2did-usb\x2dAudient_Audient_iD14\x2d00.device"
OUTPUT_NAMES = {
    "line": "alsa_output.usb-Audient_Audient_iD14-00.HiFi__Line__sink",
    "headphones": "alsa_output.usb-Audient_Audient_iD14-00.HiFi__Headphones__sink",
}


def identity():
    value = (time.time_ns() // 1_000_000 << 80) | random.SystemRandom().getrandbits(80)
    alphabet = "0123456789ABCDEFGHJKMNPQRSTVWXYZ"
    return "".join(alphabet[(value >> (5 * i)) & 31] for i in reversed(range(26)))


TRACE = identity()


def error(code, message, details=None):
    print(json.dumps({"ts": datetime.datetime.now(datetime.timezone.utc).isoformat(),
                      "level": "error", "trace_id": TRACE, "msg": message,
                      "code": code, "message": message, "details": details}),
          file=sys.stderr, flush=True)


def paths():
    home = Path(os.environ["HOME"])
    state = Path(os.environ.get("XDG_STATE_HOME", str(home / ".local/state")))
    return home, state / "id14-sr", state / "id14-sr-lifecycle"


def run(args, check=True, timeout=60):
    result = subprocess.run([str(a) for a in args], stdout=subprocess.PIPE,
                            stderr=subprocess.PIPE, text=True, timeout=timeout)
    if check and result.returncode:
        raise RuntimeError(f"command failed ({result.returncode}): {args[0]}")
    return result


def systemctl(*args, check=True, timeout=60):
    return run(["systemctl", "--user", *args], check=check, timeout=timeout)


def flags(unit):
    return {"enabled": systemctl("is-enabled", "--quiet", unit, check=False).returncode == 0,
            "active": systemctl("is-active", "--quiet", unit, check=False).returncode == 0}


def sync_directory(path):
    fd = os.open(path, os.O_RDONLY | os.O_DIRECTORY)
    try:
        os.fsync(fd)
    finally:
        os.close(fd)


def save_json(path, value):
    tmp = path.with_name(path.name + ".new")
    with tmp.open("w") as stream:
        json.dump(value, stream)
        stream.flush()
        os.fsync(stream.fileno())
    os.replace(tmp, path)
    sync_directory(path.parent)


def remove(path):
    if path.is_dir() and not path.is_symlink():
        shutil.rmtree(path)
    else:
        path.unlink(missing_ok=True)


def copy(source, destination):
    if source.is_dir() and not source.is_symlink():
        shutil.copytree(source, destination, symlinks=True)
    else:
        shutil.copy2(source, destination, follow_symlinks=False)


def durable_tree(path):
    if path.is_symlink():
        return
    if path.is_dir():
        for child in path.iterdir():
            durable_tree(child)
        sync_directory(path)
    else:
        with path.open("rb") as stream:
            os.fsync(stream.fileno())


@contextlib.contextmanager
def locked(base, blocking=True):
    base.mkdir(parents=True, exist_ok=True, mode=0o700)
    # Lock the persistent directory inode; failed installs leave no lock file.
    fd = os.open(base, os.O_RDONLY | os.O_DIRECTORY)
    try:
        try:
            fcntl.flock(fd, fcntl.LOCK_EX | (0 if blocking else fcntl.LOCK_NB))
        except BlockingIOError:
            yield False
            return
        try:
            yield True
        finally:
            fcntl.flock(fd, fcntl.LOCK_UN)
    finally:
        os.close(fd)


def restore_flags(unit, prior):
    current = flags(unit)
    if current["active"] and not prior["active"]:
        systemctl("stop", unit)
    if current["enabled"] != prior["enabled"]:
        systemctl("enable" if prior["enabled"] else "disable", unit)
    if prior["active"] and not current["active"]:
        systemctl("start", unit)


def rollback(transaction, force=False):
    manifest = transaction / "journal.json"
    data = json.loads(manifest.read_text())
    if data["phase"] != "applying" and not force:
        shutil.rmtree(transaction)
        return
    home, _, _ = paths()
    if data["fresh"] and data["activation_started"]:
        run([home / ".local/bin/id14-sr", "off"])
    # Stop any candidate before restoring the shared library it has loaded.
    if data.get("filter_touched"):
        systemctl("stop", FILTER)
    auxiliaries = data.get("aux", {LEGACY_TIMER: data.get("timer", {"active": False, "enabled": False})})
    for unit, prior in auxiliaries.items():
        if flags(unit)["active"] and not prior["active"]:
            systemctl("stop", unit)
        if flags(unit)["enabled"] and not prior["enabled"]:
            systemctl("disable", unit)
    for index, item in reversed(list(enumerate(data["files"]))):
        target = Path(item["path"])
        backup = transaction / f"backup-{index}"
        # A killed copy may have left a destination-local staging file.
        target.with_name(target.name + ".id14-sr-new").unlink(missing_ok=True)
        if item["exists"]:
            stage = target.with_name(target.name + ".id14-sr-rollback")
            remove(stage)
            copy(backup, stage)
            durable_tree(stage)
            if target.is_dir() and not target.is_symlink():
                remove(target)
            os.replace(stage, target)
        else:
            remove(target)
        sync_directory(target.parent)
    systemctl("daemon-reload")
    for unit, prior in auxiliaries.items():
        # Starting our own oneshot under this lock would deadlock. Queue it;
        # it runs after rollback releases the lock, if it previously ran.
        current = flags(unit)
        if current["enabled"] != prior["enabled"]:
            systemctl("enable" if prior["enabled"] else "disable", unit)
        if prior["active"] and not current["active"]:
            systemctl("--no-block", "start", unit)
    if data["fresh"] or data.get("filter_touched"):
        restore_flags(FILTER, data["filter"])
        if data["filter"]["active"]:
            run([home / ".local/bin/id14-sr", "reconcile"])
            if not flags(FILTER)["active"]:
                raise RuntimeError("prior filter did not recover")
    data["phase"] = "rolled_back"
    save_json(manifest, data)
    shutil.rmtree(transaction)


def recover(base):
    for transaction in sorted(base.glob("transaction-*")):
        if (transaction / "journal.json").exists():
            rollback(transaction)
        else:
            shutil.rmtree(transaction)


def guard(base, transaction, fd):
    with os.fdopen(fd, "rb") as pipe:
        pipe.read()
    with locked(base):
        if (transaction / "journal.json").exists():
            rollback(transaction)


def probe(plugin):
    resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
    class Hint(ctypes.Structure):
        _fields_ = [("descriptor", ctypes.c_int), ("lower", ctypes.c_float),
                    ("upper", ctypes.c_float)]

    class Descriptor(ctypes.Structure):
        _fields_ = [("unique_id", ctypes.c_ulong), ("label", ctypes.c_char_p),
                    ("properties", ctypes.c_int), ("name", ctypes.c_char_p),
                    ("maker", ctypes.c_char_p), ("copyright", ctypes.c_char_p),
                    ("port_count", ctypes.c_ulong),
                    ("ports", ctypes.POINTER(ctypes.c_int)),
                    ("port_names", ctypes.POINTER(ctypes.c_char_p)),
                    ("hints", ctypes.POINTER(Hint))]

    library = ctypes.CDLL(str(plugin))
    descriptor = library.ladspa_descriptor
    descriptor.argtypes = [ctypes.c_ulong]
    descriptor.restype = ctypes.POINTER(Descriptor)
    pointer = descriptor(0)
    if not pointer:
        raise ValueError("LADSPA descriptor 0 is missing")
    value = pointer.contents
    if value.label != b"id14_sr_stereo" or value.port_count != 5:
        raise ValueError("unexpected LADSPA label or port count")
    if not value.ports or not value.port_names or not value.hints:
        raise ValueError("missing LADSPA port description")
    if [value.ports[i] for i in range(5)] != [9, 9, 10, 10, 5]:
        raise ValueError("unexpected LADSPA port types")
    if [value.port_names[i] for i in range(5)] != [b"Input L", b"Input R", b"Output L", b"Output R", b"Mix"]:
        raise ValueError("unexpected LADSPA port names")
    hint = value.hints[4]
    # -1 selects auto; DEFAULT_MINIMUM makes that the host's initial value.
    if hint.descriptor != 0x43 or hint.lower != -1 or hint.upper != 200:
        raise ValueError("unexpected LADSPA Mix range")


def remix_install_destinations(plugin, home):
    import json
    import os
    import runpy
    import sys

    helper_path = Path(__file__).with_name("id14-remix.py")
    # A legacy SR-only package does not ship the optional remix helper.
    if not helper_path.is_file():
        if (os.environ.get("ID14_REMIX_PLUGIN_SOURCE") or os.environ.get("ID14_REMIX_MODEL")
                or plugin.with_name("libid14_remix_ladspa.so").exists()):
            raise RuntimeError("remix helper missing from package")
        return []
    helper = runpy.run_path(str(helper_path))
    try:
        return helper["installation_files"](plugin, home)
    except (ValueError, OSError, RuntimeError) as exc:
        print(json.dumps(helper["error"](str(exc))), file=sys.stderr)
        raise SystemExit(1) from None


def install(arguments):
    if len(arguments) > 1:
        raise ValueError("usage: install-user.sh [PLUGIN.so]")
    source = Path(__file__).resolve().parent
    plugin = Path(arguments[0]).resolve() if arguments else source.parents[2] / "target/release/libid14_sr_ladspa.so"
    for command in ("pw-dump", "wpctl", "pw-metadata", "jq", "systemctl"):
        if shutil.which(command) is None:
            raise ValueError(f"missing command: {command}")
    validation = run([sys.executable, Path(__file__).resolve(), "probe", plugin], check=False)
    if validation.returncode:
        reason = "native plugin probe failed"
        try:
            reason = json.loads(validation.stderr.splitlines()[-1])["message"]
        except (ValueError, IndexError, KeyError):
            pass
        raise ValueError(f"plugin validation failed: {reason}")
    home, state, base = paths()
    cli = home / ".local/bin/id14-sr"
    installed_plugin = home / ".local/lib/ladspa/libid14_sr_ladspa.so"
    remix_destinations = remix_install_destinations(plugin, home)
    units = home / ".config/systemd/user"
    destinations = [
        *remix_destinations,
        (source / "id14-sr", cli, 0o755),
        (plugin, installed_plugin, 0o755),
        (source / FILTER, units / FILTER, 0o644),
        (Path(__file__).resolve(), home / ".local/lib/id14-sr/lifecycle.py", 0o755),
        (source / RESTORE, units / RESTORE, 0o644),
    ]
    with locked(base):
        recover(base)
        transaction = Path(tempfile.mkdtemp(prefix="transaction-", dir=base))
        manifest = transaction / "journal.json"
        guard_process = None
        write_fd = None
        committed = False
        switch_seconds = None
        data = {"phase": "preparing", "files": [], "fresh": not cli.exists(),
                "activation_started": False, "filter_touched": False,
                "aux": {RESTORE: flags(RESTORE), LEGACY_TIMER: flags(LEGACY_TIMER)},
                "filter": flags(FILTER)}
        try:
            targets = [target for _, target, _ in destinations]
            targets.extend([units / LEGACY_TIMER, state,
                            home / ".config/pipewire/filter-chain.conf.d/90-id14-sr.conf"])
            for index, target in enumerate(targets):
                target.parent.mkdir(parents=True, exist_ok=True)
                exists = target.exists() or target.is_symlink()
                if exists:
                    copy(target, transaction / f"backup-{index}")
                data["files"].append({"path": str(target), "exists": exists})
            for index, (origin, _, mode) in enumerate(destinations):
                staged = transaction / f"stage-{index}"
                shutil.copyfile(origin, staged)
                if origin.name == "id14-sr-restore.service":
                    state_home = str(state.parent).replace("\\", "\\\\").replace('"', '\\"').replace("%", "%%")
                    if any(char in state_home for char in ('\n', '\r')):
                        raise ValueError("newline in installation path")
                    staged.write_text(staged.read_text().replace("@STATE_HOME@", state_home))
                staged.chmod(mode)
            durable_tree(transaction)
            data["phase"] = "applying"
            save_json(manifest, data)
            read_fd, write_fd = os.pipe()
            try:
                guard_process = subprocess.Popen(
                    [sys.executable, str(Path(__file__).resolve()), "guard", str(base), str(transaction), str(read_fd)],
                    pass_fds=(read_fd,), start_new_session=True,
                    stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            finally:
                os.close(read_fd)
            def publish(index):
                _, target, _ = destinations[index]
                staged = target.with_name(target.name + ".id14-sr-new")
                try:
                    shutil.copy2(transaction / f"stage-{index}", staged)
                    durable_tree(staged)
                    os.replace(staged, target)
                    sync_directory(target.parent)
                finally:
                    staged.unlink(missing_ok=True)
            for index in (3, 4):
                publish(index)
            if any(data["aux"][LEGACY_TIMER].values()) or (units / LEGACY_TIMER).exists():
                systemctl("disable", "--now", LEGACY_TIMER)
                (units / LEGACY_TIMER).unlink(missing_ok=True)
                sync_directory(units)
            systemctl("daemon-reload")
            # enable only: --now would wait on the same installation lock.
            systemctl("enable", RESTORE)
            for index in (0, 1, 2):
                publish(index)
            systemctl("daemon-reload")
            if data["fresh"]:
                data["activation_started"] = True
                save_json(manifest, data)
                run([cli, "on", "all"])
            elif data["filter"]["active"]:
                data["filter_touched"] = True
                save_json(manifest, data)
                switch_started = time.monotonic()
                systemctl("restart", FILTER)
                if not flags(FILTER)["active"]:
                    raise RuntimeError("new filter is not active")
                run([cli, "reconcile"])
                switch_seconds = time.monotonic() - switch_started
            data["phase"] = "committed"
            save_json(manifest, data)
            committed = True
            shutil.rmtree(transaction)
        except BaseException as exc:
            for sig in (signal.SIGINT, signal.SIGTERM, signal.SIGHUP):
                signal.signal(sig, signal.SIG_IGN)
            if not committed:
                if manifest.exists():
                    rollback(transaction, force=True)
                elif transaction.exists():
                    shutil.rmtree(transaction)
                raise
            # Publication is committed. Cleanup failure must not falsely
            # report a failed install whose prior version was not restored.
            error("ID14_SR_CLEANUP_PENDING", str(exc))
        finally:
            if write_fd is not None:
                os.close(write_fd)
    if guard_process is not None:
        try:
            guard_process.wait(timeout=60)
        except (subprocess.TimeoutExpired, InterruptedError):
            # The committed journal is safe for the next recovery invocation.
            pass
    print(f"installed command={cli} plugin={installed_plugin} service={units / FILTER}")
    if switch_seconds is not None:
        print(f"switch_seconds={switch_seconds:.6f}")


def restore(wait_seconds=20):
    home, state, base = paths()
    # An event concurrent with installation waits instead of being discarded.
    with locked(base):
        recover(base)
        output = state / "output"
        if not output.exists():
            return
        selected = output.read_text().strip()
        if selected not in ("all", "line", "headphones"):
            raise ValueError("invalid saved output selection")
        wanted = set(OUTPUT_NAMES.values()) if selected == "all" else {OUTPUT_NAMES[selected]}
        deadline = time.monotonic() + wait_seconds

        def remaining():
            value = deadline - time.monotonic()
            if value <= 0:
                raise TimeoutError("USB restoration exceeded the sink wait deadline")
            return value

        for attempt in range(6):
            if not output.exists() or output.read_text().strip() != selected:
                return
            if systemctl("is-active", "--quiet", DEVICE, check=False, timeout=remaining()).returncode:
                return
            nodes = json.loads(run(["pw-dump"], timeout=remaining()).stdout)
            present = {(node.get("info") or {}).get("props", {}).get("node.name") for node in nodes}
            if wanted.issubset(present):
                active = systemctl("is-active", "--quiet", FILTER, check=False, timeout=remaining()).returncode == 0
                if not active:
                    result = systemctl("enable", "--now", FILTER, check=False, timeout=remaining())
                else:
                    result = None
                if result is None or result.returncode == 0:
                    if run([home / ".local/bin/id14-sr", "reconcile"], check=False, timeout=remaining()).returncode == 0:
                        return
            if attempt < 5:
                delay = 0.5 * 2 ** attempt
                time.sleep(min(delay + random.uniform(0, delay), remaining()))
        raise RuntimeError("USB restoration failed after five retries")


def interrupted(signum, _frame):
    raise InterruptedError(f"interrupted by signal {signum}")


def main():
    for sig in (signal.SIGINT, signal.SIGTERM, signal.SIGHUP):
        signal.signal(sig, interrupted)
    try:
        command = sys.argv[1]
        if command == "install":
            install(sys.argv[2:])
        elif command == "restore":
            restore()
        elif command == "probe":
            probe(Path(sys.argv[2]))
        elif command == "guard":
            guard(Path(sys.argv[2]), Path(sys.argv[3]), int(sys.argv[4]))
        else:
            raise ValueError("unknown lifecycle command")
        return 0
    except BaseException as exc:
        error("ID14_SR_LIFECYCLE", str(exc))
        return 1


if __name__ == "__main__":
    sys.exit(main())
