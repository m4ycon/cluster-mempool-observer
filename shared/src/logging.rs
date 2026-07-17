use crate::env::{ConfigError, env_or, env_parse};
use tracing::{Level, level_filters::LevelFilter};
use tracing_appender::{non_blocking::WorkerGuard, rolling::Rotation};
use tracing_subscriber::{
    EnvFilter, Layer, filter::filter_fn, layer::SubscriberExt, util::SubscriberInitExt,
};

const DEFAULT_LOG_LEVEL: &str = "debug";
const DEFAULT_LOG_DIR: &str = "logs";
const DEFAULT_LOG_MAX_FILES: usize = 7;

#[derive(Debug, Clone)]
pub struct LoggingConfig {
    /// Tracing level filter (e.g. `trace`, `debug`, `info`, `warn`, `error`).
    pub level: String,

    /// Whether to persist logs to a daily-rolling file (in addition to stdout).
    pub to_file: bool,

    /// Directory the rolling log files are written to.
    pub dir: String,

    /// Number of rotated daily log files to keep before pruning the oldest.
    pub max_files: usize,
}

impl LoggingConfig {
    pub fn from_env() -> Result<Self, ConfigError> {
        Ok(LoggingConfig {
            level: env_or("LOG_LEVEL", DEFAULT_LOG_LEVEL),
            to_file: env_parse("LOG_TO_FILE", true)?,
            dir: env_or("LOG_DIR", DEFAULT_LOG_DIR),
            max_files: env_parse("LOG_MAX_FILES", DEFAULT_LOG_MAX_FILES)?,
        })
    }
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: DEFAULT_LOG_LEVEL.to_string(),
            to_file: true,
            dir: DEFAULT_LOG_DIR.to_string(),
            max_files: DEFAULT_LOG_MAX_FILES,
        }
    }
}

/// Initialises the global tracing subscriber.
///
/// Always logs to stdout. When `cfg.to_file` is set, also writes a daily-rolling
/// plaintext file (`<dir>/api.<date>.log`) capped at `cfg.max_files` rotations.
#[must_use] // tracing_appender returns a guard that must be kept alive to flush logs on drop
pub fn init_tracing(cfg: &LoggingConfig) -> Option<WorkerGuard> {
    // TODO: not ideal solution (with_line_number), we should get a complete
    // stack trace for errors, but this is good enough for now
    let error_layer = tracing_subscriber::fmt::layer()
        .with_file(true)
        .with_line_number(true)
        .with_filter(LevelFilter::from_level(Level::ERROR));

    let other_layer = tracing_subscriber::fmt::layer()
        .with_file(false)
        .with_line_number(false)
        .with_filter(filter_fn(|meta| *meta.level() > Level::ERROR));

    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new(&cfg.level).add_directive("tokio_postgres=info".parse().unwrap())
    });

    // Persist logs to a daily-rolling file (in addition to stdout)
    let (file_layer, guard) = if cfg.to_file {
        std::fs::create_dir_all(&cfg.dir).expect("failed to create log directory");

        let appender = tracing_appender::rolling::Builder::new()
            .rotation(Rotation::DAILY)
            .filename_prefix("api")
            .filename_suffix("log")
            .max_log_files(cfg.max_files)
            .build(&cfg.dir)
            .expect("failed to build rolling log file appender");

        let (non_blocking, guard) = tracing_appender::non_blocking(appender);

        let layer = tracing_subscriber::fmt::layer()
            .with_writer(non_blocking)
            .with_ansi(false)
            .with_line_number(true);

        (Some(layer), Some(guard))
    } else {
        (None, None)
    };

    tracing_subscriber::registry()
        .with(env_filter)
        .with(error_layer)
        .with(other_layer)
        .with(file_layer)
        .init();

    guard
}
