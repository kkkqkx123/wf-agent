<script lang="ts">
	import Icon from '#lib/ui/icons/Icon.svelte';
	import { cn } from '#lib/ui/cn';
	import Button from '#lib/ui/components/Button.svelte';
	import {
		commandLabel,
		matchCommands,
		parseCommand,
		SLASH_COMMANDS,
		type CommandAction,
	} from '#lib/config/commands';
	import { mentionSearch, type MentionCandidate } from '#lib/services/mentions';
	import { sessions } from '#lib/stores/sessions.svelte';
	import { toasts } from '#lib/stores/toast.svelte';
	import ModelPicker from '#lib/components/chat/ModelPicker.svelte';
	import AgentPicker from '#lib/components/chat/AgentPicker.svelte';
	import WorkflowPicker from '#lib/components/chat/WorkflowPicker.svelte';
	import ExecutionTypePicker, {
		type ExecutionType,
	} from '#lib/components/chat/ExecutionTypePicker.svelte';
	import type { MessageAttachment, OutgoingMessage } from '#lib/types/models';
	import type { StarterSuggestion } from '#lib/services/starters';
	import { entityMention, textRuns } from '#lib/utils/mentions';
	import { mentionClass, mentionIcon } from '#lib/utils/mention-style';
	import { t } from '#lib/i18n';
	import { SvelteSet } from 'svelte/reactivity';

	/** Largest inlined attachment per file; larger files are truncated. */
	const ATTACHMENT_LIMIT = 64_000;

	/** Entity kinds the mention picker resolves through the search channel. */
	const MENTION_KINDS = [
		'workflow',
		'agent_loop',
		'agent',
		'message',
		'execution',
		'checkpoint',
		'task',
		'event',
		'skill',
		'tool',
		'script',
		'resource',
	];

	const MENTION_DEBOUNCE_MS = 200;

	/** File references carry a path and optional line range and are typed
	 * directly; they are not served by the entity search channel. */
	const FILE_KIND = 'file';

	/** Total candidates fetched from backend; visible subset rendered first. */
	const MENTION_TOTAL_LIMIT = 30;
	const MENTION_VISIBLE_LIMIT = 12;

	interface Props {
		model?: string;
		agent?: string;
		workflow?: string;
		executionType?: ExecutionType;
		busy?: boolean;
		draftKey?: string;
		suggestions?: StarterSuggestion[];
		showSuggestions?: boolean;
		onsend?: (message: OutgoingMessage) => boolean;
		onstop?: () => void;
		oncommand?: (action: CommandAction, arg: string) => void;
		onSuggestionSelect?: (suggestion: StarterSuggestion) => void;
	}

	let {
		model = $bindable(''),
		agent = $bindable(''),
		workflow = $bindable(''),
		executionType = $bindable('agent'),
		busy = false,
		draftKey = 'new',
		suggestions = [],
		showSuggestions = false,
		onsend,
		onstop,
		oncommand,
		onSuggestionSelect,
	}: Props = $props();

	let draft = $state('');
	const mentionChips = $derived(textRuns(draft).filter((run) => run.kind !== null));
	let attachments = $state<MessageAttachment[]>([]);
	let queue = $state<OutgoingMessage[]>([]);
	let box: HTMLTextAreaElement | null = $state(null);
	let activeKey = $state('');
	let candidates = $state<MentionCandidate[]>([]);
	let mentionSelectedIndex = $state(0);
	let mentionDismissed = $state(false);
	let mentionVisibleCount = $state(MENTION_VISIBLE_LIMIT);
	let mentionMode = $state<'all' | 'files-and-dirs' | 'files' | 'directories' | 'resources'>('all');
	let searching = $state(false);
	let searchError = $state(false);
	let activePicker = $state<string | null>(null);

	$effect(() => {
		// Touch `candidates` so this effect re-runs whenever the list changes.
		// eslint-disable-next-line @typescript-eslint/no-unused-expressions
		candidates;
		mentionSelectedIndex = 0;
		mentionVisibleCount = MENTION_VISIBLE_LIMIT;
	});

	$effect(() => {
		if (mentionSelectedIndex >= filteredCandidates.length && filteredCandidates.length > 0) {
			mentionSelectedIndex = filteredCandidates.length - 1;
		}
	});

	$effect(() => {
		if (mentionSelectedIndex + 1 > mentionVisibleCount) {
			mentionVisibleCount = mentionSelectedIndex + 1;
		}
	});

	let timer: ReturnType<typeof setTimeout> | null = null;
	let latestRequest = 0;

	function switchDraft(nextKey: string): void {
		if (activeKey) sessions.setDraft(activeKey, draft);
		activeKey = nextKey;
		draft = sessions.draft(nextKey);
		attachments = [];
	}

	$effect(() => {
		if (draftKey !== activeKey) switchDraft(draftKey);
	});

	$effect(() => {
		if (!busy && queue.length > 0) sendNext();
	});

	function clearEditor(): void {
		draft = '';
		attachments = [];
		sessions.setDraft(activeKey, '');
	}

	function removeMention(token: string): void {
		const index = draft.indexOf(token);
		if (index < 0) return;
		removeMentionAt(index, token.length);
	}

	function removeMentionAt(start: number, length: number): void {
		draft = draft.slice(0, start) + draft.slice(start + length);
		sessions.setDraft(activeKey, draft);
		box?.focus();
		box?.setSelectionRange(start, start);
	}

	function focusMention(token: string): void {
		const index = draft.indexOf(token);
		if (index < 0) return;
		box?.focus();
		box?.setSelectionRange(index, index + token.length);
	}

	function submit(message: OutgoingMessage, current: boolean): void {
		if (!onsend?.(message)) return;
		// A queued message leaving the editor must not clear what the user typed
		// while it waited.
		if (!current) return;
		clearEditor();
		box?.focus();
	}

	function sendNext(): void {
		const next = queue.shift();
		if (next === undefined) return;
		queue = [...queue];
		submit(next, false);
	}

	function onSubmit(): void {
		const text = draft.trim();
		if (!text) {
			if (queue.length > 0 && !busy) sendNext();
			return;
		}
		const parsed = parseCommand(text);
		if (parsed) {
			clearEditor();
			oncommand?.(parsed.command.action, parsed.arg);
			return;
		}
		const message: OutgoingMessage = { text: draft, attachments };
		if (busy) {
			queue = [...queue, message];
			clearEditor();
			return;
		}
		submit(message, true);
	}

	function selectMention(hit: MentionCandidate): void {
		const reference = hit.kind === FILE_KIND ? `@file:${hit.id}` : entityMention(hit.kind, hit.id);
		insertReference(reference, hit.kind === FILE_KIND ? '' : ' ');
		mentionDismissed = true;
		candidates = [];
	}

	function onKey(event: KeyboardEvent): void {
		if (mentionVisible && (event.ctrlKey || event.metaKey) && !event.altKey && !event.shiftKey) {
			if (event.key === '1') {
				event.preventDefault();
				mentionMode = 'all';
				return;
			}
			if (event.key === '2') {
				event.preventDefault();
				mentionMode = 'files-and-dirs';
				return;
			}
			if (event.key === '3') {
				event.preventDefault();
				mentionMode = 'directories';
				return;
			}
			if (event.key === '4') {
				event.preventDefault();
				mentionMode = 'resources';
				return;
			}
		}
		if (mentionVisible && filteredCandidates.length > 0) {
			const pageSize = 5;
			const scrollSelected = () => {
				requestAnimationFrame(() => {
					document.querySelector('[aria-current="true"]')?.scrollIntoView({ block: 'nearest' });
				});
			};
			if (event.key === 'ArrowDown') {
				event.preventDefault();
				mentionSelectedIndex = (mentionSelectedIndex + 1) % filteredCandidates.length;
				scrollSelected();
				return;
			}
			if (event.key === 'ArrowUp') {
				event.preventDefault();
				mentionSelectedIndex =
					(mentionSelectedIndex - 1 + filteredCandidates.length) % filteredCandidates.length;
				scrollSelected();
				return;
			}
			if (event.key === 'PageDown') {
				event.preventDefault();
				mentionSelectedIndex = Math.min(
					filteredCandidates.length - 1,
					mentionSelectedIndex + pageSize,
				);
				scrollSelected();
				return;
			}
			if (event.key === 'PageUp') {
				event.preventDefault();
				mentionSelectedIndex = Math.max(0, mentionSelectedIndex - pageSize);
				scrollSelected();
				return;
			}
			if (event.key === 'Home') {
				event.preventDefault();
				mentionSelectedIndex = 0;
				scrollSelected();
				return;
			}
			if (event.key === 'End') {
				event.preventDefault();
				mentionSelectedIndex = filteredCandidates.length - 1;
				scrollSelected();
				return;
			}
			if (event.key === 'Enter' || event.key === 'Tab') {
				event.preventDefault();
				const hit = filteredCandidates[mentionSelectedIndex];
				if (hit) selectMention(hit);
				return;
			}
		}
		if (mentionVisible && event.key === 'Escape') {
			event.preventDefault();
			mentionDismissed = true;
			candidates = [];
			return;
		}
		// Delete an entire mention token when Backspace lands at its end,
		// so a half-deleted reference cannot linger in the draft.
		if (event.key === 'Backspace' && !event.shiftKey && !event.ctrlKey && !event.metaKey) {
			const cursor = box?.selectionStart ?? draft.length;
			const selectionEnd = box?.selectionEnd ?? draft.length;
			if (cursor === selectionEnd) {
				const hit = mentionChips.find((chip) => draft.slice(0, cursor).endsWith(chip.text));
				if (hit) {
					event.preventDefault();
					removeMentionAt(cursor - hit.text.length, hit.text.length);
					return;
				}
			}
		}
		if (event.key === 'Enter' && !event.shiftKey) {
			event.preventDefault();
			onSubmit();
		}
	}

	function onInput(): void {
		mentionDismissed = false;
		sessions.setDraft(activeKey, draft);
	}

	async function addFiles(files: FileList | File[]): Promise<void> {
		for (const file of Array.from(files)) {
			try {
				const text = await file.text();
				attachments = [
					...attachments,
					{
						name: file.name || 'pasted-text',
						content: text.slice(0, ATTACHMENT_LIMIT),
					},
				];
			} catch {
				toasts.error(
					t('chat.composer.readFailed', {
						name: file.name || t('chat.composer.attachment'),
					}),
				);
			}
		}
	}

	function onPaste(event: ClipboardEvent): void {
		const files = event.clipboardData?.files;
		if (files && files.length > 0) {
			event.preventDefault();
			void addFiles(files);
		}
	}

	function onDrop(event: DragEvent): void {
		const files = event.dataTransfer?.files;
		if (files && files.length > 0) {
			event.preventDefault();
			void addFiles(files);
		}
	}

	/** The `@…` fragment the caret sits behind, split into kind and term. */
	const mention = $derived.by(() => {
		const cursor = box?.selectionStart ?? draft.length;
		const match = /@([\w./:-]*)$/.exec(draft.slice(0, cursor));
		if (!match) return null;
		const separator = match[1].indexOf(':');
		if (separator < 0) return { kind: '', term: match[1] };
		return {
			kind: match[1].slice(0, separator),
			term: match[1].slice(separator + 1),
		};
	});

	const mentionVisible = $derived(mention !== null && !mentionDismissed);

	/** A typed kind narrows the search to that entity kind. */
	const mentionTypes = $derived(
		mention?.kind ? MENTION_KINDS.filter((kind) => kind === mention.kind) : MENTION_KINDS,
	);

	/** Category chips help before a term exists; they never stand in for hits.
	 * File references are not searchable, so they get their own chip. */
	const kindHints = $derived(
		mention && mention.term === '' && mention.kind === '' ? [...MENTION_KINDS, FILE_KIND] : [],
	);

	const commandHints = $derived(
		draft.startsWith('/') ? matchCommands(draft.slice(1).split(/\s+/)[0]) : [],
	);

	const GROUP_PRIORITY: Record<string, number> = {
		file: 0,
		skill: 2,
		workflow: 3,
		wf: 3,
		agent: 4,
		agent_loop: 4,
		tool: 5,
		script: 6,
		resource: 7,
	};

	function modeMatches(hit: MentionCandidate): boolean {
		const isDirectory = (hit.data as { kind?: string } | undefined)?.kind === 'directory';
		switch (mentionMode) {
			case 'all':
				return true;
			case 'files-and-dirs':
				return hit.kind === 'file';
			case 'files':
				return hit.kind === 'file' && !isDirectory;
			case 'directories':
				return hit.kind === 'file' && isDirectory;
			case 'resources':
				return hit.kind !== 'file';
			default:
				return true;
		}
	}

	/** Candidates grouped by entity type, preserving search order within each group. */
	const filteredCandidates = $derived(candidates.filter(modeMatches));
	const visibleCandidates = $derived(filteredCandidates.slice(0, mentionVisibleCount));
	const groupedCandidates = $derived.by(() => {
		const groups: { type: string; items: MentionCandidate[] }[] = [];
		const seen = new SvelteSet<string>();
		for (const hit of visibleCandidates) {
			if (!seen.has(hit.kind)) {
				seen.add(hit.kind);
				groups.push({ type: hit.kind, items: [] });
			}
			groups[groups.length - 1].items.push(hit);
		}
		groups.sort((a, b) => (GROUP_PRIORITY[a.type] ?? 9) - (GROUP_PRIORITY[b.type] ?? 9));
		return groups;
	});

	// Candidates come from the shared search channel, so the newest query wins.
	$effect(() => {
		const term = mention?.term ?? '';
		const kinds = mentionTypes;
		if (timer) clearTimeout(timer);
		if (mention?.kind !== FILE_KIND && !term) {
			candidates = [];
			searching = false;
			searchError = false;
			return;
		}
		if (mention?.kind === FILE_KIND && mentionDismissed) {
			candidates = [];
			searching = false;
			searchError = false;
			return;
		}
		const request = (latestRequest += 1);
		searching = true;
		searchError = false;
		timer = setTimeout(() => {
			void mentionSearch({ q: term, kinds, limit: MENTION_TOTAL_LIMIT })
				.then((outcome) => {
					if (request !== latestRequest) return;
					candidates = outcome.items;
					searching = false;
					searchError = false;
				})
				.catch(() => {
					if (request !== latestRequest) return;
					candidates = [];
					searching = false;
					searchError = true;
				});
		}, MENTION_DEBOUNCE_MS);
		return () => {
			if (timer) clearTimeout(timer);
		};
	});

	function onGlobalKeydown(event: KeyboardEvent): void {
		if (!(event.ctrlKey || event.metaKey) || !event.shiftKey) return;
		const key = event.key.toLowerCase();
		if (key === 'a') {
			event.preventDefault();
			activePicker = 'agent';
			box?.focus();
		} else if (key === 'e') {
			event.preventDefault();
			activePicker = 'execution';
			box?.focus();
		} else if (key === 'm') {
			event.preventDefault();
			activePicker = 'model';
			box?.focus();
		}
	}

	function insertReference(reference: string, separator: string = ' '): void {
		const cursor = box?.selectionStart ?? draft.length;
		const before = draft.slice(0, cursor).replace(/@[\w./:-]*$/, reference);
		// A full reference is followed by a separator; a bare kind prefix stays
		// open so the value flows straight into the mention.
		const trailing = reference.endsWith(':') ? '' : separator;
		draft = `${before}${trailing}${draft.slice(cursor)}`;
		box?.focus();
		const caret = before.length + trailing.length;
		box?.setSelectionRange(caret, caret);
	}

	/** Move a queued message one slot toward the head or tail. */
	function moveQueued(index: number, delta: number): void {
		const target = index + delta;
		if (target < 0 || target >= queue.length) return;
		const next = [...queue];
		const [item] = next.splice(index, 1);
		next.splice(target, 0, item);
		queue = next;
	}

	/** Pull a queued message back into the editor for editing; its
	 * attachments return with it so nothing is lost. */
	function editQueued(index: number): void {
		const item = queue[index];
		if (!item) return;
		draft = draft.trim() ? `${draft}\n${item.text}` : item.text;
		attachments = [...attachments, ...item.attachments];
		queue = queue.filter((_, current) => current !== index);
		sessions.setDraft(activeKey, draft);
		box?.focus();
	}
