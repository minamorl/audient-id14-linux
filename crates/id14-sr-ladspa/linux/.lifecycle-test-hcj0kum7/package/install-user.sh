#!/usr/bin/env bash
set -euo pipefail
here=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
exec python3 "$here/id14-sr-lifecycle.py" install "$@"
