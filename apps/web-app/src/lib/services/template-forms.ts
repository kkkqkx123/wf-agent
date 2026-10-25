import type { TemplateKind } from '#lib/types/models';

export interface TemplateFormField {
	key: string;
	label: string;
	required: boolean;
	multiline: boolean;
}

const NAME_FIELD: TemplateFormField = {
	key: 'name',
	label: 'Name',
	required: true,
	multiline: false,
};
const DESCRIPTION_FIELD: TemplateFormField = {
	key: 'description',
	label: 'Description',
	required: false,
	multiline: true,
};
const CATEGORY_FIELD: TemplateFormField = {
	key: 'category',
	label: 'Category',
	required: false,
	multiline: false,
};
const TAGS_FIELD: TemplateFormField = {
	key: 'tags',
	label: 'Tags (comma separated)',
	required: false,
	multiline: false,
};
const VERSION_FIELD: TemplateFormField = {
	key: 'version',
	label: 'Version',
	required: false,
	multiline: false,
};

/**
 * Structured fields per kind, matching the backend shapes: node metadata
 * carries no category/tags/version, trigger metadata carries category/tags
 * but no version, library definitions carry name/description/version while
 * their category/tags live on the outer template record. Library kinds
 * expose category/tags in the same field table; the edit session maps
 * those two keys to the outer record so they never land inside definition.
 */
export function templateFormFields(kind: TemplateKind): TemplateFormField[] {
	if (kind === 'node') {
		return [
			NAME_FIELD,
			{
				key: 'node_type',
				label: 'Node type',
				required: true,
				multiline: false,
			},
			DESCRIPTION_FIELD,
		];
	}
	if (kind === 'trigger') {
		return [
			NAME_FIELD,
			{
				key: 'trigger_type',
				label: 'Trigger type',
				required: true,
				multiline: false,
			},
			DESCRIPTION_FIELD,
			CATEGORY_FIELD,
			TAGS_FIELD,
		];
	}
	return [
		NAME_FIELD,
		DESCRIPTION_FIELD,
		VERSION_FIELD,
		CATEGORY_FIELD,
		TAGS_FIELD,
	];
}

function asRecord(value: unknown): Record<string, unknown> {
	return value && typeof value === 'object' && !Array.isArray(value)
		? (value as Record<string, unknown>)
		: {};
}

function fieldText(value: unknown): string {
	if (typeof value === 'string') return value;
	if (Array.isArray(value)) return value.map(String).join(', ');
	if (value === undefined || value === null) return '';
	return String(value);
}

/** Flatten a definition object into form strings. */
export function formFromDefinition(
	kind: TemplateKind,
	definition: unknown,
): Record<string, string> {
	const record = asRecord(definition);
	const form: Record<string, string> = {};
	for (const field of templateFormFields(kind)) {
		if (field.key === 'category') {
			form.category = fieldText(record.category);
		} else if (field.key === 'tags') {
			form.tags = fieldText(record.tags);
		} else {
			form[field.key] = fieldText(record[field.key]);
		}
	}
	return form;
}

/**
 * Merge form strings back into a definition object. Empty optional fields
 * are dropped so untouched keys keep their original shape.
 */
export function formToDefinition(
	kind: TemplateKind,
	form: Record<string, string>,
	base: unknown,
): Record<string, unknown> {
	const record = { ...asRecord(base) };
	for (const field of templateFormFields(kind)) {
		const text = (form[field.key] ?? '').trim();
		if (!text && !field.required) {
			delete record[field.key];
			continue;
		}
		if (field.key === 'tags') {
			record.tags = text
				.split(',')
				.map((tag) => tag.trim())
				.filter(Boolean);
		} else {
			record[field.key] = text;
		}
	}
	return record;
}

/**
 * Map a JSON syntax error to a 1-based line number within the source text.
 * V8 reports `position N`; other engines may only give a message.
 */
export function jsonErrorLine(text: string, message: string): number | null {
	const match = /position (\d+)/.exec(message);
	if (!match) return null;
	const position = Math.min(Number(match[1]), text.length);
	return text.slice(0, position).split('\n').length;
}

/**
 * Locate a dotted field path within JSON source for issue navigation. The
 * last path segment is matched as a JSON key first, then as plain text.
 */
export function jsonFieldLine(text: string, field: string): number | null {
	const segment = field.split('.').filter(Boolean).pop() ?? field;
	if (!segment) return null;
	const lines = text.split('\n');
	const keyIndex = lines.findIndex((line) => line.includes(`"${segment}"`));
	if (keyIndex >= 0) return keyIndex + 1;
	const rawIndex = lines.findIndex((line) => line.includes(segment));
	return rawIndex >= 0 ? rawIndex + 1 : null;
}
