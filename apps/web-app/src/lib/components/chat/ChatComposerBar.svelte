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
	 *  An unreachable backend keeps the i18n fallback (older servers
	 *  lack the endpoint); an intentionally empty backend hides the row. */
	async function loadStarters(activeLocale: string, attempt: number): Promise<void> {
		try {
			const items = await listStarters({ locale: activeLocale });
			if (attempt !== request) return;
			starters = items.map(toSuggestion);
			backendOk = true;
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
				showSuggestions={controller.bubbles.length === 0}
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
