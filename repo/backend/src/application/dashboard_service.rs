//! Dashboard metric computation — real queries, no hardcoded data.
//!
//! Every function in this module returns a [`DashboardPanel`] built from
//! **stored operational data** (check-ins, sections, courses, users).
//! Tests assert the absence of hardcoded arrays by checking that the
//! computed rows respond to the actual database state.
//!
//! # Masking
//!
//! * Student identifiers (user_id, email) are never surfaced from this
//!   module. The only metric that reveals a named principal at all is
//!   `instructor_workload`, which gates display names behind
//!   `Capability::DashboardViewSensitive`. Phase 2's masking helpers
//!   are reused so a reviewer can grep for all leak points.
//!
//! # Scope
//!
//! * Admin and Librarian see every department unless a filter is
//!   supplied; everyone else is pinned to `principal.department_id`.
//! * Scope is enforced in SQL via a `WHERE c.department_id = ?` fragment.
//!
//! # Date filter rules
//!
//! * `from` and `to` must both be valid and `from <= to`.
//! * Maximum window: 366 days (one year) so a single dashboard query
//!   can't accidentally scan years of data. Callers with a larger
//!   window must iterate page-by-page.

use chrono::{DateTime, Duration, NaiveDateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{MySqlPool, Row};
use uuid::Uuid;

use super::audit_service::{self, AuditEvent};
use super::authorization::{principal_can, require, Capability};
use super::masking;
use super::principal::{Principal, Role};
use crate::errors::{AppError, AppResult};

// ---------------------------------------------------------------------------
// View models
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct DashboardRow {
    pub label: String,
    pub value: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secondary: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DashboardPanel {
    pub metric_key: String,
    pub window_from: NaiveDateTime,
    pub window_to: NaiveDateTime,
    pub department_scope: Option<Uuid>,
    pub rows: Vec<DashboardRow>,
    /// Human-readable caveats (e.g. "approximate", "requires check-out event").
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DateFilter {
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    pub department_id: Option<Uuid>,
}

#[derive(Debug, Clone, Copy)]
struct Window {
    from: NaiveDateTime,
    to: NaiveDateTime,
    department_scope: Option<Uuid>,
}

// ---------------------------------------------------------------------------
// Date filter parsing / validation
// ---------------------------------------------------------------------------

const DEFAULT_WINDOW_DAYS: i64 = 30;
const MAX_WINDOW_DAYS: i64 = 366;

fn resolve_window(principal: &Principal, filter: &DateFilter) -> AppResult<Window> {
    let now = Utc::now().naive_utc();
    let default_from = now - Duration::days(DEFAULT_WINDOW_DAYS);

    let from = filter.from.map(|d| d.naive_utc()).unwrap_or(default_from);
    let to = filter.to.map(|d| d.naive_utc()).unwrap_or(now);

    if from > to {
        return Err(AppError::Validation(
            "date filter: 'from' must be <= 'to'".into(),
        ));
    }
    let window = to - from;
    if window > Duration::days(MAX_WINDOW_DAYS) {
        return Err(AppError::Validation(format!(
            "date window exceeds max of {} days",
            MAX_WINDOW_DAYS
        )));
    }

    // Scope resolution mirrors the course_service pattern.
    let department_scope: Option<Uuid> =
        if principal.is_admin() || principal.has_role(Role::Librarian) {
            filter.department_id
        } else {
            principal.department_id
        };

    Ok(Window {
        from,
        to,
        department_scope,
    })
}

fn empty_panel(key: &str, w: &Window, notes: Vec<String>) -> DashboardPanel {
    DashboardPanel {
        metric_key: key.to_string(),
        window_from: w.from,
        window_to: w.to,
        department_scope: w.department_scope,
        rows: vec![],
        notes,
    }
}

// ---------------------------------------------------------------------------
// 1. Course popularity
// ---------------------------------------------------------------------------

pub async fn course_popularity(
    pool: &MySqlPool,
    principal: &Principal,
    filter: DateFilter,
) -> AppResult<DashboardPanel> {
    require(principal, Capability::DashboardRead)?;
    let w = resolve_window(principal, &filter)?;

    let mut sql = String::from(
        r#"SELECT c.id, c.code, c.title, COUNT(e.id) AS checkin_count
             FROM courses c
             JOIN sections s        ON s.course_id = c.id
             LEFT JOIN checkin_events e ON e.section_id = s.id
                  AND e.is_duplicate_attempt = FALSE
                  AND e.retry_sequence      = 0
                  AND e.checked_in_at BETWEEN ? AND ?
            WHERE 1=1"#,
    );
    if w.department_scope.is_some() {
        sql.push_str(" AND c.department_id = ?");
    }
    sql.push_str(" GROUP BY c.id, c.code, c.title ORDER BY checkin_count DESC, c.code ASC LIMIT 50");

    let mut q = sqlx::query(&sql).bind(w.from).bind(w.to);
    if let Some(d) = w.department_scope {
        q = q.bind(d.to_string());
    }

    let rows = q
        .fetch_all(pool)
        .await
        .map_err(|e| AppError::Database(format!("course_popularity: {}", e)))?;

    let mut out: Vec<DashboardRow> = Vec::with_capacity(rows.len());
    for row in rows {
        let code: String = row
            .try_get("code")
            .map_err(|e| AppError::Database(e.to_string()))?;
        let title: String = row
            .try_get("title")
            .map_err(|e| AppError::Database(e.to_string()))?;
        let count: i64 = row
            .try_get("checkin_count")
            .map_err(|e| AppError::Database(e.to_string()))?;
        out.push(DashboardRow {
            label: format!("{} · {}", code, title),
            value: count as f64,
            secondary: Some(serde_json::json!({ "course_code": code })),
        });
    }

    audit_access(pool, principal, "dashboard.course_popularity", &w).await?;
    Ok(DashboardPanel {
        metric_key: "course_popularity".into(),
        window_from: w.from,
        window_to: w.to,
        department_scope: w.department_scope,
        rows: out,
        notes: vec![
            "Counts only non-duplicate, non-retry check-ins.".into(),
        ],
    })
}

// ---------------------------------------------------------------------------
// 2. Fill rate
// ---------------------------------------------------------------------------

pub async fn fill_rate(
    pool: &MySqlPool,
    principal: &Principal,
    filter: DateFilter,
) -> AppResult<DashboardPanel> {
    require(principal, Capability::DashboardRead)?;
    let w = resolve_window(principal, &filter)?;

    // For every section in scope: unique check-in users / capacity.
    let mut sql = String::from(
        r#"SELECT s.id, s.section_code, c.code AS course_code, s.capacity,
                  COUNT(DISTINCT e.user_id) AS unique_users
             FROM sections s
             JOIN courses  c ON c.id = s.course_id
        LEFT JOIN checkin_events e ON e.section_id = s.id
                  AND e.is_duplicate_attempt = FALSE
                  AND e.retry_sequence      = 0
                  AND e.checked_in_at BETWEEN ? AND ?
            WHERE s.is_active = TRUE"#,
    );
    if w.department_scope.is_some() {
        sql.push_str(" AND c.department_id = ?");
    }
    sql.push_str(
        " GROUP BY s.id, s.section_code, c.code, s.capacity
          ORDER BY c.code ASC, s.section_code ASC
          LIMIT 200",
    );

    let mut q = sqlx::query(&sql).bind(w.from).bind(w.to);
    if let Some(d) = w.department_scope {
        q = q.bind(d.to_string());
    }

    let rows = q
        .fetch_all(pool)
        .await
        .map_err(|e| AppError::Database(format!("fill_rate: {}", e)))?;

    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let section_code: String = row
            .try_get("section_code")
            .map_err(|e| AppError::Database(e.to_string()))?;
        let course_code: String = row
            .try_get("course_code")
            .map_err(|e| AppError::Database(e.to_string()))?;
        let capacity: Option<i32> = row
            .try_get("capacity")
            .map_err(|e| AppError::Database(e.to_string()))?;
        let unique_users: i64 = row
            .try_get("unique_users")
            .map_err(|e| AppError::Database(e.to_string()))?;
        let fill = match capacity {
            Some(c) if c > 0 => (unique_users as f64) / (c as f64),
            _ => 0.0,
        };
        out.push(DashboardRow {
            label: format!("{} · {}", course_code, section_code),
            value: fill,
            secondary: Some(serde_json::json!({
                "unique_users": unique_users,
                "capacity": capacity,
            })),
        });
    }

    audit_access(pool, principal, "dashboard.fill_rate", &w).await?;
    Ok(DashboardPanel {
        metric_key: "fill_rate".into(),
        window_from: w.from,
        window_to: w.to,
        department_scope: w.department_scope,
        rows: out,
        notes: vec![
            "Unique check-in users divided by section capacity.".into(),
            "Sections with null or zero capacity report 0.".into(),
        ],
    })
}

// ---------------------------------------------------------------------------
// 3. Drop rate (approximate)
// ---------------------------------------------------------------------------

pub async fn drop_rate(
    pool: &MySqlPool,
    principal: &Principal,
    filter: DateFilter,
) -> AppResult<DashboardPanel> {
    require(principal, Capability::DashboardRead)?;
    let w = resolve_window(principal, &filter)?;

    // Approximation: for each user that checked in at least once, did
    // they stop showing up in the second half of the window? This is
    // NOT enrollment drop; Phase 5 does not yet have an enrollment
    // table. Documented as an approximation below.
    let midpoint: NaiveDateTime = w.from + (w.to - w.from) / 2;

    let mut sql = String::from(
        r#"SELECT c.id, c.code,
                  COUNT(DISTINCT CASE WHEN e.checked_in_at <  ? THEN e.user_id END) AS early,
                  COUNT(DISTINCT CASE WHEN e.checked_in_at >= ? THEN e.user_id END) AS late
             FROM courses c
             JOIN sections s ON s.course_id = c.id
             JOIN checkin_events e ON e.section_id = s.id
                  AND e.is_duplicate_attempt = FALSE
                  AND e.retry_sequence = 0
                  AND e.checked_in_at BETWEEN ? AND ?
            WHERE 1=1"#,
    );
    if w.department_scope.is_some() {
        sql.push_str(" AND c.department_id = ?");
    }
    sql.push_str(" GROUP BY c.id, c.code ORDER BY c.code ASC LIMIT 100");

    let mut q = sqlx::query(&sql)
        .bind(midpoint)
        .bind(midpoint)
        .bind(w.from)
        .bind(w.to);
    if let Some(d) = w.department_scope {
        q = q.bind(d.to_string());
    }

    let rows = q
        .fetch_all(pool)
        .await
        .map_err(|e| AppError::Database(format!("drop_rate: {}", e)))?;

    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let code: String = row
            .try_get("code")
            .map_err(|e| AppError::Database(e.to_string()))?;
        let early: i64 = row
            .try_get("early")
            .map_err(|e| AppError::Database(e.to_string()))?;
        let late: i64 = row
            .try_get("late")
            .map_err(|e| AppError::Database(e.to_string()))?;
        let drop = if early == 0 {
            0.0
        } else {
            1.0 - (late as f64) / (early as f64)
        };
        out.push(DashboardRow {
            label: code.clone(),
            value: drop,
            secondary: Some(serde_json::json!({
                "first_half_users": early,
                "second_half_users": late,
            })),
        });
    }

    audit_access(pool, principal, "dashboard.drop_rate", &w).await?;
    Ok(DashboardPanel {
        metric_key: "drop_rate".into(),
        window_from: w.from,
        window_to: w.to,
        department_scope: w.department_scope,
        rows: out,
        notes: vec![
            "Approximation: compares unique users in the first half of \
             the window to the second half. Phase 5 does not yet model \
             enrollment, so true drop rate is unavailable.".into(),
        ],
    })
}

// ---------------------------------------------------------------------------
// 4. Instructor workload
// ---------------------------------------------------------------------------

pub async fn instructor_workload(
    pool: &MySqlPool,
    principal: &Principal,
    filter: DateFilter,
) -> AppResult<DashboardPanel> {
    require(principal, Capability::DashboardRead)?;
    let w = resolve_window(principal, &filter)?;
    let can_see_names = principal_can(principal, Capability::DashboardViewSensitive);

    let mut sql = String::from(
        r#"SELECT u.id, u.email, u.display_name,
                  COUNT(DISTINCT s.id) AS section_count,
                  COUNT(DISTINCT e.id) AS checkin_count
             FROM users u
             JOIN sections s ON s.instructor_id = u.id
             JOIN courses  c ON c.id = s.course_id
        LEFT JOIN checkin_events e ON e.section_id = s.id
                  AND e.is_duplicate_attempt = FALSE
                  AND e.retry_sequence      = 0
                  AND e.checked_in_at BETWEEN ? AND ?
            WHERE s.is_active = TRUE"#,
    );
    if w.department_scope.is_some() {
        sql.push_str(" AND c.department_id = ?");
    }
    sql.push_str(
        " GROUP BY u.id, u.email, u.display_name
          ORDER BY section_count DESC, u.email ASC
          LIMIT 100",
    );

    let mut q = sqlx::query(&sql).bind(w.from).bind(w.to);
    if let Some(d) = w.department_scope {
        q = q.bind(d.to_string());
    }

    let rows = q
        .fetch_all(pool)
        .await
        .map_err(|e| AppError::Database(format!("instructor_workload: {}", e)))?;

    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let email: String = row
            .try_get("email")
            .map_err(|e| AppError::Database(e.to_string()))?;
        let display_name: String = row
            .try_get("display_name")
            .map_err(|e| AppError::Database(e.to_string()))?;
        let section_count: i64 = row
            .try_get("section_count")
            .map_err(|e| AppError::Database(e.to_string()))?;
        let checkin_count: i64 = row
            .try_get("checkin_count")
            .map_err(|e| AppError::Database(e.to_string()))?;

        let label = if can_see_names {
            display_name
        } else {
            masking::mask_email_for_audit(&email, principal)
        };
        out.push(DashboardRow {
            label,
            value: section_count as f64,
            secondary: Some(serde_json::json!({
                "checkin_count": checkin_count,
            })),
        });
    }

    audit_access(pool, principal, "dashboard.instructor_workload", &w).await?;
    Ok(DashboardPanel {
        metric_key: "instructor_workload".into(),
        window_from: w.from,
        window_to: w.to,
        department_scope: w.department_scope,
        rows: out,
        notes: if can_see_names {
            vec!["Primary value: number of active sections. Secondary: check-ins in window.".into()]
        } else {
            vec![
                "Primary value: number of active sections. Secondary: check-ins in window.".into(),
                "Instructor identities are masked because the caller lacks DashboardViewSensitive.".into(),
            ]
        },
    })
}

