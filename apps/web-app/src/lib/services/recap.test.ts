import { describe, expect, it } from 'vitest';
import { shouldRecap } from './recap';

describe('shouldRecap', () => {
	const base = { enabled: true, autonomous: false, away: false, elapsedMs: 1_000, overflowsScreen: false };

	it('stays silent for a short watched turn', () => {
		expect(shouldRecap({ ...base })).toBe(false);
	});

	it('fires when the page was hidden', () => {
		expect(shouldRecap({ ...base, away: true })).toBe(true);
	});

	it('fires for a long turn', () => {
		expect(shouldRecap({ ...base, elapsedMs: 120_000 })).toBe(true);
	});

	it('fires when the answer overflowed one screen', () => {
		expect(shouldRecap({ ...base, overflowsScreen: true })).toBe(true);
	});

	it('stays silent when the display switch is off', () => {
		expect(shouldRecap({ ...base, away: true, elapsedMs: 999_999, overflowsScreen: true, enabled: false })).toBe(false);
	});

	it('stays silent for autonomous turns', () => {
		expect(shouldRecap({ ...base, away: true, autonomous: true })).toBe(false);
	});
});
