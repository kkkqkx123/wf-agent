<script lang="ts">
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import type { ToolCallEntry } from '$lib/types/models';
	import Icon from '$lib/components/icons/Icon.svelte';
	import type { IconName } from '$lib/components/icons/paths';
	import Button from '$lib/components/ui/Button.svelte';
	import Dialog from '$lib/components/ui/Dialog.svelte';
	import StatusBadge from './StatusBadge.svelte';
	import JsonViewer from './JsonViewer.svelte';
	import { formatDuration } from '$lib/utils/format';
	import { cn } from '$lib/utils/cn';

	interface Props {
		entry: ToolCallEntry;
		class?: string;
	}

	let { entry, class: className = '' }: Props = $props();

	let open = $state(false);
	let resultOpen = $state(false);

	/** Outputs beyond this length open in a dialog instead of inline. */
	const INLINE_OUTPUT_LIMIT = 2000;

	const isApproval = $derived(entry.kind === 'approval');
	const hasLargeOutput = $derived(entry.output.length > INLINE_OUTPUT_LIMIT);

	function reviewApprovals(): void {
		void goto(resolve('/checkpoints?tab=approvals'));
	}

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
				<JsonViewer value={entry.input} collapsed />
			</div>
			<div>
				<p
					class="mb-1 text-micro uppercase tracking-wide text-muted-foreground"
				>
					Output
				</p>
				<JsonViewer value={entry.output} />
			</div>
			{#if isApproval}
				<div
					class="flex items-center justify-between gap-2 rounded-md bg-muted px-2 py-1.5"
				>
					<p class="text-micro text-muted-foreground">
						Resolution happens in the approvals queue
					</p>
					<Button variant="outline" size="sm" onclick={reviewApprovals}>
						Review
					</Button>
				</div>
			{/if}
			{#if hasLargeOutput}
				<div class="flex justify-end">
					<Button variant="ghost" size="sm" onclick={() => (resultOpen = true)}>
						<Icon name="maximize" size={13} />
						Full result
					</Button>
				</div>
			{/if}
		</div>
	{/if}

	<Dialog
		bind:open={resultOpen}
		title={entry.name}
		description="Complete tool result"
	>
		<JsonViewer value={entry.output} maxLength={32000} />
	</Dialog>
</article>
