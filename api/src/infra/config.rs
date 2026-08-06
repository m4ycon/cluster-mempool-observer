use observer::infra::config::Config as ObserverConfig;
use shared::env::{ConfigError, env_or, env_req, load_dotenv_from};
use shared::logging::LoggingConfig;
use shared::metrics::MetricsConfig;
use std::path::Path;

const DEFAULT_BIND: &str = "127.0.0.1:3333";

/// API server configuration, loaded from environment variables
#[derive(Debug, Clone)]
pub struct ApiConfig {
    /// `host:port` the HTTP server binds to
    pub bind: String,

    /// Database connection settings
    pub database: DatabaseConfig,

    /// Logging / tracing settings
    pub logging: LoggingConfig,

    /// Prometheus recorder
    pub metrics: MetricsConfig,

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
            logging: LoggingConfig::default(),
            metrics: MetricsConfig::default(),
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
            logging: LoggingConfig::from_env()?,
            metrics: MetricsConfig::from_env()?,
            observer: ObserverConfig::from_env()?,
        })
    }
}

#[cfg(test)]
mod api_from_env_tests {
    use super::*;

    #[test]
    fn from_env_reads_bind_and_database() {
        for k in [
            "BIND",
            "DATABASE_URL",
            "RPC_HOST",
            "RPC_USER",
            "RPC_PASS",
            "ZMQ_BLOCKS_ENDPOINT",
        ] {
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
            std::env::set_var("ZMQ_BLOCKS_ENDPOINT", "tcp://127.0.0.1:28332");
            std::env::set_var("LOG_LEVEL", "warn");
            std::env::set_var("LOG_TO_FILE", "false");
            std::env::set_var("LOG_DIR", "/var/log/api");
            std::env::set_var("LOG_MAX_FILES", "14");
        }
        let cfg = ApiConfig::from_env().unwrap();
        assert_eq!(cfg.bind, "127.0.0.1:3333"); // default
        assert_eq!(cfg.database.url, "postgres://x/y");
        assert_eq!(cfg.observer.rpc.host, "h");
        assert_eq!(cfg.logging.level, "warn");
        assert!(!cfg.logging.to_file);
        assert_eq!(cfg.logging.dir, "/var/log/api");
        assert_eq!(cfg.logging.max_files, 14);
    }
}
