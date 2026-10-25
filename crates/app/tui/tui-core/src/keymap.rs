//! `Keymap`: centralized key resolution with context fallback
//! (components must not scatter bare `match` on keys).
//!
//! Resolution order: `context overrides → global overrides → builtin
//! defaults`. `Key` is a small crossterm-free struct (crossterm events map
//! here), keeping the component layer ownership-clean.

use std::collections::HashMap;

use crate::command::CommandSpec;

/// Physical key code (modifier-free). Kept deliberately small; the crossterm
/// adapter converts `KeyCode` into this.
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

    /// Human-readable description for help displays, e.g. `Ctrl-K`, `Esc`.
    pub fn describe(&self) -> String {
        let base = match self.code {
            CKey::Char(c) => c.to_string(),
            CKey::Enter => "Enter".to_string(),
            CKey::Esc => "Esc".to_string(),
            CKey::Tab => "Tab".to_string(),
            CKey::Up => "Up".to_string(),
            CKey::Down => "Down".to_string(),
            CKey::Left => "Left".to_string(),
            CKey::Right => "Right".to_string(),
            CKey::Home => "Home".to_string(),
            CKey::End => "End".to_string(),
            CKey::PageUp => "PgUp".to_string(),
            CKey::PageDown => "PgDn".to_string(),
            CKey::Backspace => "Bksp".to_string(),
            CKey::Delete => "Del".to_string(),
        };
        let mut out = String::new();
        if self.ctrl {
            out.push_str("Ctrl-");
        }
        if self.alt {
            out.push_str("Alt-");
        }
        if self.shift {
            out.push_str("Shift-");
        }
        out.push_str(&base);
        out
    }
}

/// macOS Option-composed character mapped back to its ASCII base key.
/// Terminal.app and similar terminals report e.g. `å` instead of Alt+A;
/// without this mapping such chords never match their bindings.
pub fn macos_option_char_to_ascii_key(ch: char) -> Option<char> {
    Some(match ch {
        'å' => 'a',
        '∫' => 'b',
        'ç' => 'c',
        '∂' => 'd',
        'ƒ' => 'f',
        '©' => 'g',
        '˙' => 'h',
        '∆' => 'j',
        '˚' => 'k',
        '¬' => 'l',
        'µ' => 'm',
        'ø' => 'o',
        'π' => 'p',
        'œ' => 'q',
        '®' => 'r',
        'ß' => 's',
        '†' => 't',
        '¨' => 'u',
        '√' => 'v',
        '∑' => 'w',
        '≈' => 'x',
        '¥' => 'y',
        'Ω' => 'z',
        _ => return None,
    })
}

/// Normalize a resolved key for platform quirks: on macOS an
/// Option-composed character arriving without explicit modifiers is
/// restored to its Alt+ASCII chord. Other platforms and already-modified
/// keys pass through unchanged.
pub fn normalize_key(key: Key, is_macos: bool) -> Key {
    if !is_macos || key.ctrl || key.alt || key.shift {
        return key;
    }
    if let CKey::Char(ch) = key.code {
        if let Some(ascii) = macos_option_char_to_ascii_key(ch) {
            return Key {
                code: CKey::Char(ascii),
                ctrl: false,
                alt: true,
                shift: false,
            };
        }
    }
    key
}

