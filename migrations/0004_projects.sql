-- PT: Projetos de fábrica e as blueprints de cada um.
-- EN: Factory projects and the blueprints of each one.
CREATE TABLE projects (
    id uuid PRIMARY KEY,
    user_id uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    name text NOT NULL CHECK (char_length(name) BETWEEN 1 AND 120),
    description text CHECK (description IS NULL OR char_length(description) <= 2000),
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (id, user_id)
);
CREATE UNIQUE INDEX projects_user_name_key ON projects (user_id, lower(name));

CREATE TABLE project_blueprints (
    project_id uuid NOT NULL,
    blueprint_id uuid NOT NULL,
    user_id uuid NOT NULL,
    position integer NOT NULL DEFAULT 0 CHECK (position >= 0),
    note text CHECK (note IS NULL OR char_length(note) <= 500),
    PRIMARY KEY (project_id, blueprint_id),
    FOREIGN KEY (project_id, user_id) REFERENCES projects (id, user_id) ON DELETE CASCADE,
    FOREIGN KEY (blueprint_id, user_id) REFERENCES blueprints (id, user_id) ON DELETE CASCADE
);
CREATE INDEX project_blueprints_user_idx ON project_blueprints (user_id);
