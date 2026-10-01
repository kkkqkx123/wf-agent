pub const SHELL_HEAD_LINES: usize = 20;
pub const SHELL_TAIL_LINES: usize = 40;

pub fn fold_head_tail(text: &str, head_lines: usize, tail_lines: usize) -> Option<String> {
    let lines: Vec<&str> = text.lines().collect();
    if lines.len() <= head_lines + tail_lines {
        return None;
    }
    let omitted = lines.len() - head_lines - tail_lines;
    let mut out = String::new();
    for line in &lines[..head_lines] {
        out.push_str(line);
        out.push('\n');
    }
    out.push_str(&format!("[... {omitted} lines omitted ...]\n"));
    for line in &lines[lines.len() - tail_lines..] {
        out.push_str(line);
        out.push('\n');
    }
    if out.len() >= text.len() {
        return None;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_output_is_kept_verbatim() {
        let text = (0..30)
            .map(|i| format!("line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(fold_head_tail(&text, 20, 40).is_none());
    }

    #[test]
    fn long_output_keeps_head_and_tail() {
        let text = (0..100)
            .map(|i| format!("line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let folded = fold_head_tail(&text, 20, 40).expect("long output folds");
        assert!(folded.starts_with("line 0\n"));
        assert!(folded.contains("line 99"));
        assert!(folded.contains("40 lines omitted"));
    }
}
