//! Check-in REST routes — Phase 5 implementation.
//!
//! * `POST /api/v1/checkins`              — one-tap check-in
//! * `POST /api/v1/checkins/<id>/retry`   — reasoned retry
//! * `GET  /api/v1/checkins?section_id=`  — list (scoped + masked)
//! * `GET  /api/v1/checkins/retry-reasons`— controlled list of retry reasons

use rocket::serde::json::Json;
use rocket::State;
use sqlx::MySqlPool;
use uuid::Uuid;

use crate::api::guards::{AuthedPrincipal, ClientContext};
use crate::application::checkin_service::{
    self, CheckinInput, CheckinResult, CheckinRetryInput, CheckinView, RetryReason,
};
use crate::errors::{AppError, AppResult};

pub fn routes() -> Vec<rocket::Route> {
    routes![list_checkins, list_retry_reasons, create_checkin, retry_checkin]
}

fn parse_uuid(s: &str, field: &str) -> AppResult<Uuid> {
    Uuid::parse_str(s).map_err(|_| AppError::Validation(format!("{} must be a UUID", field)))
}

#[post("/", format = "json", data = "<body>")]
pub async fn create_checkin(
    principal: AuthedPrincipal,
    pool: &State<MySqlPool>,
    ctx: ClientContext,
    body: Json<CheckinInput>,
) -> AppResult<Json<CheckinResult>> {
    let p = principal.into_inner();
    let result = checkin_service::check_in(
        pool.inner(),
        &p,
        body.into_inner(),
        ctx.ip_address.as_deref(),
        ctx.user_agent.as_deref(),
    )
    .await?;
    Ok(Json(result))
}

#[post("/<id>/retry", format = "json", data = "<body>")]
pub async fn retry_checkin(
    principal: AuthedPrincipal,
    pool: &State<MySqlPool>,
    ctx: ClientContext,
    id: &str,
    body: Json<CheckinRetryInput>,
) -> AppResult<Json<CheckinResult>> {
    let p = principal.into_inner();
    let cid = parse_uuid(id, "id")?;
    let result = checkin_service::retry_checkin(
        pool.inner(),
        &p,
        cid,
        body.into_inner(),
        ctx.ip_address.as_deref(),
        ctx.user_agent.as_deref(),
    )
    .await?;
    Ok(Json(result))
}

#[get("/?<section_id>&<limit>&<offset>")]
pub async fn list_checkins(
    principal: AuthedPrincipal,
    pool: &State<MySqlPool>,
    section_id: String,
    limit: Option<u32>,
    offset: Option<u32>,
) -> AppResult<Json<Vec<CheckinView>>> {
    let p = principal.into_inner();
    let sid = parse_uuid(&section_id, "section_id")?;
    let views = checkin_service::list_checkins_for_section(
        pool.inner(),
        &p,
        sid,
        limit.unwrap_or(100),
        offset.unwrap_or(0),
    )
    .await?;
    Ok(Json(views))
}

#[get("/retry-reasons")]
pub async fn list_retry_reasons(
    _principal: AuthedPrincipal,
    pool: &State<MySqlPool>,
) -> AppResult<Json<Vec<RetryReason>>> {
    Ok(Json(
        checkin_service::list_retry_reasons(pool.inner()).await?,
    ))
}
