export type StatusTone =
	'success' | 'danger' | 'running' | 'warning' | 'info' | 'neutral';

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
};

export function statusTone(status: string | null | undefined): StatusTone {
	if (!status) return 'neutral';
	return TONE_BY_STATUS[status.trim().toLowerCase()] ?? 'neutral';
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
	const normalized = status.replace(/[_-]+/g, ' ').trim();
	return normalized.charAt(0).toUpperCase() + normalized.slice(1);
}
