-- Add nullable preset column
ALTER TABLE dashboard_panels ADD COLUMN preset TEXT;

-- Backfill: snap lookback_minutes to nearest preset string
UPDATE dashboard_panels SET preset = CASE
  WHEN lookback_minutes <=   5 THEN '5m'
  WHEN lookback_minutes <=  15 THEN '15m'
  WHEN lookback_minutes <=  30 THEN '30m'
  WHEN lookback_minutes <=  60 THEN '1h'
  WHEN lookback_minutes <= 180 THEN '3h'
  ELSE '12h'
END;

-- Drop the old column
ALTER TABLE dashboard_panels DROP COLUMN lookback_minutes;
