#![cfg_attr(feature = "strict", deny(warnings))]

/// Config builders for integration tests.
pub mod config;
/// One-line `Deps` construction for tests: inert or node-backed, isolated or not.
pub mod deps;
/// Builders for the fake domain objects tests assert against.
pub mod fixtures;
/// Capturing and asserting on recorded metrics.
pub mod metrics;
/// Mock implementations for tests.
pub mod mocks;
/// Utilities for driving a Bitcoin Core node in integration tests.
pub mod node;
/// Slot databases for diesel-async integration tests.
pub mod postgres;
/// Waiting on state that only settles asynchronously.
pub mod wait;