/// The abstract action a key may trigger.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyAction {
    /// Reserved no-op.
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
    /// Scan the skill directory for new skills.
    Scan,
    /// Reload skills (clear cache + rescan).
    Reload,
    /// Clear the skill content/resource cache.
    CacheClear,
    /// Cycle the active panel's mode (e.g. the mention group filter).
    CycleMode,
    // Approval actions
    Approve,
    ApproveAll,
    /// Deny this tool call only; later calls ask again.
    DenyOnce,
    Deny,
    /// Pick the n-th option in the question view (1..=9).
    Pick(u8),
    /// Open a new chat session tab.
    TabNew,
    /// Close the active chat session tab.
    TabClose,
    /// Focus the next chat session tab.
    TabNext,
    /// Focus the previous chat session tab.
    TabPrev,
    /// Focus the n-th chat session tab (1..=9).
    TabPick(u8),
    /// Toggle pin on the active chat session tab.
    TabPin,
    /// Move the active tab one slot toward the start.
    TabMovePrev,
    /// Move the active tab one slot toward the end.
    TabMoveNext,
    /// Focus the parent tab of the active sub-agent tab.
    TabParent,
    /// Focus the first child tab of the active tab.
    TabChild,
    /// Cycle to the next sibling/child tab.
    TabChildNext,
    /// Cycle to the previous sibling/child tab.
    TabChildPrev,
    /// Open the session list browser overlay.
    SessionList,
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
    /// Single-line prompt composer (TUI footer prompt view, inline form).
    Composer,
    /// Selection panels: model / skill / queued prompts.
    Panel,
    /// Tool approval view (captures keys while active).
    Approval,
    /// Follow-up question view (captures keys while active).
    Question,
}

/// A `(context → key → command)` resolution table with global fallback.
///
/// The table only holds user overrides: builtin bindings resolve from the
/// command registry on demand, so an empty table already answers every
/// builtin chord.
#[derive(Debug, Clone, Default)]
pub struct Keymap {
    global: HashMap<Key, &'static CommandSpec>,
    context: HashMap<KeymapContext, HashMap<Key, &'static CommandSpec>>,
}

impl Keymap {
    pub fn new() -> Self {
        Self::default()
    }

    /// Bind a global override.
    pub fn set_global(&mut self, key: Key, command: &'static CommandSpec) -> &mut Self {
        self.global.insert(key, command);
        self
    }

    /// Bind a context-local override.
    pub fn bind(
        &mut self,
        ctx: KeymapContext,
        key: Key,
        command: &'static CommandSpec,
    ) -> &mut Self {
        self.context.entry(ctx).or_default().insert(key, command);
        self
    }

    /// Bind a context-local override from a command id.
    ///
    /// Unknown ids are rejected so a settings file can never silently bind
    /// a command that no longer exists.
    pub fn bind_id(&mut self, ctx: KeymapContext, key: Key, id: &str) -> bool {
        let Some(command) = crate::command::by_id(id) else {
            return false;
        };
        if ctx == KeymapContext::Global {
            self.set_global(key, command);
        } else {
            self.bind(ctx, key, command);
        }
        true
    }

    /// Resolve `key` in `ctx`. The context is one layer above the global
    /// base, so the same layer machinery serves both ad-hoc lookups and
    /// explicit mode stacks.
    pub fn resolve(&self, ctx: KeymapContext, key: Key) -> Option<&'static CommandSpec> {
        let stack = KeymapStack::base().with(KeyLayer::new(ctx, 10));
        self.resolve_layered(&stack, key)
    }

    /// Resolve to the low-level action when the command has one.
    pub fn resolve_action(&self, ctx: KeymapContext, key: Key) -> Option<KeyAction> {
        self.resolve(ctx, key).and_then(|command| command.action)
    }

    /// Resolve through an ordered layer stack: context overrides first,
    /// then global overrides, then builtins by layer priority, then the
    /// global builtin fallback.
    pub fn resolve_layered(&self, stack: &KeymapStack, key: Key) -> Option<&'static CommandSpec> {
        for layer in stack.layers.iter().filter(|layer| layer.enabled) {
            if let Some(command) = self
                .context
                .get(&layer.context)
                .and_then(|m| m.get(&key))
                .copied()
            {
                return Some(command);
            }
        }
        if let Some(command) = self.global.get(&key).copied() {
            return Some(command);
        }
        for layer in stack.layers.iter().filter(|layer| layer.enabled) {
            if let Some(command) = crate::command::scoped_command(layer.context, key) {
                return Some(command);
            }
        }
        crate::command::scoped_command(KeymapContext::Global, key)
    }
}

/// One layer of the keymap mode stack.
#[derive(Debug, Clone, Copy)]
pub struct KeyLayer {
    pub context: KeymapContext,
    pub priority: u8,
    pub enabled: bool,
}

