#!/usr/bin/env bash
set -euo pipefail

cli=${ID14_SR_CLI:-"$HOME/.local/bin/id14-sr"}
service=id14-sr-filter.service
null_sink=id14_sr_test_null
line_sink=alsa_output.usb-Audient_Audient_iD14-00.HiFi__Line__sink
headphones_sink=alsa_output.usb-Audient_Audient_iD14-00.HiFi__Headphones__sink
line_main=id14_sr_sink
headphones_main=id14_sr_headphones_sink
line_output=id14_sr_output
headphones_output=id14_sr_headphones_output
sentinel_key=id14-sr.test-sentinel
module_id=
pids=()
original_default=

node_id() {
    pw-dump | jq -er --arg name "$1" '
        [.[] | select(
            .type == "PipeWire:Interface:Node" and
            .info.props["node.name"] == $name
        )] | first | .id // empty
    '
}

default_name() {
    wpctl inspect @DEFAULT_SINK@ | awk -F'"' '
        /node.name =/ { print $2; found = 1; exit }
        END { if (!found) exit 1 }
    '
}

node_sink() {
    local property=$1 name=$2
    pw-dump | jq -er --arg property "$property" --arg name "$name" '
        . as $objects
        | [$objects[] | select(
            .type == "PipeWire:Interface:Node" and
            .info.props[$property] == $name
          )] | first as $stream
        | [$objects[] | select(
            .type == "PipeWire:Interface:Link" and
            .info["output-node-id"] == $stream.id
          ) | .info["input-node-id"]] | unique as $targets
        | [$objects[] | select(
            .type == "PipeWire:Interface:Node" and
            (.id as $id | $targets | index($id))
          ) | .info.props["node.name"]] | unique | join(",")
    '
}

stream_sink() { node_sink media.name "$1"; }
output_sink() { node_sink node.name "$1"; }

wait_route() {
    local name=$1 expected=$2 actual=''
    for _ in {1..80}; do
        actual=$(stream_sink "$name" 2>/dev/null || true)
        [ "$actual" = "$expected" ] && return 0
        sleep 0.1
    done
    printf 'ASSERT_ROUTE_FAILED name=%s expected=%s actual=%s\n' "$name" "$expected" "$actual" >&2
    return 1
}

wait_output_route() {
    local name=$1 expected=$2 actual=''
    for _ in {1..80}; do
        actual=$(output_sink "$name" 2>/dev/null || true)
        [ "$actual" = "$expected" ] && return 0
        sleep 0.1
    done
    printf 'ASSERT_OUTPUT_ROUTE_FAILED name=%s expected=%s actual=%s\n' \
        "$name" "$expected" "$actual" >&2
    return 1
}

show_routes() {
    local label=$1 name
    printf '%s default=%s\n' "$label" "$(default_name)"
    for name in line-pulse line-native headphones-pulse headphones-native \
        default-pulse default-native null-pulse null-native; do
        printf '%s sink=%s\n' "$name" "$(stream_sink "$name" 2>/dev/null || printf absent)"
    done
    printf '%s sink=%s\n' "$line_output" "$(output_sink "$line_output" 2>/dev/null || printf absent)"
    printf '%s sink=%s\n' "$headphones_output" \
        "$(output_sink "$headphones_output" 2>/dev/null || printf absent)"
}

start_pulse() {
    local name=$1 target=${2:-}
    local args=(--playback --raw --format=s16le --rate=48000 --channels=2
        --property="media.name=$name")
    [ -z "$target" ] || args+=(--device="$target")
    pacat "${args[@]}" </dev/zero &
    pids+=("$!")
}

start_native() {
    local name=$1 target=${2:-}
    local args=(--playback --raw --format=s16 --rate=48000 --channels=2
        --properties="media.name=$name node.name=$name")
    [ -z "$target" ] || args+=(--target="$target")
    pw-cat "${args[@]}" - </dev/zero &
    pids+=("$!")
}

start_streams() {
    start_pulse line-pulse "$line_sink"
    start_native line-native "$line_sink"
    start_pulse headphones-pulse "$headphones_sink"
    start_native headphones-native "$headphones_sink"
    start_pulse default-pulse
    start_native default-native
    start_pulse null-pulse "$null_sink"
    start_native null-native "$null_sink"
}

start_new_id14_streams() {
    start_pulse line-pulse "$line_sink"
    start_native line-native "$line_sink"
    start_pulse headphones-pulse "$headphones_sink"
    start_native headphones-native "$headphones_sink"
    start_pulse default-pulse
    start_native default-native
}

start_unrelated_streams() {
    start_pulse null-pulse "$null_sink"
    start_native null-native "$null_sink"
}

stop_streams() {
    local pid
    for pid in "${pids[@]}"; do kill "$pid" >/dev/null 2>&1 || true; done
    for pid in "${pids[@]}"; do wait "$pid" >/dev/null 2>&1 || true; done
    pids=()
}

