<script lang="ts">
	import { onDestroy, onMount } from 'svelte';
	import { Terminal } from '@xterm/xterm';
	import { FitAddon } from '@xterm/addon-fit';
	import { SerializeAddon } from '@xterm/addon-serialize';
	import { WebLinksAddon } from '@xterm/addon-web-links';
	import '@xterm/xterm/css/xterm.css';
	import { connectPty, createTerminalWriter, getPty } from '#lib/services/pty';
	import type { PtySession } from '#lib/services/pty';
	import type { PtySessionStore } from '#lib/stores/pty-session.svelte';
	import { resolvedTheme } from '#lib/stores/theme.svelte';
	import { ptyTrace } from './pty-debug';

	interface Props {
		session: {
			id: string;
			buffer?: string;
			scrollY?: number;
			cursor?: number;
			status?: 'running' | 'exited';
			rows?: number;
			cols?: number;
		};
		store: PtySessionStore;
		onConnect?: (id: string) => void;
		onSessionExited?: (id: string) => void;
		onConnectFailed?: (id: string) => void;
		/** Keep-alive registry: the multi-tab host tracks every mounted instance by session id. */
		register?: (id: string, api: TerminalApi) => void;
		unregister?: (id: string) => void;
	}

	export interface TerminalApi {
		focus: () => void;
		adoptServerSize: (id: string, rows: number, cols: number) => void;
		refit: () => void;
	}

	let {
		session,
		store,
		onConnect,
		onSessionExited,
		onConnectFailed,
		register,
		unregister,
	}: Props = $props();

	// Snapshot the mount identity: live props can read null during teardown,
	// and teardown must act on the session this instance was created with.
	// svelte-ignore state_referenced_locally
	const sessionId = typeof session?.id === 'string' ? session.id : '';

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
	// Best-known server size; skips the first sync when the viewport already matches.
	// Seeded inside onMount so the prop is read from within a closure.
	let serverKnown: { rows: number; cols: number } | undefined;
	let resizeObserver: ResizeObserver;
	let connected = false;
	let tries = 0;
	let reconnectTimer: ReturnType<typeof setTimeout> | undefined;
	let disposed = false;
	let socket: ReturnType<typeof connectPty> | null = null;
	// Keystrokes typed while the snapshot restores and the socket is not
	// open yet; flushed in order right after open() so nothing typed is lost.
	let earlyInput = '';
	const EARLY_INPUT_MAX = 4096;
	let writer: ReturnType<typeof createTerminalWriter>;
	let cursor = 0;
	let textareaFocusCleanup: (() => void) | undefined;

	const encoder = new TextEncoder();

	function byteLength(text: string): number {
		for (let i = 0; i < text.length; i += 1) {
			if (text.charCodeAt(i) > 127) return encoder.encode(text).length;
		}
		return text.length;
	}

	function isValidTerminalSize(rows: number, cols: number): boolean {
		return Number.isInteger(rows) && Number.isInteger(cols) && rows >= 1 && cols >= 1;
	}

	function scheduleFit(): void {
		if (fitFrame !== undefined || disposed) return;
		if (!container || container.clientWidth <= 0 || container.clientHeight <= 0) return;
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
		if (disposed || !isValidTerminalSize(rows, cols)) return;
		if (lastSize?.cols === cols && lastSize?.rows === rows) {
			pendingSize = undefined;
			if (sizeTimer !== undefined) clearTimeout(sizeTimer);
			sizeTimer = undefined;
			return;
		}
		if (!lastSize && serverKnown?.cols === cols && serverKnown?.rows === rows) {
			lastSize = { cols, rows };
			pendingSize = undefined;
			return;
		}
		pendingSize = { cols, rows };
		if (!lastSize) {
			lastSize = pendingSize;
			pendingSize = undefined;
			store.syncSize(sessionId, rows, cols);
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
			store.syncSize(sessionId, next.rows, next.cols);
		}, RESIZE_DEBOUNCE_MS);
	}

	function fitNow(): void {
		if (disposed || !container) return;
		if (container.clientWidth <= 0 || container.clientHeight <= 0) return;
		try {
			fitAddon.fit();
		} catch {
			// Container not ready; the observer will retry.
		}
	}

	function flushPendingResize(): void {
		if (sizeTimer !== undefined) {
			clearTimeout(sizeTimer);
			sizeTimer = undefined;
		}
		const next = pendingSize;
		pendingSize = undefined;
		if (!next || disposed) return;
		if (lastSize?.cols === next.cols && lastSize?.rows === next.rows) return;
		lastSize = next;
		store.syncSize(sessionId, next.rows, next.cols);
	}

	/** Probe the server before reconnecting: exited sessions stay view-only. */
	async function sessionAlive(): Promise<boolean> {
		try {
			const view: PtySession = await getPty(sessionId);
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
		ptyTrace('term.reconnect', { id: sessionId, tries, startCursor });
		if (tries >= MAX_RECONNECT_TRIES) {
			onConnectFailed?.(sessionId);
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
		ptyTrace('term.exited', { id: sessionId });
		if (!store.byId(sessionId) || store.isRemoved(sessionId)) return;
		store.patch(sessionId, { status: 'exited' });
		onSessionExited?.(sessionId);
	}

	function open(startCursor: number): void {
		if (disposed) return;
		clearReconnect();
		ptyTrace('term.open', { id: sessionId, startCursor });
		socket?.dispose();

		socket = connectPty(sessionId, startCursor, {
			onMeta: (next) => {
				cursor = next;
				if (!connected) {
					connected = true;
					tries = 0;
					ptyTrace('term.socket.open', { id: sessionId, cursor: next });
					onConnect?.(sessionId);
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
				ptyTrace('term.socket.close', { id: sessionId, code, disposed });
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

	/** Re-measure after a hidden tab becomes visible; no-op while still hidden. */
	export function refit(): void {
		scheduleFit();
	}

	/** Adopt the server size only when no local measurement owns the viewport yet. */
	export function adoptServerSize(id: string, rows: number, cols: number): void {
		if (id !== sessionId || disposed) return;
		if (!Number.isInteger(rows) || !Number.isInteger(cols) || rows < 1 || cols < 1) return;
		if (lastSize !== undefined || pendingSize !== undefined) return;
		serverKnown = { rows, cols };
		const current = store.byId(id);
		if (current && (current.rows !== rows || current.cols !== cols)) {
			store.patch(id, { rows, cols });
		}
	}

	function handleCopy(event: ClipboardEvent): void {
		const selection = term?.getSelection();
		if (!selection) return;
		const clipboard = event.clipboardData;
		if (!clipboard) return;
		event.preventDefault();
		clipboard.setData('text/plain', selection);
	}

	/** Bracketed-paste mode (DECSET 2004) observed from the app; paste wraps markers when on. */
	let bracketedPaste = false;

	/** Send input, stashing it while the socket is not open yet so keystrokes
	 * typed during snapshot restore are delivered after open(), in order. */
	function sendInput(data: string): void {
		if (disposed || exitHandled) return;
		if (socket) {
			socket.send(data);
			return;
		}
		if (earlyInput.length < EARLY_INPUT_MAX) earlyInput += data;
	}

	function handlePaste(event: ClipboardEvent): void {
		const clipboard = event.clipboardData;
		const text = clipboard?.getData('text/plain') ?? clipboard?.getData('text') ?? '';
		if (!text) return;
		event.preventDefault();
		event.stopPropagation();
		sendInput(bracketedPaste ? `\x1b[200~${text}\x1b[201~` : text);
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
		// The host guards with store.active(), but the dynamic import splits
		// the guard and the mount across flushes; never mount a dead view.
		if (!session || typeof session.id !== 'string') {
			ptyTrace('term.mount.invalid', {});
			return;
		}
		const restore = session.buffer ?? '';
		const restoreScrollY = session.scrollY ?? 0;
		cursor = session.cursor ?? 0;
		const openCursor = session.cursor ?? (restore ? -1 : 0);
		ptyTrace('term.mount', {
			id: sessionId,
			status: session.status ?? null,
			buffered: restore.length,
			openCursor,
		});
		if (
			typeof session.rows === 'number' &&
			typeof session.cols === 'number' &&
			Number.isInteger(session.rows) &&
			Number.isInteger(session.cols) &&
			session.rows >= 1 &&
			session.cols >= 1
		) {
			serverKnown = { rows: session.rows, cols: session.cols };
		}

		term = new Terminal({
			cursorBlink: true,
			cursorStyle: 'bar',
			fontSize: FONT_SIZE,
			fontFamily:
				"'JetBrains Mono', 'SF Mono', Menlo, Consolas, 'Noto Sans Mono CJK SC', 'PingFang SC', 'Hiragino Sans GB', 'Microsoft YaHei', monospace",
			scrollback: SCROLLBACK,
			convertEol: false,
		});
		serializeAddon = new SerializeAddon();
		fitAddon = new FitAddon();
		term.loadAddon(serializeAddon);
		term.loadAddon(fitAddon);
		term.loadAddon(new WebLinksAddon());
		// Copy/paste shortcuts belong to the browser: with a selection Ctrl+C
		// must reach the copy event (consuming it as terminal input would send
		// \x03 and clear the selection first); Ctrl+V must reach the paste
		// event (consuming it would send \x16 and no paste would ever arrive).
		term.attachCustomKeyEventHandler((event) => {
			if (event.ctrlKey && !event.altKey && !event.metaKey) {
				const key = event.key.toLowerCase();
				if (key === 'c' && term.hasSelection()) return false;
				if (key === 'v') return false;
			}
			return true;
		});
		// Observe bracketed-paste mode (DECSET 2004) for paste wrapping.
		// Returning false leaves default mode handling untouched.
		const trackPasteMode = (params: ArrayLike<number | number[]>, on: boolean): boolean => {
			if (Array.from(params).flat().includes(2004)) bracketedPaste = on;
			return false;
		};
		term.parser.registerCsiHandler({ prefix: '?', final: 'h' }, (params) =>
			trackPasteMode(params, true),
		);
		term.parser.registerCsiHandler({ prefix: '?', final: 'l' }, (params) =>
			trackPasteMode(params, false),
		);
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
		register?.(sessionId, { focus, adoptServerSize, refit });

		const onTermResize = term.onResize((size: { rows: number; cols: number }) => {
			scheduleResize(size.rows, size.cols);
		});
		const onTermData = term.onData((data: string) => {
			sendInput(data);
		});
		const onTermBinary = term.onBinary((data: string) => {
			sendInput(data);
		});
		onDestroy(() => {
			onTermResize.dispose();
			onTermData.dispose();
			onTermBinary.dispose();
			window.removeEventListener('resize', handleWindowResize);
			container.removeEventListener('copy', handleCopy, true);
			container.removeEventListener('paste', handlePaste, true);
			container.removeEventListener('pointerdown', handlePointerDown);
			container.removeEventListener('click', handleLinkClick, {
				capture: true,
			});
			textareaFocusCleanup?.();
		});

		fitNow();
		void document.fonts?.ready.then(() => scheduleFit());

		// Exited sessions stay view-only until the user clones them.
		// Restore through the coalescing writer in slices so a large
		// snapshot never blocks the main thread in one parse.
		const restoreAndOpen = async (): Promise<void> => {
			if (restore) {
				const STEP = 32_768;
				let index = 0;
				while (index < restore.length) {
					if (disposed) return;
					let end = Math.min(index + STEP, restore.length);
					if (end < restore.length) {
						const tail = restore.charCodeAt(end - 1);
						if (tail >= 0xd800 && tail <= 0xdbff) end -= 1;
					}
					writer.push(restore.slice(index, end));
					await new Promise<void>((resolve) => writer.flush(() => resolve()));
					index = end;
				}
				if (disposed) return;
				if (restoreScrollY > 0) {
					try {
						term.scrollToLine(restoreScrollY);
					} catch {
						// Container may be detached; observer retries fit.
					}
				}
			}
			if (disposed) return;
			if (session.status !== 'exited') {
				// Push the measured size before attaching so the session
				// starts with the current viewport.
				flushPendingResize();
				open(openCursor);
				// open() assigns the socket synchronously; anything typed during
				// the restore above queues in the wrapper and flushes on connect.
				if (earlyInput) {
					const pending = earlyInput;
					earlyInput = '';
					socket?.send(pending);
				}
			}
		};
		void restoreAndOpen();
	});

	onDestroy(() => {
		disposed = true;
		unregister?.(sessionId);
		ptyTrace('term.destroy', { id: sessionId, store: store.uid });
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
					store.saveBuffer(sessionId, buffer, term.buffer.active.viewportY, cursor);
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
