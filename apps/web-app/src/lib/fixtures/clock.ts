/**
 * Fixture timestamps are derived from a fixed epoch instead of `Date.now()` so
 * prerendered markup and client hydration always agree.
 */
export const FIXTURE_EPOCH = Date.parse('2026-09-22T09:00:00.000Z');

const MINUTE = 60_000;

export function minutesAgo(minutes: number): string {
	return new Date(FIXTURE_EPOCH - minutes * MINUTE).toISOString();
}

export function minutesAfter(minutes: number): string {
	return new Date(FIXTURE_EPOCH + minutes * MINUTE).toISOString();
}

export function durationOf(
	startMinutesAgo: number,
	endMinutesAgo: number,
): number {
	return Math.max(0, (startMinutesAgo - endMinutesAgo) * MINUTE);
}
