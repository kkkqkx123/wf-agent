/**
 * Preview client path substitution: `{id}` templates must resolve from
 * `params.path` (like openapi-fetch) instead of reaching the mock router
 * as the literal string '{id}', which made every GET/PUT/DELETE by id 404.
 */
import { describe, expect, it } from 'vitest';
import { client } from './client.preview';
import { call, requireData } from './envelope';

interface PtyView {
	id: string;
	title: string;
}

describe('preview client path params', () => {
	it('round-trips a pty session through path-param routes', async () => {
		const created = requireData<PtyView>(
			await call(client.POST('/api/v1/pty', { body: { title: 'Terminal 1' } })),
			'PTY session',
		);
		expect(created.id).toMatch(/^pty-preview-/);

		const fetched = requireData<PtyView>(
			await call(client.GET('/api/v1/pty/{id}', { params: { path: { id: created.id } } })),
			'PTY session',
		);
		expect(fetched.id).toBe(created.id);

		const renamed = requireData<PtyView>(
			await call(
				client.PUT('/api/v1/pty/{id}', {
					params: { path: { id: created.id } },
					body: { title: 'Renamed' },
				}),
			),
			'PTY session',
		);
		expect(renamed.title).toBe('Renamed');

		await call(client.DELETE('/api/v1/pty/{id}', { params: { path: { id: created.id } } }));
		await expect(
			call(client.GET('/api/v1/pty/{id}', { params: { path: { id: created.id } } })),
		).rejects.toThrow();
	});
});
