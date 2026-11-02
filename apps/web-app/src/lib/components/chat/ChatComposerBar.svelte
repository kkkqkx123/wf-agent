<script lang="ts">
	import Composer from '#lib/components/chat/Composer.svelte';
	import WorkflowInputForm from '#lib/components/chat/WorkflowInputForm.svelte';
	import ResizeHandle from '#lib/components/layout/ResizeHandle.svelte';
	import DecisionMap from '#lib/components/chat/DecisionMap.svelte';
	import { chatStream } from '#lib/stores/stream-run.svelte';
	import { NEW_SESSION } from '#lib/stores/sessions.svelte';
	import { getLocale, t, type MessageKey } from '#lib/i18n';
	import {
		listStarters,
		normalizeStarterLocale,
		recordStarterUsage,
		toSuggestion,
		type StarterSuggestion,
	} from '#lib/services/starters';
	import type { ChatSessionController } from '#lib/chat/chat-session.svelte';

	// Fallback chips when the backend is unreachable or returns nothing.
	// Ids are absent, so no usage is recorded for fallback picks.
	const SUGGESTION_KEYS = [
		'chat.chatComposerBar.suggestSummary',
		'chat.chatComposerBar.suggestTools',
		'chat.chatComposerBar.suggestPlan',
	] as const satisfies readonly MessageKey[];

	const HEIGHT_KEY = 'wf.chat.composer.height';
	const MIN_HEIGHT = 180;
	const MAX_RATIO = 0.75;

	interface Props {
		controller: ChatSessionController;
		fullscreen?: boolean;
		onToggleFullscreen?: () => void;
	}

	let { controller, fullscreen = $bindable(false), onToggleFullscreen }: Props = $props();

	const selectedId = $derived(controller.selectedId);
	const locale = $derived(getLocale());

	let starters = $state<StarterSuggestion[] | null>(null);
	let backendOk = $state(true);
	let request = 0;
	let composerHeight = $state<number | null>(null);

	$effect(() => {
		if (controller.executionType !== 'workflow') return;
		const id = controller.workflowId.trim();
		if (!id) return;
		void controller.loadWorkflowVariables(id);
	});

	$effect(() => {
		try {
			const raw = localStorage.getItem(HEIGHT_KEY);
			const parsed = raw === null ? NaN : Number.parseInt(raw, 10);
			composerHeight = Number.isFinite(parsed) && parsed >= MIN_HEIGHT ? parsed : null;
		} catch {
			composerHeight = null;
		}
	});

	/** Backend chips for the active locale; null while loading, so the
	 *  composer renders nothing instead of a layout-shifting skeleton.
	 *  Probing includes disabled rows so an intentional hide (rows exist
	 *  but all disabled) stays hidden instead of falling back to English;
	 *  only a locale with no rows at all falls back. An unreachable
	 *  backend keeps the i18n fallback (older servers lack the endpoint). */
	async function loadStarters(rawLocale: string, attempt: number): Promise<void> {
		const locale = normalizeStarterLocale(rawLocale) || 'en';
		try {
			const all = await listStarters({ locale, includeDisabled: true });
			if (attempt !== request) return;
			const enabled = all.filter((item) => item.enabled);
			if (enabled.length > 0) {
				starters = enabled.map(toSuggestion);
				backendOk = true;
				return;
			}
			if (all.length > 0) {
				starters = [];
				backendOk = true;
				return;
			}
			if (locale === 'en') {
				starters = [];
				backendOk = true;
				return;
			}
			try {
				const englishAll = await listStarters({ locale: 'en', includeDisabled: true });
				if (attempt !== request) return;
				starters = englishAll.filter((item) => item.enabled).map(toSuggestion);
				backendOk = true;
			} catch {
				if (attempt !== request) return;
				starters = [];
				backendOk = false;
			}
		} catch {
			if (attempt !== request) return;
			starters = [];
			backendOk = false;
		}
	}

	$effect(() => {
		request += 1;
		starters = null;
		void loadStarters(locale, request);
	});

	// A locale switch re-renders `t()`, so fallback chips follow live.
	const fallback = $derived(SUGGESTION_KEYS.map((key) => ({ title: t(key), content: t(key) })));
	const suggestions = $derived(
		starters === null ? [] : starters.length > 0 || backendOk ? starters : fallback,
	);

	/** Starter awaiting usage credit; cleared on session switch. */
	let pendingStarterId = $state<string | null>(null);

	$effect(() => {
		void selectedId;
		pendingStarterId = null;
	});

	function handleSend(message: Parameters<ChatSessionController['sendText']>[0]): boolean {
		const accepted = controller.sendText(message);
		if (accepted && pendingStarterId) void recordStarterUsage(pendingStarterId);
		pendingStarterId = null;
		return accepted;
	}

	function persistHeight(next: number | null): void {
		composerHeight = next;
		try {
			if (next === null) localStorage.removeItem(HEIGHT_KEY);
			else localStorage.setItem(HEIGHT_KEY, String(next));
		} catch {
			// Storage may be unavailable; the in-memory height still applies.
		}
	}

	function resetHeight(): void {
		persistHeight(null);
	}

	function clampHeight(next: number): number {
		const max = Math.floor(window.innerHeight * MAX_RATIO);
		return Math.min(Math.max(next, MIN_HEIGHT), Math.max(max, MIN_HEIGHT));
	}

	function onResizeHandle(delta: number): void {
		persistHeight(clampHeight((composerHeight ?? 240) + delta));
	}

	function toggleFullscreen(): void {
		if (onToggleFullscreen) onToggleFullscreen();
		else fullscreen = !fullscreen;
	}

	function onkeydown(event: KeyboardEvent): void {
		if (event.key === 'Escape' && fullscreen) {
			event.stopPropagation();
			toggleFullscreen();
		}
	}
