use observer::infra::config::Config as ObserverConfig;
use shared::env::{ConfigError, env_or, env_req, load_dotenv_from};
use std::path::Path;

const DEFAULT_BIND: &str = "127.0.0.1:3333";

/// API server configuration, loaded from environment variables
#[derive(Debug, Clone)]
pub struct ApiConfig {
    /// `host:port` the HTTP server binds to
    pub bind: String,

    /// Database connection settings
    pub database: DatabaseConfig,

    /// Observer-side config
    pub observer: ObserverConfig,
}

/// Postgres connection settings
#[derive(Debug, Clone)]
pub struct DatabaseConfig {
    pub url: String,
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            bind: DEFAULT_BIND.to_string(),
            database: DatabaseConfig { url: String::new() },
            observer: ObserverConfig::default(),
        }
    }
}

impl ApiConfig {
    pub fn from_env() -> Result<Self, ConfigError> {
        load_dotenv_from(Path::new(".env"));
        Ok(ApiConfig {
            bind: env_or("BIND", DEFAULT_BIND),
            database: DatabaseConfig {
                url: env_req("DATABASE_URL")?,
            },
            observer: ObserverConfig::from_env()?,
        })
    }
}

#[cfg(test)]
mod api_from_env_tests {
    use super::*;

    #[test]
    fn from_env_reads_bind_and_database() {
        for k in ["BIND", "DATABASE_URL", "RPC_HOST", "RPC_USER", "RPC_PASS"] {
            unsafe { std::env::remove_var(k) };
        }
        // DATABASE_URL required -> error first.
        let err = ApiConfig::from_env().unwrap_err();
        assert!(matches!(&err, ConfigError::Missing(k) if k == "DATABASE_URL"));

        unsafe {
            std::env::set_var("DATABASE_URL", "postgres://x/y");
            std::env::set_var("RPC_HOST", "h");
            std::env::set_var("RPC_USER", "u");
            std::env::set_var("RPC_PASS", "p");
        }
        let cfg = ApiConfig::from_env().unwrap();
        assert_eq!(cfg.bind, "127.0.0.1:3333"); // default
        assert_eq!(cfg.database.url, "postgres://x/y");
        assert_eq!(cfg.observer.rpc.host, "h");
    }
}
