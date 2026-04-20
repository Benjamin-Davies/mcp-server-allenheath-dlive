use std::{convert::Infallible, sync::Arc};

use axum::{
    extract::{OptionalFromRequestParts, Query, Request, State},
    http::{HeaderMap, HeaderValue, StatusCode, Uri, request},
    middleware,
    response::{IntoResponse, Response},
};

use crate::args::Args;

pub async fn validate_token_middleware(
    State(args): State<Arc<Args>>,
    auth_token: Option<AuthToken>,
    request: Request,
    next: middleware::Next,
) -> Response {
    if let Some(token) = &args.token {
        let Some(auth_token) = auth_token else {
            return StatusCode::UNAUTHORIZED.into_response();
        };
        if auth_token.token != *token {
            return StatusCode::UNAUTHORIZED.into_response();
        }
    }

    let response = next.run(request).await;
    response
}

#[derive(Debug, serde::Deserialize)]
pub struct AuthToken {
    token: String,
}

impl<S> OptionalFromRequestParts<S> for AuthToken
where
    S: Send + Sync,
{
    type Rejection = Infallible;

    fn from_request_parts(
        parts: &mut request::Parts,
        _state: &S,
    ) -> impl Future<Output = Result<Option<Self>, Self::Rejection>> + Send {
        async move {
            if let Some(auth_token) = extract_token_from_query(&parts.uri) {
                Ok(Some(auth_token))
            } else if let Some(auth_token) = extract_token_from_header(&parts.headers) {
                Ok(Some(auth_token))
            } else {
                Ok(None)
            }
        }
    }
}

fn extract_token_from_query(uri: &Uri) -> Option<AuthToken> {
    Query::try_from_uri(uri).ok().map(|q| q.0)
}

fn extract_token_from_header(headers: &HeaderMap<HeaderValue>) -> Option<AuthToken> {
    headers
        .get("Authorization")?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
        .map(|t| AuthToken {
            token: t.to_owned(),
        })
}
