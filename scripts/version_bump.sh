#!/usr/bin/env bash
set -euo pipefail

if [ $# -ne 1 ]; then
  echo "usage: $0 {major|minor|patch}" >&2
  exit 1
fi

kind="$1"
manifest="Cargo.toml"
version=$(rg -m1 '^version\s*=' "$manifest" | sed -E 's/.*"([^"]+)".*/\1/')

IFS='.' read -r major minor patch <<< "$version"
major=${major:-0}
minor=${minor:-0}
patch=${patch:-0}

case "$kind" in
  major)
    major=$((major + 1))
    minor=0
    patch=0
    ;;
  minor)
    minor=$((minor + 1))
    patch=0
    ;;
  patch)
    patch=$((patch + 1))
    ;;
  *)
    echo "unknown bump: $kind" >&2
    exit 1
    ;;
esac

new_version="${major}.${minor}.${patch}"
sed -E -i "s/^(version\\s*=\\s*\")([^\"]+)(\".*)/\\1${new_version}\\3/" "$manifest"
echo "bumped to $new_version"
