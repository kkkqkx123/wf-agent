<script lang="ts">
	import Badge from '$lib/components/ui/Badge.svelte';
	import type { BadgeSize, BadgeVariant } from '$lib/components/ui/variants';
	import { statusLabel, statusTone, type StatusTone } from '$lib/utils/status';
	import { cn } from '$lib/utils/cn';

	interface Props {
		status: string | null | undefined;
		dot?: boolean;
		size?: BadgeSize;
		class?: string;
	}

	let {
		status,
		dot = true,
		size = 'md',
		class: className = '',
	}: Props = $props();

	const TONE_VARIANT: Record<StatusTone, BadgeVariant> = {
		success: 'success',
		danger: 'danger',
		running: 'running',
		warning: 'warning',
		info: 'info',
		neutral: 'neutral',
	};

	const tone = $derived(statusTone(status));
	const variant = $derived(TONE_VARIANT[tone]);
</script>

<Badge {variant} {size} class={className}>
	{#if dot}
		<span
			class={cn(
				'h-1.5 w-1.5 shrink-0 rounded-full bg-current',
				tone === 'running' && 'animate-pulse-dot',
			)}
		></span>
	{/if}
	{statusLabel(status)}
</Badge>
