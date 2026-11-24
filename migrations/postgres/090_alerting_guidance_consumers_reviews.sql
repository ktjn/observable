-- ============================================================================
-- Migration 090: guidance consumers and reviews (observable-alerting, W4b,
-- docs/operational-guidance-graph.md, OBS-39-04, 05, 15..20)
--
--   guidance_consumers   agent API keys a tenant admin approved, with their purposes
--   guidance_reviews     append-only decisions on inferred relationships; keyed by the
--                        relationship (subject, relation, object) so a later interval of the
--                        same inference stays under the decision
-- ============================================================================

CREATE TABLE IF NOT EXISTS alerting.guidance_consumers (
    tenant_id         UUID        NOT NULL,
    api_key_id        UUID        NOT NULL,
    approved_purposes TEXT[]      NOT NULL DEFAULT '{}'
                          CHECK (approved_purposes <@ ARRAY['incident_response', 'dependency_analysis']::TEXT[]),
    approved_by       UUID        NOT NULL,
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, api_key_id)
);

CREATE TABLE IF NOT EXISTS alerting.guidance_reviews (
    review_id            UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id            UUID        NOT NULL,
    assertion_id         UUID        NOT NULL REFERENCES alerting.guidance_assertions(assertion_id) ON DELETE CASCADE,
    subject_id           UUID        NOT NULL REFERENCES alerting.guidance_entities(entity_id) ON DELETE CASCADE,
    relation             TEXT        NOT NULL,
    object_id            UUID        NOT NULL REFERENCES alerting.guidance_entities(entity_id) ON DELETE CASCADE,
    reviewer             UUID        NOT NULL,
    decision             TEXT        NOT NULL CHECK (decision IN ('reject', 'confirm')),
    reason               TEXT        NOT NULL CHECK (length(btrim(reason)) BETWEEN 1 AND 1000),
    evidence_fingerprint TEXT        NOT NULL,
    decided_at           TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS guidance_reviews_relationship_idx
    ON alerting.guidance_reviews (tenant_id, subject_id, relation, object_id, decided_at);

-- Decisions are history: no update (OBS-39-19).
CREATE OR REPLACE FUNCTION alerting.guidance_reviews_immutable() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'guidance reviews are append-only'
        USING ERRCODE = 'integrity_constraint_violation';
END $$;

DROP TRIGGER IF EXISTS guidance_reviews_immutable ON alerting.guidance_reviews;
CREATE TRIGGER guidance_reviews_immutable
    BEFORE UPDATE ON alerting.guidance_reviews
    FOR EACH ROW EXECUTE FUNCTION alerting.guidance_reviews_immutable();

-- Latest evidence behind each open inferred assertion. Mutable by design: it is the input
-- the review state compares against, not history (the assertion row stays unchanged).
CREATE TABLE IF NOT EXISTS alerting.guidance_inference_state (
    assertion_id         UUID        PRIMARY KEY REFERENCES alerting.guidance_assertions(assertion_id) ON DELETE CASCADE,
    tenant_id            UUID        NOT NULL,
    evidence_fingerprint TEXT        NOT NULL,
    last_seen            TIMESTAMPTZ NOT NULL,
    evidence             JSONB       NOT NULL DEFAULT '{}',
    updated_at           TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Deletes only through tenant purge, which runs as the same login.
GRANT SELECT, INSERT, UPDATE, DELETE
    ON alerting.guidance_consumers, alerting.guidance_reviews, alerting.guidance_inference_state
    TO observable_alerting_role;
