//! `Keymap`: centralized key resolution with context fallback
//! (`docs/cli/04` §七, `docs/cli/03` §3.1 red-line 8 — components must not
//! scatter bare `match` on keys).
//!
//! Resolution order: `context overrides → global overrides → builtin
//! defaults`. `Key` is a small crossterm-free struct (Stage 6 maps crossterm
//! events here), keeping the component layer ownership-clean.

use std::collections::HashMap;

/// Physical key code (modifier-free). Kept deliberately small; Stage 6
/// adapts crossterm `KeyCode` into this.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CKey {
    Char(char),
    Enter,
    Esc,
    Tab,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    Backspace,
    Delete,
}

/// A key chord: a code plus optional modifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Key {
    pub code: CKey,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
}

impl Key {
    pub const fn plain(code: CKey) -> Self {
        Self {
            code,
            ctrl: false,
            alt: false,
            shift: false,
        }
    }
    pub const fn ctrl(code: CKey) -> Self {
        Self {
            code,
            ctrl: true,
            alt: false,
            shift: false,
        }
    }
    pub const fn with_shift(mut self, shift: bool) -> Self {
        self.shift = shift;
        self
    }
}

/// The abstract action a key may trigger.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyAction {
    /// Reserved no-op (P1 override hooks).
    None,
    Quit,
    Redraw,
    Help,
    Palette,
    /// Contextual: interrupt a turn / cancel an operation.
    Interrupt,
    Cancel,
    Back,
    Select,
    Submit,
    Refresh,
    Delete,
    New,
    Edit,
    MovePrev,
    MoveNext,
    HistoryPrev,
    HistoryNext,
    Clear,
    Home,
    End,
    // Approval (stage 6 consumes these)
    Approve,
    ApproveAll,
    /// Deny this tool call only; later calls ask again.
    DenyOnce,
    Deny,
    /// Pick the n-th option in the question view (1..=9).
    Pick(u8),
}

/// Key resolution context.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum KeymapContext {
    #[default]
    Global,
    List,
    Detail,
    Chat,
    Input,
    Modal,
    /// Single-line prompt composer (mini footer).
    Composer,
    /// Selection panels: model / skill / queued prompts.
    Panel,
    /// Tool approval view (captures keys while active).
    Approval,
    /// Follow-up question view (captures keys while active).
    Question,
}

/// A `(context → key → action)` resolution table with global fallback.
#[derive(Debug, Clone, Default)]
pub struct Keymap {
    global: HashMap<Key, KeyAction>,
    context: HashMap<KeymapContext, HashMap<Key, KeyAction>>,
}

impl Keymap {
    /// Empty builder (builtin defaults applied on demand via
    /// [`Self::set_global`] / [`Self::bind`]).
    pub fn new() -> Self {
        Self::default()
    }

    /// Bind a global override.
    pub fn set_global(&mut self, key: Key, action: KeyAction) -> &mut Self {
        self.global.insert(key, action);
        self
    }

    /// Bind a context-local override.
    pub fn bind(&mut self, ctx: KeymapContext, key: Key, action: KeyAction) -> &mut Self {
        self.context.entry(ctx).or_default().insert(key, action);
        self
    }

    /// Resolve `key` in `ctx`: context tables, then the global table, then
    /// the builtin defaults.
    pub fn resolve(&self, ctx: KeymapContext, key: Key) -> Option<KeyAction> {
        if let Some(action) = self
            .context
            .get(&ctx)
            .and_then(|m| m.get(&key))
            .copied()
        {
            return Some(action);
        }
        if let Some(action) = self.global.get(&key).copied() {
            return Some(action);
        }
        builtin_defaults(ctx)
            .iter()
            .copied()
            .find(|(k, _): &(Key, KeyAction)| *k == key)
            .map(|(_, a)| a)
    }
}

