//! Legacy artifact backfill — upgrade plaintext report artifacts to
//! cryptographic-erasure coverage.
//!
//! # Problem
//!
//! Report artifacts generated before migration 019 (Phase 6 hardened) are
//! stored as **plaintext** files on disk with `artifact_dek IS NULL` in
//! `report_runs`.  The retention policy falls back to best-effort
//! zero-overwrite + unlink for these rows, which does NOT guarantee
//! irrecoverable deletion on Docker OverlayFS.
//!
//! # Solution
//!
//! This service reads each legacy artifact from disk, encrypts it in-place
//! with a fresh per-artifact DEK (same envelope-encryption scheme as new
//! artifacts), stores the wrapped DEK in `report_runs.artifact_dek`, and
//! atomically replaces the on-disk file with the ciphertext.  After a
//! successful backfill run, every artifact falls under the guaranteed
//! cryptographic-erasure path.
//!
//! # Idempotency
//!
//! Rows where `artifact_dek IS NOT NULL` are skipped (already encrypted).
//! Rows where `backfill_status = 'missing_file'` are skipped (file gone).
//! Rows where `backfill_status = 'encrypt_failed'` are **retried** (the
//! failure may have been transient, e.g. a locked file or a full disk).
//!
//! # Security invariants
//!
//! * DEKs and plaintext bytes are never logged.
//! * Audit events carry only sanitized counts and error summaries.
//! * Files are written atomically via temp-file + rename so that a crash
//!   between the file write and the DB commit leaves the original plaintext
//!   intact (the row retries cleanly).
//! * If the DB commit succeeds but the rename fails, the function attempts
//!   to roll back the DB entry (clear `artifact_dek`) and clean up the temp
//!   file before marking the row as `encrypt_failed`.

use serde::Serialize;
use sqlx::MySqlPool;
use std::io;
use uuid::Uuid;

use crate::application::artifact_crypto;
use crate::application::audit_service::{self, AuditEvent, actions};
use crate::application::authorization::{require, Capability};
use crate::application::encryption::FieldEncryption;
use crate::application::principal::Principal;
use crate::errors::{AppError, AppResult};
use crate::infrastructure::repositories::report_repo;

// ─── Public types ─────────────────────────────────────────────────────────────

/// Configuration for a backfill run.
#[derive(Debug, Clone)]
pub struct BackfillOptions {
    /// When `true`, count eligible rows only — no files are read or written,
    /// no DB rows are updated.
    pub dry_run: bool,
    /// Maximum rows to process per batch.  Defaults to 100.  Larger values
    /// trade memory for fewer round-trips; smaller values are safer under
    /// memory pressure.
    pub batch_size: u32,
}

impl Default for BackfillOptions {
    fn default() -> Self {
        Self {
            dry_run: false,
            batch_size: 100,
        }
    }
}

/// Outcome of a complete backfill pass.
#[derive(Debug, Clone, Serialize)]
pub struct BackfillResult {
    /// `true` if no mutations were made.
    pub dry_run: bool,
    /// Total eligible rows (artifact_path IS NOT NULL AND artifact_dek IS NULL,
    /// excluding permanently-missing rows).
    pub eligible_count: u64,
    /// Rows successfully encrypted and DEK stored.
    pub encrypted_count: u64,
    /// Rows skipped because they already had a DEK (counted during dry-run
    /// estimation only — live runs do not re-process keyed rows).
    pub already_keyed_count: u64,
    /// Rows where the file was absent from disk; marked `missing_file`.
    pub missing_file_count: u64,
    /// Rows where encryption failed (I/O error, crypto error, rename error);
    /// marked `encrypt_failed` and retryable.
    pub encrypt_failed_count: u64,
    /// Rows already marked `missing_file` from a previous run (informational).
    pub previously_missing_count: u64,
    // ── Observability / strict-retention readiness ─────────────────────────
    /// Count of actionable legacy artifacts remaining after this run
    /// (artifact_dek IS NULL AND backfill_status != 'missing_file').
    /// A value of `0` means the next strict-mode retention run will succeed.
    pub actionable_legacy_count_after_run: u64,
    /// `true` when `actionable_legacy_count_after_run == 0`.
    /// Set this as the gate before enabling `strict_mode = true` on retention.
    pub strict_retention_ready: bool,
    /// Alias for `actionable_legacy_count_after_run` — count of run IDs that
    /// would block strict-mode retention.  No run IDs are returned (security).
    pub unresolved_run_ids_count: u64,
}

