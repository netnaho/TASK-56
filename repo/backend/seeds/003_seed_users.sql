-- ============================================================================
-- Seed 003: Default Users
-- ============================================================================
-- Creates one user per role for development and testing.
-- Uses INSERT IGNORE for idempotent re-runs.
--
-- SECURITY NOTE
-- -------------
-- Password hashes are stored as the sentinel value '__BOOTSTRAP__' here so
-- that the SQL seed file contains *no* pre-computed credential material.
-- On server startup, `infrastructure::bootstrap::ensure_seed_passwords`
-- detects the sentinel and replaces it with a real Argon2id hash of the
-- documented default password (see README "Default Seed Users"). The default
-- password is 12+ characters — the minimum enforced by the password policy.
-- ============================================================================

INSERT IGNORE INTO users (id, email, display_name, password_hash, status, department_id) VALUES
    ('30000000-0000-0000-0000-000000000001', 'admin@scholarly.local',      'System Administrator', '__BOOTSTRAP__', 'active', NULL),
    ('30000000-0000-0000-0000-000000000002', 'librarian@scholarly.local',  'Default Librarian',    '__BOOTSTRAP__', 'active', '20000000-0000-0000-0000-000000000002'),
    ('30000000-0000-0000-0000-000000000003', 'instructor@scholarly.local', 'Default Instructor',   '__BOOTSTRAP__', 'active', '20000000-0000-0000-0000-000000000001'),
    ('30000000-0000-0000-0000-000000000004', 'depthead@scholarly.local',   'Default Dept Head',    '__BOOTSTRAP__', 'active', '20000000-0000-0000-0000-000000000001'),
    ('30000000-0000-0000-0000-000000000005', 'viewer@scholarly.local',     'Default Viewer',       '__BOOTSTRAP__', 'active', '20000000-0000-0000-0000-000000000003'),
    ('30000000-0000-0000-0000-000000000006', 'auditor@scholarly.local',    'Default Auditor',      '__BOOTSTRAP__', 'active', NULL);

-- ---------------------------------------------------------------------------
-- User-Role assignments
-- ---------------------------------------------------------------------------
INSERT IGNORE INTO user_roles (id, user_id, role_id) VALUES
    ('40000000-0000-0000-0000-000000000001', '30000000-0000-0000-0000-000000000001', '00000000-0000-0000-0000-000000000001'),
    ('40000000-0000-0000-0000-000000000002', '30000000-0000-0000-0000-000000000002', '00000000-0000-0000-0000-000000000002'),
    ('40000000-0000-0000-0000-000000000003', '30000000-0000-0000-0000-000000000003', '00000000-0000-0000-0000-000000000003'),
    ('40000000-0000-0000-0000-000000000004', '30000000-0000-0000-0000-000000000004', '00000000-0000-0000-0000-000000000004'),
    ('40000000-0000-0000-0000-000000000005', '30000000-0000-0000-0000-000000000005', '00000000-0000-0000-0000-000000000005'),
    ('40000000-0000-0000-0000-000000000006', '30000000-0000-0000-0000-000000000006', '00000000-0000-0000-0000-000000000006');
