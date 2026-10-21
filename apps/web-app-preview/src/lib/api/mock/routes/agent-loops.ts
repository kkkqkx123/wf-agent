/** Mock routes: agent loops, their analysis and loop-scoped checkpoints. */

import {
	agentLoops,
	loopMessages,
	loopVariables,
	loopDetail,
} from '#lib/fixtures/agentLoops';
import { checkpoints } from '#lib/fixtures/checkpoints';
import { FIXTURE_EPOCH } from '#lib/fixtures/clock';
import { camelToSnakeDeep, pageView, type MockRouter } from '../router';

export function registerAgentLoops(router: MockRouter): void {
	// ---------- agent-loops ----------

	/** Backend SummaryDto shape the shared service maps via toLoop. */
	function loopSummary(loop: (typeof agentLoops)[number]) {
		return {
			id: loop.id,
			status: loop.status,
			current_iteration: loop.iteration,
			tool_call_count: 0,
			start_time: Date.parse(loop.startedAt),
			end_time: Date.parse(loop.updatedAt),
			execution_time: null,
			profile_id: null,
		};
	}
	router.on('GET', '/api/v1/agent-loops/summaries', (_p, query) => {
		return pageView(
			agentLoops.map(loopSummary),
			+(query.limit ?? 50),
			+(query.offset ?? 0),
		);
	});
	router.on('GET', '/api/v1/agent-loops', (_p, query) => {
		return pageView(camelToSnakeDeep(agentLoops), +(query.limit ?? 50), +(query.offset ?? 0));
	});
	router.on('GET', '/api/v1/agent-loops/{id}', (params) => {
		const found = loopDetail.id === params.id
			? loopDetail
			: agentLoops.find((a) => a.id === params.id) ?? agentLoops[0];
		return camelToSnakeDeep(found);
	});
	router.on('GET', '/api/v1/agent-loops/{id}/summary', (params) => {
		const found = loopDetail.id === params.id
			? loopDetail
			: agentLoops.find((a) => a.id === params.id) ?? agentLoops[0];
		return loopSummary(found);
	});
	router.on('GET', '/api/v1/agent-loops/{id}/conversation', () => {
		return camelToSnakeDeep(loopMessages);
	});
	router.on('GET', '/api/v1/agent-loops/{id}/variables', () => {
		return {
			items: loopVariables.map((variable) => [variable.key, variable.value]),
			has_more: false,
			limit: 200,
			offset: 0,
		};
	});
	router.on('GET', '/api/v1/agent-loops/{id}/iteration-history', () => {
		return {
			items: loopDetail.iterations.map((iteration) => ({
				iteration: iteration.index,
				duration: iteration.durationMs ?? 0,
				response_content: iteration.summary,
				tool_calls: [],
			})),
			has_more: false,
			limit: 200,
			offset: 0,
		};
	});
	router.on('GET', '/api/v1/agent-loops/{id}/timeline', () => ({
		items: [],
		limit: 50,
		next_cursor: null,
		has_more: false,
	}));
	router.on('GET', '/api/v1/agent-loops/{id}/checkpoints/chain', () => {
		return {
			items: camelToSnakeDeep(checkpoints),
			has_more: false,
			limit: 200,
			offset: 0,
		};
	});
	router.on('GET', '/api/v1/agent-loops/{id}/graph', () => {
		return camelToSnakeDeep(loopDetail.graph);
	});
	router.on('GET', '/api/v1/agent-loops/{id}/graph/nodes', () => {
		return loopDetail.graph.nodes.map((node, index) => ({
			node_id: node.id,
			description: node.label,
			type: node.kind,
			iteration: index + 1,
		}));
	});
	router.on('GET', '/api/v1/agent-loops/{id}/graph/edges', () => {
		return loopDetail.graph.edges.map((edge) => ({
			edge_id: edge.id,
			from_node_id: edge.from,
			to_node_id: edge.to,
			reason: edge.label ?? null,
		}));
	});
	router.on('GET', '/api/v1/agent-loops/{id}/graph/paths/steps', () => {
		const nodes = loopDetail.graph.nodes;
		return loopDetail.iterations.map((iteration, index) => ({
			step_no: iteration.index,
			node_id: nodes[index % nodes.length]?.id ?? '',
			node_type: nodes[index % nodes.length]?.kind ?? 'task',
			description: iteration.summary,
			iteration: iteration.index,
			timestamp: FIXTURE_EPOCH,
			duration: iteration.durationMs ?? 0,
		}));
	});
	router.on('GET', '/api/v1/agent-loops/{id}/graph/tool-frequency', () => {
		// Plain { tool: count } map, matching the backend contract the
		// shared service narrows structurally. Returned raw so tool-name
		// keys are never case-mangled.
		const counts: Record<string, number> = {};
		for (const entry of loopDetail.analysis.toolFrequency) {
			counts[entry.tool] = entry.count;
		}
		return counts;
	});
	router.on('GET', '/api/v1/agent-executions/{id}/errors/chain', () => {
		return camelToSnakeDeep(loopDetail.analysis.errorChain);
	});
	router.on('GET', '/api/v1/agent-executions/{id}/errors/root-cause', () => {
		return {
			root_cause_id: '',
			error: loopDetail.analysis.rootCause ?? '',
			chain_length: loopDetail.analysis.errorChain.length,
			suggested_action: loopDetail.analysis.recoveryHints[0] ?? null,
		};
	});
	router.on('POST', '/api/v1/agent-loops/{id}/pause', () => ({ ok: true }));
	router.on('POST', '/api/v1/agent-loops/{id}/resume', () => ({ ok: true }));
	router.on('POST', '/api/v1/agent-loops/{id}/cancel', () => ({ ok: true }));
	router.on('POST', '/api/v1/agent-loops/{id}/checkpoints', () => ({ ok: true }));
	router.on('POST', '/api/v1/agent-loops/{id}/checkpoints/{cid}/restore', () => ({ ok: true }));
	router.on('POST', '/api/v1/agent-loops/{id}/run', () => ({
		agent_loop_id: 'loop-preview-run',
		result: 'Preview run accepted; connect a live backend to execute.',
		iterations: 1,
	}));
}
