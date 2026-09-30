/** Locale-independent thousands separator for compact counters. */
export function formatNumber(value: number | null | undefined): string {
	if (value === null || value === undefined || Number.isNaN(value)) return '—';
	return new Intl.NumberFormat('en-US').format(value);
}