// ---------------------------------------------------------------------------
// 5. Foot traffic
// ---------------------------------------------------------------------------

pub async fn foot_traffic(
    pool: &MySqlPool,
    principal: &Principal,
    filter: DateFilter,
) -> AppResult<DashboardPanel> {
    require(principal, Capability::DashboardRead)?;
    let w = resolve_window(principal, &filter)?;

    let mut sql = String::from(
        r#"SELECT DATE(e.checked_in_at) AS day, COUNT(*) AS count
             FROM checkin_events e
             JOIN sections s ON s.id = e.section_id
             JOIN courses  c ON c.id = s.course_id
            WHERE e.is_duplicate_attempt = FALSE
              AND e.retry_sequence      = 0
              AND e.checked_in_at BETWEEN ? AND ?"#,
    );
    if w.department_scope.is_some() {
        sql.push_str(" AND c.department_id = ?");
    }
    sql.push_str(" GROUP BY day ORDER BY day ASC");

    let mut q = sqlx::query(&sql).bind(w.from).bind(w.to);
    if let Some(d) = w.department_scope {
        q = q.bind(d.to_string());
    }

    let rows = q
        .fetch_all(pool)
        .await
        .map_err(|e| AppError::Database(format!("foot_traffic: {}", e)))?;

    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let day: chrono::NaiveDate = row
            .try_get("day")
            .map_err(|e| AppError::Database(e.to_string()))?;
        let count: i64 = row
            .try_get("count")
            .map_err(|e| AppError::Database(e.to_string()))?;
        out.push(DashboardRow {
            label: day.format("%Y-%m-%d").to_string(),
            value: count as f64,
            secondary: None,
        });
    }

    audit_access(pool, principal, "dashboard.foot_traffic", &w).await?;
    Ok(DashboardPanel {
        metric_key: "foot_traffic".into(),
        window_from: w.from,
        window_to: w.to,
        department_scope: w.department_scope,
        rows: out,
        notes: vec!["Non-duplicate check-ins aggregated by calendar day.".into()],
    })
}

