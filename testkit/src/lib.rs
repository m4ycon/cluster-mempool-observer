#![cfg_attr(feature = "strict", deny(warnings))]

/// Config builders for integration tests.
pub mod config;
/// Utilities for driving a Bitcoin Core node in integration tests.
pub mod node;
/// Postgres containers for diesel-async integration tests.
pub mod postgres;
