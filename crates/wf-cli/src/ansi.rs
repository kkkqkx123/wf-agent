//! Minimal ANSI escape → ratatui [`Line`] pipeline (`docs/cli/03` §2.3).
//!
//! We self-host a small SGR subset instead of pulling in the external
//! `ansi-to-tui` crate (deviation P1): the CLI's tool output mainly uses
//! 16/256-color and truecolor foreground/background, bold, dim, italic,
//! underline and reverse. Everything else — other CSI sequences and OSC —
//! is stripped. Tabs expand to 4 spaces and `\n` splits rows, matching how
//! a real terminal would break the byte stream into visible lines.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

/// Current SGR style accumulator while walking the byte stream.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SgrState {
    pub fg: Option<Color>,
    pub bg: Option<Color>,
    pub bold: bool,
    pub dimmed: bool,
    pub italic: bool,
    pub underline: bool,
    pub reversed: bool,
}

impl SgrState {
    /// Reset every attribute / color to the terminal default.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Materialize the ratatui [`Style`] for this state.
    pub fn style(&self) -> Style {
        let mut s = Style::default();
        if let Some(fg) = self.fg {
            s = s.fg(fg);
        }
        if let Some(bg) = self.bg {
            s = s.bg(bg);
        }
        if self.bold {
            s = s.add_modifier(Modifier::BOLD);
        }
        if self.dimmed {
            s = s.add_modifier(Modifier::DIM);
        }
        if self.italic {
            s = s.add_modifier(Modifier::ITALIC);
        }
        if self.underline {
            s = s.add_modifier(Modifier::UNDERLINED);
        }
        if self.reversed {
            s = s.add_modifier(Modifier::REVERSED);
        }
        s
    }
}

/// Byte/char scanner that converts an ANSI byte stream into styled lines.
#[derive(Debug, Clone, Default)]
pub struct AnsiParser {
    state: SgrState,
}

impl AnsiParser {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_state(state: SgrState) -> Self {
        Self { state }
    }

    pub fn state(&self) -> SgrState {
        self.state
    }

    /// Parse a byte slice into [`Line`]s. The text is decoded lossy (so a
    /// trailing partial UTF-8 multi-byte sequence degrades instead of
    /// panicking); SGR updates the running style, other escapes are
    /// stripped, tabs become 4 spaces and `\n` ends a row.
    pub fn parse(mut self, input: &[u8]) -> Vec<Line<'static>> {
        let text = String::from_utf8_lossy(input);
        let chars: Vec<char> = text.chars().collect();
        let mut out = Vec::new();
        let mut cur: Vec<Span<'static>> = Vec::new();
        let mut text_buf = String::new();