/// Builtin bindings that apply in every context (04 §八). These live in the
/// *global* table so callers can shadow them with `set_global` and they are
/// still resolved for any context.
fn builtin_global() -> Vec<(Key, KeyAction)> {
    use KeyAction::*;
    vec![
        (Key::ctrl(CKey::Char('q')), Quit),
        (Key::ctrl(CKey::Char('c')), Interrupt),
        (Key::ctrl(CKey::Char('l')), Redraw),
        (Key::plain(CKey::Char('?')), Help),
        (Key::plain(CKey::Char('/')), Palette),
    ]
}

/// Builtin bindings specific to a context (04 §八; a P0 subset). Global keys
/// are intentionally *not* included — they resolve via the global table.
fn builtin_defaults(ctx: KeymapContext) -> Vec<(Key, KeyAction)> {
    use CKey::*;
    use KeyAction::*;
    match ctx {
        KeymapContext::Global => Vec::new(),
        KeymapContext::List => vec![
            (Key::plain(Up), MovePrev),
            (Key::plain(CKey::Char('k')), MovePrev),
            (Key::plain(Down), MoveNext),
            (Key::plain(CKey::Char('j')), MoveNext),
            (Key::plain(Enter), Select),
            (Key::plain(Esc), Back),
            (Key::plain(CKey::Char('q')), Back),
            (Key::plain(CKey::Char('R')), Refresh),
            (Key::plain(CKey::Char('r')), Refresh),
            (Key::plain(CKey::Char('D')), KeyAction::Delete),
            (Key::plain(CKey::Char('N')), New),
        ],
        KeymapContext::Detail => vec![
            (Key::plain(Esc), Back),
            (Key::plain(CKey::Char('q')), Back),
            (Key::plain(CKey::Char('E')), Edit),
            (Key::plain(CKey::Char('D')), KeyAction::Delete),
        ],
        KeymapContext::Chat => vec![
            (Key::plain(Enter), Submit),
            (Key::plain(Up), HistoryPrev),
            (Key::plain(Down), HistoryNext),
            (Key::plain(Esc), Back),
        ],
        KeymapContext::Input => vec![
            (Key::plain(Enter), Submit),
            (Key::plain(Up), HistoryPrev),
            (Key::plain(Down), HistoryNext),
            (Key::plain(Esc), Clear),
        ],
        KeymapContext::Modal => vec![
            (Key::plain(Enter), Submit),
            (Key::plain(Esc), Cancel),
            // approval keys
            (Key::plain(CKey::Char('y')), Approve),
            (Key::plain(CKey::Char('a')), ApproveAll),
            (Key::plain(CKey::Char('n')), Deny),
        ],
        KeymapContext::Composer => vec![
            (Key::plain(Enter), Submit),
            (Key::plain(Esc), Back),
            (Key::plain(Up), HistoryPrev),
            (Key::plain(Down), HistoryNext),
            (Key::ctrl(CKey::Char('u')), Clear),
        ],
        KeymapContext::Panel => vec![
            (Key::plain(Up), MovePrev),
            (Key::plain(CKey::Char('k')), MovePrev),
            (Key::plain(Down), MoveNext),
            (Key::plain(CKey::Char('j')), MoveNext),
            (Key::plain(Enter), Select),
            (Key::plain(Esc), Back),
            (Key::plain(CKey::Char('E')), Edit),
            (Key::plain(CKey::Char('D')), KeyAction::Delete),
            (Key::plain(CKey::Delete), KeyAction::Delete),
            (Key::ctrl(CKey::Char('u')), Clear),
        ],
        KeymapContext::Approval => vec![
            (Key::plain(Enter), Submit),
            (Key::plain(Esc), Cancel),
            (Key::plain(CKey::Char('y')), Approve),
            (Key::plain(CKey::Char('a')), ApproveAll),
            (Key::plain(CKey::Char('n')), Deny),
            (Key::plain(CKey::Char('d')), DenyOnce),
            (Key::plain(CKey::Char('c')), Cancel),
        ],
        KeymapContext::Question => {
            let mut binds = vec![(Key::plain(Enter), Select), (Key::plain(Esc), Cancel)];
            // Digit keys select the corresponding option directly.
            for n in 1..=9u8 {
                let ch = char::from(b'0' + n);
                binds.push((Key::plain(CKey::Char(ch)), Pick(n)));
            }
            binds
        }
    }
}

