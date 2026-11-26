//! Embedded simvec backend: thin adapter over [`simvec::LocalVectorEngine`].
//!
//! The adapter owns no retrieval semantics: filters map to simvec clause
//! groups field by field, scores pass through untouched, and collection
//! lifecycle delegates to the engine. Anything simvec cannot express (for
//! example the contracts-level upsert-overwrite of a whole source) is
//! composed from engine primitives here, never by forking engine code.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use async_trait::async_trait;
use simvec::VectorEngine;

use crate::error::{Result, VectorError};
use crate::filter::{FieldFilter, FilterOperator, SearchFilter};
use crate::store::{CollectionSpec, ScrollPage, ScrollQuery, VectorQuery, VectorStorage};
use crate::types::{
    DistanceMetric, ScoredPoint, VectorData, VectorKind, VectorPoint, VectorSpaceSpec,
    NAMESPACE_KEY, PID_KEY, SOURCE_KEY,
};

mod filter;
mod mapping;
mod store;
#[cfg(test)]
mod test;

/// Embedded local vector backend powered by simvec.
#[derive(Debug, Clone)]
pub struct SimvecVectorStore {
    engine: Arc<simvec::LocalVectorEngine>,
}

impl SimvecVectorStore {
    /// Opens (or creates) the engine rooted at the directory and wraps it.
    pub fn open(dir: impl AsRef<std::path::Path>) -> Result<Self> {
        let engine = simvec::LocalVectorEngine::open(dir)
            .map_err(|err| VectorError::Backend(err.to_string()))?;
        Ok(Self {
            engine: Arc::new(engine),
        })
    }

    /// Wraps an engine the host already owns.
    pub fn from_engine(engine: Arc<simvec::LocalVectorEngine>) -> Self {
        Self { engine }
    }
}

fn map_engine_error(err: simvec::VectorSearchError) -> VectorError {
    match err {
        simvec::VectorSearchError::CollectionNotFound(name) => VectorError::CollectionMissing(name),
        simvec::VectorSearchError::InvalidConfig(message) => VectorError::InvalidInput(message),
        simvec::VectorSearchError::InvalidVectorDimension { expected, actual } => {
            VectorError::DimensionMismatch { expected, actual }
        }
        simvec::VectorSearchError::NonFiniteElement(index) => {
            VectorError::InvalidInput(format!("non-finite vector element at index {index}"))
        }
        other => VectorError::Backend(other.to_string()),
    }
}

fn map_point_error(err: simvec::VectorSearchError, collection: &str, id: &str) -> VectorError {
    match err {
        simvec::VectorSearchError::InvalidPointId(_) => VectorError::InvalidInput(format!(
            "point '{id}' not found in collection '{collection}'"
        )),
        simvec::VectorSearchError::CollectionNotFound(name) => VectorError::CollectionMissing(name),
        other => map_engine_error(other),
    }
}
