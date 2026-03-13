//! Common test utilities shared across integration and e2e tests
//!
//! This module re-exports fixtures and mocks to avoid loading them multiple times.

#[path = "../common_fixtures/mod.rs"]
pub mod fixtures;

#[path = "../common_mocks/mod.rs"]
pub mod mocks;
