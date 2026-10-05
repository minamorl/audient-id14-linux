#!/usr/bin/env bash
# Reproduce runtime discovery and its existing regression/performance gates.
set -euo pipefail
remix_crate="$(cd "$(dirname "$0")/.." && pwd)"
remix_library="${1:?usage: check_library_paths.sh /absolute/libonnxruntime.so /absolute/remix.onnx}"
remix_model="${2:?pass the trained remix-v1 model}"
cd "$remix_crate/../.."
export ID14_ORT_LIBRARY="$remix_library"
export XDG_RUNTIME_DIR="$remix_crate/.build/check-runtime"
export TMPDIR="$remix_crate/.build/tmp"
mkdir -p "$XDG_RUNTIME_DIR" "$TMPDIR"
remix_so="$remix_crate/.build/target/release/libid14_remix_ladspa.so"
remix_sentinel="$remix_crate/.build/BUILD_ONLY_ORT_SENTINEL_MUST_NOT_BE_EMBEDDED.so"
remix_records="$remix_crate/validation"
printf 'ID14_ORT_LIBRARY=%q\nXDG_RUNTIME_DIR=%q\nTMPDIR=%q\n' \
  "$ID14_ORT_LIBRARY" "$XDG_RUNTIME_DIR" "$TMPDIR" > "$remix_records/library-commands.txt"

check() {
  local remix_name="$1" remix_exit
  shift
  printf '%q ' "$@" >> "$remix_records/library-commands.txt"
  printf '\n' >> "$remix_records/library-commands.txt"
  if "$@" > "$remix_records/library-$remix_name.log" 2>&1; then
    remix_exit=0
  else
    remix_exit=$?
  fi
  printf '%s\n' "$remix_exit" > "$remix_records/library-$remix_name.exit"
  printf 'CHECK=%s\n' "$remix_name"
  tail -4 "$remix_records/library-$remix_name.log"
  printf 'EXIT=%s\n' "$remix_exit"
  return "$remix_exit"
}

check workspace bash "$remix_crate/tools/with_cargo.sh" test --workspace
check build env ID14_ORT_LIBRARY="$remix_sentinel" \
  bash "$remix_crate/tools/with_cargo.sh" build --release -p id14-remix-ladspa
check discovery bash "$remix_crate/tools/with_python.sh" \
  "$remix_crate/tools/verify_library_discovery.py" --so "$remix_so" \
  --library "$remix_library" --build-library "$remix_sentinel"
check bench bash "$remix_crate/tools/with_cargo.sh" run --release \
  -p id14-remix-ladspa --example bench_worker -- "$remix_model" 500
check so bash "$remix_crate/tools/with_python.sh" "$remix_crate/tools/verify_so.py" \
  --so "$remix_so" --output "$remix_records/library-so-results.json"
check state bash "$remix_crate/tools/with_python.sh" \
  "$remix_crate/tools/verify_state_so.py" --so "$remix_so"
check realtime bash "$remix_crate/tools/with_python.sh" \
  "$remix_crate/tools/verify_realtime_so.py" --so "$remix_so" --model "$remix_model" --seconds 30
check diff git diff --check
sha256sum "$remix_so"
