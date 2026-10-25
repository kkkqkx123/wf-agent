import type { DisplayEdge, DisplayNode } from '#lib/graph/display-model';
import { formFromDefinition, formToDefinition } from '#lib/services/template-forms';
import {
	mergeTemplateGraph,
	parseTemplateTopology,
	templateEditTarget,
	type TemplateTopology,
} from '#lib/services/template-topology';
import type { TemplateKind } from '#lib/types/models';

function isLibraryKind(kind: TemplateKind): boolean {
	return kind !== 'node' && kind !== 'trigger';
}

function cloneStructured<T>(value: T): T {
	if (value === undefined || value === null) return value;
	try {
		return JSON.parse(JSON.stringify(value)) as T;
	} catch {
		return value;
	}
}

function sessionTextFor(kind: TemplateKind, full: unknown): string {
	return JSON.stringify(templateEditTarget(kind, full) ?? null, null, 2);
}

function emptySessionFull(kind: TemplateKind): unknown {
	return isLibraryKind(kind) ? { definition: {} } : {};
}

function fieldText(value: unknown): string {
	if (typeof value === 'string') return value;
	if (Array.isArray(value)) return value.map(String).join(', ');
	if (value === undefined || value === null) return '';
	return String(value);
}

function asRecord(value: unknown): Record<string, unknown> {
	return value && typeof value === 'object' && !Array.isArray(value)
		? (value as Record<string, unknown>)
		: {};
}

/**
 * Structured edit session shared by the template drawer tabs. The last
 * valid document is the single source of truth; the JSON tab only edits a
 * text projection of it. Failed parses keep the previous document so the
 * form and canvas never clear, and graph conflicts compare a snapshot
 * revision instead of raw text equality.
 */
export class TemplateEditSession {
	private kind: TemplateKind = 'node';
	private full: unknown = {};
	private textValue = '';
	private syntaxMessage: string | null = null;
	private revision = 0;
	private graphLoadedAt = -1;
	private baseline = '';

	get text(): string {
		return this.textValue;
	}

	get syntaxError(): string | null {
		return this.syntaxMessage;
	}

	/** Whether the text projection drifted from the last clean baseline. */
	get textDirty(): boolean {
		return this.textValue !== this.baseline;
	}

	load(kind: TemplateKind, raw: unknown): void {
		this.kind = kind;
		const base = raw ?? emptySessionFull(kind);
		this.full = cloneStructured(base);
		if (this.full === null || this.full === undefined) {
			this.full = emptySessionFull(kind);
		}
		this.textValue = sessionTextFor(kind, this.full);
		this.syntaxMessage = null;
		this.revision = 0;
		this.graphLoadedAt = -1;
		this.baseline = this.textValue;
	}

	/** Record the current text as clean after a save or discard. */
	markClean(): void {
		this.baseline = this.textValue;
	}

	loadEmpty(kind: TemplateKind): void {
		this.load(kind, emptySessionFull(kind));
	}

	setKind(kind: TemplateKind): void {
		if (kind === this.kind) return;
		this.kind = kind;
		try {
			const parsed: unknown = JSON.parse(this.textValue);
			this.syntaxMessage = null;
			if (isLibraryKind(kind)) {
				this.full = { ...asRecord(this.full), definition: parsed };
			} else {
				this.full = parsed;
			}
			this.revision += 1;
		} catch {
			this.full = emptySessionFull(kind);
			this.revision += 1;
		}
	}

	applyText(next: string): void {
		this.textValue = next;
		let parsed: unknown;
		try {
			parsed = JSON.parse(next);
		} catch (e) {
			this.syntaxMessage = e instanceof Error ? e.message : 'Invalid JSON';
			return;
		}
		if (isLibraryKind(this.kind)) {
			const current = templateEditTarget(this.kind, this.full);
			if (JSON.stringify(parsed) === JSON.stringify(current ?? null)) {
				this.syntaxMessage = null;
				return;
			}
			this.full = { ...asRecord(this.full), definition: parsed };
		} else {
			if (JSON.stringify(parsed) === JSON.stringify(this.full ?? null)) {
				this.syntaxMessage = null;
				return;
			}
			this.full = parsed;
		}
		this.syntaxMessage = null;
		this.revision += 1;
	}

	definition(): unknown {
		return templateEditTarget(this.kind, this.full);
	}

	fullValue(): unknown {
		return this.full;
	}

	formSnapshot(): Record<string, string> {
		const form = formFromDefinition(this.kind, templateEditTarget(this.kind, this.full));
		if (isLibraryKind(this.kind)) {
			const outer = asRecord(this.full);
			form.category = fieldText(outer.category);
			form.tags = fieldText(outer.tags);
		}
		return form;
	}

	applyForm(form: Record<string, string>): void {
		if (isLibraryKind(this.kind)) {
			const definition = asRecord(templateEditTarget(this.kind, this.full));
			const merged = formToDefinition(this.kind, form, definition);
			delete merged.category;
			delete merged.tags;
			const outer = { ...asRecord(this.full) };
			const category = (form.category ?? '').trim();
			if (category) {
				outer.category = category;
			} else {
				delete outer.category;
			}
			const tagsText = (form.tags ?? '').trim();
			if (tagsText) {
				outer.tags = tagsText
					.split(',')
					.map((tag) => tag.trim())
					.filter(Boolean);
			} else {
				delete outer.tags;
			}
			this.full = { ...outer, definition: merged };
		} else {
			this.full = formToDefinition(this.kind, form, templateEditTarget(this.kind, this.full) ?? {});
		}
		this.revision += 1;
		this.textValue = sessionTextFor(this.kind, this.full);
		this.syntaxMessage = null;
	}

	topology(): TemplateTopology {
		return parseTemplateTopology(this.full);
	}

	noteGraphLoaded(): void {
		this.graphLoadedAt = this.revision;
	}

	hasGraphConflict(graphDirty: boolean): boolean {
		return graphDirty && this.graphLoadedAt >= 0 && this.graphLoadedAt !== this.revision;
	}

	mergeGraph(nodes: DisplayNode[], edges: DisplayEdge[]): void {
		this.full = mergeTemplateGraph(this.full, nodes, edges);
		this.revision += 1;
		this.textValue = sessionTextFor(this.kind, this.full);
		this.syntaxMessage = null;
		this.graphLoadedAt = this.revision;
	}
}