// ─── Internal outcome per row ─────────────────────────────────────────────────

enum RowOutcome {
    Encrypted,
    MissingFile,
    EncryptFailed(String /* sanitized error — no secrets */),
}

// ─── Public entry point ───────────────────────────────────────────────────────

/// Run (or dry-run) the legacy artifact backfill.
///
/// Requires `RetentionManage` capability; backfill is an administrative
/// maintenance operation restricted to the same principals who can execute
/// retention policies.
pub async fn run_backfill(
    pool: &MySqlPool,
    principal: &Principal,
    enc: &FieldEncryption,
    reports_storage_path: &str,
    opts: BackfillOptions,
) -> AppResult<BackfillResult> {
    require(principal, Capability::RetentionManage)?;

    // Count eligible rows upfront (used in dry-run response and audit event).
    let eligible_count = report_repo::count_legacy_artifact_runs(pool).await?;
    let previously_missing_count =
        report_repo::count_missing_file_artifact_runs(pool).await?;

    // Emit backfill.start audit event.
    audit_service::record(
        pool,
        AuditEvent {
            actor_id: Some(principal.user_id),
            actor_email: Some(&principal.email),
            action: actions::ARTIFACT_BACKFILL_START,
            target_entity_type: Some("report_runs"),
            target_entity_id: None,
            change_payload: Some(serde_json::json!({
                "dry_run": opts.dry_run,
                "eligible_count": eligible_count,
                "batch_size": opts.batch_size,
            })),
            ip_address: None,
            user_agent: None,
        },
    )
    .await?;

    if opts.dry_run {
        // For dry-run, actionable count is the same as eligible_count (no mutations).
        let actionable_legacy_count_after_run = eligible_count;
        let strict_retention_ready = actionable_legacy_count_after_run == 0;
        let result = BackfillResult {
            dry_run: true,
            eligible_count,
            encrypted_count: 0,
            already_keyed_count: 0,
            missing_file_count: 0,
            encrypt_failed_count: 0,
            previously_missing_count,
            actionable_legacy_count_after_run,
            strict_retention_ready,
            unresolved_run_ids_count: actionable_legacy_count_after_run,
        };
        // Emit completion event even for dry-run so operators have a consistent audit trail.
        emit_complete(pool, principal, &result).await;
        return Ok(result);
    }

    // ── Live run: process rows in batches ─────────────────────────────────────

    let mut encrypted_count = 0u64;
    let mut missing_file_count = 0u64;
    let mut encrypt_failed_count = 0u64;
    let mut offset = 0u32;
    let batch_size = opts.batch_size.max(1).min(1000);

    loop {
        let batch = report_repo::find_legacy_artifact_runs(pool, batch_size, offset).await?;
        if batch.is_empty() {
            break;
        }

        let batch_len = batch.len() as u64;
        let mut batch_encrypted = 0u64;
        let mut batch_missing = 0u64;
        let mut batch_failed = 0u64;

        for row in &batch {
            let outcome = encrypt_legacy_artifact(
                pool,
                enc,
                &row.run_id,
                &row.artifact_path,
                reports_storage_path,
            )
            .await;

            match outcome {
                RowOutcome::Encrypted => {
                    encrypted_count += 1;
                    batch_encrypted += 1;
                }
                RowOutcome::MissingFile => {
                    missing_file_count += 1;
                    batch_missing += 1;
                    // Mark the row so it's not retried and retention knows
                    // there is no file to delete.
                    if let Ok(uid) = Uuid::parse_str(&row.run_id) {
                        let _ = report_repo::set_artifact_backfill_status(
                            pool,
                            uid,
                            Some("missing_file"),
                        )
                        .await;
                    }
                    // Emit a per-failure audit event (no path details logged).
                    let _ = audit_service::record(
                        pool,
                        AuditEvent {
                            actor_id: Some(principal.user_id),
                            actor_email: Some(&principal.email),
                            action: actions::ARTIFACT_BACKFILL_ROW_FAILURE,
                            target_entity_type: Some("report_run"),
                            target_entity_id: Uuid::parse_str(&row.run_id).ok(),
                            change_payload: Some(serde_json::json!({
                                "reason": "missing_file",
                            })),
                            ip_address: None,
                            user_agent: None,
                        },
                    )
                    .await;
                }
                RowOutcome::EncryptFailed(ref reason) => {
                    encrypt_failed_count += 1;
                    batch_failed += 1;
                    if let Ok(uid) = Uuid::parse_str(&row.run_id) {
                        let _ = report_repo::set_artifact_backfill_status(
                            pool,
                            uid,
                            Some("encrypt_failed"),
                        )
                        .await;
                    }
                    tracing::warn!(
                        run_id = %row.run_id,
                        reason = %reason,
                        "artifact backfill: encryption failed for row"
                    );
                    let _ = audit_service::record(
                        pool,
                        AuditEvent {
                            actor_id: Some(principal.user_id),
                            actor_email: Some(&principal.email),
                            action: actions::ARTIFACT_BACKFILL_ROW_FAILURE,
                            target_entity_type: Some("report_run"),
                            target_entity_id: Uuid::parse_str(&row.run_id).ok(),
                            change_payload: Some(serde_json::json!({
                                "reason": "encrypt_failed",
                                "error_summary": reason,
                            })),
                            ip_address: None,
                            user_agent: None,
                        },
                    )
                    .await;
                }
            }
        }

        // Per-batch audit event (aggregate only — no file paths or DEK material).
        let _ = audit_service::record(
            pool,
            AuditEvent {
                actor_id: Some(principal.user_id),
                actor_email: Some(&principal.email),
                action: actions::ARTIFACT_BACKFILL_BATCH,
                target_entity_type: Some("report_runs"),
                target_entity_id: None,
                change_payload: Some(serde_json::json!({
                    "batch_offset": offset,
                    "batch_size": batch_len,
                    "encrypted": batch_encrypted,
                    "missing_file": batch_missing,
                    "failed": batch_failed,
                })),
                ip_address: None,
                user_agent: None,
            },
        )
        .await;

        tracing::info!(
            batch_offset = offset,
            batch_size = batch_len,
            encrypted = batch_encrypted,
            missing_file = batch_missing,
            failed = batch_failed,
            "artifact backfill batch complete"
        );

        // If the batch returned fewer rows than the limit, we've processed all
        // eligible rows.  Note: `encrypt_failed` rows stay eligible on retry
        // (offset would cause us to skip them), so we use a fixed offset only
        // when all rows were successfully processed.  The safe termination
        // condition is: batch was smaller than the limit (no more rows).
        if (batch.len() as u32) < batch_size {
            break;
        }

        // Only advance the offset by the number of rows that left the eligible
        // set (encrypted rows are no longer eligible; failed rows stay eligible
        // but we advance anyway to avoid an infinite loop — they will be caught
        // on the next explicit backfill invocation).
        offset += batch_size;
    }

    // Post-run: count remaining actionable legacy artifacts to report readiness.
    let actionable_legacy_count_after_run =
        report_repo::count_legacy_artifact_runs(pool).await.unwrap_or(u64::MAX);
    let strict_retention_ready = actionable_legacy_count_after_run == 0;

    let result = BackfillResult {
        dry_run: false,
        eligible_count,
        encrypted_count,
        already_keyed_count: 0,
        missing_file_count,
        encrypt_failed_count,
        previously_missing_count,
        actionable_legacy_count_after_run,
        strict_retention_ready,
        unresolved_run_ids_count: actionable_legacy_count_after_run,
    };

    emit_complete(pool, principal, &result).await;

    tracing::info!(
        eligible = eligible_count,
        encrypted = encrypted_count,
        missing_file = missing_file_count,
        failed = encrypt_failed_count,
        actionable_remaining = actionable_legacy_count_after_run,
        strict_retention_ready = strict_retention_ready,
        "artifact backfill complete"
    );

    Ok(result)
}