</script>

<svelte:window onkeydown={onGlobalKeydown} />

<div>
	{#if showSuggestions && suggestions.length > 0}
		<div class="mb-2 flex flex-wrap gap-1.5">
			{#each suggestions as suggestion (suggestion.id ?? suggestion.title)}
				<button
					type="button"
					title={suggestion.title}
					onclick={() => {
						draft = suggestion.content;
						onSuggestionSelect?.(suggestion);
						box?.focus();
					}}
					class="rounded-full border border-border bg-muted/50 px-2.5 py-1 text-caption text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
				>
					{suggestion.title}
				</button>
			{/each}
		</div>
	{/if}

	{#if queue.length > 0}
		<ul class="mb-2 space-y-1">
			{#each queue as item, index (index)}
				<li
					class="flex items-center gap-2 rounded-md border border-border bg-muted/50 px-2 py-1 text-caption text-muted-foreground"
				>
					<span class="font-mono text-micro">#{index + 1}</span>
					<span class="min-w-0 flex-1 truncate">{item.text}</span>
					{#if item.attachments.length > 0}
						<span class="shrink-0 font-mono text-micro">
							{t('chat.composer.fileCount', { count: item.attachments.length })}
						</span>
					{/if}
					<button
						type="button"
						aria-label={t('chat.composer.moveQueuedUp')}
						disabled={index === 0}
						class="shrink-0 hover:text-foreground disabled:opacity-40"
						onclick={() => moveQueued(index, -1)}
					>
						<Icon name="chevron-up" size={13} />
					</button>
					<button
						type="button"
						aria-label={t('chat.composer.moveQueuedDown')}
						disabled={index === queue.length - 1}
						class="shrink-0 hover:text-foreground disabled:opacity-40"
						onclick={() => moveQueued(index, 1)}
					>
						<Icon name="chevron-down" size={13} />
					</button>
					<button
						type="button"
						aria-label={t('chat.composer.editQueued')}
						class="shrink-0 hover:text-foreground"
						onclick={() => editQueued(index)}
					>
						<Icon name="pencil" size={13} />
					</button>
					<button
						type="button"
						aria-label={t('chat.composer.removeQueued')}
						class="shrink-0 hover:text-foreground"
						onclick={() => (queue = queue.filter((_, i) => i !== index))}
					>
						<Icon name="x" size={13} />
					</button>
				</li>
			{/each}
		</ul>
	{/if}

	{#if attachments.length > 0}
		<div class="mb-2 flex flex-wrap gap-1.5">
			{#each attachments as file, index (index)}
				<span
					class="flex items-center gap-1.5 rounded-md border border-border bg-muted/50 px-2 py-1 font-mono text-micro text-muted-foreground"
				>
					<Icon name="file" size={12} />
					{file.name}
					<button
						type="button"
						aria-label={t('chat.composer.removeAttachment')}
						class="hover:text-foreground"
						onclick={() => (attachments = attachments.filter((_, i) => i !== index))}
					>
						<Icon name="x" size={12} />
					</button>
				</span>
			{/each}
		</div>
	{/if}

	{#if mentionVisible}
		<div
			class="mb-2 flex max-h-72 flex-col gap-1 overflow-y-auto"
			role="listbox"
			aria-label={t('chat.composer.mentions')}
		>
			<div class="flex flex-wrap gap-1 px-2 pb-1">
				{#each [{ value: 'all', label: t('chat.composer.modeAll') }, { value: 'files-and-dirs', label: t('chat.composer.modeFilesAndDirs') }, { value: 'files', label: t('chat.composer.modeFiles') }, { value: 'directories', label: t('chat.composer.modeDirs') }, { value: 'resources', label: t('chat.composer.modeResources') }] as mode (mode.value)}
					<button
						type="button"
						onclick={() => (mentionMode = mode.value as typeof mentionMode)}
						class={cn(
							'rounded-full border border-border bg-card px-2 py-0.5 text-micro transition-colors hover:bg-accent',
							mentionMode === mode.value && 'bg-accent',
						)}
					>
						{mode.label}
					</button>
				{/each}
			</div>
			{#each groupedCandidates as group (group.type)}
				<div class="flex flex-col gap-1">
					<p class="px-2 text-micro font-semibold uppercase text-muted-foreground">
						{group.type}
					</p>
					{#each group.items as hit (`${hit.kind}-${hit.id}`)}
						<button
							type="button"
							aria-current={hit === filteredCandidates[mentionSelectedIndex] ? 'true' : undefined}
							onclick={() => selectMention(hit)}
							class={cn(
								'flex flex-col items-start gap-0.5 rounded-md border border-border bg-card px-2 py-1 text-left text-caption transition-colors hover:bg-accent',
								hit === candidates[mentionSelectedIndex] && 'bg-accent',
							)}
						>
							<span class="flex w-full items-center gap-2">
								<Icon
									name={mentionIcon(hit.kind, hit.id)}
									size={12}
									class={cn('shrink-0', mentionClass(hit.kind, hit.id))}
								/>
								<span class="min-w-0 flex-1 truncate text-foreground">{hit.label}</span>
								<span class="shrink-0 font-mono text-micro text-info">{hit.kind}</span>
							</span>
							{#if hit.description}
								<span class="w-full truncate text-micro text-muted-foreground">
									{hit.description}
								</span>
							{/if}
							<span class="w-full truncate font-mono text-micro text-muted-foreground">
								{hit.id}
							</span>
						</button>
					{/each}
				</div>
			{/each}
			{#if visibleCandidates.length < filteredCandidates.length}
				<button
					type="button"
					onclick={() =>
						(mentionVisibleCount = Math.min(
							filteredCandidates.length,
							mentionVisibleCount + MENTION_VISIBLE_LIMIT,
						))}
					class="px-2 text-micro text-info hover:underline"
				>
					{t('chat.composer.showMore', {
						count: filteredCandidates.length - visibleCandidates.length,
					})}
				</button>
			{/if}
			{#if searching}
				<p class="px-2 text-micro text-muted-foreground">
					{t('chat.composer.searching')}
				</p>
			{:else if kindHints.length > 0}
				<div class="flex flex-wrap gap-1.5">
					{#each kindHints as kind (kind)}
						<button
							type="button"
							onclick={() => insertReference(`@${kind}:`)}
							class={cn(
								'rounded-full border border-border bg-card px-2.5 py-1 font-mono text-micro transition-colors hover:bg-accent',
								mentionClass(kind),
							)}
						>
							@{kind}
						</button>
					{/each}
				</div>
			{:else if mention!.kind === FILE_KIND}
				<p class="px-2 text-micro text-muted-foreground">
					{t('chat.composer.fileHint')}
				</p>
			{:else if searchError}
				<p class="px-2 text-micro text-muted-foreground">
					{t('chat.composer.searchUnavailable')}
				</p>
			{:else if mention!.term && candidates.length === 0}
				<p class="px-2 text-micro text-muted-foreground">
					{t('chat.composer.noResults', {
						kind: mention!.kind || t('chat.composer.matching'),
						term: mention!.term,
					})}
				</p>
			{/if}
		</div>
	{/if}

	{#if commandHints.length > 0}
		<div class="mb-2 flex flex-col gap-1" role="listbox" aria-label={t('chat.composer.commands')}>
			{#each commandHints as command (command.name)}
				<button
					type="button"
					onclick={() => {
						draft = `/${command.name}${command.arg ? ' ' : ''}`;
						box?.focus();
					}}
					class="flex items-center gap-2 rounded-md border border-border bg-card px-2 py-1 text-left text-caption transition-colors hover:bg-accent"
				>
					<span class="font-mono text-micro text-info">
						{commandLabel(command)}
					</span>
					<span class="min-w-0 flex-1 truncate text-muted-foreground">
						{command.description}
					</span>
				</button>
			{/each}
		</div>
	{/if}

	{#if mentionChips.length > 0}
		<div class="mb-2 flex flex-wrap gap-1.5" aria-label={t('chat.composer.mentionsInDraft')}>
			{#each mentionChips as chip (chip.text)}
				<span
					class={cn(
						'inline-flex items-center gap-1 rounded border border-border/70 bg-muted/60 px-1.5 text-micro font-mono text-muted-foreground',
						mentionClass(chip.kind ?? '', chip.text),
					)}
					title={chip.text}
				>
					<button
						type="button"
						class="inline-flex items-center gap-1 hover:text-foreground"
						aria-label={t('chat.composer.focusMention', { name: chip.text })}
						onclick={() => focusMention(chip.text)}
					>
						<Icon name={mentionIcon(chip.kind ?? '', chip.text)} size={11} />
						{chip.text}
					</button>
					<button
						type="button"
						aria-label={t('chat.composer.removeMention', { name: chip.text })}
						class="ml-0.5 hover:text-foreground"
						onclick={(event) => {
							event.stopPropagation();
							removeMention(chip.text);
						}}
					>
						×
					</button>
				</span>
			{/each}
		</div>
	{/if}

	<div class="flex items-center gap-2">
		<ExecutionTypePicker
			bind:value={executionType}
			class="shrink-0"
			{activePicker}
			pickerKey="execution"
			onActivate={(key) => (activePicker = key)}
			onClose={() => {
				activePicker = null;
				box?.focus();
			}}
		/>
		{#if executionType === 'agent'}
			<AgentPicker
				bind:value={agent}
				class="shrink-0"
				{activePicker}
				pickerKey="agent"
				onActivate={(key) => (activePicker = key)}
				onClose={() => {
					activePicker = null;
					box?.focus();
				}}
			/>
		{:else if executionType === 'workflow'}
			<WorkflowPicker
				bind:value={workflow}
				class="shrink-0"
				{activePicker}
				pickerKey="workflow"
				onActivate={(key) => (activePicker = key)}
				onClose={() => {
					activePicker = null;
					box?.focus();
				}}
			/>
		{/if}
		<ModelPicker
			bind:value={model}
			class="shrink-0"
			{activePicker}
			pickerKey="model"
			onActivate={(key) => (activePicker = key)}
			onClose={() => {
				activePicker = null;
				box?.focus();
			}}
		/>
		<textarea
			bind:this={box}
			bind:value={draft}
			oninput={onInput}
			onkeydown={onKey}
			onpaste={onPaste}
			ondrop={onDrop}
			ondragover={(event) => event.preventDefault()}
			rows={2}
			placeholder={t('chat.composer.placeholder')}
			aria-label={t('chat.composer.ariaLabel')}
			class="max-h-36 min-h-9 flex-1 resize-y rounded-md border border-input bg-card px-2.5 py-2 text-body text-foreground placeholder:text-muted-foreground focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-ring"
		></textarea>
		{#if busy}
			<Button size="sm" variant="outline" onclick={() => onstop?.()}>
				<Icon name="square" size={13} />
				{t('chat.composer.stop')}
			</Button>
		{:else}
			<Button
				variant="brand"
				size="sm"
				disabled={!draft.trim() && queue.length === 0}
				onclick={onSubmit}
			>
				<Icon name="arrow-up" size={14} />
				{queue.length > 0
					? t('chat.composer.sendQueued', {
							count: queue.length + (draft.trim() ? 1 : 0),
						})
					: t('chat.composer.send')}
			</Button>
		{/if}
	</div>
	<p class="mt-1 text-micro text-muted-foreground">
		{t('chat.composer.hintsEnter')}{#if busy}
			· {t('chat.composer.hintsQueue')}{/if} · {SLASH_COMMANDS.map(
			(command) => `/${command.name}`,
		).join(' ')} · {t('chat.composer.hintsMentions')} · {t('chat.composer.hintsAttach')}
	</p>
</div>
