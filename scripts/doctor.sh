#!/usr/bin/env bash
# PT: Confere se o ambiente de desenvolvimento está completo e mostra a versão de cada
#     ferramenta. Não instala nada; só aponta o que falta e como resolver.
# EN: Checks that the development environment is complete and prints each tool's
#     version. It installs nothing; it only points out what is missing and how to fix it.
set -uo pipefail

# PT: Garante que as ferramentas instaladas no espaço do usuário estejam no PATH.
# EN: Makes sure the tools installed in user space are on the PATH.
export PATH="$HOME/.cargo/bin:$HOME/.local/bin:$PATH"

# shellcheck source=factorio-version.sh
source "$(dirname "$0")/factorio-version.sh"
REQUIRED_FACTORIO="$FACTORIO_VERSION"
failures=0

ok()   { printf '  \033[32mOK\033[0m    %-18s %s\n' "$1" "$2"; }
fail() { printf '  \033[31mFALHA\033[0m %-18s %s\n' "$1" "$2"; failures=$((failures + 1)); }

# PT: Roda o comando de versão e marca OK ou FALHA com a dica de correção.
# EN: Runs the version command and marks OK or FAIL with the fix hint.
check() {
    local name="$1" hint="$2"
    shift 2
    local out
    if out="$("$@" 2>/dev/null | head -n 1)" && [[ -n "$out" ]]; then
        ok "$name" "$out"
    else
        fail "$name" "$hint"
    fi
}

echo "Ferramentas / Tools"
check rustc        "rode / run: scripts/install-tools.sh"                      rustc --version
check cargo        "rode / run: rustup-init"                                   cargo --version
check clippy       "rode / run: rustup component add clippy"                    cargo clippy --version
check rustfmt      "rode / run: rustup component add rustfmt"                   rustfmt --version
check sqlx-cli     "rode / run: cargo install sqlx-cli --locked --no-default-features --features postgres,rustls" sqlx --version
check wasm-bindgen "rode / run: cargo install wasm-bindgen-cli --locked"        wasm-bindgen --version
check wasm-opt     "rode / run: scripts/install-tools.sh"                       wasm-opt --version
check node         "rode / run: scripts/install-tools.sh"                       node --version
check npm          "rode / run: scripts/install-tools.sh"                       npm --version
check gh           "rode / run: scripts/install-tools.sh"                       gh --version

if rustup target list --installed 2>/dev/null | grep -qx wasm32-unknown-unknown; then
    ok "target wasm32" "wasm32-unknown-unknown"
else
    fail "target wasm32" "rode / run: rustup target add wasm32-unknown-unknown"
fi

if gh auth status >/dev/null 2>&1; then
    ok "gh login" "$(gh api user --jq .login 2>/dev/null)"
else
    fail "gh login" "rode / run: gh auth login"
fi

echo
echo "Docker e Postgres / Docker and Postgres"
if docker info --format '{{.ServerVersion}}' >/dev/null 2>&1; then
    ok docker "$(docker info --format 'servidor / server {{.ServerVersion}}')"
    # PT: O serviço db só existe a partir da Fase 1; antes disso é só um aviso.
    # EN: The db service only exists from Phase 1 on; before that it is just a warning.
    root="$(cd "$(dirname "$0")/.." && pwd)"
    if [[ -f "$root/docker-compose.yml" ]]; then
        if docker compose -f "$root/docker-compose.yml" exec -T db pg_isready -q >/dev/null 2>&1; then
            ok postgres "aceitando conexões / accepting connections"
        else
            fail postgres "rode / run: docker compose up -d db"
        fi
    else
        printf '  --    %-18s %s\n' postgres "docker-compose.yml ainda não existe (Fase 1) / does not exist yet (Phase 1)"
    fi
else
    fail docker "rode / run: systemctl --user start docker-desktop"
fi

echo
echo "Factorio"
# PT: Procura o jogo oficial; FACTORIO_PATH tem prioridade sobre os caminhos da Steam.
# EN: Looks for the official game; FACTORIO_PATH takes priority over the Steam paths.
factorio_bin=""
for dir in "${FACTORIO_PATH:-}" \
           "$HOME/.local/share/Steam/steamapps/common/Factorio" \
           "$HOME/.steam/steam/steamapps/common/Factorio"; do
    if [[ -n "$dir" && -x "$dir/bin/x64/factorio" ]]; then
        factorio_bin="$dir/bin/x64/factorio"
        break
    fi
done

if [[ -z "$factorio_bin" ]]; then
    fail factorio "não encontrado; defina / not found; set FACTORIO_PATH"
else
    version="$("$factorio_bin" --version 2>/dev/null | head -n 1)"
    if [[ "$version" == "Version: $REQUIRED_FACTORIO "* ]]; then
        ok factorio "$version"
    else
        fail factorio "esperado / expected $REQUIRED_FACTORIO, obtido / got: ${version:-nada / nothing}"
    fi
fi

echo
if (( failures == 0 )); then
    echo "Tudo pronto. / All set."
else
    echo "$failures item(ns) com falha. / $failures item(s) failing."
    exit 1
fi
