use shared::env::{ConfigError, env_opt, env_parse, env_req};

const DEFAULT_POLL_INTERVAL_SECS: u64 = 10;

/// App configuration, loaded from environment variables
#[derive(Debug, Clone)]
pub struct Config {
    /// Bitcoin Core RPC connection config
    pub rpc: RpcConfig,

    /// Bitcoin Core ZMQ publisher endpoints, keyed by stream type
    pub zmq: ZmqConfig,

    /// Minimal seconds between watcher poll cycles
    pub poll_interval_secs: u64,

    /// Enable/disable specific watchers
    pub watchers: WatchersConfig,
}

#[derive(Debug, Clone)]
pub struct RpcConfig {
    /// Bitcoin Core RPC `host:port`
    pub host: String,
    /// RPC username
    pub user: String,
    /// RPC password
    pub pass: String,
    // TODO: add config to use cookie auth (optional)
}

#[derive(Debug, Clone, Default)]
pub struct ZmqConfig {
    /// `hashblock` endpoint (e.g. `tcp://127.0.0.1:28332`) for the block watcher
    pub blocks_endpoint: Option<String>,
}

#[derive(Debug, Clone)]
pub struct WatchersConfig {
    /// Enables the `mempool_delta` watcher (default: true)
    pub mempool_delta: bool,

    /// Enables the block watcher (default: true). Also requires `rpc.zmq_endpoint`
    pub block: bool,
}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        Ok(Config {
            rpc: RpcConfig {
                host: env_req("RPC_HOST")?,
                user: env_req("RPC_USER")?,
                pass: env_req("RPC_PASS")?,
            },
            zmq: ZmqConfig {
                blocks_endpoint: env_opt("ZMQ_BLOCKS_ENDPOINT"),
            },
            poll_interval_secs: env_parse("POLL_INTERVAL_SECS", DEFAULT_POLL_INTERVAL_SECS)?,
            watchers: WatchersConfig {
                mempool_delta: env_parse("WATCHERS_MEMPOOL_DELTA", true)?,
                block: env_parse("WATCHERS_BLOCK", true)?,
            },
        })
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            rpc: RpcConfig {
                host: String::new(),
                user: String::new(),
                pass: String::new(),
            },
            zmq: ZmqConfig {
                blocks_endpoint: None,
            },
            poll_interval_secs: DEFAULT_POLL_INTERVAL_SECS,
            watchers: WatchersConfig {
                mempool_delta: true,
                block: true,
            },
        }
    }
}

#[cfg(test)]
mod from_env_tests {
    use super::*;

    fn clear() {
        for k in [
            "RPC_HOST",
            "RPC_USER",
            "RPC_PASS",
            "ZMQ_BLOCKS_ENDPOINT",
            "POLL_INTERVAL_SECS",
            "WATCHERS_MEMPOOL_DELTA",
            "WATCHERS_BLOCK",
        ] {
            unsafe { std::env::remove_var(k) };
        }
    }

    #[test]
    fn from_env_full() {
        clear();
        unsafe {
            std::env::set_var("RPC_HOST", "127.0.0.1:8332");
            std::env::set_var("RPC_USER", "u");
            std::env::set_var("RPC_PASS", "p");
            std::env::set_var("ZMQ_BLOCKS_ENDPOINT", "tcp://127.0.0.1:28332");
            std::env::set_var("POLL_INTERVAL_SECS", "5");
            std::env::set_var("WATCHERS_BLOCK", "false");
        }
        let cfg = Config::from_env().unwrap();
        assert_eq!(cfg.rpc.host, "127.0.0.1:8332");
        assert_eq!(cfg.rpc.user, "u");
        assert_eq!(cfg.rpc.pass, "p");
        assert_eq!(
            cfg.zmq.blocks_endpoint.as_deref(),
            Some("tcp://127.0.0.1:28332")
        );
        assert_eq!(cfg.poll_interval_secs, 5);
        assert!(!cfg.watchers.block);
        assert!(cfg.watchers.mempool_delta); // default
    }

    #[test]
    fn from_env_defaults_and_missing_required() {
        clear();
        let err = Config::from_env().unwrap_err();
        assert!(matches!(err, ConfigError::Missing(k) if k == "RPC_HOST"));

        unsafe {
            std::env::set_var("RPC_HOST", "h");
            std::env::set_var("RPC_USER", "u");
            std::env::set_var("RPC_PASS", "p");
        }
        let cfg = Config::from_env().unwrap();
        assert_eq!(cfg.poll_interval_secs, 10); // default
        assert_eq!(cfg.zmq.blocks_endpoint, None);
    }
}
