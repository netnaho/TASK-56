//! Journal REST routes — Phase 3 implementation.
//!
//! Every handler is gated by `AuthedPrincipal`. Capability checks happen
//! inside the service layer (`application::journal_service`) so the route
//! body stays thin.

use rocket::serde::json::Json;
use rocket::State;
use sqlx::MySqlPool;
use uuid::Uuid;

use crate::api::guards::AuthedPrincipal;
use crate::application::journal_service::{
    self, JournalCreateInput, JournalEditInput, JournalVersionView, JournalView,
};
use crate::errors::{AppError, AppResult};

pub fn routes() -> Vec<rocket::Route> {
    routes![
        list_journals,
        create_journal,
        get_journal,
        create_draft_version,
        list_versions,
        get_version,
        approve_version,
        publish_version,
    ]
}

fn parse_uuid(s: &str, field: &str) -> AppResult<Uuid> {
    Uuid::parse_str(s).map_err(|_| AppError::Validation(format!("{} must be a UUID", field)))
}

// ── Listing and reads ───────────────────────────────────────────────────────

#[get("/?<limit>&<offset>")]
pub async fn list_journals(
    principal: AuthedPrincipal,
    pool: &State<MySqlPool>,
    limit: Option<u32>,
    offset: Option<u32>,
) -> AppResult<Json<Vec<JournalView>>> {
    let p = principal.into_inner();
    let views = journal_service::list_journals(
        pool.inner(),
        &p,
        limit.unwrap_or(50),
        offset.unwrap_or(0),
    )
    .await?;
    Ok(Json(views))
}

#[get("/<id>")]
pub async fn get_journal(
    principal: AuthedPrincipal,
    pool: &State<MySqlPool>,
    id: &str,
) -> AppResult<Json<JournalView>> {
    let p = principal.into_inner();
    let jid = parse_uuid(id, "id")?;
    let view = journal_service::get_journal_by_id(pool.inner(), &p, jid).await?;
    Ok(Json(view))
}

// ── Writes: create, draft, approve, publish ─────────────────────────────────

#[post("/", format = "json", data = "<body>")]
pub async fn create_journal(
    principal: AuthedPrincipal,
    pool: &State<MySqlPool>,
    body: Json<JournalCreateInput>,
) -> AppResult<Json<JournalView>> {
    let p = principal.into_inner();
    let view = journal_service::create_journal(pool.inner(), &p, body.into_inner()).await?;
    Ok(Json(view))
}

#[put("/<id>", format = "json", data = "<body>")]
pub async fn create_draft_version(
    principal: AuthedPrincipal,
    pool: &State<MySqlPool>,
    id: &str,
    body: Json<JournalEditInput>,
) -> AppResult<Json<JournalVersionView>> {
    let p = principal.into_inner();
    let jid = parse_uuid(id, "id")?;
    let version =
        journal_service::create_draft_version(pool.inner(), &p, jid, body.into_inner()).await?;
    Ok(Json(version))
}

#[get("/<id>/versions")]
pub async fn list_versions(
    principal: AuthedPrincipal,
    pool: &State<MySqlPool>,
    id: &str,
) -> AppResult<Json<Vec<JournalVersionView>>> {
    let p = principal.into_inner();
    let jid = parse_uuid(id, "id")?;
    let versions = journal_service::list_versions(pool.inner(), &p, jid).await?;
    Ok(Json(versions))
}

#[get("/<id>/versions/<version_id>")]
pub async fn get_version(
    principal: AuthedPrincipal,
    pool: &State<MySqlPool>,
    id: &str,
    version_id: &str,
) -> AppResult<Json<JournalVersionView>> {
    let p = principal.into_inner();
    let jid = parse_uuid(id, "id")?;
    let vid = parse_uuid(version_id, "version_id")?;
    let version = journal_service::get_version(pool.inner(), &p, jid, vid).await?;
    Ok(Json(version))
}

#[post("/<id>/versions/<version_id>/approve")]
pub async fn approve_version(
    principal: AuthedPrincipal,
    pool: &State<MySqlPool>,
    id: &str,
    version_id: &str,
) -> AppResult<Json<JournalVersionView>> {
    let p = principal.into_inner();
    let jid = parse_uuid(id, "id")?;
    let vid = parse_uuid(version_id, "version_id")?;
    let version = journal_service::approve_version(pool.inner(), &p, jid, vid).await?;
    Ok(Json(version))
}

#[post("/<id>/versions/<version_id>/publish")]
pub async fn publish_version(
    principal: AuthedPrincipal,
    pool: &State<MySqlPool>,
    id: &str,
    version_id: &str,
) -> AppResult<Json<JournalView>> {
    let p = principal.into_inner();
    let jid = parse_uuid(id, "id")?;
    let vid = parse_uuid(version_id, "version_id")?;
    let view = journal_service::publish_version(pool.inner(), &p, jid, vid).await?;
    Ok(Json(view))
}
