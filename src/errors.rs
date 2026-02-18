use std::sync::Arc;

use axum::{
    extract::Request,
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};

#[derive(Debug, thiserror::Error)]
pub enum InternalError {
    #[error(transparent)]
    Axum(#[from] axum::Error),

    #[error(transparent)]
    Db(#[from] sqlx::Error),

    #[error(transparent)]
    Discord(#[from] serenity::Error),
}

impl IntoResponse for InternalError {
    fn into_response(self) -> Response {
        let mut response = (
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal server error".to_string(),
        )
            .into_response();

        response.extensions_mut().insert(Arc::new(self));
        response
    }
}

pub async fn log_app_errors(request: Request, next: Next) -> Response {
    let response = next.run(request).await;

    if let Some(err) = response.extensions().get::<Arc<InternalError>>() {
        tracing::error!(?err, "an unexpected error occurred inside a handler");
    }

    response
}
