//! Admin endpoint for the legacy artifact backfill operation.
//!
//! # Route map
//!
//! ```text
//! POST /api/v1/admin/artifact-backfill   → run_backfill
//! ```
//!
//! Requires `RetentionManage` capability (Admin role only).  The endpoint
//! accepts an optional `dry_run` flag and `batch_size` control; with
//! `dry_run = true` it returns the count of eligible rows without touching
//! any files or DB rows.

use rocket::serde::json::Json;
use rocket::State;
use serde::Deserialize;
use sqlx::MySqlPool;

use crate::api::guards::AuthedPrincipal;
use crate::application::artifact_backfill::{self, BackfillOptions, BackfillResult};
use crate::application::encryption::FieldEncryption;
use crate::config::AppConfig;
use crate::errors::AppResult;

pub fn routes() -> Vec<rocket::Route> {
    routes![run_backfill]
}

// ---------------------------------------------------------------------------
// Request body
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct BackfillRequest {
    /// When `true`, count eligible rows only — no mutations.  Defaults to
    /// `false` when the field is absent.
    #[serde(default)]
    pub dry_run: bool,
    /// Maximum rows per processing batch.  Clamped to `[1, 1000]`.
    /// Defaults to 100.
    pub batch_size: Option<u32>,
}

// ---------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------

/// `POST /api/v1/admin/artifact-backfill`
///
/// Runs (or dry-runs) the legacy artifact backfill, encrypting any
/// `report_runs` artifacts that pre-date Phase 6 hardened migration 019 and
/// storing a fresh per-artifact DEK so that future retention passes use
/// guaranteed cryptographic erasure instead of best-effort overwrite.
///
/// The operation is idempotent: already-encrypted rows are skipped, and rows
/// confirmed absent from disk are marked `missing_file` and not retried.
#[post("/", data = "<body>")]
pub async fn run_backfill(
    pool: &State<MySqlPool>,
    config: &State<AppConfig>,
    enc: &State<FieldEncryption>,
    body: Json<BackfillRequest>,
    principal: AuthedPrincipal,
) -> AppResult<Json<BackfillResult>> {
    let reports_storage_path = config.reports_storage_path.as_str();

    let opts = BackfillOptions {
        dry_run: body.dry_run,
        batch_size: body.batch_size.unwrap_or(100),
    };

    let result = artifact_backfill::run_backfill(
        pool.inner(),
        &principal.0,
        enc.inner(),
        reports_storage_path,
        opts,
    )
    .await?;

    Ok(Json(result))
}
