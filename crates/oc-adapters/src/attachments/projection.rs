//! Per-request media projection for the selected model. Raw history keeps
//! every image; a model or wire that cannot take one receives a text note.
use crate::provider::{InputContent, InputItem};

/// What the model reads in place of an image it cannot receive.
pub(crate) const IMAGE_OMITTED: &str =
    "[image omitted: the selected model cannot see images; continuing without it]";

/// Images in `item` that `vision`/`chat` cannot carry. Chat tool messages are
/// text-only even for a vision model; user-message images remain valid there.
fn omits(item: &InputItem, vision: bool, chat: bool) -> usize {
    let tool = !vision || chat;
    match item {
        InputItem::Message { content, .. } if !vision => content
            .iter()
            .filter(|part| matches!(part, InputContent::InputImage { .. }))
            .count(),
        InputItem::ReadFunctionCallOutput { .. } if tool => 1,
        InputItem::QuestionFunctionCallOutput { output, .. } if tool => output.media().0,
        InputItem::McpFunctionCallOutput { output, .. } if tool && output.has_media() => 1,
        InputItem::ProviderOutput(value) if tool && value["type"] == "function_call_output" => {
            value["output"].as_array().map_or(0, |parts| {
                parts
                    .iter()
                    .filter(|part| {
                        !matches!(
                            part["type"].as_str(),
                            Some("input_text" | "output_text" | "text")
                        )
                    })
                    .count()
            })
        }
        _ => 0,
    }
}

/// Count of images the selected model would not receive from `input`.
pub(crate) fn omitted_images(input: &[InputItem], vision: bool, chat: bool) -> usize {
    input.iter().map(|item| omits(item, vision, chat)).sum()
}

/// Replace every image `vision`/`chat` cannot carry with [`IMAGE_OMITTED`].
pub(crate) fn project_media(input: &mut [InputItem], vision: bool, chat: bool) {
    let note = |text: &str| {
        if text.is_empty() {
            IMAGE_OMITTED.to_owned()
        } else {
            format!("{text}\n{IMAGE_OMITTED}")
        }
    };
    for item in input.iter_mut() {
        if omits(item, vision, chat) == 0 {
            continue;
        }
        let text = |call_id: &str, output: String| InputItem::FunctionCallOutput {
            call_id: call_id.to_owned(),
            output,
        };
        *item = match &*item {
            InputItem::Message { role, content } => InputItem::Message {
                role: *role,
                content: content
                    .iter()
                    .map(|part| match part {
                        InputContent::InputImage { .. } => InputContent::InputText {
                            text: IMAGE_OMITTED.into(),
                        },
                        other => other.clone(),
                    })
                    .collect(),
            },
            InputItem::ReadFunctionCallOutput { call_id, output } => {
                text(call_id, note(output.display()))
            }
            InputItem::QuestionFunctionCallOutput { call_id, output } => {
                text(call_id, note(output.display()))
            }
            InputItem::McpFunctionCallOutput { call_id, output } => {
                text(call_id, note(&output.texts().join("\n")))
            }
            InputItem::ProviderOutput(value) => {
                let parts = value["output"].as_array().expect("counted output parts");
                let kept: Vec<&str> = parts
                    .iter()
                    .filter_map(|part| part["text"].as_str())
                    .collect();
                text(
                    value["call_id"].as_str().unwrap_or_default(),
                    note(&kept.join("\n")),
                )
            }
            _ => unreachable!("only counted media items are projected"),
        };
    }
}

#[cfg(test)]
mod tests;
