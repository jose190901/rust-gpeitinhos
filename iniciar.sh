#!/bin/sh
set -eu
RUSTGPEITINHOS_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
exec "$RUSTGPEITINHOS_DIR/RustGPeitinhos" "$@"
