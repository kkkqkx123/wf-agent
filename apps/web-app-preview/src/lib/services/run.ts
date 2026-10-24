/** Starting a run of an executable definition.
 *
 * Every executable definition is started the same way: pick the definition,
 * fill in the parameters its kind takes, start it, land on the execution.
 * The kind decides the payload, because the parameters differ: a workflow
 * takes input, a filesystem scope and a pinned version, an agent takes a
 * message and a model.
 */
import { client } from '#lib/api/client';
import { call, requireData } from '#lib/api/envelope';
import { getUnifiedExecutionDetail } from '#lib/services/executions';

/** The definition kinds that can be started as a run. */
export type RunnableKind = 'workflow' | 'agent';

/** The parameters a workflow run takes. */
export interface WorkflowRunParams {
	/** The run input; absent runs the definition with no input. */
	input?: unknown;
	/** Per-run filesystem scope; absent inherits the global default. */
	fileScope?: unknown;
	/** Pinned version to run; absent runs the stored definition. */
	version?: string | null;
}

/** The parameters an agent run takes. */
export interface AgentRunParams {
	/** The message the agent answers. */
	message: string;
	/** The model the agent answers with. */
	model: string;
}

export type RunParams = WorkflowRunParams | AgentRunParams;

/**
 * Start a run of an executable definition and return the execution it
 * produced.
 */
export async function runDefinition<K extends RunnableKind>(
	kind: K,
	id: string,
	params: K extends 'workflow' ? WorkflowRunParams : AgentRunParams,
): Promise<string> {
	if (kind === 'workflow') {
		const run = params as WorkflowRunParams;
		const data = await call<{ execution_id?: string }>(
			client.POST('/api/v1/workflows/{id}/execute', {
				params: { path: { id } },
				body: {
					input: run.input ?? null,
					file_scope: run.fileScope ?? null,
					version: run.version ?? null,
				},
			}),
		);
		const executionId = data.execution_id ?? '';
		if (!executionId) {
			throw new Error(`Workflow run returned no execution id for ${id}`);
		}
		return executionId;
	}
	const run = params as AgentRunParams;
	const data = requireData(
		await call<{ agent_loop_id?: string }>(
			client.POST('/api/v1/agent-loops/{id}/run', {
				params: { path: { id } },
				body: {
					model: run.model,
					message: run.message,
					tool_call_protocol: { format: 'json' },
					conversation: [],
					file_scope: null,
				},
			}),
		),
		`Agent run returned no result for ${id}`,
	);
	const executionId = data.agent_loop_id ?? '';
	if (!executionId) {
		throw new Error(`Agent run returned no execution id for ${id}`);
	}
	return executionId;
}

/**
 * Re-run a workflow execution with the input recorded on it. A run made
 * against an older version replays that version instead of the definition
 * live now.
 */
export async function rerunWorkflowRun(runId: string): Promise<string> {
	const detail = await getUnifiedExecutionDetail(runId);
	const record =
		typeof detail.record === 'object' && detail.record !== null
			? (detail.record as Record<string, unknown>)
			: {};
	const definitionId = detail.definitionId;
	if (!definitionId) {
		throw new Error(`Run ${runId} carries no workflow definition id`);
	}
	return runDefinition('workflow', definitionId, {
		input: record.input ?? null,
		version: (record.workflow_version as string | null) ?? null,
	});
}
