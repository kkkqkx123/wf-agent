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
import type {
	ExecutionFileScope,
	RunConversationMessage,
	WorkflowInput,
	WorkflowInputValue,
	WorkflowRuntimeOptions,
} from '#lib/types/models';

/** The definition kinds that can be started as a run. */
export type RunnableKind = 'workflow' | 'agent';

/** The parameters a workflow run takes. */
export interface WorkflowRunParams {
	/** The run input; absent runs the definition with no input. */
	input?: WorkflowInput | null;
	/** Per-run filesystem scope; absent inherits the global default. */
	fileScope?: ExecutionFileScope | null;
	/** Pinned version to run; absent runs the stored definition. */
	version?: string | null;
	/** Per-run runtime overrides; absent fields use backend defaults. */
	options?: WorkflowRuntimeOptions | null;
	/** Debug marker for direct subworkflow runs. Metadata only. */
	debug?: boolean | null;
}

/** The parameters an agent run takes. */
export interface AgentRunParams {
	/** The message the agent answers. */
	message: string;
	/** The model the agent answers with. */
	model: string;
	/** Per-run filesystem scope; absent inherits the global default. */
	fileScope?: ExecutionFileScope | null;
	/** Seed conversation history; absent starts with an empty transcript. */
	conversation?: RunConversationMessage[];
	/** Upper bound on loop iterations; absent uses the template default. */
	maxIterations?: number | null;
	/** Upper bound on tokens; absent uses the template default. */
	tokenLimit?: number | null;
}

export type RunParams = WorkflowRunParams | AgentRunParams;

/** Assert a recorded or parsed value carries workflow input shape; null/missing means no input. */
export function assertWorkflowInput(value: unknown): WorkflowInput | null {
	if (value === null || value === undefined) return null;
	if (typeof value !== 'object' || Array.isArray(value)) {
		throw new Error('Workflow input must be a JSON object or empty.');
	}
	return value as WorkflowInput;
}

/** Parse a JSON text field into workflow input shape; blank means no input. */
export function parseWorkflowInputText(text: string): WorkflowInput | null {
	const trimmed = text.trim();
	if (trimmed === '') return null;
	let parsed: unknown;
	try {
		parsed = JSON.parse(trimmed) as unknown;
	} catch {
		throw new Error('Workflow input must be valid JSON.');
	}
	return assertWorkflowInput(parsed);
}

/** Assert an agent message is non-blank (rule A1). */
export function assertAgentMessage(message: string): string {
	const trimmed = message.trim();
	if (trimmed === '') throw new Error('Agent message must not be empty.');
	return trimmed;
}

/** Assert an agent model is selected (the web panel requires an explicit model). */
export function assertAgentModel(model: string): string {
	if (model.trim() === '') throw new Error('Agent model must be selected.');
	return model;
}

/** Assert a value carries file scope shape (object or missing). */
export function assertFileScope(value: unknown): ExecutionFileScope | null {
	if (value === null || value === undefined) return null;
	if (typeof value !== 'object' || Array.isArray(value)) {
		throw new Error('File scope must be a JSON object or empty.');
	}
	return value as ExecutionFileScope;
}

/** Parse a JSON text field into file scope shape; blank means no scope. */
export function parseFileScopeText(text: string): ExecutionFileScope | null {
	const trimmed = text.trim();
	if (trimmed === '') return null;
	let parsed: unknown;
	try {
		parsed = JSON.parse(trimmed) as unknown;
	} catch {
		throw new Error('File scope must be valid JSON or empty.');
	}
	return assertFileScope(parsed);
}

/** Assert a seed value is a conversation array (items carry at least role/content). */
export function assertConversationSeed(value: unknown): RunConversationMessage[] {
	if (value === null || value === undefined) return [];
	if (!Array.isArray(value)) {
		throw new Error('Conversation seed must be a JSON array or empty.');
	}
	for (const item of value) {
		if (typeof item !== 'object' || item === null || Array.isArray(item)) {
			throw new Error('Conversation seed entries must be message objects.');
		}
		const record = item as Record<string, unknown>;
		if (typeof record['content'] !== 'string') {
			throw new Error('Conversation seed entries must carry a string content.');
		}
	}
	return value as RunConversationMessage[];
}

