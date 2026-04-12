use mcp_server_allenheath_dlive::mcp_handler::Calculator;
use rmcp::{ServiceExt, transport::stdio};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .init();

    Calculator.serve(stdio()).await?.waiting().await?;
    Ok(())
}
