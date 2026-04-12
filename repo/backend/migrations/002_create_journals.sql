-- ============================================================================
-- Migration 002: Journals and Journal Versions
-- ============================================================================
-- Creates tables for academic journal management with version tracking.
-- ============================================================================

CREATE TABLE journals (
    id                 CHAR(36)     NOT NULL,
    title              VARCHAR(500) NOT NULL,
    author_id          CHAR(36)     DEFAULT NULL,
    abstract_text      TEXT         DEFAULT NULL,
    is_published       BOOLEAN      NOT NULL DEFAULT FALSE,
    current_version_id CHAR(36)     DEFAULT NULL,
    created_at         DATETIME     NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at         DATETIME     NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
    PRIMARY KEY (id),
    CONSTRAINT fk_journals_author FOREIGN KEY (author_id) REFERENCES users (id) ON DELETE SET NULL ON UPDATE CASCADE,
    INDEX idx_journals_author (author_id),
    INDEX idx_journals_published (is_published)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE journal_versions (
    id              CHAR(36)     NOT NULL,
    journal_id      CHAR(36)     NOT NULL,
    version_number  INT          NOT NULL,
    title           VARCHAR(500) DEFAULT NULL,
    body            LONGTEXT     DEFAULT NULL,
    change_summary  TEXT         DEFAULT NULL,
    created_by      CHAR(36)     DEFAULT NULL,
    created_at      DATETIME     NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (id),
    UNIQUE KEY uq_journal_versions_journal_version (journal_id, version_number),
    CONSTRAINT fk_journal_versions_journal FOREIGN KEY (journal_id) REFERENCES journals (id) ON DELETE CASCADE ON UPDATE CASCADE,
    CONSTRAINT fk_journal_versions_created_by FOREIGN KEY (created_by) REFERENCES users (id) ON DELETE SET NULL ON UPDATE CASCADE,
    INDEX idx_journal_versions_created_by (created_by)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

-- Add the deferred foreign key for current_version_id
ALTER TABLE journals
    ADD CONSTRAINT fk_journals_current_version FOREIGN KEY (current_version_id) REFERENCES journal_versions (id) ON DELETE SET NULL ON UPDATE CASCADE;
