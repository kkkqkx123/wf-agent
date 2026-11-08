import { client } from '#lib/api/client';
import { call, requireData } from '#lib/api/envelope';

export interface SystemTasks {
	triggerExecutions: number;
	agentLoops: number;
	workflows: number;
}

interface SystemTasksDto {
	trigger_executions?: number;
	agent_loops?: number;
	workflows?: number;
}

/** Unified task counts over trigger / loop / workflow ledgers. */
export async function getSystemTasks(): Promise<SystemTasks> {
	const data = requireData(
		await call<SystemTasksDto>(client.GET('/api/v1/system/tasks')),
		'System tasks missing',
	);
	return {
		triggerExecutions: data.trigger_executions ?? 0,
		agentLoops: data.agent_loops ?? 0,
		workflows: data.workflows ?? 0,
	};
}

export interface SseHealth {
	live: number;
	rejected: number;
	overflowed: number;
	max: number;
}

export interface EventHealth {
	sse: SseHealth;
	ws: { live: number };
}

interface EventHealthDto {
	sse?: Partial<SseHealth>;
	ws?: { live?: number };
}

/** Transport health: SSE admission/overflow plus live WebSocket count. */
export async function getEventHealth(): Promise<EventHealth> {
	const data = requireData(
		await call<EventHealthDto>(client.GET('/system/event-health')),
		'Event health missing',
	);
	return {
		sse: {
			live: data.sse?.live ?? 0,
			rejected: data.sse?.rejected ?? 0,
			overflowed: data.sse?.overflowed ?? 0,
			max: data.sse?.max ?? 0,
		},
		ws: { live: data.ws?.live ?? 0 },
	};
}

export interface ServerStatus {
	stopped: boolean;
	version: string;
	pid: number;
}

/** Liveness with the graceful-shutdown latch. */
export async function getServerStatus(): Promise<ServerStatus> {
	const data = requireData(
		await call<ServerStatus>(client.GET('/api/v1/server/status')),
		'Server status missing',
	);
	return data;
}

export interface ServerConfigEntry {
	key: string;
	value: string;
}

/** Effective server config entries; writes stay unavailable (503). */
export async function getServerConfig(): Promise<ServerConfigEntry[]> {
	const data = requireData(
		await call<{ entries?: ServerConfigEntry[] }>(client.GET('/api/v1/server/config')),
		'Server config missing',
	);
	return Array.isArray(data.entries) ? data.entries : [];
}