</script>

<svelte:window {onkeydown} />

<div
	class="relative border-t border-border px-3 pt-1 pb-2.5 {fullscreen
		? 'min-h-0 flex-1 overflow-y-auto'
		: 'shrink-0'}"
>
	{#if !fullscreen}
		<ResizeHandle
			label={t('chat.composer.resizeHandle')}
			disabled={fullscreen}
			class="absolute top-0 right-0 left-0 h-3 text-muted-foreground hover:text-foreground"
			pillClass="h-1 w-12 bg-border hover:bg-primary/50"
			pillActiveClass="bg-primary"
			onResize={onResizeHandle}
			onReset={resetHeight}
		/>
	{/if}
	<div
		class="w-full {composerHeight !== null && !fullscreen ? 'min-h-0' : ''} {fullscreen
			? 'h-full min-h-0'
			: ''}"
		style:height={composerHeight !== null && !fullscreen ? `${composerHeight}px` : undefined}
	>
		<DecisionMap {controller} />
		{#if controller.executionType === 'agent'}
			{@const isDraft = !selectedId}
			{@const effective =
				controller.temporaryOverride === 'full'
					? null
					: (controller.temporaryOverride ?? (isDraft ? 'restricted' : null))}
			{@const badge = selectedId ? controller.sessionBadge(selectedId) : effective}
			<div class="mb-2 flex flex-wrap items-center gap-1.5" aria-label="temporary conversation mode">
				{#if badge}
					<span
						class="rounded-full border border-amber-500/40 bg-amber-500/10 px-2 py-0.5 text-caption text-amber-700 dark:text-amber-300"
						title={badge === 'ephemeral'
							? 'Read-only Q&A: no checkpoints, transcript only'
							: 'Temporary conversation: fenced tools until promoted'}
					>
						{badge === 'ephemeral' ? 'read-only Q&A' : 'temporary'}
					</span>
				{/if}
				<div class="flex overflow-hidden rounded-md border border-border" role="group">
					<button
						type="button"
						class="px-2 py-0.5 text-caption {effective === null ? 'bg-primary text-primary-foreground' : 'text-muted-foreground hover:text-foreground'}"
						title="Full agent: all tools, checkpoints on"
						onclick={() => controller.setTemporaryOverride('full')}
					>
						full
					</button>
					<button
						type="button"
						class="px-2 py-0.5 text-caption {effective === 'restricted' ? 'bg-primary text-primary-foreground' : 'text-muted-foreground hover:text-foreground'}"
						title="Temporary: read-only, interaction and network lookup only"
						onclick={() => controller.setTemporaryOverride('restricted')}
					>
						temporary
					</button>
					<button
						type="button"
						class="px-2 py-0.5 text-caption {effective === 'ephemeral' ? 'bg-primary text-primary-foreground' : 'text-muted-foreground hover:text-foreground'}"
						title="Read-only Q&A: local inspection and follow-up only, no checkpoints"
						onclick={() => controller.setTemporaryOverride('ephemeral')}
					>
						read-only
					</button>
				</div>
				{#if badge && effective !== null}
					<button
						type="button"
						class="px-2 py-0.5 text-caption text-muted-foreground underline-offset-2 hover:text-foreground hover:underline"
						title="Promote: the next turn runs as a full agent"
						onclick={() => controller.promoteToFull()}
					>
						promote to full
					</button>
				{/if}
			</div>
		{/if}
		<div
			class="rounded-xl border border-border bg-muted/30 p-2.5 {composerHeight !== null &&
			!fullscreen
				? 'h-full min-h-0 overflow-y-auto'
				: ''} {fullscreen ? 'flex h-full min-h-0 flex-col' : ''}"
		>
			<WorkflowInputForm {controller} />
			<Composer
				bind:model={controller.model}
				bind:agent={controller.agentId}
				bind:workflow={controller.workflowId}
				bind:executionType={controller.executionType}
				busy={chatStream.active}
				draftKey={selectedId ?? NEW_SESSION}
				{suggestions}
				showSuggestions={controller.showStarters}
				{fullscreen}
				onToggleFullscreen={toggleFullscreen}
				onResetHeight={resetHeight}
				onsend={handleSend}
				onstop={() => chatStream.stop()}
				oncommand={(action, arg) => controller.COMMANDS[action](arg)}
				onSuggestionSelect={(suggestion) => {
					pendingStarterId = suggestion.id ?? null;
				}}
			/>
		</div>
	</div>
</div>
