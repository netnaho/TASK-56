//! Teaching-resource REST routes — Phase 3 implementation.
//!
//! Mirrors `api::journals`. Capability checks happen in the service layer.

use rocket::serde::json::Json;
use rocket::State;
use sqlx::MySqlPool;
use uuid::Uuid;

use crate::api::guards::AuthedPrincipal;
use crate::application::resource_service::{
    self, ResourceCreateInput, ResourceEditInput, ResourceVersionView, ResourceView,
};
use crate::errors::{AppError, AppResult};

pub fn routes() -> Vec<rocket::Route> {
    routes![
        list_resources,
        create_resource,
        get_resource,
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

#[get("/?<limit>&<offset>")]
pub async fn list_resources(
    principal: AuthedPrincipal,
    pool: &State<MySqlPool>,
    limit: Option<u32>,
    offset: Option<u32>,
) -> AppResult<Json<Vec<ResourceView>>> {
    let p = principal.into_inner();
    let views = resource_service::list_resources(
        pool.inner(),
        &p,
        limit.unwrap_or(50),
        offset.unwrap_or(0),
    )
    .await?;
    Ok(Json(views))
}

#[get("/<id>")]
pub async fn get_resource(
    principal: AuthedPrincipal,
    pool: &State<MySqlPool>,
    id: &str,
) -> AppResult<Json<ResourceView>> {
    let p = principal.into_inner();
    let rid = parse_uuid(id, "id")?;
    let view = resource_service::get_resource_by_id(pool.inner(), &p, rid).await?;
    Ok(Json(view))
}

#[post("/", format = "json", data = "<body>")]
pub async fn create_resource(
    principal: AuthedPrincipal,
    pool: &State<MySqlPool>,
    body: Json<ResourceCreateInput>,
) -> AppResult<Json<ResourceView>> {
    let p = principal.into_inner();
    let view = resource_service::create_resource(pool.inner(), &p, body.into_inner()).await?;
    Ok(Json(view))
}

#[put("/<id>", format = "json", data = "<body>")]
pub async fn create_draft_version(
    principal: AuthedPrincipal,
    pool: &State<MySqlPool>,
    id: &str,
    body: Json<ResourceEditInput>,
) -> AppResult<Json<ResourceVersionView>> {
    let p = principal.into_inner();
    let rid = parse_uuid(id, "id")?;
    let version =
        resource_service::create_draft_version(pool.inner(), &p, rid, body.into_inner())
            .await?;
    Ok(Json(version))
}

#[get("/<id>/versions")]
pub async fn list_versions(
    principal: AuthedPrincipal,
    pool: &State<MySqlPool>,
    id: &str,
) -> AppResult<Json<Vec<ResourceVersionView>>> {
    let p = principal.into_inner();
    let rid = parse_uuid(id, "id")?;
    let versions = resource_service::list_versions(pool.inner(), &p, rid).await?;
    Ok(Json(versions))
}

#[get("/<id>/versions/<version_id>")]
pub async fn get_version(
    principal: AuthedPrincipal,
    pool: &State<MySqlPool>,
    id: &str,
    version_id: &str,
) -> AppResult<Json<ResourceVersionView>> {
    let p = principal.into_inner();
    let rid = parse_uuid(id, "id")?;
    let vid = parse_uuid(version_id, "version_id")?;
    let version = resource_service::get_version(pool.inner(), &p, rid, vid).await?;
    Ok(Json(version))
}

#[post("/<id>/versions/<version_id>/approve")]
pub async fn approve_version(
    principal: AuthedPrincipal,
    pool: &State<MySqlPool>,
    id: &str,
    version_id: &str,
) -> AppResult<Json<ResourceVersionView>> {
    let p = principal.into_inner();
    let rid = parse_uuid(id, "id")?;
    let vid = parse_uuid(version_id, "version_id")?;
    let version = resource_service::approve_version(pool.inner(), &p, rid, vid).await?;
    Ok(Json(version))
}

#[post("/<id>/versions/<version_id>/publish")]
pub async fn publish_version(
    principal: AuthedPrincipal,
    pool: &State<MySqlPool>,
    id: &str,
    version_id: &str,
) -> AppResult<Json<ResourceView>> {
    let p = principal.into_inner();
    let rid = parse_uuid(id, "id")?;
    let vid = parse_uuid(version_id, "version_id")?;
    let view = resource_service::publish_version(pool.inner(), &p, rid, vid).await?;
    Ok(Json(view))
}
