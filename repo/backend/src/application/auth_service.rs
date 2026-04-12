//! Authentication orchestrator.
//!
//! Ties together:
//! * [`super::password`]     — hashing & verification
//! * [`super::lockout`]      — failed-attempt tracking
//! * [`super::session`]      — opaque bearer-token sessions
//! * [`super::audit_service`] — immutable event log
//!
//! Every public method here is the *only* legitimate entry point for the
//! corresponding auth flow. Route handlers never touch the individual
//! submodules directly.

use serde::{Deserialize, Serialize};
use sqlx::{MySqlPool, Row};
use uuid::Uuid;

use super::audit_service::{self, actions, AuditEvent};
use super::lockout;
use super::password;
use super::principal::{Principal, Role};
use super::session::{self, IssuedSession};
use crate::config::AppConfig;
use crate::errors::{AppError, AppResult};

/// Login request body.
#[derive(Debug, Deserialize)]
pub struct LoginInput {
    pub email: String,
    pub password: String,
}

/// Login response body. `token` is delivered exactly once.
#[derive(Debug, Serialize)]
pub struct LoginOutput {
    pub token: String,
    pub expires_at: chrono::NaiveDateTime,
    pub principal: Principal,
}

/// Contextual data drawn from the HTTP request for auditing.
#[derive(Debug, Clone, Default)]
pub struct RequestContext<'a> {
    pub ip_address: Option<&'a str>,
    pub user_agent: Option<&'a str>,
}

/// Execute a login attempt end-to-end.
///
/// 1. Lockout check (email scope).
/// 2. Load user + password hash.
/// 3. Verify password (constant time).
/// 4. On failure: record attempt, audit, return Unauthorized.
/// 5. On success: clear failures, create session, audit, return token.
pub async fn login(
    pool: &MySqlPool,
    config: &AppConfig,
    input: LoginInput,
    ctx: RequestContext<'_>,
) -> AppResult<LoginOutput> {
    let email = input.email.trim().to_lowercase();

    // Step 1: lockout gate — check BEFORE touching password logic so a
    // locked account never leaks timing about whether the password would
    // have worked.
    if let Err(e) = lockout::enforce_lockout(pool, &email, config).await {
        // Audit the locked attempt regardless of whether the user exists.
        let _ = audit_service::record(
            pool,
            AuditEvent {
                actor_id: None,
                actor_email: Some(&email),
                action: actions::LOGIN_LOCKED,
                target_entity_type: Some("user"),
                target_entity_id: None,
                change_payload: None,
                ip_address: ctx.ip_address,
                user_agent: ctx.user_agent,
            },
        )
        .await;
        return Err(e);
    }

    // Step 2: load the user row (if any).
    let user = load_user_with_hash(pool, &email).await?;

    let Some(user) = user else {
        return Err(record_failure_and_audit(
            pool,
            &email,
            ctx.ip_address,
            ctx.user_agent,
            "unknown_user",
        )
        .await);
    };

    if user.status != "active" {
        return Err(record_failure_and_audit(
            pool,
            &email,
            ctx.ip_address,
            ctx.user_agent,
            "inactive_status",
        )
        .await);
    }

    // Step 3: verify password in constant time.
    let password_ok = match password::verify_password(&input.password, &user.password_hash) {
        Ok(ok) => ok,
        Err(AppError::Internal(msg)) if msg.contains("bootstrap") => {
            return Err(record_failure_and_audit(
                pool,
                &email,
                ctx.ip_address,
                ctx.user_agent,
                "bootstrap_incomplete",
            )
            .await);
        }
        Err(e) => return Err(e),
    };
    if !password_ok {
        return Err(record_failure_and_audit(
            pool,
            &email,
            ctx.ip_address,
            ctx.user_agent,
            "invalid_password",
        )
        .await);
    }

    // Step 4: success path — clear failures, create session, audit.
    lockout::clear_failures(pool, &email).await?;

    let IssuedSession {
        session_id,
        raw_token,
        expires_at,
    } = session::create_session(
        pool,
        user.id,
        ctx.ip_address,
        ctx.user_agent,
        config.jwt_expiration_hours as i64,
    )
    .await?;

    // Load roles now so the Principal is complete.
    let roles = load_user_roles(pool, user.id).await?;
    let principal = Principal {
        user_id: user.id,
        session_id,
        email: user.email.clone(),
        display_name: user.display_name.clone(),
        roles,
        department_id: user.department_id,
    };

    audit_service::record(
        pool,
        AuditEvent {
            actor_id: Some(user.id),
            actor_email: Some(&user.email),
            action: actions::LOGIN_SUCCESS,
            target_entity_type: Some("session"),
            target_entity_id: Some(session_id),
            change_payload: None,
            ip_address: ctx.ip_address,
            user_agent: ctx.user_agent,
        },
    )
    .await?;

    Ok(LoginOutput {
        token: raw_token,
        expires_at,
        principal,
    })
}

