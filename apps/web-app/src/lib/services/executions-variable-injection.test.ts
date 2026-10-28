import { describe, expect, it, vi, type Mock } from 'vitest';

vi.mock('#lib/api/client', () => ({
	client: { GET: vi.fn(), POST: vi.fn(), DELETE: vi.fn() },
}));

import { client } from '#lib/api/client';
import {
	listVariableInjections,
	removeVariableInjection,
	submitVariableInjection,
} from './executions';

const post = client.POST as unknown as Mock;
const get = client.GET as unknown as Mock;
const del = client.DELETE as unknown as Mock;

function okPayload<T>(data: T) {
	return { data: { success: true, data, error: null }, error: undefined };
}

describe('submitVariableInjection', () => {
	it('posts the wire body and maps the view onto the model', async () => {
		post.mockResolvedValueOnce(
			okPayload({
				injection_id: 'inj-1',
				variable_name: 'speed',
				value: 7,
				source: 'cli',
				submitted_at: 1700000000000,
				policy: 'next_boundary',
			}),
		);
		const injection = await submitVariableInjection('exec-1', {
			variableName: 'speed',
			value: 7,
			source: 'cli',
			policy: 'next_boundary',
		});
		expect(post).toHaveBeenCalledWith('/api/v1/executions/{id}/variable-injections', {
			params: { path: { id: 'exec-1' } },
			body: { variable_name: 'speed', value: 7, source: 'cli', policy: 'next_boundary' },
		});
		expect(injection).toEqual({
			injectionId: 'inj-1',
			variableName: 'speed',
			value: 7,
			source: 'cli',
			submittedAt: 1700000000000,
			policy: 'next_boundary',
		});
	});

	it('defaults source and policy to null and rejects empty execution ids', async () => {
		post.mockResolvedValueOnce(
			okPayload({
				injection_id: 'inj-2',
				variable_name: 'flag',
				value: true,
				source: 'api',
				submitted_at: 1,
				policy: 'next_boundary',
			}),
		);
		await submitVariableInjection('exec-1', { variableName: 'flag', value: true });
		expect(post).toHaveBeenCalledWith('/api/v1/executions/{id}/variable-injections', {
			params: { path: { id: 'exec-1' } },
			body: { variable_name: 'flag', value: true, source: null, policy: null },
		});
		await expect(submitVariableInjection('', { variableName: 'x', value: 1 })).rejects.toThrow();
		expect(post).toHaveBeenCalledTimes(1);
	});
});

describe('listVariableInjections', () => {
	it('maps the injections array and treats a missing list as empty', async () => {
		get.mockResolvedValueOnce(
			okPayload({
				injections: [
					{
						injection_id: 'inj-1',
						variable_name: 'speed',
						value: 7,
						source: 'web',
						submitted_at: 2,
						policy: 'on_resume',
					},
				],
			}),
		);
		const items = await listVariableInjections('exec-1');
		expect(get).toHaveBeenCalledWith('/api/v1/executions/{id}/variable-injections', {
			params: { path: { id: 'exec-1' } },
		});
		expect(items).toEqual([
			{
				injectionId: 'inj-1',
				variableName: 'speed',
				value: 7,
				source: 'web',
				submittedAt: 2,
				policy: 'on_resume',
			},
		]);

		get.mockResolvedValueOnce(okPayload({}));
		await expect(listVariableInjections('exec-1')).resolves.toEqual([]);
	});
});

describe('removeVariableInjection', () => {
	it('deletes by injection id and rejects empty ids without a request', async () => {
		del.mockResolvedValueOnce(okPayload({ removed: true }));
		await removeVariableInjection('exec-1', 'inj-1');
		expect(del).toHaveBeenCalledWith('/api/v1/executions/{id}/variable-injections/{injection_id}', {
			params: { path: { id: 'exec-1', injection_id: 'inj-1' } },
		});
		await expect(removeVariableInjection('exec-1', '')).rejects.toThrow();
		expect(del).toHaveBeenCalledTimes(1);
	});
});
