<script lang="ts">
	import Icon from '$lib/components/icons/Icon.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import { formatNumber } from '$lib/utils/format';
	import { cn } from '$lib/utils/cn';

	interface Props {
		shown: number;
		hasMore: boolean;
		loading?: boolean;
		pageSize: number;
		class?: string;
		onloadmore?: () => void;
	}

	let {
		shown,
		hasMore,
		loading = false,
		pageSize,
		class: className = '',
		onloadmore,
	}: Props = $props();
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
