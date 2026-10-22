import { streamErrorAnalysis } from '#lib/services/streaming';
import type { ErrorRecord } from '#lib/utils/stream-parser';

export interface ErrorStreamOptions {
	onRecord: (record: ErrorRecord) => void;
	onError?: (message: string) => void;
}

/**
 * Consume the read-only error-analysis stream for one execution. The
 * channel is a one-shot GET stream (not WS): callers keep REST detail
 * fetches and use this only to append error records progressively while
 * the analysis tab is visible. Abort the returned cleanup to stop.
 */
export function watchErrorAnalysis(
	executionId: string,
	options: ErrorStreamOptions,
): () => void {
	const controller = new AbortController();
	let stopped = false;
	void streamErrorAnalysis(
		executionId,
		{
			onRecord: (record) => {
				if (!stopped) options.onRecord(record);
			},
			onError: (failure) => {
				if (!stopped) options.onError?.(failure.message);
			},
		},
		controller.signal,
	).catch(() => {});
	return () => {
		stopped = true;
		controller.abort();
	};
}
