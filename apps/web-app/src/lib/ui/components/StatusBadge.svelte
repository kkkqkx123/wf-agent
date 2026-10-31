<script lang="ts">
	import Badge from './Badge.svelte';
	import type { BadgeVariant } from './variants';
	import Icon from '../icons/Icon.svelte';
	import { statusIcon, statusLabel, statusTone, type StatusTone } from '../status';
	import { cn } from '../cn';

	interface Props {
		status: string | null | undefined;
		dot?: boolean;
		icon?: boolean;
		size?: 'sm' | 'md';
		class?: string;
	}

	let { status, dot = true, icon = true, size = 'md', class: className = '' }: Props = $props();

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
	const iconName = $derived(statusIcon(status));
</script>

<Badge {variant} class={cn(size === 'sm' && 'px-1.5 py-0 text-micro', className)}>
	{#if icon}
		<Icon name={iconName} size={size === 'sm' ? 11 : 12} />
	{/if}
	{#if dot}
		<span
			aria-hidden="true"
			class={cn(
				'h-1.5 w-1.5 shrink-0 rounded-full bg-current',
				tone === 'running' && 'animate-pulse-dot',
			)}
		></span>
	{/if}
	{statusLabel(status)}
</Badge>
