import type { IconName } from './icons/paths';

export type StatusTone =
	'success' | 'danger' | 'running' | 'warning' | 'info' | 'neutral';

export interface StatusMeta {
	tone: StatusTone;
	icon: IconName;
	label: string;
}

/**
 * The transport contract types status fields as plain strings, so unknown
 * values are expected. Every lookup falls back to the neutral tone instead of
 * leaving a badge unstyled.
 */
const TONE_BY_STATUS: Record<string, StatusTone> = {
	completed: 'success',
	complete: 'success',
	success: 'success',
	succeeded: 'success',
	done: 'success',
	ok: 'success',
	healthy: 'success',
	active: 'success',
	enabled: 'success',
	approved: 'success',

	failed: 'danger',
	failure: 'danger',
	error: 'danger',
	errored: 'danger',
	timeout: 'danger',
	timed_out: 'danger',
	aborted: 'danger',
	crashed: 'danger',
	rejected: 'danger',
	unhealthy: 'danger',

	running: 'running',
	in_progress: 'running',
	executing: 'running',
	streaming: 'running',
	started: 'running',
	resumed: 'running',

	paused: 'warning',
	pending: 'warning',
	queued: 'warning',
	waiting: 'warning',
	suspended: 'warning',
	retrying: 'warning',
	degraded: 'warning',

	cancelled: 'neutral',
	canceled: 'neutral',
	skipped: 'neutral',
	draft: 'neutral',
	archived: 'neutral',
	disabled: 'neutral',
	idle: 'neutral',
	unknown: 'neutral',
	stopped: 'neutral',

	new: 'info',
	created: 'info',
	scheduled: 'info',
	cached: 'info',
};

export function statusTone(status: string | null | undefined): StatusTone {
	if (!status) return 'neutral';
	return TONE_BY_STATUS[status.trim().toLowerCase()] ?? 'neutral';
}

/**
 * Explicit status dictionary. Color never stands alone: list rows pair the
 * icon below with the label so boundary states stay scannable.
 */
const STATUS_META: Record<string, StatusMeta> = {
	completed: { tone: 'success', icon: 'check-circle', label: 'Completed' },
	complete: { tone: 'success', icon: 'check-circle', label: 'Completed' },
	success: { tone: 'success', icon: 'check-circle', label: 'Succeeded' },
	succeeded: { tone: 'success', icon: 'check-circle', label: 'Succeeded' },
	done: { tone: 'success', icon: 'check-circle', label: 'Done' },
	ok: { tone: 'success', icon: 'check-circle', label: 'OK' },
	healthy: { tone: 'success', icon: 'check-circle', label: 'Healthy' },
	active: { tone: 'success', icon: 'check-circle', label: 'Active' },
	enabled: { tone: 'success', icon: 'check-circle', label: 'Enabled' },
	approved: { tone: 'success', icon: 'check-circle', label: 'Approved' },
	failed: { tone: 'danger', icon: 'alert-triangle', label: 'Failed' },
	failure: { tone: 'danger', icon: 'alert-triangle', label: 'Failure' },
	error: { tone: 'danger', icon: 'alert-triangle', label: 'Error' },
	errored: { tone: 'danger', icon: 'alert-triangle', label: 'Errored' },
	crashed: { tone: 'danger', icon: 'alert-circle', label: 'Crashed' },
	timeout: { tone: 'danger', icon: 'clock', label: 'Timed out' },
	timed_out: { tone: 'danger', icon: 'clock', label: 'Timed out' },
	aborted: { tone: 'danger', icon: 'x', label: 'Aborted' },
	rejected: { tone: 'danger', icon: 'x', label: 'Rejected' },
	unhealthy: { tone: 'danger', icon: 'alert-circle', label: 'Unhealthy' },
	running: { tone: 'running', icon: 'loader', label: 'Running' },
	in_progress: { tone: 'running', icon: 'loader', label: 'In progress' },
	executing: { tone: 'running', icon: 'loader', label: 'Executing' },
	streaming: { tone: 'running', icon: 'loader', label: 'Streaming' },
	started: { tone: 'running', icon: 'loader', label: 'Started' },
	resumed: { tone: 'running', icon: 'loader', label: 'Resumed' },
	paused: { tone: 'warning', icon: 'pause', label: 'Paused' },
	pending: { tone: 'warning', icon: 'clock', label: 'Pending' },
	queued: { tone: 'warning', icon: 'clock', label: 'Queued' },
	waiting: { tone: 'warning', icon: 'clock', label: 'Waiting' },
	suspended: { tone: 'warning', icon: 'pause', label: 'Suspended' },
	retrying: { tone: 'warning', icon: 'refresh', label: 'Retrying' },
	degraded: { tone: 'warning', icon: 'activity', label: 'Degraded' },
	cancelled: { tone: 'neutral', icon: 'minus', label: 'Cancelled' },
	canceled: { tone: 'neutral', icon: 'minus', label: 'Canceled' },
	skipped: { tone: 'neutral', icon: 'eye-off', label: 'Skipped' },
	draft: { tone: 'neutral', icon: 'pencil', label: 'Draft' },
	archived: { tone: 'neutral', icon: 'archive', label: 'Archived' },
	disabled: { tone: 'neutral', icon: 'minus', label: 'Disabled' },
	idle: { tone: 'neutral', icon: 'clock', label: 'Idle' },
	unknown: { tone: 'neutral', icon: 'info', label: 'Unknown' },
	new: { tone: 'info', icon: 'plus', label: 'New' },
	created: { tone: 'info', icon: 'plus', label: 'Created' },
	scheduled: { tone: 'info', icon: 'clock', label: 'Scheduled' },
	stopped: { tone: 'neutral', icon: 'square', label: 'Stopped' },
	cached: { tone: 'info', icon: 'zap', label: 'Cached' },
};

/** Dictionary entry for a raw status; unknown values keep their own text. */
export function statusMeta(status: string | null | undefined): StatusMeta {
	if (!status) return { tone: 'neutral', icon: 'info', label: 'Unknown' };
	const key = status.trim().toLowerCase();
	const hit = STATUS_META[key];
	if (hit) return hit;
	const normalized = status.replace(/[_-]+/g, ' ').trim();
	const label =
		normalized.charAt(0).toUpperCase() + normalized.slice(1);
	return { tone: statusTone(status), icon: 'info', label };
}

/** Icon for a raw status value. */
export function statusIcon(status: string | null | undefined): IconName {
	return statusMeta(status).icon;
}

/** Maps a tone onto the semantic color tokens declared in app.css. */
export function toneColorVar(tone: StatusTone): string {
	switch (tone) {
		case 'success':
			return 'var(--color-success)';
		case 'danger':
			return 'var(--color-destructive)';
		case 'running':
			return 'var(--color-running)';
		case 'warning':
			return 'var(--color-warning)';
		case 'info':
			return 'var(--color-info)';
		default:
			return 'var(--color-muted-foreground)';
	}
}

/** Readable label for a raw status value, keeping unknown values verbatim. */
export function statusLabel(status: string | null | undefined): string {
	if (!status) return 'Unknown';
	return statusMeta(status).label;
}

/** Whether the status marks an error row background. */
export function isErrorStatus(status: string | null | undefined): boolean {
	return statusTone(status) === 'danger';
}
