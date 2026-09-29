<script lang="ts">
	import type { HTMLButtonAttributes } from 'svelte/elements';
	import Icon from '$lib/components/icons/Icon.svelte';
	import type { IconName } from '$lib/components/icons/paths';
	import { buttonClass, type ButtonVariant } from './variants';
	import { cn } from '$lib/utils/cn';

	interface Props extends Omit<HTMLButtonAttributes, 'children'> {
		icon: IconName;
		label: string;
		variant?: ButtonVariant;
		/** Icon size in pixels. Named apart from Button's `size` steps (sm/md/lg). */
		iconSize?: number;
		compact?: boolean;
		active?: boolean;
		class?: string;
	}

	let {
		icon,
		label,
		variant = 'ghost',
		iconSize = 16,
		compact = false,
		active = false,
		class: className = '',
		...rest
	}: Props = $props();
</script>

<button
	type="button"
	aria-label={label}
	title={label}
	class={cn(
		buttonClass(variant, compact ? 'icon-sm' : 'icon'),
		active && 'bg-accent',
		className,
	)}
	{...rest}
>
	<Icon name={icon} size={iconSize} />
</button>
