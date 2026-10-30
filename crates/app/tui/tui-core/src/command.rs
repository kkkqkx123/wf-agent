//! The command registry: the single definition of every user-facing command.
//!
//! A command carries its stable id, display metadata, default key scopes,
//! leader chord, slash name and whether it appears in the command palette.
//! The keymap, the command palette, the slash parser and the help view all
//! derive from this table, so a new command is added in exactly one place.
//!
//! Execution is addressed by id: commands that map onto a low-level
//! [`KeyAction`] carry one, while session commands are dispatched by the
//! caller from the id alone.

use crate::keymap::{CKey, Key, KeyAction, KeymapContext};

/// The key that starts a leader sequence.
///
/// `Ctrl-X` is already the transcript decision-anchor shortcut, so the
/// leader uses the free `Ctrl-W`.
pub const LEADER: Key = Key::ctrl(CKey::Char('w'));

/// Leader sequence timeout in milliseconds.
pub const LEADER_TIMEOUT_MS: u64 = 2_000;

/// One key scope: the context a group of keys is active in.
#[derive(Debug, Clone, Copy)]
pub struct BindingScope {
    pub context: KeymapContext,
    pub keys: &'static [Key],
}

const fn scope(context: KeymapContext, keys: &'static [Key]) -> BindingScope {
    BindingScope { context, keys }
}

/// Slash exposure of a command.
#[derive(Debug, Clone, Copy)]
pub struct SlashSpec {
    /// Primary name without the leading slash.
    pub name: &'static str,
    /// Alternative names, all without the leading slash.
    pub aliases: &'static [&'static str],
    /// Argument syntax shown in help (`""` when the command takes none).
    pub argument: &'static str,
}

/// One command definition.
#[derive(Debug, Clone, Copy)]
pub struct CommandSpec {
    pub id: &'static str,
    pub title: &'static str,
    pub group: &'static str,
    pub description: &'static str,
    /// Low-level action for commands the UI can execute directly.
    pub action: Option<KeyAction>,
    pub scopes: &'static [BindingScope],
    /// Keys pressed after the leader, empty when the command has no chord.
    pub leader: &'static [Key],
    pub slash: Option<SlashSpec>,
    pub palette: bool,
}

impl CommandSpec {
    /// First default chord, used for shortcut hints.
    pub fn shortcut(&self) -> Option<String> {
        if let Some(scope) = self.scopes.iter().find(|scope| !scope.keys.is_empty()) {
            return scope.keys.first().map(Key::describe);
        }
        self.leader
            .first()
            .map(|key| format!("{} {}", LEADER.describe(), key.describe()))
    }
}

const fn key(code: CKey) -> Key {
    Key::plain(code)
}

const fn ctrl(c: char) -> Key {
    Key::ctrl(CKey::Char(c))
}

const fn alt(code: CKey) -> Key {
    Key {
        code,
        ctrl: false,
        alt: true,
        shift: false,
    }
}

const fn alt_char(c: char) -> Key {
    alt(CKey::Char(c))
}