/// Revoke the current session and audit the logout.
pub async fn logout(
    pool: &MySqlPool,
    principal: &Principal,
    ctx: RequestContext<'_>,
) -> AppResult<()> {
    session::revoke_session(pool, principal.session_id).await?;
    audit_service::record(
        pool,
        AuditEvent {
            actor_id: Some(principal.user_id),
            actor_email: Some(&principal.email),
            action: actions::LOGOUT,
            target_entity_type: Some("session"),
            target_entity_id: Some(principal.session_id),
            change_payload: None,
            ip_address: ctx.ip_address,
            user_agent: ctx.user_agent,
        },
    )
    .await?;
    Ok(())
}

/// Change a user's password. Must be done by the user themself or an admin;
/// the caller is responsible for authorizing that. All of the user's sessions
/// are revoked as a side-effect so stale tokens become unusable immediately.
pub async fn change_password(
    pool: &MySqlPool,
    actor: &Principal,
    target_user_id: Uuid,
    new_password: &str,
) -> AppResult<()> {
    password::validate_password_policy(new_password)?;
    let hash = password::hash_password(new_password)?;

    sqlx::query("UPDATE users SET password_hash = ? WHERE id = ?")
        .bind(&hash)
        .bind(target_user_id.to_string())
        .execute(pool)
        .await
        .map_err(|e| AppError::Database(format!("password update: {}", e)))?;

    session::revoke_all_for_user(pool, target_user_id).await?;

    audit_service::record(
        pool,
        AuditEvent {
            actor_id: Some(actor.user_id),
            actor_email: Some(&actor.email),
            action: actions::PASSWORD_CHANGE,
            target_entity_type: Some("user"),
            target_entity_id: Some(target_user_id),
            change_payload: None,
            ip_address: None,
            user_agent: None,
        },
    )
    .await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Failure path — records the attempt, audits the failure, and returns the
// `AppError::Unauthorized` value for the caller to wrap in `Err(...)`.
// ---------------------------------------------------------------------------
async fn record_failure_and_audit(
    pool: &MySqlPool,
    email: &str,
    ip_address: Option<&str>,
    user_agent: Option<&str>,
    reason: &'static str,
) -> AppError {
    // Errors here are intentionally swallowed: the primary goal is to
    // return Unauthorized; we log tracking failures rather than mask the
    // login failure with an internal error.
    if let Err(e) = lockout::record_failure(pool, email, ip_address, reason).await {
        tracing::warn!("record_failure: {}", e);
    }
    if let Err(e) = audit_service::record(
        pool,
        AuditEvent {
            actor_id: None,
            actor_email: Some(email),
            action: actions::LOGIN_FAILURE,
            target_entity_type: Some("user"),
            target_entity_id: None,
            change_payload: Some(serde_json::json!({ "reason": reason })),
            ip_address,
            user_agent,
        },
    )
    .await
    {
        tracing::warn!("audit login_failure: {}", e);
    }
    AppError::Unauthorized
}

// ---------------------------------------------------------------------------
// Internal loaders — kept here rather than in a repo module for locality.
// Phase 3 can migrate them into `infrastructure::repositories::user_repo`.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct UserWithHash {
    id: Uuid,
    email: String,
    display_name: String,
    password_hash: String,
    status: String,
    department_id: Option<Uuid>,
}

