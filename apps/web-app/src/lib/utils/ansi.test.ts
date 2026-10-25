import { describe, expect, it } from 'vitest';
import { ansiStyleToCss, hasAnsi, parseAnsi, stripAnsi } from './ansi';

const ESC = '\u001b';

describe('hasAnsi', () => {
	it('detects escape sequences', () => {
		expect(hasAnsi('plain text')).toBe(false);
		expect(hasAnsi(`${ESC}[31mred`)).toBe(true);
	});
});

describe('parseAnsi', () => {
	it('returns one plain run for unstyled text', () => {
		expect(parseAnsi('hello')).toEqual([
			{ text: 'hello', style: expect.objectContaining({ fg: null }) },
		]);
	});

	it('maps a basic foreground color to a theme variable', () => {
		const segments = parseAnsi(`${ESC}[31mred${ESC}[0m`);
		expect(segments[0].text).toBe('red');
		expect(segments[0].style.fg).toBe('hsl(var(--ansi-red))');
	});

	it('maps bright and 256-color indexes', () => {
		expect(parseAnsi(`${ESC}[91mx`)[0].style.fg).toBe('hsl(var(--ansi-bright-red))');
		expect(parseAnsi(`${ESC}[38;5;196mx`)[0].style.fg).toBe('rgb(255, 0, 0)');
		expect(parseAnsi(`${ESC}[38;5;232mx`)[0].style.fg).toBe('rgb(8, 8, 8)');
	});

	it('parses truecolor foreground and background', () => {
		const style = parseAnsi(`${ESC}[38;2;10;20;30mx`)[0].style;
		expect(style.fg).toBe('rgb(10, 20, 30)');
		expect(parseAnsi(`${ESC}[48;2;1;2;3mx`)[0].style.bg).toBe('rgb(1, 2, 3)');
	});

	it('tracks attribute toggles and reset', () => {
		const [bold] = parseAnsi(`${ESC}[1mbold`);
		expect(bold.style.bold).toBe(true);
		const segments = parseAnsi(`${ESC}[1m${ESC}[22mnormal`);
		expect(segments[0].style.bold).toBe(false);
	});

	it('strips non-SGR CSI and OSC sequences but keeps text', () => {
		expect(stripAnsi(`a${ESC}[2Kb`)).toBe('ab');
		expect(stripAnsi(`${ESC}]0;title\u0007body`)).toBe('body');
		expect(stripAnsi(`${ESC}]8;;http://x${ESC}\\link`)).toBe('link');
	});
});

describe('ansiStyleToCss', () => {
	it('emits color declarations and reverse swap', () => {
		const css = ansiStyleToCss({
			fg: 'rgb(1, 2, 3)',
			bg: null,
			bold: true,
			dim: false,
			italic: false,
			underline: true,
			strike: false,
			reverse: false,
		});
		expect(css).toContain('color:rgb(1, 2, 3)');
		expect(css).toContain('font-weight:600');
		expect(css).toContain('text-decoration:underline');
	});

	it('swaps foreground and background under reverse', () => {
		const css = ansiStyleToCss({
			fg: 'rgb(255, 255, 255)',
			bg: 'rgb(0, 0, 0)',
			bold: false,
			dim: false,
			italic: false,
			underline: false,
			strike: false,
			reverse: true,
		});
		expect(css).toContain('color:rgb(0, 0, 0)');
		expect(css).toContain('background-color:rgb(255, 255, 255)');
	});
});
