import { client } from '$lib/api/client';
import { call, extractPage } from '$lib/api/envelope';
import type { PageResult } from '$lib/api/envelope';
import type {
    Execution,
    ExecutionDetail,
    ToolCallEntry,
    TimelineEntry,
} from '$lib/types/models';

interface ExecutionDto {
    id?: string;
    workflow_id?: string;
    workflow_name?: string;
    status?: string;
    started_at?: string;
    ended_at?: string | null;
    duration_ms?: number | null;
    progress?: number;
    current_node?: string | null;
    trigger?: string | null;
    tasks_total?: number;
    tasks_done?: number;
    failed_nodes?: number;
    memory_peak_bytes?: number | null;
    starred?: boolean;
    tags?: string[];
}

interface ToolCallDto {
    id?: string;
    name?: string;
    kind?: string;
    status?: string;
    started_at?: string;
    duration_ms?: number;
    input?: string;
    output?: string;
}

interface TimelineDto {
    id?: string;
    at?: string;
    kind?: string;
    title?: string;
    detail?: string;
    status?: string;
}

function toExecution(d: ExecutionDto): Execution {
    return {
        id: d.id ?? '',
        workflowId: d.workflow_id ?? '',
        workflowName: d.workflow_name ?? '',
        status: d.status ?? '',
        startedAt: d.started_at ?? '',
        endedAt: d.ended_at ?? null,
        durationMs: d.duration_ms ?? null,
        progress: d.progress ?? 0,
        currentNode: d.current_node ?? null,
        trigger: d.trigger ?? null,
        tasksTotal: d.tasks_total ?? 0,
        tasksDone: d.tasks_done ?? 0,
        failedNodes: d.failed_nodes ?? 0,
        memoryPeakBytes: d.memory_peak_bytes ?? null,
        starred: d.starred,
        tags: d.tags,
    };
}

function toToolCall(d: ToolCallDto): ToolCallEntry {
    return {
        id: d.id ?? '',
        name: d.name ?? '',
        kind: d.kind ?? '',
        status: d.status ?? '',
        startedAt: d.started_at ?? '',
        durationMs: d.duration_ms ?? 0,
        input: d.input ?? '',
        output: d.output ?? '',
    };
}

function toTimelineEntry(d: TimelineDto): TimelineEntry {
    return {
        id: d.id ?? '',
        at: d.at ?? '',
        kind: d.kind ?? '',
        title: d.title ?? '',
        detail: d.detail ?? '',
        status: d.status ?? '',
    };
}

export async function listExecutions(params?: {
    limit?: number;
    offset?: number;
}): Promise<PageResult<Execution>> {
    const data = await call<unknown>(
        client.GET('/api/v1/executions', {
            params: { query: params ?? {} },
        }),
    );
    const page = extractPage<ExecutionDto>(data);
    return { ...page, items: page.items.map(toExecution) };
}

export async function getExecution(id: string): Promise<ExecutionDetail> {
    const data = await call<ExecutionDto>(
        client.GET('/api/v1/executions/{id}', {
            params: { path: { id } },
        }),
    );
    const ex = toExecution(data);
    return {
        ...ex,
        context: [],
        callStack: [],
        variables: [],
        memory: { currentBytes: 0, peakBytes: ex.memoryPeakBytes ?? 0 },
        migration: [],
        analysis: {
            slowNodes: [],
            decisionPoints: [],
            failureNodes: [],
            criticalPath: [],
            iterations: 0,
        },
    };
}

export async function listToolCalls(executionId: string): Promise<ToolCallEntry[]> {
    const data = await call<unknown>(
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        client.GET('/api/v1/executions/{id}/audit/tool-calls' as any, {
            // eslint-disable-next-line @typescript-eslint/no-explicit-any
            params: { path: { id: executionId } } as any,
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        } as any),
    );
    if (Array.isArray(data)) {
        return (data as ToolCallDto[]).map(toToolCall);
    }
    const page = extractPage<ToolCallDto>(data);
    return page.items.map(toToolCall);
}

export async function listTimeline(executionId: string): Promise<TimelineEntry[]> {
    const data = await call<unknown>(
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        client.GET('/api/v1/executions/{id}/audit/timeline' as any, {
            // eslint-disable-next-line @typescript-eslint/no-explicit-any
            params: { path: { id: executionId } } as any,
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        } as any),
    );
    if (Array.isArray(data)) {
        return (data as TimelineDto[]).map(toTimelineEntry);
    }
    const page = extractPage<TimelineDto>(data);
    return page.items.map(toTimelineEntry);
}
