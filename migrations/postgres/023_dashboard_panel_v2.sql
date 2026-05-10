ALTER TABLE dashboard_panels
    ADD COLUMN IF NOT EXISTS panel_kind TEXT NOT NULL DEFAULT 'query'
        CHECK (panel_kind IN ('query', 'text')),
    ALTER COLUMN query_kind DROP NOT NULL;

ALTER TABLE dashboard_panels
    ADD COLUMN IF NOT EXISTS query_text TEXT,
    ADD COLUMN IF NOT EXISTS content TEXT,
    ADD COLUMN IF NOT EXISTS layout JSONB NOT NULL DEFAULT '{"x":0,"y":0,"w":6,"h":4}'::jsonb,
    ADD COLUMN IF NOT EXISTS time_range JSONB NOT NULL DEFAULT '{"mode":"global"}'::jsonb;
