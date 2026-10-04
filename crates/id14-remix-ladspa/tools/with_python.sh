#!/usr/bin/env bash
set -euo pipefail
remix_crate="$(cd "$(dirname "$0")/.." && pwd)"
export XDG_CACHE_HOME="$remix_crate/.build/cache"
export PYTHONDONTWRITEBYTECODE=1
exec nix --option eval-cache false shell --impure --expr 'let pkgs = import (builtins.getFlake "nixpkgs").outPath {}; in pkgs.python3.withPackages (ps: [ps.onnx ps.numpy ps.scipy])' -c python3 "$@"
