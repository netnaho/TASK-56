//! Shared versioning vocabulary for library content.
//!
//! Journals and teaching resources both flow through the same lifecycle:
//!
//! ```text
//!   draft ──approve──▶ approved ──publish──▶ published
//!     │                    │                     │
//!     └─────── archive ────┴──────── archive ────┘
//! ```
//!
//! * `draft` — the editor's working copy. Multiple drafts can exist; only
//!   one is the "latest draft" per parent (`latest_version_id`).
//! * `approved` — a draft that has been reviewed. Still not visible to
//!   read-only audiences.
//! * `published` — the operational baseline. At most one version per
//!   parent is `published`; the parent's `current_version_id` points at it.
//! * `archived` — superseded or withdrawn. Retained for audit.
//!
//! State transitions are **strictly one-way forward** except for
//! archival, which can happen from any non-archived state. Any other
//! transition raises `AppError::Conflict`.

use serde::{Deserialize, Serialize};

use crate::errors::{AppError, AppResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VersionState {
    Draft,
    Approved,
    Published,
    Archived,
}

impl VersionState {
    pub fn as_db(self) -> &'static str {
        match self {
            VersionState::Draft => "draft",
            VersionState::Approved => "approved",
            VersionState::Published => "published",
            VersionState::Archived => "archived",
        }
    }

    pub fn from_db(s: &str) -> Option<Self> {
        match s {
            "draft" => Some(VersionState::Draft),
            "approved" => Some(VersionState::Approved),
            "published" => Some(VersionState::Published),
            "archived" => Some(VersionState::Archived),
            _ => None,
        }
    }
}

/// Validate a requested state transition.
///
/// Returns `Ok(())` if the move is legal, otherwise an `AppError::Conflict`
/// naming the offending move. This is the single source of truth for
/// state-machine legality; service methods call it before any DB write.
pub fn validate_transition(from: VersionState, to: VersionState) -> AppResult<()> {
    use VersionState::*;
    let ok = matches!(
        (from, to),
        (Draft, Approved)
            | (Approved, Published)
            | (Draft, Archived)
            | (Approved, Archived)
            | (Published, Archived)
    );
    if ok {
        Ok(())
    } else {
        Err(AppError::Conflict(format!(
            "illegal version state transition: {:?} -> {:?}",
            from, to
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legal_forward_transitions() {
        assert!(validate_transition(VersionState::Draft, VersionState::Approved).is_ok());
        assert!(validate_transition(VersionState::Approved, VersionState::Published).is_ok());
    }

    #[test]
    fn archive_is_legal_from_any_non_archived_state() {
        assert!(validate_transition(VersionState::Draft, VersionState::Archived).is_ok());
        assert!(validate_transition(VersionState::Approved, VersionState::Archived).is_ok());
        assert!(validate_transition(VersionState::Published, VersionState::Archived).is_ok());
    }

    #[test]
    fn backward_and_skipping_transitions_are_rejected() {
        // Skipping approved:
        assert!(validate_transition(VersionState::Draft, VersionState::Published).is_err());
        // Backward:
        assert!(validate_transition(VersionState::Approved, VersionState::Draft).is_err());
        assert!(validate_transition(VersionState::Published, VersionState::Draft).is_err());
        assert!(validate_transition(VersionState::Published, VersionState::Approved).is_err());
        // From archived — terminal state:
        assert!(validate_transition(VersionState::Archived, VersionState::Draft).is_err());
        assert!(validate_transition(VersionState::Archived, VersionState::Published).is_err());
    }

    #[test]
    fn archived_is_terminal_and_cannot_reach_published() {
        // Archived is a terminal state: once a version is archived, the
        // state machine must never let it be re-promoted to published.
        // This test pins the invariant explicitly in case a future edit
        // adds a "restore" transition by accident.
        assert!(
            validate_transition(VersionState::Archived, VersionState::Published).is_err(),
            "Archived -> Published must be rejected (terminal state never re-opens)"
        );
        // Also assert the inverse direction is forbidden — archived is
        // terminal both ways.
        assert!(
            validate_transition(VersionState::Archived, VersionState::Approved).is_err()
        );
        assert!(
            validate_transition(VersionState::Archived, VersionState::Draft).is_err()
        );
        assert!(
            validate_transition(VersionState::Archived, VersionState::Archived).is_err()
        );
    }

    #[test]
    fn db_round_trip() {
        for v in [
            VersionState::Draft,
            VersionState::Approved,
            VersionState::Published,
            VersionState::Archived,
        ] {
            assert_eq!(VersionState::from_db(v.as_db()), Some(v));
        }
        assert!(VersionState::from_db("nope").is_none());
    }
}
