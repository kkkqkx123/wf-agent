/** Mock routes: agent loops, their analysis and loop-scoped checkpoints. */

import { agentLoops, loopMessages, loopVariables, loopDetail } from '#lib/fixtures/agentLoops';
import { checkpoints } from '#lib/fixtures/checkpoints';
import { FIXTURE_EPOCH } from '#lib/fixtures/clock';
import { camelToSnakeDeep, pageView, type MockRouter } from '../router';
import { toCheckpointWire } from '../wire';

export function registerAgentLoops(router: MockRouter): void {
	// ---------- agent-loops ----------

	/** Backend SummaryDto shape the shared service maps via toLoop. */
	function loopSummary(loop: (typeof agentLoops)[number]) {
		return {
			id: loop.id,
			status: loop.status,
			execution_type: loop.executionType,
			title: loop.name,
			current_iteration: loop.iteration,
			tool_call_count: 0,
			start_time: Date.parse(loop.startedAt),
			end_time: Date.parse(loop.updatedAt),
			execution_time: null,
			profile_id: null,
		};
	}

	/** Wire MessageDto preserving fixture images as image_url content parts. */
	function messageWire(message: (typeof loopMessages)[number]) {
		const parts: Array<Record<string, unknown>> = [];
		if (message.content) parts.push({ type: 'text', text: message.content });
		for (const url of message.images ?? []) {
			parts.push({ type: 'image_url', image_url: { url } });
		}
		return {
			id: message.id,
			role: message.role,
			content: parts.length > 0 ? parts : message.content,
			timestamp: Date.parse(message.createdAt),
			tool_name: message.toolName ?? null,
			thinking: null,
			metadata: null,
		};
	}

	/** Example tool calls distributed over the example iterations. */
	function exampleToolCalls(index: number) {
		if (index === 2) {
			return [
				{
					name: 'fs.read',
					arguments: { path: 'crates/checkpoint/src/restore_coordinator.rs' },
					result: 'restore_coordinator.rs:141 — branch_exists returns Err(BranchConflict)',
					error: null,
					tool_call_id: 'tc-read-1',
					duration_ms: 1200,
					success: true,
				},
			];
		}
		if (index === 3) {
			return [
				{
					name: 'edit.apply',
					arguments: { files: 2 },
					result: 'patched 2 files, 34 insertions, 11 deletions',
					error: null,
					tool_call_id: 'tc-edit-1',
					duration_ms: 2400,
					success: true,
				},
			];
		}
		if (index === 4) {
			return [
				{
					name: 'shell.exec',
					arguments: { command: 'cargo test checkpoint' },
					result: '18 passed, 0 failed, 2 skipped',
					error: null,
					tool_call_id: 'tc-exec-1',
					duration_ms: 91000,
					success: true,
				},
			];
		}
		return [];
	}

	router.on('GET', '/api/v1/agent-loops/summaries', (_p, query) => {
		const term = String(query.q ?? '')
			.trim()
			.toLowerCase();
		const status = String(query.status ?? '').trim();
		const filtered = agentLoops.filter((loop) => {
			if (status && loop.status !== status) return false;
			if (!term) return true;
			return [loop.id, loop.name, loop.status].join(' ').toLowerCase().includes(term);
		});
		return pageView(filtered.map(loopSummary), +(query.limit ?? 50), +(query.offset ?? 0));
	});
	router.on('GET', '/api/v1/agent-loops', (_p, query) => {
		return pageView(camelToSnakeDeep(agentLoops), +(query.limit ?? 50), +(query.offset ?? 0));
	});
	router.on('GET', '/api/v1/agent-loops/{id}', (params) => {
		const found =
			loopDetail.id === params.id
				? loopDetail
				: (agentLoops.find((a) => a.id === params.id) ?? agentLoops[0]);
		return camelToSnakeDeep(found);
	});
	router.on('GET', '/api/v1/agent-loops/{id}/summary', (params) => {
		const found =
			loopDetail.id === params.id
				? loopDetail
				: (agentLoops.find((a) => a.id === params.id) ?? agentLoops[0]);
		return loopSummary(found);
	});
	router.on('GET', '/api/v1/agent-loops/{id}/conversation', (_p, query) => {
		return pageView(loopMessages.map(messageWire), +(query.limit ?? 500), +(query.offset ?? 0));
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
				start_time: FIXTURE_EPOCH - (loopDetail.iterations.length - iteration.index) * 60000,
				end_time:
					FIXTURE_EPOCH -
					(loopDetail.iterations.length - iteration.index) * 60000 +
					(iteration.durationMs ?? 0),
				duration: iteration.durationMs ?? 0,
				response_content: iteration.summary,
				tool_calls: exampleToolCalls(iteration.index),
			})),
			has_more: false,
			limit: 200,
			offset: 0,
		};
	});
	router.on('GET', '/api/v1/agent-loops/{id}/timeline', () => {
		const items: Array<Record<string, unknown>> = [];
		for (const iteration of loopDetail.iterations) {
			const base = FIXTURE_EPOCH - (loopDetail.iterations.length - iteration.index) * 60000;
			items.push({
				id: `tl-${iteration.index}-start`,
				timestamp: base,
				type: 'iteration_start',
				description: `Iteration ${iteration.index} started: ${iteration.summary}`,
				error_severity: null,
			});
			if (iteration.status === 'completed') {
				items.push({
					id: `tl-${iteration.index}-end`,
					timestamp: base + (iteration.durationMs ?? 0),
					type: 'iteration_end',
					description: `Iteration ${iteration.index} completed`,
					error_severity: null,
				});
			}
		}
		items.push({
			id: 'tl-checkpoint',
			timestamp: FIXTURE_EPOCH - 6 * 60000,
			type: 'checkpoint_created',
			description: 'Checkpoint created before extending coverage',
			error_severity: null,
		});
		items.sort((a, b) => (a.timestamp as number) - (b.timestamp as number));
		return {
			items,
			limit: 50,
			next_cursor: null,
			has_more: false,
		};
	});
	router.on('GET', '/api/v1/agent-loops/{id}/checkpoints/chain', (params) => {
		const wired = checkpoints.map(toCheckpointWire);
		const owned = wired.filter((row) => row.entity_id === params.id);
		if (owned.length > 0) {
			return {
				items: owned,
				has_more: false,
				limit: 200,
				offset: 0,
			};
		}
		if (params.id === loopDetail.id) {
			const example = [
				{
					id: 'cp-3c1f-3',
					executionId: loopDetail.id,
					sequence: 3,
					kind: 'state',
					actor: 'agent',
					createdAt: new Date(FIXTURE_EPOCH - 6 * 60000).toISOString(),
					sizeBytes: 18 * 1024 * 1024,
					note: 'Before extending coverage with repeated-restore case',
					restorable: true,
				},
				{
					id: 'cp-3c1f-2',
					executionId: loopDetail.id,
					sequence: 2,
					kind: 'file',
					actor: 'agent',
					createdAt: new Date(FIXTURE_EPOCH - 28 * 60000).toISOString(),
					sizeBytes: 24 * 1024 * 1024,
					note: 'After patching restore coordinator',
					restorable: true,
				},
				{
					id: 'cp-3c1f-1',
					executionId: loopDetail.id,
					sequence: 1,
					kind: 'state',
					actor: 'system',
					createdAt: new Date(FIXTURE_EPOCH - 38 * 60000).toISOString(),
					sizeBytes: 12 * 1024 * 1024,
					note: 'Initial snapshot at loop start',
					restorable: true,
				},
			].map(toCheckpointWire);
			return {
				items: example,
				has_more: false,
				limit: 200,
				offset: 0,
			};
		}
		return {
			items: owned,
			has_more: false,
			limit: 200,
			offset: 0,
		};
	});
	router.on('GET', '/api/v1/agent-loops/{id}/graph', () => {
		return {
			nodes: loopDetail.graph.nodes.map((node, index) => ({
				node_id: node.id,
				description: node.label,
				type: node.kind,
				iteration: index + 1,
			})),
			edges: loopDetail.graph.edges.map((edge) => ({
				edge_id: edge.id,
				from_node_id: edge.from,
				to_node_id: edge.to,
				reason: edge.label ?? null,
				was_taken: true,
			})),
			error_node_ids: [],
		};
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
	router.on('POST', '/api/v1/agent-loops/{id}/append', () => ({ ok: true }));
	router.on('POST', '/api/v1/agent-loops/{id}/append-mode', () => ({ ok: true }));
	router.on('POST', '/api/v1/agent-loops/{id}/convert-pending', () => ({ ok: true }));
	router.on('PATCH', '/api/v1/agent-loops/{id}/title', (params, _q, body) => {
		const title = (body as { title?: unknown } | undefined)?.title;
		return typeof title === 'string' && title ? title : params.id;
	});
	router.on('DELETE', '/api/v1/agent-loops/{id}', () => ({ ok: true }));
	router.on('GET', '/api/v1/interactions/by-execution/{executionId}', () => pageView([], 100, 0));
	router.on('POST', '/api/v1/interactions/{id}/respond', () => ({ ok: true }));

	// ---------- workflow nested agent loops ----------
	// Static preview rows in the backend snake_case shape; the shared
	// service narrows them through the same guard as live payloads.
	router.on('GET', '/api/v1/executions/{id}/agent-loops', () => [
		{
			node_id: 'planner',
			agent_loop_id: 'loop-preview-planner',
			status: 'running',
			live: true,
			steering_pending: 1,
			followup_pending: 2,
			append_mode: 'followup',
		},
		{
			node_id: 'reviewer',
			agent_loop_id: 'loop-preview-reviewer',
			status: 'paused',
			live: false,
			steering_pending: 0,
			followup_pending: 0,
			append_mode: 'steering',
		},
	]);
	router.on('POST', '/api/v1/executions/{id}/agent-loops/{node_id}/append', () => ({ ok: true }));
	router.on('POST', '/api/v1/executions/{id}/agent-loops/{node_id}/append-mode', () => ({
		ok: true,
	}));
	router.on('POST', '/api/v1/executions/{id}/agent-loops/{node_id}/convert-pending', () => ({
		ok: true,
	}));
}
