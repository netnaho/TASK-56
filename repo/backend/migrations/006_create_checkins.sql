-- ============================================================================
-- Migration 006: Check-in Events
-- ============================================================================
-- Creates the check-in tracking table supporting multiple check-in methods
-- (QR code, geofence, manual instructor, NFC beacon) with anti-duplicate
-- constraints and optional validation.
-- ============================================================================

CREATE TABLE checkin_events (
    id              CHAR(36)       NOT NULL,
    user_id         CHAR(36)       NOT NULL,
    section_id      CHAR(36)       NOT NULL,
    checkin_type    ENUM('qr_code','geofence','manual_instructor','nfc_beacon') NOT NULL,
    checked_in_at   DATETIME       NOT NULL,
    event_date      DATE           NOT NULL,
    latitude        DECIMAL(10,8)  DEFAULT NULL,
    longitude       DECIMAL(11,8)  DEFAULT NULL,
    device_info     TEXT           DEFAULT NULL,
    is_validated    BOOLEAN        NOT NULL DEFAULT FALSE,
    validated_by    CHAR(36)       DEFAULT NULL,
    created_at      DATETIME       NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (id),
    UNIQUE KEY uq_checkin_events_user_section_date (user_id, section_id, event_date),
    CONSTRAINT fk_checkin_events_user FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE ON UPDATE CASCADE,
    CONSTRAINT fk_checkin_events_section FOREIGN KEY (section_id) REFERENCES sections (id) ON DELETE CASCADE ON UPDATE CASCADE,
    CONSTRAINT fk_checkin_events_validated_by FOREIGN KEY (validated_by) REFERENCES users (id) ON DELETE SET NULL ON UPDATE CASCADE,
    INDEX idx_checkin_events_section (section_id),
    INDEX idx_checkin_events_event_date (event_date),
    INDEX idx_checkin_events_type (checkin_type)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;
