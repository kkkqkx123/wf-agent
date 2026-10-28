/** View models consumed by components. Populated from the backend API. */

import type { DisplayKind } from '#lib/utils/tool-output';

export interface KeyValue {
	key: string;
	value: string;
}

export interface Execution {
	id: string;
	workflowId: string;
	workflowName: string;
	/** Owning engine. */
	kind: ExecutionKind;
	status: string;
	startedAt: string;
	endedAt: string | null;
	durationMs: number | null;
	progress: number;
	currentNode: string | null;
	trigger: string | null;
	tasksTotal: number;
	tasksDone: number;
	failedNodes: number;
	memoryPeakBytes: number | null;
	starred?: boolean;
	tags?: string[];
	/** Workflow input captured with the execution; used to re-run it. */
	input?: unknown;
	/** Pinned workflow version the run executed. */
	version?: string | null;
	/** Terminal failure message recorded on the execution, if any. */
	error?: string | null;
	/** Parent run for subworkflow executions; null for root runs. */
	parentExecutionId?: string | null;
	/** Source run this execution was re-run from, if any. */
	rerunOf?: string | null;
	/** Debug marker for direct subworkflow runs. Metadata only. */
	debug?: boolean | null;
}

/** Which engine owns an execution. */
export type ExecutionKind = 'workflow' | 'agent' | 'chat';

/** Which executor ran a session. */
export type SessionExecutorType = 'agent' | 'chat' | 'workflow';

/** One execution as referenced from a hierarchy view. */
export interface ExecutionRef {
	executionId: string;
	executionType: ExecutionKind;
}

/** Where one execution sits in the parent/child tree of nested runs. */
export interface ExecutionHierarchy {
	executionId: string;
	executionType: ExecutionKind;
	status: string;
	/** Nesting level below the root; a root execution is 0. */
	depth: number;
	/** Absent for a root execution. */
	parent: ExecutionRef | null;
	/** The execution this one descends from; equals `executionId` at the root. */
	root: ExecutionRef;
	/** Root-to-parent id chain, oldest first, excluding this execution. */
	ancestors: string[];
}

/** One node of a subtree listing, breadth-first from the queried root. */
export interface ExecutionSubtreeNode {
	executionId: string;
	executionType: ExecutionKind;
	/** Null when the parent still references this run but its record is gone. */
	status: string | null;
	/** Nesting level relative to the queried root, which is 0. */
	depth: number;
	parentExecutionId: string | null;
}

export interface ExecutionSubtree {
	rootExecutionId: string;
	/** Set when nodes remain past the fetched pages. */
	truncated: boolean;
	/** How many descendants sit past the fetched pages. */
	omitted: number;
	nodes: ExecutionSubtreeNode[];
}

/** One agent iteration; workflow executions have none. */
export interface IterationRecord {
	iteration: number;
	durationMs: number;
	toolCallCount: number;
	toolCalls: Array<{ name: string; durationMs: number; success: boolean }>;
	responseContent: string | null;
}

/** One status transition; workflow executions record these. */
export interface StatusTransition {
	from: string;
	to: string;
	timestamp: number;
}

/** One context growth step; agent loops record these. */
export interface ContextEvolutionStep {
	timestamp: number;
	iteration: number;
	status: string;
	description: string;
	toolCalls: number | null;
}

/**
 * Everything one execution recorded, grouped by section. A section the owning
 * engine does not record comes back empty rather than absent, so consumers
 * read one shape for both engines.
 */
export interface ExecutionHistory {
	executionId: string;
	executionType: ExecutionKind;
	/**
	 * Cap on `timeline`: one read never carries more lifecycle events than
	 * this, so a run past the cap has its later events left out.
	 */
	timelineLimit: number;
	timeline: TimelineEntry[];
	nodes: NodeExecution[];
	iterations: IterationRecord[];
	variables: KeyValue[];
	contextEvolution: ContextEvolutionStep[];
	statusTransitions: StatusTransition[];
}

/** One node execution from the unified history nodes section. */
export interface NodeExecution {
	nodeId: string;
	nodeType: string;
	nodeName?: string | null;
	startedAt: string;
	endedAt: string | null;
	durationMs: number;
	error: string | null;
	input: unknown;
	result: unknown;
	metadata?: unknown;
}

export interface NodeTraceToolDependency {
	toolName: string;
	callCount: number;
}

/** One node execution record rebuilt by the backend, latest attempt wins. */
export interface NodeTrace {
	executionId: string;
	nodeId: string;
	nodeName: string;
	nodeType: string;
	status: string;
	startedAt: string;
	endedAt: string | null;
	durationMs: number | null;
	input: unknown;
	output: unknown;
	retryCount: number;
	error: string | null;
	toolDependencies: NodeTraceToolDependency[];
	metadata?: unknown;
}

