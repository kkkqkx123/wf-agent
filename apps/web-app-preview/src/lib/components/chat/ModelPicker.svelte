<script lang="ts">
	/**
	 * Model selector for the composer. Lists the registered model profiles for
	 * one-click selection and keeps free-form entry as a fallback for models the
	 * profile list does not cover: the filter box doubles as a manual value, and
	 * Enter commits whatever is typed.
	 */
	import Icon from '@wf-agent/ui/icons/Icon.svelte';
	import { cn } from '@wf-agent/ui/cn';
	import { listModelProfiles } from '$lib/services/resources';
	import type { ModelProfile } from '$lib/types/models';

	interface Props {
		value?: string;
		class?: string;
	}

	let { value = $bindable(''), class: className = '' }: Props = $props();

	let open = $state(false);
	let query = $state('');
	let profiles = $state<ModelProfile[]>([]);
	let root: HTMLDivElement | undefined = $state();

	$effect(() => {
		void listModelProfiles()
			.then((items) => {
				profiles = items;
			})
			.catch(() => {
				profiles = [];
			});
	});

	const filtered = $derived.by(() => {
		const term = query.trim().toLowerCase();
		if (!term) return profiles;
		return profiles.filter((profile) =>
			[profile.name, profile.model, profile.provider]
				.join(' ')
				.toLowerCase()
				.includes(term),
		);
	});

	const typed = $derived(query.trim());

	function choose(model: string): void {
		value = model;
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
		aria-label="Model"
		onclick={() => (open ? close() : openPanel())}
		class="flex h-9 w-44 items-center gap-1.5 rounded-md border border-input bg-card px-2 text-caption text-foreground transition-colors hover:bg-accent focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-ring"
	>
		<Icon name="cpu" size={13} class="shrink-0 text-muted-foreground" />
		<span
			class={cn(
				'min-w-0 flex-1 truncate text-left',
				!value && 'text-muted-foreground',
			)}
		>
			{value || 'Select model'}
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
				placeholder="Filter or type a model id"
				aria-label="Filter models"
				class="mb-1 w-full rounded-md border border-input bg-card px-2 py-1 text-caption text-foreground placeholder:text-muted-foreground focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-ring"
			/>
			<div class="max-h-64 overflow-y-auto">
				{#each filtered as profile (profile.id || profile.model)}
					<button
						type="button"
						role="option"
						aria-selected={value === profile.model}
						onclick={() => choose(profile.model)}
						class={cn(
							'flex w-full flex-col items-start gap-0.5 rounded-md px-2 py-1.5 text-left transition-colors hover:bg-accent',
							value === profile.model && 'bg-accent',
						)}
					>
						<span class="flex w-full items-center gap-2">
							<span
								class="min-w-0 flex-1 truncate text-caption text-foreground"
							>
								{profile.name || profile.model}
							</span>
							{#if profile.isDefault}
								<span class="shrink-0 font-mono text-micro text-info"
									>default</span
								>
							{/if}
						</span>
						<span
							class="w-full truncate font-mono text-micro text-muted-foreground"
						>
							{profile.model}{profile.provider ? ` · ${profile.provider}` : ''}
						</span>
					</button>
				{/each}
				{#if typed && !filtered.some((profile) => profile.model === typed)}
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
						No model profiles registered; type a model id to use one directly.
					</p>
				{/if}
			</div>
		</div>
	{/if}
</div>
