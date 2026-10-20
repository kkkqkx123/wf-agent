<script lang="ts">
	import Icon from '@wf-agent/ui/icons/Icon.svelte';
import { cn } from '@wf-agent/ui/cn';
	import Button from '@wf-agent/ui/components/Button.svelte';
	import {
		commandLabel,
		matchCommands,
		parseCommand,
		SLASH_COMMANDS,
		type CommandAction,
	} from '$lib/config/commands';
	import { mentionSearch, type MentionCandidate } from '$lib/services/mentions';
	import { sessions } from '$lib/stores/sessions.svelte';
	import { toasts } from '$lib/stores/toast.svelte';
	import ModelPicker from '$lib/components/chat/ModelPicker.svelte';
	import AgentPicker from '$lib/components/chat/AgentPicker.svelte';
	import WorkflowPicker from '$lib/components/chat/WorkflowPicker.svelte';
	import type { MessageAttachment, OutgoingMessage } from '$lib/types/models';
	import { entityMention } from '$lib/utils/mentions';
	import { mentionClass, mentionIcon } from '$lib/utils/mention-style';

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

	interface Props {
		model?: string;
		agent?: string;
		workflow?: string;
		mode?: 'standard' | 'chat';
		busy?: boolean;
		draftKey?: string;
		suggestions?: string[];
		showSuggestions?: boolean;
		onsend?: (message: OutgoingMessage) => boolean;
		onstop?: () => void;
		oncommand?: (action: CommandAction, arg: string) => void;
	}

	let {
		model = $bindable(''),
		agent = $bindable(''),
		workflow = $bindable(''),
		mode = $bindable('standard'),
		busy = false,
		draftKey = 'new',
		suggestions = [],
		showSuggestions = false,
		onsend,
		onstop,
		oncommand,
	}: Props = $props();

	let draft = $state('');
	let attachments = $state<MessageAttachment[]>([]);
	let queue = $state<OutgoingMessage[]>([]);
	let box: HTMLTextAreaElement | null = $state(null);
	let activeKey = $state('');
	let candidates = $state<MentionCandidate[]>([]);
	let mentionSelectedIndex = $state(0);
	let mentionDismissed = $state(false);
	let searching = $state(false);
	let searchError = $state(false);
	let activePicker = $state<string | null>(null);

	$effect(() => {
		candidates;
		mentionSelectedIndex = 0;
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
		const reference =
			hit.kind === FILE_KIND
				? `@file:${hit.id}`
				: entityMention(hit.kind, hit.id);
		insertReference(reference, hit.kind === FILE_KIND ? '' : ' ');
		mentionDismissed = true;
		candidates = [];
	}

	function onKey(event: KeyboardEvent): void {
		if (mentionVisible && candidates.length > 0) {
			if (event.key === 'ArrowDown') {
				event.preventDefault();
				mentionSelectedIndex =
					(mentionSelectedIndex + 1) % candidates.length;
				return;
			}
			if (event.key === 'ArrowUp') {
				event.preventDefault();
				mentionSelectedIndex =
					(mentionSelectedIndex - 1 + candidates.length) %
					candidates.length;
				return;
			}
			if (event.key === 'Enter' || event.key === 'Tab') {
				event.preventDefault();
				const hit = candidates[mentionSelectedIndex];
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
				toasts.error(`Could not read ${file.name || 'attachment'}`);
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
		mention?.kind
			? MENTION_KINDS.filter((kind) => kind === mention.kind)
			: MENTION_KINDS,
	);

	/** Category chips help before a term exists; they never stand in for hits.
	 * File references are not searchable, so they get their own chip. */
	const kindHints = $derived(
		mention && mention.term === '' && mention.kind === ''
			? [...MENTION_KINDS, FILE_KIND]
			: [],
	);

	const commandHints = $derived(
		draft.startsWith('/') ? matchCommands(draft.slice(1).split(/\s+/)[0]) : [],
	);

	/** Candidates grouped by entity type, preserving search order within each group. */
	const groupedCandidates = $derived.by(() => {
		const groups: { type: string; items: MentionCandidate[] }[] = [];
		const seen = new Set<string>();
		for (const hit of candidates) {
			if (!seen.has(hit.kind)) {
				seen.add(hit.kind);
				groups.push({ type: hit.kind, items: [] });
			}
			groups[groups.length - 1].items.push(hit);
		}
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
			void mentionSearch({ q: term, kinds, limit: 8 })
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
		} else if (key === 'w') {
			event.preventDefault();
			activePicker = 'workflow';
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
			{#each suggestions as suggestion (suggestion)}
				<button
					type="button"
					onclick={() => {
						draft = suggestion;
						box?.focus();
					}}
					class="rounded-full border border-border bg-muted/50 px-2.5 py-1 text-caption text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
				>
					{suggestion}
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
							+{item.attachments.length} file
						</span>
					{/if}
					<button
						type="button"
						aria-label="Move queued message up"
						disabled={index === 0}
						class="shrink-0 hover:text-foreground disabled:opacity-40"
						onclick={() => moveQueued(index, -1)}
					>
						<Icon name="chevron-up" size={13} />
					</button>
					<button
						type="button"
						aria-label="Move queued message down"
						disabled={index === queue.length - 1}
						class="shrink-0 hover:text-foreground disabled:opacity-40"
						onclick={() => moveQueued(index, 1)}
					>
						<Icon name="chevron-down" size={13} />
					</button>
					<button
						type="button"
						aria-label="Edit queued message"
						class="shrink-0 hover:text-foreground"
						onclick={() => editQueued(index)}
					>
						<Icon name="pencil" size={13} />
					</button>
					<button
						type="button"
						aria-label="Remove queued message"
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
						aria-label="Remove attachment"
						class="hover:text-foreground"
						onclick={() =>
							(attachments = attachments.filter((_, i) => i !== index))}
					>
						<Icon name="x" size={12} />
					</button>
				</span>
			{/each}
		</div>
	{/if}

	{#if mentionVisible}
		<div class="mb-2 flex flex-col gap-1" role="listbox" aria-label="Mentions">
			{#each groupedCandidates as group (group.type)}
				<div class="flex flex-col gap-1">
					<p class="px-2 text-micro font-semibold uppercase text-muted-foreground">
						{group.type}
					</p>
					{#each group.items as hit (`${hit.kind}-${hit.id}`)}
						<button
							type="button"
							aria-selected={hit === candidates[mentionSelectedIndex]}
							onclick={() => selectMention(hit)}
							class={cn(
								'flex flex-col items-start gap-0.5 rounded-md border border-border bg-card px-2 py-1 text-left text-caption transition-colors hover:bg-accent',
								mentionClass(hit.kind),
								hit === candidates[mentionSelectedIndex] && 'bg-accent',
							)}
						>
							<span class="flex w-full items-center gap-2">
								<Icon
									name={mentionIcon(hit.kind)}
									size={12}
									class="shrink-0"
								/>
								<span class="min-w-0 flex-1 truncate text-foreground"
									>{hit.label}</span
								>
								<span class="shrink-0 font-mono text-micro text-info">{hit.kind}</span
								>
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
			{#if searching}
				<p class="px-2 text-micro text-muted-foreground">Searching…</p>
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
					Type a path, optionally with a line range (e.g. @file:src/a.ts#10-20).
				</p>
			{:else if searchError}
				<p class="px-2 text-micro text-muted-foreground">
					Mention search unavailable.
				</p>
			{:else if mention!.term && candidates.length === 0}
				<p class="px-2 text-micro text-muted-foreground">
					No {mention!.kind || 'matching'} results for “{mention!.term}”.
				</p>
			{/if}
		</div>
	{/if}

	{#if commandHints.length > 0}
		<div class="mb-2 flex flex-col gap-1" role="listbox" aria-label="Commands">
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

	<div class="flex items-center gap-2">
		<AgentPicker
			bind:value={agent}
			class="shrink-0"
			activePicker={activePicker}
			pickerKey="agent"
			onActivate={(key) => (activePicker = key)}
			onClose={() => {
				activePicker = null;
				box?.focus();
			}}
		/>
		<WorkflowPicker
			bind:value={workflow}
			class="shrink-0"
			activePicker={activePicker}
			pickerKey="workflow"
			onActivate={(key) => (activePicker = key)}
			onClose={() => {
				activePicker = null;
				box?.focus();
			}}
		/>
		<button
			type="button"
			aria-pressed={mode === 'chat'}
			aria-label="Toggle plain chat mode"
			title={mode === 'chat'
				? 'Plain chat: tool-free responses. Click to switch back to the agent loop.'
				: 'Agent loop with tools. Click to switch to tool-free plain chat.'}
			onclick={() => (mode = mode === 'chat' ? 'standard' : 'chat')}
			class={cn(
				'flex h-9 shrink-0 items-center gap-1.5 rounded-md border border-input bg-card px-2 text-caption transition-colors hover:bg-accent',
				mode === 'chat'
					? 'text-foreground'
					: 'text-muted-foreground',
			)}
		>
			<Icon name="loop" size={13} class="shrink-0" />
			{mode === 'chat' ? 'Chat' : 'Agent'}
		</button>
		<ModelPicker
			bind:value={model}
			class="shrink-0"
			activePicker={activePicker}
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
			placeholder="Message the agent… (Enter to send, Shift+Enter for newline)"
			aria-label="Message composer"
			class="max-h-36 min-h-9 flex-1 resize-y rounded-md border border-input bg-card px-2.5 py-2 text-body text-foreground placeholder:text-muted-foreground focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-ring"
		></textarea>
		{#if busy}
			<Button size="sm" variant="outline" onclick={() => onstop?.()}>
				<Icon name="square" size={13} />
				Stop
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
					? `Send (${queue.length + (draft.trim() ? 1 : 0)})`
					: 'Send'}
			</Button>
		{/if}
	</div>
	<p class="mt-1 text-micro text-muted-foreground">
		Enter sends{#if busy}
			· sending queues next{/if} · {SLASH_COMMANDS.map(
			(command) => `/${command.name}`,
		).join(' ')} · @mentions resolve through search · paste or drop files to attach
	</p>
</div>
