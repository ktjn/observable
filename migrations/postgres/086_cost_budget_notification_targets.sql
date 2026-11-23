-- ============================================================================
-- Migration 086: Budget owner notification URLs (observable-control, W2b)
--
-- Budget overrun warnings (OBS-17-02) were sent to webhook URLs taken from the
-- evaluate request body. The destinations are now part of the budget, set by a
-- tenant admin and checked by the outbound policy at save and at every send.
-- Breaking change, no data carried over (2.0 decision): existing budgets are
-- removed because they have no approved destination.
-- ============================================================================

DELETE FROM control.cost_budget_alerts;
DELETE FROM control.cost_budgets;

ALTER TABLE control.cost_budgets
    ADD COLUMN primary_url TEXT NOT NULL,
    ADD COLUMN backup_url  TEXT NOT NULL,
    ADD COLUMN updated_by  UUID;

ALTER TABLE control.service_cost_attribution
    ADD COLUMN recorded_by UUID;

ALTER TABLE control.cost_budget_alerts
    ADD COLUMN last_error   TEXT,
    ADD COLUMN requested_by UUID;

-- Migration 080 created these tables after 041's one-time grant, so the scoped
-- observable_control login could not use them (the old tests ran as the owner).
GRANT SELECT, INSERT, UPDATE, DELETE
    ON control.service_cost_attribution, control.cost_budgets, control.cost_budget_alerts
    TO observable_control_role;