/// Convenience: build the keymap with every builtin bound, so callers (Stage
/// 6) usually only need `bind`/`set_global` overrides.
pub fn builtin_keymap() -> Keymap {
    let mut km = Keymap::new();
    for (key, action) in builtin_global() {
        km.set_global(key, action);
    }
    for ctx in [
        KeymapContext::List,
        KeymapContext::Detail,
        KeymapContext::Chat,
        KeymapContext::Input,
        KeymapContext::Modal,
        KeymapContext::Composer,
        KeymapContext::Panel,
        KeymapContext::Approval,
        KeymapContext::Question,
    ] {
        for (key, action) in builtin_defaults(ctx) {
            km.bind(ctx, key, action);
        }
    }
    km
}

#[cfg(test)]
mod tests {
    use super::*;
    use CKey::*;
    use KeyAction::*;

    #[test]
    fn resolves_list_navigation_from_defaults() {
        let km = builtin_keymap();
        // no overrides; ensure the 'None' noop sample? just check list keys.
        assert_eq!(km.resolve(KeymapContext::List, Key::plain(Down)), Some(MoveNext));
        assert_eq!(km.resolve(KeymapContext::List, Key::plain(CKey::Char('j'))), Some(MoveNext));
        assert_eq!(km.resolve(KeymapContext::List, Key::plain(Enter)), Some(Select));
    }

    #[test]
    fn context_override_wins_over_global() {
        let mut km = builtin_keymap();
        // global binds '?' to Help; enter it for List and override Down.
        km.set_global(Key::plain(CKey::Char('?')), Redraw);
        // '?' is global; in List it resolves through global override first.
        assert_eq!(km.resolve(KeymapContext::List, Key::plain(CKey::Char('?'))), Some(Redraw));
    }

    #[test]
    fn context_override_beats_builtin() {
        let mut km = builtin_keymap();
        km.bind(KeymapContext::List, Key::plain(Down), MoveNext);
        // rebind to nothing meaningful: check bind changes result.
        km.bind(KeymapContext::List, Key::plain(Down), Clear);
        assert_eq!(km.resolve(KeymapContext::List, Key::plain(Down)), Some(Clear));
    }

    #[test]
    fn global_ctrl_q_is_quit_everywhere() {
        let km = builtin_keymap();
        for ctx in [
            KeymapContext::Global,
            KeymapContext::List,
            KeymapContext::Chat,
            KeymapContext::Modal,
            KeymapContext::Composer,
            KeymapContext::Panel,
            KeymapContext::Approval,
            KeymapContext::Question,
        ] {
            assert_eq!(km.resolve(ctx, Key::ctrl(CKey::Char('q'))), Some(Quit));
        }
    }

    #[test]
    fn unknown_key_is_none() {
        let km = builtin_keymap();
        assert_eq!(
            km.resolve(KeymapContext::Global, Key::ctrl(CKey::Char('z'))),
            Option::None
        );
    }

    #[test]
    fn modal_offers_approval_keys() {
        let km = builtin_keymap();
        assert_eq!(km.resolve(KeymapContext::Modal, Key::plain(CKey::Char('y'))), Some(Approve));
        assert_eq!(km.resolve(KeymapContext::Modal, Key::plain(CKey::Char('n'))), Some(Deny));
    }

