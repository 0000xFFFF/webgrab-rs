#!/usr/bin/env bash
set -euo pipefail

BINARY="target/release/webgrab"
INSTALL_DIR="/usr/local/bin"
INSTALL_PATH="$INSTALL_DIR/webgrab"

if [[ ! -f "$BINARY" ]]; then
    echo "Error: $BINARY not found. Build the project first."
    exit 1
fi

echo "Installing webgrab..."

sudo install -Dm755 "$BINARY" "$INSTALL_PATH"

echo "Installed successfully: $INSTALL_PATH"
