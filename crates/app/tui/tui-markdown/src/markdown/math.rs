//! Dependency-free Unicode rendering for LaTeX math expressions.
//!
//! The document model keeps the raw expression in [`StyledSpan::latex`] while
//! this module derives the readable fallback shown on screen. Only widely
//! supported single-cell glyphs are used so the output survives the terminal
//! cell measurement unchanged.

use std::fmt::Write;

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
            let _ = write!(rendered, "({})/({})", num.unwrap_or_default(), den.unwrap_or_default());
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
}
