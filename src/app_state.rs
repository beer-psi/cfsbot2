use std::{fmt::Debug, sync::Arc, time::Duration};

use sqlx::sqlite::{SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use tracing::debug;

use crate::config::Config;

pub struct AppState {
    pub config: Config,

    pub pool: sqlx::SqlitePool,

    pub serenity_verifier: serenity::interactions_endpoint::Verifier,
    pub serenity_http: serenity::http::Http,
}

impl Debug for AppState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppState")
            .field("config", &self.config)
            .field("pool", &self.pool)
            .field("serenity_http", &self.serenity_http)
            .finish()
    }
}

impl AppState {
    pub async fn new() -> Arc<Self> {
        let config = Config::init();

        debug!(options = ?config.database_url, "connecting to database");

        let pool = SqlitePoolOptions::new()
            .max_connections(config.database_max_connections)
            .connect_with(
                config
                    .database_url
                    .clone()
                    .create_if_missing(true)
                    .foreign_keys(true)
                    .journal_mode(SqliteJournalMode::Wal)
                    .synchronous(SqliteSynchronous::Normal)
                    .busy_timeout(Duration::from_secs(5))
                    .optimize_on_close(true, Some(400))
                    .pragma("recursive_triggers", "ON")
                    .pragma("cache_size", "-51200") // 51200 KiB = 50 MiB
                    .pragma("mmap_size", "52428800")
                    .pragma("temp_store", "MEMORY"),
            )
            .await
            .expect("couldn't connect to database");
        let serenity_verifier =
            serenity::interactions_endpoint::Verifier::new(&config.discord_public_key);
        let serenity_http = serenity::http::Http::new(&config.discord_token);

        serenity_http.set_application_id(config.discord_application_id);

        Arc::new(Self {
            config,
            pool,
            serenity_verifier,
            serenity_http,
        })
    }
}
