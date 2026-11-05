<script lang="ts">
	import type { Snippet } from 'svelte';
	import Icon from '../icons/Icon.svelte';
	import type { IconName } from '../icons/paths';
	import IconButton from './IconButton.svelte';
	import FloatingPanel from './FloatingPanel.svelte';
	import type { FloatingCloseReason } from './FloatingPanel.svelte';
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

	function onFloatingClose(reason: FloatingCloseReason): void {
		close(reason === 'escape');
	}

	$effect(() => {
		if (!open || !menu) return;
		menu.querySelector<HTMLButtonElement>('button[role="menuitem"]')?.focus();
	});

	function onmenukeydown(event: KeyboardEvent): void {
		const buttons = Array.from(
			menu?.querySelectorAll<HTMLButtonElement>('button[role="menuitem"]') ?? [],
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

<div bind:this={root} class={cn('inline-flex', className)}>
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

	<FloatingPanel
		anchor={root ?? null}
		{open}
		side="bottom"
		align={align === 'right' ? 'end' : 'start'}
		{width}
		role="menu"
		ariaLabel={label}
		onClose={onFloatingClose}
	>
		<div
			bind:this={menu}
			tabindex="-1"
			role="menu"
			onkeydown={onmenukeydown}
			class="p-1 outline-none"
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
	</FloatingPanel>
</div>
