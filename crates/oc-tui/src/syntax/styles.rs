//! Pinned `packages/theme/src/tui/syntax.ts` semantic rules. Colors continue to
//! come from the validated shared Theme, not a syntax-specific palette.
use crate::theme::{MarkdownToken as Md, SyntaxToken as Syn, Theme};
use ratatui::style::{Modifier, Style};

pub(super) fn capture(scope: &str, theme: &Theme) -> Option<Style> {
    exact(scope, theme).or_else(|| {
        scope
            .split_once('.')
            .and_then(|(base, _)| exact(base, theme))
    })
}

fn exact(scope: &str, t: &Theme) -> Option<Style> {
    let s = |token| Style::default().fg(t.syntax(token));
    let m = |token| Style::default().fg(t.markdown(token));
    let fg = |color| Style::default().fg(color);
    let italic = Modifier::ITALIC;
    let bold = Modifier::BOLD;
    let underline = Modifier::UNDERLINED;
    Some(match scope {
        "default" | "spell" | "nospell" => fg(t.text()),
        "prompt" => fg(t.hue("accent", 200).expect("validated accent scale")),
        "extmark.file" => fg(t.warning()).add_modifier(bold),
        "extmark.agent" => fg(t.categorical_agents()[0]).add_modifier(bold),
        "extmark.skill" => {
            let colors = t.categorical_agents();
            fg(*colors.get(1).unwrap_or(&colors[0])).add_modifier(bold)
        }
        "extmark.paste" => fg(t
            .color("text.action.primary.focused")
            .expect("validated action role"))
        .bg(t.warning())
        .add_modifier(bold),
        "comment" | "comment.documentation" => s(Syn::Comment).add_modifier(italic),
        "string" | "symbol" | "character.special" | "character" => s(Syn::String),
        "number" | "boolean" | "constant" | "float" => s(Syn::Number),
        "keyword.return"
        | "keyword.conditional"
        | "keyword.repeat"
        | "keyword.coroutine"
        | "keyword"
        | "keyword.directive"
        | "keyword.modifier"
        | "keyword.exception" => s(Syn::Keyword).add_modifier(italic),
        "keyword.type" => s(Syn::Type).add_modifier(bold | italic),
        "keyword.function" | "function.method" => s(Syn::Function),
        "keyword.import" | "string.escape" | "string.regexp" | "tag.attribute"
        | "keyword.export" => s(Syn::Keyword),
        "operator"
        | "keyword.operator"
        | "punctuation.delimiter"
        | "keyword.conditional.ternary"
        | "punctuation.special"
        | "tag.delimiter" => s(Syn::Operator),
        "variable"
        | "variable.parameter"
        | "function.method.call"
        | "function.call"
        | "property"
        | "parameter"
        | "field" => s(Syn::Variable),
        "variable.member" | "function" | "constructor" => s(Syn::Function),
        "type" | "module" | "class" | "namespace" => s(Syn::Type),
        "type.definition" => s(Syn::Type).add_modifier(bold),
        "punctuation" | "punctuation.bracket" => s(Syn::Punctuation),
        "variable.builtin" | "type.builtin" | "function.builtin" | "module.builtin"
        | "constant.builtin" | "variable.super" => fg(t.error()),
        "markup.heading" | "markup.heading.2" | "markup.heading.3" | "markup.heading.4"
        | "markup.heading.5" | "markup.heading.6" => m(Md::Heading).add_modifier(bold),
        "markup.heading.1" => m(Md::Heading).add_modifier(bold | underline),
        "markup.bold" | "markup.strong" => m(Md::Strong).add_modifier(bold),
        "markup.italic" => m(Md::Emphasis).add_modifier(italic),
        "markup.list" => m(Md::ListItem),
        "markup.quote" => m(Md::BlockQuote).add_modifier(italic),
        "markup.raw" | "markup.raw.block" => m(Md::Code),
        "markup.raw.inline" => m(Md::Code).bg(t.background()),
        "markup.link" | "markup.link.url" | "string.special" | "string.special.url" => {
            m(Md::Link).add_modifier(underline)
        }
        "markup.link.label" => m(Md::LinkText).add_modifier(underline),
        "label" => m(Md::LinkText),
        "markup.underline" => fg(t.text()).add_modifier(underline),
        "comment.error" => fg(t.error()).add_modifier(italic | bold),
        "comment.warning" => fg(t.warning()).add_modifier(italic | bold),
        "comment.todo" | "comment.note" => fg(t.info()).add_modifier(italic | bold),
        "attribute" | "annotation" => fg(t.warning()),
        "tag" | "error" => fg(t.error()).add_modifier(if scope == "error" {
            bold
        } else {
            Modifier::empty()
        }),
        "markup.strikethrough" | "markup.list.unchecked" | "debug" => fg(t.text_muted()),
        "markup.list.checked" => fg(t.success()),
        "diff.plus" => fg(t.diff_added()).bg(t.diff_added_background()),
        "diff.minus" => fg(t.diff_removed()).bg(t.diff_removed_background()),
        "diff.delta" => fg(t.diff_context()).bg(t.diff_context_background()),
        "warning" => fg(t.warning()).add_modifier(bold),
        "info" => fg(t.info()),
        _ => return None,
    })
}
