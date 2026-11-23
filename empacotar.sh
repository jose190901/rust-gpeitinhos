#!/bin/sh
set -eu
RUSTGPEITINHOS_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
RUSTGPEITINHOS_ROOT=$(CDPATH= cd -- "$RUSTGPEITINHOS_DIR/../.." && pwd)
if [ -f "$RUSTGPEITINHOS_DIR/.rustgpeitinhos-package" ]; then
    RUSTGPEITINHOS_RUNTIME="$RUSTGPEITINHOS_DIR/runtime"
    RUSTGPEITINHOS_DEFAULT_DEST="$RUSTGPEITINHOS_DIR/../RustGPeitinhos-pacote"
else
    RUSTGPEITINHOS_RUNTIME="$RUSTGPEITINHOS_ROOT/work/runtime"
    RUSTGPEITINHOS_DEFAULT_DEST="$RUSTGPEITINHOS_ROOT/outputs/RustGPeitinhos-pacote"
fi
RUSTGPEITINHOS_DEST=${1:-"$RUSTGPEITINHOS_DEFAULT_DEST"}
if [ "$#" -gt 1 ]; then
    echo "Uso: $0 [pasta-de-destino-nova]" >&2
    exit 2
fi
python3 - "$RUSTGPEITINHOS_DIR" "$RUSTGPEITINHOS_RUNTIME" "$RUSTGPEITINHOS_DEST" <<'PY'
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

source, runtime, destination = (Path(value).absolute() for value in sys.argv[1:])
required = [source / "RustGPeitinhos", runtime / "node_modules/@openai/codex/bin/codex.js"]
for path in required:
    if not path.is_file():
        sys.exit(f"Arquivo obrigatório ausente: {path}. Compile antes de empacotar.")
if destination.exists() or destination.is_symlink():
    sys.exit(f"O destino já existe: {destination}. Escolha outra pasta para preservar o pacote anterior.")
if not destination.parent.is_dir():
    sys.exit(f"A pasta pai do destino não existe: {destination.parent}")

# Copiar apenas arquivos de distribuição declarados. Nenhum data/, preferências,
# histórico, credencial, cache, target/ ou toolchain entra no pacote.
files = [
    "RustGPeitinhos", "Cargo.toml", "Cargo.lock", "iniciar.sh", "empacotar.sh",
    "README.md", "PRODUCT.md", "DESIGN.md", "compilar.sh", "preparar.sh",
    "LICENSE", "TERMS.md", "PRIVACY.md", "SECURITY.md", "THIRD_PARTY_NOTICES.md",
]
staging = Path(tempfile.mkdtemp(prefix=".rustgpeitinhos-pacote-", dir=destination.parent))
try:
    for name in files:
        if (source / name).is_file():
            shutil.copy2(source / name, staging / name)
    for name in ("src", "assets", "docs", "examples"):
        if (source / name).is_dir():
            shutil.copytree(source / name, staging / name, symlinks=True)
    packaged_runtime = staging / "runtime"
    packaged_runtime.mkdir()
    for name in ("package.json", "package-lock.json"):
        if (runtime / name).is_file():
            shutil.copy2(runtime / name, packaged_runtime / name)
    openai_packages = packaged_runtime / "node_modules/@openai"
    openai_packages.mkdir(parents=True)
    packages = [runtime / "node_modules/@openai/codex"]
    packages.extend(sorted((runtime / "node_modules/@openai").glob("codex-*")))
    for package in packages:
        if package.is_dir() and not package.is_symlink():
            subprocess.run(
                ["cp", "-a", "--reflink=auto", "--", str(package), str(openai_packages / package.name)],
                check=True,
            )
    binary_dir = packaged_runtime / "node_modules/.bin"
    binary_dir.mkdir()
    (binary_dir / "codex").symlink_to("../@openai/codex/bin/codex.js")
    (staging / ".rustgpeitinhos-package").write_text("1\n", encoding="utf-8")

    # O pacote pode ser movido: o lançador descobre a própria pasta e não usa
    # o caminho da compilação. O .desktop reflete o destino desta exportação;
    # depois de mover, iniciar.sh continua funcionando sem regenerá-lo.
    def desktop_string(value):
        return str(value).replace("\\", "\\\\").replace("\n", "\\n").replace("\r", "\\r")

    def desktop_quoted(value):
        escaped = str(value).replace("\\", "\\\\").replace('"', '\\"').replace("`", "\\`").replace("$", "\\$")
        return '"' + desktop_string(escaped).replace("%", "%%") + '"'

    (staging / "RustGPeitinhos.desktop").write_text(
        "[Desktop Entry]\nType=Application\nName=RUST GPeitinhos\n"
        "Comment=Menu Rust para o Codex com sua assinatura ChatGPT\n"
        f"Exec={desktop_quoted(destination / 'iniciar.sh')}\n"
        f"Path={desktop_string(destination)}\n"
        f"Icon={desktop_string(destination / 'assets/logo.png')}\n"
        "Terminal=false\nCategories=Development;Utility;\nStartupWMClass=RustGPeitinhos\n",
        encoding="utf-8",
    )
    (staging / "PACOTE.md").write_text(
        "# RUST GPeitinhos — pacote local\n\n"
        "Abra `iniciar.sh` dentro desta pasta. O pacote inclui o cliente e uma cópia oficial do Codex CLI, "
        "sem precisar da pasta de desenvolvimento. Pode mover a pasta inteira; após mover, use `iniciar.sh` "
        "(o arquivo `.desktop` contém o caminho de exportação). Não há instalação global.\n\n"
        "O alvo deste pacote é o Linux da compilação. O lançador oficial do CLI requer Node.js 16 ou superior. "
        "O executável gráfico também depende das bibliotecas gráficas do sistema.\n\n"
        "Nenhuma conversa, token, cookie, preferência ou credencial foi copiada. O Codex usa o login e as "
        "configurações da conta local no computador em que for executado. As preferências novas ficam em `data/`. "
        "As skills e os plugins disponíveis dependem dessa instalação e das conexões da conta.\n\n"
        "O código-fonte está incluído. Para recompilar neste pacote, instale Rust e execute `compilar.sh`; "
        "o toolchain e os artefatos de desenvolvimento não foram incluídos. O README geral descreve as funções "
        "e os limites da versão. O runtime npm está em `runtime/`, com seus metadados, avisos e licenças "
        "fornecidos pelos pacotes oficiais. Antes de redistribuir binários, confira os textos e avisos "
        "de todas as dependências e fontes tipográficas conforme THIRD_PARTY_NOTICES.md; esta exportação "
        "local não é uma auditoria de licenças de binários.\n",
        encoding="utf-8",
    )
    for name in ("RustGPeitinhos", "iniciar.sh", "compilar.sh", "preparar.sh", "empacotar.sh"):
        (staging / name).chmod((staging / name).stat().st_mode | 0o111)
    # Uma pasta nova evita alterar ou apagar distribuições já existentes.
    if destination.exists() or destination.is_symlink():
        raise FileExistsError(f"O destino apareceu durante a exportação: {destination}")
    os.rename(staging, destination)
except BaseException:
    shutil.rmtree(staging, ignore_errors=True)
    raise
print(f"Pacote criado em: {destination}")
PY
