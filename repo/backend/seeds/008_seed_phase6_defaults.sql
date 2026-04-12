-- Phase 6 seed: default report definitions, retention policy corrections,
-- and encryption/scheduler admin settings.
--
-- All inserts use INSERT IGNORE so this seed is idempotent.

-- ─── Retention policy corrections ────────────────────────────────────────────
-- The Phase 1 seed (004) set audit_logs retention to 365 days; correct to
-- 2555 days (7 years) and add an operational_events policy at 1095 days (3 years).

UPDATE retention_policies
   SET retention_days = 2555,
       rationale      = 'Legal/compliance: 7-year minimum for institutional audit trails',
       updated_at     = NOW()
 WHERE target_entity_type = 'audit_logs';

UPDATE retention_policies
   SET retention_days = 30,
       rationale      = 'Security hygiene: revoked/expired sessions purged after 30 days',
       updated_at     = NOW()
 WHERE target_entity_type = 'sessions';

-- Add missing operational_events policy (check-in activity data)
INSERT IGNORE INTO retention_policies
    (id, target_entity_type, retention_days, action, rationale, is_active, created_by, created_at, updated_at)
VALUES
    (UUID(), 'operational_events', 1095, 'delete',
     'Operational: check-in and engagement data retained for 3 years', 1, NULL, NOW(), NOW()),
    (UUID(), 'report_runs', 365, 'delete',
     'Storage hygiene: generated report artifacts removed after 1 year', 1, NULL, NOW(), NOW());

-- ─── Admin settings: Phase 6 additions ───────────────────────────────────────

-- Scheduler tick interval in seconds (60 = check every minute)
INSERT IGNORE INTO admin_settings (setting_key, setting_value, description, updated_at, created_at)
VALUES
    ('reports.scheduler_tick_seconds', '60',
     'How often the background scheduler polls for due report schedules (seconds)',
     NOW(), NOW()),

    ('reports.max_artifact_age_days', '365',
     'Maximum age in days for generated report artifact files before they are eligible for deletion',
     NOW(), NOW()),

    ('reports.default_format', '"csv"',
     'Default output format for new report definitions (csv or xlsx)',
     NOW(), NOW()),

    ('retention.dry_run_default', 'false',
     'If true, retention execution defaults to dry-run mode (logs counts without deleting)',
     NOW(), NOW()),

    ('encryption.field_encryption_enabled', 'true',
     'Whether AES-256-GCM field-level encryption is active for sensitive columns',
     NOW(), NOW());
