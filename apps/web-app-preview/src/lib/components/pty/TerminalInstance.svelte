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
	let pendingSize: { rows: number; cols: number } | undefined;
	let lastSize: { rows: number; cols: number } | undefined;
	let resizeObserver: ResizeObserver;
	let connected = false;
	let tries = 0;
	let reconnectTimer: ReturnType<typeof setTimeout> | undefined;
	let disposed = false;
	let socket: ReturnType<typeof connectPty> | null = null;
	let writer: ReturnType<typeof createTerminalWriter>;
	let cursor = 0;
	let textareaFocusCleanup: (() => void) | undefined;

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
		if (disposed) return;
		if (lastSize?.cols === cols && lastSize?.rows === rows) {
			pendingSize = undefined;
			if (sizeTimer !== undefined) clearTimeout(sizeTimer);
			sizeTimer = undefined;
			return;
		}
		pendingSize = { cols, rows };
		if (!lastSize) {
			lastSize = pendingSize;
			store.syncSize(session.id, rows, cols);
			return;
		}
		if (sizeTimer !== undefined) return;
		sizeTimer = setTimeout(() => {
			sizeTimer = undefined;
			const next = pendingSize;
			if (!next) return;
			pendingSize = undefined;
			if (disposed) return;
			if (lastSize?.cols === next.cols && lastSize?.rows === next.rows) return;
			lastSize = next;
			store.syncSize(session.id, next.rows, next.cols);
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
		const delay = Math.min(250 * 2 ** Math.min(tries, 4), 4000);
		tries += 1;
		reconnectTimer = setTimeout(() => {
			reconnectTimer = undefined;
			if (disposed) return;
			void sessionAlive().then((alive) => {
				if (disposed) return;
				if (!alive) {
					markExited();
					return;
				}
				open(startCursor);
			});
		}, delay);
	}

	// Terminal close codes: the session is gone or stopped; stay view-only.
	const TERMINAL_CLOSE_CODES = new Set([4001, 4004, 4010]);

	let exitHandled = false;

	function markExited(): void {
		if (exitHandled) return;
		exitHandled = true;
		store.patch(session.id, { status: 'exited' });
		onSessionExited?.(session.id);
	}

	function open(startCursor: number): void {
		if (disposed) return;
		clearReconnect();
		socket?.dispose();

		socket = connectPty(session.id, startCursor, {
			onMeta: (next) => {
				cursor = next;
				if (!connected) {
					connected = true;
					tries = 0;
					onConnect?.(session.id);
				}
			},
			onData: (chunk) => {
				if (!writer) return;
				writer.push(chunk);
				cursor += byteLength(chunk);
			},
			onExit: () => {
				markExited();
			},
			onClose: (code) => {
				if (disposed) return;
				connected = false;
				if (TERMINAL_CLOSE_CODES.has(code)) {
					markExited();
					return;
				}
				scheduleReconnect(cursor);
			},
		});
	}

	export function focus(): void {
		term?.focus();
		term?.textarea?.focus();
	}

	function handleCopy(event: ClipboardEvent): void {
		const selection = term?.getSelection();
		if (!selection) return;
		const clipboard = event.clipboardData;
		if (!clipboard) return;
		event.preventDefault();
		clipboard.setData('text/plain', selection);
	}

	function handlePaste(event: ClipboardEvent): void {
		const clipboard = event.clipboardData;
		const text = clipboard?.getData('text/plain') ?? clipboard?.getData('text') ?? '';
		if (!text) return;
		event.preventDefault();
		event.stopPropagation();
		if (exitHandled) return;
		socket?.send(text);
	}

	function handlePointerDown(): void {
		const active = document.activeElement;
		if (active instanceof HTMLElement && active !== container && !container.contains(active)) {
			active.blur();
		}
		focus();
	}

	function handleLinkClick(event: MouseEvent): void {
		if (event.button !== 0) return;
		if (event.altKey) return;
		if (!event.shiftKey && !event.ctrlKey && !event.metaKey) {
			event.stopImmediatePropagation();
		}
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

		writer = createTerminalWriter((data, done) => {
			term.write(data, done);
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

		container.addEventListener('copy', handleCopy, true);
		container.addEventListener('paste', handlePaste, true);
		container.addEventListener('pointerdown', handlePointerDown);
		container.addEventListener('click', handleLinkClick, { capture: true });
		const textarea = term.textarea;
		const onTextareaFocus = () => {
			if (!disposed) term.options.cursorBlink = true;
		};
		const onTextareaBlur = () => {
			if (!disposed) term.options.cursorBlink = false;
		};
		textarea?.addEventListener('focus', onTextareaFocus);
		textarea?.addEventListener('blur', onTextareaBlur);
		textareaFocusCleanup = () => {
			textarea?.removeEventListener('focus', onTextareaFocus);
			textarea?.removeEventListener('blur', onTextareaBlur);
		};

		resizeObserver = new ResizeObserver(() => scheduleFit());
		resizeObserver.observe(container);
		const handleWindowResize = () => scheduleFit();
		window.addEventListener('resize', handleWindowResize);

		const onTermResize = term.onResize((size: { rows: number; cols: number }) => {
			scheduleResize(size.rows, size.cols);
		});
		const onTermData = term.onData((data: string) => {
			if (disposed || exitHandled) return;
			socket?.send(data);
		});
		const onTermBinary = term.onBinary((data: string) => {
			if (disposed || exitHandled) return;
			socket?.send(data);
		});
		onDestroy(() => {
			onTermResize.dispose();
			onTermData.dispose();
			onTermBinary.dispose();
			window.removeEventListener('resize', handleWindowResize);
			container.removeEventListener('copy', handleCopy, true);
			container.removeEventListener('paste', handlePaste, true);
			container.removeEventListener('pointerdown', handlePointerDown);
			container.removeEventListener('click', handleLinkClick, { capture: true });
			textareaFocusCleanup?.();
		});

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
		const finalize = () => {
			try {
				const buffer = serializeAddon?.serialize();
				if (buffer && buffer.length > 0 && term) {
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
		};
		if (writer) writer.flush(finalize);
		else finalize();
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
