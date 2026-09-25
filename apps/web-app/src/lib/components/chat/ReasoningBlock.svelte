<script lang="ts">
	import Icon from '$lib/components/icons/Icon.svelte';
	import { cn } from '$lib/utils/cn';

	interface Props {
		content: string;
		streaming?: boolean;
		class?: string;
	}

	let { content, streaming = false, class: className = '' }: Props = $props();

	let open = $state(true);
</script>

<div
	class={cn(
		'overflow-hidden rounded-lg border border-border bg-muted/40',
		className,
	)}
>
	<button
		type="button"
		onclick={() => (open = !open)}
		aria-expanded={open}
		class="flex w-full items-center gap-2 px-3 py-1.5 text-left transition-colors hover:bg-accent/50"
	>
		<Icon
			name="sparkles"
			size={13}
			class={cn('shrink-0 text-info', streaming && 'animate-pulse-dot')}
		/>
		<span class="flex-1 text-caption font-medium text-muted-foreground">
			{streaming ? 'Thinking…' : 'Thinking'}
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
		<div
			class="max-h-56 overflow-y-auto border-t border-border px-3 py-2 text-caption whitespace-pre-wrap break-words text-muted-foreground"
		>
			{content}
		</div>
	{/if}
</div>
