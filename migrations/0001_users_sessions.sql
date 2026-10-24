-- PT: Usuários, sessões, convites e uso de cota. Todos os textos têm limite também no banco,
--     como segunda barreira caso a aplicação falhe.
-- EN: Users, sessions, invites and quota usage. All texts are also limited in the database,
--     as a second barrier in case the application fails.

CREATE TABLE users (
    id uuid PRIMARY KEY,
    email text NOT NULL CHECK (char_length(email) BETWEEN 3 AND 254),
    password_hash text NOT NULL CHECK (char_length(password_hash) <= 512),
    display_name text CHECK (display_name IS NULL OR char_length(display_name) <= 64),
    locale text NOT NULL DEFAULT 'pt-BR' CHECK (locale IN ('pt-BR', 'en')),
    failed_logins integer NOT NULL DEFAULT 0 CHECK (failed_logins >= 0),
    locked_until timestamptz,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX users_email_key ON users (lower(email));

-- PT: Só o SHA-256 do token fica no banco; o token em si só existe no cookie.
-- EN: Only the token's SHA-256 is stored; the token itself only exists in the cookie.
CREATE TABLE sessions (
    token_hash bytea PRIMARY KEY CHECK (octet_length(token_hash) = 32),
    user_id uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    created_at timestamptz NOT NULL DEFAULT now(),
    expires_at timestamptz NOT NULL,
    last_seen_at timestamptz NOT NULL DEFAULT now(),
    user_agent text CHECK (user_agent IS NULL OR char_length(user_agent) <= 256)
);
CREATE INDEX sessions_user_idx ON sessions (user_id);
CREATE INDEX sessions_expires_idx ON sessions (expires_at);

CREATE TABLE invites (
    token_hash bytea PRIMARY KEY CHECK (octet_length(token_hash) = 32),
    created_by uuid REFERENCES users (id) ON DELETE SET NULL,
    expires_at timestamptz NOT NULL,
    used_by uuid REFERENCES users (id) ON DELETE SET NULL,
    used_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now()
);

-- PT: Uso de cota por usuário, atualizado na mesma transação de cada gravação.
-- EN: Per-user quota usage, updated in the same transaction as each write.
CREATE TABLE user_usage (
    user_id uuid PRIMARY KEY REFERENCES users (id) ON DELETE CASCADE,
    blueprints integer NOT NULL DEFAULT 0 CHECK (blueprints >= 0),
    projects integer NOT NULL DEFAULT 0 CHECK (projects >= 0),
    generations integer NOT NULL DEFAULT 0 CHECK (generations >= 0),
    bytes bigint NOT NULL DEFAULT 0 CHECK (bytes >= 0),
    updated_at timestamptz NOT NULL DEFAULT now()
);
