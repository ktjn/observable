CREATE INDEX alert_firings_suppressed_by_idx
    ON alert_firings(suppressed_by_firing_id)
    WHERE suppressed_by_firing_id IS NOT NULL;
