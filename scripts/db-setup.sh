#!/usr/bin/env bash
# PT: Prepara o banco com privilégio mínimo:
#       - bf_migrate: dono das tabelas, roda as migrations (sem superusuário);
#       - bf_app: a aplicação, só lê e escreve dados, com limites de tempo;
#     o superusuário do contêiner só é usado aqui (pelo psql do contêiner) e nos testes.
#     Lê as senhas do .env sem executá-lo como shell.
# EN: Prepares the database with least privilege:
#       - bf_migrate: owns the tables, runs the migrations (no superuser);
#       - bf_app: the application, only reads and writes data, with time limits;
#     the container superuser is only used here (through the container's psql) and in tests.
#     Reads passwords from .env without running it as shell code.
set -euo pipefail

export PATH="$HOME/.cargo/bin:$HOME/.local/bin:$PATH"
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

while IFS='=' read -r key value; do
    [[ "$key" =~ ^[A-Z_][A-Z0-9_]*$ ]] && export "$key=$value"
done < .env

if [[ ! "${BF_DB_NAME:-}" =~ ^[a-z_][a-z0-9_]*$ ]]; then
    echo "BF_DB_NAME precisa ser minúsculo, dígitos ou _ / must be lowercase, digits or _" >&2
    exit 1
fi
for name in BF_APP_DB_PASSWORD BF_MIGRATE_DB_PASSWORD; do
    value="${!name:-}"
    if [[ ! "$value" =~ ^[A-Za-z0-9]{16,}$ ]]; then
        echo "$name precisa ter 16+ letras ou dígitos / needs 16+ letters or digits" >&2
        exit 1
    fi
done

psql() {
    docker compose exec -T db psql -v ON_ERROR_STOP=1 -U "$BF_DB_USER" -d "$BF_DB_NAME" "$@"
}

psql <<SQL
DO \$\$
BEGIN
    IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = 'bf_migrate') THEN
        CREATE ROLE bf_migrate LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE PASSWORD '${BF_MIGRATE_DB_PASSWORD}';
    ELSE
        ALTER ROLE bf_migrate LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE PASSWORD '${BF_MIGRATE_DB_PASSWORD}';
    END IF;
    IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = 'bf_app') THEN
        CREATE ROLE bf_app LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE PASSWORD '${BF_APP_DB_PASSWORD}';
    ELSE
        ALTER ROLE bf_app LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE PASSWORD '${BF_APP_DB_PASSWORD}';
    END IF;
END
\$\$;
ALTER ROLE bf_app SET statement_timeout = '5s';
ALTER ROLE bf_app SET idle_in_transaction_session_timeout = '10s';
ALTER ROLE bf_app CONNECTION LIMIT 20;
REVOKE CREATE ON SCHEMA public FROM PUBLIC;
REVOKE TEMPORARY ON DATABASE ${BF_DB_NAME} FROM PUBLIC;
GRANT USAGE, CREATE ON SCHEMA public TO bf_migrate;
GRANT USAGE ON SCHEMA public TO bf_app;
-- PT: Tabelas criadas antes (pelo superusuário) passam a ser do bf_migrate.
-- EN: Tables created earlier (by the superuser) now belong to bf_migrate.
DO \$\$
DECLARE item record;
BEGIN
    FOR item IN SELECT tablename FROM pg_tables WHERE schemaname = 'public' LOOP
        EXECUTE format('ALTER TABLE public.%I OWNER TO bf_migrate', item.tablename);
    END LOOP;
END
\$\$;
ALTER DEFAULT PRIVILEGES FOR ROLE bf_migrate IN SCHEMA public
    GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO bf_app;
SQL

DATABASE_URL="$BF_MIGRATE_DATABASE_URL" sqlx migrate run --source migrations

psql <<SQL
GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO bf_app;
REVOKE ALL ON _sqlx_migrations FROM bf_app;
SQL
echo "banco pronto / database ready"
