#!/usr/bin/env bash
# PT: Baixa o servidor headless oficial do Factorio (gratuito) na versão fixada e confere o
#     sha256 contra o valor fixado aqui e contra a lista publicada pela Wube.
#     Instala em ~/.local/opt/factorio-headless-<versão>, com write-data próprio (não usa ~/.factorio).
# EN: Downloads the official Factorio headless server (free) at the pinned version and checks
#     the sha256 against the value pinned here and against the list published by Wube.
#     Installs into ~/.local/opt/factorio-headless-<version>, with its own write-data (no ~/.factorio).
set -euo pipefail

# shellcheck source=factorio-version.sh
source "$(dirname "$0")/factorio-version.sh"
VERSION="$FACTORIO_VERSION"
FILE="factorio-headless_linux_${VERSION}.tar.xz"
# PT: Valor fixado na primeira verificação (23/10/2026). EN: Value pinned on first verification (2026-10-23).
PINNED_SHA256="4633250f7747676279fb210a1717ba34caed6b9890c951fb3e469eefe43c46b9"

DEST="$HOME/.local/opt/factorio-headless-${VERSION}"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

if [[ -x "$DEST/bin/x64/factorio" ]]; then
    echo "já instalado / already installed: $DEST"
    exit 0
fi
if [[ -e "$DEST" ]]; then
    echo "ERRO/ERROR: $DEST existe mas está incompleto; apague-o e rode de novo / exists but is incomplete; delete it and run again" >&2
    exit 1
fi

curl --proto '=https' --tlsv1.2 -sSfL -o "$WORK/$FILE" "https://www.factorio.com/get-download/${VERSION}/headless/linux64"
curl --proto '=https' --tlsv1.2 -sSfL -o "$WORK/sums.txt" "https://www.factorio.com/download/sha256sums/"

published="$(awk -v f="$FILE" '$2 == f { print $1 }' "$WORK/sums.txt")"
actual="$(sha256sum "$WORK/$FILE" | cut -d' ' -f1)"
if [[ "$actual" != "$PINNED_SHA256" || "$actual" != "$published" ]]; then
    echo "ERRO/ERROR: sha256 não confere / does not match" >&2
    echo "  fixado/pinned:      $PINNED_SHA256" >&2
    echo "  publicado/published: ${published:-<ausente/missing>}" >&2
    echo "  obtido/actual:      $actual" >&2
    exit 1
fi
echo "ok  $actual  $FILE"

mkdir -p "$WORK/extract"
tar -xJf "$WORK/$FILE" -C "$WORK/extract"
mkdir -p "$(dirname "$DEST")"
mv "$WORK/extract/factorio" "$DEST"
"$DEST/bin/x64/factorio" --version | head -n 1
