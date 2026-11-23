//! HTML text extraction shared by the web tools.
//!
//! Search backends extract short fragments from result markup while the fetch
//! handler converts whole pages. Both use the same entity decoding and tag
//! stripping rules so a snippet cannot drift from the page it points at.

use std::sync::LazyLock;

use regex::{Captures, Regex};

static SCRIPT_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?is)<script\b[^>]*>.*?</script\s*>")
        .expect("invariant: fixed script pattern must compile")
});
static STYLE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?is)<style\b[^>]*>.*?</style\s*>")
        .expect("invariant: fixed style pattern must compile")
});
static BLOCK_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)</?(?:p|div|h[1-6]|li|br|tr|section|article)\b[^>]*>")
        .expect("invariant: fixed block pattern must compile")
});
static TAG_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"<[^>]*>").expect("invariant: fixed tag pattern must compile"));
static ENTITY_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"&(amp|lt|gt|quot|apos|nbsp|#\d+|#x[0-9a-fA-F]+);")
        .expect("invariant: fixed entity pattern must compile")
});

/// Decode named and numeric HTML entities while leaving unknown tokens intact.
pub fn decode_html_entities(input: &str) -> String {
    ENTITY_RE
        .replace_all(input, |captures: &Captures| {
            let token = &captures[1];
            match token {
                "amp" => "&".to_string(),
                "lt" => "<".to_string(),
                "gt" => ">".to_string(),
                "quot" => "\"".to_string(),
                "apos" | "#39" | "#x27" => "'".to_string(),
                "nbsp" => " ".to_string(),
                numeric if numeric.starts_with("#x") => {
                    let code = u32::from_str_radix(&numeric[2..], 16).ok();
                    code.and_then(char::from_u32)
                        .map(|c| c.to_string())
                        .unwrap_or_else(|| captures[0].to_string())
                }
                numeric => numeric[1..]
                    .parse::<u32>()
                    .ok()
                    .and_then(char::from_u32)
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| captures[0].to_string()),
            }
        })
        .into_owned()
}

/// Strip scripts, styles and tags, then collapse whitespace runs.
pub fn html_to_text(input: &str) -> String {
    let without_scripts = SCRIPT_RE.replace_all(input, "");
    let without_styles = STYLE_RE.replace_all(&without_scripts, "");
    let with_breaks = BLOCK_RE.replace_all(&without_styles, " ");
    let without_tags = TAG_RE.replace_all(&with_breaks, "");
    let decoded = decode_html_entities(&without_tags);
    let mut out = String::with_capacity(decoded.len());
    let mut previous_was_space = true;
    for c in decoded.chars() {
        if c.is_whitespace() {
            if !previous_was_space {
                out.push(' ');
            }
            previous_was_space = true;
        } else {
            out.push(c);
            previous_was_space = false;
        }
    }
    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_scripts_styles_and_decodes_entities() {
        let html = "<style>p{color:red}</style><h1>Title &amp; more</h1>\
                    <script>alert('x')</script><p>A&nbsp; &lt;b&gt;body&lt;/b&gt;</p>";
        assert_eq!(html_to_text(html), "Title & more A <b>body</b>");
    }

    #[test]
    fn decodes_numeric_entities() {
        assert_eq!(decode_html_entities("caf&#233; &#x27;x&#x27;"), "café 'x'");
    }

    #[test]
    fn keeps_unknown_entities_untouched() {
        assert_eq!(decode_html_entities("&custom; &amp;"), "&custom; &");
    }

    #[test]
    fn collapses_block_boundaries() {
        assert_eq!(html_to_text("<div>one</div><p>two</p>"), "one two");
    }
}
