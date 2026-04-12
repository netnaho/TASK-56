use serde::{Serialize, Deserialize};
use chrono::NaiveDateTime;
use uuid::Uuid;

/// The method by which the check-in was recorded.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum CheckinType {
    /// QR code scanned at the venue.
    QrCode,
    /// Geolocation-based proximity check.
    Geofence,
    /// Manual entry by the instructor.
    ManualInstructor,
    /// NFC tap or Bluetooth beacon.
    /// TODO: confirm hardware integration requirements in phase 2.
    NfcBeacon,
}

/// Records a single attendance check-in event.
///
/// Each event ties a user to a section at a specific point in time.
///
/// # Anti-duplicate strategy
///
/// To prevent the same user from registering multiple check-ins for the same
/// section meeting, the system enforces a **unique constraint** on
/// `(user_id, section_id, event_date)` where `event_date` is the calendar
/// date derived from `checked_in_at`.
///
/// At the application layer a debounce window (configurable, default 15 min)
/// rejects repeat submissions before they hit the database.
///
/// TODO: finalise whether the debounce window is per-section or global in phase 2.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckinEvent {
    pub id: Uuid,

    /// The user checking in.
    pub user_id: Uuid,

    /// The section (class meeting) being attended.
    pub section_id: Uuid,

    /// How the check-in was captured.
    pub checkin_type: CheckinType,

    /// Exact timestamp of the check-in.
    pub checked_in_at: NaiveDateTime,

    /// Optional latitude captured at check-in time.
    /// TODO: decide on precision and privacy policy in phase 2.
    pub latitude: Option<f64>,

    /// Optional longitude captured at check-in time.
    pub longitude: Option<f64>,

    /// Device or client identifier that submitted the event.
    pub device_id: Option<String>,

    /// Whether the event has been validated (e.g. within geofence radius).
    pub is_validated: bool,

    pub created_at: NaiveDateTime,
}
