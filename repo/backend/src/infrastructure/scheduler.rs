//! Background report scheduler.
//!
//! Runs as a long-lived tokio task. Every 60 seconds it queries for due
//! report schedules (active, `next_run_at <= NOW()`) and executes each one
//! in-process via `report_service::trigger_run_internal`.
//!
//! On startup the scheduler also fixes any active schedules whose
//! `next_run_at` is NULL by computing the next occurrence from their cron
//! expression.

use chrono::Utc;
use sqlx::MySqlPool;
use tokio::time::{interval, Duration};
use uuid::Uuid;

use crate::application::encryption::FieldEncryption;
use crate::application::report_service;
use crate::infrastructure::repositories::report_repo;

/// Owned scheduler state — all fields are `Clone`/`Send`/`Sync`.
pub struct ReportScheduler {
    pool: MySqlPool,
    reports_storage_path: String,
    encryption: FieldEncryption,
}

impl ReportScheduler {
    pub fn new(
        pool: MySqlPool,
        reports_storage_path: String,
        encryption: FieldEncryption,
    ) -> Self {
        Self {
            pool,
            reports_storage_path,
            encryption,
        }
    }

    /// Spawn the scheduler as a background tokio task.
    pub fn spawn(self) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move { self.run().await })
    }

    async fn run(&self) {
        // ── Phase 1: fix NULL next_run_at for active schedules ─────────────
        self.fix_null_next_run_at().await;

        // ── Phase 2: periodic execution loop ──────────────────────────────
        let mut ticker = interval(Duration::from_secs(60));
        // Consume the initial tick which fires immediately.
        ticker.tick().await;

        loop {
            ticker.tick().await;
            self.process_due_schedules().await;
        }
    }

    /// Recompute `next_run_at` for any active schedule that has `NULL`.
    async fn fix_null_next_run_at(&self) {
        match report_repo::find_active_schedules_without_next_run(&self.pool).await {
            Ok(schedules) => {
                for sched in schedules {
                    let id = match Uuid::parse_str(&sched.id) {
                        Ok(u) => u,
                        Err(e) => {
                            tracing::error!(
                                schedule_id = %sched.id,
                                "scheduler: bad UUID in schedule: {}",
                                e
                            );
                            continue;
                        }
                    };
                    match report_service::compute_next_run(&sched.cron_expression) {
                        Ok(Some(next)) => {
                            if let Err(e) =
                                report_repo::update_schedule_next_run(&self.pool, id, next).await
                            {
                                tracing::error!(
                                    schedule_id = %id,
                                    "scheduler: failed to update next_run_at: {}",
                                    e
                                );
                            } else {
                                tracing::info!(
                                    schedule_id = %id,
                                    next_run_at = %next,
                                    "scheduler: initialized next_run_at"
                                );
                            }
                        }
                        Ok(None) => {
                            tracing::warn!(
                                schedule_id = %id,
                                "scheduler: cron expression has no future occurrence, skipping"
                            );
                        }
                        Err(e) => {
                            tracing::error!(
                                schedule_id = %id,
                                "scheduler: invalid cron expression: {}",
                                e
                            );
                        }
                    }
                }
            }
            Err(e) => {
                tracing::error!("scheduler: failed to query null-next_run_at schedules: {}", e);
            }
        }
    }

    /// Find all due schedules and trigger a run for each.
    async fn process_due_schedules(&self) {
        let due_schedules = match report_repo::find_due_schedules(&self.pool).await {
            Ok(s) => s,
            Err(e) => {
                tracing::error!("scheduler: failed to query due schedules: {}", e);
                return;
            }
        };

        if due_schedules.is_empty() {
            return;
        }

        tracing::info!(count = due_schedules.len(), "scheduler: processing due schedules");

        for sched in &due_schedules {
            let schedule_id = match Uuid::parse_str(&sched.id) {
                Ok(u) => u,
                Err(e) => {
                    tracing::error!(
                        schedule_id = %sched.id,
                        "scheduler: bad UUID in schedule: {}",
                        e
                    );
                    continue;
                }
            };

            let now = Utc::now().naive_utc();

            // Compute the next occurrence BEFORE running so even if the run
            // fails we advance the schedule pointer.
            let next_run = match report_service::compute_next_run(&sched.cron_expression) {
                Ok(n) => n,
                Err(e) => {
                    tracing::error!(
                        schedule_id = %schedule_id,
                        "scheduler: invalid cron expression, deactivating: {}",
                        e
                    );
                    // Advance last_run_at but leave next_run_at NULL so the
                    // startup fixer will try again (or the operator corrects it).
                    let _ = report_repo::update_schedule_ran(
                        &self.pool,
                        schedule_id,
                        now,
                        None,
                    )
                    .await;
                    continue;
                }
            };

            tracing::info!(
                schedule_id = %schedule_id,
                report_id = %sched.report_id,
                "scheduler: triggering run"
            );

            match report_service::trigger_run_internal(
                &self.pool,
                sched,
                &self.reports_storage_path,
                &self.encryption,
            )
            .await
            {
                Ok((run_id, success)) => {
                    if success {
                        tracing::info!(
                            schedule_id = %schedule_id,
                            run_id = %run_id,
                            "scheduler: run completed successfully"
                        );
                    } else {
                        tracing::warn!(
                            schedule_id = %schedule_id,
                            run_id = %run_id,
                            "scheduler: run completed with errors (artifact may not have been written)"
                        );
                    }
                }
                Err(e) => {
                    tracing::error!(
                        schedule_id = %schedule_id,
                        "scheduler: run trigger failed: {}",
                        e
                    );
                }
            }

            // Always advance the schedule pointer regardless of run outcome.
            if let Err(e) =
                report_repo::update_schedule_ran(&self.pool, schedule_id, now, next_run).await
            {
                tracing::error!(
                    schedule_id = %schedule_id,
                    "scheduler: failed to update schedule after run: {}",
                    e
                );
            }
        }
    }
}
