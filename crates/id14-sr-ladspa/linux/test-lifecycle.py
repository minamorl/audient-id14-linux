#!/usr/bin/env python3
"""Hermetic lifecycle tests. Fixtures implement only the supplied CLI seam."""

import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import tempfile
import time
import unittest
from unittest import mock


SOURCE = Path(__file__).resolve().parent
FILTER = "id14-sr-filter.service"
TIMER = "id14-sr-restore.timer"
RESTORE = "id14-sr-restore.service"
DEVICE = r"dev-snd-by\x2did-usb\x2dAudient_Audient_iD14\x2d00.device"
NAMES = {
    "line": "alsa_output.usb-Audient_Audient_iD14-00.HiFi__Line__sink",
    "headphones": "alsa_output.usb-Audient_Audient_iD14-00.HiFi__Headphones__sink",
}

FAKE = r'''
import ctypes, hashlib, json, os, signal, subprocess, sys, time
from pathlib import Path
db = Path(os.environ['FAKE_DB'])
data = json.loads(db.read_text())
command = Path(sys.argv[0]).name
args = sys.argv[1:]
home = Path(os.environ['HOME'])
state = Path(os.environ['XDG_STATE_HOME']) / 'id14-sr'
config = home / '.config/pipewire/filter-chain.conf.d/90-id14-sr.conf'
filter_unit = 'id14-sr-filter.service'
timer_unit = 'id14-sr-restore.timer'
restore_unit = 'id14-sr-restore.service'
device_unit = r'dev-snd-by\x2did-usb\x2dAudient_Audient_iD14\x2d00.device'
events = Path(os.environ['FAKE_EVENTS'])
with events.open('a') as stream:
    stream.write(json.dumps([command, args]) + '\n')
def save():
    db.write_text(json.dumps(data))
def injected(point):
    if os.environ.get('FAIL_POINT') != point:
        return
    marker = Path(os.environ['INJECTED'])
    if marker.exists():
        return
    marker.touch()
    save()
    if os.environ.get('FAIL_SIGNAL'):
        os.kill(os.getppid(), int(os.environ['FAIL_SIGNAL']))
    sys.exit(71)
def load_plugin():
    plugin = home / '.local/lib/ladspa/libid14_sr_ladspa.so'
    library = ctypes.CDLL(str(plugin))
    library.ladspa_descriptor.restype = ctypes.c_void_p
    if not library.ladspa_descriptor(0):
        raise SystemExit(72)
    data['loaded_plugin'] = hashlib.sha256(plugin.read_bytes()).hexdigest()
def start_filter(action):
    global data
    data['units'][filter_unit]['active'] = False
    data['loaded_plugin'] = None
    data['metadata'] = {}
    save()
    injected('stopped:' + action)
    time.sleep(0.025)
    load_plugin()
    data['units'][filter_unit]['active'] = True
    save()
    injected('loaded:' + action)
    result = subprocess.run([str(home / '.local/bin/id14-sr'), 'reconcile'])
    data = json.loads(db.read_text())
    if result.returncode:
        data['units'][filter_unit]['active'] = False
        data['loaded_plugin'] = None
        save()
        raise SystemExit(result.returncode)
if command == 'systemctl':
    args = [a for a in args if a not in ('--user', '--no-block')]
    action = args[0]
    unit = args[-1]
    if action == 'device-set':
        connected = unit == 'active'
        data['units'][device_unit]['active'] = connected
        service = home / '.config/systemd/user' / restore_unit
        if not connected:
            data['nodes'] = {}
            data['metadata'] = {}
            if service.exists() and 'BindsTo=' + device_unit in service.read_text():
                data['units'][restore_unit]['active'] = False
            save()
            raise SystemExit(0)
        save()
        if data['units'][restore_unit]['enabled'] and not data['units'][restore_unit]['active']:
            text = service.read_text()
            assert 'WantedBy=' + device_unit in text
            assert 'RemainAfterExit=yes' in text
            result = subprocess.run([sys.executable, str(home / '.local/lib/id14-sr/lifecycle.py'), 'restore'])
            data = json.loads(db.read_text())
            data['units'][restore_unit]['active'] = result.returncode == 0
            save()
            raise SystemExit(result.returncode)
        raise SystemExit(0)
    if action in ('is-active', 'is-enabled'):
        key = 'active' if action == 'is-active' else 'enabled'
        sys.exit(0 if data['units'].get(unit, {}).get(key, False) else 1)
    if action == 'daemon-reload':
        plugin = home / '.local/lib/ladspa/libid14_sr_ladspa.so'
        if plugin.exists() and plugin.read_bytes() == Path(os.environ['FAKE_CANDIDATE']).read_bytes():
            injected('final-daemon-reload')
        injected('daemon-reload')
    else:
        flags = data['units'].setdefault(unit, {'enabled': False, 'active': False})
        if action == 'enable':
            flags['enabled'] = True
            if '--now' in args:
                flags['active'] = True
                if unit == filter_unit:
                    start_filter('start')
        elif action == 'disable':
            flags['enabled'] = False
            if '--now' in args:
                flags['active'] = False
        elif action in ('start', 'restart'):
            flags['active'] = True
            if unit == filter_unit:
                start_filter(action)
        elif action == 'stop':
            flags['active'] = False
            if unit == filter_unit:
                data['loaded_plugin'] = None
                data['metadata'] = {}
        else:
            raise SystemExit('unexpected systemctl action')
        injected(action + ':' + unit)
    save()
elif command == 'pw-dump':
    if data.get('pending_nodes') and time.monotonic() >= data['nodes_ready_at']:
        data['nodes'] = data.pop('pending_nodes')
        data.pop('nodes_ready_at')
        save()
    print(json.dumps([{'id': number, 'info': {'props': {'node.name': name}}}
                      for name, number in data['nodes'].items()]))
elif command == 'id14-sr':
    action = args[0]
    if action == 'off':
        data['metadata'] = {}
        data['units'][filter_unit] = {'enabled': False, 'active': False}
        data['loaded_plugin'] = None
        for name in ('output', 'physical-id-line', 'physical-id-headphones', 'prior-default', 'pinned-targets'):
            (state / name).unlink(missing_ok=True)
        config.unlink(missing_ok=True)
    elif action in ('on', 'reconcile'):
        state.mkdir(parents=True, exist_ok=True)
        if action == 'on':
            (state / 'output').write_text(args[1] + '\n')
            config.parent.mkdir(parents=True, exist_ok=True)
            config.write_text('# Managed by id14-sr; removed when processing is off.\nfixture\n')
            data['units'][filter_unit] = {'enabled': True, 'active': True}
            load_plugin()
            injected('activate')
        if not (state / 'output').exists():
            raise SystemExit(1)
        selected = (state / 'output').read_text().strip()
        wanted = ('line', 'headphones') if selected == 'all' else (selected,)
        found = False
        for output in wanted:
            name = 'alsa_output.usb-Audient_Audient_iD14-00.HiFi__' + ('Line' if output == 'line' else 'Headphones') + '__sink'
            if name in data['nodes']:
                found = True
                data['metadata'][str(data['nodes'][name])] = 'filter:' + output
                data['applied_mix'][output] = (state / 'mix').read_text() if (state / 'mix').exists() else 'auto'
        injected('reconcile')
        if not found:
            raise SystemExit(1)
    else:
        raise SystemExit('unexpected CLI action')
    save()
else:
    # All host audio commands are intercepted, including unused commands.
    raise SystemExit('unexpected audio command: ' + command)
'''