/// Every command. Order defines palette group and row order.
pub static COMMANDS: &[CommandSpec] = &[
    // Application.
    CommandSpec {
        id: "app.quit",
        title: "Quit",
        group: "App",
        description: "Exit the TUI",
        action: Some(KeyAction::Quit),
        scopes: &[scope(KeymapContext::Global, &[ctrl('q')])],
        leader: &[key(CKey::Char('q'))],
        slash: Some(SlashSpec {
            name: "quit",
            aliases: &["q", "exit"],
            argument: "",
        }),
        palette: true,
    },
    CommandSpec {
        id: "app.help",
        title: "Help",
        group: "App",
        description: "Show keys and commands",
        action: Some(KeyAction::Help),
        scopes: &[scope(KeymapContext::Global, &[key(CKey::Char('?'))])],
        leader: &[key(CKey::Char('h'))],
        slash: Some(SlashSpec {
            name: "help",
            aliases: &["?"],
            argument: "",
        }),
        palette: true,
    },
    CommandSpec {
        id: "app.redraw",
        title: "Redraw",
        group: "App",
        description: "Repaint the screen",
        action: Some(KeyAction::Redraw),
        scopes: &[scope(KeymapContext::Global, &[ctrl('l')])],
        leader: &[],
        slash: None,
        palette: false,
    },
    CommandSpec {
        id: "app.interrupt",
        title: "Interrupt",
        group: "App",
        description: "Interrupt the running turn",
        action: Some(KeyAction::Interrupt),
        scopes: &[scope(KeymapContext::Global, &[ctrl('c')])],
        leader: &[],
        slash: None,
        palette: false,
    },
    CommandSpec {
        id: "command.palette",
        title: "Command palette",
        group: "App",
        description: "Search and run a command",
        action: Some(KeyAction::Palette),
        scopes: &[scope(KeymapContext::Global, &[ctrl('p')])],
        leader: &[key(CKey::Char('p'))],
        slash: None,
        palette: false,
    },
    // Session.
    CommandSpec {
        id: "session.new",
        title: "New session",
        group: "Session",
        description: "Start a new session tab",
        action: None,
        scopes: &[],
        leader: &[key(CKey::Char('n'))],
        slash: Some(SlashSpec {
            name: "new",
            aliases: &["n"],
            argument: "",
        }),
        palette: true,
    },
    CommandSpec {
        id: "session.list",
        title: "Sessions",
        group: "Session",
        description: "Browse and switch sessions",
        action: Some(KeyAction::SessionList),
        scopes: &[scope(
            KeymapContext::Global,
            &[alt_char('s'), alt_char('S')],
        )],
        leader: &[key(CKey::Char('l'))],
        slash: Some(SlashSpec {
            name: "sessions",
            aliases: &["list", "session"],
            argument: "",
        }),
        palette: true,
    },
    CommandSpec {
        id: "session.model",
        title: "Model",
        group: "Session",
        description: "Pick the model profile for the next turns",
        action: None,
        scopes: &[],
        leader: &[key(CKey::Char('m'))],
        slash: Some(SlashSpec {
            name: "model",
            aliases: &["m"],
            argument: "",
        }),
        palette: true,
    },
    CommandSpec {
        id: "session.skills",
        title: "Skills",
        group: "Session",
        description: "List and run a skill",
        action: None,
        scopes: &[],
        leader: &[key(CKey::Char('s'))],
        slash: Some(SlashSpec {
            name: "skills",
            aliases: &["skill"],
            argument: "",
        }),
        palette: true,
    },
    CommandSpec {
        id: "session.workflows",
        title: "Workflows",
        group: "Session",
        description: "List and run a workflow",
        action: None,
        scopes: &[],
        leader: &[key(CKey::Char('w'))],
        slash: Some(SlashSpec {
            name: "workflows",
            aliases: &["workflow"],
            argument: "",
        }),
        palette: true,
    },
    CommandSpec {
        id: "session.queued",
        title: "Queued prompts",
        group: "Session",
        description: "Manage queued prompts",
        action: None,
        scopes: &[],
        leader: &[key(CKey::Char('u'))],
        slash: Some(SlashSpec {
            name: "queued",
            aliases: &["queue"],
            argument: "",
        }),
        palette: true,
    },
    CommandSpec {
        id: "session.editor",
        title: "Editor",
        group: "Session",
        description: "Edit the draft in $EDITOR",
        action: None,
        scopes: &[],
        leader: &[key(CKey::Char('e'))],
        slash: Some(SlashSpec {
            name: "editor",
            aliases: &["edit"],
            argument: "",
        }),
        palette: true,
    },
    CommandSpec {
        id: "session.auth",
        title: "Auth",
        group: "Session",
        description: "Log in or out of LLM providers",
        action: None,
        scopes: &[],
        leader: &[key(CKey::Char('a'))],
        slash: Some(SlashSpec {
            name: "auth",
            aliases: &["login"],
            argument: "",
        }),
        palette: true,
    },
    CommandSpec {
        id: "session.copy",
        title: "Copy",
        group: "Session",
        description: "Copy a message, code block or the whole transcript",
        action: None,
        scopes: &[],
        leader: &[key(CKey::Char('y'))],
        slash: Some(SlashSpec {
            name: "copy",
            aliases: &["y"],
            argument: "message | code [N] | all | id",
        }),
        palette: true,
    },
    CommandSpec {
        id: "session.web",
        title: "Web link",
        group: "Session",
        description: "Copy the web conversation link for this session",
        action: None,
        scopes: &[],
        leader: &[key(CKey::Char('o'))],
        slash: Some(SlashSpec {
            name: "web",
            aliases: &["open-web"],
            argument: "",
        }),
        palette: true,
    },
    CommandSpec {
        id: "session.resume",
        title: "Resume",
        group: "Session",
        description: "Resume a recent session",
        action: None,
        scopes: &[],
        leader: &[key(CKey::Char('r'))],
        slash: Some(SlashSpec {
            name: "resume",
            aliases: &["recent"],
            argument: "",
        }),
        palette: true,
    },
    CommandSpec {
        id: "session.recap",
        title: "Recap",
        group: "Session",
        description: "Show a one-line recap of where the turn left off",
        action: None,
        scopes: &[],
        leader: &[],
        slash: Some(SlashSpec {
            name: "recap",
            aliases: &[],
            argument: "",
        }),
        palette: true,
    },
    CommandSpec {
        id: "session.tree",
        title: "Session tree",
        group: "Session",
        description: "Browse this session as a navigable tree (also on double-Esc)",
        action: None,
        scopes: &[],
        leader: &[key(CKey::Char('b'))],
        slash: Some(SlashSpec {
            name: "tree",
            aliases: &[],
            argument: "",
        }),
        palette: true,
    },
    CommandSpec {
        id: "session.fork",
        title: "Branch to new tab",
        group: "Session",
        description: "Branch this session into a new tab (history is preserved)",
        action: None,
        scopes: &[],
        leader: &[key(CKey::Char('f'))],
        slash: Some(SlashSpec {
            name: "fork",
            aliases: &["branch"],
            argument: "",
        }),
        palette: true,
    },
    CommandSpec {
        id: "agent.append_mode_steering",
        title: "Append Mode: Steering",
        group: "Agent",
        description: "Set runtime append mode to steering",
        action: None,
        scopes: &[],
        leader: &[],
        slash: Some(SlashSpec {
            name: "append-mode-steering",
            aliases: &[],
            argument: "",
        }),
        palette: true,
    },
    CommandSpec {
        id: "agent.append_mode_followup",
        title: "Append Mode: Follow-up",
        group: "Agent",
        description: "Set runtime append mode to follow-up",
        action: None,
        scopes: &[],
        leader: &[],
        slash: Some(SlashSpec {
            name: "append-mode-followup",
            aliases: &[],
            argument: "",
        }),
        palette: true,
    },
    CommandSpec {
        id: "agent.convert_pending",
        title: "Convert Pending to Steering",
        group: "Agent",
        description: "Move buffered follow-ups into steering",
        action: None,
        scopes: &[],
        leader: &[],
        slash: Some(SlashSpec {
            name: "convert-pending",
            aliases: &["steer-pending"],
            argument: "",
        }),
        palette: true,
    },
    CommandSpec {
        id: "session.executions",
        title: "Executions",
        group: "Session",
        description: "List recent executions",
        action: None,
        scopes: &[],
        leader: &[key(CKey::Char('x'))],
        slash: Some(SlashSpec {
            name: "executions",
            aliases: &["exec"],
            argument: "",
        }),
        palette: true,
    },
    // Tabs.
    CommandSpec {
        id: "tab.new",
        title: "New tab",
        group: "Tabs",
        description: "Open a new session tab",
        action: Some(KeyAction::TabNew),
        scopes: &[scope(
            KeymapContext::Global,
            &[alt_char('t'), alt_char('T')],
        )],
        leader: &[key(CKey::Char('t'))],
        slash: None,
        palette: false,
    },
    CommandSpec {
        id: "tab.close",
        title: "Close tab",
        group: "Tabs",
        description: "Close the active session tab",
        action: Some(KeyAction::TabClose),
        scopes: &[scope(
            KeymapContext::Global,
            &[alt_char('w'), alt_char('W')],
        )],
        leader: &[key(CKey::Char('c'))],
        slash: None,
        palette: false,
    },
    CommandSpec {
        id: "tab.next",
        title: "Next tab",
        group: "Tabs",
        description: "Focus the next session tab",
        action: Some(KeyAction::TabNext),
        scopes: &[scope(
            KeymapContext::Global,
            &[alt_char(']'), alt(CKey::Right)],
        )],
        leader: &[key(CKey::Char('.'))],
        slash: None,
        palette: false,
    },
    CommandSpec {
        id: "tab.prev",
        title: "Previous tab",
        group: "Tabs",
        description: "Focus the previous session tab",
        action: Some(KeyAction::TabPrev),
        scopes: &[scope(
            KeymapContext::Global,
            &[alt_char('['), alt(CKey::Left)],
        )],
        leader: &[key(CKey::Char(','))],
        slash: None,
        palette: false,
    },
    CommandSpec {
        id: "tab.pick.1",
        title: "Pick tab 1",
        group: "Tabs",
        description: "Focus session tab 1",
        action: Some(KeyAction::TabPick(1)),
        scopes: &[scope(KeymapContext::Global, &[alt_char('1')])],
        leader: &[],
        slash: None,
        palette: false,
    },
    CommandSpec {
        id: "tab.pick.2",
        title: "Pick tab 2",
        group: "Tabs",
        description: "Focus session tab 2",
        action: Some(KeyAction::TabPick(2)),
        scopes: &[scope(KeymapContext::Global, &[alt_char('2')])],
        leader: &[],
        slash: None,
        palette: false,
    },
    CommandSpec {
        id: "tab.pick.3",
        title: "Pick tab 3",
        group: "Tabs",
        description: "Focus session tab 3",
        action: Some(KeyAction::TabPick(3)),
        scopes: &[scope(KeymapContext::Global, &[alt_char('3')])],
        leader: &[],
        slash: None,
        palette: false,
    },
    CommandSpec {
        id: "tab.pick.4",
        title: "Pick tab 4",
        group: "Tabs",
        description: "Focus session tab 4",
        action: Some(KeyAction::TabPick(4)),
        scopes: &[scope(KeymapContext::Global, &[alt_char('4')])],
        leader: &[],
        slash: None,
        palette: false,
    },
    CommandSpec {
        id: "tab.pick.5",
        title: "Pick tab 5",
        group: "Tabs",
        description: "Focus session tab 5",
        action: Some(KeyAction::TabPick(5)),
        scopes: &[scope(KeymapContext::Global, &[alt_char('5')])],
        leader: &[],
        slash: None,
        palette: false,
    },
    CommandSpec {
        id: "tab.pick.6",
        title: "Pick tab 6",
        group: "Tabs",
        description: "Focus session tab 6",
        action: Some(KeyAction::TabPick(6)),
        scopes: &[scope(KeymapContext::Global, &[alt_char('6')])],
        leader: &[],
        slash: None,
        palette: false,
    },
    CommandSpec {
        id: "tab.pick.7",
        title: "Pick tab 7",
        group: "Tabs",
        description: "Focus session tab 7",
        action: Some(KeyAction::TabPick(7)),
        scopes: &[scope(KeymapContext::Global, &[alt_char('7')])],
        leader: &[],
        slash: None,
        palette: false,
    },
    CommandSpec {
        id: "tab.pick.8",
        title: "Pick tab 8",
        group: "Tabs",
        description: "Focus session tab 8",
        action: Some(KeyAction::TabPick(8)),
        scopes: &[scope(KeymapContext::Global, &[alt_char('8')])],
        leader: &[],
        slash: None,
        palette: false,
    },
    CommandSpec {
        id: "tab.pick.9",
        title: "Pick tab 9",
        group: "Tabs",
        description: "Focus session tab 9",
        action: Some(KeyAction::TabPick(9)),
        scopes: &[scope(KeymapContext::Global, &[alt_char('9')])],
        leader: &[],
        slash: None,
        palette: false,
    },
    CommandSpec {
        id: "tab.pin",
        title: "Pin tab",
        group: "Tabs",
        description: "Toggle pin on the active tab",
        action: Some(KeyAction::TabPin),
        scopes: &[scope(
            KeymapContext::Global,
            &[alt_char('p'), alt_char('P')],
        )],
        leader: &[],
        slash: None,
        palette: false,
    },
    CommandSpec {
        id: "tab.move_prev",
        title: "Move tab left",
        group: "Tabs",
        description: "Move the active tab one slot toward the start",
        action: Some(KeyAction::TabMovePrev),
        scopes: &[scope(KeymapContext::Global, &[alt_char(',')])],
        leader: &[],
        slash: None,
        palette: false,
    },
    CommandSpec {
        id: "tab.move_next",
        title: "Move tab right",
        group: "Tabs",
        description: "Move the active tab one slot toward the end",
        action: Some(KeyAction::TabMoveNext),
        scopes: &[scope(KeymapContext::Global, &[alt_char('.')])],
        leader: &[],
        slash: None,
        palette: false,
    },
    CommandSpec {
        id: "tab.parent",
        title: "Parent tab",
        group: "Tabs",
        description: "Focus the parent of the active sub-agent tab",
        action: Some(KeyAction::TabParent),
        scopes: &[scope(KeymapContext::Global, &[alt(CKey::Up)])],
        leader: &[],
        slash: None,
        palette: false,
    },
    CommandSpec {
        id: "tab.child",
        title: "Child tab",
        group: "Tabs",
        description: "Focus the first child of the active tab",
        action: Some(KeyAction::TabChild),
        scopes: &[scope(KeymapContext::Global, &[alt(CKey::Down)])],
        leader: &[],
        slash: None,
        palette: false,
    },
    CommandSpec {
        id: "tab.child_next",
        title: "Next child tab",
        group: "Tabs",
        description: "Cycle to the next child tab",
        action: Some(KeyAction::TabChildNext),
        scopes: &[scope(
            KeymapContext::Global,
            &[alt_char('j'), alt_char('J')],
        )],
        leader: &[],
        slash: None,
        palette: false,
    },
    CommandSpec {
        id: "tab.child_prev",
        title: "Previous child tab",
        group: "Tabs",
        description: "Cycle to the previous child tab",
        action: Some(KeyAction::TabChildPrev),
        scopes: &[scope(
            KeymapContext::Global,
            &[alt_char('k'), alt_char('K')],
        )],
        leader: &[],
        slash: None,
        palette: false,
    },
    // Shared navigation.
    CommandSpec {
        id: "ui.move_prev",
        title: "Move up",
        group: "Navigation",
        description: "Move the selection up",
        action: Some(KeyAction::MovePrev),
        scopes: &[
            scope(KeymapContext::List, &[key(CKey::Up), key(CKey::Char('k'))]),
            scope(KeymapContext::Panel, &[key(CKey::Up), key(CKey::Char('k'))]),
        ],
        leader: &[],
        slash: None,
        palette: false,
    },
    CommandSpec {
        id: "ui.move_next",
        title: "Move down",
        group: "Navigation",
        description: "Move the selection down",
        action: Some(KeyAction::MoveNext),
        scopes: &[
            scope(
                KeymapContext::List,
                &[key(CKey::Down), key(CKey::Char('j'))],
            ),
            scope(
                KeymapContext::Panel,
                &[key(CKey::Down), key(CKey::Char('j'))],
            ),
        ],
        leader: &[],
        slash: None,
        palette: false,
    },
    CommandSpec {
        id: "ui.select",
        title: "Select",
        group: "Navigation",
        description: "Accept the selected item",
        action: Some(KeyAction::Select),
        scopes: &[
            scope(KeymapContext::List, &[key(CKey::Enter)]),
            scope(KeymapContext::Panel, &[key(CKey::Enter)]),
            scope(KeymapContext::Question, &[key(CKey::Enter)]),
        ],
        leader: &[],
        slash: None,
        palette: false,
    },
    CommandSpec {
        id: "ui.back",
        title: "Back",
        group: "Navigation",
        description: "Leave the current view",
        action: Some(KeyAction::Back),
        scopes: &[
            scope(KeymapContext::List, &[key(CKey::Esc), key(CKey::Char('q'))]),
            scope(
                KeymapContext::Detail,
                &[key(CKey::Esc), key(CKey::Char('q'))],
            ),
            scope(KeymapContext::Chat, &[key(CKey::Esc)]),
            scope(KeymapContext::Composer, &[key(CKey::Esc)]),
            scope(KeymapContext::Panel, &[key(CKey::Esc)]),
        ],
        leader: &[],
        slash: None,
        palette: false,
    },
    CommandSpec {
        id: "ui.cancel",
        title: "Cancel",
        group: "Navigation",
        description: "Dismiss the active modal",
        action: Some(KeyAction::Cancel),
        scopes: &[
            scope(KeymapContext::Modal, &[key(CKey::Esc)]),
            scope(KeymapContext::Approval, &[key(CKey::Esc)]),
            scope(KeymapContext::Question, &[key(CKey::Esc)]),
        ],
        leader: &[],
        slash: None,
        palette: false,
    },
    CommandSpec {
        id: "ui.submit",
        title: "Submit",
        group: "Navigation",
        description: "Submit the current input",
        action: Some(KeyAction::Submit),
        scopes: &[
            scope(KeymapContext::Chat, &[key(CKey::Enter)]),
            scope(KeymapContext::Input, &[key(CKey::Enter)]),
            scope(KeymapContext::Composer, &[key(CKey::Enter)]),
            scope(KeymapContext::Modal, &[key(CKey::Enter)]),
        ],
        leader: &[],
        slash: None,
        palette: false,
    },
    CommandSpec {
        id: "ui.refresh",
        title: "Refresh",
        group: "Navigation",
        description: "Reload the current list",
        action: Some(KeyAction::Refresh),
        scopes: &[scope(
            KeymapContext::List,
            &[key(CKey::Char('R')), key(CKey::Char('r'))],
        )],
        leader: &[],
        slash: None,
        palette: false,
    },
    CommandSpec {
        id: "ui.delete",
        title: "Delete",
        group: "Navigation",
        description: "Delete the selected item",
        action: Some(KeyAction::Delete),
        scopes: &[
            scope(KeymapContext::List, &[key(CKey::Char('D'))]),
            scope(KeymapContext::Detail, &[key(CKey::Char('D'))]),
            scope(
                KeymapContext::Panel,
                &[key(CKey::Char('D')), key(CKey::Delete)],
            ),
        ],
        leader: &[],
        slash: None,
        palette: false,
    },
    CommandSpec {
        id: "ui.new",
        title: "New",
        group: "Navigation",
        description: "Create a new item",
        action: Some(KeyAction::New),
        scopes: &[scope(KeymapContext::List, &[key(CKey::Char('N'))])],
        leader: &[],
        slash: None,
        palette: false,
    },
    CommandSpec {
        id: "ui.edit",
        title: "Edit",
        group: "Navigation",
        description: "Edit the selected item",
        action: Some(KeyAction::Edit),
        scopes: &[
            scope(KeymapContext::Detail, &[key(CKey::Char('E'))]),
            scope(KeymapContext::Panel, &[key(CKey::Char('E'))]),
        ],
        leader: &[],
        slash: None,
        palette: false,
    },
    CommandSpec {
        id: "ui.history_prev",
        title: "Previous history",
        group: "Navigation",
        description: "Recall the previous input",
        action: Some(KeyAction::HistoryPrev),
        scopes: &[
            scope(KeymapContext::Chat, &[key(CKey::Up)]),
            scope(KeymapContext::Input, &[key(CKey::Up)]),
            scope(KeymapContext::Composer, &[key(CKey::Up)]),
        ],
        leader: &[],
        slash: None,
        palette: false,
    },
    CommandSpec {
        id: "ui.history_next",
        title: "Next history",
        group: "Navigation",
        description: "Recall the next input",
        action: Some(KeyAction::HistoryNext),
        scopes: &[
            scope(KeymapContext::Chat, &[key(CKey::Down)]),
            scope(KeymapContext::Input, &[key(CKey::Down)]),
            scope(KeymapContext::Composer, &[key(CKey::Down)]),
        ],
        leader: &[],
        slash: None,
        palette: false,
    },
    CommandSpec {
        id: "ui.clear",
        title: "Clear input",
        group: "Navigation",
        description: "Clear the current input or filter",
        action: Some(KeyAction::Clear),
        scopes: &[
            scope(KeymapContext::Input, &[key(CKey::Esc)]),
            scope(KeymapContext::Composer, &[ctrl('u')]),
            scope(KeymapContext::Panel, &[ctrl('u')]),
        ],
        leader: &[],
        slash: None,
        palette: false,
    },
    CommandSpec {
        id: "ui.scan",
        title: "Scan",
        group: "Navigation",
        description: "Scan for new items",
        action: Some(KeyAction::Scan),
        scopes: &[scope(KeymapContext::Panel, &[key(CKey::Char('S'))])],
        leader: &[],
        slash: None,
        palette: false,
    },
    CommandSpec {
        id: "ui.reload",
        title: "Reload",
        group: "Navigation",
        description: "Clear caches and reload",
        action: Some(KeyAction::Reload),
        scopes: &[scope(KeymapContext::Panel, &[key(CKey::Char('R'))])],
        leader: &[],
        slash: None,
        palette: false,
    },
    CommandSpec {
        id: "ui.cache_clear",
        title: "Clear cache",
        group: "Navigation",
        description: "Drop cached content",
        action: Some(KeyAction::CacheClear),
        scopes: &[scope(KeymapContext::Panel, &[key(CKey::Char('C'))])],
        leader: &[],
        slash: None,
        palette: false,
    },
    CommandSpec {
        id: "ui.cycle_mode",
        title: "Cycle mode",
        group: "Navigation",
        description: "Cycle the active panel mode",
        action: Some(KeyAction::CycleMode),
        scopes: &[scope(KeymapContext::Panel, &[key(CKey::Tab)])],
        leader: &[],
        slash: None,
        palette: false,
    },
    // Approval.
    CommandSpec {
        id: "approval.approve",
        title: "Approve",
        group: "Approval",
        description: "Allow this call once",
        action: Some(KeyAction::Approve),
        scopes: &[
            scope(KeymapContext::Modal, &[key(CKey::Char('y'))]),
            scope(KeymapContext::Approval, &[key(CKey::Char('y'))]),
        ],
        leader: &[],
        slash: None,
        palette: false,
    },
    CommandSpec {
        id: "approval.approve_all",
        title: "Approve all",
        group: "Approval",
        description: "Allow this tool for the session",
        action: Some(KeyAction::ApproveAll),
        scopes: &[
            scope(KeymapContext::Modal, &[key(CKey::Char('a'))]),
            scope(KeymapContext::Approval, &[key(CKey::Char('a'))]),
        ],
        leader: &[],
        slash: None,
        palette: false,
    },
    CommandSpec {
        id: "approval.deny",
        title: "Deny all",
        group: "Approval",
        description: "Deny this tool for the session",
        action: Some(KeyAction::Deny),
        scopes: &[
            scope(KeymapContext::Modal, &[key(CKey::Char('n'))]),
            scope(KeymapContext::Approval, &[key(CKey::Char('n'))]),
        ],
        leader: &[],
        slash: None,
        palette: false,
    },
    CommandSpec {
        id: "approval.deny_once",
        title: "Deny once",
        group: "Approval",
        description: "Deny this call only",
        action: Some(KeyAction::DenyOnce),
        scopes: &[scope(KeymapContext::Approval, &[key(CKey::Char('d'))])],
        leader: &[],
        slash: None,
        palette: false,
    },
    CommandSpec {
        id: "approval.cancel",
        title: "Cancel approval",
        group: "Approval",
        description: "Dismiss the approval request",
        action: Some(KeyAction::Cancel),
        scopes: &[scope(KeymapContext::Approval, &[key(CKey::Char('c'))])],
        leader: &[],
        slash: None,
        palette: false,
    },
    // Question picks.
    CommandSpec {
        id: "question.pick.1",
        title: "Pick option 1",
        group: "Question",
        description: "Choose the first option",
        action: Some(KeyAction::Pick(1)),
        scopes: &[scope(KeymapContext::Question, &[key(CKey::Char('1'))])],
        leader: &[],
        slash: None,
        palette: false,
    },
    CommandSpec {
        id: "question.pick.2",
        title: "Pick option 2",
        group: "Question",
        description: "Choose the second option",
        action: Some(KeyAction::Pick(2)),
        scopes: &[scope(KeymapContext::Question, &[key(CKey::Char('2'))])],
        leader: &[],
        slash: None,
        palette: false,
    },
    CommandSpec {
        id: "question.pick.3",
        title: "Pick option 3",
        group: "Question",
        description: "Choose the third option",
        action: Some(KeyAction::Pick(3)),
        scopes: &[scope(KeymapContext::Question, &[key(CKey::Char('3'))])],
        leader: &[],
        slash: None,
        palette: false,
    },
    CommandSpec {
        id: "question.pick.4",
        title: "Pick option 4",
        group: "Question",
        description: "Choose the fourth option",
        action: Some(KeyAction::Pick(4)),
        scopes: &[scope(KeymapContext::Question, &[key(CKey::Char('4'))])],
        leader: &[],
        slash: None,
        palette: false,
    },
    CommandSpec {
        id: "question.pick.5",
        title: "Pick option 5",
        group: "Question",
        description: "Choose the fifth option",
        action: Some(KeyAction::Pick(5)),
        scopes: &[scope(KeymapContext::Question, &[key(CKey::Char('5'))])],
        leader: &[],
        slash: None,
        palette: false,
    },
    CommandSpec {
        id: "question.pick.6",
        title: "Pick option 6",
        group: "Question",
        description: "Choose the sixth option",
        action: Some(KeyAction::Pick(6)),
        scopes: &[scope(KeymapContext::Question, &[key(CKey::Char('6'))])],
        leader: &[],
        slash: None,
        palette: false,
    },
    CommandSpec {
        id: "question.pick.7",
        title: "Pick option 7",
        group: "Question",
        description: "Choose the seventh option",
        action: Some(KeyAction::Pick(7)),
        scopes: &[scope(KeymapContext::Question, &[key(CKey::Char('7'))])],
        leader: &[],
        slash: None,
        palette: false,
    },
    CommandSpec {
        id: "question.pick.8",
        title: "Pick option 8",
        group: "Question",
        description: "Choose the eighth option",
        action: Some(KeyAction::Pick(8)),
        scopes: &[scope(KeymapContext::Question, &[key(CKey::Char('8'))])],
        leader: &[],
        slash: None,
        palette: false,
    },
    CommandSpec {
        id: "question.pick.9",
        title: "Pick option 9",
        group: "Question",
        description: "Choose the ninth option",
        action: Some(KeyAction::Pick(9)),
        scopes: &[scope(KeymapContext::Question, &[key(CKey::Char('9'))])],
        leader: &[],
        slash: None,
        palette: false,
    },
];

