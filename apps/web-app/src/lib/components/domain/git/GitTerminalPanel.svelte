<script lang="ts">
	import PtyPanel from '#lib/components/pty/PtyPanel.svelte';
	import type { GitWorkbench } from '#lib/git/workbench.svelte';

	interface Props {
		workbench: GitWorkbench;
	}

	let { workbench }: Props = $props();
</script>

<PtyPanel
	repo={workbench.meta?.root ?? ''}
	open={workbench.terminalOpen}
	onClose={() => {
		workbench.terminalOpen = false;
	}}
/>
