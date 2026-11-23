#!/bin/sh
set -eu
RUSTGPEITINHOS_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
command -v npm >/dev/null 2>&1 || { echo "Instale Node.js 16 ou superior e npm antes de preparar o Codex." >&2; exit 1; }
mkdir -p -- "$RUSTGPEITINHOS_DIR/runtime"
npm install --prefix "$RUSTGPEITINHOS_DIR/runtime" --save-exact --no-audit --no-fund '@openai/codex@0.171.0'
printf '%s
' 'Codex local instalado. Para entrar com sua conta ChatGPT, execute:' './runtime/node_modules/.bin/codex login'
