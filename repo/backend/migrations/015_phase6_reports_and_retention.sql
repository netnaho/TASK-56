-- Phase 6: Extend report and retention tables.
--
-- Adds:
--   * artifact_size_bytes to report_runs (track generated file size)
--   * department_scope_id to report_schedules (scope scheduled runs to a department)
--   * triggered_source to report_runs (distinguish manual vs scheduled)
--   * last_execution_result to retention_policies (JSON summary of last run)
--
-- Each ALTER TABLE is a single operation.
-- Note: ADD COLUMN IF NOT EXISTS is MariaDB syntax and NOT supported by MySQL 8.0.
-- Use plain ADD COLUMN; idempotency is handled by the migration runner.

-- report_runs: add artifact size tracking
ALTER TABLE report_runs
    ADD COLUMN artifact_size_bytes BIGINT NULL
        COMMENT 'Size in bytes of the generated artifact file' AFTER artifact_path;

ALTER TABLE report_runs
    ADD COLUMN triggered_source ENUM('manual','scheduled') NOT NULL DEFAULT 'manual'
        COMMENT 'Whether the run was triggered manually or by the scheduler' AFTER triggered_by;

-- report_schedules: add department scope
ALTER TABLE report_schedules
    ADD COLUMN department_scope_id CHAR(36) NULL
        COMMENT 'If set, report is scoped to this department; NULL means all departments' AFTER cron_expression;

CREATE INDEX idx_report_schedules_dept ON report_schedules (department_scope_id);

-- retention_policies: add last execution result for observability
ALTER TABLE retention_policies
    ADD COLUMN last_execution_result JSON NULL
        COMMENT 'JSON summary of the last retention enforcement run' AFTER last_executed_at;
