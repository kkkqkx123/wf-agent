import { client } from '#lib/api/client';
import { call, requireData } from '#lib/api/envelope';
import type { components } from '#lib/api/schema';

export type RagScope = {
	collection: string;
	namespace: string;
};

export type RagStats = components['schemas']['RagCollectionStatsView'];
export type RagPreviewHit = components['schemas']['RagPreviewHitView'];
export type RagDiagnosis = components['schemas']['RagNoHitDiagnosisView'];
export type RagSourceStatusEntry = components['schemas']['RagSourceStatusEntry'];
export type RagSourceWrite = components['schemas']['RagSourceWrite'];
export type RagSourceRef = components['schemas']['RagSourceRef'];
export type RagIngestResult = components['schemas']['RagIngestResultView'];

export type RagPreviewParams = RagScope & {
	query: string;
	top_k?: number;
	excerpt_chars?: number;
};

export type RagDiagnoseParams = {
	query: string;
	top_k?: number;
	filter_fields?: number;
	score_threshold?: number;
	collection?: string;
	namespace?: string;
};

export async function ragStats(scope: RagScope): Promise<RagStats> {
	return requireData(
		await call<RagStats>(client.GET('/api/v1/rag/stats', { params: { query: scope } })),
		'RAG stats',
	);
}

export async function ragPreview(params: RagPreviewParams): Promise<RagPreviewHit[]> {
	const { collection, namespace, query, top_k, excerpt_chars } = params;
	return requireData(
		await call<RagPreviewHit[]>(
			client.GET('/api/v1/rag/preview', {
				params: { query: { collection, namespace, query, top_k, excerpt_chars } },
			}),
		),
		'RAG preview',
	);
}

export async function ragDiagnose(params: RagDiagnoseParams): Promise<RagDiagnosis> {
	return requireData(
		await call<RagDiagnosis>(client.GET('/api/v1/rag/diagnose', { params: { query: params } })),
		'RAG diagnosis',
	);
}

export async function ragSources(scope: RagScope): Promise<RagSourceStatusEntry[]> {
	return requireData(
		await call<RagSourceStatusEntry[]>(
			client.GET('/api/v1/rag/sources', { params: { query: scope } }),
		),
		'RAG sources',
	);
}

export async function ingestRagSource(body: RagSourceWrite): Promise<RagIngestResult> {
	return requireData(
		await call<RagIngestResult>(client.POST('/api/v1/rag/sources', { body })),
		'RAG ingest result',
	);
}

/**
 * Rebuilds one source. Only the identity triple travels: reader kind, URI,
 * converter and reader options come from the state the server persisted, so a
 * rebuild cannot change how the source is read.
 */
export async function reindexRagSource(body: RagSourceRef): Promise<RagIngestResult> {
	return requireData(
		await call<RagIngestResult>(client.POST('/api/v1/rag/sources/reindex', { body })),
		'RAG reindex result',
	);
}

export async function deleteRagSource(body: RagSourceRef): Promise<number> {
	return requireData(
		await call<number>(client.DELETE('/api/v1/rag/sources', { body })),
		'RAG delete result',
	);
}
