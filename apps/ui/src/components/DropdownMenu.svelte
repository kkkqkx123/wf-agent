<script lang="ts">
	import type { Snippet } from 'svelte';
	import Icon from '../icons/Icon.svelte';
	import type { IconName } from '../icons/paths';
	import IconButton from './IconButton.svelte';
	import { cn } from '../cn';

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
	let menu: HTMLDivElement | undefined = $state();

	function toggle(): void {
		open = !open;
	}

	function close(returnFocus = true): void {
		open = false;
		if (returnFocus) root?.querySelector('button')?.focus();
	}

	function select(item: MenuItem): void {
		if (item.disabled) return;
		close();
		item.onselect?.();
	}

	$effect(() => {
		if (!open || !menu) return;
		menu.querySelector<HTMLButtonElement>('button[role="menuitem"]')?.focus();
	});

	function onwindowclick(event: MouseEvent): void {
		if (!open) return;
		const target = event.target as Node | null;
		if (target && root?.contains(target)) return;
		close(false);
	}

	function onkeydown(event: KeyboardEvent): void {
		if (event.key === 'Escape') close();
	}

	function onmenukeydown(event: KeyboardEvent): void {
		const buttons = Array.from(
			menu?.querySelectorAll<HTMLButtonElement>('button[role="menuitem"]') ??
				[],
		).filter((button) => !button.disabled);
		if (buttons.length === 0) return;
		const index = buttons.indexOf(document.activeElement as HTMLButtonElement);
		if (event.key === 'ArrowDown') {
			event.preventDefault();
			buttons[(index + 1) % buttons.length]?.focus();
		} else if (event.key === 'ArrowUp') {
			event.preventDefault();
			buttons[(index - 1 + buttons.length) % buttons.length]?.focus();
		} else if (event.key === 'Home') {
			event.preventDefault();
			buttons[0]?.focus();
		} else if (event.key === 'End') {
			event.preventDefault();
			buttons[buttons.length - 1]?.focus();
		}
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
		<IconButton
			icon="more-horizontal"
			{label}
			compact
			aria-haspopup="menu"
			aria-expanded={open}
			onclick={toggle}
		/>
	{/if}

	{#if open}
		<div
			bind:this={menu}
			role="menu"
			tabindex="-1"
			onkeydown={onmenukeydown}
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
