-- PT: Preferências do usuário. EN: User preferences.
CREATE TABLE user_preferences (
    user_id uuid PRIMARY KEY REFERENCES users (id) ON DELETE CASCADE,
    rate_unit text NOT NULL DEFAULT 'minute' CHECK (rate_unit IN ('second', 'minute', 'hour')),
    analysis_defaults jsonb NOT NULL DEFAULT '{}'::jsonb
        CHECK (pg_column_size(analysis_defaults) <= 16384),
    generator_defaults jsonb NOT NULL DEFAULT '{}'::jsonb
        CHECK (pg_column_size(generator_defaults) <= 16384),
    updated_at timestamptz NOT NULL DEFAULT now()
);
