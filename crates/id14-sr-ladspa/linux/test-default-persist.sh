#!/usr/bin/env bash
set -euo pipefail

linux_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
fixture=$(mktemp -d "$linux_dir/.default-persist-test.XXXXXX")
trap 'rm -rf "$fixture"' EXIT
mkdir -p "$fixture/bin"
export PATH=$fixture/bin:$PATH
unset ID14_SR_PLUGIN ID14_SR_LINE_SINK ID14_SR_HEADPHONES_SINK ID14_SR_HARDWARE_SINK

# Model only the external commands used by the real CLI. Never contact an
# audio server or the user's systemd manager, including on an unexpected call.
cat >"$fixture/bin/mock" <<'PY'
#!/usr/bin/env python3
import json
import os
from pathlib import Path
import subprocess
import sys

home = Path(os.environ['HOME'])
runtime = home / 'mock-runtime'
runtime.mkdir(exist_ok=True)
command = Path(sys.argv[0]).name
args = sys.argv[1:]
config = home / '.config/pipewire/filter-chain.conf.d/90-id14-sr.conf'
unit = home / '.config/systemd/user/id14-sr-filter.service'
active = runtime / 'active'
enabled = home / 'mock-enabled'
metadata_file = runtime / 'metadata.json'
metadata = json.loads(metadata_file.read_text()) if metadata_file.exists() else {}

def fail():
    sys.exit(f'unexpected mock command: {command} {args}')

def nodes():
    names = {
        10: 'alsa_output.usb-Audient_Audient_iD14-00.HiFi__Line__sink',
        11: 'alsa_output.usb-Audient_Audient_iD14-00.HiFi__Headphones__sink',
        12: 'alsa_output.hw_iD14_0',
    }
    if active.exists() and config.exists():
        text = config.read_text()
        for output, ids in [('line', [20, 22, 24, 26]), ('headphones', [21, 23, 25, 27])]:
            prefix = 'id14_sr' if output == 'line' else 'id14_sr_headphones'
            transport = 'id14_sr_transport' if output == 'line' else 'id14_sr_headphones_transport'
            for node_id, name in zip(ids, [prefix + '_sink', prefix + '_output', transport, transport + '_output']):
                if f'node.name = "{name}"' in text:
                    names[node_id] = name
    return names

def start():
    lines = unit.read_text().splitlines()
    assert 'WantedBy=default.target' in lines
    post = next(line.removeprefix('ExecStartPost=') for line in lines if line.startswith('ExecStartPost='))
    active.touch()
    result = subprocess.run(post.replace('%h', str(home)).split(), check=False)
    if result.returncode:
        active.unlink(missing_ok=True)
    sys.exit(result.returncode)

if command == 'systemctl':
    service = 'id14-sr-filter.service'
    with (home / 'systemctl.log').open('a') as log:
        log.write(' '.join(args) + '\n')
    if args == ['--user', 'is-active', '--quiet', service]:
        sys.exit(0 if active.exists() else 3)
    elif args == ['--user', 'daemon-reload']:
        pass
    elif args in [['--user', 'enable', service], ['--user', 'enable', '--now', service]]:
        if (home / 'fail-enable').exists():
            sys.exit(1)
        enabled.touch()
        if '--now' in args:
            start()
    elif args == ['--user', 'disable', '--now', service]:
        enabled.unlink(missing_ok=True)
        active.unlink(missing_ok=True)
    elif args == ['--user', 'start', service]:
        start()
    else:
        fail()
elif command == 'pw-dump':
    if args:
        fail()
    print(json.dumps([{'id': key, 'type': 'PipeWire:Interface:Node', 'info': {'props': {'node.name': name}}} for key, name in nodes().items()]))
elif command == 'wpctl':
    if args != ['inspect', '@DEFAULT_SINK@']:
        fail()
    print('node.name = "unchanged-default"')
elif command == 'pw-metadata':
    if args[:2] != ['-n', 'filters']:
        fail()
    if len(args) == 2:
        for key, value in metadata.items():
            node_id, prop = key.split(':', 1)
            print(f"update: id:{node_id} key:'{prop}' value:'{value[0]}' type:'{value[1]}'")
    elif len(args) == 6:
        metadata[args[2] + ':' + args[3]] = args[4:6]
    elif len(args) == 5 and args[2] == '-d':
        metadata.pop(args[3] + ':' + args[4], None)
    else:
        fail()
    metadata_file.write_text(json.dumps(metadata))
elif command == 'pw-cli':
    if len(args) != 4 or args[0] != 'set-param' or args[2] != 'Props' or int(args[1]) not in nodes():
        fail()
    with (runtime / 'mix.log').open('a') as log:
        log.write(' '.join(args) + '\n')
else:
    fail()
PY
chmod +x "$fixture/bin/mock"
for command in systemctl pw-dump pw-metadata wpctl pw-cli; do
    ln -s mock "$fixture/bin/$command"
