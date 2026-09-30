/**
 * Windowed rendering is approximate on purpose: rows are estimated before they
 * are measured, so the numbers here trade scroll precision for a bounded
 * amount of rendered content.
 */

/** Row count above which a list switches to windowed rendering. */
export const DEFAULT_VIRTUALIZE_THRESHOLD = 200;
