//! Scholarly backend library crate.
//!
//! This crate exposes [`build_rocket`] so both `src/main.rs` and the
//! integration tests in `backend/tests/` can assemble an identical Rocket
//! instance.

// Pulls in the full set of Rocket attribute macros (`#[get]`, `#[post]`,
// `routes!`, `catchers!`, `uri!`, etc.) crate-wide. Keeping this here
// avoids per-module `use rocket::{get, post, ...}` boilerplate.
#[macro_use]
extern crate rocket;

pub mod api;
pub mod application;
pub mod config;
pub mod domain;
pub mod errors;
pub mod infrastructure;

use rocket::{Build, Rocket};

/// Assemble the full Rocket instance: connect to MySQL, run the one-time
/// bootstrap routine (replacing seed sentinel password hashes), initialise
/// field-level encryption, and mount every route namespace.
///
/// Phase 6 additions:
/// * `FieldEncryption` added as managed state (injected into API handlers).
/// * `ReportScheduler` spawned as a background Tokio task.
pub async fn build_rocket() -> Rocket<Build> {
    let app_config = config::AppConfig::from_env();
    tracing::info!(url = %redact_db_url(&app_config.database_url), "connecting to database");

    let pool = infrastructure::database::init_pool(&app_config.database_url)
        .await
        .expect("database pool initialization failed");

    // First-boot bootstrap: hash seed passwords if still sentinels.
    // Failure here is fatal: the process exits rather than starting with
    // unusable sentinel rows (which would let the service report healthy
    // while rejecting every login attempt).
    infrastructure::bootstrap::ensure_seed_passwords(&pool)
        .await
        .expect("bootstrap failed: cannot start without valid seed password hashes");

    // Phase 6: initialise AES-256-GCM field encryption.
    let encryption =
        application::encryption::FieldEncryption::from_base64(&app_config.field_encryption_key)
            .expect("FIELD_ENCRYPTION_KEY must be a valid base64url-encoded 32-byte key");

    if encryption.is_dev_key() {
        tracing::warn!(
            "FIELD_ENCRYPTION_KEY is the insecure development default. \
             Set a real random key in production using: \
             openssl rand -base64 32 | tr '+/' '-_' | tr -d '='"
        );
    }

    // Phase 6: spawn the background report scheduler.
    // The scheduler polls every ~60 seconds for due report schedules and
    // executes them, storing generated artifact files in reports_storage_path.
    {
        let scheduler = infrastructure::scheduler::ReportScheduler::new(
            pool.clone(),
            app_config.reports_storage_path.clone(),
            encryption.clone(),
        );
        scheduler.spawn();
    }

    rocket::build()
        .manage(pool)
        .manage(app_config)
        .manage(encryption)
        // ── Auth ──
        .mount("/api/v1/auth", api::auth::routes())
        // ── Users & RBAC ──
        .mount("/api/v1/users", api::users::routes())
        .mount("/api/v1/roles", api::roles::routes())
        // ── Core domains ──
        .mount("/api/v1/journals", api::journals::routes())
        .mount("/api/v1/teaching-resources", api::teaching_resources::routes())
        .mount("/api/v1/courses", api::courses::routes())
        .mount("/api/v1/sections", api::sections::routes())
        .mount("/api/v1/attachments", api::attachments::routes())
        // ── Operations ──
        .mount("/api/v1/checkins", api::checkins::routes())
        .mount("/api/v1/metrics", api::metrics::routes())
        .mount("/api/v1/dashboards", api::dashboards::routes())
        .mount("/api/v1/reports", api::reports::routes())
        // ── Administration ──
        .mount("/api/v1/audit-logs", api::audit_logs::routes())
        .mount("/api/v1/admin/config", api::admin_config::routes())
        .mount("/api/v1/admin/retention", api::retention::routes())
        .mount("/api/v1/admin/artifact-backfill", api::artifact_backfill::routes())
        // ── Health ──
        .mount("/api/v1", api::health::routes())
        // ── Error catchers — must be last ──
        .register("/", api::catchers::catchers())
}

/// Strip the password from a database URL for safe logging.
fn redact_db_url(url: &str) -> String {
    // mysql://user:password@host:port/db  -> mysql://user:***@host:port/db
    if let Some(scheme_end) = url.find("://") {
        let rest = &url[scheme_end + 3..];
        if let Some(at) = rest.find('@') {
            let userpass = &rest[..at];
            if let Some(colon) = userpass.find(':') {
                let user = &userpass[..colon];
                let host_part = &rest[at..];
                return format!("{}://{}:***{}", &url[..scheme_end], user, host_part);
            }
        }
    }
    url.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redact_db_url_masks_password() {
        let masked = redact_db_url("mysql://user:supersecret@db:3306/scholarly");
        assert!(!masked.contains("supersecret"));
        assert!(masked.contains("user"));
        assert!(masked.contains("db:3306/scholarly"));
    }

    #[test]
    fn redact_db_url_handles_missing_password() {
        // If the URL has no password, redaction should be a no-op.
        let masked = redact_db_url("mysql://db/scholarly");
        assert_eq!(masked, "mysql://db/scholarly");
    }
}
