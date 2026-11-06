// @vitest-environment jsdom
/**
 * Reproduction harness for the IDE terminal panel issues:
 * - closing the last terminal looks like a fresh terminal instead of closing
 * - creating a terminal takes two clicks
 *
 * Mounts the real PtyPanel with a fake PTY service and stubbed xterm, then
 * drives the actual rendered buttons and asserts store + DOM state.
 */
import { beforeEach, describe, expect, it, vi } from 'vitest';
// Import the client entry directly: under vitest SSR resolution the bare
// 'svelte' specifier settles on the server build where mount() is stubbed.
import { mount, tick, unmount } from '/home/kkkqkx/code/wf-agent/apps/node_modules/svelte/src/index-client.js';

// jsdom lacks these; the terminal view touches them on mount.
if (typeof globalThis.ResizeObserver === 'undefined') {
	globalThis.ResizeObserver = class {
		observe(): void {}
		unobserve(): void {}
		disconnect(): void {}
	} as unknown as typeof ResizeObserver;
}
if (typeof globalThis.requestAnimationFrame === 'undefined') {
	globalThis.requestAnimationFrame = ((callback: FrameRequestCallback): number =>
		setTimeout(() => callback(Date.now()), 0) as unknown as number) as typeof requestAnimationFrame;
	globalThis.cancelAnimationFrame = ((handle: number): void => {
		clearTimeout(handle);
	}) as typeof cancelAnimationFrame;
}

interface FakePty {
	id: string;
	title: string;
	status: 'running' | 'exited';
	exit_code: number | null;
	rows: number;
	cols: number;
}

const server = new Map<string, FakePty>();
let seq = 0;

vi.mock('#lib/services/pty', async (importOriginal) => {
	const original = await importOriginal<typeof import('#lib/services/pty')>();
	return {
		...original,
		createPty: async (input: {
			cwd?: string;
			command?: string;
			rows?: number;
			cols?: number;
			title?: string;
		} = {}) => {
			seq += 1;
			const session: FakePty = {
				id: `pty-repro-${seq}`,
				title: input.title ?? `Terminal ${seq}`,
				status: 'running',
				exit_code: null,
				rows: 24,
				cols: 80,
			};
			server.set(session.id, { ...session });
			return { ...session };
		},
		listPtys: async () => [...server.values()].map((s) => ({ ...s })),
		getPty: async (id: string) => {
			const session = server.get(id);
			if (!session) throw new Error(`PTY session not found: ${id}`);
			return { ...session };
		},
		updatePty: async (id: string, patch: { title?: string; rows?: number; cols?: number }) => {
			const session = server.get(id);
			if (!session) throw new Error(`PTY session not found: ${id}`);
			const next = {
				...session,
				title: patch.title ?? session.title,
				rows: patch.rows ?? session.rows,
				cols: patch.cols ?? session.cols,
			};
			server.set(id, next);
			return { ...next };
		},
		closePty: async (id: string) => {
			server.delete(id);
		},
		connectPty: (
			_id: string,
			_cursor: number,
			options: { onMeta?: (cursor: number) => void } = {},
		) => {
			setTimeout(() => options.onMeta?.(0), 0);
			return {
				socket: () => null,
				send: () => {},
				dispose: () => {},
			};
		},
	};
});

vi.mock('@xterm/xterm', () => {
	class Terminal {
		options: Record<string, unknown>;
		textarea: HTMLTextAreaElement | null = null;
		parser = { registerCsiHandler: () => {} };
		buffer = { active: { viewportY: 0 } };
		constructor(options: Record<string, unknown> = {}) {
			this.options = { ...options };
		}
		loadAddon(): void {}
		open(container: HTMLElement): void {
			this.textarea = document.createElement('textarea');
			container.appendChild(this.textarea);
		}
		write(_data: string, callback?: () => void): void {
			callback?.();
		}
		focus(): void {}
		dispose(): void {}
		hasSelection(): boolean {
			return false;
		}
		getSelection(): string {
			return '';
		}
		scrollToLine(): void {}
		attachCustomKeyEventHandler(): void {}
		onData(_listener: unknown): { dispose(): void } {
			return { dispose: () => {} };
		}
		onBinary(_listener: unknown): { dispose(): void } {
			return { dispose: () => {} };
		}
		onResize(_listener: unknown): { dispose(): void } {
			return { dispose: () => {} };
		}
	}
	return { Terminal };
});