done
# The installer must use the command in its installation destination, even
# when another installation appears earlier on PATH.
cat >"$fixture/bin/id14-sr" <<'EOF'
#!/usr/bin/env bash
printf 'unexpected CLI from PATH\n' >&2
exit 88
EOF
chmod +x "$fixture/bin/id14-sr"
printf 'fake plugin\n' >"$fixture/plugin.so"

new_home() {
    export HOME=$fixture/$1
    mkdir -p "$HOME"
    if [ "$2" = custom ]; then export XDG_STATE_HOME=$HOME/custom-state
    else unset XDG_STATE_HOME
    fi
    state=${XDG_STATE_HOME:-$HOME/.local/state}/id14-sr
    cli=$HOME/.local/bin/id14-sr
    config=$HOME/.config/pipewire/filter-chain.conf.d/90-id14-sr.conf
}

assert_on() {
    local mode=$1 mix=$2 count=1 output value=$2 id
    [ "$mode" != all ] || count=2
    [ "$mix" != auto ] || value=-1
    [ -f "$HOME/mock-enabled" ]
    [ -f "$HOME/mock-runtime/active" ]
    [ "$(<"$state/output")" = "$mode" ]
    [ "$("$cli" mix get)" = "$mix" ]
    [ "$(grep -Fc "control = { Mix = $value }" "$config")" = "$count" ]
    output=$("$cli" status)
    [[ $output == "state=on requested=$mode service=active "* ]]
    [[ $output == *'default=unchanged-default'* ]]
    for id in 20 21; do
        [ "$mode:$id" != line:21 ] && [ "$mode:$id" != headphones:20 ] || continue
        grep -Fq "set-param $id Props { params = [ \"sr:Mix\" $value.0 ] }" "$HOME/mock-runtime/mix.log"
    done
}

restart_session() {
    # Discard only runtime state, as at logout/reboot. Keep installed units,
    # config, enabled state and the owner's XDG state directory.
    [ -f "$HOME/mock-enabled" ]
    rm -rf "$HOME/mock-runtime"
    systemctl --user start id14-sr-filter.service
}

new_home fresh default
bash "$linux_dir/install-user.sh" "$fixture/plugin.so"
assert_on all auto
restart_session
assert_on all auto
printf 'FRESH_INSTALL_DEFAULT_ON_AND_SESSION_RESTORE_OK\n'

for mode in all line headphones; do
    new_home "$mode" custom
    mkdir -p "$state"
    printf '%s\n' "$mode" >"$state/output"
    printf 'manual:137\n' >"$state/mix"
    bash "$linux_dir/install-user.sh" "$fixture/plugin.so"
    assert_on "$mode" 137
    [ "$(<"$state/mix")" = manual:137 ]
    "$cli" mix set 163
    bash "$linux_dir/install-user.sh" "$fixture/plugin.so"
    assert_on "$mode" 163
    [ "$(<"$state/mix")" = manual:163 ]
    restart_session
    assert_on "$mode" 163
    # A running but disabled service must be re-enabled by an explicit on.
    rm "$HOME/mock-enabled"
    "$cli" on "$mode"
    assert_on "$mode" 163
    "$cli" mix auto
    bash "$linux_dir/install-user.sh" "$fixture/plugin.so"
    assert_on "$mode" auto
    [ "$(<"$state/mix")" = auto ]
    restart_session
    assert_on "$mode" auto
    printf 'REINSTALL_AND_SESSION_RESTORE_OK mode=%s state=custom\n' "$mode"
done

"$cli" mix set 0
"$cli" off
[ "$(<"$state/mix")" = manual:0 ]
bash "$linux_dir/install-user.sh" "$fixture/plugin.so"
assert_on all 0
printf 'INSTALL_AFTER_OFF_PRESERVES_ZERO_AND_DEFAULTS_ON_OK\n'

new_home failure default
mkdir -p "$state"
printf 'manual:200\n' >"$state/mix"
touch "$HOME/fail-enable"
if bash "$linux_dir/install-user.sh" "$fixture/plugin.so" >"$HOME/failure.log" 2>&1; then
    printf 'installation ignored activation failure\n' >&2
    exit 1
fi
[ "$(<"$state/mix")" = manual:200 ]
! grep -q '^installed command=' "$HOME/failure.log"
rm "$HOME/fail-enable"
bash "$linux_dir/install-user.sh" "$fixture/plugin.so"
assert_on all 200
printf 'ACTIVATION_FAILURE_REPORTED_AND_SAVED_MIX_RETRY_OK\n'

rm "$HOME/mock-enabled"
touch "$HOME/fail-enable"
if "$cli" on all >"$HOME/failure.log" 2>&1; then
    printf 'on ignored enable failure for an already running service\n' >&2
    exit 1
fi
grep -Fq 'could not enable SR for the next login' "$HOME/failure.log"
[ "$(<"$state/mix")" = manual:200 ]
rm "$HOME/fail-enable"
"$cli" on all
assert_on all 200
printf 'RUNNING_SERVICE_ENABLE_FAILURE_AND_RETRY_OK\n'
