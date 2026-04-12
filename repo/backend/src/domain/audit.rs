use serde::{Serialize, Deserialize};
use chrono::NaiveDateTime;
use uuid::Uuid;

/// A single entry in the append-only audit log.
///
/// Audit logs capture who did what, when, and to which entity.  They are
/// designed to be immutable once written.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditLog {
    pub id: Uuid,

    /// The user who performed the action (`None` for system-initiated events).
    pub actor_id: Option<Uuid>,

    /// Dot-delimited action identifier, e.g. "course.publish", "user.deactivate".
    pub action: String,

    /// The type of entity affected, e.g. "Course", "User".
    pub target_entity_type: String,

    /// Primary key of the affected entity.
    pub target_entity_id: Uuid,

    /// JSON snapshot of the changed fields (before/after).
    /// TODO: decide on diff format (full snapshot vs delta) in phase 2.
    pub change_payload: Option<String>,

    /// Source IP address of the request.
    pub ip_address: Option<String>,

    /// User-Agent string of the client.
    pub user_agent: Option<String>,

    /// Reference to the hash-chain entry that seals this log row.
    pub hash_chain_id: Option<Uuid>,

    pub created_at: NaiveDateTime,
}

/// Chained-hash record that guarantees the integrity of audit log entries.
///
/// Each row contains a SHA-256 hash computed over its own payload concatenated
/// with the previous row's hash, forming a tamper-evident chain similar to a
/// blockchain.  Verification walks the chain and re-computes each hash.
///
/// # Integrity guarantee
///
/// If any row in the chain is modified or deleted, the hash of every
/// subsequent row will fail verification, making tampering detectable.
///
/// TODO: decide on hash algorithm (SHA-256 vs BLAKE3) and external anchor
///       strategy (e.g. periodic publish to an external transparency log) in phase 2.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditHashChain {
    pub id: Uuid,

    /// The audit log entry this hash covers.
    pub audit_log_id: Uuid,

    /// The hash of the previous chain entry, or a well-known genesis value for the first entry.
    pub previous_hash: String,

    /// The computed hash for this entry: H(previous_hash || serialised audit row).
    pub current_hash: String,

    /// Monotonically increasing sequence number within the chain.
    pub sequence_number: i64,

    pub created_at: NaiveDateTime,
}
