<script lang="ts">
	/**
	 * Shared picker for the composer instances (agents, models). The list is
	 * layered: pinned entries first, then recently used ones, with the full
	 * list behind a browse-all view so the trigger never grows long.
	 */
	import Icon from '@wf-agent/ui/icons/Icon.svelte';
	import { cn } from '@wf-agent/ui/cn';
	import type { IconName } from '@wf-agent/ui/icons/paths';
	import { t } from '#lib/i18n';

	/** One selectable entry. */
	export interface PickerOption {
		/** Stable id of the entry. */
		id: string;
		/** Primary label. */
		label: string;
		/** Secondary line under the label. */
		detail?: string;
		/** Short marker on the label line. */
		badge?: string;
	}

	interface Props {
		value?: string;
		/** Extra classes of the picker root. */
		class?: string;
		/** Every entry the picker can select. */
		options: PickerOption[];
		/** Ids of the pinned entries, most recently pinned first. */
		pinnedIds?: string[];
		/** Ids of the recently used entries, most recent first. */
		recentIds?: string[];
		/** Whether a value outside the list may be typed in. */
		allowFreeform?: boolean;
		/** Accessible name of the trigger. */
		ariaLabel: string;
		/** Text of the trigger while nothing is selected. */
		placeholder: string;
		/** Placeholder of the filter box. */
		filterPlaceholder: string;
		/** Text shown when the list has no entry. */
		emptyLabel: string;
		/** Icon of the trigger. */
		icon?: IconName;
		/** Width class of the trigger. */
		triggerClass?: string;
		activePicker?: string | null;
		pickerKey?: string;
		onActivate?: (key: string | null) => void;
		onClose?: () => void;
		/** Called when the user pins or unpins an entry. */
		onPin?: (id: string, pinned: boolean) => void;
	}

	let {
		value = $bindable(''),
		class: className = '',
		options,
		pinnedIds = [],
		recentIds = [],
		allowFreeform = false,
		ariaLabel,
		placeholder,
		filterPlaceholder,
		emptyLabel,
		icon = 'cpu',
		triggerClass = 'w-40',
		activePicker = undefined,
		pickerKey = 'instance',
		onActivate,
		onClose,
		onPin,
	}: Props = $props();

	let internalOpen = $state(false);
	const open = $derived(
		activePicker === undefined ? internalOpen : activePicker === pickerKey,
	);
	let query = $state('');
	/** Whether the full list is shown instead of the pinned/recent layers. */
	let browseAll = $state(false);
	let root: HTMLDivElement | undefined = $state();

	const filtered = $derived.by(() => {
		const term = query.trim().toLowerCase();
		if (!term) return options;
		return options.filter((option) =>
			[option.label, option.id, option.detail ?? '']
				.join(' ')
				.toLowerCase()
				.includes(term),
		);
	});

	/** The pinned layer, in pin order. */
	const pinned = $derived(
		pinnedIds
			.map((id) => filtered.find((option) => option.id === id))
			.filter((option): option is PickerOption => option !== undefined),
	);

	/** The recently used layer, in recency order and without pinned entries. */
	const recent = $derived(
		recentIds
			.map((id) => filtered.find((option) => option.id === id))
			.filter(
				(option): option is PickerOption =>
					option !== undefined && !pinnedIds.includes(option.id),
			),
	);

	const typed = $derived(query.trim());

	/** The selected entry, so the trigger can show its label. */
	const selected = $derived(options.find((option) => option.id === value));

	function isPinned(id: string): boolean {
		return pinnedIds.includes(id);
	}

	function choose(id: string): void {
		value = id;
		close();
	}

	function openPanel(): void {
		query = '';
		browseAll = false;
		if (activePicker === undefined) internalOpen = true;
		else onActivate?.(pickerKey);
	}

	function close(): void {
		query = '';
		browseAll = false;
		if (activePicker === undefined) internalOpen = false;
		else onActivate?.(null);
		onClose?.();
	}

	async function togglePin(event: MouseEvent, id: string): Promise<void> {
		event.stopPropagation();
		await onPin?.(id, !isPinned(id));
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
		} else if (event.key === 'Enter' && typed && allowFreeform) {
			event.preventDefault();
			choose(typed);
		}
	}
</script>

<svelte:window onclick={onwindowclick} {onkeydown} />

