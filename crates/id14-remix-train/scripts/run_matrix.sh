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

case "$data_kind" in
    --moises|--slakh|--musdb) ;;
    *) echo "unsupported DATA_KIND: $data_kind" >&2; exit 2 ;;
esac

for size in 131k 444k; do
    for lookahead in 0 2 4; do
        python -m remix_train.train \
            "$data_kind" "$data_root" \
            --output "$run_root" \
            --size "$size" \
            --lookahead "$lookahead"
    done
done
