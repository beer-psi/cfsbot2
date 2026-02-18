use std::net::SocketAddr;

use axum::{Router, routing::post};
use clap::Parser;
use serenity::all::{CommandOptionType, CreateCommand, CreateCommandOption};
use tokio::{net::TcpListener, signal};
use tower_http::trace::TraceLayer;
use tracing::{error, info};

use crate::{
    app_state::AppState,
    errors::log_app_errors,
    routes::{
        confession::post_confession,
        interaction::{post_interaction, verify_interaction_request},
    },
};

mod app_state;
mod commands;
mod config;
mod errors;
mod models;
mod routes;

#[derive(Parser)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(clap::Subcommand)]
enum Command {
    RegisterCommands,
}

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    tracing_subscriber::fmt()
        .with_ansi(true)
        .with_max_level(if cfg!(debug_assertions) {
            tracing::Level::TRACE
        } else {
            tracing::Level::INFO
        })
        .with_env_filter("sqlx=info,axum=info,cfsbot2=debug")
        .with_file(true)
        .with_line_number(true)
        .init();

    let cli = Cli::parse();
    let app_state = AppState::new().await;

    // we only want to automatically run migrations in production, where we might
    // not have control over the system or have a shell
    #[cfg(not(debug_assertions))]
    {
        use sqlx::migrate::MigrationType;

        let migrator = sqlx::migrate!();

        info!(
            "applying {} migrations",
            migrator
                .iter()
                .filter(|m| m.migration_type == MigrationType::Simple
                    || m.migration_type == MigrationType::ReversibleUp)
                .count()
        );

        migrator
            .run(&app_state.pool)
            .await
            .expect("database migration failed");
    }

    match cli.command {
        Some(Command::RegisterCommands) => {
            let commands = vec![
                CreateCommand::new("confessions")
                    .description("Tạo bài đăng từ các confession đã duyệt")
                    .add_option(
                        CreateCommandOption::new(
                            CommandOptionType::Integer,
                            "limit",
                            "số lượng confession trong bài đăng",
                        )
                        .min_int_value(1)
                        .max_int_value(30),
                    ),
            ];

            info!("registering {} commands", commands.len());

            if let Err(e) = app_state
                .config
                .confessions_guild_id
                .set_commands(&app_state.serenity_http, commands)
                .await
            {
                error!(error = ?e, "could not register commands");
            }
        }
        None => {
            let app = Router::new()
                .route("/confession", post(post_confession))
                .route(
                    "/interaction",
                    post(post_interaction).layer(axum::middleware::from_fn_with_state(
                        app_state.clone(),
                        verify_interaction_request,
                    )),
                )
                .layer(axum::middleware::from_fn(log_app_errors))
                .layer(TraceLayer::new_for_http())
                .with_state(app_state.clone());
            let listener = TcpListener::bind(app_state.config.listen_address)
                .await
                .expect("could not bind to provided listen address");

            info!("listening on {}", app_state.config.listen_address);

            axum::serve(
                listener,
                app.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .with_graceful_shutdown(shutdown_signal())
            .await
            .unwrap();
        }
    }
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}
