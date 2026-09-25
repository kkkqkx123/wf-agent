<script lang="ts">
	import type { Snippet } from 'svelte';
	import type {
		HTMLAnchorAttributes,
		HTMLButtonAttributes,
	} from 'svelte/elements';
	import { resolve } from '$app/paths';
	import { buttonClass, type ButtonSize, type ButtonVariant } from './variants';
	import { cn } from '$lib/utils/cn';
	import type { AppPath } from '$lib/utils/route';

	interface Props extends Omit<HTMLButtonAttributes, 'size'> {
		variant?: ButtonVariant;
		size?: ButtonSize;
		/** When set the button renders as an internal link resolved through the base path. */
		href?: AppPath;
		active?: boolean;
		class?: string;
		children: Snippet;
	}

	let {
		variant = 'default',
		size = 'md',
		href,
		active = false,
		class: className = '',
		children,
		...rest
	}: Props = $props();

	const anchorRest = $derived(href ? (rest as HTMLAnchorAttributes) : {});
</script>

{#if href}
	<a
		href={resolve(href)}
		class={cn(
			buttonClass(variant, size),
			active && 'ring-1 ring-ring',
			className,
		)}
		{...anchorRest}
	>
		{@render children()}
	</a>
{:else}
	<button
		type="button"
		class={cn(
			buttonClass(variant, size),
			active && 'ring-1 ring-ring',
			className,
		)}
		{...rest}
	>
		{@render children()}
	</button>
{/if}
