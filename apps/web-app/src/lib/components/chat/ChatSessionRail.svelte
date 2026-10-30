<script lang="ts">
	import { onMount } from 'svelte';
	import Icon from '#lib/ui/icons/Icon.svelte';
	import IconButton from '#lib/ui/components/IconButton.svelte';
	import { sessions } from '#lib/stores/sessions.svelte';
	import { cn } from '#lib/ui/cn';
	import { t } from '#lib/i18n';
	import type { ChatSessionController } from '#lib/chat/chat-session.svelte';

	const WIDTH_KEY = 'wf.chat.sessionRail.width';
	const COLLAPSED_KEY = 'wf.chat.sessionRail.collapsed';
	const DEFAULT_WIDTH = 264;
	const MIN_WIDTH = 200;
	const MAX_WIDTH = 360;

	interface Props {
		controller: ChatSessionController;
	}

	let { controller }: Props = $props();

	const selectedId = $derived(controller.selectedId);
	const all = $derived(sessions.sessions);
	const starred = $derived(sessions.starred);
	const recent = $derived(sessions.recent.filter((item) => !sessions.isStarred(item.id)));

	let query = $state('');
	let width = $state(DEFAULT_WIDTH);
	let collapsed = $state(false);
	let searchBox: HTMLInputElement | null = $state(null);

	onMount(() => {
		try {
			const raw = localStorage.getItem(WIDTH_KEY);
			const parsed = raw === null ? NaN : Number.parseInt(raw, 10);
			if (Number.isFinite(parsed)) width = Math.min(MAX_WIDTH, Math.max(MIN_WIDTH, parsed));
			collapsed = localStorage.getItem(COLLAPSED_KEY) === '1';
		} catch {
			// Storage may be unavailable; defaults still apply.
		}
		void sessions.refresh();
	});

	function persistWidth(next: number): void {
		width = Math.min(MAX_WIDTH, Math.max(MIN_WIDTH, Math.round(next)));
		try {
			localStorage.setItem(WIDTH_KEY, String(width));
		} catch {
			// Storage may be unavailable; the in-memory width still applies.
		}
	}

	function persistCollapsed(next: boolean): void {
		collapsed = next;
		try {
			if (next) localStorage.setItem(COLLAPSED_KEY, '1');
			else localStorage.removeItem(COLLAPSED_KEY);
		} catch {
			// Storage may be unavailable; the in-memory state still applies.
		}
	}

	function resetWidth(): void {
		persistWidth(DEFAULT_WIDTH);
	}

	function startResize(event: PointerEvent): void {
		const handle = event.currentTarget as HTMLElement | null;
		if (!handle) return;
		event.preventDefault();
		handle.setPointerCapture(event.pointerId);
		const onMove = (moveEvent: PointerEvent): void => {
			const rect = (handle.parentElement as HTMLElement | null)?.getBoundingClientRect();
			if (rect) persistWidth(moveEvent.clientX - rect.left);
		};
		const onUp = (upEvent: PointerEvent): void => {
			handle.releasePointerCapture(upEvent.pointerId);
			handle.removeEventListener('pointermove', onMove);
			handle.removeEventListener('pointerup', onUp);
			handle.removeEventListener('pointercancel', onUp);
		};
		handle.addEventListener('pointermove', onMove);
		handle.addEventListener('pointerup', onUp);
		handle.addEventListener('pointercancel', onUp);
	}

	const filtered = $derived.by(() => {
		const term = query.trim().toLowerCase();
		if (!term) return null;
		return all.filter((item) =>
			[sessions.label(item.id), item.id].join(' ').toLowerCase().includes(term),
		);
	});
</script>

