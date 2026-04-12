//! Dashboard REST routes — Phase 5.
//!
//! Every endpoint computes its values from stored data inside
//! `application::dashboard_service`. Nothing on this layer makes up
//! numbers.

use chrono::{DateTime, Utc};
use rocket::serde::json::Json;
use rocket::State;
use sqlx::MySqlPool;
use uuid::Uuid;

use crate::api::guards::AuthedPrincipal;
use crate::application::dashboard_service::{self, DashboardPanel, DateFilter};
use crate::errors::{AppError, AppResult};

pub fn routes() -> Vec<rocket::Route> {
    routes![
        course_popularity,
        fill_rate,
        drop_rate,
        instructor_workload,
        foot_traffic,
        dwell_time,
        interaction_quality,
    ]
}

fn parse_filter(
    from: Option<&str>,
    to: Option<&str>,
    department_id: Option<&str>,
) -> AppResult<DateFilter> {
    fn parse_ts(raw: Option<&str>, field: &str) -> AppResult<Option<DateTime<Utc>>> {
        match raw {
            None => Ok(None),
            Some(s) if s.is_empty() => Ok(None),
            Some(s) => DateTime::parse_from_rfc3339(s)
                .map(|d| Some(d.with_timezone(&Utc)))
                .map_err(|_| AppError::Validation(format!("{} must be RFC3339", field))),
        }
    }
    fn parse_uuid_opt(raw: Option<&str>, field: &str) -> AppResult<Option<Uuid>> {
        match raw {
            None => Ok(None),
            Some(s) if s.is_empty() => Ok(None),
            Some(s) => Uuid::parse_str(s)
                .map(Some)
                .map_err(|_| AppError::Validation(format!("{} must be a UUID", field))),
        }
    }
    Ok(DateFilter {
        from: parse_ts(from, "from")?,
        to: parse_ts(to, "to")?,
        department_id: parse_uuid_opt(department_id, "department_id")?,
    })
}

macro_rules! dashboard_route {
    ($name:ident, $service_fn:ident, $path:expr) => {
        #[get($path)]
        pub async fn $name(
            principal: AuthedPrincipal,
            pool: &State<MySqlPool>,
            from: Option<String>,
            to: Option<String>,
            department_id: Option<String>,
        ) -> AppResult<Json<DashboardPanel>> {
            let p = principal.into_inner();
            let filter = parse_filter(from.as_deref(), to.as_deref(), department_id.as_deref())?;
            let panel = dashboard_service::$service_fn(pool.inner(), &p, filter).await?;
            Ok(Json(panel))
        }
    };
}

dashboard_route!(
    course_popularity,
    course_popularity,
    "/course-popularity?<from>&<to>&<department_id>"
);
dashboard_route!(
    fill_rate,
    fill_rate,
    "/fill-rate?<from>&<to>&<department_id>"
);
dashboard_route!(
    drop_rate,
    drop_rate,
    "/drop-rate?<from>&<to>&<department_id>"
);
dashboard_route!(
    instructor_workload,
    instructor_workload,
    "/instructor-workload?<from>&<to>&<department_id>"
);
dashboard_route!(
    foot_traffic,
    foot_traffic,
    "/foot-traffic?<from>&<to>&<department_id>"
);
dashboard_route!(
    dwell_time,
    dwell_time,
    "/dwell-time?<from>&<to>&<department_id>"
);
dashboard_route!(
    interaction_quality,
    interaction_quality,
    "/interaction-quality?<from>&<to>&<department_id>"
);
