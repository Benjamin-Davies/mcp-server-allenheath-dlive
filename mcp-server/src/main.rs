use std::sync::Arc;

use axum::Router;
use clap::Parser;
use rmcp::transport::{
    StreamableHttpService, streamable_http_server::session::local::LocalSessionManager,
};

use crate::{args::Args, handler::DLiveHandler};

mod args;
mod handler;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv()?;
    tracing_subscriber::fmt().init();

    let args = Args::parse();
    let args = Arc::new(args);

    let session_manager = LocalSessionManager::default();
    let mcp_service = StreamableHttpService::new(
        move || Ok(DLiveHandler::new(args.clone())),
        Arc::new(session_manager),
        Default::default(),
    );

    let app = Router::new().nest_service("/mcp", mcp_service);
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    tracing::info!("Listening on {:?}", listener.local_addr().unwrap());
    axum::serve(listener, app).await?;
    Ok(())
}
