#!/usr/bin/env bash
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)
plugin=${1:-"$root/target/release/libid14_sr_ladspa.so"}
command_source=$root/crates/id14-sr-ladspa/linux/id14-sr
service_source=$root/crates/id14-sr-ladspa/linux/id14-sr-filter.service

[ -r "$plugin" ] || { printf 'missing release plugin: %s\n' "$plugin" >&2; exit 1; }
[ -r "$command_source" ] || { printf 'missing command: %s\n' "$command_source" >&2; exit 1; }
[ -r "$service_source" ] || { printf 'missing service: %s\n' "$service_source" >&2; exit 1; }

if command -v id14-sr >/dev/null 2>&1; then
    id14-sr off >/dev/null
fi
install -Dm755 "$command_source" "$HOME/.local/bin/id14-sr"
install -Dm755 "$plugin" "$HOME/.local/lib/ladspa/libid14_sr_ladspa.so"
install -Dm644 "$service_source" "$HOME/.config/systemd/user/id14-sr-filter.service"
systemctl --user daemon-reload
"$HOME/.local/bin/id14-sr" off
printf 'installed command=%s plugin=%s service=%s\n' \
    "$HOME/.local/bin/id14-sr" "$HOME/.local/lib/ladspa/libid14_sr_ladspa.so" \
    "$HOME/.config/systemd/user/id14-sr-filter.service"
