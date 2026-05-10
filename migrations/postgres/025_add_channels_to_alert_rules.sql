ALTER TABLE alert_rules ADD COLUMN IF NOT EXISTS notification_channels UUID[] NOT NULL DEFAULT '{}';
