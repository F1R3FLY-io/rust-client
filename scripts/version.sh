#!/usr/bin/env bash
# Version discovery for rust-client releases.
#
# A version that was ever released is never released again, so a consumer that
# pins a tag always gets the same code. Released means named by a v* tag or by
# a `chore(release): vX.Y.Z` commit on the current branch; the commit counts
# because a tag can be gone while the release it named was published.
#
# Usage (sourced):
#   source scripts/version.sh
#   release_version        # sets CURRENT_VERSION, NEXT_VERSION, TAG_NAME
#
# Usage (standalone):
#   ./scripts/version.sh [current|next]

set -euo pipefail

TAG_PREFIX="v"
SEMVER='^[0-9]+\.[0-9]+\.[0-9]+$'

# Whether version $1 sorts after version $2.
version_gt() {
    [ "$1" != "$2" ] && [ "$(printf '%s\n%s\n' "$1" "$2" | sort -V | tail -1)" = "$1" ]
}

# Sets CURRENT_VERSION to the highest version ever released, or 0.1.0 if none.
get_current_version() {
    local tag
    while read -r tag; do
        if ! [[ "${tag#"$TAG_PREFIX"}" =~ $SEMVER ]]; then
            echo "ERROR: tag '$tag' is not a version" >&2
            exit 1
        fi
    done < <(git tag -l "${TAG_PREFIX}*")

    CURRENT_VERSION=$(
        {
            git tag -l "${TAG_PREFIX}*" | sed "s/^${TAG_PREFIX}//"
            git log --format=%s | sed -nE 's/^chore\(release\): v([0-9]+\.[0-9]+\.[0-9]+)$/\1/p'
        } | sort -V | tail -1
    )
    CURRENT_VERSION="${CURRENT_VERSION:-0.1.0}"
}

# Sets NEXT_VERSION and TAG_NAME: the Cargo.toml version when it is ahead of
# every released version, which is how a minor or major release is declared,
# and otherwise the next patch.
release_version() {
    get_current_version

    local declared
    declared=$(sed -nE 's/^version = "(.*)"$/\1/p' Cargo.toml | head -1)
    if ! [[ "$declared" =~ $SEMVER ]]; then
        echo "ERROR: Cargo.toml version '$declared' is not a version" >&2
        exit 1
    fi

    if version_gt "$declared" "$CURRENT_VERSION"; then
        NEXT_VERSION="$declared"
    else
        local major minor patch
        IFS=. read -r major minor patch <<<"$CURRENT_VERSION"
        NEXT_VERSION="${major}.${minor}.$((patch + 1))"
    fi
    TAG_NAME="${TAG_PREFIX}${NEXT_VERSION}"
}

if [ "${BASH_SOURCE[0]}" = "${0}" ]; then
    case "${1:-current}" in
        current)
            get_current_version
            echo "$CURRENT_VERSION"
            ;;
        next)
            release_version
            echo "$NEXT_VERSION (tag: $TAG_NAME)"
            ;;
        *)
            echo "Usage: $0 [current|next]" >&2
            exit 1
            ;;
    esac
fi
