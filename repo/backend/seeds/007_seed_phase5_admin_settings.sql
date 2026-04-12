-- ============================================================================
-- Seed 007: Phase 5 admin settings
-- ============================================================================
-- * `checkin.allowed_client_cidrs`  — the **truthful** implementation of
--   the "approved SSID / local network rule". Browsers cannot read the
--   current Wi-Fi SSID in a normal web security context, so we enforce
--   "on-campus" server-side by comparing the client IP against an
--   admin-maintained list of CIDR ranges. Empty = rule disabled.
--   See `application::checkin_service::ip_matches_any_cidr`.
-- * `checkin.network_hint_label` — display-only string shown on the
--   check-in screen (e.g. "Connect to 'CampusWiFi' before checking in")
--   so operators can still surface their SSID guidance. Nothing is
--   enforced from this value.
-- * `checkin.max_retry_count`      — number of retries allowed per
--   original attempt (Phase 5 default: 1).
-- * `checkin.duplicate_window_minutes` was already seeded in Phase 2
--   (`seeds/005_seed_admin_settings.sql`, default 10).
-- ============================================================================

INSERT IGNORE INTO admin_settings (setting_key, setting_value, description) VALUES
    ('checkin.allowed_client_cidrs',
     JSON_ARRAY(),
     'Server-side IP CIDR whitelist for the "on-campus" rule. Empty array disables the rule. Check-in verifies the request IP at write time against this list; there is no reliance on browser-reported SSID.'),
    ('checkin.network_hint_label',
     CAST('""' AS JSON),
     'Display-only hint shown on the check-in screen (e.g. "Connect to CampusWiFi before checking in"). Not used for enforcement.'),
    ('checkin.max_retry_count',
     CAST('1' AS JSON),
     'Maximum number of retries allowed after the original check-in attempt.');