vi.mock('@xterm/addon-fit', () => ({
	FitAddon: class {
		fit(): void {}
	},
}));

vi.mock('@xterm/addon-serialize', () => ({
	SerializeAddon: class {
		serialize(): string {
			return '';
		}
	},
}));

vi.mock('@xterm/addon-web-links', () => ({
	WebLinksAddon: class {},
}));

const { default: PtyPanel } = await import('./PtyPanel.svelte');
const { ptyStoreFor } = await import('#lib/stores/pty-session.svelte');
const { t } = await import('#lib/i18n');

async function settle(rounds = 8): Promise<void> {
	for (let i = 0; i < rounds; i += 1) {
		await new Promise<void>((resolve) => setTimeout(resolve, 0));
		await tick();
	}
}

function buttons(host: HTMLElement): HTMLButtonElement[] {
	return [...host.querySelectorAll('button')];
}

function findButton(host: HTMLElement, label: string): HTMLButtonElement | undefined {
	return buttons(host).find((button) => button.getAttribute('aria-label') === label);
}

function tabCloseButtons(host: HTMLElement): HTMLButtonElement[] {
	const label = t('pty.ptyPanel.closeTerminal');
	return buttons(host).filter((button) => button.getAttribute('aria-label') === label);
}

describe('PtyPanel terminal open/close behaviour', () => {
	beforeEach(() => {
		server.clear();
		seq = 0;
		localStorage.clear();
		document.body.innerHTML = '';
	});

	it('creates exactly one terminal per plus click', async () => {
		const host = document.createElement('div');
		document.body.appendChild(host);
		const app = mount(PtyPanel, {
			target: host,
			props: { repo: 'repro-create', open: true, embedded: true },
		});
		try {
			await settle();
			const store = ptyStoreFor('repro-create');
			expect(store.sessions.length).toBe(1);

			findButton(host, t('pty.ptyPanel.newTerminal'))!.click();
			await settle();
			expect(store.sessions.length).toBe(2);
		} finally {
			unmount(app);
			host.remove();
		}
	});

	it('closing the last terminal empties the panel and stays empty', async () => {
		const host = document.createElement('div');
		document.body.appendChild(host);
		const app = mount(PtyPanel, {
			target: host,
			props: { repo: 'repro-close', open: true, embedded: true },
		});
		try {
			await settle();
			const store = ptyStoreFor('repro-close');
			expect(store.sessions.length).toBe(1);

			tabCloseButtons(host)[0]?.click();
			await settle();

			expect(store.sessions.length).toBe(0);
			expect(store.activeId).toBeNull();
			expect(host.textContent).toContain(t('pty.ptyPanel.noSessions'));

			// No delayed resurrection without a remount.
			await settle(4);
			expect(store.sessions.length).toBe(0);
		} finally {
			unmount(app);
			host.remove();
		}
	});

	it('remounting after closing the last terminal does not resurrect one', async () => {
		const host = document.createElement('div');
		document.body.appendChild(host);
		const first = mount(PtyPanel, {
			target: host,
			props: { repo: 'repro-remount', open: true, embedded: true },
		});
		await settle();
		const store = ptyStoreFor('repro-remount');
		expect(store.sessions.length).toBe(1);
		tabCloseButtons(host)[0]?.click();
		await settle();
		expect(store.sessions.length).toBe(0);
		unmount(first);
		host.innerHTML = '';

		const second = mount(PtyPanel, {
			target: host,
			props: { repo: 'repro-remount', open: true, embedded: true },
		});
		try {
			await settle();
			expect(store.sessions.length).toBe(0);
		} finally {
			unmount(second);
			host.remove();
		}
	});
});
