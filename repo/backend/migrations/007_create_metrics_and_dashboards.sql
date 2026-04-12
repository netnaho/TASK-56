-- ============================================================================
-- Migration 007: Metrics and Dashboards
-- ============================================================================
-- Creates tables for metric definitions (with versioning), dashboard layouts,
-- and dashboard widgets for configurable analytics displays.
-- ============================================================================

CREATE TABLE metric_definitions (
    id                 CHAR(36)     NOT NULL,
    key_name           VARCHAR(255) NOT NULL,
    display_name       VARCHAR(255) NOT NULL,
    unit               VARCHAR(50)  DEFAULT NULL,
    polarity           ENUM('higher_is_better','lower_is_better','neutral') NOT NULL DEFAULT 'neutral',
    current_version_id CHAR(36)     DEFAULT NULL,
    created_by         CHAR(36)     DEFAULT NULL,
    created_at         DATETIME     NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at         DATETIME     NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
    PRIMARY KEY (id),
    UNIQUE KEY uq_metric_definitions_key_name (key_name),
    CONSTRAINT fk_metric_definitions_created_by FOREIGN KEY (created_by) REFERENCES users (id) ON DELETE SET NULL ON UPDATE CASCADE,
    INDEX idx_metric_definitions_created_by (created_by)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE metric_definition_versions (
    id                    CHAR(36) NOT NULL,
    metric_definition_id  CHAR(36) NOT NULL,
    version_number        INT      NOT NULL,
    formula               TEXT     DEFAULT NULL,
    description           TEXT     DEFAULT NULL,
    change_summary        TEXT     DEFAULT NULL,
    created_by            CHAR(36) DEFAULT NULL,
    created_at            DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (id),
    UNIQUE KEY uq_metric_def_versions_def_version (metric_definition_id, version_number),
    CONSTRAINT fk_metric_def_versions_definition FOREIGN KEY (metric_definition_id) REFERENCES metric_definitions (id) ON DELETE CASCADE ON UPDATE CASCADE,
    CONSTRAINT fk_metric_def_versions_created_by FOREIGN KEY (created_by) REFERENCES users (id) ON DELETE SET NULL ON UPDATE CASCADE,
    INDEX idx_metric_def_versions_created_by (created_by)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

-- Add the deferred foreign key for metric_definitions.current_version_id
ALTER TABLE metric_definitions
    ADD CONSTRAINT fk_metric_definitions_current_version FOREIGN KEY (current_version_id) REFERENCES metric_definition_versions (id) ON DELETE SET NULL ON UPDATE CASCADE;

CREATE TABLE dashboard_definitions (
    id              CHAR(36)     NOT NULL,
    title           VARCHAR(255) NOT NULL,
    owner_id        CHAR(36)     DEFAULT NULL,
    is_shared       BOOLEAN      NOT NULL DEFAULT FALSE,
    layout_json     JSON         DEFAULT NULL,
    created_at      DATETIME     NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at      DATETIME     NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
    PRIMARY KEY (id),
    CONSTRAINT fk_dashboard_definitions_owner FOREIGN KEY (owner_id) REFERENCES users (id) ON DELETE SET NULL ON UPDATE CASCADE,
    INDEX idx_dashboard_definitions_owner (owner_id),
    INDEX idx_dashboard_definitions_shared (is_shared)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE dashboard_widgets (
    id                    CHAR(36)     NOT NULL,
    dashboard_id          CHAR(36)     NOT NULL,
    metric_definition_id  CHAR(36)     DEFAULT NULL,
    widget_type           VARCHAR(100) NOT NULL,
    config_json           JSON         DEFAULT NULL,
    position_x            INT          DEFAULT NULL,
    position_y            INT          DEFAULT NULL,
    width                 INT          DEFAULT NULL,
    height                INT          DEFAULT NULL,
    created_at            DATETIME     NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (id),
    CONSTRAINT fk_dashboard_widgets_dashboard FOREIGN KEY (dashboard_id) REFERENCES dashboard_definitions (id) ON DELETE CASCADE ON UPDATE CASCADE,
    CONSTRAINT fk_dashboard_widgets_metric FOREIGN KEY (metric_definition_id) REFERENCES metric_definitions (id) ON DELETE SET NULL ON UPDATE CASCADE,
    INDEX idx_dashboard_widgets_dashboard (dashboard_id),
    INDEX idx_dashboard_widgets_metric (metric_definition_id)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;