INJECTION = r'''
import os
from pathlib import Path
original = os.replace
def replace(src, dst, *args, **kwargs):
    destinations = ['id14-sr', 'libid14_sr_ladspa.so', 'id14-sr-filter.service',
                    'lifecycle.py', 'id14-sr-restore.service']
    point = os.environ.get('FAIL_POINT', '')
    marker = Path(os.environ['INJECTED'])
    hit = str(src).endswith('.id14-sr-new') and Path(dst).name in destinations
    if hit and not marker.exists() and point in ('replace-before:' + Path(dst).name, 'replace-after:' + Path(dst).name):
        marker.touch()
        if point.startswith('replace-after:'):
            original(src, dst, *args, **kwargs)
        if os.environ.get('FAIL_SIGNAL'):
            os.kill(os.getpid(), int(os.environ['FAIL_SIGNAL']))
        raise OSError('injected publication failure')
    return original(src, dst, *args, **kwargs)
os.replace = replace
original_chmod = os.chmod
def chmod(path, *args, **kwargs):
    marker = Path(os.environ['INJECTED'])
    if os.environ.get('FAIL_POINT') == 'stage-mode:' + Path(path).name and not marker.exists():
        marker.touch()
        raise OSError('injected staging failure')
    return original_chmod(path, *args, **kwargs)
os.chmod = chmod
'''


