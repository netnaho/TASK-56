-- ============================================================================
-- Migration 010: Retention Policies
-- ============================================================================
-- Creates the data retention policy table for managing lifecycle rules
-- (delete, anonymize, archive, flag for review) across entity types.
-- ============================================================================

CREATE TABLE retention_policies (
    id                  CHAR(36)     NOT NULL,
    target_entity_type  VARCHAR(100) NOT NULL,
    retention_days      INT          NOT NULL,
    action              ENUM('delete','anonymize','archive','flag_for_review') NOT NULL,
    rationale           TEXT         DEFAULT NULL,
    is_active           BOOLEAN      NOT NULL DEFAULT TRUE,
    created_by          CHAR(36)     DEFAULT NULL,
    last_executed_at    DATETIME     DEFAULT NULL,
    created_at          DATETIME     NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at          DATETIME     NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
    PRIMARY KEY (id),
    UNIQUE KEY uq_retention_policies_entity_type (target_entity_type),
    CONSTRAINT fk_retention_policies_created_by FOREIGN KEY (created_by) REFERENCES users (id) ON DELETE SET NULL ON UPDATE CASCADE,
    INDEX idx_retention_policies_active (is_active)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;
