-- ============================================================================
-- Migration 003: Teaching Resources and Resource Versions
-- ============================================================================
-- Creates tables for managing teaching resources (documents, videos,
-- presentations, etc.) with version tracking.
-- ============================================================================

CREATE TABLE teaching_resources (
    id                 CHAR(36)     NOT NULL,
    owner_id           CHAR(36)     DEFAULT NULL,
    title              VARCHAR(500) DEFAULT NULL,
    resource_type      ENUM('document','video','presentation','assessment','external_link','dataset','other') NOT NULL DEFAULT 'document',
    tags               JSON         DEFAULT NULL,
    is_published       BOOLEAN      NOT NULL DEFAULT FALSE,
    current_version_id CHAR(36)     DEFAULT NULL,
    created_at         DATETIME     NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at         DATETIME     NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
    PRIMARY KEY (id),
    CONSTRAINT fk_teaching_resources_owner FOREIGN KEY (owner_id) REFERENCES users (id) ON DELETE SET NULL ON UPDATE CASCADE,
    INDEX idx_teaching_resources_owner (owner_id),
    INDEX idx_teaching_resources_type (resource_type),
    INDEX idx_teaching_resources_published (is_published)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE resource_versions (
    id              CHAR(36)     NOT NULL,
    resource_id     CHAR(36)     NOT NULL,
    version_number  INT          NOT NULL,
    content_url     TEXT         DEFAULT NULL,
    mime_type       VARCHAR(255) DEFAULT NULL,
    size_bytes      BIGINT       DEFAULT NULL,
    description     TEXT         DEFAULT NULL,
    change_summary  TEXT         DEFAULT NULL,
    created_by      CHAR(36)     DEFAULT NULL,
    created_at      DATETIME     NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (id),
    UNIQUE KEY uq_resource_versions_resource_version (resource_id, version_number),
    CONSTRAINT fk_resource_versions_resource FOREIGN KEY (resource_id) REFERENCES teaching_resources (id) ON DELETE CASCADE ON UPDATE CASCADE,
    CONSTRAINT fk_resource_versions_created_by FOREIGN KEY (created_by) REFERENCES users (id) ON DELETE SET NULL ON UPDATE CASCADE,
    INDEX idx_resource_versions_created_by (created_by)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

-- Add the deferred foreign key for current_version_id
ALTER TABLE teaching_resources
    ADD CONSTRAINT fk_teaching_resources_current_version FOREIGN KEY (current_version_id) REFERENCES resource_versions (id) ON DELETE SET NULL ON UPDATE CASCADE;
