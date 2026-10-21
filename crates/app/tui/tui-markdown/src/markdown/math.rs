//! Dependency-free Unicode rendering for LaTeX math expressions.
//!
//! The document model keeps the raw expression in [`crate::markdown::document::StyledSpan::latex`] while
//! this module derives the readable fallback shown on screen. Only widely
//! supported single-cell glyphs are used so the output survives the terminal
//! cell measurement unchanged.
//!
//! Display math follows [`LatexMode`]: inline math always renders Unicode,
//! display math renders Unicode in [`LatexMode::Unicode`] and requests an
//! external TeX-to-image backend in [`LatexMode::TexImage`]. No raster backend
//! is bundled, so the image path keeps the Unicode text plus an explanatory
//! reason instead of dropping the expression.

use std::fmt::Write;

use super::render_options::LatexMode;

/// Reason reported when display math requests an image backend that is not
/// bundled. The Unicode fallback stays on screen beside the raw expression.
pub const TEX_IMAGE_UNAVAILABLE_REASON: &str = "tex backend unavailable; showing unicode";

/// Whether an external TeX-to-image backend is available. Always false in
/// this version: the crate ships no raster backend, so image requests fall
/// back to Unicode with [`TEX_IMAGE_UNAVAILABLE_REASON`].
pub fn is_tex_backend_available() -> bool {
    false
}

/// Rendered display math: readable text plus an optional backend reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplayMathRender {
    /// Readable Unicode text shown on screen.
    pub text: String,
    /// Why an image was not produced (`None` when Unicode was requested or
    /// a backend handled the expression).
    pub reason: Option<String>,
}

/// Render display math under `mode`. Unicode mode returns text with no
/// reason; image mode without a backend keeps the Unicode text and reports
/// why no image was produced, so the expression stays readable and the raw
/// LaTeX remains in the span.
pub fn render_display_math(latex: &str, mode: LatexMode) -> DisplayMathRender {
    let text = latex_to_unicode(latex);
    match mode {
        LatexMode::Unicode => DisplayMathRender { text, reason: None },
        LatexMode::TexImage => {
            if is_tex_backend_available() {
                DisplayMathRender { text, reason: None }
            } else {
                DisplayMathRender {
                    text,
                    reason: Some(TEX_IMAGE_UNAVAILABLE_REASON.to_string()),
                }
            }
        }
    }
}

/// Normalize math delimiters, fences and environments in full markdown
/// source to one canonical form before parsing.
///
/// * `\[...\]` becomes `$$...$$`, `\(...\)` becomes `$...$`;
/// * known display environments (`equation`, `align`, `gather`,
///   `multline`, `math`, `displaymath`, starred variants) become `$$...$$`;
/// * ` ```math ` fences become `$$...$$`.
///
/// The rewrite is prefix-preserving and idempotent: normalizing a growing
/// stream prefix yields a prefix of the normalized whole, so streaming and
/// whole-source renders agree and repeated normalization is a no-op.
pub fn normalize_math_source(src: &str) -> String {
    let mut out = normalize_math_fences(src);
    out = normalize_math_environments(&out);
    out = out.replace("\\[", "$$").replace("\\]", "$$");
    out = out.replace("\\(", "$").replace("\\)", "$");
    out
}

/// Normalize ` ```math ` fenced blocks to `$$` display delimiters.
fn normalize_math_fences(src: &str) -> String {
    let mut lines: Vec<&str> = Vec::new();
    let mut in_math_fence = false;
    for line in src.split_inclusive('\n') {
        let (body, newline) = match line.strip_suffix('\n') {
            Some(body) => (body, "\n"),
            None => (line, ""),
        };
        let trimmed = body.trim();
        if !in_math_fence
            && (trimmed.eq_ignore_ascii_case("```math") || trimmed.eq_ignore_ascii_case("~~~math"))
        {
            in_math_fence = true;
            lines.push("$$");
            lines.push(newline);
            continue;
        }
        if in_math_fence && (trimmed == "```" || trimmed == "~~~") {
            in_math_fence = false;
            lines.push("$$");
            lines.push(newline);
            continue;
        }
        lines.push(body);
        lines.push(newline);
    }
    lines.concat()
}

