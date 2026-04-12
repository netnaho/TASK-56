-- Phase 6: Field-level encryption flags for sensitive columns.
--
-- Adds a boolean flag beside each plaintext TEXT column that may hold
-- encrypted ciphertext (AES-256-GCM, stored as "enc:<base64url>").
-- The flag lets readers detect whether a value is ciphertext or legacy
-- plaintext, enabling a clean forward-only migration without touching
-- existing rows.
--
-- Fields encrypted starting with Phase 6:
--   * section_versions.notes         — instructor notes; may reference students
--
-- Fields NOT field-encrypted (reason documented):
--   * section_versions.schedule_json — JSON column; not "enc:"-prefix safe
--   * checkin_events.device_fingerprint — JSON column; see docs/phase_6_summary.md
--   * audit_logs entries — encrypting would break chain hash verification

ALTER TABLE section_versions
    ADD COLUMN notes_encrypted TINYINT(1) NOT NULL DEFAULT 0
        COMMENT '1 = notes column holds AES-256-GCM ciphertext (enc:<base64url>)';
