#![cfg_attr(feature = "strict", deny(warnings))]

/// Helpers to call a retriever over the in-process bus.
pub mod call_retriever;
/// Config builders for integration tests.
pub mod config;
/// Utilities for driving a Bitcoin Core node in integration tests.
pub mod node;
