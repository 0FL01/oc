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
    /// Exit the TUI.
    Quit,
    /// Open the model picker.
    OpenModelPicker,
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
    match name {
        "quit" => Some(CommandAction::Quit),
        "model" => Some(CommandAction::OpenModelPicker),
        "agents" => Some(CommandAction::OpenAgents),
        "sessions" => Some(CommandAction::OpenSessions),
        "skills" => Some(CommandAction::OpenSkills),
        "cards" => Some(CommandAction::OpenCards),
        "location" => Some(CommandAction::SwitchLocation {
            path: args.to_string(),
        }),
        "dcp-compress" => Some(CommandAction::DcpCompress {
            focus: args.to_string(),
        }),
        "help" => Some(CommandAction::Help(if args.is_empty() {
            None
        } else {
            Some(args.to_string())
        })),
        _ => Some(CommandAction::Help(None)),
    }
}

/// Built-in command names for completion (exact table, sorted).
pub const BUILTINS: [&str; 8] = [
    "agents",
    "dcp-compress",
    "help",
    "location",
    "model",
    "quit",
    "sessions",
    "skills",
];

/// Complete a `/prefix` against the built-in table.
pub fn complete(prefix: &str) -> Vec<&'static str> {
    let needle = prefix.strip_prefix('/').unwrap_or(prefix);
    BUILTINS
        .into_iter()
        .filter(|name| name.starts_with(needle))
        .collect()
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
        assert_eq!(complete("/s"), ["sessions", "skills"]);
        assert_eq!(complete("/a"), ["agents"]);
        assert_eq!(complete("/d"), ["dcp-compress"]);
        assert_eq!(
            complete("/"),
            [
                "agents",
                "dcp-compress",
                "help",
                "location",
                "model",
                "quit",
                "sessions",
                "skills"
            ]
        );
    }
}
