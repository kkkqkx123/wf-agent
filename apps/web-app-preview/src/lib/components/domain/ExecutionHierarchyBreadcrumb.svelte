<script lang="ts">
	import type { ExecutionHierarchy } from '$lib/types/models';
	import Icon from '@wf-agent/ui/icons/Icon.svelte';
	import { cn } from '@wf-agent/ui/cn';
	import { resolve } from '$app/paths';

	interface Props {
		hierarchy: ExecutionHierarchy;
		class?: string;
	}

	let { hierarchy, class: className = '' }: Props = $props();

	// The stored chain runs root-to-parent; the inspected execution closes it,
	// so the last crumb is this run and is not a link.
	const crumbs = $derived([
		...hierarchy.ancestors.map((executionId) => ({
			executionId,
			current: false,
		})),
		{ executionId: hierarchy.executionId, current: true },
	]);
</script>

<nav
	aria-label="Execution ancestry"
	class={cn('flex flex-wrap items-center gap-1 text-caption', className)}
>
	{#each crumbs as crumb, index (crumb.executionId)}
		{#if index > 0}
			<Icon name="chevron-right" size={12} class="text-muted-foreground" />
		{/if}
		{#if crumb.current}
			<span class="font-medium text-foreground">{crumb.executionId}</span>
		{:else}
			<a
				href={resolve('/executions/[id]', { id: crumb.executionId })}
				class="truncate text-muted-foreground underline-offset-2 hover:text-foreground hover:underline"
			>
				{crumb.executionId}
			</a>
		{/if}
	{/each}
	<span
		class="rounded border border-border px-1.5 text-micro text-muted-foreground"
	>
		{hierarchy.depth === 0 ? 'root execution' : `depth ${hierarchy.depth}`}
	</span>
</nav>
