#!/usr/bin/env bash
# PT: Compila o bf-wasm, gera os bindings JS com wasm-bindgen e otimiza o .wasm com wasm-opt.
#     A saída vai para web/src/lib/wasm/pkg (gerada, fora do git).
# EN: Builds bf-wasm, generates the JS bindings with wasm-bindgen and optimizes the .wasm with
#     wasm-opt. Output goes to web/src/lib/wasm/pkg (generated, not in git).
set -euo pipefail

export PATH="$HOME/.cargo/bin:$HOME/.local/bin:$PATH"
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

out="web/src/lib/wasm/pkg"
wasm="target/wasm32-unknown-unknown/release-wasm/bf_wasm.wasm"

cargo build -p bf-wasm --target wasm32-unknown-unknown --profile release-wasm
rm -rf "$out"
wasm-bindgen --target web --out-dir "$out" "$wasm"

# PT: Recursos do WebAssembly que o Rust já usa por padrão no target wasm32.
# EN: WebAssembly features Rust already uses by default on the wasm32 target.
wasm-opt -Oz \
    --enable-bulk-memory \
    --enable-nontrapping-float-to-int \
    --enable-sign-ext \
    --enable-mutable-globals \
    --enable-reference-types \
    --enable-multivalue \
    "$out/bf_wasm_bg.wasm" -o "$out/bf_wasm_bg.wasm"

ls -l "$out/bf_wasm_bg.wasm" | awk '{ print "bf_wasm_bg.wasm: " $5 " bytes" }'
