use std::sync::Arc;

use axum::{
    Router,
    extract::{Request, State},
    http::StatusCode,
    middleware::{self, Next},
    response::{IntoResponse, Response},
};
use clap::Parser;
use rmcp::transport::{
    StreamableHttpService, streamable_http_server::session::local::LocalSessionManager,
};

use crate::{args::Args, handler::DLiveHandler};

mod args;
mod handler;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    if dotenvy::dotenv().is_err() {
        eprintln!("Couldn't load env file");
    }
    tracing_subscriber::fmt().init();

    let args = Args::parse();
    let args = Arc::new(args);

    let session_manager = LocalSessionManager::default();
    let args_clone = args.clone();
    let mcp_service = StreamableHttpService::new(
        move || Ok(DLiveHandler::new(args_clone.clone())),
        Arc::new(session_manager),
        Default::default(),
    );

    let app =
        Router::new()
            .nest_service("/mcp", mcp_service)
            .layer(middleware::from_fn_with_state(
                args,
                validate_token_middleware,
            ));
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    tracing::info!("Listening on {:?}", listener.local_addr().unwrap());
    axum::serve(listener, app).await?;
    Ok(())
}

async fn validate_token_middleware(
    State(args): State<Arc<Args>>,
    request: Request,
    next: Next,
) -> Response {
    if let Some(token) = &args.token {
        let Some(t) = extract_token(&request) else {
            return StatusCode::UNAUTHORIZED.into_response();
        };
        if t != token {
            return StatusCode::UNAUTHORIZED.into_response();
        }
    }

    let response = next.run(request).await;
    response
}

fn extract_token(request: &Request) -> Option<&str> {
    request
        .headers()
        .get("Authorization")?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
}
