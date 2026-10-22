#!/usr/bin/env bash
# Publish the rndr crates to crates.io in dependency order.
#
# Usage:
#   scripts/publish-crates.sh            # publish unpublished versions
#   scripts/publish-crates.sh --dry-run  # package only, do not upload
#
# A real publish reads CARGO_REGISTRY_TOKEN from the environment. Cargo picks
# that up itself. This script never prints the token and never passes it as an
# argument. Do not run the script under `bash -x`.
#
# Order: rndr_core, rndr_paint, rndr_std, rndr_render, rndr.
# A version already present in the sparse index is skipped.
# After each upload, the script waits until that version is visible in the
# index before publishing the next crate.
#
# `--dry-run` does not upload and does not need CARGO_REGISTRY_TOKEN.
# A crate whose workspace dependencies are already on the index gets
# `cargo publish --dry-run` (package, verify, stop before upload).
# A crate that still depends on an unpublished sibling cannot be verified
# against crates.io, so the dry-run falls back to `cargo package --no-verify`.

set -euo pipefail

dry_run=0
case "${1:-}" in
"") ;;
--dry-run) dry_run=1 ;;
-h | --help)
    echo "usage: scripts/publish-crates.sh [--dry-run]"
    exit 0
    ;;
*)
    echo "unknown argument: $1" >&2
    exit 2
    ;;
esac
if [[ $# -gt 1 ]]; then
    echo "unexpected extra arguments" >&2
    exit 2
fi

cd "$(dirname "$0")/.."

if [[ "$dry_run" -eq 0 && -z "${CARGO_REGISTRY_TOKEN:-}" ]]; then
    echo "error: CARGO_REGISTRY_TOKEN is not set" >&2
    exit 1
fi

CRATES=(rndr_core rndr_paint rndr_std rndr_render rndr)

declare -A VERSIONS=()
while IFS=$'\t' read -r name version; do
    VERSIONS["$name"]="$version"
done < <(cargo metadata --format-version 1 --no-deps | python3 -c '
import json, sys
wanted = {"rndr_core", "rndr_paint", "rndr_std", "rndr_render", "rndr"}
meta = json.load(sys.stdin)
for package in meta["packages"]:
    if package["name"] in wanted:
        print("%s\t%s" % (package["name"], package["version"]))
')

index_url() {
    local name="${1,,}"
    local n=${#name}
    if ((n == 1)); then
        echo "https://index.crates.io/1/${name}"
    elif ((n == 2)); then
        echo "https://index.crates.io/2/${name}"
    elif ((n == 3)); then
        echo "https://index.crates.io/3/${name:0:1}/${name}"
    else
        echo "https://index.crates.io/${name:0:2}/${name:2:2}/${name}"
    fi
}

# True when the sparse index already lists this exact version.
published() {
    local name="$1"
    local version="$2"
    local body
    body="$(curl -fsS -A "rndr-publish (https://github.com/EntasisLabs/rndr)" "$(index_url "$name")" 2>/dev/null || true)"
    [[ -n "$body" ]] && grep -F -q "\"vers\":\"${version}\"" <<<"$body"
}

# Workspace crates this package needs to see on crates.io before a full dry-run.
workspace_deps() {
    case "$1" in
    rndr_core) ;;
    rndr_paint | rndr_std) echo rndr_core ;;
    rndr_render) echo rndr_core rndr_paint ;;
    rndr) echo rndr_core rndr_paint rndr_std rndr_render ;;
    *)
        echo "unknown crate: $1" >&2
        return 1
        ;;
    esac
}

deps_are_published() {
    local crate dep
    crate="$1"
    for dep in $(workspace_deps "$crate"); do
        if ! published "$dep" "${VERSIONS[$dep]}"; then
            return 1
        fi
    done
}

wait_for_index() {
    local name="$1"
    local version="$2"
    local attempt
    echo "waiting for ${name} ${version} to appear in the crates.io sparse index"
    for attempt in $(seq 1 90); do
        if published "$name" "$version"; then
            echo "${name} ${version} is in the index"
            return 0
        fi
        sleep 10
    done
    echo "timed out waiting for ${name} ${version}" >&2
    return 1
}

for index in "${!CRATES[@]}"; do
    name="${CRATES[$index]}"
    version="${VERSIONS[$name]:-}"
    if [[ -z "$version" ]]; then
        echo "error: no version for ${name}" >&2
        exit 1
    fi
    if published "$name" "$version"; then
        echo "skip ${name} ${version} (already published)"
        continue
    fi
    if [[ "$dry_run" -eq 1 ]]; then
        if deps_are_published "$name"; then
            echo "dry-run ${name} ${version}"
            cargo publish --locked --dry-run -p "$name"
        else
            echo "dry-run ${name} ${version} (dependencies are not on crates.io yet; packaging only)"
            cargo package --locked --no-verify -p "$name"
        fi
    else
        echo "publish ${name} ${version}"
        cargo publish --locked -p "$name"
        if [[ "$index" -lt $((${#CRATES[@]} - 1)) ]]; then
            wait_for_index "$name" "$version"
        fi
    fi
done
