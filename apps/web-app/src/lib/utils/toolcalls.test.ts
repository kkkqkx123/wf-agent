import { describe, expect, it } from 'vitest';
import { inferToolKind, parseRagCitations } from './toolcalls';

describe('inferToolKind', () => {
	it('classifies retrieval tools as knowledge before generic search rules', () => {
		expect(inferToolKind('rag_search')).toBe('knowledge');
		expect(inferToolKind('rag_index_status')).toBe('knowledge');
		expect(inferToolKind('web_search')).toBe('network');
	});
});

describe('parseRagCitations', () => {
	it('reads tool hit arrays with citation locators', () => {
		const citations = parseRagCitations(
			JSON.stringify([
				{
					chunk_id: 'handbook.md#3',
					source: 'handbook.md',
					content: 'Approvals are checkpoint references.',
					score: 0.9123,
				},
				{ chunk_id: 'other.md#1', excerpt: 'Second passage.' },
			]),
		);
		expect(citations).toHaveLength(2);
		expect(citations[0].chunkId).toBe('handbook.md#3');
		expect(citations[0].score).toBeCloseTo(0.9123);
		expect(citations[1].content).toBe('Second passage.');
	});

	it('returns no citations for non-array output', () => {
		expect(parseRagCitations('{"chunk_id":"x"}')).toEqual([]);
		expect(parseRagCitations('not json')).toEqual([]);
	});
});
