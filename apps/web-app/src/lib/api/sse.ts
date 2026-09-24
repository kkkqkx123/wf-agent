/**
 * Light SSE client.  openapi-typescript cannot describe EventSource frames
 * so this is hand-rolled and lives completely outside the typed client.
 *
 * EventSource cannot carry custom headers (no x-api-key), so we piggyback
 * the key as a query param when present — same convention the backend SSE
 * handlers accept.
 */
export interface SseFrame<T> {
	event?: string;
	data: T;
	id?: string;
	retry?: number;
}

export interface SseOptions<T> {
	apiKey?: string;
	onMessage: (frame: SseFrame<T>) => void;
	onError?: (err: unknown) => void;
	onOpen?: () => void;
	withCredentials?: boolean;
}

export function subscribeSse<T>(
	url: string,
	options: SseOptions<T>,
): () => void {
	const apiKey =
		options.apiKey ?? (import.meta.env.VITE_API_KEY as string | undefined);
	const fullUrl = apiKey
		? `${url}${url.includes('?') ? '&' : '?'}api_key=${encodeURIComponent(apiKey)}`
		: url;

	const es = new EventSource(fullUrl, {
		withCredentials: options.withCredentials,
	});

	if (options.onOpen) es.onopen = options.onOpen;
	if (options.onError) es.onerror = (e) => options.onError!(e);
	es.onmessage = (ev) => {
		let parsed: T;
		try {
			parsed = JSON.parse(ev.data) as T;
		} catch {
			parsed = ev.data as unknown as T;
		}
		onMessageCompat<T>(ev, parsed, options.onMessage);
	};

	return () => es.close();
}

/**
 * `MessageEvent<T>` (DOM lib) has neither `.event` nor `.lastEventId` typed
 * in our TS5.9 DOM shim.  Duck-extract with a local helper.
 */
function onMessageCompat<T>(
	ev: MessageEvent,
	parsed: T,
	onMessage: (frame: SseFrame<T>) => void,
): void {
	const anyEv = ev as unknown as { event?: string; lastEventId?: string };
	onMessage({
		data: parsed,
		event: anyEv.event || undefined,
		id: anyEv.lastEventId || undefined,
	});
}
