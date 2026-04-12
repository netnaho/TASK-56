-- ============================================================================
-- Seed 006: Check-in retry reasons (Phase 5)
-- ============================================================================
-- The controlled list of reasons that a user must select when retrying a
-- check-in after the first attempt was rejected as a duplicate or a
-- network-rule failure. The service layer validates the submitted
-- reason_code against this table; there is no "other" free-text path.
-- ============================================================================

INSERT IGNORE INTO checkin_retry_reasons (reason_code, display_name, description) VALUES
    ('wrong_section',
     'Wrong section selected',
     'The user accidentally tapped into the wrong course section.'),
    ('device_glitch',
     'Device froze or network hiccup',
     'The user browser froze or the initial submission did not complete cleanly.'),
    ('offline_proxy',
     'Proxy / offline check-in replay',
     'The original attempt happened while offline and is being replayed on reconnect.'),
    ('instructor_override',
     'Instructor requested a redo',
     'An instructor asked the learner to re-tap after a manual correction.'),
    ('scanner_misread',
     'QR scanner misread',
     'The QR scanner captured a garbled payload on the first attempt.');
