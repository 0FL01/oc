//! Built-in slash command dispatch (UI06).
//!
//! Bounded `/name rest` parsing over an exact built-in table. Workspace
//! defined commands arrive with the configured workspace (T25/A13); this
//! table only routes commands the TUI implements, bound to the current
//! generation by the caller.

/// Max command name bytes (bounded view/dispatch state).
pub const COMMAND_NAME_MAX: usize = 32;
/// Max trailing argument bytes kept.
pub const COMMAND_ARGS_MAX: usize = 512;

/// Dispatched built-in command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandAction {
    /// Search genuine native commands.
    OpenCommands,
    /// Toggle the existing native sidebar.
    ToggleSidebar,
    /// Exit the TUI.
    Quit,
    /// Open the model picker.
    OpenModelPicker,
    /// Open the declared variants of the effective model.
    OpenVariants,
    /// Create and attach a genuine empty application session.
    NewSession,
    /// Open the primary agent selector.
    OpenAgents,
    /// Open the session list.
    OpenSessions,
    /// Open the skill catalog.
    OpenSkills,
    /// Open the tool-card list.
    OpenCards,
    /// Help, optionally for one topic.
    Help(Option<String>),
    /// Switch the running application to another Location (project path).
    SwitchLocation {
        /// Target project path (as typed; the application canonicalises).
        path: String,
    },
    /// Manual DCP compress with bounded focus (UI04).
    DcpCompress {
        /// Focus instruction (possibly empty).
        focus: String,
    },
}

/// Parse input; `None` when it is not a slash command.
pub fn dispatch(input: &str) -> Option<CommandAction> {
    let rest = input.strip_prefix('/')?;
    let (name, args) = match rest.split_once(char::is_whitespace) {
        Some((name, args)) => (name, args.trim()),
        None => (rest, ""),
    };
    if name.is_empty() || name.len() > COMMAND_NAME_MAX {
        return Some(CommandAction::Help(None));
    }
    let args = args.get(..args.len().min(COMMAND_ARGS_MAX)).unwrap_or("");
    let Some(command) = REGISTRY.iter().find(|c| c.aliases.contains(&name)) else {
        return Some(CommandAction::Help(None));
    };
    Some(match command.action {
        CommandAction::SwitchLocation { .. } => CommandAction::SwitchLocation {
            path: args.to_string(),
        },
        CommandAction::DcpCompress { .. } => CommandAction::DcpCompress {
            focus: args.to_string(),
        },
        CommandAction::Help(_) => CommandAction::Help(if args.is_empty() {
            None
        } else {
            Some(args.to_string())
        }),
        _ => command.action.clone(),
    })
}

/// Single registry for selectable palette actions, aliases, and binding labels.
/// Unsupported original capabilities are inventoried in recovery-v04/capabilities.md.
pub struct CommandSpec {
    pub id: &'static str,
    pub title: &'static str,
    pub group: &'static str,
    pub shortcuts: &'static [&'static str],
    pub aliases: &'static [&'static str],
    pub action: CommandAction,
}

impl CommandSpec {
    /// Availability is shared by palette, slash and bindings, checked again on activation.
    pub fn unavailable(&self, busy: bool, has_variants: bool) -> Option<&'static str> {
        if busy
            && matches!(
                self.action,
                CommandAction::NewSession
                    | CommandAction::OpenSessions
                    | CommandAction::OpenModelPicker
                    | CommandAction::OpenVariants
                    | CommandAction::OpenAgents
                    | CommandAction::SwitchLocation { .. }
                    | CommandAction::DcpCompress { .. }
            )
        {
            return Some("turn active; action unavailable");
        }
        if self.action == CommandAction::OpenVariants && !has_variants {
            return Some("No variants available");
        }
        None
    }

    pub fn in_palette(&self, has_variants: bool) -> bool {
        !matches!(
            self.action,
            CommandAction::OpenCommands | CommandAction::SwitchLocation { .. }
        ) && (self.action != CommandAction::OpenVariants || has_variants)
    }
}

pub fn spec(action: &CommandAction) -> &'static CommandSpec {
    REGISTRY
        .iter()
        .find(|c| std::mem::discriminant(&c.action) == std::mem::discriminant(action))
        .expect("every builtin action has one registry entry")
}

/// Match the displayed binding itself, so a label cannot drift from its handler.
pub fn direct(event: crossterm::event::KeyEvent) -> Option<CommandAction> {
    use crossterm::event::{KeyCode, KeyModifiers};
    REGISTRY
        .iter()
        .find(|c| {
            c.shortcuts.iter().any(|binding| {
                let (code, modifiers) = match *binding {
                    "shift+tab" => (KeyCode::BackTab, KeyModifiers::SHIFT),
                    s if s.starts_with("ctrl+") && s.len() == 6 => (
                        KeyCode::Char(s.as_bytes()[5] as char),
                        KeyModifiers::CONTROL,
                    ),
                    _ => return false,
                };
                event.code == code && event.modifiers == modifiers
            })
        })
        .map(|c| c.action.clone())
}

pub const COMMANDS_BINDING: &str = "ctrl+p";
pub const AGENTS_BINDING: &str = "shift+tab";