def write(path, contents, mode=0o644):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(contents)
    path.chmod(mode)


class Lifecycle(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix=".lifecycle-test-", dir=SOURCE)
        self.root = Path(self.tmp.name)
        self.source = self.root / "package"
        self.home = self.root / "home"
        self.state = self.root / "state/id14-sr"
        self.base = self.root / "state/id14-sr-lifecycle"
        self.bin = self.root / "bin"
        self.db = self.root / "runtime.json"
        self.marker = self.root / "injected"
        self.events = self.root / 'events.jsonl'
        (self.root / 'tmp').mkdir()
        self.env = dict(os.environ, HOME=str(self.home), XDG_STATE_HOME=str(self.state.parent),
                        PATH=str(self.bin) + os.pathsep + os.environ['PATH'],
                        FAKE_DB=str(self.db), INJECTED=str(self.marker),
                        FAKE_EVENTS=str(self.events),
                        FAKE_CANDIDATE=str(self.root / 'candidate.so'),
                        TMPDIR=str(self.root / 'tmp'),
                        PYTHONDONTWRITEBYTECODE="1", PYTHONPATH=str(self.root / "injection"))
        self.env.pop("FAIL_POINT", None)
        self.env.pop("FAIL_SIGNAL", None)
        self.source.mkdir()
        for name in ('install-user.sh', 'id14-sr-lifecycle.py', 'id14-sr-restore.service'):
            shutil.copy2(SOURCE / name, self.source / name)
        fake = '#!' + sys.executable + '\n' + FAKE
        write(self.source / 'id14-sr', fake, 0o755)
        write(self.source / FILTER, '[Service]\nType=simple\nExecStart=/fixture/pipewire -c filter-chain.conf\n')
        for command in ('systemctl', 'pw-dump', 'pw-cli', 'wpctl', 'pw-metadata', 'jq'):
            write(self.bin / command, fake, 0o755)
        write(self.root / 'injection/sitecustomize.py', INJECTION)
        self.plugin = self.root / 'candidate.so'
        cfile = self.root / 'plugin.c'
        write(cfile, '''
#include <stdlib.h>
struct hint { int descriptor; float lower, upper; };
struct descriptor { unsigned long id; const char *label; int properties;
    const char *name, *maker, *copyright; unsigned long count;
    const int *ports; const char * const *names; const struct hint *hints;
    void *implementation;
    void *(*instantiate)(const struct descriptor *, unsigned long);
    void (*connect)(void *, unsigned long, float *);
    void (*activate)(void *);
    void (*run)(void *, unsigned long);
    void (*run_adding)(void *, unsigned long);
    void (*set_gain)(void *, float);
    void (*deactivate)(void *);
    void (*cleanup)(void *);
};
struct instance { float *ports[5]; };
static void *instantiate(const struct descriptor *d, unsigned long rate) {
    (void)d; (void)rate; return calloc(1, sizeof(struct instance));
}
static void connect(void *handle, unsigned long port, float *data) {
    if (port < 5) ((struct instance *)handle)->ports[port] = data;
}
static void run(void *handle, unsigned long count) {
    struct instance *instance = handle;
    for (unsigned long i = 0; i < count; ++i) {
        instance->ports[2][i] = instance->ports[0][i];
        instance->ports[3][i] = instance->ports[1][i];
    }
}
static const int ports[] = {9, 9, 10, 10, 5};
static const char *names[] = {"Input L", "Input R", "Output L", "Output R", "Mix"};
static const struct hint hints[] = {{0,0,0}, {0,0,0}, {0,0,0}, {0,0,0}, {3,0,200}};
static const struct descriptor descriptor = {1, "id14_sr_stereo", 0,
    "fixture", "fixture", "fixture", 5, ports, names, hints,
    NULL, instantiate, connect, NULL, run, NULL, NULL, NULL, free};
const void *ladspa_descriptor(unsigned long i) { return i ? 0 : &descriptor; }
''')
        subprocess.run(['cc', '-shared', '-fPIC', str(cfile), '-o', str(self.plugin)], check=True,
                       stdout=subprocess.PIPE, stderr=subprocess.PIPE, env=self.env)
        self.old_plugin = self.root / 'prior.so'
        old_cfile = self.root / 'prior.c'
        write(old_cfile, cfile.read_text().replace('{1, "id14_sr_stereo"', '{2, "id14_sr_stereo"'))
        subprocess.run(['cc', '-shared', '-fPIC', str(old_cfile), '-o', str(self.old_plugin)], check=True,
                       stdout=subprocess.PIPE, stderr=subprocess.PIPE, env=self.env)
        self.reset()

    def tearDown(self):
        self.tmp.cleanup()

    def reset(self, selected='headphones', mix='manual:147\n', active=True, enabled=True, timer=False):
        for path in (self.home, self.state.parent):
            if path.exists():
                shutil.rmtree(path)
        self.marker.unlink(missing_ok=True)
        write(self.home / '.local/bin/id14-sr', '#!' + sys.executable + '\n' + FAKE + '\n# prior version\n', 0o755)
        installed_plugin = self.home / '.local/lib/ladspa/libid14_sr_ladspa.so'
        installed_plugin.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(self.old_plugin, installed_plugin)
        installed_plugin.chmod(0o755)
        write(self.home / '.config/systemd/user' / FILTER, 'prior filter unit\n')
        write(self.state / 'mix', mix)
        write(self.state / 'mix.lock', '')
        if selected:
            write(self.state / 'output', selected + '\n')
            write(self.state / 'physical-id-line', '11\n')
            write(self.state / 'physical-id-headphones', '12\n')
            write(self.state / 'prior-default', 'saved prior default\n')
            write(self.state / 'pinned-targets', 'saved pin\n')
            write(self.home / '.config/pipewire/filter-chain.conf.d/90-id14-sr.conf',
                  '# Managed by id14-sr; removed when processing is off.\nprior config\n')
        if timer:
            write(self.home / '.config/systemd/user' / TIMER, 'prior timer\n')
            write(self.home / '.config/systemd/user/id14-sr-restore.service', 'prior restore unit\n')
            write(self.home / '.local/lib/id14-sr/lifecycle.py', 'prior helper\n', 0o755)
        wanted = ('line', 'headphones') if selected == 'all' else ((selected,) if selected else ())
        ids = {'line': '11', 'headphones': '12'}
        self.save_db({'units': {FILTER: {'enabled': enabled, 'active': active},
                               TIMER: {'enabled': timer, 'active': timer},
                               RESTORE: {'enabled': False, 'active': False},
                               DEVICE: {'enabled': False, 'active': True}},
                      'loaded_plugin': hashlib.sha256(self.old_plugin.read_bytes()).hexdigest() if active else None,
                      'nodes': {NAMES['line']: 11, NAMES['headphones']: 12},
                      'metadata': {ids[x]: 'filter:' + x for x in wanted},
                      'applied_mix': {x: mix for x in wanted}})

    def save_db(self, value):
        self.db.write_text(json.dumps(value))

    def load_db(self):
        return json.loads(self.db.read_text())

    def snapshot(self):
        files = {}
        for root in (self.home, self.state.parent):
            if root.exists():
                for file in root.rglob('*'):
                    if file.is_file():
                        files[str(file.relative_to(self.root))] = (hashlib.sha256(file.read_bytes()).hexdigest(), file.stat().st_mode & 0o777)
        return files, self.load_db()

    def invoke(self, command='install', point=None, signum=None):
        env = dict(self.env)
        if point:
            env['FAIL_POINT'] = point
        if signum:
            env['FAIL_SIGNAL'] = str(signum)
        args = ['bash', str(self.source / 'install-user.sh'), str(self.plugin)] if command == 'install' else [sys.executable, str(self.source / 'id14-sr-lifecycle.py'), 'restore']
        return subprocess.run(args, env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=30)

    def wait_guard(self):
        deadline = time.monotonic() + 10
        while self.base.exists() and list(self.base.glob('transaction-*')) and time.monotonic() < deadline:
            time.sleep(0.05)
        self.assertFalse(list(self.base.glob('transaction-*')))

    def test_reinstall_failure_at_every_publication_boundary(self):
        names = ['id14-sr', 'libid14_sr_ladspa.so', FILTER, 'lifecycle.py', RESTORE]
        for prefix in ('replace-before:', 'replace-after:'):
            for name in names:
                with self.subTest(point=prefix + name):
                    self.reset()
                    before = self.snapshot()
                    result = self.invoke(point=prefix + name)
                    self.assertNotEqual(result.returncode, 0, result.stdout)
                    self.assertTrue(self.marker.exists())
                    self.wait_guard()
                    self.assertEqual(before, self.snapshot())

    def test_reinstall_systemctl_failure_and_signals(self):
        for point in ('daemon-reload', 'final-daemon-reload', 'enable:' + RESTORE,
                      'stopped:restart', 'loaded:restart', 'restart:' + FILTER):
            for signum in (None, signal.SIGTERM, signal.SIGINT, signal.SIGHUP, signal.SIGKILL):
                for timer in (False, True):
                    with self.subTest(point=point, signal=signum, existing_timer=timer):
                        self.reset(timer=timer)
                        before = self.snapshot()
                        result = self.invoke(point=point, signum=signum)
                        self.assertNotEqual(result.returncode, 0)
                        self.wait_guard()
                        self.assertEqual(before, self.snapshot())

    def test_sigkill_during_each_file_publication(self):
        for name in ('id14-sr', 'libid14_sr_ladspa.so', FILTER, 'lifecycle.py', RESTORE):
            with self.subTest(file=name):
                self.reset()
                before = self.snapshot()
                result = self.invoke(point='replace-after:' + name, signum=signal.SIGKILL)
                self.assertEqual(result.returncode, -signal.SIGKILL)
                self.wait_guard()
                self.assertEqual(before, self.snapshot())

    def test_preflight_bad_plugin_retains_everything(self):
        before = self.snapshot()
        self.plugin.write_bytes(b'not a shared library')
        result = self.invoke()
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(before, self.snapshot())

    def test_wrong_ladspa_contract_retains_everything(self):
        cfile = self.root / 'plugin.c'
        original = cfile.read_text()
        variants = [original.replace('id14_sr_stereo', 'wrong_label'),
                    original.replace('{3,0,200}', '{3,0,100}'),
                    original.replace('{9, 9, 10, 10, 5}', '{9, 9, 10, 10, 9}')]
        for contents in variants:
            with self.subTest(contents=contents):
                before = self.snapshot()
                cfile.write_text(contents)
                subprocess.run(['cc', '-shared', '-fPIC', str(cfile), '-o', str(self.plugin)],
                               check=True, env=self.env, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
                self.assertNotEqual(self.invoke().returncode, 0)
                self.assertEqual(before, self.snapshot())

    def test_staging_failure_retains_everything(self):
        for index in range(5):
            with self.subTest(stage=index):
                self.reset()
                before = self.snapshot()
                self.assertNotEqual(self.invoke(point=f'stage-mode:stage-{index}').returncode, 0)
                self.assertTrue(self.marker.exists())
                self.wait_guard()
                self.assertEqual(before, self.snapshot())

    def test_failure_preserves_degraded_and_off_states(self):
        for selection in ('all', 'line', 'headphones', None):
            for enabled in (False, True):
                with self.subTest(selection=selection, enabled=enabled):
                    self.reset(selected=selection, active=False, enabled=enabled, mix='manual:200\n')
                    before = self.snapshot()
                    self.assertNotEqual(self.invoke(point='enable:' + RESTORE).returncode, 0)
                    self.wait_guard()
                    self.assertEqual(before, self.snapshot())

    def test_generated_units_do_not_pin_python_store_path(self):
        # Resolve the interpreter so Nix profile symlinks cannot conceal a
        # store path in this regression test's installer process.
        result = subprocess.run(
            [str(Path(sys.executable).resolve()), str(self.source / 'id14-sr-lifecycle.py'),
             'install', str(self.plugin)],
            env=self.env, text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=30)
        self.assertEqual(result.returncode, 0, result.stderr)
        units = self.home / '.config/systemd/user'
        for unit in units.glob('*.service'):
            with self.subTest(unit=unit.name):
                self.assertNotIn('/nix/store', unit.read_text())
        restore = (units / RESTORE).read_text()
        self.assertIn('ExecStart=/usr/bin/env python3 "%h/.local/lib/id14-sr/lifecycle.py" restore', restore)
        self.assertIn('%h/.nix-profile/bin:/run/current-system/sw/bin', restore)
        self.assertNotIn('@PYTHON@', restore)

    def test_success_preserves_running_selection_mix_and_off(self):
        timings = []
        for selection in ('all', 'line', 'headphones', None):
            for mix in ('auto\n', 'manual:0\n', 'manual:200\n'):
                with self.subTest(selection=selection, mix=mix):
                    self.reset(selected=selection, mix=mix, active=selection is not None, enabled=selection is not None)
                    before = self.snapshot()
                    result = self.invoke()
                    self.assertEqual(result.returncode, 0, result.stderr)
                    self.assertIn('installed command=', result.stdout)
                    after = self.snapshot()
                    for path, value in before[0].items():
                        if '/state/' in '/' + path or 'filter-chain.conf.d' in path:
                            self.assertEqual(value, after[0][path])
                    for key in ('metadata', 'applied_mix'):
                        self.assertEqual(before[1][key], after[1][key])
                    self.assertEqual(before[1]['units'][FILTER], after[1]['units'][FILTER])
                    self.assertTrue(after[1]['units'][RESTORE]['enabled'])
                    self.assertFalse(after[1]['units'][TIMER]['active'])
                    if selection:
                        self.assertEqual(after[1]['loaded_plugin'], hashlib.sha256(self.plugin.read_bytes()).hexdigest())
                        self.assertNotEqual(before[1]['loaded_plugin'], after[1]['loaded_plugin'])
                        measured = [line for line in result.stdout.splitlines() if line.startswith('switch_seconds=')]
                        self.assertEqual(len(measured), 1)
                        timings.append(float(measured[0].split('=')[1]))
                    else:
                        self.assertIsNone(after[1]['loaded_plugin'])
        print(f'HERMETIC_SWITCH_SECONDS min={min(timings):.6f} max={max(timings):.6f} fixture_start_delay=0.025', flush=True)

    def device_event(self, active):
        return subprocess.run(['systemctl', '--user', 'device-set', 'active' if active else 'inactive'],
                              env=self.env, text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=30)

    def test_usb_new_ids_and_wireplumber_restart(self):
        for selection in ('all', 'line', 'headphones'):
            for active in (True, False):
                for mix in ('auto\n', 'manual:173\n'):
                    with self.subTest(selection=selection, active=active, mix=mix):
                        self.reset(selected=selection, mix=mix)
                        self.assertEqual(self.invoke().returncode, 0)
                        self.assertEqual(self.device_event(False).returncode, 0)
                        data = self.load_db()
                        data['units'][FILTER]['active'] = active
                        if not active:
                            data['loaded_plugin'] = None
                        self.save_db(data)
                        unplugged = self.snapshot()
                        self.assertEqual(self.invoke('restore').returncode, 0)
                        self.assertEqual(unplugged, self.snapshot())
                        data['nodes'] = {NAMES['line']: 81, NAMES['headphones']: 92}
                        self.save_db(data)
                        result = self.device_event(True)
                        self.assertEqual(result.returncode, 0, result.stderr)
                        data = self.load_db()
                        wanted = ('line', 'headphones') if selection == 'all' else (selection,)
                        self.assertEqual(data['metadata'], {str(data['nodes'][NAMES[x]]): 'filter:' + x for x in wanted})
                        for output in wanted:
                            self.assertEqual(data['applied_mix'][output], mix)
                        self.assertEqual((self.state / 'output').read_text(), selection + '\n')
                        self.assertEqual((self.state / 'mix').read_text(), mix)
                        self.assertTrue(data['units'][FILTER]['active'])
                        self.assertTrue(data['units'][RESTORE]['active'])
                        # Same connected device state must not retrigger the
                        # completed oneshot or run pw-dump during idle time.
                        count = self.events.read_text().count('pw-dump')
                        time.sleep(0.05)
                        self.assertEqual(self.device_event(True).returncode, 0)
                        self.assertEqual(count, self.events.read_text().count('pw-dump'))

    def test_off_does_not_reactivate_on_replug(self):
        self.reset(selected=None, active=False, enabled=False)
        self.assertEqual(self.invoke().returncode, 0)
        before = self.snapshot()
        self.assertEqual(self.device_event(False).returncode, 0)
        result = self.device_event(True)
        self.assertEqual(result.returncode, 0, result.stderr)
        after = self.snapshot()
        self.assertEqual(before[0], after[0])
        self.assertEqual(before[1]['units'][FILTER], after[1]['units'][FILTER])
        self.assertIsNone(after[1]['loaded_plugin'])
        self.assertEqual(before[1]['metadata'], after[1]['metadata'])

    def test_device_event_waits_for_delayed_sinks(self):
        self.assertEqual(self.invoke().returncode, 0)
        self.assertEqual(self.device_event(False).returncode, 0)
        data = self.load_db()
        data['pending_nodes'] = {NAMES['line']: 181, NAMES['headphones']: 192}
        data['nodes_ready_at'] = time.monotonic() + 0.3
        data['units'][FILTER]['active'] = False
        data['loaded_plugin'] = None
        self.save_db(data)
        result = self.device_event(True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.load_db()['metadata'], {'192': 'filter:headphones'})

    def test_missing_sinks_have_a_deadline(self):
        self.assertEqual(self.invoke().returncode, 0)
        data = self.load_db()
        data['nodes'] = {}
        data['metadata'] = {}
        self.save_db(data)
        before = self.snapshot()
        sys.dont_write_bytecode = True
        spec = importlib.util.spec_from_file_location('lifecycle_under_test', SOURCE / 'id14-sr-lifecycle.py')
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        started = time.monotonic()
        with mock.patch.dict(os.environ, self.env, clear=True):
            with self.assertRaises((TimeoutError, subprocess.TimeoutExpired)):
                module.restore(wait_seconds=0.08)
        self.assertLess(time.monotonic() - started, 1)
        self.assertEqual(before, self.snapshot())

    def test_candidate_reconcile_failure_restores_prior_loaded_plugin(self):
        before = self.snapshot()
        result = self.invoke(point='reconcile')
        self.assertNotEqual(result.returncode, 0)
        self.wait_guard()
        self.assertEqual(before, self.snapshot())

    def test_old_timer_is_removed_and_failure_restores_it(self):
        self.reset(timer=True)
        before = self.snapshot()
        self.assertNotEqual(self.invoke(point='disable:' + TIMER).returncode, 0)
        self.wait_guard()
        self.assertEqual(before, self.snapshot())
        result = self.invoke()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertFalse((self.home / '.config/systemd/user' / TIMER).exists())
        self.assertEqual(self.load_db()['units'][TIMER], {'active': False, 'enabled': False})

    def test_restore_retries_transient_failure(self):
        self.assertEqual(self.invoke().returncode, 0)
        data = self.load_db()
        data['metadata'] = {}
        self.save_db(data)
        result = self.invoke('restore', point='reconcile')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertTrue(self.marker.exists())
        self.assertEqual(self.load_db()['metadata'], {'12': 'filter:headphones'})

    def test_fresh_install_defaults_on_and_keeps_mix(self):
        self.reset(selected=None, active=False, enabled=False, mix='manual:162\n')
        (self.home / '.local/bin/id14-sr').unlink()
        result = self.invoke()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual((self.state / 'output').read_text(), 'all\n')
        self.assertEqual((self.state / 'mix').read_text(), 'manual:162\n')
        self.assertEqual(self.load_db()['units'][FILTER], {'enabled': True, 'active': True})

    def test_fresh_activation_failure_rolls_back(self):
        self.reset(selected=None, active=False, enabled=False)
        (self.home / '.local/bin/id14-sr').unlink()
        before = self.snapshot()
        result = self.invoke(point='activate')
        self.assertNotEqual(result.returncode, 0)
        self.wait_guard()
        self.assertEqual(before, self.snapshot())


if __name__ == '__main__':
    unittest.main(verbosity=2)
