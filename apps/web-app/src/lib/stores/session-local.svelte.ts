import { browser } from '$app/environment';

/** Draft key for a composer that has not produced a loop yet. */
export const NEW_SESSION = 'new';

const TITLES_KEY = 'wf-session-titles';
const RECENTS_KEY = 'wf-session-recents';
const DRAFTS_KEY = 'wf-session-drafts';

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

	get recentIds(): string[] {
		return this.recents;
	}

	get draftIds(): string[] {
		return Object.keys(this.draftTexts).filter(
			(id) => id !== NEW_SESSION && this.draftTexts[id].trim() !== '',
		);
	}

	label(id: string): string {
		return this.titles[id] ?? id;
	}

	/** First user message names the session; a recorded title is never rewritten. */
	recordTitle(id: string, content: string): void {
		if (this.titles[id]) return;
		const line = content.trim().split('\n', 1)[0];
		if (!line) return;
		this.titles = { ...this.titles, [id]: line.slice(0, 60) };
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
		else delete next[id];
		this.draftTexts = next;
		write(DRAFTS_KEY, next);
	}
}
