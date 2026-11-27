//! Retrieval management commands over the RAG HTTP routes.

use crate::args::{Cli, RagSourceSub, RagSub};
use crate::cmd::render::render_envelope;
use crate::connection::ClientHandle;
use crate::error::{CliError, CliResult};
use crate::output::OutputEnvelope;
use wf_client::models::{RagSourceRef, RagSourceWrite};

pub async fn run(cli: &Cli, sub: &RagSub) -> CliResult<()> {
    let handle = ClientHandle::from_cli(cli).await?;
    let client = handle.client();
    match sub {
        RagSub::Stats {
            collection,
            namespace,
        } => {
            let envelope = client
                .generated()
                .handle_rag_stats(collection, namespace)
                .await
                .map_err(wf_client::client::into_typed_client_error)?;
            let data = envelope_value(envelope.into_inner())?;
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
            let envelope = client
                .generated()
                .handle_rag_preview(
                    collection,
                    Some(*excerpt_chars as u64),
                    namespace,
                    query,
                    Some(*top_k as u64),
                )
                .await
                .map_err(wf_client::client::into_typed_client_error)?;
            let data = envelope_value(envelope.into_inner())?;
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
            let envelope = client
                .generated()
                .handle_rag_diagnose(
                    collection.as_deref(),
                    Some(*filter_fields as u64),
                    namespace.as_deref(),
                    query,
                    *score_threshold,
                    Some(*top_k as u64),
                )
                .await
                .map_err(wf_client::client::into_typed_client_error)?;
            let data = envelope_value(envelope.into_inner())?;
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
            let envelope = client
                .generated()
                .handle_rag_sources(collection, namespace)
                .await
                .map_err(wf_client::client::into_typed_client_error)?;
            let data = envelope_value(envelope.into_inner())?;
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
            let body = RagSourceWrite {
                source: source.clone(),
                uri: uri.clone(),
                kind: kind.clone(),
                collection: collection.clone(),
                namespace: namespace.clone(),
                converter: converter.clone(),
                options: parse_options(options.as_deref())?,
                force: Some(*force),
            };
            let envelope = client
                .generated()
                .handle_rag_ingest_source(&body)
                .await
                .map_err(wf_client::client::into_typed_client_error)?;
            let data = envelope_value(envelope.into_inner())?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("rag-source-add", data).with_entity(source.clone()),
            )
        }
        RagSourceSub::Reindex {
            source,
            collection,
            namespace,
        } => {
            let body = source_ref(source, collection, namespace);
            let envelope = client
                .generated()
                .handle_rag_reindex_source(&body)
                .await
                .map_err(wf_client::client::into_typed_client_error)?;
            let data = envelope_value(envelope.into_inner())?;
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
            let body = source_ref(source, collection, namespace);
            let envelope = client
                .generated()
                .handle_rag_delete_source(&body)
                .await
                .map_err(wf_client::client::into_typed_client_error)?;
            let data = envelope_value(envelope.into_inner())?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("rag-source-delete", data).with_entity(source.clone()),
            )
        }
    }
}

/// Unwraps one generated envelope into the JSON the renderers consume.
fn envelope_value<T: serde::Serialize>(envelope: T) -> CliResult<serde_json::Value> {
    wf_client::client::unwrap_generated_envelope(envelope).map_err(CliError::from)
}

/// Identity triple shared by the delete and rebuild routes.
fn source_ref(source: &str, collection: &str, namespace: &str) -> RagSourceRef {
    RagSourceRef {
        source: source.to_string(),
        collection: collection.to_string(),
        namespace: namespace.to_string(),
    }
}

/// Parses reader options, rejecting anything that is not a JSON object.
fn parse_options(raw: Option<&str>) -> CliResult<Option<serde_json::Value>> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    let value: serde_json::Value = serde_json::from_str(raw)
        .map_err(|err| CliError::Arguments(format!("invalid source options JSON: {err}")))?;
    if !value.is_object() {
        return Err(CliError::Arguments(
            "source options must be a JSON object".to_string(),
        ));
    }
    Ok(Some(value))
}
