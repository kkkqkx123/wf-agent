use super::shell::fold_head_tail;

pub const GENERIC_HEAD_LINES: usize = 20;
pub const GENERIC_TAIL_LINES: usize = 20;

pub fn fold_generic(text: &str) -> Option<String> {
    fold_head_tail(text, GENERIC_HEAD_LINES, GENERIC_TAIL_LINES)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generic_fold_keeps_short_text() {
        assert!(fold_generic("hello").is_none());
    }
}
