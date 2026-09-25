/**
 * Windowed rendering is approximate on purpose: rows are estimated before they
 * are measured, so the numbers here trade scroll precision for a bounded
 * amount of rendered content. Every list that windows reads its limits here.
 */

/** Row count above which a list switches to windowed rendering. */
export const VIRTUALIZE_THRESHOLD = 200;

/** Messages grouped into one windowed transcript row. */
export const TRANSCRIPT_ROW_SIZE = 6;

/** Height assumed for a transcript row that has not been measured yet. */
export const TRANSCRIPT_ROW_ESTIMATE_PX = 320;

/** Transcript rows kept rendered just outside the visible window. */
export const TRANSCRIPT_OVERSCAN_ROWS = 2;

/** Distance from the bottom that still counts as following the transcript tail. */
export const FOLLOW_TAIL_SLACK_PX = 64;