/** Assert a runtime number is a finite positive value when set. */
export function assertPositiveRuntime(value: number | null | undefined, name: string): number | null {
	if (value === null || value === undefined) return null;
	if (!Number.isFinite(value) || value <= 0) {
		throw new Error(`${name} must be a positive number or empty.`);
	}
	return value;
}

/** Assert runtime overrides carry only finite positive numbers when set. */
export function assertRuntimeOptions(
	options: WorkflowRuntimeOptions | null | undefined,
): WorkflowRuntimeOptions | null {
	if (options === null || options === undefined) return null;
	assertPositiveRuntime(options.maxSteps ?? null, 'maxSteps');
	assertPositiveRuntime(options.timeoutMs ?? null, 'timeoutMs');
	assertPositiveRuntime(options.maxExecutionTimeMs ?? null, 'maxExecutionTimeMs');
	assertPositiveRuntime(options.nodeTimeoutMs ?? null, 'nodeTimeoutMs');
	assertPositiveRuntime(options.maxPauseDurationMs ?? null, 'maxPauseDurationMs');
	assertPositiveRuntime(options.maxNavigationMultiplier ?? null, 'maxNavigationMultiplier');
	assertPositiveRuntime(options.loopMaxIterationsCap ?? null, 'loopMaxIterationsCap');
	return options;
}

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
		const input = assertWorkflowInput(run.input ?? null);
		const fileScope = assertFileScope(run.fileScope ?? null);
		const options = assertRuntimeOptions(run.options ?? null);
		const data = await call<{ execution_id?: string }>(
			client.POST('/api/v1/workflows/{id}/execute', {
				params: { path: { id } },
				body: {
					input,
					file_scope: fileScope,
					version: run.version ?? null,
					max_steps: options?.maxSteps ?? null,
					timeout: options?.timeoutMs ?? null,
					max_execution_time: options?.maxExecutionTimeMs ?? null,
					enable_checkpoints: options?.enableCheckpoints ?? null,
					node_timeout: options?.nodeTimeoutMs ?? null,
					max_pause_duration: options?.maxPauseDurationMs ?? null,
					max_navigation_multiplier: options?.maxNavigationMultiplier ?? null,
					loop_max_iterations_cap: options?.loopMaxIterationsCap ?? null,
					debug: run.debug ?? null,
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
	const message = assertAgentMessage(run.message);
	const model = assertAgentModel(run.model);
	const fileScope = assertFileScope(run.fileScope ?? null);
	const conversation = run.conversation ?? [];
	const data = requireData(
		await call<{ agent_loop_id?: string }>(
			client.POST('/api/v1/agent-loops/{id}/run', {
				params: { path: { id } },
				body: {
					model,
					message,
					tool_call_protocol: { format: 'json' },
					conversation,
					file_scope: fileScope,
					max_iterations: run.maxIterations ?? null,
					token_limit: run.tokenLimit ?? null,
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
	if (detail.executionType !== 'workflow') {
		throw new Error(
			`Rerun not supported for ${detail.executionType} runs: only workflow runs replay`,
		);
	}
	if (!definitionId) {
		throw new Error(`Run ${runId} carries no workflow definition id`);
	}
	const rawInput = 'input' in record ? record.input : null;
	const input = assertWorkflowInput(rawInput ?? null);
	const debug = 'debug' in record && typeof record.debug === 'boolean' ? record.debug : null;
	return runDefinition('workflow', definitionId, {
		input,
		version: (record.workflow_version as string | null) ?? null,
		debug,
	});
}

/** Whether an execution type supports rerun. Only workflow replays. */
export function isRerunnableType(kind: string): boolean {
	return kind === 'workflow';
}

/** One workflow input variable as declared on the definition. */
export interface WorkflowVariableDefinition {
	name: string;
	type?: 'number' | 'string' | 'boolean' | 'array' | 'object' | null;
	required?: boolean | null;
	value?: unknown;
	properties?: Record<string, { type: WorkflowVariableDefinition['type']; required?: boolean | null }> | null;
	element_type?: WorkflowVariableDefinition['type'] | null;
}

function workflowValueKind(value: unknown): string {
	if (value === null || value === undefined) return 'null';
	if (Array.isArray(value)) return 'array';
	switch (typeof value) {
		case 'string': return 'string';
		case 'number': return 'number';
		case 'boolean': return 'boolean';
		default: return 'object';
	}
}

function workflowValueMatches(
	expected: NonNullable<WorkflowVariableDefinition['type']>,
	value: unknown,
): boolean {
	if (value === null || value === undefined) return false;
	if (expected === 'array') return Array.isArray(value);
	if (expected === 'object') return typeof value === 'object' && !Array.isArray(value);
	return typeof value === expected;
}

/** Fetch the declared workflow input variables for one definition. */
export async function getWorkflowVariables(id: string): Promise<WorkflowVariableDefinition[]> {
	const data = await call<Record<string, unknown> | null>(
		client.GET('/api/v1/workflows/{id}', { params: { path: { id } } }),
	);
	const doc = (data ?? {}) as Record<string, unknown>;
	const variables = (doc as { variables?: unknown }).variables;
	if (!Array.isArray(variables)) return [];
	return (variables as WorkflowVariableDefinition[]).filter(
		(variable) => variable && typeof variable.name === 'string',
	);
}

/** Fetch the unknown-key policy for one workflow definition. `true` rejects
 * unknown top-level keys; missing or falsy keeps lenient acceptance. */
export async function getWorkflowInputStrict(id: string): Promise<boolean> {
	try {
		const data = await call<Record<string, unknown> | null>(
			client.GET('/api/v1/workflows/{id}', { params: { path: { id } } }),
		);
		const doc = (data ?? {}) as Record<string, unknown>;
		return (doc as { input_strict?: unknown }).input_strict === true;
	} catch {
		return false;
	}
}

/** Backfill absent workflow input keys from declared defaults. Caller
 * non-null values always win; missing and null entries take the default
 * when one exists, matching the server gate. */
export function applyWorkflowInputDefaults(
	input: Record<string, unknown>,
	variables: WorkflowVariableDefinition[],
): Record<string, unknown> {
	const merged: Record<string, unknown> = { ...(input ?? {}) };
	for (const variable of variables) {
		const provided = merged[variable.name];
		if (provided !== undefined && provided !== null) continue;
		if (variable.value === undefined || variable.value === null) continue;
		merged[variable.name] = variable.value as WorkflowInputValue;
	}
	return merged;
}

/** Validate a workflow input object against its declarations: defaults
 * backfill first, then top-level kinds plus one level of nesting
 * (object properties, array element type) and required keys. Returns
 * every problem so the form can fix all keys in one round. */
export function validateWorkflowInput(
	input: Record<string, unknown>,
	variables: WorkflowVariableDefinition[],
	strict = false,
): string[] {
	const merged = applyWorkflowInputDefaults(input ?? {}, variables);
	const issues: string[] = [];
	for (const [key, value] of Object.entries(merged)) {
		if (value === null || value === undefined) continue;
		const declared = variables.find((variable) => variable.name === key);
		if (!declared) {
			if (strict) issues.push(`workflow input has unknown variable '${key}'`);
			continue;
		}
		if (!declared.type) continue;
		if (!workflowValueMatches(declared.type, value)) {
			issues.push(`variable '${key}' expects ${declared.type}, got ${workflowValueKind(value)}`);
			continue;
		}
		if (declared.type === 'object' && declared.properties && typeof value === 'object' && !Array.isArray(value)) {
			const record = value as Record<string, unknown>;
			for (const [propName, rule] of Object.entries(declared.properties)) {
				const propValue = record[propName];
				if (propValue === null || propValue === undefined) {
					if (rule.required) issues.push(`variable '${key}.${propName}' is missing required property`);
					continue;
				}
					if (rule.type && !workflowValueMatches(rule.type, propValue)) {
						issues.push(`variable '${key}.${propName}' expects ${rule.type}, got ${workflowValueKind(propValue)}`);
					}
			}
		}
		if (declared.type === 'array' && declared.element_type && Array.isArray(value)) {
			value.forEach((item, index) => {
				if (item === null || item === undefined) return;
				if (declared.element_type && !workflowValueMatches(declared.element_type, item)) {
					issues.push(`variable '${key}[${index}]' expects ${declared.element_type}, got ${workflowValueKind(item)}`);
				}
			});
		}
	}
	for (const variable of variables) {
		if (!variable.required) continue;
		const provided = merged[variable.name];
		if (provided === undefined || provided === null) {
			issues.push(`workflow input is missing required variable '${variable.name}'`);
		}
	}
	return issues;
}

export type {
	WorkflowInput,
	WorkflowInputValue,
	WorkflowRuntimeOptions,
	ExecutionFileScope,
	RunConversationMessage,
};
