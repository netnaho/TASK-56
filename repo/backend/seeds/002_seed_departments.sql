-- ============================================================================
-- Seed 002: Departments
-- ============================================================================
-- Populates example departments for development and testing.
-- Uses INSERT IGNORE for idempotent re-runs.
-- ============================================================================

INSERT IGNORE INTO departments (id, name, code) VALUES
    ('20000000-0000-0000-0000-000000000001', 'Computer Science',   'CS'),
    ('20000000-0000-0000-0000-000000000002', 'Library Sciences',   'LS'),
    ('20000000-0000-0000-0000-000000000003', 'Mathematics',        'MATH');
