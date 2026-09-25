<script lang="ts">
	import type { Snippet } from 'svelte';
	import Icon from '$lib/components/icons/Icon.svelte';
	import type { IconName } from '$lib/components/icons/paths';
	import IconButton from './IconButton.svelte';
	import { cn } from '$lib/utils/cn';

	export interface MenuItem {
		id: string;
		label: string;
		icon?: IconName;
		danger?: boolean;
		disabled?: boolean;
		onselect?: () => void;
	}

	interface Props {
		items: MenuItem[];
		label?: string;
		align?: 'left' | 'right';
		width?: string;
		class?: string;
		trigger?: Snippet;
	}

	let {
		items,
		label = 'Open menu',
		align = 'right',
		width = '12rem',
		class: className = '',
		trigger,
	}: Props = $props();

	let open = $state(false);
	let root: HTMLDivElement | undefined = $state();

	function toggle(): void {
		open = !open;
	}

	function select(item: MenuItem): void {
		if (item.disabled) return;
		open = false;
		item.onselect?.();
	}

	function onwindowclick(event: MouseEvent): void {
		if (!open) return;
		const target = event.target as Node | null;
		if (target && root?.contains(target)) return;
		open = false;
	}

	function onkeydown(event: KeyboardEvent): void {
		if (event.key === 'Escape') open = false;
	}
</script>

<svelte:window onclick={onwindowclick} {onkeydown} />

<div bind:this={root} class={cn('relative inline-flex', className)}>
	{#if trigger}
		<button
			type="button"
			onclick={toggle}
			aria-haspopup="menu"
			aria-expanded={open}
			class="inline-flex"
		>
			{@render trigger()}
		</button>
	{:else}
		<IconButton icon="more-horizontal" {label} compact onclick={toggle} />
	{/if}

	{#if open}
		<div
			role="menu"
			style:width
			class={cn(
				'animate-panel-in absolute top-full z-40 mt-1 overflow-hidden rounded-lg border border-border bg-popover p-1 text-body shadow-popover',
				align === 'right' ? 'right-0' : 'left-0',
			)}
		>
			{#each items as item (item.id)}
				<button
					type="button"
					role="menuitem"
					disabled={item.disabled}
					onclick={() => select(item)}
					class={cn(
						'flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left transition-colors disabled:opacity-50',
						item.danger
							? 'text-destructive hover:bg-destructive/10'
							: 'text-foreground hover:bg-accent',
					)}
				>
					{#if item.icon}
						<Icon name={item.icon} size={14} />
					{/if}
					<span class="truncate">{item.label}</span>
				</button>
			{/each}
		</div>
	{/if}
</div>
