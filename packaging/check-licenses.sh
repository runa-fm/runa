#!/usr/bin/env bash
#
# Checks the License tag of the Fedora spec matches the licenses of the crates built into runa/rn.
#
# Usage: packaging/check-licenses.sh
#
# Lists all licenses like %cargo_license_summary

set -euo pipefail

SPEC="packaging/fedora/runa.spec"

license_tag() {
    sed -n 's/^License:[[:space:]]*//p' "$SPEC" | sed 's/ AND /\n/g'
}

normalized_term() {
    local term=${1//[()]/}
    printf '%s\n' "${term// OR /$'\n'}" | sort | paste -sd '|'
}

terms() {
    sed -e 's/ AND /\n/g' | while IFS= read -r term; do
        [[ -n $term ]] || continue
        normalized_term "$term"
    done | sort -u
}

expression() {
    local crates=$1 term k out=""
    local -A wanted=() written=()

    while IFS= read -r term; do
        wanted[$term]=1
    done <<<"$crates"

    while IFS= read -r term; do
        [[ -n $term ]] || continue
        k=$(normalized_term "$term")
        [[ -n ${wanted[$k]:-} && -z ${written[$k]:-} ]] || continue
        written[$k]=1
        out+="${out:+ AND }$term"
    done < <(license_tag)

    while IFS= read -r term; do
        [[ -z ${written[$term]:-} ]] || continue
        [[ $term == *'|'* ]] && term="(${term//|/ OR })"
        out+="${out:+ AND }$term"
    done <<<"$crates"

    echo "$out"
}

main() {
    cd -P -- "$(dirname -- "${BASH_SOURCE[0]}")/.."
    export LC_ALL=C

    local spec crates
    spec=$(license_tag | terms)
    crates=$(cargo tree --locked --edges no-build,no-dev,no-proc-macro --target all \
        --no-dedupe --prefix none --format '{l}' | sed 's: */ *: OR :g' | terms)

    if [[ $spec == "$crates" ]]; then
        echo "The License tag matches the dependencies"
        return
    fi

    printf 'The License tag does not match the dependencies (< spec, > crates):\n\n' >&2
    diff --unchanged-line-format= --old-line-format='< %L' --new-line-format='> %L' \
        <(echo "${spec//|/ OR }") <(echo "${crates//|/ OR }") >&2 || true
    printf '\nLicense:        %s\n' "$(expression "$crates")" >&2
    exit 1
}

main "$@"
