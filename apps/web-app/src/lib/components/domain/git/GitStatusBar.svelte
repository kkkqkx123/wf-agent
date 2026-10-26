<script lang="ts">
	import { git as gitStore } from '#lib/stores/git.svelte';
	import { visibleControls } from '#lib/git/bindings';
	import { GUARDED_DANGERS } from '#lib/git/workbench.svelte';
	import type { GitWorkbench } from '#lib/git/workbench.svelte';

	interface Props {
		workbench: GitWorkbench;
	}

	let { workbench }: Props = $props();

	let controls = $derived(visibleControls(workbench.panel));
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
	<span class="flex flex-wrap items-center gap-x-2 gap-y-0.5" role="group" aria-label="Actions">
		{#each controls as control (control.id)}
			<button
				type="button"
				disabled={gitStore.busy}
				title={`${control.name} (${control.hint})`}
				onclick={() => void workbench.runActionId(control.id)}
				class="rounded-sm px-1 hover:bg-accent disabled:opacity-50 {GUARDED_DANGERS.has(
					control.danger,
				)
					? 'text-destructive'
					: 'text-foreground'}"
			>
				<span class="text-muted-foreground">{control.hint}</span>
				{control.name}
			</button>
		{/each}
	</span>
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
	</span>
</div>
