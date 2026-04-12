-- ============================================================================
-- Migration 008: Reports, Report Runs, and Report Schedules
-- ============================================================================
-- Creates tables for report definitions, execution tracking, and scheduled
-- report generation with notification support.
-- ============================================================================

CREATE TABLE reports (
    id               CHAR(36)     NOT NULL,
    title            VARCHAR(500) NOT NULL,
    description      TEXT         DEFAULT NULL,
    query_definition JSON         NOT NULL,
    default_format   ENUM('pdf','csv','excel','html','json') NOT NULL DEFAULT 'pdf',
    created_by       CHAR(36)     DEFAULT NULL,
    created_at       DATETIME     NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at       DATETIME     NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
    PRIMARY KEY (id),
    CONSTRAINT fk_reports_created_by FOREIGN KEY (created_by) REFERENCES users (id) ON DELETE SET NULL ON UPDATE CASCADE,
    INDEX idx_reports_created_by (created_by)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE report_runs (
    id              CHAR(36)     NOT NULL,
    report_id       CHAR(36)     NOT NULL,
    status          ENUM('queued','running','completed','failed','cancelled') NOT NULL DEFAULT 'queued',
    parameters_json JSON         DEFAULT NULL,
    format          ENUM('pdf','csv','excel','html','json') DEFAULT NULL,
    artifact_path   TEXT         DEFAULT NULL,
    error_message   TEXT         DEFAULT NULL,
    started_at      DATETIME     DEFAULT NULL,
    completed_at    DATETIME     DEFAULT NULL,
    triggered_by    CHAR(36)     DEFAULT NULL,
    created_at      DATETIME     NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (id),
    CONSTRAINT fk_report_runs_report FOREIGN KEY (report_id) REFERENCES reports (id) ON DELETE CASCADE ON UPDATE CASCADE,
    CONSTRAINT fk_report_runs_triggered_by FOREIGN KEY (triggered_by) REFERENCES users (id) ON DELETE SET NULL ON UPDATE CASCADE,
    INDEX idx_report_runs_report (report_id),
    INDEX idx_report_runs_status (status),
    INDEX idx_report_runs_triggered_by (triggered_by)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE report_schedules (
    id                CHAR(36)     NOT NULL,
    report_id         CHAR(36)     NOT NULL,
    cron_expression   VARCHAR(100) NOT NULL,
    is_active         BOOLEAN      NOT NULL DEFAULT TRUE,
    parameters_json   JSON         DEFAULT NULL,
    format            ENUM('pdf','csv','excel','html','json') DEFAULT NULL,
    notify_recipients JSON         DEFAULT NULL,
    last_run_at       DATETIME     DEFAULT NULL,
    next_run_at       DATETIME     DEFAULT NULL,
    created_by        CHAR(36)     DEFAULT NULL,
    created_at        DATETIME     NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at        DATETIME     NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
    PRIMARY KEY (id),
    CONSTRAINT fk_report_schedules_report FOREIGN KEY (report_id) REFERENCES reports (id) ON DELETE CASCADE ON UPDATE CASCADE,
    CONSTRAINT fk_report_schedules_created_by FOREIGN KEY (created_by) REFERENCES users (id) ON DELETE SET NULL ON UPDATE CASCADE,
    INDEX idx_report_schedules_report (report_id),
    INDEX idx_report_schedules_active (is_active),
    INDEX idx_report_schedules_next_run (next_run_at)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;