pub const REGISTRY: &[CommandSpec] = &[
    CommandSpec {
        id: "commands.show",
        title: "Show command palette",
        group: "System",
        shortcuts: &[COMMANDS_BINDING],
        aliases: &["commands"],
        action: CommandAction::OpenCommands,
    },
    CommandSpec {
        id: "session.list",
        title: "Switch session",
        group: "Session",
        shortcuts: &["ctrl+x l"],
        aliases: &["sessions", "session", "resume", "continue"],
        action: CommandAction::OpenSessions,
    },
    CommandSpec {
        id: "session.new",
        title: "New session",
        group: "Session",
        shortcuts: &["ctrl+x n"],
        aliases: &["new", "clear"],
        action: CommandAction::NewSession,
    },
    CommandSpec {
        id: "session.sidebar.toggle",
        title: "Toggle sidebar",
        group: "Session",
        shortcuts: &["ctrl+x b"],
        aliases: &["sidebar"],
        action: CommandAction::ToggleSidebar,
    },
    CommandSpec {
        id: "model.list",
        title: "Switch model",
        group: "Agent",
        shortcuts: &["ctrl+x m"],
        aliases: &["model", "models"],
        action: CommandAction::OpenModelPicker,
    },
    CommandSpec {
        id: "variant.list",
        title: "Switch model variant",
        group: "Agent",
        shortcuts: &[],
        aliases: &["variants", "thinking", "effort"],
        action: CommandAction::OpenVariants,
    },
    CommandSpec {
        id: "agent.list",
        title: "Switch agent",
        group: "Agent",
        shortcuts: &["ctrl+x a", AGENTS_BINDING],
        aliases: &["agents", "agent"],
        action: CommandAction::OpenAgents,
    },
    CommandSpec {
        id: "skill.list",
        title: "Skills",
        group: "Agent",
        shortcuts: &[],
        aliases: &["skills"],
        action: CommandAction::OpenSkills,
    },
    CommandSpec {
        id: "native.cards",
        title: "Tool cards",
        group: "Native",
        shortcuts: &[],
        aliases: &["cards"],
        action: CommandAction::OpenCards,
    },
    CommandSpec {
        id: "native.location",
        title: "Change location",
        group: "Native",
        shortcuts: &[],
        aliases: &["location"],
        action: CommandAction::SwitchLocation {
            path: String::new(),
        },
    },
    CommandSpec {
        id: "native.compress",
        title: "Compress DCP context",
        group: "Native",
        shortcuts: &[],
        aliases: &["dcp-compress"],
        action: CommandAction::DcpCompress {
            focus: String::new(),
        },
    },
    CommandSpec {
        id: "help.show",
        title: "Help",
        group: "System",
        shortcuts: &[],
        aliases: &["help"],
        action: CommandAction::Help(None),
    },
    CommandSpec {
        id: "app.exit",
        title: "Exit the app",
        group: "System",
        shortcuts: &["ctrl+c", "ctrl+d"],
        aliases: &["quit", "exit"],
        action: CommandAction::Quit,
    },
];

/// Complete a `/prefix` against the built-in table.
pub fn complete(prefix: &str) -> Vec<&'static str> {
    let needle = prefix.strip_prefix('/').unwrap_or(prefix);
    let mut names: Vec<_> = REGISTRY
        .iter()
        .flat_map(|c| c.aliases.iter().copied())
        .filter(|name| name.starts_with(needle))
        .collect();
    names.sort_unstable();
    names
}

#[cfg(test)]
mod tests {
    use super::{CommandAction, complete, dispatch};

    #[test]
    fn routes_builtins() {
        assert_eq!(dispatch("/quit"), Some(CommandAction::Quit));
        assert_eq!(dispatch("/model"), Some(CommandAction::OpenModelPicker));
        assert_eq!(dispatch("/agents"), Some(CommandAction::OpenAgents));
        assert_eq!(dispatch("/sessions"), Some(CommandAction::OpenSessions));
        assert_eq!(dispatch("/skills"), Some(CommandAction::OpenSkills));
        assert_eq!(
            dispatch("/help model"),
            Some(CommandAction::Help(Some("model".to_string())))
        );
        assert_eq!(dispatch("hello"), None);
        assert_eq!(dispatch("/unknown"), Some(CommandAction::Help(None)));
        assert_eq!(
            dispatch("/location /srv/other"),
            Some(CommandAction::SwitchLocation {
                path: "/srv/other".to_string()
            })
        );
        assert_eq!(
            dispatch("/dcp-compress draft span"),
            Some(CommandAction::DcpCompress {
                focus: "draft span".to_string()
            })
        );
        assert_eq!(
            dispatch("/dcp-compress"),
            Some(CommandAction::DcpCompress {
                focus: String::new()
            })
        );
    }

    #[test]
    fn completes_prefix() {
        assert_eq!(complete("/s"), ["session", "sessions", "sidebar", "skills"]);
        assert_eq!(complete("/a"), ["agent", "agents"]);
        assert_eq!(complete("/d"), ["dcp-compress"]);
        for command in super::REGISTRY {
            for alias in command.aliases {
                assert!(complete("/").contains(alias));
                assert_eq!(dispatch(&format!("/{alias}")), Some(command.action.clone()));
            }
        }
        for unsupported in ["open", "projects", "project", "mcps"] {
            assert!(!complete("/").contains(&unsupported));
        }
    }
}
