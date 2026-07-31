#![cfg_attr(feature = "strict", deny(warnings))]

/// Config builders for integration tests.
pub mod config;
/// One-line `Deps` construction for tests: inert or node-backed, isolated or not.
pub mod deps;
/// Capturing and asserting on recorded metrics.
pub mod metrics;
/// Mock implementations for tests.
pub mod mocks;
/// Utilities for driving a Bitcoin Core node in integration tests.
pub mod node;
/// Postgres containers for diesel-async integration tests.
pub mod postgres;
