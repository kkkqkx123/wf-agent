import { client } from '#lib/api/client';
import { call } from '#lib/api/envelope';

export const DEFAULT_RETENTION_SETTINGS: RetentionSettings = {
	enabled: false,
	scan_interval_secs: 3600,
	batch_size: 200,
	dry_run: true,
	checkpoints: { max_age_days: 30, max_checkpoints: 50, protect_favorites: true },
	file_checkpoints: { history_max_age_days: 30, keep_recent_heads: 0 },
	executions: { history_retention_days: 180, keep_recent: 20, protect_running: true },
	tasks: { retention_days: 30, trigger_retention_days: 30 },
};

export interface RetentionSettings {
	enabled: boolean;
	scan_interval_secs: number;
	batch_size: number;
	dry_run: boolean;
	checkpoints: {
		max_age_days: number;
		max_checkpoints: number;
		protect_favorites: boolean;
	};
	file_checkpoints: {
		history_max_age_days: number;
		keep_recent_heads: number;
	};
	executions: {
		history_retention_days: number;
		keep_recent: number;
		protect_running: boolean;
	};
	tasks: {
		retention_days: number;
		trigger_retention_days: number;
	};
}

export type ExecutionLifecycleState = 'active' | 'preserved' | 'archived';

export interface ExecutionLifecycle {
	execution_id?: string;
	state: ExecutionLifecycleState;
	archived_at_ms?: number;
	preserved_at_ms?: number;
	bundle_digest?: string;
	updated_at_ms?: number;
}

export interface RetentionReport {
	scanned: number;
	dry_run: boolean;
	checkpoints_removed: number;
	file_histories_discarded: number;
	executions_archived: number;
	executions_deleted: number;
	tasks_removed: number;
	trigger_executions_removed: number;
	failures: string[];
}

export interface RestorePlan {
	execution_id: string;
	action: string;
	[ key: string ]: unknown;
}

export async function loadRetentionSettings(): Promise<RetentionSettings> {
	return call<RetentionSettings>(client.GET('/api/v1/system/retention'));
}

export async function saveRetentionSettings(settings: RetentionSettings): Promise<RetentionSettings> {
	return call<RetentionSettings>(
		client.PUT('/api/v1/system/retention', { body: settings as unknown }),
	);
}

export async function scanRetention(dryRun: boolean): Promise<RetentionReport> {
	return call<RetentionReport>(
		client.POST('/api/v1/system/retention/scan', { body: { dry_run: dryRun } }),
	);
}

export async function getExecutionLifecycle(id: string): Promise<ExecutionLifecycle | null> {
	return call<ExecutionLifecycle | null>(
		client.GET('/api/v1/executions/{id}/lifecycle', { params: { path: { id } } }),
	);
}

export async function favoriteExecution(id: string): Promise<ExecutionLifecycle> {
	return call<ExecutionLifecycle>(
		client.PUT('/api/v1/executions/{id}/favorite', { params: { path: { id } } }),
	);
}

export async function unfavoriteExecution(id: string): Promise<boolean> {
	return call<boolean>(
		client.DELETE('/api/v1/executions/{id}/favorite', { params: { path: { id } } }),
	);
}

export async function refreshFavorite(id: string): Promise<ExecutionLifecycle> {
	return call<ExecutionLifecycle>(
		client.POST('/api/v1/executions/{id}/favorite/refresh', { params: { path: { id } } }),
	);
}

export async function archiveExecution(id: string): Promise<ExecutionLifecycle> {
	return call<ExecutionLifecycle>(
		client.POST('/api/v1/executions/{id}/archive', { params: { path: { id } } }),
	);
}

export async function restorePreservedExecution(
	id: string,
	options: { replace?: boolean; dryRun?: boolean } = {},
): Promise<RestorePlan> {
	return call<RestorePlan>(
		client.POST('/api/v1/executions/{id}/preserved-bundle/restore', {
			params: { path: { id } },
			body: {
				replace: options.replace ?? false,
				dry_run: options.dryRun ?? false,
			},
		}),
	);
}
