#![cfg_attr(feature = "strict", deny(warnings))]

/// Config builders for integration tests.
pub mod config;
/// Utilities for driving a Bitcoin Core node in integration tests.
pub mod node;

// DISABLED reference (NATS removed in step 1): kept for restoring real-transport
// test helpers in step 4. `cfg(any())` is always false, so these never compile.
/// Helpers to call a retriever.
#[cfg(any())]
pub mod call_retriever;
/// A `nats-server` runner for integration tests.
#[cfg(any())]
pub mod nats_server;
