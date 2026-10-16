import { describe, expect, it } from 'vitest';
import { StreamRunStore, streamReleaseChunk } from './stream-run.svelte';

describe('streamReleaseChunk', () => {
	it('passes a small backlog through whole', () => {
		expect(streamReleaseChunk(0)).toBe(0);
		expect(streamReleaseChunk(120)).toBe(120);
		expect(streamReleaseChunk(320)).toBe(320);
	});

	it('releases a large backlog proportionally with a floor', () => {
		expect(streamReleaseChunk(1000)).toBe(500);
		expect(streamReleaseChunk(100000)).toBe(50000);
	});

	it('never releases more than the backlog', () => {
		for (const backlog of [321, 800, 5000, 99999]) {
			expect(streamReleaseChunk(backlog)).toBeLessThanOrEqual(backlog);
			expect(streamReleaseChunk(backlog)).toBeGreaterThan(0);
		}
	});
});

describe('StreamRunStore flushing', () => {
	it('drains the whole buffer when the run completes', () => {
		const store = new StreamRunStore();
		store.start('s');
		store.appendDelta('x'.repeat(1000));
		store.appendReasoning('r'.repeat(1000));
		store.complete();
		expect(store.answer).toHaveLength(1000);
		expect(store.reasoning).toHaveLength(1000);
		expect(store.done).toBe(true);
	});

	it('drains the whole buffer when the run fails', () => {
		const store = new StreamRunStore();
		store.start('s');
		store.appendDelta('y'.repeat(900));
		store.fail('boom');
		expect(store.answer).toHaveLength(900);
		expect(store.error).toBe('boom');
	});
});
