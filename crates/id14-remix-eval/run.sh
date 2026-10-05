#!/usr/bin/env bash
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
if [[ -n "${REMIX_EVAL_PYTHON:-}" ]]; then
  python_bin="$REMIX_EVAL_PYTHON"
elif [[ -x /tmp/metric-harness/venv/bin/python ]]; then
  python_bin=/tmp/metric-harness/venv/bin/python
else
  python_bin=python3
fi
exec "$python_bin" "$here/evaluate.py" "$@"
