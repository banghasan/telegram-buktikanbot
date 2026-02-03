#!/usr/bin/env bash
set -euo pipefail

kind="${1:-}"
if [ -z "$kind" ]; then
  if [ -t 1 ]; then
    COLOR_RED=$'\033[0;31m'
    COLOR_GREEN=$'\033[0;32m'
    COLOR_YELLOW=$'\033[0;33m'
    COLOR_BLUE=$'\033[0;34m'
    COLOR_BOLD=$'\033[1m'
    COLOR_RESET=$'\033[0m'
  else
    COLOR_RED=""
    COLOR_GREEN=""
    COLOR_YELLOW=""
    COLOR_BLUE=""
    COLOR_BOLD=""
    COLOR_RESET=""
  fi
  echo "Pilih jenis bump versi:"
  echo "  ${COLOR_RED}${COLOR_BOLD}1) major${COLOR_RESET} - naikkan ${COLOR_RED}${COLOR_BOLD}X${COLOR_RESET}.0.0 (ubah besar, reset minor/patch)"
  echo "  ${COLOR_YELLOW}${COLOR_BOLD}2) minor${COLOR_RESET} - naikkan 0.${COLOR_YELLOW}${COLOR_BOLD}X${COLOR_RESET}.0 (fitur baru, reset patch)"
  echo "  ${COLOR_GREEN}${COLOR_BOLD}3) patch${COLOR_RESET} - naikkan 0.0.${COLOR_GREEN}${COLOR_BOLD}X${COLOR_RESET} (perbaikan kecil)"
  echo "  ${COLOR_BLUE}${COLOR_BOLD}0) batal${COLOR_RESET} - keluar tanpa perubahan"
  printf "${COLOR_BOLD}Masukkan pilihan (0/1/2/3): ${COLOR_RESET}"
  read -r choice
  case "$choice" in
    0) echo "❌ Proses dibatalkan"; exit 0 ;;
    1) kind="major" ;;
    2) kind="minor" ;;
    3) kind="patch" ;;
    *) echo "pilihan tidak valid" >&2; exit 1 ;;
  esac
elif [ $# -ne 1 ]; then
  echo "usage: $0 {major|minor|patch}" >&2
  exit 1
fi
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
