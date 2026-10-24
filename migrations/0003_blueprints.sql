-- PT: Blueprints salvas e as análises de cada uma. A chave composta (id, user_id) deixa o próprio
--     banco impedir que um usuário ligue dados seus aos de outro.
-- EN: Saved blueprints and their analyses. The composite key (id, user_id) lets the database
--     itself prevent a user from linking their data to someone else's.
CREATE TABLE blueprints (
    id uuid PRIMARY KEY,
    user_id uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    title text NOT NULL CHECK (char_length(title) BETWEEN 1 AND 200),
    description text CHECK (description IS NULL OR char_length(description) <= 2000),
    source text NOT NULL CHECK (octet_length(source) <= 8388608),
    content_sha256 bytea NOT NULL CHECK (octet_length(content_sha256) = 32),
    is_book boolean NOT NULL,
    blueprint_count integer NOT NULL CHECK (blueprint_count >= 0),
    entity_count integer NOT NULL CHECK (entity_count >= 0),
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (user_id, content_sha256),
    UNIQUE (id, user_id)
);
CREATE INDEX blueprints_user_created_idx ON blueprints (user_id, created_at DESC, id DESC);

CREATE TABLE blueprint_analyses (
    id uuid PRIMARY KEY,
    blueprint_id uuid NOT NULL,
    user_id uuid NOT NULL,
    selected integer NOT NULL CHECK (selected >= 0),
    options_sha256 bytea NOT NULL CHECK (octet_length(options_sha256) = 32),
    options jsonb NOT NULL CHECK (pg_column_size(options) <= 65536),
    summary jsonb NOT NULL CHECK (pg_column_size(summary) <= 262144),
    data_version text NOT NULL CHECK (char_length(data_version) <= 32),
    analyzer_version text NOT NULL CHECK (char_length(analyzer_version) <= 32),
    created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (blueprint_id, selected, options_sha256),
    FOREIGN KEY (blueprint_id, user_id) REFERENCES blueprints (id, user_id) ON DELETE CASCADE
);
CREATE INDEX blueprint_analyses_blueprint_idx ON blueprint_analyses (blueprint_id, created_at DESC);
