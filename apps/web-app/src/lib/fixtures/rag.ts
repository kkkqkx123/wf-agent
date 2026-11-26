import type { RagPreviewHit, RagSourceStatusEntry, RagStats } from '#lib/services/rag';

export const ragStats: RagStats = {
	collection: 'kb',
	namespace: 'kb',
	point_count: 184,
	keyword_point_count: 179,
};

export const ragPreviewHits: RagPreviewHit[] = [
	{
		chunk_id: 'handbook.md#3',
		source: 'handbook.md',
		excerpt:
			'Approvals are recorded as file-checkpoint references; resolving one never rewrites the surrounding history.',
		score: 0.914,
	},
	{
		chunk_id: 'runbook.md#11',
		source: 'runbook.md',
		excerpt:
			'When a tool call fails, keep the original error code and attach a suggested next action for the model.',
		score: 0.821,
	},
	{
		chunk_id: 'https://example.com/docs/architecture#2',
		source: 'https://example.com/docs/architecture',
		excerpt:
			'Retrieval reads from the indexed namespace and returns ranked passages with a stable chunk locator.',
		score: 0.763,
	},
];

export const ragSources: RagSourceStatusEntry[] = [
	{
		state: {
			collection: 'kb',
			namespace: 'kb',
			source: 'handbook.md',
			kind: 'file',
			uri: 'docs/handbook.md',
			converter: null,
			vector_kind: 'dense',
			encoder: null,
			encoder_fingerprint: null,
			encoder_schema: null,
			content_hash: 'c81e728d9d4c2f636f067f89cc14862c',
			etag: null,
			last_modified: null,
			fetched_at: '2026-01-18T09:12:00Z',
			documents: 1,
			chunks: 24,
			points: 24,
			status: 'active',
			error: null,
		},
		point_count: 24,
		keyword_point_count: 23,
	},
	{
		state: {
			collection: 'kb',
			namespace: 'kb',
			source: 'https://example.com/docs/architecture',
			kind: 'url',
			uri: 'https://example.com/docs/architecture',
			converter: null,
			vector_kind: 'dense',
			encoder: null,
			encoder_fingerprint: null,
			encoder_schema: null,
			content_hash: '5d41402abc4b2a76b9719d911017c592',
			etag: '"arch-v4"',
			last_modified: 'Thu, 15 Jan 2026 10:00:00 GMT',
			fetched_at: '2026-01-19T11:40:00Z',
			documents: 1,
			chunks: 18,
			points: 18,
			status: 'active',
			error: null,
		},
		point_count: 18,
		keyword_point_count: 18,
	},
	{
		state: {
			collection: 'kb',
			namespace: 'kb',
			source: 'legacy-guide.md',
			kind: 'file',
			uri: 'docs/legacy-guide.md',
			converter: null,
			vector_kind: 'dense',
			encoder: null,
			encoder_fingerprint: null,
			encoder_schema: null,
			content_hash: '',
			etag: null,
			last_modified: null,
			fetched_at: null,
			documents: 0,
			chunks: 0,
			points: 0,
			status: 'failed',
			error: 'reader could not resolve the configured source root',
		},
		point_count: 0,
		keyword_point_count: null,
	},
];
