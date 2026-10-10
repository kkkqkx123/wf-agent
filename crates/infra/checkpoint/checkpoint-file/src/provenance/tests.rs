use super::timeline::content_similarity;
use super::*;

#[test]
fn rename_similarity_threshold_matches_standard_default() {
    assert_eq!(RENAME_SIMILARITY_THRESHOLD, 0.5);
    assert_eq!(content_similarity(b"same", b"same"), 1.0);
    assert_eq!(content_similarity(b"a\0b", b"a\0c"), 0.0);
    let before = "l1\nl2\nl3\nl4\nl5\n";
    let after = "l1\nl2\nCHANGED\nl4\nl5\n";
    let score = content_similarity(before.as_bytes(), after.as_bytes());
    assert!(
        score >= RENAME_SIMILARITY_THRESHOLD,
        "small edit must still count as rename, got {score}"
    );
    let distant = content_similarity(b"aaa\nbbb\n", b"xxx\nyyy\nzzz\n");
    assert!(
        distant < RENAME_SIMILARITY_THRESHOLD,
        "unrelated content must not count as rename, got {distant}"
    );
}
