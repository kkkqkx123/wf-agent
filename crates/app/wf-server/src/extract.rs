//! Common extractors shared by handlers across domains.

use serde::Deserialize;

/// `{id}` path parameter used by `/workflows/{id}`, `/executions/{id}` and
/// other single-resource routes.
#[derive(Deserialize)]
pub(crate) struct IdPath {
    pub(crate) id: String,
}

/// `{id}` + `{nodeId}` path pair used by nested resource routes.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct IdNodePath {
    pub(crate) id: String,
    pub(crate) node_id: String,
}

/// `{id}` + `{version}` path pair used by workflow version routes.
#[derive(Deserialize)]
pub(crate) struct IdVersionPath {
    pub(crate) id: String,
    pub(crate) version: String,
}

/// `{id}` + `{cid}` path pair used by checkpoint restore routes.
#[derive(Deserialize)]
pub(crate) struct IdCidPath {
    pub(crate) id: String,
    pub(crate) cid: String,
}

/// `{cid}` path parameter used by checkpoint restore routes.
#[derive(Deserialize)]
pub(crate) struct CidPath {
    pub(crate) cid: String,
}

/// `{id}` + `{name}` path pair used by variable / trigger routes.
#[derive(Deserialize)]
pub(crate) struct IdNamePath {
    pub(crate) id: String,
    pub(crate) name: String,
}

/// `{name}` path parameter used by by-name routes.
#[derive(Deserialize)]
pub(crate) struct NamePath {
    pub(crate) name: String,
}

/// `{tid}` path parameter used by trigger routes.
#[derive(Deserialize)]
pub(crate) struct TidPath {
    pub(crate) tid: String,
}

/// `{defId}` path parameter used by by-definition routes.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DefIdPath {
    pub(crate) def_id: String,
}

/// `{status}` path parameter used by by-status routes.
#[derive(Deserialize)]
pub(crate) struct StatusPath {
    pub(crate) status: String,
}

/// `{id}` + `{errorId}` path pair used by error analysis routes.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct IdErrorPath {
    pub(crate) id: String,
    pub(crate) error_id: String,
}

/// `{executionId}` path parameter used by by-execution routes.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ExecutionIdPath {
    pub(crate) execution_id: String,
}

/// `{entityId}` path parameter used by checkpoint entity routes.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EntityIdPath {
    pub(crate) entity_id: String,
}

/// Shared `limit` / `offset` pagination query parameters.
#[derive(Deserialize, Default)]
pub(crate) struct ListQuery {
    pub(crate) limit: Option<u64>,
    pub(crate) offset: Option<u64>,
}
