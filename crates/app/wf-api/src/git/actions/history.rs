use crate::git::config::command_timeout_ms;
use crate::git::runner::run_git;
use crate::git::types::GitResult;
use crate::infra::context::ApiContext;

use super::{echo_of, finish, param, require_confirm, ActionOutcome, ActionRequest};
use crate::git::config::validate_repo_path;

pub fn handle(
    ctx: &ApiContext,
    action: &str,
    request: &ActionRequest,
    root: &std::path::Path,
) -> Option<GitResult<ActionOutcome>> {
    match action {
        "reset.soft" | "reset.mixed" | "reset.hard" => Some(reset(ctx, action, request, root)),
        "conflict.accept-ours" | "conflict.accept-theirs" => {
            Some(conflict_accept(ctx, action, request, root))
        }
        "conflict.resolve-block" => Some(conflict_resolve_block(ctx, action, request, root)),
        _ => None,
    }
}

fn reset(
    ctx: &ApiContext,
    action: &str,
    request: &ActionRequest,
    root: &std::path::Path,
) -> GitResult<ActionOutcome> {
    let commit = param(request, "commit")?;
    if action == "reset.hard" {
        require_confirm(request, &commit, action)?;
    }
    let flag = if action == "reset.soft" {
        "--soft"
    } else if action == "reset.mixed" {
        "--mixed"
    } else {
        "--hard"
    };
    let argv = vec!["reset".to_string(), flag.to_string(), commit.clone()];
    let echo = echo_of(action, &argv);
    let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
    run_git(root, &refs, command_timeout_ms(ctx, false))?;
    Ok(finish(action, format!("reset {flag} to {commit}"), echo))
}

fn is_marker_start(line: &str) -> bool {
    line.starts_with("<<<<<<<")
}

fn is_marker_base(line: &str) -> bool {
    line.starts_with("|||||||")
}

fn is_marker_mid(line: &str) -> bool {
    line == "=======" || line.starts_with("======= ") || line.starts_with("=======\t")
}

fn is_marker_end(line: &str) -> bool {
    line.starts_with(">>>>>>>")
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ConflictBlockSpan {
    start: usize,
    base_sep: Option<usize>,
    mid: usize,
    end: usize,
}

fn parse_block_spans(lines: &[&str]) -> Vec<ConflictBlockSpan> {
    let mut out = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        if !is_marker_start(lines[index]) {
            index += 1;
            continue;
        }
        let start = index;
        let mut base_sep: Option<usize> = None;
        let mut mid: Option<usize> = None;
        let mut end: Option<usize> = None;
        let mut cursor = index + 1;
        while cursor < lines.len() {
            let line = lines[cursor];
            if is_marker_base(line) && base_sep.is_none() && mid.is_none() {
                base_sep = Some(cursor);
            } else if is_marker_mid(line) && mid.is_none() {
                mid = Some(cursor);
            } else if is_marker_end(line) && mid.is_some() {
                end = Some(cursor);
                break;
            } else if is_marker_start(line) {
                break;
            }
            cursor += 1;
        }
        match (mid, end) {
            (Some(mid_pos), Some(end_pos)) => {
                out.push(ConflictBlockSpan {
                    start,
                    base_sep,
                    mid: mid_pos,
                    end: end_pos,
                });
                index = end_pos + 1;
            }
            _ => {
                index = start + 1;
            }
        }
    }
    out
}

fn apply_block_resolution(content: &str, block: usize, side: &str) -> Result<String, String> {
    let trailing_newline = content.ends_with('\n');
    let lines: Vec<&str> = content.lines().collect();
    let spans = parse_block_spans(&lines);
    if spans.is_empty() {
        return Err("no conflict markers in file".to_string());
    }
    let span = spans.get(block).ok_or_else(|| {
        format!(
            "conflict block {block} out of range ({} blocks)",
            spans.len()
        )
    })?;
    let ours_end = span.base_sep.unwrap_or(span.mid);
    let replacement: Vec<&str> = if side == "ours" {
        lines[span.start + 1..ours_end].to_vec()
    } else if side == "theirs" {
        lines[span.mid + 1..span.end].to_vec()
    } else {
        return Err("side must be ours or theirs".to_string());
    };
    let mut next: Vec<&str> = Vec::with_capacity(lines.len());
    next.extend_from_slice(&lines[..span.start]);
    next.extend_from_slice(&replacement);
    next.extend_from_slice(&lines[span.end + 1..]);
    let mut resolved = next.join("\n");
    if trailing_newline && !resolved.is_empty() {
        resolved.push('\n');
    }
    Ok(resolved)
}

fn has_remaining_markers(content: &str) -> bool {
    content.lines().any(is_marker_start)
}

