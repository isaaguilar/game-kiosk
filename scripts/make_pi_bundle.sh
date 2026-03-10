#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Create a local Raspberry Pi bundle for on-device install.

Usage:
  ./scripts/make_pi_bundle.sh [--target TARGET] [--output-dir DIR] [--no-build]

Defaults:
  --target      aarch64-unknown-linux-gnu
  --output-dir  ./dist/pi-bundle
  build         enabled
USAGE
}

TARGET="aarch64-unknown-linux-gnu"
OUTPUT_DIR="$(pwd)/dist/pi-bundle"
DO_BUILD=1

while [[ $# -gt 0 ]]; do
  case "$1" in
    --target)
      TARGET="${2:-}"
      shift 2
      ;;
    --output-dir)
      OUTPUT_DIR="${2:-}"
      shift 2
      ;;
    --no-build)
      DO_BUILD=0
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

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUTPUT_DIR="${OUTPUT_DIR/#\~/$HOME}"
mkdir -p "$OUTPUT_DIR"

if [[ "$DO_BUILD" -eq 1 ]]; then
  echo "==> Ensuring Rust target: $TARGET"
  rustup target add "$TARGET" >/dev/null || true

  echo "==> Building workspace"
  (cd "$REPO_ROOT" && cargo build --workspace --release --target "$TARGET")
fi

BIN_DIR="${REPO_ROOT}/target/${TARGET}/release"
for b in kiosk charades pictionary; do
  if [[ ! -f "${BIN_DIR}/${b}" ]]; then
    echo "Missing binary: ${BIN_DIR}/${b}" >&2
    echo "Run without --no-build, or verify your target/toolchain." >&2
    exit 1
  fi
done

echo "==> Creating bundle at ${OUTPUT_DIR}"
cp "${BIN_DIR}/kiosk" "${OUTPUT_DIR}/kiosk"
cp "${BIN_DIR}/charades" "${OUTPUT_DIR}/charades"
cp "${BIN_DIR}/pictionary" "${OUTPUT_DIR}/pictionary"

rm -rf "${OUTPUT_DIR}/charades-assets" "${OUTPUT_DIR}/pictionary-assets"
cp -R "${REPO_ROOT}/charades/assets" "${OUTPUT_DIR}/charades-assets"
cp -R "${REPO_ROOT}/pictionary/assets" "${OUTPUT_DIR}/pictionary-assets"
cp "${REPO_ROOT}/kiosk.desktop" "${OUTPUT_DIR}/kiosk.desktop"

chmod +x "${OUTPUT_DIR}/kiosk" "${OUTPUT_DIR}/charades" "${OUTPUT_DIR}/pictionary"

echo "Bundle ready: ${OUTPUT_DIR}"
