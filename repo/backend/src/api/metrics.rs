//! Metric semantic-layer REST routes — Phase 5 implementation.

use rocket::serde::json::Json;
use rocket::State;
use sqlx::MySqlPool;
use uuid::Uuid;

use crate::api::guards::AuthedPrincipal;
use crate::application::metric_service::{
    self, MetricCreateInput, MetricDefinitionView, MetricEditInput, MetricVersionView,
};
use crate::errors::{AppError, AppResult};

pub fn routes() -> Vec<rocket::Route> {
    routes![
        list_metrics,
        create_metric,
        get_metric,
        new_draft,
        list_versions,
        approve_version,
        publish_version,
        mark_widget_verified,
    ]
}

fn parse_uuid(s: &str, field: &str) -> AppResult<Uuid> {
    Uuid::parse_str(s).map_err(|_| AppError::Validation(format!("{} must be a UUID", field)))
}

#[get("/?<limit>&<offset>")]
pub async fn list_metrics(
    principal: AuthedPrincipal,
    pool: &State<MySqlPool>,
    limit: Option<u32>,
    offset: Option<u32>,
) -> AppResult<Json<Vec<MetricDefinitionView>>> {
    let p = principal.into_inner();
    let views = metric_service::list_metrics(
        pool.inner(),
        &p,
        limit.unwrap_or(100),
        offset.unwrap_or(0),
    )
    .await?;
    Ok(Json(views))
}

#[post("/", format = "json", data = "<body>")]
pub async fn create_metric(
    principal: AuthedPrincipal,
    pool: &State<MySqlPool>,
    body: Json<MetricCreateInput>,
) -> AppResult<Json<MetricDefinitionView>> {
    let p = principal.into_inner();
    Ok(Json(
        metric_service::create_metric(pool.inner(), &p, body.into_inner()).await?,
    ))
}

#[get("/<id>", rank = 2)]
pub async fn get_metric(
    principal: AuthedPrincipal,
    pool: &State<MySqlPool>,
    id: &str,
) -> AppResult<Json<MetricDefinitionView>> {
    let p = principal.into_inner();
    let mid = parse_uuid(id, "id")?;
    Ok(Json(
        metric_service::get_metric_by_id(pool.inner(), &p, mid).await?,
    ))
}

#[put("/<id>", format = "json", data = "<body>")]
pub async fn new_draft(
    principal: AuthedPrincipal,
    pool: &State<MySqlPool>,
    id: &str,
    body: Json<MetricEditInput>,
) -> AppResult<Json<MetricVersionView>> {
    let p = principal.into_inner();
    let mid = parse_uuid(id, "id")?;
    Ok(Json(
        metric_service::create_draft_version(pool.inner(), &p, mid, body.into_inner()).await?,
    ))
}

#[get("/<id>/versions")]
pub async fn list_versions(
    principal: AuthedPrincipal,
    pool: &State<MySqlPool>,
    id: &str,
) -> AppResult<Json<Vec<MetricVersionView>>> {
    let p = principal.into_inner();
    let mid = parse_uuid(id, "id")?;
    Ok(Json(
        metric_service::list_versions(pool.inner(), &p, mid).await?,
    ))
}

#[post("/<id>/versions/<vid>/approve")]
pub async fn approve_version(
    principal: AuthedPrincipal,
    pool: &State<MySqlPool>,
    id: &str,
    vid: &str,
) -> AppResult<Json<MetricVersionView>> {
    let p = principal.into_inner();
    let mid = parse_uuid(id, "id")?;
    let v = parse_uuid(vid, "vid")?;
    Ok(Json(
        metric_service::approve_version(pool.inner(), &p, mid, v).await?,
    ))
}

#[post("/<id>/versions/<vid>/publish")]
pub async fn publish_version(
    principal: AuthedPrincipal,
    pool: &State<MySqlPool>,
    id: &str,
    vid: &str,
) -> AppResult<Json<MetricDefinitionView>> {
    let p = principal.into_inner();
    let mid = parse_uuid(id, "id")?;
    let v = parse_uuid(vid, "vid")?;
    Ok(Json(
        metric_service::publish_version(pool.inner(), &p, mid, v).await?,
    ))
}

#[post("/widgets/<widget_id>/verify")]
pub async fn mark_widget_verified(
    principal: AuthedPrincipal,
    pool: &State<MySqlPool>,
    widget_id: &str,
) -> AppResult<Json<serde_json::Value>> {
    let p = principal.into_inner();
    let wid = parse_uuid(widget_id, "widget_id")?;
    metric_service::mark_widget_verified(pool.inner(), &p, wid).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}
