/** Mock routes: workflows, their graphs and edit locks. */

import { workflows, workflowDetail } from '#lib/fixtures/workflows';
import type { Workflow, WorkflowDetail } from '#lib/types/models';
import { FIXTURE_EPOCH } from '#lib/fixtures/clock';
import { camelToSnakeDeep, pageView, type MockRouter } from '../router';

/** The document the server answers a workflow with: the definition plus
 * the display fields the list view reads from it. */
function workflowDocument(workflow: Workflow | WorkflowDetail): Record<string, unknown> {
	const definition =
		'definition' in workflow
			? workflow.definition
			: { id: workflow.id, name: workflow.name, description: workflow.description };
	return {
		...definition,
		category: workflow.category,
		tags: workflow.tags,
		author: workflow.author,
		status: workflow.status,
		node_count: workflow.nodeCount,
		edge_count: workflow.edgeCount,
		runs: workflow.runs,
		success_rate: workflow.successRate,
		published: workflow.published,
	};
}

export function registerWorkflows(router: MockRouter): void {
	// ---------- workflows ----------
	router.on('GET', '/api/v1/workflows', (_p, query) => {
		return pageView(camelToSnakeDeep(workflows), +(query.limit ?? 50), +(query.offset ?? 0));
	});
	router.on('GET', '/api/v1/workflows/{id}', (params) => {
		const found = workflowDetail.id === params.id
			? workflowDetail
			: workflows.find((w) => w.id === params.id) ?? workflows[0];
		return camelToSnakeDeep(workflowDocument(found));
	});
	router.on('GET', '/api/v1/workflows/{id}/graph', (params) => {
		const detail = workflowDetail.id === params.id
			? workflowDetail
			: workflows.find((w) => w.id === params.id)
					? workflowDetail
					: workflowDetail;
		return camelToSnakeDeep(detail.graph);
	});
	router.on('GET', '/api/v1/workflows/{id}/versions', () => {
		return camelToSnakeDeep([
			{ version: 12, created_at: '', author: 'platform', note: 'Nightly build bump', current: true },
			{ version: 11, created_at: '', author: 'platform', note: 'Reorganise test matrix', current: false },
			{ version: 10, created_at: '', author: 'sre', note: 'Tighten sandbox defaults', current: false },
		]);
	});
	router.on('GET', '/api/v1/workflows/{id}/graph/nodes', () => {
		return camelToSnakeDeep(
			workflowDetail.graph.nodes.map((node) => ({
				id: node.id,
				name: node.label,
				nodeType: node.kind,
			})),
		);
	});
	router.on('GET', '/api/v1/workflows/{id}/graph/edges', () => {
		return camelToSnakeDeep(
			workflowDetail.graph.edges.map((edge) => ({
				id: edge.id,
				sourceNodeId: edge.from,
				targetNodeId: edge.to,
				edgeType: 'default',
				condition: edge.label ?? null,
			})),
		);
	});
	router.on('GET', '/api/v1/workflows/{id}/graph/summary', () => {
		const nodes = workflowDetail.graph.nodes;
		const edges = workflowDetail.graph.edges;
		const targets = new Set(edges.map((edge) => edge.to));
		const sources = new Set(edges.map((edge) => edge.from));
		const nodeCountsByType: Record<string, number> = {};
		for (const node of nodes) {
			nodeCountsByType[node.kind] = (nodeCountsByType[node.kind] ?? 0) + 1;
		}
		return camelToSnakeDeep({
			workflowId: workflowDetail.id,
			nodeCount: nodes.length,
			edgeCount: edges.length,
			startNodeId: nodes.find((node) => !targets.has(node.id))?.id ?? null,
			endNodeIds: nodes.filter((node) => !sources.has(node.id)).map((node) => node.id),
			nodeCountsByType,
		});
	});
	router.on('GET', '/api/v1/workflows/{id}/graph/neighbors/{nodeId}', () => {
		// Path params never reach the mock (preview calls carry the raw
		// template, not interpolated values), so serve the hub node
		// neighborhood as representative sample data.
		const edges = workflowDetail.graph.edges;
		const nodeId = 'build-bundle';
		return camelToSnakeDeep({
			nodeId,
			predecessors: edges.filter((e) => e.to === nodeId).map((e) => e.from),
			successors: edges.filter((e) => e.from === nodeId).map((e) => e.to),
		});
	});

	// Preview has no concurrent editors, so the workflow lock is always free.
	router.on('GET', '/api/v1/workflows/{id}/lock', () => null);
	router.on('POST', '/api/v1/workflows/{id}/lock/acquire', (_p, _q, body) => ({
		owner_id:
			typeof body?.owner_id === 'string' && body.owner_id ? body.owner_id : 'preview-owner',
		owner_name:
			typeof body?.owner_name === 'string' && body.owner_name
				? body.owner_name
				: 'Preview Owner',
		expires_at: FIXTURE_EPOCH
	}));
	router.on('POST', '/api/v1/workflows/{id}/lock/heartbeat', (_p, _q, body) => ({
		owner_id:
			typeof body?.owner_id === 'string' && body.owner_id ? body.owner_id : 'preview-owner',
		owner_name: 'Preview Owner',
		expires_at: FIXTURE_EPOCH
	}));
	router.on('POST', '/api/v1/workflows/{id}/lock/release', () => null);
}
