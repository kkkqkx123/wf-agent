import { describe, expect, it } from 'vitest';
import { makeRouter } from '../router';
import { registerExecutions } from './executions';

function routed() {
	const router = makeRouter();
	registerExecutions(router);
	return router;
}

describe('variable injection mock routes', () => {
	it('round-trips submit, list and delete in submission order', () => {
		const router = routed();
		interface InjectionRecord {
			injection_id: string;
			variable_name: string;
			value: unknown;
			source: string;
			submitted_at: number;
			policy: string;
		}
		const first = router.dispatch(
			'POST',
			'/api/v1/executions/exec-7f31c2a9/variable-injections',
			{},
			{
				variable_name: 'speed',
				value: 7,
				source: 'cli',
				policy: 'next_boundary',
			},
		) as InjectionRecord;
		const second = router.dispatch(
			'POST',
			'/api/v1/executions/exec-7f31c2a9/variable-injections',
			{},
			{ variable_name: 'flag', value: true },
		) as InjectionRecord;
		expect(first.source).toBe('cli');
		expect(first.policy).toBe('next_boundary');
		expect(second.source).toBe('api');
		expect(second.policy).toBe('next_boundary');

		const listed = router.dispatch(
			'GET',
			'/api/v1/executions/exec-7f31c2a9/variable-injections',
			{},
			undefined,
		) as { injections: Array<{ injection_id: string }> };
		expect(listed.injections.map((entry) => entry.injection_id)).toEqual([
			first.injection_id,
			second.injection_id,
		]);

		const removed = router.dispatch(
			'DELETE',
			`/api/v1/executions/exec-7f31c2a9/variable-injections/${first.injection_id}`,
			{},
			undefined,
		);
		expect(removed).toEqual({ removed: true });
		const again = router.dispatch(
			'GET',
			'/api/v1/executions/exec-7f31c2a9/variable-injections',
			{},
			undefined,
		) as { injections: unknown[] };
		expect(again.injections).toHaveLength(1);
	});

	it('reports unknown executions and injections as unrouted', () => {
		const router = routed();
		expect(
			router.dispatch('GET', '/api/v1/executions/missing/variable-injections', {}, undefined),
		).toBeUndefined();
		expect(
			router.dispatch(
				'POST',
				'/api/v1/executions/missing/variable-injections',
				{},
				{
					variable_name: 'x',
					value: 1,
				},
			),
		).toBeUndefined();
		expect(
			router.dispatch(
				'DELETE',
				'/api/v1/executions/exec-7f31c2a9/variable-injections/nope',
				{},
				undefined,
			),
		).toBeUndefined();
	});
});
