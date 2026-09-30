import { browser } from '$app/environment';
import type { PresenceCursor } from '$lib/graph/canvas-model';

interface PresenceFrame extends PresenceCursor {
	at: number;
}

const CHANNEL_PREFIX = 'wf-presence-';
const PEER_TTL_MS = 5_000;
const SWEEP_MS = 2_000;
const SEND_GAP_MS = 100;

const PALETTE = [
	'#f97316',
	'#8b5cf6',
	'#0ea5e9',
	'#10b981',
	'#eab308',
	'#ec4899',
] as const;

/** Stable cursor color per client so peers stay recognizable. */
export function colorForClient(clientId: string): string {
	let hash = 0;
	for (let index = 0; index < clientId.length; index += 1) {
		hash = (hash * 31 + clientId.charCodeAt(index)) | 0;
	}
	return PALETTE[Math.abs(hash) % PALETTE.length];
}

/** Fresh remote frames with the local client removed and `at` stripped. */
export function livePeers(
	frames: Record<string, PresenceFrame>,
	selfId: string,
	now: number,
): PresenceCursor[] {
	return Object.values(frames)
		.filter(
			(frame) => frame.clientId !== selfId && now - frame.at <= PEER_TTL_MS,
		)
		.map(({ clientId, name, color, x, y }) => ({
			clientId,
			name,
			color,
			x,
			y,
		}));
}

function isFrame(value: unknown): value is PresenceFrame {
	if (!value || typeof value !== 'object') return false;
	const frame = value as Record<string, unknown>;
	return (
		typeof frame.clientId === 'string' &&
		typeof frame.name === 'string' &&
		typeof frame.color === 'string' &&
		typeof frame.x === 'number' &&
		typeof frame.y === 'number' &&
		typeof frame.at === 'number'
	);
}

/**
 * Same-origin presence over BroadcastChannel: every open tab of one workflow
 * broadcasts its cursor and renders the others. No server transport exists
 * yet (the WS protocol defers presence), so this stays tab-local; swapping
 * the channel for a socket later keeps the canvas `presence` prop unchanged.
 */
export class PresenceStore {
	peers = $state<PresenceCursor[]>([]);

	private channel: BroadcastChannel | null = null;
	private frames: Record<string, PresenceFrame> = {};
	private clientId = '';
	private displayName = '';
	private lastSent = 0;
	private timer: ReturnType<typeof setInterval> | null = null;

	join(workflowId: string, name = 'this browser'): void {
		if (!browser) return;
		this.leave();
		this.clientId = `${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`;
		this.displayName = `${name} · ${this.clientId.slice(-4)}`;
		let channel: BroadcastChannel;
		try {
			channel = new BroadcastChannel(`${CHANNEL_PREFIX}${workflowId}`);
		} catch {
			return;
		}
		this.channel = channel;
		this.channel.onmessage = (event: MessageEvent) => {
			if (!isFrame(event.data)) return;
			const frame = event.data;
			if (frame.clientId === this.clientId) return;
			this.frames[frame.clientId] = frame;
			this.peers = livePeers(this.frames, this.clientId, Date.now());
		};
		this.timer = setInterval(() => {
			this.peers = livePeers(this.frames, this.clientId, Date.now());
		}, SWEEP_MS);
	}

	move(x: number, y: number): void {
		if (!this.channel || !this.clientId) return;
		const now = Date.now();
		if (now - this.lastSent < SEND_GAP_MS) return;
		this.lastSent = now;
		try {
			this.channel.postMessage({
				clientId: this.clientId,
				name: this.displayName,
				color: colorForClient(this.clientId),
				x,
				y,
				at: now,
			} satisfies PresenceFrame);
		} catch {
			// Closed or saturated channels simply drop this frame.
		}
	}

	leave(): void {
		if (this.timer !== null) {
			clearInterval(this.timer);
			this.timer = null;
		}
		try {
			this.channel?.close();
		} catch {
			// Closing an already-closed channel is harmless.
		}
		this.channel = null;
		this.frames = {};
		this.clientId = '';
		this.peers = [];
		this.lastSent = 0;
	}
}

export const presence = new PresenceStore();
