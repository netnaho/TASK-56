-- Migration 021: Convert course_versions.credit_hours and contact_hours from
-- DECIMAL to FLOAT to match the Rust f32 type used by sqlx throughout
-- course_service, export_service, import_service, and report_service.
--
-- DECIMAL is incompatible with sqlx's f32/FLOAT mapping and causes a runtime
-- "mismatched types" error on every course read/create.

ALTER TABLE course_versions
    MODIFY COLUMN credit_hours  FLOAT DEFAULT NULL,
    MODIFY COLUMN contact_hours FLOAT DEFAULT NULL;
