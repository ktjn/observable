-- Add nullable preset column
ALTER TABLE dashboard_panels ADD COLUMN IF NOT EXISTS preset TEXT;

-- Backfill: snap lookback_minutes to nearest preset string
DO $$
BEGIN
  IF EXISTS (
    SELECT 1
    FROM information_schema.columns
    WHERE table_name = 'dashboard_panels'
      AND column_name = 'lookback_minutes'
  ) THEN
    UPDATE dashboard_panels SET preset = CASE
      WHEN lookback_minutes <=   5 THEN '5m'
      WHEN lookback_minutes <=  15 THEN '15m'
      WHEN lookback_minutes <=  30 THEN '30m'
      WHEN lookback_minutes <=  60 THEN '1h'
      WHEN lookback_minutes <= 180 THEN '3h'
      ELSE '12h'
    END;
  END IF;
END $$;

-- Drop the old column
ALTER TABLE dashboard_panels DROP COLUMN IF EXISTS lookback_minutes;
