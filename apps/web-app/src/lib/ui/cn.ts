type ClassValue =
	| string
	| number
	| null
	| undefined
	| false
	| ClassValue[]
	| Record<string, boolean | null | undefined>;

const FONT_SIZE_TOKENS = new Set([
	'heading',
	'title',
	'body',
	'caption',
	'small',
	'micro',
	'xs',
	'sm',
	'base',
	'lg',
	'xl',
	'2xl',
	'3xl',
	'4xl',
	'5xl',
	'6xl',
	'7xl',
	'8xl',
	'9xl',
]);

const FONT_FAMILY_TOKENS = new Set(['sans', 'mono', 'serif']);

const DISPLAY_TOKENS = new Set([
	'block',
	'inline-block',
	'inline',
	'flex',
	'inline-flex',
	'table',
	'inline-table',
	'table-caption',
	'table-cell',
	'table-column',
	'table-row',
	'flow-root',
	'grid',
	'inline-grid',
	'contents',
	'list-item',
	'hidden',
]);

const POSITION_TOKENS = new Set(['static', 'fixed', 'absolute', 'relative', 'sticky']);

const FLEX_DIRECTION_TOKENS = new Set(['row', 'row-reverse', 'col', 'col-reverse']);

const FLEX_WRAP_TOKENS = new Set(['wrap', 'wrap-reverse', 'nowrap']);

const FLEX_SHORTHAND_TOKENS = new Set(['1', 'auto', 'initial', 'none']);

const OUTLINE_STYLE_TOKENS = new Set(['none', 'solid', 'dashed', 'dotted', 'double', 'hidden']);

function isArbitraryColor(inner: string): boolean {
	const v = inner.trim();
	return /^(hsl|rgb|rgba|oklch|oklab|lab|lch|color-mix|#|var\(--|current|transparent|inherit)/i.test(
		v,
	);
}

function isArbitraryLength(inner: string): boolean {
	const v = inner.trim();
	return (
		/^[\d.]+(rem|px|em|cap|ch|ex|ic|lh|rlh|vw|vh|vi|vb|vmin|vmax|svw|svh|dvw|dvh|cqi|cqb|cqx|cqw|cqh|cm|mm|in|pt|pc|q|%)(\/.*)?$/i.test(
			v,
		) || /^[\d.]+$/.test(v)
	);
}

function textGroup(tail: string): string {
	const body = tail.slice(5);
	if (body.startsWith('[')) {
		const inner = body.slice(1, body.endsWith(']') ? -1 : undefined);
		if (isArbitraryColor(inner)) return 'text-color';
		if (isArbitraryLength(inner)) return 'text-size';
		return `text-arbitrary:${tail}`;
	}
	const core = body.split('/')[0];
	if (FONT_SIZE_TOKENS.has(core)) return 'text-size';
	return 'text-color';
}

function borderGroup(tail: string): string {
	if (tail === 'border') return 'border-width-all';
	const rest = tail.slice('border-'.length);
	const core = rest.split('/')[0];
	if (/^(0|2|4|8)$/.test(core)) return 'border-width-all';
	const dirMatch = /^(x|y|t|b|l|r|s|e)(-(0|2|4|8))?$/.exec(core);
	if (dirMatch) return `border-width-${dirMatch[1]}`;
	const colorDirMatch = /^(x|y|t|b|l|r|s|e)-(.+)$/.exec(core);
	if (colorDirMatch) return `border-color-${colorDirMatch[1]}`;
	return 'border-color';
}

function divideGroup(tail: string): string {
	if (tail === 'divide') return 'divide-width';
	const rest = tail.slice('divide-'.length).split('/')[0];
	if (/^(0|2|4|8|x|y|x-0|x-2|x-4|x-8|y-0|y-2|y-4|y-8)$/.test(rest)) {
		return rest.startsWith('x')
			? 'divide-width-x'
			: rest.startsWith('y')
				? 'divide-width-y'
				: 'divide-width';
	}
	return 'divide-color';
}

function ringGroup(tail: string): string {
	if (tail === 'ring' || tail === 'ring-inset') return 'ring-width';
	if (tail.startsWith('ring-offset-')) return 'ring-offset';
	const rest = tail.slice('ring-'.length).split('/')[0];
	if (/^(0|1|2|4|8)$/.test(rest)) return 'ring-width';
	if (rest.startsWith('[')) {
		const inner = rest.slice(1, rest.endsWith(']') ? -1 : undefined);
		if (isArbitraryColor(inner)) return 'ring-color';
		return 'ring-width';
	}
	return 'ring-color';
}

function outlineGroup(tail: string): string {
	if (tail === 'outline') return 'outline-width';
	if (tail.startsWith('outline-offset-')) return 'outline-offset';
	const rest = tail.slice('outline-'.length).split('/')[0];
	if (/^(0|1|2|4|8)$/.test(rest)) return 'outline-width';
	if (OUTLINE_STYLE_TOKENS.has(rest)) return 'outline-style';
	if (rest.startsWith('[')) {
		const inner = rest.slice(1, rest.endsWith(']') ? -1 : undefined);
		if (isArbitraryColor(inner)) return 'outline-color';
		if (isArbitraryLength(inner)) return 'outline-width';
		return `outline-arbitrary:${tail}`;
	}
	return 'outline-color';
}