fn conflict_resolve_block(
    ctx: &ApiContext,
    action: &str,
    request: &ActionRequest,
    root: &std::path::Path,
) -> GitResult<ActionOutcome> {
    use crate::git::types::GitError;
    let path = param(request, "path")?;
    let absolute = validate_repo_path(root, &path)?;
    let block_raw = param(request, "block")?;
    let block: usize = block_raw
        .parse()
        .map_err(|_| GitError::Precondition(format!("invalid block '{block_raw}'")))?;
    let side = param(request, "side")?;
    if side != "ours" && side != "theirs" {
        return Err(GitError::Precondition(
            "side must be ours or theirs".to_string(),
        ));
    }
    let current = std::fs::read_to_string(&absolute)
        .map_err(|e| GitError::Command(format!("failed to read {path}: {e}")))?;
    let resolved =
        apply_block_resolution(&current, block, &side).map_err(GitError::Precondition)?;
    std::fs::write(&absolute, &resolved)
        .map_err(|e| GitError::Command(format!("failed to write {path}: {e}")))?;
    if has_remaining_markers(&resolved) {
        let echo = format!("edit {path} block {block} {side}");
        return Ok(finish(
            action,
            format!("resolved block {block} of {path} with {side}"),
            echo,
        ));
    }
    let argv = ["add".to_string(), "--".to_string(), path.clone()];
    let echo = echo_of(action, &argv);
    let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
    run_git(root, &refs, command_timeout_ms(ctx, false))?;
    Ok(finish(
        action,
        format!("resolved block {block} of {path} with {side}"),
        echo,
    ))
}

fn conflict_accept(
    ctx: &ApiContext,
    action: &str,
    request: &ActionRequest,
    root: &std::path::Path,
) -> GitResult<ActionOutcome> {
    let path = param(request, "path")?;
    validate_repo_path(root, &path)?;
    let side = if action == "conflict.accept-ours" {
        "--ours"
    } else {
        "--theirs"
    };
    let argv = vec![
        "checkout".to_string(),
        side.to_string(),
        "--".to_string(),
        path.clone(),
    ];
    let echo = echo_of(action, &argv);
    let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
    run_git(root, &refs, command_timeout_ms(ctx, false))?;
    let argv = ["add".to_string(), "--".to_string(), path.clone()];
    let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
    run_git(root, &refs, command_timeout_ms(ctx, false))?;
    Ok(finish(action, format!("accepted {side} for {path}"), echo))
}

#[cfg(test)]
mod resolve_block_tests {
    use super::{apply_block_resolution, has_remaining_markers, parse_block_spans};

    fn sample_two_blocks() -> String {
        [
            "head",
            "<<<<<<< HEAD",
            "ours-a",
            "=======",
            "theirs-a",
            ">>>>>>> branch",
            "middle",
            "<<<<<<< HEAD",
            "ours-b",
            "=======",
            "theirs-b",
            ">>>>>>> branch",
            "tail",
        ]
        .join("\n")
            + "\n"
    }

    #[test]
    fn parses_two_blocks_with_correct_spans() {
        let content = sample_two_blocks();
        let lines: Vec<&str> = content.lines().collect();
        let spans = parse_block_spans(&lines);
        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0].start, 1);
        assert_eq!(spans[1].start, 7);
    }

    #[test]
    fn resolve_first_block_keeps_other_markers() {
        let resolved = apply_block_resolution(&sample_two_blocks(), 0, "ours").expect("resolves");
        assert!(resolved.contains("ours-a"));
        assert!(!resolved.contains("theirs-a"));
        assert!(resolved.contains("<<<<<<< HEAD"));
        assert!(has_remaining_markers(&resolved));
    }

    #[test]
    fn resolve_single_block_clears_all_markers() {
        let single = "a\n<<<<<<< HEAD\nours\n=======\ntheirs\n>>>>>>> b\n";
        let resolved = apply_block_resolution(single, 0, "theirs").expect("resolves");
        assert_eq!(resolved, "a\ntheirs\n");
        assert!(!has_remaining_markers(&resolved));
    }

    #[test]
    fn sequential_resolves_clear_both_blocks() {
        let once = apply_block_resolution(&sample_two_blocks(), 0, "theirs").expect("first");
        assert!(has_remaining_markers(&once));
        let twice = apply_block_resolution(&once, 0, "ours").expect("second");
        assert!(!has_remaining_markers(&twice));
        assert!(twice.contains("theirs-a"));
        assert!(twice.contains("ours-b"));
    }

    #[test]
    fn rejects_out_of_range_and_missing_markers() {
        assert!(apply_block_resolution(&sample_two_blocks(), 9, "ours").is_err());
        assert!(apply_block_resolution("plain\ntext\n", 0, "ours").is_err());
        assert!(apply_block_resolution(&sample_two_blocks(), 0, "both").is_err());
    }

    #[test]
    fn supports_base_separator_without_leaking_it() {
        let content = [
            "<<<<<<< ours",
            "ours-line",
            "||||||| base",
            "base-line",
            "=======",
            "theirs-line",
            ">>>>>>> theirs",
        ]
        .join("\n")
            + "\n";
        let ours = apply_block_resolution(&content, 0, "ours").expect("ours");
        assert!(ours.contains("ours-line"));
        assert!(!ours.contains("base-line"));
        let theirs = apply_block_resolution(&content, 0, "theirs").expect("theirs");
        assert!(theirs.contains("theirs-line"));
        assert!(!theirs.contains("base-line"));
    }
}
