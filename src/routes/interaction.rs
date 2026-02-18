use std::sync::Arc;

use axum::{
    Json,
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serenity::all::{CreateInteractionResponse, Interaction};
use tracing::{error, warn};

use crate::{
    app_state::AppState,
    commands::{handle_command, handle_component, handle_modal},
    errors::InternalError,
};

pub async fn post_interaction(
    State(app_state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, InternalError> {
    let Some(Ok(signature)) = headers.get("X-Signature-Ed25519").map(|v| v.to_str()) else {
        return Ok(StatusCode::UNAUTHORIZED.into_response());
    };
    let Some(Ok(timestamp)) = headers.get("X-Signature-Timestamp").map(|v| v.to_str()) else {
        return Ok(StatusCode::UNAUTHORIZED.into_response());
    };

    if app_state
        .serenity_verifier
        .verify(signature, timestamp, &body)
        .is_err()
    {
        return Ok(StatusCode::UNAUTHORIZED.into_response());
    }

    let Ok(interaction) = serde_json::from_slice::<Interaction>(&body) else {
        return Ok(StatusCode::BAD_REQUEST.into_response());
    };

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