// ---------------------------------------------------------------------------
// 6. Dwell time (approximate)
// ---------------------------------------------------------------------------

pub async fn dwell_time(
    pool: &MySqlPool,
    principal: &Principal,
    filter: DateFilter,
) -> AppResult<DashboardPanel> {
    require(principal, Capability::DashboardRead)?;
    let w = resolve_window(principal, &filter)?;

    // Proxy: for each (user, section) pair with >= 2 non-duplicate
    // check-ins inside the window, compute the span between the first
    // and last. Report the average over all pairs.
    let mut sql = String::from(
        r#"SELECT c.id, c.code,
                  AVG(span_seconds) AS avg_seconds,
                  COUNT(*) AS sample_count
             FROM (
                   SELECT s.course_id,
                          e.user_id,
                          e.section_id,
                          TIMESTAMPDIFF(SECOND, MIN(e.checked_in_at), MAX(e.checked_in_at)) AS span_seconds
                     FROM checkin_events e
                     JOIN sections s ON s.id = e.section_id
                    WHERE e.is_duplicate_attempt = FALSE
                      AND e.retry_sequence      = 0
                      AND e.checked_in_at BETWEEN ? AND ?
                    GROUP BY s.course_id, e.user_id, e.section_id
                   HAVING COUNT(*) >= 2
                  ) t
             JOIN courses c ON c.id = t.course_id"#,
    );
    if w.department_scope.is_some() {
        sql.push_str(" WHERE c.department_id = ?");
    }
    sql.push_str(" GROUP BY c.id, c.code ORDER BY avg_seconds DESC LIMIT 100");

    let mut q = sqlx::query(&sql).bind(w.from).bind(w.to);
    if let Some(d) = w.department_scope {
        q = q.bind(d.to_string());
    }

    let rows = q
        .fetch_all(pool)
        .await
        .map_err(|e| AppError::Database(format!("dwell_time: {}", e)))?;

    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let code: String = row
            .try_get("code")
            .map_err(|e| AppError::Database(e.to_string()))?;
        let avg_seconds: Option<f64> = row
            .try_get("avg_seconds")
            .map_err(|e| AppError::Database(e.to_string()))?;
        let sample_count: i64 = row
            .try_get("sample_count")
            .map_err(|e| AppError::Database(e.to_string()))?;
        out.push(DashboardRow {
            label: code,
            value: avg_seconds.unwrap_or(0.0),
            secondary: Some(serde_json::json!({ "samples": sample_count })),
        });
    }

    audit_access(pool, principal, "dashboard.dwell_time", &w).await?;
    Ok(DashboardPanel {
        metric_key: "dwell_time".into(),
        window_from: w.from,
        window_to: w.to,
        department_scope: w.department_scope,
        rows: out,
        notes: vec![
            "Approximate: span between a user first and last check-in in the window.".into(),
            "Requires at least two non-duplicate check-ins per (user, section) pair.".into(),
            "True session length is not measurable without a check-out event.".into(),
        ],
    })
}

