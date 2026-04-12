-- ============================================================================
-- Migration 004: Courses, Course Versions, Sections, and Section Versions
-- ============================================================================
-- Creates tables for course and section management with full version tracking
-- for both entities.
-- ============================================================================

CREATE TABLE courses (
    id                 CHAR(36)     NOT NULL,
    code               VARCHAR(50)  NOT NULL,
    title              VARCHAR(500) NOT NULL,
    department_id      CHAR(36)     DEFAULT NULL,
    owner_id           CHAR(36)     DEFAULT NULL,
    is_active          BOOLEAN      NOT NULL DEFAULT TRUE,
    current_version_id CHAR(36)     DEFAULT NULL,
    created_at         DATETIME     NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at         DATETIME     NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
    PRIMARY KEY (id),
    UNIQUE KEY uq_courses_code (code),
    CONSTRAINT fk_courses_department FOREIGN KEY (department_id) REFERENCES departments (id) ON DELETE SET NULL ON UPDATE CASCADE,
    CONSTRAINT fk_courses_owner FOREIGN KEY (owner_id) REFERENCES users (id) ON DELETE SET NULL ON UPDATE CASCADE,
    INDEX idx_courses_department (department_id),
    INDEX idx_courses_owner (owner_id),
    INDEX idx_courses_active (is_active)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE course_versions (
    id              CHAR(36)      NOT NULL,
    course_id       CHAR(36)      NOT NULL,
    version_number  INT           NOT NULL,
    description     TEXT          DEFAULT NULL,
    syllabus        LONGTEXT      DEFAULT NULL,
    credit_hours    DECIMAL(3,1)  DEFAULT NULL,
    change_summary  TEXT          DEFAULT NULL,
    created_by      CHAR(36)      DEFAULT NULL,
    created_at      DATETIME      NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (id),
    UNIQUE KEY uq_course_versions_course_version (course_id, version_number),
    CONSTRAINT fk_course_versions_course FOREIGN KEY (course_id) REFERENCES courses (id) ON DELETE CASCADE ON UPDATE CASCADE,
    CONSTRAINT fk_course_versions_created_by FOREIGN KEY (created_by) REFERENCES users (id) ON DELETE SET NULL ON UPDATE CASCADE,
    INDEX idx_course_versions_created_by (created_by)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

-- Add the deferred foreign key for courses.current_version_id
ALTER TABLE courses
    ADD CONSTRAINT fk_courses_current_version FOREIGN KEY (current_version_id) REFERENCES course_versions (id) ON DELETE SET NULL ON UPDATE CASCADE;

CREATE TABLE sections (
    id                 CHAR(36)     NOT NULL,
    course_id          CHAR(36)     NOT NULL,
    instructor_id      CHAR(36)     DEFAULT NULL,
    section_code       VARCHAR(50)  NOT NULL,
    term               VARCHAR(50)  NOT NULL,
    year               INT          NOT NULL,
    capacity           INT          DEFAULT NULL,
    is_active          BOOLEAN      NOT NULL DEFAULT TRUE,
    current_version_id CHAR(36)     DEFAULT NULL,
    created_at         DATETIME     NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at         DATETIME     NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
    PRIMARY KEY (id),
    UNIQUE KEY uq_sections_course_section_term_year (course_id, section_code, term, year),
    CONSTRAINT fk_sections_course FOREIGN KEY (course_id) REFERENCES courses (id) ON DELETE CASCADE ON UPDATE CASCADE,
    CONSTRAINT fk_sections_instructor FOREIGN KEY (instructor_id) REFERENCES users (id) ON DELETE SET NULL ON UPDATE CASCADE,
    INDEX idx_sections_instructor (instructor_id),
    INDEX idx_sections_term_year (term, year),
    INDEX idx_sections_active (is_active)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE section_versions (
    id              CHAR(36)     NOT NULL,
    section_id      CHAR(36)     NOT NULL,
    version_number  INT          NOT NULL,
    location        VARCHAR(255) DEFAULT NULL,
    schedule_json   JSON         DEFAULT NULL,
    notes           TEXT         DEFAULT NULL,
    created_by      CHAR(36)     DEFAULT NULL,
    created_at      DATETIME     NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (id),
    UNIQUE KEY uq_section_versions_section_version (section_id, version_number),
    CONSTRAINT fk_section_versions_section FOREIGN KEY (section_id) REFERENCES sections (id) ON DELETE CASCADE ON UPDATE CASCADE,
    CONSTRAINT fk_section_versions_created_by FOREIGN KEY (created_by) REFERENCES users (id) ON DELETE SET NULL ON UPDATE CASCADE,
    INDEX idx_section_versions_created_by (created_by)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

-- Add the deferred foreign key for sections.current_version_id
ALTER TABLE sections
    ADD CONSTRAINT fk_sections_current_version FOREIGN KEY (current_version_id) REFERENCES section_versions (id) ON DELETE SET NULL ON UPDATE CASCADE;
