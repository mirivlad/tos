#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-or-later
set -eu
ROOT=$(CDPATH= cd -- "$(dirname "$0")/../.." && pwd)
exec python3 "$ROOT/scripts/check-stage2-language-contract.py" --root "$ROOT"