/// Look a command up by its stable id.
pub fn by_id(id: &str) -> Option<&'static CommandSpec> {
    COMMANDS.iter().find(|spec| spec.id == id)
}

/// The builtin command active for `key` in `context`, if any.
pub fn scoped_command(context: KeymapContext, key: Key) -> Option<&'static CommandSpec> {
    COMMANDS.iter().find(|spec| {
        spec.scopes
            .iter()
            .any(|scope| scope.context == context && scope.keys.contains(&key))
    })
}

/// Commands shown in the palette, in registry order.
pub fn palette_commands() -> impl Iterator<Item = &'static CommandSpec> {
    COMMANDS.iter().filter(|spec| spec.palette)
}

/// Resolve a typed slash command (leading slash optional).
pub fn lookup_slash(typed: &str) -> Option<&'static CommandSpec> {
    let needle = typed.trim().trim_start_matches('/');
    if needle.is_empty() {
        return None;
    }
    COMMANDS.iter().find(|spec| {
        spec.slash.is_some_and(|slash| {
            slash.name.eq_ignore_ascii_case(needle)
                || slash
                    .aliases
                    .iter()
                    .any(|alias| alias.eq_ignore_ascii_case(needle))
        })
    })
}

/// Closest slash command names for an unknown command.
pub fn slash_suggestions(typed: &str, limit: usize) -> Vec<String> {
    let needle = typed.trim().trim_start_matches('/').to_lowercase();
    if needle.is_empty() {
        return Vec::new();
    }
    let mut scored: Vec<(u8, bool, &'static str)> = Vec::new();
    for spec in COMMANDS {
        let Some(slash) = spec.slash else {
            continue;
        };
        let mut best: Option<(u8, usize)> = None;
        for name in std::iter::once(slash.name).chain(slash.aliases.iter().copied()) {
            let lower = name.to_lowercase();
            let prefix = lower.starts_with(&needle);
            let distance = edit_distance(&needle, &lower);
            if prefix || distance <= 2 {
                let rank = usize::from(!prefix);
                let candidate = (distance.min(u8::MAX as usize) as u8, rank);
                if best.is_none_or(|current| candidate < current) {
                    best = Some(candidate);
                }
            }
        }
        if let Some((distance, rank)) = best {
            scored.push((distance, rank == 0, slash.name));
        }
    }
    scored.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)).then(a.2.cmp(b.2)));
    let mut names: Vec<String> = Vec::new();
    for (_, _, name) in scored {
        let formatted = format!("/{name}");
        if !names.contains(&formatted) {
            names.push(formatted);
        }
        if names.len() == limit {
            break;
        }
    }
    names
}

fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    if a.is_empty() {
        return b.len();
    }
    if b.is_empty() {
        return a.len();
    }
    let mut previous: Vec<usize> = (0..=b.len()).collect();
    let mut current = vec![0usize; b.len() + 1];
    for (i, ac) in a.iter().enumerate() {
        current[0] = i + 1;
        for (j, bc) in b.iter().enumerate() {
            let substitution = previous[j] + usize::from(ac != bc);
            current[j + 1] = substitution.min(previous[j + 1] + 1).min(current[j] + 1);
        }
        std::mem::swap(&mut previous, &mut current);
    }
    previous[b.len()]
}

/// One help row: the chord text and what it runs.
#[derive(Debug, Clone)]
pub struct HelpRow {
    pub keys: String,
    pub title: &'static str,
}

/// Command help grouped by command group, followed by the leader section.
pub fn help_sections() -> Vec<(&'static str, Vec<HelpRow>)> {
    let mut sections: Vec<(&'static str, Vec<HelpRow>)> = Vec::new();
    for spec in COMMANDS {
        if spec.id.starts_with("ui.") || spec.id.starts_with("question.") {
            continue;
        }
        let mut keys: Vec<String> = Vec::new();
        for scope in spec.scopes {
            if scope.context != KeymapContext::Global {
                continue;
            }
            for key in scope.keys {
                let text = key.describe();
                if !keys.contains(&text) {
                    keys.push(text);
                }
            }
        }
        if let Some(slash) = spec.slash {
            let mut text = format!("/{}", slash.name);
            if !slash.argument.is_empty() {
                text.push(' ');
                text.push_str(slash.argument);
            }
            keys.push(text);
        }
        if keys.is_empty() {
            continue;
        }
        let position = sections
            .iter()
            .position(|(group, _)| *group == spec.group)
            .unwrap_or_else(|| {
                sections.push((spec.group, Vec::new()));
                sections.len() - 1
            });
        sections[position].1.push(HelpRow {
            keys: keys.join(" / "),
            title: spec.title,
        });
    }
    let leader_rows: Vec<HelpRow> = COMMANDS
        .iter()
        .filter_map(|spec| {
            spec.leader.first().map(|key| HelpRow {
                keys: format!("{} {}", LEADER.describe(), key.describe()),
                title: spec.title,
            })
        })
        .collect();
    if !leader_rows.is_empty() {
        sections.push(("Leader", leader_rows));
    }
    sections
}

/// One-line key hints for the commands scoped to `context`, in registry
/// order. Numbered pick bindings collapse into a single range hint.
pub fn hint_line(context: KeymapContext) -> String {
    let mut parts: Vec<String> = Vec::new();
    let mut has_picks = false;
    for spec in COMMANDS {
        let Some(scope) = spec.scopes.iter().find(|scope| scope.context == context) else {
            continue;
        };
        let Some(key) = scope.keys.first() else {
            continue;
        };
        if spec.id.starts_with("question.pick.") {
            has_picks = true;
            continue;
        }
        parts.push(format!("{} {}", key.describe(), spec.title));
    }
    if has_picks {
        parts.push("1-9 Pick option".to_string());
    }
    parts.join(" · ")
}

/// A leader chord entry: the keys after the leader plus the command.
#[derive(Debug, Clone, Copy)]
pub struct LeaderBinding {
    pub keys: &'static [Key],
    pub command: &'static CommandSpec,
}

/// Resolve leader chords and describe the available continuations.
#[derive(Debug, Clone)]
pub struct SequenceMap {
    leader: Key,
    timeout_ms: u64,
    bindings: Vec<LeaderBinding>,
}

impl SequenceMap {
    pub fn new(leader: Key, timeout_ms: u64) -> Self {
        let bindings = COMMANDS
            .iter()
            .filter(|spec| !spec.leader.is_empty())
            .map(|spec| LeaderBinding {
                keys: spec.leader,
                command: spec,
            })
            .collect();
        Self {
            leader,
            timeout_ms,
            bindings,
        }
    }

    pub fn builtin() -> Self {
        Self::new(LEADER, LEADER_TIMEOUT_MS)
    }

    pub fn leader(&self) -> Key {
        self.leader
    }

    pub fn timeout_ms(&self) -> u64 {
        self.timeout_ms
    }

    pub fn is_leader(&self, key: Key) -> bool {
        key == self.leader
    }

    /// The command whose leader chord is exactly `keys`.
    pub fn resolve(&self, keys: &[Key]) -> Option<&'static CommandSpec> {
        if keys.is_empty() {
            return None;
        }
        self.bindings
            .iter()
            .find(|binding| binding.keys == keys)
            .map(|binding| binding.command)
    }

    /// Whether `keys` is a strict prefix of a longer leader chord.
    pub fn is_prefix(&self, keys: &[Key]) -> bool {
        if keys.is_empty() {
            return !self.bindings.is_empty();
        }
        self.bindings
            .iter()
            .any(|binding| binding.keys.len() > keys.len() && binding.keys.starts_with(keys))
    }

    /// Keys available after `prefix`, with the command they run.
    pub fn continuations(&self, prefix: &[Key]) -> Vec<(Key, &'static CommandSpec)> {
        let mut out: Vec<(Key, &'static CommandSpec)> = Vec::new();
        for binding in &self.bindings {
            if binding.keys.len() <= prefix.len() || !binding.keys.starts_with(prefix) {
                continue;
            }
            let next = binding.keys[prefix.len()];
            if out.iter().any(|(key, _)| *key == next) {
                continue;
            }
            out.push((next, binding.command));
        }
        out
    }

    /// One-line which-key hint for the current prefix.
    pub fn which_key(&self, prefix: &[Key]) -> String {
        let entries = self.continuations(prefix);
        if entries.is_empty() {
            return format!("{}: no matching command", self.leader.describe());
        }
        let body = entries
            .iter()
            .map(|(key, spec)| format!("{} {}", key.describe(), spec.title))
            .collect::<Vec<_>>()
            .join(" · ");
        format!("{}: {}", self.leader.describe(), body)
    }
}

/// Keys typed after the leader is pressed.
#[derive(Debug, Clone)]
pub struct PendingSequence {
    keys: Vec<Key>,
    started_ms: u64,
}

impl PendingSequence {
    pub fn start(now_ms: u64) -> Self {
        Self {
            keys: Vec::new(),
            started_ms: now_ms,
        }
    }

    pub fn push(&mut self, key: Key) {
        self.keys.push(key);
    }

    pub fn keys(&self) -> &[Key] {
        &self.keys
    }

    pub fn is_expired(&self, now_ms: u64, timeout_ms: u64) -> bool {
        now_ms.saturating_sub(self.started_ms) > timeout_ms
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_and_slash_names_are_unique() {
        let mut ids: Vec<&str> = COMMANDS.iter().map(|spec| spec.id).collect();
        ids.sort_unstable();
        let count = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), count, "command ids must be unique");

        let mut slash_names: Vec<&str> = COMMANDS
            .iter()
            .flat_map(|spec| {
                spec.slash.iter().flat_map(|slash| {
                    std::iter::once(slash.name).chain(slash.aliases.iter().copied())
                })
            })
            .collect();
        slash_names.sort_unstable();
        let count = slash_names.len();
        slash_names.dedup();
        assert_eq!(slash_names.len(), count, "slash names must be unique");
    }

    #[test]
    fn palette_commands_have_slash_names() {
        for spec in palette_commands() {
            assert!(
                spec.slash.is_some(),
                "palette command {} needs a slash name",
                spec.id
            );
        }
    }

    #[test]
    fn scope_keys_do_not_collide_within_a_context() {
        for (index, spec) in COMMANDS.iter().enumerate() {
            for scope in spec.scopes {
                for key in scope.keys {
                    for other in COMMANDS.iter().skip(index + 1) {
                        for other_scope in other.scopes {
                            if other_scope.context != scope.context {
                                continue;
                            }
                            assert!(
                                !other_scope.keys.contains(key),
                                "{} and {} both bind {:?} in {:?}",
                                spec.id,
                                other.id,
                                key,
                                scope.context
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn slash_lookup_accepts_names_aliases_and_leading_slash() {
        assert_eq!(
            lookup_slash("/model").map(|spec| spec.id),
            Some("session.model")
        );
        assert_eq!(lookup_slash("m").map(|spec| spec.id), Some("session.model"));
        assert_eq!(lookup_slash("exit").map(|spec| spec.id), Some("app.quit"));
        assert!(lookup_slash("nope").is_none());
        assert!(lookup_slash("/").is_none());
    }

    #[test]
    fn unknown_slash_commands_suggest_neighbours() {
        let suggestions = slash_suggestions("modl", 3);
        assert!(suggestions.contains(&"/model".to_string()));
        assert!(slash_suggestions("zzzz", 3).is_empty());
    }

    #[test]
    fn leader_sequence_resolves_prefix_and_hint() {
        let map = SequenceMap::builtin();
        assert!(map.is_leader(LEADER));
        assert!(map.is_prefix(&[]));
        assert_eq!(
            map.resolve(&[key(CKey::Char('n'))]).map(|spec| spec.id),
            Some("session.new")
        );
        assert!(map.which_key(&[]).contains("New session"));
        assert!(map.which_key(&[]).starts_with("Ctrl-w"));
    }

    #[test]
    fn pending_sequence_expires_after_the_timeout() {
        let pending = PendingSequence::start(1_000);
        assert!(!pending.is_expired(2_500, LEADER_TIMEOUT_MS));
        assert!(pending.is_expired(3_500, LEADER_TIMEOUT_MS));
    }

    #[test]
    fn help_sections_cover_app_and_leader() {
        let sections = help_sections();
        assert!(sections.iter().any(|(group, _)| *group == "App"));
        assert!(sections.iter().any(|(group, _)| *group == "Leader"));
    }
}
