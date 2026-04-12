-- Migration 017: Canonical session table name reconciliation
--
-- Context
-- -------
-- The authoritative schema (migration 001) has always created a table named
-- `sessions`.  Application code in the retention and user-management paths
-- mistakenly referenced `user_sessions`, a table that was never created.
-- This migration makes deployments that somehow ended up with a `user_sessions`
-- table (e.g. from a hand-applied hotfix or a partial rollback) safe to
-- migrate forward.
--
-- Scenarios handled
-- -----------------
-- 1. Normal case — only `sessions` exists (created by migration 001):
--    Nothing to do; the RENAME branch is skipped by the IF NOT EXISTS guard.
--
-- 2. Legacy case — only `user_sessions` exists (table was renamed by hand):
--    Rename `user_sessions` → `sessions` so all code paths work correctly.
--
-- 3. Both tables exist:
--    This is the ambiguous case.  We do NOT automatically merge or drop either
--    table to avoid destroying live session data.  A DBA must reconcile the
--    two tables manually (see the comment block below) before re-running
--    migrations.  The migration is intentionally idempotent: if `sessions`
--    already exists the procedure exits cleanly on the second run too.
--
-- Manual merge instructions (scenario 3 only)
-- --------------------------------------------
-- 1. Identify which table holds the live (non-revoked, non-expired) tokens:
--      SELECT COUNT(*) FROM sessions      WHERE revoked_at IS NULL AND expires_at > NOW();
--      SELECT COUNT(*) FROM user_sessions WHERE revoked_at IS NULL AND expires_at > NOW();
-- 2. Copy rows from the table with fewer live rows into the other:
--      INSERT IGNORE INTO sessions SELECT * FROM user_sessions;
--    (or vice-versa; both schemas are identical)
-- 3. Drop the now-merged table:
--      DROP TABLE user_sessions;
-- 4. Re-run this migration to let the stored procedure exit normally.

DROP PROCEDURE IF EXISTS _scholarly_017_rename_user_sessions;

DELIMITER $$

CREATE PROCEDURE _scholarly_017_rename_user_sessions()
BEGIN
    DECLARE sessions_exists     INT DEFAULT 0;
    DECLARE user_sessions_exists INT DEFAULT 0;

    SELECT COUNT(*) INTO sessions_exists
      FROM information_schema.TABLES
     WHERE TABLE_SCHEMA = DATABASE()
       AND TABLE_NAME   = 'sessions';

    SELECT COUNT(*) INTO user_sessions_exists
      FROM information_schema.TABLES
     WHERE TABLE_SCHEMA = DATABASE()
       AND TABLE_NAME   = 'user_sessions';

    IF user_sessions_exists = 1 AND sessions_exists = 0 THEN
        -- Legacy environment: rename to the canonical name.
        RENAME TABLE user_sessions TO sessions;

    ELSEIF user_sessions_exists = 1 AND sessions_exists = 1 THEN
        -- Ambiguous: both tables present.  Bail out with an actionable error
        -- rather than silently dropping data.  See the manual merge
        -- instructions above.
        SIGNAL SQLSTATE '45000'
            SET MESSAGE_TEXT =
                'Migration 017 requires manual intervention: both `sessions` '
                'and `user_sessions` tables exist. See migration comment for '
                'merge instructions before re-running.';

    -- ELSE: only `sessions` exists — nothing to do (normal case).
    END IF;
END$$

DELIMITER ;

CALL _scholarly_017_rename_user_sessions();

DROP PROCEDURE IF EXISTS _scholarly_017_rename_user_sessions;