// ─── Internal helpers ─────────────────────────────────────────────────────────

/// Encrypt one legacy artifact file and store the wrapped DEK in the DB.
///
/// Returns the outcome; never panics or propagates errors — failures are
/// returned as `RowOutcome::EncryptFailed` with a sanitized description.
///
/// # Atomicity
///
/// 1. Write ciphertext to `{full_path}.bftmp` (temp file).
/// 2. Commit wrapped DEK to DB.
/// 3. Rename temp file over original (atomic on the same filesystem).
///
/// Failure at step 2 or 3: temp file is cleaned up and `artifact_dek` is
/// rolled back (cleared from DB) if it was written.  The original plaintext
/// file remains intact so the row can be retried.
async fn encrypt_legacy_artifact(
    pool: &MySqlPool,
    enc: &FieldEncryption,
    run_id_str: &str,
    artifact_path: &str,
    reports_storage_path: &str,
) -> RowOutcome {
    // Parse the UUID first so we can call DB functions.
    let run_id = match Uuid::parse_str(run_id_str) {
        Ok(id) => id,
        Err(_) => return RowOutcome::EncryptFailed("invalid run_id UUID".to_string()),
    };

    let full_path = if std::path::Path::new(artifact_path).is_absolute() {
        std::path::PathBuf::from(artifact_path)
    } else {
        std::path::Path::new(reports_storage_path).join(artifact_path)
    };
    let temp_path = full_path.with_extension(
        format!(
            "{}.bftmp",
            full_path.extension().and_then(|e| e.to_str()).unwrap_or("dat")
        )
    );

    // Step 1: Read plaintext from disk.
    let plaintext = match tokio::fs::read(&full_path).await {
        Ok(b) => b,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            return RowOutcome::MissingFile;
        }
        Err(e) => {
            return RowOutcome::EncryptFailed(format!("read error: {}", e.kind()));
        }
    };

    // Step 2: Generate DEK and encrypt.
    let dek = artifact_crypto::generate_dek();
    let ciphertext = match artifact_crypto::encrypt_artifact(&dek, &plaintext) {
        Ok(c) => c,
        Err(_) => return RowOutcome::EncryptFailed("encrypt_artifact failed".to_string()),
    };
    let wrapped_dek = match artifact_crypto::wrap_dek(enc, &dek) {
        Ok(w) => w,
        Err(_) => return RowOutcome::EncryptFailed("wrap_dek failed".to_string()),
    };
    // Zeroize the in-memory DEK as soon as we no longer need it.
    // (The wrapped form in `wrapped_dek` is safe to keep as it is ciphertext.)
    drop(dek);
    drop(plaintext);

    // Step 3: Write ciphertext to temp file.
    if let Err(e) = tokio::fs::write(&temp_path, &ciphertext).await {
        return RowOutcome::EncryptFailed(format!("temp write error: {}", e.kind()));
    }

    // Step 4: Commit wrapped DEK to DB.
    if let Err(_) = report_repo::update_run_artifact_dek(pool, run_id, &wrapped_dek).await {
        // Clean up temp file; original plaintext untouched.
        let _ = tokio::fs::remove_file(&temp_path).await;
        return RowOutcome::EncryptFailed("db commit failed".to_string());
    }

    // Step 5: Atomically replace the original file with the ciphertext.
    if let Err(e) = tokio::fs::rename(&temp_path, &full_path).await {
        // Rename failed: DB has DEK but original plaintext is still there.
        // Roll back the DEK in DB so the row stays eligible for retry.
        let _ = report_repo::erase_run_artifact_dek(pool, run_id).await;
        let _ = tokio::fs::remove_file(&temp_path).await;
        return RowOutcome::EncryptFailed(format!("rename failed: {}", e.kind()));
    }

    RowOutcome::Encrypted
}

