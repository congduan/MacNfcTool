#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")" && pwd)"
LIBNFC_SRC_DIR="$ROOT_DIR/third_party/libnfc"
LIBNFC_BUILD_DIR="$LIBNFC_SRC_DIR/build"
LIBNFC_INSTALL_DIR="$LIBNFC_BUILD_DIR/install"
TAURI_ICON_PATH="$ROOT_DIR/src-tauri/icons/icon.png"

require_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "[ERROR] Missing command: $1"
    exit 1
  fi
}

build_libnfc_from_source() {
  if [ ! -d "$LIBNFC_SRC_DIR" ]; then
    echo "[ERROR] Missing libnfc source: $LIBNFC_SRC_DIR"
    echo "Please download libnfc source and place it under third_party/libnfc"
    exit 1
  fi

  mkdir -p "$LIBNFC_BUILD_DIR"

  if [ ! -f "$LIBNFC_SRC_DIR/configure" ]; then
    if command -v autoreconf >/dev/null 2>&1; then
      echo "[INFO] Running autoreconf -fi for libnfc"
      (cd "$LIBNFC_SRC_DIR" && autoreconf -fi)
    elif [ -x "$LIBNFC_SRC_DIR/autogen.sh" ]; then
      echo "[INFO] Running autogen.sh for libnfc"
      (cd "$LIBNFC_SRC_DIR" && ./autogen.sh)
    else
      echo "[ERROR] libnfc configure not found."
      echo "Install autotools and retry: brew install autoconf automake libtool"
      exit 1
    fi
  fi

  if [ ! -f "$LIBNFC_INSTALL_DIR/lib/libnfc.a" ]; then
    echo "[INFO] Configuring libnfc"
    (
      cd "$LIBNFC_BUILD_DIR"
      "$LIBNFC_SRC_DIR/configure" \
        --prefix="$LIBNFC_INSTALL_DIR" \
        --disable-shared \
        --enable-static
    )

    echo "[INFO] Building libnfc"
    make -C "$LIBNFC_BUILD_DIR" -j"$(sysctl -n hw.ncpu)"

    echo "[INFO] Installing libnfc to $LIBNFC_INSTALL_DIR"
    make -C "$LIBNFC_BUILD_DIR" install
  else
    echo "[INFO] Reusing built libnfc: $LIBNFC_INSTALL_DIR/lib/libnfc.a"
  fi
}

ensure_tauri_icon() {
  if [ -f "$TAURI_ICON_PATH" ]; then
    return
  fi
  mkdir -p "$(dirname "$TAURI_ICON_PATH")"
  # 1x1 RGBA PNG
  printf 'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVQIHWP4////fwAJ+wP9KobjigAAAABJRU5ErkJggg==' | base64 -d > "$TAURI_ICON_PATH"
}

detect_pn532_uart_device() {
  local dev
  for dev in /dev/cu.wchusbserial* /dev/cu.usbserial* /dev/tty.wchusbserial* /dev/tty.usbserial*; do
    if [ -e "$dev" ]; then
      printf "pn532_uart:%s" "$dev"
      return 0
    fi
  done
  return 1
}

main() {
  require_cmd npm
  require_cmd cargo
  require_cmd make
  require_cmd pkg-config

  ensure_tauri_icon
  build_libnfc_from_source

  export LIBNFC_INCLUDE_DIR="$LIBNFC_INSTALL_DIR/include"
  export LIBNFC_LIB_DIR="$LIBNFC_INSTALL_DIR/lib"
  export PKG_CONFIG_PATH="$LIBNFC_LIB_DIR/pkgconfig:${PKG_CONFIG_PATH:-}"
  export LIBNFC_AUTO_SCAN="${LIBNFC_AUTO_SCAN:-true}"
  export LIBNFC_INTRUSIVE_SCAN="${LIBNFC_INTRUSIVE_SCAN:-true}"
  export LIBNFC_LOG_LEVEL="${LIBNFC_LOG_LEVEL:-1}"

  if [ $# -ge 1 ] && [ -n "${1:-}" ]; then
    export LIBNFC_DEVICE="$1"
  elif [ -z "${LIBNFC_DEVICE:-}" ]; then
    if auto_dev="$(detect_pn532_uart_device)"; then
      export LIBNFC_DEVICE="$auto_dev"
    fi
  fi

  echo "[INFO] LIBNFC_INCLUDE_DIR=$LIBNFC_INCLUDE_DIR"
  echo "[INFO] LIBNFC_LIB_DIR=$LIBNFC_LIB_DIR"
  echo "[INFO] LIBNFC_AUTO_SCAN=$LIBNFC_AUTO_SCAN"
  echo "[INFO] LIBNFC_INTRUSIVE_SCAN=$LIBNFC_INTRUSIVE_SCAN"
  if [ -n "${LIBNFC_DEVICE:-}" ]; then
    echo "[INFO] LIBNFC_DEVICE=$LIBNFC_DEVICE"
  fi

  if [ ! -d "$ROOT_DIR/node_modules" ]; then
    echo "[INFO] Installing npm dependencies"
    (cd "$ROOT_DIR" && npm install)
  fi

  echo "[INFO] Running cargo check"
  (cd "$ROOT_DIR/src-tauri" && cargo check)

  echo "[INFO] Starting tauri dev"
  (cd "$ROOT_DIR" && npm run tauri:dev)
}

main "$@"