wait_all_streams() {
    local name ready
    for _ in {1..80}; do
        ready=1
        for name in line-pulse line-native headphones-pulse headphones-native \
            default-pulse default-native null-pulse null-native; do
            stream_sink "$name" >/dev/null 2>&1 || ready=0
        done
        [ "$ready" -eq 0 ] || return 0
        sleep 0.1
    done
    return 1
}

assert_on_graph() {
    wait_route line-pulse "$line_main"
    wait_route line-native "$line_main"
    wait_route headphones-pulse "$headphones_main"
    wait_route headphones-native "$headphones_main"
    wait_route default-pulse "$line_main"
    wait_route default-native "$line_main"
    wait_route null-pulse "$null_sink"
    wait_route null-native "$null_sink"
    wait_output_route "$line_output" "$line_sink"
    wait_output_route "$headphones_output" "$headphones_sink"
}

assert_off_graph() {
    wait_route line-pulse "$line_sink"
    wait_route line-native "$line_sink"
    wait_route headphones-pulse "$headphones_sink"
    wait_route headphones-native "$headphones_sink"
    wait_route default-pulse "$line_sink"
    wait_route default-native "$line_sink"
    wait_route null-pulse "$null_sink"
    wait_route null-native "$null_sink"
}

assert_clean_off() {
    local status name
    status=$("$cli" status)
    grep -Fq 'state=off requested=none service=inactive' <<<"$status"
    [ "$(systemctl --user is-enabled "$service" 2>/dev/null || true)" = disabled ]
    test ! -e "$HOME/.config/pipewire/filter-chain.conf.d/90-id14-sr.conf"
    test ! -e "${XDG_STATE_HOME:-$HOME/.local/state}/id14-sr/output"
    for name in "$line_main" "$headphones_main" "$line_output" "$headphones_output" \
        id14_sr_transport id14_sr_transport_output \
        id14_sr_headphones_transport id14_sr_headphones_transport_output; do
        if node_id "$name" >/dev/null 2>&1; then return 1; fi
    done
}

