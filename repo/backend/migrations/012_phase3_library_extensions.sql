-- ============================================================================
-- Migration 012: Phase 3 — Library serials / resources versioning + attachments
-- ============================================================================
-- Extends the Phase 1 library tables with the workflow semantics Phase 3
-- needs:
--
--   * journal_versions / resource_versions gain a `state` column
--     (draft -> approved -> published -> archived) and approval/publish
--     metadata so historical versions carry their own audit trail.
--   * journals / teaching_resources gain `latest_version_id` so callers
--     can distinguish "the head draft pointer" from "the published
--     baseline" (`current_version_id`).
--   * attachments gain `sha256_checksum`, `original_filename`, `category`,
--     `stored_filename`, and `is_deleted` — everything the upload service
--     needs to store files safely on disk while keeping rich metadata
--     in MySQL.
-- ============================================================================

-- ── journal_versions ──────────────────────────────────────────────────────
ALTER TABLE journal_versions
    ADD COLUMN state ENUM('draft','approved','published','archived')
        NOT NULL DEFAULT 'draft' AFTER body,
    ADD COLUMN approved_by  CHAR(36) DEFAULT NULL AFTER created_at,
    ADD COLUMN approved_at  DATETIME DEFAULT NULL AFTER approved_by,
    ADD COLUMN published_by CHAR(36) DEFAULT NULL AFTER approved_at,
    ADD COLUMN published_at DATETIME DEFAULT NULL AFTER published_by,
    ADD CONSTRAINT fk_jv_approved_by FOREIGN KEY (approved_by)
        REFERENCES users (id) ON DELETE SET NULL ON UPDATE CASCADE,
    ADD CONSTRAINT fk_jv_published_by FOREIGN KEY (published_by)
        REFERENCES users (id) ON DELETE SET NULL ON UPDATE CASCADE,
    ADD INDEX idx_jv_state (state);

-- ── journals ──────────────────────────────────────────────────────────────
ALTER TABLE journals
    ADD COLUMN latest_version_id CHAR(36) DEFAULT NULL AFTER current_version_id,
    ADD CONSTRAINT fk_journals_latest_version FOREIGN KEY (latest_version_id)
        REFERENCES journal_versions (id) ON DELETE SET NULL ON UPDATE CASCADE;

-- ── resource_versions ─────────────────────────────────────────────────────
ALTER TABLE resource_versions
    ADD COLUMN state ENUM('draft','approved','published','archived')
        NOT NULL DEFAULT 'draft' AFTER description,
    ADD COLUMN approved_by  CHAR(36) DEFAULT NULL AFTER created_at,
    ADD COLUMN approved_at  DATETIME DEFAULT NULL AFTER approved_by,
    ADD COLUMN published_by CHAR(36) DEFAULT NULL AFTER approved_at,
    ADD COLUMN published_at DATETIME DEFAULT NULL AFTER published_by,
    ADD CONSTRAINT fk_rv_approved_by FOREIGN KEY (approved_by)
        REFERENCES users (id) ON DELETE SET NULL ON UPDATE CASCADE,
    ADD CONSTRAINT fk_rv_published_by FOREIGN KEY (published_by)
        REFERENCES users (id) ON DELETE SET NULL ON UPDATE CASCADE,
    ADD INDEX idx_rv_state (state);

-- ── teaching_resources ────────────────────────────────────────────────────
ALTER TABLE teaching_resources
    ADD COLUMN latest_version_id CHAR(36) DEFAULT NULL AFTER current_version_id,
    ADD CONSTRAINT fk_teaching_resources_latest_version FOREIGN KEY (latest_version_id)
        REFERENCES resource_versions (id) ON DELETE SET NULL ON UPDATE CASCADE;

-- ── attachments ───────────────────────────────────────────────────────────
ALTER TABLE attachments
    ADD COLUMN sha256_checksum CHAR(64)     DEFAULT NULL AFTER size_bytes,
    ADD COLUMN original_filename VARCHAR(500) DEFAULT NULL AFTER sha256_checksum,
    ADD COLUMN stored_filename   VARCHAR(255) DEFAULT NULL AFTER original_filename,
    ADD COLUMN category          VARCHAR(50)  DEFAULT NULL AFTER stored_filename,
    ADD COLUMN is_deleted        BOOLEAN      NOT NULL DEFAULT FALSE AFTER category,
    ADD INDEX idx_attachments_category (category),
    ADD INDEX idx_attachments_is_deleted (is_deleted);
