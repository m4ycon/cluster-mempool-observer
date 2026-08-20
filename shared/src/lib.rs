#![cfg_attr(feature = "strict", deny(warnings))]

pub mod api;
pub mod env;
pub mod events;
pub mod logging;
pub mod metrics;
pub mod models;
pub mod pubsub;
pub mod snapshot;
pub mod subjects;
pub mod ws;

/// Where ts-rs writes the generated TypeScript bindings. Relative to ts-rs'
/// default export dir (`shared/bindings/`), so `../../web/...` lands at the repo
/// root `web/src/types/generated/`.
pub(crate) const TS_EXPORT_DIR: &str = "../../web/src/types/generated/";
