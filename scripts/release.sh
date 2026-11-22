#!/usr/bin/env bash
set -euo pipefail

# scripts/release.sh - Automated release and versioning helper for X806 workspace
#
# Usage:
#   ./scripts/release.sh [patch|minor|major|<x.y.z>] [OPTIONS]
#
# Options:
#   --dry-run       Preview changes without writing files or git operations
#   --allow-dirty   Allow running with uncommitted changes in git
#   --no-tag        Do not create a git tag
#   --no-commit     Do not create a git commit

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

DRY_RUN=false
ALLOW_DIRTY=false
DO_TAG=true
DO_COMMIT=true
TARGET_VERSION=""

for arg in "$@"; do
    case "$arg" in
        --dry-run)
            DRY_RUN=true
            ;;
        --allow-dirty)
            ALLOW_DIRTY=true
            ;;
        --no-tag)
            DO_TAG=false
            ;;
        --no-commit)
            DO_COMMIT=false
            ;;
        patch|minor|major)
            TARGET_VERSION="$arg"
            ;;
        [0-9]*.[0-9]*.[0-9]*)
            TARGET_VERSION="$arg"
            ;;
        -h|--help)
            echo "Usage: $0 [patch|minor|major|<x.y.z>] [--dry-run] [--allow-dirty] [--no-tag] [--no-commit]"
            exit 0
            ;;
        *)
            echo "Unknown argument: $arg" >&2
            exit 1
            ;;
    esac
done

# Check git status
if [ "$ALLOW_DIRTY" = false ]; then
    if ! git diff-index --quiet HEAD -- 2>/dev/null; then
        echo "Error: Working directory has uncommitted changes. Commit or stash them, or use --allow-dirty." >&2
        exit 1
    fi
fi

# Extract current workspace version from Cargo.toml
CURRENT_VERSION=$(grep -m 1 -E '^version = "[0-9]+\.[0-9]+\.[0-9]+"' Cargo.toml | sed -E 's/version = "([^"]+)"/\1/')
if [ -z "$CURRENT_VERSION" ]; then
    echo "Error: Could not extract workspace version from Cargo.toml" >&2
    exit 1
fi

echo "Current workspace version: $CURRENT_VERSION"

# Parse SemVer components
IFS='.' read -r MAJOR MINOR PATCH <<< "$CURRENT_VERSION"

# Find latest git tag if available
LATEST_TAG=$(git describe --tags --abbrev=0 2>/dev/null || echo "")

if [ -n "$LATEST_TAG" ]; then
    COMMIT_RANGE="${LATEST_TAG}..HEAD"
    echo "Analyzing commits since tag: $LATEST_TAG"
else
    COMMIT_RANGE="HEAD"
    echo "No tags found. Analyzing all commits."
fi

# Automatically determine next version from conventional commits if not specified
if [ -z "$TARGET_VERSION" ] || [ "$TARGET_VERSION" = "patch" ] || [ "$TARGET_VERSION" = "minor" ] || [ "$TARGET_VERSION" = "major" ]; then
    BUMP_TYPE="$TARGET_VERSION"
    if [ -z "$BUMP_TYPE" ]; then
        if git log "$COMMIT_RANGE" --grep="BREAKING CHANGE" -E --grep="^[a-z]+(\([^\)]+\))?!:" --oneline | grep -q .; then
            BUMP_TYPE="major"
        elif git log "$COMMIT_RANGE" --grep="^feat" --oneline | grep -q .; then
            BUMP_TYPE="minor"
        elif git log "$COMMIT_RANGE" --grep="^fix" --grep="^perf" --oneline | grep -q .; then
            BUMP_TYPE="patch"
        else
            BUMP_TYPE="patch"
        fi
        echo "Detected bump type from conventional commits: $BUMP_TYPE"
    fi

    case "$BUMP_TYPE" in
        major)
            NEXT_VERSION="$((MAJOR + 1)).0.0"
            ;;
        minor)
            NEXT_VERSION="${MAJOR}.$((MINOR + 1)).0"
            ;;
        patch)
            NEXT_VERSION="${MAJOR}.${MINOR}.$((PATCH + 1))"
            ;;
    esac
else
    NEXT_VERSION="$TARGET_VERSION"
fi

echo "Target version: $NEXT_VERSION"