impl KeyLayer {
    pub const fn new(context: KeymapContext, priority: u8) -> Self {
        Self {
            context,
            priority,
            enabled: true,
        }
    }
}

/// Ordered key layers. Higher priority resolves first; the last layer is
/// the global base.
#[derive(Debug, Clone, Default)]
pub struct KeymapStack {
    layers: Vec<KeyLayer>,
}

impl KeymapStack {
    pub fn new() -> Self {
        Self::default()
    }

    /// Stack with only the global base layer.
    pub fn base() -> Self {
        Self::new().with(KeyLayer::new(KeymapContext::Global, 0))
    }

    /// Push a layer and keep the stack ordered by descending priority.
    pub fn push(&mut self, layer: KeyLayer) {
        self.layers.push(layer);
        self.layers
            .sort_by_key(|layer| std::cmp::Reverse(layer.priority));
    }

    pub fn with(mut self, layer: KeyLayer) -> Self {
        self.push(layer);
        self
    }

    /// Enable or disable every layer using `context`.
    pub fn set_enabled(&mut self, context: KeymapContext, enabled: bool) {
        for layer in &mut self.layers {
            if layer.context == context {
                layer.enabled = enabled;
            }
        }
    }

    pub fn layers(&self) -> &[KeyLayer] {
        &self.layers
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use CKey::*;
    use KeyAction::*;

    #[test]
    fn resolves_list_navigation_from_defaults() {
        let km = Keymap::new();
        // no overrides; ensure the 'None' noop sample? just check list keys.
        assert_eq!(
            km.resolve_action(KeymapContext::List, Key::plain(Down)),
            Some(MoveNext)
        );
        assert_eq!(
            km.resolve_action(KeymapContext::List, Key::plain(CKey::Char('j'))),
            Some(MoveNext)
        );
        assert_eq!(
            km.resolve_action(KeymapContext::List, Key::plain(Enter)),
            Some(Select)
        );
    }

    #[test]
    fn context_override_wins_over_global() {
        let mut km = Keymap::new();
        km.set_global(
            Key::plain(CKey::Char('?')),
            crate::command::by_id("app.redraw").expect("app.redraw is registered"),
        );
        // '?' is a global builtin; the override takes precedence.
        assert_eq!(
            km.resolve_action(KeymapContext::List, Key::plain(CKey::Char('?'))),
            Some(Redraw)
        );
    }

    #[test]
    fn context_override_beats_builtin() {
        let mut km = Keymap::new();
        km.bind(
            KeymapContext::List,
            Key::plain(Down),
            crate::command::by_id("ui.move_next").expect("ui.move_next is registered"),
        );
        km.bind(
            KeymapContext::List,
            Key::plain(Down),
            crate::command::by_id("ui.clear").expect("ui.clear is registered"),
        );
        assert_eq!(
            km.resolve_action(KeymapContext::List, Key::plain(Down)),
            Some(Clear)
        );
    }

    #[test]
    fn global_ctrl_q_is_quit_everywhere() {
        let km = Keymap::new();
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
            assert_eq!(
                km.resolve_action(ctx, Key::ctrl(CKey::Char('q'))),
                Some(Quit)
            );
        }
    }

    #[test]
    fn unknown_key_is_none() {
        let km = Keymap::new();
        assert_eq!(
            km.resolve_action(KeymapContext::Global, Key::ctrl(CKey::Char('z'))),
            Option::None
        );
    }

    #[test]
    fn modal_offers_approval_keys() {
        let km = Keymap::new();
        assert_eq!(
            km.resolve_action(KeymapContext::Modal, Key::plain(CKey::Char('y'))),
            Some(Approve)
        );
        assert_eq!(
            km.resolve_action(KeymapContext::Modal, Key::plain(CKey::Char('n'))),
            Some(Deny)
        );
    }

