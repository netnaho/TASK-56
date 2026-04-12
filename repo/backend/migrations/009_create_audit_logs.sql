-- ============================================================================
-- Migration 009: Audit Logs and Audit Hash Chain
-- ============================================================================
-- Creates the audit logging infrastructure with a tamper-evident hash chain
-- for integrity verification. The actor_id is nullable to support system-
-- generated events.
-- ============================================================================

CREATE TABLE audit_logs (
    id                  CHAR(36)     NOT NULL,
    actor_id            CHAR(36)     DEFAULT NULL,
    actor_email         VARCHAR(255) DEFAULT NULL,
    action              VARCHAR(255) NOT NULL,
    target_entity_type  VARCHAR(100) DEFAULT NULL,
    target_entity_id    CHAR(36)     DEFAULT NULL,
    change_payload      JSON         DEFAULT NULL,
    ip_address          VARCHAR(45)  DEFAULT NULL,
    user_agent          TEXT         DEFAULT NULL,
    created_at          DATETIME     NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (id),
    INDEX idx_audit_logs_actor (actor_id),
    INDEX idx_audit_logs_target (target_entity_type, target_entity_id),
    INDEX idx_audit_logs_created_at (created_at),
    INDEX idx_audit_logs_action (action)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE audit_hash_chain (
    id              CHAR(36)     NOT NULL,
    audit_log_id    CHAR(36)     NOT NULL,
    sequence_number BIGINT       NOT NULL,
    previous_hash   VARCHAR(128) DEFAULT NULL,
    current_hash    VARCHAR(128) NOT NULL,
    created_at      DATETIME     NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (id),
    UNIQUE KEY uq_audit_hash_chain_log (audit_log_id),
    UNIQUE KEY uq_audit_hash_chain_sequence (sequence_number),
    CONSTRAINT fk_audit_hash_chain_log FOREIGN KEY (audit_log_id) REFERENCES audit_logs (id) ON DELETE CASCADE ON UPDATE CASCADE,
    INDEX idx_audit_hash_chain_sequence (sequence_number)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;
