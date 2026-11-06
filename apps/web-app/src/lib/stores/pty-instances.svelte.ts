import { SvelteMap } from 'svelte/reactivity';
import { isPtyRemoved } from '#lib/stores/pty-session.svelte';

/** Structural view handle; mirrors TerminalInstance's exported TerminalApi. */
export interface PtyViewApi {
	focus: () => void;
	adoptServerSize: (id: string, rows: number, cols: number) => void;
	refit: () => void;
}

/**
 * Module-level terminal view registry shared by every panel.
 * Panels only subscribe; the layout host uses the same table to know
 * which sessions already have a visible view and must not gain a second
 * socket from another mount of the same id.
 */
const instances = new SvelteMap<string, PtyViewApi>();

export function registerPtyInstance(id: string, api: PtyViewApi): void {
	if (!id || isPtyRemoved(id)) return;
	instances.set(id, api);
}

export function unregisterPtyInstance(id: string): void {
	if (!id) return;
	instances.delete(id);
}

export function ptyInstanceFor(id: string): PtyViewApi | undefined {
	return instances.get(id);
}

export function hasPtyInstance(id: string): boolean {
	return instances.has(id);
}

export function focusPtyInstance(id: string | null): void {
	if (!id) return;
	instances.get(id)?.focus();
}

export function refitPtyInstance(id: string | null): void {
	if (!id) return;
	instances.get(id)?.refit();
}
