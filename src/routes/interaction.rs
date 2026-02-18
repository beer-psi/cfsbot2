use std::sync::Arc;

use axum::{
    Json,
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use serenity::all::{CreateInteractionResponse, Interaction};
use tracing::{error, warn};

use crate::{
    app_state::AppState,
    commands::{handle_command, handle_component, handle_modal},
    errors::InternalError,
};

pub async fn verify_interaction_request(
    State(app_state): State<Arc<AppState>>,
    request: Request,
    next: Next,
) -> Result<Response, InternalError> {
    let (parts, body) = request.into_parts();
    let Some(Ok(signature)) = parts.headers.get("X-Signature-Ed25519").map(|v| v.to_str()) else {
        return Ok(StatusCode::UNAUTHORIZED.into_response());
    };
    let Some(Ok(timestamp)) = parts
        .headers
        .get("X-Signature-Timestamp")
        .map(|v| v.to_str())
    else {
        return Ok(StatusCode::UNAUTHORIZED.into_response());
    };

    let bytes = axum::body::to_bytes(body, usize::MAX).await?;

    if app_state
        .serenity_verifier
        .verify(signature, timestamp, &bytes)
        .is_err()
    {
        return Ok(StatusCode::UNAUTHORIZED.into_response());
    }

    let request = Request::from_parts(parts, axum::body::Body::from(bytes));

    Ok(next.run(request).await)
}

pub async fn post_interaction(
    State(app_state): State<Arc<AppState>>,
    Json(interaction): Json<Interaction>,
) -> Result<Response, InternalError> {
    if let Interaction::Ping(_) = interaction {
        return Ok(Json(CreateInteractionResponse::Pong).into_response());
    }

    tokio::spawn(async move {
        match interaction {
            Interaction::Command(command_interaction) => {
                if let Err(e) = handle_command(app_state, &command_interaction).await {
                    error!(interaction = ?command_interaction, error = ?e, "error handling command interaction");
                }
            }
            Interaction::Autocomplete(_) => {}
            Interaction::Component(mut component_interaction) => {
                if let Err(e) = handle_component(app_state, &mut component_interaction).await {
                    error!(interaction = ?component_interaction, error = ?e, "error handling component interaction");
                }
            }
            Interaction::Modal(modal_interaction) => {
                if let Err(e) = handle_modal(app_state.clone(), &modal_interaction).await {
                    error!(interaction = ?modal_interaction, error = ?e, "error handling modal interaction");
                }
            }
            _ => {
                warn!(interaction = ?interaction, "received unknown discord interaction type");
            }
        }
    });

    Ok((StatusCode::ACCEPTED, ()).into_response())
}
