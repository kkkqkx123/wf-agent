const MERGED_DIFF_MIN_CHARS: usize = 4000;
const MERGED_DIFF_MARKER_LINES: usize = 40;

fn is_diff_marker(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with("diff ")
        || trimmed.starts_with("+++")
        || trimmed.starts_with("---")
        || trimmed.starts_with("@@")
        || trimmed.starts_with("<<<<<<<")
        || trimmed.starts_with(">>>>>>>")
        || trimmed.starts_with("***")
}

pub fn fold_merged_diff(text: &str) -> Option<String> {
    if text.len() < MERGED_DIFF_MIN_CHARS {
        return None;
    }
    let lines: Vec<&str> = text.lines().collect();
    if lines.len() <= 60 {
        return None;
    }
    let head = 10;
    let tail = 10;
    let mut markers: Vec<&str> = Vec::new();
    for line in &lines {
        if is_diff_marker(line) && markers.len() < MERGED_DIFF_MARKER_LINES {
            markers.push(line);
        }
    }
    let mut out = String::new();
    out.push_str(&format!(
        "[edit folded: {} lines merged, {} diff markers kept]\n",
        lines.len(),
        markers.len()
    ));
    for line in &lines[..head] {
        out.push_str(line);
        out.push('\n');
    }
    for marker in &markers {
        out.push_str(marker);
        out.push('\n');
    }
    for line in &lines[lines.len() - tail..] {
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
    fn small_edit_output_is_kept() {
        assert!(fold_merged_diff("edited ok").is_none());
    }

    #[test]
    fn large_diff_collapses_to_markers() {
        let mut text = String::new();
        for i in 0..200 {
            text.push_str(&format!(
                "context line {i} with filler content padding 0123456789\n"
            ));
            if i % 10 == 0 {
                text.push_str(&format!("@@ -{i},5 +{i},5 @@ hunk header\n"));
            }
        }
        let folded = fold_merged_diff(&text).expect("large diff folds");
        assert!(folded.starts_with("[edit folded:"));
        assert!(folded.contains("@@"));
        assert!(folded.len() < text.len());
    }
}
