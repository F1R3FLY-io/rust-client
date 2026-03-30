#!/usr/bin/env bash
# Manual release script for rust-client
#
# Usage: ./scripts/release.sh [major|minor|patch]
#   Default bump type: minor
#
# This is an escape hatch for when you need a non-minor bump (e.g., major
# or patch). For normal releases, merging to main triggers the CI
# workflow which auto-bumps minor.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

cd "$REPO_DIR"

# Ensure working tree is clean
git diff --quiet && git diff --cached --quiet || {
    echo "ERROR: working tree is dirty — commit or stash changes first"
    exit 1
}

# Source shared version logic
source "$SCRIPT_DIR/version.sh"
bump_version "${1:-minor}"

echo "Current: ${CURRENT_VERSION} -> Next: ${NEXT_VERSION} (${TAG_NAME})"
echo ""

# Update Cargo.toml version (portable sed for GNU + BSD/macOS)
sed -i'' -e "0,/^version = \".*\"/s//version = \"${NEXT_VERSION}\"/" Cargo.toml
echo "Updated Cargo.toml to ${NEXT_VERSION}"

# Update Cargo.lock
cargo generate-lockfile 2>/dev/null || true
echo "Updated Cargo.lock"

# Generate CHANGELOG if git-cliff is available
if command -v git-cliff &>/dev/null; then
    git-cliff --config cliff.toml --tag "$TAG_NAME" -o CHANGELOG.md
    echo "Generated CHANGELOG.md"
else
    echo "WARNING: git-cliff not found, skipping CHANGELOG generation"
    echo "Install: cargo install git-cliff"
fi

# Commit and tag
git add Cargo.toml Cargo.lock CHANGELOG.md
git commit -m "chore(release): v${NEXT_VERSION}"
git tag -a "$TAG_NAME" -m "Release v${NEXT_VERSION}"

echo ""
echo "Release ${TAG_NAME} created."
echo ""
echo "To publish:"
echo "  git push origin $(git branch --show-current) --follow-tags"
