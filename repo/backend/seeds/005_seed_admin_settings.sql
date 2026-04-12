-- ============================================================================
-- Seed 005: Admin Configuration Defaults
-- ============================================================================
-- Seeds the admin_settings key-value table with the defaults that later
-- domain features depend on. Values are JSON-encoded. Every row is
-- idempotent via INSERT IGNORE so re-running the entrypoint is safe.
-- ============================================================================

-- Approved SSID names for the "check-in via local wifi" policy.
-- Empty by default; an admin must set this before check-in is permitted.
INSERT IGNORE INTO admin_settings (setting_key, setting_value, description) VALUES
    ('network.approved_ssids',
     JSON_ARRAY(),
     'List of approved Wi-Fi SSID names that count as "on-campus" for check-in purposes.');

-- Check-in duplicate suppression window (in minutes).
-- Default 10 minutes as required.
INSERT IGNORE INTO admin_settings (setting_key, setting_value, description) VALUES
    ('checkin.duplicate_window_minutes',
     CAST('10' AS JSON),
     'A check-in from the same user for the same section within this many minutes is considered a duplicate and rejected.');

-- Retention default for audit log rows (7 years, expressed in days).
INSERT IGNORE INTO admin_settings (setting_key, setting_value, description) VALUES
    ('retention.audit_log_days',
     CAST('2555' AS JSON),
     'Default retention period (days) for audit log entries. Default 7 years.');

-- Retention default for operational events (3 years, expressed in days).
INSERT IGNORE INTO admin_settings (setting_key, setting_value, description) VALUES
    ('retention.operational_events_days',
     CAST('1095' AS JSON),
     'Default retention period (days) for non-audit operational events. Default 3 years.');

-- Attachment upload constraints.
INSERT IGNORE INTO admin_settings (setting_key, setting_value, description) VALUES
    ('attachments.max_bytes',
     CAST('52428800' AS JSON),
     'Maximum attachment size in bytes (default 50 MiB).'),
    ('attachments.allowed_mime_types',
     JSON_ARRAY(
        'application/pdf',
        'image/png',
        'image/jpeg',
        'text/plain',
        'text/csv',
        'application/vnd.openxmlformats-officedocument.wordprocessingml.document',
        'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet',
        'application/vnd.openxmlformats-officedocument.presentationml.presentation'
     ),
     'Whitelist of MIME types accepted by the attachment upload endpoint.');

-- Report schedule scaffold: list of named schedules. Empty until Phase 3.
INSERT IGNORE INTO admin_settings (setting_key, setting_value, description) VALUES
    ('reports.schedules',
     JSON_ARRAY(),
     'Scheduled report definitions. Populated by the reports module in Phase 3.');

-- Lockout policy defaults (5 failed logins in 15 minutes).
-- These values are authoritative defaults; AppConfig environment variables
-- can override them at process start.
INSERT IGNORE INTO admin_settings (setting_key, setting_value, description) VALUES
    ('auth.max_failed_logins',
     CAST('5' AS JSON),
     'Maximum failed login attempts before an account is locked.'),
    ('auth.lockout_minutes',
     CAST('15' AS JSON),
     'Duration (minutes) an account remains locked after exceeding the failed-login threshold.'),
    ('auth.min_password_length',
     CAST('12' AS JSON),
     'Minimum password length enforced at set/change time.');
