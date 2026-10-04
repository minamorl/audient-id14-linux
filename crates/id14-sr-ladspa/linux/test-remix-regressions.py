#!/usr/bin/env python3
"""Execute existing suites without reading them, with isolated HOME and guards."""
import os
from pathlib import Path
import subprocess
import sys
import tempfile

here = Path(__file__).resolve().parent
root = here.parents[2]
failed = False
with tempfile.TemporaryDirectory(prefix=".remix-regression-", dir=here) as temporary:
    base = Path(temporary)
    for name in ("bin", "home", "runtime", "state", "config", "data"):
        (base / name).mkdir()
    for name in ("pw-dump", "pw-cli", "wpctl", "pw-metadata", "systemctl"):
        path = base / "bin" / name
        path.write_text("#!/bin/sh\nexit 91\n")
        path.chmod(0o755)
    env = dict(os.environ, HOME=str(base / "home"), TMPDIR=temporary,
               XDG_RUNTIME_DIR=str(base / "runtime"), XDG_STATE_HOME=str(base / "state"),
               XDG_CONFIG_HOME=str(base / "config"), XDG_DATA_HOME=str(base / "data"),
               PATH=str(base / "bin") + ":" + os.environ["PATH"], PYTHONDONTWRITEBYTECODE="1",
               PIPEWIRE_REMOTE="hermetic-test-only", DBUS_SESSION_BUS_ADDRESS="unix:path=" + str(base / "no-bus"))
    for variable in ("ID14_REMIX_PLUGIN", "ID14_REMIX_PLUGIN_SOURCE", "ID14_REMIX_MODEL", "ID14_SR_PLUGIN"):
        env.pop(variable, None)
    commands = [(["bash", "crates/id14-sr-ladspa/linux/test-mix-cli.sh"], root),
                (["python3", "test-lifecycle.py"], here)]
    if sys.argv[1:] == ["mix"]:
        commands = commands[:1]
    elif sys.argv[1:] == ["lifecycle"]:
        commands = commands[1:]
    elif sys.argv[1:]:
        raise SystemExit("usage: test-remix-regressions.py [mix|lifecycle]")
    for command, cwd in commands:
        print("COMMAND: " + " ".join(command), flush=True)
        result = subprocess.run(command, cwd=cwd, env=env)
        print("EXIT=" + str(result.returncode), flush=True)
        failed |= result.returncode != 0
sys.exit(int(failed))