{#snippet row(option: PickerOption)}
	<div
		class={cn(
			'flex w-full items-center gap-1 rounded-md px-1 transition-colors hover:bg-accent',
			value === option.id && 'bg-accent',
		)}
	>
		<button
			type="button"
			role="option"
			aria-selected={value === option.id}
			onclick={() => choose(option.id)}
			class="flex min-w-0 flex-1 flex-col items-start gap-0.5 rounded-md px-1 py-1.5 text-left"
		>
			<span class="flex w-full items-center gap-2">
				<span class="min-w-0 flex-1 truncate text-caption text-foreground">
					{option.label}
				</span>
				{#if option.badge}
					<span class="shrink-0 font-mono text-micro text-info">
						{option.badge}
					</span>
				{/if}
			</span>
			{#if option.detail}
				<span
					class="w-full truncate font-mono text-micro text-muted-foreground"
				>
					{option.detail}
				</span>
			{/if}
		</button>
		{#if onPin}
			<button
				type="button"
				aria-label={isPinned(option.id)
					? t('chat.instancePicker.unpin', { name: option.label })
					: t('chat.instancePicker.pin', { name: option.label })}
				aria-pressed={isPinned(option.id)}
				onclick={(event) => void togglePin(event, option.id)}
				class="shrink-0 rounded p-1 text-muted-foreground transition-colors hover:text-foreground"
			>
				<Icon name="star" size={13} />
			</button>
		{/if}
	</div>
{/snippet}

<div bind:this={root} class={cn('relative', triggerClass, className)}>
	<button
		type="button"
		aria-haspopup="listbox"
		aria-expanded={open}
		aria-label={ariaLabel}
		onclick={() => (open ? close() : openPanel())}
		class="flex h-9 w-full items-center gap-1.5 rounded-md border border-input bg-card px-2 text-caption text-foreground transition-colors hover:bg-accent focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-ring"
	>
		<Icon name={icon} size={13} class="shrink-0 text-muted-foreground" />
		<span
			class={cn(
				'min-w-0 flex-1 truncate text-left',
				!value && 'text-muted-foreground',
			)}
		>
			{selected?.label ?? (value || placeholder)}
		</span>
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
			<div class="mb-1 flex items-center gap-1">
				<input
					bind:value={query}
					placeholder={filterPlaceholder}
					aria-label={filterPlaceholder}
					class="min-w-0 flex-1 rounded-md border border-input bg-card px-2 py-1 text-caption text-foreground placeholder:text-muted-foreground focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-ring"
				/>
				<button
					type="button"
					aria-pressed={browseAll}
					onclick={() => (browseAll = !browseAll)}
					class={cn(
						'shrink-0 rounded-md border border-input px-2 py-1 text-micro text-muted-foreground transition-colors hover:bg-accent hover:text-foreground',
						browseAll && 'border-ring text-foreground',
					)}
				>
					{browseAll
						? t('chat.instancePicker.shortList')
						: t('chat.instancePicker.all', { count: options.length })}
				</button>
			</div>
			<div class="max-h-64 overflow-y-auto">
				{#if browseAll}
					{#each filtered as option (option.id)}
						{@render row(option)}
					{/each}
					{#if filtered.length === 0}
						<p class="px-2 py-1.5 text-micro text-muted-foreground">
							{emptyLabel}
						</p>
					{/if}
				{:else}
					{#if pinned.length > 0}
						<p
							class="px-2 py-1 text-micro uppercase tracking-wide text-muted-foreground"
						>
							{t('chat.instancePicker.pinned')}
						</p>
						{#each pinned as option (option.id)}
							{@render row(option)}
						{/each}
					{/if}
					{#if recent.length > 0}
						<p
							class="px-2 py-1 text-micro uppercase tracking-wide text-muted-foreground"
						>
							{t('chat.instancePicker.recent')}
						</p>
						{#each recent as option (option.id)}
							{@render row(option)}
						{/each}
					{/if}
					{#if pinned.length === 0 && recent.length === 0}
						<p class="px-2 py-1.5 text-micro text-muted-foreground">
							{emptyLabel}
						</p>
					{/if}
				{/if}
				{#if allowFreeform && typed && !options.some((option) => option.id === typed)}
					<button
						type="button"
						role="option"
						aria-selected="false"
						onclick={() => choose(typed)}
						class="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left transition-colors hover:bg-accent"
					>
						<Icon name="pencil" size={13} class="text-muted-foreground" />
						<span
							class="min-w-0 flex-1 truncate font-mono text-micro text-foreground"
						>
							{t('chat.instancePicker.useValue', { value: typed })}
						</span>
					</button>
				{/if}
			</div>
		</div>
	{/if}
</div>
