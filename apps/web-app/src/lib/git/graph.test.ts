import { describe, expect, it } from 'vitest';
import { assignGraphLanes } from './graph';

describe('assignGraphLanes', () => {
	it('keeps a straight line on one lane', () => {
		const rows = assignGraphLanes([
			{ id: 'c', parents: ['b'] },
			{ id: 'b', parents: ['a'] },
			{ id: 'a', parents: [] },
		]);
		expect(rows.map((r) => r.lane)).toEqual([0, 0, 0]);
		expect(rows.map((r) => r.laneCount)).toEqual([1, 1, 1]);
	});

	it('opens a second lane for a merge and collapses it', () => {
		const rows = assignGraphLanes([
			{ id: 'm', parents: ['a', 'b'] },
			{ id: 'a', parents: ['base'] },
			{ id: 'b', parents: ['base'] },
			{ id: 'base', parents: [] },
		]);
		expect(rows[0].lane).toBe(0);
		expect(rows[0].laneCount).toBe(2);
		expect(rows[1].lane).toBe(0);
		expect(rows[2].lane).toBe(1);
		expect(rows[3].lane).toBe(0);
		expect(rows[3].laneCount).toBe(1);
	});

	it('reuses the expecting lane for a branch parent', () => {
		const rows = assignGraphLanes([
			{ id: 'm', parents: ['a', 'b'] },
			{ id: 'b', parents: ['base'] },
			{ id: 'a', parents: ['base'] },
			{ id: 'base', parents: [] },
		]);
		expect(rows[1].lane).toBe(1);
		expect(rows[2].lane).toBe(0);
		expect(rows[3].laneCount).toBe(1);
	});

	it('restarts lanes per pagination window', () => {
		const first = assignGraphLanes([{ id: 'c', parents: ['b'] }]);
		const second = assignGraphLanes([{ id: 'b', parents: ['a'] }]);
		expect(first[0].lane).toBe(0);
		expect(second[0].lane).toBe(0);
		expect(second[0].laneCount).toBe(1);
	});
});
