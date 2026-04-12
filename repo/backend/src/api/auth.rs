//! Real authentication routes.
//!
//! All failures flow through the standardized error envelope defined in
//! [`crate::errors`]. Client context (IP, User-Agent) is captured by the
//! [`ClientContext`] guard, not taken as a raw request reference.

use rocket::serde::json::Json;
use rocket::State;
use serde::Serialize;
use sqlx::MySqlPool;

use crate::api::guards::{AuthedPrincipal, ClientContext};
use crate::application::auth_service::{self, LoginInput, LoginOutput, RequestContext};
use crate::config::AppConfig;
use crate::errors::AppResult;

pub fn routes() -> Vec<rocket::Route> {
    routes![login, logout, me]
}

#[derive(Serialize)]
pub struct LoginBody {
    pub token: String,
    pub expires_at: chrono::NaiveDateTime,
    pub user: UserPublic,
}

#[derive(Serialize)]
pub struct UserPublic {
    pub id: uuid::Uuid,
    pub email: String,
    pub display_name: String,
    pub roles: Vec<String>,
    pub department_id: Option<uuid::Uuid>,
}

fn to_public(out: LoginOutput) -> LoginBody {
    LoginBody {
        token: out.token,
        expires_at: out.expires_at,
        user: UserPublic {
            id: out.principal.user_id,
            email: out.principal.email,
            display_name: out.principal.display_name,
            roles: out
                .principal
                .roles
                .iter()
                .map(|r| r.as_db_name().to_string())
                .collect(),
            department_id: out.principal.department_id,
        },
    }
}

/// POST /api/v1/auth/login
#[post("/login", format = "application/json", data = "<body>")]
pub async fn login(
    body: Json<LoginInput>,
    pool: &State<MySqlPool>,
    config: &State<AppConfig>,
    ctx: ClientContext,
) -> AppResult<Json<LoginBody>> {
    let request_ctx = RequestContext {
        ip_address: ctx.ip_address.as_deref(),
        user_agent: ctx.user_agent.as_deref(),
    };
    let out = auth_service::login(
        pool.inner(),
        config.inner(),
        body.into_inner(),
        request_ctx,
    )
    .await?;
    Ok(Json(to_public(out)))
}

/// POST /api/v1/auth/logout
#[post("/logout")]
pub async fn logout(
    principal: AuthedPrincipal,
    pool: &State<MySqlPool>,
    ctx: ClientContext,
) -> AppResult<Json<serde_json::Value>> {
    let request_ctx = RequestContext {
        ip_address: ctx.ip_address.as_deref(),
        user_agent: ctx.user_agent.as_deref(),
    };
    auth_service::logout(pool.inner(), principal.as_ref(), request_ctx).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

/// GET /api/v1/auth/me — return the current principal.
#[get("/me")]
pub async fn me(principal: AuthedPrincipal) -> AppResult<Json<UserPublic>> {
    let p = principal.into_inner();
    Ok(Json(UserPublic {
        id: p.user_id,
        email: p.email,
        display_name: p.display_name,
        roles: p.roles.iter().map(|r| r.as_db_name().to_string()).collect(),
        department_id: p.department_id,
    }))
}
