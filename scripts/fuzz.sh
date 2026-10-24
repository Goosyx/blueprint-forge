#!/usr/bin/env bash
# PT: Roda cada alvo de fuzzing por N segundos (padrão 60) com limite de memória de 512 MB.
#     Uso: scripts/fuzz.sh [segundos]. Requer o Rust nightly e o cargo-fuzz.
# EN: Runs each fuzzing target for N seconds (default 60) with a 512 MB memory limit.
#     Usage: scripts/fuzz.sh [seconds]. Requires Rust nightly and cargo-fuzz.
set -euo pipefail

export PATH="$HOME/.cargo/bin:$PATH"
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root/fuzz"
seconds="${1:-60}"
# shellcheck source=factorio-version.sh
source "$root/scripts/factorio-version.sh"
export BF_GAME_ASSETS="${BF_GAME_ASSETS:-$root/game-assets/$FACTORIO_VERSION}"

mkdir -p corpus/decode corpus/analyze
cp -n "$root/crates/bf-blueprint/tests/fixtures/"*.txt corpus/decode/ 2>/dev/null || true

for target in decode analyze; do
    echo "==> $target (${seconds}s)"
    cargo +nightly fuzz run "$target" "corpus/$target" -- \
        -max_total_time="$seconds" -rss_limit_mb=512 -max_len=65536 -dict=dict/blueprint.dict
done
