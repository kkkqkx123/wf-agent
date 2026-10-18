<script lang="ts">
	import { onDestroy, onMount } from 'svelte';
	import { Terminal } from '@xterm/xterm';
	import { FitAddon } from '@xterm/addon-fit';
	import { SerializeAddon } from '@xterm/addon-serialize';
	import { WebLinksAddon } from '@xterm/addon-web-links';
	import '@xterm/xterm/css/xterm.css';
	import {
		connectPty,
		createTerminalWriter,
		resizePty,
		getPty,
	} from '$lib/services/pty';
	import type { PtySession } from '$lib/services/pty';
	import type { PtySessionStore } from '$lib/stores/pty-session.svelte';
	import { resolvedTheme } from '$lib/stores/theme.svelte';

	interface Props {
		session: { id: string; buffer?: string; scrollY?: number; cursor?: number; status?: 'running' | 'exited' };
		store: PtySessionStore;
		onConnect?: (id: string) => void;
		onSessionExited?: (id: string) => void;
		onConnectFailed?: (id: string) => void;
	}

	let { session, store, onConnect, onSessionExited, onConnectFailed }: Props =
		$props();

	let container: HTMLDivElement;

	const SCROLLBACK = 10_000;
	const FONT_SIZE = 14;
	const RESIZE_DEBOUNCE_MS = 100;
	const MAX_RECONNECT_TRIES = 5;

	let term: Terminal;
	let fitAddon: FitAddon;
	let serializeAddon: SerializeAddon;
	let fitFrame: number | undefined;
	let sizeTimer: ReturnType<typeof setTimeout> | undefined;
	let resizeObserver: ResizeObserver;
	let connected = false;
	let tries = 0;
	let reconnectTimer: ReturnType<typeof setTimeout> | undefined;
	let disposed = false;
	let socket: ReturnType<typeof connectPty> | null = null;
	let writer: ReturnType<typeof createTerminalWriter>;
	let cursor = 0;

	const encoder = new TextEncoder();

	function byteLength(text: string): number {
		return encoder.encode(text).length;
	}

	function scheduleFit(): void {
		if (fitFrame !== undefined) return;
		fitFrame = requestAnimationFrame(() => {
			fitFrame = undefined;
			if (disposed) return;
			try {
				fitAddon.fit();
			} catch {
				// Container not ready; the observer will retry.
			}
		});
	}

	function scheduleResize(rows: number, cols: number): void {
		if (sizeTimer !== undefined) clearTimeout(sizeTimer);
		sizeTimer = setTimeout(() => {
			sizeTimer = undefined;
			if (disposed) return;
			store.patch(session.id, { rows, cols });
			resizePty(session.id, rows, cols).catch(() => {});
		}, RESIZE_DEBOUNCE_MS);
	}

	/** Probe the server before reconnecting: exited sessions stay view-only. */
	async function sessionAlive(): Promise<boolean> {
		try {
			const view: PtySession = await getPty(session.id);
			return view.status === 'running';
		} catch {
			return false;
		}
	}

	function clearReconnect(): void {
		if (reconnectTimer !== undefined) {
			clearTimeout(reconnectTimer);
			reconnectTimer = undefined;
		}
	}

	function scheduleReconnect(startCursor: number): void {
		if (disposed || reconnectTimer !== undefined) return;
		if (tries >= MAX_RECONNECT_TRIES) {
			onConnectFailed?.(session.id);
			return;
		}
		const delay = Math.min(250 * 2 ** tries, 4000);
		tries += 1;
		reconnectTimer = setTimeout(() => {
			reconnectTimer = undefined;
			if (disposed) return;
			void sessionAlive().then((alive) => {
				if (disposed) return;
				if (!alive) {
					store.patch(session.id, { status: 'exited' });
					onSessionExited?.(session.id);
					return;
				}
				open(startCursor);
			});
		}, delay);
	}

	function open(startCursor: number): void {
		if (disposed) return;
		clearReconnect();
		socket?.dispose();

		const handle = connectPty(session.id, startCursor, {
			reconnect: false,
			onMeta: (next) => {
				cursor = next;
				if (!connected) {
					connected = true;
					onConnect?.(session.id);
				}
			},
			onData: (chunk) => {
				if (!writer) return;
				writer.push(chunk);
				cursor += byteLength(chunk);
			},
			onServerClose: () => {
				store.patch(session.id, { status: 'exited' });
				onSessionExited?.(session.id);
			},
		});
		socket = handle;

		// Detect abnormal close ourselves: the service only fires onServerClose
		// for explicit server close frames, so poll the close code once the
		// socket leaves the OPEN state.
		const watch = handle.socket();
		if (!watch) return;
		watch.addEventListener('close', () => {
			if (disposed) return;
			connected = false;
			scheduleReconnect(cursor);
		});
	}

	export function focus(): void {
		term?.focus();
	}

	onMount(() => {
		const restore = session.buffer ?? '';
		const restoreScrollY = session.scrollY ?? 0;
		const startCursor = session.cursor ?? 0;
		cursor = startCursor;

		term = new Terminal({
			cursorBlink: true,
			cursorStyle: 'bar',
			fontSize: FONT_SIZE,
			fontFamily: "'JetBrains Mono', 'SF Mono', Menlo, Consolas, monospace",
			scrollback: SCROLLBACK,
			convertEol: false,
		});
		serializeAddon = new SerializeAddon();
		fitAddon = new FitAddon();
		term.loadAddon(serializeAddon);
		term.loadAddon(fitAddon);
		term.loadAddon(new WebLinksAddon());
		term.open(container);

		writer = createTerminalWriter((data) => {
			term.write(data);
		});

		const applyTheme = (): void => {
			const theme = resolvedTheme();
			term.options.theme = {
				background: theme === 'dark' ? '#16181d' : '#ffffff',
				foreground: theme === 'dark' ? '#d4d4d4' : '#1f2328',
				cursor: theme === 'dark' ? '#d4d4d4' : '#1f2328',
				selectionBackground: theme === 'dark' ? '#d4d4d433' : '#1f232833',
			};
		};
		applyTheme();
		$effect(() => {
			resolvedTheme();
			applyTheme();
		});

		resizeObserver = new ResizeObserver(() => scheduleFit());
		resizeObserver.observe(container);

		const onTermResize = term.onResize((size: { rows: number; cols: number }) => {
			scheduleResize(size.rows, size.cols);
		});
		onDestroy(() => onTermResize.dispose());

		if (restore) {
			term.write(restore);
			if (restoreScrollY > 0) term.scrollToLine(restoreScrollY);
		}

		scheduleFit();
		void document.fonts?.ready.then(() => scheduleFit());

		// Exited sessions stay view-only until the user clones them.
		if (session.status !== 'exited') {
			open(startCursor);
		}
	});

	onDestroy(() => {
		disposed = true;
		clearReconnect();
		if (sizeTimer !== undefined) clearTimeout(sizeTimer);
		if (fitFrame !== undefined) cancelAnimationFrame(fitFrame);
		resizeObserver?.disconnect();
		socket?.dispose();
		socket = null;
		writer?.flush();
		try {
			const buffer = serializeAddon?.serialize();
			if (buffer && buffer.length > 0) {
				store.saveBuffer(
					session.id,
					buffer,
					term.buffer.active.viewportY,
					cursor,
				);
			}
		} catch {
			// Serialization may fail after dispose; skip persistence.
		}
		term?.dispose();
	});
</script>

<div
	bind:this={container}
class="pty-instance h-full w-full min-h-0 min-w-0 overflow-hidden px-3 py-2"
></div>

<style>
	.pty-instance :global(.xterm) {
		height: 100%;
	}
</style>
