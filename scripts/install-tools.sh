#!/usr/bin/env bash
# PT: Instala no espaço do usuário (sem sudo) as ferramentas que não vêm do cargo:
#     gh, binaryen (wasm-opt) e Node.js. Cada download é conferido contra o sha256
#     publicado pelo próprio fornecedor antes de ser extraído.
# EN: Installs into user space (no sudo) the tools that do not come from cargo:
#     gh, binaryen (wasm-opt) and Node.js. Every download is checked against the
#     sha256 published by the vendor itself before it is extracted.
set -euo pipefail

# PT: Versões fixas; para atualizar, troque aqui e rode de novo.
# EN: Pinned versions; to upgrade, change them here and run again.
GH_VERSION="2.103.0"
BINARYEN_VERSION="133"
NODE_VERSION="24.22.1"

# PT: sha256 fixados na primeira verificação (23/10/2026). Cada download precisa bater com o valor
#     fixado E com a lista publicada pelo fornecedor; ao trocar a versão, atualize os dois juntos.
# EN: sha256 values pinned on first verification (2026-10-23). Each download must match the pinned
#     value AND the vendor's published list; when changing a version, update both together.
GH_SHA256="fdd768eb70f6409f80fcfa9f3a1c0adf0fc98d827ab3ce0bed2ac22762c5d786"
BINARYEN_SHA256="2dc9c7813f5375db93d96ead4b78222fcc3e2677bbb832297af4797782a37489"
NODE_SHA256="6c172a9ab6718f7b801fb5a7bd486af2ee5e32758086a04b84fcce2aa77095e6"

OPT_DIR="$HOME/.local/opt"
BIN_DIR="$HOME/.local/bin"
WORK_DIR="$(mktemp -d)"
trap 'rm -rf "$WORK_DIR"' EXIT

# PT: Baixa só por HTTPS com TLS 1.2 ou superior e falha em qualquer erro HTTP.
# EN: Downloads over HTTPS only, with TLS 1.2 or newer, failing on any HTTP error.
fetch() {
    curl --proto '=https' --tlsv1.2 -sSfL -o "$2" "$1"
}

# PT: Confere o sha256 do arquivo $1 contra o valor publicado $2 e o fixado $3; aborta se diferir.
# EN: Checks the sha256 of file $1 against the published value $2 and the pinned $3; aborts on mismatch.
verify() {
    local actual
    actual="$(sha256sum "$1" | cut -d' ' -f1)"
    if [[ -z "$2" || "$actual" != "$2" || "$actual" != "$3" ]]; then
        echo "ERRO/ERROR: sha256 de $(basename "$1") não confere / does not match" >&2
        echo "  publicado/published: ${2:-<vazio/empty>}" >&2
        echo "  fixado/pinned:       $3" >&2
        echo "  obtido/actual:       $actual" >&2
        exit 1
    fi
    echo "ok  $actual  $(basename "$1")"
}

# PT: Procura o hash de um arquivo dentro de uma lista de checksums no formato "hash  nome".
# EN: Looks up a file's hash inside a checksum list in the "hash  name" format.
hash_from_list() {
    awk -v name="$2" '$2 == name || $2 == "*"name { print $1 }' "$1"
}

mkdir -p "$OPT_DIR" "$BIN_DIR"

# --- gh (GitHub CLI) ---
gh_file="gh_${GH_VERSION}_linux_amd64.tar.gz"
gh_base="https://github.com/cli/cli/releases/download/v${GH_VERSION}"
fetch "$gh_base/$gh_file" "$WORK_DIR/$gh_file"
fetch "$gh_base/gh_${GH_VERSION}_checksums.txt" "$WORK_DIR/gh.sums"
verify "$WORK_DIR/$gh_file" "$(hash_from_list "$WORK_DIR/gh.sums" "$gh_file")" "$GH_SHA256"
rm -rf "$OPT_DIR/gh_${GH_VERSION}_linux_amd64"
tar -xzf "$WORK_DIR/$gh_file" -C "$OPT_DIR"
ln -sfn "$OPT_DIR/gh_${GH_VERSION}_linux_amd64/bin/gh" "$BIN_DIR/gh"

# --- binaryen (wasm-opt) ---
bin_file="binaryen-version_${BINARYEN_VERSION}-x86_64-linux.tar.gz"
bin_base="https://github.com/WebAssembly/binaryen/releases/download/version_${BINARYEN_VERSION}"
fetch "$bin_base/$bin_file" "$WORK_DIR/$bin_file"
fetch "$bin_base/$bin_file.sha256" "$WORK_DIR/binaryen.sums"
verify "$WORK_DIR/$bin_file" "$(cut -d' ' -f1 "$WORK_DIR/binaryen.sums")" "$BINARYEN_SHA256"
rm -rf "$OPT_DIR/binaryen-version_${BINARYEN_VERSION}"
tar -xzf "$WORK_DIR/$bin_file" -C "$OPT_DIR"
ln -sfn "$OPT_DIR/binaryen-version_${BINARYEN_VERSION}/bin/wasm-opt" "$BIN_DIR/wasm-opt"

# --- Node.js ---
node_dir="node-v${NODE_VERSION}-linux-x64"
node_file="$node_dir.tar.xz"
node_base="https://nodejs.org/dist/v${NODE_VERSION}"
fetch "$node_base/$node_file" "$WORK_DIR/$node_file"
fetch "$node_base/SHASUMS256.txt" "$WORK_DIR/node.sums"
verify "$WORK_DIR/$node_file" "$(hash_from_list "$WORK_DIR/node.sums" "$node_file")" "$NODE_SHA256"
rm -rf "$OPT_DIR/$node_dir"
tar -xJf "$WORK_DIR/$node_file" -C "$OPT_DIR"
for tool in node npm npx; do
    ln -sfn "$OPT_DIR/$node_dir/bin/$tool" "$BIN_DIR/$tool"
done

echo "Ferramentas instaladas em / Tools installed in $OPT_DIR (links em / links in $BIN_DIR)."
