/**
 * Minimal ANSI SGR parser for tool output rendering.
 *
 * The engine streams shell / script output as a byte stream, so terminal
 * escape sequences can reach the browser. This parser turns SGR sequences
 * into structured style runs while stripping every other CSI and OSC
 * sequence but keeping the visible text between them. It mirrors the CLI
 * `ansi` parser: 16/256/truecolor foreground and background plus bold, dim,
 * italic, underline, strike and reverse.
 *
 * Base colors resolve to theme-driven CSS variables so the same run stays
 * readable under both light and dark themes; 256-color and truecolor values
 * become literal `rgb()` strings because they carry their own RGB.
 */

/** One parsed style run. Booleans mirror the SGR attribute set. */
export interface AnsiStyle {
	fg: string | null;
	bg: string | null;
	bold: boolean;
	dim: boolean;
	italic: boolean;
	underline: boolean;
	strike: boolean;
	reverse: boolean;
}

/** One contiguous run of text sharing a single style. */
export interface AnsiSegment {
	text: string;
	style: AnsiStyle;
}

const ESC = '\u001b';

/** Base 16 color names, low half first then bright half. */
const BASE_COLOR_NAMES = [
	'black',
	'red',
	'green',
	'yellow',
	'blue',
	'magenta',
	'cyan',
	'white',
] as const;

function defaultStyle(): AnsiStyle {
	return {
		fg: null,
		bg: null,
		bold: false,
		dim: false,
		italic: false,
		underline: false,
		strike: false,
		reverse: false,
	};
}

function colorCubeLevel(n: number): number {
	return n === 0 ? 0 : 55 + n * 40;
}

function ansiColorFromIndex(index: number): string | null {
	if (index < 0) return null;
	if (index < 8) return `hsl(var(--ansi-${BASE_COLOR_NAMES[index]}))`;
	if (index < 16) {
		return `hsl(var(--ansi-bright-${BASE_COLOR_NAMES[index - 8]}))`;
	}
	if (index < 232) {
		const n = index - 16;
		const r = colorCubeLevel(Math.floor(n / 36));
		const g = colorCubeLevel(Math.floor((n % 36) / 6));
		const b = colorCubeLevel(n % 6);
		return `rgb(${r}, ${g}, ${b})`;
	}
	const gray = 8 + (index - 232) * 10;
	return `rgb(${gray}, ${gray}, ${gray})`;
}

/** Apply one SGR parameter list to the running style. Parameters that
 * select a color consume their following arguments, so the loop index is
 * advanced in place. */
function applySgr(params: number[], style: AnsiStyle): void {
	for (let i = 0; i < params.length; i += 1) {
		const code = params[i];
		if (code === 0) {
			Object.assign(style, defaultStyle());
		} else if (code === 1) {
			style.bold = true;
		} else if (code === 2) {
			style.dim = true;
		} else if (code === 3) {
			style.italic = true;
		} else if (code === 4) {
			style.underline = true;
		} else if (code === 7) {
			style.reverse = true;
		} else if (code === 9) {
			style.strike = true;
		} else if (code === 22) {
			style.bold = false;
			style.dim = false;
		} else if (code === 23) {
			style.italic = false;
		} else if (code === 24) {
			style.underline = false;
		} else if (code === 27) {
			style.reverse = false;
		} else if (code === 29) {
			style.strike = false;
		} else if (code >= 30 && code <= 37) {
			style.fg = ansiColorFromIndex(code - 30);
		} else if (code === 39) {
			style.fg = null;
		} else if (code >= 40 && code <= 47) {
			style.bg = ansiColorFromIndex(code - 40);
		} else if (code === 49) {
			style.bg = null;
		} else if (code >= 90 && code <= 97) {
			style.fg = ansiColorFromIndex(code - 90 + 8);
		} else if (code >= 100 && code <= 107) {
			style.bg = ansiColorFromIndex(code - 100 + 8);
		} else if (code === 38 || code === 48) {
			const extended = code === 38 ? 'fg' : 'bg';
			const mode = params[i + 1];
			if (mode === 5 && params.length > i + 2) {
				style[extended] = ansiColorFromIndex(params[i + 2]);
				i += 2;
			} else if (mode === 2 && params.length > i + 4) {
				const [r, g, b] = [params[i + 2], params[i + 3], params[i + 4]];
				style[extended] = `rgb(${r}, ${g}, ${b})`;
				i += 4;
			}
		}
	}
}

/** Whether the text carries at least one escape sequence. */
export function hasAnsi(text: string): boolean {
	return text.includes(ESC);
}

/** Split styled text into runs, dropping non-SGR control sequences.
 * A string with no sequences yields a single plain run. */
export function parseAnsi(text: string): AnsiSegment[] {
	const segments: AnsiSegment[] = [];
	const style = defaultStyle();
	let buffer = '';

	const flush = (): void => {
		if (buffer === '') return;
		segments.push({ text: buffer, style: { ...style } });
		buffer = '';
	};

	let i = 0;
	while (i < text.length) {
		const ch = text[i];
		if (ch !== ESC) {
			buffer += ch;
			i += 1;
			continue;
		}
		const next = text[i + 1];
		if (next === '[') {
			let end = i + 2;
			while (end < text.length) {
				const code = text.charCodeAt(end);
				if (code >= 0x40 && code <= 0x7e) break;
				end += 1;
			}
			const final = text[end];
			if (final === 'm') {
				const body = text.slice(i + 2, end);
				const params = body
					.split(';')
					.map((part) => (part === '' ? 0 : Number.parseInt(part, 10)))
					.map((value) => (Number.isNaN(value) ? 0 : value));
				flush();
				applySgr(params, style);
			}
			i = end < text.length ? end + 1 : text.length;
			continue;
		}
		if (next === ']') {
			let end = i + 2;
			while (end < text.length) {
				if (text[end] === '\u0007') break;
				if (text[end] === ESC && text[end + 1] === '\\') {
					end += 1;
					break;
				}
				end += 1;
			}
			i = end < text.length ? end + 1 : text.length;
			continue;
		}
		// Lone escape or two-character sequence: drop it, keep the text.
		i += next === undefined ? 1 : 2;
	}
	flush();
	return segments;
}

/** Visible text with every escape sequence removed. */
export function stripAnsi(text: string): string {
	return parseAnsi(text)
		.map((segment) => segment.text)
		.join('');
}

/** Build the CSS declarations for one style run. Under `reverse` the
 * foreground and background swap, matching terminal behavior. */
export function ansiStyleToCss(style: AnsiStyle): string {
	const fg = style.reverse ? style.bg : style.fg;
	const bg = style.reverse ? style.fg : style.bg;
	const declarations: string[] = [];
	if (fg) declarations.push(`color:${fg}`);
	if (bg) declarations.push(`background-color:${bg}`);
	if (style.bold) declarations.push('font-weight:600');
	if (style.dim) declarations.push('opacity:0.7');
	if (style.italic) declarations.push('font-style:italic');
	const decoration: string[] = [];
	if (style.underline) decoration.push('underline');
	if (style.strike) decoration.push('line-through');
	if (decoration.length > 0) {
		declarations.push(`text-decoration:${decoration.join(' ')}`);
	}
	return declarations.join(';');
}
