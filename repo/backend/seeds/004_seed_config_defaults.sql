-- ============================================================================
-- Seed 004: Default Retention Policies
-- ============================================================================
-- Seeds the two baseline retention policies that every deployment requires:
--   * audit_logs  — 365 days, anonymize (7-year target set by seed 008)
--   * sessions    — 30 days, delete
--
-- Uses INSERT IGNORE for idempotent re-runs.
-- Schema is owned by migration 010_create_retention_policies.sql;
-- this seed must run AFTER that migration.
-- ============================================================================

INSERT IGNORE INTO retention_policies
    (id, target_entity_type, retention_days, action, rationale, is_active,
     created_by, created_at, updated_at)
VALUES
    ('50000000-0000-0000-0000-000000000001',
     'audit_logs', 365, 'anonymize',
     'Regulatory: audit trail anonymised after retention window (corrected to 2555 days by seed 008)',
     1, NULL, NOW(), NOW()),

    ('50000000-0000-0000-0000-000000000002',
     'sessions', 30, 'delete',
     'Security hygiene: revoked/expired sessions purged after 30 days',
     1, NULL, NOW(), NOW());
