use serde::Deserialize;
use std::io;

#[derive(Debug, Clone, Deserialize)]
pub struct NatsConfig {
    /// NATS server address (`host:port`) to connect to
    #[serde(default = "default_nats_address")]
    pub address: String,
    /// NATS username for authentication (optional)
    #[serde(default)]
    pub username: Option<String>,
    /// NATS password for authentication (optional)
    #[serde(default)]
    pub password: Option<String>,
}

impl Default for NatsConfig {
    fn default() -> Self {
        Self {
            address: default_nats_address(),
            username: None,
            password: None,
        }
    }
}

pub fn default_nats_address() -> String {
    "127.0.0.1:4222".to_string()
}

/// Connects to the NATS server and returns the client.
pub async fn connect(config: &NatsConfig) -> Result<async_nats::Client, io::Error> {
    prepare_connection(config)?
        .connect(&config.address)
        .await
        .map_err(io::Error::other)
}

/// Populates ConnectOptions with a username and password, if the passed
/// NATS config has one set.
pub fn prepare_connection(config: &NatsConfig) -> Result<async_nats::ConnectOptions, io::Error> {
    match &config.username {
        Some(user) => {
            let pass = &config.password;
            if pass.is_none() {
                tracing::warn!(
                    "No NATS password supplied for connection to NATS server {} with user={}",
                    config.address,
                    user,
                );
            }

            tracing::info!(
                "Connecting to NATS-server {} with user={} and password=***",
                config.address,
                user
            );

            Ok(async_nats::ConnectOptions::new()
                .user_and_password(user.to_string(), pass.clone().unwrap_or_default()))
        }
        None => {
            tracing::debug!(
                "Connecting to NATS-server at {} without authentification",
                config.address
            );
            Ok(async_nats::ConnectOptions::new())
        }
    }
}
