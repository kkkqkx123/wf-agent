<script lang="ts">
	/**
	 * Workflow selector for the composer. Lists runnable workflow templates as
	 * optional conversation context; "none" clears the selection. Workflow
	 * context is always an explicit choice, so there is no free-form id entry.
	 */
	import Icon from '@wf-agent/ui/icons/Icon.svelte';
	import { cn } from '@wf-agent/ui/cn';
	import { isRunnableTemplate, listTemplates } from '$lib/services/templates';
	import type { Template } from '$lib/types/models';

	interface Props {
		value?: string;
		class?: string;
	}

	let { value = $bindable(''), class: className = '' }: Props = $props();

	let open = $state(false);
	let query = $state('');
	let templates = $state<Template[]>([]);
	let root: HTMLDivElement | undefined = $state();
	let loadError = $state(false);

	$effect(() => {
		void listTemplates({ kind: 'workflow' })
			.then((items) => {
				templates = items.filter(isRunnableTemplate);
				loadError = false;
			})
			.catch(() => {
				templates = [];
				loadError = true;
			});
	});

	const filtered = $derived.by(() => {
		const term = query.trim().toLowerCase();
		if (!term) return templates;
		return templates.filter((template) =>
			[template.name, template.id, template.description, template.category]
				.join(' ')
				.toLowerCase()
				.includes(term),
		);
	});

	/** Trigger label: selected template name, raw id fallback, or none. */
	const selectedLabel = $derived(
		value === ''
			? ''
			: (templates.find((template) => template.id === value)?.name ?? value),
	);

	function choose(workflowId: string): void {
		value = workflowId;
		close();
	}

	function openPanel(): void {
		open = true;
		query = '';
	}

	function close(): void {
		open = false;
		query = '';
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
		aria-label="Workflow context"
		onclick={() => (open ? close() : openPanel())}
		class="flex h-9 w-40 items-center gap-1.5 rounded-md border border-input bg-card px-2 text-caption text-foreground transition-colors hover:bg-accent focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-ring"
	>
		<Icon name="workflow" size={13} class="shrink-0 text-muted-foreground" />
		<span
			class={cn(
				'min-w-0 flex-1 truncate text-left',
				!value && 'text-muted-foreground',
			)}
		>
			{value ? selectedLabel : 'No workflow'}
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
			<input
				bind:value={query}
				placeholder="Filter workflows"
				aria-label="Filter workflows"
				class="mb-1 w-full rounded-md border border-input bg-card px-2 py-1 text-caption text-foreground placeholder:text-muted-foreground focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-ring"
			/>
			<div class="max-h-64 overflow-y-auto">
				<button
					type="button"
					role="option"
					aria-selected={value === ''}
					onclick={() => choose('')}
					class={cn(
						'flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left transition-colors hover:bg-accent',
						value === '' && 'bg-accent',
					)}
				>
					<Icon name="x" size={13} class="shrink-0 text-muted-foreground" />
					<span class="min-w-0 flex-1 truncate text-caption text-muted-foreground">
						No workflow context
					</span>
				</button>
				{#each filtered as template (template.id)}
					<button
						type="button"
						role="option"
						aria-selected={value === template.id}
						onclick={() => choose(template.id)}
						class={cn(
							'flex w-full flex-col items-start gap-0.5 rounded-md px-2 py-1.5 text-left transition-colors hover:bg-accent',
							value === template.id && 'bg-accent',
						)}
					>
						<span class="min-w-0 flex-1 truncate text-caption text-foreground">
							{template.name || template.id}
						</span>
						<span
							class="w-full truncate font-mono text-micro text-muted-foreground"
						>
							{template.id}{template.category ? ` · ${template.category}` : ''}
						</span>
					</button>
				{/each}
				{#if filtered.length === 0}
					<p class="px-2 py-1.5 text-micro text-muted-foreground">
						{loadError
							? 'Workflow templates could not be loaded.'
							: 'No runnable workflow templates registered.'}
					</p>
				{/if}
			</div>
		</div>
	{/if}
</div>
