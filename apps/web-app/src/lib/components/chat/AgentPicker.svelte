<script lang="ts">
	/**
	 * Agent selector for the composer. Lists runnable agent templates for
	 * one-click selection and keeps free-form entry as a fallback.
	 */
	import Icon from '@wf-agent/ui/icons/Icon.svelte';
	import { cn } from '@wf-agent/ui/cn';
	import { isRunnableTemplate, listTemplates, DEFAULT_AGENT_ID } from '$lib/services/templates';
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

	$effect(() => {
		void listTemplates({ kind: 'agent' })
			.then((items) => {
				templates = items.filter(isRunnableTemplate);
			})
			.catch(() => {
				templates = [];
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

	const typed = $derived(query.trim());

	function choose(agentId: string): void {
		value = agentId;
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
		} else if (event.key === 'Enter' && typed) {
			event.preventDefault();
			choose(typed);
		}
	}
</script>

<svelte:window onclick={onwindowclick} {onkeydown} />

<div bind:this={root} class={cn('relative', className)}>
	<button
		type="button"
		aria-haspopup="listbox"
		aria-expanded={open}
		aria-label="Agent"
		onclick={() => (open ? close() : openPanel())}
		class="flex h-9 w-40 items-center gap-1.5 rounded-md border border-input bg-card px-2 text-caption text-foreground transition-colors hover:bg-accent focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-ring"
	>
		<Icon name="cpu" size={13} class="shrink-0 text-muted-foreground" />
		<span
			class={cn(
				'min-w-0 flex-1 truncate text-left',
				!value && 'text-muted-foreground',
			)}
		>
			{value || 'Select agent'}
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
				placeholder="Filter or type an agent id"
				aria-label="Filter agents"
				class="mb-1 w-full rounded-md border border-input bg-card px-2 py-1 text-caption text-foreground placeholder:text-muted-foreground focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-ring"
			/>
			<div class="max-h-64 overflow-y-auto">
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
						<span class="flex w-full items-center gap-2">
							<span
								class="min-w-0 flex-1 truncate text-caption text-foreground"
							>
								{template.name || template.id}
							</span>
							{#if template.id === DEFAULT_AGENT_ID}
								<span class="shrink-0 font-mono text-micro text-info"
									>default</span
								>
							{/if}
						</span>
						<span
							class="w-full truncate font-mono text-micro text-muted-foreground"
						>
							{template.id}{template.category ? ` · ${template.category}` : ''}
						</span>
					</button>
				{/each}
				{#if typed && !filtered.some((template) => template.id === typed)}
					<button
						type="button"
						role="option"
						aria-selected="false"
						onclick={() => choose(typed)}
						class="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left transition-colors hover:bg-accent"
					>
						<Icon
							name="pencil"
							size={13}
							class="shrink-0 text-muted-foreground"
						/>
						<span
							class="min-w-0 flex-1 truncate font-mono text-caption text-foreground"
						>
							Use “{typed}”
						</span>
					</button>
				{/if}
				{#if !typed && filtered.length === 0}
					<p class="px-2 py-1.5 text-micro text-muted-foreground">
						No runnable agents registered; type an agent id to use one directly.
					</p>
				{/if}
			</div>
		</div>
	{/if}
</div>