<script lang="ts">
	import Icon from '$lib/components/icons/Icon.svelte';
	import SessionRow from '$lib/components/domain/SessionRow.svelte';
	import { sessions } from '$lib/stores/sessions.svelte';
	import { ui } from '$lib/stores/ui.svelte';
	import { cn } from '$lib/utils/cn';

	interface Props {
		selectedId?: string | null;
		class?: string;
	}

	let { selectedId = null, class: className = '' }: Props = $props();

	const GROUP_TONE =
		'px-2 pb-1 pt-3 text-micro uppercase tracking-wide text-muted-foreground';

	function openSession(id: string): void {
		sessions.open(id);
		ui.closeMobileNav();
	}

	function startDraft(): void {
		sessions.startDraft();
		ui.closeMobileNav();
	}
</script>

<div class={cn('flex min-h-0 flex-col', className)}>
	<div class="flex shrink-0 items-center gap-1.5 px-2 pt-2">
		<button
			type="button"
			onclick={startDraft}
			class="flex h-8 min-w-0 flex-1 items-center gap-2 rounded-md border border-sidebar-border bg-card px-2 text-body text-sidebar-foreground transition-colors hover:bg-accent hover:text-foreground"
		>
			<Icon name="plus" size={14} />
			New chat
		</button>
		<button
			type="button"
			aria-label="Search everything"
			onclick={() => ui.toggleCommand()}
			class="flex h-8 w-8 shrink-0 items-center justify-center rounded-md border border-sidebar-border text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
		>
			<Icon name="search" size={14} />
		</button>
	</div>

	<div class="min-h-0 flex-1 overflow-y-auto pb-3">
		{#if sessions.starred.length > 0}
			<p class={GROUP_TONE}>Starred</p>
			<ul class="space-y-0.5">
				{#each sessions.starred as session (session.id)}
					<SessionRow
						{session}
						label={sessions.label(session.id)}
						selected={selectedId === session.id}
						starred
						draft={sessions.draftIds.includes(session.id)}
						onselect={openSession}
						onstar={(id) => void sessions.toggleStar(id)}
					/>
				{/each}
			</ul>
		{/if}

		{#if sessions.recent.length > 0}
			<p class={GROUP_TONE}>Recent</p>
			<ul class="space-y-0.5">
				{#each sessions.recent as session (session.id)}
					<SessionRow
						{session}
						label={sessions.label(session.id)}
						selected={selectedId === session.id}
						starred={sessions.isStarred(session.id)}
						draft={sessions.draftIds.includes(session.id)}
						onselect={openSession}
						onstar={(id) => void sessions.toggleStar(id)}
					/>
				{/each}
			</ul>
		{/if}

		{#if sessions.drafts.length > 0}
			<p class={GROUP_TONE}>Drafts</p>
			<ul class="space-y-0.5">
				{#each sessions.drafts as session (session.id)}
					<SessionRow
						{session}
						label={sessions.label(session.id)}
						selected={selectedId === session.id}
						starred={sessions.isStarred(session.id)}
						draft
						onselect={openSession}
						onstar={(id) => void sessions.toggleStar(id)}
					/>
				{/each}
			</ul>
		{/if}

		<p class={GROUP_TONE}>All</p>
		{#if sessions.list.loading && sessions.sessions.length === 0}
			<p class="px-2 py-4 text-center text-caption text-muted-foreground">
				Loading sessions…
			</p>
		{:else if sessions.list.error}
			<p
				class="mx-2 mt-1 rounded-md border border-destructive/40 bg-destructive/10 px-2 py-1.5 text-micro text-destructive"
			>
				{sessions.list.error}
			</p>
		{:else if sessions.sessions.length === 0}
			<p class="px-2 py-4 text-center text-caption text-muted-foreground">
				No sessions yet. Start a new one.
			</p>
		{:else}
			<ul class="space-y-0.5">
				{#each sessions.sessions as session (session.id)}
					<SessionRow
						{session}
						label={sessions.label(session.id)}
						selected={selectedId === session.id}
						starred={sessions.isStarred(session.id)}
						draft={sessions.draftIds.includes(session.id)}
						onselect={openSession}
						onstar={(id) => void sessions.toggleStar(id)}
					/>
				{/each}
			</ul>
			{#if sessions.list.hasMore}
				<button
					type="button"
					disabled={sessions.list.loading}
					onclick={() => void sessions.list.loadMore()}
					class="mt-1 flex w-full items-center justify-center gap-1.5 rounded-md px-2 py-1.5 text-caption text-muted-foreground transition-colors hover:bg-accent/60 hover:text-foreground disabled:opacity-60"
				>
					<Icon
						name="loader"
						size={12}
						class={sessions.list.loading ? 'animate-spin' : ''}
					/>
					Load more
				</button>
			{/if}
		{/if}
	</div>
</div>
