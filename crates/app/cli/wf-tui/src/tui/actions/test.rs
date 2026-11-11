use super::conflict::{parse_rebase_selector, rebuild_conflict_content};
use super::*;

#[test]
fn rebase_selector_parses_named_fields() {
    let params = parse_rebase_selector("main prefix=abc mode=copy").expect("parses");
    assert_eq!(params.branch.as_deref(), Some("main"));
    assert_eq!(params.commit_prefix.as_deref(), Some("abc"));
    assert_eq!(params.behind, None);
    assert_eq!(params.mode, "copy");
    let params = parse_rebase_selector("behind=3").expect("parses");
    assert_eq!(params.branch, None);
    assert_eq!(params.behind, Some(3));
    assert_eq!(params.mode, "borrow");
}

#[test]
fn rebase_selector_rejects_bad_input() {
    assert!(parse_rebase_selector("a b").is_err());
    assert!(parse_rebase_selector("prefix=abc behind=3").is_err());
    assert!(parse_rebase_selector("mode=fast").is_err());
    assert!(parse_rebase_selector("behind=many").is_err());
}

#[test]
fn rebuild_accepts_ours_over_theirs() {
    let merged = "head\n<<<<<<< ours\nours line\n=======\ntheirs line\n>>>>>>> theirs\ntail\n";
    assert_eq!(
        rebuild_conflict_content(merged, true).as_deref(),
        Some("head\nours line\ntail\n")
    );
    assert_eq!(
        rebuild_conflict_content(merged, false).as_deref(),
        Some("head\ntheirs line\ntail\n")
    );
}

#[test]
fn rebuild_handles_diff3_base_section() {
    let merged = "<<<<<<< ours\na\n||||||| base\nb\n=======\nc\n>>>>>>> theirs\n";
    assert_eq!(
        rebuild_conflict_content(merged, true).as_deref(),
        Some("a\n")
    );
    assert_eq!(
        rebuild_conflict_content(merged, false).as_deref(),
        Some("c\n")
    );
}

#[test]
fn rebuild_rejects_markerless_and_unterminated_content() {
    assert_eq!(rebuild_conflict_content("clean\n", true), None);
    assert_eq!(rebuild_conflict_content("<<<<<<< ours\na\n", true), None);
}

#[test]
fn rebuild_preserves_body_without_trailing_newline() {
    let merged = "x\n<<<<<<< ours\nours\n=======\ntheirs\n>>>>>>> theirs";
    assert_eq!(
        rebuild_conflict_content(merged, true).as_deref(),
        Some("x\nours")
    );
}
