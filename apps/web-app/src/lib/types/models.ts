/** View models consumed by components. Populated from fixtures in this stage. */

export interface KeyValue {
	key: string;
	value: string;
}

export interface Execution {
	id: string;
	workflowId: string;
	workflowName: string;
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
}

export interface StackFrame {
	node: string;
	depth: number;
	enteredAt: string;
	status: string;
}

export interface ExecutionDetail extends Execution {
	context: KeyValue[];
	callStack: StackFrame[];
	variables: KeyValue[];
	memory: { currentBytes: number; peakBytes: number };
	migration: Array<{ at: string; from: string; to: string; reason: string }>;
	analysis: {
		slowNodes: Array<{ node: string; durationMs: number }>;
		decisionPoints: string[];
		failureNodes: string[];
		criticalPath: string[];
		iterations: number;
	};
}

export interface GraphNode {
	id: string;
	label: string;
	kind: string;
	status?: string;
	x: number;
	y: number;
}

export interface GraphEdge {
	id: string;
	from: string;
	to: string;
	label?: string;
}

export interface WorkflowGraph {
	nodes: GraphNode[];
	edges: GraphEdge[];
}

export interface WorkflowVersion {
	version: number;
	createdAt: string;
	author: string;
	note: string;
	current: boolean;
}

export interface WorkflowDraft {
	id: string;
	name: string;
	updatedAt: string;
	valid: boolean;
	issues: string[];
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
}

export interface WorkflowDetail extends Workflow {
	graph: WorkflowGraph;
	versions: WorkflowVersion[];
	drafts: WorkflowDraft[];
	neighbors: Array<{ id: string; label: string; reachable: boolean }>;
}

export type MessageRole = 'user' | 'assistant' | 'system' | 'tool';

export interface LoopMessage {
	id: string;
	role: MessageRole;
	content: string;
	createdAt: string;
	tokens: number | null;
	toolName?: string;
}

export interface LoopVariable {
	key: string;
	type: string;
	value: string;
	scope: string;
	updatedAt: string;
}

export interface AgentLoop {
	id: string;
	name: string;
	status: string;
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
}

export type FileChangeType = 'added' | 'modified' | 'deleted' | 'renamed';

export interface FileChange {
	id: string;
	path: string;
	changeType: FileChangeType;
	actor: string;
	at: string;
	additions: number;
	deletions: number;
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

export interface Template {
	id: string;
	name: string;
	kind: TemplateKind;
	category: string;
	description: string;
	usage: number;
	featured: boolean;
	tags: string[];
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

export type MetricTone =
	'success' | 'danger' | 'warning' | 'info' | 'running' | 'neutral';

export interface Metric {
	label: string;
	value: string;
	delta?: string;
	tone?: MetricTone;
	hint?: string;
}

export interface TimelineEntry {
	id: string;
	at: string;
	kind: string;
	title: string;
	detail: string;
	status: string;
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
}