        let mut i = 0;
        while i < chars.len() {
            let c = chars[i];
            match c {
                '\x1b' => i = self.consume_escape(&chars, i, &mut cur, &mut text_buf),
                '\n' => {
                    Self::take_span(&mut cur, &mut text_buf, self.state.style());
                    out.push(Line::from(std::mem::take(&mut cur)));
                    i += 1;
                }
                '\t' => {
                    text_buf.push_str("    ");
                    i += 1;
                }
                '\r' => i += 1,
                _ if c.is_control() => i += 1,
                _ => {
                    text_buf.push(c);
                    i += 1;
                }
            }
        }
        Self::take_span(&mut cur, &mut text_buf, self.state.style());
        out.push(Line::from(cur));
        out
    }

    /// Consume one escape sequence starting at `ESC`; `cur`/`text_buf` are
    /// flushed with the running style **before** any SGR mutates the state
    /// so the preceding text keeps its own styling. Returns the index after
    /// the sequence.
    fn consume_escape(
        &mut self,
        chars: &[char],
        i: usize,
        cur: &mut Vec<Span<'static>>,
        text_buf: &mut String,
    ) -> usize {
        Self::take_span(cur, text_buf, self.state.style());
        let j = i + 1;
        let Some(&c0) = chars.get(j) else {
            return j;
        };
        match c0 {
            // CSI: `ESC [ params final` where final ∈ 0x40..=0x7e.
            '[' => {
                let mut k = j + 1;
                while k < chars.len() {
                    let c = chars[k];
                    if ('\x40'..='\x7e').contains(&c) {
                        if c == 'm' {
                            let params: String = chars[j + 1..k].iter().collect();
                            self.apply_sgr(&params);
                        }
                        return k + 1;
                    }
                    k += 1;
                }
                chars.len() // unterminated CSI: treat the rest as garbage
            }
            // OSC: skip until BEL or `ESC \` (ST).
            ']' => {
                let mut k = j + 1;
                while k + 1 < chars.len() {
                    if chars[k] == '\x07' {
                        return k + 1;
                    }
                    if chars[k] == '\x1b' && chars[k + 1] == '\\' {
                        return k + 2;
                    }
                    k += 1;
                }
                chars.len()
            }
            // Two/three-codepoint escapes (charset selectors etc.).
            _ => {
                let extra = matches!(c0, '(' | ')' | '#' | '%' | '@');
                j + 1 + usize::from(extra)
            }
        }
    }

    /// Apply an SGR parameter string (the part between `[` and `m`).
    fn apply_sgr(&mut self, params: &str) {
        let tokens: Vec<u16> = params
            .split(';')
            .filter_map(|t| t.parse().ok())
            .collect();
        if tokens.is_empty() {
            self.state.reset();
            return;
        }
        let mut idx = 0;
        while idx < tokens.len() {
            match tokens[idx] {
                0 => self.state.reset(),
                1 => self.state.bold = true,
                2 => self.state.dimmed = true,
                3 => self.state.italic = true,
                4 => self.state.underline = true,
                7 => self.state.reversed = true,
                21 | 22 => {
                    self.state.bold = false;
                    self.state.dimmed = false;
                }
                23 => self.state.italic = false,
                24 => self.state.underline = false,
                27 => self.state.reversed = false,
                // Standard fg 30..=37.
                30..=37 => self.state.fg = Some(Color::Indexed((tokens[idx] - 30) as u8)),
                38 => {
                    let (fg, consumed) = Self::parse_extended(&tokens, idx);
                    self.state.fg = fg;
                    idx = consumed;
                }
                39 => self.state.fg = None,
                // Standard bg 40..=47.
                40..=47 => self.state.bg = Some(Color::Indexed((tokens[idx] - 40) as u8)),
                48 => {
                    let (bg, consumed) = Self::parse_extended(&tokens, idx);
                    self.state.bg = bg;
                    idx = consumed;
                }
                49 => self.state.bg = None,
                // Bright fg 90..=97 → palette indices 8..=15.
                90..=97 => self.state.fg = Some(Color::Indexed((tokens[idx] - 90 + 8) as u8)),
                // Bright bg 100..=107 → palette indices 8..=15.
                100..=107 => self.state.bg = Some(Color::Indexed((tokens[idx] - 100 + 8) as u8)),
                _ => {}
            }
            idx += 1;
        }
    }

    /// Parse the extended color carried by `38`/`48`: either `5;N` (256-color)
    /// or `2;R;G;B` (truecolor). Returns `(color, last_consumed_index)`.
    fn parse_extended(tokens: &[u16], idx: usize) -> (Option<Color>, usize) {
        let j = idx + 1;
        if j >= tokens.len() {
            return (None, idx);
        }
        match tokens[j] {
            5 if j + 1 < tokens.len() => {
                let c = Color::Indexed(tokens[j + 1] as u8);
                (Some(c), j + 1)
            }
            2 if j + 3 < tokens.len() => {
                let c = Color::Rgb(
                    tokens[j + 1] as u8,
                    tokens[j + 2] as u8,
                    tokens[j + 3] as u8,
                );
                (Some(c), j + 3)
            }
            // Well-formed but unsupported form (e.g. 4-byte CMYK) — drop it.
            _ => (None, j),
        }
    }

    /// Push accumulated plain text as a styled span, if any.
    fn take_span(cur: &mut Vec<Span<'static>>, text_buf: &mut String, style: Style) {
        if !text_buf.is_empty() {
            cur.push(Span::styled(std::mem::take(text_buf), style));
        }
    }
}