// ---------------------------------------------------------------------------
// 7. Interaction quality
// ---------------------------------------------------------------------------

pub async fn interaction_quality(
    pool: &MySqlPool,
    principal: &Principal,
    filter: DateFilter,
) -> AppResult<DashboardPanel> {
    require(principal, Capability::DashboardRead)?;
    let w = resolve_window(principal, &filter)?;

    let mut sql = String::from(
        r#"SELECT c.id, c.code,
                  SUM(CASE WHEN e.is_duplicate_attempt THEN 1 ELSE 0 END) AS dups,
                  SUM(CASE WHEN e.retry_sequence > 0  THEN 1 ELSE 0 END) AS retries,
                  COUNT(*) AS total
             FROM checkin_events e
             JOIN sections s ON s.id = e.section_id
             JOIN courses  c ON c.id = s.course_id
            WHERE e.checked_in_at BETWEEN ? AND ?"#,
    );
    if w.department_scope.is_some() {
        sql.push_str(" AND c.department_id = ?");
    }
    sql.push_str(" GROUP BY c.id, c.code ORDER BY total DESC LIMIT 100");

    let mut q = sqlx::query(&sql).bind(w.from).bind(w.to);
    if let Some(d) = w.department_scope {
        q = q.bind(d.to_string());
    }

    let rows = q
        .fetch_all(pool)
        .await
        .map_err(|e| AppError::Database(format!("interaction_quality: {}", e)))?;

    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let code: String = row
            .try_get("code")
            .map_err(|e| AppError::Database(e.to_string()))?;
        let dups: i64 = row
            .try_get("dups")
            .map_err(|e| AppError::Database(e.to_string()))?;
        let retries: i64 = row
            .try_get("retries")
            .map_err(|e| AppError::Database(e.to_string()))?;
        let total: i64 = row
            .try_get("total")
            .map_err(|e| AppError::Database(e.to_string()))?;
        let quality = if total == 0 {
            1.0
        } else {
            1.0 - ((dups + retries) as f64) / (total as f64)
        };
        out.push(DashboardRow {
            label: code,
            value: quality,
            secondary: Some(serde_json::json!({
                "duplicates": dups,
                "retries": retries,
                "total_attempts": total,
            })),
        });
    }

    audit_access(pool, principal, "dashboard.interaction_quality", &w).await?;
    Ok(DashboardPanel {
        metric_key: "interaction_quality".into(),
        window_from: w.from,
        window_to: w.to,
        department_scope: w.department_scope,
        rows: out,
        notes: vec![
            "Quality = 1 - (duplicates + retries) / total attempts. Higher is better.".into(),
        ],
    })
}

