-- ============================================================================
-- Migration 013: Phase 4 — Academic scheduling extensions
-- ============================================================================
-- * Extends `course_versions` and `section_versions` with the same
--   draft/approved/published/archived state machine used by journals and
--   teaching resources in Phase 3.
-- * Adds the `contact_hours` field to `course_versions` (credit_hours was
--   already present).
-- * Adds the `latest_version_id` pointer to `courses` / `sections` so the
--   parent row can distinguish the published baseline from the current
--   head draft (same two-pointer invariant as Phase 3).
-- * Adds `course_prerequisites` — a directed adjacency list. Enforces the
--   no-self-loop constraint at the DB level; cycle prevention (beyond
--   direct self-references) is enforced in the service layer.
-- * Adds `import_jobs` for traceability of bulk imports (both dry-run
--   and commit runs); row-level errors stay in the API response, but the
--   job envelope — counts + outcome — is persisted.
-- ============================================================================

-- ── course_versions ───────────────────────────────────────────────────────
ALTER TABLE course_versions
    ADD COLUMN contact_hours DECIMAL(4,1) DEFAULT NULL AFTER credit_hours,
    ADD COLUMN state ENUM('draft','approved','published','archived')
        NOT NULL DEFAULT 'draft' AFTER change_summary,
    ADD COLUMN approved_by  CHAR(36) DEFAULT NULL AFTER created_at,
    ADD COLUMN approved_at  DATETIME DEFAULT NULL AFTER approved_by,
    ADD COLUMN published_by CHAR(36) DEFAULT NULL AFTER approved_at,
    ADD COLUMN published_at DATETIME DEFAULT NULL AFTER published_by,
    ADD CONSTRAINT fk_cv_approved_by FOREIGN KEY (approved_by)
        REFERENCES users (id) ON DELETE SET NULL ON UPDATE CASCADE,
    ADD CONSTRAINT fk_cv_published_by FOREIGN KEY (published_by)
        REFERENCES users (id) ON DELETE SET NULL ON UPDATE CASCADE,
    ADD INDEX idx_cv_state (state);

ALTER TABLE courses
    ADD COLUMN latest_version_id CHAR(36) DEFAULT NULL AFTER current_version_id,
    ADD CONSTRAINT fk_courses_latest_version FOREIGN KEY (latest_version_id)
        REFERENCES course_versions (id) ON DELETE SET NULL ON UPDATE CASCADE;

-- ── section_versions ──────────────────────────────────────────────────────
ALTER TABLE section_versions
    ADD COLUMN state ENUM('draft','approved','published','archived')
        NOT NULL DEFAULT 'draft' AFTER notes,
    ADD COLUMN approved_by  CHAR(36) DEFAULT NULL AFTER created_at,
    ADD COLUMN approved_at  DATETIME DEFAULT NULL AFTER approved_by,
    ADD COLUMN published_by CHAR(36) DEFAULT NULL AFTER approved_at,
    ADD COLUMN published_at DATETIME DEFAULT NULL AFTER published_by,
    ADD CONSTRAINT fk_sv_approved_by FOREIGN KEY (approved_by)
        REFERENCES users (id) ON DELETE SET NULL ON UPDATE CASCADE,
    ADD CONSTRAINT fk_sv_published_by FOREIGN KEY (published_by)
        REFERENCES users (id) ON DELETE SET NULL ON UPDATE CASCADE,
    ADD INDEX idx_sv_state (state);

ALTER TABLE sections
    ADD COLUMN latest_version_id CHAR(36) DEFAULT NULL AFTER current_version_id,
    ADD CONSTRAINT fk_sections_latest_version FOREIGN KEY (latest_version_id)
        REFERENCES section_versions (id) ON DELETE SET NULL ON UPDATE CASCADE;

-- ── course_prerequisites ──────────────────────────────────────────────────
-- A simple AND-list: every row (course_id, prerequisite_course_id) means
-- "course_id depends on prerequisite_course_id". No OR groups for Phase 4.
-- Self-references are rejected in the service layer (MySQL CHECK syntax
-- varies by version); direct duplicates are caught by the composite PK.
CREATE TABLE course_prerequisites (
    course_id              CHAR(36) NOT NULL,
    prerequisite_course_id CHAR(36) NOT NULL,
    min_grade              VARCHAR(4) DEFAULT NULL,
    created_by             CHAR(36) DEFAULT NULL,
    created_at             DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (course_id, prerequisite_course_id),
    CONSTRAINT fk_prereq_course FOREIGN KEY (course_id)
        REFERENCES courses (id) ON DELETE CASCADE ON UPDATE CASCADE,
    CONSTRAINT fk_prereq_prerequisite FOREIGN KEY (prerequisite_course_id)
        REFERENCES courses (id) ON DELETE CASCADE ON UPDATE CASCADE,
    CONSTRAINT fk_prereq_created_by FOREIGN KEY (created_by)
        REFERENCES users (id) ON DELETE SET NULL ON UPDATE CASCADE,
    INDEX idx_prereq_prerequisite (prerequisite_course_id)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

-- ── import_jobs ───────────────────────────────────────────────────────────
-- Records the envelope of a bulk import run. Row-level errors live in
-- the API response (they can be large); only the aggregate counts and
-- outcome are persisted for audit/traceability.
CREATE TABLE import_jobs (
    id            CHAR(36)     NOT NULL,
    job_type      ENUM('courses','sections') NOT NULL,
    mode          ENUM('dry_run','commit') NOT NULL,
    source_format ENUM('csv','xlsx') NOT NULL,
    status        ENUM('pending','validated','committed','failed') NOT NULL,
    total_rows    INT NOT NULL DEFAULT 0,
    valid_rows    INT NOT NULL DEFAULT 0,
    error_rows    INT NOT NULL DEFAULT 0,
    initiated_by  CHAR(36) DEFAULT NULL,
    created_at    DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    completed_at  DATETIME DEFAULT NULL,
    PRIMARY KEY (id),
    CONSTRAINT fk_import_jobs_initiator FOREIGN KEY (initiated_by)
        REFERENCES users (id) ON DELETE SET NULL ON UPDATE CASCADE,
    INDEX idx_import_jobs_created_at (created_at),
    INDEX idx_import_jobs_status (status)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;