    #[test]
    fn composer_offers_submit_history_and_clear() {
        let km = Keymap::new();
        let ctx = KeymapContext::Composer;
        assert_eq!(km.resolve_action(ctx, Key::plain(Enter)), Some(Submit));
        assert_eq!(km.resolve_action(ctx, Key::plain(Esc)), Some(Back));
        assert_eq!(km.resolve_action(ctx, Key::plain(Up)), Some(HistoryPrev));
        assert_eq!(km.resolve_action(ctx, Key::plain(Down)), Some(HistoryNext));
        assert_eq!(
            km.resolve_action(ctx, Key::ctrl(CKey::Char('u'))),
            Some(Clear)
        );
    }

    #[test]
    fn composer_leaves_character_input_unbound() {
        // Text entry is handled by the composer itself; the keymap only
        // covers commands, so printable characters resolve to None.
        let km = Keymap::new();
        assert_eq!(
            km.resolve_action(KeymapContext::Composer, Key::plain(CKey::Char('a'))),
            Option::None
        );
    }

    #[test]
    fn panel_offers_navigation_edit_and_delete() {
        let km = Keymap::new();
        let ctx = KeymapContext::Panel;
        assert_eq!(km.resolve_action(ctx, Key::plain(Up)), Some(MovePrev));
        assert_eq!(
            km.resolve_action(ctx, Key::plain(CKey::Char('j'))),
            Some(MoveNext)
        );
        assert_eq!(km.resolve_action(ctx, Key::plain(Enter)), Some(Select));
        assert_eq!(km.resolve_action(ctx, Key::plain(Esc)), Some(Back));
        assert_eq!(
            km.resolve_action(ctx, Key::plain(CKey::Char('E'))),
            Some(Edit)
        );
        assert_eq!(
            km.resolve_action(ctx, Key::plain(CKey::Char('D'))),
            Some(KeyAction::Delete)
        );
        assert_eq!(
            km.resolve_action(ctx, Key::plain(CKey::Delete)),
            Some(KeyAction::Delete)
        );
        assert_eq!(
            km.resolve_action(ctx, Key::ctrl(CKey::Char('u'))),
            Some(Clear)
        );
    }

    #[test]
    fn approval_offers_deny_once_and_cancel() {
        let km = Keymap::new();
        let ctx = KeymapContext::Approval;
        assert_eq!(
            km.resolve_action(ctx, Key::plain(CKey::Char('y'))),
            Some(Approve)
        );
        assert_eq!(
            km.resolve_action(ctx, Key::plain(CKey::Char('a'))),
            Some(ApproveAll)
        );
        assert_eq!(
            km.resolve_action(ctx, Key::plain(CKey::Char('n'))),
            Some(Deny)
        );
        assert_eq!(
            km.resolve_action(ctx, Key::plain(CKey::Char('d'))),
            Some(DenyOnce)
        );
        assert_eq!(
            km.resolve_action(ctx, Key::plain(CKey::Char('c'))),
            Some(Cancel)
        );
        assert_eq!(km.resolve_action(ctx, Key::plain(Esc)), Some(Cancel));
        // The approval view has no submit chord.
        assert_eq!(km.resolve_action(ctx, Key::plain(Enter)), Option::None);
    }

    #[test]
    fn question_offers_numeric_picks() {
        let km = Keymap::new();
        let ctx = KeymapContext::Question;
        assert_eq!(
            km.resolve_action(ctx, Key::plain(CKey::Char('1'))),
            Some(Pick(1))
        );
        assert_eq!(
            km.resolve_action(ctx, Key::plain(CKey::Char('5'))),
            Some(Pick(5))
        );
        assert_eq!(
            km.resolve_action(ctx, Key::plain(CKey::Char('9'))),
            Some(Pick(9))
        );
        assert_eq!(km.resolve_action(ctx, Key::plain(Enter)), Some(Select));
        assert_eq!(km.resolve_action(ctx, Key::plain(Esc)), Some(Cancel));
        // Digits beyond 9 have no pick binding.
        assert_eq!(
            km.resolve_action(ctx, Key::plain(CKey::Char('0'))),
            Option::None
        );
    }

    #[test]
    fn approval_bindings_override_modal_builtin() {
        // The approval context is a superset of the modal keys (y/a/n/Esc)
        // plus d/c; both resolve without ambiguity.
        let km = Keymap::new();
        assert_eq!(
            km.resolve_action(KeymapContext::Approval, Key::plain(CKey::Char('y'))),
            km.resolve_action(KeymapContext::Modal, Key::plain(CKey::Char('y')))
        );
    }

