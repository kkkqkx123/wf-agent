<script lang="ts">
	/**
	 * Execution type selector for the composer. The entry answers one
	 * question: which executor runs the message. Agent runs the full loop
	 * with tools, Chat answers tool-free multi-turn questions, and Workflow
	 * hands the message to a graph run configured on the workflow surface.
	 */
	import Icon from '#lib/ui/icons/Icon.svelte';
	import { cn } from '#lib/ui/cn';
	import type { IconName } from '#lib/ui/icons/paths';
	import { t } from '#lib/i18n';

	/** The executors the composer can hand a message to. */
	export type ExecutionType = 'agent' | 'workflow' | 'chat';

	interface ExecutionOption {
		id: ExecutionType;
		label: string;
		detail: string;
		icon: IconName;
	}

	// Labels resolve through getters so `t()` re-runs when the locale changes.
	const OPTIONS: ExecutionOption[] = [
		{
			id: 'agent',
			get label() {
				return t('chat.executionTypePicker.labelAgent');
			},
			get detail() {
				return t('chat.executionTypePicker.detailAgent');
			},
			icon: 'cpu',
		},
		{
			id: 'workflow',
			get label() {
				return t('chat.executionTypePicker.labelWorkflow');
			},
			get detail() {
				return t('chat.executionTypePicker.detailWorkflow');
			},
			icon: 'workflow',
		},
		{
			id: 'chat',
			get label() {
				return t('chat.executionTypePicker.labelChat');
			},
			get detail() {
				return t('chat.executionTypePicker.detailChat');
			},
			icon: 'sparkles',
		},
	];

	interface Props {
		value?: ExecutionType;
		class?: string;
		activePicker?: string | null;
		pickerKey?: string;
		onActivate?: (key: string | null) => void;
		onClose?: () => void;
	}

	let {
		value = $bindable('agent'),
		class: className = '',
		activePicker = undefined,
		pickerKey = 'execution',
		onActivate,
		onClose,
	}: Props = $props();

	let internalOpen = $state(false);
	const open = $derived(
		activePicker === undefined ? internalOpen : activePicker === pickerKey,
	);
	let root: HTMLDivElement | undefined = $state();

	const selected = $derived(
		OPTIONS.find((option) => option.id === value) ?? OPTIONS[0],
	);

	function choose(id: ExecutionType): void {
		value = id;
		close();
	}

	function openPanel(): void {
		if (activePicker === undefined) internalOpen = true;
		else onActivate?.(pickerKey);
	}

	function close(): void {
		if (activePicker === undefined) internalOpen = false;
		else onActivate?.(null);
		onClose?.();
	}

	function onwindowclick(event: MouseEvent): void {
		if (!open) return;
		const target = event.target as Node | null;
		if (target && root?.contains(target)) return;
		close();
	}

	function onkeydown(event: KeyboardEvent): void {
		if (!open) return;
		if (event.key === 'Escape') {
			event.stopPropagation();
			close();
		}
	}
</script>

<svelte:window onclick={onwindowclick} {onkeydown} />

<div bind:this={root} class={cn('relative', className)}>
	<button
		type="button"
		aria-haspopup="listbox"
		aria-expanded={open}
		aria-label={t('chat.executionTypePicker.ariaLabel')}
		onclick={() => (open ? close() : openPanel())}
		class="flex h-9 w-36 items-center gap-1.5 rounded-md border border-input bg-card px-2 text-caption text-foreground transition-colors hover:bg-accent focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-ring"
	>
		<Icon
			name={selected.icon}
			size={13}
			class="shrink-0 text-muted-foreground"
		/>
		<span class="min-w-0 flex-1 truncate text-left">{selected.label}</span>
		<Icon
			name="chevrons-up-down"
			size={12}
			class="shrink-0 text-muted-foreground"
		/>
	</button>

	{#if open}
		<div
			role="listbox"
			tabindex="-1"
			class="animate-panel-in absolute bottom-full left-0 z-40 mb-1 w-72 overflow-hidden rounded-lg border border-border bg-popover p-1 text-body shadow-popover"
		>
			{#each OPTIONS as option (option.id)}
				<button
					type="button"
					role="option"
					aria-selected={value === option.id}
					onclick={() => choose(option.id)}
					class={cn(
						'flex w-full flex-col items-start gap-0.5 rounded-md px-2 py-1.5 text-left transition-colors hover:bg-accent',
						value === option.id && 'bg-accent',
					)}
				>
					<span class="flex w-full items-center gap-2">
						<Icon
							name={option.icon}
							size={13}
							class="shrink-0 text-muted-foreground"
						/>
						<span class="min-w-0 flex-1 truncate text-caption text-foreground">
							{option.label}
						</span>
					</span>
					<span class="w-full truncate text-micro text-muted-foreground">
						{option.detail}
					</span>
				</button>
			{/each}
		</div>
	{/if}
</div>