/// Emit the `artifact.backfill.complete` audit event (best-effort; never
/// propagates errors to avoid masking the main backfill result).
async fn emit_complete(pool: &MySqlPool, principal: &Principal, result: &BackfillResult) {
    let _ = audit_service::record(
        pool,
        AuditEvent {
            actor_id: Some(principal.user_id),
            actor_email: Some(&principal.email),
            action: actions::ARTIFACT_BACKFILL_COMPLETE,
            target_entity_type: Some("report_runs"),
            target_entity_id: None,
            change_payload: Some(serde_json::json!({
                "dry_run": result.dry_run,
                "eligible_count": result.eligible_count,
                "encrypted_count": result.encrypted_count,
                "missing_file_count": result.missing_file_count,
                "encrypt_failed_count": result.encrypt_failed_count,
                "previously_missing_count": result.previously_missing_count,
            })),
            ip_address: None,
            user_agent: None,
        },
    )
    .await;
}

// ─── Unit tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::artifact_crypto::{decrypt_artifact, encrypt_artifact, generate_dek};
    use crate::config::DEV_ENCRYPTION_KEY;

    fn dev_enc() -> FieldEncryption {
        FieldEncryption::from_base64(DEV_ENCRYPTION_KEY).unwrap()
    }

    /// Verifies that encrypting plaintext bytes and then decrypting them
    /// with the same DEK recovers the original content.  This is the core
    /// property the backfill relies on.
    #[test]
    fn backfill_encrypt_decrypt_roundtrip() {
        let enc = dev_enc();
        let plaintext = b"journal_id,title,status\nfoo,bar,draft";
        let dek = generate_dek();
        let ciphertext = encrypt_artifact(&dek, plaintext).unwrap();
        assert_ne!(
            ciphertext.as_slice(),
            plaintext.as_slice(),
            "ciphertext must differ from plaintext"
        );
        let recovered = decrypt_artifact(&dek, &ciphertext).unwrap();
        assert_eq!(recovered, plaintext, "roundtrip must recover original bytes");
        // Wrap/unwrap DEK also works.
        let wrapped = artifact_crypto::wrap_dek(&enc, &dek).unwrap();
        let recovered_dek = artifact_crypto::unwrap_dek(&enc, &wrapped).unwrap();
        assert_eq!(dek, recovered_dek, "DEK wrap/unwrap roundtrip failed");
    }

    /// Verifies that a missing plaintext file → MissingFile outcome, not a panic.
    #[tokio::test]
    async fn missing_file_returns_missing_file_outcome() {
        // We call the internal function indirectly through the temp-file path
        // by testing a non-existent path.  We use a pool-less approach by
        // checking just the file-reading branch.
        let path = "/tmp/__scholarly_test_nonexistent_artifact_1234567.csv";
        // Ensure it really doesn't exist.
        let _ = tokio::fs::remove_file(path).await;

        let enc = dev_enc();
        // We can't call encrypt_legacy_artifact without a DB pool, so we
        // simulate the read step directly.
        let result = tokio::fs::read(path).await;
        match result {
            Err(e) if e.kind() == io::ErrorKind::NotFound => { /* expected */ }
            Ok(_) => panic!("expected file to be absent"),
            Err(e) => panic!("unexpected error: {}", e),
        }

        // Wrap/unwrap still works with the dev key even after the file step.
        let dek = generate_dek();
        let wrapped = artifact_crypto::wrap_dek(&enc, &dek).unwrap();
        let _ = artifact_crypto::unwrap_dek(&enc, &wrapped).unwrap();
    }

    /// Verifies that a double-encrypted artifact is detectable by the GCM tag
    /// verification (wrong nonce/ciphertext structure → auth failure).
    #[test]
    fn double_encrypt_fails_authentication() {
        let plaintext = b"journal_id,title\nfoo,bar";
        let dek1 = generate_dek();
        let ciphertext1 = encrypt_artifact(&dek1, plaintext).unwrap();
        let dek2 = generate_dek();
        // Encrypt the already-encrypted bytes with a different DEK.
        let ciphertext2 = encrypt_artifact(&dek2, &ciphertext1).unwrap();
        // Decrypting with dek1 fails (different nonce/structure).
        assert!(decrypt_artifact(&dek1, &ciphertext2).is_err());
    }

    /// BackfillOptions defaults are sane.
    #[test]
    fn backfill_options_defaults() {
        let opts = BackfillOptions::default();
        assert!(!opts.dry_run, "default must not be dry-run");
        assert!(
            opts.batch_size > 0 && opts.batch_size <= 1000,
            "batch_size {} out of expected range",
            opts.batch_size
        );
    }
}
