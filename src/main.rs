// xray-bridge: HTTP→gRPC bridge for Xray node management API.
// Single stateless binary — no state persisted between requests.

mod auth;
mod config;
mod error;
mod nodes;
mod proto;
mod routes;
mod xray;

use std::net::{IpAddr, SocketAddr};

use config::Settings;
use nodes::AppState;

fn init_tracing() {
    let filter = std::env::var("RUST_LOG").unwrap_or_else(|_| "info,xray_bridge=debug".to_owned());
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::builder().parse_lossy(filter))
        .init();
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load .env file if present; ignore errors (file is optional)
    let _ = dotenvy::dotenv();

    init_tracing();

    let settings = Settings::from_env()?;
    let state = AppState::new(settings.clone());
    let app = routes::build_router(state);

    let addr = SocketAddr::new(IpAddr::from([0, 0, 0, 0]), settings.port);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("xray-bridge listening on {addr}");

    axum::serve(listener, app).await?;
    Ok(())
}