async fn load_user_with_hash(
    pool: &MySqlPool,
    email: &str,
) -> AppResult<Option<UserWithHash>> {
    let row = sqlx::query(
        r#"
        SELECT id, email, display_name, password_hash, status, department_id
          FROM users
         WHERE email = ?
         LIMIT 1
        "#,
    )
    .bind(email)
    .fetch_optional(pool)
    .await
    .map_err(|e| AppError::Database(format!("load_user: {}", e)))?;

    let Some(row) = row else { return Ok(None) };
    let id: String = row.try_get("id").map_err(|e| AppError::Database(e.to_string()))?;
    let email_s: String = row
        .try_get("email")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let display_name: String = row
        .try_get("display_name")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let password_hash: String = row
        .try_get("password_hash")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let status: String = row
        .try_get("status")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let department_id: Option<String> = row
        .try_get("department_id")
        .map_err(|e| AppError::Database(e.to_string()))?;

    Ok(Some(UserWithHash {
        id: Uuid::parse_str(&id).map_err(|e| AppError::Database(e.to_string()))?,
        email: email_s,
        display_name,
        password_hash,
        status,
        department_id: department_id
            .as_deref()
            .and_then(|s| Uuid::parse_str(s).ok()),
    }))
}

/// Load a user's roles by user_id. Returns an empty vec if the user has none.
pub async fn load_user_roles(pool: &MySqlPool, user_id: Uuid) -> AppResult<Vec<Role>> {
    let rows = sqlx::query(
        r#"
        SELECT r.name
          FROM user_roles ur
          JOIN roles r ON r.id = ur.role_id
         WHERE ur.user_id = ?
        "#,
    )
    .bind(user_id.to_string())
    .fetch_all(pool)
    .await
    .map_err(|e| AppError::Database(format!("load_user_roles: {}", e)))?;

    let mut out = Vec::new();
    for row in rows {
        let name: String = row
            .try_get("name")
            .map_err(|e| AppError::Database(e.to_string()))?;
        if let Some(r) = Role::from_db_name(&name) {
            out.push(r);
        }
    }
    Ok(out)
}

/// Load the full principal for an already-validated session.
pub async fn load_principal_for_session(
    pool: &MySqlPool,
    session: &session::SessionRecord,
) -> AppResult<Principal> {
    let row = sqlx::query(
        r#"
        SELECT id, email, display_name, department_id
          FROM users
         WHERE id = ?
         LIMIT 1
        "#,
    )
    .bind(session.user_id.to_string())
    .fetch_optional(pool)
    .await
    .map_err(|e| AppError::Database(format!("load_user_by_id: {}", e)))?
    .ok_or(AppError::Unauthorized)?;

    let id: String = row.try_get("id").map_err(|e| AppError::Database(e.to_string()))?;
    let email: String = row.try_get("email").map_err(|e| AppError::Database(e.to_string()))?;
    let display_name: String = row
        .try_get("display_name")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let department_id: Option<String> = row
        .try_get("department_id")
        .map_err(|e| AppError::Database(e.to_string()))?;

    let user_id = Uuid::parse_str(&id).map_err(|e| AppError::Database(e.to_string()))?;
    let roles = load_user_roles(pool, user_id).await?;

    Ok(Principal {
        user_id,
        session_id: session.id,
        email,
        display_name,
        roles,
        department_id: department_id.as_deref().and_then(|s| Uuid::parse_str(s).ok()),
    })
}
