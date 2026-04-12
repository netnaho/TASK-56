//! MySQL connection pool initialization.
//!
//! The pool is created once during `build_rocket` and handed to Rocket as
//! managed state. Handlers obtain it via `&State<MySqlPool>`.

use sqlx::mysql::MySqlPoolOptions;
use sqlx::MySqlPool;
use std::time::Duration;

/// Build the shared MySQL pool. Caller is expected to surface errors to
/// the process startup path.
pub async fn init_pool(database_url: &str) -> Result<MySqlPool, sqlx::Error> {
    MySqlPoolOptions::new()
        .max_connections(20)
        .acquire_timeout(Duration::from_secs(10))
        .connect(database_url)
        .await
}
