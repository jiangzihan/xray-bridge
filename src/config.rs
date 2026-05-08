// Application configuration loaded from environment variables.
// Only two settings: BRIDGE_TOKEN (required) and PORT (optional, default 8080).

use anyhow::{Context, Result};

#[derive(Debug, Clone)]
pub struct Settings {
    pub bridge_token: String,
    pub port: u16,
}

impl Settings {
    /// Load settings from environment variables.
    /// Fails fast if BRIDGE_TOKEN is missing or empty.
    pub fn from_env() -> Result<Self> {
        let bridge_token = std::env::var("BRIDGE_TOKEN")
            .context("BRIDGE_TOKEN environment variable is required")?;

        if bridge_token.is_empty() {
            anyhow::bail!("BRIDGE_TOKEN must not be empty");
        }

        let port = std::env::var("PORT")
            .ok()
            .and_then(|s| s.parse::<u16>().ok())
            .unwrap_or(8080);

        Ok(Self { bridge_token, port })
    }
}