/// Math environments normalized to `$$` delimiters.
const MATH_ENVIRONMENTS: &[&str] = &[
    "equation",
    "equation*",
    "align",
    "align*",
    "gather",
    "gather*",
    "multline",
    "multline*",
    "math",
    "displaymath",
];

/// Replace known `\begin{...}` and `\end{...}` math environments with `$$`.
fn normalize_math_environments(src: &str) -> String {
    let mut out = src.to_string();
    for env in MATH_ENVIRONMENTS {
        out = out.replace(&format!("\\begin{{{env}}}"), "$$");
        out = out.replace(&format!("\\end{{{env}}}"), "$$");
    }
    out
}

/// Render a raw LaTeX expression to readable Unicode text.
pub fn latex_to_unicode(latex: &str) -> String {
    let mut out = String::new();
    let chars: Vec<char> = latex.chars().collect();
    let mut i = 0usize;
    while i < chars.len() {
        let ch = chars[i];
        if ch == '\\' {
            let start = i + 1;
            let mut end = start;
            while end < chars.len() && chars[end].is_ascii_alphabetic() {
                end += 1;
            }
            if end == start {
                if end < chars.len() {
                    match chars[end] {
                        '{' | '}' | '$' | '%' | '&' | '#' | '_' => {
                            out.push(chars[end]);
                        }
                        ',' | ';' | ':' | ' ' => out.push(' '),
                        _ => {
                            out.push(chars[end]);
                        }
                    }
                    i = end + 1;
                } else {
                    i += 1;
                }
                continue;
            }
            let name: String = chars[start..end].iter().collect();
            i = end;
            match render_command(&name, &chars, &mut i) {
                Some(text) => out.push_str(&text),
                None => {
                    out.push_str(&name);
                }
            }
            continue;
        }
        if ch == '^' {
            let (text, next) = braced_or_single(&chars, i + 1);
            out.push_str(&to_superscript(&text));
            i = next;
            continue;
        }
        if ch == '_' {
            let (text, next) = braced_or_single(&chars, i + 1);
            out.push_str(&to_subscript(&text));
            i = next;
            continue;
        }
        if ch == '{' || ch == '}' {
            i += 1;
            continue;
        }
        out.push(ch);
        i += 1;
    }
    out
}

/// Render one backslash command, consuming brace arguments from `chars[i..]`
/// and advancing `i` past them. Returns `None` for unknown commands.
fn render_command(name: &str, chars: &[char], i: &mut usize) -> Option<String> {
    if let Some(glyph) = greek_letter(name) {
        return Some(glyph.to_string());
    }
    if let Some(glyph) = operator_symbol(name) {
        return Some(glyph.to_string());
    }
    match name {
        "frac" => {
            let (num, next) = take_braced(chars, *i);
            let (den, next) = take_braced(chars, next);
            *i = next;
            let mut rendered = String::new();
            let _ = write!(
                rendered,
                "({})/({})",
                num.unwrap_or_default(),
                den.unwrap_or_default()
            );
            Some(rendered)
        }
        "sqrt" => {
            let (inner, next) = take_braced(chars, *i);
            *i = next;
            Some(format!("√({})", inner.unwrap_or_default()))
        }
        "text" | "mathrm" | "mathbf" | "mathit" => {
            let (inner, next) = take_braced(chars, *i);
            *i = next;
            Some(inner.unwrap_or_default())
        }
        _ => None,
    }
}

