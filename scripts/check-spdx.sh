#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-or-later
# TOS licence-header gate: every tracked source file must carry the project
# SPDX identifier on the first line (.rs) or on the second line after the
# shebang (.sh). Fails with a non-zero exit code listing offenders.
set -eu
cd "$(dirname "$0")/.."

fail=0
for f in $(git ls-files '*.rs' '*.sh' '*.py'); do
    case "$f" in
        *.rs)
            line1=$(sed -n '1p' "$f")
            if [ "$line1" != "// SPDX-License-Identifier: GPL-3.0-or-later" ]; then
                echo "missing SPDX (line 1): $f"; fail=1
            fi ;;
        *.sh)
            line1=$(sed -n '1p' "$f")
            line2=$(sed -n '2p' "$f")
            if [ "$line2" != "# SPDX-License-Identifier: GPL-3.0-or-later" ]; then
                echo "missing SPDX (line 2 after shebang): $f"; fail=1
            fi ;;
    esac
done
if [ "$fail" -eq 0 ]; then
    echo "check-spdx: OK (all tracked source files carry the GPL-3.0-or-later header)"
else
    echo "check-spdx: FAIL" >&2
fi
exit "$fail"
