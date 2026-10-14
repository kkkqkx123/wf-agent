<script lang="ts">
	import type { Snippet } from 'svelte';
	import Icon from '@wf-agent/ui/icons/Icon.svelte';
	import ToolCallCard from '$lib/components/domain/ToolCallCard.svelte';
	import {
		describeToolGroup,
		groupToolNames,
		type ToolCallGroup,
	} from '$lib/utils/tool-output';
	import type { ToolCallEntry } from '$lib/types/models';
	import { cn } from '@wf-agent/ui/cn';

	interface Props {
		group: ToolCallGroup;
		open: boolean;
		ontoggle: () => void;
		/** Member row rendering; defaults to a plain tool card. Call sites
		 * pass their locate-on-graph wrapper here. */
		member?: Snippet<[ToolCallEntry]>;
		class?: string;
	}

	let {
		group,
		open,
		ontoggle,
		member,
		class: className = '',
	}: Props = $props();

	const title = $derived(describeToolGroup(group));
	const subtitle = $derived(groupToolNames(group));
</script>

<section
	class={cn(
		'overflow-hidden rounded-lg border border-border bg-card',
		className,
	)}
>
	<button
		type="button"
		onclick={ontoggle}
		aria-expanded={open}
		class="flex w-full items-center gap-2 px-3 py-2 text-left transition-colors hover:bg-accent/50"
	>
		<Icon
			name={group.kind === 'edit' ? 'file' : 'search'}
			size={14}
			class="shrink-0 text-muted-foreground"
		/>
		<span class="min-w-0 flex-1 truncate text-caption text-foreground">
			{title}
			<span class="ml-1 font-mono text-micro text-muted-foreground">
				{subtitle}
			</span>
		</span>
		<Icon
			name="chevron-down"
			size={14}
			class={cn(
				'shrink-0 text-muted-foreground transition-transform duration-150',
				open && 'rotate-180',
			)}
		/>
	</button>

	{#if open}
		<div class="animate-panel-in space-y-2 border-t border-border px-3 py-2.5">
			{#each group.members as entry (entry.id)}
				{#if member}
					{@render member(entry)}
				{:else}
					<ToolCallCard {entry} />
				{/if}
			{/each}
		</div>
	{/if}
</section>
