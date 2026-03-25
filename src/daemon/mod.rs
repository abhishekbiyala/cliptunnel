pub mod clipboard;
pub mod launchd;
pub mod server;

use anyhow::Result;

pub async fn run_foreground(port: u16) -> Result<()> {
    let token = crate::config::load_or_create_token()?;
    tracing::info!("starting daemon on 127.0.0.1:{port}");
    tracing::info!("token location: {}", crate::config::token_path().display());
    server::run(port, &token).await
}
