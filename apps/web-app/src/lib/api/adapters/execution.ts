/**
 * Adapter: execution endpoint payload → Execution / ExecutionDetail ViewModel.
 *
 * Execution detail consumes the most sub-endpoints (~40) — we intentionally
 * only cover fields the current +page.svelte actually renders.  Extra
 * sub-endpoints can be wired later without touching existing adapters.
 *
 * Last schema.d.ts sync: 2026-09-24
 */
import type {
	Execution,
	ExecutionDetail,
	StackFrame,
	KeyValue,
} from '$lib/types/models';
import { adaptGraph } from './agent';
import { pick, pickStr, pickNum, pickArr, type AnyRecord } from './_utils';

export function adaptExecution(raw: unknown): Execution {
	const o = (raw ?? {}) as AnyRecord;
	const tags = pickArr<string>(o, 'tags');
	const startedAt = pickStr(o, 'started_at');
	const endedAt = pickStr(o, 'ended_at', null as unknown as string) || null;
	let durationMs = pickNum(o, 'duration_ms', null);
	if (durationMs == null && startedAt && endedAt) {
		const a = Date.parse(startedAt);
		const b = Date.parse(endedAt);
		if (Number.isFinite(a) && Number.isFinite(b) && b >= a) {
			durationMs = b - a;
		}
	}
	return {
		id: pickStr(o, 'id'),
		workflowId: pickStr(o, 'workflow_id'),
		workflowName: pickStr(o, 'workflow_name') || pickStr(o, 'workflow'),
		status: pickStr(o, 'status', 'unknown'),
		startedAt,
		endedAt,
		durationMs,
		progress: pickNum(o, 'progress', 0)!,
		currentNode: pickStr(o, 'current_node', null as unknown as string) || null,
		trigger: pickStr(o, 'trigger', null as unknown as string) || null,
		tasksTotal: pickNum(o, 'tasks_total', 0)!,
		tasksDone: pickNum(o, 'tasks_done', 0)!,
		failedNodes: pickNum(o, 'failed_nodes', 0)!,
		memoryPeakBytes: pickNum(o, 'memory_peak_bytes', null),
		starred: pickStr(o, 'starred', undefined as unknown as string) ? true : undefined,
		tags,
	};
}

function adaptKeyValue(raw: unknown): KeyValue {
	const o = (raw ?? {}) as AnyRecord;
	return {
		key: pickStr(o, 'key'),
		value: String(pick<unknown>(o, 'value', '') ?? ''),
	};
}

function adaptStackFrame(raw: unknown): StackFrame {
	const o = (raw ?? {}) as AnyRecord;
	return {
		node: pickStr(o, 'node'),
		depth: pickNum(o, 'depth', 0)!,
		enteredAt: pickStr(o, 'entered_at'),
		status: pickStr(o, 'status', 'unknown'),
	};
}

export function adaptExecutionDetail(
	exec: unknown,
	extras?: {
		nodes?: unknown;
		graph?: unknown;
		context?: unknown;
		callStack?: unknown;
		variables?: unknown;
	},
): ExecutionDetail {
	const base = adaptExecution(exec);
	const o = (exec ?? {}) as AnyRecord;

	const contextArr: unknown[] = (extras?.context as unknown[]) ?? pickArr(o, 'context');
	const callStackArr: unknown[] =
		(extras?.callStack as unknown[]) ?? pickArr(o, 'call_stack');
	const variablesArr: unknown[] =
		(extras?.variables as unknown[]) ?? pickArr(o, 'variables');
	const graphRaw = extras?.graph ?? pick<unknown>(o, 'graph', null);

	const analysis = (o.analysis as AnyRecord) ?? {};

	return {
		...base,
		context: contextArr.map(adaptKeyValue),
		callStack: callStackArr.map(adaptStackFrame),
		variables: variablesArr.map(adaptKeyValue),
		memory: {
			currentBytes: pickNum(o, 'memory_current_bytes', 0)!,
			peakBytes: pickNum(o, 'memory_peak_bytes', 0)!,
		},
		migration: pickArr<AnyRecord>(o, 'migration').map((m) => ({
			at: pickStr(m, 'at'),
			from: pickStr(m, 'from'),
			to: pickStr(m, 'to'),
			reason: pickStr(m, 'reason'),
		})),
		analysis: {
			slowNodes: pickArr<AnyRecord>(analysis, 'slow_nodes').map((n) => ({
				node: pickStr(n, 'node'),
				durationMs: pickNum(n, 'duration_ms', 0)!,
			})),
			decisionPoints: pickArr<string>(analysis, 'decision_points'),
			failureNodes: pickArr<string>(analysis, 'failure_nodes'),
			criticalPath: pickArr<string>(analysis, 'critical_path'),
			iterations: pickNum(analysis, 'iterations', 0)!,
		},
		// WorkflowDetail-shared shape reused for agent-loop / workflow graphs
		...(graphRaw ? { graph: adaptGraph(graphRaw) } : {}),
	} as ExecutionDetail;
}
