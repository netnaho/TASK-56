-- Migration 018: Normalize report format columns to supported values
--
-- Background
-- ----------
-- Migration 008 defined three format columns as:
--   ENUM('pdf','csv','excel','html','json')
--
-- The application domain (domain/report.rs) only supports two output formats:
--   * csv   — text/csv
--   * xlsx  — application/vnd.openxmlformats-officedocument.spreadsheetml.sheet
--
-- The value 'xlsx' was never present in the DB enum, so any INSERT or MODIFY
-- using it would fail with a MySQL "Data truncated for column" error.
--
-- Data-migration policy
-- ---------------------
--   excel  → xlsx  (direct rename; same semantic)
--   pdf    → csv   (no PDF renderer; graceful fallback already in from_db())
--   html   → csv   (no HTML renderer; graceful fallback already in from_db())
--   json   → csv   (no JSON format renderer; graceful fallback already in from_db())
--
-- The three-step approach below is required because MySQL cannot ALTER a column
-- to an enum that does not contain all values currently stored in the column.
-- We therefore:
--   1. Widen the enum to include 'xlsx' alongside the legacy values.
--   2. Migrate the data.
--   3. Narrow the enum to the final ('csv','xlsx') set.
--
-- Idempotency: the UPDATEs are no-ops when re-run (already-migrated values
-- are not in the old set). The ALTERs are idempotent because MySQL silently
-- re-applies a MODIFY COLUMN when the definition is already correct.

-- ── Step 1: widen enums to include 'xlsx' (needed before data migration) ──────

ALTER TABLE reports
    MODIFY COLUMN default_format
        ENUM('pdf','csv','excel','html','json','xlsx') NOT NULL DEFAULT 'csv';

ALTER TABLE report_runs
    MODIFY COLUMN format
        ENUM('pdf','csv','excel','html','json','xlsx') DEFAULT NULL;

ALTER TABLE report_schedules
    MODIFY COLUMN format
        ENUM('pdf','csv','excel','html','json','xlsx') DEFAULT NULL;

-- ── Step 2: normalise legacy data ─────────────────────────────────────────────

-- excel → xlsx (same semantics, renamed to match the standard file extension)
UPDATE reports        SET default_format = 'xlsx' WHERE default_format = 'excel';
UPDATE report_runs    SET format         = 'xlsx' WHERE format         = 'excel';
UPDATE report_schedules SET format       = 'xlsx' WHERE format         = 'excel';

-- pdf / html / json → csv (no renderers; align with domain's from_db() fallback)
UPDATE reports        SET default_format = 'csv'  WHERE default_format IN ('pdf','html','json');
UPDATE report_runs    SET format         = 'csv'  WHERE format         IN ('pdf','html','json');
UPDATE report_schedules SET format       = 'csv'  WHERE format         IN ('pdf','html','json');

-- Normalise the global admin-settings value if it stored a legacy format name.
UPDATE admin_settings SET value = 'xlsx' WHERE name = 'reports.default_format' AND value = 'excel';
UPDATE admin_settings SET value = 'csv'  WHERE name = 'reports.default_format' AND value IN ('pdf','html','json');

-- ── Step 3: narrow enums to only the supported values ─────────────────────────

ALTER TABLE reports
    MODIFY COLUMN default_format
        ENUM('csv','xlsx') NOT NULL DEFAULT 'csv';

ALTER TABLE report_runs
    MODIFY COLUMN format
        ENUM('csv','xlsx') DEFAULT NULL;

ALTER TABLE report_schedules
    MODIFY COLUMN format
        ENUM('csv','xlsx') DEFAULT NULL;
