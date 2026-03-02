//! clify-runtime: Shared runtime for Clify-generated CLI binaries.
//!
//! Provides auth, HTTP client, output formatting, pagination, and config
//! management. Generated CLIs depend on this crate.

pub mod auth;
pub mod client;
pub mod config;
pub mod output;

pub use auth::AuthManager;
pub use client::ApiClient;
pub use config::CliConfig;
pub use output::OutputFormatter;
