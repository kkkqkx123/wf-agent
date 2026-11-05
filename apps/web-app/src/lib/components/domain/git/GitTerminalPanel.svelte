<script lang="ts">
	import PtyPanel from '#lib/components/pty/PtyPanel.svelte';
	import type { GitWorkbench } from '#lib/git/workbench.svelte';

	interface Props {
		workbench: GitWorkbench;
	}

	let { workbench }: Props = $props();

	// Git sessions never prompt for credentials and never page; both are
	// consumed by git itself and applied to the spawned session as-is.
	const GIT_ENV: Record<string, string> = {
		GIT_TERMINAL_PROMPT: '0',
		GIT_PAGER: 'cat',
	};
</script>

<div
	class="mb-2 rounded-md border border-warning/40 bg-warning/5 px-2 py-1 font-mono text-micro text-warning"
>
	Terminal acts directly on the server repository ({workbench.meta?.root ?? 'unknown root'}).
	Destructive commands ask for confirmation here; prefer the key input for guarded actions.
</div>
<PtyPanel
	repo={workbench.meta?.root ?? ''}
	cwd={workbench.meta?.root}
	env={GIT_ENV}
	open={workbench.terminalOpen}
	onClose={() => {
		workbench.terminalOpen = false;
		workbench.layoutFocus = 'none';
	}}
/>
