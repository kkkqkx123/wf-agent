//! Shared infrastructure: error types, context, config parsing, events,
//! streams, subscriptions, persistence and metrics helpers.

pub mod config;
pub mod context;
pub mod diagnostics;
pub mod error;
pub mod events;
pub mod handler_chain;
pub mod metrics;
pub mod persistence;
pub mod reference;
pub mod state_tracker;
pub mod stream;
pub mod subscription;
pub mod util;
