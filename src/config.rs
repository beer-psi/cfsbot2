use std::net::SocketAddr;

use serenity::all::{ApplicationId, ChannelId, GuildId, RoleId};
use sqlx::sqlite::SqliteConnectOptions;

#[derive(Debug, Clone)]
pub struct Config {
    pub listen_address: SocketAddr,

    pub database_url: SqliteConnectOptions,
    pub database_max_connections: u32,

    pub discord_token: String,
    pub discord_application_id: ApplicationId,
    pub discord_public_key: String,

    pub confessions_guild_id: GuildId,
    pub confessions_channel_id: ChannelId,
    pub confessions_role_id: RoleId,
    pub confessions_submission_url: String,
}

macro_rules! required {
    ($env:expr, $helpful_typ:expr) => {
        std::env::var($env)
            .expect(concat!($env, " must be set"))
            .parse()
            .expect(concat!($env, " should be a valid ", $helpful_typ))
    };
}

macro_rules! optional {
    ($env:expr) => {
        std::env::var($env)
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    };
    ($env:expr, $default:expr, $helpful_typ:expr) => {
        std::env::var($env)
            .ok()
            .map_or_else(|| Ok($default), |v| v.parse())
            .expect(concat!($env, " should be a valid ", $helpful_typ))
    };
}

impl Config {
    pub fn init() -> Self {
        Self {
            listen_address: optional!("LISTEN_ADDRESS", SocketAddr::from(([127, 0, 0, 1], 8000)), "IP address"),

            database_url: std::env::var("DATABASE_URL")
                .expect("DATABASE_URL must be set")
                .parse()
                .expect("DATABASE_URL must be a valid PostgreSQL connection URL. See https://docs.rs/sqlx/0.8.6/sqlx/sqlite/struct.SqliteConnectOptions.html"),
            database_max_connections: optional!("DATABASE_MAX_CONNECTIONS", 100, "non-negative number"),

            discord_token: required!("DISCORD_TOKEN", "discord token"),
            discord_application_id: required!("DISCORD_APPLICATION_ID", "unsigned 64-bit integer"),
            discord_public_key: required!("DISCORD_PUBLIC_KEY", "public key"),

            confessions_guild_id: required!("CONFESSIONS_GUILD_ID", "unsigned 64-bit integer"),
            confessions_channel_id: required!("CONFESSIONS_CHANNEL_ID", "unsigned 64-bit integer"),
            confessions_role_id: required!("CONFESSIONS_ROLE_ID", "unsigned 64-bit integer"),
            confessions_submission_url: required!("CONFESSIONS_SUBMISSION_URL", "URL"),
        }
    }
}
