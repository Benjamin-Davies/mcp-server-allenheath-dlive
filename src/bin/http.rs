use std::sync::Arc;

use axum::Router;
use mcp_server_allenheath_dlive::mcp_handler::DLiveHandler;
use rmcp::transport::{
    StreamableHttpService, streamable_http_server::session::local::LocalSessionManager,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().init();

    let session_manager = LocalSessionManager::default();
    let mcp_service = StreamableHttpService::new(
        move || Ok(DLiveHandler::new()),
        Arc::new(session_manager),
        Default::default(),
    );

    let app = Router::new().nest_service("/mcp", mcp_service);
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    log::info!("Listening on {:?}", listener.local_addr().unwrap());
    axum::serve(listener, app).await?;
    Ok(())
}
