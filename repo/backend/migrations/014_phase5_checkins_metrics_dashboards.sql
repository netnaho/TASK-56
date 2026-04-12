-- ============================================================================
-- Migration 014: Phase 5 — Check-ins, metric semantic layer, dashboards
-- ============================================================================
-- 1. Reworks `checkin_events` for Phase 5's behaviour:
--    * Drops the per-day uniqueness constraint so the service layer can
--      enforce a configurable time window and allow exactly one retry.
--    * Adds `retry_of_id`, `retry_sequence`, `retry_reason`,
--      `device_fingerprint`, `network_hint`, `network_verified`,
--      `client_ip`, `is_duplicate_attempt`.
--
-- 2. Creates `checkin_retry_reasons` — the controlled list of reasons
--    that must accompany every retry attempt. Seed rows live in
--    `seeds/006_seed_checkin_retry_reasons.sql`.
--
-- 3. Promotes `metric_definition_versions` to a full workflow model
--    with state, approver, and `lineage_refs` (an array of
--    `{definition_id, version_id}` pointers to input metric versions).
--    Adds `metric_type` (base | derived) and `window_seconds` for
--    metrics whose calculation has a natural time window.
--    Adds `latest_version_id` to `metric_definitions`.
--
-- 4. Adds `verification_needed` and `based_on_version_id` to
--    `dashboard_widgets` so that admin edits to a metric definition
--    can mark every dependent chart for re-review.
-- ============================================================================

-- ── checkin_events rework ─────────────────────────────────────────────────
ALTER TABLE checkin_events
    DROP INDEX uq_checkin_events_user_section_date,
    ADD COLUMN retry_of_id           CHAR(36)    DEFAULT NULL AFTER device_info,
    ADD COLUMN retry_sequence        INT         NOT NULL DEFAULT 0 AFTER retry_of_id,
    ADD COLUMN retry_reason          VARCHAR(80) DEFAULT NULL AFTER retry_sequence,
    ADD COLUMN device_fingerprint    JSON        DEFAULT NULL AFTER retry_reason,
    ADD COLUMN network_hint          VARCHAR(255) DEFAULT NULL AFTER device_fingerprint,
    ADD COLUMN network_verified      BOOLEAN     NOT NULL DEFAULT FALSE AFTER network_hint,
    ADD COLUMN client_ip             VARCHAR(45) DEFAULT NULL AFTER network_verified,
    ADD COLUMN is_duplicate_attempt  BOOLEAN     NOT NULL DEFAULT FALSE AFTER client_ip,
    ADD CONSTRAINT fk_checkin_retry_of FOREIGN KEY (retry_of_id)
        REFERENCES checkin_events (id) ON DELETE SET NULL ON UPDATE CASCADE,
    ADD INDEX idx_checkin_retry_of (retry_of_id),
    ADD INDEX idx_checkin_user_section (user_id, section_id),
    ADD INDEX idx_checkin_is_duplicate (is_duplicate_attempt);

-- ── checkin_retry_reasons (controlled list) ───────────────────────────────
CREATE TABLE checkin_retry_reasons (
    reason_code  VARCHAR(80) NOT NULL,
    display_name VARCHAR(255) NOT NULL,
    description  TEXT DEFAULT NULL,
    is_active    BOOLEAN NOT NULL DEFAULT TRUE,
    created_at   DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (reason_code)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

-- ── metric definition version workflow ───────────────────────────────────
ALTER TABLE metric_definition_versions
    ADD COLUMN metric_type    ENUM('base','derived') NOT NULL DEFAULT 'base' AFTER formula,
    ADD COLUMN window_seconds INT DEFAULT NULL AFTER metric_type,
    ADD COLUMN lineage_refs   JSON DEFAULT NULL AFTER window_seconds,
    ADD COLUMN state          ENUM('draft','approved','published','archived')
        NOT NULL DEFAULT 'draft' AFTER change_summary,
    ADD COLUMN approved_by    CHAR(36) DEFAULT NULL AFTER created_at,
    ADD COLUMN approved_at    DATETIME DEFAULT NULL AFTER approved_by,
    ADD COLUMN published_by   CHAR(36) DEFAULT NULL AFTER approved_at,
    ADD COLUMN published_at   DATETIME DEFAULT NULL AFTER published_by,
    ADD CONSTRAINT fk_mdv_approved_by FOREIGN KEY (approved_by)
        REFERENCES users (id) ON DELETE SET NULL ON UPDATE CASCADE,
    ADD CONSTRAINT fk_mdv_published_by FOREIGN KEY (published_by)
        REFERENCES users (id) ON DELETE SET NULL ON UPDATE CASCADE,
    ADD INDEX idx_mdv_state (state);

ALTER TABLE metric_definitions
    ADD COLUMN latest_version_id CHAR(36) DEFAULT NULL AFTER current_version_id,
    ADD CONSTRAINT fk_metric_definitions_latest_version FOREIGN KEY (latest_version_id)
        REFERENCES metric_definition_versions (id) ON DELETE SET NULL ON UPDATE CASCADE;

-- ── dashboard_widgets verification ───────────────────────────────────────
ALTER TABLE dashboard_widgets
    ADD COLUMN based_on_version_id   CHAR(36) DEFAULT NULL AFTER metric_definition_id,
    ADD COLUMN verification_needed   BOOLEAN  NOT NULL DEFAULT FALSE AFTER based_on_version_id,
    ADD COLUMN verified_by           CHAR(36) DEFAULT NULL AFTER verification_needed,
    ADD COLUMN verified_at           DATETIME DEFAULT NULL AFTER verified_by,
    ADD CONSTRAINT fk_widget_based_on_version FOREIGN KEY (based_on_version_id)
        REFERENCES metric_definition_versions (id) ON DELETE SET NULL ON UPDATE CASCADE,
    ADD CONSTRAINT fk_widget_verified_by FOREIGN KEY (verified_by)
        REFERENCES users (id) ON DELETE SET NULL ON UPDATE CASCADE,
    ADD INDEX idx_dashboard_widgets_verification_needed (verification_needed);
