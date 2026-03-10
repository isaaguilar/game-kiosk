#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Pi-local installer (no SSH/SCP).

Usage:
  ./scripts/install_pi_local.sh [--source-dir DIR] [--install-dir DIR] [--no-autostart] [--dry-run]

Defaults:
  --source-dir   ./dist/pi-bundle
  --install-dir  ~/.local/games-kiosk
  autostart      enabled

Expected source-dir layout:
  kiosk
  charades
  pictionary
  charades-assets/
  pictionary-assets/
  kiosk.desktop
USAGE
}

SOURCE_DIR="$(pwd)/dist/pi-bundle"
INSTALL_DIR="${HOME}/.local/games-kiosk"
ENABLE_AUTOSTART=1
DRY_RUN=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --source-dir)
      SOURCE_DIR="${2:-}"
      shift 2
      ;;
    --install-dir)
      INSTALL_DIR="${2:-}"
      shift 2
      ;;
    --no-autostart)
      ENABLE_AUTOSTART=0
      shift
      ;;
    --dry-run)
      DRY_RUN=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown argument: $1" >&2
      usage
      exit 1
      ;;
  esac
done

if [[ "$(uname -s)" != "Linux" ]]; then
  echo "This installer is intended for Linux (Raspberry Pi OS)." >&2
  exit 1
fi

SOURCE_DIR="$(cd "$(dirname "$SOURCE_DIR")" && pwd)/$(basename "$SOURCE_DIR")"
INSTALL_DIR="${INSTALL_DIR/#\~/$HOME}"

required_files=(
  "${SOURCE_DIR}/kiosk"
  "${SOURCE_DIR}/charades"
  "${SOURCE_DIR}/pictionary"
  "${SOURCE_DIR}/kiosk.desktop"
)
required_dirs=(
  "${SOURCE_DIR}/charades-assets"
  "${SOURCE_DIR}/pictionary-assets"
)

for file in "${required_files[@]}"; do
  if [[ ! -f "$file" ]]; then
    echo "Missing required file: $file" >&2
    exit 1
  fi
done
for dir in "${required_dirs[@]}"; do
  if [[ ! -d "$dir" ]]; then
    echo "Missing required directory: $dir" >&2
    exit 1
  fi
done

DESKTOP_DIR="${HOME}/Desktop"
APPS_DIR="${HOME}/.local/share/applications"
AUTOSTART_DIR="${HOME}/.config/autostart"

run() {
  if [[ "$DRY_RUN" -eq 1 ]]; then
    echo "[dry-run] $*"
  else
    eval "$@"
  fi
}

echo "Installing Game Kiosk"
echo "  source:  ${SOURCE_DIR}"
echo "  install: ${INSTALL_DIR}"
echo "  autostart: $([[ "$ENABLE_AUTOSTART" -eq 1 ]] && echo enabled || echo disabled)"

run "mkdir -p \"${INSTALL_DIR}\""
run "mkdir -p \"${DESKTOP_DIR}\" \"${APPS_DIR}\" \"${AUTOSTART_DIR}\""

# Copy binaries and assets into stable on-device paths.
run "cp \"${SOURCE_DIR}/kiosk\" \"${INSTALL_DIR}/kiosk\""
run "cp \"${SOURCE_DIR}/charades\" \"${INSTALL_DIR}/charades\""
run "cp \"${SOURCE_DIR}/pictionary\" \"${INSTALL_DIR}/pictionary\""
run "rm -rf \"${INSTALL_DIR}/charades-assets\" \"${INSTALL_DIR}/pictionary-assets\""
run "cp -R \"${SOURCE_DIR}/charades-assets\" \"${INSTALL_DIR}/charades-assets\""
run "cp -R \"${SOURCE_DIR}/pictionary-assets\" \"${INSTALL_DIR}/pictionary-assets\""
run "chmod +x \"${INSTALL_DIR}/kiosk\" \"${INSTALL_DIR}/charades\" \"${INSTALL_DIR}/pictionary\""

TMP_DESKTOP="$(mktemp)"
cleanup() {
  rm -f "$TMP_DESKTOP"
}
trap cleanup EXIT

sed "s|^Exec=.*$|Exec=${INSTALL_DIR}/kiosk|" "${SOURCE_DIR}/kiosk.desktop" > "$TMP_DESKTOP"

run "cp \"$TMP_DESKTOP\" \"${DESKTOP_DIR}/kiosk.desktop\""
run "cp \"$TMP_DESKTOP\" \"${APPS_DIR}/kiosk.desktop\""
run "chmod +x \"${DESKTOP_DIR}/kiosk.desktop\" \"${APPS_DIR}/kiosk.desktop\""

if [[ "$ENABLE_AUTOSTART" -eq 1 ]]; then
  run "cp \"$TMP_DESKTOP\" \"${AUTOSTART_DIR}/kiosk.desktop\""
  run "chmod +x \"${AUTOSTART_DIR}/kiosk.desktop\""
else
  if [[ "$DRY_RUN" -eq 1 ]]; then
    echo "[dry-run] remove ${AUTOSTART_DIR}/kiosk.desktop if it exists"
  else
    rm -f "${AUTOSTART_DIR}/kiosk.desktop"
  fi
fi

echo
echo "Install complete."
echo "Launch: ${INSTALL_DIR}/kiosk"
echo "Desktop icon: ${DESKTOP_DIR}/kiosk.desktop"
if [[ "$ENABLE_AUTOSTART" -eq 1 ]]; then
  echo "Autostart: ${AUTOSTART_DIR}/kiosk.desktop"
else
  echo "Autostart: disabled"
fi
