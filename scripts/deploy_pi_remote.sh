#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Create/update bundle locally, copy it to Raspberry Pi via SCP, and run the Pi-local installer over SSH.

Usage:
  ./scripts/deploy_pi_remote.sh --host HOST [--user USER] [--target TARGET] [--bundle-dir DIR] [--remote-dir DIR] [--no-build] [--no-autostart]

Defaults:
  --user        pi
  --target      aarch64-unknown-linux-gnu
  --bundle-dir  ./dist/pi-bundle
  --remote-dir  ~/pi-bundle
  build         enabled
  autostart     enabled
USAGE
}

HOST=""
USER_NAME="pi"
TARGET="aarch64-unknown-linux-gnu"
BUNDLE_DIR="$(pwd)/dist/pi-bundle"
REMOTE_DIR="~/pi-bundle"
DO_BUILD=1
ENABLE_AUTOSTART=1

while [[ $# -gt 0 ]]; do
  case "$1" in
    --host)
      HOST="${2:-}"
      shift 2
      ;;
    --user)
      USER_NAME="${2:-}"
      shift 2
      ;;
    --target)
      TARGET="${2:-}"
      shift 2
      ;;
    --bundle-dir)
      BUNDLE_DIR="${2:-}"
      shift 2
      ;;
    --remote-dir)
      REMOTE_DIR="${2:-}"
      shift 2
      ;;
    --no-build)
      DO_BUILD=0
      shift
      ;;
    --no-autostart)
      ENABLE_AUTOSTART=0
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

if [[ -z "$HOST" ]]; then
  echo "Error: --host is required" >&2
  usage
  exit 1
fi

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BUNDLE_DIR="${BUNDLE_DIR/#\~/$HOME}"

MAKE_ARGS=(--target "$TARGET" --output-dir "$BUNDLE_DIR")
if [[ "$DO_BUILD" -eq 0 ]]; then
  MAKE_ARGS+=(--no-build)
fi

echo "==> Preparing local bundle"
"${REPO_ROOT}/scripts/make_pi_bundle.sh" "${MAKE_ARGS[@]}"

REMOTE="${USER_NAME}@${HOST}"
echo "==> Ensuring remote bundle dir: ${REMOTE_DIR}"
ssh "$REMOTE" "mkdir -p ${REMOTE_DIR}"

echo "==> Uploading bundle"
scp -r "${BUNDLE_DIR}/." "${REMOTE}:${REMOTE_DIR}/"

echo "==> Uploading installer"
scp "${REPO_ROOT}/scripts/install_pi_local.sh" "${REMOTE}:${REMOTE_DIR}/install_pi_local.sh"

REMOTE_CMD="cd ${REMOTE_DIR} && chmod +x install_pi_local.sh && ./install_pi_local.sh --source-dir ${REMOTE_DIR}"
if [[ "$ENABLE_AUTOSTART" -eq 0 ]]; then
  REMOTE_CMD+=" --no-autostart"
fi

echo "==> Running installer on Pi"
ssh "$REMOTE" "$REMOTE_CMD"

echo
echo "Remote deploy complete."
