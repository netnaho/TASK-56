-- Migration 020 — Artifact backfill status tracking
--
-- Purpose
-- -------
-- Adds a `backfill_status` column to `report_runs` so the legacy-artifact
-- backfill process (Phase 6 hardening) can record why a specific row could
-- not be upgraded to cryptographic-erasure coverage.
--
-- Values
-- ------
--   NULL           → not yet assessed, or successfully encrypted (DEK present)
--   'missing_file' → file was absent from disk; cannot encrypt; no physical
--                    deletion needed at retention time
--   'encrypt_failed' → file exists but encryption failed (I/O error, corrupt);
--                      retryable on next backfill run
--
-- Idempotency
-- -----------
-- Uses a stored procedure that checks INFORMATION_SCHEMA before issuing the
-- ALTER TABLE, so re-running this migration on a database that already has
-- the column is safe.

DROP PROCEDURE IF EXISTS _scholarly_020_add_backfill_status;

DELIMITER $$

CREATE PROCEDURE _scholarly_020_add_backfill_status()
BEGIN
    DECLARE col_exists INT DEFAULT 0;

    SELECT COUNT(*) INTO col_exists
      FROM information_schema.COLUMNS
     WHERE TABLE_SCHEMA = DATABASE()
       AND TABLE_NAME   = 'report_runs'
       AND COLUMN_NAME  = 'backfill_status';

    IF col_exists = 0 THEN
        ALTER TABLE report_runs
            ADD COLUMN backfill_status VARCHAR(64) DEFAULT NULL
            COMMENT 'backfill outcome: NULL=not assessed/ok, missing_file, encrypt_failed';
    END IF;
END$$

DELIMITER ;

CALL _scholarly_020_add_backfill_status();

DROP PROCEDURE IF EXISTS _scholarly_020_add_backfill_status;
