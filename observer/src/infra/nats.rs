use std::fmt;
use std::io;
use std::sync::OnceLock;

use crate::infra::config::NatsConfig;

// works like a singleton, with `init` and `get`
static NATS_CLIENT: OnceLock<async_nats::Client> = OnceLock::new();

/// Connects to the NATS server and stores the client in the singleton.
pub async fn init(config: &NatsConfig) -> Result<(), io::Error> {
    let client = prepare_connection(config)?
        .connect(&config.address)
        .await
        .map_err(io::Error::other)?;
    let _ = NATS_CLIENT.set(client);
    Ok(())
}

pub fn get() -> &'static async_nats::Client {
    NATS_CLIENT.get().expect("NATS client not initialized")
}

/// Subjects that events are published to. TODO: maybe Subject shouldn't be in this file?
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Subject {
    RawMempool,
}

impl Subject {
    pub fn as_str(&self) -> &'static str {
        match self {
            Subject::RawMempool => "mempool.rawmempool",
        }
    }
}

impl fmt::Display for Subject {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl AsRef<str> for Subject {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
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