    #[test]
    fn composer_offers_submit_history_and_clear() {
        let km = builtin_keymap();
        let ctx = KeymapContext::Composer;
        assert_eq!(km.resolve(ctx, Key::plain(Enter)), Some(Submit));
        assert_eq!(km.resolve(ctx, Key::plain(Esc)), Some(Back));
        assert_eq!(km.resolve(ctx, Key::plain(Up)), Some(HistoryPrev));
        assert_eq!(km.resolve(ctx, Key::plain(Down)), Some(HistoryNext));
        assert_eq!(km.resolve(ctx, Key::ctrl(CKey::Char('u'))), Some(Clear));
    }

    #[test]
    fn composer_leaves_character_input_unbound() {
        // Text entry is handled by the composer itself; the keymap only
        // covers commands, so printable characters resolve to None.
        let km = builtin_keymap();
        assert_eq!(
            km.resolve(KeymapContext::Composer, Key::plain(CKey::Char('a'))),
            Option::None
        );
    }

    #[test]
    fn panel_offers_navigation_edit_and_delete() {
        let km = builtin_keymap();
        let ctx = KeymapContext::Panel;
        assert_eq!(km.resolve(ctx, Key::plain(Up)), Some(MovePrev));
        assert_eq!(km.resolve(ctx, Key::plain(CKey::Char('j'))), Some(MoveNext));
        assert_eq!(km.resolve(ctx, Key::plain(Enter)), Some(Select));
        assert_eq!(km.resolve(ctx, Key::plain(Esc)), Some(Back));
        assert_eq!(km.resolve(ctx, Key::plain(CKey::Char('E'))), Some(Edit));
        assert_eq!(km.resolve(ctx, Key::plain(CKey::Char('D'))), Some(KeyAction::Delete));
        assert_eq!(km.resolve(ctx, Key::plain(CKey::Delete)), Some(KeyAction::Delete));
        assert_eq!(km.resolve(ctx, Key::ctrl(CKey::Char('u'))), Some(Clear));
    }

    #[test]
    fn approval_offers_deny_once_and_cancel() {
        let km = builtin_keymap();
        let ctx = KeymapContext::Approval;
        assert_eq!(km.resolve(ctx, Key::plain(CKey::Char('y'))), Some(Approve));
        assert_eq!(km.resolve(ctx, Key::plain(CKey::Char('a'))), Some(ApproveAll));
        assert_eq!(km.resolve(ctx, Key::plain(CKey::Char('n'))), Some(Deny));
        assert_eq!(km.resolve(ctx, Key::plain(CKey::Char('d'))), Some(DenyOnce));
        assert_eq!(km.resolve(ctx, Key::plain(CKey::Char('c'))), Some(Cancel));
        assert_eq!(km.resolve(ctx, Key::plain(Esc)), Some(Cancel));
        assert_eq!(km.resolve(ctx, Key::plain(Enter)), Some(Submit));
    }

    #[test]
    fn question_offers_numeric_picks() {
        let km = builtin_keymap();
        let ctx = KeymapContext::Question;
        assert_eq!(km.resolve(ctx, Key::plain(CKey::Char('1'))), Some(Pick(1)));
        assert_eq!(km.resolve(ctx, Key::plain(CKey::Char('5'))), Some(Pick(5)));
        assert_eq!(km.resolve(ctx, Key::plain(CKey::Char('9'))), Some(Pick(9)));
        assert_eq!(km.resolve(ctx, Key::plain(Enter)), Some(Select));
        assert_eq!(km.resolve(ctx, Key::plain(Esc)), Some(Cancel));
        // Digits beyond 9 have no pick binding.
        assert_eq!(km.resolve(ctx, Key::plain(CKey::Char('0'))), Option::None);
    }

    #[test]
    fn approval_bindings_override_modal_builtin() {
        // The approval context is a superset of the modal keys (y/a/n/Esc)
        // plus d/c; both resolve without ambiguity.
        let km = builtin_keymap();
        assert_eq!(
            km.resolve(KeymapContext::Approval, Key::plain(CKey::Char('y'))),
            km.resolve(KeymapContext::Modal, Key::plain(CKey::Char('y')))
        );
    }
}