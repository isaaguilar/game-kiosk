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
  trivia
  charades-assets/
  pictionary-assets/
  trivia-assets/
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
  "${SOURCE_DIR}/trivia"
  "${SOURCE_DIR}/kiosk.desktop"
)
required_dirs=(
  "${SOURCE_DIR}/charades-assets"
  "${SOURCE_DIR}/pictionary-assets"
  "${SOURCE_DIR}/trivia-assets"
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
CONFIG_DIR="${HOME}/.config/games-kiosk"
ENV_FILE="${CONFIG_DIR}/trivia.env"
LAUNCHER="${INSTALL_DIR}/kiosk-launch"

run() {
  if [[ "$DRY_RUN" -eq 1 ]]; then
    echo "[dry-run] $*"
  else
    eval "$@"
  fi
}

# Replace executables via rename so deploys work while the current kiosk binary is running.
install_executable() {
  local src="$1"
  local dest="$2"

  if [[ "$DRY_RUN" -eq 1 ]]; then
    echo "[dry-run] install ${src} -> ${dest} via atomic rename"
    return
  fi

  local tmp
  tmp="$(mktemp "${dest}.tmp.XXXXXX")"
  if ! install -m 755 "$src" "$tmp"; then
    rm -f "$tmp"
    return 1
  fi
  if ! mv -f "$tmp" "$dest"; then
    rm -f "$tmp"
    return 1
  fi
}

echo "Installing Game Kiosk"
echo "  source:  ${SOURCE_DIR}"
echo "  install: ${INSTALL_DIR}"
echo "  autostart: $([[ "$ENABLE_AUTOSTART" -eq 1 ]] && echo enabled || echo disabled)"

run "mkdir -p \"${INSTALL_DIR}\""
run "mkdir -p \"${DESKTOP_DIR}\" \"${APPS_DIR}\" \"${AUTOSTART_DIR}\""
run "mkdir -p \"${CONFIG_DIR}\""

# Copy binaries and assets into stable on-device paths.
install_executable "${SOURCE_DIR}/kiosk" "${INSTALL_DIR}/kiosk"
install_executable "${SOURCE_DIR}/charades" "${INSTALL_DIR}/charades"
install_executable "${SOURCE_DIR}/pictionary" "${INSTALL_DIR}/pictionary"
install_executable "${SOURCE_DIR}/trivia" "${INSTALL_DIR}/trivia"
run "rm -rf \"${INSTALL_DIR}/charades-assets\" \"${INSTALL_DIR}/pictionary-assets\" \"${INSTALL_DIR}/trivia-assets\""
run "cp -R \"${SOURCE_DIR}/charades-assets\" \"${INSTALL_DIR}/charades-assets\""
run "cp -R \"${SOURCE_DIR}/pictionary-assets\" \"${INSTALL_DIR}/pictionary-assets\""
run "cp -R \"${SOURCE_DIR}/trivia-assets\" \"${INSTALL_DIR}/trivia-assets\""

if [[ "$DRY_RUN" -eq 1 ]]; then
  cat <<EOF
[dry-run] write launcher script at ${LAUNCHER}
[dry-run] chmod +x ${LAUNCHER}
EOF
else
  cat > "${LAUNCHER}" <<EOF
#!/usr/bin/env bash
set -euo pipefail
ENV_FILE="${ENV_FILE}"
if [[ -f "\$ENV_FILE" ]]; then
  # shellcheck source=/dev/null
  source "\$ENV_FILE"
fi
exec "${INSTALL_DIR}/kiosk" "\$@"
EOF
  chmod +x "${LAUNCHER}"
fi

if [[ "$DRY_RUN" -eq 1 ]]; then
  echo "[dry-run] create ${ENV_FILE} template (if missing) with 0600 permissions"
else
  if [[ ! -f "${ENV_FILE}" ]]; then
    cat > "${ENV_FILE}" <<'EOF'
# Set your Gemini API key for Trivia mode.
# Keep this file private: chmod 600 ~/.config/games-kiosk/trivia.env
export GOOGLE_API_KEY=""
EOF
  fi
  chmod 600 "${ENV_FILE}"
fi

TMP_DESKTOP="$(mktemp)"
cleanup() {
  rm -f "$TMP_DESKTOP"
}
trap cleanup EXIT

sed "s|^Exec=.*$|Exec=${LAUNCHER}|" "${SOURCE_DIR}/kiosk.desktop" > "$TMP_DESKTOP"

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
echo "Trivia env file: ${ENV_FILE} (chmod 600)"
