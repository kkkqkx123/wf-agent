<script lang="ts">
	import Icon from '$lib/components/icons/Icon.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import { toasts } from '$lib/stores/toast.svelte';

	/** Largest inlined attachment per file; larger files are truncated. */
	const ATTACHMENT_LIMIT = 64_000;
	const DRAFTS_KEY = 'wf-chat-drafts';

	const MENTION_KINDS = ['file', 'skill', 'workflow', 'model', 'tool'];
	const SLASH_COMMANDS = ['/new', '/retry', '/continue'];

	interface Attachment {
		name: string;
		content: string;
	}

	interface Props {
		draft?: string;
		model?: string;
		busy?: boolean;
		draftKey?: string;
		suggestions?: string[];
		showSuggestions?: boolean;
		onsend?: (text: string) => boolean;
		onstop?: () => void;
		oncommand?: (command: string) => void;
	}

	let {
		draft = $bindable(''),
		model = $bindable(''),
		busy = false,
		draftKey = 'new',
		suggestions = [],
		showSuggestions = false,
		onsend,
		onstop,
		oncommand,
	}: Props = $props();

	let attachments = $state<Attachment[]>([]);
	let queue = $state<string[]>([]);
	let box: HTMLTextAreaElement | null = $state(null);
	let activeKey = $state('');

	function readDrafts(): Record<string, string> {
		try {
			const raw = localStorage.getItem(DRAFTS_KEY);
			return raw ? (JSON.parse(raw) as Record<string, string>) : {};
		} catch {
			return {};
		}
	}

	function persistDrafts(next: Record<string, string>): void {
		try {
			localStorage.setItem(DRAFTS_KEY, JSON.stringify(next));
		} catch {
			// Storage may be unavailable; the composer still works in memory.
		}
	}

	function switchDraft(nextKey: string): void {
		const drafts = readDrafts();
		if (draft !== (drafts[activeKey] ?? '') || draft !== '') {
			drafts[activeKey] = draft;
			persistDrafts(drafts);
		}
		activeKey = nextKey;
		draft = drafts[nextKey] ?? '';
		attachments = [];
	}

	$effect(() => {
		if (draftKey !== activeKey) switchDraft(draftKey);
	});

	$effect(() => {
		if (!busy && queue.length > 0) sendNext();
	});

	function withAttachments(text: string): string {
		if (attachments.length === 0) return text;
		const blocks = attachments.map(
			(file) =>
				`<attachment name="${file.name}">\n${file.content}\n</attachment>`,
		);
		return `${text}\n\n${blocks.join('\n\n')}`;
	}

	function submit(text: string): void {
		const full = withAttachments(text);
		if (!onsend?.(full)) return;
		if (text === draft) draft = '';
		attachments = [];
		const drafts = readDrafts();
		drafts[activeKey] = '';
		persistDrafts(drafts);
		box?.focus();
	}

	function sendNext(): void {
		const next = queue.shift();
		if (next === undefined) {
			if (draft.trim()) submit(draft);
			return;
		}
		queue = [...queue];
		submit(next);
	}

	function onSubmit(): void {
		const text = draft.trim();
		if (!text) {
			if (queue.length > 0 && !busy) sendNext();
			return;
		}
		if (text.startsWith('/')) {
			const token = text.split(/\s/, 1)[0];
			if ((SLASH_COMMANDS as string[]).includes(token)) {
				draft = '';
				oncommand?.(token.slice(1));
				return;
			}
		}
		if (busy) {
			queue = [...queue, draft];
			draft = '';
			return;
		}
		submit(draft);
	}

	function onKey(event: KeyboardEvent): void {
		if (event.key === 'Enter' && !event.shiftKey) {
			event.preventDefault();
			onSubmit();
		}
	}

	function onInput(): void {
		const drafts = readDrafts();
		drafts[activeKey] = draft;
		persistDrafts(drafts);
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

	const mentionQuery = $derived.by(() => {
		const cursor = box?.selectionStart ?? draft.length;
		const before = draft.slice(0, cursor);
		const match = /@([\w-]*)$/.exec(before);
		return match ? match[1] : null;
	});

	const mentionHints = $derived(
		mentionQuery === null
			? []
			: MENTION_KINDS.filter((kind) => kind.startsWith(mentionQuery)),
	);

	function insertMention(kind: string): void {
		const cursor = box?.selectionStart ?? draft.length;
		const before = draft.slice(0, cursor).replace(/@[\w-]*$/, `@${kind}:`);
		draft = before + draft.slice(cursor);
		box?.focus();
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
					<span class="min-w-0 flex-1 truncate">{item}</span>
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

	{#if mentionHints.length > 0}
		<div
			class="mb-2 flex flex-wrap gap-1.5"
			role="listbox"
			aria-label="Mentions"
		>
			{#each mentionHints as kind (kind)}
				<button
					type="button"
					onclick={() => insertMention(kind)}
					class="rounded-full border border-border bg-card px-2.5 py-1 font-mono text-micro text-info transition-colors hover:bg-accent"
				>
					@{kind}
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
			· sending queues next{/if} · /new /retry /continue · @mentions files, skills,
		workflows, models, tools · paste or drop files to attach
	</p>
</div>
