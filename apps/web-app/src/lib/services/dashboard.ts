import type { Execution, Metric } from '$lib/types/models';
import { listCheckpoints } from '$lib/services/checkpoints';
import { listExecutions } from '$lib/services/executions';
import { listWorkflows } from '$lib/services/workflows';

/** Aggregated overview metrics, mirroring the TUI DashboardData. */
export interface DashboardStats {
	workflowCount: number;
	executionCount: number;
	runningCount: number;
	checkpointCount: number;
	recentExecutions: Execution[];
	metrics: Metric[];
}

/**
 * Fetch the overview numbers. Counts reflect the loaded page rather than a
 * server total (the list contract exposes no total), so limits are set high
 * enough for an overview to read as a count.
 */
export async function loadDashboardStats(): Promise<DashboardStats> {
	const [workflows, executions, checkpoints] = await Promise.all([
		listWorkflows({ limit: 200 }),
		listExecutions({ limit: 200 }),
		listCheckpoints({ limit: 200 }).catch(() => null),
	]);

	const runningCount = executions.items.filter(
		(e) => e.status === 'running',
	).length;
	const workflowCount = workflows.items.length;
	const executionCount = executions.items.length;
	const checkpointCount = checkpoints?.items.length ?? 0;
	const recentExecutions = executions.items.slice(0, 8);

	const metrics: Metric[] = [
		{ label: 'Workflows', value: String(workflowCount) },
		{ label: 'Executions', value: String(executionCount) },
		{
			label: 'Running',
			value: String(runningCount),
			tone: runningCount > 0 ? 'running' : 'neutral',
		},
		{ label: 'Checkpoints', value: String(checkpointCount) },
	];

	return {
		workflowCount,
		executionCount,
		runningCount,
		checkpointCount,
		recentExecutions,
		metrics,
	};
}
