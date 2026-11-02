import { browser } from '$app/env';
import type { ImageAttachment, MessageAttachment } from '#lib/types/models';

/** Draft key for a composer that has not produced a loop yet. */
export const NEW_SESSION = 'new';

const TITLES_KEY = 'wf-session-titles';
const RECENTS_KEY = 'wf-session-recents';
const DRAFTS_KEY = 'wf-session-drafts';
const DRAFT_ATTACHMENTS_KEY = 'wf-session-draft-attachments';
const DRAFT_IMAGES_KEY = 'wf-session-draft-images';
const TEMPORARY_BADGE_KEY = 'wf-temporary-badge-v1';

export const RECENT_LIMIT = 12;

/** Upper bound on retained temporary badges; oldest entries are evicted first. */
export const TEMPORARY_BADGE_LIMIT = 100;

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

function readTemporaryBadge(): Record<string, 'restricted' | 'ephemeral'> {
	if (!browser) return {};
	try {
		const raw = localStorage.getItem(TEMPORARY_BADGE_KEY);
		if (!raw) return {};
		const parsed = JSON.parse(raw) as Record<string, unknown>;
		const next: Record<string, 'restricted' | 'ephemeral'> = {};
		for (const [id, value] of Object.entries(parsed)) {
			if (value === 'restricted' || value === 'ephemeral') next[id] = value;
			if (Object.keys(next).length >= TEMPORARY_BADGE_LIMIT) break;
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
 * recently opened ids, unsent composer drafts and temporary-conversation
 * badges. Badges are local only by design: clearing storage or switching
 * devices drops them, and the checkpoint empty state falls back to the
 * generic copy instead of the ephemeral hint.
 */
export class SessionLocalState {
	private titles = $state<Record<string, string>>(readRecord(TITLES_KEY));
	private recents = $state<string[]>(readList(RECENTS_KEY));
	private temporaryBadges = $state<Record<string, 'restricted' | 'ephemeral'>>(
		readTemporaryBadge(),
	);
	private draftTexts = $state<Record<string, string>>(readRecord(DRAFTS_KEY));
	private draftFiles = $state<Record<string, MessageAttachment[]>>(
		readFileRecord(DRAFT_ATTACHMENTS_KEY),
	);
	private draftImageMap = $state<Record<string, ImageAttachment[]>>(
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
					(this.draftImageMap[id]?.length ?? 0) > 0),
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

	/** Local temporary badge of one session; null when full or unknown. */
	temporaryBadge(id: string): 'restricted' | 'ephemeral' | null {
		return this.temporaryBadges[id] ?? null;
	}

	markTemporary(id: string, mode: 'restricted' | 'ephemeral'): void {
		if (this.temporaryBadges[id] === mode) return;
		const next = { ...this.temporaryBadges };
		delete next[id];
		next[id] = mode;
		while (Object.keys(next).length > TEMPORARY_BADGE_LIMIT) {
			const oldest = Object.keys(next)[0];
			if (oldest === undefined) break;
			delete next[oldest];
		}
		this.temporaryBadges = next;
		write(TEMPORARY_BADGE_KEY, next);
	}

	clearTemporary(id: string): void {
		if (!(id in this.temporaryBadges)) return;
		const next = { ...this.temporaryBadges };
		delete next[id];
		this.temporaryBadges = next;
		write(TEMPORARY_BADGE_KEY, next);
	}

	/** Drop badges whose sessions no longer exist; keeps the map bounded. */
	pruneTemporary(keepIds: string[]): void {
		if (Object.keys(this.temporaryBadges).length === 0) return;
		const next: Record<string, 'restricted' | 'ephemeral'> = {};
		for (const [id, mode] of Object.entries(this.temporaryBadges)) {
			if (keepIds.includes(id)) next[id] = mode;
		}
		if (Object.keys(next).length === Object.keys(this.temporaryBadges).length) return;
		this.temporaryBadges = next;
		write(TEMPORARY_BADGE_KEY, next);
	}

	draft(id: string): string {
		return this.draftTexts[id] ?? '';
	}

	setDraft(id: string, text: string): void {
		const next = { ...this.draftTexts };
		if (text.trim()) next[id] = text;
		else if ((this.draftFiles[id]?.length ?? 0) === 0 && (this.draftImageMap[id]?.length ?? 0) === 0)
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
		return this.draftImageMap[id] ?? [];
	}

	setDraftImages(id: string, images: ImageAttachment[]): void {
		const next = { ...this.draftImageMap };
		if (images.length > 0) next[id] = images;
		else delete next[id];
		this.draftImageMap = next;
		write(DRAFT_IMAGES_KEY, next);
	}

	clearDraftFiles(id: string): void {
		const files = { ...this.draftFiles };
		delete files[id];
		this.draftFiles = files;
		write(DRAFT_ATTACHMENTS_KEY, files);
		const images = { ...this.draftImageMap };
		delete images[id];
		this.draftImageMap = images;
		write(DRAFT_IMAGES_KEY, images);
	}
}