/// Join the plain text of parsed lines (drops styling; for snapshots).
pub fn plain_text(lines: &[Line<'_>]) -> String {
    lines
        .iter()
        .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use Color::*;

    fn parse(s: &str) -> Vec<Line<'static>> {
        AnsiParser::new().parse(s.as_bytes())
    }

    #[test]
    fn plain_text_passes_through() {
        assert_eq!(plain_text(&parse("hello\nworld")), "hello\nworld");
    }

    #[test]
    fn resets_at_end_of_stream() {
        let lines = parse("\x1b[31mred");
        assert_eq!(
            lines[0].spans[0].style.fg,
            Some(Indexed(1)),
            "fg 31 → palette 1"
        );
    }

    #[test]
    fn splits_16_color_and_attributes() {
        let lines = parse("\x1b[1;34mbold blue\x1b[0m plain");
        let spans = &lines[0].spans;
        assert_eq!(spans.len(), 2, "styled + reset plain");
        assert_eq!(plain_text(&lines), "bold blue plain");
        assert_eq!(spans[0].style.fg, Some(Indexed(4)), "fg 34 → 4");
        assert!(spans[0]
            .style
            .add_modifier.contains(ratatui::style::Modifier::BOLD));
        assert_eq!(spans[1].style.fg, None);
    }

    #[test]
    fn parses_256_color() {
        let lines = parse("\x1b[38;5;196mX");
        assert_eq!(lines[0].spans[0].style.fg, Some(Indexed(196)));
        let lines = parse("\x1b[48;5;21mX");
        assert_eq!(lines[0].spans[0].style.bg, Some(Indexed(21)));
    }

    #[test]
    fn parses_truecolor() {
        let lines = parse("\x1b[38;2;255;0;128mX");
        assert_eq!(lines[0].spans[0].style.fg, Some(Rgb(255, 0, 128)));
        let lines = parse("\x1b[48;2;10;20;30mX");
        assert_eq!(lines[0].spans[0].style.bg, Some(Rgb(10, 20, 30)));
    }

    #[test]
    fn strips_unknown_sequences() {
        let lines = parse("a\x1b[2;3H\x1b[1;1H\x1b]0;title\x07b\x1b(Bc");
        assert_eq!(plain_text(&lines), "abc");
    }

    #[test]
    fn expands_tabs_to_four_spaces() {
        assert_eq!(plain_text(&parse("a\tb")), "a    b");
    }

    #[test]
    fn mid_line_style_change_splits_spans() {
        let lines = parse("a\x1b[31m b");
        let spans = &lines[0].spans;
        assert_eq!(spans.len(), 2, "plain 'a' then styled ' b'");
        assert_eq!(spans[0].style.fg, None);
        assert_eq!(spans[1].style.fg, Some(Indexed(1)));
        assert_eq!(plain_text(&lines), "a b");
    }

    #[test]
    fn bright_colors_map_past_8() {
        let lines = parse("\x1b[91mX");
        assert_eq!(lines[0].spans[0].style.fg, Some(Indexed(9)));
    }

    #[test]
    fn mixed_sgr_output_shape_is_stable() {
        // In-memory golden value; the file rendering is regenerated by the
        // `component_output` example into `crates/wf-cli/outputs/`.
        let input = concat!(
            "\x1b[1;32mok\x1b[0m ",
            "\x1b[38;5;208morange\x1b[0m ",
            "\x1b[38;2;100;200;50mtruecol\r\n",
            "tab:\tend"
        );
        assert_eq!(
            plain_text(&AnsiParser::new().parse(input.as_bytes())),
            "ok orange truecol\ntab:    end"
        );
    }
}