export interface NodeInputVariable {
	name: string;
	value: string;
	type: string;
	source: string | null;
}

/** Parameters and variables visible to a node when it executed. */
export interface NodeInputContext {
	nodeId: string;
	nodeName: string;
	nodeType: string;
	inputParameters: KeyValue[];
	availableVariables: NodeInputVariable[];
	recordedAt: string | null;
}

export interface LlmReasoningStep {
	stepId: string;
	type: string;
	content: string;
	confidence: number | null;
	conclusions: string[];
}

export interface GraphNode {
	id: string;
	label: string;
	kind: string;
	status?: string;
	iteration?: number;
}

export interface GraphEdge {
	id: string;
	from: string;
	to: string;
	label?: string;
	kind?: string;
	taken?: boolean;
}

export interface WorkflowGraph {
	nodes: GraphNode[];
	edges: GraphEdge[];
}

export interface GraphSummary {
	workflowId: string;
	nodeCount: number;
	edgeCount: number;
	startNodeId: string | null;
	endNodeIds: string[];
	nodeCountsByType: Record<string, number>;
}

export interface CycleResult {
	hasCycle: boolean;
	cycleNodes: string[];
	cycleEdges: string[];
}

export interface TopologyResult {
	success: boolean;
	sortedNodes: string[];
	cycleNodes: string[];
}

export interface ReachabilityResult {
	reachableFromStart: string[];
	reachableToEnd: string[];
	unreachableNodes: string[];
	deadEndNodes: string[];
}

export interface GraphAnalysisResult {
	cycleDetection: CycleResult;
	topologicalSort: TopologyResult;
	reachability: ReachabilityResult;
	nodeTotal: number;
	edgeTotal: number;
	nodeCountsByType: Record<string, number>;
}

export interface ExecutionPathStat {
	nodeCount: number;
	edgeCount: number;
	nodes: string[];
}

export interface ValidationIssue {
	field: string;
	message: string;
}

export interface DependentImpact {
	workflowId: string;
	workflowName: string;
	nodeId: string;
	field: string;
	level: string;
	errors: string[];
	warnings: string[];
}

export interface PromoteReport {
	resourceKind: string;
	resourceId: string;
	dependents: DependentImpact[];
	errorCount: number;
	warningCount: number;
	passCount: number;
}

export interface DecisionPathStep {
	stepNo: number;
	nodeId: string;
	nodeType: string;
	description: string;
	iteration: number;
	timestamp: number;
	duration: number | null;
}

/**
 * The definition kinds that can be edited and therefore carry drafts,
 * versions and editing locks.
 */
export type DefinitionKind = 'workflow' | 'agent' | 'node' | 'trigger';

/** One validation issue on a draft: the field it was raised on and why. */
export interface DefinitionIssue {
	field: string;
	message: string;
}

/** A draft with its validation attached. */
export interface DefinitionDraft {
	kind: DefinitionKind;
	id: string;
	name: string;
	updatedAt: string;
	/** Whether the draft is promotable as it stands. */
	valid: boolean;
	/** The issues that do not block the promotion. */
	issues: DefinitionIssue[];
	/** Full upstream definition retained for inspection; drafts may be incomplete. */
	definition: Record<string, unknown>;
}

/** One saved version of a formal definition. */
export interface DefinitionVersion {
	kind: DefinitionKind;
	version: string;
	createdAt: string;
	author: string;
	note: string;
	/** The newest version is the one in place now. */
	current: boolean;
	/** The definition as it was saved. */
	definition: Record<string, unknown>;
}

export interface Workflow {
	id: string;
	name: string;
	description: string;
	category: string;
	tags: string[];
	author: string;
	version: number;
	status: string;
	nodeCount: number;
	edgeCount: number;
	updatedAt: string;
	runs: number;
	successRate: number | null;
	/** Whether the workflow is published to the template library. */
	published: boolean;
	/**
	 * Persisted derivation source. A template is the reusable view of a
	 * definition; a workflow is the runnable instance derived from it.
	 * Publish/unpublish only flips `published`; derivation always writes
	 * this field. Absent means sourceless history, never backfilled.
	 */
	derivedFrom?: DefinitionDerivation | null;
}

/** First-class derivation lineage shared by every definition body. */
export interface DefinitionDerivation {
	/** Source kind: workflow, agent, node, trigger. */
	sourceKind: string;
	/** Source definition id. */
	sourceId: string;
	/** Derivation time as Unix milliseconds. */
	derivedAt: number;
}

