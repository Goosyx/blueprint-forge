#!/usr/bin/env bash
# PT: CI local. Roda formatação, lint, testes e builds em ordem e para
#     no primeiro erro. As etapas da web só rodam quando a pasta web/ existe.
# EN: Local CI. Runs formatting, lint, tests and builds in order and
#     stops at the first error. Web steps only run when the web/ folder exists.
set -euo pipefail

export PATH="$HOME/.cargo/bin:$HOME/.local/bin:$PATH"
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
# shellcheck source=factorio-version.sh
source scripts/factorio-version.sh

# PT: No CI local os dados do jogo são obrigatórios; sem eles os testes que dependem
#     do jogo falham em vez de serem pulados.
# EN: In local CI the game data is mandatory; without it the tests that depend on the
#     game fail instead of being skipped.
export BF_REQUIRE_GAME_ASSETS="${BF_REQUIRE_GAME_ASSETS:-1}"
export BF_GAME_ASSETS="${BF_GAME_ASSETS:-$root/game-assets/$FACTORIO_VERSION}"

step() { printf '\n\033[1m==> %s\033[0m\n' "$1"; }

# PT: Banco: as migrations rodam antes dos testes; os testes de banco criam bancos temporários
#     com o usuário dono. Sem banco, compila com os metadados offline do sqlx (.sqlx/).
# EN: Database: migrations run before the tests; database tests create temporary databases with
#     the owner user. Without a database, it compiles with the sqlx offline metadata (.sqlx/).
if [[ -f .env ]]; then
    while IFS='=' read -r key value; do
        [[ "$key" =~ ^[A-Z_][A-Z0-9_]*$ ]] && export "$key=$value"
    done < .env
fi
if docker compose exec -T db pg_isready -q >/dev/null 2>&1; then
    step "banco / database (migrations)"
    scripts/db-setup.sh
    # PT: Os testes criam bancos temporários, o que exige o superusuário do contêiner.
    # EN: Tests create temporary databases, which requires the container superuser.
    export DATABASE_URL="$BF_ADMIN_DATABASE_URL"
else
    echo "Postgres desligado: usando SQLX_OFFLINE / Postgres down: using SQLX_OFFLINE"
    export SQLX_OFFLINE=true
fi

step "cargo fmt"
cargo fmt --all --check

step "cargo clippy"
cargo clippy --workspace --all-targets -- -D warnings

step "cargo test"
cargo test --workspace

if [[ "${SQLX_OFFLINE:-}" != "true" ]]; then
    step "sqlx prepare --check"
    cargo sqlx prepare --workspace --check -- --all-targets
fi

step "dependências / dependencies (cargo deny, cargo audit)"
if command -v cargo-deny >/dev/null && command -v cargo-audit >/dev/null; then
    cargo deny check advisories bans licenses sources
    cargo audit --deny warnings
else
    echo "cargo-deny/cargo-audit ausentes / missing: cargo install --locked cargo-deny cargo-audit"
    exit 1
fi

step "wasm32 build"
scripts/build-wasm.sh

if [[ -f web/package.json ]]; then
    step "web: npm ci"
    (cd web && npm ci --ignore-scripts --no-fund)
    step "web: audit, check, lint, test, build"
    (cd web && npm audit --audit-level=low && npm run check && npm run lint && npm test && npm run build)
fi

step "validação no jogo / in-game validation"
headless="$HOME/.local/opt/factorio-headless-$FACTORIO_VERSION/bin/x64/factorio"
if [[ -x "$headless" ]]; then
    cargo build --release -p bf-validate
    for scenario in validation/scenarios/*.json; do
        target/release/bf-validate run "$scenario" >/dev/null
        echo "ok  $scenario"
    done
elif [[ "$BF_REQUIRE_GAME_ASSETS" == "1" ]]; then
    echo "headless ausente; rode scripts/fetch-headless.sh / headless missing; run scripts/fetch-headless.sh"
    exit 1
fi

printf '\n\033[32mCI ok\033[0m\n'
