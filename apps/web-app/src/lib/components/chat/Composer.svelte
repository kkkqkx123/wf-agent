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
	import StarterRow from '#lib/components/chat/StarterRow.svelte';
	import ComposerQueue from '#lib/components/chat/ComposerQueue.svelte';
	import ComposerAttachments from '#lib/components/chat/ComposerAttachments.svelte';
	import IconButton from '#lib/ui/components/IconButton.svelte';
	import ExecutionTypePicker, {
		type ExecutionType,
	} from '#lib/components/chat/ExecutionTypePicker.svelte';
	import type {
		ImageAttachment,
		ImageDetail,
		MessageAttachment,
		OutgoingMessage,
	} from '#lib/types/models';
	import type { StarterSuggestion } from '#lib/services/starters';
	import { entityMention, textRuns } from '#lib/utils/mentions';
	import { mentionClass, mentionIcon } from '#lib/utils/mention-style';
	import { t } from '#lib/i18n';
	import { SvelteSet } from 'svelte/reactivity';
	import { currentMediaLimits, loadMediaLimits } from '#lib/config/media';
	import {
		imageExtMime,
		imageMimeAllowed as isImageMimeAllowed,
		preflightImage,
		stagedDecodedBytes,
	} from '#lib/utils/image-preflight';

	let mediaLimits = $state(currentMediaLimits());

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
		fullscreen?: boolean;
		/** Fixed-height card (resized via the drag handle): the input stretches. */
		grow?: boolean;
		onToggleFullscreen?: () => void;
		onResetHeight?: () => void;
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
		fullscreen = false,
		grow = false,
		onToggleFullscreen,
		onResetHeight,
		onsend,
		onstop,
		oncommand,
		onSuggestionSelect,
	}: Props = $props();

	let draft = $state('');
	const mentionChips = $derived(textRuns(draft).filter((run) => run.kind !== null));
	let attachments = $state<MessageAttachment[]>([]);
	let images = $state<ImageAttachment[]>([]);
	let queue = $state<OutgoingMessage[]>([]);

	/** Render detail applied to newly staged images; null means provider default. */
	const IMAGE_DETAIL_KEY = 'wf-image-detail';
	const IMAGE_DETAILS: ImageDetail[] = ['auto', 'low', 'high', 'original'];

	function loadImageDetail(): ImageDetail | null {
		try {
			const raw =
				typeof localStorage !== 'undefined' ? localStorage.getItem(IMAGE_DETAIL_KEY) : null;
			return (IMAGE_DETAILS as string[]).includes(raw ?? '') ? (raw as ImageDetail) : null;
		} catch {
			// Storage may be unavailable; fall back to the provider default.
			return null;
		}
	}

	let imageDetail = $state<ImageDetail | null>(loadImageDetail());

	function setImageDetail(next: ImageDetail | null): void {
		imageDetail = next;
		try {
			if (next) localStorage.setItem(IMAGE_DETAIL_KEY, next);
			else localStorage.removeItem(IMAGE_DETAIL_KEY);
		} catch {
			// Storage may be unavailable; the detail still applies this session.
		}
	}
	let box: HTMLTextAreaElement | null = $state(null);
	let attachInput: HTMLInputElement | null = $state(null);
	let imageInput: HTMLInputElement | null = $state(null);
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
		void loadMediaLimits().then((next) => {
			mediaLimits = next;
		});
	});

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

	function persistDraft(): void {
		if (!activeKey) return;
		sessions.setDraft(activeKey, draft);
		sessions.setDraftAttachments(activeKey, attachments);
		sessions.setDraftImages(activeKey, images);
	}

	function switchDraft(nextKey: string): void {
		if (activeKey) persistDraft();
		activeKey = nextKey;
		draft = sessions.draft(nextKey);
		attachments = sessions.draftAttachments(nextKey);
		images = sessions.draftImages(nextKey);
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
		images = [];
		if (!activeKey) return;
		sessions.setDraft(activeKey, '');
		sessions.clearDraftFiles(activeKey);
	}

	function removeMention(token: string): void {
		const index = draft.indexOf(token);
		if (index < 0) return;
		removeMentionAt(index, token.length);
	}

	function removeMentionAt(start: number, length: number): void {
		draft = draft.slice(0, start) + draft.slice(start + length);
		persistDraft();
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
		const next = queue[0];
		if (next === undefined) return;
		if (!onsend?.(next)) return;
		queue = queue.slice(1);
	}

	function onSubmit(): void {
		const text = draft.trim();
		if (!text && attachments.length === 0 && images.length === 0) {
			if (queue.length > 0 && !busy) sendNext();
			return;
		}
		const parsed = parseCommand(text);
		if (parsed) {
			clearEditor();
			oncommand?.(parsed.command.action, parsed.arg);
			return;
		}
		const message: OutgoingMessage = { text: draft, attachments, images };
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
		persistDraft();
	}

	function imageMimeAllowed(file: File): boolean {
		return isImageMimeAllowed(file.type, file.name, mediaLimits);
	}

	function persistFiles(): void {
		persistDraft();
	}

	async function addFiles(files: FileList | File[]): Promise<void> {
		for (const file of Array.from(files)) {
			if (file.type.startsWith('image/') || imageMimeAllowed(file)) {
				if (executionType === 'workflow') {
					toasts.error(
						'Remove staged images before running a workflow; switch to an agent turn to send them',
					);
					continue;
				}
				if (!imageMimeAllowed(file)) {
					toasts.error(
						`Unsupported image type: ${file.name || 'pasted image'} (use png, jpg, gif, webp)`,
					);
					continue;
				}
				stageImage(file);
				continue;
			}
			if (attachments.length >= mediaLimits.attachmentMaxCount) {
				toasts.error(
					`Too many attachments (max ${mediaLimits.attachmentMaxCount}); remove one first`,
				);
				continue;
			}
			const stagedBytes = attachments.reduce((total, item) => total + item.content.length, 0);
			if (stagedBytes >= mediaLimits.attachmentTotalLimit) {
				toasts.error('Attachments exceed the per-turn budget; remove one first');
				continue;
			}
			try {
				const text = await file.text();
				if (text.includes('\0')) {
					toasts.error(
						`Binary file skipped: ${file.name || 'pasted text'} (send images via the image button)`,
					);
					continue;
				}
				const truncated = text.length > mediaLimits.attachmentLimit;
				const originalChars = truncated ? text.length : undefined;
				attachments = [
					...attachments,
					{
						name: file.name || 'pasted-text',
						content: text.slice(0, mediaLimits.attachmentLimit),
						...(truncated ? { truncated: true, originalChars } : {}),
					},
				];
				persistFiles();
			} catch {
				toasts.error(
					t('chat.composer.readFailed', {
						name: file.name || t('chat.composer.attachment'),
					}),
				);
			}
		}
	}

	/** Stage one image file: type-checked, size-capped, previewable. */
	function stageImage(file: File): void {
		const stagedBytes = stagedDecodedBytes(images);
		const rejection = preflightImage(
			{
				mime: file.type,
				name: file.name,
				size: file.size,
				stagedCount: images.length,
				stagedBytes,
			},
			mediaLimits,
		);
		if (rejection === 'unsupported-type') {
			toasts.error(
				`Unsupported image type: ${file.name || 'pasted image'} (use png, jpg, gif, webp)`,
			);
			return;
		}
		if (rejection === 'too-many') {
			toasts.error(`Too many images (max ${mediaLimits.maxImagesPerTurn}); remove one first`);
			return;
		}
		if (rejection === 'too-large') {
			toasts.error(
				t('chat.composer.imageTooLarge', {
					name: file.name || t('chat.composer.attachment'),
				}),
			);
			return;
		}
		if (rejection === 'total-too-large') {
			toasts.error(t('chat.composer.imagesTooLarge'));
			return;
		}
		const reader = new FileReader();
		reader.onload = () => {
			const dataUrl = typeof reader.result === 'string' ? reader.result : '';
			if (!dataUrl) {
				toasts.error(
					t('chat.composer.readFailed', {
						name: file.name || t('chat.composer.attachment'),
					}),
				);
				return;
			}
			// A concurrent read may have filled the budget while this one was in
			// flight; re-check decoded bytes before staging.
			if (stagedDecodedBytes(images) + file.size > mediaLimits.imageTotalLimit) {
				toasts.error(t('chat.composer.imagesTooLarge'));
				return;
			}
			const mime = imageExtMime(file.type, file.name);
			images = [
				...images,
				{
					name: file.name || 'pasted-image',
					dataUrl,
					...(mime ? { mediaType: mime } : {}),
					...(imageDetail ? { detail: imageDetail } : {}),
				},
			];
			persistFiles();
		};
		reader.onerror = () => {
			toasts.error(
				t('chat.composer.readFailed', {
					name: file.name || t('chat.composer.attachment'),
				}),
			);
		};
		reader.readAsDataURL(file);
	}

	function triggerAttachPicker(): void {
		attachInput?.click();
	}

	function triggerImagePicker(): void {
		imageInput?.click();
	}

	function onAttachInput(event: Event): void {
		const input = event.target as HTMLInputElement | null;
		if (input?.files && input.files.length > 0) void addFiles(input.files);
		if (input) input.value = '';
	}

	function onImageInput(event: Event): void {
		const input = event.target as HTMLInputElement | null;
		if (input?.files && input.files.length > 0) void addFiles(input.files);
		if (input) input.value = '';
	}

	const stagedBudget = $derived.by(() => {
		const imageBytes = stagedDecodedBytes(images);
		const attachmentBytes = attachments.reduce((total, item) => total + item.content.length, 0);
		return {
			imageBytes,
			attachmentBytes,
			imageCount: images.length,
			attachmentCount: attachments.length,
			imageRemaining: Math.max(0, mediaLimits.imageTotalLimit - imageBytes),
			attachmentRemaining: Math.max(0, mediaLimits.attachmentTotalLimit - attachmentBytes),
		};
	});

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
		agent: 4,
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
			const key = hit.kind === 'wf' ? 'workflow' : hit.kind === 'agent_loop' ? 'agent' : hit.kind;
			if (!seen.has(key)) {
				seen.add(key);
				groups.push({ type: key, items: [] });
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
	 * attachments and images return with it so nothing is lost. */
	function editQueued(index: number): void {
		const item = queue[index];
		if (!item) return;
		draft = draft.trim() ? `${draft}\n${item.text}` : item.text;
		attachments = [...attachments, ...item.attachments];
		images = [...images, ...(item.images ?? [])];
		queue = queue.filter((_, current) => current !== index);
		persistDraft();
		box?.focus();
	}
</script>

<svelte:window onkeydown={onGlobalKeydown} />

<div class="flex min-h-0 flex-1 flex-col gap-2">
	{#if showSuggestions && suggestions.length > 0}
		{#key draftKey}
			<StarterRow
				{suggestions}
				onSelect={(suggestion) => {
					draft = suggestion.content;
					onSuggestionSelect?.(suggestion);
					box?.focus();
				}}
			/>
		{/key}
	{/if}

	<ComposerQueue
		{queue}
		onmove={moveQueued}
		onedit={editQueued}
		onremove={(index) => (queue = queue.filter((_, i) => i !== index))}
	/>

	<ComposerAttachments
		{attachments}
		{images}
		onremove={(index) => {
			attachments = attachments.filter((_, i) => i !== index);
			persistFiles();
		}}
		onremoveImage={(index) => {
			images = images.filter((_, i) => i !== index);
			persistFiles();
		}}
	/>

	{#if executionType === 'workflow' && images.length > 0}
		<p class="text-micro text-warning">
			Images are not sent to workflows; remove them or switch back to an agent turn.
		</p>
	{/if}
	{#if stagedBudget.imageCount > 0 || stagedBudget.attachmentCount > 0}
		<p class="text-micro text-muted-foreground">
			Staged: {stagedBudget.imageCount}/{mediaLimits.maxImagesPerTurn} images · {stagedBudget.attachmentCount}/{mediaLimits.attachmentMaxCount}
			files · {(stagedBudget.imageRemaining / 1024).toFixed(0)} KB image budget left
		</p>
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

	<div
		class="flex flex-wrap items-center gap-2"
		role="toolbar"
		aria-label={t('chat.composer.ariaLabel')}
	>
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
		<IconButton
			icon="file"
			label={t('chat.composer.attachFiles')}
			onclick={triggerAttachPicker}
			compact
		/>
		<IconButton
			icon="image"
			label={t('chat.composer.attachImages')}
			onclick={triggerImagePicker}
			compact
			disabled={executionType === 'workflow'}
		/>
		<select
			aria-label={t('chat.composer.imageDetail')}
			value={imageDetail ?? ''}
			onchange={(event) =>
				setImageDetail((event.currentTarget.value || null) as ImageDetail | null)}
			disabled={executionType === 'workflow'}
			title="Render detail for newly staged images"
			class="shrink-0 rounded-md border border-input bg-card px-1.5 py-1 text-micro text-muted-foreground"
		>
			<option value="">Detail: default</option>
			<option value="auto">Detail: auto</option>
			<option value="low">Detail: low</option>
			<option value="high">Detail: high</option>
			<option value="original">Detail: original</option>
		</select>
		<input bind:this={attachInput} type="file" multiple class="hidden" onchange={onAttachInput} />
		<input
			bind:this={imageInput}
			type="file"
			multiple
			accept="image/png,image/jpeg,image/gif,image/webp"
			class="hidden"
			onchange={onImageInput}
		/>
		<span class="min-w-2 flex-1"></span>
		{#if onResetHeight}
			<IconButton
				icon="undo"
				label={t('chat.composer.resetHeight')}
				onclick={() => onResetHeight?.()}
				compact
			/>
		{/if}
		{#if onToggleFullscreen}
			<IconButton
				icon={fullscreen ? 'minimize' : 'maximize'}
				label={fullscreen ? t('chat.composer.collapseComposer') : t('chat.composer.expandComposer')}
				onclick={() => onToggleFullscreen?.()}
				compact
				active={fullscreen}
			/>
		{/if}
	</div>
	<textarea
		bind:this={box}
		bind:value={draft}
		oninput={onInput}
		onkeydown={onKey}
		onpaste={onPaste}
		ondrop={onDrop}
		ondragover={(event) => event.preventDefault()}
		rows={4}
		placeholder={t('chat.composer.placeholder')}
		aria-label={t('chat.composer.ariaLabel')}
		class={fullscreen
			? 'min-h-48 w-full flex-1 resize-none overflow-y-auto rounded-md border border-input bg-card px-2.5 py-2 text-body text-foreground placeholder:text-muted-foreground focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-ring'
			: grow
				? 'min-h-28 w-full flex-1 resize-none overflow-y-auto rounded-md border border-input bg-card px-2.5 py-2 text-body text-foreground placeholder:text-muted-foreground focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-ring'
				: 'max-h-[32vh] min-h-28 w-full resize-none overflow-y-auto rounded-md border border-input bg-card px-2.5 py-2 text-body text-foreground placeholder:text-muted-foreground focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-ring'}
	></textarea>
	<div class="flex flex-wrap items-center gap-2">
		<p class="min-w-0 flex-1 text-micro text-muted-foreground">
			{t('chat.composer.hintsEnter')}{#if busy}
				· {t('chat.composer.hintsQueue')}{/if} · {SLASH_COMMANDS.map(
				(command) => `/${command.name}`,
			).join(' ')} · {t('chat.composer.hintsMentions')} · {t('chat.composer.hintsAttach')}
		</p>
		{#if busy}
			<Button size="sm" variant="outline" onclick={() => onstop?.()}>
				<Icon name="square" size={13} />
				{t('chat.composer.stop')}
			</Button>
		{:else}
			<Button
				variant="brand"
				size="sm"
				disabled={!draft.trim() &&
					attachments.length === 0 &&
					images.length === 0 &&
					queue.length === 0}
				onclick={onSubmit}
			>
				<Icon name="arrow-up" size={14} />
				{queue.length > 0
					? t('chat.composer.sendQueued', {
							count:
								queue.length +
								(draft.trim() || attachments.length > 0 || images.length > 0 ? 1 : 0),
						})
					: t('chat.composer.send')}
			</Button>
		{/if}
	</div>
</div>
