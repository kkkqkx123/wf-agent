import { browser } from '$app/env';
import type { ImageAttachment, MessageAttachment } from '#lib/types/models';

/** Draft key for a composer that has not produced a loop yet. */
export const NEW_SESSION = 'new';

const TITLES_KEY = 'wf-session-titles';
const RECENTS_KEY = 'wf-session-recents';
const DRAFTS_KEY = 'wf-session-drafts';
const DRAFT_ATTACHMENTS_KEY = 'wf-session-draft-attachments';
const DRAFT_IMAGES_KEY = 'wf-session-draft-images';

export const RECENT_LIMIT = 12;

function readRecord(key: string): Record<string, string> {
	if (!browser) return {};
	try {
		const raw = localStorage.getItem(key);
		return raw ? (JSON.parse(raw) as Record<string, string>) : {};
	} catch {
		return {};
	}
}

function readList(key: string): string[] {
	if (!browser) return [];
	try {
		const raw = localStorage.getItem(key);
		return raw ? (JSON.parse(raw) as string[]) : [];
	} catch {
		return [];
	}
}

function readFileRecord(key: string): Record<string, MessageAttachment[]> {
	if (!browser) return {};
	try {
		const raw = localStorage.getItem(key);
		const parsed = raw ? (JSON.parse(raw) as Record<string, unknown>) : {};
		const next: Record<string, MessageAttachment[]> = {};
		for (const [id, value] of Object.entries(parsed)) {
			if (!Array.isArray(value)) continue;
			const files = (value as unknown[]).filter(
				(item): item is MessageAttachment =>
					typeof item === 'object' &&
					item !== null &&
					typeof (item as { name?: unknown }).name === 'string' &&
					typeof (item as { content?: unknown }).content === 'string',
			);
			if (files.length > 0) next[id] = files;
		}
		return next;
	} catch {
		return {};
	}
}

function readImageRecord(key: string): Record<string, ImageAttachment[]> {
	if (!browser) return {};
	try {
		const raw = localStorage.getItem(key);
		const parsed = raw ? (JSON.parse(raw) as Record<string, unknown>) : {};
		const next: Record<string, ImageAttachment[]> = {};
		for (const [id, value] of Object.entries(parsed)) {
			if (!Array.isArray(value)) continue;
			const images = (value as unknown[]).filter(
				(item): item is ImageAttachment =>
					typeof item === 'object' &&
					item !== null &&
					typeof (item as { name?: unknown }).name === 'string' &&
					typeof (item as { dataUrl?: unknown }).dataUrl === 'string',
			);
			if (images.length > 0) next[id] = images;
		}
		return next;
	} catch {
		return {};
	}
}

function write(key: string, value: unknown): void {
	if (!browser) return;
	try {
		localStorage.setItem(key, JSON.stringify(value));
	} catch {
		// Storage may be unavailable; the store still works in memory.
	}
}

/**
 * Local session conventions the backend does not model: display titles,
 * recently opened ids and unsent composer drafts.
 */
export class SessionLocalState {
	private titles = $state<Record<string, string>>(readRecord(TITLES_KEY));
	private recents = $state<string[]>(readList(RECENTS_KEY));
	private draftTexts = $state<Record<string, string>>(readRecord(DRAFTS_KEY));
	private draftFiles = $state<Record<string, MessageAttachment[]>>(
		readFileRecord(DRAFT_ATTACHMENTS_KEY),
	);
	private draftImages = $state<Record<string, ImageAttachment[]>>(
		readImageRecord(DRAFT_IMAGES_KEY),
	);

	get recentIds(): string[] {
		return this.recents;
	}

	get draftIds(): string[] {
		return Object.keys(this.draftTexts).filter(
			(id) =>
				id !== NEW_SESSION &&
				(this.draftTexts[id].trim() !== '' ||
					(this.draftFiles[id]?.length ?? 0) > 0 ||
					(this.draftImages[id]?.length ?? 0) > 0),
		);
	}

	label(id: string): string {
		return this.titles[id] ?? id;
	}

	/** Whether a display title was recorded for the session. */
	hasTitle(id: string): boolean {
		return this.titles[id] !== undefined;
	}

	/** First user message names the session; a recorded title is never rewritten. */
	recordTitle(id: string, content: string): void {
		if (this.titles[id]) return;
		const line = content.trim().split('\n', 1)[0];
		if (!line) return;
		this.titles = { ...this.titles, [id]: line.slice(0, 60) };
		write(TITLES_KEY, this.titles);
	}

	/**
	 * Apply a server-confirmed title (LLM generation or manual rename).
	 * Unlike the truncation placeholder this overwrites, so a late LLM
	 * answer can upgrade the first-message label.
	 */
	applyTitle(id: string, title: string): void {
		const line = title.trim().split('\n', 1)[0] ?? '';
		if (!line) return;
		this.titles = { ...this.titles, [id]: line.slice(0, 80) };
		write(TITLES_KEY, this.titles);
	}

	touch(id: string): void {
		const next = [id, ...this.recents.filter((entry) => entry !== id)];
		this.recents = next.slice(0, RECENT_LIMIT);
		write(RECENTS_KEY, this.recents);
	}

	draft(id: string): string {
		return this.draftTexts[id] ?? '';
	}

	setDraft(id: string, text: string): void {
		const next = { ...this.draftTexts };
		if (text.trim()) next[id] = text;
		else if ((this.draftFiles[id]?.length ?? 0) === 0 && (this.draftImages[id]?.length ?? 0) === 0)
			delete next[id];
		else next[id] = text;
		this.draftTexts = next;
		write(DRAFTS_KEY, next);
	}

	draftAttachments(id: string): MessageAttachment[] {
		return this.draftFiles[id] ?? [];
	}

	setDraftAttachments(id: string, files: MessageAttachment[]): void {
		const next = { ...this.draftFiles };
		if (files.length > 0) next[id] = files;
		else delete next[id];
		this.draftFiles = next;
		write(DRAFT_ATTACHMENTS_KEY, next);
	}

	draftImages(id: string): ImageAttachment[] {
		return this.draftImages[id] ?? [];
	}

	setDraftImages(id: string, images: ImageAttachment[]): void {
		const next = { ...this.draftImages };
		if (images.length > 0) next[id] = images;
		else delete next[id];
		this.draftImages = next;
		write(DRAFT_IMAGES_KEY, next);
	}

	clearDraftFiles(id: string): void {
		const files = { ...this.draftFiles };
		delete files[id];
		this.draftFiles = files;
		write(DRAFT_ATTACHMENTS_KEY, files);
		const images = { ...this.draftImages };
		delete images[id];
		this.draftImages = images;
		write(DRAFT_IMAGES_KEY, images);
	}
}