    #[test]
    fn tab_chords_resolve_globally() {
        let km = Keymap::new();
        let alt = |code| Key {
            code,
            ctrl: false,
            alt: true,
            shift: false,
        };
        assert_eq!(
            km.resolve_action(KeymapContext::Chat, alt(CKey::Char('1'))),
            Some(TabPick(1))
        );
        assert_eq!(
            km.resolve_action(KeymapContext::List, alt(CKey::Char('9'))),
            Some(TabPick(9))
        );
        assert_eq!(
            km.resolve_action(KeymapContext::Chat, alt(CKey::Char('t'))),
            Some(TabNew)
        );
        assert_eq!(
            km.resolve_action(KeymapContext::Chat, alt(CKey::Char('T'))),
            Some(TabNew)
        );
        assert_eq!(
            km.resolve_action(KeymapContext::Chat, alt(CKey::Char('w'))),
            Some(TabClose)
        );
        assert_eq!(
            km.resolve_action(KeymapContext::Chat, alt(CKey::Char(']'))),
            Some(TabNext)
        );
        assert_eq!(
            km.resolve_action(KeymapContext::Chat, alt(CKey::Char('['))),
            Some(TabPrev)
        );
        assert_eq!(
            km.resolve_action(KeymapContext::Chat, alt(CKey::Right)),
            Some(TabNext)
        );
        assert_eq!(
            km.resolve_action(KeymapContext::Chat, alt(CKey::Left)),
            Some(TabPrev)
        );
        assert_eq!(
            km.resolve_action(KeymapContext::Chat, alt(CKey::Char('p'))),
            Some(TabPin)
        );
        assert_eq!(
            km.resolve_action(KeymapContext::Chat, alt(CKey::Char('s'))),
            Some(SessionList)
        );
        assert_eq!(
            km.resolve_action(KeymapContext::Chat, alt(CKey::Char(','))),
            Some(TabMovePrev)
        );
        assert_eq!(
            km.resolve_action(KeymapContext::Chat, alt(CKey::Up)),
            Some(TabParent)
        );
        assert_eq!(
            km.resolve_action(KeymapContext::Chat, alt(CKey::Down)),
            Some(TabChild)
        );
    }

    #[test]
    fn macos_option_chars_map_back_to_ascii() {
        assert_eq!(macos_option_char_to_ascii_key('å'), Some('a'));
        assert_eq!(macos_option_char_to_ascii_key('ß'), Some('s'));
        assert_eq!(macos_option_char_to_ascii_key('∂'), Some('d'));
        assert_eq!(macos_option_char_to_ascii_key('a'), Option::None);
        assert_eq!(macos_option_char_to_ascii_key('1'), Option::None);
    }

    #[test]
    fn normalize_restores_alt_chords_on_macos_only() {
        let composed = Key::plain(CKey::Char('å'));
        let normalized = normalize_key(composed, true);
        assert_eq!(
            normalized,
            Key {
                code: CKey::Char('a'),
                ctrl: false,
                alt: true,
                shift: false,
            }
        );
        assert_eq!(normalize_key(composed, false), composed);
        let plain = Key::plain(CKey::Char('a'));
        assert_eq!(normalize_key(plain, true), plain);
        let modified = Key::ctrl(CKey::Char('å'));
        assert_eq!(normalize_key(modified, true), modified);
    }

    #[test]
    fn stack_resolves_by_priority_and_respects_disabled_layers() {
        let km = Keymap::new();
        let mut stack = KeymapStack::base().with(KeyLayer::new(KeymapContext::Panel, 10));
        assert_eq!(
            km.resolve_layered(&stack, Key::plain(Down))
                .map(|command| command.id),
            Some("ui.move_next")
        );
        stack.set_enabled(KeymapContext::Panel, false);
        assert!(km.resolve_layered(&stack, Key::plain(Down)).is_none());
    }
}