export interface WorkflowDetail extends Workflow {
	graph: WorkflowGraph;
	versions: DefinitionVersion[];
	drafts: DefinitionDraft[];
	/** The workflow definition document, in the shape the server stores it. */
	definition: WorkflowDefinitionDoc;
}

/** A workflow definition document, in the shape the server stores it. */
export type WorkflowDefinitionDoc = Record<string, unknown>;

export type MessageRole = 'user' | 'assistant' | 'system' | 'tool';

/** One JSON value allowed inside a workflow input object. */
export type WorkflowInputValue =
	| string
	| number
	| boolean
	| null
	| WorkflowInputValue[]
	| { [key: string]: WorkflowInputValue };

/**
 * Top-level workflow run input: a parameter object, never a free
 * string/array/scalar. This is a shape gate, not a per-workflow input
 * contract; required fields arrive with the second-stage input schema.
 */
export type WorkflowInput = Readonly<Record<string, WorkflowInputValue>>;

/** Per-run workflow runtime overrides; absent fields use backend defaults. */
export interface WorkflowRuntimeOptions {
	maxSteps?: number | null;
	timeoutMs?: number | null;
	maxExecutionTimeMs?: number | null;
	enableCheckpoints?: boolean | null;
	nodeTimeoutMs?: number | null;
	maxPauseDurationMs?: number | null;
	maxNavigationMultiplier?: number | null;
	loopMaxIterationsCap?: number | null;
}

/** Per-run filesystem scope; mirrors backend `ExecutionFileScope`. */
export interface ExecutionFileScope {
	enabled?: boolean | null;
	read_prefixes?: string[] | null;
	write_prefixes?: string[] | null;
	checkpoint_prefixes?: string[] | null;
	workdir?: string | null;
}

/**
 * Conversation message accepted by the run endpoints, compatible with the
 * backend `Message` wire shape (`id/role/content/timestamp` required).
 */
export interface RunConversationMessage {
	id: string;
	role: string;
	content: string;
	timestamp: number;
	tool_call_id?: string | null;
	tool_name?: string | null;
	thinking?: string | null;
}

export interface LoopMessage {
	id: string;
	role: MessageRole;
	content: string;
	createdAt: string;
	tokens?: number | null;
	toolName?: string | null;
	thinking?: string | null;
}

export interface LoopVariable {
	key: string;
	type?: string;
	value: string;
	scope?: string;
	updatedAt?: string;
}

export interface LoopIteration {
	index: number;
	startedAt: string;
	durationMs: number | null;
	summary: string;
	toolCalls: ToolCallEntry[];
}

export interface ToolRun {
	success: boolean;
	output: string;
	error: string;
	durationMs: number;
	retries: number;
}

export interface AgentLoop {
	id: string;
	name: string;
	status: string;
	/** Which executor ran the session. */
	executionType: SessionExecutorType;
	iteration: number;
	maxIterations: number;
	model: string;
	tokens: number;
	startedAt: string;
	updatedAt: string;
	checkpoints: number;
	errors: number;
	starred: boolean;
	tags: string[];
	endedAt?: string | null;
	profileId?: string | null;
	toolCalls?: number;
	durationMs?: number | null;
}

export interface AgentLoopDetail extends AgentLoop {
	summary: string;
	variables: LoopVariable[];
	messages: LoopMessage[];
	iterations: Array<{
		index: number;
		status: string;
		durationMs: number;
		summary: string;
	}>;
	graph: WorkflowGraph;
	analysis: {
		rootCause: string | null;
		errorChain: string[];
		recoveryHints: string[];
		toolFrequency: Array<{ tool: string; count: number }>;
	};
}

export interface Checkpoint {
	id: string;
	executionId: string;
	sequence: number;
	kind: string;
	actor: string;
	createdAt: string;
	sizeBytes: number;
	note: string;
	restorable: boolean;
	chainPosition?: number | null;
	status?: string;
	tags?: string[];
	entityType?: string;
}

export type FileChangeType = 'added' | 'modified' | 'deleted' | 'renamed';

export interface FileChange {
	id: string;
	path: string;
	/** Absent when the wire row carries no change kind (the delta feed). */
	changeType: FileChangeType | null;
	actor: string;
	at: string;
	/** Absent when the wire row carries no line counts. */
	additions: number | null;
	deletions: number | null;
	session: string;
}

export interface Approval {
	id: string;
	title: string;
	kind: string;
	requester: string;
	requestedAt: string;
	status: string;
	detail: string;
	executionId: string;
}

