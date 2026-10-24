<script lang="ts">
	import Icon from '../icons/Icon.svelte';
	import Button from './Button.svelte';
	import { formatNumber } from '../format';
	import { cn } from '../cn';

	interface Props {
		shown: number;
		hasMore: boolean;
		loading?: boolean;
		pageSize?: number;
		/** Auto-load the next page when the sentinel scrolls into view. */
		autoLoad?: boolean;
		class?: string;
		onloadmore?: () => void;
	}

	let {
		shown,
		hasMore,
		loading = false,
		pageSize = 50,
		autoLoad = false,
		class: className = '',
		onloadmore,
	}: Props = $props();

	let sentinel: HTMLElement | null = $state(null);

	// Auto-load fires when the sentinel becomes visible; loading guards keep a
	// slow page from being requested twice while it is in flight.
	$effect(() => {
		if (!autoLoad || !sentinel) return;
		const observer = new IntersectionObserver(
			(entries) => {
				const entry = entries[0];
				if (entry.isIntersecting && hasMore && !loading) onloadmore?.();
			},
			{ rootMargin: '10rem' },
		);
		observer.observe(sentinel);
		return () => observer.disconnect();
	});
</script>

<div
	class={cn(
		'flex items-center justify-between gap-3 border-t border-border px-3 py-2',
		className,
	)}
>
	<p class="text-caption text-muted-foreground">
		<!-- The contract exposes `has_more` instead of a total, so no page count is rendered. -->
		{formatNumber(shown)} loaded
	</p>
	<div class="flex items-center gap-2">
		{#if loading}
			<Icon
				name="loader"
				size={14}
				class="animate-spin text-muted-foreground"
			/>
		{/if}
		<Button
			variant="outline"
			size="sm"
			disabled={!hasMore || loading}
			onclick={() => onloadmore?.()}
		>
			{hasMore ? `Load next ${pageSize}` : 'End of results'}
		</Button>
	</div>
</div>

{#if autoLoad && hasMore}
	<div bind:this={sentinel} aria-hidden="true" class="h-px w-full"></div>
{/if}
