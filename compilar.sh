#!/bin/sh
set -eu
RUSTGPEITINHOS_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$RUSTGPEITINHOS_DIR/target}"
cargo build --release --locked --manifest-path "$RUSTGPEITINHOS_DIR/Cargo.toml"
RUSTGPEITINHOS_TEMP=$(mktemp "$RUSTGPEITINHOS_DIR/.RustGPeitinhos.new.XXXXXX")
trap 'rm -f -- "$RUSTGPEITINHOS_TEMP"' 0
trap 'exit 1' HUP INT TERM
cp "$CARGO_TARGET_DIR/release/rustgpeitinhos" "$RUSTGPEITINHOS_TEMP"
chmod 755 "$RUSTGPEITINHOS_TEMP"
mv -f -- "$RUSTGPEITINHOS_TEMP" "$RUSTGPEITINHOS_DIR/RustGPeitinhos"
