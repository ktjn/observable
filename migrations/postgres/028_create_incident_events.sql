CREATE TABLE IF NOT EXISTS incident_events (
    event_id    UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    incident_id UUID        NOT NULL REFERENCES incidents(incident_id) ON DELETE CASCADE,
    event_time  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    event_type  TEXT        NOT NULL CHECK (event_type IN ('triggered', 'acknowledged', 'comment', 'status_change', 'deployment_linked', 'alert_fired', 'alert_resolved')),
    actor       TEXT        NOT NULL DEFAULT 'system',
    message     TEXT
);

CREATE INDEX IF NOT EXISTS incident_events_incident_time_idx ON incident_events (incident_id, event_time);
