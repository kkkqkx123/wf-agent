<script lang="ts">
	import Composer from '#lib/components/chat/Composer.svelte';
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
	}

	let { controller }: Props = $props();

	const selectedId = $derived(controller.selectedId);
	const locale = $derived(getLocale());

	let starters = $state<StarterSuggestion[] | null>(null);
	let backendOk = $state(true);
	let request = 0;
	let fullscreen = $state(false);
	let composerHeight = $state<number | null>(null);
	let dragging = $state(false);
	let dragStartY = 0;
	let dragStartHeight = 0;

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

	function startDrag(event: PointerEvent): void {
		if (fullscreen) return;
		dragging = true;
		dragStartY = event.clientY;
		dragStartHeight = composerHeight ?? 240;
		(event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
		event.preventDefault();
	}

	function onDrag(event: PointerEvent): void {
		if (!dragging) return;
		persistHeight(clampHeight(dragStartHeight + (dragStartY - event.clientY)));
	}

	function endDrag(): void {
		dragging = false;
	}

	function toggleFullscreen(): void {
		fullscreen = !fullscreen;
	}

	function onkeydown(event: KeyboardEvent): void {
		if (event.key === 'Escape' && fullscreen) {
			event.stopPropagation();
			fullscreen = false;
		}
	}

	$effect(() => {
		document.body.style.overflow = fullscreen ? 'hidden' : '';
		return () => {
			document.body.style.overflow = '';
		};
	});
</script>

<svelte:window {onkeydown} />

{#if fullscreen}
	<div class="fixed inset-0 z-50 flex flex-col bg-background/95 p-4 backdrop-blur-sm">
		<div class="mx-auto flex min-h-0 w-full max-w-4xl flex-1 flex-col">
			<DecisionMap {controller} />
			<div class="flex min-h-0 flex-1 flex-col rounded-xl border border-border bg-muted/30 p-2.5">
				<Composer
					bind:model={controller.model}
					bind:agent={controller.agentId}
					bind:workflow={controller.workflowId}
					bind:executionType={controller.executionType}
					busy={chatStream.active}
					draftKey={selectedId ?? NEW_SESSION}
					{suggestions}
					showSuggestions={controller.bubbles.length === 0}
					fullscreen={true}
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
{:else}
	<div class="relative shrink-0 border-t border-border px-3 pt-1 pb-2.5">
		<!-- Focusable ARIA separator: standard pattern for a resizable splitter handle -->
		<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
		<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
		<div
			role="separator"
			aria-orientation="horizontal"
			aria-label={t('chat.composer.resizeHandle')}
			tabindex="0"
			class="absolute top-0 right-0 left-0 flex h-3 cursor-ns-resize items-center justify-center text-muted-foreground hover:text-foreground"
			onpointerdown={startDrag}
			onpointermove={onDrag}
			onpointerup={endDrag}
			onpointercancel={endDrag}
			ondblclick={resetHeight}
			onkeydown={(event) => {
				if (event.key === 'ArrowUp' || event.key === 'ArrowDown') {
					event.preventDefault();
					const base = composerHeight ?? 240;
					persistHeight(clampHeight(base + (event.key === 'ArrowUp' ? 20 : -20)));
				} else if (event.key === 'Home') {
					event.preventDefault();
					resetHeight();
				}
			}}
		>
			<span
				class="h-1 w-12 rounded-full transition-colors {dragging
					? 'bg-primary'
					: 'bg-border hover:bg-primary/50'}"
			></span>
		</div>
		<div
			class="mx-auto max-w-4xl {composerHeight !== null ? 'min-h-0' : ''}"
			style:height={composerHeight !== null ? `${composerHeight}px` : undefined}
		>
			<DecisionMap {controller} />
			<div
				class="rounded-xl border border-border bg-muted/30 p-2.5 {composerHeight !== null
					? 'h-full min-h-0 overflow-y-auto'
					: ''}"
			>
				<Composer
					bind:model={controller.model}
					bind:agent={controller.agentId}
					bind:workflow={controller.workflowId}
					bind:executionType={controller.executionType}
					busy={chatStream.active}
					draftKey={selectedId ?? NEW_SESSION}
					{suggestions}
					showSuggestions={controller.bubbles.length === 0}
					fullscreen={false}
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
{/if}
