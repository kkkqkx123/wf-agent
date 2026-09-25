<script lang="ts">
	import { cn } from '$lib/utils/cn';

	interface Props {
		shape?: 'line' | 'block' | 'circle';
		width?: string;
		height?: string;
		lines?: number;
		class?: string;
	}

	let {
		shape = 'line',
		width,
		height,
		lines = 1,
		class: className = '',
	}: Props = $props();
</script>

{#if shape === 'line' && lines > 1}
	<div class={cn('space-y-2', className)}>
		{#each Array.from({ length: lines }, (_, position) => position) as index (index)}
			<div
				class="skeleton-shimmer h-3 rounded-sm"
				style:width={index === lines - 1 ? '62%' : '100%'}
			></div>
		{/each}
	</div>
{:else}
	<div
		class={cn(
			'skeleton-shimmer',
			shape === 'circle'
				? 'rounded-full'
				: shape === 'block'
					? 'rounded-lg'
					: 'h-3 rounded-sm',
			className,
		)}
		style:width={width ?? (shape === 'line' ? '100%' : undefined)}
		style:height={height ?? (shape === 'block' ? '4rem' : undefined)}
	></div>
{/if}
