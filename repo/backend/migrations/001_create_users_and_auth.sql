-- ============================================================================
-- Migration 001: Users, Authentication, and Authorization
-- ============================================================================
-- Creates the foundational tables for user management, role-based access
-- control, session tracking, failed login monitoring, and departments.
-- ============================================================================

CREATE TABLE departments (
    id              CHAR(36)     NOT NULL,
    name            VARCHAR(255) NOT NULL,
    code            VARCHAR(50)  NOT NULL,
    parent_department_id CHAR(36) DEFAULT NULL,
    created_at      DATETIME     NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at      DATETIME     NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
    PRIMARY KEY (id),
    UNIQUE KEY uq_departments_code (code),
    CONSTRAINT fk_departments_parent FOREIGN KEY (parent_department_id) REFERENCES departments (id) ON DELETE SET NULL ON UPDATE CASCADE,
    INDEX idx_departments_parent (parent_department_id)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE users (
    id              CHAR(36)     NOT NULL,
    email           VARCHAR(255) NOT NULL,
    display_name    VARCHAR(255) NOT NULL,
    password_hash   VARCHAR(512) NOT NULL,
    status          ENUM('pending_verification','active','suspended','deactivated') NOT NULL DEFAULT 'active',
    phone           VARCHAR(50)  DEFAULT NULL,
    avatar_url      TEXT         DEFAULT NULL,
    department_id   CHAR(36)     DEFAULT NULL,
    created_at      DATETIME     NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at      DATETIME     NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
    PRIMARY KEY (id),
    UNIQUE KEY uq_users_email (email),
    INDEX idx_users_status (status),
    INDEX idx_users_department (department_id)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE roles (
    id              CHAR(36)     NOT NULL,
    name            VARCHAR(100) NOT NULL,
    display_name    VARCHAR(255) NOT NULL,
    description     TEXT         DEFAULT NULL,
    created_at      DATETIME     NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (id),
    UNIQUE KEY uq_roles_name (name)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE permissions (
    id              CHAR(36)     NOT NULL,
    key_name        VARCHAR(255) NOT NULL,
    description     TEXT         DEFAULT NULL,
    created_at      DATETIME     NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (id),
    UNIQUE KEY uq_permissions_key_name (key_name)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE role_permissions (
    role_id         CHAR(36)     NOT NULL,
    permission_id   CHAR(36)     NOT NULL,
    PRIMARY KEY (role_id, permission_id),
    CONSTRAINT fk_role_permissions_role FOREIGN KEY (role_id) REFERENCES roles (id) ON DELETE CASCADE ON UPDATE CASCADE,
    CONSTRAINT fk_role_permissions_permission FOREIGN KEY (permission_id) REFERENCES permissions (id) ON DELETE CASCADE ON UPDATE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE user_roles (
    id              CHAR(36)     NOT NULL,
    user_id         CHAR(36)     NOT NULL,
    role_id         CHAR(36)     NOT NULL,
    assigned_by     CHAR(36)     DEFAULT NULL,
    assigned_at     DATETIME     NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (id),
    UNIQUE KEY uq_user_roles_user_role (user_id, role_id),
    CONSTRAINT fk_user_roles_user FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE ON UPDATE CASCADE,
    CONSTRAINT fk_user_roles_role FOREIGN KEY (role_id) REFERENCES roles (id) ON DELETE CASCADE ON UPDATE CASCADE,
    CONSTRAINT fk_user_roles_assigned_by FOREIGN KEY (assigned_by) REFERENCES users (id) ON DELETE SET NULL ON UPDATE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE sessions (
    id                 CHAR(36)     NOT NULL,
    user_id            CHAR(36)     NOT NULL,
    refresh_token_hash VARCHAR(512) NOT NULL,
    ip_address         VARCHAR(45)  DEFAULT NULL,
    user_agent         TEXT         DEFAULT NULL,
    expires_at         DATETIME     NOT NULL,
    revoked_at         DATETIME     DEFAULT NULL,
    created_at         DATETIME     NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (id),
    CONSTRAINT fk_sessions_user FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE ON UPDATE CASCADE,
    INDEX idx_sessions_user (user_id),
    INDEX idx_sessions_expires (expires_at)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE failed_login_attempts (
    id              CHAR(36)     NOT NULL,
    email           VARCHAR(255) NOT NULL,
    ip_address      VARCHAR(45)  DEFAULT NULL,
    attempted_at    DATETIME     NOT NULL DEFAULT CURRENT_TIMESTAMP,
    reason          VARCHAR(255) DEFAULT NULL,
    PRIMARY KEY (id),
    INDEX idx_failed_logins_email (email),
    INDEX idx_failed_logins_ip (ip_address),
    INDEX idx_failed_logins_attempted (attempted_at)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

-- Add the deferred foreign key from users to departments
ALTER TABLE users
    ADD CONSTRAINT fk_users_department FOREIGN KEY (department_id) REFERENCES departments (id) ON DELETE SET NULL ON UPDATE CASCADE;
