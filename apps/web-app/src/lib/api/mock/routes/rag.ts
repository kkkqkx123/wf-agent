/** Mock routes for retrieval stats, preview, diagnosis and source sync. */

import { ragPreviewHits, ragSources, ragStats } from '#lib/fixtures/rag';
import type { RagIngestResult, RagSourceStatusEntry } from '#lib/services/rag';
import type { MockRouter } from '../router';

type SourceBody = {
	source: string;
	uri: string;
	kind: string;
	collection: string;
	namespace: string;
	converter?: string | null;
	force?: boolean;
};

type SourceRefBody = {
	source: string;
	collection: string;
	namespace: string;
};

function ingestResult(body: SourceBody, action: 'insert' | 'update' | 'skip'): RagIngestResult {
	return {
		source: body.source,
		action,
		documents: 1,
		chunks: action === 'skip' ? 0 : 12,
		points: action === 'skip' ? 0 : 12,
		skipped: action === 'skip' ? 1 : 0,
		content_hash: '098f6bcd4621d373cade4e832627b4f6',
		revision: {
			content_hash: '098f6bcd4621d373cade4e832627b4f6',
			etag: body.kind === 'url' ? '"preview-etag"' : null,
			fetched_at: '2026-02-01T08:30:00Z',
			last_modified: null,
		},
		metadata: {},
	};
}

function sameEntry(entry: RagSourceStatusEntry, ref: SourceRefBody): boolean {
	return (
		entry.state.source === ref.source &&
		entry.state.collection === ref.collection &&
		entry.state.namespace === ref.namespace
	);
}

export function registerRag(router: MockRouter): void {
	const sources: RagSourceStatusEntry[] = structuredClone(ragSources);

	const findEntry = (ref: SourceRefBody): RagSourceStatusEntry | undefined =>
		sources.find((entry) => sameEntry(entry, ref));

	router.on('GET', '/api/v1/rag/stats', () => ragStats);

	router.on('GET', '/api/v1/rag/preview', (_p, query) => {
		if (String(query.query ?? '').trim() === '__no_hits__') return [];
		return ragPreviewHits;
	});

	router.on('GET', '/api/v1/rag/diagnose', (_p, query) => {
		const causes = [`query “${String(query.query ?? '')}” was searched against namespace kb`];
		if (Number(query.top_k ?? 5) === 0) causes.push('top_k is zero; no hits can be returned');
		causes.push('chunking may be too coarse for the question; compare a finer target length');
		return { likely_causes: causes };
	});

	router.on('GET', '/api/v1/rag/sources', (_p, query) => {
		const collection = String(query.collection ?? 'kb');
		const namespace = String(query.namespace ?? 'kb');
		return sources.filter(
			(entry) => entry.state.collection === collection && entry.state.namespace === namespace,
		);
	});

	// A rebuild only carries the identity triple; the fixture replays the
	// stored reader shape the way the server replays its persisted state.
	router.on('POST', '/api/v1/rag/sources/reindex', (_p, _query, body) => {
		const ref = body as SourceRefBody;
		const existing = findEntry(ref);
		if (!existing) return ingestResult({ ...ref, uri: '', kind: '' }, 'update');
		const result = ingestResult(
			{ ...ref, uri: existing.state.uri, kind: existing.state.kind },
			'update',
		);
		existing.state.status = 'active';
		existing.state.error = null;
		existing.state.content_hash = result.content_hash;
		existing.state.chunks = result.chunks;
		existing.state.points = result.points;
		existing.point_count = result.points;
		return result;
	});

	router.on('POST', '/api/v1/rag/sources', (_p, _query, body) => {
		const source = body as SourceBody;
		const existing = sources.find(
			(entry) =>
				entry.state.source === source.source &&
				entry.state.collection === source.collection &&
				entry.state.namespace === source.namespace,
		);
		if (existing && !source.force) return ingestResult(source, 'skip');
		const result = ingestResult(source, existing ? 'update' : 'insert');
		if (existing) {
			existing.state.kind = source.kind;
			existing.state.uri = source.uri;
			existing.state.status = 'active';
			existing.state.error = null;
			existing.state.chunks = result.chunks;
			existing.state.points = result.points;
			existing.state.content_hash = result.content_hash;
			existing.point_count = result.points;
		} else {
			sources.push({
				state: {
					collection: source.collection,
					namespace: source.namespace,
					source: source.source,
					kind: source.kind,
					uri: source.uri,
					converter: source.converter ?? null,
					vector_kind: 'dense',
					encoder: null,
					encoder_fingerprint: null,
					encoder_schema: null,
					content_hash: result.content_hash,
					etag: result.revision.etag ?? null,
					last_modified: null,
					fetched_at: result.revision.fetched_at ?? null,
					documents: 1,
					chunks: result.chunks,
					points: result.points,
					status: 'active',
					error: null,
				},
				point_count: result.points,
				keyword_point_count: result.points,
			});
		}
		return result;
	});

	router.on('DELETE', '/api/v1/rag/sources', (_p, _query, body) => {
		const ref = body as SourceRefBody;
		const before = sources.length;
		const index = sources.findIndex((entry) => sameEntry(entry, ref));
		if (index >= 0) sources.splice(index, 1);
		return before - sources.length;
	});
}
