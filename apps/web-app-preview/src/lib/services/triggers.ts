import { client, request } from '$lib/api/client';
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
		request('GET', '/api/v1/triggers/history', {
			params: { query: params ?? {} },
		}),
	);
	const page = extractPage<TriggerRecordDto>(data);
	return { ...page, items: page.items.map(toTriggerRecord) };
}

export async function listTriggerExecutions(params?: {
	limit?: number;
	offset?: number;
}): Promise<PageResult<TriggerRecord>> {
	const data = await call<unknown>(
		client.GET('/api/v1/trigger-executions', {
			params: { query: params ?? {} },
		}),
	);
	const page = extractPage<TriggerRecordDto>(data);
	return { ...page, items: page.items.map(toTriggerRecord) };
}

export async function listHooks(): Promise<Hook[]> {
	// Hooks are registered one-by-one. There is no bulk list endpoint.
	return [];
}

export interface WebhookFireResult {
	status: string;
	detail: string;
}

export async function fireHook(
	name: string,
	payload: unknown,
): Promise<WebhookFireResult> {
	const data = await call<unknown>(
		client.POST('/api/v1/hooks/{name}', {
			params: { path: { name } },
			body: payload,
		}),
	);
	const d = (data ?? {}) as Record<string, unknown>;
	return {
		status: String(d.status ?? 'fired'),
		detail: String(d.detail ?? d.message ?? JSON.stringify(d).slice(0, 120)),
	};
}
