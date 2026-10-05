#!/usr/bin/env bash
set -euo pipefail
remix_crate="$(cd "$(dirname "$0")/.." && pwd)"
export XDG_CACHE_HOME="$remix_crate/.build/cache"
export CARGO_HOME="$remix_crate/.build/cargo-home"
export CARGO_TARGET_DIR="$remix_crate/.build/target"
export CARGO_BUILD_JOBS=2
export CARGO_PROFILE_DEV_DEBUG=0
mkdir -p "$remix_crate/.build/tmp"
export TMPDIR="$remix_crate/.build/tmp"
cd "$remix_crate/../.."
if [[ -z "${ID14_ORT_LIBRARY:-}" ]]; then
  # Runtime environment for cargo test/run only; the plugin embeds no library path.
  remix_ort_store="$(nix --option eval-cache false build --no-link --print-out-paths nixpkgs#onnxruntime)"
  export ID14_ORT_LIBRARY="$remix_ort_store/lib/libonnxruntime.so"
fi
exec nix --option eval-cache false shell nixpkgs#rustc nixpkgs#cargo -c cargo "$@"
