import { API_BASE_URL, resolveApiKey } from '#lib/api/client';

/** Local truncation budget mirroring the session store placeholder. */
export const TITLE_TRUNCATE_CHARS = 60;

/**
 * Truncate a prompt into a short session title. First line only, capped
 * at {@link TITLE_TRUNCATE_CHARS} characters. Empty input yields an empty
 * string so callers can fall back to the session id.
 */
export function truncateTitle(content: string): string {
	const line = content.trim().split('\n', 1)[0] ?? '';
	if (!line) return '';
	return line.length <= TITLE_TRUNCATE_CHARS ? line : line.slice(0, TITLE_TRUNCATE_CHARS);
}

interface TitleEnvelope {
	text?: string | null;
}

async function postTitle(
	path: string,
	body: unknown,
	signal?: AbortSignal,
): Promise<string | null> {
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
		const envelope = (await response.json()) as { success?: boolean; data?: TitleEnvelope };
		const text = envelope.data?.text;
		return typeof text === 'string' && text.trim().length > 0 ? text : null;
	} catch {
		// Title generation is best effort: network errors, aborts and
		// malformed bodies all converge to silence, and the caller keeps
		// the truncation placeholder.
		return null;
	}
}

/**
 * Read one LLM-generated title for a chat session. Null means silence:
 * side call failed, no profile, or the request was aborted.
 */
export function postChatTitle(
	sessionId: string,
	profileId: string,
	signal?: AbortSignal,
): Promise<string | null> {
	if (!sessionId.trim() || !profileId.trim()) return Promise.resolve(null);
	return postTitle('/api/v1/chat/title', { session_id: sessionId, profile_id: profileId }, signal);
}

/** Read one LLM-generated title for an agent loop. Null means silence. */
export function postLoopTitle(
	loopId: string,
	profileId: string,
	signal?: AbortSignal,
): Promise<string | null> {
	if (!loopId.trim() || !profileId.trim()) return Promise.resolve(null);
	return postTitle(
		`/api/v1/agent-loops/${encodeURIComponent(loopId)}/title`,
		{ profile_id: profileId },
		signal,
	);
}
