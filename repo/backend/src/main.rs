use scholarly_backend::build_rocket;

#[rocket::main]
async fn main() -> Result<(), rocket::Error> {
    tracing_subscriber::fmt::init();
    tracing::info!("Starting Scholarly backend server");

    let rocket = build_rocket().await;
    rocket.launch().await?;
    Ok(())
}
