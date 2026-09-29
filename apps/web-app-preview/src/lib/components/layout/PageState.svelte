<script lang="ts">
	import type { Snippet } from 'svelte';
	import EmptyState from '$lib/components/ui/EmptyState.svelte';
	import ErrorState from '$lib/components/ui/ErrorState.svelte';
	import Skeleton from '$lib/components/ui/Skeleton.svelte';
	import type { IconName } from '$lib/components/icons/paths';
	import { cn } from '$lib/utils/cn';

	interface Props {
		loading?: boolean;
		error?: string | null;
		empty?: boolean;
		emptyTitle?: string;
		emptyDescription?: string;
		emptyIcon?: IconName;
		skeletonLines?: number;
		errorTitle?: string;
		class?: string;
		onretry?: () => void;
		emptyActions?: Snippet;
		children: Snippet;
	}

	let {
		loading = false,
		error = null,
		empty = false,
		emptyTitle = 'Nothing to show',
		emptyDescription,
		emptyIcon,
		skeletonLines = 5,
		errorTitle = 'Failed to load',
		class: className = '',
		onretry,
		emptyActions,
		children,
	}: Props = $props();
</script>

<!-- Loading wins over error so a retry keeps the skeleton until it settles. -->
{#if loading}
	<Skeleton
		lines={skeletonLines}
		class={cn('rounded-lg border border-border bg-card p-4', className)}
	/>
{:else if error}
	<ErrorState
		title={errorTitle}
		description={error}
		{onretry}
		class={className}
	/>
{:else if empty}
	<EmptyState
		title={emptyTitle}
		description={emptyDescription}
		class={className}
		{...emptyIcon ? { icon: emptyIcon } : {}}
	>
		{#snippet actions()}
			{#if emptyActions}
				{@render emptyActions()}
			{/if}
		{/snippet}
	</EmptyState>
{:else}
	{@render children()}
{/if}
