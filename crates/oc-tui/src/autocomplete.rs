//! Inline slash options from the admitted command registry and current catalog.
//! Pinned reference: `component/prompt/autocomplete.tsx:466-504,520-575,584-626,780-810`.

use std::collections::BTreeMap;

use crate::commands::{self, CommandAction};
use crate::fuzzy::{Query, Target};

#[derive(Clone)]
pub(crate) struct SlashOption {
    pub name: String,
    pub description: String,
    pub action: Option<CommandAction>,
    pub arguments: bool,
}

/// `prompt/display.ts:52-61`: commands are offered only at the start of the
/// prompt, before a space or a second slash; caret position is a byte boundary.
pub(crate) fn query(text: &str, cursor: usize) -> Option<&str> {
    let before = text.get(..cursor)?;
    let filter = before.strip_prefix('/')?;
    (!filter.contains(char::is_whitespace) && !filter.contains('/')).then_some(filter)
}

/// File mentions start at a line boundary or after an ASCII space. Only the
/// segment before the caret participates; later text is left in the draft.
pub(crate) fn mention(text: &str, cursor: usize) -> Option<(usize, &str)> {
    let before = text.get(..cursor)?;
    let start = before.rfind('@')?;
    if start > 0 && !matches!(before.as_bytes()[start - 1], b' ' | b'\n') {
        return None;
    }
    let query = &before[start + 1..];
    (!query.chars().any(char::is_whitespace) && !query.contains('@') && query.len() <= 256)
        .then_some((start, query))
}

pub(crate) fn options(
    filter: &str,
    workspace: &[String],
    descriptions: &BTreeMap<String, String>,
) -> Vec<SlashOption> {
    let mut options: Vec<_> = commands::REGISTRY
        .iter()
        .flat_map(|command| {
            command.aliases.iter().map(move |name| SlashOption {
                name: (*name).into(),
                description: command.title.into(),
                arguments: matches!(
                    command.action,
                    CommandAction::RenameSession { .. }
                        | CommandAction::SwitchLocation { .. }
                        | CommandAction::DcpCompress { .. }
                ),
                action: Some(command.action.clone()),
            })
        })
        .collect();
    for name in workspace {
        // A colliding alias dispatches to the built-in in the existing composer.
        // Never advertise the same spelling as a separately executable action.
        if !options.iter().any(|option| option.name == *name) {
            options.push(SlashOption {
                name: name.clone(),
                description: descriptions.get(name).cloned().unwrap_or_default(),
                action: None,
                arguments: true,
            });
        }
    }
    options.sort_by(|a, b| a.name.cmp(&b.name));
    if filter.is_empty() {
        return options;
    }
    // A configured name is at most 128 ASCII bytes; built-in descriptions
    // are short. Do not re-score a maximum-size prompt on every frame.
    if filter.len() > 512 {
        return Vec::new();
    }
    let search = Query::new(filter);
    let scores = options.iter().enumerate().filter_map(|(index, option)| {
        let name = format!("/{}", option.name);
        search
            .score(
                &[Target::new(&name), Target::new(&option.description)],
                false,
            )
            .map(|score| {
                // Upstream doubles prefix matches after fuzzysort scoring.
                (
                    index,
                    if option.name.starts_with(filter) {
                        score * 2.0
                    } else {
                        score
                    },
                )
            })
    });
    crate::fuzzy::rank(scores)
        .into_iter()
        .take(10)
        .map(|index| options[index].clone())
        .collect()
}
