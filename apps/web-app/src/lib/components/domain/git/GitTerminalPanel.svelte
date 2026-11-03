<script lang="ts">
	import PtyPanel from '#lib/components/pty/PtyPanel.svelte';
	import type { GitWorkbench } from '#lib/git/workbench.svelte';

	interface Props {
		workbench: GitWorkbench;
	}

	let { workbench }: Props = $props();

	// Git sessions never prompt for credentials and never page; both are
	// GIT_* variables the server allowlists for PTY env injection.
	const GIT_ENV: Record<string, string> = {
		GIT_TERMINAL_PROMPT: '0',
		GIT_PAGER: 'cat',
	};
</script>

<PtyPanel
	repo={workbench.meta?.root ?? ''}
	cwd={workbench.meta?.root}
	env={GIT_ENV}
	open={workbench.terminalOpen}
	onClose={() => {
		workbench.terminalOpen = false;
	}}
/>