{#if collapsed}
	<div
		class="hidden w-10 shrink-0 flex-col items-center gap-1 border-r border-border bg-card py-2 lg:flex"
	>
		<IconButton
			icon="plus"
			label={t('chat.sessionRail.newSession')}
			onclick={() => sessions.startDraft()}
		/>
		<IconButton
			icon="panel-left"
			label={t('chat.sessionRail.expand')}
			onclick={() => persistCollapsed(false)}
		/>
	</div>
{:else}
	<aside
		class="relative hidden min-h-0 shrink-0 flex-col border-r border-border bg-card lg:flex"
		style:width={`${width}px`}
	>
		<div class="flex h-12 shrink-0 items-center gap-1 border-b border-border px-2">
			<button
				type="button"
				onclick={() => sessions.startDraft()}
				class="flex min-w-0 flex-1 items-center gap-1.5 rounded-md px-2 py-1.5 text-body font-medium transition-colors hover:bg-accent"
			>
				<Icon name="plus" size={14} class="shrink-0" />
				<span class="truncate">{t('chat.sessionRail.newSession')}</span>
			</button>
			<IconButton
				icon="refresh"
				label={t('chat.sessionRail.refresh')}
				compact
				onclick={() => void sessions.refresh()}
			/>
			<IconButton
				icon="panel-left"
				label={t('chat.sessionRail.collapse')}
				compact
				onclick={() => persistCollapsed(true)}
			/>
		</div>
		<div class="shrink-0 border-b border-border p-2">
			<div class="flex items-center gap-1.5 rounded-md border border-input bg-card px-2">
				<Icon name="search" size={13} class="shrink-0 text-muted-foreground" />
				<input
					bind:this={searchBox}
					bind:value={query}
					placeholder={t('chat.sessionRail.searchPlaceholder')}
					aria-label={t('chat.sessionRail.searchPlaceholder')}
					class="min-w-0 flex-1 bg-transparent py-1.5 text-body placeholder:text-muted-foreground focus:outline-none"
				/>
				{#if query}
					<button
						type="button"
						aria-label={t('chat.sessionRail.searchPlaceholder')}
						class="shrink-0 text-muted-foreground hover:text-foreground"
						onclick={() => {
							query = '';
							searchBox?.focus();
						}}
					>
						<Icon name="x" size={12} />
					</button>
				{/if}
			</div>
		</div>
		<div class="min-h-0 flex-1 overflow-y-auto p-2">
			{#if filtered !== null}
				{#if filtered.length === 0}
					<p class="px-2 py-4 text-center text-caption text-muted-foreground">
						{t('chat.sessionRail.empty')}
					</p>
				{:else}
					<ul class="space-y-0.5">
						{#each filtered as item (item.id)}
							<li>
								<button
									type="button"
									aria-current={selectedId === item.id ? 'page' : undefined}
									aria-label={t('chat.sessionRail.openSession', {
										name: sessions.label(item.id),
									})}
									onclick={() => void sessions.open(item.id)}
									class={cn(
										'flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-body transition-colors',
										selectedId === item.id ? 'bg-accent font-medium' : 'hover:bg-accent/60',
									)}
								>
									<span class="min-w-0 flex-1 truncate">
										{sessions.label(item.id)}
									</span>
								</button>
							</li>
						{/each}
					</ul>
				{/if}
			{:else}
				{#if starred.length > 0}
					<p
						class="px-2 pt-1 pb-1 text-micro font-semibold uppercase tracking-wide text-muted-foreground"
					>
						{t('chat.sessionRail.starred')}
					</p>
					<ul class="mb-2 space-y-0.5">
						{#each starred as item (item.id)}
							<li>
								<button
									type="button"
									aria-current={selectedId === item.id ? 'page' : undefined}
									aria-label={t('chat.sessionRail.openSession', {
										name: sessions.label(item.id),
									})}
									onclick={() => void sessions.open(item.id)}
									class={cn(
										'flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-body transition-colors',
										selectedId === item.id ? 'bg-accent font-medium' : 'hover:bg-accent/60',
									)}
								>
									<span class="min-w-0 flex-1 truncate">
										{sessions.label(item.id)}
									</span>
									<Icon name="star" size={12} class="shrink-0 text-warning" />
								</button>
							</li>
						{/each}
					</ul>
				{/if}
				{#if recent.length > 0}
					<p
						class="px-2 pt-1 pb-1 text-micro font-semibold uppercase tracking-wide text-muted-foreground"
					>
						{t('chat.sessionRail.recent')}
					</p>
					<ul class="mb-2 space-y-0.5">
						{#each recent as item (item.id)}
							<li>
								<button
									type="button"
									aria-current={selectedId === item.id ? 'page' : undefined}
									aria-label={t('chat.sessionRail.openSession', {
										name: sessions.label(item.id),
									})}
									onclick={() => void sessions.open(item.id)}
									class={cn(
										'flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-body transition-colors',
										selectedId === item.id ? 'bg-accent font-medium' : 'hover:bg-accent/60',
									)}
								>
									<span class="min-w-0 flex-1 truncate">
										{sessions.label(item.id)}
									</span>
								</button>
							</li>
						{/each}
					</ul>
				{/if}
				<p
					class="px-2 pt-1 pb-1 text-micro font-semibold uppercase tracking-wide text-muted-foreground"
				>
					{t('chat.sessionRail.allSessions')}
				</p>
				{#if all.length === 0}
					<p class="px-2 py-4 text-center text-caption text-muted-foreground">
						{t('chat.sessionRail.empty')}
					</p>
				{:else}
					<ul class="space-y-0.5">
						{#each all as item (item.id)}
							<li>
								<button
									type="button"
									aria-current={selectedId === item.id ? 'page' : undefined}
									aria-label={t('chat.sessionRail.openSession', {
										name: sessions.label(item.id),
									})}
									onclick={() => void sessions.open(item.id)}
									class={cn(
										'flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-body transition-colors',
										selectedId === item.id ? 'bg-accent font-medium' : 'hover:bg-accent/60',
									)}
								>
									<span class="min-w-0 flex-1 truncate">
										{sessions.label(item.id)}
									</span>
								</button>
							</li>
						{/each}
					</ul>
					{#if sessions.list.hasMore}
						<button
							type="button"
							onclick={() => void sessions.list.loadMore()}
							disabled={sessions.list.loading}
							class="mt-1 w-full rounded-md px-2 py-1.5 text-center text-micro text-info hover:underline disabled:opacity-50"
						>
							{sessions.list.loading ? t('common.loading') : t('chat.sessionRail.loadMore')}
						</button>
					{/if}
				{/if}
			{/if}
		</div>
		<button
			type="button"
			aria-label={t('layout.sidebar.resize')}
			class="absolute top-0 right-0 z-10 h-full w-1.5 cursor-col-resize bg-transparent hover:bg-primary/20"
			onpointerdown={startResize}
			ondblclick={resetWidth}
			onkeydown={(event) => {
				if (event.key === 'ArrowLeft' || event.key === 'ArrowRight') {
					event.preventDefault();
					persistWidth(width + (event.key === 'ArrowRight' ? 8 : -8));
				} else if (event.key === 'Home') {
					event.preventDefault();
					resetWidth();
				}
			}}
		></button>
	</aside>
{/if}
