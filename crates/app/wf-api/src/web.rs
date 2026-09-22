//! Web-facing support state: UI preferences and favorites.
//!
//! Both live in the context's persistence layer as single snapshot documents,
//! so no new storage entity is needed and durability follows the configured
//! persistence backend (memory in tests, file-backed in production).

pub mod favorites;
pub mod preferences;
