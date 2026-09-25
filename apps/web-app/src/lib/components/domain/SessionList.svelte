<script lang="ts">
	import Icon from '$lib/components/icons/Icon.svelte';
	import IconButton from '$lib/components/ui/IconButton.svelte';
	import StatusBadge from '$lib/components/domain/StatusBadge.svelte';
	import type { AgentLoop } from '$lib/types/models';
	import { formatRelativeTime } from '$lib/utils/format';
	import { cn } from '$lib/utils/cn';

	interface Props {
		sessions: AgentLoop[];
		selectedId?: string | null;
		loading?: boolean;
		query?: string;
		class?: string;
		onselect?: (id: string) => void;
		onnew?: () => void;
		onstar?: (id: string, starred: boolean) => void;
	}

	let {
		sessions,
		selectedId = null,
		loading = false,
		query = $bindable(''),
		class: className = '',
		onselect,
		onnew,
		onstar,
	}: Props = $props();

	let starredOnly = $state(false);

	const visible = $derived(
		sessions
			.filter((session) => {
				if (starredOnly && !session.starred) return false;
				const needle = query.trim().toLowerCase();
				if (!needle) return true;
				return (
					session.name.toLowerCase().includes(needle) ||
					session.id.toLowerCase().includes(needle) ||
					session.model.toLowerCase().includes(needle)
				);
			})
			.slice()
			.sort((a, b) => (a.updatedAt < b.updatedAt ? 1 : -1)),
	);
</script>

<section class={cn('flex min-h-0 flex-col', className)} aria-label="Sessions">
	<div class="flex shrink-0 items-center gap-1.5 px-2 pt-2">
		<div class="relative min-w-0 flex-1">
			<Icon
				name="search"
				size={13}
				class="pointer-events-none absolute left-2 top-1/2 -translate-y-1/2 text-muted-foreground"
			/>
			<input
				bind:value={query}
				placeholder="Search sessions…"
				aria-label="Search sessions"
				class="h-8 w-full rounded-md border border-input bg-card pl-7 pr-2 text-caption text-foreground placeholder:text-muted-foreground focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-ring"
			/>
		</div>
		<IconButton
			icon="star"
			label="Toggle starred only"
			active={starredOnly}
			onclick={() => (starredOnly = !starredOnly)}
		/>
		<IconButton icon="plus" label="New session" onclick={() => onnew?.()} />
	</div>

	<div class="min-h-0 flex-1 overflow-y-auto p-2">
		{#if loading && sessions.length === 0}
			<p class="px-2 py-6 text-center text-caption text-muted-foreground">
				Loading sessions…
			</p>
		{:else if visible.length === 0}
			<p class="px-2 py-6 text-center text-caption text-muted-foreground">
				{starredOnly || query.trim()
					? 'No sessions match this filter.'
					: 'No sessions yet. Start a new one.'}
			</p>
		{:else}
			<ul class="space-y-0.5">
				{#each visible as session (session.id)}
					<li
						class={cn(
							'flex items-center gap-0.5 rounded-md transition-colors',
							selectedId === session.id
								? 'bg-accent text-accent-foreground'
								: 'hover:bg-accent/60',
						)}
					>
						<button
							type="button"
							aria-label={session.starred ? 'Unstar session' : 'Star session'}
							aria-pressed={session.starred}
							class="shrink-0 rounded p-1.5 transition-colors hover:bg-accent"
							onclick={() => onstar?.(session.id, !session.starred)}
						>
							<Icon
								name="star"
								size={12}
								class={session.starred
									? 'text-warning'
									: 'text-muted-foreground/50'}
							/>
						</button>
						<button
							type="button"
							onclick={() => onselect?.(session.id)}
							aria-current={selectedId === session.id}
							class="flex min-w-0 flex-1 items-center gap-2 rounded-md px-1 py-1.5 text-left"
						>
							<span class="min-w-0 flex-1">
								<span class="block truncate text-body font-medium">
									{session.name || session.id}
								</span>
								<span
									class="mt-0.5 block truncate text-micro text-muted-foreground"
								>
									{session.model} · {formatRelativeTime(session.updatedAt)}
								</span>
							</span>
							<StatusBadge status={session.status} size="sm" dot={false} />
						</button>
					</li>
				{/each}
			</ul>
		{/if}
	</div>
</section>
