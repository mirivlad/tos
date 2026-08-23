#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-or-later
# TOS licence-header gate.
#
# Every tracked file is classified: it either carries an SPDX identifier,
# resolves through a checked binary-artwork record, or is exempt for a stated
# reason. A file that matches none of those is a FAILURE, not a skip - the
# previous version listed '*.py' among the files to scan but had no case arm for
# it, so Python files were silently unchecked, and .toml, .ld, .md, .yml and
# .tsv were never covered at all. A gate that quietly ignores what it does not
# recognise reports success it has not earned.
#
# The gate checks that a licence is declared and that it is one of the
# licences of the LICENSE.md matrix. It deliberately does NOT decide which one
# a given file should use: that is a licensing decision for the
# maintainer, not something a shell script should enforce by directory.
set -eu
cd "$(dirname "$0")/.."

# Identifiers permitted by LICENSE.md.
allowed='GPL-3.0-or-later|Apache-2.0|CC-BY-SA-4.0|Unicode-3.0|MIT|GPL-3.0-or-later OR Apache-2.0'

fail=0
checked=0
exempt=0

# Capsule fixtures are binary generated artifacts. Their licence obligations are
# not inferable from an extension or a single SPDX expression: ADR-0019 requires
# one checked provenance entry for every tracked fixture before this source-file
# gate accepts the set.
vector_bins=$(git ls-files -- 'source/tests/vectors/capsule-v1/*.bin')
if [ -n "$vector_bins" ]; then
    if ! python3 scripts/check-capsule-vector-provenance.py --root .; then
        fail=1
    fi
fi

unicode_data_paths=$(git ls-files -- 'source/crates/tos-core/unicode/ucd-17.0.0/*.txt')
if [ -n "$unicode_data_paths" ] \
    && ! python3 scripts/check-tos-core-unicode-provenance.py --root .; then
    fail=1
fi

# Stage 1 can embed separately licensed artwork as data only through a checked
# record that retains the canonical source, attribution and licence identity.
# Test repositories without this specific boundary do not need the record; a
# partial or complete copy of the boundary must validate it.
embedded_artwork_paths=$(git ls-files -- \
    assets/mascot/pyro-stage1-provenance.json \
    assets/mascot/tos_ascii-art2.txt \
    source/nucleus/src/framebuffer.rs)
if [ -n "$embedded_artwork_paths" ] \
    && ! python3 scripts/check-embedded-artwork-provenance.py --root .; then
    fail=1
fi

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
        source/tests/vectors/capsule-v1/*.bin)
                                    checked=$((checked+1)); continue ;;  # validated above by ADR-0019 provenance manifest
        source/crates/tos-core/unicode/ucd-17.0.0/*.txt)
                                    checked=$((checked+1)); continue ;;  # exact UCD set validated above by ADR-0029 provenance
        *.lock)                     exempt=$((exempt+1)); continue ;;  # generated dependency lock
        *.gitignore)                exempt=$((exempt+1)); continue ;;  # tooling config, no licensable content
        */rust-toolchain.toml)      exempt=$((exempt+1)); continue ;;  # toolchain pin, no comment convention
    esac

    case "$f" in
        # --- binary artwork: licence/provenance lives in a tracked directory
        # record because PNG has no repository-standard source-comment slot.
        # This is deliberately path-by-path, not a global extension exemption:
        # a new image that is not listed in its directory record fails.
        *.png)
            record="${f%/*}/README.md"
            checked=$((checked+1))
            if ! git ls-files --error-unmatch "$record" >/dev/null 2>&1; then
                echo "missing binary artwork record: $f (expected $record)"
                fail=1
            elif ! grep -F "| \`$f\` |" "$record" \
                    | grep -Fq '`CC-BY-SA-4.0`'; then
                echo "binary artwork not licensed in $record: $f"
                fail=1
            fi
            continue ;;

        # --- JSON metadata: JSON has no comment syntax, so a project record
        # licence lives in its first field.  This classifies the metadata file,
        # not any generated binary described by it.
        *.json)
            checked=$((checked+1))
            line=$(head -5 "$f" | grep -m1 '"record_spdx_license"' || true)
            lic=$(printf '%s' "$line" | sed -n 's/.*"record_spdx_license"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p')
            if [ -z "$lic" ]; then
                echo "missing record_spdx_license: $f"; fail=1
            elif ! printf '%s' "$lic" | grep -qE "^($allowed)$"; then
                echo "licence not in the LICENSE.md matrix: $f ($lic)"; fail=1
            fi
            continue ;;

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
        *.rs|*.S|*.sh|*.py|*.ld|*.md|*.yml|*.yaml|*.toml|*.tsv|*.tos|*.txt)
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