/// Take a `{...}` group at `i`, returning its inner text and the next index.
/// Without braces, consumes one character as the argument.
fn take_braced(chars: &[char], i: usize) -> (Option<String>, usize) {
    let mut j = i;
    while j < chars.len() && chars[j].is_whitespace() {
        j += 1;
    }
    if j < chars.len() && chars[j] == '{' {
        j += 1;
        let mut depth = 1usize;
        let mut inner = String::new();
        while j < chars.len() && depth > 0 {
            if chars[j] == '{' {
                depth += 1;
                inner.push(chars[j]);
            } else if chars[j] == '}' {
                depth -= 1;
                if depth > 0 {
                    inner.push(chars[j]);
                }
            } else {
                inner.push(chars[j]);
            }
            j += 1;
        }
        (Some(latex_to_unicode(&inner)), j)
    } else if j < chars.len() {
        (Some(chars[j].to_string()), j + 1)
    } else {
        (None, j)
    }
}

/// Text of a `^`/`_` suffix: a braced group or a single character.
fn braced_or_single(chars: &[char], i: usize) -> (String, usize) {
    let (inner, next) = take_braced(chars, i);
    (inner.unwrap_or_default(), next)
}

/// Greek letter commands to Unicode glyphs.
fn greek_letter(name: &str) -> Option<&'static str> {
    Some(match name {
        "alpha" => "α",
        "beta" => "β",
        "gamma" => "γ",
        "delta" => "δ",
        "epsilon" => "ε",
        "zeta" => "ζ",
        "eta" => "η",
        "theta" => "θ",
        "iota" => "ι",
        "kappa" => "κ",
        "lambda" => "λ",
        "mu" => "μ",
        "nu" => "ν",
        "xi" => "ξ",
        "pi" => "π",
        "rho" => "ρ",
        "sigma" => "σ",
        "tau" => "τ",
        "upsilon" => "υ",
        "phi" => "φ",
        "chi" => "χ",
        "psi" => "ψ",
        "omega" => "ω",
        "Gamma" => "Γ",
        "Delta" => "Δ",
        "Theta" => "Θ",
        "Lambda" => "Λ",
        "Xi" => "Ξ",
        "Pi" => "Π",
        "Sigma" => "Σ",
        "Phi" => "Φ",
        "Psi" => "Ψ",
        "Omega" => "Ω",
        _ => return None,
    })
}

/// Operator and relation commands to Unicode glyphs.
fn operator_symbol(name: &str) -> Option<&'static str> {
    Some(match name {
        "times" => "×",
        "div" => "÷",
        "pm" => "±",
        "mp" => "∓",
        "cdot" => "·",
        "leq" | "le" => "≤",
        "geq" | "ge" => "≥",
        "neq" | "ne" => "≠",
        "approx" => "≈",
        "equiv" => "≡",
        "infty" => "∞",
        "sum" => "∑",
        "prod" => "∏",
        "int" => "∫",
        "partial" => "∂",
        "to" | "rightarrow" => "→",
        "leftarrow" => "←",
        "Rightarrow" => "⇒",
        "Leftarrow" => "⇐",
        "langle" => "⟨",
        "rangle" => "⟩",
        _ => return None,
    })
}

/// Map ASCII letters and digits to superscript glyphs, leaving the rest.
fn to_superscript(text: &str) -> String {
    text.chars()
        .map(|ch| match ch {
            '0' => '⁰',
            '1' => '¹',
            '2' => '²',
            '3' => '³',
            '4' => '⁴',
            '5' => '⁵',
            '6' => '⁶',
            '7' => '⁷',
            '8' => '⁸',
            '9' => '⁹',
            '+' => '⁺',
            '-' => '⁻',
            '=' => '⁼',
            '(' => '⁽',
            ')' => '⁾',
            'a' => 'ᵃ',
            'b' => 'ᵇ',
            'c' => 'ᶜ',
            'd' => 'ᵈ',
            'e' => 'ᵉ',
            'f' => 'ᶠ',
            'g' => 'ᵍ',
            'h' => 'ʰ',
            'i' => 'ⁱ',
            'j' => 'ʲ',
            'k' => 'ᵏ',
            'l' => 'ˡ',
            'm' => 'ᵐ',
            'n' => 'ⁿ',
            'o' => 'ᵒ',
            'p' => 'ᵖ',
            'r' => 'ʳ',
            's' => 'ˢ',
            't' => 'ᵗ',
            'u' => 'ᵘ',
            'v' => 'ᵛ',
            'w' => 'ʷ',
            'x' => 'ˣ',
            'y' => 'ʸ',
            'z' => 'ᶻ',
            _ => ch,
        })
        .collect()
}

