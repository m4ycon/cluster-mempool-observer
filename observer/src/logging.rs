use tracing::{Level, level_filters::LevelFilter};
use tracing_subscriber::{
    EnvFilter, Layer,
    filter::filter_fn,
    layer::SubscriberExt,
    util::SubscriberInitExt,
};

pub fn init_tracing() {
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

    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::registry()
        .with(env_filter)
        .with(error_layer)
        .with(other_layer)
        .init();
}
