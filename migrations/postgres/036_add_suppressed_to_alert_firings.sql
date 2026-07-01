ALTER TABLE alert_firings DROP CONSTRAINT alert_firings_state_check;
ALTER TABLE alert_firings ADD CONSTRAINT alert_firings_state_check
    CHECK (state IN ('pending', 'active', 'resolved', 'suppressed'));

ALTER TABLE alert_firings
    ADD COLUMN suppressed_by_firing_id UUID REFERENCES alert_firings(firing_id);