export interface TriggerRecord {
	id: string;
	triggerName: string;
	workflowName: string;
	executionId: string;
	status: string;
	firedAt: string;
	payload: string;
}

export interface Hook {
	name: string;
	description: string;
	deliveries: number;
	lastStatus: string;
	lastDeliveredAt: string | null;
}

export interface ModelProfile {
	id: string;
	name: string;
	provider: string;
	model: string;
	isDefault: boolean;
	status: string;
	requests: number;
	tokens: number;
	cost: number | null;
}

export interface Provider {
	id: string;
	name: string;
	baseUrl: string;
	models: number;
	status: string;
}

export interface Tool {
	id: string;
	name: string;
	kind: string;
	description: string;
	enabled: boolean;
	calls: number;
	successRate: number | null;
}

export interface Script {
	id: string;
	name: string;
	runtime: string;
	enabled: boolean;
	runs: number;
	updatedAt: string;
}

export interface Skill {
	id: string;
	name: string;
	description: string;
	enabled: boolean;
	version: string;
	promptPreview: string;
}

export type TemplateKind = 'node' | 'trigger' | 'agent' | 'workflow';

/** Tab selection inside the template edit panel. */
export type TemplateEditTab = 'json' | 'form' | 'graph';

/**
 * Governance flags mirror the backend's tri-state: `undefined` means the
 * wire row did not set the flag (treated as unset, not as true).
 */
export type GovernanceFlag = boolean | undefined;

export interface Template {
	id: string;
	name: string;
	kind: TemplateKind;
	category: string;
	description: string;
	usage: number;
	featured: boolean;
	tags: string[];
	enabled?: GovernanceFlag;
	isPublic?: GovernanceFlag;
	isSystem?: GovernanceFlag;
	/** Whether the template is published to the library. */
	published?: GovernanceFlag;
	author?: string | null;
}

export interface QueryResult {
	columns: string[];
	rows: Array<Record<string, string | number | null>>;
	elapsedMs: number;
	truncated: boolean;
}

export interface AuditReport {
	id: string;
	executionId: string;
	generatedAt: string;
	status: string;
	nodes: number;
	durationMs: number;
	toolCalls: number;
	llmCalls: number;
}

export interface ErrorAnalysis {
	id: string;
	category: string;
	rootCause: string;
	occurrences: number;
	firstSeen: string;
	lastSeen: string;
	status: string;
	similar: string[];
}

export interface PerfNode {
	node: string;
	calls: number;
	avgMs: number;
	p95Ms: number;
	share: number;
}

export interface EventRecord {
	id: string;
	type: string;
	source: string;
	at: string;
	executionId: string | null;
	payload: string;
}

export interface Dependency {
	id: string;
	caller: string;
	callee: string;
	kind: string;
	calls: number;
	lastCalledAt: string;
}

export interface Diagnostic {
	name: string;
	status: string;
	value: string;
	detail: string;
}

export type MetricTone = 'success' | 'danger' | 'warning' | 'info' | 'running' | 'neutral';

export interface Metric {
	label: string;
	value: string;
	delta?: string;
	tone?: MetricTone;
	hint?: string;
}

export interface MessageAttachment {
	name: string;
	content: string;
}

export interface OutgoingMessage {
	text: string;
	attachments: MessageAttachment[];
}

export interface TimelineEntry {
	id: string;
	at: string;
	kind: string;
	title: string;
	detail: string;
	status: string;
	/** Owning graph node id, extracted from event metadata when present. */
	nodeId?: string;
}

export interface ToolCallEntry {
	id: string;
	name: string;
	kind: string;
	status: string;
	startedAt: string;
	durationMs: number;
	input: string;
	output: string;
	endpoint?: string;
	exitCode?: number | null;
	approvalId?: string;
	/** Short failure reason; the full text stays in `output`. */
	error?: string;
	/** Presentation kind for the output body. Absent on persisted records;
	 * views fall back to name-based inference. */
	displayKind?: DisplayKind;
	/** Server-provided preview excerpt for the output body. */
	displayPreview?: string;
	/** Whether the server truncated the preview by design. */
	displayTruncated?: boolean;
	/** Owning loop's definition id: launching workflow node id for loops
	 * started by a workflow node, else the agent definition id. Joins to
	 * graph nodes by exact id equality only. */
	nodeId?: string;
	/** Owning iteration for agent-loop tool calls. */
	iteration?: number;
	/** Background session produced by this tool call, from the live stream
	 * marker. Null for foreground tools, history records and older
	 * servers. */
	backgroundSessionId?: string | null;
}
