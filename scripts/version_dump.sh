#!/usr/bin/env bash
set -euo pipefail

manifest="Cargo.toml"
version=$(rg -m1 '^version\s*=' "$manifest" | sed -E 's/.*"([^"]+)".*/\1/')

IFS='.' read -r major minor patch <<< "$version"

echo "version=$version"
echo "major=${major:-0}"
echo "minor=${minor:-0}"
echo "patch=${patch:-0}"
