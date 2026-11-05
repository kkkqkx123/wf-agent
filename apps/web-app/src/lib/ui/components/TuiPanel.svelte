<script lang="ts">
	/**
	 * Terminal-style panel for dense operational surfaces (git, terminal).
	 * Business cards should use Card instead: Card carries rounded corners
	 * and a header/footer rhythm, while TuiPanel keeps square corners and
	 * an overlapped mono title for a terminal look.
	 */
	import type { Snippet } from 'svelte';
	import { cn } from '../cn';

	interface Props {
		title: string;
		id?: string;
		description?: string;
		count?: number;
		focused?: boolean;
		collapsible?: boolean;
		collapsed?: boolean;
		onToggle?: () => void;
		class?: string;
		bodyClass?: string;
		actions?: Snippet;
		children: Snippet;
		footer?: Snippet;
	}

	let {
		title,
		id,
		description,
		count,
		focused = false,
		collapsible = false,
		collapsed = false,
		onToggle,
		class: className = '',
		bodyClass = '',
		actions,
		children,
		footer,
	}: Props = $props();

	let label = $derived(count !== undefined ? `${title} (${count})` : title);
</script>

<section
	{id}
	class={cn(
		'relative flex min-h-0 flex-col rounded-none border bg-card text-card-foreground',
		focused ? 'border-info ring-1 ring-info/40' : 'border-border',
		className,
	)}
>
	{#if collapsible}
		<button
			type="button"
			aria-expanded={!collapsed}
			onclick={() => onToggle?.()}
			class={cn(
				'absolute -top-2 left-2 flex max-w-[70%] items-center gap-1 bg-card px-1 font-mono text-micro font-semibold tracking-wider uppercase',
				focused ? 'text-info' : 'text-muted-foreground',
			)}
		>
			<span aria-hidden="true">{collapsed ? '▸' : '▾'}</span>
			<span class="truncate">{label}</span>
		</button>
	{:else}
		<span
			aria-hidden="true"
			class={cn(
				'absolute -top-2 left-2 max-w-[70%] truncate bg-card px-1 font-mono text-micro font-semibold tracking-wider uppercase',
				focused ? 'text-info' : 'text-muted-foreground',
			)}
		>
			{label}
		</span>
	{/if}
	{#if actions}
		<span class="absolute -top-2.5 right-2 flex items-center gap-1 bg-card px-1">
			{@render actions()}
		</span>
	{/if}
	{#if !collapsed}
		{#if description}
			<p class="px-2 pt-2.5 font-mono text-micro text-muted-foreground">{description}</p>
		{/if}
		<div class={cn('min-h-0 px-2 py-1.5', bodyClass)}>
			{@render children()}
		</div>
		{#if footer}
			<footer class="border-t border-border px-2 py-1.5 font-mono text-micro text-muted-foreground">
				{@render footer()}
			</footer>
		{/if}
	{/if}
</section>
