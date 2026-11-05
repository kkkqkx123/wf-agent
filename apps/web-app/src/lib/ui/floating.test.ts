import { describe, expect, it } from 'vitest';
import { resolveFloatingPosition } from './floating';

const viewport = { width: 1280, height: 800 };

describe('resolveFloatingPosition', () => {
	it('prefers the requested side when it fits', () => {
		const placement = resolveFloatingPosition(
			{ x: 100, y: 600, width: 160, height: 36 },
			{ width: 288, height: 200 },
			viewport,
			{ side: 'top', align: 'start', gap: 4, margin: 8 },
		);
		expect(placement.side).toBe('top');
		expect(placement.left).toBe(100);
		expect(placement.top).toBe(600 - 4 - 200);
	});

	it('flips to the roomier side when the preferred side is starved', () => {
		const placement = resolveFloatingPosition(
			{ x: 100, y: 40, width: 160, height: 36 },
			{ width: 288, height: 300 },
			viewport,
			{ side: 'top', align: 'start', gap: 4, margin: 8 },
		);
		expect(placement.side).toBe('bottom');
		expect(placement.top).toBe(40 + 36 + 4);
	});

	it('clamps the panel inside narrow viewports', () => {
		const placement = resolveFloatingPosition(
			{ x: 1200, y: 700, width: 60, height: 30 },
			{ width: 288, height: 400 },
			{ width: 320, height: 500 },
			{ side: 'bottom', align: 'start', gap: 4, margin: 8 },
		);
		expect(placement.left + placement.maxWidth).toBeLessThanOrEqual(320 - 8);
		expect(placement.top + placement.maxHeight).toBeLessThanOrEqual(500 - 8);
		expect(placement.left).toBeGreaterThanOrEqual(8);
		expect(placement.top).toBeGreaterThanOrEqual(8);
	});

	it('centers tooltips on the anchor', () => {
		const placement = resolveFloatingPosition(
			{ x: 100, y: 200, width: 40, height: 20 },
			{ width: 120, height: 28 },
			viewport,
			{ side: 'top', align: 'center', gap: 8, margin: 4 },
		);
		expect(placement.left).toBe(100 + (40 - 120) / 2);
	});
});
