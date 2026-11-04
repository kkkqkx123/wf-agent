<script lang="ts">
	import type { GitWorkbench } from '#lib/git/workbench.svelte';

	interface Props {
		workbench: GitWorkbench;
	}

	let { workbench }: Props = $props();
</script>

<div
	class="flex flex-wrap items-center gap-x-3 gap-y-1 border border-border bg-card px-2 py-1 font-mono text-micro"
	role="status"
	aria-label="Git key hints"
>
	<span class="font-semibold tracking-wider text-info uppercase">[{workbench.panel}]</span>
	{#if workbench.selection}
		<span class="max-w-64 truncate text-muted-foreground">{workbench.selection}</span>
	{/if}
	<span class="text-muted-foreground">{workbench.controls.length} actions</span>
	<span class="ml-auto flex items-center gap-2">
		<button
			type="button"
			title="All actions"
			onclick={() => {
				workbench.helpOpen = true;
			}}
			class="rounded-sm px-1 text-foreground hover:bg-accent"
		>
			<span class="text-muted-foreground">?</span> help
		</button>
		<button
			type="button"
			title="Focus filter"
			onclick={() => workbench.focusFilter()}
			class="rounded-sm px-1 text-foreground hover:bg-accent"
		>
			<span class="text-muted-foreground">/</span> filter
		</button>
		<button
			type="button"
			title="Focus key input"
			onclick={() => workbench.focusKeyInput()}
			class="rounded-sm px-1 text-foreground hover:bg-accent"
		>
			<span class="text-muted-foreground">^/</span> keys
		</button>
	</span>
</div>
