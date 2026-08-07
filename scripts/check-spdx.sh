#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-or-later
# TOS licence-header gate.
#
# Every tracked file is classified: it either must carry an SPDX identifier, or
# it is exempt for a stated reason. A file that matches neither list is a
# FAILURE, not a skip - the previous version listed '*.py' among the files to
# scan but had no case arm for it, so Python files were silently unchecked, and
# .toml, .ld, .md, .yml and .tsv were never covered at all. A gate that quietly
# ignores what it does not recognise reports success it has not earned.
#
# The gate checks that a licence is declared and that it is one of the three
# licences of the LICENSE.md matrix. It deliberately does NOT decide which of
# the three a given file should use: that is a licensing decision for the
# maintainer, not something a shell script should enforce by directory.
set -eu
cd "$(dirname "$0")/.."

# Identifiers permitted by LICENSE.md.
allowed='GPL-3.0-or-later|Apache-2.0|CC-BY-SA-4.0|GPL-3.0-or-later OR Apache-2.0'

fail=0
checked=0
exempt=0

# Report an unrecognised path so a new file type cannot enter unnoticed.
unclassified=''

for f in $(git ls-files); do
    case "$f" in
        # --- exempt, with the reason ---
        LICENSES/*)                 exempt=$((exempt+1)); continue ;;  # the licence texts themselves
        DCO)                        exempt=$((exempt+1)); continue ;;  # verbatim upstream certificate
        VERSION|SHA256SUMS)         exempt=$((exempt+1)); continue ;;  # single-value data files
        MANIFEST.txt)               exempt=$((exempt+1)); continue ;;  # package manifest, no comment syntax
        docs/SPECIFICATION_SOURCES.txt) exempt=$((exempt+1)); continue ;;  # generator input list
        *.bin)                      exempt=$((exempt+1)); continue ;;  # binary test fixtures
        *.lock)                     exempt=$((exempt+1)); continue ;;  # generated dependency lock
        *.gitignore)                exempt=$((exempt+1)); continue ;;  # tooling config, no licensable content
        */rust-toolchain.toml)      exempt=$((exempt+1)); continue ;;  # toolchain pin, no comment convention
    esac

    case "$f" in
        # --- Cargo manifests: the licence belongs in the `license` field ---
        *Cargo.toml)
            if grep -q '^\[package\]' "$f"; then
                lic=$(sed -n 's/^license *= *"\(.*\)"/\1/p' "$f" | head -1)
                checked=$((checked+1))
                if [ -z "$lic" ]; then
                    echo "missing 'license' field: $f"; fail=1
                elif ! printf '%s' "$lic" | grep -qE "^($allowed)$"; then
                    echo "licence not in the LICENSE.md matrix: $f ($lic)"; fail=1
                fi
            else
                exempt=$((exempt+1))  # virtual workspace manifest: no package to license
            fi
            continue ;;

        # --- text formats that must carry an SPDX header ---
        *.rs|*.sh|*.py|*.ld|*.md|*.yml|*.yaml|*.toml|*.tsv|*.tos|*.txt)
            checked=$((checked+1))
            # The identifier must appear in the first five lines: after a
            # shebang, an HTML comment opener or a short header block, but not
            # buried in the body where a reader would not find it.
            line=$(head -5 "$f" | grep -m1 'SPDX-License-Identifier:' || true)
            if [ -z "$line" ]; then
                echo "missing SPDX identifier: $f"; fail=1
                continue
            fi
            id=$(printf '%s' "$line" | sed -e 's/.*SPDX-License-Identifier:[[:space:]]*//' \
                                           -e 's/[[:space:]]*-->[[:space:]]*$//' \
                                           -e 's|[[:space:]]*\*/[[:space:]]*$||' \
                                           -e 's/[[:space:]]*$//')
            if ! printf '%s' "$id" | grep -qE "^($allowed)$"; then
                echo "licence not in the LICENSE.md matrix: $f ($id)"; fail=1
            fi
            continue ;;
    esac

    unclassified="$unclassified $f"
done

if [ -n "$unclassified" ]; then
    echo "unclassified file type - extend scripts/check-spdx.sh before adding it:"
    for u in $unclassified; do echo "    $u"; done
    fail=1
fi

if [ "$fail" -eq 0 ]; then
    echo "check-spdx: OK ($checked file(s) carry a licence from the LICENSE.md matrix, $exempt exempt by rule)"
else
    echo "check-spdx: FAIL" >&2
fi
exit "$fail"
