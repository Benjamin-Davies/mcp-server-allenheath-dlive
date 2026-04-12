use mcp_server_allenheath_dlive::mcp_handler::DLiveHandler;
use rmcp::{ServiceExt, transport::stdio};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .init();

    DLiveHandler::new().serve(stdio()).await?.waiting().await?;
    Ok(())
}
