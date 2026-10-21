/** Mock routes: executions, unified listings, timelines and artifacts. */

import { agentLoops } from '#lib/fixtures/agentLoops';
import {
	executions,
	executionDetail,
	executionHierarchy,
	executionHistory,
	executionSubtree,
	executionToolCalls,
} from '#lib/fixtures/executions';
import { executionTimeline } from '#lib/fixtures/triggers';
import { camelToSnakeDeep, cursorPageView, pageView, type MockRouter } from '../router';

export function registerExecutions(router: MockRouter): void {
	// ---------- executions ----------
	router.on('GET', '/api/v1/executions', (_p, query) => {
		return pageView(camelToSnakeDeep(executions), +(query.limit ?? 50), +(query.offset ?? 0));
	});
	router.on('GET', '/api/v1/executions/{id}', (params) => {
		const found = executionDetail.id === params.id
			? executionDetail
			: executions.find((e) => e.id === params.id) ?? executions[0];
		return camelToSnakeDeep(found);
	});
	router.on('GET', '/api/v1/executions/{id}/audit/tool-calls', () => {
		return camelToSnakeDeep(executionToolCalls);
	});
	router.on('GET', '/api/v1/executions/{id}/audit/timeline', () => {
		return camelToSnakeDeep(executionTimeline);
	});
	router.on('GET', '/api/v1/events/execution-timeline/{executionId}', (_p, query) => {
		const events = executionTimeline.map((entry) => ({
			...entry,
			timestamp: Date.parse(entry.at),
			type: entry.kind,
		}));
		const page = cursorPageView(
			camelToSnakeDeep(events),
			+(query.limit ?? 50),
			query.cursor,
		);
		return { ...page, total: events.length };
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
				parent_execution_id: null,
				error: e.error ?? null,
			})),
			...agentLoops.map((loop) => ({
				execution_id: loop.id,
				execution_type: 'agent_loop',
				status: loop.status,
				start_time: Date.parse(loop.startedAt),
				end_time: Date.parse(loop.updatedAt),
				definition_id: loop.name,
				parent_execution_id: null,
				error: null,
			})),
		].sort((a, b) => b.start_time - a.start_time);
		const status = typeof query.status === 'string' ? query.status : null;
		const filtered = status
			? unified.filter((e) => e.status === status)
			: unified;
		return cursorPageView(filtered, +(query.limit ?? 50), query.cursor);
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
		critical_path: []
	}));
}
