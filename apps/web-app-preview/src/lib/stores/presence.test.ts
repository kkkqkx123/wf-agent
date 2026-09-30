import { describe, expect, it } from 'vitest';
import { colorForClient, livePeers } from './presence.svelte';

describe('colorForClient', () => {
	it('is stable per client id', () => {
		expect(colorForClient('abc')).toBe(colorForClient('abc'));
	});

	it('stays inside the palette', () => {
		const palette = new Set(
			['a', 'b', 'c', 'tab-1', 'tab-2', 'owner-xyz'].map(colorForClient),
		);
		for (const color of palette) {
			expect(color).toMatch(/^#[0-9a-f]{6}$/);
		}
	});
});

describe('livePeers', () => {
	it('drops the local client and expired frames', () => {
		const now = 1_000_000;
		const peers = livePeers(
			{
				self: {
					clientId: 'self',
					name: 'me',
					color: '#f97316',
					x: 1,
					y: 2,
					at: now,
				},
				fresh: {
					clientId: 'other',
					name: 'peer',
					color: '#8b5cf6',
					x: 3,
					y: 4,
					at: now - 1_000,
				},
				stale: {
					clientId: 'ghost',
					name: 'gone',
					color: '#0ea5e9',
					x: 5,
					y: 6,
					at: now - 30_000,
				},
			},
			'self',
			now,
		);
		expect(peers).toEqual([
			{ clientId: 'other', name: 'peer', color: '#8b5cf6', x: 3, y: 4 },
		]);
	});
});
