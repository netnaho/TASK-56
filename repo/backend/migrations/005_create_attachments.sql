-- ============================================================================
-- Migration 005: Attachments
-- ============================================================================
-- Creates a polymorphic attachments table that can associate files with any
-- entity type (journals, resources, courses, etc.) via entity_type/entity_id.
-- ============================================================================

CREATE TABLE attachments (
    id              CHAR(36)     NOT NULL,
    entity_type     VARCHAR(100) NOT NULL,
    entity_id       CHAR(36)     NOT NULL,
    file_name       VARCHAR(500) NOT NULL,
    file_path       TEXT         NOT NULL,
    mime_type       VARCHAR(255) DEFAULT NULL,
    size_bytes      BIGINT       DEFAULT NULL,
    uploaded_by     CHAR(36)     DEFAULT NULL,
    created_at      DATETIME     NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (id),
    CONSTRAINT fk_attachments_uploaded_by FOREIGN KEY (uploaded_by) REFERENCES users (id) ON DELETE SET NULL ON UPDATE CASCADE,
    INDEX idx_attachments_entity (entity_type, entity_id),
    INDEX idx_attachments_uploaded_by (uploaded_by)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;
