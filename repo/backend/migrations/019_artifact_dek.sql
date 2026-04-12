-- Phase 6 (hardened) — Cryptographic erasure for report artifact files.
--
-- Adds a nullable `artifact_dek` column to `report_runs`.  Each generated
-- artifact is encrypted on disk with a per-artifact Data Encryption Key (DEK)
-- that is itself wrapped (encrypted) by the master `FIELD_ENCRYPTION_KEY`.
-- The wrapped DEK is stored here in `enc:<base64url>` format.
--
-- Retention-policy deletion now NULLs this column first (cryptographic
-- erasure), making the on-disk ciphertext irrecoverable even on
-- copy-on-write / container overlay filesystems where physical-block zeroing
-- is not guaranteed.  The physical file is then removed best-effort.
--
-- Legacy rows (artifact_dek IS NULL) continue to use the previous
-- zero-overwrite physical-deletion path.

ALTER TABLE report_runs
    ADD COLUMN artifact_dek TEXT DEFAULT NULL
    COMMENT 'enc:<base64url> wrapped per-artifact DEK; NULL for legacy artifacts';
