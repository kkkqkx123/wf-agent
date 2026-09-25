<script lang="ts">
	import Icon from '$lib/components/icons/Icon.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import {
		commandLabel,
		matchCommands,
		parseCommand,
		SLASH_COMMANDS,
		type CommandAction,
	} from '$lib/config/commands';
	import { unifiedSearch, type SearchHit } from '$lib/services/search';
	import { sessions } from '$lib/stores/sessions.svelte';
	import { toasts } from '$lib/stores/toast.svelte';
	import type { MessageAttachment, OutgoingMessage } from '$lib/types/models';
	import { mentionRef } from '$lib/utils/mentions';

	/** Largest inlined attachment per file; larger files are truncated. */
	const ATTACHMENT_LIMIT = 64_000;

	/** Entity kinds the mention picker resolves through the search channel. */
	const MENTION_KINDS = [
		'workflow',
		'agent_loop',
		'message',
		'execution',
		'checkpoint',
		'task',
		'event',
	];

	const MENTION_DEBOUNCE_MS = 200;

	interface Props {
		model?: string;
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
	let candidates = $state<SearchHit[]>([]);
	let searching = $state(false);

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

	function onKey(event: KeyboardEvent): void {
		if (event.key === 'Enter' && !event.shiftKey) {
			event.preventDefault();
			onSubmit();
		}
	}

	function onInput(): void {
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
		const match = /@([\w.:-]*)$/.exec(draft.slice(0, cursor));
		if (!match) return null;
		const separator = match[1].indexOf(':');
		if (separator < 0) return { kind: '', term: match[1] };
		return {
			kind: match[1].slice(0, separator),
			term: match[1].slice(separator + 1),
		};
	});

	/** A typed kind narrows the search to that entity kind. */
	const mentionTypes = $derived(
		mention?.kind
			? MENTION_KINDS.filter((kind) => kind === mention.kind)
			: MENTION_KINDS,
	);

	/** Category chips help before a term exists; they never stand in for hits. */
	const kindHints = $derived(
		mention && mention.term === '' && mention.kind === '' ? MENTION_KINDS : [],
	);

	const commandHints = $derived(
		draft.startsWith('/') ? matchCommands(draft.slice(1).split(/\s+/)[0]) : [],
	);

	// Candidates come from the shared search channel, so the newest query wins.
	$effect(() => {
		const term = mention?.term ?? '';
		const types = mentionTypes.join(',');
		if (timer) clearTimeout(timer);
		if (!term) {
			candidates = [];
			searching = false;
			return;
		}
		const request = (latestRequest += 1);
		searching = true;
		timer = setTimeout(() => {
			void unifiedSearch({ q: term, types, limit: 8 })
				.then((outcome) => {
					if (request !== latestRequest) return;
					candidates = outcome.items;
					searching = false;
				})
				.catch(() => {
					if (request !== latestRequest) return;
					candidates = [];
					searching = false;
				});
		}, MENTION_DEBOUNCE_MS);
		return () => {
			if (timer) clearTimeout(timer);
		};
	});

	function insertReference(reference: string): void {
		const cursor = box?.selectionStart ?? draft.length;
		const before = draft.slice(0, cursor).replace(/@[\w.:-]*$/, reference);
		draft = `${before} ${draft.slice(cursor)}`;
		box?.focus();
		box?.setSelectionRange(before.length + 1, before.length + 1);
	}
</script>

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

	{#if mention !== null}
		<div class="mb-2 flex flex-col gap-1" role="listbox" aria-label="Mentions">
			{#each candidates as hit (`${hit.type}-${hit.id}`)}
				<button
					type="button"
					onclick={() => insertReference(mentionRef(hit.type, hit.label))}
					class="flex items-center gap-2 rounded-md border border-border bg-card px-2 py-1 text-left text-caption transition-colors hover:bg-accent"
				>
					<span class="min-w-0 flex-1 truncate text-foreground"
						>{hit.label}</span
					>
					<span class="shrink-0 font-mono text-micro text-info">{hit.type}</span
					>
				</button>
			{/each}
			{#if searching}
				<p class="px-2 text-micro text-muted-foreground">Searching…</p>
			{:else if kindHints.length > 0}
				<div class="flex flex-wrap gap-1.5">
					{#each kindHints as kind (kind)}
						<button
							type="button"
							onclick={() => insertReference(`@${kind}:`)}
							class="rounded-full border border-border bg-card px-2.5 py-1 font-mono text-micro text-info transition-colors hover:bg-accent"
						>
							@{kind}
						</button>
					{/each}
				</div>
			{:else if mention.term && candidates.length === 0}
				<p class="px-2 text-micro text-muted-foreground">
					No {mention.kind || 'matching'} results for “{mention.term}”.
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
		<input
			bind:value={model}
			placeholder="Model"
			aria-label="Model"
			class="h-9 w-44 shrink-0 rounded-md border border-input bg-card px-2 font-mono text-caption text-foreground placeholder:text-muted-foreground focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-ring"
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
