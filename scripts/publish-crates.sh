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
# Order: rndr_core, rndr_paint, rndr_std, rndr_term, rndr_render, rndr.
# rndr_term depends on rndr_core, and its dev-dependency on rndr_paint is
# versioned. Cargo writes a lockfile for the packaged crate and resolves that
# dev-dependency from crates.io, so rndr_paint has to be published first.
# rndr's optional `term` feature depends on rndr_term, so the facade is last.
# A version already present in the sparse index is skipped.
# After each upload, the script waits until that version is visible in the
# index before publishing the next crate.
#
# `--dry-run` does not upload and does not need CARGO_REGISTRY_TOKEN.
# It packages every publishable crate with `cargo package --workspace
# --no-verify`, which does not need the crates.io index. A crate whose
# workspace dependencies are already on the index also gets
# `cargo publish --dry-run` (verify, then stop before upload). Packaging one
# dependent with `-p` fails until those dependencies exist in the index, so
# the dry-run does not do that.

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

CRATES=(rndr_core rndr_paint rndr_std rndr_term rndr_render rndr)

declare -A VERSIONS=()
while IFS=$'\t' read -r name version; do
    VERSIONS["$name"]="$version"
done < <(cargo metadata --format-version 1 --no-deps | python3 -c '
import json, sys
wanted = {"rndr_core", "rndr_term", "rndr_paint", "rndr_std", "rndr_render", "rndr"}
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
    # rndr_paint is a versioned dev-dependency. The packaged lockfile resolves it.
    rndr_term) echo rndr_core rndr_paint ;;
    rndr_render) echo rndr_core rndr_paint ;;
    rndr) echo rndr_core rndr_paint rndr_std rndr_term rndr_render ;;
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

if [[ "$dry_run" -eq 1 ]]; then
    echo "packaging publishable crates without uploading"
    cargo package --workspace --locked --no-verify \
        --exclude expand --exclude idle --exclude overlay --exclude palette --exclude terminal --exclude stream
fi

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
            echo "note: ${name} ${version} is in the workspace package above; cargo publish cannot verify it until its dependencies are on the index"
        fi
    else
        echo "publish ${name} ${version}"
        cargo publish --locked -p "$name"
        if [[ "$index" -lt $((${#CRATES[@]} - 1)) ]]; then
            wait_for_index "$name" "$version"
        fi
    fi
done
