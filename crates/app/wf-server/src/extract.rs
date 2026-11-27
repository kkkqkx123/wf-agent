//! Common extractors shared by handlers across domains.

use serde::Deserialize;
use utoipa::IntoParams;

/// `{id}` path parameter used by `/workflows/{id}`, `/executions/{id}` and
/// other single-resource routes.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Path)]
pub(crate) struct IdPath {
    pub(crate) id: String,
}

/// `{id}` + `{nodeId}` path pair used by nested resource routes.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Path)]
#[serde(rename_all = "camelCase")]
pub(crate) struct IdNodePath {
    pub(crate) id: String,
    pub(crate) node_id: String,
}

/// `{id}` + `{version}` path pair used by workflow version routes.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Path)]
pub(crate) struct IdVersionPath {
    pub(crate) id: String,
    pub(crate) version: String,
}

/// `{id}` + `{node_id}` path pair used by nested agent-loop routes (snake_case
/// placeholder, matching the URL template).
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Path)]
pub(crate) struct IdSnakeNodePath {
    pub(crate) id: String,
    pub(crate) node_id: String,
}

/// `{id}` + `{injection_id}` path pair used by variable injection routes.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Path)]
pub(crate) struct IdInjectionPath {
    pub(crate) id: String,
    pub(crate) injection_id: String,
}

/// `{id}` + `{cid}` path pair used by checkpoint restore routes.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Path)]
pub(crate) struct IdCidPath {
    pub(crate) id: String,
    pub(crate) cid: String,
}

/// `{cid}` path parameter used by checkpoint restore routes.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Path)]
pub(crate) struct CidPath {
    pub(crate) cid: String,
}

/// `{id}` + `{name}` path pair used by variable routes.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Path)]
pub(crate) struct IdNamePath {
    pub(crate) id: String,
    pub(crate) name: String,
}

/// `{name}` path parameter used by by-name routes.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Path)]
pub(crate) struct NamePath {
    pub(crate) name: String,
}

/// `{defId}` path parameter used by by-definition routes.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Path)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DefIdPath {
    pub(crate) def_id: String,
}

/// `{status}` path parameter used by by-status routes.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Path)]
pub(crate) struct StatusPath {
    pub(crate) status: String,
}

/// `{id}` + `{errorId}` path pair used by error analysis routes.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Path)]
#[serde(rename_all = "camelCase")]
pub(crate) struct IdErrorPath {
    pub(crate) id: String,
    pub(crate) error_id: String,
}

/// `{executionId}` path parameter used by by-execution routes.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Path)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ExecutionIdPath {
    pub(crate) execution_id: String,
}

/// `{entityId}` path parameter used by checkpoint entity routes.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Path)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EntityIdPath {
    pub(crate) entity_id: String,
}

/// Shared `limit` / `offset` pagination query parameters.
#[derive(Deserialize, Debug, Default, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct ListQuery {
    pub(crate) limit: Option<u64>,
    pub(crate) offset: Option<u64>,
}

/// Shared `limit` / `cursor` pagination query parameters for cursor-paged
/// timelines and listings. The cursor is opaque: callers pass back the
/// `next_cursor` of the previous page verbatim.
#[derive(Deserialize, Debug, Default, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct CursorQuery {
    pub(crate) limit: Option<u64>,
    pub(crate) cursor: Option<String>,
}

/// `{code}` path parameter used by pairing connect routes.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Path)]
pub(crate) struct CodePath {
    pub(crate) code: String,
}

/// `?code=` query used by the pairing connect endpoint for clients that
/// cannot place the code in the path.
#[derive(Deserialize, Debug, Default, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct CodeQuery {
    pub(crate) code: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn camel_case_path_params_match_route_placeholders() {
        let path: IdNodePath = serde_json::from_value(serde_json::json!({
            "id": "wf-1",
            "nodeId": "node-1"
        }))
        .expect("camelCase node path");
        assert_eq!(path.id, "wf-1");
        assert_eq!(path.node_id, "node-1");

        let path: IdErrorPath = serde_json::from_value(serde_json::json!({
            "id": "exec-1",
            "errorId": "error-1"
        }))
        .expect("camelCase error path");
        assert_eq!(path.error_id, "error-1");
    }

    #[test]
    fn snake_case_path_params_match_route_placeholders() {
        let path: IdSnakeNodePath = serde_json::from_value(serde_json::json!({
            "id": "loop-1",
            "node_id": "node-1"
        }))
        .expect("snake_case node path");
        assert_eq!(path.node_id, "node-1");

        let path: IdInjectionPath = serde_json::from_value(serde_json::json!({
            "id": "exec-1",
            "injection_id": "inject-1"
        }))
        .expect("injection path");
        assert_eq!(path.injection_id, "inject-1");
    }

    #[test]
    fn missing_path_param_is_rejected() {
        let result = serde_json::from_value::<IdVersionPath>(serde_json::json!({
            "id": "wf-1"
        }));
        assert!(result.is_err(), "version placeholder must be present");
    }
}
