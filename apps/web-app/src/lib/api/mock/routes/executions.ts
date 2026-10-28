/** Mock routes: unified executions, audit views and artifacts. */

import { agentLoops } from '#lib/fixtures/agentLoops';
import {
	executions,
	executionHierarchy,
	executionHistory,
	executionSubtree,
	executionToolCalls,
} from '#lib/fixtures/executions';
import { executionTimeline } from '#lib/fixtures/triggers';
import { camelToSnakeDeep, cursorPageView, type MockRouter } from '../router';

export function registerExecutions(router: MockRouter): void {
	router.on('GET', '/api/v1/executions/{id}/audit/tool-calls', () => {
		return camelToSnakeDeep(executionToolCalls);
	});
	router.on('GET', '/api/v1/executions/{id}/audit/timeline', () => {
		return camelToSnakeDeep(executionTimeline);
	});
	router.on('GET', '/api/v1/unified-executions', (_p, query) => {
		const unified = [
			...executions.map((e) => ({
				execution_id: e.id,
				execution_type: 'workflow',
				status: e.status,
				start_time: Date.parse(e.startedAt),
				end_time: e.endedAt ? Date.parse(e.endedAt) : null,
				definition_id: e.workflowId,
				version: e.version ?? null,
				parent_execution_id: null,
				error: e.error ?? null,
			})),
			...agentLoops.map((loop) => ({
				execution_id: loop.id,
				execution_type: loop.executionType,
				status: loop.status,
				start_time: Date.parse(loop.startedAt),
				end_time: Date.parse(loop.updatedAt),
				definition_id: loop.name,
				version: null,
				parent_execution_id: null,
				error: null,
			})),
		].sort((a, b) => b.start_time - a.start_time);
		const status = typeof query.status === 'string' ? query.status : null;
		const executionType = typeof query.execution_type === 'string' ? query.execution_type : null;
		const definitionId = typeof query.definition_id === 'string' ? query.definition_id : null;
		const filtered = unified.filter(
			(e) =>
				(!status || e.status === status) &&
				(!executionType || e.execution_type === executionType) &&
				(!definitionId || e.definition_id === definitionId),
		);
		return cursorPageView(filtered, +(query.limit ?? 50), query.cursor);
	});
	router.on('GET', '/api/v1/unified-executions/{id}', (params) => {
		const workflow = executions.find((e) => e.id === params.id);
		if (workflow) {
			return {
				execution_id: workflow.id,
				execution_type: 'workflow',
				status: workflow.status,
				start_time: Date.parse(workflow.startedAt),
				end_time: workflow.endedAt ? Date.parse(workflow.endedAt) : null,
				definition_id: workflow.workflowId,
				parent_execution_id: null,
				error: workflow.error ?? null,
				record: workflow,
			};
		}
		const loop = agentLoops.find((l) => l.id === params.id) ?? agentLoops[0];
		return {
			execution_id: loop.id,
			execution_type: loop.executionType,
			status: loop.status,
			start_time: Date.parse(loop.startedAt),
			end_time: Date.parse(loop.updatedAt),
			definition_id: loop.name,
			parent_execution_id: null,
			error: null,
			record: loop,
		};
	});
	router.on('GET', '/api/v1/executions/{id}/logs', (_p, query) => {
		const entries = executionTimeline.map((entry, index) => ({
			execution_id: null,
			workflow_id: null,
			timestamp: Date.parse(entry.at) + index,
			event_type: entry.kind,
			event_name: null,
			message: `${entry.title} — ${entry.detail}`,
		}));
		return cursorPageView(entries, +(query.limit ?? 50), query.cursor);
	});
	router.on('GET', '/api/v1/executions/{id}/artifacts', (_p, query) => {
		const entries = [
			{
				execution_id: 'preview',
				execution_type: 'workflow',
				name: 'output',
				kind: 'workflow_output',
				preview: '{"preview": true}',
				truncated: false,
				size_bytes: 18,
			},
		];
		return cursorPageView(entries, +(query.limit ?? 50), query.cursor);
	});
	router.on('GET', '/api/v1/executions/{id}/hierarchy', () => {
		return camelToSnakeDeep(executionHierarchy);
	});
	router.on('GET', '/api/v1/executions/{id}/subtree', () => {
		return camelToSnakeDeep(executionSubtree);
	});
	router.on('GET', '/api/v1/executions/{id}/history', () => {
		return camelToSnakeDeep(executionHistory);
	});
	// Minimal execution overview; the canvas renders an empty graph.
	router.on('GET', '/api/v1/executions/{id}/graph/overview', () => ({
		graph: { nodes: [], edges: [] },
		failed_nodes: [],
		critical_path: [],
	}));
	// Preview-only buffer for externally submitted variable injections.
	// Unknown executions read as unrouted (404), mirroring the backend;
	// validation parity (names, sources, policies) lives server-side only.
	let injectionSeq = 0;
	const injectionStore = new Map<
		string,
		Array<{
			injection_id: string;
			variable_name: string;
			value: unknown;
			source: string;
			submitted_at: number;
			policy: string;
		}>
	>();
	const knownExecution = (id: string): boolean => executions.some((e) => e.id === id);
	router.on('POST', '/api/v1/executions/{id}/variable-injections', (params, _q, body) => {
		if (!knownExecution(params.id)) return undefined;
		const record = {
			injection_id: `inj-preview-${(injectionSeq += 1)}`,
			variable_name: String(body?.variable_name ?? ''),
			value: body?.value ?? null,
			source: typeof body?.source === 'string' && body.source !== '' ? body.source : 'api',
			submitted_at: Date.now(),
			policy:
				typeof body?.policy === 'string' && body.policy !== '' ? body.policy : 'next_boundary',
		};
		const buffered = injectionStore.get(params.id) ?? [];
		buffered.push(record);
		injectionStore.set(params.id, buffered);
		return record;
	});
	router.on('GET', '/api/v1/executions/{id}/variable-injections', (params) => {
		if (!knownExecution(params.id)) return undefined;
		const buffered = [...(injectionStore.get(params.id) ?? [])].sort(
			(a, b) => a.submitted_at - b.submitted_at || (a.injection_id < b.injection_id ? -1 : 1),
		);
		return { injections: buffered };
	});
	router.on('DELETE', '/api/v1/executions/{id}/variable-injections/{injection_id}', (params) => {
		if (!knownExecution(params.id)) return undefined;
		const buffered = injectionStore.get(params.id) ?? [];
		const index = buffered.findIndex((entry) => entry.injection_id === params.injection_id);
		if (index === -1) return undefined;
		buffered.splice(index, 1);
		return { removed: true };
	});
}