if [ "$CURRENT_VERSION" = "$NEXT_VERSION" ]; then
    echo "Version is already $CURRENT_VERSION. Nothing to bump."
    exit 0
fi

RELEASE_DATE=$(date +%Y-%m-%d)
TAG_NAME="v$NEXT_VERSION"

echo "=== Release Summary ==="
echo "  Version: $CURRENT_VERSION -> $NEXT_VERSION"
echo "  Date:    $RELEASE_DATE"
echo "  Tag:     $TAG_NAME"
echo "======================="

# Collect categorized commits since last tag
FEATS=$(git log "$COMMIT_RANGE" --grep="^feat" --pretty=format:"- %s ([%h](https://github.com/renanalencar/X806/commit/%H))" || true)
FIXES=$(git log "$COMMIT_RANGE" --grep="^fix" --pretty=format:"- %s ([%h](https://github.com/renanalencar/X806/commit/%H))" || true)
DOCS=$(git log "$COMMIT_RANGE" --grep="^doc" --pretty=format:"- %s ([%h](https://github.com/renanalencar/X806/commit/%H))" || true)
PERFS=$(git log "$COMMIT_RANGE" --grep="^perf" --pretty=format:"- %s ([%h](https://github.com/renanalencar/X806/commit/%H))" || true)
REFACTORS=$(git log "$COMMIT_RANGE" --grep="^refactor" --pretty=format:"- %s ([%h](https://github.com/renanalencar/X806/commit/%H))" || true)

CHANGELOG_ENTRY="## [$NEXT_VERSION] - $RELEASE_DATE"
if [ -n "$FEATS" ]; then
    CHANGELOG_ENTRY="$CHANGELOG_ENTRY

### Features

$FEATS"
fi
if [ -n "$FIXES" ]; then
    CHANGELOG_ENTRY="$CHANGELOG_ENTRY

### Bug Fixes

$FIXES"
fi
if [ -n "$DOCS" ]; then
    CHANGELOG_ENTRY="$CHANGELOG_ENTRY

### Documentation

$DOCS"
fi
if [ -n "$PERFS" ]; then
    CHANGELOG_ENTRY="$CHANGELOG_ENTRY

### Performance

$PERFS"
fi
if [ -n "$REFACTORS" ]; then
    CHANGELOG_ENTRY="$CHANGELOG_ENTRY

### Refactoring

$REFACTORS"
fi

if [ "$DRY_RUN" = true ]; then
    echo ""
    echo "[DRY-RUN] Proposed changelog addition:"
    echo "----------------------------------------"
    echo "$CHANGELOG_ENTRY"
    echo "----------------------------------------"
    echo "[DRY-RUN] Completed without modifying files."
    exit 0
fi

# 1. Update Cargo.toml
sed -i -E "s/^version = \"$CURRENT_VERSION\"/version = \"$NEXT_VERSION\"/" Cargo.toml

# 2. Update Cargo.lock
cargo check --workspace --quiet

# 3. Update CHANGELOG.md (insert new section under header)
TEMP_CHANGELOG=$(mktemp)
python3 -c '
import sys

new_entry = sys.argv[1]
with open("CHANGELOG.md", "r") as f:
    content = f.read()

# Look for the first "## [" occurrence to prepend before it
pos = content.find("## [")
if pos != -1:
    updated = content[:pos] + new_entry + "\n\n" + content[pos:]
else:
    updated = content + "\n\n" + new_entry + "\n"

with open(sys.argv[2], "w") as f:
    f.write(updated)
' "$CHANGELOG_ENTRY" "$TEMP_CHANGELOG"
mv "$TEMP_CHANGELOG" CHANGELOG.md

echo "Updated Cargo.toml, Cargo.lock, and CHANGELOG.md."

# 4. Commit and Tag if requested
if [ "$DO_COMMIT" = true ]; then
    git add Cargo.toml Cargo.lock CHANGELOG.md
    git commit -m "chore(release): v$NEXT_VERSION"
    echo "Created commit: chore(release): v$NEXT_VERSION"

    if [ "$DO_TAG" = true ]; then
        git tag -a "$TAG_NAME" -m "Release $TAG_NAME"
        echo "Created git tag: $TAG_NAME"
    fi
fi

echo "Release $TAG_NAME prepared successfully!"
