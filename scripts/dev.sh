#!/usr/bin/env bash
# PT: Atalhos de desenvolvimento.
#     setup  instala o hook de pre-push que roda o CI local.
#     serve  compila WASM, web e servidor e sobe em http://localhost:8080.
#     create-user <email>  cria uma conta (a senha é lida da entrada padrão).
# EN: Development shortcuts.
#     setup  installs the pre-push hook that runs the local CI.
#     serve  builds WASM, web and server and starts them at http://localhost:8080.
#     create-user <email>  creates an account (the password is read from standard input).
set -euo pipefail

export PATH="$HOME/.cargo/bin:$HOME/.local/bin:$PATH"
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

# PT: Lê o .env como dados (CHAVE=valor), sem executá-lo como shell.
# EN: Reads .env as data (KEY=value), without running it as shell code.
load_env() {
    if [[ -f .env ]]; then
        while IFS='=' read -r key value; do
            [[ "$key" =~ ^[A-Z_][A-Z0-9_]*$ ]] && export "$key=$value"
        done < .env
    fi
}

case "${1:-}" in
    setup)
        printf '#!/usr/bin/env bash\nexec "%s/scripts/ci.sh"\n' "$root" > .git/hooks/pre-push
        chmod +x .git/hooks/pre-push
        echo "hook de pre-push instalado / pre-push hook installed"
        ;;
    serve)
        scripts/build-wasm.sh
        (cd web && npm run build)
        cargo build --release -p bf-server
        load_env
        exec target/release/bf-server
        ;;
    create-user)
        [[ -n "${2:-}" ]] || { echo "uso / usage: scripts/dev.sh create-user <email>" >&2; exit 2; }
        cargo build --release -p bf-server
        load_env
        exec target/release/bf-server create-user "$2"
        ;;
    *)
        echo "uso / usage: scripts/dev.sh setup|serve|create-user <email>" >&2
        exit 2
        ;;
esac
