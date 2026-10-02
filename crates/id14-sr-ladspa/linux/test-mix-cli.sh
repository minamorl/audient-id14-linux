#!/usr/bin/env bash
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)
cli=$root/crates/id14-sr-ladspa/linux/id14-sr
fixture=$(mktemp -d "$root/.mix-test.XXXXXX")
trap 'rm -rf "$fixture"' EXIT
mkdir -p "$fixture/home" "$fixture/bin"
export HOME=$fixture/home
export XDG_STATE_HOME=$HOME/.local/state
export PATH=$fixture/bin:$PATH

cat >"$fixture/bin/pw-dump" <<'EOF'
#!/usr/bin/env bash
cat <<'JSON'
[
  {"id":20,"type":"PipeWire:Interface:Node","info":{"props":{"node.name":"id14_sr_sink"}}},
  {"id":21,"type":"PipeWire:Interface:Node","info":{"props":{"node.name":"id14_sr_headphones_sink"}}}
]
JSON
EOF
cat >"$fixture/bin/systemctl" <<'EOF'
#!/usr/bin/env bash
if [ "${2:-}" = is-active ]; then exit 0; fi
printf 'unexpected systemctl call: %s\n' "$*" >&2
exit 1
EOF
cat >"$fixture/bin/pw-cli" <<'EOF'
#!/usr/bin/env bash
printf '%s\n' "$*" >>"$HOME/pw-cli.log"
if [ -e "$HOME/fail-headphones" ] && [ "$2" = 21 ] && [[ $* == *'80.0'* ]]; then
    exit 1
fi
EOF
cat >"$fixture/bin/wpctl" <<'EOF'
#!/usr/bin/env bash
exit 0
EOF
cat >"$fixture/bin/pw-metadata" <<'EOF'
#!/usr/bin/env bash
exit 0
EOF
cat >"$fixture/bin/zenity" <<'EOF'
#!/usr/bin/env bash
[[ $* == *'--print-partial'* ]] || exit 2
[[ $* == *'--max-value=200'* ]] || exit 2
if [ -e "$HOME/zenity-cancel" ]; then
    printf '30\n'
    exit 1
fi
printf '60\n'
for _ in {1..100}; do
    [ ! -e "$HOME/slider-continue" ] || break
    sleep 0.05
done
[ -e "$HOME/slider-continue" ] || exit 2
printf '70\n80\n'
EOF
chmod +x "$fixture/bin/"*

[ "$("$cli" mix get)" = auto ]
"$cli" mix bar | jq -e '.class == "off" and .mode == "auto" and .text == "SR auto"' >/dev/null
state=$HOME/.local/state/id14-sr
mkdir -p "$state"
printf '100\n' >"$state/mix"
[ "$("$cli" mix get)" = auto ]
[ "$("$cli" mix set 0)" = 0 ]
[ "$(<"$state/mix")" = manual:0 ]
[ "$("$cli" mix step +25)" = 25 ]
[ "$("$cli" mix step -999)" = 0 ]
[ "$("$cli" mix step +999)" = 200 ]
"$cli" mix bar | jq -e '.class == "off" and .mode == "manual" and .percentage == 200 and .text == "SR 200%"' >/dev/null
[ "$("$cli" mix step -100)" = 100 ]
[ "$("$cli" mix set 101)" = 101 ]
[ "$("$cli" mix set 200)" = 200 ]
if "$cli" mix set 201 >"$fixture/invalid.out" 2>&1; then exit 1; fi
grep -Fq 'mix must be auto or an integer from 0 to 200' "$fixture/invalid.out"
printf 'OFF_MIX_SET_STEP_BAR_OK\n'

printf 'all\n' >"$state/output"
[ "$("$cli" mix set 40)" = 40 ]
"$cli" mix bar | jq -e '.class == "degraded" and .percentage == 40' >/dev/null
config=$HOME/.config/pipewire/filter-chain.conf.d/90-id14-sr.conf
[ "$(grep -Fc 'control = { Mix = 40 }' "$config")" = 2 ]
grep -Fq 'set-param 20 Props { params = [ "sr:Mix" 40.0 ] }' "$HOME/pw-cli.log"
grep -Fq 'set-param 21 Props { params = [ "sr:Mix" 40.0 ] }' "$HOME/pw-cli.log"
[ "$("$cli" mix step +10)" = 50 ]
[ "$(<"$state/mix")" = manual:50 ]
[ "$(grep -Fc 'control = { Mix = 50 }' "$config")" = 2 ]
[ "$("$cli" mix set 150)" = 150 ]
[ "$(<"$state/mix")" = manual:150 ]
[ "$(grep -Fc 'control = { Mix = 150 }' "$config")" = 2 ]
grep -Fq 'set-param 20 Props { params = [ "sr:Mix" 150.0 ] }' "$HOME/pw-cli.log"
grep -Fq 'set-param 21 Props { params = [ "sr:Mix" 150.0 ] }' "$HOME/pw-cli.log"
[ "$("$cli" mix set 50)" = 50 ]
touch "$HOME/fail-headphones"
if "$cli" mix set 80 >"$fixture/failed.out" 2>&1; then exit 1; fi
grep -Fq 'could not set live mix on headphones' "$fixture/failed.out" || {
    cat "$fixture/failed.out" >&2
    exit 1
}
[ "$(<"$state/mix")" = manual:50 ]
[ "$(grep -Fc 'control = { Mix = 50 }' "$config")" = 2 ]
rm "$HOME/fail-headphones"
"$cli" mix slider &
slider_pid=$!
for _ in {1..100}; do
    [ ! -f "$state/mix" ] || [ "$(<"$state/mix")" != manual:60 ] || break
    sleep 0.05
done
[ "$(<"$state/mix")" = manual:60 ]
printf 'SLIDER_PARTIAL_APPLIED_WHILE_OPEN\n'
touch "$HOME/slider-continue"
wait "$slider_pid"
[ "$(<"$state/mix")" = manual:80 ]
for value in 60 70 80; do
    grep -Fq "set-param 20 Props { params = [ \"sr:Mix\" $value.0 ] }" "$HOME/pw-cli.log"
    grep -Fq "set-param 21 Props { params = [ \"sr:Mix\" $value.0 ] }" "$HOME/pw-cli.log"
done
[ "$("$cli" mix auto)" = auto ]
[ "$(<"$state/mix")" = auto ]
[ "$(grep -Fc 'control = { Mix = -1 }' "$config")" = 2 ]
grep -Fq 'set-param 20 Props { params = [ "sr:Mix" -1.0 ] }' "$HOME/pw-cli.log"
grep -Fq 'set-param 21 Props { params = [ "sr:Mix" -1.0 ] }' "$HOME/pw-cli.log"
touch "$HOME/zenity-cancel"
"$cli" mix slider
[ "$(<"$state/mix")" = auto ]
grep -Fq 'set-param 20 Props { params = [ "sr:Mix" 30.0 ] }' "$HOME/pw-cli.log"
[ "$(grep -Fc 'set-param 20 Props { params = [ "sr:Mix" -1.0 ] }' "$HOME/pw-cli.log")" -ge 2 ]
printf 'LIVE_TWO_OUTPUT_MIX_AND_ROLLBACK_OK\n'
