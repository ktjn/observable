-- ============================================================================
-- Migration 089: operational guidance ledger (observable-alerting, W4a,
-- docs/operational-guidance-graph.md, OBS-39)
--
--   guidance_entities     stable identity per tenant, kind and canonical source key
--   guidance_assertions   append-only relationships with source, confidence, validity;
--                         only valid_to may be set, once, from null
--   guidance_source_runs  one row per pull of a source, for coverage and as-of bounds
-- ============================================================================

CREATE TABLE IF NOT EXISTS alerting.guidance_entities (
    entity_id     UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id     UUID        NOT NULL,
    kind          TEXT        NOT NULL CHECK (kind IN
                      ('service', 'unregistered_service', 'owner', 'journey', 'provider', 'change', 'incident')),
    canonical_key TEXT        NOT NULL CHECK (length(canonical_key) BETWEEN 1 AND 512),
    display_name  TEXT        NOT NULL DEFAULT '',
    first_seen_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (tenant_id, kind, canonical_key)
);

CREATE TABLE IF NOT EXISTS alerting.guidance_assertions (
    assertion_id         UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id            UUID        NOT NULL,
    subject_id           UUID        NOT NULL REFERENCES alerting.guidance_entities(entity_id) ON DELETE CASCADE,
    relation             TEXT        NOT NULL CHECK (relation IN
                             ('owned_by', 'part_of_journey', 'provided_by', 'changed_by', 'affected', 'calls')),
    object_id            UUID        NOT NULL REFERENCES alerting.guidance_entities(entity_id) ON DELETE CASCADE,
    -- Qualifier within the relation, e.g. the owner role or the provider function.
    qualifier            TEXT        NOT NULL DEFAULT '',
    source               TEXT        NOT NULL CHECK (source IN ('catalog', 'catalog_authoritative', 'control', 'alerting', 'telemetry')),
    source_ref           TEXT        NOT NULL CHECK (length(source_ref) BETWEEN 1 AND 512),
    confidence           TEXT        NOT NULL CHECK (confidence IN ('declared', 'recorded', 'inferred')),
    valid_from           TIMESTAMPTZ NOT NULL,
    -- 'source' when the source gave the time, 'observed_at_sync' when the first sync saw it.
    valid_from_basis     TEXT        NOT NULL CHECK (valid_from_basis IN ('source', 'observed_at_sync')),
    valid_to             TIMESTAMPTZ,
    recorded_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    evidence             JSONB       NOT NULL DEFAULT '{}',
    evidence_fingerprint TEXT        NOT NULL DEFAULT '',
    CHECK (valid_to IS NULL OR valid_to >= valid_from)
);

CREATE INDEX IF NOT EXISTS guidance_assertions_subject_idx
    ON alerting.guidance_assertions (tenant_id, subject_id, relation);
CREATE INDEX IF NOT EXISTS guidance_assertions_object_idx
    ON alerting.guidance_assertions (tenant_id, object_id, relation);
-- At most one open assertion per source claim.
CREATE UNIQUE INDEX IF NOT EXISTS guidance_assertions_open_uq
    ON alerting.guidance_assertions (tenant_id, subject_id, relation, object_id, qualifier, source)
    WHERE valid_to IS NULL;

-- History is immutable: only valid_to may change, once, from null (OBS-39-13).
CREATE OR REPLACE FUNCTION alerting.guidance_assertions_immutable() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    IF OLD.valid_to IS NOT NULL
       OR NEW.valid_to IS NULL
       OR (NEW.assertion_id, NEW.tenant_id, NEW.subject_id, NEW.relation, NEW.object_id,
           NEW.qualifier, NEW.source, NEW.source_ref, NEW.confidence, NEW.valid_from,
           NEW.valid_from_basis, NEW.recorded_at, NEW.evidence, NEW.evidence_fingerprint)
          IS DISTINCT FROM
          (OLD.assertion_id, OLD.tenant_id, OLD.subject_id, OLD.relation, OLD.object_id,
           OLD.qualifier, OLD.source, OLD.source_ref, OLD.confidence, OLD.valid_from,
           OLD.valid_from_basis, OLD.recorded_at, OLD.evidence, OLD.evidence_fingerprint)
    THEN
        RAISE EXCEPTION 'guidance assertions are append-only; only an open valid_to may be closed'
            USING ERRCODE = 'integrity_constraint_violation';
    END IF;
    RETURN NEW;
END $$;

DROP TRIGGER IF EXISTS guidance_assertions_immutable ON alerting.guidance_assertions;
CREATE TRIGGER guidance_assertions_immutable
    BEFORE UPDATE ON alerting.guidance_assertions
    FOR EACH ROW EXECUTE FUNCTION alerting.guidance_assertions_immutable();

CREATE TABLE IF NOT EXISTS alerting.guidance_source_runs (
    run_id       UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id    UUID        NOT NULL,
    source       TEXT        NOT NULL CHECK (source IN ('catalog', 'control', 'alerting', 'telemetry')),
    started_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    completed_at TIMESTAMPTZ,
    complete     BOOLEAN     NOT NULL DEFAULT false,
    error        TEXT
);

CREATE INDEX IF NOT EXISTS guidance_source_runs_tenant_idx
    ON alerting.guidance_source_runs (tenant_id, source, started_at);

-- The alerting login may read, add and close; deletes happen only in tenant purge, which
-- runs as the same login, so DELETE stays granted.
GRANT SELECT, INSERT, UPDATE, DELETE
    ON alerting.guidance_entities, alerting.guidance_assertions, alerting.guidance_source_runs
    TO observable_alerting_role;
