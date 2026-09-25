import { API_BASE_URL, resolveApiKey } from '$lib/api/client';

export interface PostStreamOptions {
	path: string;
	body: unknown;
	signal: AbortSignal;
	onFrame: (event: string, data: unknown) => void;
	onError: (message: string) => void;
}

export interface GetStreamOptions {
	path: string;
	query?: Record<string, string>;
	signal: AbortSignal;
	onFrame: (event: string, data: unknown) => void;
	onError: (message: string) => void;
}

function dispatchFrame(
	raw: string,
	onFrame: (event: string, data: unknown) => void,
): void {
	let current = '';
	for (const line of raw.split('\n')) {
		if (line.startsWith('event:')) {
			current = line.slice(6).trim();
		} else if (line.startsWith('data:')) {
			const text = line.slice(5).trim();
			if (!text || text === '[DONE]') continue;
			try {
				onFrame(current, JSON.parse(text) as unknown);
			} catch {
				onFrame(current, text);
			}
		}
	}
}

async function pump(
	response: Response,
	signal: AbortSignal,
	onFrame: (event: string, data: unknown) => void,
	onError: (message: string) => void,
): Promise<void> {
	if (!response.body) {
		onError('Stream has no body');
		return;
	}
	const reader = response.body.getReader();
	const decoder = new TextDecoder();
	let buffer = '';
	try {
		while (!signal.aborted) {
			const { done, value } = await reader.read();
			if (done) break;
			buffer += decoder.decode(value, { stream: true });
			let boundary = buffer.indexOf('\n\n');
			while (boundary !== -1) {
				dispatchFrame(buffer.slice(0, boundary), onFrame);
				buffer = buffer.slice(boundary + 2);
				boundary = buffer.indexOf('\n\n');
			}
		}
	} catch (e) {
		if (!signal.aborted) {
			onError(e instanceof Error ? e.message : 'Stream interrupted');
		}
	} finally {
		reader.releaseLock();
	}
}

async function readError(response: Response): Promise<string> {
	try {
		const payload = (await response.json()) as {
			error?: { message?: string };
		};
		if (payload.error?.message) return payload.error.message;
	} catch {
		// Fall through to the status detail below.
	}
	return `Stream failed (HTTP ${response.status})`;
}

/**
 * POST a JSON body and consume the text/event-stream reply frame by frame.
 * EventSource only supports GET, so cancellable runs use fetch plus a
 * manual reader. Resolves when the server closes the stream; transport
 * failures arrive via onError.
 */
export async function openPostStream(
	options: PostStreamOptions,
): Promise<void> {
	const { path, body, signal, onFrame, onError } = options;
	const key = resolveApiKey();
	let response: Response;
	try {
		response = await fetch(`${API_BASE_URL}${path}`, {
			method: 'POST',
			headers: {
				'Content-Type': 'application/json',
				...(key ? { 'x-api-key': key } : {}),
			},
			body: JSON.stringify(body),
			signal,
		});
	} catch (e) {
		if (!signal.aborted) {
			onError(e instanceof Error ? e.message : 'Stream request failed');
		}
		return;
	}
	if (!response.ok) {
		onError(await readError(response));
		return;
	}
	await pump(response, signal, onFrame, onError);
}

/**
 * GET variant for the read-only event streams. Shares the same frame pump
 * so callers see identical event/data handling on both methods.
 */
export async function openGetStream(options: GetStreamOptions): Promise<void> {
	const { path, query, signal, onFrame, onError } = options;
	const params = new URLSearchParams(query ?? {});
	const key = resolveApiKey();
	if (key) params.set('api_key', key);
	const suffix = params.toString();
	const url = `${API_BASE_URL}${path}${suffix ? `?${suffix}` : ''}`;
	let response: Response;
	try {
		response = await fetch(url, {
			headers: { Accept: 'text/event-stream' },
			signal,
		});
	} catch (e) {
		if (!signal.aborted) {
			onError(e instanceof Error ? e.message : 'Stream request failed');
		}
		return;
	}
	if (!response.ok) {
		onError(await readError(response));
		return;
	}
	await pump(response, signal, onFrame, onError);
}
