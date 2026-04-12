use serde::{Serialize, Deserialize};
use chrono::NaiveDateTime;
use uuid::Uuid;

/// A user-configurable dashboard that arranges widgets for at-a-glance insight.
///
/// Dashboards may be personal (owned by one user) or shared across a role
/// or department.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardDefinition {
    pub id: Uuid,

    /// The user who created the dashboard.
    pub owner_id: Uuid,

    /// Display title, e.g. "My Teaching Overview".
    pub title: String,

    /// Optional description of the dashboard's purpose.
    pub description: Option<String>,

    /// Whether the dashboard is visible to users other than the owner.
    pub is_shared: bool,

    /// Ordered layout specification (grid positions, sizes).
    /// TODO: decide on layout schema (JSON blob vs normalised rows) in phase 2.
    pub layout_json: Option<String>,

    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

/// A single widget placed on a dashboard.
///
/// Each widget references a metric or data source and carries display
/// configuration (chart type, colour palette, etc.).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardWidget {
    pub id: Uuid,

    /// The dashboard this widget belongs to.
    pub dashboard_id: Uuid,

    /// Optional link to a `MetricDefinition` that feeds this widget.
    pub metric_definition_id: Option<Uuid>,

    /// Human-readable widget title.
    pub title: String,

    /// Visualisation type, e.g. "bar_chart", "line_chart", "stat_card".
    /// TODO: convert to an enum once the widget catalogue is finalised in phase 2.
    pub widget_type: String,

    /// JSON blob holding chart options, colours, thresholds, etc.
    /// TODO: define a typed schema for widget config in phase 2.
    pub config_json: Option<String>,

    /// Sort / display order within the dashboard.
    pub position: i32,

    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}
