use crate::db::NATIVE_RESOLUTION_SECS;
use observer::infra::config::Config as ObserverConfig;
use shared::env::{ConfigError, env_or, env_parse, env_req, load_dotenv_from};
use shared::logging::LoggingConfig;
use shared::metrics::MetricsConfig;
use std::path::Path;

const DEFAULT_BIND: &str = "127.0.0.1:3333";

const DEFAULT_DATA_DIR: &str = "data";

/// Default cadence for the `mempool_snapshots` sampler.
const DEFAULT_SNAPSHOT_INTERVAL_SECS: u64 = 60;

/// Default capacity of the tx backfill queue.
const DEFAULT_TX_BACKFILL_QUEUE_CAPACITY: usize = 100_000;

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

    /// Where the disk-usage sampler looks for the service data directories
    pub data_dir: String,

    /// Seconds between `mempool_snapshots` samples
    pub snapshot_interval_secs: u64,

    /// Capacity of the tx backfill queue
    pub tx_backfill_queue_capacity: usize,
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
            data_dir: DEFAULT_DATA_DIR.to_string(),
            snapshot_interval_secs: DEFAULT_SNAPSHOT_INTERVAL_SECS,
            tx_backfill_queue_capacity: DEFAULT_TX_BACKFILL_QUEUE_CAPACITY,
        }
    }
}

impl ApiConfig {
    pub fn from_env() -> Result<Self, ConfigError> {
        load_dotenv_from(Path::new(".env"));

        let snapshot_interval_secs =
            env_parse("SNAPSHOT_INTERVAL_SECS", DEFAULT_SNAPSHOT_INTERVAL_SECS)?;
        if snapshot_interval_secs < NATIVE_RESOLUTION_SECS as u64 {
            return Err(ConfigError::Invalid {
                key: "SNAPSHOT_INTERVAL_SECS".to_string(),
                value: snapshot_interval_secs.to_string(),
                reason: format!("must be >= NATIVE_RESOLUTION_SECS ({NATIVE_RESOLUTION_SECS}s)"),
            });
        }

        Ok(ApiConfig {
            bind: env_or("BIND", DEFAULT_BIND),
            database: DatabaseConfig {
                url: env_req("DATABASE_URL")?,
            },
            logging: LoggingConfig::from_env()?,
            metrics: MetricsConfig::from_env()?,
            observer: ObserverConfig::from_env()?,
            data_dir: env_or("DATA_DIR", DEFAULT_DATA_DIR),
            snapshot_interval_secs,
            tx_backfill_queue_capacity: env_parse(
                "TX_BACKFILL_QUEUE_CAPACITY",
                DEFAULT_TX_BACKFILL_QUEUE_CAPACITY,
            )?,
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
            "SNAPSHOT_INTERVAL_SECS",
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
        assert_eq!(cfg.snapshot_interval_secs, 60); // default
    }

    #[test]
    fn from_env_rejects_snapshot_interval_below_native_resolution() {
        unsafe { std::env::set_var("SNAPSHOT_INTERVAL_SECS", "30") };

        let err = ApiConfig::from_env().unwrap_err();
        match err {
            ConfigError::Invalid { key, value, reason } => {
                assert_eq!(key, "SNAPSHOT_INTERVAL_SECS");
                assert_eq!(value, "30");
                assert!(
                    reason.contains("60"),
                    "reason should name NATIVE_RESOLUTION_SECS: {reason}"
                );
            }
            other => panic!("expected ConfigError::Invalid, got {other:?}"),
        }
    }

    #[test]
    fn from_env_accepts_snapshot_interval_at_or_above_native_resolution() {
        for k in [
            "DATABASE_URL",
            "RPC_HOST",
            "RPC_USER",
            "RPC_PASS",
            "ZMQ_BLOCKS_ENDPOINT",
        ] {
            unsafe { std::env::remove_var(k) };
        }
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

        unsafe { std::env::set_var("SNAPSHOT_INTERVAL_SECS", "60") };
        assert_eq!(ApiConfig::from_env().unwrap().snapshot_interval_secs, 60);

        unsafe { std::env::set_var("SNAPSHOT_INTERVAL_SECS", "120") };
        assert_eq!(ApiConfig::from_env().unwrap().snapshot_interval_secs, 120);
    }
}