/// Map ASCII letters and digits to subscript glyphs, leaving the rest.
fn to_subscript(text: &str) -> String {
    text.chars()
        .map(|ch| match ch {
            '0' => '₀',
            '1' => '₁',
            '2' => '₂',
            '3' => '₃',
            '4' => '₄',
            '5' => '₅',
            '6' => '₆',
            '7' => '₇',
            '8' => '₈',
            '9' => '₉',
            '+' => '₊',
            '-' => '₋',
            '=' => '₌',
            '(' => '₍',
            ')' => '₎',
            'a' => 'ₐ',
            'e' => 'ₑ',
            'h' => 'ₕ',
            'i' => 'ᵢ',
            'j' => 'ⱼ',
            'k' => 'ₖ',
            'l' => 'ₗ',
            'm' => 'ₘ',
            'n' => 'ₙ',
            'o' => 'ₒ',
            'p' => 'ₚ',
            'r' => 'ᵣ',
            's' => 'ₛ',
            't' => 'ₜ',
            'u' => 'ᵤ',
            'v' => 'ᵥ',
            'x' => 'ₓ',
            _ => ch,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn greek_commands_become_glyphs() {
        assert_eq!(latex_to_unicode("\\alpha + \\beta"), "α + β");
        assert_eq!(latex_to_unicode("\\Omega"), "Ω");
    }

    #[test]
    fn frac_and_sqrt_keep_structure() {
        assert_eq!(latex_to_unicode("\\frac{a}{b}"), "(a)/(b)");
        assert_eq!(latex_to_unicode("\\sqrt{x}"), "√(x)");
    }

    #[test]
    fn super_and_subscripts_map_to_unicode() {
        assert_eq!(latex_to_unicode("x^2"), "x²");
        assert_eq!(latex_to_unicode("x_{i}"), "xᵢ");
    }

    #[test]
    fn operators_render_readably() {
        assert_eq!(latex_to_unicode("a \\times b \\to \\infty"), "a × b → ∞");
    }

    #[test]
    fn plain_text_passes_through() {
        assert_eq!(latex_to_unicode("E = mc^2"), "E = mc²");
        assert_eq!(latex_to_unicode(""), "");
    }

    #[test]
    fn display_modes_keep_unicode_readable() {
        use super::super::render_options::LatexMode;
        let unicode = super::render_display_math("x^2", LatexMode::Unicode);
        assert_eq!(unicode.text, "x²");
        assert_eq!(unicode.reason, None);
        let image = super::render_display_math("x^2", LatexMode::TexImage);
        assert_eq!(image.text, "x²");
        assert_eq!(
            image.reason.as_deref(),
            Some(super::TEX_IMAGE_UNAVAILABLE_REASON)
        );
        assert!(!super::is_tex_backend_available());
    }

    #[test]
    fn normalization_canonicalizes_delimiters_and_envs() {
        let src = "\\[a\\] and \\(b\\) with \\begin{equation}c\\end{equation}\n";
        let normalized = super::normalize_math_source(src);
        assert!(normalized.contains("$$a$$"));
        assert!(normalized.contains("$b$"));
        assert!(normalized.contains("$$c$$"));
        assert_eq!(
            super::normalize_math_source(&normalized),
            normalized,
            "normalization must be idempotent"
        );
    }

    #[test]
    fn math_fences_become_display_delimiters() {
        let src = "```math\nx^2\n```\n";
        let normalized = super::normalize_math_source(src);
        assert!(normalized.contains("$$"));
        assert!(!normalized.contains("```math"));
    }
}
