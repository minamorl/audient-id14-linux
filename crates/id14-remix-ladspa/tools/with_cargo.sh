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
exec nix --option eval-cache false shell nixpkgs#rustc nixpkgs#cargo -c cargo "$@"
