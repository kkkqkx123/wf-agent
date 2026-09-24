const TIME_UNITS: Array<[limit: number, divisor: number, suffix: string]> = [
	[60_000, 1000, 's'],
	[3_600_000, 60_000, 'm'],
	[86_400_000, 3_600_000, 'h'],
	[2_592_000_000, 86_400_000, 'd'],
];

/** Compact elapsed label such as `12s`, `4m`, `3h`, `2d`. */
export function formatDuration(ms: number | null | undefined): string {
	if (ms === null || ms === undefined || Number.isNaN(ms)) return '—';
	if (ms < 1000) return `${Math.max(0, Math.round(ms))}ms`;
	for (const [limit, divisor, suffix] of TIME_UNITS) {
		if (ms < limit || suffix === 'd') {
			return `${Math.round(ms / divisor)}${suffix}`;
		}
	}
	return `${Math.round(ms / 2_592_000_000)}d`;
}

/** Human readable distance from now, tolerant of numeric or ISO input. */
export function formatRelativeTime(
	input: string | number | null | undefined,
): string {
	if (input === null || input === undefined) return '—';
	const value = typeof input === 'number' ? input : Date.parse(input);
	if (Number.isNaN(value)) return '—';
	const delta = Date.now() - value;
	if (delta < 0) return 'just now';
	return `${formatDuration(delta)} ago`;
}

const DATE_FORMAT = new Intl.DateTimeFormat('en-CA', {
	year: 'numeric',
	month: '2-digit',
	day: '2-digit',
	hour: '2-digit',
	minute: '2-digit',
	hour12: false,
});

/** Stable `YYYY-MM-DD HH:mm` rendering, independent of host locale. */
export function formatDateTime(
	input: string | number | null | undefined,
): string {
	if (input === null || input === undefined) return '—';
	const value = typeof input === 'number' ? input : Date.parse(input);
	if (Number.isNaN(value)) return '—';
	return DATE_FORMAT.format(new Date(value)).replace(',', '');
}

const BYTE_UNITS = ['B', 'KB', 'MB', 'GB', 'TB'];

export function formatBytes(bytes: number | null | undefined): string {
	if (bytes === null || bytes === undefined || Number.isNaN(bytes)) return '—';
	let value = bytes;
	let unit = 0;
	while (value >= 1024 && unit < BYTE_UNITS.length - 1) {
		value /= 1024;
		unit += 1;
	}
	const digits = value < 10 && unit > 0 ? 1 : 0;
	return `${value.toFixed(digits)} ${BYTE_UNITS[unit]}`;
}

export function formatNumber(value: number | null | undefined): string {
	if (value === null || value === undefined || Number.isNaN(value)) return '—';
	return new Intl.NumberFormat('en-US').format(value);
}

export function formatPercent(
	value: number | null | undefined,
	digits = 1,
): string {
	if (value === null || value === undefined || Number.isNaN(value)) return '—';
	return `${(value * 100).toFixed(digits)}%`;
}

export function truncate(value: string, max = 48): string {
	if (value.length <= max) return value;
	return `${value.slice(0, Math.max(0, max - 1))}…`;
}

/** Stable id tail used when a full identifier is too wide for a list row. */
export function shortId(value: string | null | undefined, size = 8): string {
	if (!value) return '—';
	return value.length <= size ? value : value.slice(0, size);
}

export function pluralize(
	count: number,
	singular: string,
	plural = `${singular}s`,
): string {
	return `${formatNumber(count)} ${count === 1 ? singular : plural}`;
}
