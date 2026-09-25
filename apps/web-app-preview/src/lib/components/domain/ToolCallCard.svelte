<script lang="ts">
	import type { ToolCallEntry } from '$lib/types/models';
	import Icon from '$lib/components/icons/Icon.svelte';
	import type { IconName } from '$lib/components/icons/paths';
	import StatusBadge from './StatusBadge.svelte';
	import { formatDuration } from '$lib/utils/format';
	import { cn } from '$lib/utils/cn';

	interface Props {
		entry: ToolCallEntry;
		class?: string;
	}

	let { entry, class: className = '' }: Props = $props();

	let open = $state(false);

	const KIND_ICON: Record<string, IconName> = {
		bash: 'terminal',
		file: 'file',
		search: 'search',
		approval: 'shield',
		mcp: 'blocks',
		network: 'link',
	};

	const KIND_TONE: Record<string, string> = {
		bash: 'text-success',
		file: 'text-info',
		search: 'text-warning',
		approval: 'text-running',
		mcp: 'text-muted-foreground',
		network: 'text-muted-foreground',
	};

	const icon = $derived(KIND_ICON[entry.kind] ?? 'blocks');
	const tone = $derived(KIND_TONE[entry.kind] ?? 'text-muted-foreground');
</script>

<article
	class={cn(
		'overflow-hidden rounded-lg border border-border bg-card',
		className,
	)}
>
	<button
		type="button"
		onclick={() => (open = !open)}
		aria-expanded={open}
		class="flex w-full items-center gap-2 px-3 py-2 text-left transition-colors hover:bg-accent/50"
	>
		<Icon name={icon} size={14} class={cn('shrink-0', tone)} />
		<span class="min-w-0 flex-1 truncate font-mono text-caption text-foreground"
			>{entry.name}</span
		>
		<span class="shrink-0 text-micro tabular-nums text-muted-foreground">
			{formatDuration(entry.durationMs)}
		</span>
		<StatusBadge status={entry.status} size="sm" />
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
			<div>
				<p
					class="mb-1 text-micro uppercase tracking-wide text-muted-foreground"
				>
					Input
				</p>
				<pre
					class="overflow-x-auto rounded-md bg-muted px-2 py-1.5 font-mono text-micro text-foreground">{entry.input}</pre>
			</div>
			<div>
				<p
					class="mb-1 text-micro uppercase tracking-wide text-muted-foreground"
				>
					Output
				</p>
				<pre
					class="max-h-48 overflow-auto rounded-md bg-muted px-2 py-1.5 font-mono text-micro text-foreground">{entry.output}</pre>
			</div>
		</div>
	{/if}
</article>
