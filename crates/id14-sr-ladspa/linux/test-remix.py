#!/usr/bin/env python3
"""Hermetic integration checks; existing target/harness source is never inspected."""

import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import unittest


HERE = Path(__file__).resolve().parent
HELPER = HERE / "id14-remix.py"
CLI = HERE / "id14-sr"


class RemixTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix=".remix-test-", dir=HERE)
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.home = self.root / "home"
        self.home.mkdir()
        self.bin = self.root / "bin"
        self.bin.mkdir()
        self.runtime = self.root / "runtime"
        self.runtime.mkdir()
        self.plugin = self.home / ".local/lib/ladspa/libid14_remix_ladspa.so"
        self.plugin.parent.mkdir(parents=True)
        self.plugin.touch()
        self.env = dict(os.environ, HOME=str(self.home), XDG_STATE_HOME=str(self.root / "state"),
                        XDG_CONFIG_HOME=str(self.root / "config"), XDG_DATA_HOME=str(self.root / "data"),
                        XDG_RUNTIME_DIR=str(self.runtime), TMPDIR=str(self.root),
                        PATH=str(self.bin) + os.pathsep + os.environ["PATH"],
                        ID14_REMIX_PLUGIN=str(self.plugin), ID14_SR_PLUGIN=str(self.plugin.with_name("libid14_sr_ladspa.so")),
                        REMIX_TEST_ROOT=str(self.root), PYTHONDONTWRITEBYTECODE="1",
                        DBUS_SESSION_BUS_ADDRESS="unix:path=" + str(self.root / "no-bus"),
                        PIPEWIRE_REMOTE="hermetic-test-only")
        self.env.pop("ID14_REMIX_PLUGIN_SOURCE", None)
        self.env.pop("ID14_REMIX_MODEL", None)
        self.plugin.with_name("libid14_sr_ladspa.so").touch()
        self.dump = self.root / "dump.json"
        self.nodes([11, 22])
        self.script("pw-dump", 'cat "$REMIX_TEST_ROOT/dump.json"\n')
        self.script("pw-cli", '''python3 - "$@" <<'PY'
import json, os, pathlib, sys
root = pathlib.Path(os.environ['REMIX_TEST_ROOT'])
with (root / 'calls.jsonl').open('a') as stream:
    stream.write(json.dumps(sys.argv[1:]) + '\\n')
sys.exit(1 if (root / 'fail-cli').exists() else 0)
PY
''')
        for name in ("wpctl", "pw-metadata", "systemctl", "jq"):
            self.script(name, 'exit 0\n')

    def script(self, name, text):
        path = self.bin / name
        path.write_text("#!/usr/bin/env bash\n" + text)
        path.chmod(0o755)

    def nodes(self, ids):
        self.dump.write_text(json.dumps([
            {"id": i, "type": "PipeWire:Interface:Node", "info": {"params": {"Props": [
                {"params": {"sr:Mix": 100, "remix:Enabled": 1}}]}}} for i in ids] + [
            {"id": 99, "type": "PipeWire:Interface:Node", "info": {"params": {"Props": [
                {"params": {"sr:Mix": 75}}]}}}]))

    def run_command(self, args, expected=0, env=None):
        result = subprocess.run(args, env=self.env if env is None else env, text=True,
                                capture_output=True, timeout=30, cwd=HERE)
        self.assertEqual(result.returncode, expected, result.stdout + result.stderr)
        return result

    def cli(self, *args, expected=0):
        return self.run_command(["bash", str(CLI), "remix", *args], expected)

    def data(self):
        return json.loads(self.cli("json").stdout)

    def calls(self):
        path = self.root / "calls.jsonl"
        return [json.loads(line) for line in path.read_text().splitlines()] if path.exists() else []

    def telemetry(self, state=1, pid=None, instance=1, age=0, **changes):
        pid = os.getpid() if pid is None else pid
        names = ["Loading", "Active", "Off", "ZeroAmounts", "ModelMissing", "ModelInvalid", "Overloaded", "PeakProtected", "UnsupportedRate"]
        item = dict(contract="remix-state-v1", pid=pid, instance=instance, state=state,
                    state_name=names[state], overloads=2 if state == 6 else 0, model=None,
                    latency_frames=3776, updated_unix_ms=time.time_ns() // 1_000_000 - age)
        item.update(changes)
        directory = self.runtime / "id14-sr/remix-state"
        directory.mkdir(parents=True, exist_ok=True)
        path = directory / f"{pid}-{instance}.json"
        path.write_text(json.dumps(item))
        return path

    def test_defaults_and_cli_independent_amounts(self):
        self.assertEqual(self.data()["amounts_db"], {"vocals": 3, "drums": 0, "bass": 0, "other": -3})
        self.assertTrue(self.data()["enabled"])
        for part, value in (("voice", "4.25"), ("drums", "-6"), ("bass", "6"), ("other", "-2.5")):
            self.cli(part, value)
        self.assertEqual(self.data()["amounts_db"], {"vocals": 4.25, "drums": -6, "bass": 6, "other": -2.5})
        for invalid in ("7", "-6.1", "nan", "inf", "x", "$(touch BAD)"):
            error = self.cli("vocals", invalid, expected=1)
            self.assertIn("trace_id", json.loads(error.stderr))
        self.assertEqual(self.data()["amounts_db"]["vocals"], 4.25)
        self.assertFalse((HERE / "BAD").exists())

    def test_both_outputs_and_one_action_sr_only(self):
        self.cli("bass", "2")
        self.cli("off")
        controls = [call for call in self.calls() if call[2] == "Props"]
        self.assertEqual({call[1] for call in controls}, {"11", "22"})
        self.assertTrue(all('"remix:Enabled" 0.0' in call[3] for call in controls[-2:]))
        self.assertTrue(all('"sr:Mix"' not in call[3] for call in controls))
        self.assertEqual(self.data()["amounts_db"]["bass"], 2)
        self.assertFalse(self.data()["enabled"])
        self.assertTrue(all("rate = 4800" in c[3] for c in self.calls() if c[2] == "ProcessLatency"))
        self.assertTrue(all(c[0] == "set-param" for c in self.calls()))

    def test_persistence_reapply_and_unplugged_controls(self):
        self.nodes([])
        self.cli("vocals", "5")
        self.cli("off")
        self.assertEqual(self.calls(), [])
        self.nodes([33, 44])
        for node in (33, 44):
            self.run_command([sys.executable, str(HELPER), "apply", str(node)])
        controls = [call for call in self.calls() if call[2] == "Props"]
        self.assertEqual({c[1] for c in controls}, {"33", "44"})
        self.assertTrue(all('"remix:Vocals" 5.000000' in c[3] and '"remix:Enabled" 0.0' in c[3] for c in controls))
        env = dict(self.env, XDG_RUNTIME_DIR=str(self.root / "next-login-runtime"))
        result = self.run_command(["bash", str(CLI), "remix", "json"], env=env)
        self.assertEqual(json.loads(result.stdout)["amounts_db"]["vocals"], 5)
        self.assertFalse(json.loads(result.stdout)["enabled"])

    def test_apply_failure_keeps_desired_state_and_reports_error(self):
        (self.root / "fail-cli").touch()
        result = self.cli("other", "-4", expected=1)
        self.assertEqual(json.loads(result.stderr)["code"], "REMIX_ERROR")
        self.assertEqual(self.data()["amounts_db"]["other"], -4)
        (self.root / "fail-cli").unlink()
        self.run_command([sys.executable, str(HELPER), "apply", "11"])
        self.assertIn('"remix:Other" -4.000000', self.calls()[-2][3])

    def test_pw_dump_array_controls_and_reconcile_application_seam(self):
        self.dump.write_text(json.dumps([
            {"id": 55, "type": "PipeWire:Interface:Node", "info": {"params": {"Props": [
                {"params": ["sr:Mix", 100, "remix:Enabled", 1]}]}}}]))
        self.cli("bass", "-3")
        self.assertEqual(self.calls()[0][1], "55")
        shell = '''source "$1" remix status >/dev/null
node_id() { printf '66\\n'; }
filter_main() { printf 'fixture-filter\\n'; }
apply_remix_to_output headphones
'''
        self.run_command(["bash", "-c", shell, "test-remix", str(CLI)])
        self.assertEqual(self.calls()[-2][1], "66")
        self.assertIn('"remix:Bass" -3.000000', self.calls()[-2][3])

    def test_runtime_telemetry_is_not_desired_state(self):
        self.assertEqual(self.data()["state_name"], "Unknown")
        self.telemetry(state=1)
        self.telemetry(state=6, instance=2)
        result = self.cli("json")
        self.assertEqual(len(result.stdout.splitlines()), 1)
        data = json.loads(result.stdout)
        self.assertTrue(data["overloaded"])
        self.assertEqual(data["state_name"], "Overloaded")
        self.assertEqual(data["class"], "remix-overloaded")
        self.assertEqual(len(data["instances"]), 2)
        self.assertIn("overloads=2", self.cli("status").stdout)
        self.assertEqual(json.loads(self.cli("status", "--json").stdout)["state"], 6)

    def test_stale_dead_future_malformed_and_all_runtime_states(self):
        self.telemetry(state=6, age=6000)
        self.telemetry(state=6, pid=2_147_483_647)
        self.telemetry(state=6, pid=10**30)
        self.telemetry(state=6, instance=3, age=-60000)
        bad = self.telemetry(state=6, instance=4)
        bad.write_text("{")
        self.telemetry(state=6, instance=5, contract="wrong")
        self.assertEqual(self.data()["instances"], [])
        for state in range(9):
            self.telemetry(state=state, instance=8)
            self.assertEqual(self.data()["state"], state)

    def test_graph_order_links_defaults_saved_values_and_absent_plugin(self):
        result = self.run_command([sys.executable, str(HELPER), "graph", "145"])
        graph = result.stdout
        self.assertLess(graph.index("name = remix"), graph.index("name = sr"))
        self.assertIn('output = "remix:Output L" input = "sr:Input L"', graph)
        self.assertIn('output = "remix:Output R" input = "sr:Input R"', graph)
        self.assertIn('inputs = [ "remix:Input L" "remix:Input R" ]', graph)
        self.assertIn('outputs = [ "sr:Output L" "sr:Output R" ]', graph)
        self.assertIn("Vocals = 3 Drums = 0 Bass = 0 Other = -3 Enabled = 1", graph)
        self.assertIn("Mix = 145", graph)
        self.cli("vocals", "-1.75")
        self.cli("off")
        graph = self.run_command([sys.executable, str(HELPER), "graph", "145"]).stdout
        self.assertIn("Vocals = -1.75", graph)
        self.assertIn("Enabled = 0", graph)
        self.assertIn("name = remix", graph)
        self.plugin.unlink()
        graph = self.run_command([sys.executable, str(HELPER), "graph", "145"]).stdout
        self.assertNotIn("remix", graph)
        self.assertIn("Mix = 145", graph)
        self.assertIn('inputs = [ "sr:Input L" "sr:Input R" ]', graph)
        self.assertEqual(self.data()["state_name"], "PluginMissing")

    def test_actual_emit_modules_seam(self):
        # Execute the public seam; do not open the existing shell implementation.
        self.cli("drums", "1.5")
        self.cli("off")
        for output in ("line", "headphones"):
            shell = f'source "$1" remix status >/dev/null; emit_modules {output} 145'
            result = self.run_command(["bash", "-c", shell, "test-remix", str(CLI), output])
            self.assertLess(result.stdout.index("name = remix"), result.stdout.index("name = sr"))
            self.assertIn("Drums = 1.5", result.stdout)
            self.assertIn("Enabled = 0", result.stdout)
            self.assertIn('output = "remix:Output R" input = "sr:Input R"', result.stdout)
        self.plugin.unlink()
        result = self.run_command(["bash", "-c", shell, "test-remix", str(CLI)])
        self.assertNotIn("name = remix", result.stdout)
        self.assertIn("name = sr", result.stdout)

    def build_plugin(self, label="id14_remix_stereo", count=11, flags=None):
        source = self.root / "fixture.c"
        library = self.root / "fixture.so"
        flags = flags or "9,9,10,10,5,5,5,5,5,6,6"
        source.write_text('''
typedef struct { int descriptor; float lower, upper; } Hint;
typedef struct {
 unsigned long id; const char *label; int properties;
 const char *name, *maker, *copyright; unsigned long count;
 const int *flags; const char *const *names; const Hint *hints;
} Descriptor;
static const int flags[] = {FLAGS};
static const char *const names[] = {"Input L","Input R","Output L","Output R",
 "Vocals","Drums","Bass","Other","Enabled","State","latency"};
static const Hint hints[] = {{0,0,0},{0,0,0},{0,0,0},{0,0,0},
 {3,-6,6},{3,-6,6},{3,-6,6},{3,-6,6},{3,0,1},{3,0,8},{3,0,65536}};
static const Descriptor descriptor = {1,"LABEL",0,"Fixture","Test","None",COUNT,flags,names,hints};
const Descriptor *ladspa_descriptor(unsigned long i) { return i == 0 ? &descriptor : 0; }
'''.replace("FLAGS", flags).replace("LABEL", label).replace("COUNT", str(count)))
        self.run_command(["cc", "-shared", "-fPIC", str(source), "-o", str(library)])
        return library

    def test_native_probe_accepts_contract_and_rejects_wrong_label_ports(self):
        for label, count, flags, expected in (
            ("id14_remix_stereo", 11, None, 0),
            ("wrong", 11, None, 1),
            ("id14_remix_stereo", 10, None, 1),
            ("id14_remix_stereo", 11, "9,9,10,10,5,5,5,5,5,5,6", 1),
        ):
            library = self.build_plugin(label, count, flags)
            self.run_command([sys.executable, str(HELPER), "_probe", str(library)], expected)

    def test_installer_destinations_and_model_opt_in(self):
        library = self.build_plugin()
        model = self.root / "model.onnx"
        model.write_bytes(b"model-fixture")
        self.env["ID14_REMIX_PLUGIN_SOURCE"] = str(library)
        self.env["ID14_REMIX_MODEL"] = str(model)
        code = '''import json, pathlib, runpy, sys
module = runpy.run_path(sys.argv[1])
files = module['installation_files'](pathlib.Path(sys.argv[2]), pathlib.Path.home())
print(json.dumps([[str(s),str(d),m] for s,d,m in files]))
'''
        args = [sys.executable, "-c", code, str(HELPER), str(self.root / "libid14_sr_ladspa.so")]
        files = json.loads(self.run_command(args).stdout)
        self.assertEqual([Path(row[1]).name for row in files], ["id14-remix.py", "libid14_remix_ladspa.so", "remix.onnx"])
        self.assertEqual(Path(files[-1][1]), self.home / ".local/share/id14-sr/remix.onnx")
        self.env.pop("ID14_REMIX_MODEL")
        self.assertNotIn("remix.onnx", self.run_command(args).stdout)

    def test_installer_preflight_failure_preserves_remix_files_and_settings(self):
        # Execute the actual install seam with a bad SR candidate. No source read.
        library = self.build_plugin()
        model = self.root / "model.onnx"
        model.write_bytes(b"model-fixture")
        sr = self.root / "libid14_sr_ladspa.so"
        sr.write_bytes(b"invalid SR candidate")
        self.cli("vocals", "4")
        self.cli("off")
        saved = self.root / "state/id14-sr/remix.json"
        before = saved.read_bytes()
        self.plugin.write_bytes(b"prior-remix-plugin")
        installed_model = self.home / ".local/share/id14-sr/remix.onnx"
        installed_model.parent.mkdir(parents=True)
        installed_model.write_bytes(b"prior-model")
        self.env["ID14_REMIX_PLUGIN_SOURCE"] = str(library)
        self.env["ID14_REMIX_MODEL"] = str(model)
        code = '''import pathlib, runpy, sys
module = runpy.run_path(sys.argv[1])
try:
    module['install']([sys.argv[2]])
except ValueError as exc:
    print(str(exc))
    sys.exit(1)
'''
        args = [sys.executable, "-c", code, str(HERE / "id14-sr-lifecycle.py"), str(sr)]
        self.run_command(args, expected=1)
        self.assertEqual(self.plugin.read_bytes(), b"prior-remix-plugin")
        self.assertEqual(installed_model.read_bytes(), b"prior-model")
        self.assertEqual(saved.read_bytes(), before)




if __name__ == "__main__":
    unittest.main(verbosity=2)
