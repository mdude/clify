//! clify-runtime: Shared runtime for Clify-generated CLI binaries.
//!
//! Provides auth, HTTP client, output formatting, pagination, and config
//! management. Generated CLIs depend on this crate so they stay lightweight
//! while sharing common functionality.

pub mod auth;
pub mod client;
pub mod config;
pub mod output;

// TODO: Implement modules in Phase 2
