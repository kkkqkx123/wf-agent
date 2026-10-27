import { client, resolveApiKey } from '#lib/api/client';
import { API_BASE_URL } from '#lib/api/client';

/** Long-turn threshold mirroring the terminal `RECAP_AFTER` gate. */
export const RECAP_AFTER_MS = 120_000;

/** Signals for the auto-recap gate. All three mirror the terminal view. */
export interface RecapSignals {
	/** Milliseconds the turn took. */
	elapsedMs: number;
	/** The page was hidden at least once during the turn. */
	away: boolean;
	/** The last answer overflowed one screen. */
	overflowsScreen: boolean;
	/** Display switch; false disables the automatic note only. */
	enabled: boolean;
	/** Autonomous goal/loop turns never produce a recap. */
	autonomous: boolean;
}

/**
 * Pure auto-recap gate: short, watched, on-screen turns stay silent.
 * Manual `/recap` bypasses this gate entirely.
 */
export function shouldRecap(signals: RecapSignals): boolean {
	if (!signals.enabled) return false;
	if (signals.autonomous) return false;
	return (
		signals.away || signals.elapsedMs >= RECAP_AFTER_MS || signals.overflowsScreen
	);
}

interface RecapEnvelope {
	text?: string | null;
}

async function postRecap(path: string, body: unknown, signal?: AbortSignal): Promise<string | null> {
	try {
		const key = resolveApiKey();
		const response = await fetch(`${API_BASE_URL}${path}`, {
			method: 'POST',
			headers: {
				'Content-Type': 'application/json',
				...(key ? { 'x-api-key': key } : {}),
			},
			body: JSON.stringify(body),
			signal,
		});
		if (!response.ok) return null;
		const envelope = (await response.json()) as { success?: boolean; data?: RecapEnvelope };
		const text = envelope.data?.text;
		return typeof text === 'string' && text.length > 0 ? text : null;
	} catch {
		// Recap is screen-only: network errors, aborts and malformed bodies
		// all converge to silence.
		return null;
	}
}

/**
 * Read one screen-only recap for a chat session. Null means silence:
 * nothing worth saying, side call failed, or the request was aborted.
 */
export function postChatRecap(
	sessionId: string,
	profileId: string,
	signal?: AbortSignal,
): Promise<string | null> {
	if (!sessionId.trim() || !profileId.trim()) return Promise.resolve(null);
	return postRecap('/api/v1/chat/recap', { session_id: sessionId, profile_id: profileId }, signal);
}

/** Read one screen-only recap for an agent loop. Null means silence. */
export function postLoopRecap(
	loopId: string,
	profileId: string,
	signal?: AbortSignal,
): Promise<string | null> {
	if (!loopId.trim() || !profileId.trim()) return Promise.resolve(null);
	return postRecap(`/api/v1/agent-loops/${encodeURIComponent(loopId)}/recap`, { profile_id: profileId }, signal);
}

// Re-export the typed client for tests that assert the envelope shape.
export { client };
