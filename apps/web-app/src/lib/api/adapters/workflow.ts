/**
 * Adapter: workflow endpoint payload → Workflow / WorkflowDetail ViewModel.
 *
 * Backend `/workflows/summaries` returns `PageView { items: unknown[] }` —
 * items are still untyped.  `pick*` helpers absorb snake_case drift.
 *
 * Last schema.d.ts sync: 2026-09-24
 */
import type {
	Workflow,
	WorkflowDetail,
	WorkflowVersion,
	WorkflowDraft,
} from '$lib/types/models';
import { adaptGraph } from './agent';
import { pick, pickStr, pickNum, pickBool, pickArr, type AnyRecord } from './_utils';

export function adaptWorkflow(raw: unknown): Workflow {
	const o = (raw ?? {}) as AnyRecord;
	const tags = pickArr<string>(o, 'tags');
	return {
		id: pickStr(o, 'id'),
		name: pickStr(o, 'name'),
		description: pickStr(o, 'description'),
		category: pickStr(o, 'category', 'general'),
		tags,
		author: pickStr(o, 'author'),
		version: pickNum(o, 'version', 1)!,
		status: pickStr(o, 'status', 'active'),
		nodeCount: pickNum(o, 'node_count', 0)!,
		edgeCount: pickNum(o, 'edge_count', 0)!,
		updatedAt: pickStr(o, 'updated_at'),
		runs: pickNum(o, 'runs', 0)!,
		successRate: pickNum(o, 'success_rate', null),
	};
}

export function adaptWorkflowDetail(
	wf: unknown,
	extras?: {
		graph?: unknown;
		versions?: unknown;
		drafts?: unknown;
		neighbors?: unknown;
	},
): WorkflowDetail {
	const base = adaptWorkflow(wf);
	const o = (wf ?? {}) as AnyRecord;
	const graphRaw = extras?.graph ?? pick<unknown>(o, 'graph', null);
	const versionsArr: unknown[] =
		(extras?.versions as unknown[]) ?? pickArr(o, 'versions');
	const draftsArr: unknown[] =
		(extras?.drafts as unknown[]) ?? pickArr(o, 'drafts');
	const neighborsArr: unknown[] =
		(extras?.neighbors as unknown[]) ?? pickArr(o, 'neighbors');

	const versions: WorkflowVersion[] = versionsArr.map((v) => {
		const x = (v ?? {}) as AnyRecord;
		return {
			version: pickNum(x, 'version', 1)!,
			createdAt: pickStr(x, 'created_at'),
			author: pickStr(x, 'author'),
			note: pickStr(x, 'note'),
			current: pickBool(x, 'current'),
		};
	});

	const drafts: WorkflowDraft[] = draftsArr.map((d) => {
		const x = (d ?? {}) as AnyRecord;
		return {
			id: pickStr(x, 'id'),
			name: pickStr(x, 'name'),
			updatedAt: pickStr(x, 'updated_at'),
			valid: pickBool(x, 'valid', true),
			issues: pickArr<string>(x, 'issues'),
		};
	});

	const neighbors = neighborsArr.map((n) => {
		const x = (n ?? {}) as AnyRecord;
		return {
			id: pickStr(x, 'id'),
			label: pickStr(x, 'label'),
			reachable: pickBool(x, 'reachable'),
		};
	});

	return {
		...base,
		graph: adaptGraph(graphRaw),
		versions,
		drafts,
		neighbors,
	};
}