cleanup() {
    local result=$? line_id headphones_id
    trap - EXIT INT TERM
    set +e
    "$cli" off >/dev/null 2>&1
    stop_streams
    if line_id=$(node_id "$line_sink" 2>/dev/null); then
        pw-metadata -n filters -d "$line_id" "$sentinel_key" >/dev/null 2>&1
    fi
    if headphones_id=$(node_id "$headphones_sink" 2>/dev/null); then
        pw-metadata -n filters -d "$headphones_id" "$sentinel_key" >/dev/null 2>&1
    fi
    if [ -n "$module_id" ]; then pactl unload-module "$module_id" >/dev/null 2>&1; fi
    printf 'CLEANUP status=%s service=%s null_sink=%s default=%s processes=%s\n' \
        "$("$cli" status | head -n 1)" \
        "$(systemctl --user is-active "$service" 2>/dev/null)" \
        "$(if node_id "$null_sink" >/dev/null 2>&1; then printf present; else printf absent; fi)" \
        "$(default_name 2>/dev/null || printf none)" \
        "$(if [ "${#pids[@]}" -eq 0 ]; then printf none; else printf present; fi)"
    exit "$result"
}
trap cleanup EXIT INT TERM

original_default=$(default_name)
[ "$original_default" = "$line_sink" ] || {
    printf 'test requires iD14 Line default, got %s\n' "$original_default" >&2
    exit 1
}
"$cli" off
assert_clean_off
"$cli" mix auto >/dev/null

module_id=$(pactl load-module module-null-sink \
    sink_name="$null_sink" sink_properties=device.description=ID14_SR_Test_Null)
for _ in {1..40}; do node_id "$null_sink" >/dev/null 2>&1 && break; sleep 0.1; done
node_id "$null_sink" >/dev/null

printf 'ATOMIC_ROLLBACK_TESTS\n'
for stage in transport metadata; do
    set +e
    failure=$(ID14_SR_TEST_FAIL_STAGE=$stage "$cli" on 2>&1)
    failure_exit=$?
    set -e
    printf 'ROLLBACK stage=%s exit=%s output=%s\n' "$stage" "$failure_exit" "$failure"
    [ "$failure_exit" -ne 0 ]
    grep -Fq 'activation rolled back' <<<"$failure"
    assert_clean_off
    [ "$(default_name)" = "$original_default" ]
done

printf 'CYCLE1_EXISTING_STREAMS\n'
start_streams
wait_all_streams
show_routes BEFORE_ON_1
"$cli" on
assert_on_graph
show_routes AFTER_ON_1
status_on=$("$cli" status)
printf 'STATUS_ON\n%s\n' "$status_on"
grep -Fq 'state=on requested=all service=active active_outputs=line,headphones' <<<"$status_on"
idempotent=$("$cli" on)
printf 'IDEMPOTENT_ON=%s\n' "$idempotent"
grep -Fq '(already active)' <<<"$idempotent"

printf 'LIVE_MIX_ROUTING\n'
service_pid_before=$(systemctl --user show "$service" -p MainPID --value)
line_main_before=$(node_id "$line_main")
headphones_main_before=$(node_id "$headphones_main")
for mix in 25 75 0 100; do
    [ "$("$cli" mix set "$mix")" = "$mix" ]
    [ "$("$cli" mix get)" = "$mix" ]
    assert_on_graph
    [ "$(systemctl --user show "$service" -p MainPID --value)" = "$service_pid_before" ]
    [ "$(node_id "$line_main")" = "$line_main_before" ]
    [ "$(node_id "$headphones_main")" = "$headphones_main_before" ]
    printf 'MIX=%s service_pid=%s line_node=%s headphones_node=%s\n' \
        "$mix" "$service_pid_before" "$line_main_before" "$headphones_main_before"
done

line_id=$(node_id "$line_sink")
headphones_id=$(node_id "$headphones_sink")
pw-metadata -n filters "$line_id" "$sentinel_key" line-keep Spa:String >/dev/null
pw-metadata -n filters "$headphones_id" "$sentinel_key" headphones-keep Spa:String >/dev/null
pw-metadata -n filters -d "$line_id" filter.smart.after >/dev/null
degraded=$("$cli" status)
printf 'STATUS_METADATA_LOSS\n%s\n' "$degraded"
grep -Fq 'state=degraded' <<<"$degraded"
grep -Fq 'degraded_outputs=line' <<<"$degraded"
recovered=$("$cli" on)
printf 'RECOVERY=%s\n' "$recovered"
grep -Fq '(metadata recovered)' <<<"$recovered"
assert_on_graph

printf 'SERVICE_RESTART\n'
systemctl --user restart "$service"
assert_on_graph
"$cli" status

printf 'WIREPLUMBER_RESTART\n'
wp_pid_before=$(systemctl --user show wireplumber.service -p MainPID --value)
line_id_before=$(node_id "$line_sink")
headphones_id_before=$(node_id "$headphones_sink")
systemctl --user restart wireplumber.service
assert_on_graph
wp_pid_after=$(systemctl --user show wireplumber.service -p MainPID --value)
line_id_after=$(node_id "$line_sink")
headphones_id_after=$(node_id "$headphones_sink")
printf 'WIREPLUMBER_IDS before_pid=%s after_pid=%s line=%s/%s headphones=%s/%s\n' \
    "$wp_pid_before" "$wp_pid_after" "$line_id_before" "$line_id_after" \
    "$headphones_id_before" "$headphones_id_after"
[ "$wp_pid_before" != "$wp_pid_after" ]
assert_on_graph
"$cli" status

# WirePlumber recreates the metadata store on restart. Add fresh unrelated keys
# while the effect is active so OFF must preserve concurrently added metadata.
line_id=$(node_id "$line_sink")
headphones_id=$(node_id "$headphones_sink")
pw-metadata -n filters "$line_id" "$sentinel_key" line-keep Spa:String >/dev/null
pw-metadata -n filters "$headphones_id" "$sentinel_key" headphones-keep Spa:String >/dev/null

printf 'OFF_SENTINEL_PRESERVATION\n'
"$cli" off
assert_off_graph
pw-metadata -n filters | grep -Fq "key:'$sentinel_key' value:'line-keep'"
pw-metadata -n filters | grep -Fq "key:'$sentinel_key' value:'headphones-keep'"
if pw-metadata -n filters | grep -Eq "key:'(id14-sr.owner|filter.smart|filter.smart.name|filter.smart.target|filter.smart.after)'"; then
    printf 'owned metadata remained after off\n' >&2
    exit 1
fi
line_id=$(node_id "$line_sink")
headphones_id=$(node_id "$headphones_sink")
pw-metadata -n filters -d "$line_id" "$sentinel_key" >/dev/null
pw-metadata -n filters -d "$headphones_id" "$sentinel_key" >/dev/null
stop_streams

printf 'CYCLE2_NEW_STREAMS_WHILE_ON\n'
start_unrelated_streams
wait_route null-pulse "$null_sink"
wait_route null-native "$null_sink"
"$cli" on
start_new_id14_streams
wait_all_streams
assert_on_graph
show_routes AFTER_ON_2
"$cli" off
assert_off_graph
show_routes AFTER_OFF_2
stop_streams
assert_clean_off

printf 'SMART_FILTER_DUAL_SCOPE_TEST=PASS default=%s module_id=%s\n' \
    "$original_default" "$module_id"
