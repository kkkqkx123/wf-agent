import { openEventStream, type StreamState } from '#lib/api/sse';
import { openWebSocket, type WsSubscription } from '#lib/api/ws';
import type { EventRecord } from '#lib/types/models';

export type RealtimeDimension = WsSubscription;

export interface RealtimeOptions {
	dimensions: RealtimeDimension[];
	onEvent: (event: EventRecord) => void;
	onState: (state: StreamState) => void;
}

function isSseOnlyDimension(dim: RealtimeDimension): boolean {
	if ('notifications' in dim) return false;
	return true;
}

function toSseOptions(dim: RealtimeDimension): {
	executionId?: string;
	agentLoopId?: string;
	workflowId?: string;
	checkpointOnly?: boolean;
	filePath?: string;
	snapshotId?: string;
} | null {
	if ('executionId' in dim) return { executionId: dim.executionId };
	if ('agentLoopId' in dim) return { agentLoopId: dim.agentLoopId };
	if ('workflowId' in dim) return { workflowId: dim.workflowId };
	if ('global' in dim) return {};
	if ('checkpoint' in dim) {
		return {
			checkpointOnly: true,
			filePath: dim.filePath,
			snapshotId: dim.snapshotId,
		};
	}
	return null;
}

/**
 * Unified realtime entry: one stable subscription keeps running, single
 * dimensions stay on SSE for simplicity while multi-dimensional or
 * notification/global fan-in upgrades to one WebSocket connection.
 * Pushes carry metadata only; callers REST-fetch details and reconcile on
 * reconnect with `since`/cursor replay plus one REST pass.
 */
export function openRealtime(options: RealtimeOptions): () => void {
	const { dimensions, onEvent, onState } = options;
	if (dimensions.length === 0) {
		onState('closed');
		return () => {};
	}
	const singleSse = dimensions.length === 1 && isSseOnlyDimension(dimensions[0]);
	if (singleSse) {
		const sse = toSseOptions(dimensions[0]);
		if (sse) {
			return openEventStream({ ...sse, onEvent, onState });
		}
	}
	return openWebSocket({ subscriptions: dimensions, onEvent, onState });
}
