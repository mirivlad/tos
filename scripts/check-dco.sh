#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-or-later
# TOS DCO gate: every commit on the current branch must carry a
# Signed-off-by trailer matching the repository identity
# (mirivlad <mirvtop@yandex.ru>). Fails with a non-zero exit code listing
# offending commits.
set -eu
cd "$(dirname "$0")/.."

signer="mirivlad <mirvtop@yandex.ru>"
fail=0
for c in $(git rev-list --all); do
    if ! git show -s --format='%B' "$c" | grep -q "Signed-off-by: $signer"; then
        echo "missing DCO: $c $(git show -s --format='%s' "$c")"
        fail=1
    fi
done
if [ "$fail" -eq 0 ]; then
    echo "check-dco: OK (every commit carries Signed-off-by: $signer)"
else
    echo "check-dco: FAIL" >&2
fi
exit "$fail"
