import { describe, expect, it } from 'vitest';
import { toBackendRows } from './backends';

function row(overrides: Record<string, unknown> = {}): unknown {
	return {
		name: 'alpha',
		entry: {
			host: 'example',
			username: 'dev',
		},
		url: 'http://127.0.0.1:18080',
		status: { reachable: true, latency_ms: 12, version: '1.0.0', api_version: 'v1' },
		tunnel: { running: true },
		...overrides,
	};
}

describe('toBackendRows', () => {
	it('returns no rows for nullish or malformed views', () => {
		expect(toBackendRows(null)).toEqual([]);
		expect(toBackendRows(undefined)).toEqual([]);
		expect(toBackendRows({} as never)).toEqual([]);
		expect(toBackendRows({ backends: null } as never)).toEqual([]);
	});

	it('maps a full row', () => {
		const [mapped] = toBackendRows({ backends: [row()], source: 'server' } as never);
		expect(mapped).toEqual({
			name: 'alpha',
			url: 'http://127.0.0.1:18080',
			reachable: true,
			latencyMs: 12,
			version: '1.0.0',
			apiVersion: 'v1',
			tunnelRunning: true,
			interruptedReason: null,
		});
	});

	it('degrades absent optional fields instead of failing', () => {
		const [mapped] = toBackendRows({
			backends: [
				row({
					status: { reachable: false },
					tunnel: { running: false, interrupted_reason: 'tunnel exited: x' },
				}),
			],
			source: 'server',
		} as never);
		expect(mapped.reachable).toBe(false);
		expect(mapped.latencyMs).toBeNull();
		expect(mapped.version).toBeNull();
		expect(mapped.interruptedReason).toBe('tunnel exited: x');
	});

	it('sorts rows by name', () => {
		const rows = toBackendRows({
			backends: [row({ name: 'beta' }), row({ name: 'alpha' })],
			source: 'server',
		} as never);
		expect(rows.map((r) => r.name)).toEqual(['alpha', 'beta']);
	});
});