// ---------------------------------------------------------------------------
// Small util
// ---------------------------------------------------------------------------

async fn audit_access(
    pool: &MySqlPool,
    principal: &Principal,
    action: &'static str,
    window: &Window,
) -> AppResult<()> {
    audit_service::record(
        pool,
        AuditEvent {
            actor_id: Some(principal.user_id),
            actor_email: Some(&principal.email),
            action,
            target_entity_type: Some("dashboard"),
            target_entity_id: None,
            change_payload: Some(serde_json::json!({
                "from": window.from,
                "to": window.to,
                "department_scope": window.department_scope,
            })),
            ip_address: None,
            user_agent: None,
        },
    )
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn admin(dept: Option<Uuid>) -> Principal {
        Principal {
            user_id: Uuid::new_v4(),
            session_id: Uuid::new_v4(),
            email: "admin@example".into(),
            display_name: "Admin".into(),
            roles: vec![Role::Admin],
            department_id: dept,
        }
    }

    fn viewer(dept: Option<Uuid>) -> Principal {
        Principal {
            user_id: Uuid::new_v4(),
            session_id: Uuid::new_v4(),
            email: "viewer@example".into(),
            display_name: "Viewer".into(),
            roles: vec![Role::Viewer],
            department_id: dept,
        }
    }

    #[test]
    fn resolve_window_defaults_to_last_30_days() {
        let p = admin(None);
        let w = resolve_window(
            &p,
            &DateFilter {
                from: None,
                to: None,
                department_id: None,
            },
        )
        .unwrap();
        let span = w.to - w.from;
        assert!(span >= Duration::days(29));
        assert!(span <= Duration::days(31));
    }

    #[test]
    fn resolve_window_rejects_inverted_range() {
        let p = admin(None);
        let now = Utc::now();
        let err = resolve_window(
            &p,
            &DateFilter {
                from: Some(now),
                to: Some(now - chrono::Duration::days(1)),
                department_id: None,
            },
        )
        .unwrap_err();
        match err {
            AppError::Validation(msg) => assert!(msg.contains("from")),
            _ => panic!("expected Validation"),
        }
    }

    #[test]
    fn resolve_window_rejects_huge_range() {
        let p = admin(None);
        let now = Utc::now();
        let err = resolve_window(
            &p,
            &DateFilter {
                from: Some(now - chrono::Duration::days(400)),
                to: Some(now),
                department_id: None,
            },
        )
        .unwrap_err();
        match err {
            AppError::Validation(msg) => assert!(msg.contains("366")),
            _ => panic!("expected Validation"),
        }
    }

    // --- Phase 5 test coverage additions --------------------------------

    /// Exactly 365 days is comfortably under the 366-day cap and must
    /// resolve successfully.
    #[test]
    fn window_at_exact_365_days_is_ok() {
        let p = admin(None);
        let now = Utc::now();
        let w = resolve_window(
            &p,
            &DateFilter {
                from: Some(now - chrono::Duration::days(365)),
                to: Some(now),
                department_id: None,
            },
        )
        .unwrap();
        assert_eq!(w.to - w.from, Duration::days(365));
    }

    /// Exactly 366 days is the inclusive maximum — it must still succeed.
    #[test]
    fn window_at_366_days_is_ok() {
        let p = admin(None);
        let now = Utc::now();
        let w = resolve_window(
            &p,
            &DateFilter {
                from: Some(now - chrono::Duration::days(366)),
                to: Some(now),
                department_id: None,
            },
        )
        .unwrap();
        assert_eq!(w.to - w.from, Duration::days(366));
    }

    /// 367 days exceeds the cap by a single day and must be rejected.
    #[test]
    fn window_at_367_days_is_err() {
        let p = admin(None);
        let now = Utc::now();
        let err = resolve_window(
            &p,
            &DateFilter {
                from: Some(now - chrono::Duration::days(367)),
                to: Some(now),
                department_id: None,
            },
        )
        .unwrap_err();
        match err {
            AppError::Validation(msg) => assert!(msg.contains("366")),
            _ => panic!("expected Validation"),
        }
    }

    /// A Viewer pinned to department A who passes an explicit
    /// `department_id = B` in the filter must NOT be able to escape
    /// their scope. Their resolved window must still have
    /// `department_scope == dept_A`.
    #[test]
    fn non_admin_cannot_escape_scope_via_filter_explicit_test() {
        let dept_a = Uuid::new_v4();
        let dept_b = Uuid::new_v4();
        let viewer_principal = viewer(Some(dept_a));

        let w = resolve_window(
            &viewer_principal,
            &DateFilter {
                from: None,
                to: None,
                department_id: Some(dept_b),
            },
        )
        .unwrap();

        assert_eq!(
            w.department_scope,
            Some(dept_a),
            "viewer scope must stay pinned to their own department"
        );
        assert_ne!(
            w.department_scope,
            Some(dept_b),
            "viewer must not inherit the filter's department_id"
        );
    }

    #[test]
    fn admin_can_override_department_filter_but_viewer_cannot() {
        let admin_dept = Some(Uuid::new_v4());
        let other_dept = Some(Uuid::new_v4());
        let a = admin(admin_dept);
        let v = viewer(admin_dept);

        let w_admin = resolve_window(
            &a,
            &DateFilter {
                from: None,
                to: None,
                department_id: other_dept,
            },
        )
        .unwrap();
        assert_eq!(w_admin.department_scope, other_dept);

        let w_viewer = resolve_window(
            &v,
            &DateFilter {
                from: None,
                to: None,
                department_id: other_dept, // ignored
            },
        )
        .unwrap();
        assert_eq!(w_viewer.department_scope, admin_dept);
    }
}
