#!/usr/bin/env bash
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)
plugin=${1:-"$root/target/release/libid14_sr_ladspa.so"}
command_source=$root/crates/id14-sr-ladspa/linux/id14-sr
service_source=$root/crates/id14-sr-ladspa/linux/id14-sr-filter.service
installed_command=$HOME/.local/bin/id14-sr
mode_file=${XDG_STATE_HOME:-"$HOME/.local/state"}/id14-sr/output

[ -r "$plugin" ] || { printf 'missing release plugin: %s\n' "$plugin" >&2; exit 1; }
[ -r "$command_source" ] || { printf 'missing command: %s\n' "$command_source" >&2; exit 1; }
[ -r "$service_source" ] || { printf 'missing service: %s\n' "$service_source" >&2; exit 1; }

# off removes the enabled output state, so retain the selected outputs first.
# The mix file belongs to the owner and is preserved by off and installation.
mode=all
if [ -f "$mode_file" ]; then
    mode=$(<"$mode_file")
fi
case "$mode" in
    all | line | headphones) ;;
    *) printf 'invalid saved output mode: %s\n' "$mode" >&2; exit 1 ;;
esac
if [ -x "$installed_command" ]; then
    "$installed_command" off >/dev/null
fi
install -Dm755 "$command_source" "$installed_command"
install -Dm755 "$plugin" "$HOME/.local/lib/ladspa/libid14_sr_ladspa.so"
install -Dm644 "$service_source" "$HOME/.config/systemd/user/id14-sr-filter.service"
systemctl --user daemon-reload
"$installed_command" on "$mode"
printf 'installed command=%s plugin=%s service=%s\n' \
    "$HOME/.local/bin/id14-sr" "$HOME/.local/lib/ladspa/libid14_sr_ladspa.so" \
    "$HOME/.config/systemd/user/id14-sr-filter.service"
