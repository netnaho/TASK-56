-- ============================================================================
-- Migration 011: Admin Settings (key-value store)
-- ============================================================================
-- Stores admin-managed configuration: approved SSIDs for check-in, the
-- check-in duplicate window, retention defaults, attachment upload constraints,
-- and report schedule scaffolding values. All values are JSON-encoded so they
-- can carry scalars, objects, or arrays without per-setting table churn.
-- Every mutation is routed through /api/v1/admin/config and written to the
-- audit log with a chained hash.
-- ============================================================================

CREATE TABLE admin_settings (
    setting_key     VARCHAR(255) NOT NULL,
    setting_value   JSON         NOT NULL,
    description     TEXT         DEFAULT NULL,
    updated_by      CHAR(36)     DEFAULT NULL,
    updated_at      DATETIME     NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
    created_at      DATETIME     NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (setting_key),
    CONSTRAINT fk_admin_settings_updated_by FOREIGN KEY (updated_by) REFERENCES users (id) ON DELETE SET NULL ON UPDATE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;
