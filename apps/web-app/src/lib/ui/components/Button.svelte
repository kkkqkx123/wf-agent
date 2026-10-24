<script lang="ts">
	import type { Snippet } from 'svelte';
	import type {
		HTMLAnchorAttributes,
		HTMLButtonAttributes,
	} from 'svelte/elements';
	import { resolveHref } from '../link';
	import { buttonClass, type ButtonSize, type ButtonVariant } from './variants';
	import { cn } from '../cn';

	interface Props extends Omit<HTMLButtonAttributes, 'size'> {
		variant?: ButtonVariant;
		size?: ButtonSize;
		/** When set the button renders as a link. The host installs the
		 * route-to-URL resolver; without one the route is used verbatim. */
		href?: string;
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
		href={resolveHref(href)}
		class={cn(
			buttonClass(variant, size),
			active && 'ring-1 ring-[hsl(var(--ring))]',
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
			active && 'ring-1 ring-[hsl(var(--ring))]',
			className,
		)}
		{...rest}
	>
		{@render children()}
	</button>
{/if}
