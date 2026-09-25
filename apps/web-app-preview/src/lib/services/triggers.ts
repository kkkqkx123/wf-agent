import { client } from '$lib/api/client';
import { call, extractPage } from '$lib/api/envelope';
import type { PageResult } from '$lib/api/envelope';
import type { Hook, TriggerRecord } from '$lib/types/models';

interface TriggerRecordDto {
    id?: string;
    trigger_name?: string;
    workflow_name?: string;
    execution_id?: string;
    status?: string;
    fired_at?: string;
    payload?: string;
}

function toTriggerRecord(d: TriggerRecordDto): TriggerRecord {
    return {
        id: d.id ?? '',
        triggerName: d.trigger_name ?? '',
        workflowName: d.workflow_name ?? '',
        executionId: d.execution_id ?? '',
        status: d.status ?? '',
        firedAt: d.fired_at ?? '',
        payload: d.payload ?? '',
    };
}

export async function listTriggerHistory(params?: {
    limit?: number;
    offset?: number;
}): Promise<PageResult<TriggerRecord>> {
    const data = await call<unknown>(
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        client.GET('/api/v1/triggers/history' as any, {
            // eslint-disable-next-line @typescript-eslint/no-explicit-any
            params: { query: params ?? {} } as any,
	// eslint-disable-next-line @typescript-eslint/no-explicit-any
        } as any),
    );
    const page = extractPage<TriggerRecordDto>(data);
    return { ...page, items: page.items.map(toTriggerRecord) };
}

export async function listTriggerExecutions(params?: {
    limit?: number;
    offset?: number;
}): Promise<PageResult<TriggerRecord>> {
    const data = await call<unknown>(
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        client.GET('/api/v1/trigger-executions' as any, {
            // eslint-disable-next-line @typescript-eslint/no-explicit-any
            params: { query: params ?? {} } as any,
	// eslint-disable-next-line @typescript-eslint/no-explicit-any
        } as any),
    );
    const page = extractPage<TriggerRecordDto>(data);
    return { ...page, items: page.items.map(toTriggerRecord) };
}

export async function listHooks(): Promise<Hook[]> {
    // Hooks are registered one-by-one. There is no bulk list endpoint.
    return [];
}
