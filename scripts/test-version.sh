#!/usr/bin/env bash
# Tests scripts/version.sh against throwaway repositories.

set -euo pipefail

VERSION_SH="$(cd "$(dirname "$0")" && pwd)/version.sh"
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
FAILED=0

# A fresh repository whose Cargo.toml declares version $1.
repo() {
    local dir="$WORK/$RANDOM$RANDOM"
    git init -q "$dir"
    cd "$dir"
    git config user.name test
    git config user.email test@example.com
    printf '[package]\nname = "x"\nversion = "%s"\n' "$1" >Cargo.toml
    git add Cargo.toml
    git commit -qm "initial"
}

commit() {
    git commit -q --allow-empty -m "$1"
}

expect_next() {
    local what="$1" expected="$2" actual
    actual=$("$VERSION_SH" next 2>&1) || actual="error: $actual"
    if [ "$actual" = "$expected" ]; then
        echo "ok   $what"
    else
        echo "FAIL $what: expected '$expected', got '$actual'"
        FAILED=1
    fi
}

repo 0.2.7
git tag v0.2.7
expect_next "a tag alone gets the next patch" "0.2.8 (tag: v0.2.8)"

repo 0.2.7
git tag v0.2.7
commit "chore(release): v0.2.8"
expect_next "a release whose tag is gone is never re-cut" "0.2.9 (tag: v0.2.9)"

repo 0.3.0
git tag v0.2.7
commit "chore(release): v0.2.8"
expect_next "Cargo.toml ahead of every release declares the version" "0.3.0 (tag: v0.3.0)"

repo 0.2.7
git tag v0.2.10
expect_next "versions compare numerically" "0.2.11 (tag: v0.2.11)"

repo 0.2.7
git tag v0.2.7
git tag vnext
expect_next "a tag that is not a version is an error" "error: ERROR: tag 'vnext' is not a version"

exit "$FAILED"
