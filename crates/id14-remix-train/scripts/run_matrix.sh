#!/bin/sh
set -eu

if [ "$#" -lt 2 ]; then
    echo "usage: $0 DATA_KIND DATA_ROOT [RUN_ROOT]" >&2
    echo "DATA_KIND is --moises, --slakh, or --musdb" >&2
    exit 2
fi

data_kind=$1
data_root=$2
run_root=${3:-runs}
pid_file=$run_root/training.pid

mkdir -p "$run_root"
cleanup() {
    rm -f "$pid_file"
}
trap cleanup EXIT INT TERM

case "$data_kind" in
    --moises|--slakh|--musdb) ;;
    *) echo "unsupported DATA_KIND: $data_kind" >&2; exit 2 ;;
esac

for variant in "131k 2" "131k 0" "131k 4" "444k 0" "444k 2" "444k 4"; do
    set -- $variant
    python -m remix_train.train \
        "$data_kind" "$data_root" \
        --output "$run_root" \
        --size "$1" \
        --lookahead "$2" &
    training_pid=$!
    printf '%s\n' "$training_pid" >"$pid_file"
    wait "$training_pid"
done
