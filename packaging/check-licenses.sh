#!/usr/bin/env bash
#
# Checks the License tag of the Fedora spec matches the licenses of the crates built into runa/rn.
#
# Usage: packaging/check-licenses.sh
#
# Lists all licenses like %cargo_license_summary

set -euo pipefail

SPEC="packaging/fedora/runa.spec"

terms() {
    sed -e 's/ AND /\n/g' | tr -d '()' | while IFS= read -r term; do
        [[ -n $term ]] || continue
        printf '%s\n' "${term// OR /$'\n'}" | sort | paste -sd '|'
    done | sort -u
}

expression() {
    local term out=""
    while IFS= read -r term; do
        [[ $term == *'|'* ]] && term="(${term//|/ OR })"
        out+="${out:+ AND }$term"
    done
    echo "$out"
}

main() {
    cd -P -- "$(dirname -- "${BASH_SOURCE[0]}")/.."
    export LC_ALL=C

    local spec crates
    spec=$(sed -n 's/^License:[[:space:]]*//p' "$SPEC" | terms)
    crates=$(cargo tree --locked --edges no-build,no-dev,no-proc-macro --target all \
        --no-dedupe --prefix none --format '{l}' | sed 's: */ *: OR :g' | terms)

    if [[ $spec == "$crates" ]]; then
        echo "The License tag matches the dependencies"
        return
    fi

    printf 'The License tag does not match the dependencies (< spec, > crates):\n\n' >&2
    diff <(echo "$spec") <(echo "$crates") >&2 || true
    printf '\nLicense:        %s\n' "$(expression <<<"$crates")" >&2
    exit 1
}

main "$@"