function fontGroup(tail: string): string {
	const rest = tail.slice('font-'.length).split('/')[0];
	if (FONT_FAMILY_TOKENS.has(rest)) return 'font-family';
	return 'font-weight';
}

function tailGroup(tail: string): string {
	if (tail.startsWith('text-')) return textGroup(tail);
	if (tail === 'border' || tail.startsWith('border-')) return borderGroup(tail);
	if (tail === 'divide' || tail.startsWith('divide-')) return divideGroup(tail);
	if (tail === 'ring' || tail.startsWith('ring-')) return ringGroup(tail);
	if (tail === 'outline' || tail.startsWith('outline-')) return outlineGroup(tail);
	if (tail.startsWith('font-')) return fontGroup(tail);
	if (DISPLAY_TOKENS.has(tail)) return 'display';
	if (POSITION_TOKENS.has(tail)) return 'position';
	if (tail.startsWith('flex-')) {
		const rest = tail.slice(5);
		if (FLEX_DIRECTION_TOKENS.has(rest)) return 'flex-direction';
		if (FLEX_WRAP_TOKENS.has(rest)) return 'flex-wrap';
		if (FLEX_SHORTHAND_TOKENS.has(rest)) return 'flex';
		return 'flex-misc';
	}
	if (tail.startsWith('grid-cols-')) return 'grid-cols';
	if (tail.startsWith('grid-rows-')) return 'grid-rows';
	if (tail.startsWith('min-w-') || tail === 'min-w') return 'min-w';
	if (tail.startsWith('max-w-') || tail === 'max-w') return 'max-w';
	if (tail.startsWith('min-h-') || tail === 'min-h') return 'min-h';
	if (tail.startsWith('max-h-') || tail === 'max-h') return 'max-h';
	if (tail.startsWith('gap-x-') || tail === 'gap-x') return 'gap-x';
	if (tail.startsWith('gap-y-') || tail === 'gap-y') return 'gap-y';
	if (tail.startsWith('gap-') || tail === 'gap') return 'gap';
	if (tail.startsWith('space-x-')) return 'space-x';
	if (tail.startsWith('space-y-')) return 'space-y';
	if (tail.startsWith('inset-x-') || tail === 'inset-x') return 'inset-x';
	if (tail.startsWith('inset-y-') || tail === 'inset-y') return 'inset-y';
	if (tail.startsWith('overflow-x-')) return 'overflow-x';
	if (tail.startsWith('overflow-y-')) return 'overflow-y';
	if (
		tail === 'underline' ||
		tail === 'overline' ||
		tail === 'line-through' ||
		tail === 'no-underline'
	) {
		return 'text-decoration';
	}
	if (tail.startsWith('underline-offset-')) return 'underline-offset';
	if (
		tail.startsWith('rounded-tl-') ||
		tail.startsWith('rounded-tr-') ||
		tail.startsWith('rounded-bl-') ||
		tail.startsWith('rounded-br-')
	) {
		return tail.slice(0, 'rounded-xx'.length);
	}
	if (tail.startsWith('rounded-t-') || tail === 'rounded-t') return 'rounded-t';
	if (tail.startsWith('rounded-b-') || tail === 'rounded-b') return 'rounded-b';
	if (tail.startsWith('rounded-l-') || tail === 'rounded-l') return 'rounded-l';
	if (tail.startsWith('rounded-r-') || tail === 'rounded-r') return 'rounded-r';
	if (tail.startsWith('rounded-s-') || tail === 'rounded-s') return 'rounded-s';
	if (tail.startsWith('rounded-e-') || tail === 'rounded-e') return 'rounded-e';
	const dash = tail.indexOf('-');
	return dash === -1 ? tail : tail.slice(0, dash);
}

/**
 * Joins conditional class names and keeps the last declaration of any
 * duplicated utility group. Duplicate groups are collapsed from the right so
 * caller overrides win over component defaults.
 */
export function cn(...values: ClassValue[]): string {
	const out: string[] = [];

	const walk = (value: ClassValue): void => {
		if (!value && value !== 0) return;
		if (typeof value === 'string' || typeof value === 'number') {
			const text = String(value).trim();
			if (text) out.push(text);
			return;
		}
		if (Array.isArray(value)) {
			value.forEach(walk);
			return;
		}
		for (const [key, enabled] of Object.entries(value)) {
			if (enabled) walk(key);
		}
	};

	values.forEach(walk);

	const seen = new Map<string, string>();
	for (const group of out.join(' ').split(/\s+/)) {
		if (!group) continue;
		const variant = group.split(':');
		const tail = variant[variant.length - 1];
		const base = tailGroup(tail);
		// Variant prefixes stay part of the key so `hover:bg-card` never
		// collapses into `bg-card`.
		const prefix = variant.slice(0, -1).join(':');
		seen.set(prefix ? `${prefix}|${base}` : base, group);
	}

	return [...seen.values()].join(' ');
}
