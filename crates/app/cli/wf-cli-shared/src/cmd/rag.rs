//! Retrieval management commands over the RAG HTTP routes.

use crate::args::{Cli, RagSourceSub, RagSub};
use crate::cmd::render::render_envelope;
use crate::connection::ClientHandle;
use crate::error::{CliError, CliResult};
use crate::output::OutputEnvelope;

pub async fn run(cli: &Cli, sub: &RagSub) -> CliResult<()> {
    let handle = ClientHandle::from_cli(cli).await?;
    let client = handle.client();
    match sub {
        RagSub::Stats {
            collection,
            namespace,
        } => {
            let path = query_path(
                "/api/v1/rag/stats",
                &[
                    ("collection", Some(collection.clone())),
                    ("namespace", Some(namespace.clone())),
                ],
            );
            let data: serde_json::Value = client.get_json(&path).await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("rag-stats", data).with_entity(collection.clone()),
            )
        }
        RagSub::Preview {
            query,
            top_k,
            excerpt_chars,
            collection,
            namespace,
        } => {
            let path = query_path(
                "/api/v1/rag/preview",
                &[
                    ("collection", Some(collection.clone())),
                    ("namespace", Some(namespace.clone())),
                    ("query", Some(query.clone())),
                    ("top_k", Some(top_k.to_string())),
                    ("excerpt_chars", Some(excerpt_chars.to_string())),
                ],
            );
            let data: serde_json::Value = client.get_json(&path).await?;
            crate::rag_render::render_hits(cli.output, &data)?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("rag-preview", data).with_entity(query.clone()),
            )
        }
        RagSub::Diagnose {
            query,
            top_k,
            filter_fields,
            score_threshold,
            collection,
            namespace,
        } => {
            let path = query_path(
                "/api/v1/rag/diagnose",
                &[
                    ("query", Some(query.clone())),
                    ("top_k", Some(top_k.to_string())),
                    ("filter_fields", Some(filter_fields.to_string())),
                    (
                        "score_threshold",
                        score_threshold.map(|value| value.to_string()),
                    ),
                    ("collection", collection.clone()),
                    ("namespace", namespace.clone()),
                ],
            );
            let data: serde_json::Value = client.get_json(&path).await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("rag-diagnose", data).with_entity(query.clone()),
            )
        }
        RagSub::Sources { sub } => run_sources(cli, client, sub).await,
    }
}

async fn run_sources(cli: &Cli, client: &wf_client::WfClient, sub: &RagSourceSub) -> CliResult<()> {
    match sub {
        RagSourceSub::List {
            collection,
            namespace,
        } => {
            let path = query_path(
                "/api/v1/rag/sources",
                &[
                    ("collection", Some(collection.clone())),
                    ("namespace", Some(namespace.clone())),
                ],
            );
            let data: serde_json::Value = client.get_json(&path).await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("rag-source-list", data).with_entity(collection.clone()),
            )
        }
        RagSourceSub::Add {
            source,
            uri,
            kind,
            collection,
            namespace,
            converter,
            options,
            force,
        } => {
            let body = source_body(
                source,
                uri,
                kind,
                collection,
                namespace,
                converter.as_deref(),
                options.as_deref(),
                *force,
            )?;
            let data: serde_json::Value = client.post_json("/api/v1/rag/sources", &body).await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("rag-source-add", data).with_entity(source.clone()),
            )
        }
        RagSourceSub::Reindex {
            source,
            uri,
            kind,
            collection,
            namespace,
            converter,
            options,
        } => {
            let body = source_body(
                source,
                uri,
                kind,
                collection,
                namespace,
                converter.as_deref(),
                options.as_deref(),
                true,
            )?;
            let data: serde_json::Value = client
                .post_json("/api/v1/rag/sources/reindex", &body)
                .await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("rag-source-reindex", data).with_entity(source.clone()),
            )
        }
        RagSourceSub::Delete {
            source,
            collection,
            namespace,
        } => {
            let body = serde_json::json!({
                "source": source,
                "collection": collection,
                "namespace": namespace,
            });
            let data: serde_json::Value = client
                .delete_json_with_body("/api/v1/rag/sources", &body)
                .await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("rag-source-delete", data).with_entity(source.clone()),
            )
        }
    }
}

fn query_path(base: &str, params: &[(&str, Option<String>)]) -> String {
    let query: Vec<String> = params
        .iter()
        .filter_map(|(key, value)| value.as_ref().map(|value| format!("{key}={value}")))
        .collect();
    if query.is_empty() {
        base.to_string()
    } else {
        format!("{base}?{}", query.join("&"))
    }
}

fn source_body(
    source: &str,
    uri: &str,
    kind: &str,
    collection: &str,
    namespace: &str,
    converter: Option<&str>,
    options: Option<&str>,
    force: bool,
) -> CliResult<serde_json::Value> {
    let options = match options {
        Some(raw) => serde_json::from_str(raw)
            .map_err(|err| CliError::Arguments(format!("invalid source options JSON: {err}")))?,
        None => serde_json::json!({}),
    };
    Ok(serde_json::json!({
        "source": source,
        "uri": uri,
        "kind": kind,
        "collection": collection,
        "namespace": namespace,
        "converter": converter,
        "options": options,
        "force": force,
    }))
}
