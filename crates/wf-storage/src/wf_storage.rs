use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use serde::{Serialize, Deserialize};

include!("adapter.rs");
include!("memory.rs");
#[cfg(feature = "sqlite")]
include!("sqlite.rs");
#[cfg(feature = "postgres")]
include!("postgres.rs");
