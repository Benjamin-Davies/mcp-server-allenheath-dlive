use std::sync::Arc;

use axum::{Router, middleware};
use clap::Parser;
use rmcp::transport::{
    StreamableHttpService, streamable_http_server::session::local::LocalSessionManager,
};
use tokio::sync::watch;

use crate::{
    args::{Args, ChannelConfig},
    handler::DLiveHandler,
};

mod args;
mod auth;
mod handler;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    if dotenvy::dotenv().is_err() {
        eprintln!("Couldn't load env file");
    }
    tracing_subscriber::fmt().init();

    let args = Args::parse();
    let args = Arc::new(args);

    let initial_config = args.channel_config();
    let (config_tx, config_rx) = watch::channel(initial_config);

    tokio::spawn(sighup_task(config_tx));

    let session_manager = LocalSessionManager::default();
    let args_clone = args.clone();
    let mcp_service = StreamableHttpService::new(
        move || Ok(DLiveHandler::new(args_clone.clone(), config_rx.clone())),
        Arc::new(session_manager),
        Default::default(),
    );

    let app =
        Router::new()
            .nest_service("/mcp", mcp_service)
            .layer(middleware::from_fn_with_state(
                args,
                auth::validate_token_middleware,
            ));
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    tracing::info!("Listening on {:?}", listener.local_addr().unwrap());
    axum::serve(listener, app).await?;
    Ok(())
}

async fn sighup_task(tx: watch::Sender<ChannelConfig>) {
    use tokio::signal::unix::{SignalKind, signal};

    let mut stream = match signal(SignalKind::hangup()) {
        Ok(s) => s,
        Err(err) => {
            tracing::error!("Failed to register SIGHUP handler: {err}");
            return;
        }
    };

    loop {
        stream.recv().await;
        tracing::info!("SIGHUP received, reloading channel config from .env");
        match ChannelConfig::load() {
            Ok(config) => {
                tx.send_replace(config);
                tracing::info!("Channel config reloaded successfully");
            }
            Err(err) => {
                tracing::error!("Failed to reload channel config: {err:#}");
            }
        }
    }
}
